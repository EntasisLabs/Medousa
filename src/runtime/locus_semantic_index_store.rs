use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use locus_core_rs::domain::contracts::{
    SemanticIndexStore, SemanticIndexStoreInitializer, TagEmbedding,
};
use locus_core_rs::domain::models::{
    SemanticTagNodeRef, SemanticTagQueryFilter, SemanticTagRecord,
};
use locus_core_rs::storage::SurrealDbSemanticIndexStore;
use locus_core_rs::storage::surrealdb::client::{QueryParams, SurrealDbClient};
use locus_core_rs::storage::surrealdb::raw_queries;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

const MATCH_ALL_TAGS_QUERY: &str = r#"
    SELECT SyncKey
    FROM (
        SELECT sync_key AS SyncKey, count() AS TagCount
        FROM semantic_tag_index
        WHERE tenant_id = $tenant_id
          AND tag IN $tags
          AND ($all_sessions = true OR session_id = $session_id)
        GROUP BY sync_key
    )
    WHERE TagCount = array::len($tags)
    LIMIT $limit;
"#;

const MATCH_ANY_TAGS_QUERY: &str = r#"
    SELECT sync_key AS SyncKey
    FROM semantic_tag_index
    WHERE tenant_id = $tenant_id
      AND tag IN $tags
      AND ($all_sessions = true OR session_id = $session_id)
    GROUP BY sync_key
    LIMIT $limit;
"#;

const UPDATE_TAG_ROW_QUERY: &str = r#"
    UPDATE semantic_tag_index
    SET tenant_id = $tenant_id,
        session_id = $session_id,
        node_id = $node_id,
        sync_key = $sync_key,
        tag = $tag,
        updated_at = time::now()
    WHERE tenant_id = $tenant_id AND sync_key = $sync_key AND tag = $tag;
"#;

const CREATE_TAG_ROW_QUERY: &str = r#"
    CREATE semantic_tag_index
    SET tenant_id = $tenant_id,
        session_id = $session_id,
        node_id = $node_id,
        sync_key = $sync_key,
        tag = $tag,
        updated_at = time::now();
"#;

const UPDATE_EMBEDDED_TAG_ROW_QUERY: &str = r#"
    UPDATE semantic_tag_index
    SET tenant_id = $tenant_id,
        session_id = $session_id,
        node_id = $node_id,
        sync_key = $sync_key,
        tag = $tag,
        embedding = $embedding,
        embedding_model = $embedding_model,
        embedding_dimensions = $embedding_dimensions,
        embedded_at = time::now(),
        updated_at = time::now()
    WHERE tenant_id = $tenant_id AND sync_key = $sync_key AND tag = $tag;
"#;

const CREATE_EMBEDDED_TAG_ROW_QUERY: &str = r#"
    CREATE semantic_tag_index
    SET tenant_id = $tenant_id,
        session_id = $session_id,
        node_id = $node_id,
        sync_key = $sync_key,
        tag = $tag,
        embedding = $embedding,
        embedding_model = $embedding_model,
        embedding_dimensions = $embedding_dimensions,
        embedded_at = time::now(),
        updated_at = time::now();
"#;

#[derive(Debug, Deserialize)]
struct TagRecord {
    #[serde(rename = "Tag", default)]
    tag: String,
}

#[derive(Debug, Deserialize)]
struct SyncKeyRecord {
    #[serde(rename = "SyncKey", default)]
    sync_key: String,
}

/// Typed compatibility adapter for the Locus semantic-index port.
///
/// Locus 0.5.0 emits SQL-style `HAVING`, `SELECT DISTINCT`, and table-level
/// `UPSERT ... WHERE` statements that are not compatible with the bundled
/// SurrealDB. Keep those dialect details behind the semantic port while
/// delegating the rest of Locus's implementation unchanged.
pub struct MedousaSurrealSemanticIndexStore {
    inner: SurrealDbSemanticIndexStore,
    client: Arc<dyn SurrealDbClient>,
}

