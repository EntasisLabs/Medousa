//! HTTP handlers for workspace APIs (`/v1/workspace/*`).

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use futures_util::stream::{self, Stream};
use stasis::application::runtime::runtime_factory::RuntimeComposition;

use crate::daemon_api::{
    WorkCardDetail, WorkspaceCardActionResponse, WorkspaceCardsQuery, WorkspaceCardsResponse,
    WorkspaceFeedQuery, WorkspaceFeedResponse, WorkspaceLinkVaultRequest, WorkspaceRebuildResponse,
    WorkspaceSnapshot, WorkspaceSnapshotQuery, WorkspaceStreamQuery,
};
use crate::workspace::WorkspaceService;
use crate::workspace::actions::{archive_card, cancel_card, link_vault_card, retry_card, CardActionError};
use crate::workspace::feed::spawn_workspace_stream;

#[derive(Clone)]
pub struct WorkspaceHandlerState {
    pub composition: Arc<RuntimeComposition>,
    pub worker_id: String,
}

fn map_card_action_error(err: CardActionError) -> (StatusCode, String) {
    match err {
        CardActionError::NotFound => (StatusCode::NOT_FOUND, err.message()),
        CardActionError::NotActionable(reason) => (StatusCode::BAD_REQUEST, reason),
        CardActionError::Internal(reason) => (StatusCode::INTERNAL_SERVER_ERROR, reason),
    }
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

pub async fn rebuild_workspace(
    State(state): State<WorkspaceHandlerState>,
) -> Result<Json<WorkspaceRebuildResponse>, (StatusCode, String)> {
    WorkspaceService::rebuild(state.composition.as_ref())
        .await
        .map(Json)
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))
}

pub async fn cancel_workspace_card(
    State(state): State<WorkspaceHandlerState>,
    Path(card_id): Path<String>,
) -> Result<Json<WorkspaceCardActionResponse>, (StatusCode, String)> {
    cancel_card(state.composition, &card_id)
        .await
        .map(Json)
        .map_err(map_card_action_error)
}

pub async fn archive_workspace_card(
    State(state): State<WorkspaceHandlerState>,
    Path(card_id): Path<String>,
    Json(request): Json<crate::daemon_api::ArchiveAskJobRequest>,
) -> Result<Json<WorkspaceCardActionResponse>, (StatusCode, String)> {
    archive_card(state.composition, &card_id, request.purge_output)
        .await
        .map(Json)
        .map_err(map_card_action_error)
}

pub async fn retry_workspace_card(
    State(state): State<WorkspaceHandlerState>,
    Path(card_id): Path<String>,
) -> Result<Json<WorkspaceCardActionResponse>, (StatusCode, String)> {
    retry_card(state.composition, &card_id, &state.worker_id)
        .await
        .map(Json)
        .map_err(map_card_action_error)
}

pub async fn link_workspace_card_vault(
    State(state): State<WorkspaceHandlerState>,
    Path(card_id): Path<String>,
    Json(request): Json<WorkspaceLinkVaultRequest>,
) -> Result<Json<WorkspaceCardActionResponse>, (StatusCode, String)> {
    match WorkspaceService::get_card_detail(state.composition.clone(), &card_id).await {
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                format!("card not found: {card_id}"),
            ));
        }
        Err(err) => return Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string())),
        Ok(Some(_)) => {}
    }

    link_vault_card(&card_id, &request.vault_path)
        .map(Json)
        .map_err(map_card_action_error)
}

pub async fn workspace_stream(
    State(state): State<WorkspaceHandlerState>,
    Query(query): Query<WorkspaceStreamQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, String)> {
    let receiver = spawn_workspace_stream(state.composition, query);

    let stream = stream::unfold(receiver, |mut rx| async move {
        match rx.recv().await {
            Some(payload) => {
                let event_type = payload.stream_event_type.clone();
                let event = match Event::default().event(event_type).json_data(payload) {
                    Ok(value) => value,
                    Err(err) => Event::default()
                        .event("error")
                        .data(format!("workspace stream serialization error: {err}")),
                };
                Some((Ok::<Event, Infallible>(event), rx))
            }
            None => None,
        }
    });

    Ok(
        Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)).text(
            "keep-alive",
        )),
    )
}
