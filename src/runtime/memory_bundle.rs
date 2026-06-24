use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use locus_core_rs::domain::contracts::{NodeStore, NodeStoreInitializer, SemanticIndexStoreInitializer};
use locus_core_rs::storage::surrealdb::node_store::SurrealDbNodeStore;
use locus_core_rs::storage::SurrealDbSemanticIndexStore;
use stasis::infrastructure::memory::locus_context_reader::LocusContextReader;
use stasis::infrastructure::memory::locus_node_store_factory::LocusMemoryStore;
use stasis::infrastructure::memory::locus_memory_operations::LocusMemoryOperations;
use stasis::ports::outbound::memory::identity_memory_store::IdentityMemoryStore;
use stasis::ports::outbound::memory::memory_context_reader::MemoryContextReader;
use stasis::ports::outbound::memory::memory_context_writer::MemoryContextWriter;
use stasis::ports::outbound::memory::memory_operations::MemoryOperations;
use stasis::prelude::{RuntimeBackend, RuntimeComposition, RuntimeFactory};
use stasis::prelude_ext::LocusNodeStoreFactory;
use tokio::time::timeout;

use crate::identity_memory;
use crate::identity_store_ext::MedousaIdentityMemoryStore;
use crate::locus_memory::{MedousaLocusContextWriter, resolve_locus_ingest_profile};
use crate::runtime::locus_surreal_client::StasisSurrealDbClient;
use crate::runtime::surreal_startup::{timed_step, verify_surreal_responsive};

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
    pub locus_memory: Arc<LocusMemoryStore>,
    pub memory_reader: Arc<dyn MemoryContextReader>,
    pub memory_writer: Arc<dyn MemoryContextWriter>,
    pub memory_operations: Arc<dyn MemoryOperations>,
    pub identity_store: Arc<MedousaIdentityMemoryStore>,
}

impl MemoryAdapterBundle {
    pub fn node_store(&self) -> Arc<dyn NodeStore> {
        self.locus_memory.node_store.clone()
    }

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
        let locus_memory = LocusNodeStoreFactory::in_memory().await?;
        let identity_store = identity_memory::build_seeded_medousa_identity_store()?;
        Ok(Self::from_locus_and_identity(locus_memory, identity_store))
    }

    async fn from_surreal_db(db: surrealdb::Surreal<surrealdb::engine::any::Any>) -> Result<Self> {
        const LOCUS_INIT_TIMEOUT: Duration = Duration::from_secs(180);
        const IDENTITY_INIT_TIMEOUT: Duration = Duration::from_secs(120);

        verify_surreal_responsive(&db)
            .await
            .context("surreal connection not responsive before memory adapters")?;

        let client = StasisSurrealDbClient::new(db.clone());
        let node_store = Arc::new(SurrealDbNodeStore::new(client.clone()));
        let semantic_index = Arc::new(SurrealDbSemanticIndexStore::new(client));

        let skip_locus = timed_step("locus table probe", || async {
            Ok(should_skip_locus_init_on_existing_graph(&db).await)
        })
        .await?;
        if skip_locus {
            eprintln!(
                "medousa-daemon: skipping Locus initialize_async (graph tables already present — avoids temporal_node/calibration backfill scan; set MEDOUSA_FORCE_LOCUS_INIT_ON_DAEMON=1 to override)"
            );
        } else {
            eprintln!(
                "medousa-daemon: initializing Locus graph schema (can be slow on large remote DBs)…"
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
        }

        let index_initializer: Arc<dyn SemanticIndexStoreInitializer> = semantic_index.clone();
        timeout(Duration::from_secs(60), index_initializer.initialize_async())
            .await
            .map_err(|_| anyhow::anyhow!("Locus semantic index init timed out"))?
            .map_err(|err| anyhow::anyhow!("failed to initialize surreal semantic index: {err}"))?;

        let locus_memory = Arc::new(LocusMemoryStore {
            node_store: node_store as Arc<dyn NodeStore>,
            semantic_index,
        });

        let identity_store = timeout(
            IDENTITY_INIT_TIMEOUT,
            identity_memory::build_seeded_medousa_identity_store_for_db(db),
        )
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "identity memory init timed out after {}s — see last `surreal step` / `identity upsert` line for the wedged query",
                IDENTITY_INIT_TIMEOUT.as_secs()
            )
        })?
        .context("failed to build seeded identity memory store for surreal runtime")?;
        eprintln!("medousa-daemon: identity memory ready");

        Ok(Self::from_locus_and_identity(locus_memory, identity_store))
    }

    fn from_locus_and_identity(
        locus_memory: Arc<LocusMemoryStore>,
        identity_store: Arc<MedousaIdentityMemoryStore>,
    ) -> Self {
        let ingest_profile = resolve_locus_ingest_profile();
        let memory_reader: Arc<dyn MemoryContextReader> =
            Arc::new(LocusContextReader::new(locus_memory.clone()));
        let memory_writer: Arc<dyn MemoryContextWriter> = Arc::new(MedousaLocusContextWriter::new(
            locus_memory.node_store.clone(),
            ingest_profile,
        ));
        let memory_operations: Arc<dyn MemoryOperations> =
            Arc::new(LocusMemoryOperations::new(locus_memory.clone(), None));

        Self {
            locus_memory,
            memory_reader,
            memory_writer,
            memory_operations,
            identity_store,
        }
    }

    pub fn identity_store_dyn(&self) -> Arc<dyn IdentityMemoryStore> {
        self.identity_store.clone() as Arc<dyn IdentityMemoryStore>
    }
}
