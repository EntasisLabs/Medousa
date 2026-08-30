//! Portable agent-runtime namespace used by the embedded deployment.
//!
//! The embedded host owns turn execution through `medousa-runtime`; shared
//! tools only need the active-turn context and prompt policy portions of the
//! daemon agent runtime.

#[path = "agent_runtime/execution_context.rs"]
pub mod execution_context;
#[path = "agent_runtime/prompt_policy.rs"]
pub(crate) mod prompt_policy;
#[path = "agent_runtime/turn_worker/policy.rs"]
pub mod turn_worker_policy;

pub mod turn_worker {
    pub use super::turn_worker_policy::*;
}

pub mod stream_sink {
    pub use medousa_engine::stream_sink::*;
}

#[path = "agent_runtime/ambient_context.rs"]
pub mod ambient_context;
#[path = "agent_runtime/host_context.rs"]
pub mod host_context;
#[path = "agent_runtime/vibe_signature.rs"]
pub mod vibe_signature;
pub use vibe_signature::{default_handoff_model_avec, derive_vibe_signature};

pub mod prompt_prep {
    pub use crate::text_budget::truncate_text_for_budget;
}
