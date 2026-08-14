//! Durable session deletion across the complete registered surface inventory.

use std::sync::Arc;

use medousa_types::daemon_api::{SessionDeletionStatus, SessionDeletionSurfaceResult};
use stasis::ports::outbound::memory::memory_models::{
    MemoryEvictMode, MemoryEvictRequest, MemoryFilter, MemoryScope,
};
use stasis::ports::outbound::memory::memory_operations::MemoryOperations;

use crate::locus_memory::{
    LOCUS_DEFAULT_TENANT, derive_locus_tenant_id, resolve_workshop_locus_session,
};
use crate::session_deletion::{BeginDeletion, SessionDeletionRecord};
use crate::turn_ticket::TurnTicketRegistry;

#[derive(Debug, Clone)]
pub struct SessionDeleteSummary {
    pub session_id: String,
    pub deletion_id: String,
    pub status: SessionDeletionStatus,
    pub deleted: bool,
    pub locus_purged: bool,
    pub locus_nodes_deleted: usize,
    pub cancelled_active_turn: bool,
    pub surfaces: Vec<SessionDeletionSurfaceResult>,
}

impl SessionDeleteSummary {
    pub(crate) fn from_record(record: SessionDeletionRecord) -> Self {
        Self {
            session_id: record.session_id,
            deletion_id: record.deletion_id,
            status: record.status,
            deleted: record.status == SessionDeletionStatus::Complete,
            locus_purged: record.locus_purged,
            locus_nodes_deleted: record.locus_nodes_deleted,
            cancelled_active_turn: record.cancelled_active_turn,
            surfaces: record.surfaces,
        }
    }
}

