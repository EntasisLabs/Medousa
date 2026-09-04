//! Worker execution and host synthesis (Phase 1).

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex, Weak};

use chrono::Utc;
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::Value;
use stasis::application::orchestration::prompt_pipeline::PromptExecutionContext;
use stasis::application::orchestration::tool_loop_pipeline::ToolLoopExecutionRequest;
use tokio::sync::RwLock;

use crate::agent_runtime::provider_stream::{ProviderStreamBridge, fail_on_stream_overflow};
use crate::agent_runtime::stream_sink::SharedAgentStreamSink;
use crate::agent_runtime::turn_completion::ToolLoopCompletionGate;
use crate::agent_runtime::turn_ledger::append_tool_loop_policy;
use crate::agent_runtime::turn_ledger::{
    TurnLedgerEventKind, TurnLedgerRecord, persist_ledger_record,
};
use crate::agent_runtime::turn_loop_settings::TurnLoopSettings;
use crate::agent_runtime::turn_services;
use crate::agent_runtime::{
    MAX_REQUEST_PROMPT_CHARS, prompt_prep::truncate_text_for_budget,
    settings::runtime_settings_for_interactive_turn,
};
use crate::channel_delivery::ChannelDeliveryTarget;
use crate::daemon_api::InteractiveTurnRequest;
use crate::stage_routing::StageRoutingMatrix;
use crate::tools::TuiRuntime;
use crate::tui::settings::RuntimeSettings;
use crate::turn_continuation::TurnContinuationScope;
use stasis::application::orchestration::prompt_pipeline::{
    PromptExecutionPipeline, PromptExecutionRequest,
};
use stasis::application::orchestration::tool_registry::ToolRegistry;
use stasis::domain::runtime::placement::WorkerCapabilities;
use stasis::ports::outbound::ai_chat_client::AiChatClient;

use stasis::prelude::RuntimeComposition;

use super::model_routing::resolve_worker_llm_target;
use super::policy::{TurnWorkerIntent, max_worker_tool_rounds};
use crate::agent_runtime::turn_context::WorkerHandoffCapsule;
use crate::agent_runtime::worker_continuity::{
    InProcessDelegationRecord, record_in_process_delegation,
};
use crate::workshop_contract::{
    ExecutionPlacementResolution, ExecutionResolutionReason, ExecutionTargetCandidate,
    ExecutionTargetSelection, resolve_execution_target,
};

use super::prompts::{
    synthesis_user_prompt, synthesis_user_prompt_with_handoff, worker_failure_user_prompt,
};
use super::registry::{
    AllowlistToolRegistry, SessionBootstrapToolRegistry, WorkerSessionToolRegistry,
};
use super::store::{
    TurnWorkDisposition, TurnWorkRecord, TurnWorkStatus, TurnWorkerStore, turn_worker_store,
};
fn worker_canvas_lane_enabled(is_bound_workshop: bool, record: &TurnWorkRecord) -> bool {
    is_bound_workshop || record.supports_ui_artifacts
}

/// Catalog visibility is read authority for user-facing sessions, not the
/// execution grant for an internal delegated session. Delegated work is
/// authorized by the identity-bound durable record admitted from the signed
/// mesh grant; projecting that derived session into the visible catalog would
/// violate its intentionally private lifecycle.
fn worker_session_execution_authorized(record: &TurnWorkRecord, identity_user_id: &str) -> bool {
    if record.disposition == TurnWorkDisposition::Delegated {
        return record
            .identity_user_id
            .as_deref()
            .is_some_and(|bound| bound.trim() == identity_user_id);
    }
    crate::session_catalog::session_visible_to_profile(&record.session_id, identity_user_id)
}

fn delegated_task_grant_error(
    record: &TurnWorkRecord,
    identity_user_id: &str,
) -> Option<&'static str> {
    if record.disposition != TurnWorkDisposition::Delegated {
        return None;
    }
    let Some(grant) = record.task_execution_grant.as_ref() else {
        // Durable records created before destination-issued grants existed keep
        // the historical safe assistant ceiling and cannot gain new scopes.
        return None;
    };
    if let Some(spec) = record.worker_spawn_spec.as_ref()
        && (crate::delegated_task::validate_worker_spawn_spec(spec).is_err()
            || spec.intent != record.intent
            || spec.task != record.task_prompt
            || spec.execution_placement != record.execution_placement
            || spec.parent.turn_correlation_id
                != record.parent_turn_correlation_id.as_deref().unwrap_or_default()
            || grant.requested_tool_names != spec.tools.names)
    {
        return Some("canonical worker specification does not match the durable worker");
    }
    if grant.schema_version
        != crate::peer_execution_policy::TASK_EXECUTION_GRANT_SCHEMA_VERSION
    {
        return Some("unsupported task execution grant");
    }
    if grant.expires_at <= Utc::now() {
        return Some("task execution grant expired before execution");
    }
    if grant.work_id != record.work_id
        || identity_user_id != format!("peer:{}", grant.peer_device_id)
        || grant.origin_runtime_id != record.parent_runtime_id
        || record.parent_turn_correlation_id.as_deref() != Some(grant.correlation_id.as_str())
        || grant.worker_intent != record.intent
        || !grant
            .effective_tool_domains
            .iter()
            .any(|domain| domain == "turn")
    {
        return Some("task execution grant does not match the durable worker");
    }
    if record.execution_placement.resolution_reason
        != ExecutionResolutionReason::LegacyUnknown
        && grant.destination_runtime_id != record.execution_placement.resolved_runtime_id
    {
        return Some("task execution grant does not match the resolved runtime");
    }
    None
}

fn worker_turn_scope(record: &TurnWorkRecord) -> TurnContinuationScope {
    let canvas_lane =
        worker_canvas_lane_enabled(record.disposition == TurnWorkDisposition::Bound, record);
    TurnContinuationScope {
        turn_correlation_id: record
            .parent_turn_correlation_id
            .clone()
            .unwrap_or_else(|| format!("work-{}", record.work_id)),
        session_id: record.session_id.clone(),
        identity_user_id: record.identity_user_id.clone(),
        original_prompt: record.task_prompt.clone(),
        delivery_target: record
            .delivery_target
            .as_ref()
            .map(ChannelDeliveryTarget::from),
        provider: record.provider.clone(),
        model: record.model.clone(),
        response_depth_mode: record.response_depth_mode.clone(),
        supports_ui_artifacts: canvas_lane,
        supports_liquid_markdown: record.supports_liquid_markdown,
        supports_browser_host: record.supports_browser_host,
        channel_surface: Some(match record.disposition {
            TurnWorkDisposition::Bound => "workshop-canvas".to_string(),
            TurnWorkDisposition::Delegated => "delegated-worker".to_string(),
            TurnWorkDisposition::Parallel => "worker".to_string(),
        }),
    }
}

/// Live host-turn context used when spawning a worker from the tool loop.
#[derive(Clone)]
pub struct ActiveWorkerBusSession {
    pub sink: SharedAgentStreamSink,
    pub stream_turn_id: u64,
    pub session_id: String,
    pub identity_user_id: Option<String>,
    pub backend: String,
    pub parent_user_prompt: String,
    pub provider: String,
    pub model: String,
    pub response_depth_mode: String,
    pub parent_turn_correlation_id: Option<String>,
    /// Stable runtime identity of the daemon admitting this parent turn.
    pub parent_runtime_id: String,
    pub delivery_target: Option<crate::turn_continuation::StoredDeliveryTarget>,
    pub host_handoff_slot: Arc<tokio::sync::RwLock<Option<WorkerHandoffCapsule>>>,
    pub host_continuity_bundle:
        Option<crate::agent_runtime::worker_continuity::HostContinuityBundle>,
    /// Operator `max_tool_rounds` from the delegating host turn (not a separate worker cap).
    pub configured_max_tool_rounds: usize,
    /// Home client advertised HTML/canvas support when the host delegated this work.
    pub supports_ui_artifacts: bool,
    /// Host client advertised Liquid Markdown hydration support.
    pub supports_liquid_markdown: bool,
    pub supports_browser_host: bool,
    pub parent_agent_mode: Option<String>,
    pub parent_code_work_id: Option<String>,
}

