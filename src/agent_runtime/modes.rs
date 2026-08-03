//! First-class behavioral modes above lanes, specialists, and model routing.

use crate::daemon_api::AgentModeId;

/// Versioned, immutable mode contract resolved at the beginning of a turn.
///
/// Later slices will add compiled context, tool-surface, lane, and completion
/// policies. Keeping the snapshot explicit now prevents request state from
/// drifting during a live tool loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedAgentMode {
    pub id: AgentModeId,
    pub contract_revision: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentModeUnavailable {
    pub requested: AgentModeId,
    pub reason: &'static str,
}

impl std::fmt::Display for AgentModeUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "agent mode '{}' is unavailable: {}",
            self.requested.as_str(),
            self.reason
        )
    }
}

impl std::error::Error for AgentModeUnavailable {}

/// Resolve a request to the contract used for the complete turn.
///
/// Coder is represented in the shared protocol now, but remains explicitly
/// unavailable until its authority, entry context, and tool surface ship.
pub fn resolve_agent_mode(
    requested: AgentModeId,
) -> Result<ResolvedAgentMode, AgentModeUnavailable> {
    match requested {
        AgentModeId::General => Ok(ResolvedAgentMode {
            id: AgentModeId::General,
            contract_revision: "general-v1",
        }),
        AgentModeId::Coder => Err(AgentModeUnavailable {
            requested,
            reason: "Coder entry and repository authority are not installed yet",
        }),
    }
}

/// Apply the resolved mode's stable system policy.
///
/// General intentionally returns the exact existing prompt for first-slice
/// byte parity. Future modes compose a stable Medousa core with an overlay.
pub fn system_prompt_for_mode<'a>(base: &'a str, mode: &ResolvedAgentMode) -> &'a str {
    match mode.id {
        AgentModeId::General => base,
        AgentModeId::Coder => unreachable!("unavailable modes cannot enter a turn"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct RequestModeFixture {
        #[serde(default)]
        agent_mode: AgentModeId,
    }

    #[test]
    fn omitted_request_mode_defaults_to_general() {
        let request: RequestModeFixture = serde_json::from_str("{}").expect("request fixture");
        assert_eq!(request.agent_mode, AgentModeId::General);
        assert_eq!(
            serde_json::to_string(&AgentModeId::Coder).expect("serialize coder"),
            "\"coder\""
        );
    }

    #[test]
    fn general_resolves_to_versioned_snapshot() {
        let mode = resolve_agent_mode(AgentModeId::General).expect("general mode");
        assert_eq!(mode.id, AgentModeId::General);
        assert_eq!(mode.contract_revision, "general-v1");
    }

    #[test]
    fn coder_fails_explicitly_until_entry_contract_ships() {
        let err = resolve_agent_mode(AgentModeId::Coder).expect_err("coder unavailable");
        assert_eq!(err.requested, AgentModeId::Coder);
        assert!(err.to_string().contains("repository authority"));
    }

    #[test]
    fn general_system_prompt_preserves_byte_parity() {
        let mode = resolve_agent_mode(AgentModeId::General).expect("general mode");
        let base = "existing Medousa prompt";
        assert_eq!(system_prompt_for_mode(base, &mode).as_ptr(), base.as_ptr());
        assert_eq!(system_prompt_for_mode(base, &mode), base);
    }
}
