//! Channel-agnostic turn request and stream event types.
//!
//! Aliased to the daemon interactive turn contract until Phase 2 unifies routes.

pub use crate::daemon_api::{
    InteractiveTurnRequest as AgentTurnRequest, InteractiveTurnStreamEvent as AgentStreamEvent,
};
