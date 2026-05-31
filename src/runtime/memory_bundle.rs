use std::sync::Arc;

use anyhow::Result;
use locus_core_rs::domain::contracts::NodeStoreInitializer;
use locus_core_rs::storage::surrealdb::node_store::SurrealDbNodeStore;
use locus_core_rs::NodeStore;
use stasis::infrastructure::memory::locus_context_reader::LocusContextReader;
use stasis::infrastructure::memory::locus_context_writer::LocusContextWriter;
use stasis::infrastructure::memory::locus_memory_operations::LocusMemoryOperations;
use stasis::ports::outbound::memory::identity_memory_store::IdentityMemoryStore;
use stasis::ports::outbound::memory::memory_context_reader::MemoryContextReader;
use stasis::ports::outbound::memory::memory_context_writer::MemoryContextWriter;
use stasis::ports::outbound::memory::memory_operations::MemoryOperations;
use stasis::prelude::{RuntimeBackend, RuntimeComposition, RuntimeFactory};
use stasis::prelude_ext::LocusNodeStoreFactory;

use crate::identity_memory;
use crate::runtime::locus_surreal_client::StasisSurrealDbClient;

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
        let client = StasisSurrealDbClient::new(db.clone());
        let node_store = Arc::new(SurrealDbNodeStore::new(client));
        node_store
            .initialize_async()
            .await
            .map_err(|err| anyhow::anyhow!("failed to initialize surreal locus schema: {err}"))?;

        let locus_store: Arc<dyn NodeStore> = node_store;
        let identity_store =
            identity_memory::build_seeded_identity_memory_store_for_runtime(
                &RuntimeFactory::from_db(db),
            )
            .await?;

        Ok(Self::from_locus_and_identity(locus_store, identity_store))
    }

    fn from_locus_and_identity(
        locus_store: Arc<dyn NodeStore>,
        identity_store: Arc<dyn IdentityMemoryStore>,
    ) -> Self {
        let memory_reader: Arc<dyn MemoryContextReader> =
            Arc::new(LocusContextReader::new(locus_store.clone()));
        let memory_writer: Arc<dyn MemoryContextWriter> =
            Arc::new(LocusContextWriter::new(locus_store.clone()));
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