pub async fn delete_session(
    session_id: &str,
    memory_operations: Option<Arc<dyn MemoryOperations>>,
    turn_tickets: &TurnTicketRegistry,
    turn_streams: Option<&crate::daemon::turn_stream_registry::TurnStreamRegistry>,
    purge_locus: bool,
) -> Result<SessionDeleteSummary, String> {
    let session_id =
        crate::session_storage::SessionId::parse(session_id).map_err(|error| error.to_string())?;
    let session_id_text = session_id.as_str();
    let begin = tokio::task::block_in_place(|| {
        crate::session_deletion::coordinator().begin_deletion(&session_id, purge_locus)
    })?;
    let mut deletion = match begin {
        BeginDeletion::Owner(deletion) => deletion,
        BeginDeletion::AlreadyActive(record) | BeginDeletion::Complete(record) => {
            return Ok(SessionDeleteSummary::from_record(record));
        }
    };

    let active = crate::turn_ticket::get_active_interactive_turn(turn_tickets, session_id_text)
        .await
        .active;
    let cancelled_ticket = if active {
        crate::turn_ticket::cancel_interactive_for_session(turn_tickets, session_id_text).await
    } else {
        None
    };
    let mut active_result = if let Some(ticket) = cancelled_ticket.as_ref() {
        let result = if let Some(streams) = turn_streams {
            crate::daemon::turn_stream_registry::delete_turn_stream(streams, &ticket.turn_id).await
        } else {
            Err("turn stream registry unavailable".to_string())
        };
        crate::turn_ticket::clear_turn(turn_tickets, &ticket.turn_id).await;
        result
    } else {
        Ok(())
    };
    if active_result.is_ok()
        && crate::turn_ticket::get_active_interactive_turn(turn_tickets, session_id_text)
            .await
            .active
    {
        active_result = Err("active turn remains after cancellation".to_string());
    }
    deletion.record_runtime_outcome(Some(active), None)?;
    deletion.record_surface(surface_result("active_turn", active_result))?;
    deletion.record_surface(surface_result(
        "turn_recovery",
        crate::engine_recovery::delete_session_recovery(session_id_text),
    ))?;

    if deletion.record().purge_locus {
        let locus_result = if let Some(ops) = memory_operations {
            let locus_session = resolve_workshop_locus_session(session_id_text);
            let tenant = derive_locus_tenant_id(&locus_session);
            let mut scope = MemoryScope {
                session_ids: Some(vec![locus_session]),
                ..Default::default()
            };
            if tenant != LOCUS_DEFAULT_TENANT {
                scope.tenant_id = Some(tenant);
            }
            ops.evict(&MemoryEvictRequest {
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
            .map(|response| response.deleted)
            .map_err(|error| error.to_string())
        } else {
            Err("memory backend unavailable".to_string())
        };
        match locus_result {
            Ok(deleted) => {
                deletion.record_runtime_outcome(None, Some((true, deleted)))?;
                deletion.record_surface(success("locus"))?;
            }
            Err(error) => {
                deletion.record_surface(surface_result("locus", Err(error)))?;
            }
        }
    }

    for surface in LocalSessionSurface::ALL {
        let result = surface.delete(&session_id);
        deletion.record_surface(surface_result(surface.name(), result))?;
    }

    let turn_workers = crate::agent_runtime::turn_worker::turn_worker_store();
    let mut turn_worker_result = turn_workers.delete_session(session_id_text);
    if turn_worker_result.is_ok() {
        crate::workspace::persist::flush_persist_writer().await;
        turn_worker_result =
            crate::agent_runtime::turn_worker::TurnWorkerStore::session_absent_on_disk(
                session_id_text,
            )
            .and_then(|absent| {
                absent
                    .then_some(())
                    .ok_or_else(|| "turn-worker references remain after deletion".to_string())
            });
    }
    deletion.record_surface(surface_result("workspace_turn_workers", turn_worker_result))?;

    let channel_result =
        crate::channel_session_store::purge_session_references(session_id_text).await;
    deletion.record_surface(surface_result("channel_references", channel_result))?;

    let has_failures = deletion
        .record()
        .surfaces
        .iter()
        .any(|surface| !surface.deleted);
    let blocked = deletion.record().surfaces.iter().any(|surface| {
        matches!(
            surface.reason_class.as_deref(),
            Some("confinement" | "invalid_state")
        )
    });
    let status = if blocked {
        SessionDeletionStatus::Blocked
    } else if has_failures {
        SessionDeletionStatus::RetryablePartial
    } else {
        SessionDeletionStatus::Complete
    };
    let record = deletion.finish(status)?;
    Ok(SessionDeleteSummary::from_record(record))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LocalSessionSurface {
    Transcript,
    Catalog,
    SharedCatalog,
    SessionMetadata,
    AgentMode,
    Artifacts,
    Media,
    Extractions,
    Verifications,
    ContextPacks,
    ToolSurface,
    TurnLedger,
    CoderTurnCheckpoints,
}

impl LocalSessionSurface {
    const ALL: [Self; 13] = [
        Self::Transcript,
        Self::Catalog,
        Self::SharedCatalog,
        Self::SessionMetadata,
        Self::AgentMode,
        Self::Artifacts,
        Self::Media,
        Self::Extractions,
        Self::Verifications,
        Self::ContextPacks,
        Self::ToolSurface,
        Self::TurnLedger,
        Self::CoderTurnCheckpoints,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::Transcript => "transcript",
            Self::Catalog => "catalog",
            Self::SharedCatalog => "shared_catalog",
            Self::SessionMetadata => "session_metadata",
            Self::AgentMode => "agent_mode",
            Self::Artifacts => "artifacts",
            Self::Media => "media",
            Self::Extractions => "extractions",
            Self::Verifications => "verifications",
            Self::ContextPacks => "context_packs",
            Self::ToolSurface => "tool_surface",
            Self::TurnLedger => "turn_ledger",
            Self::CoderTurnCheckpoints => "coder_turn_checkpoints",
        }
    }

    fn delete(self, session_id: &crate::session_storage::SessionId) -> Result<(), String> {
        let text = session_id.as_str();
        match self {
            Self::Transcript => crate::session_store::delete_session_transcript(session_id),
            Self::Catalog => crate::session_catalog::delete_catalog_row(session_id),
            Self::SharedCatalog => crate::shared_session_catalog::delete_shared_row(text),
            Self::SessionMetadata => crate::session_meta_store::delete_session_meta(text),
            Self::AgentMode => crate::agent_mode_state::delete_session_mode_state(text),
            Self::Artifacts => crate::artifact_store::delete_artifacts_for_session(text),
            Self::Media => crate::media_store::delete_media_for_session(text),
            Self::Extractions => crate::artifact_extraction::delete_extractions_for_session(text),
            Self::Verifications => {
                crate::verification_store::delete_verifications_for_session(text)
            }
            Self::ContextPacks => crate::context_pack::delete_context_packs_for_session(text),
            Self::ToolSurface => crate::tool_bootstrap::delete_session_tool_surface(text),
            Self::TurnLedger => crate::agent_runtime::turn_ledger::delete_turn_ledger(session_id),
            Self::CoderTurnCheckpoints => {
                crate::agent_runtime::coder_turn_checkpoint::coder_turn_checkpoint_store()
                    .delete_session(session_id)
            }
        }
    }
}

fn success(surface: &str) -> SessionDeletionSurfaceResult {
    SessionDeletionSurfaceResult {
        surface: surface.to_string(),
        deleted: true,
        reason_class: None,
    }
}

fn surface_result(surface: &str, result: Result<(), String>) -> SessionDeletionSurfaceResult {
    match result {
        Ok(()) => success(surface),
        Err(error) => SessionDeletionSurfaceResult {
            surface: surface.to_string(),
            deleted: false,
            reason_class: Some(reason_class(&error).to_string()),
        },
    }
}

fn reason_class(error: &str) -> &'static str {
    let lower = error.to_ascii_lowercase();
    if lower.contains("confin") || lower.contains("symlink") || lower.contains("reparse") {
        "confinement"
    } else if lower.contains("corrupt") || lower.contains("wrong type") || lower.contains("invalid")
    {
        "invalid_state"
    } else if lower.contains("unavailable") || lower.contains("timeout") {
        "backend_unavailable"
    } else {
        "io"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_reasons_are_bounded_classes() {
        assert_eq!(reason_class("outside confinement"), "confinement");
        assert_eq!(
            reason_class("backend unavailable at secret path"),
            "backend_unavailable"
        );
        assert_eq!(reason_class("permission denied: /secret"), "io");
    }

    #[test]
    fn registered_local_surface_names_are_unique() {
        let names = LocalSessionSurface::ALL.map(LocalSessionSurface::name);
        let unique = names.into_iter().collect::<std::collections::HashSet<_>>();
        assert_eq!(unique.len(), LocalSessionSurface::ALL.len());
    }

    #[test]
    fn delete_response_reads_pre_h02_4_successes() {
        let response = serde_json::from_value::<medousa_types::daemon_api::SessionDeleteResponse>(
            serde_json::json!({
                "session_id": "session-a",
                "deleted": true,
                "locus_purged": true,
                "locus_nodes_deleted": 2,
                "cancelled_active_turn": false
            }),
        )
        .unwrap();
        assert_eq!(response.status, SessionDeletionStatus::Complete);
        assert!(response.deletion_id.is_empty());
        assert!(response.surfaces.is_empty());
    }

    #[test]
    fn fresh_process_fixture() {
        let Ok(action) = std::env::var("MEDOUSA_H02_FRESH_PROCESS_ACTION") else {
            return;
        };
        let data_root = crate::paths::medousa_data_dir();
        let session_id = crate::session_storage::SessionId::parse("session-fresh-process").unwrap();
        match action.as_str() {
            "populate" => {
                for (directory, extension) in [
                    ("history", "jsonl"),
                    ("catalog", "json"),
                    ("shared_catalog", "json"),
                    ("session_surfaces", "json"),
                    ("turn_ledger", "jsonl"),
                ] {
                    crate::session_storage::SessionFileStore::new(
                        data_root.join(directory),
                        extension,
                    )
                    .atomic_write(&session_id, b"{}\n")
                    .unwrap();
                }
                for directory in [
                    "artifacts",
                    "media",
                    "extractions",
                    "verifications",
                    "context_packs",
                    "coder_turn_checkpoints",
                ] {
                    crate::session_storage::SessionDirectoryStore::new(data_root.join(directory))
                        .atomic_write(
                            &session_id,
                            &crate::store_root::StorePath::parse("payload.json").unwrap(),
                            b"{}",
                        )
                        .unwrap();
                }
                crate::session_meta_store::set_session_display_name(
                    session_id.as_str(),
                    "Deletion fixture",
                )
                .unwrap();
                crate::agent_mode_state::set_session_mode(
                    session_id.as_str(),
                    medousa_types::daemon_api::SetSessionAgentModeRequest {
                        mode: medousa_types::daemon_api::AgentModeId::Coder,
                        scope: medousa_types::daemon_api::AgentModeScope::Session,
                        task_id: None,
                        expires_at_utc: None,
                    },
                )
                .unwrap();
                let envelope = medousa_engine::TurnEnvelope::new(
                    "turn-fresh-process",
                    medousa_engine::Principal::operator(),
                )
                .with_surface(Some(medousa_engine::TurnSurface {
                    channel_surface: Some("home".to_string()),
                    channel_id: Some(session_id.to_string()),
                    user_id: None,
                }));
                let log = medousa_engine::TurnEventLog::open_in(
                    data_root.join(medousa_engine::TURN_LOG_DIR),
                    envelope,
                )
                .unwrap();
                log.append(medousa_engine::TurnEvent::ContentDelta {
                    delta: "fixture".to_string(),
                });
                drop(log);
                crate::engine_recovery::mark_recovery_ledger(
                    session_id.as_str(),
                    "turn-fresh-process",
                );
            }
            "delete" => {
                let runtime = tokio::runtime::Runtime::new().unwrap();
                let summary = runtime
                    .block_on(delete_session(
                        session_id.as_str(),
                        None,
                        &crate::turn_ticket::new_registry(),
                        None,
                        false,
                    ))
                    .unwrap();
                assert_eq!(summary.status, SessionDeletionStatus::Complete);
                assert!(summary.deleted);
            }
            "verify" => {
                for (directory, extension) in [
                    ("history", "jsonl"),
                    ("catalog", "json"),
                    ("shared_catalog", "json"),
                    ("session_surfaces", "json"),
                    ("turn_ledger", "jsonl"),
                ] {
                    let store = crate::session_storage::SessionFileStore::new(
                        data_root.join(directory),
                        extension,
                    );
                    assert!(!store.contains(&session_id).unwrap(), "{directory} remains");
                }
                for directory in [
                    "artifacts",
                    "media",
                    "extractions",
                    "verifications",
                    "context_packs",
                    "coder_turn_checkpoints",
                ] {
                    let store = crate::session_storage::SessionDirectoryStore::new(
                        data_root.join(directory),
                    );
                    assert!(
                        !store.contains_session(&session_id).unwrap(),
                        "{directory} remains"
                    );
                }
                assert!(
                    crate::session_deletion::coordinator()
                        .record(&session_id)
                        .unwrap()
                        .is_some()
                );
                assert!(crate::session_deletion::acquire_mutation(&session_id).is_err());
                assert!(
                    crate::session_meta_store::get_session_display_name(session_id.as_str())
                        .is_none()
                );
                assert!(
                    crate::agent_runtime::turn_worker::TurnWorkerStore::session_absent_on_disk(
                        session_id.as_str()
                    )
                    .unwrap()
                );
                assert!(
                    !crate::engine_recovery::load_recovery_ledger()
                        .contains_key(session_id.as_str())
                );
                let turn_log = data_root.join(medousa_engine::TURN_LOG_DIR);
                assert!(std::fs::read_dir(turn_log).unwrap().flatten().all(|entry| {
                    entry.path().extension().and_then(|value| value.to_str()) != Some("jsonl")
                }));
            }
            other => panic!("unknown fresh-process action: {other}"),
        }
    }

    #[test]
    fn deletion_inventory_is_absent_from_a_fresh_process() {
        let temp = tempfile::tempdir().unwrap();
        let executable = std::env::current_exe().unwrap();
        for action in ["populate", "delete", "verify"] {
            let status = std::process::Command::new(&executable)
                .arg("--exact")
                .arg("session_lifecycle::tests::fresh_process_fixture")
                .arg("--nocapture")
                .env("MEDOUSA_DATA_DIR", temp.path())
                .env("MEDOUSA_H02_FRESH_PROCESS_ACTION", action)
                .status()
                .unwrap();
            assert!(status.success(), "fresh-process {action} phase failed");
        }
    }
}
