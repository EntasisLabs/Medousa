use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;

use crate::daemon::bounded_set::BoundedDedupSet;
use crate::daemon::turn_event_channel::TurnEventChannel;
use chrono::{DateTime, Utc};
use serde_json::Value;
use tokio::sync::RwLock;
use tracing::Instrument;

use crate::channel_delivery::{ChannelDeliveryTarget, JobDeliveryRecord, JobDeliveryState};
use crate::daemon_api::{InteractiveTurnRequest, InteractiveTurnStreamEvent};
use crate::interactive_turn_runtime;
use crate::media_store::{merge_media_refs_into_prompt, validate_media_refs};
use crate::media_vision;
use crate::payload_receipt::ArtifactReceiptMeta;
use crate::session::load_history;
use crate::session_active_turn::{self, TurnTicketRegistry};
use crate::turn_parts::{
    TurnPartsAccumulator, artifact_refs_from_stream, user_conversation_turn,
    user_conversation_turn_with_context_media_and_speaker,
};
use crate::workspace::ask_job_store::{self, AskJobStore};

use crate::turn_continuation::{TurnContinuationScope, TurnOutcome, turn_continuation_store};

use super::prompt_prep::{MAX_REQUEST_PROMPT_CHARS, truncate_text_for_budget};
use super::settings::{runtime_settings_for_interactive_turn, stage_routing_for_interactive_turn};
use super::stream_sink::AgentStreamSink;
use super::stream_sink::SharedAgentStreamSink;
use super::turn_context::TurnScratchpad;
use super::turn_orchestrator::{self, AssembleLocalTurnParams, PrepareTurnPromptParams};

/// Delivery registry hooks for interactive turns (mirrors ingest `channel_deliveries` pattern).
#[derive(Clone)]
pub struct InteractiveTurnDeliveryContext {
    pub turn_key: String,
    pub delivery_records: Arc<RwLock<HashMap<String, JobDeliveryRecord>>>,
    pub channel_deliveries: Arc<RwLock<HashMap<String, ChannelDeliveryTarget>>>,
    pub last_turn_at: Arc<RwLock<Option<DateTime<Utc>>>>,
    pub last_turn_latency_ms: Arc<RwLock<Option<u64>>>,
    pub started: Instant,
}

impl InteractiveTurnDeliveryContext {
    pub async fn mark_complete(&self, error: Option<String>) {
        let latency_ms = self.started.elapsed().as_millis() as u64;
        let now = Utc::now();
        self.delivery_records.write().await.insert(
            self.turn_key.clone(),
            JobDeliveryRecord {
                state: JobDeliveryState::Delivered,
                delivered_at: Some(now),
                error,
                latency_ms: Some(latency_ms),
            },
        );
        *self.last_turn_at.write().await = Some(now);
        *self.last_turn_latency_ms.write().await = Some(latency_ms);
        self.channel_deliveries.write().await.remove(&self.turn_key);
    }
}

/// Optional session registry + cancel hooks for daemon interactive turns.
#[derive(Clone, Default)]
pub struct InteractiveTurnSessionHooks {
    pub cancelled_turns: Option<Arc<RwLock<BoundedDedupSet>>>,
    pub turn_ticket_registry: Option<TurnTicketRegistry>,
    /// When set, mirror terminal/interim outcomes into ask job store + workspace cards.
    pub ask_job_id: Option<String>,
    /// When set, store the latest turn-start context budget per session.
    pub context_usage_by_session:
        Option<Arc<RwLock<HashMap<String, crate::daemon_api::ContextUsageReport>>>>,
}

pub struct InteractiveTurnStreamSink {
    turn_id: String,
    session_id: String,
    stream_tx: Arc<TurnEventChannel>,
    event_log: Arc<medousa_engine::TurnEventLog>,
    delivery: Option<InteractiveTurnDeliveryContext>,
    session_hooks: InteractiveTurnSessionHooks,
    parts: std::sync::Mutex<TurnPartsAccumulator>,
    /// Current presentation draft assembled from content deltas.
    streamed_markdown: std::sync::Mutex<String>,
    pending_slice_scratch: std::sync::Mutex<Option<TurnScratchpad>>,
}

impl InteractiveTurnStreamSink {
    fn append_stream_delta(&self, delta: &str) {
        if delta.is_empty() {
            return;
        }
        if let Ok(mut body) = self.streamed_markdown.lock() {
            body.push_str(delta);
        }
    }

    fn clear_streamed_markdown(&self) {
        if let Ok(mut body) = self.streamed_markdown.lock() {
            body.clear();
        }
    }

    fn streamed_markdown(&self) -> String {
        self.streamed_markdown
            .lock()
            .map(|body| body.clone())
            .unwrap_or_default()
    }

    fn take_pending_scratch(&self) -> Option<TurnScratchpad> {
        self.pending_slice_scratch
            .lock()
            .ok()
            .and_then(|mut slot| slot.take())
    }

    /// Persist a finalized transcript turn off the hot path.
    ///
    /// Terminal sink methods publish the SSE event first and then hand the turn
    /// to the single persistence writer actor (Phase 1d) so the client never
    /// waits on the (potentially fsync-bound) SurrealKV write. The actor batches
    /// commits and applies backpressure instead of the old per-turn
    /// `tokio::spawn` fire-and-forget, which piled up unbounded under FD pressure
    /// and dropped turns. The write is never silently dropped.
    fn spawn_persist_turn(&self, turn: crate::session::ConversationTurn) {
        let scratch = self.take_pending_scratch();
        crate::session_writer::persist_turn(&self.session_id, turn, scratch);
    }

    /// Commit a finalized terminal/handoff body **through the durable event-log
    /// spine projection**: the body is first lifted into the typed
    /// [`TurnEvent`](super::turn_event::TurnEvent) vocabulary and then folded back
    /// into the persisted `ConversationTurn` via
    /// [`project_turn_to_history`](super::turn_event_log::project_turn_to_history)
    /// — the same fold SSE/history read off — so persistence is a projection of
    /// the spine rather than an ad-hoc re-derivation. This is byte-identical to
    /// the legacy direct persist (locked by the `fold_byte_matches_legacy_*`
    /// tests); the `unwrap_or` keeps an exact fallback should the projection ever
    /// decline a body.
    fn persist_via_spine(
        &self,
        assistant_turn: crate::session::ConversationTurn,
        event: super::turn_event::TurnEvent,
    ) {
        let projected =
            super::turn_event_log::project_turn_to_history(&event).unwrap_or(assistant_turn);
        self.spawn_persist_turn(projected);
    }

    /// Terminal model/control output is the durable source of truth.
    ///
    /// Stream deltas are presentation-only and travel through an asynchronous bridge;
    /// using their arrival state here can persist a stale preamble or a partial final.
    fn canonical_terminal_body(terminal_text: &str) -> String {
        terminal_text.to_string()
    }

    async fn turn_cancelled(&self) -> bool {
        match &self.session_hooks.cancelled_turns {
            Some(set) => set.read().await.contains(&self.turn_id),
            None => false,
        }
    }

    async fn emit_cancelled_if_needed(&self) -> bool {
        if !self.turn_cancelled().await {
            return false;
        }

        self.publish_tracked(interactive_turn_runtime::error_stream_event_from_failure(
            &self.turn_id,
            &crate::turn_failure::TurnFailure::cancelled(),
        ))
        .await;

        if let Some(delivery) = &self.delivery {
            delivery
                .mark_complete(Some("interactive turn cancelled".to_string()))
                .await;
        }

        true
    }

    async fn publish_tracked(&self, event: anyhow::Result<InteractiveTurnStreamEvent>) {
        self.publish_tracked_with_journal(event, None).await;
    }

