//! Bootstrap Stasis runtime Surreal tables required by the dashboard and scheduler.
//!
//! `RuntimeFactory::connect_surreal_any` only ensures identity-memory schema today.
//! Dashboard panels that list recurring definitions or load workflow builder state query
//! tables before any row exists, so we define them up front.

use stasis::prelude::RuntimeComposition;
use surrealdb::Surreal;
use surrealdb::engine::any::Any;

pub const DAEMON_PERSISTENCE_SCHEMA_REVISION: u32 = 1;

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
    "DEFINE TABLE recurring_feed_binding",
    "DEFINE TABLE cluster_node",
    "DEFINE TABLE cluster_forward_outcome",
];

async fn apply_schema_statements(db: &Surreal<Any>, statements: &[&str]) -> anyhow::Result<()> {
    for statement in statements {
        let result = match db.query(*statement).await {
            Ok(response) => response.check().map(|_| ()),
            Err(error) => Err(error),
        };
        if let Err(err) = result {
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

#[cfg(test)]
mod tests {
    use super::*;
    use stasis::prelude::{RuntimeBackend, RuntimeComposition, RuntimeFactory};

    #[tokio::test]
    async fn common_runtime_schema_is_fresh_and_idempotent() {
        let runtime = RuntimeFactory::build(RuntimeBackend::surreal_mem(
            "medousa-schema-tests",
            uuid::Uuid::new_v4().simple().to_string(),
        ))
        .await
        .expect("runtime");

        ensure_stasis_runtime_schema(&runtime)
            .await
            .expect("fresh schema");
        ensure_stasis_runtime_schema(&runtime)
            .await
            .expect("idempotent schema");

        let RuntimeComposition::Surreal(runtime) = runtime else {
            panic!("expected Surreal runtime");
        };
        for table in [
            "delivery_endpoint",
            "endpoint_delivery_status",
            "recurring_definition",
            "cluster_node",
        ] {
            runtime
                .job_store
                .db()
                .query(format!("SELECT * FROM {table} LIMIT 1"))
                .await
                .expect("query runtime table")
                .check()
                .expect("runtime table exists");
        }
        assert_eq!(DAEMON_PERSISTENCE_SCHEMA_REVISION, 1);
    }
}
