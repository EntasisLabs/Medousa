//! Workspace projector — single writer, materialized read model, coalesced refresh.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use once_cell::sync::OnceCell;
use stasis::application::runtime::runtime_factory::RuntimeComposition;
use tokio::sync::{mpsc, oneshot, watch};

use crate::daemon_api::WorkCard;
use crate::workspace::card::{counts_by_column, project_workspace_items, ProjectedWorkItem};
use crate::workspace::event::event_for_column_transition;
use crate::workspace::store::workspace_store;

const DEBOUNCE_MS: u64 = 150;
const PERIODIC_REFRESH_MS: u64 = 1000;

static HUB: OnceCell<WorkspaceHub> = OnceCell::new();

/// Materialized board view — cheap to clone via `Arc` for HTTP and SSE.
#[derive(Debug, Clone)]
pub struct WorkspaceReadSnapshot {
    pub revision: u64,
    pub items: Arc<HashMap<String, ProjectedWorkItem>>,
    pub cards: Arc<Vec<WorkCard>>,
    pub counts_by_column: HashMap<String, u32>,
}

impl WorkspaceReadSnapshot {
    fn empty() -> Self {
        Self {
            revision: workspace_store().revision(),
            items: Arc::new(HashMap::new()),
            cards: Arc::new(Vec::new()),
            counts_by_column: HashMap::new(),
        }
    }
}

enum RefreshRequest {
    Invalidate,
    Immediate(oneshot::Sender<()>),
}

pub struct WorkspaceHub {
    composition: Arc<RuntimeComposition>,
    snapshot_tx: watch::Sender<Arc<WorkspaceReadSnapshot>>,
    refresh_tx: mpsc::Sender<RefreshRequest>,
}

/// Start the global workspace hub (daemon bootstrap). Idempotent.
pub fn init_workspace_hub(composition: Arc<RuntimeComposition>) {
    let _ = HUB.get_or_init(|| WorkspaceHub::spawn(composition));
}

pub fn workspace_hub() -> Option<&'static WorkspaceHub> {
    HUB.get()
}

impl WorkspaceHub {
    fn spawn(composition: Arc<RuntimeComposition>) -> Self {
        let initial = Arc::new(WorkspaceReadSnapshot::empty());
        let (snapshot_tx, _) = watch::channel(initial);
        let (refresh_tx, refresh_rx) = mpsc::channel(256);

        let worker_composition = composition.clone();
        let worker_snapshot_tx = snapshot_tx.clone();
        tokio::spawn(async move {
            run_projector_loop(worker_composition, worker_snapshot_tx, refresh_rx).await;
        });

        let hub = Self {
            composition,
            snapshot_tx,
            refresh_tx,
        };

        hub
    }

    pub fn composition(&self) -> &Arc<RuntimeComposition> {
        &self.composition
    }

    pub fn snapshot(&self) -> Arc<WorkspaceReadSnapshot> {
        self.snapshot_tx.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<Arc<WorkspaceReadSnapshot>> {
        self.snapshot_tx.subscribe()
    }

    /// Coalesce into the projector loop (non-blocking).
    pub fn trigger_refresh(&self) {
        let _ = self.refresh_tx.try_send(RefreshRequest::Invalidate);
    }

    /// Wait for a fresh projection (mutations, card lookup miss).
    pub async fn refresh_now(&self) {
        let (tx, rx) = oneshot::channel();
        if self
            .refresh_tx
            .send(RefreshRequest::Immediate(tx))
            .await
            .is_err()
        {
            return;
        }
        let _ = rx.await;
    }

    pub fn card_detail(&self, card_id: &str) -> Option<ProjectedWorkItem> {
        self.snapshot().items.get(card_id).cloned()
    }

    pub fn projected_items(&self, include_terminal: bool) -> Vec<ProjectedWorkItem> {
        self.snapshot()
            .items
            .values()
            .filter(|item| include_terminal || !item.detail.terminal)
            .cloned()
            .collect()
    }
}

async fn run_projector_loop(
    composition: Arc<RuntimeComposition>,
    snapshot_tx: watch::Sender<Arc<WorkspaceReadSnapshot>>,
    mut refresh_rx: mpsc::Receiver<RefreshRequest>,
) {
    let mut periodic = tokio::time::interval(Duration::from_millis(PERIODIC_REFRESH_MS));
    periodic.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let _ = periodic.tick().await;

    loop {
        tokio::select! {
            request = refresh_rx.recv() => {
                match request {
                    Some(RefreshRequest::Immediate(done)) => {
                        run_projection(&composition, &snapshot_tx).await;
                        let _ = done.send(());
                    }
                    Some(RefreshRequest::Invalidate) => {
                        coalesce_and_project(&composition, &snapshot_tx, &mut refresh_rx).await;
                    }
                    None => break,
                }
            }
            _ = periodic.tick() => {
                run_projection(&composition, &snapshot_tx).await;
            }
        }
    }
}

async fn coalesce_and_project(
    composition: &Arc<RuntimeComposition>,
    snapshot_tx: &watch::Sender<Arc<WorkspaceReadSnapshot>>,
    refresh_rx: &mut mpsc::Receiver<RefreshRequest>,
) {
    tokio::time::sleep(Duration::from_millis(DEBOUNCE_MS)).await;
    while let Ok(next) = refresh_rx.try_recv() {
        if let RefreshRequest::Immediate(done) = next {
            run_projection(composition, snapshot_tx).await;
            let _ = done.send(());
            return;
        }
    }
    run_projection(composition, snapshot_tx).await;
}

async fn run_projection(
    composition: &Arc<RuntimeComposition>,
    snapshot_tx: &watch::Sender<Arc<WorkspaceReadSnapshot>>,
) {
    let Ok(items) = project_workspace_items(composition.as_ref(), true).await else {
        return;
    };
    apply_projection_to_store(&items);

    let cards: Vec<WorkCard> = items.iter().map(|item| item.card.clone()).collect();
    let mut by_id = HashMap::with_capacity(items.len());
    for item in items {
        by_id.insert(item.card.id.0.clone(), item);
    }

    let snapshot = Arc::new(WorkspaceReadSnapshot {
        revision: workspace_store().revision(),
        items: Arc::new(by_id),
        cards: Arc::new(cards.clone()),
        counts_by_column: counts_by_column(&cards),
    });
    let _ = snapshot_tx.send(snapshot);
}

pub(crate) fn apply_projection_to_store(items: &[ProjectedWorkItem]) {
    let store = workspace_store();
    let active_ids = items
        .iter()
        .map(|item| item.card.id.0.clone())
        .collect::<HashSet<_>>();

    for item in items {
        let previous = store.previous_column(&item.card.id.0);
        if previous != Some(item.card.column) {
            if let Some(event) =
                event_for_column_transition(&item.detail, previous, item.card.column)
            {
                store.append_event(event);
            }
            store.remember_column(&item.card.id.0, item.card.column);
        }
    }

    let stale_states: Vec<String> = store
        .card_states_snapshot()
        .into_keys()
        .filter(|card_id| !active_ids.contains(card_id))
        .collect();
    for card_id in stale_states {
        store.prune_card_state(&card_id);
    }

    let cutoff = Utc::now() - ChronoDuration::days(7);
    store.prune_feed_older_than(cutoff);
}