    async fn publish_tracked_with_journal(
        &self,
        event: anyhow::Result<InteractiveTurnStreamEvent>,
        journal_override: Option<super::turn_event::TurnEvent>,
    ) {
        if let Ok(mut payload) = event {
            if let Some(registry) = &self.session_hooks.turn_ticket_registry {
                session_active_turn::note_stream_event(
                    registry,
                    &self.turn_id,
                    &payload.event_type,
                    &payload.phase,
                    payload.terminal,
                )
                .await;
            }
            let journal = crate::sse_turn_projection::journal_turn_event_for_stream(
                &payload,
                journal_override,
            );
            let sequenced = self.event_log.append(journal);
            payload.seq = sequenced.seq();
            self.stream_tx.publish(payload);
        }
    }

    async fn sync_ask_job_interim(&self, text: String) {
        let Some(job_id) = self.session_hooks.ask_job_id.as_deref() else {
            return;
        };
        if AskJobStore::is_ask_job_id(job_id) {
            ask_job_store::ask_job_store().set_interim_text(job_id, text);
        }
    }

    async fn sync_ask_job_succeeded(&self, text: String) {
        let Some(job_id) = self.session_hooks.ask_job_id.as_deref() else {
            return;
        };
        if AskJobStore::is_ask_job_id(job_id) {
            ask_job_store::ask_job_store().mark_succeeded(job_id, text);
        }
    }

    async fn sync_ask_job_failed(&self, message: String) {
        let Some(job_id) = self.session_hooks.ask_job_id.as_deref() else {
            return;
        };
        if AskJobStore::is_ask_job_id(job_id) {
            ask_job_store::ask_job_store().mark_failed(job_id, message);
        }
    }
}

#[async_trait]
impl AgentStreamSink for InteractiveTurnStreamSink {
    async fn model_receipt(&self, _turn_id: u64, provider: String, model: String) {
        if self.emit_cancelled_if_needed().await {
            return;
        }
        if let Ok(mut parts) = self.parts.lock() {
            parts.set_model_receipt(&provider, &model);
        }
        self.publish_tracked(interactive_turn_runtime::model_receipt_stream_event(
            &self.turn_id,
            &provider,
            &model,
        ))
        .await;
    }

    async fn content_chunk(&self, _turn_id: u64, delta: String) {
        if self.emit_cancelled_if_needed().await {
            return;
        }
        self.append_stream_delta(&delta);
        if let Ok(mut parts) = self.parts.lock() {
            parts.push_content_delta(&delta);
        }
        self.publish_tracked(interactive_turn_runtime::content_delta_stream_event(
            &self.turn_id,
            &delta,
        ))
        .await;
    }

    async fn reasoning_chunk(&self, _turn_id: u64, delta: String) {
        if self.emit_cancelled_if_needed().await {
            return;
        }
        if let Ok(mut parts) = self.parts.lock() {
            parts.push_reasoning_delta(&delta);
        }
        self.publish_tracked(interactive_turn_runtime::reasoning_delta_stream_event(
            &self.turn_id,
            &delta,
        ))
        .await;
    }

    async fn agent_worker_ack(
        &self,
        _turn_id: u64,
        text: String,
        tool_names: Vec<String>,
        work_id: Option<String>,
    ) {
        if self.emit_cancelled_if_needed().await {
            return;
        }

        let assistant_turn = self
            .parts
            .lock()
            .map(|mut parts| {
                parts.finalize_worker_ack_turn(text.clone(), tool_names.clone(), work_id.clone())
            })
            .unwrap_or_else(|_| user_conversation_turn(text.clone()));

        let wire = interactive_turn_runtime::worker_ack_stream_event_with_tools(
            &self.turn_id,
            &text,
            tool_names.clone(),
            work_id.as_deref(),
        );
        let event = super::turn_event::TurnEvent::worker_ack_from_turn(&assistant_turn, work_id);
        self.publish_tracked_with_journal(wire, Some(event.clone()))
            .await;
        self.persist_via_spine(assistant_turn, event);
        self.sync_ask_job_interim(text).await;
    }

    async fn agent_workshop_ack(
        &self,
        _turn_id: u64,
        text: String,
        tool_names: Vec<String>,
        work_id: Option<String>,
    ) {
        if self.emit_cancelled_if_needed().await {
            return;
        }

        let assistant_turn = self
            .parts
            .lock()
            .map(|mut parts| {
                parts.finalize_worker_ack_turn(text.clone(), tool_names.clone(), work_id.clone())
            })
            .unwrap_or_else(|_| user_conversation_turn(text.clone()));

        let wire = interactive_turn_runtime::workshop_ack_stream_event_with_tools(
            &self.turn_id,
            &text,
            tool_names.clone(),
            work_id.as_deref(),
        );
        let event = super::turn_event::TurnEvent::worker_ack_from_turn(&assistant_turn, work_id);
        self.publish_tracked_with_journal(wire, Some(event.clone()))
            .await;
        self.persist_via_spine(assistant_turn, event);
        self.sync_ask_job_interim(text).await;
    }

    async fn agent_response(&self, _turn_id: u64, text: String, tool_names: Vec<String>) {
        if self.emit_cancelled_if_needed().await {
            return;
        }

        let body = Self::canonical_terminal_body(&text);

        let assistant_turn = self
            .parts
            .lock()
            .map(|mut parts| parts.finalize_assistant_turn(body.clone(), tool_names.clone(), None))
            .unwrap_or_else(|_| {
                crate::turn_parts::conversation_turn_from_parts(
                    "assistant",
                    body.clone(),
                    tool_names.clone(),
                    None,
                    vec![crate::turn_parts::TurnPart::Text {
                        markdown: body.clone(),
                    }],
                )
            });

        // Always carry the authoritative body in the terminal commit. The client
        // replaces any partial streamed draft with this value.
        let final_event = interactive_turn_runtime::final_stream_event_with_tools(
            &self.turn_id,
            &body,
            tool_names.clone(),
        );
        let event = super::turn_event::TurnEvent::final_response_from_turn(&assistant_turn);
        self.publish_tracked_with_journal(final_event, Some(event.clone()))
            .await;
        self.persist_via_spine(assistant_turn, event);
        self.sync_ask_job_succeeded(body).await;

        if let Some(delivery) = &self.delivery {
            delivery.mark_complete(None).await;
        }
    }

    async fn agent_turn_checkpoint(&self, _turn_id: u64, text: String, tool_names: Vec<String>) {
        if self.emit_cancelled_if_needed().await {
            return;
        }

        let body = Self::canonical_terminal_body(&text);

        let assistant_turn = self
            .parts
            .lock()
            .map(|mut parts| {
                parts.finalize_assistant_turn(
                    body.clone(),
                    tool_names.clone(),
                    Some("checkpoint".to_string()),
                )
            })
            .unwrap_or_else(|_| {
                crate::turn_parts::conversation_turn_from_parts(
                    "assistant",
                    body.clone(),
                    tool_names.clone(),
                    Some("checkpoint".to_string()),
                    vec![crate::turn_parts::TurnPart::Text {
                        markdown: body.clone(),
                    }],
                )
            });

        let checkpoint_event = interactive_turn_runtime::turn_checkpoint_stream_event(
            &self.turn_id,
            &body,
            tool_names.clone(),
        );
        let event = super::turn_event::TurnEvent::checkpoint_from_turn(&assistant_turn);
        self.publish_tracked_with_journal(checkpoint_event, Some(event.clone()))
            .await;
        self.persist_via_spine(assistant_turn, event);
        self.sync_ask_job_succeeded(body).await;

        if let Some(delivery) = &self.delivery {
            delivery.mark_complete(None).await;
        }
    }

