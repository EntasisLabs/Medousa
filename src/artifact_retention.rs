//! Operator settings + Stasis recurring schedule for artifact index maintenance.

use chrono::Utc;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use stasis::domain::runtime::recurring::RecurringDefinition;
use stasis::prelude::{Result as StasisResult, RuntimeComposition, StasisError};
use std::path::PathBuf;
use std::sync::RwLock;

use crate::artifact_maintenance_job::{
    ARTIFACT_MAINTENANCE_JOB_TYPE, ArtifactMaintenanceJobPayload, DEFAULT_MAX_AGE_DAYS,
    DEFAULT_MAX_PER_SESSION,
};
use crate::daemon_api::{
    ArtifactRetentionSettingsResponse, ArtifactRetentionStatusResponse,
    UpdateArtifactRetentionRequest, UpdateArtifactRetentionResponse,
};
use crate::recurring_handlers;
use crate::recurring_schedule::RecurringScheduleSpec;
use crate::runtime_composition_ext::RuntimeCompositionExt;

pub const ARTIFACT_MAINTENANCE_RECURRING_ID: &str = "system-artifact-maintenance";
const ARTIFACT_MAINTENANCE_QUEUE: &str = "maintenance";
/// Weekly Sunday 03:00 UTC — sec min hour dom month dow year
pub const DEFAULT_MAINTENANCE_CRON: &str = "0 0 3 * * Sun *";
static SETTINGS_PATH: Lazy<RwLock<Option<PathBuf>>> = Lazy::new(|| RwLock::new(None));

pub fn configure_artifact_retention_settings_path(path: impl Into<PathBuf>) -> Result<(), String> {
    *SETTINGS_PATH
        .write()
        .map_err(|_| "artifact retention settings path lock poisoned".to_string())? =
        Some(path.into());
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRetentionSettings {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_max_age_days")]
    pub max_age_days: i64,
    #[serde(default = "default_max_per_session")]
    pub max_per_session: usize,
}

fn default_enabled() -> bool {
    true
}

fn default_max_age_days() -> i64 {
    DEFAULT_MAX_AGE_DAYS
}

fn default_max_per_session() -> usize {
    DEFAULT_MAX_PER_SESSION
}

impl Default for ArtifactRetentionSettings {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            max_age_days: default_max_age_days(),
            max_per_session: default_max_per_session(),
        }
    }
}

fn settings_path() -> std::path::PathBuf {
    if let Some(path) = SETTINGS_PATH
        .read()
        .ok()
        .and_then(|path| path.as_ref().cloned())
    {
        return path;
    }
    crate::session::medousa_data_dir().join("artifact_retention.json")
}

