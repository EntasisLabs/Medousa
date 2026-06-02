//! Phase 2: host turn routing (inline vs delegate) and bus activation policy.

use super::policy::TurnWorkerIntent;
use crate::agent_runtime::turn_services::TurnActivationDecision;

/// Max tool rounds for the host when the bus is active (coordinator only).
pub const HOST_BUS_MAX_TOOL_ROUNDS: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostBusEnvMode {
    /// Slim host only when heuristic says delegate.
    Auto,
    /// Always slim host on tool turns.
    Force,
    /// Full tool registry on host (spawn tools still available).
    Off,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostTurnRoute {
    HandleInline,
    Delegate {
        intent: TurnWorkerIntent,
        reason: &'static str,
    },
}

impl HostTurnRoute {
    pub fn suggested_worker_intent(&self) -> Option<TurnWorkerIntent> {
        match self {
            Self::HandleInline => None,
            Self::Delegate { intent, .. } => Some(*intent),
        }
    }

    pub fn route_label(&self) -> &'static str {
        match self {
            Self::HandleInline => "handle_inline",
            Self::Delegate { intent, .. } => match intent {
                TurnWorkerIntent::MemoryAvecCalibrate => "delegate:memory.avec_calibrate",
                TurnWorkerIntent::MemoryContext => "delegate:memory.context",
                TurnWorkerIntent::Research => "delegate:research",
                TurnWorkerIntent::General => "delegate:general",
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct HostTurnProfile {
    pub env_mode: HostBusEnvMode,
    pub route: HostTurnRoute,
    pub host_bus_active: bool,
    pub host_max_tool_rounds: usize,
}

pub fn host_bus_env_mode() -> HostBusEnvMode {
    match std::env::var("MEDOUSA_TURN_HOST_BUS")
        .ok()
        .map(|v| v.trim().to_ascii_lowercase())
        .as_deref()
    {
        None | Some("") | Some("auto") => HostBusEnvMode::Auto,
        Some("1") | Some("true") | Some("yes") | Some("on") | Some("force") => HostBusEnvMode::Force,
        Some("0") | Some("false") | Some("off") | Some("no") => HostBusEnvMode::Off,
        Some(_) => HostBusEnvMode::Auto,
    }
}

/// Back-compat: force-only check (prefer [`host_bus_env_mode`] + [`resolve_host_turn_profile`]).
pub fn host_bus_force_enabled() -> bool {
    matches!(host_bus_env_mode(), HostBusEnvMode::Force)
}

fn prompt_has_research_intent(lower: &str) -> bool {
    let memory_only = ["memory", "locus", "avec", "calibrat", "mood", "recall"]
        .iter()
        .any(|n| lower.contains(n));
    let research = [
        "websearch",
        "web search",
        "search the web",
        "look up",
        "lookup",
        "fetch",
        "http",
        "scrape",
        "current",
        "latest",
        "news",
        "today",
        "evidence",
        "verify online",
    ]
    .iter()
    .any(|n| lower.contains(n));
    (research && !memory_only)
        || [
            "grapheme",
            "run script",
            "modules.search",
            "capability_invoke",
        ]
        .iter()
        .any(|n| lower.contains(n))
}

fn prompt_has_avec_calibrate_ritual(lower: &str) -> bool {
    let avec_posture = ["avec", "mood", "focused", "focus", "calibrat"]
        .iter()
        .any(|n| lower.contains(n));
    let pull_ritual =
        lower.contains("pull") && (lower.contains("preset") || lower.contains("focused") || lower.contains("avec"));
    (avec_posture && lower.contains("calibrat")) || pull_ritual
}

fn prompt_has_memory_context_intent(lower: &str) -> bool {
    ["memory", "locus", "recall", "context", "pull"]
        .iter()
        .any(|n| lower.contains(n))
        && !lower.contains("calibrat")
}

/// Code-first host routing (Phase 2). LLM classifier may extend later.
pub fn classify_host_turn_route_heuristic(prompt: &str) -> HostTurnRoute {
    let lower = prompt.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return HostTurnRoute::HandleInline;
    }

    if prompt_has_research_intent(&lower) {
        return HostTurnRoute::Delegate {
            intent: TurnWorkerIntent::Research,
            reason: "research_or_web_intent",
        };
    }

    if prompt_has_avec_calibrate_ritual(&lower) {
        return HostTurnRoute::Delegate {
            intent: TurnWorkerIntent::MemoryAvecCalibrate,
            reason: "avec_calibrate_ritual",
        };
    }

    if prompt_has_memory_context_intent(&lower) {
        return HostTurnRoute::Delegate {
            intent: TurnWorkerIntent::MemoryContext,
            reason: "memory_context_intent",
        };
    }

    HostTurnRoute::HandleInline
}

pub fn resolve_host_bus_active(env_mode: HostBusEnvMode, route: &HostTurnRoute) -> bool {
    match env_mode {
        HostBusEnvMode::Off => false,
        HostBusEnvMode::Force => true,
        HostBusEnvMode::Auto => matches!(route, HostTurnRoute::Delegate { .. }),
    }
}

pub fn resolve_host_turn_profile(prompt: &str, configured_max_tool_rounds: usize) -> HostTurnProfile {
    let env_mode = host_bus_env_mode();
    let route = classify_host_turn_route_heuristic(prompt);
    let host_bus_active = resolve_host_bus_active(env_mode, &route);
    let host_max_tool_rounds = if host_bus_active {
        configured_max_tool_rounds
            .min(HOST_BUS_MAX_TOOL_ROUNDS)
            .max(1)
    } else {
        configured_max_tool_rounds
    };

    HostTurnProfile {
        env_mode,
        route,
        host_bus_active,
        host_max_tool_rounds,
    }
}

pub fn apply_host_profile_to_activation(
    mut activation: TurnActivationDecision,
    profile: &HostTurnProfile,
) -> TurnActivationDecision {
    if profile.host_bus_active {
        activation.max_tool_rounds = profile.host_max_tool_rounds;
        if activation.enforce_no_tools {
            activation.enforce_no_tools = false;
            activation.tool_call_mode = stasis::application::orchestration::tool_loop_pipeline::ToolCallMode::Auto;
            activation.reason = "host_bus_delegate_requires_spawn";
        }
    }
    activation
}

pub fn host_route_notice(profile: &HostTurnProfile) -> String {
    let route = profile.route.route_label();
    let intent = profile
        .route
        .suggested_worker_intent()
        .map(|i| i.as_str())
        .unwrap_or("n/a");
    let reason = match &profile.route {
        HostTurnRoute::HandleInline => "inline",
        HostTurnRoute::Delegate { reason, .. } => reason,
    };
    format!(
        "◈ host_route route={route} intent={intent} host_bus={} env={:?} reason={reason}",
        profile.host_bus_active, profile.env_mode
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_avec_calibrate() {
        let route = classify_host_turn_route_heuristic("pull focused AVEC and calibrate my posture");
        assert!(matches!(
            route,
            HostTurnRoute::Delegate {
                intent: TurnWorkerIntent::MemoryAvecCalibrate,
                ..
            }
        ));
    }

    #[test]
    fn routes_research() {
        let route = classify_host_turn_route_heuristic("search the web for latest rust release");
        assert!(matches!(
            route,
            HostTurnRoute::Delegate {
                intent: TurnWorkerIntent::Research,
                ..
            }
        ));
    }

    #[test]
    fn auto_enables_bus_only_on_delegate() {
        let profile = resolve_host_turn_profile("hello there", 10);
        assert!(!profile.host_bus_active);
        let profile = resolve_host_turn_profile("calibrate my focused avec", 10);
        assert!(profile.host_bus_active);
    }
}