    async fn agent_needs_input(&self, _turn_id: u64, text: String, tool_names: Vec<String>) {
        if self.emit_cancelled_if_needed().await {
            return;
        }

        let body = Self::canonical_terminal_body(&text);

        let assistant_turn = self
            .parts
            .lock()
            .map(|mut parts| {
                parts.finalize_assistant_turn(
                    body.clone(),
                    tool_names.clone(),
                    Some("needs_input".to_string()),
                )
            })
            .unwrap_or_else(|_| {
                crate::turn_parts::conversation_turn_from_parts(
                    "assistant",
                    body.clone(),
                    tool_names.clone(),
                    Some("needs_input".to_string()),
                    vec![crate::turn_parts::TurnPart::Text {
                        markdown: body.clone(),
                    }],
                )
            });

        let needs_input_event = interactive_turn_runtime::needs_input_stream_event_with_tools(
            &self.turn_id,
            &body,
            tool_names.clone(),
        );
        let event = super::turn_event::TurnEvent::needs_input_from_turn(&assistant_turn);
        self.publish_tracked_with_journal(needs_input_event, Some(event.clone()))
            .await;
        self.persist_via_spine(assistant_turn, event);

        if let Some(delivery) = &self.delivery {
            delivery.mark_complete(None).await;
        }
    }

    async fn agent_final_pending(&self, turn_id: u64, text: String, tool_names: Vec<String>) {
        self.agent_turn_progress(turn_id, text, tool_names).await;
    }

    async fn agent_turn_progress(&self, _turn_id: u64, message: String, tool_names: Vec<String>) {
        if self.emit_cancelled_if_needed().await {
            return;
        }

        self.publish_tracked(interactive_turn_runtime::turn_progress_stream_event(
            &self.turn_id,
            &message,
            tool_names,
        ))
        .await;
    }

    async fn agent_pack_hold(
        &self,
        _turn_id: u64,
        fragments: Vec<String>,
        tool_names: Vec<String>,
    ) {
        if self.emit_cancelled_if_needed().await {
            return;
        }

        self.publish_tracked(interactive_turn_runtime::pack_hold_stream_event(
            &self.turn_id,
            &fragments,
            tool_names,
        ))
        .await;
    }

    async fn agent_error(&self, _turn_id: u64, message: String) {
        let failure = crate::turn_failure::TurnFailure::from_debug(&message);

        // Do not persist raw provider/runtime errors as assistant transcript turns.
        self.publish_tracked(interactive_turn_runtime::error_stream_event_from_failure(
            &self.turn_id,
            &failure,
        ))
        .await;
        self.sync_ask_job_failed(failure.debug_message.clone())
            .await;

        if let Some(delivery) = &self.delivery {
            delivery
                .mark_complete(Some(failure.operator_message.clone()))
                .await;
        }
    }

    async fn stage_persist_scratch(&self, scratch: serde_json::Value) {
        if let Ok(scratch) = serde_json::from_value::<TurnScratchpad>(scratch)
            && let Ok(mut slot) = self.pending_slice_scratch.lock()
        {
            *slot = Some(scratch);
        }
    }

    async fn notice(&self, message: String) {
        self.publish_tracked(interactive_turn_runtime::debug_status_stream_event(
            &self.turn_id,
            "orchestration",
            &message,
        ))
        .await;
    }

    async fn scratch_reset(&self, _turn_id: u64) {
        let slice = self.streamed_markdown();
        if !slice.trim().is_empty() {
            if let Ok(mut parts) = self.parts.lock() {
                parts.archive_progress_note(&slice);
            }
            let _ = self
                .publish_tracked(interactive_turn_runtime::turn_progress_stream_event(
                    &self.turn_id,
                    slice.trim(),
                    Vec::new(),
                ))
                .await;
        }
        self.clear_streamed_markdown();
        self.publish_tracked(interactive_turn_runtime::scratch_reset_stream_event(
            &self.turn_id,
        ))
        .await;
    }

    async fn reset_streamed_markdown(&self) {
        self.clear_streamed_markdown();
    }

    async fn turn_budget_approval_required(
        &self,
        _turn_id: u64,
        request_id: String,
        rounds_executed: usize,
        max_tool_rounds: usize,
        requested_rounds: usize,
        reason: String,
        progress_summary: Option<String>,
    ) {
        if self.emit_cancelled_if_needed().await {
            return;
        }

        self.publish_tracked(interactive_turn_runtime::budget_approval_stream_event(
            &self.turn_id,
            &request_id,
            rounds_executed,
            max_tool_rounds,
            requested_rounds,
            &reason,
            progress_summary.as_deref(),
        ))
        .await;
    }

    async fn browser_challenge_required(
        &self,
        _turn_correlation_id: &str,
        session_id: String,
        challenge_url: String,
        reason: String,
    ) {
        if self.emit_cancelled_if_needed().await {
            return;
        }

        self.publish_tracked(interactive_turn_runtime::browser_challenge_stream_event(
            &self.turn_id,
            &session_id,
            &challenge_url,
            &reason,
        ))
        .await;
    }

    async fn browser_navigated(
        &self,
        _turn_correlation_id: &str,
        url: String,
        title: Option<String>,
        _opened_by_agent: bool,
    ) {
        if self.emit_cancelled_if_needed().await {
            return;
        }

        self.publish_tracked(interactive_turn_runtime::browser_navigated_stream_event(
            &self.turn_id,
            &url,
            title.as_deref(),
        ))
        .await;
    }

    async fn tool_invoked(&self, tool_name: String, input_summary: String) {
        self.publish_tracked(interactive_turn_runtime::debug_status_stream_event(
            &self.turn_id,
            "tool",
            &format!("tool={tool_name} {input_summary}"),
        ))
        .await;
    }

    async fn tool_run_started(
        &self,
        tool_run_id: String,
        tool_name: String,
        input_summary: String,
        input_params: Vec<medousa_types::daemon_api::ToolInputParam>,
        tool_round: usize,
    ) {
        if self.emit_cancelled_if_needed().await {
            return;
        }
        if let Ok(mut parts) = self.parts.lock() {
            parts.tool_started(&tool_run_id, &tool_name, &input_summary, tool_round);
        }
        self.publish_tracked(interactive_turn_runtime::tool_started_stream_event(
            &self.turn_id,
            &tool_run_id,
            &tool_name,
            &input_summary,
            input_params,
            tool_round,
        ))
        .await;
    }