pub fn load_settings() -> ArtifactRetentionSettings {
    let path = settings_path();
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn save_settings(settings: &ArtifactRetentionSettings) -> Result<(), String> {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let raw = serde_json::to_string_pretty(settings).map_err(|err| err.to_string())?;
    std::fs::write(path, raw).map_err(|err| err.to_string())
}

pub async fn sync_recurring_schedule(
    runtime: &RuntimeComposition,
    settings: &ArtifactRetentionSettings,
) -> StasisResult<RecurringDefinition> {
    let payload =
        ArtifactMaintenanceJobPayload::new(settings.max_age_days, settings.max_per_session);
    let payload_template_ref = payload.to_payload_ref()?;

    let now = Utc::now();
    let existing =
        recurring_handlers::get_recurring_definition(runtime, ARTIFACT_MAINTENANCE_RECURRING_ID)
            .await?;

    let mut definition = if let Some(existing) = existing {
        existing
    } else {
        RecurringScheduleSpec::new(
            ARTIFACT_MAINTENANCE_RECURRING_ID,
            ARTIFACT_MAINTENANCE_QUEUE,
            ARTIFACT_MAINTENANCE_JOB_TYPE,
            payload_template_ref.clone(),
            DEFAULT_MAINTENANCE_CRON,
            "UTC",
        )
        .enabled(settings.enabled)
        .max_attempts(1)
        .build(now)?
    };

    definition.job_type = ARTIFACT_MAINTENANCE_JOB_TYPE.to_string();
    definition.queue = ARTIFACT_MAINTENANCE_QUEUE.to_string();
    definition.payload_template_ref = payload_template_ref;
    definition.enabled = settings.enabled;
    // This is a system-owned schedule rather than an operator-defined cron.
    // Reasserting the canonical value also repairs the legacy eight-field
    // expression that could have been persisted by older builds.
    definition.cron_expr = DEFAULT_MAINTENANCE_CRON.to_string();
    definition.timezone = "UTC".to_string();
    definition.next_run_at = definition.compute_next_run_at(now)?;

    runtime.register_recurring(definition.clone()).await?;
    Ok(definition)
}

#[cfg(test)]
mod tests {
    use super::DEFAULT_MAINTENANCE_CRON;

    #[test]
    fn default_maintenance_cron_is_a_valid_weekly_schedule() {
        crate::recurring_delivery::validate_recurring_cron(DEFAULT_MAINTENANCE_CRON, "UTC")
            .expect("artifact maintenance cron must remain valid");
    }
}

pub async fn ensure_schedule_on_startup(runtime: &RuntimeComposition) -> StasisResult<()> {
    let settings = load_settings();
    if !settings.enabled {
        return Ok(());
    }
    let _ = sync_recurring_schedule(runtime, &settings).await?;
    Ok(())
}

pub async fn get_status(
    runtime: &RuntimeComposition,
) -> StasisResult<ArtifactRetentionStatusResponse> {
    let settings = load_settings();
    let definition =
        recurring_handlers::get_recurring_definition(runtime, ARTIFACT_MAINTENANCE_RECURRING_ID)
            .await?;

    let mut last_run_summary = None;
    let mut last_run_at_utc = None;
    if definition.is_some() {
        let runs = recurring_handlers::list_recurring_runs(
            runtime,
            ARTIFACT_MAINTENANCE_RECURRING_ID,
            crate::daemon_api::RecurringRunsQuery { limit: Some(1) },
        )
        .await?;
        if let Some(run) = runs.runs.first() {
            last_run_at_utc = Some(run.updated_at_utc);
            last_run_summary = run.output_text.clone().or(run.latest_outcome.clone());
        }
    }

    Ok(ArtifactRetentionStatusResponse {
        settings: ArtifactRetentionSettingsResponse {
            enabled: settings.enabled,
            max_age_days: settings.max_age_days,
            max_per_session: settings.max_per_session,
            recurring_id: ARTIFACT_MAINTENANCE_RECURRING_ID.to_string(),
            cron_expr: definition
                .as_ref()
                .map(|def| def.cron_expr.clone())
                .unwrap_or_else(|| DEFAULT_MAINTENANCE_CRON.to_string()),
        },
        scheduled: definition.is_some(),
        enabled: definition.as_ref().is_some_and(|def| def.enabled),
        next_run_at_utc: definition.as_ref().map(|def| def.next_run_at),
        last_run_at_utc,
        last_run_summary,
    })
}

pub async fn update_settings(
    runtime: &RuntimeComposition,
    request: UpdateArtifactRetentionRequest,
) -> StasisResult<UpdateArtifactRetentionResponse> {
    let mut settings = load_settings();
    if let Some(enabled) = request.enabled {
        settings.enabled = enabled;
    }
    if let Some(max_age_days) = request.max_age_days {
        settings.max_age_days = max_age_days.clamp(1, 3650);
    }
    if let Some(max_per_session) = request.max_per_session {
        settings.max_per_session = max_per_session.clamp(1, 10_000);
    }

    save_settings(&settings).map_err(StasisError::PortFailure)?;
    let definition = sync_recurring_schedule(runtime, &settings).await?;

    Ok(UpdateArtifactRetentionResponse {
        settings: ArtifactRetentionSettingsResponse {
            enabled: settings.enabled,
            max_age_days: settings.max_age_days,
            max_per_session: settings.max_per_session,
            recurring_id: ARTIFACT_MAINTENANCE_RECURRING_ID.to_string(),
            cron_expr: definition.cron_expr,
        },
        next_run_at_utc: definition.next_run_at,
    })
}
