use crate::upstream::Target;
use crate::{error::Error, upstream::dotbit::DotBit, upstream::Fetcher};
use crate::{
    graph::new_db_connection, graph::vertex::Identity, upstream::Platform, util::naive_now,
};

#[tokio::test]
async fn test_smoke_dotbit() -> Result<(), Error> {
    let target = Target::Identity(Platform::Dotbit, "test0920.bit".into());

    DotBit::fetch(&target).await?;

    let db = new_db_connection().await?;
    let found = Identity::find_by_platform_identity(&db, &target.platform()?, &target.identity()?)
        .await?
        .expect("Record not found");

    assert_eq!(found.updated_at.timestamp(), naive_now().timestamp());

    Ok(())
}