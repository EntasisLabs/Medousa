//! Worker execution and host synthesis (Phase 1).

use std::sync::Arc;

use chrono::Utc;
use serde_json::{Value, json};
use stasis::application::orchestration::tool_loop_pipeline::ToolLoopExecutionRequest;
use stasis::application::orchestration::prompt_pipeline::PromptExecutionContext;
use tokio::sync::RwLock;

use crate::agent_runtime::stream_sink::SharedAgentStreamSink;
use crate::agent_runtime::turn_completion::ToolLoopCompletionGate;
use crate::agent_runtime::turn_ledger::{TurnLedgerEventKind, TurnLedgerRecord, persist_ledger_record};
use crate::agent_runtime::turn_services;
use crate::agent_runtime::{
    MAX_REQUEST_PROMPT_CHARS, prompt_prep::truncate_text_for_budget,
    settings::runtime_settings_for_interactive_turn,
};
use crate::daemon_api::InteractiveTurnRequest;
use crate::stage_routing::StageRoutingMatrix;
use crate::agent_runtime::system_prompt::DEFAULT_SYSTEM_PROMPT;
use crate::tui::settings::RuntimeSettings;
use stasis::application::orchestration::prompt_pipeline::{
    PromptExecutionPipeline, PromptExecutionRequest,
};
use stasis::application::orchestration::tool_registry::ToolRegistry;
use stasis::infrastructure::llm::genai_chat_client::GenaiChatClient;
use stasis::ports::outbound::ai_chat_client::AiChatClient;

use super::policy::{TurnWorkerIntent, allowed_tool_names_for_intent, max_worker_tool_rounds};
use super::prompts::{synthesis_user_prompt, worker_failure_user_prompt, worker_system_prompt};
use super::registry::{AllowlistToolRegistry, WorkerSessionToolRegistry};
use super::store::{TurnWorkRecord, TurnWorkStatus, TurnWorkerStore};

/// Live host-turn context used when spawning a worker from the tool loop.
#[derive(Clone)]
pub struct ActiveWorkerBusSession {
    pub sink: SharedAgentStreamSink,
    pub stream_turn_id: u64,
    pub session_id: String,
    pub backend: String,
    pub parent_user_prompt: String,
    pub provider: String,
    pub model: String,
    pub response_depth_mode: String,
    pub parent_turn_correlation_id: Option<String>,
    pub delivery_target: Option<crate::turn_continuation::StoredDeliveryTarget>,
}

/// Tooling snapshot for background workers (no `Arc<TuiRuntime>` required).
#[derive(Clone)]
pub struct WorkerRuntimeContext {
    pub tool_registry: Arc<dyn ToolRegistry>,
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
}

impl WorkerRuntimeContext {
    pub fn from_tui_runtime(rt: &crate::tools::TuiRuntime) -> Self {
        let provider = crate::resolve_llm_provider(None);
        let model = crate::resolve_llm_model(None);
        let base_url = crate::resolve_llm_base_url(Some(&provider), None);
        Self {
            tool_registry: rt.tool_registry.clone(),
            provider,
            model,
            base_url,
        }
    }
}

pub struct TurnWorkerScheduler {
    store: Arc<TurnWorkerStore>,
    runtime_ctx: RwLock<Option<WorkerRuntimeContext>>,
    bus_session: RwLock<Option<ActiveWorkerBusSession>>,
}

impl TurnWorkerScheduler {
    pub fn new(store: Arc<TurnWorkerStore>) -> Self {
        Self {
            store,
            runtime_ctx: RwLock::new(None),
            bus_session: RwLock::new(None),
        }
    }

    pub async fn set_runtime_context(&self, ctx: WorkerRuntimeContext) {
        *self.runtime_ctx.write().await = Some(ctx);
    }

    pub async fn attach_runtime(&self, runtime: Arc<crate::tools::TuiRuntime>) {
        self.set_runtime_context(WorkerRuntimeContext::from_tui_runtime(runtime.as_ref()))
            .await;
    }

    pub async fn set_bus_session(&self, session: ActiveWorkerBusSession) {
        *self.bus_session.write().await = Some(session);
    }

    pub async fn clear_bus_session(&self) {
        *self.bus_session.write().await = None;
    }

    pub fn store(&self) -> Arc<TurnWorkerStore> {
        self.store.clone()
    }