/// Tooling snapshot for background workers (no full `Arc<TuiRuntime>` required).
#[derive(Clone)]
pub struct WorkerRuntimeContext {
    pub tool_registry: Arc<dyn ToolRegistry>,
    pub client_registry: crate::client_tools::ClientRegistry,
    pub identity_memory_store: Option<
        Arc<dyn stasis::ports::outbound::memory::identity_memory_store::IdentityMemoryStore>,
    >,
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
    pub turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
}

impl WorkerRuntimeContext {
    pub fn from_tui_runtime(rt: &crate::tools::TuiRuntime) -> Self {
        let provider = crate::resolve_llm_provider(None);
        let model = crate::resolve_llm_model(None);
        let base_url = crate::resolve_llm_base_url(Some(&provider), None);
        Self {
            tool_registry: rt.tool_registry.clone(),
            client_registry: rt.client_registry.clone(),
            identity_memory_store: Some(rt.identity_memory_store.clone()),
            provider,
            model,
            base_url,
            turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess::default(),
        }
    }
}

pub struct TurnWorkerScheduler {
    store: Arc<TurnWorkerStore>,
    runtime: RwLock<Option<Arc<RuntimeComposition>>>,
    execution_runtime_id: std::sync::RwLock<String>,
    parents: Mutex<WorkerParentState>,
}

const MAX_ACTIVE_WORKER_PARENTS: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WorkerParentHandle(uuid::Uuid);

impl WorkerParentHandle {
    fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

struct WorkerParentContext {
    runtime: WorkerRuntimeContext,
    bus: ActiveWorkerBusSession,
}

struct WorkerParentState {
    live: HashMap<WorkerParentHandle, Arc<WorkerParentContext>>,
    high_water: usize,
}

pub struct WorkerParentLease {
    scheduler: Weak<TurnWorkerScheduler>,
    handle: WorkerParentHandle,
    context: Arc<WorkerParentContext>,
}

impl Drop for WorkerParentLease {
    fn drop(&mut self) {
        let Some(scheduler) = self.scheduler.upgrade() else {
            return;
        };
        let mut state = scheduler.parents.lock().expect("worker parents poisoned");
        if state
            .live
            .get(&self.handle)
            .is_some_and(|current| Arc::ptr_eq(current, &self.context))
        {
            state.live.remove(&self.handle);
        }
    }
}

tokio::task_local! {
    static ACTIVE_WORKER_PARENT: WorkerParentHandle;
}

pub async fn with_worker_parent_scope<F>(future: F) -> F::Output
where
    F: Future,
{
    ACTIVE_WORKER_PARENT
        .scope(WorkerParentHandle::new(), future)
        .await
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SpawnTurnWorkerOutput {
    pub ok: bool,
    pub worker_spawned: bool,
    pub work_id: String,
    pub stasis_job_id: String,
    pub intent: String,
    pub manuscript_id: Option<String>,
    pub stage_role: Option<String>,
    pub parent_runtime_id: String,
    pub execution_placement: ExecutionPlacementResolution,
    pub status: String,
    pub user_ack: String,
    pub handoff_summary: String,
    pub scratch_digest: String,
    pub message: String,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum EnterBoundWorkshopOutput {
    Failure {
        ok: bool,
        workshop_entered: bool,
        error: String,
    },
    Entered {
        ok: bool,
        workshop_entered: bool,
        work_id: String,
        stasis_job_id: String,
        intent: String,
        parent_runtime_id: String,
        execution_placement: Box<ExecutionPlacementResolution>,
        status: String,
        user_ack: String,
        message: String,
        handoff_summary: String,
        scratch_digest: String,
    },
}

impl TurnWorkerScheduler {
    pub fn new(store: Arc<TurnWorkerStore>) -> Self {
        Self {
            store,
            runtime: RwLock::new(None),
            execution_runtime_id: std::sync::RwLock::new(
                crate::workshop_authority::current()
                    .map(|authority| authority.as_str().to_string())
                    .unwrap_or_else(|_| {
                        crate::workshop_contract::default_unknown_runtime_id()
                    }),
            ),
            parents: Mutex::new(WorkerParentState {
                live: HashMap::with_capacity(MAX_ACTIVE_WORKER_PARENTS),
                high_water: 0,
            }),
        }
    }

    pub async fn attach_runtime(&self, runtime: Arc<crate::tools::TuiRuntime>) {
        *self.runtime.write().await = Some(runtime.runtime.clone());
    }

    /// Set the stable Stasis node identity advertised by this process's
    /// worker host. Placement provenance and durable job constraints must use
    /// the same identifier or exact-target jobs can never be leased.
    pub fn set_execution_runtime_id(
        &self,
        runtime_id: impl Into<String>,
    ) -> Result<(), &'static str> {
        let runtime_id = runtime_id.into();
        let runtime_id = runtime_id.trim();
        if runtime_id.is_empty() {
            return Err("execution runtime id must not be empty");
        }
        *self
            .execution_runtime_id
            .write()
            .expect("execution runtime id poisoned") = runtime_id.to_string();
        Ok(())
    }

    pub fn execution_runtime_id(&self) -> String {
        self.execution_runtime_id
            .read()
            .expect("execution runtime id poisoned")
            .clone()
    }

    pub fn register_parent(
        self: &Arc<Self>,
        runtime: WorkerRuntimeContext,
        bus: ActiveWorkerBusSession,
    ) -> Result<WorkerParentLease, &'static str> {
        let handle = ACTIVE_WORKER_PARENT
            .try_with(|handle| *handle)
            .map_err(|_| "worker parent scope missing")?;
        let context = Arc::new(WorkerParentContext { runtime, bus });
        let mut state = self.parents.lock().expect("worker parents poisoned");
        if state.live.len() >= MAX_ACTIVE_WORKER_PARENTS {
            return Err("worker parent capacity reached");
        }
        if state.live.contains_key(&handle) {
            return Err("worker parent already registered");
        }
        state.live.insert(handle, context.clone());
        state.high_water = state.high_water.max(state.live.len());
        Ok(WorkerParentLease {
            scheduler: Arc::downgrade(self),
            handle,
            context,
        })
    }

    pub async fn active_bus_session_id(&self) -> Option<String> {
        self.active_parent()
            .ok()
            .map(|parent| parent.bus.session_id.clone())
    }

    fn active_parent(&self) -> Result<Arc<WorkerParentContext>, &'static str> {
        let handle = ACTIVE_WORKER_PARENT
            .try_with(|handle| *handle)
            .map_err(|_| "worker parent scope missing")?;
        self.parents
            .lock()
            .expect("worker parents poisoned")
            .live
            .get(&handle)
            .cloned()
            .ok_or("worker parent is not registered")
    }

    pub fn active_parent_count(&self) -> usize {
        self.parents
            .lock()
            .expect("worker parents poisoned")
            .live
            .len()
    }

    pub fn parent_high_water(&self) -> usize {
        self.parents
            .lock()
            .expect("worker parents poisoned")
            .high_water
    }

