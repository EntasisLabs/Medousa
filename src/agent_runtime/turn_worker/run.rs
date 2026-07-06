//! Worker execution and host synthesis (Phase 1).

use std::sync::Arc;

use chrono::Utc;
use serde_json::{Value, json};
use stasis::application::orchestration::tool_loop_pipeline::ToolLoopExecutionRequest;
use stasis::application::orchestration::prompt_pipeline::PromptExecutionContext;
use tokio::sync::RwLock;

use crate::agent_runtime::stream_sink::SharedAgentStreamSink;
use crate::agent_runtime::turn_completion::ToolLoopCompletionGate;
use crate::agent_runtime::turn_ledger::append_tool_loop_policy;
use crate::agent_runtime::turn_loop_settings::TurnLoopSettings;
use crate::agent_runtime::turn_ledger::{TurnLedgerEventKind, TurnLedgerRecord, persist_ledger_record};
use crate::agent_runtime::turn_services;
use crate::agent_runtime::{
    MAX_REQUEST_PROMPT_CHARS, prompt_prep::truncate_text_for_budget,
    settings::runtime_settings_for_interactive_turn,
};
use crate::daemon_api::InteractiveTurnRequest;
use crate::stage_routing::StageRoutingMatrix;
use crate::agent_runtime::system_prompt::DEFAULT_SYSTEM_PROMPT;
use crate::channel_delivery::ChannelDeliveryTarget;
use crate::turn_continuation::TurnContinuationScope;
use crate::tui::settings::RuntimeSettings;
use stasis::application::orchestration::prompt_pipeline::{
    PromptExecutionPipeline, PromptExecutionRequest,
};
use stasis::application::orchestration::tool_registry::ToolRegistry;
use stasis::infrastructure::llm::genai_chat_client::GenaiChatClient;
use stasis::ports::outbound::ai_chat_client::AiChatClient;

use stasis::prelude::RuntimeComposition;

use super::model_routing::resolve_worker_llm_target;
use super::policy::{TurnWorkerIntent, max_worker_tool_rounds};
use crate::agent_runtime::turn_context::WorkerHandoffCapsule;
use crate::agent_runtime::worker_continuity::{
    InProcessDelegationRecord, record_in_process_delegation,
};

use super::prompts::{
    synthesis_user_prompt, synthesis_user_prompt_with_handoff, worker_failure_user_prompt,
    worker_system_prompt,
};
use super::registry::{AllowlistToolRegistry, SessionBootstrapToolRegistry, WorkerSessionToolRegistry};
use crate::tool_bootstrap::{ToolSurfaceLane, handoff_implies_resolved_execution, unlock_session_domains, worker_should_unlock_vault};
use super::store::{
    TurnWorkDisposition, TurnWorkRecord, TurnWorkStatus, TurnWorkerStore, turn_worker_store,
};

fn worker_canvas_lane_enabled(is_bound_workshop: bool, record: &TurnWorkRecord) -> bool {
    is_bound_workshop || record.supports_ui_artifacts
}

async fn establish_worker_canvas_turn_scope(
    turn_scope: &Arc<RwLock<Option<TurnContinuationScope>>>,
    record: &TurnWorkRecord,
) {
    let previous = turn_scope.read().await.clone();
    let mut next = previous.unwrap_or_else(|| TurnContinuationScope {
        turn_correlation_id: record
            .parent_turn_correlation_id
            .clone()
            .unwrap_or_else(|| format!("work-{}", record.work_id)),
        session_id: record.session_id.clone(),
        original_prompt: record.task_prompt.clone(),
        delivery_target: record.delivery_target.as_ref().map(ChannelDeliveryTarget::from),
        provider: record.provider.clone(),
        model: record.model.clone(),
        response_depth_mode: record.response_depth_mode.clone(),
        supports_ui_artifacts: true,
        supports_browser_host: record.supports_browser_host,
        channel_surface: Some("workshop-canvas".to_string()),
    });
    next.supports_ui_artifacts = true;
    if record.supports_browser_host {
        next.supports_browser_host = true;
    }
    next.session_id = record.session_id.clone();
    *turn_scope.write().await = Some(next);
}

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
    pub host_handoff_slot: Arc<tokio::sync::RwLock<Option<WorkerHandoffCapsule>>>,
    pub host_continuity_bundle: Option<crate::agent_runtime::worker_continuity::HostContinuityBundle>,
    /// Operator `max_tool_rounds` from the delegating host turn (not a separate worker cap).
    pub configured_max_tool_rounds: usize,
    /// Home client advertised HTML/canvas support when the host delegated this work.
    pub supports_ui_artifacts: bool,
    pub supports_browser_host: bool,
}

