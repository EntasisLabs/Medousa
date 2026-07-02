//! Workspace projector — incremental updates + periodic full reconcile.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use once_cell::sync::OnceCell;
use stasis::application::runtime::runtime_factory::RuntimeComposition;
use tokio::sync::{mpsc, oneshot, watch};

use crate::daemon_api::WorkCard;
use crate::workspace::card::{counts_by_column, project_workspace_items, ProjectedWorkItem};
use crate::workspace::domain_event::WorkspaceDomainEvent;
use crate::workspace::event::event_for_column_transition;
use crate::workspace::incremental::{domain_event_for_kind, project_domain_event};
use crate::workspace::store::workspace_store;

const EVENT_DEBOUNCE_MS: u64 = 50;
const INVALIDATE_DEBOUNCE_MS: u64 = 150;
const FULL_RECONCILE_SECS: u64 = 60;

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

enum ProjectorRequest {
    Event(WorkspaceDomainEvent),
    Invalidate,
    ReconcileNow(oneshot::Sender<()>),
    FullRebuild(oneshot::Sender<()>),
}

pub struct WorkspaceHub {
    composition: Arc<RuntimeComposition>,
    snapshot_tx: watch::Sender<Arc<WorkspaceReadSnapshot>>,
    request_tx: mpsc::Sender<ProjectorRequest>,
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
        let (request_tx, request_rx) = mpsc::channel(512);

        let worker_composition = composition.clone();
        let worker_snapshot_tx = snapshot_tx.clone();
        tokio::spawn(async move {
            run_projector_loop(worker_composition, worker_snapshot_tx, request_rx).await;
        });

        Self {
            composition,
            snapshot_tx,
            request_tx,
        }
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

    pub fn notify_event(&self, event: WorkspaceDomainEvent) {
        let _ = self
            .request_tx
            .try_send(ProjectorRequest::Event(event));
    }

    pub fn trigger_invalidate(&self) {
        let _ = self.request_tx.try_send(ProjectorRequest::Invalidate);
    }

    /// Back-compat alias for generic invalidation.
    pub fn trigger_refresh(&self) {
        self.trigger_invalidate();
    }

    pub async fn reconcile_now(&self) {
        let (tx, rx) = oneshot::channel();
        if self
            .request_tx
            .send(ProjectorRequest::ReconcileNow(tx))
            .await
            .is_err()
        {
            return;
        }
        let _ = rx.await;
    }

    pub async fn rebuild_full(&self) {
        let (tx, rx) = oneshot::channel();
        if self
            .request_tx
            .send(ProjectorRequest::FullRebuild(tx))
            .await
            .is_err()
        {
            return;
        }
        let _ = rx.await;
    }

    pub async fn refresh_now(&self) {
        self.rebuild_full().await;
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
    mut request_rx: mpsc::Receiver<ProjectorRequest>,
) {
    let mut full_reconcile =
        tokio::time::interval(Duration::from_secs(FULL_RECONCILE_SECS));
    full_reconcile.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let _ = full_reconcile.tick().await;

    loop {
        tokio::select! {
            request = request_rx.recv() => {
                let Some(request) = request else { break };
                match request {
                    ProjectorRequest::Event(event) => {
                        coalesce_events(&composition, &snapshot_tx, &mut request_rx, event).await;
                    }
                    ProjectorRequest::Invalidate => {
                        coalesce_invalidate(&composition, &snapshot_tx, &mut request_rx).await;
                    }
                    ProjectorRequest::ReconcileNow(done) => {
                        reconcile_visible_cards(&composition, &snapshot_tx).await;
                        let _ = done.send(());
                    }
                    ProjectorRequest::FullRebuild(done) => {
                        run_full_projection(&composition, &snapshot_tx).await;
                        let _ = done.send(());
                    }
                }
            }
            _ = full_reconcile.tick() => {
                run_full_projection(&composition, &snapshot_tx).await;
            }
        }
    }
}

async fn coalesce_events(
    composition: &Arc<RuntimeComposition>,
    snapshot_tx: &watch::Sender<Arc<WorkspaceReadSnapshot>>,
    request_rx: &mut mpsc::Receiver<ProjectorRequest>,
    first: WorkspaceDomainEvent,
) {
    let mut batch = HashSet::from([first]);
    let debounce = Duration::from_millis(EVENT_DEBOUNCE_MS);
    let deadline = tokio::time::Instant::now() + debounce;

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        tokio::select! {
            message = request_rx.recv() => {
                match message {
                    Some(ProjectorRequest::Event(event)) => {
                        batch.insert(event);
                    }
                    Some(ProjectorRequest::FullRebuild(done)) => {
                        apply_incremental_batch(composition, snapshot_tx, batch).await;
                        run_full_projection(composition, snapshot_tx).await;
                        let _ = done.send(());
                        return;
                    }
                    Some(ProjectorRequest::ReconcileNow(done)) => {
                        apply_incremental_batch(composition, snapshot_tx, batch).await;
                        reconcile_visible_cards(composition, snapshot_tx).await;
                        let _ = done.send(());
                        return;
                    }
                    Some(ProjectorRequest::Invalidate) => {
                        apply_incremental_batch(composition, snapshot_tx, batch).await;
                        coalesce_invalidate(composition, snapshot_tx, request_rx).await;
                        return;
                    }
                    None => break,
                }
            }
            _ = tokio::time::sleep(remaining) => break,
        }
    }

