//! HTTP handlers for `/v1/feeds/*`.

use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use futures_util::stream::{self, Stream};
use medousa_types::feed::{
    FeedLatestGoodQuery, FeedLatestGoodResponse, FeedListResponse, FeedReadRequest,
    FeedStreamEvent, FeedStreamQuery, FeedTailQuery, FeedTailResponse,
};
use std::convert::Infallible;
use std::time::Duration;
use tokio::sync::broadcast;

use crate::environment_store::resolve_profile_id;
use crate::feed_bus::{feed_hub, list_feeds};
use crate::feed_store::feed_store;

#[derive(Clone)]
pub struct FeedApiState;

pub fn feed_router() -> Router {
    Router::new()
        .route("/v1/feeds", get(list_feeds_handler))
        .route("/v1/feeds/stream", get(stream_feeds))
        .route("/v1/feeds/{feed_id}/tail", get(tail_feed))
        .route("/v1/feeds/{feed_id}/latest-good", get(latest_good_feed))
        .route("/v1/feeds/{feed_id}/read", post(mark_feed_read))
        .with_state(FeedApiState)
}

async fn list_feeds_handler(
    Query(query): Query<FeedTailQuery>,
) -> Json<FeedListResponse> {
    Json(list_feeds(query.profile_id.as_deref()).await)
}

async fn tail_feed(
    Path(feed_id): Path<String>,
    Query(query): Query<FeedTailQuery>,
) -> Result<Json<FeedTailResponse>, (StatusCode, String)> {
    let profile_id = resolve_profile_id(query.profile_id.as_deref());
    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let events = feed_store().tail(&profile_id, feed_id.trim(), limit).await;
    Ok(Json(FeedTailResponse {
        feed_id: feed_id.trim().to_string(),
        events,
    }))
}

async fn latest_good_feed(
    Path(feed_id): Path<String>,
    Query(query): Query<FeedLatestGoodQuery>,
) -> Result<Json<FeedLatestGoodResponse>, (StatusCode, String)> {
    let profile_id = resolve_profile_id(query.profile_id.as_deref());
    let feed_id = feed_id.trim();
    let result = feed_store().latest_good(&profile_id, feed_id).await;
    match result {
        Some(response) => Ok(Json(response)),
        None => Err((StatusCode::NOT_FOUND, format!("no latest good result for feed '{feed_id}'"))),
    }
}

async fn mark_feed_read(
    Path(feed_id): Path<String>,
    Json(body): Json<FeedReadRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let profile_id = resolve_profile_id(body.profile_id.as_deref());
    let seq = body.seq.unwrap_or(0);
    feed_store()
        .set_read_cursor(&profile_id, feed_id.trim(), seq)
        .await;
    Ok(StatusCode::NO_CONTENT)
}

async fn stream_feeds(
    Query(query): Query<FeedStreamQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, String)> {
    let _profile_id = resolve_profile_id(query.profile_id.as_deref());
    let rx = feed_hub().subscribe();
    let stream = stream::unfold(rx, move |mut rx| async move {
        match rx.recv().await {
            Ok(event) => {
                let payload = serde_json::to_string(&event).unwrap_or_else(|_| "{}".to_string());
                Some((Ok(Event::default().data(payload)), rx))
            }
            Err(broadcast::error::RecvError::Lagged(_)) => Some((
                Ok(Event::default().data(
                    serde_json::to_string(&FeedStreamEvent {
                        seq: 0,
                        event_type: "heartbeat".to_string(),
                        emitted_at_utc: Utc::now(),
                        feed_event: None,
                        component_patches: None,
                    })
                    .unwrap_or_else(|_| "{}".to_string()),
                )),
                rx,
            )),
            Err(broadcast::error::RecvError::Closed) => None,
        }
    });

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(30))
            .text("keep-alive"),
    ))
}