    pub async fn spawn_worker(
        &self,
        intent: TurnWorkerIntent,
        task: &str,
        user_ack: &str,
        parent_user_prompt: Option<&str>,
    ) -> stasis::prelude::Result<Value> {
        let bus = self
            .bus_session
            .read()
            .await
            .clone()
            .ok_or_else(|| {
                stasis::domain::errors::StasisError::PortFailure(
                    "cognition_spawn_turn_worker: no active host turn session".to_string(),
                )
            })?;

        let _runtime_ctx = self.runtime_ctx.read().await.clone().ok_or_else(|| {
            stasis::domain::errors::StasisError::PortFailure(
                "cognition_spawn_turn_worker: agent runtime context not ready (start a turn first)"
                    .to_string(),
            )
        })?;

        let parent_turn_correlation_id = bus.parent_turn_correlation_id.clone();
        let delivery_target = bus.delivery_target.clone();

        let work_id = format!("work-{}", uuid::Uuid::new_v4());
        let now = Utc::now();
        let record = TurnWorkRecord {
            work_id: work_id.clone(),
            session_id: bus.session_id.clone(),
            parent_turn_correlation_id,
            intent: intent.as_str().to_string(),
            task_prompt: task.trim().to_string(),
            status: TurnWorkStatus::Pending,
            result_text: None,
            tool_names: Vec::new(),
            termination_reason: None,
            error: None,
            user_ack: user_ack.trim().to_string(),
            provider: bus.provider.clone(),
            model: bus.model.clone(),
            response_depth_mode: bus.response_depth_mode.clone(),
            delivery_target,
            parent_user_prompt: parent_user_prompt
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .or_else(|| Some(bus.parent_user_prompt.clone())),
            created_at: now,
            updated_at: now,
        };

        self.store.insert(record);
        ledger_bus_event(
            &bus.session_id,
            bus.stream_turn_id,
            TurnLedgerEventKind::WorkDelegated,
            format!("work_id={work_id} intent={}", intent.as_str()),
        );

        bus.sink
            .notice(format!(
                "◈ work_delegated work_id={work_id} intent={}",
                intent.as_str()
            ))
            .await;

        let store = self.store.clone();
        let work_id_spawn = work_id.clone();
        let sink = bus.sink.clone();
        let stream_turn_id = bus.stream_turn_id;
        let ctx = self.runtime_ctx.read().await.clone().expect("runtime ctx");
        tokio::spawn(async move {
            run_worker_turn(store, ctx, work_id_spawn, sink, stream_turn_id).await;
        });

        Ok(json!({
            "ok": true,
            "worker_spawned": true,
            "work_id": work_id,
            "intent": intent.as_str(),
            "status": "pending",
            "user_ack": user_ack,
            "message": "Worker started in background; host turn may end with user_ack.",
        }))
    }

}

impl Clone for TurnWorkerScheduler {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            runtime_ctx: RwLock::new(None),
            bus_session: RwLock::new(None),
        }
    }
}

fn ledger_bus_event(session_id: &str, stream_turn_id: u64, kind: TurnLedgerEventKind, detail: String) {
    let record = TurnLedgerRecord {
        timestamp: Utc::now(),
        stream_turn_id,
        kind,
        detail,
        tools_invoked: Vec::new(),
        missing_tools: Vec::new(),
        rounds_executed: 0,
    };
    persist_ledger_record(Some(session_id), &record);
}

