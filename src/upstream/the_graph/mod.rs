#[cfg(test)]
mod tests;

use crate::{
    config::C,
    error::Error,
    graph::{
        create_identity_to_contract_record,
        edge::{hold::Hold, resolve::DomainNameSystem, Resolve},
        new_db_connection,
        vertex::{
            contract::{Chain, ContractCategory},
            Contract, ContractRecord, Identity,
        },
        Edge, Vertex,
    },
    upstream::{DataFetcher, DataSource, Fetcher, Platform, Target, TargetProcessedList},
    util::{naive_now, parse_timestamp},
};
use aragog::DatabaseConnection;
use async_trait::async_trait;
use gql_client::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use uuid::Uuid;

#[derive(Serialize)]
struct QueryVars {
    target: String,
}

#[derive(Deserialize, Debug)]
struct QueryResponse {
    domains: Vec<Domain>,
    #[serde(rename = "wrappedDomains")]
    wrapped_domains: Vec<WrappedDomain>,
}

#[derive(Deserialize, Debug, Clone)]
struct Domain {
    /// ENS name (`something.eth`)
    name: String,
    /// Creation timestamp (in secods)
    #[serde(rename = "createdAt")]
    created_at: String,
    /// ETH event logs for this ENS.
    events: Vec<DomainEvent>,
    /// Reverse resolve record set on this ENS.
    #[serde(rename = "resolvedAddress")]
    resolved_address: Option<Account>,
    /// Owner info
    owner: Account,
}

#[derive(Deserialize, Debug, Clone)]
struct WrappedDomain {
    name: String,
    owner: Account,
    domain: Domain,
}

#[derive(Deserialize, Debug, Clone)]
struct Account {
    /// Ethereum wallet
    id: String,
}

#[derive(Deserialize, Debug, Clone)]
struct DomainEvent {
    #[serde(rename = "transactionID")]
    transaction_id: String,
}

const QUERY_BY_ENS: &str = r#"
        query OwnerAddressByENS($target: String!){
            domains(where: { name: $target }) {
                name
                createdAt
                events(first: 1) {
                    transactionID
                }
                resolvedAddress {
                  id
                }
                owner{
                  id
                }
              }
            wrappedDomains(where: { name: $target }) {
              name
              domain {
                name
                createdAt
                events(first: 1) {
                    transactionID
                }
                resolvedAddress {
                  id
                }
                owner{
                  id
                }
              }
              owner {
                id
              }
            }
        }
    "#;

const QUERY_BY_WALLET: &str = r#"
        query ENSByOwnerAddress($target: String!){
            domains(where: { owner: $target }) {
                name
                createdAt
                events(first: 1) {
                    transactionID
                }
                resolvedAddress {
                  id
                }
                owner {
                  id
                }
              }
            wrappedDomains(where: { owner: $target }) {
              name
              domain {
                name
                createdAt
                events(first: 1) {
                    transactionID
                }
                resolvedAddress {
                  id
                }
                owner{
                  id
                }
              }
              owner {
                id
              }
            }
        }
    "#;

pub struct TheGraph {}

#[async_trait]
impl Fetcher for TheGraph {
    async fn fetch(target: &Target) -> Result<TargetProcessedList, Error> {
        if !Self::can_fetch(target) {
            return Ok(vec![]);
        }

        perform_fetch(target).await
    }

    fn can_fetch(target: &Target) -> bool {
        target.in_platform_supported(vec![Platform::Ethereum])
            || target.in_nft_supported(vec![ContractCategory::ENS], vec![Chain::Ethereum])
    }
}

