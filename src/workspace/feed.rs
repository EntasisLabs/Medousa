//! Workspace SSE stream — diffs from materialized read model (no per-tick rescan).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use stasis::application::runtime::runtime_factory::RuntimeComposition;
use tokio::sync::mpsc;

use crate::daemon_api::{
    WorkCard, WorkspaceSnapshot, WorkspaceSnapshotQuery, WorkspaceStreamEvent,
    WorkspaceStreamQuery,
};
use crate::workspace::card::counts_by_column;
use crate::workspace::projector::{init_workspace_hub, workspace_hub, WorkspaceReadSnapshot};
use crate::workspace::service::WorkspaceService;
use crate::workspace::store::workspace_store;

const HEARTBEAT_SECS: u64 = 30;

pub fn spawn_workspace_stream(
    composition: Arc<RuntimeComposition>,
    query: WorkspaceStreamQuery,
) -> mpsc::Receiver<WorkspaceStreamEvent> {
    let (tx, rx) = mpsc::channel(128);
    tokio::spawn(async move {
        if workspace_hub().is_none() {
            init_workspace_hub(composition.clone());
        }

        let initial = match initial_snapshot_event(&composition, &query).await {
            Ok(event) => event,
            Err(_) => stream_error_event(),
        };
        if tx.send(initial).await.is_err() {
            return;
        }

        let Some(hub) = workspace_hub() else {
            return;
        };

        let mut snapshot_rx = hub.subscribe();
        let mut last_revision = workspace_store().revision();
        let mut last_feed_index = workspace_store().feed_len();
        let mut last_cards = card_map_from_snapshot(&snapshot_rx.borrow(), query.session_id.as_deref());

        let mut heartbeat =
            tokio::time::interval(Duration::from_secs(HEARTBEAT_SECS));
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let _ = heartbeat.tick().await;

        loop {
            tokio::select! {
                changed = snapshot_rx.changed() => {
                    if changed.is_err() {
                        return;
                    }
                    let snap = snapshot_rx.borrow().clone();
                    if emit_snapshot_delta(
                        &tx,
                        &snap,
                        query.session_id.as_deref(),
                        &mut last_revision,
                        &mut last_feed_index,
                        &mut last_cards,
                    )
                    .await
                    .is_err()
                    {
                        return;
                    }
                }
                _ = heartbeat.tick() => {
                    let frame = WorkspaceStreamEvent {
                        workspace_revision: workspace_store().revision(),
                        stream_event_type: "heartbeat".to_string(),
                        emitted_at_utc: Utc::now(),
                        card: None,
                        feed_event: None,
                        counts: None,
                        snapshot: None,
                    };
                    if tx.send(frame).await.is_err() {
                        return;
                    }
                }
            }
        }
    });

    rx
}

async fn initial_snapshot_event(
    composition: &Arc<RuntimeComposition>,
    query: &WorkspaceStreamQuery,
) -> anyhow::Result<WorkspaceStreamEvent> {
    let snapshot = WorkspaceService::snapshot(
        composition.clone(),
        &WorkspaceSnapshotQuery {
            since_revision: query.since_revision,
            feed_tail_limit: query.feed_tail_limit.or(Some(20)),
        },
    )
    .await?;

    let filtered = filter_snapshot_cards(&snapshot, query.session_id.as_deref());

    Ok(WorkspaceStreamEvent {
        workspace_revision: filtered.workspace_revision,
        stream_event_type: "snapshot".to_string(),
        emitted_at_utc: Utc::now(),
        card: None,
        feed_event: None,
        counts: Some(filtered.counts_by_column.clone()),
        snapshot: Some(filtered),
    })
}

fn filter_snapshot_cards(
    snapshot: &WorkspaceSnapshot,
    session_id: Option<&str>,
) -> WorkspaceSnapshot {
    let Some(session_id) = session_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return snapshot.clone();
    };

    if let Some(hub) = workspace_hub() {
        let snap = hub.snapshot();
        let cards: Vec<WorkCard> = snap
            .items
            .values()
            .filter(|item| hub_session_matches(&item.card.id.0, session_id))
            .map(|item| item.card.clone())
            .collect();
        return WorkspaceSnapshot {
            workspace_revision: snapshot.workspace_revision,
            server_time_utc: snapshot.server_time_utc,
            counts_by_column: counts_by_column(&cards),
            feed_tail: snapshot.feed_tail.clone(),
            cards,
        };
    }

    snapshot.clone()
}