impl MedousaSurrealSemanticIndexStore {
    pub fn new(client: Arc<dyn SurrealDbClient>) -> Self {
        Self {
            inner: SurrealDbSemanticIndexStore::new(client.clone()),
            client,
        }
    }

    fn canonical_tag(tag: &str) -> String {
        tag.trim().to_lowercase()
    }

    async fn write_tag(
        &self,
        node_ref: &SemanticTagNodeRef,
        tag: &str,
        embedding: Option<&TagEmbedding>,
    ) -> Result<()> {
        let mut params = QueryParams::new();
        params.insert("tenant_id".to_string(), json!(&node_ref.tenant_id));
        params.insert("session_id".to_string(), json!(&node_ref.session_id));
        params.insert("node_id".to_string(), json!(&node_ref.node_id));
        params.insert("sync_key".to_string(), json!(&node_ref.sync_key));
        params.insert("tag".to_string(), json!(tag));

        let (update_query, create_query) = if let Some(embedding) = embedding {
            params.insert("embedding".to_string(), json!(embedding.vector));
            params.insert("embedding_model".to_string(), json!(embedding.model));
            params.insert(
                "embedding_dimensions".to_string(),
                json!(embedding.vector.len()),
            );
            (UPDATE_EMBEDDED_TAG_ROW_QUERY, CREATE_EMBEDDED_TAG_ROW_QUERY)
        } else {
            (UPDATE_TAG_ROW_QUERY, CREATE_TAG_ROW_QUERY)
        };

        let updated = self.client.raw_query(update_query, params.clone()).await?;
        if !updated.is_empty() {
            return Ok(());
        }

        match self.client.raw_query(create_query, params.clone()).await {
            Ok(_) => Ok(()),
            Err(create_error) => {
                // Another writer may have created the unique tenant/sync/tag row
                // between our update and create. A successful retry confirms that
                // race without classifying errors by their message text.
                let recovered = self.client.raw_query(update_query, params).await?;
                if recovered.is_empty() {
                    Err(create_error)
                } else {
                    Ok(())
                }
            }
        }
    }
}

#[async_trait]
impl SemanticIndexStoreInitializer for MedousaSurrealSemanticIndexStore {
    async fn initialize_async(&self) -> Result<()> {
        self.inner.initialize_async().await
    }
}

#[async_trait]
impl SemanticIndexStore for MedousaSurrealSemanticIndexStore {
    async fn sync_node_tags_async(
        &self,
        node_ref: SemanticTagNodeRef,
        tags: &[String],
        embeddings: Option<&HashMap<String, TagEmbedding>>,
    ) -> Result<()> {
        let canonical_tags: HashSet<String> = tags
            .iter()
            .map(|tag| Self::canonical_tag(tag))
            .filter(|tag| !tag.is_empty())
            .collect();

        let mut list_params = QueryParams::new();
        list_params.insert("tenant_id".to_string(), json!(&node_ref.tenant_id));
        list_params.insert("sync_key".to_string(), json!(&node_ref.sync_key));
        let existing = decode_rows::<TagRecord>(
            self.client
                .raw_query(raw_queries::LIST_TAGS_FOR_SYNC_KEY_QUERY, list_params)
                .await?,
        )?;

        for record in existing {
            if canonical_tags.contains(&record.tag) {
                continue;
            }
            let mut params = QueryParams::new();
            params.insert("tenant_id".to_string(), json!(&node_ref.tenant_id));
            params.insert("sync_key".to_string(), json!(&node_ref.sync_key));
            params.insert("tag".to_string(), json!(record.tag));
            self.client
                .raw_query(
                    r#"
                    DELETE semantic_tag_index
                    WHERE tenant_id = $tenant_id AND sync_key = $sync_key AND tag = $tag;
                    "#,
                    params,
                )
                .await?;
        }

        for tag in canonical_tags {
            let embedding = embeddings.and_then(|values| {
                values.get(&tag).or_else(|| {
                    values
                        .iter()
                        .find(|(key, _)| Self::canonical_tag(key) == tag)
                        .map(|(_, value)| value)
                })
            });
            self.write_tag(&node_ref, &tag, embedding).await?;
        }

        Ok(())
    }

