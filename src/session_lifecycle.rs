//! Session delete orchestration — transcript, catalog, Locus purge, satellites.

use std::path::PathBuf;
use std::sync::Arc;

use stasis::ports::outbound::memory::memory_models::{
    MemoryEvictMode, MemoryEvictRequest, MemoryFilter, MemoryScope,
};
use stasis::ports::outbound::memory::memory_operations::MemoryOperations;

use crate::locus_memory::{derive_locus_tenant_id, resolve_workshop_locus_session, LOCUS_DEFAULT_TENANT};
use crate::session::medousa_data_dir;
use crate::turn_ticket::TurnTicketRegistry;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionDeleteSummary {
    pub session_id: String,
    pub deleted: bool,
    pub locus_purged: bool,
    pub locus_nodes_deleted: usize,
    pub cancelled_active_turn: bool,
}

pub async fn delete_session(
    session_id: &str,
    memory_operations: Option<Arc<dyn MemoryOperations>>,
    turn_tickets: &TurnTicketRegistry,
    purge_locus: bool,
) -> Result<SessionDeleteSummary, String> {
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return Err("session_id is required".to_string());
    }

    let mut cancelled_active_turn = false;
    if crate::turn_ticket::get_active_interactive_turn(turn_tickets, session_id)
        .await
        .active
    {
        crate::turn_ticket::cancel_interactive_for_session(turn_tickets, session_id).await;
        cancelled_active_turn = true;
    }

    let mut locus_nodes_deleted = 0;
    let mut locus_purged = false;
    if purge_locus
        && let Some(ops) = memory_operations {
            let locus_session = resolve_workshop_locus_session(session_id);
            let tenant = derive_locus_tenant_id(&locus_session);
            let mut scope = MemoryScope {
                session_ids: Some(vec![locus_session]),
                ..Default::default()
            };
            if tenant != LOCUS_DEFAULT_TENANT {
                scope.tenant_id = Some(tenant);
            }
            let response = ops
                .evict(&MemoryEvictRequest {
                    mode: MemoryEvictMode::PurgeSession,
                    scope,
                    filter: MemoryFilter::default(),
                    dry_run: false,
                    force: true,
                    max_nodes: 50_000,
                    include_calibration: true,
                    include_checkpoints: true,
                    ..Default::default()
                })
                .await
                .map_err(|err| format!("locus purge failed: {err}"))?;
            locus_nodes_deleted = response.deleted;
            locus_purged = true;
        }

    crate::session_store::delete_session_transcript(session_id);
    crate::session_catalog::delete_catalog_row(session_id);
    crate::session_meta_store::delete_session_meta(session_id);
    crate::verification_store::delete_verifications_for_session(session_id);
    crate::tool_bootstrap::delete_session_tool_surface(session_id);
    remove_turn_ledger_dir(session_id);
    remove_session_history_file(session_id);
    crate::channel_session_store::purge_session_references(session_id);

    Ok(SessionDeleteSummary {
        session_id: session_id.to_string(),
        deleted: true,
        locus_purged,
        locus_nodes_deleted,
        cancelled_active_turn,
    })
}

fn remove_session_history_file(session_id: &str) {
    let path = medousa_data_dir()
        .join("history")
        .join(format!("{session_id}.jsonl"));
    let _ = std::fs::remove_file(path);
}

fn remove_turn_ledger_dir(session_id: &str) {
    let path = crate::agent_runtime::turn_ledger::turn_ledger_path(session_id);
    let _ = std::fs::remove_dir_all(path);
}

pub fn session_surfaces_path(session_id: &str) -> PathBuf {
    medousa_data_dir()
        .join("session_surfaces")
        .join(format!("{session_id}.json"))
}