fn hub_session_matches(card_id: &str, session_id: &str) -> bool {
    workspace_hub()
        .and_then(|hub| hub.card_detail(card_id))
        .and_then(|item| item.detail.session_id)
        .is_some_and(|id| id == session_id)
}

fn card_map_from_snapshot(
    snapshot: &Arc<WorkspaceReadSnapshot>,
    session_id: Option<&str>,
) -> HashMap<String, WorkCard> {
    let mut map = HashMap::new();
    for item in snapshot.items.values() {
        if let Some(session_id) = session_id.map(str::trim).filter(|value| !value.is_empty()) {
            if !item
                .detail
                .session_id
                .as_deref()
                .is_some_and(|id| id == session_id)
            {
                continue;
            }
        }
        map.insert(item.card.id.0.clone(), item.card.clone());
    }
    map
}

async fn emit_snapshot_delta(
    tx: &mpsc::Sender<WorkspaceStreamEvent>,
    snapshot: &Arc<WorkspaceReadSnapshot>,
    session_id: Option<&str>,
    last_revision: &mut u64,
    last_feed_index: &mut usize,
    last_cards: &mut HashMap<String, WorkCard>,
) -> Result<(), mpsc::error::SendError<WorkspaceStreamEvent>> {
    let revision = snapshot.revision;
    let feed_index = workspace_store().feed_len();

    if feed_index > *last_feed_index {
        for event in workspace_store().feed_events_from(*last_feed_index) {
            let frame = WorkspaceStreamEvent {
                workspace_revision: revision,
                stream_event_type: "feed_appended".to_string(),
                emitted_at_utc: Utc::now(),
                card: None,
                feed_event: Some(event),
                counts: None,
                snapshot: None,
            };
            tx.send(frame).await?;
        }
        *last_feed_index = feed_index;
    }

    let cards = card_map_from_snapshot(snapshot, session_id);

    for (card_id, card) in &cards {
        match last_cards.get(card_id) {
            None => send_card_upserted(tx, revision, card).await?,
            Some(previous) if previous != card => send_card_upserted(tx, revision, card).await?,
            _ => {}
        }
    }

    for card_id in last_cards.keys() {
        if !cards.contains_key(card_id) {
            let frame = WorkspaceStreamEvent {
                workspace_revision: revision,
                stream_event_type: "card_removed".to_string(),
                emitted_at_utc: Utc::now(),
                card: Some(WorkCard {
                    id: crate::daemon_api::WorkCardId(card_id.clone()),
                    column: last_cards[card_id].column,
                    title: last_cards[card_id].title.clone(),
                    status_label: last_cards[card_id].status_label.clone(),
                    created_at_utc: last_cards[card_id].created_at_utc,
                    updated_at_utc: Utc::now(),
                }),
                feed_event: None,
                counts: None,
                snapshot: None,
            };
            tx.send(frame).await?;
        }
    }

    if revision != *last_revision {
        let counts = counts_by_column(&cards.values().cloned().collect::<Vec<_>>());
        let frame = WorkspaceStreamEvent {
            workspace_revision: revision,
            stream_event_type: "column_counts".to_string(),
            emitted_at_utc: Utc::now(),
            card: None,
            feed_event: None,
            counts: Some(counts),
            snapshot: None,
        };
        tx.send(frame).await?;
        *last_revision = revision;
    }

    *last_cards = cards;
    Ok(())
}

async fn send_card_upserted(
    tx: &mpsc::Sender<WorkspaceStreamEvent>,
    revision: u64,
    card: &WorkCard,
) -> Result<(), mpsc::error::SendError<WorkspaceStreamEvent>> {
    tx.send(WorkspaceStreamEvent {
        workspace_revision: revision,
        stream_event_type: "card_upserted".to_string(),
        emitted_at_utc: Utc::now(),
        card: Some(card.clone()),
        feed_event: None,
        counts: None,
        snapshot: None,
    })
    .await
}

fn stream_error_event() -> WorkspaceStreamEvent {
    WorkspaceStreamEvent {
        workspace_revision: workspace_store().revision(),
        stream_event_type: "error".to_string(),
        emitted_at_utc: Utc::now(),
        card: None,
        feed_event: None,
        counts: None,
        snapshot: None,
    }
}
