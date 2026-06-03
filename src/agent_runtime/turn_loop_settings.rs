//! Operator-configurable tool-loop limits (TUI / daemon settings).

use crate::tui::settings::{RuntimeSettings, parse_usize_with_bounds};

use super::turn_worker::HostBusEnvMode;

pub use crate::tui::settings::{
    OPERATOR_RETRY_LIMIT_MAX as RETRY_LIMIT_MAX, OPERATOR_RETRY_LIMIT_MIN as RETRY_LIMIT_MIN,
    OPERATOR_ROUND_LIMIT_MAX as ROUND_LIMIT_MAX, OPERATOR_ROUND_LIMIT_MIN as ROUND_LIMIT_MIN,
};

pub const DEFAULT_HOST_BUS_MAX_TOOL_ROUNDS: usize = 8;
pub const DEFAULT_ACTIVATION_TOOL_INTENT_MAX_ROUNDS: usize = 12;
pub const DEFAULT_ACTIVATION_SHORT_TURN_MAX_TOOL_ROUNDS: usize = 1;
pub const DEFAULT_CONTINUATION_MAX_TOOL_ROUNDS: usize = 4;
pub const DEFAULT_MAX_TEXT_ONLY_STUCK_CONTINUES: usize = 10;
pub const DEFAULT_CLASSIFIER_RESTRICTED_MAX_TOOL_ROUNDS: usize = 1;

/// Parsed tool-loop policy from [`RuntimeSettings`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TurnLoopSettings {
    pub configured_max_tool_rounds: usize,
    pub host_bus_max_tool_rounds: usize,
    pub host_bus_env_mode: HostBusEnvMode,
    pub activation_tool_intent_max_rounds: usize,
    pub activation_short_turn_max_tool_rounds: usize,
    pub continuation_max_tool_rounds: usize,
    pub max_text_only_stuck_continues: usize,
    pub classifier_restricted_max_tool_rounds: usize,
}

impl TurnLoopSettings {
    pub fn from_runtime_settings(settings: &RuntimeSettings) -> Self {
        let mut settings = settings.clone();
        apply_turn_loop_field_defaults(&mut settings);
        let configured_max_tool_rounds = parse_usize_with_bounds(
            &settings.max_tool_rounds,
            10,
            ROUND_LIMIT_MIN,
            ROUND_LIMIT_MAX,
        );
        Self {
            configured_max_tool_rounds,
            host_bus_max_tool_rounds: parse_usize_with_bounds(
                &settings.host_bus_max_tool_rounds,
                DEFAULT_HOST_BUS_MAX_TOOL_ROUNDS,
                ROUND_LIMIT_MIN,
                ROUND_LIMIT_MAX,
            ),
            host_bus_env_mode: parse_host_turn_bus_mode(&settings.host_turn_bus_mode),
            activation_tool_intent_max_rounds: parse_usize_with_bounds(
                &settings.activation_tool_intent_max_rounds,
                DEFAULT_ACTIVATION_TOOL_INTENT_MAX_ROUNDS,
                ROUND_LIMIT_MIN,
                ROUND_LIMIT_MAX,
            ),
            activation_short_turn_max_tool_rounds: parse_usize_with_bounds(
                &settings.activation_short_turn_max_tool_rounds,
                DEFAULT_ACTIVATION_SHORT_TURN_MAX_TOOL_ROUNDS,
                ROUND_LIMIT_MIN,
                ROUND_LIMIT_MAX,
            ),
            continuation_max_tool_rounds: parse_usize_with_bounds(
                &settings.continuation_max_tool_rounds,
                DEFAULT_CONTINUATION_MAX_TOOL_ROUNDS,
                ROUND_LIMIT_MIN,
                ROUND_LIMIT_MAX,
            ),
            max_text_only_stuck_continues: parse_usize_with_bounds(
                &settings.max_text_only_stuck_continues,
                DEFAULT_MAX_TEXT_ONLY_STUCK_CONTINUES,
                ROUND_LIMIT_MIN,
                ROUND_LIMIT_MAX,
            ),
            classifier_restricted_max_tool_rounds: parse_usize_with_bounds(
                &settings.classifier_restricted_max_tool_rounds,
                DEFAULT_CLASSIFIER_RESTRICTED_MAX_TOOL_ROUNDS,
                ROUND_LIMIT_MIN,
                ROUND_LIMIT_MAX,
            ),
        }
    }

