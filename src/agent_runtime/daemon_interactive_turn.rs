use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;

use crate::daemon::bounded_set::BoundedDedupSet;
use chrono::{DateTime, Utc};
use medousa_engine::TurnPipelineHandle;
use medousa_types::turn_stream::{TurnCompletionOutcomeV3, TurnStreamEventV3, WorkerAckKind};
use serde_json::Value;
use tokio::sync::RwLock;
use tracing::Instrument;

use crate::channel_delivery::{ChannelDeliveryTarget, JobDeliveryRecord, JobDeliveryState};
use crate::daemon_api::InteractiveTurnRequest;
use crate::media_store::{merge_media_refs_into_prompt, validate_media_refs};
use crate::media_vision;
use crate::payload_receipt::ArtifactReceiptMeta;
use crate::session::load_history;
use crate::session_active_turn::{self, TurnTicketRegistry};
use crate::turn_parts::{
    TurnPartsAccumulator, artifact_refs_from_stream, user_conversation_turn,
    user_conversation_turn_with_context_media_and_speaker,
};
use crate::turn_pipeline_output::{TurnJournalOutput, daemon_turn_pipeline_budget};
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
    pipeline: TurnPipelineHandle,
    delivery: Option<InteractiveTurnDeliveryContext>,
    session_hooks: InteractiveTurnSessionHooks,
    parts: std::sync::Mutex<TurnPartsAccumulator>,
    text: std::sync::Mutex<ChronologicalTextState>,
    pending_slice_scratch: std::sync::Mutex<Option<TurnScratchpad>>,
}

#[derive(Debug)]
struct ActiveTextSegment {
    segment_id: String,
    model_round: usize,
    markdown: String,
}

#[derive(Debug)]
struct CommittedTextSegment {
    segment_id: String,
    model_round: usize,
    markdown: String,
}

#[derive(Debug)]
struct ChronologicalTextState {
    model_round: usize,
    next_ordinal: usize,
    active: Option<ActiveTextSegment>,
    committed_markdown: Vec<String>,
}

impl Default for ChronologicalTextState {
    fn default() -> Self {
        Self {
            model_round: 1,
            next_ordinal: 0,
            active: None,
            committed_markdown: Vec::new(),
        }
    }
}

impl InteractiveTurnStreamSink {
    fn prepare_content_delta(
        &self,
        delta: String,
    ) -> Option<(Option<TurnStreamEventV3>, TurnStreamEventV3)> {
        if delta.is_empty() {
            return None;
        }
        let mut state = self
            .text
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let started = if state.active.is_none() {
            state.next_ordinal = state.next_ordinal.saturating_add(1);
            let segment_id = format!("{}:text:{}", self.turn_id, state.next_ordinal);
            let model_round = state.model_round;
            state.active = Some(ActiveTextSegment {
                segment_id: segment_id.clone(),
                model_round,
                markdown: String::new(),
            });
            Some(TurnStreamEventV3::AssistantTextStarted {
                segment_id,
                model_round,
            })
        } else {
            None
        };
        let active = state.active.as_mut().expect("active segment initialized");
        active.markdown.push_str(&delta);
        Some((
            started,
            TurnStreamEventV3::ContentAppend {
                segment_id: active.segment_id.clone(),
                text: delta,
            },
        ))
    }

    fn take_active_segment(&self, advance_model_round: bool) -> Option<CommittedTextSegment> {
        let mut state = self
            .text
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let active = state.active.take();
        if advance_model_round {
            state.model_round = state.model_round.saturating_add(1);
        }
        let active = active.filter(|segment| !segment.markdown.is_empty())?;
        state.committed_markdown.push(active.markdown.clone());
        Some(CommittedTextSegment {
            segment_id: active.segment_id,
            model_round: active.model_round,
            markdown: active.markdown,
        })
    }

    async fn commit_active_segment(&self, advance_model_round: bool) {
        let Some(segment) = self.take_active_segment(advance_model_round) else {
            return;
        };
        if let Ok(mut parts) = self.parts.lock() {
            parts.commit_text_segment(
                &segment.markdown,
                Some(&segment.segment_id),
                Some(segment.model_round),
            );
        }
        self.publish_tracked(TurnStreamEventV3::AssistantTextCommitted {
            segment_id: segment.segment_id,
        })
        .await;
    }

    async fn ensure_response_text(&self, response_text: Option<String>) -> bool {
        let Some(response_text) = response_text.filter(|text| !text.trim().is_empty()) else {
            return false;
        };
        let needs_fallback = self
            .text
            .lock()
            .map(|state| {
                state
                    .active
                    .as_ref()
                    .is_none_or(|segment| segment.markdown.is_empty())
            })
            .unwrap_or(false);
        if !needs_fallback {
            return false;
        }
        let Some((started, append)) = self.prepare_content_delta(response_text) else {
            return false;
        };
        if let Some(started) = started {
            self.publish_tracked(started).await;
        }
        self.publish_tracked(append).await;
        true
    }

    fn aggregate_text(&self) -> String {
        self.text
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .committed_markdown
            .join("\n\n")
    }

    async fn terminal_body(&self, fallback: &str) -> String {
        self.commit_active_segment(false).await;
        let terminal_text_is_new = self
            .text
            .lock()
            .map(|state| {
                let fallback = fallback.trim();
                !fallback.is_empty()
                    && state.committed_markdown.join("\n\n").trim() != fallback
                    && state
                        .committed_markdown
                        .last()
                        .is_none_or(|last| last.trim() != fallback)
            })
            .unwrap_or(false);
        if terminal_text_is_new
            && let Some((started, append)) = self.prepare_content_delta(fallback.to_string())
        {
            if let Some(started) = started {
                self.publish_tracked(started).await;
            }
            self.publish_tracked(append).await;
            self.commit_active_segment(false).await;
        }
        let aggregate = self.aggregate_text();
        if aggregate.trim().is_empty() {
            fallback.to_string()
        } else {
            aggregate
        }
    }

    fn take_pending_scratch(&self) -> Option<TurnScratchpad> {
        self.pending_slice_scratch
            .lock()
            .ok()
            .and_then(|mut slot| slot.take())
    }

