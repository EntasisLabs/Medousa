//! Daemon JSON API traffic routes through [`medousa-sdk`] (LAN / Iroh via Tauri transport).
//!
//! Streaming (`get_bytes_stream`) and multipart uploads still use [`crate::workshop_transport`]
//! directly because the SDK transport surface is JSON-only today.

use serde::de::DeserializeOwned;
use tauri::State;

use crate::daemon::sdk::{client, sdk_error};
use crate::daemon::DaemonState;
use crate::pairing_client::WorkshopTransportConfig;
use crate::workshop_transport::{self, MultipartField, WorkshopByteStream};

pub fn transport_config(state: &State<'_, DaemonState>) -> WorkshopTransportConfig {
    let base = state.daemon_url.lock().expect("daemon url lock").clone();
    workshop_transport::config_from_lan_base(&base)
}

pub fn path_with_query(path: &str, query: &[(&str, String)]) -> String {
    medousa_sdk::transport::path_with_query(path, query)
}

pub async fn get_json<T: DeserializeOwned>(
    state: &State<'_, DaemonState>,
    path: &str,
) -> Result<T, String> {
    client(state).http().get(path).await.map_err(sdk_error)
}

pub async fn get_json_query<T: DeserializeOwned>(
    state: &State<'_, DaemonState>,
    path: &str,
    query: &[(&str, String)],
) -> Result<T, String> {
    client(state)
        .http()
        .get_query(path, query)
        .await
        .map_err(sdk_error)
}

pub async fn post_json<T: DeserializeOwned, B: serde::Serialize>(
    state: &State<'_, DaemonState>,
    path: &str,
    body: &B,
) -> Result<T, String> {
    client(state)
        .http()
        .post(path, body)
        .await
        .map_err(sdk_error)
}

pub async fn post_empty_json<T: DeserializeOwned>(
    state: &State<'_, DaemonState>,
    path: &str,
) -> Result<T, String> {
    client(state)
        .http()
        .post_empty(path)
        .await
        .map_err(sdk_error)
}

pub async fn put_json<T: DeserializeOwned, B: serde::Serialize>(
    state: &State<'_, DaemonState>,
    path: &str,
    body: &B,
) -> Result<T, String> {
    client(state)
        .http()
        .put(path, body)
        .await
        .map_err(sdk_error)
}

pub async fn put_raw<T: DeserializeOwned>(
    state: &State<'_, DaemonState>,
    path: &str,
    content_type: &str,
    body: &[u8],
    extra_headers: &[(&str, &str)],
) -> Result<T, String> {
    workshop_transport::workshop_put_raw(
        &transport_config(state),
        path,
        content_type,
        body,
        extra_headers,
    )
    .await
}

pub async fn patch_json<T: DeserializeOwned, B: serde::Serialize>(
    state: &State<'_, DaemonState>,
    path: &str,
    body: &B,
) -> Result<T, String> {
    client(state)
        .http()
        .patch(path, body)
        .await
        .map_err(sdk_error)
}

pub async fn delete_json<T: DeserializeOwned>(
    state: &State<'_, DaemonState>,
    path: &str,
) -> Result<T, String> {
    client(state)
        .http()
        .delete(path)
        .await
        .map_err(sdk_error)
}

pub async fn post_multipart<T: DeserializeOwned>(
    state: &State<'_, DaemonState>,
    path: &str,
    fields: &[MultipartField],
) -> Result<T, String> {
    workshop_transport::workshop_post_multipart(&transport_config(state), path, fields).await
}

pub async fn get_bytes_stream(
    state: &State<'_, DaemonState>,
    path: &str,
) -> Result<WorkshopByteStream, String> {
    workshop_transport::workshop_get_bytes_stream(&transport_config(state), path).await
}

pub async fn get_bytes_stream_for_config(
    config: &WorkshopTransportConfig,
    path: &str,
) -> Result<WorkshopByteStream, String> {
    workshop_transport::workshop_get_bytes_stream(config, path).await
}
