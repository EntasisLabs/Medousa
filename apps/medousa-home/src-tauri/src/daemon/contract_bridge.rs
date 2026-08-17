//! Closed native dispatcher for generated daemon operations.
//!
//! Existing endpoint-shaped `#[tauri::command]` proxies remain as ticketed
//! shims until each Home feature migrates onto `daemon_unary` /
//! `daemon_stream_start` / `daemon_stream_cancel`.

use std::collections::HashMap;

use serde_json::Value;
use tauri::State;

use crate::daemon::generated_ops::DaemonOperation;
use crate::daemon::workshop_http;
use crate::daemon::DaemonState;

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
    let pairs: Vec<(&str, &str)> = path_params
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    let path = medousa_sdk::generated::expand_path(op.path, &pairs)?;
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
    operation: DaemonOperation,
    path_params: HashMap<String, String>,
) -> Result<String, String> {
    let op = medousa_sdk::generated::ops::by_id(operation.id())
        .ok_or_else(|| format!("unknown operation {}", operation.id()))?;
    if !op.streaming {
        return Err("operation is not a stream".into());
    }
    let pairs: Vec<(&str, &str)> = path_params
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    let path = medousa_sdk::generated::expand_path(op.path, &pairs)?;
    Ok(path)
}

#[tauri::command]
pub fn daemon_stream_cancel(_handle: String) -> Result<(), String> {
    Ok(())
}