    apply_incremental_batch(composition, snapshot_tx, batch).await;
}

async fn coalesce_invalidate(
    composition: &Arc<RuntimeComposition>,
    snapshot_tx: &watch::Sender<Arc<WorkspaceReadSnapshot>>,
    request_rx: &mut mpsc::Receiver<ProjectorRequest>,
) {
    let debounce = Duration::from_millis(INVALIDATE_DEBOUNCE_MS);
    let deadline = tokio::time::Instant::now() + debounce;

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        tokio::select! {
            message = request_rx.recv() => {
                match message {
                    Some(ProjectorRequest::Invalidate) => {}
                    Some(ProjectorRequest::Event(event)) => {
                        apply_incremental_batch(
                            composition,
                            snapshot_tx,
                            HashSet::from([event]),
                        )
                        .await;
                    }
                    Some(ProjectorRequest::FullRebuild(done)) => {
                        run_full_projection(composition, snapshot_tx).await;
                        let _ = done.send(());
                        return;
                    }
                    Some(ProjectorRequest::ReconcileNow(done)) => {
                        reconcile_visible_cards(composition, snapshot_tx).await;
                        let _ = done.send(());
                        return;
                    }
                    None => break,
                }
            }
            _ = tokio::time::sleep(remaining) => break,
        }
    }

    reconcile_visible_cards(composition, snapshot_tx).await;
}

async fn apply_incremental_batch(
    composition: &Arc<RuntimeComposition>,
    snapshot_tx: &watch::Sender<Arc<WorkspaceReadSnapshot>>,
    events: HashSet<WorkspaceDomainEvent>,
) {
    let current = snapshot_tx.borrow().clone();
    let mut map = (*current.items).clone();

    for event in events {
        let Some((card_id, item)) = project_domain_event(composition, &event).await else {
            continue;
        };
        merge_projected(&mut map, &card_id, item);
        if let Some(projected) = map.get(&card_id) {
            apply_item_column_transition(projected);
        } else {
            workspace_store().prune_card_state(&card_id);
        }
    }

    publish_snapshot(snapshot_tx, map);
}

async fn reconcile_visible_cards(
    composition: &Arc<RuntimeComposition>,
    snapshot_tx: &watch::Sender<Arc<WorkspaceReadSnapshot>>,
) {
    let current = snapshot_tx.borrow().clone();
    let events: HashSet<_> = current
        .items
        .iter()
        .map(|(card_id, item)| domain_event_for_kind(card_id, item.detail.kind))
        .collect();
    if events.is_empty() {
        return;
    }
    apply_incremental_batch(composition, snapshot_tx, events).await;
}

async fn run_full_projection(
    composition: &Arc<RuntimeComposition>,
    snapshot_tx: &watch::Sender<Arc<WorkspaceReadSnapshot>>,
) {
    let Ok(items) = project_workspace_items(composition.as_ref(), true).await else {
        return;
    };
    apply_projection_to_store(&items);

    let mut by_id = HashMap::with_capacity(items.len());
    for item in items {
        by_id.insert(item.card.id.0.clone(), item);
    }
    publish_snapshot(snapshot_tx, by_id);
}

fn merge_projected(
    map: &mut HashMap<String, ProjectedWorkItem>,
    card_id: &str,
    item: Option<ProjectedWorkItem>,
) {
    match item {
        Some(projected) => {
            map.insert(card_id.to_string(), projected);
        }
        None => {
            map.remove(card_id);
        }
    }
}

fn publish_snapshot(
    snapshot_tx: &watch::Sender<Arc<WorkspaceReadSnapshot>>,
    map: HashMap<String, ProjectedWorkItem>,
) {
    let mut cards: Vec<WorkCard> = map.values().map(|item| item.card.clone()).collect();
    cards.sort_by(|left, right| right.updated_at_utc.cmp(&left.updated_at_utc));

    let snapshot = Arc::new(WorkspaceReadSnapshot {
        revision: workspace_store().revision(),
        items: Arc::new(map),
        cards: Arc::new(cards.clone()),
        counts_by_column: counts_by_column(&cards),
    });
    let _ = snapshot_tx.send(snapshot.clone());
    crate::home_live_activity::notify_snapshot(&snapshot);
}

fn apply_item_column_transition(item: &ProjectedWorkItem) {
    let store = workspace_store();
    let card_id = &item.card.id.0;
    let previous = store.previous_column(card_id);
    if previous != Some(item.card.column) {
        if let Some(event) =
            event_for_column_transition(&item.detail, previous, item.card.column)
        {
            store.append_event(event);
        }
        crate::home_push::notify_column_transition(
            &item.detail,
            previous,
            item.card.column,
        );
        store.remember_column(card_id, item.card.column);
    }
}

pub(crate) fn apply_projection_to_store(items: &[ProjectedWorkItem]) {
    let store = workspace_store();
    let active_ids = items
        .iter()
        .map(|item| item.card.id.0.clone())
        .collect::<HashSet<_>>();

    for item in items {
        apply_item_column_transition(item);
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
