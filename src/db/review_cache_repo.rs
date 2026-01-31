use bson::{doc, DateTime as BsonDateTime};
use mongodb::Collection;
use serde::{Deserialize, Serialize};

use crate::db::MongoClient;
use crate::types::{Diagnostic, Suggestion};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedReview {
    #[serde(rename = "_id")]
    pub cache_key: String,
    pub repo_url: String,
    pub commit_sha: String,
    pub results: Vec<Diagnostic>,
    pub suggestions: Vec<Suggestion>,
    pub created_at: BsonDateTime,
}

pub struct ReviewCacheRepository {
    client: MongoClient,
}

impl ReviewCacheRepository {
    pub fn new(client: MongoClient) -> Self {
        Self { client }
    }

    fn collection(&self) -> Collection<CachedReview> {
        self.client.database().collection("review_cache")
    }

    pub async fn get(&self, cache_key: &str) -> Result<Option<CachedReview>, mongodb::error::Error> {
        self.collection()
            .find_one(doc! { "_id": cache_key })
            .await
    }

    pub async fn save(
        &self,
        cache_key: &str,
        repo_url: &str,
        commit_sha: &str,
        results: &[Diagnostic],
        suggestions: &[Suggestion],
    ) -> Result<(), mongodb::error::Error> {
        let cached = CachedReview {
            cache_key: cache_key.to_string(),
            repo_url: repo_url.to_string(),
            commit_sha: commit_sha.to_string(),
            results: results.to_vec(),
            suggestions: suggestions.to_vec(),
            created_at: BsonDateTime::now(),
        };

        self.collection()
            .replace_one(doc! { "_id": cache_key }, &cached)
            .upsert(true)
            .await?;

        Ok(())
    }
}
