//! HTTP handlers for workspace APIs (`/v1/workspace/*`).

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{get, post};
use futures_util::stream::{self, Stream};
use stasis::application::runtime::runtime_factory::RuntimeComposition;

use crate::daemon::route_policy::{
    BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RoutePolicy,
};
use crate::daemon_api::{
    WorkCardDetail, WorkspaceCardActionResponse, WorkspaceCardsQuery, WorkspaceCardsResponse,
    WorkspaceFeedQuery, WorkspaceFeedResponse, WorkspaceLinkVaultRequest, WorkspaceRebuildResponse,
    WorkspaceSnapshot, WorkspaceSnapshotQuery, WorkspaceStreamQuery,
};
use crate::workspace::WorkspaceService;
use crate::workspace::actions::{CardActionError, archive_card, cancel_card, link_vault_card};
use crate::workspace::feed::spawn_workspace_stream;

#[derive(Clone)]
pub struct WorkspaceHandlerState {
    pub composition: Arc<RuntimeComposition>,
}

pub fn workspace_surface() -> DeclaredRouter<WorkspaceHandlerState> {
    DeclaredRouter::default()
        .route(
            workspace_read_policy("/v1/workspace/cards"),
            get(list_workspace_cards),
        )
        .route(
            workspace_read_policy("/v1/workspace/cards/{card_id}"),
            get(get_workspace_card),
        )
        .route(
            workspace_write_policy("/v1/workspace/cards/{card_id}/cancel", 1024),
            post(cancel_workspace_card),
        )
        .route(
            workspace_write_policy("/v1/workspace/cards/{card_id}/archive", 16 * 1024),
            post(archive_workspace_card),
        )
        .route(
            workspace_write_policy("/v1/workspace/cards/{card_id}/link-vault", 64 * 1024),
            post(link_workspace_card_vault),
        )
        .route(
            workspace_read_policy("/v1/workspace/feed"),
            get(list_workspace_feed),
        )
        .route(
            workspace_read_policy("/v1/workspace/snapshot"),
            get(get_workspace_snapshot),
        )
        .route(
            workspace_admin_policy("/v1/workspace/rebuild"),
            post(rebuild_workspace),
        )
        .route(
            workspace_stream_policy("/v1/workspace/stream"),
            get(workspace_stream),
        )
}

fn workspace_read_policy(path: &'static str) -> RoutePolicy {
    workspace_policy(
        axum::http::Method::GET,
        path,
        crate::request_principal::Capability::WorkshopRead,
        1024,
        RateLimitClass::Read,
    )
}

fn workspace_write_policy(path: &'static str, body_limit: usize) -> RoutePolicy {
    workspace_policy(
        axum::http::Method::POST,
        path,
        crate::request_principal::Capability::WorkspaceWrite,
        body_limit,
        RateLimitClass::Mutation,
    )
}

fn workspace_admin_policy(path: &'static str) -> RoutePolicy {
    RoutePolicy {
        method: axum::http::Method::POST,
        path,
        group: RouteGroup::Administration,
        required_capability: Some(crate::request_principal::Capability::AdminRuntime),
        bootstrap_public: false,
        browser_policy: BrowserPolicy::NativeOnly,
        body_limit: 1024,
        rate_limit_class: RateLimitClass::Administration,
    }
}

fn workspace_stream_policy(path: &'static str) -> RoutePolicy {
    RoutePolicy {
        rate_limit_class: RateLimitClass::Stream,
        ..workspace_read_policy(path)
    }
}

fn workspace_policy(
    method: axum::http::Method,
    path: &'static str,
    required_capability: crate::request_principal::Capability,
    body_limit: usize,
    rate_limit_class: RateLimitClass,
) -> RoutePolicy {
    RoutePolicy {
        method,
        path,
        group: RouteGroup::Portal,
        required_capability: Some(required_capability),
        bootstrap_public: false,
        browser_policy: BrowserPolicy::NativeOnly,
        body_limit,
        rate_limit_class,
    }
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

pub async fn link_workspace_card_vault(
    State(state): State<WorkspaceHandlerState>,
    Path(card_id): Path<String>,
    Json(request): Json<WorkspaceLinkVaultRequest>,
) -> Result<Json<WorkspaceCardActionResponse>, (StatusCode, String)> {
    match WorkspaceService::get_card_detail(state.composition.clone(), &card_id).await {
        Ok(None) => {
            return Err((StatusCode::NOT_FOUND, format!("card not found: {card_id}")));
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

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_inventory_separates_reads_writes_streams_and_host_ops() {
        let entries = workspace_surface()
            .inventory()
            .entries()
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 9);
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.required_capability == Some("workshop.read"))
                .count(),
            5
        );
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.required_capability == Some("workspace.write"))
                .count(),
            3
        );
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.required_capability == Some("admin.runtime"))
                .count(),
            1
        );
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.rate_limit_class == RateLimitClass::Stream)
                .count(),
            1
        );
    }
}