/// TODO: reverse lookup for ENS is not provided by official TheGraph for now.
/// See also: https://github.com/ensdomains/ens-subgraph/issues/25
/// Consider deploy a self-hosted reverse lookup service like:
/// https://github.com/fafrd/ens-reverse-lookup
async fn perform_fetch(target: &Target) -> Result<TargetProcessedList, Error> {
    let query: String;
    let target_var: String;
    match target {
        Target::Identity(_platform_, identity) => {
            query = QUERY_BY_WALLET.to_string();
            target_var = identity.clone();
        }
        Target::NFT(_chain, _category, _contract_addr, ens_name) => {
            query = QUERY_BY_ENS.to_string();
            target_var = ens_name.clone();
        }
    }

    let client = Client::new(&C.upstream.the_graph.ens);
    let vars = QueryVars { target: target_var };

    let resp = client.query_with_vars::<QueryResponse, QueryVars>(&query, vars);

    let data: Option<QueryResponse> =
        match tokio::time::timeout(std::time::Duration::from_secs(5), resp).await {
            Ok(resp) => match resp {
                Ok(resp) => resp,
                Err(err) => {
                    warn!(?target, ?err, "TheGraph: Failed to fetch");
                    None
                }
            },
            Err(_) => {
                warn!(?target, "TheGraph: Timeout: no response in 5 seconds.");
                None
            }
        };

    if data.is_none() {
        info!(?target, "TheGraph: No result");
        return Ok(vec![]);
    }
    let res = data.unwrap();
    debug!(?target, wrapped = res.wrapped_domains.len(), domains = res.domains.len(), "Records found.");
    let mut merged_domains: Vec<Domain> = vec![];
    // Rewrite correct owner info for wrapped domains.
    for wd in res.wrapped_domains.into_iter() {
        debug!(?target, domain = wd.name, "TheGraph: Wrapped ENS found.");
        let mut domain = wd.domain.clone();
        domain.owner = wd.owner;
        merged_domains.push(domain);
    }
    for domain in res.domains.into_iter() {
        if merged_domains.iter().any(|md| md.name == domain.name) {
            debug!(?target, domain = domain.name, "TheGraph: Wrapped ENS found before. Skip this.");
            continue;
        } else {
            merged_domains.push(domain);
        }
    }

    if merged_domains.is_empty() {
        info!(?target, "TheGraph: No result");
        return Ok(vec![]);
    }
    let db = new_db_connection().await?;
    let mut next_targets: TargetProcessedList = vec![];

    for domain in merged_domains.into_iter() {
        // Create own record
        let contract_record = create_or_update_own(&db, &domain).await?;

        // Deal with resolve target.
        let resolved_address = domain.resolved_address.map(|r| r.id);
        match resolved_address.clone() {
            Some(address) => {
                if address != "0x0000000000000000000000000000000000000000".to_string() {
                    // Create resolve record
                    debug!(?target, address, domain = domain.name, "TheGraph: Resolved");
                    let resolve_target = Identity {
                        uuid: Some(Uuid::new_v4()),
                        platform: Platform::Ethereum,
                        identity: address.clone(),
                        created_at: None,
                        display_name: None,
                        added_at: naive_now(),
                        avatar_url: None,
                        profile_url: None,
                        updated_at: naive_now(),
                    }
                    .create_or_update(&db)
                    .await?;
                    let resolve = Resolve {
                        uuid: Uuid::new_v4(),
                        source: DataSource::TheGraph,
                        system: DomainNameSystem::ENS,
                        name: domain.name.clone(),
                        fetcher: DataFetcher::RelationService,
                        updated_at: naive_now(),
                    };

                    // 'reverse' resolution
                    resolve
                        .connect(&db, &resolve_target, &contract_record)
                        .await?;
                }
            }
            None => {
                // Resolve record not existed anymore. Maybe deleted by user.
                // TODO: Should find existed connection and delete it.
            }
        }

        // Append up_next
        match target {
            Target::Identity(_, _) => next_targets.push(Target::NFT(
                Chain::Ethereum,
                ContractCategory::ENS,
                ContractCategory::ENS.default_contract_address().unwrap(),
                domain.name.clone(),
            )),
            Target::NFT(_, _, _, _) => {
                let owner_address = domain.owner.id.clone();
                next_targets.push(Target::Identity(Platform::Ethereum, owner_address.clone()));
                if resolved_address.is_some() && resolved_address != Some(owner_address) {
                    next_targets.push(Target::Identity(
                        Platform::Ethereum,
                        resolved_address.unwrap(),
                    ));
                }
            }
        }
    }
    Ok(next_targets)
}

/// Focus on `Hold` record.
async fn create_or_update_own(
    db: &DatabaseConnection,
    domain: &Domain,
) -> Result<ContractRecord, Error> {
    let creation_tx = domain
        .events
        .first() // TODO: really?
        .map(|event| event.transaction_id.clone());
    let ens_created_at = parse_timestamp(&domain.created_at).ok();
    let owner = Identity {
        uuid: Some(Uuid::new_v4()),
        platform: Platform::Ethereum,
        identity: domain.owner.id.clone(),
        created_at: None,
        display_name: None,
        added_at: naive_now(),
        avatar_url: None,
        profile_url: None,
        updated_at: naive_now(),
    };
    let conrtract = Contract {
        uuid: Uuid::new_v4(),
        category: ContractCategory::ENS,
        address: ContractCategory::ENS.default_contract_address().unwrap(),
        chain: Chain::Ethereum,
        symbol: None,
        updated_at: naive_now(),
    };
    let ownership: Hold = Hold {
        uuid: Uuid::new_v4(),
        transaction: creation_tx,
        id: domain.name.clone(),
        source: DataSource::TheGraph,
        created_at: ens_created_at,
        updated_at: naive_now(),
        fetcher: DataFetcher::RelationService,
    };
    let (owner_record, contract_record, _hold_record) =
        create_identity_to_contract_record(db, &owner, &conrtract, &ownership).await?;

    let resolve = Resolve {
        uuid: Uuid::new_v4(),
        source: DataSource::TheGraph,
        system: DomainNameSystem::ENS,
        name: domain.name.clone(),
        fetcher: DataFetcher::RelationService,
        updated_at: naive_now(),
    };
    // As the same time record 'regular' resolution
    resolve
        .connect(&db, &contract_record, &owner_record)
        .await?;
    Ok(contract_record)
}