/// Tooling snapshot for background workers (no full `Arc<TuiRuntime>` required).
#[derive(Clone)]
pub struct WorkerRuntimeContext {
    pub tool_registry: Arc<dyn ToolRegistry>,
    pub identity_memory_store: Option<Arc<dyn stasis::ports::outbound::memory::identity_memory_store::IdentityMemoryStore>>,
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
    pub turn_scope: Arc<RwLock<Option<crate::turn_continuation::TurnContinuationScope>>>,
}

impl WorkerRuntimeContext {
    pub fn from_tui_runtime(rt: &crate::tools::TuiRuntime) -> Self {
        let provider = crate::resolve_llm_provider(None);
        let model = crate::resolve_llm_model(None);
        let base_url = crate::resolve_llm_base_url(Some(&provider), None);
        Self {
            tool_registry: rt.tool_registry.clone(),
            identity_memory_store: Some(rt.identity_memory_store.clone()),
            provider,
            model,
            base_url,
            turn_scope: rt.turn_scope.clone(),
        }
    }
}

pub struct TurnWorkerScheduler {
    store: Arc<TurnWorkerStore>,
    runtime_ctx: RwLock<Option<WorkerRuntimeContext>>,
    runtime: RwLock<Option<Arc<RuntimeComposition>>>,
    bus_session: RwLock<Option<ActiveWorkerBusSession>>,
}

impl TurnWorkerScheduler {
    pub fn new(store: Arc<TurnWorkerStore>) -> Self {
        Self {
            store,
            runtime_ctx: RwLock::new(None),
            runtime: RwLock::new(None),
            bus_session: RwLock::new(None),
        }
    }

    pub async fn set_runtime_context(&self, ctx: WorkerRuntimeContext) {
        *self.runtime_ctx.write().await = Some(ctx);
    }

    pub async fn attach_runtime(&self, runtime: Arc<crate::tools::TuiRuntime>) {
        self.set_runtime_context(WorkerRuntimeContext::from_tui_runtime(runtime.as_ref()))
            .await;
        *self.runtime.write().await = Some(runtime.runtime.clone());
    }

    pub async fn set_bus_session(&self, session: ActiveWorkerBusSession) {
        *self.bus_session.write().await = Some(session);
    }

    pub async fn clear_bus_session(&self) {
        *self.bus_session.write().await = None;
    }

