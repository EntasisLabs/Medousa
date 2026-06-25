//! Bridges Tauri workshop routing (LAN / Iroh) to `medousa-sdk`.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use medousa_sdk::{MedousaClient, SdkError, Transport};
use tauri::State;

use crate::daemon::workshop_http;
use crate::daemon::DaemonState;
use crate::pairing_client::WorkshopTransportConfig;
use crate::workshop_transport;

#[derive(Clone)]
struct TauriWorkshopTransport {
    config: WorkshopTransportConfig,
}

impl TauriWorkshopTransport {
    fn new(config: WorkshopTransportConfig) -> Self {
        Self { config }
    }
}

impl Transport for TauriWorkshopTransport {
    fn get_json<'a>(
        &'a self,
        _base_url: &'a str,
        path: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SdkError>> + Send + 'a>> {
        let config = self.config.clone();
        let path = path.to_string();
        Box::pin(async move {
            workshop_transport::workshop_get_json::<serde_json::Value>(&config, &path)
                .await
                .map_err(SdkError::Http)
        })
    }

    fn post_json<'a>(
        &'a self,
        _base_url: &'a str,
        path: &'a str,
        body: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SdkError>> + Send + 'a>> {
        let config = self.config.clone();
        let path = path.to_string();
        Box::pin(async move {
            workshop_transport::workshop_post_json::<serde_json::Value, _>(&config, &path, &body)
                .await
                .map_err(SdkError::Http)
        })
    }

    fn delete_json<'a>(
        &'a self,
        _base_url: &'a str,
        path: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SdkError>> + Send + 'a>> {
        let config = self.config.clone();
        let path = path.to_string();
        Box::pin(async move {
            workshop_transport::workshop_delete_json::<serde_json::Value>(&config, &path)
                .await
                .map_err(SdkError::Http)
        })
    }

    fn put_json<'a>(
        &'a self,
        _base_url: &'a str,
        path: &'a str,
        body: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SdkError>> + Send + 'a>> {
        let config = self.config.clone();
        let path = path.to_string();
        Box::pin(async move {
            workshop_transport::workshop_put_json::<serde_json::Value, _>(&config, &path, &body)
                .await
                .map_err(SdkError::Http)
        })
    }
}

pub fn client(state: &State<DaemonState>) -> MedousaClient {
    let base_url = state.daemon_url.lock().expect("daemon url lock").clone();
    let config = workshop_http::transport_config(state);
    MedousaClient::with_transport(Arc::new(TauriWorkshopTransport::new(config)), base_url)
}