    /// Persist a finalized transcript turn through the bounded writer and wait
    /// for the backing store's receipt. Terminal success is not published until
    /// this acknowledgement arrives; failures become an explicit stream error.
    async fn persist_turn(
        &self,
        turn: crate::session::ConversationTurn,
    ) -> Result<crate::session_store::CommitReceipt, crate::session_store::StoreError> {
        let scratch = self.take_pending_scratch();
        let caused_by = crate::workshop_authority::execution_ref(&self.session_id, &self.turn_id)
            .map_err(crate::session_store::StoreError::InvalidInput)?;
        crate::session_writer::persist_turn_with_execution(
            &self.session_id,
            turn,
            scratch,
            Some(caused_by),
        )
        .await
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
    async fn persist_via_spine(
        &self,
        assistant_turn: crate::session::ConversationTurn,
        event: super::turn_event::TurnEvent,
    ) -> Result<crate::session_store::CommitReceipt, crate::session_store::StoreError> {
        let projected =
            super::turn_event_log::project_turn_to_history(&event).unwrap_or(assistant_turn);
        self.persist_turn(projected).await
    }

    async fn persist_or_publish_error(
        &self,
        assistant_turn: crate::session::ConversationTurn,
        event: super::turn_event::TurnEvent,
    ) -> bool {
        if let Err(error) = self.persist_via_spine(assistant_turn, event).await {
            let message = format!("turn persistence failed: {error}");
            self.publish_failure(TurnCompletionOutcomeV3::Failed, message.clone(), None)
                .await;
            self.sync_ask_job_failed(message).await;
            return false;
        }
        true
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

        let failure = crate::turn_failure::TurnFailure::cancelled();
        self.commit_active_segment(false).await;
        self.publish_failure(
            TurnCompletionOutcomeV3::Cancelled,
            failure.operator_message,
            Some(failure.debug_message),
        )
        .await;

        if let Some(delivery) = &self.delivery {
            delivery
                .mark_complete(Some("interactive turn cancelled".to_string()))
                .await;
        }

        true
    }

    async fn publish_failure(
        &self,
        outcome: TurnCompletionOutcomeV3,
        operator_message: String,
        debug_message: Option<String>,
    ) {
        self.publish_tracked(TurnStreamEventV3::Error {
            operator_message: operator_message.clone(),
            debug_message: debug_message.clone(),
        })
        .await;
        self.publish_tracked(TurnStreamEventV3::TurnCompleted {
            outcome,
            aggregate_text: self.aggregate_text(),
            tool_names: Vec::new(),
            operator_message: Some(operator_message),
            debug_message,
        })
        .await;
    }

    async fn publish_tracked(&self, event: TurnStreamEventV3) {
        self.publish_tracked_with_journal(event, None).await;
    }

    async fn publish_tracked_with_journal(
        &self,
        event: TurnStreamEventV3,
        journal_override: Option<super::turn_event::TurnEvent>,
    ) {
        if let Some(registry) = &self.session_hooks.turn_ticket_registry {
            let (event_type, phase, terminal) = stream_tracking(&event);
            session_active_turn::note_stream_event(
                registry,
                &self.turn_id,
                event_type,
                phase,
                terminal,
            )
            .await;
        }
        let deferred_append = journal_override.is_none()
            && matches!(
                &event,
                TurnStreamEventV3::ContentAppend { .. } | TurnStreamEventV3::ReasoningAppend { .. }
            );
        let result = if deferred_append {
            self.pipeline.admit_v3(event).await
        } else {
            self.pipeline
                .emit_v3_with_journal(event, journal_override)
                .await
                .map(|_| ())
        };
        if let Err(error) = result {
            tracing::warn!(turn_id = %self.turn_id, %error, "turn pipeline rejected stream event");
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

fn stream_tracking(event: &TurnStreamEventV3) -> (&str, &str, bool) {
    match event {
        TurnStreamEventV3::AssistantTextStarted { .. } => {
            ("assistant_text_started", "streaming", false)
        }
        TurnStreamEventV3::ContentAppend { .. } => ("content_delta", "streaming", false),
        TurnStreamEventV3::AssistantTextCommitted { .. } => {
            ("assistant_text_committed", "streaming", false)
        }
        TurnStreamEventV3::ReasoningAppend { .. } => ("reasoning_delta", "streaming", false),
        TurnStreamEventV3::Status { phase, .. } => ("status", phase, false),
        TurnStreamEventV3::Progress { .. } => ("turn_progress", "tool_loop", false),
        TurnStreamEventV3::ModelReceipt { .. } => ("model_receipt", "inference", false),
        TurnStreamEventV3::WorkerAck { ack_kind, .. } => match ack_kind {
            WorkerAckKind::Worker => ("worker_ack", "worker_ack", false),
            WorkerAckKind::Workshop => ("workshop_ack", "workshop_ack", false),
        },
        TurnStreamEventV3::WorkerSynthesis { .. } => {
            ("worker_synthesis", "worker_synthesis", false)
        }
        TurnStreamEventV3::Error { .. } => ("error", "failed", false),
        TurnStreamEventV3::ToolStarted { .. } => ("tool_started", "tool_loop", false),
        TurnStreamEventV3::ToolFinished { .. } => ("tool_finished", "tool_loop", false),
        TurnStreamEventV3::ArtifactPresented { .. } => ("artifact_presented", "tool_loop", false),
        TurnStreamEventV3::ArtifactUpdated { .. } => ("artifact_updated", "tool_loop", false),
        TurnStreamEventV3::UiScene { .. } => ("ui_scene", "tool_loop", false),
        TurnStreamEventV3::BudgetApprovalRequired { .. } => {
            ("budget_approval", "awaiting_operator", false)
        }
        TurnStreamEventV3::BrowserChallenge { .. } => {
            ("browser_challenge", "awaiting_operator", false)
        }
        TurnStreamEventV3::BrowserNavigated { .. } => ("browser_navigated", "tool", false),
        TurnStreamEventV3::ContextUsage { .. } => ("context_usage", "context", false),
        TurnStreamEventV3::PermissionRequest { .. } => ("permission_request", "permission", false),
        TurnStreamEventV3::SecretRequest { .. } => ("secret_request", "awaiting_operator", false),
        TurnStreamEventV3::TurnCompleted { outcome, .. } => match outcome {
            TurnCompletionOutcomeV3::Completed => ("turn_completed", "complete", true),
            TurnCompletionOutcomeV3::NeedsInput => ("turn_completed", "awaiting_operator", true),
            TurnCompletionOutcomeV3::Checkpointed => ("turn_completed", "handoff", true),
            TurnCompletionOutcomeV3::Failed
            | TurnCompletionOutcomeV3::Cancelled
            | TurnCompletionOutcomeV3::FuseExhausted => ("turn_completed", "failed", true),
        },
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
        self.publish_tracked(TurnStreamEventV3::ModelReceipt { provider, model })
            .await;
    }

    async fn content_chunk(&self, _turn_id: u64, delta: String) {
        if self.emit_cancelled_if_needed().await {
            return;
        }
        let Some((started, append)) = self.prepare_content_delta(delta) else {
            return;
        };
        if let Some(started) = started {
            self.publish_tracked(started).await;
        }
        self.publish_tracked(append).await;
    }

    async fn reasoning_chunk(&self, _turn_id: u64, delta: String) {
        if self.emit_cancelled_if_needed().await {
            return;
        }
        if let Ok(mut parts) = self.parts.lock() {
            parts.push_reasoning_delta(&delta);
        }
        self.publish_tracked(TurnStreamEventV3::ReasoningAppend { text: delta })
            .await;
    }

    async fn model_response_completed_with_text(
        &self,
        _turn_id: u64,
        model_round: usize,
        response_text: Option<String>,
    ) {
        let wait_started = Instant::now();
        let recovered_response_text = self.ensure_response_text(response_text).await;
        self.commit_active_segment(true).await;
        let pipeline = self.pipeline.metrics();
        tracing::debug!(
            target: "medousa::turn_latency",
            turn_id = %self.turn_id,
            model_round,
            response_fence_wait_us = wait_started.elapsed().as_micros() as u64,
            recovered_response_text,
            pipeline_receipt_wait_count = pipeline.receipt_wait_count,
            pipeline_receipt_wait_total_us = pipeline.receipt_wait_nanos / 1_000,
            pipeline_receipt_wait_max_us = pipeline.receipt_wait_max_nanos / 1_000,
            pipeline_blocked_send_total_us = pipeline.blocked_send_nanos / 1_000,
            "model response chronology fence committed"
        );
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

        let wire = TurnStreamEventV3::WorkerAck {
            ack_kind: WorkerAckKind::Worker,
            text: text.clone(),
            tool_names: tool_names.clone(),
            work_id: work_id.clone(),
        };
        let event = super::turn_event::TurnEvent::worker_ack_from_turn(&assistant_turn, work_id);
        if !self
            .persist_or_publish_error(assistant_turn, event.clone())
            .await
        {
            return;
        }
        self.publish_tracked_with_journal(wire, Some(event)).await;
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

        let wire = TurnStreamEventV3::WorkerAck {
            ack_kind: WorkerAckKind::Workshop,
            text: text.clone(),
            tool_names: tool_names.clone(),
            work_id: work_id.clone(),
        };
        let event = super::turn_event::TurnEvent::worker_ack_from_turn(&assistant_turn, work_id);
        if !self
            .persist_or_publish_error(assistant_turn, event.clone())
            .await
        {
            return;
        }
        self.publish_tracked_with_journal(wire, Some(event)).await;
        self.sync_ask_job_interim(text).await;
    }

    async fn agent_response(&self, _turn_id: u64, text: String, tool_names: Vec<String>) {
        if self.emit_cancelled_if_needed().await {
            return;
        }

        let body = self.terminal_body(&text).await;

        let assistant_turn = self
            .parts
            .lock()
            .map(|mut parts| {
                parts.finalize_chronological_turn(body.clone(), tool_names.clone(), None)
            })
            .unwrap_or_else(|_| {
                crate::turn_parts::conversation_turn_from_parts(
                    "assistant",
                    body.clone(),
                    tool_names.clone(),
                    None,
                    vec![crate::turn_parts::TurnPart::Text {
                        markdown: body.clone(),
                        segment_id: None,
                        model_round: None,
                    }],
                )
            });

        let final_event = TurnStreamEventV3::TurnCompleted {
            outcome: TurnCompletionOutcomeV3::Completed,
            aggregate_text: body.clone(),
            tool_names: tool_names.clone(),
            operator_message: None,
            debug_message: None,
        };
        let event = super::turn_event::TurnEvent::final_response_from_turn(&assistant_turn);
        if !self
            .persist_or_publish_error(assistant_turn, event.clone())
            .await
        {
            return;
        }
        self.publish_tracked_with_journal(final_event, Some(event))
            .await;
        self.sync_ask_job_succeeded(body).await;

        if let Some(delivery) = &self.delivery {
            delivery.mark_complete(None).await;
        }
    }

    async fn agent_turn_checkpoint(&self, _turn_id: u64, text: String, tool_names: Vec<String>) {
        if self.emit_cancelled_if_needed().await {
            return;
        }

        let body = self.terminal_body(&text).await;

        let assistant_turn = self
            .parts
            .lock()
            .map(|mut parts| {
                parts.finalize_chronological_turn(
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
                        segment_id: None,
                        model_round: None,
                    }],
                )
            });

        let checkpoint_event = TurnStreamEventV3::TurnCompleted {
            outcome: TurnCompletionOutcomeV3::Checkpointed,
            aggregate_text: body.clone(),
            tool_names: tool_names.clone(),
            operator_message: None,
            debug_message: None,
        };
        let event = super::turn_event::TurnEvent::checkpoint_from_turn(&assistant_turn);
        if !self
            .persist_or_publish_error(assistant_turn, event.clone())
            .await
        {
            return;
        }
        self.publish_tracked_with_journal(checkpoint_event, Some(event))
            .await;
        self.sync_ask_job_succeeded(body).await;

        if let Some(delivery) = &self.delivery {
            delivery.mark_complete(None).await;
        }
    }

    async fn agent_needs_input(&self, _turn_id: u64, text: String, tool_names: Vec<String>) {
        if self.emit_cancelled_if_needed().await {
            return;
        }

        let body = self.terminal_body(&text).await;

        let assistant_turn = self
            .parts
            .lock()
            .map(|mut parts| {
                parts.finalize_chronological_turn(
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
                        segment_id: None,
                        model_round: None,
                    }],
                )
            });

        let needs_input_event = TurnStreamEventV3::TurnCompleted {
            outcome: TurnCompletionOutcomeV3::NeedsInput,
            aggregate_text: body.clone(),
            tool_names: tool_names.clone(),
            operator_message: None,
            debug_message: None,
        };
        let event = super::turn_event::TurnEvent::needs_input_from_turn(&assistant_turn);
        if !self
            .persist_or_publish_error(assistant_turn, event.clone())
            .await
        {
            return;
        }
        self.publish_tracked_with_journal(needs_input_event, Some(event))
            .await;

        if let Some(delivery) = &self.delivery {
            delivery.mark_complete(None).await;
        }
    }

