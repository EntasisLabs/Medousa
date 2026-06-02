//! Bootstrap Stasis runtime Surreal tables required by the dashboard and scheduler.
//!
//! `RuntimeFactory::connect_surreal_any` only ensures identity-memory schema today.
//! Dashboard panels that list recurring definitions or load workflow builder state query
//! tables before any row exists, so we define them up front.

use stasis::prelude::RuntimeComposition;
use surrealdb::engine::any::Any;
use surrealdb::Surreal;

/// Schemaless table definitions for Stasis runtime stores. Matches table names in stasis-rs
/// (`surreal_recurring_store`, `surreal_workflow_definition_store`, etc.).
const STASIS_RUNTIME_TABLES: &[&str] = &[
    "DEFINE TABLE job",
    "DEFINE TABLE job_attempt",
    "DEFINE TABLE outbox_event",
    "DEFINE TABLE recurring_definition",
    "DEFINE TABLE workflow_definition",
    "DEFINE TABLE workflow_revision",
    "DEFINE TABLE thread",
    "DEFINE TABLE thread_event",
    "DEFINE TABLE delivery_endpoint",
    "DEFINE TABLE endpoint_delivery_status",
    "DEFINE TABLE recurring_delivery_binding",
    "DEFINE TABLE cluster_node",
    "DEFINE TABLE cluster_forward_outcome",
];

async fn apply_schema_statements(db: &Surreal<Any>, statements: &[&str]) -> anyhow::Result<()> {
    for statement in statements {
        if let Err(err) = db.query(*statement).await {
            let text = err.to_string();
            if text.contains("already exists")
                || text.contains("already defined")
                || text.contains("Overwrite index")
            {
                continue;
            }
            anyhow::bail!("stasis surreal schema bootstrap failed on `{statement}`: {text}");
        }
    }
    Ok(())
}

pub async fn ensure_stasis_runtime_schema(runtime: &RuntimeComposition) -> anyhow::Result<()> {
    match runtime {
        RuntimeComposition::Surreal(rt) => {
            apply_schema_statements(&rt.job_store.db(), STASIS_RUNTIME_TABLES).await?;
            eprintln!("Stasis runtime Surreal tables ensured (dashboard + scheduler)");
        }
        RuntimeComposition::InMemory(_) => {}
    }
    Ok(())
}
