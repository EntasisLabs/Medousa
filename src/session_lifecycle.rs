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
    let session_id = crate::session_storage::validate_session_id(session_id)
        .map_err(|error| error.to_string())?;

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
    crate::shared_session_catalog::delete_shared_row(session_id);
    crate::session_meta_store::delete_session_meta(session_id);
    crate::agent_mode_state::delete_session_mode_state(session_id);

    let mut failed_surfaces = Vec::new();
    record_surface_result(
        &mut failed_surfaces,
        "artifacts",
        crate::artifact_store::delete_artifacts_for_session(session_id),
    );
    record_surface_result(
        &mut failed_surfaces,
        "media",
        crate::media_store::delete_media_for_session(session_id),
    );
    record_surface_result(
        &mut failed_surfaces,
        "extractions",
        crate::artifact_extraction::delete_extractions_for_session(session_id),
    );
    record_surface_result(
        &mut failed_surfaces,
        "verifications",
        crate::verification_store::delete_verifications_for_session(session_id),
    );
    record_surface_result(
        &mut failed_surfaces,
        "context_packs",
        crate::context_pack::delete_context_packs_for_session(session_id),
    );
    record_surface_result(
        &mut failed_surfaces,
        "tool_surface",
        crate::tool_bootstrap::delete_session_tool_surface(session_id),
    );
    record_surface_result(
        &mut failed_surfaces,
        "turn_ledger",
        remove_turn_ledger_file(session_id),
    );
    crate::channel_session_store::purge_session_references(session_id);

    if !failed_surfaces.is_empty() {
        return Err(format!(
            "session deletion incomplete; retry required for: {}",
            failed_surfaces.join(", ")
        ));
    }

    Ok(SessionDeleteSummary {
        session_id: session_id.to_string(),
        deleted: true,
        locus_purged,
        locus_nodes_deleted,
        cancelled_active_turn,
    })
}

fn record_surface_result(
    failed_surfaces: &mut Vec<&'static str>,
    surface: &'static str,
    result: Result<(), String>,
) {
    if result.is_err() {
        failed_surfaces.push(surface);
    }
}

fn remove_turn_ledger_file(session_id: &str) -> Result<(), String> {
    crate::session_storage::remove_session_file(
        &medousa_data_dir().join("turn_ledger"),
        session_id,
        "jsonl",
    )
    .map_err(|error| format!("turn ledger delete failed: {error}"))
}

pub fn session_surfaces_path(session_id: &str) -> PathBuf {
    crate::session_storage::session_file_for_read(
        &medousa_data_dir().join("session_surfaces"),
        session_id,
        "json",
    )
}