pub async fn run_worker_turn(
    store: Arc<TurnWorkerStore>,
    ctx: WorkerRuntimeContext,
    work_id: String,
    sink: SharedAgentStreamSink,
    stream_turn_id: u64,
) {
    let Some(record) = store.get(&work_id) else {
        return;
    };

    store.update(&work_id, |r| {
        r.status = TurnWorkStatus::Running;
    });
    sink.notice(format!("◈ work_running work_id={work_id}"))
        .await;

    let intent = TurnWorkerIntent::parse(&record.intent).unwrap_or(TurnWorkerIntent::General);
    let allowlist = allowed_tool_names_for_intent(intent);
    let session_registry = Arc::new(WorkerSessionToolRegistry::new(
        ctx.tool_registry.clone(),
        record.session_id.clone(),
    ));
    let filtered_registry = Arc::new(AllowlistToolRegistry::new(session_registry, allowlist));
    let worker_pipeline = crate::tui::runtime_services::build_tool_loop_pipeline_for_target(
        &record.provider,
        &record.model,
        ctx.base_url.as_deref(),
        filtered_registry,
    );

    let settings = worker_settings_from_record(&record);
    let worker_rounds = TurnWorkerIntent::parse(&record.intent)
        .map(max_worker_tool_rounds)
        .unwrap_or(10);
    let activation = turn_services::decide_turn_activation(
        &record.task_prompt,
        turn_services::parse_tool_call_mode(&settings.tool_call_mode),
        worker_rounds,
        0,
        256,
        32,
        256,
    );

    let request = ToolLoopExecutionRequest {
        user_prompt: record.task_prompt.clone(),
        system_prompt: Some(worker_system_prompt(&record.session_id)),
        context: PromptExecutionContext::default(),
        tool_name: String::new(),
        tool_input: Value::Null,
        tool_call_mode: activation.tool_call_mode,
    };

    let mut completion_gate = ToolLoopCompletionGate {
        stream_turn_id,
        session_id: Some(record.session_id.clone()),
        sink: Some(sink.clone()),
        orchestration: None,
        budget: None,
    };

    let worker_turn_id = stream_turn_id.wrapping_add(10_000);
    let result = worker_pipeline
        .execute_with_stream_prior_messages_max_rounds(
            request,
            Vec::new(),
            None,
            activation.max_tool_rounds,
            Some(&mut completion_gate),
        )
        .await;

    match result {
        Ok(response) => {
            let tool_names: Vec<String> = response
                .tool_invocations
                .iter()
                .map(|i| i.tool_name.clone())
                .collect();
            store.update(&work_id, |r| {
                r.status = TurnWorkStatus::Completed;
                r.result_text = Some(response.text.clone());
                r.tool_names = tool_names;
                r.termination_reason = Some(response.termination_reason.clone());
            });
            ledger_bus_event(
                &record.session_id,
                stream_turn_id,
                TurnLedgerEventKind::WorkCompleted,
                format!("work_id={work_id}"),
            );
            sink.notice(format!("◈ work_completed work_id={work_id}"))
                .await;
            if let Some(updated) = store.get(&work_id) {
                run_synthesis_turn(&ctx, updated, sink, stream_turn_id).await;
            }
        }
        Err(err) => {
            let message = err.to_string();
            store.update(&work_id, |r| {
                r.status = TurnWorkStatus::Failed;
                r.error = Some(message.clone());
            });
            ledger_bus_event(
                &record.session_id,
                stream_turn_id,
                TurnLedgerEventKind::WorkFailed,
                format!("work_id={work_id} error={message}"),
            );
            sink.notice(format!("◈ work_failed work_id={work_id} error={message}"))
                .await;
            if let Some(failed) = store.get(&work_id) {
                run_worker_failure_notify(&ctx, failed, sink, stream_turn_id).await;
            }
        }
    }
}

async fn run_worker_failure_notify(
    ctx: &WorkerRuntimeContext,
    record: TurnWorkRecord,
    sink: SharedAgentStreamSink,
    notify_turn_id: u64,
) {
    let parent_prompt = record
        .parent_user_prompt
        .clone()
        .unwrap_or_else(|| record.task_prompt.clone());
    let error = record
        .error
        .clone()
        .unwrap_or_else(|| "unknown worker error".to_string());

    sink.notice(format!(
        "◈ work_failure_notify work_id={} delivering user-visible error",
        record.work_id
    ))
    .await;

    let prompt = worker_failure_user_prompt(
        &parent_prompt,
        &record.work_id,
        &record.intent,
        &error,
    );

    let resolved_provider = crate::resolve_llm_provider(Some(record.provider.as_str()));
    let resolved_model = crate::resolve_llm_model(Some(record.model.as_str()));
    let resolved_base_url =
        crate::resolve_llm_base_url(Some(&resolved_provider), ctx.base_url.as_deref());
    let chat_client: Arc<dyn AiChatClient> = Arc::new(
        GenaiChatClient::from_provider_model_with_base_url(
            Some(&resolved_provider),
            &resolved_model,
            resolved_base_url.as_deref(),
        ),
    );
    let pipeline = PromptExecutionPipeline::new(chat_client);
    let request = PromptExecutionRequest::from_user_prompt(truncate_text_for_budget(
        &prompt,
        MAX_REQUEST_PROMPT_CHARS,
    ))
    .with_context(PromptExecutionContext::default())
    .with_system_prompt(DEFAULT_SYSTEM_PROMPT.to_string());

    let text = match pipeline.execute(request).await {
        Ok(response) => response.text,
        Err(err) => format!(
            "The background task didn't finish (notify error: {err}). Worker error: {}",
            truncate_text_for_budget(&error, 400)
        ),
    };

    crate::session::append_turn(
        &record.session_id,
        &crate::session::ConversationTurn {
            role: "assistant".to_string(),
            content: text.clone(),
            timestamp: chrono::Utc::now(),
            tool_names: vec!["turn_worker.failure".to_string()],
            answer_state: None,
        },
    );
    sink.agent_response(notify_turn_id, text, vec!["turn_worker.failure".to_string()])
        .await;
}

