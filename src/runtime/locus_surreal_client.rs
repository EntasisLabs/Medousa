use std::sync::Arc;

use anyhow::{Result};
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

fn query_context_label(query: &str) -> String {
    let trimmed = query.trim();
    let head = trimmed.lines().next().unwrap_or(trimmed).trim();
    if head.len() > 160 {
        format!("{}…", &head[..160])
    } else {
        head.to_string()
    }
}

#[async_trait]
impl SurrealDbClient for StasisSurrealDbClient {
    async fn raw_query(&self, query: &str, parameters: QueryParams) -> Result<Vec<Value>> {
        let label = query_context_label(query);
        let mut request = self.db.query(query);
        for (key, value) in parameters {
            request = request.bind((key, value));
        }

        let mut response = request.await.map_err(|err| {
            anyhow::anyhow!("surreal query transport failed for `{label}`: {err}")
        })?;

        response = response.check().map_err(|err| {
            anyhow::anyhow!("surreal query rejected for `{label}`: {err}")
        })?;

        response.take(0).map_err(|err| {
            anyhow::anyhow!("surreal query result decode failed for `{label}`: {err}")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_context_label_truncates_multiline_create() {
        let query = "CREATE temporal_node:`abc` SET\n  tenant_id = $tenant_id,\n  session_id = $session_id;";
        let label = query_context_label(query);
        assert!(label.starts_with("CREATE temporal_node"));
        assert!(label.len() <= 161);
    }
}