    pub async fn active_bus_session_id(&self) -> Option<String> {
        self.bus_session
            .read()
            .await
            .as_ref()
            .map(|session| session.session_id.clone())
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
        manuscript: Option<crate::identity_manuscript::ManuscriptContext>,
        stage_role: Option<&str>,
        model_hint: Option<&str>,
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

        let runtime_ctx = self.runtime_ctx.read().await.clone().ok_or_else(|| {
            stasis::domain::errors::StasisError::PortFailure(
                "cognition_spawn_turn_worker: agent runtime context not ready (start a turn first)"
                    .to_string(),
            )
        })?;

        let parent_turn_correlation_id = bus.parent_turn_correlation_id.clone();
        let delivery_target = bus.delivery_target.clone();

        let work_id = format!("work-{}", uuid::Uuid::new_v4());
        let now = Utc::now();
        let mut handoff = bus
            .host_handoff_slot
            .write()
            .await
            .take()
            .unwrap_or_else(|| {
            WorkerHandoffCapsule::from_host_context(
                &bus.session_id,
                bus.stream_turn_id,
                parent_turn_correlation_id.clone(),
                parent_user_prompt
                    .filter(|s| !s.is_empty())
                    .unwrap_or(bus.parent_user_prompt.as_str()),
                &crate::agent_runtime::turn_context::TurnScratchpad::from_user_prompt(task),
                None,
                None,
                bus.host_continuity_bundle.clone(),
            )
            });
        if handoff.host_continuity.is_none() {
            handoff.host_continuity = bus.host_continuity_bundle.clone();
        }
        if let Some(ref manuscript_ctx) = manuscript {
            handoff.manuscript = Some(manuscript_ctx.into());
            if let Some(bundle) = handoff.host_continuity.as_mut() {
                if let Some(store) = runtime_ctx.identity_memory_store.as_ref() {
                    bundle.identity_summary = Some(
                        crate::identity_manuscript::compile_manuscript_identity_summary(
                            store,
                            manuscript_ctx,
                            Some(task),
                        )
                        .await,
                    );
                }
            }
        }
        handoff.apply_spawn(intent.as_str(), task, &work_id);
        crate::turn_slice::enrich_handoff_tool_history(
            &mut handoff,
            &crate::session::load_history(&bus.session_id),
        );
        let handoff_summary = handoff.handoff_summary();
        let scratch_digest = handoff.scratch_digest_hash.clone();
        let parent_corr_log = handoff
            .parent_turn_correlation_id
            .clone()
            .unwrap_or_else(|| "-".to_string());
        let continuity_summary = handoff
            .host_continuity
            .as_ref()
            .map(|bundle| bundle.log_summary())
            .unwrap_or_else(|| "none".to_string());
        let delegation_parent_turn = handoff.parent_turn_correlation_id.clone();

        let max_tool_rounds = manuscript
            .as_ref()
            .and_then(|ctx| ctx.max_tool_rounds)
            .map(|rounds| rounds.max(1))
            .unwrap_or_else(|| bus.configured_max_tool_rounds.max(1));

        let manuscript_stage_role = manuscript
            .as_ref()
            .and_then(|ctx| ctx.worker_stage_role.as_deref());
        let manuscript_model_hint = manuscript
            .as_ref()
            .and_then(|ctx| ctx.worker_model_hint.as_deref());
        let resolved_stage_role = stage_role
            .or(manuscript_stage_role)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let resolved_model_hint = model_hint
            .or(manuscript_model_hint)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let (provider, model) = resolve_worker_llm_target(
            &bus.provider,
            &bus.model,
            intent,
            resolved_stage_role.as_deref(),
            resolved_model_hint.as_deref(),
        );
        let manuscript_id = manuscript.as_ref().map(|ctx| ctx.id.clone());

        let record = TurnWorkRecord {
            work_id: work_id.clone(),
            session_id: bus.session_id.clone(),
            parent_turn_correlation_id,
            parent_stream_turn_id: bus.stream_turn_id,
            intent: intent.as_str().to_string(),
            task_prompt: task.trim().to_string(),
            status: TurnWorkStatus::Pending,
            result_text: None,
            tool_names: Vec::new(),
            termination_reason: None,
            error: None,
            user_ack: user_ack.trim().to_string(),
            provider,
            model,
            response_depth_mode: bus.response_depth_mode.clone(),
            max_tool_rounds,
            delivery_target,
            parent_user_prompt: parent_user_prompt
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .or_else(|| Some(bus.parent_user_prompt.clone())),
            handoff_capsule: Some(handoff),
            worker_scratch: None,
            synthesis_delivered: false,
            stasis_job_id: None,
            thread_id: None,
            stage_role: resolved_stage_role.clone(),
            model_hint: resolved_model_hint,
            manuscript_id: manuscript_id.clone(),
            branch_group_id: None,
            archived: false,
            disposition: TurnWorkDisposition::Parallel,
            steer_messages: Vec::new(),
            supports_ui_artifacts: bus.supports_ui_artifacts,
            supports_browser_host: bus.supports_browser_host,
            created_at: now,
            updated_at: now,
        };

        self.store.insert(record);
        ledger_bus_event(
            &bus.session_id,
            bus.stream_turn_id,
            TurnLedgerEventKind::WorkDelegated,
            format!(
                "work_id={work_id} intent={intent} parent_turn_correlation_id={parent_corr_log} scratch_digest={scratch_digest}",
                intent = intent.as_str(),
            ),
        );

        record_in_process_delegation(&InProcessDelegationRecord {
            work_id: work_id.clone(),
            session_id: bus.session_id.clone(),
            intent: intent.as_str().to_string(),
            parent_turn_correlation_id: delegation_parent_turn,
            parent_stream_turn_id: bus.stream_turn_id,
            sequential: true,
            continuity_summary: continuity_summary.clone(),
            manuscript_id: manuscript_id.clone(),
            spawned_at: now,
        });

        bus.sink
            .notice(format!(
                "◈ work_delegated work_id={work_id} intent={} continuity={continuity_summary}",
                intent.as_str()
            ))
            .await;
        bus.sink
            .notice(format!(
                "◈ worker_delegation work_id={work_id} intent={intent} sequential=true continuity={continuity_summary}",
                intent = intent.as_str(),
            ))
            .await;
        if let Some(manuscript_id) = manuscript_id.as_deref() {
            bus.sink
                .notice(format!(
                    "◈ worker_manuscript work_id={work_id} id={manuscript_id} intent={}",
                    intent.as_str()
                ))
                .await;
        }

        let runtime = self.runtime.read().await.clone().ok_or_else(|| {
            stasis::domain::errors::StasisError::PortFailure(
                "cognition_spawn_turn_worker: stasis runtime not ready".to_string(),
            )
        })?;
        crate::agent_runtime::turn_worker_job::enqueue_turn_worker_job(
            runtime.as_ref(),
            &work_id,
            bus.stream_turn_id,
        )
        .await?;

        Ok(json!({
            "ok": true,
            "worker_spawned": true,
            "work_id": work_id,
            "stasis_job_id": work_id,
            "intent": intent.as_str(),
            "manuscript_id": manuscript_id,
            "stage_role": record_stage_role_for_response(resolved_stage_role.as_deref()),
            "status": "pending",
            "user_ack": user_ack,
            "handoff_summary": handoff_summary,
            "scratch_digest": scratch_digest,
            "message": "Worker enqueued on durable bus; host turn may end with user_ack.",
        }))
    }