    async fn tool_run_finished(
        &self,
        tool_run_id: String,
        tool_name: String,
        status: String,
        input_summary: String,
        output_summary: Option<String>,
        tool_input: Value,
        tool_output: Value,
        input_receipt: Option<ArtifactReceiptMeta>,
        output_receipt: Option<ArtifactReceiptMeta>,
        tool_round: usize,
    ) {
        if self.emit_cancelled_if_needed().await {
            return;
        }
        let safe_input = crate::settings_guard::redact_json_value(&tool_input);
        let safe_output = crate::settings_guard::redact_json_value(&tool_output);
        let input_receipt = input_receipt.or_else(|| {
            crate::payload_receipt::receipt_meta(
                &safe_input,
                crate::payload_receipt::DEFAULT_MAX_INLINE_BYTES,
            )
        });
        let output_receipt = output_receipt.or_else(|| {
            crate::payload_receipt::receipt_meta(
                &safe_output,
                crate::payload_receipt::DEFAULT_MAX_INLINE_BYTES,
            )
        });
        let mut artifact_refs = super::tool_stream::artifact_refs_from_receipts(
            input_receipt.as_ref(),
            output_receipt.as_ref(),
        );
        artifact_refs = super::tool_stream::persist_and_enrich_artifact_refs(
            &self.session_id,
            &tool_name,
            &tool_input,
            &tool_output,
            input_receipt.as_ref(),
            output_receipt.as_ref(),
            artifact_refs,
        );
        if let Ok(mut parts) = self.parts.lock() {
            parts.tool_finished(
                &tool_run_id,
                &status,
                output_summary.clone(),
                artifact_refs_from_stream(&artifact_refs),
            );
            if (tool_name == crate::ui_present_tools::COGNITION_UI_PRESENT
                || tool_name == crate::artifact_tools::COGNITION_ARTIFACT_WRITE)
                && let Some(ui_artifact) =
                    super::tool_stream::ui_artifact_from_tool_output(&tool_output)
            {
                if tool_name == crate::artifact_tools::COGNITION_ARTIFACT_WRITE
                    && tool_output
                        .get("previous_artifact_id")
                        .and_then(|value| value.as_str())
                        .is_some_and(|value| !value.trim().is_empty())
                {
                    let previous = tool_output
                        .get("previous_artifact_id")
                        .and_then(|value| value.as_str())
                        .unwrap_or_default();
                    parts.replace_attachment_ref(
                        previous,
                        &ui_artifact.artifact_id,
                        &ui_artifact.mime,
                        &ui_artifact.label,
                        ui_artifact.byte_size,
                        Some(ui_artifact.presentation.clone()),
                        ui_artifact.height_px,
                    );
                } else {
                    parts.push_attachment_ref(
                        &ui_artifact.artifact_id,
                        &ui_artifact.mime,
                        &ui_artifact.label,
                        ui_artifact.byte_size,
                        Some(ui_artifact.presentation.clone()),
                        ui_artifact.height_px,
                    );
                }
            }
        }
        if tool_name == crate::ui_present_tools::COGNITION_UI_PRESENT
            && let Some(ui_artifact) =
                super::tool_stream::ui_artifact_from_tool_output(&tool_output)
        {
            self.publish_tracked(interactive_turn_runtime::artifact_presented_stream_event(
                &self.turn_id,
                ui_artifact,
            ))
            .await;
        }
        if crate::ui_build_tools::is_ui_scene_stream_tool(&tool_name)
            && let Some(scene) = super::tool_stream::scene_ops_from_tool_output(&tool_output)
        {
            self.publish_tracked(interactive_turn_runtime::scene_ops_stream_event(
                &self.turn_id,
                scene,
            ))
            .await;
        }
        if tool_name == crate::artifact_tools::COGNITION_ARTIFACT_WRITE
            && let Some(ui_artifact) =
                super::tool_stream::ui_artifact_from_tool_output(&tool_output)
        {
            if let Some(previous) = tool_output
                .get("previous_artifact_id")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                let root = tool_output
                    .get("root_artifact_id")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty());
                self.publish_tracked(interactive_turn_runtime::artifact_updated_stream_event(
                    &self.turn_id,
                    previous,
                    ui_artifact,
                    root,
                ))
                .await;
            } else {
                self.publish_tracked(interactive_turn_runtime::artifact_presented_stream_event(
                    &self.turn_id,
                    ui_artifact,
                ))
                .await;
            }
        }
        self.publish_tracked(interactive_turn_runtime::tool_finished_stream_event(
            &self.turn_id,
            &tool_run_id,
            &tool_name,
            &status,
            &input_summary,
            super::tool_stream::preview_tool_input(&tool_input),
            output_summary.as_deref(),
            tool_round,
            artifact_refs,
        ))
        .await;
        let _ = (tool_input, tool_output, input_receipt, output_receipt);
    }

    async fn tool_payload(
        &self,
        tool_name: String,
        _tool_input: Value,
        _tool_output: Value,
        _input_receipt: Option<ArtifactReceiptMeta>,
        _output_receipt: Option<ArtifactReceiptMeta>,
    ) {
        self.publish_tracked(interactive_turn_runtime::status_stream_event(
            &self.turn_id,
            "tool",
            &format!("tool_payload={tool_name}"),
        ))
        .await;
    }
}

fn publish_to_stream(
    stream: &crate::daemon::turn_stream_registry::TurnStreamEntry,
    event: anyhow::Result<InteractiveTurnStreamEvent>,
) {
    crate::daemon::ingest::publish_interactive_turn_event(stream, event);
}

#[allow(clippy::too_many_arguments)]
/// Run a full agent turn for `POST /v1/interactive/turn`, streaming via SSE.
pub async fn run_daemon_interactive_turn(
    turn_id: &str,
    request: InteractiveTurnRequest,
    backend: &str,
    agent_rt: &super::runtime::MedousaAgentRuntime,
    project_state: crate::daemon::state::AppState,
    stream: crate::daemon::turn_stream_registry::TurnStreamEntry,
    delivery: Option<InteractiveTurnDeliveryContext>,
    continuation_scope: Option<TurnContinuationScope>,
    session_hooks: Option<InteractiveTurnSessionHooks>,
) {
    use super::turn_event::{Principal, TurnEnvelope};

    let correlation_id = continuation_scope
        .as_ref()
        .map(|scope| scope.turn_correlation_id.clone())
        .unwrap_or_else(|| turn_id.to_string());
    let envelope =
        TurnEnvelope::new(turn_id, Principal::operator()).with_correlation_id(correlation_id);

    async {
        publish_to_stream(
            &stream,
            interactive_turn_runtime::status_stream_event(
                turn_id,
                "accepted",
                "interactive turn accepted; agent runtime started",
            ),
        );

        let session_id = request.session_id.trim().to_string();
        let interactive_sink = Arc::new(InteractiveTurnStreamSink {
            turn_id: turn_id.to_string(),
            session_id,
            stream_tx: stream.channel.clone(),
            event_log: stream.log.clone(),
            delivery,
            session_hooks: session_hooks.unwrap_or_default(),
            parts: std::sync::Mutex::new(TurnPartsAccumulator::default()),
            streamed_markdown: std::sync::Mutex::new(String::new()),
            pending_slice_scratch: std::sync::Mutex::new(None),
        });
        let sink: SharedAgentStreamSink = interactive_sink.clone();

        run_agent_turn(
            turn_id,
            request,
            backend,
            agent_rt,
            sink,
            continuation_scope,
            Some(interactive_sink),
            Some(project_state),
        )
        .await;
    }
    .instrument(crate::observability::turn_span(&envelope))
    .await;
}