    /// Env vars override TUI when set (ops escape hatch).
    pub fn effective_host_bus_max_tool_rounds(&self) -> usize {
        std::env::var("MEDOUSA_HOST_BUS_MAX_TOOL_ROUNDS")
            .ok()
            .and_then(|raw| raw.trim().parse::<usize>().ok())
            .map(|n| n.clamp(ROUND_LIMIT_MIN, ROUND_LIMIT_MAX))
            .unwrap_or(self.host_bus_max_tool_rounds)
    }

    pub fn effective_host_bus_env_mode(&self) -> HostBusEnvMode {
        match std::env::var("MEDOUSA_TURN_HOST_BUS")
            .ok()
            .map(|v| v.trim().to_ascii_lowercase())
            .as_deref()
        {
            None | Some("") | Some("auto") => self.host_bus_env_mode,
            Some("1") | Some("true") | Some("yes") | Some("on") | Some("force") => {
                HostBusEnvMode::Force
            }
            Some("0") | Some("false") | Some("off") | Some("no") => HostBusEnvMode::Off,
            Some(_) => HostBusEnvMode::Auto,
        }
    }

    pub fn operator_summary(&self) -> String {
        format!(
            "configured_max={} host_bus_cap={} host_bus_mode={:?} tool_intent_cap={} \
             short_turn_cap={} continuation_cap={} text_only_stuck_cap={} classifier_restricted={}",
            self.configured_max_tool_rounds,
            self.effective_host_bus_max_tool_rounds(),
            self.effective_host_bus_env_mode(),
            self.activation_tool_intent_max_rounds,
            self.activation_short_turn_max_tool_rounds,
            self.continuation_max_tool_rounds,
            self.max_text_only_stuck_continues,
            self.classifier_restricted_max_tool_rounds,
        )
    }
}

pub fn parse_host_turn_bus_mode(raw: &str) -> HostBusEnvMode {
    match raw.trim().to_ascii_lowercase().as_str() {
        "off" | "0" | "false" | "no" => HostBusEnvMode::Off,
        "force" | "on" | "1" | "true" | "yes" => HostBusEnvMode::Force,
        _ => HostBusEnvMode::Auto,
    }
}

pub fn default_host_turn_bus_mode_label() -> &'static str {
    "auto"
}

/// Fill turn-loop limit fields on [`RuntimeSettings`] when missing (daemon/TUI defaults).
pub fn apply_turn_loop_field_defaults(settings: &mut RuntimeSettings) {
    if settings.host_bus_max_tool_rounds.trim().is_empty() {
        settings.host_bus_max_tool_rounds = DEFAULT_HOST_BUS_MAX_TOOL_ROUNDS.to_string();
    }
    if settings.host_turn_bus_mode.trim().is_empty() {
        settings.host_turn_bus_mode = default_host_turn_bus_mode_label().to_string();
    }
    if settings.activation_tool_intent_max_rounds.trim().is_empty() {
        settings.activation_tool_intent_max_rounds =
            DEFAULT_ACTIVATION_TOOL_INTENT_MAX_ROUNDS.to_string();
    }
    if settings.activation_short_turn_max_tool_rounds.trim().is_empty() {
        settings.activation_short_turn_max_tool_rounds =
            DEFAULT_ACTIVATION_SHORT_TURN_MAX_TOOL_ROUNDS.to_string();
    }
    if settings.continuation_max_tool_rounds.trim().is_empty() {
        settings.continuation_max_tool_rounds = DEFAULT_CONTINUATION_MAX_TOOL_ROUNDS.to_string();
    }
    if settings.max_text_only_stuck_continues.trim().is_empty() {
        settings.max_text_only_stuck_continues = DEFAULT_MAX_TEXT_ONLY_STUCK_CONTINUES.to_string();
    }
    if settings.classifier_restricted_max_tool_rounds.trim().is_empty() {
        settings.classifier_restricted_max_tool_rounds =
            DEFAULT_CLASSIFIER_RESTRICTED_MAX_TOOL_ROUNDS.to_string();
    }
}
