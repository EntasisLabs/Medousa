//! Work card hide/wipe policy — resolved from `tui_defaults.json` and env overrides.

use chrono::{DateTime, Duration, Utc};

use crate::session::{self, TuiDefaults};

pub const DEFAULT_HIDE_AFTER_HOURS: u32 = 24;
pub const DEFAULT_WIPE_AFTER_DAYS: u32 = 7;
pub const MIN_HIDE_AFTER_HOURS: u32 = 1;
pub const MAX_HIDE_AFTER_HOURS: u32 = 168;
pub const MIN_WIPE_AFTER_DAYS: u32 = 1;
pub const MAX_WIPE_AFTER_DAYS: u32 = 90;

const HIDE_AFTER_HOURS_ENV: &str = "MEDOUSA_WORK_CARD_HIDE_AFTER_HOURS";
const WIPE_AFTER_DAYS_ENV: &str = "MEDOUSA_WORK_CARD_WIPE_AFTER_DAYS";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceRetentionConfig {
    pub hide_after_hours: u32,
    pub wipe_after_days: u32,
}

impl WorkspaceRetentionConfig {
    pub fn from_tui_defaults(defaults: &TuiDefaults) -> Self {
        Self {
            hide_after_hours: resolve_hide_after_hours(defaults),
            wipe_after_days: resolve_wipe_after_days(defaults),
        }
    }

    pub fn load() -> Self {
        Self::from_tui_defaults(&session::load_tui_defaults())
    }

    pub fn hide_ttl(&self) -> Duration {
        Duration::hours(i64::from(self.hide_after_hours))
    }

    pub fn wipe_cutoff(&self, now: DateTime<Utc>) -> DateTime<Utc> {
        now - Duration::days(i64::from(self.wipe_after_days))
    }
}

pub fn resolve_hide_after_hours(defaults: &TuiDefaults) -> u32 {
    if let Ok(raw) = std::env::var(HIDE_AFTER_HOURS_ENV) {
        if let Ok(value) = raw.trim().parse::<u32>() {
            return clamp_hide_after_hours(value);
        }
    }
    defaults
        .work_card_hide_after_hours
        .map(clamp_hide_after_hours)
        .unwrap_or(DEFAULT_HIDE_AFTER_HOURS)
}

pub fn resolve_wipe_after_days(defaults: &TuiDefaults) -> u32 {
    if let Ok(raw) = std::env::var(WIPE_AFTER_DAYS_ENV) {
        if let Ok(value) = raw.trim().parse::<u32>() {
            return clamp_wipe_after_days(value);
        }
    }
    defaults
        .work_card_wipe_after_days
        .map(clamp_wipe_after_days)
        .unwrap_or(DEFAULT_WIPE_AFTER_DAYS)
}

pub fn clamp_hide_after_hours(hours: u32) -> u32 {
    hours.clamp(MIN_HIDE_AFTER_HOURS, MAX_HIDE_AFTER_HOURS)
}

pub fn clamp_wipe_after_days(days: u32) -> u32 {
    days.clamp(MIN_WIPE_AFTER_DAYS, MAX_WIPE_AFTER_DAYS)
}

pub fn terminal_card_stale(at: DateTime<Utc>, hide_ttl: Duration) -> bool {
    Utc::now().signed_duration_since(at) > hide_ttl
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_when_unset() {
        let config = WorkspaceRetentionConfig::from_tui_defaults(&TuiDefaults::default());
        assert_eq!(config.hide_after_hours, DEFAULT_HIDE_AFTER_HOURS);
        assert_eq!(config.wipe_after_days, DEFAULT_WIPE_AFTER_DAYS);
    }

    #[test]
    fn clamps_out_of_range_values() {
        let defaults = TuiDefaults {
            work_card_hide_after_hours: Some(999),
            work_card_wipe_after_days: Some(0),
            ..Default::default()
        };
        let config = WorkspaceRetentionConfig::from_tui_defaults(&defaults);
        assert_eq!(config.hide_after_hours, MAX_HIDE_AFTER_HOURS);
        assert_eq!(config.wipe_after_days, MIN_WIPE_AFTER_DAYS);
    }
}