/// Run a full agent turn, streaming events through the provided sink.
#[allow(clippy::too_many_arguments)]
pub async fn run_agent_turn(
    turn_id: &str,
    request: InteractiveTurnRequest,
    backend: &str,
    agent_rt: &super::runtime::MedousaAgentRuntime,
    sink: SharedAgentStreamSink,
    continuation_scope: Option<TurnContinuationScope>,
    context_telemetry: Option<Arc<InteractiveTurnStreamSink>>,
    project_state: Option<crate::daemon::state::AppState>,
) {
    let previous_scope = agent_rt.turn_scope.read().await.clone();
    let turn_correlation_id = continuation_scope
        .as_ref()
        .map(|scope| scope.turn_correlation_id.clone());
    let supports_ui_artifacts =
        crate::ui_present_tools::surface_supports_ui_artifacts(request.surface.as_ref());
    let supports_liquid_markdown = request
        .surface
        .as_ref()
        .is_some_and(|surface| surface.supports_liquid_markdown);
    let supports_browser_host =
        crate::browser_tools::surface_supports_browser_host(request.surface.as_ref());
    let channel_surface = request
        .surface
        .as_ref()
        .and_then(|surface| surface.channel_surface.clone());
    let mut effective_scope = continuation_scope.unwrap_or_else(|| TurnContinuationScope {
        turn_correlation_id: turn_id.to_string(),
        session_id: request.session_id.clone(),
        identity_user_id: Some(
            request
                .identity_user_id
                .clone()
                .unwrap_or_else(crate::user_profiles::resolve_workshop_identity_user_id),
        ),
        original_prompt: request.prompt.clone(),
        delivery_target: None,
        provider: request.provider.clone(),
        model: request.model.clone(),
        response_depth_mode: request.response_depth_mode.clone(),
        supports_ui_artifacts,
        supports_liquid_markdown,
        supports_browser_host,
        channel_surface: channel_surface.clone(),
    });
    effective_scope.supports_ui_artifacts = supports_ui_artifacts;
    effective_scope.supports_liquid_markdown = supports_liquid_markdown;
    effective_scope.supports_browser_host = supports_browser_host;
    effective_scope.channel_surface = channel_surface;
    *agent_rt.turn_scope.write().await = Some(effective_scope);
    let outcome: Arc<RwLock<Option<TurnOutcome>>> = Arc::new(RwLock::new(None));
    let tracking_sink: SharedAgentStreamSink = Arc::new(TurnOutcomeTrackingSink {
        inner: sink,
        outcome: outcome.clone(),
    });
    crate::engine_adapters::set_active_tool_sink(Some(
        crate::engine_adapters::AgentStreamToolSinkAdapter::new(tracking_sink.clone()),
    ))
    .await;

    run_agent_turn_inner(
        turn_id,
        request,
        backend,
        agent_rt,
        tracking_sink,
        context_telemetry,
        project_state,
    )
    .await;

    if let Some(correlation_id) = turn_correlation_id {
        let final_outcome = outcome.read().await.unwrap_or(TurnOutcome::Error);
        tracing::info!(
            target: "medousa::turn",
            turn_id = %turn_id,
            correlation_id = %correlation_id,
            outcome = ?final_outcome,
            "interactive_turn_finished"
        );
        let _ = turn_continuation_store()
            .mark_turn_finished(&correlation_id, final_outcome)
            .await;
    }

    crate::engine_adapters::set_active_tool_sink(None).await;
    *agent_rt.turn_scope.write().await = previous_scope;
}

