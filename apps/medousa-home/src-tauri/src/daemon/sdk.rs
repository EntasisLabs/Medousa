//! Bridges Tauri workshop routing (LAN / Iroh) to `medousa-sdk` via `medousa-sdk-iroh`.

use std::sync::Arc;

use medousa_sdk::{MedousaClient, SdkError, Transport};
use medousa_sdk_iroh::{WorkshopTransport, WorkshopTransportConfig};
use tauri::State;

use crate::daemon::DaemonState;
use crate::pairing_client::WorkshopTransportConfig;

#[cfg(any(target_os = "ios", target_os = "android"))]
use crate::daemon::iroh_hook::TauriIrohHook;

pub fn sdk_error(err: SdkError) -> String {
    err.to_string()
}

pub fn transport_config(state: &State<DaemonState>) -> WorkshopTransportConfig {
    let base = state.daemon_url.lock().expect("daemon url lock").clone();
    crate::workshop_transport::config_from_lan_base(&base)
}

fn build_sdk_transport(config: &WorkshopTransportConfig) -> Arc<dyn Transport> {
    let sdk_config = WorkshopTransportConfig::from_workshop_parts(
        config.lan_base.clone(),
        config.session_token.clone(),
        config.iroh_ticket.clone(),
    );
    let mut transport = WorkshopTransport::new(sdk_config);
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(ticket) = config.iroh_ticket.as_deref() {
        transport = transport.with_iroh_hook(Arc::new(TauriIrohHook::new(ticket)));
    }
    Arc::new(transport)
}

pub fn client(state: &State<DaemonState>) -> MedousaClient {
    let base_url = state.daemon_url.lock().expect("daemon url lock").clone();
    let config = transport_config(state);
    MedousaClient::with_transport(build_sdk_transport(&config), base_url)
}
