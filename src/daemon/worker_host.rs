use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use futures_util::FutureExt;
use stasis::prelude::{RuntimeComposition, RuntimeSdk};
use tokio::sync::watch;
use tokio::task::JoinSet;

use crate::daemon::heartbeat::{
    SchedulerTickSideEffects, safe_materialize_recurring_now, safe_process_once,
    safe_publish_pending_events,
};
use crate::product_config::RuntimeWorkerConfig;

pub const AGENT_QUEUE: &str = "agents";
pub const SCHEDULED_QUEUE: &str = "default";
pub const MAINTENANCE_QUEUE: &str = "maintenance";

pub(crate) const IDLE_BACKOFF_MIN: Duration = Duration::from_millis(500);
pub(crate) const IDLE_BACKOFF_MAX: Duration = Duration::from_secs(10);
const ERROR_BACKOFF: Duration = Duration::from_secs(1);
const DELIVERY_BATCH_SIZE: usize = 200;

#[derive(Clone, Copy, Debug)]
enum SlotKind {
    Agent,
    Scheduled,
    Delivery,
    Maintenance,
    Flexible,
}

impl SlotKind {
    fn label(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Scheduled => "scheduled",
            Self::Delivery => "delivery",
            Self::Maintenance => "maintenance",
            Self::Flexible => "flexible",
        }
    }

    fn queues(self) -> [&'static str; 3] {
        match self {
            Self::Agent => [AGENT_QUEUE, SCHEDULED_QUEUE, MAINTENANCE_QUEUE],
            Self::Scheduled => [SCHEDULED_QUEUE, AGENT_QUEUE, MAINTENANCE_QUEUE],
            Self::Maintenance => [MAINTENANCE_QUEUE, AGENT_QUEUE, SCHEDULED_QUEUE],
            Self::Delivery | Self::Flexible => [AGENT_QUEUE, SCHEDULED_QUEUE, MAINTENANCE_QUEUE],
        }
    }
}

pub async fn run_worker_host(
    runtime: RuntimeComposition,
    worker_id: String,
    config: RuntimeWorkerConfig,
    mut shutdown_rx: watch::Receiver<bool>,
    side_effects: Arc<dyn SchedulerTickSideEffects>,
) {
    let slots = slot_kinds(&config);
    let mut workers = JoinSet::new();
    for (index, kind) in slots.iter().copied().enumerate() {
        spawn_slot(
            &mut workers,
            runtime.clone(),
            &worker_id,
            index,
            kind,
            shutdown_rx.clone(),
            side_effects.clone(),
        );
    }

    tracing::info!(
        max_in_flight = config.max_in_flight,
        agents = config.agents,
        scheduled = config.scheduled,
        delivery = config.delivery,
        maintenance = config.maintenance,
        "runtime worker host started"
    );

    loop {
        tokio::select! {
            changed = shutdown_rx.changed() => {
                if changed.is_err() || *shutdown_rx.borrow() {
                    workers.abort_all();
                    while workers.join_next().await.is_some() {}
                    return;
                }
            }
            completed = workers.join_next() => {
                let Some(completed) = completed else { return; };
                match completed {
                    Ok((index, kind)) => {
                        if !*shutdown_rx.borrow() {
                            tracing::warn!(slot = index, kind = kind.label(), "runtime worker exited; restarting");
                            spawn_slot(&mut workers, runtime.clone(), &worker_id, index, kind, shutdown_rx.clone(), side_effects.clone());
                        }
                    }
                    Err(err) => {
                        tracing::error!(error = %err, "runtime worker task failed; restoring flexible capacity");
                        let index = config.max_in_flight + workers.len();
                        spawn_slot(&mut workers, runtime.clone(), &worker_id, index, SlotKind::Flexible, shutdown_rx.clone(), side_effects.clone());
                    }
                }
            }
        }
    }
}

pub async fn run_materializer_loop(
    runtime: RuntimeComposition,
    scheduler_id: String,
    interval: Duration,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    let sdk = RuntimeSdk::new(runtime);
    loop {
        match safe_materialize_recurring_now(&sdk, &scheduler_id).await {
            Ok(count) if count > 0 => tracing::info!(count, "materialized recurring jobs"),
            Ok(_) => {}
            Err(err) => {
                crate::observability::rate_limited_error("scheduler.materialize_error", || {
                    format!("recurring materialization failed: {err}")
                })
            }
        }
        if wait_or_shutdown(interval, &mut shutdown_rx).await {
            return;
        }
    }
}

