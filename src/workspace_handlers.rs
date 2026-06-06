//! HTTP handlers for workspace APIs (`/v1/workspace/*`).

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use stasis::application::runtime::runtime_factory::RuntimeComposition;

use crate::daemon_api::{
    WorkCardDetail, WorkspaceCardsQuery, WorkspaceCardsResponse, WorkspaceFeedQuery,
    WorkspaceFeedResponse, WorkspaceSnapshot, WorkspaceSnapshotQuery,
};
use crate::workspace::WorkspaceService;

#[derive(Clone)]
pub struct WorkspaceHandlerState {
    pub composition: Arc<RuntimeComposition>,
}

pub async fn list_workspace_cards(
    State(state): State<WorkspaceHandlerState>,
    Query(query): Query<WorkspaceCardsQuery>,
) -> Result<Json<WorkspaceCardsResponse>, (StatusCode, String)> {
    WorkspaceService::list_cards(state.composition, &query)
        .await
        .map(Json)
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))
}

pub async fn get_workspace_card(
    State(state): State<WorkspaceHandlerState>,
    Path(card_id): Path<String>,
) -> Result<Json<WorkCardDetail>, (StatusCode, String)> {
    let card_id = card_id.trim();
    if card_id.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "card_id is required".to_string()));
    }

    match WorkspaceService::get_card_detail(state.composition, card_id).await {
        Ok(Some(detail)) => Ok(Json(detail)),
        Ok(None) => Err((StatusCode::NOT_FOUND, format!("card not found: {card_id}"))),
        Err(err) => Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string())),
    }
}

pub async fn list_workspace_feed(
    State(state): State<WorkspaceHandlerState>,
    Query(query): Query<WorkspaceFeedQuery>,
) -> Result<Json<WorkspaceFeedResponse>, (StatusCode, String)> {
    WorkspaceService::list_feed(state.composition, &query)
        .await
        .map(Json)
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))
}

pub async fn get_workspace_snapshot(
    State(state): State<WorkspaceHandlerState>,
    Query(query): Query<WorkspaceSnapshotQuery>,
) -> Result<Json<WorkspaceSnapshot>, (StatusCode, String)> {
    WorkspaceService::snapshot(state.composition, &query)
        .await
        .map(Json)
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))
}