    pub async fn enter_bound_workshop(
        &self,
        message: &str,
        goal: &str,
        intent: TurnWorkerIntent,
    ) -> stasis::prelude::Result<Value> {
        let bus = self
            .bus_session
            .read()
            .await
            .clone()
            .ok_or_else(|| {
                stasis::domain::errors::StasisError::PortFailure(
                    "cognition_turn_begin_work: no active host turn session".to_string(),
                )
            })?;

        if self
            .store
            .active_bound_workshop(&bus.session_id)
            .is_some()
        {
            return Ok(json!({
                "ok": false,
                "workshop_entered": false,
                "error": "A bound workshop is already active for this session; steer or cancel it first.",
            }));
        }

        let runtime_ctx = self.runtime_ctx.read().await.clone().ok_or_else(|| {
            stasis::domain::errors::StasisError::PortFailure(
                "cognition_turn_begin_work: agent runtime context not ready (start a turn first)"
                    .to_string(),
            )
        })?;

        let task = goal.trim();
        if task.is_empty() {
            return Ok(json!({
                "ok": false,
                "workshop_entered": false,
                "error": "goal is required and must be non-empty",
            }));
        }

        let user_ack = message.trim();
        if user_ack.is_empty() {
            return Ok(json!({
                "ok": false,
                "workshop_entered": false,
                "error": "message is required and must be non-empty",
            }));
        }

        let parent_turn_correlation_id = bus.parent_turn_correlation_id.clone();
        let delivery_target = bus.delivery_target.clone();
        let work_id = format!("work-bound-{}", uuid::Uuid::new_v4());
        let now = Utc::now();
        let mut handoff = bus
            .host_handoff_slot
            .write()
            .await
            .take()
            .unwrap_or_else(|| {
                WorkerHandoffCapsule::from_host_context(
                    &bus.session_id,
                    bus.stream_turn_id,
                    parent_turn_correlation_id.clone(),
                    &bus.parent_user_prompt,
                    &crate::agent_runtime::turn_context::TurnScratchpad::from_user_prompt(task),
                    None,
                    None,
                    bus.host_continuity_bundle.clone(),
                )
            });
        if handoff.host_continuity.is_none() {
            handoff.host_continuity = bus.host_continuity_bundle.clone();
        }
        handoff.apply_spawn(intent.as_str(), task, &work_id);
        crate::turn_slice::enrich_handoff_tool_history(
            &mut handoff,
            &crate::session::load_history(&bus.session_id),
        );
        let handoff_summary = handoff.handoff_summary();
        let scratch_digest = handoff.scratch_digest_hash.clone();
        let parent_corr_log = handoff
            .parent_turn_correlation_id
            .clone()
            .unwrap_or_else(|| "-".to_string());
        let continuity_summary = handoff
            .host_continuity
            .as_ref()
            .map(|bundle| bundle.log_summary())
            .unwrap_or_else(|| "none".to_string());
        let delegation_parent_turn = handoff.parent_turn_correlation_id.clone();

        let max_tool_rounds = bus.configured_max_tool_rounds.max(1);
        let (provider, model) = resolve_worker_llm_target(
            &bus.provider,
            &bus.model,
            intent,
            None,
            None,
        );

        let record = TurnWorkRecord {
            work_id: work_id.clone(),
            session_id: bus.session_id.clone(),
            parent_turn_correlation_id,
            parent_stream_turn_id: bus.stream_turn_id,
            intent: intent.as_str().to_string(),
            task_prompt: task.to_string(),
            status: TurnWorkStatus::Pending,
            result_text: None,
            tool_names: Vec::new(),
            termination_reason: None,
            error: None,
            user_ack: user_ack.to_string(),
            provider,
            model,
            response_depth_mode: bus.response_depth_mode.clone(),
            max_tool_rounds,
            delivery_target,
            parent_user_prompt: Some(bus.parent_user_prompt.clone()),
            handoff_capsule: Some(handoff),
            worker_scratch: None,
            synthesis_delivered: false,
            stasis_job_id: None,
            thread_id: None,
            stage_role: None,
            model_hint: None,
            manuscript_id: None,
            branch_group_id: None,
            archived: false,
            disposition: TurnWorkDisposition::Bound,
            steer_messages: Vec::new(),
            supports_ui_artifacts: true,
            supports_browser_host: bus.supports_browser_host,
            created_at: now,
            updated_at: now,
        };

        self.store.insert(record);
        ledger_bus_event(
            &bus.session_id,
            bus.stream_turn_id,
            TurnLedgerEventKind::WorkDelegated,
            format!(
                "work_id={work_id} disposition=bound intent={intent} parent_turn_correlation_id={parent_corr_log} scratch_digest={scratch_digest}",
                intent = intent.as_str(),
            ),
        );

        record_in_process_delegation(&InProcessDelegationRecord {
            work_id: work_id.clone(),
            session_id: bus.session_id.clone(),
            intent: intent.as_str().to_string(),
            parent_turn_correlation_id: delegation_parent_turn,
            parent_stream_turn_id: bus.stream_turn_id,
            sequential: true,
            continuity_summary: continuity_summary.clone(),
            manuscript_id: None,
            spawned_at: now,
        });

        bus.sink
            .notice(format!(
                "◈ workshop_entered work_id={work_id} intent={} continuity={continuity_summary}",
                intent.as_str()
            ))
            .await;

        let runtime = self.runtime.read().await.clone().ok_or_else(|| {
            stasis::domain::errors::StasisError::PortFailure(
                "cognition_turn_begin_work: stasis runtime not ready".to_string(),
            )
        })?;
        crate::agent_runtime::turn_worker_job::enqueue_turn_worker_job(
            runtime.as_ref(),
            &work_id,
            bus.stream_turn_id,
        )
        .await?;

        Ok(json!({
            "ok": true,
            "workshop_entered": true,
            "work_id": work_id,
            "stasis_job_id": work_id,
            "intent": intent.as_str(),
            "status": "pending",
            "user_ack": user_ack,
            "message": user_ack,
            "handoff_summary": handoff_summary,
            "scratch_digest": scratch_digest,
        }))
    }

}