async fn run_agent_turn_inner(
    turn_id: &str,
    request: InteractiveTurnRequest,
    backend: &str,
    agent_rt: &super::runtime::MedousaAgentRuntime,
    sink: SharedAgentStreamSink,
    context_telemetry: Option<Arc<InteractiveTurnStreamSink>>,
    project_state: Option<crate::daemon::state::AppState>,
) {
    let session_id = request.session_id.trim().to_string();
    let prompt = request.prompt.trim().to_string();
    let host_context = request
        .host_context
        .as_ref()
        .map(crate::agent_runtime::host_context::bound_host_context);
    let has_media = !request.media_refs.is_empty();
    let has_vision_media = media_vision::has_vision_media(&request.media_refs);
    if session_id.is_empty() || (prompt.is_empty() && !has_media) {
        sink.agent_error(1, "session_id and prompt are required".to_string())
            .await;
        return;
    }

    let mode_selection = crate::agent_mode_state::resolve_for_turn(&session_id, request.agent_mode);
    let mut agent_mode = match super::modes::resolve_agent_mode(mode_selection.mode) {
        Ok(mode) => mode,
        Err(err) => {
            sink.agent_error(1, err.to_string()).await;
            return;
        }
    };
    let mut resolved_code_context = request.code_context.clone().unwrap_or_default();
    if resolved_code_context
        .work_id
        .as_deref()
        .is_none_or(|value| value.trim().is_empty())
        && let Ok(binding) = crate::agent_mode_state::get_session_code_binding(&session_id)
    {
        resolved_code_context.work_id = binding.work_id;
    }
    let forge = project_state.as_ref().map(|state| state.forge.clone());
    let checkpoint_store = super::coder_turn_checkpoint::coder_turn_checkpoint_store();
    let (
        coder_authority,
        coder_registry,
        coder_entry,
        coder_resume_checkpoint,
        mode_context_appendix,
        tool_registry_override,
    ) = if agent_mode.id == crate::daemon_api::AgentModeId::Coder {
        let Some(forge) = forge else {
            sink.agent_error(
                1,
                "Coder mode requires daemon-hosted Forge authority".to_string(),
            )
            .await;
            return;
        };
        if resolved_code_context.work_id.is_none() {
            let Some(state) = project_state.clone() else {
                sink.agent_error(
                    1,
                    "Coder project setup requires daemon-hosted Forge authority".to_string(),
                )
                .await;
                return;
            };
            let registry = match super::coder_setup_tools::CoderSetupToolRegistry::new(
                agent_rt.tool_registry.clone(),
                state,
                session_id.clone(),
            ) {
                Ok(registry) => Arc::new(registry),
                Err(err) => {
                    sink.agent_error(1, format!("cannot prepare Coder project setup: {err}"))
                        .await;
                    return;
                }
            };
            let registry_override: Arc<
                dyn stasis::application::orchestration::tool_registry::ToolRegistry,
            > = registry;
            (None, None, None, None, None, Some(registry_override))
        } else {
            agent_mode.coder_phase = Some(super::modes::CoderRuntimePhase::Work);
            let work_id = medousa_forge::model::WorkId::from(
                resolved_code_context
                    .work_id
                    .as_deref()
                    .unwrap_or_default()
                    .trim()
                    .to_string(),
            );
            let executor = medousa_forge::model::ExecutorDescriptor {
                kind: "medousa-coder".into(),
                detail: serde_json::json!({
                    "session_id": session_id.clone(),
                    "turn_id": turn_id,
                    "contract_revision": agent_mode.contract_revision,
                }),
            };
            let mut recovery_plan = match super::coder_turn_checkpoint::plan_coder_recovery(
                &checkpoint_store,
                &forge,
                &super::coder_activity::coder_activity_store(),
                &session_id,
                &work_id,
            ) {
                Ok(plan) => plan,
                Err(err) => {
                    sink.notice(format!(
                        "⚠ Coder recovery index unavailable; starting from Forge state: {err}"
                    ))
                    .await;
                    super::coder_turn_checkpoint::CoderRecoveryPlan::Fresh
                }
            };
            if let Some(checkpoint) = recovery_plan.exact_checkpoint()
                && (checkpoint.agent_mode != "coder"
                    || checkpoint.contract_revision != agent_mode.contract_revision)
            {
                recovery_plan = super::coder_turn_checkpoint::CoderRecoveryPlan::Semantic {
                    checkpoint: checkpoint.clone(),
                    reason: "Coder mode contract changed since the checkpoint".into(),
                };
            }
            let source_attempt = recovery_plan.exact_checkpoint().map(|checkpoint| {
                medousa_forge::model::AttemptId::from(checkpoint.forge.attempt_id.clone())
            });
            let can_rebind_source = source_attempt.as_ref().is_some_and(|source| {
                forge.load(&work_id).is_ok_and(|item| {
                    !item.has_active_attempts()
                        && item
                            .attempt(source)
                            .and_then(|attempt| attempt.environment.as_ref())
                            .is_some()
                })
            });
            if source_attempt.is_some()
                && !can_rebind_source
                && let Some(checkpoint) = recovery_plan.exact_checkpoint().cloned()
            {
                recovery_plan = super::coder_turn_checkpoint::CoderRecoveryPlan::Semantic {
                    checkpoint,
                    reason: "exact Forge environment can no longer be rebound".into(),
                };
            }
            let begin_result = if can_rebind_source {
                match forge.begin_isolated_attempt_from(
                    &work_id,
                    source_attempt.as_ref().expect("checked source attempt"),
                    executor.clone(),
                    Some(std::process::id()),
                    &medousa_forge::forge::Forge::system_actor(),
                ) {
                    Ok(value) => Ok(value),
                    Err(rebind_err) => {
                        if let Some(checkpoint) = recovery_plan.exact_checkpoint().cloned() {
                            recovery_plan =
                                super::coder_turn_checkpoint::CoderRecoveryPlan::Semantic {
                                    checkpoint,
                                    reason: format!(
                                        "exact Forge environment rebind failed: {rebind_err}"
                                    ),
                                };
                        }
                        forge.begin_isolated_attempt(
                            &work_id,
                            executor,
                            Some(std::process::id()),
                            &medousa_forge::forge::Forge::system_actor(),
                        )
                    }
                }
            } else {
                forge.begin_isolated_attempt(
                    &work_id,
                    executor,
                    Some(std::process::id()),
                    &medousa_forge::forge::Forge::system_actor(),
                )
            };
            let (item, lease) = match begin_result {
                Ok(value) => value,
                Err(err) => {
                    sink.agent_error(1, format!("cannot acquire Coder authority: {err}"))
                        .await;
                    return;
                }
            };
            let recovery_note = recovery_plan.prompt_note();
            let entry = match super::coder_mode::compile_coder_entry_for_attempt(
                &forge,
                &resolved_code_context,
                &lease.attempt_id,
            ) {
                Ok(entry) => Arc::new(entry),
                Err(err) => {
                    let _ = forge.interrupt_attempt(
                        &lease,
                        medousa_forge::model::RecoveryDisposition::RestartAllowed,
                        &medousa_forge::forge::Forge::system_actor(),
                    );
                    sink.agent_error(1, err.to_string()).await;
                    return;
                }
            };
            if let Err(err) =
                crate::agent_mode_state::set_session_code_binding(&session_id, &entry.work_id)
            {
                let _ = forge.interrupt_attempt(
                    &lease,
                    medousa_forge::model::RecoveryDisposition::RestartAllowed,
                    &medousa_forge::forge::Forge::system_actor(),
                );
                sink.agent_error(
                    1,
                    format!("cannot preserve Coder undertaking binding: {err}"),
                )
                .await;
                return;
            }
            let identity = super::coder_activity::CoderAgentIdentity::for_turn(
                &session_id,
                turn_id,
                &lease.attempt_id.to_string(),
            );
            let authority = match super::coder_tools::CoderTurnLease::new(
                forge,
                lease,
                super::coder_activity::coder_activity_store(),
                identity,
            ) {
                Ok(authority) => Arc::new(authority),
                Err(err) => {
                    sink.agent_error(1, format!("cannot enter Coder shared space: {err}"))
                        .await;
                    return;
                }
            };
            let registry = Arc::new(
                super::coder_tools::CoderBoundToolRegistry::new_with_catalog(
                    agent_rt.tool_registry.clone(),
                    agent_rt.tool_catalog.clone(),
                    &authority,
                    entry.clone(),
                    item.policy,
                ),
            );
            if let Some(checkpoint) = recovery_plan.exact_checkpoint()
                && let Err(err) = registry.restore_checkpoint_surface(
                    &checkpoint.visible_tools,
                    checkpoint.locus_cursor.as_deref(),
                )
            {
                sink.notice(format!(
                    "⚠ exact Coder tool-surface restore degraded: {err}"
                ))
                .await;
            }
            let shared_space_appendix = match registry.initial_prompt_appendix().await {
                Ok(appendix) => appendix,
                Err(err) => {
                    sink.agent_error(1, err.to_string()).await;
                    return;
                }
            };
            let registry_override: Arc<
                dyn stasis::application::orchestration::tool_registry::ToolRegistry,
            > = registry.clone();
            let resume_checkpoint = recovery_plan.exact_checkpoint().cloned();
            if let super::coder_turn_checkpoint::CoderRecoveryPlan::Semantic { checkpoint, reason } =
                &recovery_plan
                && let Err(err) = checkpoint_store.mark_superseded(checkpoint, reason)
            {
                tracing::warn!(error = %err, "failed to supersede unsafe Coder checkpoint");
            }
            if let Some(note) = recovery_note.as_ref() {
                sink.notice(note.lines().take(5).collect::<Vec<_>>().join(" "))
                    .await;
            }
            let recovery_appendix = recovery_note
                .map(|note| format!("\n\n{note}"))
                .unwrap_or_default();
            (
                Some(authority),
                Some(registry),
                Some(entry.clone()),
                resume_checkpoint,
                Some(format!(
                    "{}\n\n{}{}",
                    entry.prompt_appendix(),
                    shared_space_appendix,
                    recovery_appendix,
                )),
                Some(registry_override),
            )
        }
    } else {
        (None, None, None, None, None, None)
    };
    sink.notice(format!(
        "◈ agent_mode id={} source={:?} contract={} lane={:?}",
        agent_mode.id.as_str(),
        mode_selection.source,
        agent_mode.contract_revision,
        agent_mode.execution_lane,
    ))
    .await;

    if has_media && let Err(err) = validate_media_refs(&session_id, &request.media_refs) {
        sink.agent_error(1, err).await;
        return;
    }

    let saved_defaults = crate::session::load_tui_defaults();
    let settings = runtime_settings_for_interactive_turn(backend, &request);
    let main_target = crate::inference_profiles::main_target(&saved_defaults);
    let vision_target = if has_vision_media {
        match crate::inference_profiles::vision_target(&saved_defaults) {
            Some(target) => target,
            None => {
                sink.agent_error(
                    1,
                    "Configure a vision model in Settings → Models before sending images."
                        .to_string(),
                )
                .await;
                return;
            }
        }
    } else {
        main_target.clone()
    };
    let vision_plan = if has_vision_media {
        match media_vision::plan_turn_media(
            &session_id,
            &request.media_refs,
            &vision_target.provider,
            &vision_target.model,
        ) {
            Ok(plan) => plan,
            Err(err) => {
                sink.agent_error(1, err).await;
                return;
            }
        }
    } else {
        media_vision::TurnMediaVisionPlan::empty()
    };

    let effective_prompt = merge_media_refs_into_prompt(
        &prompt,
        &session_id,
        &request.media_refs,
        &vision_plan.merge_options,
    );
    let effective_prompt = crate::agent_runtime::host_context::append_host_context(
        &effective_prompt,
        host_context.as_ref(),
    );
    let effective_prompt = if request.code_project_setup_authorized
        && agent_mode.id == crate::daemon_api::AgentModeId::Coder
        && agent_mode.coder_phase == Some(super::modes::CoderRuntimePhase::Setup)
    {
        format!(
            "{effective_prompt}\n\n[MEDOUSA_CODE_PROJECT_SETUP_AUTHORITY]\nprincipal_authorized=true\nscope=choose_bind_or_create_project\nsource=explicit_surface_action"
        )
    } else {
        effective_prompt
    };

    if has_vision_media
        && let Some(notice) =
            vision_plan.stream_notice(&vision_target.provider, &vision_target.model)
    {
        sink.notice(notice).await;
    }

    let stage_routing = stage_routing_for_interactive_turn(&request);
    let final_route = stage_routing.get("final_response").cloned();
    let verifier_route = stage_routing.get("verifier").cloned();

    let active_turn_resume =
        super::coder_turn_checkpoint::CoderTurnCheckpointController::initial_resume_state(
            coder_resume_checkpoint.clone(),
        );
    let active_turn_checkpoint_sink: Option<
        Arc<dyn super::coder_turn_checkpoint::ActiveTurnCheckpointSink>,
    > = if let (Some(authority), Some(registry), Some(entry), Some(forge)) = (
        coder_authority.as_ref(),
        coder_registry.as_ref(),
        coder_entry.as_ref(),
        project_state.as_ref().map(|state| state.forge.clone()),
    ) {
        let (checkpoint_provider, checkpoint_model) = final_route
            .as_ref()
            .map(|route| (route.provider.clone(), route.model.clone()))
            .unwrap_or_else(|| (vision_target.provider.clone(), vision_target.model.clone()));
        match super::coder_turn_checkpoint::CoderTurnCheckpointController::new(
            super::coder_turn_checkpoint::CoderTurnCheckpointControllerParams {
                store: checkpoint_store.clone(),
                session_id: session_id.clone(),
                daemon_turn_id: turn_id.to_string(),
                agent_mode: agent_mode.id.as_str().to_string(),
                contract_revision: agent_mode.contract_revision.to_string(),
                provider: checkpoint_provider,
                model: checkpoint_model,
                authoritative_prompt: effective_prompt.clone(),
                forge,
                lease: authority.lease().clone(),
                entry: entry.clone(),
                registry: registry.clone(),
                resume_from: coder_resume_checkpoint.clone(),
            },
        ) {
            Ok(controller) => Some(controller),
            Err(err) => {
                if let Some(source) = coder_resume_checkpoint.as_ref()
                    && let Err(mark_err) = checkpoint_store
                        .mark_superseded(source, "new fenced checkpoint could not be established")
                {
                    tracing::warn!(error = %mark_err, "failed to retire unrecoverable Coder checkpoint");
                }
                sink.notice(format!(
                    "⚠ exact Coder checkpointing unavailable for this turn: {err}"
                ))
                .await;
                None
            }
        }
    } else {
        None
    };
    let active_turn_resume = active_turn_checkpoint_sink.as_ref().and(active_turn_resume);

    if let Some(route) = final_route.as_ref() {
        sink.notice(format!(
            "◈ stage route final_response target={}:{} policy={} fallback={}",
            route.provider,
            route.model,
            route.policy_profile,
            route.fallback_chain.join(","),
        ))
        .await;
    }

    let speaker_profile_id = crate::user_profiles::resolve_workshop_identity_user_id_for_turn(
        request.identity_user_id.as_deref(),
    );

    let mut conversation = load_history(&session_id);
    if request.persist_user_turn {
        let user_turn = user_conversation_turn_with_context_media_and_speaker(
            prompt.clone(),
            host_context.clone(),
            &request.media_refs,
            Some(speaker_profile_id.as_str()),
        );
        // The in-memory transcript already carries this turn for the rest of the run;
        // persist off the hot path so the user message write (and its catalog cascade)
        // doesn't block prompt prep / first token on a SurrealKV fsync.
        conversation.push(user_turn.clone());
        crate::session_writer::persist_turn(&session_id, user_turn, None);
    }

    let manuscript_id = request
        .manuscript_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let scheduled_tool_allowlist = request
        .scheduled_tool_allowlist
        .as_ref()
        .map(|tools| {
            tools
                .iter()
                .map(|tool| tool.trim().to_string())
                .filter(|tool| !tool.is_empty())
                .collect::<std::collections::HashSet<_>>()
        })
        .filter(|tools| !tools.is_empty())
        .or_else(|| {
            manuscript_id.and_then(|id| {
                crate::identity_manuscript::build_manuscript_context(id)
                    .ok()
                    .map(|ctx| {
                        crate::identity_manuscript::scheduled_tool_allowlist_for_manuscript(&ctx)
                    })
            })
        });

    if let Some(manuscript_id) = manuscript_id {
        sink.notice(format!(
            "◈ manuscript_load id={manuscript_id} lane=scheduled"
        ))
        .await;
        if let Some(allowlist) = scheduled_tool_allowlist.as_ref() {
            sink.notice(format!(
                "◈ manuscript_tools allowed={} lane=scheduled",
                allowlist.len()
            ))
            .await;
        }
    }

    let additional_manuscript_ids = request
        .additional_manuscript_ids
        .as_deref()
        .filter(|ids| !ids.is_empty());
    let suggested_capability_ids = request
        .suggested_capability_ids
        .as_deref()
        .filter(|ids| !ids.is_empty());

    let identity_user_id = speaker_profile_id.clone();

    let prepared = turn_orchestrator::prepare_turn_prompt(PrepareTurnPromptParams {
        agent_mode,
        mode_context_appendix: mode_context_appendix.as_deref(),
        session_id: &session_id,
        prompt: &effective_prompt,
        selected_context_pack_query: None,
        settings: &settings,
        verifier_route: verifier_route.as_ref(),
        final_route: final_route.as_ref(),
        response_depth_mode: &request.response_depth_mode,
        surface: request.surface.as_ref(),
        tui_rt: agent_rt,
        manuscript_id,
        additional_manuscript_ids,
        suggested_capability_ids,
        voice_preset_id: request
            .voice_preset_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty()),
        voice_appendix: request
            .voice_appendix
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty()),
        identity_user_id: &identity_user_id,
    })
    .await;

    if let Some(err) = &prepared.recall_probe.error {
        sink.notice(format!("◈ cheap_recall error={err}")).await;
    } else if prepared.recall_probe.attempted {
        sink.notice(format!(
            "◈ cheap_recall retrieved={} path={} keys={}",
            prepared.recall_probe.retrieved,
            prepared
                .recall_probe
                .retrieval_path
                .as_deref()
                .unwrap_or("n/a"),
            prepared.recall_probe.node_sync_keys.len(),
        ))
        .await;
    }

    if let Some(summary) = &prepared.identity_probe.summary {
        sink.notice(format!(
            "◈ identity_context loaded summary={}",
            truncate_text_for_budget(summary, 180)
        ))
        .await;
    }

    sink.notice(format!("◈ {}", prepared.compiler_output.compiler_summary))
        .await;

    if let Some(note) = &prepared.pack_note {
        sink.notice(note.clone()).await;
    }

    let resolved_prompt =
        truncate_text_for_budget(&prepared.resolved_prompt, MAX_REQUEST_PROMPT_CHARS);
    let resolved_prompt_chars = resolved_prompt.chars().count();
    let assembled = turn_orchestrator::assemble_local_turn(AssembleLocalTurnParams {
        session_id: &session_id,
        settings: &settings,
        conversation: &conversation,
        prompt: &effective_prompt,
        persist_user_turn: request.persist_user_turn,
        prepared: &prepared,
        resolved_prompt,
        tui_rt: agent_rt,
        tool_registry_override,
        final_route: final_route.as_ref(),
        response_depth_mode: &request.response_depth_mode,
        reasoning_effort: &request.reasoning_effort,
        max_tool_rounds_override: request.max_tool_rounds,
        turn_id: 1,
        scheduled_tool_allowlist,
        media_refs: request.media_refs.clone(),
        vision_plan,
        inference_profile_kind: if has_vision_media {
            crate::inference_profiles::InferenceProfileKind::Vision
        } else {
            crate::inference_profiles::InferenceProfileKind::Main
        },
        surface: request.surface.clone(),
        round_context_provider: coder_registry
            .clone()
            .map(|registry| registry as Arc<dyn super::turn_context::ToolRoundContextProvider>),
        evidence_undertaking_id: coder_registry
            .as_ref()
            .map(|registry| registry.undertaking_id().to_string()),
        compact_evidence_receipt_sink: coder_registry
            .clone()
            .map(|registry| registry as Arc<dyn super::coder_evidence::CompactEvidenceReceiptSink>),
        active_turn_checkpoint_sink,
        active_turn_resume,
    });

    if let Some(route_notice) = assembled.pipeline_selection.route_dispatch_notice {
        sink.notice(route_notice).await;
    }

    sink.notice(format!(
        "◈ activation heuristic class={} mode={} rounds={} no_tools={} reason={}",
        assembled.activation.turn_class,
        match assembled.activation.tool_call_mode {
            stasis::application::orchestration::tool_loop_pipeline::ToolCallMode::Auto => "auto",
            stasis::application::orchestration::tool_loop_pipeline::ToolCallMode::Strict => {
                "strict"
            }
        },
        assembled.activation.max_tool_rounds,
        assembled.activation.enforce_no_tools,
        assembled.activation.reason,
    ))
    .await;

    sink.notice(format!(
        "◈ turn slicing hot_turns={} cold_turns={} cold_chars={} prior_chars={}",
        assembled.prior_build.hot_turns_included,
        assembled.prior_build.cold_turns_summarized,
        assembled.prior_build.cold_summary_chars,
        assembled.prior_build.total_chars,
    ))
    .await;

    let system_prompt = super::turn_worker::system_prompt_for_host_profile(
        super::DEFAULT_SYSTEM_PROMPT,
        true,
        crate::ui_present_tools::surface_supports_ui_artifacts(request.surface.as_ref()),
        request
            .surface
            .as_ref()
            .is_some_and(|surface| surface.supports_liquid_markdown),
        None,
    );
    let (tool_count, tool_schema_chars) =
        crate::agent_runtime::context_usage::estimate_tool_schema_chars(
            &assembled.execution.tool_registry,
        )
        .await;
    let context_limit_tokens = final_route.as_ref().and_then(|route| {
        crate::model_capability_registry::registry()
            .resolve(&route.provider, &route.model)
            .model
            .and_then(|record| record.max_input_tokens)
            .and_then(|limit| u32::try_from(limit).ok())
    });
    let context_report = crate::agent_runtime::context_usage::build_context_usage_report(
        crate::agent_runtime::context_usage::ContextUsageInput {
            system_prompt_chars: system_prompt.chars().count(),
            user_prompt_chars: effective_prompt.chars().count(),
            resolved_prompt_chars,
            prompt_for_request_chars: assembled.execution.prompt_for_request.chars().count(),
            ambient_chars: prepared.ambient_appendix.chars().count(),
            prior_build: &assembled.prior_build,
            tool_count,
            tool_schema_chars,
            context_limit_tokens,
        },
    );
    let context_summary = crate::agent_runtime::context_usage::operator_summary(&context_report);
    tracing::info!(
        target: "medousa::context_usage",
        turn_id = %turn_id,
        total_tokens = context_report.total_tokens_estimate,
        tool_count = context_report.tool_count,
        "turn context budget"
    );
    if let Ok(event) = interactive_turn_runtime::context_usage_stream_event(
        turn_id,
        &context_report,
        &context_summary,
    ) && let Some(stream_sink) = context_telemetry
    {
        if let Some(cache) = &stream_sink.session_hooks.context_usage_by_session {
            let session_id = stream_sink.session_id.clone();
            cache
                .write()
                .await
                .insert(session_id, context_report.clone());
        }
        stream_sink.publish_tracked(Ok(event)).await;
    }

    turn_orchestrator::execute_local_turn(sink, assembled.execution).await;
    if let Some(registry) = coder_registry {
        let _ = registry.flush_memory_queue().await;
        registry.interrupt_shell_sessions().await;
    }
    drop(coder_authority);
}

