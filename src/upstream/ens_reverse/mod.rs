#[cfg(test)]
mod tests;

use crate::{
    config::C,
    error::Error,
    graph::{new_db_connection, vertex::Identity, Vertex},
    util::{make_client, parse_body, request_with_timeout},
};
use async_trait::async_trait;
use hyper::{Body, Method};
use serde::Deserialize;
use tracing::info;

use super::{Fetcher, Platform, Target, TargetProcessedList};

#[derive(Deserialize, Debug, Clone)]
struct Response {
    #[serde(rename = "reverseRecord")]
    pub reverse_record: Option<String>,
    #[allow(unused)]
    pub domains: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct ENSReverseLookup {}

#[async_trait]
impl Fetcher for ENSReverseLookup {
    async fn fetch(target: &Target) -> Result<TargetProcessedList, Error> {
        if !Self::can_fetch(target) {
            return Ok(vec![]);
        }
        let wallet = target.identity().unwrap().to_lowercase();
        let record = fetch_record(&wallet).await?;
        // If reverse lookup record is reset to empty by user,
        // our cache should also be cleared.
        // Reach this by setting `display_name` into `Some("")`.
        let reverse_ens = record.reverse_record.unwrap_or("".into());

        info!("ENS Reverse record: {} => {}", wallet, reverse_ens);

        let mut identity = Identity::default();
        identity.platform = Platform::Ethereum;
        identity.identity = wallet.clone();
        identity.display_name = Some(reverse_ens);
        let db = new_db_connection().await?;
        identity.create_or_update(&db).await?;

        Ok(vec![])
    }

    fn can_fetch(target: &Target) -> bool {
        target.in_platform_supported(vec![Platform::Ethereum])
    }
}

async fn fetch_record(wallet: &str) -> Result<Response, Error> {
    let client = make_client();
    let url: http::Uri = format!("{}{}", C.upstream.ens_reverse.url, wallet)
        .parse()
        .map_err(|err: http::uri::InvalidUri| {
            Error::ParamError(format!("URI Format error: {}", err))
        })?;

    let req = hyper::Request::builder()
        .method(Method::GET)
        .uri(url)
        .body(Body::empty())
        .map_err(|_err| Error::ParamError(format!("ENSReverse Build Request Error {}", _err)))?;

    let mut resp = request_with_timeout(&client, req).await.map_err(|err| {
        Error::ManualHttpClientError(format!(
            "ENSReverse fetch | fetch_record error: {:?}",
            err.to_string()
        ))
    })?;

    if !resp.status().is_success() {
        return Err(Error::General(
            format!("ENSReverse fetch Error: {}", resp.status()),
            resp.status(),
        ));
    }
    parse_body(&mut resp).await
}
