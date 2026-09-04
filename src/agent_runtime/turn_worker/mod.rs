//! In-process host/worker turn bus (Phase 1).

mod host_resume;
mod model_routing;
mod policy;
mod prompts;
mod registry;
mod routing;
mod run;
mod status;
mod store;

pub use host_resume::{
    fallback_host_resume_text, host_resume_prompt, maybe_resume_host_after_parallel_worker,
    register_host_resume_ports,
};
pub use model_routing::{
    default_stage_role_for_intent, resolve_worker_llm_target, resolve_worker_llm_target_with_matrix,
};
pub use policy::{
    MOBILE_FOREGROUND_TOOL_CEILING, REMOTE_DELEGATED_TOOL_CEILING, TurnWorkerIntent,
    allowed_tool_names_for_intent, host_bus_tool_names, max_worker_tool_rounds,
    mobile_foreground_tool_ceiling, remote_delegated_tool_ceiling,
    remote_delegated_tool_ceiling_for_grant, tool_allowed,
    worker_allowlist_for_intent_and_tools,
};
pub use prompts::{
    host_system_prompt_for_parent_mode, system_prompt_for_host_profile, worker_system_prompt,
    worker_system_prompt_for_parent_mode,
};
pub use registry::{
    AllowlistToolRegistry, SessionBootstrapToolRegistry, WorkerSessionToolRegistry,
    inject_worker_session_id,
};
pub use routing::{
    HOST_BUS_MAX_TOOL_ROUNDS, HostBusEnvMode, HostTurnProfile, HostTurnRoute,
    apply_host_profile_to_activation, classify_host_turn_route_heuristic, host_bus_env_mode,
    host_bus_force_enabled, host_route_notice, resolve_host_turn_profile,
};
pub use run::{
    ActiveWorkerBusSession, EnterBoundWorkshopOutput, SpawnTurnWorkerOutput, TurnWorkerScheduler,
    WorkerRuntimeContext, host_bus_mode_enabled, pipeline_for_turn_profile,
    resume_synthesis_if_needed, run_worker_turn, system_prompt_for_host_bus,
    with_worker_parent_scope,
};
pub use status::{append_active_workers_hint, format_active_workers_block};
pub use store::{
    BoundWorkshopAdmissionError, BoundWorkshopMutationError, DelegatedWorkAdmissionError,
    DelegatedWorkControlError, TurnWorkDisposition, TurnWorkRecord, TurnWorkStatus,
    TurnWorkerMutationError, TurnWorkerStore, WorkerExecutionLease,
    WorkerExecutionRegistrationError, WorkerToolActivity, turn_worker_store,
};