    pub fn store(&self) -> Arc<TurnWorkerStore> {
        self.store.clone()
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn spawn_worker(
        &self,
        intent: TurnWorkerIntent,
        task: &str,
        user_ack: &str,
        parent_user_prompt: Option<&str>,
        manuscript: Option<crate::identity_manuscript::ManuscriptContext>,
        stage_role: Option<&str>,
        model_hint: Option<&str>,
        execution_target: Option<ExecutionTargetSelection>,
    ) -> stasis::prelude::Result<SpawnTurnWorkerOutput> {
        let parent = self.active_parent().map_err(|error| {
            stasis::domain::errors::StasisError::PortFailure(format!(
                "cognition_workshop_mutate: {error}"
            ))
        })?;
        let bus = &parent.bus;
        let runtime_ctx = &parent.runtime;

        let parent_turn_correlation_id = bus.parent_turn_correlation_id.clone();
        let delivery_target = bus.delivery_target.clone();

        let work_id = format!("work-{}", uuid::Uuid::new_v4());
        let now = Utc::now();
        let parent_runtime_id = bus.parent_runtime_id.clone();
        let local_capabilities = WorkerCapabilities::any()
            .node_id(&parent_runtime_id)
            .platform(std::env::consts::OS)
            .architecture(std::env::consts::ARCH)
            .with_capability("assistant.work");
        let execution_placement = resolve_execution_target(
            execution_target.unwrap_or_default(),
            &parent_runtime_id,
            &[ExecutionTargetCandidate {
                runtime_id: parent_runtime_id.clone(),
                label: "This workshop".to_string(),
                capabilities: local_capabilities,
                user_selectable: true,
                agent_selectable: true,
            }],
        )
        .map_err(|error| {
            stasis::domain::errors::StasisError::PortFailure(error.to_string())
        })?;
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
            if let Some(bundle) = handoff.host_continuity.as_mut()
                && let Some(store) = runtime_ctx.identity_memory_store.as_ref()
            {
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
        let manuscript_spec = manuscript.as_ref().map(|value| {
            crate::delegated_task::WorkerManuscriptSpec {
                id: value.id.clone(),
                name: value.name.clone(),
                worker_intent: value.worker_intent.clone(),
                stage_role: value.worker_stage_role.clone(),
                model_hint: value.worker_model_hint.clone(),
                voice_appendix: value.voice_appendix.clone(),
                system_appendix: value.system_appendix.clone(),
                max_tool_rounds: value.max_tool_rounds,
                tools_allow: value.tools_allow.clone(),
                openshell_enabled: value.openshell_enabled,
                openshell_policy_template: value.openshell_policy_template.clone(),
                openshell_sandbox_from: value.openshell_sandbox_from.clone(),
            }
        });
        let mut requested_tool_names =
            super::policy::worker_allowlist_for_intent_and_tools(
                intent,
                manuscript_spec
                    .as_ref()
                    .map(|value| value.tools_allow.as_slice())
                    .unwrap_or(&[]),
            )
            .into_iter()
            .collect::<Vec<_>>();
        requested_tool_names.sort();
        let bot = crate::agent_runtime::execution_context::active_turn_execution_context()
            .and_then(|execution| {
                execution.bot_identity().map(|bot| crate::delegated_task::WorkerBotSpec {
                    bot_id: bot.bot_id().to_string(),
                    profile_revision: bot.profile_revision(),
                    memory_scope_id: bot.memory_scope_id().to_string(),
                    prompt_appendix: bot.prompt_appendix(),
                })
            });
        let worker_spawn_spec = crate::delegated_task::WorkerSpawnSpec {
            schema_version: crate::delegated_task::WORKER_SPAWN_SPEC_SCHEMA_VERSION,
            intent: intent.as_str().to_string(),
            task: task.trim().to_string(),
            user_ack: user_ack.trim().to_string(),
            manuscript_ids: manuscript_id.clone().into_iter().collect(),
            manuscript: manuscript_spec,
            stage_role: resolved_stage_role.clone(),
            model_hint: resolved_model_hint.clone(),
            parent: crate::delegated_task::WorkerParentSpec {
                stream_turn_id: bus.stream_turn_id,
                turn_correlation_id: parent_turn_correlation_id
                    .clone()
                    .unwrap_or_else(|| work_id.clone()),
                agent_mode: bus.parent_agent_mode.clone(),
                original_user_prompt: parent_user_prompt
                    .map(str::trim)
                    .filter(|prompt| !prompt.is_empty())
                    .unwrap_or(&bus.parent_user_prompt)
                    .to_string(),
                provider: bus.provider.clone(),
                model: bus.model.clone(),
                response_depth_mode: bus.response_depth_mode.clone(),
                code_work_id: bus.parent_code_work_id.clone(),
                bot,
                supports_ui_artifacts: bus.supports_ui_artifacts,
                supports_liquid_markdown: bus.supports_liquid_markdown,
                supports_browser_host: bus.supports_browser_host,
            },
            execution_placement: execution_placement.clone(),
            max_tool_rounds,
            tools: crate::delegated_task::WorkerToolRequest {
                names: requested_tool_names,
            },
        };
        crate::delegated_task::validate_worker_spawn_spec(&worker_spawn_spec).map_err(|error| {
            stasis::domain::errors::StasisError::PortFailure(format!(
                "cognition_workshop_mutate: invalid worker specification: {error}"
            ))
        })?;

        let record = TurnWorkRecord {
            work_id: work_id.clone(),
            session_id: bus.session_id.clone(),
            identity_user_id: bus.identity_user_id.clone(),
            parent_turn_correlation_id,
            parent_stream_turn_id: bus.stream_turn_id,
            parent_runtime_id: parent_runtime_id.clone(),
            execution_placement: execution_placement.clone(),
            task_execution_grant: None,
            worker_spawn_spec: Some(worker_spawn_spec),
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
            parent_agent_mode: bus.parent_agent_mode.clone(),
            parent_code_work_id: bus.parent_code_work_id.clone(),
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
            processed_steer_control_ids: Vec::new(),
            supports_ui_artifacts: bus.supports_ui_artifacts,
            supports_liquid_markdown: bus.supports_liquid_markdown,
            supports_browser_host: bus.supports_browser_host,
            live_tool_activity: Vec::new(),
            live_thinking: String::new(),
            live_output: String::new(),
            thinking_started_at: None,
            thinking_finished_at: None,
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
                "cognition_workshop_mutate: stasis runtime not ready".to_string(),
            )
        })?;
        crate::agent_runtime::turn_worker_job::enqueue_turn_worker_job(
            runtime.as_ref(),
            &work_id,
            bus.stream_turn_id,
        )
        .await?;

        Ok(SpawnTurnWorkerOutput {
            ok: true,
            worker_spawned: true,
            work_id: work_id.clone(),
            stasis_job_id: work_id,
            intent: intent.as_str().to_string(),
            manuscript_id,
            stage_role: record_stage_role_for_response(resolved_stage_role.as_deref()),
            parent_runtime_id: parent_runtime_id.clone(),
            execution_placement: execution_placement.clone(),
            status: "pending".to_string(),
            user_ack: user_ack.to_string(),
            handoff_summary,
            scratch_digest,
            message: "Worker enqueued on durable bus; host turn may end with user_ack.".to_string(),
        })
    }