    async fn agent_turn_progress(&self, _turn_id: u64, message: String, tool_names: Vec<String>) {
        if self.emit_cancelled_if_needed().await {
            return;
        }

        if let Ok(mut parts) = self.parts.lock() {
            parts.archive_progress_note(&message);
        }
        self.publish_tracked(TurnStreamEventV3::Progress {
            message,
            tool_names,
        })
        .await;
    }

    async fn agent_error(&self, _turn_id: u64, message: String) {
        let failure = crate::turn_failure::TurnFailure::from_debug(&message);

        // Do not persist raw provider/runtime errors as assistant transcript turns.
        self.commit_active_segment(false).await;
        self.publish_failure(
            TurnCompletionOutcomeV3::Failed,
            failure.operator_message.clone(),
            Some(failure.debug_message.clone()),
        )
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
        self.publish_tracked(TurnStreamEventV3::Status {
            phase: "orchestration".into(),
            operator_message: None,
            debug_message: Some(message),
        })
        .await;
    }

    async fn reset_streamed_markdown(&self) {
        self.commit_active_segment(false).await;
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

        self.publish_tracked(TurnStreamEventV3::BudgetApprovalRequired {
            request_id,
            rounds_executed,
            max_tool_rounds,
            requested_rounds,
            reason,
            progress_summary,
        })
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

        self.publish_tracked(TurnStreamEventV3::BrowserChallenge {
            session_id,
            challenge_url,
            reason,
        })
        .await;
    }

