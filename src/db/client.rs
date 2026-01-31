use mongodb::{Client, Database};

pub struct MongoClient {
    client: Client,
    db: Database,
}

impl MongoClient {
    pub async fn new(url: &str, db_name: &str) -> Result<Self, mongodb::error::Error> {
        let client = Client::with_uri_str(url).await?;
        let db = client.database(db_name);

        Ok(Self { client, db })
    }

    pub fn database(&self) -> &Database {
        &self.db
    }

    pub fn client(&self) -> &Client {
        &self.client
    }
}