    pub async fn enter_bound_workshop(
        &self,
        message: &str,
        goal: &str,
        intent: TurnWorkerIntent,
    ) -> stasis::prelude::Result<EnterBoundWorkshopOutput> {
        let parent = self.active_parent().map_err(|error| {
            stasis::domain::errors::StasisError::PortFailure(format!(
                "cognition_turn_begin_work: {error}"
            ))
        })?;
        let bus = &parent.bus;

        let task = goal.trim();
        if task.is_empty() {
            return Ok(EnterBoundWorkshopOutput::Failure {
                ok: false,
                workshop_entered: false,
                error: "goal is required and must be non-empty".to_string(),
            });
        }

        let user_ack = message.trim();
        if user_ack.is_empty() {
            return Ok(EnterBoundWorkshopOutput::Failure {
                ok: false,
                workshop_entered: false,
                error: "message is required and must be non-empty".to_string(),
            });
        }

        let parent_turn_correlation_id = bus.parent_turn_correlation_id.clone();
        let delivery_target = bus.delivery_target.clone();
        let work_id = format!("work-bound-{}", uuid::Uuid::new_v4());
        let now = Utc::now();
        let parent_runtime_id = bus.parent_runtime_id.clone();
        let execution_placement = ExecutionPlacementResolution::resolved(
            ExecutionTargetSelection::SameAsParent,
            parent_runtime_id.clone(),
            ExecutionResolutionReason::SameAsParent,
        );
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
        let (provider, model) =
            resolve_worker_llm_target(&bus.provider, &bus.model, intent, None, None);

        let record = TurnWorkRecord {
            work_id: work_id.clone(),
            session_id: bus.session_id.clone(),
            identity_user_id: bus.identity_user_id.clone(),
            parent_turn_correlation_id,
            parent_stream_turn_id: bus.stream_turn_id,
            parent_runtime_id: parent_runtime_id.clone(),
            execution_placement: execution_placement.clone(),
            task_execution_grant: None,
            worker_spawn_spec: None,
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
            parent_agent_mode: bus.parent_agent_mode.clone(),
            parent_code_work_id: bus.parent_code_work_id.clone(),
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
            processed_steer_control_ids: Vec::new(),
            supports_ui_artifacts: true,
            supports_liquid_markdown: bus.supports_liquid_markdown,
            supports_browser_host: bus.supports_browser_host,
            live_tool_activity: Vec::new(),
            live_thinking: String::new(),
            live_output: String::new(),
            thinking_started_at: None,
            thinking_finished_at: None,
            created_at: now,
            updated_at: now,
        };

        if let Err(error) = self.store.try_insert_bound(record) {
            let error = match error {
                super::store::BoundWorkshopAdmissionError::SessionDeleting => {
                    "The session is being deleted; workshop admission was rejected.".to_string()
                }
                super::store::BoundWorkshopAdmissionError::ActiveGeneration { work_id } => {
                    format!(
                        "A bound workshop is already active for this session ({work_id}); steer or cancel that exact generation first."
                    )
                }
            };
            return Ok(EnterBoundWorkshopOutput::Failure {
                ok: false,
                workshop_entered: false,
                error,
            });
        }
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

        Ok(EnterBoundWorkshopOutput::Entered {
            ok: true,
            workshop_entered: true,
            work_id: work_id.clone(),
            stasis_job_id: work_id,
            intent: intent.as_str().to_string(),
            parent_runtime_id,
            execution_placement: Box::new(execution_placement),
            status: "pending".to_string(),
            user_ack: user_ack.to_string(),
            message: user_ack.to_string(),
            handoff_summary,
            scratch_digest,
        })
    }
}

fn record_stage_role_for_response(stage_role: Option<&str>) -> Option<String> {
    stage_role.map(str::to_string)
}

fn ledger_bus_event(
    session_id: &str,
    stream_turn_id: u64,
    kind: TurnLedgerEventKind,
    detail: String,
) {
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
        bot_id: None,
        bot_profile_revision: None,
    };
    persist_ledger_record(Some(session_id), &record);
}

pub async fn run_worker_turn(
    store: Arc<TurnWorkerStore>,
    ctx: WorkerRuntimeContext,
    work_id: String,
    sink: SharedAgentStreamSink,
    stream_turn_id: u64,
    agent: Arc<TuiRuntime>,
) {
    let Some(record) = store.get(&work_id) else {
        return;
    };
    let Some(identity_user_id) = record
        .identity_user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
    else {
        store.update(&work_id, |record| {
            record.status = TurnWorkStatus::Failed;
            record.error = Some("worker execution identity is missing".to_string());
        });
        sink.notice(format!(
            "◈ work_failed work_id={work_id} error=missing_identity"
        ))
        .await;
        return;
    };
    if let Some(error) = delegated_task_grant_error(&record, &identity_user_id) {
        store.update(&work_id, |record| {
            record.status = TurnWorkStatus::Cancelled;
            record.error = Some(error.to_string());
            record.termination_reason = Some("task_execution_grant_denied".to_string());
        });
        sink.notice(format!(
            "◈ work_cancelled work_id={work_id} error=task_execution_grant_denied"
        ))
        .await;
        return;
    }
    if !worker_session_execution_authorized(&record, &identity_user_id) {
        store.update(&work_id, |record| {
            record.status = TurnWorkStatus::Failed;
            record.error = Some("worker session authority was revoked".to_string());
        });
        sink.notice(format!(
            "◈ work_failed work_id={work_id} error=revoked_authority"
        ))
        .await;
        return;
    }
    let Ok(session_id) = crate::session_storage::SessionId::parse(&record.session_id) else {
        store.update(&work_id, |record| {
            record.status = TurnWorkStatus::Failed;
            record.error = Some("worker session id is invalid".to_string());
        });
        return;
    };
    let execution_lease = match store.register_execution(&work_id) {
        Ok(lease) => lease,
        Err(error) => {
            sink.notice(format!(
                "◈ work_execution_rejected work_id={work_id} reason={error:?}"
            ))
            .await;
            return;
        }
    };
    let execution_budget = record
        .task_execution_grant
        .as_ref()
        .and_then(|grant| {
            grant
                .expires_at
                .signed_duration_since(Utc::now())
                .to_std()
                .ok()
        })
        .map(|remaining| remaining.min(std::time::Duration::from_secs(2 * 60 * 60)))
        .unwrap_or_else(|| std::time::Duration::from_secs(2 * 60 * 60));
    let scope = worker_turn_scope(&record);
    let execution_context = Arc::new(
        crate::agent_runtime::execution_context::TurnExecutionContext::new(
            work_id.clone(),
            record
                .parent_turn_correlation_id
                .clone()
                .unwrap_or_else(|| work_id.clone()),
            session_id,
            crate::request_principal::RequestPrincipal::worker(identity_user_id),
            crate::agent_runtime::execution_context::ProviderRoute::new(
                record.provider.clone(),
                record.model.clone(),
            ),
            crate::agent_runtime::execution_context::SurfaceCapabilities {
                ui_artifacts: record.supports_ui_artifacts,
                liquid_markdown: record.supports_liquid_markdown,
                browser_host: record.supports_browser_host,
            },
            execution_lease.cancellation().clone(),
            std::time::Instant::now() + execution_budget,
            scope.clone(),
        ),
    );
    let _execution_lease = execution_lease;
    crate::agent_runtime::execution_context::with_turn_execution_context(
        execution_context,
        run_worker_turn_inner(store, ctx, work_id, sink, stream_turn_id, record, agent),
    )
    .await;
}

