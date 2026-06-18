//! All workshop daemon API traffic routes through [`crate::workshop_transport`] (LAN or Iroh).

use serde::de::DeserializeOwned;
use tauri::State;

use crate::daemon::DaemonState;
use crate::pairing_client::WorkshopTransportConfig;
use crate::workshop_transport::{self, MultipartField, WorkshopByteStream};

pub fn config(state: &State<'_, DaemonState>) -> WorkshopTransportConfig {
    let base = state.daemon_url.lock().expect("daemon url lock").clone();
    workshop_transport::config_from_lan_base(&base)
}

pub fn path_with_query(path: &str, query: &[(&str, String)]) -> String {
    workshop_transport::path_with_query(path, query)
}

pub async fn get_json<T: DeserializeOwned>(
    state: &State<'_, DaemonState>,
    path: &str,
) -> Result<T, String> {
    workshop_transport::workshop_get_json(&config(state), path).await
}

pub async fn get_json_query<T: DeserializeOwned>(
    state: &State<'_, DaemonState>,
    path: &str,
    query: &[(&str, String)],
) -> Result<T, String> {
    let path = path_with_query(path, query);
    workshop_transport::workshop_get_json(&config(state), &path).await
}

pub async fn post_json<T: DeserializeOwned, B: serde::Serialize>(
    state: &State<'_, DaemonState>,
    path: &str,
    body: &B,
) -> Result<T, String> {
    workshop_transport::workshop_post_json(&config(state), path, body).await
}

pub async fn post_empty_json<T: DeserializeOwned>(
    state: &State<'_, DaemonState>,
    path: &str,
) -> Result<T, String> {
    workshop_transport::workshop_post_empty_json(&config(state), path).await
}

pub async fn put_json<T: DeserializeOwned, B: serde::Serialize>(
    state: &State<'_, DaemonState>,
    path: &str,
    body: &B,
) -> Result<T, String> {
    workshop_transport::workshop_put_json(&config(state), path, body).await
}

pub async fn put_raw<T: DeserializeOwned>(
    state: &State<'_, DaemonState>,
    path: &str,
    content_type: &str,
    body: &[u8],
    extra_headers: &[(&str, &str)],
) -> Result<T, String> {
    workshop_transport::workshop_put_raw(
        &config(state),
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
    workshop_transport::workshop_patch_json(&config(state), path, body).await
}

pub async fn delete_json<T: DeserializeOwned>(
    state: &State<'_, DaemonState>,
    path: &str,
) -> Result<T, String> {
    workshop_transport::workshop_delete_json(&config(state), path).await
}

pub async fn post_multipart<T: DeserializeOwned>(
    state: &State<'_, DaemonState>,
    path: &str,
    fields: &[MultipartField],
) -> Result<T, String> {
    workshop_transport::workshop_post_multipart(&config(state), path, fields).await
}

pub async fn get_bytes_stream(
    state: &State<'_, DaemonState>,
    path: &str,
) -> Result<WorkshopByteStream, String> {
    workshop_transport::workshop_get_bytes_stream(&config(state), path).await
}

pub async fn get_bytes_stream_for_config(
    config: &WorkshopTransportConfig,
    path: &str,
) -> Result<WorkshopByteStream, String> {
    workshop_transport::workshop_get_bytes_stream(config, path).await
}

pub fn transport_config(state: &State<'_, DaemonState>) -> WorkshopTransportConfig {
    config(state)
}
