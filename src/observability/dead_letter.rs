//! Bounded dead-letter queue enforcement for the Stasis runtime job store.

use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{DateTime, Utc};
use stasis::application::runtime::runtime_factory::RuntimeComposition;
use stasis::domain::runtime::job::{Job, JobState};
use stasis::ports::outbound::runtime::job_store::JobStore;

/// Default cap on concurrently retained dead-letter jobs (override with `MEDOUSA_DEAD_LETTER_CAP`).
pub const DEFAULT_DEAD_LETTER_CAP: usize = 512;

static CAP_HITS: AtomicU64 = AtomicU64::new(0);
static JOBS_PRUNED: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DeadLetterCapMetrics {
    pub cap_hits: u64,
    pub jobs_pruned: u64,
}

pub fn dead_letter_cap_metrics() -> DeadLetterCapMetrics {
    DeadLetterCapMetrics {
        cap_hits: CAP_HITS.load(Ordering::Relaxed),
        jobs_pruned: JOBS_PRUNED.load(Ordering::Relaxed),
    }
}

pub fn dead_letter_cap() -> usize {
    std::env::var("MEDOUSA_DEAD_LETTER_CAP")
        .ok()
        .and_then(|raw| raw.trim().parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_DEAD_LETTER_CAP)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeadLetterCapReport {
    pub before: usize,
    pub after: usize,
    pub pruned: usize,
    pub cap: usize,
}

/// When dead-letter jobs exceed the cap, prune the oldest terminal records so the
/// pile cannot grow without bound. Uses `JobStore::prune_terminal_before` with a
/// cutoff derived from the oldest excess dead letters (also removes other terminal
/// jobs finished before that cutoff — acceptable hygiene for stale terminal rows).
pub async fn enforce_dead_letter_cap(
    composition: &RuntimeComposition,
) -> anyhow::Result<DeadLetterCapReport> {
    let cap = dead_letter_cap();
    let mut jobs = list_dead_letter_jobs(composition).await?;
    let before = jobs.len();
    if before <= cap {
        return Ok(DeadLetterCapReport {
            before,
            after: before,
            pruned: 0,
            cap,
        });
    }

    CAP_HITS.fetch_add(1, Ordering::Relaxed);

    jobs.sort_by_key(job_sort_key);
    let excess = before.saturating_sub(cap);
    let cutoff = job_sort_key(&jobs[excess - 1]);
    let pruned = prune_terminal_before(composition, cutoff).await?;
    JOBS_PRUNED.fetch_add(pruned as u64, Ordering::Relaxed);

    let after = list_dead_letter_jobs(composition).await?.len();
    Ok(DeadLetterCapReport {
        before,
        after,
        pruned,
        cap,
    })
}

async fn list_dead_letter_jobs(composition: &RuntimeComposition) -> anyhow::Result<Vec<Job>> {
    match composition {
        RuntimeComposition::InMemory(rt) => Ok(rt.job_store.list_by_state(JobState::DeadLetter).await?),
        RuntimeComposition::Surreal(rt) => Ok(rt.job_store.list_by_state(JobState::DeadLetter).await?),
    }
}

async fn prune_terminal_before(
    composition: &RuntimeComposition,
    cutoff: DateTime<Utc>,
) -> anyhow::Result<usize> {
    match composition {
        RuntimeComposition::InMemory(rt) => {
            Ok(rt.job_store.prune_terminal_before(cutoff).await?)
        }
        RuntimeComposition::Surreal(rt) => Ok(rt.job_store.prune_terminal_before(cutoff).await?),
    }
}

fn job_sort_key(job: &Job) -> DateTime<Utc> {
    job.finished_at.unwrap_or(job.scheduled_at)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_cap_is_reasonable() {
        assert_eq!(dead_letter_cap(), DEFAULT_DEAD_LETTER_CAP);
    }
}