fn record_stage_role_for_response(stage_role: Option<&str>) -> Option<String> {
    stage_role.map(str::to_string)
}

impl Clone for TurnWorkerScheduler {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            runtime_ctx: RwLock::new(None),
            runtime: RwLock::new(None),
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
        scratch: None,
        active_profile_id: None,
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

    let is_bound_workshop = record.disposition == TurnWorkDisposition::Bound;
    if is_bound_workshop {
        if let Some(started) = store.get(&work_id) {
            crate::feed_adapters::publish_workshop_started(&started).await;
        }
    }

    let intent = TurnWorkerIntent::parse(&record.intent).unwrap_or(TurnWorkerIntent::General);
    let manuscript_tools = record
        .handoff_capsule
        .as_ref()
        .and_then(|capsule| capsule.manuscript.as_ref())
        .map(|manuscript| manuscript.tools_allow.as_slice())
        .unwrap_or(&[] as &[String]);
    let allowlist = super::policy::worker_allowlist_for_intent_and_tools(intent, manuscript_tools);
    if handoff_implies_resolved_execution(record.handoff_capsule.as_ref()) {
        let _ = unlock_session_domains(&record.session_id, ToolSurfaceLane::Worker, &["execute"]);
    }
    if worker_should_unlock_vault(&record.task_prompt, intent) {
        let _ = unlock_session_domains(&record.session_id, ToolSurfaceLane::Worker, &["vault"]);
    }
    if is_bound_workshop {
        crate::tool_bootstrap::ensure_bound_workshop_session_tool_defaults(&record.session_id);
    } else {
        let _ = unlock_session_domains(&record.session_id, ToolSurfaceLane::Worker, &["memory"]);
    }
    let session_registry = Arc::new(WorkerSessionToolRegistry::new(
        ctx.tool_registry.clone(),
        record.session_id.clone(),
    ));
    let canvas_lane = worker_canvas_lane_enabled(is_bound_workshop, &record);
    let filtered_registry: Arc<dyn ToolRegistry> = if canvas_lane {
        Arc::new(SessionBootstrapToolRegistry::bound_workshop(
            session_registry,
            record.session_id.clone(),
            allowlist,
            true,
            record.supports_browser_host || is_bound_workshop,
        ))
    } else {
        Arc::new(SessionBootstrapToolRegistry::worker(
            session_registry,
            record.session_id.clone(),
            allowlist,
        ))
    };
    let previous_turn_scope = if canvas_lane {
        let previous = ctx.turn_scope.read().await.clone();
        establish_worker_canvas_turn_scope(&ctx.turn_scope, &record).await;
        Some(previous)
    } else {
        None
    };
    let worker_pipeline = crate::tui::runtime_services::build_tool_loop_pipeline_for_target(
        &record.provider,
        &record.model,
        ctx.base_url.as_deref(),
        filtered_registry,
    );