fn slot_kinds(config: &RuntimeWorkerConfig) -> Vec<SlotKind> {
    let mut slots = Vec::with_capacity(config.max_in_flight);
    slots.extend(std::iter::repeat_n(SlotKind::Agent, config.agents));
    slots.extend(std::iter::repeat_n(SlotKind::Scheduled, config.scheduled));
    slots.extend(std::iter::repeat_n(SlotKind::Delivery, config.delivery));
    slots.extend(std::iter::repeat_n(
        SlotKind::Maintenance,
        config.maintenance,
    ));
    slots.resize(config.max_in_flight, SlotKind::Flexible);
    slots
}

fn spawn_slot(
    workers: &mut JoinSet<(usize, SlotKind)>,
    runtime: RuntimeComposition,
    worker_id: &str,
    index: usize,
    kind: SlotKind,
    shutdown_rx: watch::Receiver<bool>,
    side_effects: Arc<dyn SchedulerTickSideEffects>,
) {
    let worker_id = format!("{worker_id}:{}:{index}", kind.label());
    workers.spawn(async move {
        let result = std::panic::AssertUnwindSafe(run_slot(
            runtime,
            worker_id,
            kind,
            shutdown_rx,
            side_effects,
        ))
        .catch_unwind()
        .await;
        if result.is_err() {
            tracing::error!(slot = index, kind = kind.label(), "runtime worker panicked");
        }
        (index, kind)
    });
}

async fn run_slot(
    runtime: RuntimeComposition,
    worker_id: String,
    kind: SlotKind,
    mut shutdown_rx: watch::Receiver<bool>,
    side_effects: Arc<dyn SchedulerTickSideEffects>,
) {
    let sdk = RuntimeSdk::new(runtime);
    let mut idle_backoff = IDLE_BACKOFF_MIN;
    loop {
        let result = run_one(&sdk, &worker_id, kind).await;
        let delay = match result {
            Ok(SlotOutcome::Processed(job_id)) => {
                side_effects.on_processed_job(&job_id).await;
                idle_backoff = IDLE_BACKOFF_MIN;
                Duration::ZERO
            }
            Ok(SlotOutcome::Activity) => {
                idle_backoff = IDLE_BACKOFF_MIN;
                Duration::ZERO
            }
            Ok(SlotOutcome::Idle) => {
                let delay = idle_backoff;
                idle_backoff = (idle_backoff * 2).min(IDLE_BACKOFF_MAX);
                delay
            }
            Err(err) => {
                crate::observability::rate_limited_error("worker_host.slot_error", || {
                    format!("runtime worker {worker_id} failed: {err}")
                });
                ERROR_BACKOFF
            }
        };
        if wait_or_shutdown(delay, &mut shutdown_rx).await {
            return;
        }
    }
}

enum SlotOutcome {
    Idle,
    Activity,
    Processed(String),
}

async fn run_one(sdk: &RuntimeSdk, worker_id: &str, kind: SlotKind) -> Result<SlotOutcome> {
    if matches!(kind, SlotKind::Delivery) {
        let published = safe_publish_pending_events(sdk, DELIVERY_BATCH_SIZE).await?;
        if published > 0 {
            return Ok(SlotOutcome::Activity);
        }
    }
    for queue in kind.queues() {
        if let Some(job_id) = safe_process_once(sdk, queue, worker_id).await? {
            return Ok(SlotOutcome::Processed(job_id));
        }
    }
    Ok(SlotOutcome::Idle)
}

async fn wait_or_shutdown(delay: Duration, shutdown_rx: &mut watch::Receiver<bool>) -> bool {
    if *shutdown_rx.borrow() {
        return true;
    }
    tokio::select! {
        _ = tokio::time::sleep(delay) => false,
        changed = shutdown_rx.changed() => changed.is_err() || *shutdown_rx.borrow(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_slots_match_capacity_and_lane_shares() {
        let config = RuntimeWorkerConfig::default();
        let slots = slot_kinds(&config);
        assert_eq!(slots.len(), 8);
        assert_eq!(
            slots
                .iter()
                .filter(|slot| matches!(slot, SlotKind::Agent))
                .count(),
            2
        );
        assert_eq!(
            slots
                .iter()
                .filter(|slot| matches!(slot, SlotKind::Scheduled))
                .count(),
            2
        );
        assert_eq!(
            slots
                .iter()
                .filter(|slot| matches!(slot, SlotKind::Delivery))
                .count(),
            1
        );
        assert_eq!(
            slots
                .iter()
                .filter(|slot| matches!(slot, SlotKind::Maintenance))
                .count(),
            1
        );
        assert_eq!(
            slots
                .iter()
                .filter(|slot| matches!(slot, SlotKind::Flexible))
                .count(),
            2
        );
        assert_eq!(IDLE_BACKOFF_MIN, Duration::from_millis(500));
        assert_eq!(IDLE_BACKOFF_MAX, Duration::from_secs(10));
    }
}
