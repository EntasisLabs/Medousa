//! Resume the host turn after a parallel spawn cohort finishes.
//!
//! Bound workshop keeps prompt-only synthesis. Parallel workers return receipts
//! to a new host tool-loop turn so the host can answer.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use once_cell::sync::OnceCell;
use serde_json::Value;

use crate::agent_runtime::execution_context::TurnExecutionRegistry;
use crate::agent_runtime::prompt_prep::truncate_text_for_budget;
use crate::agent_runtime::stream_sink::{AgentStreamSink, SharedAgentStreamSink};
use crate::agent_runtime::turn_worker::store::{TurnWorkDisposition, TurnWorkStatus};
use crate::agent_runtime::turn_worker::{TurnWorkRecord, WorkerRuntimeContext, turn_worker_store};
use crate::agent_runtime::{MAX_REQUEST_PROMPT_CHARS, run_agent_turn};
use crate::daemon_api::{AgentModeId, CodeIntentContext};
use crate::payload_receipt::ArtifactReceiptMeta;
use crate::session_mapping::build_interactive_turn_request_for_ingest;
use crate::stage_routing::StageRoutingMatrix;
use crate::tools::TuiRuntime;
use crate::turn_continuation::TurnContinuationScope;
use crate::turn_ticket::{TurnTicketRegistry, get_active_interactive_turn};

#[derive(Clone)]
struct HostResumePorts {
    turn_tickets: TurnTicketRegistry,
    project_state: crate::daemon::state::AppState,
    backend: String,
}

static HOST_RESUME_PORTS: OnceCell<HostResumePorts> = OnceCell::new();

pub fn register_host_resume_ports(
    turn_tickets: TurnTicketRegistry,
    project_state: crate::daemon::state::AppState,
    backend: impl Into<String>,
) {
    let _ = HOST_RESUME_PORTS.set(HostResumePorts {
        turn_tickets,
        project_state,
        backend: backend.into(),
    });
}

