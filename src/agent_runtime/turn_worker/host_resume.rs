//! Terminal delivery for locally spawned parallel workers.
use super::{TurnWorkDisposition, TurnWorkRecord, TurnWorkStatus, turn_worker_store};
use crate::delegation::session_results::NativeSessionWait;
use crate::session_store::TranscriptAppend;
use anyhow::{Context, Result};
use stasis::{domain::agent::envelope::AgentEnvelopeKind, prelude::RuntimeComposition};

pub(super) async fn worker_wait(
    runtime: &RuntimeComposition,
    session: &str,
    work_id: &str,
) -> stasis::prelude::Result<NativeSessionWait> {
    NativeSessionWait::open(runtime, "workshop.spawn", session, work_id, work_id).await
}

pub(crate) async fn deliver_worker_parent_outcome(
    runtime: &RuntimeComposition,
    record: &TurnWorkRecord,
) {
    if record.archived || record.disposition == TurnWorkDisposition::Delegated {
        return;
    }
    if let Err(error) = return_worker_result(runtime, record).await {
        tracing::warn!(%error, work_id = %record.work_id, "worker result pending delivery");
    }
}
async fn return_worker_result(runtime: &RuntimeComposition, record: &TurnWorkRecord) -> Result<()> {
    if record.synthesis_delivered {
        return Ok(());
    }
    let kind = match record.status {
        TurnWorkStatus::Completed => AgentEnvelopeKind::TurnCompleted,
        TurnWorkStatus::Failed => AgentEnvelopeKind::Failed,
        TurnWorkStatus::Cancelled => AgentEnvelopeKind::Cancelled,
        TurnWorkStatus::Pending | TurnWorkStatus::Running => return Ok(()),
    };
    let owner = record
        .identity_user_id
        .as_deref()
        .context("missing source identity")?;
    let session = record.session_id.clone();
    let owner = owner.to_owned();
    let visible = crate::peer_completion_delivery::completion_execution_service()
        .run(
            medousa_forge::execution::ExecutionClass::StoreIo,
            1024,
            move || {
                Ok(crate::session_catalog::session_visible_to_profile(
                    &session, &owner,
                ))
            },
        )
        .await?;
    anyhow::ensure!(visible, "source session unavailable");
    let wait = worker_wait(runtime, &record.session_id, &record.work_id).await?;
    let result = record
        .result_text
        .as_deref()
        .or(record.error.as_deref())
        .unwrap_or_default();
    let payload =
        serde_json::json!({"text": result, "request_id": record.work_id, "status": record.status});
    let cause = crate::workshop_authority::execution_ref(&record.session_id, &wait.id)
        .map_err(anyhow::Error::msg)?;
    let store = crate::session_store::get_session_store();
    let scan_store = store.clone();
    let scan_cause = cause.clone();
    // Serialize recovery and live observers of the same terminal record across the append.
    static PRESENT: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _delivery = PRESENT.lock().await;
    let existing = crate::peer_completion_delivery::completion_execution_service()
        .run(
            medousa_forge::execution::ExecutionClass::StoreIo,
            medousa_forge::execution::MAX_STORE_PAYLOAD_BYTES,
            move || {
                Ok(crate::peer_completion_delivery::find_completion_entry(
                    &*scan_store,
                    &scan_cause.session_id,
                    &scan_cause,
                ))
            },
        )
        .await??;
    if existing.is_none() {
        let turn = medousa_types::ConversationTurn::plain(
            "assistant",
            payload.to_string(),
            record.updated_at,
            record.tool_names.clone(),
            Some("worker_synthesis".into()),
        );
        store
            .append_transcript_batch(
                &cause.session_id,
                &[TranscriptAppend::native(turn, Some(cause.clone()))],
            )
            .await
            .context("commit worker result to source session")?;
    }
    wait.settle(kind, &record.work_id, payload).await?;
    if existing.is_none() {
        crate::turn_worker_notify::publish_worker_synthesis_to_parent_turn(
            record,
            result,
            &record.tool_names,
        )
        .await;
    }
    if record.disposition == TurnWorkDisposition::Bound
        && existing.is_none()
        && record.status == TurnWorkStatus::Completed
    {
        crate::feed_adapters::publish_workshop_terminal(record, "done", Some(result)).await;
    }
    turn_worker_store().mark_result_delivered(&record.work_id)?;
    Ok(())
}
