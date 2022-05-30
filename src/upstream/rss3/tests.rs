mod tests {
    use crate::{error::Error, upstream::rss3::Rss3, upstream::Fetcher};

    #[tokio::test]
    async fn test_smoke_rss3() -> Result<(), Error> {

        let rs: Rss3 = Rss3 {
            account: "0x6875e13A6301040388F61f5DBa5045E1bE01c657".to_string(),
            network: "ethereum".to_string(),
        };

        let result = rs.fetch(None).await?;

        println!("{:?}", result);
        assert_ne!(result.len(), 0);
     
        Ok(())
    }
}