    let settings = worker_settings_from_record(&record);
    let turn_loop_settings = TurnLoopSettings::from_runtime_settings(&settings);
    let intent_floor = max_worker_tool_rounds(intent);
    let worker_max_rounds = record.max_tool_rounds.max(intent_floor).max(1);
    let tool_call_mode = turn_services::parse_tool_call_mode(&settings.tool_call_mode);
    sink.notice(format!(
        "◈ work_round_budget work_id={work_id} max_tool_rounds={worker_max_rounds} host_config={} intent_floor={intent_floor}",
        record.max_tool_rounds,
    ))
    .await;
    let tool_loop_policy = append_tool_loop_policy(&record.task_prompt, worker_max_rounds);
    let initial_worker_scratch = record.handoff_capsule.as_ref().map(|c| {
        let mut scratch = c.initial_worker_scratch();
        if matches!(
            intent,
            TurnWorkerIntent::Research | TurnWorkerIntent::General
        ) {
            // Host-lane receipt gaps (e.g. calibrate) must not block workshop finalize.
            scratch.open_gaps.clear();
        }
        scratch
    });
    let user_prompt = record
        .handoff_capsule
        .as_ref()
        .map(|c| c.worker_tier_user_prompt(&tool_loop_policy))
        .unwrap_or(tool_loop_policy.clone());

    let request = ToolLoopExecutionRequest {
        user_prompt,
        system_prompt: Some(worker_system_prompt(
            &record.session_id,
            TurnWorkerIntent::parse(&record.intent).unwrap_or(TurnWorkerIntent::General),
            record
                .handoff_capsule
                .as_ref()
                .and_then(|capsule| capsule.manuscript.as_ref()),
        )),
        context: PromptExecutionContext::default(),
        tool_name: String::new(),
        tool_input: Value::Null,
        tool_call_mode,
    };

    let mut worker_scratch: Option<crate::agent_runtime::turn_context::TurnScratchpad> = None;
    let mut completion_gate = ToolLoopCompletionGate {
        stream_turn_id,
        session_id: Some(record.session_id.clone()),
        sink: Some(sink.clone()),
        orchestration: None,
        budget: None,
        max_tool_rounds: worker_max_rounds,
        max_text_only_stuck_continues: turn_loop_settings.max_text_only_stuck_continues,
        scratch_out: Some(&mut worker_scratch),
        host_handoff_slot: None,
        parent_turn_correlation_id: record.parent_turn_correlation_id.clone(),
        initial_worker_scratch,
        handoff_parent_user_prompt: record.parent_user_prompt.clone(),
        handoff_vibe_signature: record
            .handoff_capsule
            .as_ref()
            .and_then(|cap| cap.vibe_signature.clone()),
        handoff_model_avec: record
            .handoff_capsule
            .as_ref()
            .and_then(|cap| cap.model_avec.map(Into::into)),
        handoff_continuity_bundle: record
            .handoff_capsule
            .as_ref()
            .and_then(|cap| cap.host_continuity.clone()),
        skip_avec_ritual_check: matches!(
            intent,
            TurnWorkerIntent::Research | TurnWorkerIntent::General
        ),
        channel: record
            .delivery_target
            .as_ref()
            .map(|target| target.channel.clone()),
        delivery_target: record.delivery_target.clone(),
        tool_round_budget_ceiling: worker_max_rounds,
        require_operator_budget_gate: false,
        host_scheduler_lane: false,
        cancel_poll_work_id: Some(work_id.clone()),
        steer_poll_work_id: is_bound_workshop.then_some(work_id.clone()),
    };

    if store.is_work_cancelled(&work_id) {
        if let Some(restore) = previous_turn_scope {
            *ctx.turn_scope.write().await = restore;
        }
        store.update(&work_id, |r| {
            r.status = TurnWorkStatus::Cancelled;
            r.termination_reason = Some("workshop_cancelled".to_string());
        });
        sink.notice(format!("◈ work_cancelled work_id={work_id}"))
            .await;
        return;
    }

