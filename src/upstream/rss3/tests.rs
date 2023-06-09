use crate::{
    error::Error,
    graph::edge::Hold,
    graph::new_db_connection,
    graph::vertex::{contract::Chain, Contract, Identity},
    upstream::rss3::Rss3,
    upstream::Platform,
    upstream::{Fetcher, Target},
};

#[tokio::test]
async fn test_smoke_nft_rss3() -> Result<(), Error> {
    let target = Target::Identity(
        Platform::Ethereum,
        "0x934b510d4c9103e6a87aef13b816fb080286d649".to_lowercase(),
    );
    let _ = Rss3::fetch(&target).await?;
    let db = new_db_connection().await?;

    let owner = Identity::find_by_platform_identity(&db, &Platform::Ethereum, &target.identity()?)
        .await?
        .expect("Record not found");
    let contract = Contract::find_by_chain_address(
        &db,
        &Chain::Ethereum,
        "0x596cfe8d6709a86d51ff0c18ebf0e66561b08ae3",
    )
    .await?
    .unwrap();

    let _ = Hold::find_by_from_to_id(&db, &owner, &contract, "87")
        .await
        .expect("Record not found");

    Ok(())
}
