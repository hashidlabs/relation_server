extern crate futures;
mod tests;

use crate::config::C;
use crate::error::Error;
use crate::graph::{edge::Proof, new_db_connection, vertex::Identity};
use crate::graph::{Edge, Vertex};
use crate::upstream::{Connection, DataSource, Fetcher, Platform};
use crate::util::{make_client, naive_now, parse_body, timestamp_to_naive};

use async_trait::async_trait;
use futures::future::join_all;
use serde::Deserialize;
use serde_json::{Map, Value};
use std::str::FromStr;
use uuid::Uuid;

/// https://github.com/nextdotid/proof-server/blob/master/docs/api.apib
#[derive(Deserialize, Debug)]
pub struct ProofQueryResponse {
    pub pagination: ProofQueryResponsePagination,
    pub ids: Vec<ProofPersona>,
}

#[derive(Deserialize, Debug)]
pub struct ProofPersona {
    pub persona: String,
    pub proofs: Vec<ProofRecord>,
}

#[derive(Deserialize, Debug)]
pub struct ProofRecord {
    pub platform: String,
    pub identity: String,
    pub created_at: String,
    pub last_checked_at: String,
    pub is_valid: bool,
    pub invalid_reason: String,
}

#[derive(Deserialize, Debug)]
pub struct ProofQueryResponsePagination {
    pub total: u32,
    pub per: u32,
    pub current: u32,
    pub next: u32,
}

#[derive(Deserialize, Debug)]
pub struct ErrorResponse {
    pub message: String,
}

pub struct ProofClient {
    pub persona: String,
}

async fn save_item(p: ProofRecord) -> Option<Connection> {
    let db = new_db_connection().await.ok()?;

    let from: Identity = Identity {
        uuid: Some(Uuid::new_v4()),
        platform: Platform::NextID,
        identity: p.identity.clone(),
        created_at: Some(timestamp_to_naive(
            p.created_at.to_string().parse().unwrap(),
        )),
        display_name: p.identity.clone(),
        added_at: naive_now(),
        avatar_url: None,
        profile_url: None,
        updated_at: naive_now(),
    };

    let from_record = from.create_or_update(&db).await.ok()?;

    let to: Identity = Identity {
        uuid: Some(Uuid::new_v4()),
        platform: Platform::from_str(p.platform.as_str()).unwrap(),
        identity: p.identity.to_string(),
        created_at: Some(timestamp_to_naive(
            p.created_at.to_string().parse().unwrap(),
        )),
        display_name: p.identity.clone(),
        added_at: naive_now(),
        avatar_url: None,
        profile_url: None,
        updated_at: naive_now(),
    };
    let to_record = to.create_or_update(&db).await.ok()?;

    let pf: Proof = Proof {
        uuid: Uuid::new_v4(),
        source: DataSource::NextID,
        record_id: None,
        created_at: Some(timestamp_to_naive(
            p.created_at.to_string().parse().unwrap(),
        )),
        last_fetched_at: naive_now(),
    };
    let proof_record = pf.connect(&db, &from_record, &to_record).await.ok()?;

    let cnn: Connection = Connection {
        from: from_record,
        to: to_record,
        proof: proof_record,
    };

    return Some(cnn);
}

#[async_trait]
impl Fetcher for ProofClient {
    async fn fetch(&self, _url: Option<String>) -> Result<Vec<Connection>, Error> {
        let client = make_client();
        let uri: http::Uri = match format!(
            "{}/v1/proof?platform=nextid&identity={}",
            C.upstream.proof_service.url, self.persona
        )
        .parse()
        {
            Ok(n) => n,
            Err(err) => {
                return Err(Error::ParamError(format!(
                    "Uri format Error: {}",
                    err.to_string()
                )))
            }
        };
        let mut resp = client.get(uri).await?;

        if !resp.status().is_success() {
            let body: ErrorResponse = parse_body(&mut resp).await?;
            return Err(Error::General(
                format!("Proof Result Get Error: {}", body.message),
                resp.status(),
            ));
        }

        let mut body: ProofQueryResponse = parse_body(&mut resp).await?;
        if body.pagination.total == 0 {
            return Err(Error::NoResult);
        }

        let proofs = match body.ids.pop() {
            Some(i) => i,
            None => {
                return Err(Error::NoResult);
            }
        };

        // parse
        let futures: Vec<_> = proofs.proofs.into_iter().map(|p| save_item(p)).collect();
        let results = join_all(futures).await;
        let parse_body: Vec<Connection> = results.into_iter().filter_map(|i| i).collect();
        Ok(parse_body)
    }

    fn ability() -> Vec<(Platform, Vec<Platform>)> {
        let x: (Platform, Vec<Platform>) = (Platform::NextID, vec![Platform::Twitter, Platform::Ethereum, Platform::Github]);
        //let y: (Platform, Vec<Platform>) = (Platform::Twitter, vec![Platform::Twitter, Platform::Ethereum, Platform::Github]);
        let mut vec = Vec::new();
        vec.push(x);
        return vec;
    }
}
