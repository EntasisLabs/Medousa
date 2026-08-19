//! Co-located reads of daemon-owned integration slots.
//!
//! Home never returns these values to the webview. Adapter spawn and local
//! credential-presence flags may read the daemon store when this process shares
//! the workshop data directory.

use medousa_secrets::SecretStore;
use medousa_types::{
    ConnectionId, DaemonSecretPath, InstallationId, IntegrationSecretSlot,
};
use serde::Deserialize;

use super::product_config;

#[derive(Debug, Deserialize)]
struct ConnectionRecord {
    connection_id: String,
    kind: String,
}

#[derive(Debug, Default, Deserialize)]
struct ConnectionDocument {
    #[serde(default)]
    connections: Vec<ConnectionRecord>,
}

fn data_dir() -> std::path::PathBuf {
    crate::paths::medousa_data_dir()
}

fn secret_store() -> SecretStore {
    SecretStore::new(data_dir())
}

fn load_document() -> ConnectionDocument {
    let path = data_dir().join("integration_connections.json");
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn find_kind(kind: &str) -> Option<ConnectionId> {
    let kind = kind.trim().to_ascii_lowercase();
    let matches: Vec<_> = load_document()
        .connections
        .into_iter()
        .filter(|row| row.kind.eq_ignore_ascii_case(&kind))
        .collect();
    let row = matches.first()?;
    ConnectionId::parse(&row.connection_id).ok()
}

fn installation_id() -> Option<InstallationId> {
    secret_store().ensure_installation_id().ok()
}

pub fn load_kind_slot(kind: &str, slot: IntegrationSecretSlot) -> Option<String> {
    let installation = installation_id()?;
    let connection = find_kind(kind)?;
    let path = DaemonSecretPath::integration(installation, connection, slot);
    secret_store().get_daemon(&path)
}

pub fn kind_slot_configured(kind: &str, slot: IntegrationSecretSlot) -> bool {
    load_kind_slot(kind, slot).is_some()
}

pub fn overlay_channel_credentials(summary: &mut product_config::ProductConfigSummary) {
    summary.telegram.credentials_set =
        kind_slot_configured("telegram", IntegrationSecretSlot::BotToken);
    summary.discord.credentials_set =
        kind_slot_configured("discord", IntegrationSecretSlot::BotToken);
    summary.slack.bot_token_set = kind_slot_configured("slack", IntegrationSecretSlot::BotToken);
    summary.slack.app_token_set = kind_slot_configured("slack", IntegrationSecretSlot::AppToken);
}
