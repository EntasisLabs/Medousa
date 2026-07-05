//! Stasis job handler: prune stale HTML artifact index rows and unreferenced payloads.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;
use stasis::application::runtime::in_memory_runtime::{JobExecutionOutcome, JobHandler};
use stasis::domain::runtime::job::Job;
use stasis::prelude::{Result as StasisResult, RuntimeComposition, StasisError};

pub const ARTIFACT_MAINTENANCE_JOB_TYPE: &str = "workflow.medousa.artifact_maintenance";

pub const DEFAULT_MAX_AGE_DAYS: i64 = 90;
pub const DEFAULT_MAX_PER_SESSION: usize = 200;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactMaintenanceJobPayload {
    #[serde(default = "default_max_per_session")]
    pub max_per_session: usize,
    #[serde(default = "default_max_age_days")]
    pub max_age_days: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

fn default_max_per_session() -> usize {
    DEFAULT_MAX_PER_SESSION
}

fn default_max_age_days() -> i64 {
    DEFAULT_MAX_AGE_DAYS
}

impl ArtifactMaintenanceJobPayload {
    pub fn new(max_age_days: i64, max_per_session: usize) -> Self {
        Self {
            max_per_session,
            max_age_days,
            display_name: Some("Presentation cleanup".to_string()),
        }
    }

    pub fn to_payload_ref(&self) -> StasisResult<String> {
        serde_json::to_string(self).map_err(|err| {
            StasisError::PortFailure(format!("failed to encode artifact maintenance payload: {err}"))
        })
    }
}

pub async fn register_artifact_maintenance_handler(
    composition: &RuntimeComposition,
) -> anyhow::Result<()> {
    let handler = ArtifactMaintenanceJobHandler;
    match composition {
        RuntimeComposition::InMemory(rt) => rt.register_handler(handler)?,
        RuntimeComposition::Surreal(rt) => rt.register_handler(handler)?,
    }
    Ok(())
}

struct ArtifactMaintenanceJobHandler;

#[async_trait]
impl JobHandler for ArtifactMaintenanceJobHandler {
    fn job_type(&self) -> &'static str {
        ARTIFACT_MAINTENANCE_JOB_TYPE
    }

    async fn execute(&self, job: &Job) -> StasisResult<JobExecutionOutcome> {
        let payload: ArtifactMaintenanceJobPayload =
            serde_json::from_str(&job.payload_ref).map_err(|err| {
                StasisError::PortFailure(format!(
                    "invalid artifact maintenance payload for job {}: {err}",
                    job.id
                ))
            })?;

        let max_per_session = payload.max_per_session.clamp(1, 10_000);
        let max_age_days = payload.max_age_days.clamp(1, 3650);

        let report = tokio::task::spawn_blocking(move || {
            crate::artifact_store::run_artifact_maintenance(max_per_session, max_age_days)
        })
        .await
        .map_err(|err| StasisError::PortFailure(format!("artifact maintenance join error: {err}")))?
        .map_err(|err| StasisError::PortFailure(err))?;

        let diagnostics = json!({
            "records_before": report.records_before,
            "records_after": report.records_after,
            "missing_payload_pruned": report.missing_payload_pruned,
            "deduped_records_pruned": report.deduped_records_pruned,
            "retention_pruned": report.retention_pruned,
            "payload_files_deleted": report.payload_files_deleted,
            "max_age_days": max_age_days,
            "max_per_session": max_per_session,
        })
        .to_string();

        Ok(JobExecutionOutcome::Success {
            sttp_output_node_id: format!("sttp:out:artifact-maintenance:{}", job.id),
            execution_id: Some(job.id.clone()),
            diagnostics: Some(diagnostics),
        })
    }
}
