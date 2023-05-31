use crate::upstream::Platform;
use serde::{Deserialize, Serialize};
use strum_macros::{Display, EnumIter, EnumString};

/// All domain system name.
#[derive(
    Serialize,
    Deserialize,
    Debug,
    Clone,
    Display,
    EnumString,
    PartialEq,
    Eq,
    EnumIter,
    Default,
    Copy,
    async_graphql::Enum,
)]
pub enum DomainNameSystem {
    /// ENS name system on the ETH chain.
    /// https://ens.domains
    #[strum(serialize = "ENS")]
    #[serde(rename = "ENS")]
    #[graphql(name = "ENS")]
    ENS,

    /// https://www.did.id/
    #[strum(serialize = "dotbit")]
    #[serde(rename = "dotbit")]
    #[graphql(name = "dotbit")]
    DotBit,

    /// https://api.lens.dev/playground
    #[strum(serialize = "lens")]
    #[serde(rename = "lens")]
    #[graphql(name = "lens")]
    Lens,

    /// https://unstoppabledomains.com/
    #[strum(serialize = "unstoppabledomains")]
    #[serde(rename = "unstoppabledomains")]
    #[graphql(name = "unstoppabledomains")]
    UnstoppableDomains,

    /// https://api.prd.space.id/
    #[strum(serialize = "space_id")]
    #[serde(rename = "space_id")]
    #[graphql(name = "space_id")]
    SpaceId,

    #[default]
    #[strum(serialize = "unknown")]
    #[serde(rename = "unknown")]
    #[graphql(name = "unknown")]
    Unknown,
}

impl From<DomainNameSystem> for Platform {
    fn from(domain: DomainNameSystem) -> Self {
        match domain {
            DomainNameSystem::DotBit => Platform::Dotbit,
            DomainNameSystem::UnstoppableDomains => Platform::UnstoppableDomains,
            DomainNameSystem::Lens => Platform::Lens,
            DomainNameSystem::SpaceId => Platform::SpaceId,
            _ => Platform::Unknown,
        }
    }
}