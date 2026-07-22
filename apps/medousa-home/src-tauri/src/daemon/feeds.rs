use medousa_types::feed::{FeedLatestGoodQuery, FeedLatestGoodResponse, FeedTailQuery, FeedTailResponse};
use tauri::State;

use super::sdk::{client, sdk_error};
use super::DaemonState;

#[tauri::command]
pub async fn feed_tail(
    state: State<'_, DaemonState>,
    feed_id: String,
    profile_id: Option<String>,
    limit: Option<usize>,
) -> Result<FeedTailResponse, String> {
    let feed_id = feed_id.trim();
    if feed_id.is_empty() {
        return Err("feed_id is required".to_string());
    }
    if !medousa_types::feed::is_valid_feed_id(feed_id) {
        return Err(format!("invalid feed_id '{feed_id}'"));
    }

    let query = FeedTailQuery {
        profile_id: profile_id.filter(|id| !id.trim().is_empty()),
        limit: limit.map(|value| value.clamp(1, 100)),
    };
    client(&state)
        .feeds()
        .tail(feed_id, &query)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn feed_latest_good(
    state: State<'_, DaemonState>,
    feed_id: String,
    profile_id: Option<String>,
) -> Result<FeedLatestGoodResponse, String> {
    let feed_id = feed_id.trim();
    if feed_id.is_empty() {
        return Err("feed_id is required".to_string());
    }
    if !medousa_types::feed::is_valid_feed_id(feed_id) {
        return Err(format!("invalid feed_id '{feed_id}'"));
    }

    let query = FeedLatestGoodQuery {
        profile_id: profile_id.filter(|id| !id.trim().is_empty()),
    };
    client(&state)
        .feeds()
        .latest_good(feed_id, &query)
        .await
        .map_err(sdk_error)
}
