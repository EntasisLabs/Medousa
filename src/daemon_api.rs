use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const DEFAULT_DAEMON_BIND: &str = "127.0.0.1:7419";
pub const DEFAULT_DAEMON_URL: &str = "http://127.0.0.1:7419";

pub fn resolve_daemon_url(explicit: Option<&str>) -> String {
    explicit
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| std::env::var("MEDOUSA_DAEMON_URL").ok())
        .or_else(|| std::env::var("STASIS_DAEMON_URL").ok())
        .unwrap_or_else(|| DEFAULT_DAEMON_URL.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub backend: String,
    pub worker_id: String,
    pub now_utc: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnqueueAskRequest {
    pub prompt: String,
    pub policy_profile: Option<String>,
    pub model_hint: Option<String>,
    pub max_turns: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnqueuePromptRequest {
    pub prompt: String,
    pub system_prompt: Option<String>,
    pub policy_profile: Option<String>,
    pub model_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnqueueResponse {
    pub job_id: String,
    pub queue: String,
    pub accepted_at_utc: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterRecurringPromptRequest {
    pub id: Option<String>,
    pub queue: Option<String>,
    pub prompt: String,
    pub system_prompt: Option<String>,
    pub cron_expr: String,
    pub timezone: Option<String>,
    pub jitter_seconds: Option<i64>,
    pub enabled: Option<bool>,
    pub max_attempts: Option<u32>,
    pub policy_profile: Option<String>,
    pub model_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterRecurringResponse {
    pub recurring_id: String,
    pub queue: String,
    pub next_run_at_utc: DateTime<Utc>,
    pub cron_expr: String,
    pub timezone: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonStatsResponse {
    pub enqueued_jobs: usize,
    pub running_jobs: usize,
    pub succeeded_jobs: usize,
    pub failed_jobs: usize,
    pub dead_letter_jobs: usize,
    pub pending_outbox_events: usize,
    pub recurring_definitions: usize,
    pub last_tick_at_utc: Option<DateTime<Utc>>,
}
