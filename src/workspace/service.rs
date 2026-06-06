//! Workspace orchestration — sync runtime projections, feed, snapshot.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{Duration, Utc};
use stasis::application::runtime::runtime_factory::RuntimeComposition;

use crate::daemon_api::{
    WorkCardDetail, WorkspaceCardsQuery, WorkspaceCardsResponse, WorkspaceFeedQuery,
    WorkspaceFeedResponse, WorkspaceSnapshot, WorkspaceSnapshotQuery,
};
use crate::workspace::card::{
    counts_by_column, parse_column_filter, project_workspace_items, ProjectedWorkItem,
};
use crate::workspace::event::{event_for_column_transition, filter_events_by_card};
use crate::workspace::store::workspace_store;

pub struct WorkspaceService;

impl WorkspaceService {
    pub async fn sync_runtime(runtime: &RuntimeComposition, include_terminal: bool) {
        let Ok(items) = project_workspace_items(runtime, include_terminal).await else {
            return;
        };
        let store = workspace_store();
        let active_ids = items
            .iter()
            .map(|item| item.card.id.0.clone())
            .collect::<std::collections::HashSet<_>>();

        for item in &items {
            let previous = store.previous_column(&item.card.id.0);
            if previous != Some(item.card.column) {
                if let Some(event) =
                    event_for_column_transition(&item.card, previous, item.card.column)
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

        let cutoff = Utc::now() - Duration::days(7);
        store.prune_feed_older_than(cutoff);
    }

    pub async fn list_cards(
        runtime: Arc<RuntimeComposition>,
        query: &WorkspaceCardsQuery,
    ) -> anyhow::Result<WorkspaceCardsResponse> {
        let include_terminal = query.include_terminal.unwrap_or(false);
        Self::sync_runtime(runtime.as_ref(), include_terminal).await;

        let limit = query.limit.unwrap_or(50).clamp(1, 200);
        let mut items = project_workspace_items(runtime.as_ref(), include_terminal).await?;
        items = apply_card_filters(items, query);
        items.truncate(limit);

        Ok(WorkspaceCardsResponse {
            workspace_revision: workspace_store().revision(),
            cards: items.into_iter().map(|item| item.card).collect(),
        })
    }

    pub async fn get_card_detail(
        runtime: Arc<RuntimeComposition>,
        card_id: &str,
    ) -> anyhow::Result<Option<WorkCardDetail>> {
        Self::sync_runtime(runtime.as_ref(), true).await;
        let items = project_workspace_items(runtime.as_ref(), true).await?;
        Ok(items
            .into_iter()
            .find(|item| item.card.id.0 == card_id)
            .map(|item| item.detail))
    }

    pub async fn list_feed(
        runtime: Arc<RuntimeComposition>,
        query: &WorkspaceFeedQuery,
    ) -> anyhow::Result<WorkspaceFeedResponse> {
        Self::sync_runtime(runtime.as_ref(), true).await;
        let limit = query.limit.unwrap_or(50).clamp(1, 200);
        let store = workspace_store();

        let mut events = store.list_feed(
            query.since_id.as_deref(),
            query.since_revision,
            limit,
        );

        if let Some(card_id) = query.card_id.as_deref() {
            events = filter_events_by_card(&events, card_id)
                .into_iter()
                .cloned()
                .collect();
        }

        Ok(WorkspaceFeedResponse {
            workspace_revision: store.revision(),
            events,
        })
    }

    pub async fn snapshot(
        runtime: Arc<RuntimeComposition>,
        query: &WorkspaceSnapshotQuery,
    ) -> anyhow::Result<WorkspaceSnapshot> {
        let feed_tail_limit = query.feed_tail_limit.unwrap_or(20).clamp(1, 100);
        Self::sync_runtime(runtime.as_ref(), true).await;

        let items = project_workspace_items(runtime.as_ref(), true).await?;
        let cards = items.into_iter().map(|item| item.card).collect::<Vec<_>>();
        let revision = workspace_store().revision();

        if query.since_revision.is_some_and(|since| since >= revision) {
            return Ok(WorkspaceSnapshot {
                workspace_revision: revision,
                server_time_utc: Utc::now(),
                cards: Vec::new(),
                counts_by_column: HashMap::new(),
                feed_tail: Vec::new(),
            });
        }

        Ok(WorkspaceSnapshot {
            workspace_revision: revision,
            server_time_utc: Utc::now(),
            cards: cards.clone(),
            counts_by_column: counts_by_column(&cards),
            feed_tail: workspace_store().feed_tail(feed_tail_limit),
        })
    }
}

fn apply_card_filters(
    items: Vec<ProjectedWorkItem>,
    query: &WorkspaceCardsQuery,
) -> Vec<ProjectedWorkItem> {
    let mut filtered = items;

    if let Some(session_id) = query.session_id.as_deref().map(str::trim).filter(|v| !v.is_empty())
    {
        filtered.retain(|item| {
            item.detail
                .session_id
                .as_deref()
                .is_some_and(|id| id == session_id)
        });
    }

    if let Some(column_raw) = query.column.as_deref() {
        if let Some(column) = parse_column_filter(column_raw) {
            filtered.retain(|item| item.card.column == column);
        }
    }

    filtered
}