    async fn delete_node_tags_async(&self, tenant_id: &str, sync_key: &str) -> Result<()> {
        self.inner.delete_node_tags_async(tenant_id, sync_key).await
    }

    async fn find_sync_keys_by_tags_async(
        &self,
        tenant_id: &str,
        tags: &[String],
        match_all: bool,
        session_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<String>> {
        let canonical_tags: HashSet<String> = tags
            .iter()
            .map(|tag| Self::canonical_tag(tag))
            .filter(|tag| !tag.is_empty())
            .collect();
        if canonical_tags.is_empty() {
            return Ok(Vec::new());
        }

        let mut params = QueryParams::new();
        params.insert("tenant_id".to_string(), json!(tenant_id));
        params.insert("tags".to_string(), json!(canonical_tags));
        params.insert("all_sessions".to_string(), json!(session_id.is_none()));
        params.insert(
            "session_id".to_string(),
            session_id.map_or_else(|| json!(""), |value| json!(value)),
        );
        params.insert("limit".to_string(), json!(limit.max(1)));

        let query = if match_all {
            MATCH_ALL_TAGS_QUERY
        } else {
            MATCH_ANY_TAGS_QUERY
        };
        Ok(
            decode_rows::<SyncKeyRecord>(self.client.raw_query(query, params).await?)?
                .into_iter()
                .map(|record| record.sync_key)
                .collect(),
        )
    }

    async fn find_tags_async(
        &self,
        tenant_id: &str,
        prefix: Option<&str>,
        limit: usize,
    ) -> Result<Vec<String>> {
        self.inner.find_tags_async(tenant_id, prefix, limit).await
    }

    async fn query_tag_records_async(
        &self,
        filter: SemanticTagQueryFilter,
    ) -> Result<Vec<SemanticTagRecord>> {
        self.inner.query_tag_records_async(filter).await
    }
}

fn decode_rows<T: DeserializeOwned>(rows: Vec<Value>) -> Result<Vec<T>> {
    rows.into_iter()
        .map(serde_json::from_value)
        .collect::<std::result::Result<Vec<T>, _>>()
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct RecordingClient {
        calls: Mutex<Vec<(String, QueryParams)>>,
    }

    #[async_trait]
    impl SurrealDbClient for RecordingClient {
        async fn raw_query(&self, query: &str, parameters: QueryParams) -> Result<Vec<Value>> {
            self.calls
                .lock()
                .unwrap()
                .push((query.to_string(), parameters));
            Ok(vec![json!({ "SyncKey": "node-1" })])
        }
    }

    #[tokio::test]
    async fn tag_query_uses_an_explicit_all_sessions_flag() {
        let client = Arc::new(RecordingClient::default());
        let store = MedousaSurrealSemanticIndexStore::new(client.clone());

        let keys = store
            .find_sync_keys_by_tags_async("tenant", &[" durable ".to_string()], true, None, 8)
            .await
            .unwrap();
        assert_eq!(keys, ["node-1"]);

        let calls = client.calls.lock().unwrap();
        let (query, parameters) = calls.last().unwrap();
        assert!(query.contains("$all_sessions = true"));
        assert_eq!(parameters.get("all_sessions"), Some(&json!(true)));
        assert_eq!(parameters.get("session_id"), Some(&json!("")));
    }

    #[tokio::test]
    async fn tag_query_preserves_a_scoped_session_filter() {
        let client = Arc::new(RecordingClient::default());
        let store = MedousaSurrealSemanticIndexStore::new(client.clone());

        store
            .find_sync_keys_by_tags_async(
                "tenant",
                &["durable".to_string()],
                false,
                Some("session-1"),
                8,
            )
            .await
            .unwrap();

        let calls = client.calls.lock().unwrap();
        let (_, parameters) = calls.last().unwrap();
        assert_eq!(parameters.get("all_sessions"), Some(&json!(false)));
        assert_eq!(parameters.get("session_id"), Some(&json!("session-1")));
    }
}
