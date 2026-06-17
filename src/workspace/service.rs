//! Workspace orchestration — read from materialized view, invalidate via projector.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use stasis::application::runtime::runtime_factory::RuntimeComposition;

use crate::daemon_api::{
    WorkCardDetail, WorkspaceCardsQuery, WorkspaceCardsResponse, WorkspaceFeedQuery,
    WorkspaceFeedResponse, WorkspaceSnapshot, WorkspaceSnapshotQuery,
};
use crate::workspace::card::{parse_column_filter, project_workspace_items, ProjectedWorkItem};
use crate::workspace::event::filter_events_by_card;
use crate::workspace::projector::{apply_projection_to_store, workspace_hub};
use crate::workspace::store::workspace_store;

pub struct WorkspaceService;

impl WorkspaceService {
    /// Notify the projector — does not block on a full rescan.
    pub async fn sync_runtime(runtime: &RuntimeComposition, _include_terminal: bool) {
        if let Some(hub) = workspace_hub() {
            hub.trigger_refresh();
            return;
        }
        let _ = legacy_sync_and_project(runtime, true).await;
    }

    pub async fn refresh_now(runtime: &RuntimeComposition) {
        if let Some(hub) = workspace_hub() {
            hub.refresh_now().await;
            return;
        }
        let _ = legacy_sync_and_project(runtime, true).await;
    }

    pub async fn list_cards(
        runtime: Arc<RuntimeComposition>,
        query: &WorkspaceCardsQuery,
    ) -> anyhow::Result<WorkspaceCardsResponse> {
        let include_terminal = query.include_terminal.unwrap_or(false);
        let limit = query.limit.unwrap_or(50).clamp(1, 200);
        let mut items = load_projected_items(runtime.as_ref(), include_terminal).await?;
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
        let card_id = card_id.trim();
        if let Some(hub) = workspace_hub() {
            if let Some(item) = hub.card_detail(card_id) {
                return Ok(Some(item.detail));
            }
            hub.refresh_now().await;
            return Ok(hub.card_detail(card_id).map(|item| item.detail));
        }

        let items = legacy_sync_and_project(runtime.as_ref(), true).await?;
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

        if let Some(hub) = workspace_hub() {
            let snap = hub.snapshot();
            return Ok(WorkspaceSnapshot {
                workspace_revision: snap.revision,
                server_time_utc: Utc::now(),
                cards: snap.cards.as_ref().clone(),
                counts_by_column: snap.counts_by_column.clone(),
                feed_tail: workspace_store().feed_tail(feed_tail_limit),
            });
        }

        let items = legacy_sync_and_project(runtime.as_ref(), true).await?;
        let cards = items.iter().map(|item| item.card.clone()).collect::<Vec<_>>();

        Ok(WorkspaceSnapshot {
            workspace_revision: revision,
            server_time_utc: Utc::now(),
            cards: cards.clone(),
            counts_by_column: crate::workspace::card::counts_by_column(&cards),
            feed_tail: workspace_store().feed_tail(feed_tail_limit),
        })
    }
}

async fn load_projected_items(
    runtime: &RuntimeComposition,
    include_terminal: bool,
) -> anyhow::Result<Vec<ProjectedWorkItem>> {
    if let Some(hub) = workspace_hub() {
        return Ok(hub.projected_items(include_terminal));
    }
    legacy_sync_and_project(runtime, include_terminal).await
}

async fn legacy_sync_and_project(
    runtime: &RuntimeComposition,
    include_terminal: bool,
) -> anyhow::Result<Vec<ProjectedWorkItem>> {
    let items = project_workspace_items(runtime, include_terminal).await?;
    apply_projection_to_store(&items);
    Ok(items)
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
