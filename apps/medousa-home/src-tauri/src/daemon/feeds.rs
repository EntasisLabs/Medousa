use medousa_types::feed::FeedTailResponse;
use tauri::State;

use super::workshop_http::{self, path_with_query};
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

    let mut query = Vec::new();
    if let Some(profile_id) = profile_id.filter(|id| !id.trim().is_empty()) {
        query.push(("profile_id", profile_id));
    }
    if let Some(limit) = limit {
        query.push(("limit", limit.clamp(1, 100).to_string()));
    }

    let path = if query.is_empty() {
        format!("/v1/feeds/{feed_id}/tail")
    } else {
        path_with_query(
            &format!("/v1/feeds/{feed_id}/tail"),
            &query.iter().map(|(k, v)| (*k, v.clone())).collect::<Vec<_>>(),
        )
    };

    workshop_http::get_json(&state, &path).await
}