async fn run_worker_turn_inner(
    store: Arc<TurnWorkerStore>,
    ctx: WorkerRuntimeContext,
    work_id: String,
    sink: SharedAgentStreamSink,
    stream_turn_id: u64,
    record: TurnWorkRecord,
    agent: Arc<TuiRuntime>,
) {
    store.update(&work_id, |record| {
        if record.status == TurnWorkStatus::Pending {
            record.status = TurnWorkStatus::Running;
        }
    });
    if store.is_work_cancelled(&work_id) {
        store.update(&work_id, |record| {
            record.status = TurnWorkStatus::Cancelled;
            record.termination_reason = Some("workshop_cancelled".to_string());
        });
        sink.notice(format!("◈ work_cancelled work_id={work_id}"))
            .await;
        if let Some(updated) = store.get(&work_id) {
            let is_bound_workshop = updated.disposition == TurnWorkDisposition::Bound;
            deliver_worker_parent_outcome(
                &ctx,
                &agent,
                updated,
                sink,
                stream_turn_id,
                is_bound_workshop,
            )
            .await;
        }
        return;
    }
    sink.notice(format!("◈ work_running work_id={work_id}"))
        .await;

    let is_bound_workshop = record.disposition == TurnWorkDisposition::Bound;
    let is_delegated = record.disposition == TurnWorkDisposition::Delegated;
    if is_bound_workshop && let Some(started) = store.get(&work_id) {
        crate::feed_adapters::publish_workshop_started(&started).await;
    }

    let intent = TurnWorkerIntent::parse(&record.intent).unwrap_or(TurnWorkerIntent::General);
    let manuscript_tools = record
        .handoff_capsule
        .as_ref()
        .and_then(|capsule| capsule.manuscript.as_ref())
        .map(|manuscript| manuscript.tools_allow.as_slice())
        .unwrap_or(&[] as &[String]);
    let allowlist = if is_delegated {
        let destination_grant = super::policy::remote_delegated_tool_ceiling_for_grant(
            record.task_execution_grant.as_ref(),
        );
        let semantic_ceiling =
            super::policy::worker_allowlist_for_intent_and_tools(intent, manuscript_tools);
        destination_grant
            .intersection(&semantic_ceiling)
            .cloned()
            .collect()
    } else {
        super::policy::worker_allowlist_for_intent_and_tools(intent, manuscript_tools)
    };
    let session_registry = Arc::new(WorkerSessionToolRegistry::new(
        ctx.tool_registry.clone(),
        record.session_id.clone(),
    ));
    let canvas_lane = worker_canvas_lane_enabled(is_bound_workshop, &record);
    let filtered_registry: Arc<dyn ToolRegistry> = if is_delegated {
        Arc::new(AllowlistToolRegistry::delegated(
            session_registry,
            allowlist,
        ))
    } else if canvas_lane {
        Arc::new(SessionBootstrapToolRegistry::bound_workshop(
            session_registry,
            record.session_id.clone(),
            allowlist,
            true,
            record.supports_browser_host || is_bound_workshop,
            Some("workshop-canvas".to_string()),
            ctx.client_registry.clone(),
        ))
    } else {
        Arc::new(SessionBootstrapToolRegistry::worker(
            session_registry,
            record.session_id.clone(),
            allowlist,
        ))
    };
    // Genai reads env vars; keys live in keyring/file first — inject for the
    // *worker* provider (may differ from the host turn).
    crate::workshop_env::apply_provider_llm_env(&record.provider);
    let worker_pipeline = crate::tui::runtime_services::build_tool_loop_pipeline_for_target(
        &record.provider,
        &record.model,
        crate::model_route::resolve_route_base_url(
            &record.provider,
            &ctx.provider,
            ctx.base_url.as_deref(),
        )
        .as_deref(),
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
        system_prompt: Some(super::prompts::worker_system_prompt_for_parent_mode(
            &record.session_id,
            TurnWorkerIntent::parse(&record.intent).unwrap_or(TurnWorkerIntent::General),
            record
                .handoff_capsule
                .as_ref()
                .and_then(|capsule| capsule.manuscript.as_ref()),
            record.supports_ui_artifacts,
            record.supports_liquid_markdown,
            record.parent_agent_mode.as_deref(),
        )),
        context: PromptExecutionContext::default(),
        tool_name: String::new(),
        tool_input: Value::Null,
        tool_call_mode,
    };

    let mut worker_scratch: Option<crate::agent_runtime::turn_context::TurnScratchpad> = None;
    let delegation_control: Arc<dyn medousa_runtime::DelegationControlPort> = store.clone();
    let runtime_ports = medousa_runtime::RuntimePorts::new()
        .with_optional_ledger_sink(super::super::turn_ledger::session_turn_ledger_sink(Some(
            &record.session_id,
        )))
        .with_tool_run_events(Arc::new(
            super::super::tool_stream::DaemonToolRunEventPort::new(sink.clone()),
        ))
        .with_turn_presentation(Arc::new(
            super::super::turn_presentation::DaemonTurnPresentationPort::new(sink.clone()),
        ))
        .with_budget_approval(Arc::new(
            crate::turn_budget_request::DaemonTurnBudgetApprovalPort::new(
                record.parent_turn_correlation_id.clone(),
                stream_turn_id,
                Some(record.session_id.clone()),
                record
                    .delivery_target
                    .as_ref()
                    .map(|target| target.channel.clone()),
                record.delivery_target.clone(),
                Some(sink.clone()),
            ),
        ))
        .with_delegation_control(delegation_control);
    let mut completion_gate = ToolLoopCompletionGate {
        stream_turn_id,
        runtime_ports,
        orchestration: None,
        budget: None,
        max_tool_rounds: worker_max_rounds,
        max_text_only_stuck_continues: turn_loop_settings.max_text_only_stuck_continues,
        scratch_out: Some(&mut worker_scratch),
        parent_turn_correlation_id: record.parent_turn_correlation_id.clone(),
        initial_worker_scratch,
        skip_avec_ritual_check: matches!(
            intent,
            TurnWorkerIntent::Research | TurnWorkerIntent::General
        ),
        tool_round_budget_ceiling: worker_max_rounds,
        hard_tool_round_ceiling: None,
        require_operator_budget_gate: false,
        completion_profile:
            crate::agent_runtime::turn_completion_fsm::TurnCompletionProfile::WorkerSynthesis,
        cancel_poll_work_id: Some(work_id.clone()),
        steer_poll_work_id: is_bound_workshop.then_some(work_id.clone()),
        round_context_provider: None,
        active_turn_checkpoint_sink: None,
        active_turn_resume: None,
    };

    if store.is_work_cancelled(&work_id) {
        store.update(&work_id, |r| {
            r.status = TurnWorkStatus::Cancelled;
            r.termination_reason = Some("workshop_cancelled".to_string());
        });
        sink.notice(format!("◈ work_cancelled work_id={work_id}"))
            .await;
        if let Some(updated) = store.get(&work_id) {
            deliver_worker_parent_outcome(
                &ctx,
                &agent,
                updated,
                sink,
                stream_turn_id,
                is_bound_workshop,
            )
            .await;
        }
        return;
    }

    let mut chunk_bridge = ProviderStreamBridge::new(sink.clone(), stream_turn_id);
    let chunk_stream = chunk_bridge.attempt();

    let result = worker_pipeline
        .execute_with_stream_prior_messages_max_rounds(
            request,
            Vec::new(),
            Some(chunk_stream.sender()),
            worker_max_rounds,
            Some(&mut completion_gate),
            None,
        )
        .await;

    let result = fail_on_stream_overflow(result, chunk_stream.finish().await);
    chunk_bridge.drain().await;

    match result {
        Ok(response) => {
            if store.is_work_cancelled(&work_id) {
                store.update(&work_id, |r| {
                    r.status = TurnWorkStatus::Cancelled;
                    r.termination_reason = Some("workshop_cancelled".to_string());
                });
                sink.notice(format!("◈ work_cancelled work_id={work_id}"))
                    .await;
                if let Some(updated) = store.get(&work_id) {
                    deliver_worker_parent_outcome(
                        &ctx,
                        &agent,
                        updated,
                        sink.clone(),
                        stream_turn_id,
                        is_bound_workshop,
                    )
                    .await;
                }
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
                if is_bound_workshop && let Some(updated) = store.get(&work_id) {
                    crate::feed_adapters::publish_workshop_working(
                        &updated,
                        updated.tool_names.len() as u32,
                        &updated.tool_names,
                    )
                    .await;
                }
                if let Some(updated) = store.get(&work_id) {
                    deliver_worker_parent_outcome(
                        &ctx,
                        &agent,
                        updated,
                        sink,
                        stream_turn_id,
                        is_bound_workshop,
                    )
                    .await;
                }
            }
        }
        Err(err) => {
            if store.is_work_cancelled(&work_id) {
                store.update(&work_id, |record| {
                    record.termination_reason = Some("workshop_cancelled".to_string());
                });
                sink.notice(format!("◈ work_cancelled work_id={work_id}"))
                    .await;
                if is_bound_workshop && let Some(cancelled) = store.get(&work_id) {
                    crate::feed_adapters::publish_workshop_terminal(&cancelled, "cancelled", None)
                        .await;
                }
                if let Some(updated) = store.get(&work_id) {
                    deliver_worker_parent_outcome(
                        &ctx,
                        &agent,
                        updated,
                        sink,
                        stream_turn_id,
                        is_bound_workshop,
                    )
                    .await;
                }
                return;
            }
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
            if is_bound_workshop && let Some(failed) = store.get(&work_id) {
                crate::feed_adapters::publish_workshop_terminal(
                    &failed,
                    "failed",
                    failed.error.as_deref(),
                )
                .await;
            }
            if let Some(failed) = store.get(&work_id) {
                deliver_worker_parent_outcome(
                    &ctx,
                    &agent,
                    failed,
                    sink,
                    stream_turn_id,
                    is_bound_workshop,
                )
                .await;
            }
        }
    }
}

