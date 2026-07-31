//! Transport-safe bridge for Forge / World JSON requests.
//!
//! These APIs have a broad, evolving surface, so Home sends the path and JSON
//! payload through one restricted command instead of making WebView `fetch`
//! calls. The workshop transport then handles local, LAN, and Iroh routes in
//! the same way as the rest of the daemon API.

use serde_json::Value;
use tauri::State;

use super::{DaemonState, workshop_http};

fn allowed_path(path: &str) -> bool {
    let path_without_query = path.split('?').next().unwrap_or(path);
    (path_without_query == "/v1/forge"
        || path_without_query.starts_with("/v1/forge/")
        || path_without_query == "/v1/world"
        || path_without_query.starts_with("/v1/world/"))
        && !path.contains(['\r', '\n'])
}

#[tauri::command]
pub async fn forge_request(
    state: State<'_, DaemonState>,
    method: String,
    path: String,
    body: Option<Value>,
) -> Result<Value, String> {
    if !allowed_path(&path) {
        return Err("unsupported Forge API path".into());
    }

    let method = method.trim().to_ascii_uppercase();
    if !matches!(method.as_str(), "GET" | "POST" | "PUT" | "PATCH" | "DELETE") {
        return Err("unsupported Forge API method".into());
    }
    crate::workshop_transport::workshop_json_request(
        &workshop_http::transport_config(&state),
        &method,
        &path,
        body.as_ref(),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::allowed_path;

    #[test]
    fn restricts_bridge_to_forge_and_world_routes() {
        assert!(allowed_path("/v1/forge/items?limit=5"));
        assert!(allowed_path("/v1/world/find?query=thing"));
        assert!(!allowed_path("/v1/sessions"));
        assert!(!allowed_path("http://example.test/v1/forge/items"));
        assert!(!allowed_path("/v1/forge/items\r\nX-Test: bad"));
    }
}