struct TurnOutcomeTrackingSink {
    inner: SharedAgentStreamSink,
    outcome: Arc<RwLock<Option<TurnOutcome>>>,
}

#[async_trait]
impl AgentStreamSink for TurnOutcomeTrackingSink {
    async fn model_receipt(&self, turn_id: u64, provider: String, model: String) {
        self.inner.model_receipt(turn_id, provider, model).await;
    }

    async fn content_chunk(&self, turn_id: u64, delta: String) {
        self.inner.content_chunk(turn_id, delta).await;
    }

    async fn reasoning_chunk(&self, turn_id: u64, delta: String) {
        self.inner.reasoning_chunk(turn_id, delta).await;
    }

    async fn agent_worker_ack(
        &self,
        turn_id: u64,
        text: String,
        tool_names: Vec<String>,
        work_id: Option<String>,
    ) {
        self.inner
            .agent_worker_ack(turn_id, text, tool_names, work_id)
            .await;
    }

    async fn agent_workshop_ack(
        &self,
        turn_id: u64,
        text: String,
        tool_names: Vec<String>,
        work_id: Option<String>,
    ) {
        self.inner
            .agent_workshop_ack(turn_id, text, tool_names, work_id)
            .await;
    }

    async fn agent_response(&self, turn_id: u64, text: String, tool_names: Vec<String>) {
        *self.outcome.write().await = Some(TurnOutcome::Success);
        self.inner.agent_response(turn_id, text, tool_names).await;
    }

