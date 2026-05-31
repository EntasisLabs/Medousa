use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use locus_core_rs::storage::surrealdb::client::{QueryParams, SurrealDbClient};
use serde_json::Value;
use surrealdb::engine::any::Any;
use surrealdb::Surreal;

/// Bridges Stasis's shared `Surreal<Any>` handle to locus-core-rs `SurrealDbClient`.
#[derive(Clone)]
pub struct StasisSurrealDbClient {
    db: Surreal<Any>,
}

impl StasisSurrealDbClient {
    pub fn new(db: Surreal<Any>) -> Arc<Self> {
        Arc::new(Self { db })
    }
}

#[async_trait]
impl SurrealDbClient for StasisSurrealDbClient {
    async fn raw_query(&self, query: &str, parameters: QueryParams) -> Result<Vec<Value>> {
        let mut request = self.db.query(query);
        for (key, value) in parameters {
            request = request.bind((key, value));
        }

        let mut response = request
            .await
            .with_context(|| format!("surreal query failed: {query}"))?;

        response
            .take(0)
            .with_context(|| format!("surreal query result decode failed: {query}"))
    }
}
