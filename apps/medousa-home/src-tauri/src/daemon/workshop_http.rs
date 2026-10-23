//! Daemon JSON API traffic routes through [`medousa-sdk`] typed accessors where available.
//!
//! `workshop_http` remains a thin wrapper over `client().http()` for routes without SDK
//! accessors (workflows, identity, grapheme, manuscripts, etc.) and for non-JSON I/O:
//! streaming (`get_bytes_stream`), multipart uploads (`post_multipart`), and raw PUT
//! (`put_raw`) when not covered by an accessor.

use serde::de::DeserializeOwned;
use tauri::State;
use super::window_request::WorkshopHttpSource;

use crate::daemon::DaemonState;
use crate::pairing_client::WorkshopTransportConfig;
use crate::workshop_transport::{self, MultipartField, WorkshopByteStream};

pub fn transport_config(
    _state: &State<'_, DaemonState>,
) -> Result<WorkshopTransportConfig, String> {
    crate::active_workshop::transport_config()
}

pub fn path_with_query(path: &str, query: &[(&str, String)]) -> String {
    medousa_sdk::transport::path_with_query(path, query)
}

pub async fn get_json<T: DeserializeOwned>(
    state: &impl WorkshopHttpSource,
    path: &str,
) -> Result<T, String> {
    workshop_transport::workshop_get_json(&state.config().await?, path).await
}

pub async fn get_json_query<T: DeserializeOwned>(
    state: &impl WorkshopHttpSource,
    path: &str,
    query: &[(&str, String)],
) -> Result<T, String> {
    let path = path_with_query(path, query);
    workshop_transport::workshop_get_json(&state.config().await?, &path).await
}

pub async fn post_json<T: DeserializeOwned, B: serde::Serialize>(
    state: &impl WorkshopHttpSource,
    path: &str,
    body: &B,
) -> Result<T, String> {
    workshop_transport::workshop_post_json(&state.config().await?, path, body).await
}

pub async fn post_empty_json<T: DeserializeOwned>(
    state: &impl WorkshopHttpSource,
    path: &str,
) -> Result<T, String> {
    workshop_transport::workshop_post_empty_json(&state.config().await?, path).await
}

pub async fn put_json<T: DeserializeOwned, B: serde::Serialize>(
    state: &impl WorkshopHttpSource,
    path: &str,
    body: &B,
) -> Result<T, String> {
    workshop_transport::workshop_put_json(&state.config().await?, path, body).await
}

pub async fn put_raw<T: DeserializeOwned>(
    state: &impl WorkshopHttpSource,
    path: &str,
    content_type: &str,
    body: &[u8],
    extra_headers: &[(&str, &str)],
) -> Result<T, String> {
    workshop_transport::workshop_put_raw(
        &state.config().await?,
        path,
        content_type,
        body,
        extra_headers,
    )
    .await
}

pub async fn patch_json<T: DeserializeOwned, B: serde::Serialize>(
    state: &impl WorkshopHttpSource,
    path: &str,
    body: &B,
) -> Result<T, String> {
    workshop_transport::workshop_patch_json(&state.config().await?, path, body).await
}

pub async fn delete_json<T: DeserializeOwned>(
    state: &impl WorkshopHttpSource,
    path: &str,
) -> Result<T, String> {
    workshop_transport::workshop_delete_json(&state.config().await?, path).await
}

pub async fn post_multipart<T: DeserializeOwned>(
    state: &impl WorkshopHttpSource,
    path: &str,
    fields: &[MultipartField],
) -> Result<T, String> {
    workshop_transport::workshop_post_multipart(&state.config().await?, path, fields).await
}

pub async fn get_bytes_stream(
    state: &impl WorkshopHttpSource,
    path: &str,
) -> Result<WorkshopByteStream, String> {
    workshop_transport::workshop_get_bytes_stream(&state.config().await?, path).await
}

pub async fn get_bytes_stream_for_config(
    config: &WorkshopTransportConfig,
    path: &str,
) -> Result<WorkshopByteStream, String> {
    workshop_transport::workshop_get_bytes_stream(config, path).await
}
