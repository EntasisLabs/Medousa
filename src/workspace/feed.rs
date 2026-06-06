//! Workspace SSE stream — live card + feed updates with revision reconciliation.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use stasis::application::runtime::runtime_factory::RuntimeComposition;
use tokio::sync::mpsc;

use crate::daemon_api::{
    WorkCard, WorkspaceSnapshotQuery, WorkspaceStreamEvent, WorkspaceStreamQuery,
};
use crate::workspace::card::{counts_by_column, project_workspace_items};
use crate::workspace::service::WorkspaceService;
use crate::workspace::store::workspace_store;

const STREAM_POLL_MS: u64 = 1000;
const HEARTBEAT_TICKS: u64 = 30;

pub fn spawn_workspace_stream(
    composition: Arc<RuntimeComposition>,
    query: WorkspaceStreamQuery,
) -> mpsc::Receiver<WorkspaceStreamEvent> {
    let (tx, rx) = mpsc::channel(128);
    tokio::spawn(async move {
        let initial = match initial_snapshot_event(&composition, &query).await {
            Ok(event) => event,
            Err(_) => stream_error_event(),
        };
        if tx.send(initial).await.is_err()
        {
            return;
        }

        let mut last_revision = workspace_store().revision();
        let mut last_feed_index = workspace_store().feed_len();
        let mut last_cards = current_card_map(&composition, query.session_id.as_deref()).await;
        let mut tick_count = 0u64;

        let mut interval = tokio::time::interval(Duration::from_millis(STREAM_POLL_MS));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            interval.tick().await;
            WorkspaceService::sync_runtime(composition.as_ref(), true).await;

            let revision = workspace_store().revision();
            let feed_index = workspace_store().feed_len();
            let cards = current_card_map(&composition, query.session_id.as_deref()).await;

            if feed_index > last_feed_index {
                for event in workspace_store().feed_events_from(last_feed_index) {
                    let frame = WorkspaceStreamEvent {
                        workspace_revision: revision,
                        stream_event_type: "feed_appended".to_string(),
                        emitted_at_utc: Utc::now(),
                        card: None,
                        feed_event: Some(event),
                        counts: None,
                        snapshot: None,
                    };
                    if tx.send(frame).await.is_err() {
                        return;
                    }
                }
                last_feed_index = feed_index;
            }

            for (card_id, card) in &cards {
                match last_cards.get(card_id) {
                    None => {
                        if send_card_upserted(&tx, revision, card).await.is_err() {
                            return;
                        }
                    }
                    Some(previous) if previous != card => {
                        if send_card_upserted(&tx, revision, card).await.is_err() {
                            return;
                        }
                    }
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
                    if tx.send(frame).await.is_err() {
                        return;
                    }
                }
            }

            if revision != last_revision {
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
                if tx.send(frame).await.is_err() {
                    return;
                }
                last_revision = revision;
            }

            last_cards = cards;
            tick_count += 1;
            if tick_count >= HEARTBEAT_TICKS {
                tick_count = 0;
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

    Ok(WorkspaceStreamEvent {
        workspace_revision: snapshot.workspace_revision,
        stream_event_type: "snapshot".to_string(),
        emitted_at_utc: Utc::now(),
        card: None,
        feed_event: None,
        counts: Some(snapshot.counts_by_column.clone()),
        snapshot: Some(snapshot),
    })
}

async fn current_card_map(
    composition: &Arc<RuntimeComposition>,
    session_id: Option<&str>,
) -> HashMap<String, WorkCard> {
    let Ok(mut items) = project_workspace_items(composition.as_ref(), true).await else {
        return HashMap::new();
    };

    if let Some(session_id) = session_id.map(str::trim).filter(|value| !value.is_empty()) {
        items.retain(|item| {
            item.detail
                .session_id
                .as_deref()
                .is_some_and(|id| id == session_id)
        });
    }

    items
        .into_iter()
        .map(|item| (item.card.id.0.clone(), item.card))
        .collect()
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
