//! Shared persistent Locus composition over a Stasis-owned Surreal handle.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use locus_core_rs::domain::contracts::{
    NodeStore, NodeStoreInitializer, SemanticIndexStoreInitializer,
};
use locus_core_rs::storage::surrealdb::node_store::SurrealDbNodeStore;
use stasis::infrastructure::memory::locus_node_store_factory::LocusMemoryStore;
use tokio::time::timeout;

#[cfg(not(feature = "full-daemon"))]
use crate::locus_semantic_index_store::MedousaSurrealSemanticIndexStore;
#[cfg(not(feature = "full-daemon"))]
use crate::locus_surreal_client::StasisSurrealDbClient;
#[cfg(feature = "full-daemon")]
use crate::runtime::locus_semantic_index_store::MedousaSurrealSemanticIndexStore;
#[cfg(feature = "full-daemon")]
use crate::runtime::locus_surreal_client::StasisSurrealDbClient;
#[cfg(feature = "full-daemon")]
use crate::runtime::surreal_startup::{timed_step, verify_surreal_responsive};
#[cfg(not(feature = "full-daemon"))]
use crate::surreal_startup::{timed_step, verify_surreal_responsive};

const LOCUS_SCHEMA_COMPATIBILITY_STATEMENTS: &[&str] = &[
    "DEFINE FIELD OVERWRITE source_metadata ON TABLE temporal_node TYPE option<object> FLEXIBLE",
    "DEFINE FIELD OVERWRITE semantic_links ON TABLE temporal_node TYPE option<array<object>> FLEXIBLE",
    // Older Surreal versions retain the generated strict array-item definition
    // when only its parent is overwritten. Repair that definition explicitly.
    "DEFINE FIELD OVERWRITE semantic_links.* ON TABLE temporal_node TYPE object FLEXIBLE",
];

