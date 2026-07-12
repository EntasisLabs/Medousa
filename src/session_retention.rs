//! Optional retention pass — purge stale Locus raw tier + runtime terminal records.

use std::sync::Arc;
use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use stasis::ports::outbound::memory::memory_models::{
    MemoryEvictMode, MemoryEvictRequest, MemoryFilter, MemoryScope,
};
use stasis::ports::outbound::memory::memory_operations::MemoryOperations;
use stasis::prelude::RuntimeComposition;

#[derive(Debug, Clone)]
pub struct SessionRetentionConfig {
    /// Delete Locus `raw` tier nodes older than this many days (0 = disabled).
    pub locus_raw_max_age_days: u32,
    /// Prune terminal Stasis jobs/attempts/outbox older than this many days (0 = disabled).
    pub runtime_terminal_max_age_days: u32,
}

impl SessionRetentionConfig {
    pub fn from_env() -> Self {
        Self {
            locus_raw_max_age_days: parse_env_u32("MEDOUSA_LOCUS_RAW_RETENTION_DAYS", 0),
            runtime_terminal_max_age_days: parse_env_u32("MEDOUSA_RUNTIME_TERMINAL_RETENTION_DAYS", 0),
        }
    }

    pub fn enabled(&self) -> bool {
        self.locus_raw_max_age_days > 0 || self.runtime_terminal_max_age_days > 0
    }
}

fn parse_env_u32(key: &str, default: u32) -> u32 {
    std::env::var(key)
        .ok()
        .and_then(|raw| raw.trim().parse().ok())
        .unwrap_or(default)
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct SessionRetentionReport {
    pub locus_raw_deleted: usize,
    pub runtime_jobs_pruned: usize,
    pub runtime_attempts_pruned: usize,
    pub runtime_outbox_pruned: usize,
}

pub async fn run_retention_pass(
    config: &SessionRetentionConfig,
    memory_operations: Option<Arc<dyn MemoryOperations>>,
    composition: &RuntimeComposition,
) -> SessionRetentionReport {
    let mut report = SessionRetentionReport::default();
    if !config.enabled() {
        return report;
    }

    if config.locus_raw_max_age_days > 0
        && let Some(ops) = memory_operations {
            let cutoff = Utc::now() - ChronoDuration::days(i64::from(config.locus_raw_max_age_days));
            if let Ok(response) = ops
                .evict(&MemoryEvictRequest {
                    mode: MemoryEvictMode::ByFilter,
                    scope: MemoryScope {
                        tiers: Some(vec!["raw".to_string()]),
                        to_utc: Some(cutoff),
                        ..Default::default()
                    },
                    filter: MemoryFilter::default(),
                    dry_run: false,
                    force: false,
                    max_nodes: 10_000,
                    ..Default::default()
                })
                .await
            {
                report.locus_raw_deleted = response.deleted;
            }
        }

    if config.runtime_terminal_max_age_days > 0 {
        let cutoff = Utc::now() - ChronoDuration::days(i64::from(config.runtime_terminal_max_age_days));
        match composition {
            RuntimeComposition::Surreal(rt) => {
                if let Ok(summary) = rt.prune_terminal_records(cutoff).await {
                    report.runtime_jobs_pruned = summary.jobs_pruned;
                    report.runtime_attempts_pruned = summary.attempts_pruned;
                    report.runtime_outbox_pruned = summary.outbox_events_pruned;
                }
            }
            RuntimeComposition::InMemory(rt) => {
                if let Ok(summary) = rt.prune_terminal_records(cutoff).await {
                    report.runtime_jobs_pruned = summary.jobs_pruned;
                    report.runtime_attempts_pruned = summary.attempts_pruned;
                    report.runtime_outbox_pruned = summary.outbox_events_pruned;
                }
            }
        }
    }

    report
}

/// Background tick interval when retention env is enabled.
pub fn retention_tick_interval() -> Duration {
    Duration::from_secs(6 * 60 * 60)
}