async fn run_synthesis_turn(
    ctx: &WorkerRuntimeContext,
    record: TurnWorkRecord,
    sink: SharedAgentStreamSink,
    synthesis_turn_id: u64,
) {
    let parent_prompt = record
        .parent_user_prompt
        .clone()
        .unwrap_or_else(|| record.task_prompt.clone());
    let worker_result = record
        .result_text
        .clone()
        .unwrap_or_else(|| "(worker produced no text)".to_string());

    let synthesis_prompt = synthesis_user_prompt(
        &parent_prompt,
        &record.task_prompt,
        &record.work_id,
        &record.intent,
        &worker_result,
        &record.tool_names,
    );

    sink.notice(format!(
        "◈ work_synthesis work_id={} delivering final answer",
        record.work_id
    ))
    .await;

    let resolved_provider = crate::resolve_llm_provider(Some(record.provider.as_str()));
    let resolved_model = crate::resolve_llm_model(Some(record.model.as_str()));
    let resolved_base_url =
        crate::resolve_llm_base_url(Some(&resolved_provider), ctx.base_url.as_deref());
    let chat_client: Arc<dyn AiChatClient> = Arc::new(
        GenaiChatClient::from_provider_model_with_base_url(
            Some(&resolved_provider),
            &resolved_model,
            resolved_base_url.as_deref(),
        ),
    );
    let pipeline = PromptExecutionPipeline::new(chat_client);
    let mut request =
        PromptExecutionRequest::from_user_prompt(truncate_text_for_budget(
            &synthesis_prompt,
            MAX_REQUEST_PROMPT_CHARS,
        ))
        .with_context(PromptExecutionContext::default());
    request = request.with_system_prompt(DEFAULT_SYSTEM_PROMPT.to_string());
    let response = match pipeline.execute(request).await {
        Ok(response) => response,
        Err(err) => {
            sink.agent_error(
                synthesis_turn_id,
                format!("Worker synthesis failed: {err}"),
            )
            .await;
            return;
        }
    };

    let tool_names = record.tool_names.clone();
    let text = response.text.clone();
    crate::session::append_turn(
        &record.session_id,
        &crate::session::ConversationTurn {
            role: "assistant".to_string(),
            content: text.clone(),
            timestamp: chrono::Utc::now(),
            tool_names: tool_names.clone(),
            answer_state: None,
        },
    );
    sink.agent_response(synthesis_turn_id, text, tool_names).await;
}

fn worker_settings_from_record(record: &TurnWorkRecord) -> RuntimeSettings {
    let request = InteractiveTurnRequest {
        session_id: record.session_id.clone(),
        prompt: record.task_prompt.clone(),
        persist_user_turn: false,
        response_depth_mode: record.response_depth_mode.clone(),
        provider: record.provider.clone(),
        model: record.model.clone(),
        stage_routing: StageRoutingMatrix::default_for(&record.provider, &record.model),
    };
    let mut settings = runtime_settings_for_interactive_turn("worker", &request);
    settings.max_tool_rounds = "12".to_string();
    settings
}

/// Prefer [`super::routing::resolve_host_turn_profile`] for Phase 2 auto routing.
pub fn host_bus_mode_enabled() -> bool {
    super::routing::host_bus_force_enabled()
}

pub fn pipeline_for_turn_profile(
    tool_registry: Arc<dyn ToolRegistry>,
    provider: &str,
    model: &str,
    base_url: Option<&str>,
    host_bus: bool,
) -> crate::medousa_tool_loop::MedousaToolLoopPipeline {
    if host_bus {
        let allowlist = super::policy::host_bus_tool_names();
        let filtered = Arc::new(AllowlistToolRegistry::new(tool_registry, allowlist));
        crate::tui::runtime_services::build_tool_loop_pipeline_for_target(
            provider, model, base_url, filtered,
        )
    } else {
        crate::tui::runtime_services::build_tool_loop_pipeline_for_target(
            provider, model, base_url, tool_registry,
        )
    }
}

pub fn system_prompt_for_host_bus(base: &str, host_bus: bool) -> String {
    super::prompts::system_prompt_for_host_profile(base, host_bus, None)
}
