//! Shared runtime identity and health response construction.

use chrono::{DateTime, Utc};
use medousa_types::{AuthorityId, DaemonRuntimeDescriptor, HealthResponse};
use stasis::sdk::runtime_sdk::RuntimeStatsSnapshot;

use crate::daemon_api::DaemonStatsResponse;

pub const AGENT_RUNTIME_VERSION: &str = "centralized-v1";

/// Assembly-time label for the singleton daemon agent runtime — never a chat
/// session. It lives in the shared layer so profile validation is independent
/// of the native/TUI composition.
pub const RUNTIME_BOOTSTRAP_SESSION_ID: &str = "__runtime_bootstrap__";

/// Legacy bootstrap label retained for reserved-profile migration guards.
pub const LEGACY_RUNTIME_BOOTSTRAP_SESSION_ID: &str = "daemon-agent-runtime";

pub fn is_runtime_bootstrap_session_id(session_id: &str) -> bool {
    let trimmed = session_id.trim();
    trimmed == RUNTIME_BOOTSTRAP_SESSION_ID || trimmed == LEGACY_RUNTIME_BOOTSTRAP_SESSION_ID
}

const COMMON_CAPABILITIES: &[&str] = &[
    "agent.loop",
    "daemon.authority",
    "grapheme.runtime",
    "memory.locus",
    "notes.vault",
    "persistence.surrealkv",
    "profiles.workshop",
    "scheduling.stasis",
    "sessions.coordinates",
    "stasis.control-plane",
    "turns.sequenced",
];

pub struct DaemonHealthSnapshot {
    pub backend: String,
    pub worker_id: String,
    pub agent_runtime_version: String,
    pub tool_registry_count: usize,
    pub last_agent_turn_latency_ms: Option<u64>,
    pub last_agent_turn_at_utc: Option<DateTime<Utc>>,
    pub active_profile_id: String,
    pub active_profile_display_name: String,
}

pub struct DaemonStatsObservation {
    pub last_tick_at_utc: Option<DateTime<Utc>>,
    pub active_turn_executions: usize,
    pub active_turn_executions_high_water: usize,
    pub missing_turn_context_invocations: u64,
}

pub fn stats_response(
    snapshot: RuntimeStatsSnapshot,
    observation: DaemonStatsObservation,
) -> DaemonStatsResponse {
    DaemonStatsResponse {
        enqueued_jobs: snapshot.enqueued_jobs,
        running_jobs: snapshot.running_jobs,
        succeeded_jobs: snapshot.succeeded_jobs,
        failed_jobs: snapshot.failed_jobs,
        dead_letter_jobs: snapshot.dead_letter_jobs,
        pending_outbox_events: snapshot.pending_outbox_events,
        recurring_definitions: snapshot.recurring_definitions,
        last_tick_at_utc: observation.last_tick_at_utc,
        active_turn_executions: observation.active_turn_executions,
        active_turn_executions_high_water: observation.active_turn_executions_high_water,
        missing_turn_context_invocations: observation.missing_turn_context_invocations,
    }
}

pub fn health_response(
    authority_id: AuthorityId,
    deployment_profile: &str,
    advertised_capabilities: impl IntoIterator<Item = impl Into<String>>,
    snapshot: DaemonHealthSnapshot,
) -> HealthResponse {
    let mut advertised_capabilities = COMMON_CAPABILITIES
        .iter()
        .map(|value| (*value).to_string())
        .chain(advertised_capabilities.into_iter().map(Into::into))
        .collect::<Vec<_>>();
    advertised_capabilities.sort();
    advertised_capabilities.dedup();

    HealthResponse {
        runtime: DaemonRuntimeDescriptor {
            authority_id,
            product_version: env!("CARGO_PKG_VERSION").to_string(),
            build_revision: build_revision().to_string(),
            contract_revision: medousa_types::DAEMON_API_CONTRACT_REVISION,
            base_schema_revision: base_schema_revision(),
            deployment_profile: deployment_profile.to_string(),
            deployment_target: format!(
                "{deployment_profile}:{}:{}",
                std::env::consts::OS,
                std::env::consts::ARCH
            ),
            advertised_capabilities,
        },
        status: "ok".to_string(),
        backend: snapshot.backend,
        worker_id: snapshot.worker_id,
        now_utc: Utc::now(),
        agent_runtime_version: snapshot.agent_runtime_version,
        tool_registry_count: snapshot.tool_registry_count,
        last_agent_turn_latency_ms: snapshot.last_agent_turn_latency_ms,
        last_agent_turn_at_utc: snapshot.last_agent_turn_at_utc,
        active_profile_id: snapshot.active_profile_id,
        active_profile_display_name: snapshot.active_profile_display_name,
    }
}

fn build_revision() -> &'static str {
    option_env!("MEDOUSA_BUILD_REVISION")
        .or(option_env!("GITHUB_SHA"))
        .unwrap_or(env!("CARGO_PKG_VERSION"))
}

fn base_schema_revision() -> u32 {
    #[cfg(feature = "full-daemon")]
    {
        crate::runtime::stasis_surreal_schema::DAEMON_PERSISTENCE_SCHEMA_REVISION
    }
    #[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
    {
        crate::stasis_surreal_schema::DAEMON_PERSISTENCE_SCHEMA_REVISION
    }
}
