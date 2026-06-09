//! In-process host/worker turn bus (Phase 1).

mod policy;
mod prompts;
mod registry;
mod routing;
mod run;
mod store;

pub use policy::{
    TurnWorkerIntent, allowed_tool_names_for_intent, host_bus_tool_names, max_worker_tool_rounds,
    tool_allowed,
};
pub use prompts::{
    HOST_BUS_TURN_APPENDIX, WORKER_SYSTEM_APPENDIX, system_prompt_for_host_profile,
    worker_system_prompt,
};
pub use registry::{
    AllowlistToolRegistry, SessionBootstrapToolRegistry, WorkerSessionToolRegistry,
    inject_worker_session_id,
};
pub use routing::{
    HostBusEnvMode, HostTurnProfile, HostTurnRoute, apply_host_profile_to_activation,
    classify_host_turn_route_heuristic, host_bus_env_mode, host_bus_force_enabled,
    host_route_notice, resolve_host_turn_profile, HOST_BUS_MAX_TOOL_ROUNDS,
};
pub use run::{
    ActiveWorkerBusSession, TurnWorkerScheduler, WorkerRuntimeContext, host_bus_mode_enabled,
    pipeline_for_turn_profile, run_worker_turn, system_prompt_for_host_bus,
};
pub use store::{TurnWorkRecord, TurnWorkStatus, TurnWorkerStore, turn_worker_store};