pub fn host_resume_prompt(records: &[TurnWorkRecord]) -> String {
    let parent_prompt = records
        .iter()
        .find_map(|record| {
            record
                .parent_user_prompt
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
        .unwrap_or("(original user prompt unavailable)");
    let mut blocks = Vec::new();
    for record in records {
        let status = match record.status {
            TurnWorkStatus::Completed => "completed",
            TurnWorkStatus::Failed => "failed",
            TurnWorkStatus::Cancelled => "cancelled",
            TurnWorkStatus::Pending => "pending",
            TurnWorkStatus::Running => "running",
        };
        let body = if let Some(error) = record
            .error
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            format!("error: {error}")
        } else {
            record
                .result_text
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("(worker produced no text)")
                .to_string()
        };
        blocks.push(format!(
            "- work_id={}\n  status={status}\n  intent={}\n  task={}\n  result:\n{}",
            record.work_id,
            record.intent,
            record.task_prompt.trim(),
            body
        ));
    }
    format!(
        "[MEDOUSA_WORKER_RESULTS]\n\
         source=parallel_workers\n\
         audience=host\n\n\
         ORIGINAL_USER_MESSAGE:\n{parent_prompt}\n\n\
         WORKERS:\n{}",
        blocks.join("\n\n")
    )
}

pub fn fallback_host_resume_text(records: &[TurnWorkRecord]) -> String {
    records
        .iter()
        .map(|record| {
            let heading = format!("{} ({:?})", record.work_id, record.status);
            let body = record
                .result_text
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .or_else(|| record.error.clone())
                .unwrap_or_else(|| "Worker finished with no text.".to_string());
            format!("{heading}\n{body}")
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

async fn session_blocks_host_resume(session_id: &str) -> bool {
    let Some(ports) = HOST_RESUME_PORTS.get() else {
        return false;
    };
    let active = get_active_interactive_turn(&ports.turn_tickets, session_id).await;
    let composer_handoff = active
        .turn
        .as_ref()
        .map(|turn| turn.composer_handoff)
        .unwrap_or(true);
    interactive_turn_blocks_host_resume(active.turn.is_some(), composer_handoff)
}

fn interactive_turn_blocks_host_resume(has_active_turn: bool, composer_handoff: bool) -> bool {
    has_active_turn && !composer_handoff
}

fn parent_agent_mode(records: &[TurnWorkRecord]) -> Option<AgentModeId> {
    match records
        .iter()
        .find_map(|record| record.parent_agent_mode.as_deref())
    {
        Some("coder") => Some(AgentModeId::Coder),
        Some("teacher") => Some(AgentModeId::Teacher),
        Some("instant") => Some(AgentModeId::Instant),
        Some("general") => Some(AgentModeId::General),
        _ => None,
    }
}

fn parent_code_context(records: &[TurnWorkRecord]) -> Option<CodeIntentContext> {
    let work_id = records.iter().find_map(|record| {
        record
            .parent_code_work_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })?;
    Some(CodeIntentContext {
        work_id: Some(work_id),
        ..CodeIntentContext::default()
    })
}

/// After a parallel worker reaches a terminal state, resume the host once the
/// whole spawn cohort is finished. Bound workshop is handled by synthesis.
pub async fn maybe_resume_host_after_parallel_worker(
    ctx: &WorkerRuntimeContext,
    execution_registry: &TurnExecutionRegistry,
    agent: &TuiRuntime,
    record: &TurnWorkRecord,
    sink: SharedAgentStreamSink,
) {
    if record.disposition != TurnWorkDisposition::Parallel {
        return;
    }
    if session_blocks_host_resume(&record.session_id).await {
        sink.notice(format!(
            "◈ host_resume deferred session_id={} parent_stream_turn_id={} (session busy)",
            record.session_id, record.parent_stream_turn_id
        ))
        .await;
        return;
    }
    let Some(cohort) = turn_worker_store()
        .try_claim_parallel_cohort_intake(&record.session_id, record.parent_stream_turn_id)
    else {
        return;
    };
    sink.notice(format!(
        "◈ host_resume cohort session_id={} parent_stream_turn_id={} workers={}",
        record.session_id,
        record.parent_stream_turn_id,
        cohort.len()
    ))
    .await;
    run_host_resume_turn(ctx, execution_registry, agent, &cohort, sink).await;
}

#[allow(clippy::too_many_arguments)]
async fn run_host_resume_turn(
    ctx: &WorkerRuntimeContext,
    execution_registry: &TurnExecutionRegistry,
    agent: &TuiRuntime,
    cohort: &[TurnWorkRecord],
    sink: SharedAgentStreamSink,
) {
    let Some(primary) = cohort.first() else {
        return;
    };
    let Some(identity_user_id) = primary
        .identity_user_id
        .clone()
        .filter(|value| !value.trim().is_empty())
    else {
        tracing::warn!(
            work_id = %primary.work_id,
            "refusing host resume without identity"
        );
        deliver_fallback(cohort, sink).await;
        return;
    };
    if !crate::session_catalog::session_visible_to_profile(&primary.session_id, &identity_user_id) {
        tracing::warn!(
            work_id = %primary.work_id,
            "refusing host resume after authority revocation"
        );
        return;
    }

    let prompt = truncate_text_for_budget(&host_resume_prompt(cohort), MAX_REQUEST_PROMPT_CHARS);
    let mut request = build_interactive_turn_request_for_ingest(
        &primary.session_id,
        prompt,
        &ctx.provider,
        &ctx.model,
        &primary.response_depth_mode,
        crate::reasoning_effort::REASONING_EFFORT_DEFAULT,
        None,
        None,
        None,
        None,
    );
    request.persist_user_turn = false;
    request.agent_mode = parent_agent_mode(cohort);
    request.code_context = parent_code_context(cohort);
    request.identity_user_id = Some(identity_user_id.clone());
    request.provider = ctx.provider.clone();
    request.model = ctx.model.clone();
    request.stage_routing = StageRoutingMatrix::default_for(&ctx.provider, &ctx.model);
    request.max_tool_rounds = Some(primary.max_tool_rounds.max(1));

    let ports = HOST_RESUME_PORTS.get();
    let backend = ports
        .map(|ports| ports.backend.as_str())
        .unwrap_or("daemon");
    let project_state = ports.map(|ports| ports.project_state.clone());
    let turn_id = format!("{}-host-resume", primary.work_id);
    let scope = TurnContinuationScope {
        turn_correlation_id: turn_id.clone(),
        session_id: primary.session_id.clone(),
        identity_user_id: Some(identity_user_id.clone()),
        original_prompt: request.prompt.clone(),
        delivery_target: primary
            .delivery_target
            .as_ref()
            .map(crate::channel_delivery::ChannelDeliveryTarget::from),
        provider: request.provider.clone(),
        model: request.model.clone(),
        response_depth_mode: request.response_depth_mode.clone(),
        supports_ui_artifacts: primary.supports_ui_artifacts,
        supports_liquid_markdown: primary.supports_liquid_markdown,
        supports_browser_host: primary.supports_browser_host,
        channel_surface: Some("host-resume".to_string()),
    };
    let execution = match crate::agent_runtime::execution_context::TurnExecutionContext::from_scope(
        turn_id.clone(),
        crate::request_principal::RequestPrincipal::continuation(identity_user_id),
        tokio_util::sync::CancellationToken::new(),
        std::time::Instant::now() + std::time::Duration::from_secs(2 * 60 * 60),
        scope.clone(),
    ) {
        Ok(execution) => execution,
        Err(error) => {
            tracing::warn!(work_id = %primary.work_id, error = %error, "host resume execution context failed");
            deliver_fallback(cohort, sink).await;
            return;
        }
    };
    let execution_lease = match execution_registry.admit(execution) {
        Ok(lease) => lease,
        Err(error) => {
            tracing::warn!(work_id = %primary.work_id, error = %error, "host resume admission rejected");
            deliver_fallback(cohort, sink).await;
            return;
        }
    };

    let captured = Arc::new(Mutex::new(None));
    let resume_sink: SharedAgentStreamSink = Arc::new(HostResumeSink {
        inner: sink.clone(),
        primary: primary.clone(),
        captured: captured.clone(),
    });
    run_agent_turn(
        &turn_id,
        request,
        backend,
        agent,
        resume_sink,
        Some(scope),
        execution_lease.context().clone(),
        None,
        project_state,
    )
    .await;
    drop(execution_lease);

    let delivered = captured
        .lock()
        .expect("host resume capture")
        .as_ref()
        .is_some_and(|text| !text.trim().is_empty());
    if !delivered {
        deliver_fallback(cohort, sink).await;
    }
}

async fn deliver_fallback(cohort: &[TurnWorkRecord], sink: SharedAgentStreamSink) {
    let Some(primary) = cohort.first() else {
        return;
    };
    let text = fallback_host_resume_text(cohort);
    let tool_names: Vec<String> = cohort
        .iter()
        .flat_map(|record| record.tool_names.iter().cloned())
        .collect();
    sink.reset_streamed_markdown().await;
    sink.agent_response(
        primary.parent_stream_turn_id,
        text.clone(),
        tool_names.clone(),
    )
    .await;
    crate::turn_worker_notify::publish_worker_synthesis_to_parent_turn(primary, &text, &tool_names)
        .await;
}

struct HostResumeSink {
    inner: SharedAgentStreamSink,
    primary: TurnWorkRecord,
    captured: Arc<Mutex<Option<String>>>,
}

#[async_trait]
impl AgentStreamSink for HostResumeSink {
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

    async fn agent_response(&self, turn_id: u64, text: String, tool_names: Vec<String>) {
        self.capture_delivery(&text);
        self.inner
            .agent_response(turn_id, text.clone(), tool_names.clone())
            .await;
        crate::turn_worker_notify::publish_worker_synthesis_to_parent_turn(
            &self.primary,
            &text,
            &tool_names,
        )
        .await;
    }

    async fn agent_needs_input(&self, turn_id: u64, text: String, tool_names: Vec<String>) {
        self.capture_delivery(&text);
        self.inner
            .agent_needs_input(turn_id, text.clone(), tool_names.clone())
            .await;
        crate::turn_worker_notify::publish_worker_synthesis_to_parent_turn(
            &self.primary,
            &text,
            &tool_names,
        )
        .await;
    }

    async fn agent_turn_progress(&self, turn_id: u64, message: String, tool_names: Vec<String>) {
        self.inner
            .agent_turn_progress(turn_id, message, tool_names)
            .await;
    }

    async fn agent_turn_checkpoint(&self, turn_id: u64, message: String, tool_names: Vec<String>) {
        self.capture_delivery(&message);
        self.inner
            .agent_turn_checkpoint(turn_id, message.clone(), tool_names.clone())
            .await;
        crate::turn_worker_notify::publish_worker_synthesis_to_parent_turn(
            &self.primary,
            &message,
            &tool_names,
        )
        .await;
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

    async fn agent_error(&self, turn_id: u64, message: String) {
        self.inner.agent_error(turn_id, message).await;
    }

    async fn model_receipt(&self, turn_id: u64, provider: String, model: String) {
        self.inner.model_receipt(turn_id, provider, model).await;
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

    async fn stage_persist_scratch(&self, scratch: Value) {
        self.inner.stage_persist_scratch(scratch).await;
    }
}

impl HostResumeSink {
    fn capture_delivery(&self, text: &str) {
        let mut captured = self.captured.lock().expect("host resume capture");
        *captured = Some(text.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_runtime::turn_worker::store::TurnWorkRecord;
    use chrono::Utc;

    fn record(
        work_id: &str,
        status: TurnWorkStatus,
        result: Option<&str>,
        error: Option<&str>,
    ) -> TurnWorkRecord {
        TurnWorkRecord {
            work_id: work_id.to_string(),
            session_id: "sess".to_string(),
            identity_user_id: None,
            parent_turn_correlation_id: None,
            parent_stream_turn_id: 4,
            parent_runtime_id: "runtime-test".to_string(),
            execution_placement: Default::default(),
            task_execution_grant: None,
            worker_spawn_spec: None,
            intent: "research".to_string(),
            task_prompt: format!("task for {work_id}"),
            status,
            result_text: result.map(str::to_string),
            tool_names: vec!["cognition_web_search".to_string()],
            termination_reason: None,
            error: error.map(str::to_string),
            user_ack: "On it".to_string(),
            provider: "openai".to_string(),
            model: "gpt".to_string(),
            response_depth_mode: "normal".to_string(),
            max_tool_rounds: 8,
            delivery_target: None,
            parent_user_prompt: Some("What did the sources say?".to_string()),
            parent_agent_mode: Some("general".to_string()),
            parent_code_work_id: None,
            handoff_capsule: None,
            worker_scratch: None,
            synthesis_delivered: false,
            stasis_job_id: None,
            thread_id: None,
            stage_role: None,
            model_hint: None,
            manuscript_id: None,
            branch_group_id: None,
            archived: false,
            disposition: TurnWorkDisposition::Parallel,
            steer_messages: Vec::new(),
            supports_ui_artifacts: false,
            supports_liquid_markdown: false,
            supports_browser_host: false,
            live_tool_activity: Vec::new(),
            live_thinking: String::new(),
            live_output: String::new(),
            thinking_started_at: None,
            thinking_finished_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn host_resume_prompt_includes_all_results_and_the_original_question() {
        let prompt = host_resume_prompt(&[
            record(
                "work-a",
                TurnWorkStatus::Completed,
                Some("Source A says 12."),
                None,
            ),
            record(
                "work-b",
                TurnWorkStatus::Failed,
                None,
                Some("fetch timeout"),
            ),
        ]);
        assert!(prompt.contains("[MEDOUSA_WORKER_RESULTS]"));
        assert!(prompt.contains("What did the sources say?"));
        assert!(prompt.contains("work-a"));
        assert!(prompt.contains("Source A says 12."));
        assert!(prompt.contains("work-b"));
        assert!(prompt.contains("fetch timeout"));
        assert!(prompt.contains("source=parallel_workers"));
        assert!(!prompt.contains("Prose is delivered immediately"));
    }

    #[test]
    fn fallback_concatenates_completed_and_failed_workers() {
        let text = fallback_host_resume_text(&[
            record("work-a", TurnWorkStatus::Completed, Some("ok result"), None),
            record("work-b", TurnWorkStatus::Failed, None, Some("boom")),
        ]);
        assert!(text.contains("ok result"));
        assert!(text.contains("boom"));
    }

    #[test]
    fn busy_non_handoff_turn_defers_host_resume() {
        assert!(interactive_turn_blocks_host_resume(true, false));
        assert!(!interactive_turn_blocks_host_resume(true, true));
        assert!(!interactive_turn_blocks_host_resume(false, false));
    }
}
