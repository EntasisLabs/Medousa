//! Stasis OpenTelemetry wiring driven by TUI defaults (`stasis_otel_enabled`).
//!
//! Stasis treats an unset `STASIS_OTEL_ENABLED` as enabled; Medousa defaults to off and
//! sets the variable explicitly so runtime behavior matches the settings toggle.

use stasis::prelude::StasisRuntimeBuilder;

use crate::session::{self, TuiDefaults};

const ENV_STASIS_OTEL_ENABLED: &str = "STASIS_OTEL_ENABLED";

/// Whether the user opted in via `tui_defaults.json` (default: off).
pub fn stasis_otel_enabled_from_defaults(defaults: &TuiDefaults) -> bool {
    defaults.stasis_otel_enabled.unwrap_or(false)
}

/// Apply the persisted TUI preference to the process environment.
pub fn apply_stasis_otel_user_preference(enabled: bool) {
    // SAFETY: Medousa sets process-global env for Stasis builder construction, same as TUI env overrides.
    unsafe {
        std::env::set_var(ENV_STASIS_OTEL_ENABLED, if enabled { "true" } else { "false" });
    }
}

/// Load `tui_defaults.json` and apply the OTEL master switch.
pub fn prepare_stasis_otel_from_tui_defaults() {
    apply_stasis_otel_from_defaults(&session::load_tui_defaults());
}

pub fn apply_stasis_otel_from_defaults(defaults: &TuiDefaults) {
    apply_stasis_otel_user_preference(stasis_otel_enabled_from_defaults(defaults));
}

/// Attach OTLP telemetry to a Stasis builder when the master switch is on.
pub fn attach_otel_to_builder(builder: StasisRuntimeBuilder) -> anyhow::Result<StasisRuntimeBuilder> {
    if !stasis_otel_enabled() {
        return Ok(builder);
    }
    builder
        .with_otel_from_env()
        .map_err(|err| anyhow::anyhow!("{err}"))
}

fn stasis_otel_enabled() -> bool {
    stasis::infrastructure::telemetry::otel::otel_enabled()
}

/// Short observability hint when OTEL is active (after preference was applied).
pub fn stasis_otel_obs_summary() -> Option<String> {
    if !stasis_otel_enabled() {
        return None;
    }
    let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "http://localhost:4317 (SDK default)".to_string());
    let service = std::env::var("STASIS_OTEL_SERVICE_NAME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            std::env::var("OTEL_SERVICE_NAME")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or_else(|| "medousa".to_string());
    Some(format!(
        "stasis OpenTelemetry on (service={service}, endpoint={endpoint}) — restart daemon if it was already running"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_preference_forces_master_switch_off_by_default() {
        let key = ENV_STASIS_OTEL_ENABLED;
        let prior = std::env::var(key).ok();
        apply_stasis_otel_user_preference(false);
        assert!(!stasis_otel_enabled());
        apply_stasis_otel_user_preference(true);
        assert!(stasis_otel_enabled());
        // SAFETY: test-local env restore.
        unsafe {
            match prior {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }

    #[test]
    fn defaults_opt_in_is_off() {
        assert!(!stasis_otel_enabled_from_defaults(&TuiDefaults::default()));
        let mut defaults = TuiDefaults::default();
        defaults.stasis_otel_enabled = Some(true);
        assert!(stasis_otel_enabled_from_defaults(&defaults));
    }
}
