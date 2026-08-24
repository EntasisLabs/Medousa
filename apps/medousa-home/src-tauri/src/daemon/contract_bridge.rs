//! Closed native dispatcher for generated daemon operations.
//!
//! Existing endpoint-shaped `#[tauri::command]` proxies remain as ticketed
//! shims until each Home feature migrates onto `daemon_unary` /
//! `daemon_stream_start` / `daemon_stream_cancel`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::watch;

use crate::daemon::DaemonState;
use crate::daemon::generated_ops::DaemonOperation;
use crate::daemon::sse::stream_sse_json_workshop;
use crate::daemon::workshop_http;
use crate::workshop_transport;

static STREAM_SEQ: AtomicU64 = AtomicU64::new(1);

#[tauri::command]
pub async fn daemon_unary(
    state: State<'_, DaemonState>,
    operation: DaemonOperation,
    path_params: HashMap<String, String>,
    body: Option<Value>,
) -> Result<Value, String> {
    let op = medousa_sdk::generated::ops::by_id(operation.id())
        .ok_or_else(|| format!("unknown operation {}", operation.id()))?;
    if op.streaming {
        return Err("use daemon_stream_start for streaming operations".into());
    }
    let path = expand_operation_path(op.path, &path_params, None)?;
    match op.method {
        "GET" => workshop_http::get_json(&state, &path).await,
        "DELETE" => workshop_http::delete_json(&state, &path).await,
        "POST" => match &body {
            Some(value) => workshop_http::post_json(&state, &path, value).await,
            None => workshop_http::post_empty_json(&state, &path).await,
        },
        "PUT" => workshop_http::put_json(&state, &path, &body.unwrap_or(Value::Null)).await,
        "PATCH" => workshop_http::patch_json(&state, &path, &body.unwrap_or(Value::Null)).await,
        other => Err(format!("unsupported method {other}")),
    }
}

#[tauri::command]
pub async fn daemon_stream_start(
    app: AppHandle,
    state: State<'_, DaemonState>,
    operation: DaemonOperation,
    path_params: HashMap<String, String>,
    query: Option<HashMap<String, String>>,
    client_handle: Option<String>,
) -> Result<String, String> {
    let op = medousa_sdk::generated::ops::by_id(operation.id())
        .ok_or_else(|| format!("unknown operation {}", operation.id()))?;
    if !op.streaming {
        return Err("operation is not a stream".into());
    }
    let path = expand_operation_path(op.path, &path_params, query.as_ref())?;
    let handle = match client_handle {
        Some(handle) => validate_client_stream_handle(handle)?,
        None => format!(
            "{}-{}",
            operation.id(),
            STREAM_SEQ.fetch_add(1, Ordering::Relaxed)
        ),
    };
    let (tx, rx) = watch::channel(false);
    let mut streams = state.contract_streams.lock().expect("contract stream lock");
    if streams.contains_key(&handle) {
        return Err("daemon stream handle is already active".into());
    }
    streams.insert(handle.clone(), tx);
    drop(streams);

    let config = crate::daemon::sdk::transport_config(&state)?;
    let event_name = format!("daemon-stream://{handle}/event");
    let error_event = format!("daemon-stream://{handle}/error");
    tokio::spawn(async move {
        match workshop_transport::workshop_get_bytes_stream(&config, &path).await {
            Ok(source) => {
                stream_sse_json_workshop::<Value>(&app, source, &event_name, &error_event, rx)
                    .await;
            }
            Err(err) => {
                let _ = app.emit(&error_event, serde_json::json!({ "message": err }));
            }
        }
    });
    Ok(handle)
}

fn validate_client_stream_handle(handle: String) -> Result<String, String> {
    let valid = !handle.is_empty()
        && handle.len() <= 160
        && handle
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'));
    if !valid {
        return Err("invalid daemon stream handle".into());
    }
    Ok(handle)
}

#[tauri::command]
pub fn daemon_stream_cancel(state: State<'_, DaemonState>, handle: String) -> Result<(), String> {
    if let Some(tx) = state
        .contract_streams
        .lock()
        .expect("contract stream lock")
        .remove(&handle)
    {
        let _ = tx.send(true);
    }
    Ok(())
}

pub(crate) fn expand_operation_path(
    template: &str,
    path_params: &HashMap<String, String>,
    query: Option<&HashMap<String, String>>,
) -> Result<String, String> {
    let pairs: Vec<(&str, &str)> = path_params
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    let path = medousa_sdk::generated::expand_path(template, &pairs)?;
    let Some(query) = query else {
        return Ok(path);
    };
    if query.is_empty() {
        return Ok(path);
    }
    let mut entries: Vec<(&str, String)> = query
        .iter()
        .map(|(key, value)| (key.as_str(), value.clone()))
        .collect();
    entries.sort_by(|left, right| left.0.cmp(right.0));
    Ok(medousa_sdk::transport::path_with_query(&path, &entries))
}

#[cfg(test)]
mod tests {
    use super::{expand_operation_path, validate_client_stream_handle};
    use std::collections::HashMap;

    #[test]
    fn client_stream_handles_are_bounded_and_event_name_safe() {
        assert!(validate_client_stream_handle("forge.stream-123:4".into()).is_ok());
        assert!(validate_client_stream_handle("../forge stream".into()).is_err());
        assert!(validate_client_stream_handle("x".repeat(161)).is_err());
    }

    #[test]
    fn expands_generated_stream_path_and_query() {
        let path = expand_operation_path(
            medousa_sdk::generated::ops::WORKSPACE_STREAM_GET.path,
            &HashMap::new(),
            Some(&HashMap::from([(
                "since_revision".to_string(),
                "7".to_string(),
            )])),
        )
        .expect("expand");
        assert_eq!(path, "/v1/workspace/stream?since_revision=7");
    }

    #[test]
    fn encodes_catch_all_as_one_segment() {
        let path = expand_operation_path(
            medousa_sdk::generated::ops::VAULT_NOTES_BY_NOTE_PATH_GET.path,
            &HashMap::from([("note_path".to_string(), "a/b.md".to_string())]),
            None,
        )
        .expect("expand");
        assert_eq!(path, "/v1/vault/notes/a%2Fb.md");
    }
}
