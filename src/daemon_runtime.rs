//! Shared runtime identity and health response construction.

use chrono::{DateTime, Utc};
use medousa_types::{AuthorityId, DaemonRuntimeDescriptor, HealthResponse};

pub const AGENT_RUNTIME_VERSION: &str = "centralized-v1";

const COMMON_CAPABILITIES: &[&str] = &[
    "daemon.authority",
    "memory.locus",
    "persistence.surrealkv",
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
