use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use locus_core_rs::domain::contracts::NodeStore;
use stasis::infrastructure::memory::locus_context_reader::LocusContextReader;
use stasis::infrastructure::memory::locus_memory_operations::LocusMemoryOperations;
use stasis::infrastructure::memory::locus_node_store_factory::LocusMemoryStore;
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
use crate::runtime::persistent_locus::build_persistent_locus_memory;

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
        const IDENTITY_INIT_TIMEOUT: Duration = Duration::from_secs(120);

        let locus_memory = build_persistent_locus_memory(db.clone()).await?;

        let identity_store = timeout(
            IDENTITY_INIT_TIMEOUT,
            identity_memory::build_seeded_medousa_identity_store_for_db(db),
        )
        .await
                .map_err(|_| {
                    anyhow::anyhow!(
                        "identity memory init timed out after {}s at startup step `identity baseline probe` (increase MEDOUSA_SURREAL_STEP_TIMEOUT_SECS if remote Surreal is slow)",
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
