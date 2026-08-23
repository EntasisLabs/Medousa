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
    db.query("INFO FOR TABLE node").await.is_ok()
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

    Ok(Arc::new(LocusMemoryStore {
        node_store: node_store as Arc<dyn NodeStore>,
        semantic_index,
    }))
}