async fn deliver_worker_parent_outcome(
    ctx: &WorkerRuntimeContext,
    agent: &TuiRuntime,
    record: TurnWorkRecord,
    sink: SharedAgentStreamSink,
    stream_turn_id: u64,
    is_bound_workshop: bool,
) {
    if record.disposition == TurnWorkDisposition::Delegated {
        return;
    }
    if is_bound_workshop {
        match record.status {
            TurnWorkStatus::Completed => {
                run_synthesis_turn(ctx, record, sink, stream_turn_id).await;
            }
            TurnWorkStatus::Failed => {
                run_worker_failure_notify(ctx, record, sink, stream_turn_id).await;
            }
            _ => {}
        }
        return;
    }
    super::host_resume::maybe_resume_host_after_parallel_worker(
        ctx,
        &agent.execution_registry,
        agent,
        &record,
        sink,
    )
    .await;
}

fn parallel_worker_uses_host_resume(disposition: TurnWorkDisposition) -> bool {
    disposition == TurnWorkDisposition::Parallel
}

pub async fn resume_synthesis_if_needed(
    ctx: &WorkerRuntimeContext,
    execution_registry: &crate::agent_runtime::execution_context::TurnExecutionRegistry,
    record: TurnWorkRecord,
    sink: SharedAgentStreamSink,
    agent: Option<&TuiRuntime>,
) {
    if record.disposition == TurnWorkDisposition::Delegated {
        return;
    }
    if parallel_worker_uses_host_resume(record.disposition) {
        let Some(agent) = agent else {
            tracing::warn!(
                work_id = %record.work_id,
                "parallel host resume skipped without agent runtime"
            );
            return;
        };
        super::host_resume::maybe_resume_host_after_parallel_worker(
            ctx,
            execution_registry,
            agent,
            &record,
            sink,
        )
        .await;
        return;
    }
    if record.synthesis_delivered || record.status != TurnWorkStatus::Completed {
        return;
    }
    let Some(identity_user_id) = record
        .identity_user_id
        .clone()
        .filter(|value| !value.trim().is_empty())
    else {
        tracing::warn!(work_id = %record.work_id, "refusing synthesis resume without identity");
        return;
    };
    if !crate::session_catalog::session_visible_to_profile(&record.session_id, &identity_user_id) {
        tracing::warn!(work_id = %record.work_id, "refusing synthesis resume after authority revocation");
        return;
    }
    let scope = worker_turn_scope(&record);
    let execution = match crate::agent_runtime::execution_context::TurnExecutionContext::from_scope(
        format!("{}-synthesis", record.work_id),
        crate::request_principal::RequestPrincipal::worker(identity_user_id),
        tokio_util::sync::CancellationToken::new(),
        std::time::Instant::now() + std::time::Duration::from_secs(2 * 60 * 60),
        scope,
    ) {
        Ok(execution) => execution,
        Err(error) => {
            tracing::warn!(work_id = %record.work_id, error = %error, "refusing synthesis resume with invalid session");
            return;
        }
    };
    let execution_lease = match execution_registry.admit(execution) {
        Ok(lease) => lease,
        Err(error) => {
            tracing::warn!(work_id = %record.work_id, error = %error, "synthesis resume admission rejected");
            return;
        }
    };
    let execution_context = execution_lease.context().clone();
    let stream_turn_id = record.parent_stream_turn_id;
    crate::agent_runtime::execution_context::with_turn_execution_context(
        execution_context,
        run_synthesis_turn(ctx, record, sink, stream_turn_id),
    )
    .await;
    drop(execution_lease);
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

    let prompt =
        worker_failure_user_prompt(&parent_prompt, &record.work_id, &record.intent, &error);

    // Prefer the host turn's credentials for the user-visible failure note —
    // the worker provider may be the reason the turn failed (missing key, etc.).
    crate::workshop_env::apply_provider_llm_env(&ctx.provider);
    let resolved_provider = crate::resolve_llm_provider(Some(ctx.provider.as_str()));
    let resolved_model = crate::resolve_llm_model(Some(ctx.model.as_str()));
    let resolved_base_url = crate::model_route::resolve_route_base_url(
        &resolved_provider,
        &ctx.provider,
        ctx.base_url.as_deref(),
    );
    let chat_client: Arc<dyn AiChatClient> = Arc::new(crate::build_genai_chat_client(
        &resolved_provider,
        &resolved_model,
        resolved_base_url.as_deref(),
    ));
    let pipeline = PromptExecutionPipeline::new(chat_client);
    let request = PromptExecutionRequest::from_user_prompt(truncate_text_for_budget(
        &prompt,
        MAX_REQUEST_PROMPT_CHARS,
    ))
    .with_context(PromptExecutionContext::default())
    .with_system_prompt(super::prompts::host_system_prompt_for_parent_mode(
        record.parent_agent_mode.as_deref(),
    ));

    let text = match pipeline.execute(request).await {
        Ok(response) => response.text,
        Err(err) => format!(
            "The background task didn't finish (notify error: {err}). Worker error: {}",
            truncate_text_for_budget(&error, 400)
        ),
    };

    sink.agent_response(
        notify_turn_id,
        text,
        vec!["turn_worker.failure".to_string()],
    )
    .await;

    crate::feed_adapters::publish_workshop_finish_activity(
        &record,
        "failed",
        Some(
            &record
                .error
                .clone()
                .unwrap_or_else(|| "worker failed".to_string()),
        ),
    )
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

    crate::workshop_env::apply_provider_llm_env(&record.provider);
    let resolved_provider = crate::resolve_llm_provider(Some(record.provider.as_str()));
    let resolved_model = crate::resolve_llm_model(Some(record.model.as_str()));
    let resolved_base_url = crate::model_route::resolve_route_base_url(
        &resolved_provider,
        &ctx.provider,
        ctx.base_url.as_deref(),
    );
    let chat_client: Arc<dyn AiChatClient> = Arc::new(crate::build_genai_chat_client(
        &resolved_provider,
        &resolved_model,
        resolved_base_url.as_deref(),
    ));
    let pipeline = PromptExecutionPipeline::new(chat_client);
    let mut request = PromptExecutionRequest::from_user_prompt(truncate_text_for_budget(
        &synthesis_prompt,
        MAX_REQUEST_PROMPT_CHARS,
    ))
    .with_context(PromptExecutionContext::default());
    request = request.with_system_prompt(super::prompts::host_system_prompt_for_parent_mode(
        record.parent_agent_mode.as_deref(),
    ));
    let response = match pipeline.execute(request).await {
        Ok(response) => response,
        Err(err) => {
            turn_worker_store().update(&record.work_id, |worker| {
                worker.synthesis_delivered = true;
            });
            sink.agent_error(synthesis_turn_id, format!("Worker synthesis failed: {err}"))
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
    crate::turn_worker_notify::publish_worker_synthesis_to_parent_turn(record, &text, &tool_names)
        .await;
    turn_worker_store().update(&record.work_id, |worker| {
        worker.synthesis_delivered = true;
        worker.result_text = Some(text.clone());
    });
    // Work-scoped finish on the workspace feed bus (independent of parent SSE).
    crate::feed_adapters::publish_workshop_finish_activity(record, "synthesis", Some(&text)).await;
    if record.disposition == TurnWorkDisposition::Bound {
        crate::feed_adapters::publish_workshop_synthesis(record, &text).await;
        crate::feed_adapters::publish_workshop_terminal(record, "done", Some(&text)).await;
    }
}

fn worker_settings_from_record(record: &TurnWorkRecord) -> RuntimeSettings {
    let request = InteractiveTurnRequest {
        session_id: record.session_id.clone(),
        prompt: record.task_prompt.clone(),
        agent_mode: None,
        code_context: None,
        code_project_setup_authorized: false,
        persist_user_turn: false,
        response_depth_mode: record.response_depth_mode.clone(),
        reasoning_effort: crate::reasoning_effort::REASONING_EFFORT_DEFAULT.to_string(),
        provider: record.provider.clone(),
        model: record.model.clone(),
        stage_routing: StageRoutingMatrix::default_for(&record.provider, &record.model),
        surface: None,
        host_context: None,
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

#[allow(clippy::too_many_arguments)]
pub fn pipeline_for_turn_profile(
    tool_registry: Arc<dyn ToolRegistry>,
    provider: &str,
    model: &str,
    base_url: Option<&str>,
    host_bus: bool,
    session_id: Option<&str>,
    supports_ui_artifacts: bool,
    supports_browser_host: bool,
    channel_surface: Option<&str>,
    client_registry: crate::client_tools::ClientRegistry,
) -> medousa_runtime::MedousaToolLoopPipeline {
    if host_bus {
        let allowlist = super::policy::host_bus_tool_names();
        let filtered: Arc<dyn ToolRegistry> =
            if let Some(session_id) = session_id.filter(|id| !id.trim().is_empty()) {
                Arc::new(SessionBootstrapToolRegistry::host(
                    tool_registry,
                    session_id,
                    allowlist,
                    supports_ui_artifacts,
                    supports_browser_host,
                    channel_surface.map(str::to_string),
                    client_registry,
                ))
            } else {
                Arc::new(AllowlistToolRegistry::new(tool_registry, allowlist))
            };
        crate::tui::runtime_services::build_tool_loop_pipeline_for_target(
            provider, model, base_url, filtered,
        )
    } else {
        crate::tui::runtime_services::build_tool_loop_pipeline_for_target(
            provider,
            model,
            base_url,
            tool_registry,
        )
    }
}

pub fn system_prompt_for_host_bus(base: &str, host_bus: bool) -> String {
    super::prompts::system_prompt_for_host_profile(base, host_bus, false, false, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_runtime::turn_worker::store::{TurnWorkRecord, TurnWorkStatus};
    use async_trait::async_trait;
    use medousa_engine::receipt::ArtifactReceiptMeta;
    use serde_json::Value;
    use stasis::application::orchestration::tool_registry::InMemoryToolRegistry;

    struct NoopSink;

    #[async_trait]
    impl crate::agent_runtime::stream_sink::AgentStreamSink for NoopSink {
        async fn content_chunk(&self, _turn_id: u64, _delta: String) {}
        async fn reasoning_chunk(&self, _turn_id: u64, _delta: String) {}
        async fn agent_response(&self, _turn_id: u64, _text: String, _tools: Vec<String>) {}
        async fn agent_error(&self, _turn_id: u64, _message: String) {}
        async fn notice(&self, _message: String) {}
        async fn tool_invoked(&self, _tool_name: String, _input_summary: String) {}
        async fn tool_payload(
            &self,
            _tool_name: String,
            _tool_input: Value,
            _tool_output: Value,
            _input_receipt: Option<ArtifactReceiptMeta>,
            _output_receipt: Option<ArtifactReceiptMeta>,
        ) {
        }
    }

    fn parent_context(session_id: &str) -> (WorkerRuntimeContext, ActiveWorkerBusSession) {
        let turn_scope = crate::agent_runtime::execution_context::TurnScopeAccess::default();
        let runtime = WorkerRuntimeContext {
            tool_registry: Arc::new(InMemoryToolRegistry::default()),
            client_registry: crate::client_tools::ClientRegistry::new(),
            identity_memory_store: None,
            provider: "provider".to_string(),
            model: "model".to_string(),
            base_url: None,
            turn_scope,
        };
        let bus = ActiveWorkerBusSession {
            sink: Arc::new(NoopSink),
            stream_turn_id: 1,
            session_id: session_id.to_string(),
            identity_user_id: None,
            backend: "memory".to_string(),
            parent_user_prompt: "prompt".to_string(),
            provider: "provider".to_string(),
            model: "model".to_string(),
            response_depth_mode: "standard".to_string(),
            parent_turn_correlation_id: Some(format!("turn-{session_id}")),
            parent_runtime_id: "runtime-local".to_string(),
            delivery_target: None,
            host_handoff_slot: Arc::new(RwLock::new(None)),
            host_continuity_bundle: None,
            configured_max_tool_rounds: 8,
            supports_ui_artifacts: false,
            supports_liquid_markdown: false,
            supports_browser_host: false,
            parent_agent_mode: None,
            parent_code_work_id: None,
        };
        (runtime, bus)
    }

    fn sample_record(
        termination_reason: Option<&str>,
        result_text: Option<&str>,
    ) -> TurnWorkRecord {
        TurnWorkRecord {
            work_id: "w1".to_string(),
            session_id: "s1".to_string(),
            identity_user_id: None,
            parent_turn_correlation_id: None,
            parent_stream_turn_id: 0,
            parent_runtime_id: "runtime-test".to_string(),
            execution_placement: Default::default(),
            task_execution_grant: None,
            worker_spawn_spec: None,
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
            parent_agent_mode: None,
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
            processed_steer_control_ids: Vec::new(),
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
        assert!(!worker_synthesis_pass_through(&sample_record(
            None,
            Some("done")
        )));
    }

    /// Workers only pass through on an explicit cognition_turn_finish;
    /// partial or fused results still go through host synthesis.
    #[test]
    fn worker_requires_finish_for_pass_through() {
        let mut record = sample_record(Some("max_rounds_fuse"), Some("partial"));
        record.disposition = TurnWorkDisposition::Bound;
        assert!(!worker_synthesis_pass_through(&record));
        assert!(worker_synthesis_pass_through(&sample_record(
            Some("cognition_turn_finish"),
            Some("Here is the report.")
        )));
    }

    #[test]
    fn worker_scope_is_rebuilt_only_from_the_durable_record() {
        let mut record = sample_record(Some("cognition_turn_finish"), Some("done"));
        record.identity_user_id = Some("user:alice".to_string());
        record.provider = "worker-provider".to_string();
        record.model = "worker-model".to_string();
        let scope = worker_turn_scope(&record);

        assert_eq!(scope.session_id, "s1");
        assert_eq!(scope.identity_user_id.as_deref(), Some("user:alice"));
        assert_eq!(scope.provider, "worker-provider");
        assert_eq!(scope.model, "worker-model");
    }

    #[test]
    fn delegated_worker_uses_its_durable_identity_binding_not_catalog_visibility() {
        let mut record = sample_record(None, None);
        record.disposition = TurnWorkDisposition::Delegated;
        record.session_id = "ses_internal_delegated".to_string();
        record.identity_user_id = Some("peer:phone-a".to_string());

        assert!(worker_session_execution_authorized(&record, "peer:phone-a"));
        assert!(!worker_session_execution_authorized(
            &record,
            "peer:phone-b"
        ));
    }

    #[tokio::test]
    async fn concurrent_parent_scopes_never_cross_sessions() {
        let scheduler = Arc::new(TurnWorkerScheduler::new(turn_worker_store()));
        let barrier = Arc::new(tokio::sync::Barrier::new(2));
        let run = |session: &'static str,
                   scheduler: Arc<TurnWorkerScheduler>,
                   barrier: Arc<tokio::sync::Barrier>| async move {
            with_worker_parent_scope(async move {
                let (runtime, bus) = parent_context(session);
                let _lease = scheduler.register_parent(runtime, bus).unwrap();
                barrier.wait().await;
                tokio::task::yield_now().await;
                scheduler.active_bus_session_id().await.unwrap()
            })
            .await
        };

        let (first, second) = tokio::join!(
            run("session-a", scheduler.clone(), barrier.clone()),
            run("session-b", scheduler.clone(), barrier.clone())
        );

        assert_eq!(first, "session-a");
        assert_eq!(second, "session-b");
        assert_eq!(scheduler.active_parent_count(), 0);
        assert_eq!(scheduler.parent_high_water(), 2);
    }

    #[tokio::test]
    async fn stale_parent_lease_cannot_remove_replacement_generation() {
        let scheduler = Arc::new(TurnWorkerScheduler::new(turn_worker_store()));
        with_worker_parent_scope(async {
            let (runtime, bus) = parent_context("stale-session");
            let stale = scheduler.register_parent(runtime, bus).unwrap();
            let handle = ACTIVE_WORKER_PARENT.with(|handle| *handle);
            let (runtime, bus) = parent_context("replacement-session");
            let replacement = Arc::new(WorkerParentContext { runtime, bus });
            scheduler
                .parents
                .lock()
                .unwrap()
                .live
                .insert(handle, replacement.clone());

            drop(stale);

            let current = scheduler
                .parents
                .lock()
                .unwrap()
                .live
                .get(&handle)
                .cloned()
                .expect("replacement must remain live");
            assert!(Arc::ptr_eq(&current, &replacement));
            assert_eq!(current.bus.session_id, "replacement-session");
            scheduler.parents.lock().unwrap().live.remove(&handle);
        })
        .await;
        assert_eq!(scheduler.active_parent_count(), 0);
    }

    #[tokio::test]
    async fn parent_admission_is_bounded_and_returns_to_zero() {
        let scheduler = Arc::new(TurnWorkerScheduler::new(turn_worker_store()));
        let mut leases = Vec::with_capacity(MAX_ACTIVE_WORKER_PARENTS);
        for index in 0..MAX_ACTIVE_WORKER_PARENTS {
            let session_id = format!("session-{index}");
            let lease = with_worker_parent_scope(async {
                let (runtime, bus) = parent_context(&session_id);
                scheduler.register_parent(runtime, bus).unwrap()
            })
            .await;
            leases.push(lease);
        }

        let error = with_worker_parent_scope(async {
            let (runtime, bus) = parent_context("over-capacity");
            scheduler.register_parent(runtime, bus).err().unwrap()
        })
        .await;
        assert_eq!(error, "worker parent capacity reached");
        assert_eq!(scheduler.active_parent_count(), MAX_ACTIVE_WORKER_PARENTS);
        assert_eq!(scheduler.parent_high_water(), MAX_ACTIVE_WORKER_PARENTS);

        drop(leases);
        assert_eq!(scheduler.active_parent_count(), 0);
    }

    #[test]
    fn bound_workshop_admission_is_atomic_per_session() {
        let store = Arc::new(TurnWorkerStore::empty_for_tests());
        let barrier = Arc::new(std::sync::Barrier::new(3));
        let spawn = |work_id: &'static str,
                     store: Arc<TurnWorkerStore>,
                     barrier: Arc<std::sync::Barrier>| {
            std::thread::spawn(move || {
                let mut record = sample_record(None, None);
                record.work_id = work_id.to_string();
                record.status = TurnWorkStatus::Pending;
                record.disposition = TurnWorkDisposition::Bound;
                barrier.wait();
                store.try_insert_bound(record)
            })
        };
        let first = spawn("bound-a", store.clone(), barrier.clone());
        let second = spawn("bound-b", store.clone(), barrier.clone());
        barrier.wait();
        let results = [first.join().unwrap(), second.join().unwrap()];

        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(store.list_for_session("s1").len(), 1);
        assert!(results.iter().any(|result| matches!(
            result,
            Err(super::super::store::BoundWorkshopAdmissionError::ActiveGeneration { .. })
        )));
    }

    #[test]
    fn stale_steer_cannot_mutate_replacement_workshop() {
        let store = TurnWorkerStore::empty_for_tests();
        let mut first = sample_record(None, None);
        first.work_id = "bound-old".to_string();
        first.status = TurnWorkStatus::Pending;
        first.disposition = TurnWorkDisposition::Bound;
        store.try_insert_bound(first).unwrap();
        store.update("bound-old", |record| {
            record.status = TurnWorkStatus::Cancelled
        });

        let mut replacement = sample_record(None, None);
        replacement.work_id = "bound-new".to_string();
        replacement.status = TurnWorkStatus::Pending;
        replacement.disposition = TurnWorkDisposition::Bound;
        store.try_insert_bound(replacement).unwrap();

        let stale = store.push_steer_exact(
            "s1",
            "bound-old",
            "stale guidance".to_string(),
            Some("user:alice".to_string()),
        );
        assert!(matches!(
            stale,
            Err(super::super::store::BoundWorkshopMutationError::StaleGeneration {
                active_work_id: Some(active),
            }) if active == "bound-new"
        ));
        assert!(store.get("bound-new").unwrap().steer_messages.is_empty());

        let updated = store
            .push_steer_exact(
                "s1",
                "bound-new",
                "current guidance".to_string(),
                Some("user:alice".to_string()),
            )
            .unwrap();
        assert_eq!(updated.steer_messages.len(), 1);
    }

    #[test]
    fn exact_cancel_cannot_cross_session_authority() {
        let store = Arc::new(TurnWorkerStore::empty_for_tests());
        let mut record = sample_record(None, None);
        record.work_id = "session-a-worker".to_string();
        record.status = TurnWorkStatus::Running;
        store.insert(record);
        let execution = store.register_execution("session-a-worker").unwrap();
        assert_eq!(store.live_execution_count(), 1);
        assert!(!execution.cancellation().is_cancelled());

        assert!(matches!(
            store.register_execution("session-a-worker"),
            Err(super::super::store::WorkerExecutionRegistrationError::AlreadyRunning)
        ));

        assert!(matches!(
            store.cancel_exact("session-b", "session-a-worker"),
            Err(super::super::store::TurnWorkerMutationError::ForeignSession)
        ));
        assert_eq!(
            store.get("session-a-worker").unwrap().status,
            TurnWorkStatus::Running
        );
        assert!(!execution.cancellation().is_cancelled());

        let cancelled = store.cancel_exact("s1", "session-a-worker").unwrap();
        assert_eq!(cancelled.status, TurnWorkStatus::Cancelled);
        assert!(execution.cancellation().is_cancelled());

        drop(execution);
        assert_eq!(store.live_execution_count(), 0);
    }

    #[test]
    fn parallel_workers_resume_host_bound_workers_synthesize() {
        assert!(parallel_worker_uses_host_resume(
            TurnWorkDisposition::Parallel
        ));
        assert!(!parallel_worker_uses_host_resume(
            TurnWorkDisposition::Bound
        ));
    }
}