fn parse_env_flag(key: &str) -> Option<bool> {
    std::env::var(key).ok().map(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

async fn surreal_locus_node_table_exists(
    db: &surrealdb::Surreal<surrealdb::engine::any::Any>,
) -> bool {
    let Ok(mut response) = db.query("INFO FOR DB").await else {
        return false;
    };
    let Ok(Some(info)) = response.take::<Option<serde_json::Value>>(0) else {
        return false;
    };
    info["tables"].get("temporal_node").is_some()
}

async fn ensure_locus_schema_compatibility(
    db: &surrealdb::Surreal<surrealdb::engine::any::Any>,
) -> Result<()> {
    for statement in LOCUS_SCHEMA_COMPATIBILITY_STATEMENTS {
        db.query(*statement)
            .await
            .with_context(|| format!("apply Locus schema compatibility statement `{statement}`"))?
            .check()
            .with_context(|| format!("apply Locus schema compatibility statement `{statement}`"))?;
    }
    Ok(())
}

async fn should_skip_locus_init_on_existing_graph(
    db: &surrealdb::Surreal<surrealdb::engine::any::Any>,
) -> bool {
    if parse_env_flag("MEDOUSA_FORCE_LOCUS_INIT_ON_DAEMON") == Some(true) {
        return false;
    }
    if parse_env_flag("MEDOUSA_SKIP_LOCUS_INIT_ON_DAEMON") == Some(true) {
        return true;
    }
    surreal_locus_node_table_exists(db).await
}

/// Build the daemon's persistent Locus node and semantic-index stores over the
/// same Surreal connection that owns Stasis control-plane state.
pub async fn build_persistent_locus_memory(
    db: surrealdb::Surreal<surrealdb::engine::any::Any>,
) -> Result<Arc<LocusMemoryStore>> {
    const LOCUS_INIT_TIMEOUT: Duration = Duration::from_secs(180);

    verify_surreal_responsive(&db)
        .await
        .context("surreal connection not responsive before memory adapters")?;

    let client = StasisSurrealDbClient::new(db.clone());
    let node_store = Arc::new(SurrealDbNodeStore::new(client.clone()));
    let semantic_index = Arc::new(MedousaSurrealSemanticIndexStore::new(client));

    let skip_locus = timed_step("locus table probe", || async {
        Ok(should_skip_locus_init_on_existing_graph(&db).await)
    })
    .await?;
    if skip_locus {
        let message = "skipping Locus initialize_async (graph tables already present — avoids temporal_node/calibration backfill scan; set MEDOUSA_FORCE_LOCUS_INIT_ON_DAEMON=1 to override)";
        eprintln!("medousa-daemon: {message}");
        tracing::info!("{message}");
    } else {
        let force = parse_env_flag("MEDOUSA_FORCE_LOCUS_INIT_ON_DAEMON") == Some(true);
        eprintln!(
            "medousa-daemon: initializing Locus graph schema (can be slow on large remote DBs)…"
        );
        tracing::info!(
            force_locus_init = force,
            "initializing Locus graph schema on Surreal backend"
        );
        let node_initializer: Arc<dyn NodeStoreInitializer> = node_store.clone();
        timeout(LOCUS_INIT_TIMEOUT, node_initializer.initialize_async())
            .await
            .map_err(|_| {
                anyhow::anyhow!(
                    "Locus schema init timed out after {}s — large DB backfill may be running; retry or set MEDOUSA_SKIP_LOCUS_INIT_ON_DAEMON=1",
                    LOCUS_INIT_TIMEOUT.as_secs()
                )
            })?
            .map_err(|err| anyhow::anyhow!("failed to initialize surreal locus schema: {err}"))?;
        eprintln!("medousa-daemon: Locus graph schema ready");
        tracing::info!("Locus graph schema ready");
    }

    let index_initializer: Arc<dyn SemanticIndexStoreInitializer> = semantic_index.clone();
    timeout(
        Duration::from_secs(60),
        index_initializer.initialize_async(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Locus semantic index init timed out"))?
    .map_err(|err| anyhow::anyhow!("failed to initialize surreal semantic index: {err}"))?;

    ensure_locus_schema_compatibility(&db)
        .await
        .context("align Locus object field schema")?;

    Ok(Arc::new(LocusMemoryStore {
        node_store: node_store as Arc<dyn NodeStore>,
        semantic_index,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn table_probe_requires_an_existing_table_even_when_it_has_no_rows() {
        let db = surrealdb::engine::any::connect("mem://").await.unwrap();
        db.use_ns("compatibility").use_db("probe").await.unwrap();
        assert!(!surreal_locus_node_table_exists(&db).await);
        db.query("DEFINE TABLE temporal_node SCHEMAFULL")
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(surreal_locus_node_table_exists(&db).await);
    }

    #[tokio::test]
    async fn compatibility_repairs_existing_nested_array_schema_and_preserves_links() {
        let db = surrealdb::engine::any::connect("mem://").await.unwrap();
        db.use_ns("compatibility").use_db("existing").await.unwrap();
        db.query("DEFINE TABLE temporal_node SCHEMAFULL; DEFINE FIELD semantic_links ON temporal_node TYPE option<array<object>>; DEFINE FIELD source_metadata ON temporal_node TYPE option<object>; DEFINE FIELD OVERWRITE semantic_links.* ON temporal_node TYPE object;")
            .await.unwrap().check().unwrap();
        ensure_locus_schema_compatibility(&db).await.unwrap();
        ensure_locus_schema_compatibility(&db).await.unwrap();
        let links = serde_json::json!([{"rel":"related_to","target":"concept:memory-schema","confidence":0.88}]);
        db.query(
            "CREATE temporal_node:compat SET semantic_links = $links, source_metadata = $metadata",
        )
        .bind(("links", links.clone()))
        .bind((
            "metadata",
            serde_json::json!({"source":"test","nested":{"kept":true}}),
        ))
        .await
        .unwrap()
        .check()
        .unwrap();
        let mut response = db
            .query("SELECT semantic_links, source_metadata FROM ONLY temporal_node:compat")
            .await
            .unwrap();
        let row: Option<serde_json::Value> = response.take(0).unwrap();
        let row = row.unwrap();
        assert_eq!(row["semantic_links"], links);
        assert_eq!(row["source_metadata"]["nested"]["kept"], true);
    }
}