    async fn browser_navigated(
        &self,
        _turn_correlation_id: &str,
        url: String,
        title: Option<String>,
        opened_by_agent: bool,
    ) {
        if self.emit_cancelled_if_needed().await {
            return;
        }

        self.publish_tracked(TurnStreamEventV3::BrowserNavigated {
            url,
            title,
            opened_by_agent,
        })
        .await;
    }

    async fn secret_request_required(
        &self,
        request_id: String,
        label: String,
        reason: String,
        provider_type: String,
        credential_key: String,
        backend: String,
        allowed_hosts: Vec<String>,
    ) {
        if self.emit_cancelled_if_needed().await {
            return;
        }

        self.publish_tracked(TurnStreamEventV3::SecretRequest {
            request_id,
            label,
            reason,
            provider_type,
            credential_key,
            backend,
            allowed_hosts,
        })
        .await;
    }

    async fn tool_invoked(&self, tool_name: String, input_summary: String) {
        self.publish_tracked(TurnStreamEventV3::Status {
            phase: "tool".into(),
            operator_message: None,
            debug_message: Some(format!("tool={tool_name} {input_summary}")),
        })
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
        let wait_started = Instant::now();
        self.commit_active_segment(false).await;
        if let Ok(mut parts) = self.parts.lock() {
            parts.tool_started_with_params(
                &tool_run_id,
                &tool_name,
                &input_summary,
                input_params.clone(),
                tool_round,
            );
        }
        self.publish_tracked(TurnStreamEventV3::ToolStarted {
            tool_run_id,
            tool_name,
            input_summary,
            input_params,
            tool_round,
        })
        .await;
        let pipeline = self.pipeline.metrics();
        tracing::debug!(
            target: "medousa::turn_latency",
            turn_id = %self.turn_id,
            tool_round,
            tool_start_boundary_wait_us = wait_started.elapsed().as_micros() as u64,
            pipeline_receipt_wait_count = pipeline.receipt_wait_count,
            pipeline_receipt_wait_total_us = pipeline.receipt_wait_nanos / 1_000,
            pipeline_receipt_wait_max_us = pipeline.receipt_wait_max_nanos / 1_000,
            pipeline_blocked_send_total_us = pipeline.blocked_send_nanos / 1_000,
            "tool invocation released after chronological start receipt"
        );
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
            self.publish_tracked(TurnStreamEventV3::ArtifactPresented {
                artifact: ui_artifact,
            })
            .await;
        }
        if crate::ui_build_tools::is_ui_scene_stream_tool(&tool_name)
            && let Some(scene) = super::tool_stream::scene_ops_from_tool_output(&tool_output)
        {
            self.publish_tracked(TurnStreamEventV3::UiScene { scene })
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
                self.publish_tracked(TurnStreamEventV3::ArtifactUpdated {
                    previous_artifact_id: previous.to_string(),
                    artifact: ui_artifact,
                    root_artifact_id: root.map(str::to_string),
                })
                .await;
            } else {
                self.publish_tracked(TurnStreamEventV3::ArtifactPresented {
                    artifact: ui_artifact,
                })
                .await;
            }
        }
        self.publish_tracked(TurnStreamEventV3::ToolFinished {
            tool_run_id,
            tool_name,
            status,
            input_summary,
            input_params: super::tool_stream::preview_tool_input(&tool_input),
            output_summary,
            tool_round,
            artifact_refs,
        })
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
        self.publish_tracked(TurnStreamEventV3::Status {
            phase: "tool".into(),
            operator_message: Some(format!("tool_payload={tool_name}")),
            debug_message: None,
        })
        .await;
    }
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
    execution_context: Arc<super::execution_context::TurnExecutionContext>,
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
        let pipeline = TurnPipelineHandle::spawn(
            turn_id,
            0,
            daemon_turn_pipeline_budget(),
            Arc::new(TurnJournalOutput::new(
                Arc::clone(&stream.channel),
                Arc::clone(&stream.log),
            )),
        );
        if let Err(error) = pipeline
            .emit_v3(TurnStreamEventV3::Status {
                phase: "accepted".into(),
                operator_message: Some("interactive turn accepted; agent runtime started".into()),
                debug_message: None,
            })
            .await
        {
            tracing::warn!(turn_id, %error, "turn pipeline rejected accepted event");
            return;
        }

        let session_id = request.session_id.trim().to_string();
        let interactive_sink = Arc::new(InteractiveTurnStreamSink {
            turn_id: turn_id.to_string(),
            session_id,
            pipeline,
            delivery,
            session_hooks: session_hooks.unwrap_or_default(),
            parts: std::sync::Mutex::new(TurnPartsAccumulator::default()),
            text: std::sync::Mutex::new(ChronologicalTextState::default()),
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
            execution_context,
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
    execution_context: Arc<super::execution_context::TurnExecutionContext>,
    context_telemetry: Option<Arc<InteractiveTurnStreamSink>>,
    project_state: Option<crate::daemon::state::AppState>,
) {
    let turn_correlation_id = continuation_scope
        .as_ref()
        .map(|scope| scope.turn_correlation_id.clone());
    let outcome: Arc<RwLock<Option<TurnOutcome>>> = Arc::new(RwLock::new(None));
    let tracking_sink: SharedAgentStreamSink = Arc::new(TurnOutcomeTrackingSink {
        inner: sink,
        outcome: outcome.clone(),
    });
    let tool_sink = crate::engine_adapters::AgentStreamToolSinkAdapter::new(tracking_sink.clone());
    let turn_future = crate::engine_adapters::with_active_tool_sink(
        tool_sink,
        run_agent_turn_inner(
            turn_id,
            request,
            backend,
            agent_rt,
            tracking_sink.clone(),
            context_telemetry,
            project_state,
        ),
    );
    let cancellation = execution_context.cancellation().clone();
    let deadline = tokio::time::Instant::from_std(execution_context.deadline());
    let scoped_turn =
        super::execution_context::with_turn_execution_context(execution_context, turn_future);
    tokio::pin!(scoped_turn);
    tokio::select! {
        () = cancellation.cancelled() => {
            tracking_sink
                .agent_error(0, "turn cancelled".to_string())
                .await;
            // Let Coder unwind its bound shell before its Forge lease drops.
            // Dropping the future here can strand a PTY that still has access
            // to a principal-owned attached checkout.
            scoped_turn.await;
        }
        () = tokio::time::sleep_until(deadline) => {
            cancellation.cancel();
            tracking_sink
                .agent_error(0, "turn execution deadline exceeded".to_string())
                .await;
            scoped_turn.await;
        }
        () = &mut scoped_turn => {}
    }

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
}

pub(crate) fn prepare_attached_native_coder_handoff(
    forge: &medousa_forge::forge::Forge,
    work_id: &medousa_forge::model::WorkId,
    session_id: &str,
    turn_id: &str,
) -> Result<(), medousa_forge::error::ForgeError> {
    let item = forge.load(work_id)?;
    if !item.uses_attached_checkout() {
        return Ok(());
    }
    let active_human = item
        .active_attempt_ids()
        .into_iter()
        .filter_map(|attempt_id| item.attempt(attempt_id))
        .find(|attempt| attempt.executor.kind == "human")
        .and_then(|attempt| attempt.lease.clone());
    let Some(lease) = active_human else {
        return Ok(());
    };
    forge.append_command_log(
        &lease,
        &serde_json::json!({
            "kind": "executor_handoff",
            "from": "human",
            "to": "medousa-coder",
            "session_id": session_id,
            "turn_id": turn_id,
            "at": chrono::Utc::now(),
        }),
    )?;
    forge.interrupt_attempt(
        &lease,
        medousa_forge::model::RecoveryDisposition::RestartAllowed,
        &medousa_forge::forge::Forge::system_actor(),
    )?;
    Ok(())
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

    let active_execution = super::execution_context::active_turn_execution_context();
    let bot_default_mode = active_execution
        .as_deref()
        .and_then(super::execution_context::TurnExecutionContext::bot_identity)
        .and_then(super::execution_context::BotTurnIdentity::default_mode);
    let memory_session_id = active_execution
        .as_deref()
        .map(super::execution_context::TurnExecutionContext::memory_session_id)
        .unwrap_or(&session_id)
        .to_string();
    let bot_profile_appendix = active_execution
        .as_deref()
        .and_then(super::execution_context::TurnExecutionContext::bot_identity)
        .map(super::execution_context::BotTurnIdentity::prompt_appendix);
    let mode_selection = crate::agent_mode_state::resolve_for_turn_with_fallback(
        &session_id,
        request.agent_mode,
        bot_default_mode,
    );
    let mut agent_mode = match super::modes::resolve_agent_mode(mode_selection.mode) {
        Ok(mode) => mode,
        Err(err) => {
            sink.agent_error(1, err.to_string()).await;
            return;
        }
    };
    let session_code_binding = crate::agent_mode_state::get_session_code_binding(&session_id).ok();
    let mut resolved_code_context = request.code_context.clone().unwrap_or_default();
    if resolved_code_context
        .work_id
        .as_deref()
        .is_none_or(|value| value.trim().is_empty())
        && let Some(binding) = session_code_binding.as_ref()
    {
        resolved_code_context.work_id = binding.work_id.clone();
    }
    let local_runtime_id = crate::workshop_authority::current()
        .map(|authority| authority.as_str().to_string())
        .unwrap_or_else(|_| crate::workshop_contract::default_unknown_runtime_id());
    let remote_coder_binding = (agent_mode.id == crate::daemon_api::AgentModeId::Coder)
        .then(|| session_code_binding.clone())
        .flatten()
        .filter(|binding| {
            binding
                .execution_runtime_id
                .as_deref()
                .map(str::trim)
                .is_some_and(|runtime_id| {
                    !runtime_id.is_empty()
                        && runtime_id != local_runtime_id
                        && runtime_id != crate::workshop_contract::UNKNOWN_EXECUTION_RUNTIME_ID
                })
        });
    let forge = project_state.as_ref().map(|state| state.forge.clone());
    let checkpoint_store = super::coder_turn_checkpoint::coder_turn_checkpoint_store();
    let (
        coder_authority,
        coder_registry,
        coder_entry,
        coder_resume_checkpoint,
        mode_context_appendix,
        tool_registry_override,
    ) = if remote_coder_binding.is_some() {
        agent_mode.coder_phase = Some(super::modes::CoderRuntimePhase::Work);
        (None, None, None, None, None, None)
    } else if agent_mode.id == crate::daemon_api::AgentModeId::Coder {
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
            if let Err(err) = prepare_attached_native_coder_handoff(
                forge.as_ref(),
                &work_id,
                &session_id,
                turn_id,
            ) {
                sink.agent_error(
                    1,
                    format!("cannot hand the current checkout to Coder: {err}"),
                )
                .await;
                return;
            }
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
                        && item.attempt(source).is_some_and(|attempt| {
                            attempt.environment.is_some() || item.uses_attached_checkout()
                        })
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
                match forge.begin_workspace_attempt_from(
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
                        forge.begin_workspace_attempt(
                            &work_id,
                            executor,
                            Some(std::process::id()),
                            &medousa_forge::forge::Forge::system_actor(),
                        )
                    }
                }
            } else {
                forge.begin_workspace_attempt(
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
            let local_runtime_id = crate::workshop_authority::current()
                .map(|authority| authority.as_str().to_string())
                .unwrap_or_else(|_| crate::workshop_contract::default_unknown_runtime_id());
            if let Err(err) = crate::agent_mode_state::set_session_code_binding_authority(
                &session_id,
                &entry.work_id,
                Some(&local_runtime_id),
                Some(&entry.repo_id),
            ) {
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
    } else if agent_mode.id == crate::daemon_api::AgentModeId::Instant {
        let registry_override: Arc<
            dyn stasis::application::orchestration::tool_registry::ToolRegistry,
        > = Arc::new(super::turn_worker::AllowlistToolRegistry::new_exact(
            agent_rt.tool_registry.clone(),
            crate::agent_mode_context::instant_tool_names(),
        ));
        (
            None,
            None,
            None,
            None,
            Some(crate::agent_mode_context::INSTANT_CAPABILITY_CONTEXT.to_string()),
            Some(registry_override),
        )
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
    if let Some(bot) = active_execution
        .as_deref()
        .and_then(super::execution_context::TurnExecutionContext::bot_identity)
    {
        sink.notice(format!(
            "◈ bot_identity id={} revision={} memory_scope=bot",
            bot.bot_id(),
            bot.profile_revision(),
        ))
        .await;
    }

    if has_media && let Err(err) = validate_media_refs(&session_id, &request.media_refs) {
        sink.agent_error(1, err).await;
        return;
    }

    let saved_defaults = crate::session::load_tui_defaults();
    let settings = runtime_settings_for_interactive_turn(backend, &request);
    let stage_routing = stage_routing_for_interactive_turn(&request);
    let final_route = stage_routing.get("final_response").cloned();
    let verifier_route = stage_routing.get("verifier").cloned();
    let selected_target = final_route
        .as_ref()
        .map(|route| crate::inference_profiles::InferenceTarget {
            provider: route.provider.clone(),
            model: route.model.clone(),
            base_url: (route
                .provider
                .eq_ignore_ascii_case(settings.provider.trim())
                && !settings.base_url.trim().is_empty())
            .then(|| settings.base_url.clone()),
        })
        .unwrap_or_else(|| crate::inference_profiles::InferenceTarget {
            provider: settings.provider.clone(),
            model: settings.model.clone(),
            base_url: (!settings.base_url.trim().is_empty()).then(|| settings.base_url.clone()),
        });
    let inference_profile_kind = if has_vision_media {
        crate::inference_profiles::InferenceProfileKind::Vision
    } else {
        crate::inference_profiles::InferenceProfileKind::Main
    };
    let mut inference_targets = if has_vision_media {
        crate::inference_router::vision_targets_for_turn(selected_target, &saved_defaults)
    } else {
        crate::inference_router::main_targets_for_turn(selected_target, &saved_defaults)
    };
    if has_vision_media {
        inference_targets.retain(|target| {
            crate::model_capability_registry::registry()
                .supports_vision(&target.provider, &target.model)
        });
    }
    let Some(active_inference_target) = inference_targets.first().cloned() else {
        sink.agent_error(
            1,
            "The selected model cannot read images. Choose a vision-capable model or configure a Vision fallback in Settings → Models."
                .to_string(),
        )
        .await;
        return;
    };
    let vision_plan = if has_vision_media {
        match media_vision::plan_turn_media(
            &session_id,
            &request.media_refs,
            &active_inference_target.provider,
            &active_inference_target.model,
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
        && let Some(notice) = vision_plan.stream_notice(
            &active_inference_target.provider,
            &active_inference_target.model,
        )
    {
        sink.notice(notice).await;
    }

    let active_turn_resume =
        super::coder_turn_checkpoint::CoderTurnCheckpointController::initial_resume_state(
            coder_resume_checkpoint.clone(),
        );
    let event_log = if let Some(state) = project_state.as_ref() {
        state
            .interactive_turn_streams
            .read()
            .await
            .get(turn_id)
            .map(|entry| entry.log.clone())
    } else {
        None
    };
    let active_turn_checkpoint_sink: Option<
        Arc<dyn super::coder_turn_checkpoint::ActiveTurnCheckpointSink>,
    > = if let (Some(authority), Some(registry), Some(entry), Some(forge)) = (
        coder_authority.as_ref(),
        coder_registry.as_ref(),
        coder_entry.as_ref(),
        project_state.as_ref().map(|state| state.forge.clone()),
    ) {
        let (checkpoint_provider, checkpoint_model) = (
            active_inference_target.provider.clone(),
            active_inference_target.model.clone(),
        );
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
                event_log,
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
        let caused_by = crate::workshop_authority::execution_ref(&session_id, turn_id).ok();
        if let Err(error) = crate::session_writer::persist_turn_with_execution(
            &session_id,
            user_turn,
            None,
            caused_by,
        )
        .await
        {
            sink.agent_error(1, format!("user turn persistence failed: {error}"))
                .await;
            return;
        }
    }

    if let Some(binding) = remote_coder_binding.as_ref() {
        let runtime_id = binding
            .execution_runtime_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .expect("remote Coder binding has a runtime id");
        if let Some(selection) = request.worker_execution_target.as_ref() {
            let matches_binding = matches!(
                selection,
                crate::workshop_contract::ExecutionTargetSelection::Exact {
                    runtime_id: selected
                } if selected.trim() == runtime_id
            );
            if !matches_binding {
                sink.agent_error(
                    1,
                    "Coder's active project is bound to a different workshop; reopen the project before changing execution targets"
                        .to_string(),
                )
                .await;
                return;
            }
        }
        let user_ack = "Coder is working on this in the selected workshop.".to_string();
        let result = agent_rt
            .tool_registry
            .invoke_tool(
                crate::public_api::COGNITION_WORKSHOP_MUTATE,
                serde_json::json!({
                    "action": "workshop.spawn",
                    "intent": "coder",
                    "task": effective_prompt,
                    "user_ack": user_ack,
                    "execution_target": {
                        "kind": "exact",
                        "runtime_id": runtime_id,
                    },
                }),
            )
            .await;
        match result {
            Ok(output) => {
                let work_id = output
                    .get("work_id")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                sink.agent_worker_ack(
                    1,
                    user_ack,
                    vec![crate::public_api::COGNITION_WORKSHOP_MUTATE.to_string()],
                    work_id,
                )
                .await;
            }
            Err(error) => {
                sink.agent_error(1, format!("remote Coder admission failed: {error}"))
                    .await;
            }
        }
        return;
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
        memory_session_id: &memory_session_id,
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
        bot_profile_appendix: bot_profile_appendix.as_deref(),
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
        inference_profile_kind,
        inference_targets,
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

    let compiled_policy = super::modes::compiled_system_policy_for_mode(&agent_mode);
    let system_prompt = &compiled_policy.rendered;
    let tool_footprint = crate::agent_runtime::context_usage::measure_tool_schema_footprint(
        &assembled.execution.tool_registry,
    )
    .await;
    let tool_count = tool_footprint.tool_count;
    let tool_schema_chars = tool_footprint.total_chars;
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
    let policy_slice_chars = compiled_policy
        .footprint
        .slices
        .iter()
        .map(|slice| (slice.id, slice.chars, slice.tokens_estimate))
        .collect::<Vec<_>>();
    let largest_tool_schema_chars = tool_footprint
        .tools
        .iter()
        .take(8)
        .map(|tool| (tool.name.as_str(), tool.chars, tool.tokens_estimate))
        .collect::<Vec<_>>();
    tracing::debug!(
        target: "medousa::context_usage",
        turn_id = %turn_id,
        policy_mode = compiled_policy.mode.as_str(),
        policy_actor = compiled_policy.actor.as_str(),
        policy_total_chars = compiled_policy.footprint.total_chars,
        policy_total_tokens_estimate = compiled_policy.footprint.total_tokens_estimate,
        policy_envelope_chars = compiled_policy.footprint.envelope_chars,
        policy_envelope_tokens_estimate = compiled_policy.footprint.envelope_tokens_estimate,
        estimator = crate::agent_runtime::context_usage::ESTIMATOR_LABEL,
        policy_slice_chars = ?policy_slice_chars,
        tool_schema_total_chars = tool_footprint.total_chars,
        largest_tool_schema_chars = ?largest_tool_schema_chars,
        "turn prompt footprint constituents"
    );
    if let Some(stream_sink) = context_telemetry {
        if let Some(cache) = &stream_sink.session_hooks.context_usage_by_session {
            let session_id = stream_sink.session_id.clone();
            cache
                .write()
                .await
                .insert(session_id, context_report.clone());
        }
        stream_sink
            .publish_tracked(TurnStreamEventV3::ContextUsage {
                report: context_report,
                operator_summary: Some(context_summary),
            })
            .await;
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

    async fn model_response_completed_with_text(
        &self,
        turn_id: u64,
        model_round: usize,
        response_text: Option<String>,
    ) {
        self.inner
            .model_response_completed_with_text(turn_id, model_round, response_text)
            .await;
    }

    async fn agent_worker_ack(
        &self,
        turn_id: u64,
        text: String,
        tool_names: Vec<String>,
        work_id: Option<String>,
    ) {
        *self.outcome.write().await = Some(TurnOutcome::Success);
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

    async fn agent_turn_progress(&self, turn_id: u64, message: String, tool_names: Vec<String>) {
        self.inner
            .agent_turn_progress(turn_id, message, tool_names)
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

    async fn reset_streamed_markdown(&self) {
        self.inner.reset_streamed_markdown().await;
    }
}

#[cfg(test)]
mod chronological_sink_tests {
    use std::sync::Mutex;

    use medousa_engine::{
        TURN_PIPELINE_BYTE_CAPACITY, TurnPipelineEmission, TurnPipelineEnvelope, TurnPipelineError,
        TurnPipelineOutput,
    };
    use medousa_types::turn::TurnPart;
    use tokio::sync::Semaphore;

    use super::*;

    #[derive(Default)]
    struct RecordingOutput {
        events: Mutex<Vec<TurnPipelineEnvelope>>,
    }

    impl TurnPipelineOutput for RecordingOutput {
        async fn publish(&self, emission: TurnPipelineEmission) -> Result<(), TurnPipelineError> {
            self.events
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(emission.envelope);
            Ok(())
        }
    }

    fn sink(output: Arc<RecordingOutput>) -> InteractiveTurnStreamSink {
        InteractiveTurnStreamSink {
            turn_id: "turn-chronological".into(),
            session_id: "session-1".into(),
            pipeline: TurnPipelineHandle::spawn(
                "turn-chronological",
                0,
                Arc::new(Semaphore::new(TURN_PIPELINE_BYTE_CAPACITY * 2)),
                output,
            ),
            delivery: None,
            session_hooks: InteractiveTurnSessionHooks::default(),
            parts: std::sync::Mutex::new(TurnPartsAccumulator::default()),
            text: std::sync::Mutex::new(ChronologicalTextState::default()),
            pending_slice_scratch: std::sync::Mutex::new(None),
        }
    }

    #[tokio::test]
    async fn completed_model_responses_commit_chronological_segments() {
        let output = Arc::new(RecordingOutput::default());
        let sink = sink(Arc::clone(&output));

        sink.content_chunk(1, "First response.".into()).await;
        sink.model_response_completed_with_text(1, 1, None).await;
        sink.content_chunk(1, "Second response.".into()).await;
        sink.model_response_completed_with_text(1, 2, None).await;
        let body = sink
            .terminal_body("First response.\n\nSecond response.")
            .await;
        sink.publish_tracked(TurnStreamEventV3::TurnCompleted {
            outcome: TurnCompletionOutcomeV3::Completed,
            aggregate_text: body.clone(),
            tool_names: Vec::new(),
            operator_message: None,
            debug_message: None,
        })
        .await;

        assert_eq!(body, "First response.\n\nSecond response.");
        let turn = sink
            .parts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .finalize_chronological_turn(body, Vec::new(), None);
        let text_parts = turn
            .parts
            .unwrap()
            .into_iter()
            .filter_map(|part| match part {
                TurnPart::Text {
                    markdown,
                    segment_id,
                    model_round,
                } => Some((markdown, segment_id, model_round)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(text_parts.len(), 2);
        assert_eq!(text_parts[0].0, "First response.");
        assert_eq!(text_parts[0].2, Some(1));
        assert_eq!(text_parts[1].0, "Second response.");
        assert_eq!(text_parts[1].2, Some(2));
        assert!(text_parts.iter().all(|(_, id, _)| id.is_some()));

        let events = output
            .events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(
            events
                .iter()
                .map(TurnPipelineEnvelope::seq)
                .collect::<Vec<_>>(),
            [1, 2, 3, 4, 5, 6, 7]
        );
        assert!(
            events
                .iter()
                .all(|event| matches!(event, TurnPipelineEnvelope::V3(_)))
        );
        assert!(!events.iter().any(|event| matches!(
            event,
            TurnPipelineEnvelope::V3(envelope)
                if matches!(&envelope.event, TurnStreamEventV3::Progress { .. })
        )));
        drop(events);
        sink.pipeline.cancel();
    }

    #[tokio::test]
    async fn completed_response_recovers_prose_that_provider_did_not_stream() {
        let output = Arc::new(RecordingOutput::default());
        let sink = sink(Arc::clone(&output));

        sink.model_response_completed_with_text(1, 1, Some("I found a lead.".into()))
            .await;
        sink.tool_run_started(
            "run-1".into(),
            "search".into(),
            "query".into(),
            Vec::new(),
            1,
        )
        .await;

        assert_eq!(sink.aggregate_text(), "I found a lead.");
        let events = output
            .events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert!(matches!(
            &events[0],
            TurnPipelineEnvelope::V3(envelope)
                if matches!(&envelope.event, TurnStreamEventV3::AssistantTextStarted { .. })
        ));
        assert!(matches!(
            &events[1],
            TurnPipelineEnvelope::V3(envelope)
                if matches!(&envelope.event, TurnStreamEventV3::ContentAppend { text, .. } if text == "I found a lead.")
        ));
        assert!(matches!(
            &events[2],
            TurnPipelineEnvelope::V3(envelope)
                if matches!(&envelope.event, TurnStreamEventV3::AssistantTextCommitted { .. })
        ));
        assert!(matches!(
            &events[3],
            TurnPipelineEnvelope::V3(envelope)
                if matches!(&envelope.event, TurnStreamEventV3::ToolStarted { .. })
        ));
        drop(events);
        sink.pipeline.cancel();
    }

    #[tokio::test]
    async fn tool_start_commits_visible_prose_before_the_receipt() {
        let output = Arc::new(RecordingOutput::default());
        let sink = sink(Arc::clone(&output));

        sink.content_chunk(1, "I’ll inspect it.".into()).await;
        sink.tool_run_started(
            "run-1".into(),
            "search".into(),
            "query".into(),
            Vec::new(),
            1,
        )
        .await;

        let events = output
            .events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(events.len(), 4);
        assert!(matches!(
            &events[2],
            TurnPipelineEnvelope::V3(envelope)
                if matches!(&envelope.event, TurnStreamEventV3::AssistantTextCommitted { .. })
        ));
        assert!(matches!(
            &events[3],
            TurnPipelineEnvelope::V3(envelope)
                if matches!(&envelope.event, TurnStreamEventV3::ToolStarted { tool_run_id, .. } if tool_run_id == "run-1")
        ));
        let projected_v2 = events
            .iter()
            .filter_map(|event| match event {
                TurnPipelineEnvelope::V3(envelope) => {
                    crate::sse_turn_projection::v3_to_v2(envelope)
                        .expect("valid V2 compatibility projection")
                }
                TurnPipelineEnvelope::V2(_) => panic!("sink must author native V3 facts"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            projected_v2
                .iter()
                .map(|event| event.seq)
                .collect::<Vec<_>>(),
            [2, 4]
        );
        assert!(matches!(
            &projected_v2[0].event,
            medousa_types::TurnStreamEventV2::ContentAppend { text }
                if text == "I’ll inspect it."
        ));
        assert!(matches!(
            &projected_v2[1].event,
            medousa_types::TurnStreamEventV2::ToolStarted { tool_run_id, .. }
                if tool_run_id == "run-1"
        ));
        drop(events);
        sink.pipeline.cancel();
    }

    #[tokio::test]
    async fn terminal_tool_message_follows_earlier_interim_prose() {
        let output = Arc::new(RecordingOutput::default());
        let sink = sink(Arc::clone(&output));

        sink.content_chunk(1, "I found the likely cause.".into())
            .await;
        sink.model_response_completed_with_text(1, 1, None).await;
        sink.model_response_completed_with_text(1, 2, None).await;
        let body = sink.terminal_body("The fix is ready.").await;

        assert_eq!(body, "I found the likely cause.\n\nThe fix is ready.");
        let parts = sink
            .parts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .preview_parts();
        let text = parts
            .iter()
            .filter_map(|part| match part {
                TurnPart::Text { markdown, .. } => Some(markdown.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(text, ["I found the likely cause.", "The fix is ready."]);

        sink.pipeline.cancel();
    }

    #[tokio::test]
    async fn turn_progress_is_archived_at_its_timeline_position() {
        let output = Arc::new(RecordingOutput::default());
        let sink = sink(Arc::clone(&output));

        sink.agent_turn_progress(1, "Checking the durable transcript.".into(), vec![])
            .await;

        let parts = sink
            .parts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .preview_parts();
        assert!(matches!(
            parts.as_slice(),
            [TurnPart::Progress { markdown }] if markdown == "Checking the durable transcript."
        ));

        sink.pipeline.cancel();
    }
}