    let result = worker_pipeline
        .execute_with_stream_prior_messages_max_rounds(
            request,
            Vec::new(),
            None,
            worker_max_rounds,
            Some(&mut completion_gate),
            None,
        )
        .await;

    match result {
        Ok(response) => {
            if store.is_work_cancelled(&work_id) {
                store.update(&work_id, |r| {
                    r.status = TurnWorkStatus::Cancelled;
                    r.termination_reason = Some("workshop_cancelled".to_string());
                });
                sink.notice(format!("◈ work_cancelled work_id={work_id}"))
                    .await;
            } else {
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
                r.worker_scratch = worker_scratch.clone();
            });
            ledger_bus_event(
                &record.session_id,
                stream_turn_id,
                TurnLedgerEventKind::WorkCompleted,
                format!("work_id={work_id}"),
            );
            sink.notice(format!("◈ work_completed work_id={work_id}"))
                .await;
            if is_bound_workshop {
                if let Some(updated) = store.get(&work_id) {
                    crate::feed_adapters::publish_workshop_working(
                        &updated,
                        updated.tool_names.len() as u32,
                        &updated.tool_names,
                    )
                    .await;
                }
            }
            if let Some(updated) = store.get(&work_id) {
                run_synthesis_turn(&ctx, updated, sink, stream_turn_id).await;
            }
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
            if is_bound_workshop {
                if let Some(failed) = store.get(&work_id) {
                    crate::feed_adapters::publish_workshop_terminal(
                        &failed,
                        "failed",
                        failed.error.as_deref(),
                    )
                    .await;
                }
            }
            if let Some(failed) = store.get(&work_id) {
                run_worker_failure_notify(&ctx, failed, sink, stream_turn_id).await;
            }
        }
    }

    if let Some(restore) = previous_turn_scope {
        *ctx.turn_scope.write().await = restore;
    }
}

pub async fn resume_synthesis_if_needed(
    ctx: &WorkerRuntimeContext,
    record: TurnWorkRecord,
    sink: SharedAgentStreamSink,
) {
    if record.synthesis_delivered || record.status != TurnWorkStatus::Completed {
        return;
    }
    let stream_turn_id = record.parent_stream_turn_id;
    run_synthesis_turn(ctx, record, sink, stream_turn_id).await;
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

    sink.agent_response(notify_turn_id, text, vec!["turn_worker.failure".to_string()])
        .await;
}

async fn run_synthesis_turn(
    ctx: &WorkerRuntimeContext,
    record: TurnWorkRecord,
    sink: SharedAgentStreamSink,
    synthesis_turn_id: u64,
) {
    if worker_synthesis_pass_through(&record) {
        let text = record
            .result_text
            .clone()
            .unwrap_or_else(|| "(worker produced no text)".to_string());
        sink.notice(format!(
            "◈ work_synthesis work_id={} pass-through (worker finish)",
            record.work_id
        ))
        .await;
        deliver_synthesis_response(&record, &sink, synthesis_turn_id, text).await;
        return;
    }

    let parent_prompt = record
        .parent_user_prompt
        .clone()
        .unwrap_or_else(|| record.task_prompt.clone());
    let worker_result = record
        .result_text
        .clone()
        .unwrap_or_else(|| "(worker produced no text)".to_string());

    let worker_tools_summary = if record.tool_names.is_empty() {
        "(none)".to_string()
    } else {
        record
            .tool_names
            .iter()
            .map(|name| format!("- {name}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let synthesis_prompt = if let Some(capsule) = record.handoff_capsule.as_ref() {
        synthesis_user_prompt_with_handoff(
            capsule,
            record.worker_scratch.as_ref(),
            &worker_result,
            &record.tool_names,
            &worker_tools_summary,
        )
    } else {
        synthesis_user_prompt(
            &parent_prompt,
            &record.task_prompt,
            &record.work_id,
            &record.intent,
            &worker_result,
            &record.tool_names,
        )
    };

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
            turn_worker_store().update(&record.work_id, |worker| {
                worker.synthesis_delivered = true;
            });
            sink.agent_error(
                synthesis_turn_id,
                format!("Worker synthesis failed: {err}"),
            )
            .await;
            return;
        }
    };

    let text = response.text.clone();
    deliver_synthesis_response(&record, &sink, synthesis_turn_id, text).await;
}