    async fn agent_needs_input(&self, turn_id: u64, text: String, tool_names: Vec<String>) {
        *self.outcome.write().await = Some(TurnOutcome::Success);
        self.inner
            .agent_needs_input(turn_id, text, tool_names)
            .await;
    }

    async fn agent_final_pending(&self, turn_id: u64, text: String, tool_names: Vec<String>) {
        self.inner
            .agent_final_pending(turn_id, text, tool_names)
            .await;
    }

    async fn agent_turn_progress(&self, turn_id: u64, message: String, tool_names: Vec<String>) {
        self.inner
            .agent_turn_progress(turn_id, message, tool_names)
            .await;
    }

    async fn agent_pack_hold(&self, turn_id: u64, fragments: Vec<String>, tool_names: Vec<String>) {
        self.inner
            .agent_pack_hold(turn_id, fragments, tool_names)
            .await;
    }

    async fn agent_turn_checkpoint(&self, turn_id: u64, message: String, tool_names: Vec<String>) {
        *self.outcome.write().await = Some(TurnOutcome::Success);
        self.inner
            .agent_turn_checkpoint(turn_id, message, tool_names)
            .await;
    }

    async fn agent_error(&self, turn_id: u64, message: String) {
        *self.outcome.write().await = Some(TurnOutcome::Error);
        self.inner.agent_error(turn_id, message).await;
    }

    async fn notice(&self, message: String) {
        self.inner.notice(message).await;
    }

    async fn tool_invoked(&self, tool_name: String, input_summary: String) {
        self.inner.tool_invoked(tool_name, input_summary).await;
    }

    async fn tool_run_started(
        &self,
        tool_run_id: String,
        tool_name: String,
        input_summary: String,
        input_params: Vec<medousa_types::daemon_api::ToolInputParam>,
        tool_round: usize,
    ) {
        self.inner
            .tool_run_started(
                tool_run_id,
                tool_name,
                input_summary,
                input_params,
                tool_round,
            )
            .await;
    }

    async fn tool_run_finished(
        &self,
        tool_run_id: String,
        tool_name: String,
        status: String,
        input_summary: String,
        output_summary: Option<String>,
        tool_input: Value,
        tool_output: Value,
        input_receipt: Option<ArtifactReceiptMeta>,
        output_receipt: Option<ArtifactReceiptMeta>,
        tool_round: usize,
    ) {
        self.inner
            .tool_run_finished(
                tool_run_id,
                tool_name,
                status,
                input_summary,
                output_summary,
                tool_input,
                tool_output,
                input_receipt,
                output_receipt,
                tool_round,
            )
            .await;
    }

    async fn tool_payload(
        &self,
        tool_name: String,
        tool_input: Value,
        tool_output: Value,
        input_receipt: Option<ArtifactReceiptMeta>,
        output_receipt: Option<ArtifactReceiptMeta>,
    ) {
        self.inner
            .tool_payload(
                tool_name,
                tool_input,
                tool_output,
                input_receipt,
                output_receipt,
            )
            .await;
    }

    async fn scratch_reset(&self, turn_id: u64) {
        self.inner.scratch_reset(turn_id).await;
    }

    async fn reset_streamed_markdown(&self) {
        self.inner.reset_streamed_markdown().await;
    }
}
