extern crate futures;
mod tests;

use crate::config::C;
use crate::error::Error;
use crate::graph::{edge::Proof, new_db_connection, vertex::Identity};
use crate::graph::{Edge, Vertex};
use crate::upstream::{Connection, DataSource, Fetcher, Platform};
use crate::util::{make_client, naive_now, parse_body, timestamp_to_naive};
use async_trait::async_trait;
use serde::Deserialize;

use serde_json::{Map, Value};

use uuid::Uuid;

use futures::future::join_all;

#[derive(Deserialize, Debug)]
pub struct SybilListItem {
    pub twitter_name: String,
    pub eth_addr: String,
    pub timestamp: i64,
}

#[derive(Deserialize, Debug)]
pub struct VerifiedItem {
    pub twitter: TwitterItem,
}

#[derive(Deserialize, Debug)]
pub struct TwitterItem {
    pub timestamp: i64,
    #[serde(rename = "tweetID")]
    pub tweet_id: String,
    pub handle: String,
}

#[derive(Deserialize, Debug)]
pub struct ErrorResponse {
    pub message: String,
}

pub struct SybilList {}

async fn save_item(eth_wallet_address: String, value: Value) -> Option<Connection> {
    let db = new_db_connection().await.ok()?;

    let item: VerifiedItem = serde_json::from_value(value).ok()?;

    let from: Identity = Identity {
        uuid: Some(Uuid::new_v4()),
        platform: Platform::Ethereum,
        identity: eth_wallet_address.clone(),
        created_at: None,
        display_name: eth_wallet_address.clone(),
        added_at: naive_now(),
        avatar_url: None,
        profile_url: None,
        updated_at: naive_now(),
    };
    let from_record = from.create_or_update(&db).await.ok()?;

    let to: Identity = Identity {
        uuid: Some(Uuid::new_v4()),
        platform: Platform::Twitter,
        identity: item.twitter.handle.clone(),
        created_at: None,
        display_name: item.twitter.handle.clone(),
        added_at: naive_now(),
        avatar_url: None,
        profile_url: None,
        updated_at: naive_now(),
    };
    let to_record = to.create_or_update(&db).await.ok()?;

    let pf: Proof = Proof {
        uuid: Uuid::new_v4(),
        source: DataSource::SybilList,
        record_id: Some(item.twitter.tweet_id.clone()),
        created_at: Some(timestamp_to_naive(item.twitter.timestamp)),
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
impl Fetcher for SybilList {
    async fn fetch(&self, _url: Option<String>) -> Result<Vec<Connection>, Error> {
        let client = make_client();
        let uri: http::Uri = (C.upstream.sybil_service.url).parse().unwrap();

        let mut resp = client.get(uri).await?;

        if !resp.status().is_success() {
            let body: ErrorResponse = parse_body(&mut resp).await?;
            return Err(Error::General(
                format!("SybilList Get error: {}", body.message),
                resp.status(),
            ));
        }

        // all records in sybil list
        let body: Map<String, Value> = parse_body(&mut resp).await?;

        // parse
        let futures: Vec<_> = body
            .into_iter()
            .map(|(eth_wallet_address, value)| {
                save_item(eth_wallet_address.to_string(), value.to_owned())
            })
            .collect();
        let results = join_all(futures).await;
        let parse_body: Vec<Connection> = results.into_iter().filter_map(|i| i).collect();
        Ok(parse_body)
    }

    fn ability() -> Vec<(Platform, Vec<Platform>)> {
        let addr_connections: (Platform, Vec<Platform>) = (Platform::Ethereum, vec![Platform::Twitter]);
        let mut vec = Vec::new();
        vec.push(addr_connections);
        return vec;
    }
}