/// Phase 7C / 8D.2: skip host synthesis LLM when the worker committed via `cognition_turn_finish`.
pub(crate) fn worker_synthesis_pass_through(record: &TurnWorkRecord) -> bool {
    record.termination_reason.as_deref() == Some("cognition_turn_finish")
        && record
            .result_text
            .as_ref()
            .is_some_and(|text| !text.trim().is_empty())
}

async fn deliver_synthesis_response(
    record: &TurnWorkRecord,
    sink: &SharedAgentStreamSink,
    synthesis_turn_id: u64,
    text: String,
) {
    let tool_names = record.tool_names.clone();
    // Worker synthesis must commit explicit finish prose, not stale host/worker stream draft.
    sink.reset_streamed_markdown().await;
    sink.agent_response(synthesis_turn_id, text.clone(), tool_names.clone())
        .await;
    crate::turn_worker_notify::publish_worker_synthesis_to_parent_turn(
        record,
        &text,
        &tool_names,
    )
    .await;
    turn_worker_store().update(&record.work_id, |worker| {
        worker.synthesis_delivered = true;
        worker.result_text = Some(text.clone());
    });
    if record.disposition == TurnWorkDisposition::Bound {
        crate::feed_adapters::publish_workshop_synthesis(record, &text).await;
        crate::feed_adapters::publish_workshop_terminal(record, "done", Some(&text)).await;
    }
}

fn worker_settings_from_record(record: &TurnWorkRecord) -> RuntimeSettings {
    let request = InteractiveTurnRequest {
        session_id: record.session_id.clone(),
        prompt: record.task_prompt.clone(),
        persist_user_turn: false,
        response_depth_mode: record.response_depth_mode.clone(),
        reasoning_effort: crate::reasoning_effort::REASONING_EFFORT_DEFAULT.to_string(),
        provider: record.provider.clone(),
        model: record.model.clone(),
        stage_routing: StageRoutingMatrix::default_for(&record.provider, &record.model),
        surface: None,
        max_tool_rounds: None,
        retry_runtime_max_rounds: None,
        manuscript_id: None,
        additional_manuscript_ids: None,
        suggested_capability_ids: None,
        scheduled_tool_allowlist: None,
        voice_preset_id: None,
        voice_appendix: None,
        media_refs: Vec::new(),
        identity_user_id: None,
    };
    let mut settings = runtime_settings_for_interactive_turn("worker", &request);
    settings.max_tool_rounds = record.max_tool_rounds.max(1).to_string();
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
    session_id: Option<&str>,
    supports_ui_artifacts: bool,
    supports_browser_host: bool,
) -> crate::medousa_tool_loop::MedousaToolLoopPipeline {
    if host_bus {
        let allowlist = super::policy::host_bus_tool_names();
        let filtered: Arc<dyn ToolRegistry> = if let Some(session_id) = session_id.filter(|id| !id.trim().is_empty()) {
            Arc::new(SessionBootstrapToolRegistry::host(
                tool_registry,
                session_id,
                allowlist,
                supports_ui_artifacts,
                supports_browser_host,
            ))
        } else {
            Arc::new(AllowlistToolRegistry::new(tool_registry, allowlist))
        };
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_runtime::turn_worker::store::{TurnWorkRecord, TurnWorkStatus};

    fn sample_record(termination_reason: Option<&str>, result_text: Option<&str>) -> TurnWorkRecord {
        TurnWorkRecord {
            work_id: "w1".to_string(),
            session_id: "s1".to_string(),
            parent_turn_correlation_id: None,
            parent_stream_turn_id: 0,
            intent: "general".to_string(),
            task_prompt: "task".to_string(),
            status: TurnWorkStatus::Completed,
            result_text: result_text.map(str::to_string),
            tool_names: vec!["cognition_grapheme_run".to_string()],
            termination_reason: termination_reason.map(str::to_string),
            error: None,
            user_ack: "On it".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            response_depth_mode: "normal".to_string(),
            max_tool_rounds: 8,
            delivery_target: None,
            parent_user_prompt: None,
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
            supports_browser_host: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn pass_through_when_worker_finished_with_message() {
        assert!(worker_synthesis_pass_through(&sample_record(
            Some("cognition_turn_finish"),
            Some("Here is the report.")
        )));
    }

    #[test]
    fn no_pass_through_without_finish_or_empty_result() {
        assert!(!worker_synthesis_pass_through(&sample_record(
            Some("max_rounds_fuse"),
            Some("partial")
        )));
        assert!(!worker_synthesis_pass_through(&sample_record(
            Some("cognition_turn_finish"),
            Some("   ")
        )));
        assert!(!worker_synthesis_pass_through(&sample_record(None, Some("done"))));
    }
}
