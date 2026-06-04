use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::time::timeout;
use locus_core_rs::domain::contracts::NodeStoreInitializer;
use locus_core_rs::storage::surrealdb::node_store::SurrealDbNodeStore;
use locus_core_rs::NodeStore;
use stasis::infrastructure::memory::locus_context_reader::LocusContextReader;
use stasis::infrastructure::memory::locus_memory_operations::LocusMemoryOperations;
use stasis::ports::outbound::memory::identity_memory_store::IdentityMemoryStore;
use stasis::ports::outbound::memory::memory_context_reader::MemoryContextReader;
use stasis::ports::outbound::memory::memory_context_writer::MemoryContextWriter;
use stasis::ports::outbound::memory::memory_operations::MemoryOperations;
use stasis::prelude::{RuntimeBackend, RuntimeComposition, RuntimeFactory};
use stasis::prelude_ext::LocusNodeStoreFactory;

use crate::identity_memory;
use crate::locus_memory::{MedousaLocusContextWriter, resolve_locus_ingest_profile};
use crate::runtime::locus_surreal_client::StasisSurrealDbClient;

fn parse_env_flag(key: &str) -> Option<bool> {
    std::env::var(key).ok().map(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

/// Skip full Locus `initialize_async` (includes heavy backfill scans on existing data).
async fn surreal_locus_node_table_exists(db: &surrealdb::Surreal<surrealdb::engine::any::Any>) -> bool {
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
    // Production DBs (e.g. keryx/sttp_mcp) already have `node` — backfill can scan millions of rows.
    surreal_locus_node_table_exists(db).await
}

/// Shared memory adapters wired into Stasis and the agent tool surface.
pub struct MemoryAdapterBundle {
    pub locus_store: Arc<dyn NodeStore>,
    pub memory_reader: Arc<dyn MemoryContextReader>,
    pub memory_writer: Arc<dyn MemoryContextWriter>,
    pub memory_operations: Arc<dyn MemoryOperations>,
    pub identity_store: Arc<dyn IdentityMemoryStore>,
}

impl MemoryAdapterBundle {
    pub async fn build_for_backend(backend: &RuntimeBackend) -> Result<Self> {
        match backend {
            RuntimeBackend::SurrealKv { .. }
            | RuntimeBackend::SurrealWs { .. }
            | RuntimeBackend::SurrealMem { .. } => {
                let shell = RuntimeFactory::build(backend.clone()).await?;
                Self::from_runtime_shell(&shell).await
            }
            _ => Self::build_in_memory().await,
        }
    }

    pub async fn from_runtime_shell(runtime: &RuntimeComposition) -> Result<Self> {
        match runtime {
            RuntimeComposition::Surreal(rt) => {
                let db = rt.job_store.db();
                Self::from_surreal_db(db).await
            }
            _ => Self::build_in_memory().await,
        }
    }

    pub async fn build_in_memory() -> Result<Self> {
        let locus_store = LocusNodeStoreFactory::in_memory().await?;
        let identity_store = identity_memory::build_seeded_identity_memory_store()?;
        Ok(Self::from_locus_and_identity(locus_store, identity_store))
    }

    async fn from_surreal_db(db: surrealdb::Surreal<surrealdb::engine::any::Any>) -> Result<Self> {
        const LOCUS_INIT_TIMEOUT: Duration = Duration::from_secs(180);
        const IDENTITY_INIT_TIMEOUT: Duration = Duration::from_secs(120);

        let client = StasisSurrealDbClient::new(db.clone());
        let node_store = Arc::new(SurrealDbNodeStore::new(client));
        if should_skip_locus_init_on_existing_graph(&db).await {
            eprintln!(
                "medousa-daemon: skipping Locus initialize_async (graph tables already present — avoids temporal_node/calibration backfill scan; set MEDOUSA_FORCE_LOCUS_INIT_ON_DAEMON=1 to override)"
            );
        } else {
            eprintln!(
                "medousa-daemon: initializing Locus graph schema (can be slow on large remote DBs)…"
            );
            timeout(LOCUS_INIT_TIMEOUT, node_store.initialize_async())
                .await
                .map_err(|_| {
                    anyhow::anyhow!(
                        "Locus schema init timed out after {}s — large DB backfill may be running; retry or set MEDOUSA_SKIP_LOCUS_INIT_ON_DAEMON=1",
                        LOCUS_INIT_TIMEOUT.as_secs()
                    )
                })?
                .map_err(|err| anyhow::anyhow!("failed to initialize surreal locus schema: {err}"))?;
            eprintln!("medousa-daemon: Locus graph schema ready");
        }

        let locus_store: Arc<dyn NodeStore> = node_store;
        eprintln!("medousa-daemon: seeding identity memory on Surreal…");
        let identity_store = timeout(
            IDENTITY_INIT_TIMEOUT,
            identity_memory::build_seeded_identity_memory_store_for_runtime(
                &RuntimeFactory::from_db(db),
            ),
        )
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "identity memory init timed out after {}s",
                IDENTITY_INIT_TIMEOUT.as_secs()
            )
        })?
        .context("failed to build seeded identity memory store for surreal runtime")?;
        eprintln!("medousa-daemon: identity memory ready");

        Ok(Self::from_locus_and_identity(locus_store, identity_store))
    }

    fn from_locus_and_identity(
        locus_store: Arc<dyn NodeStore>,
        identity_store: Arc<dyn IdentityMemoryStore>,
    ) -> Self {
        let ingest_profile = resolve_locus_ingest_profile();
        let memory_reader: Arc<dyn MemoryContextReader> =
            Arc::new(LocusContextReader::new(locus_store.clone()));
        let memory_writer: Arc<dyn MemoryContextWriter> = Arc::new(MedousaLocusContextWriter::new(
            locus_store.clone(),
            ingest_profile,
        ));
        let memory_operations: Arc<dyn MemoryOperations> =
            Arc::new(LocusMemoryOperations::new(locus_store.clone(), None));

        Self {
            locus_store,
            memory_reader,
            memory_writer,
            memory_operations,
            identity_store,
        }
    }
}
