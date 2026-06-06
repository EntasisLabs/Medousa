//! Surreal schema bootstrap for vault link index (Phase V1).

use stasis::prelude::RuntimeComposition;
use surrealdb::engine::any::Any;
use surrealdb::Surreal;

const VAULT_LINK_TABLES: &[&str] = &[
    "DEFINE TABLE vault_note_link SCHEMAFULL",
    "DEFINE FIELD from_path ON vault_note_link TYPE string",
    "DEFINE FIELD to_path ON vault_note_link TYPE string",
    "DEFINE INDEX idx_vault_link_from ON vault_note_link COLUMNS from_path",
    "DEFINE INDEX idx_vault_link_to ON vault_note_link COLUMNS to_path",
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
            anyhow::bail!("vault surreal schema bootstrap failed on `{statement}`: {text}");
        }
    }
    Ok(())
}

pub async fn ensure_vault_surreal_schema(runtime: &RuntimeComposition) -> anyhow::Result<()> {
    match runtime {
        RuntimeComposition::Surreal(rt) => {
            apply_schema_statements(&rt.job_store.db(), VAULT_LINK_TABLES).await?;
            eprintln!("Vault Surreal tables ensured (vault_note_link)");
        }
        RuntimeComposition::InMemory(_) => {}
    }
    Ok(())
}
