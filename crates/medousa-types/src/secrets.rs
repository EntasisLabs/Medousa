//! Typed secret-path grammar and integration-connection wire DTOs.
//!
//! Keyring account strings stay off the HTTP wire. Clients send `connection_id`
//! + `slot`; path construction lives in the secrets store.

use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::authority_id::{IdentifierError, McpServerId, StorageAuthorityKey, WorkshopScopeId};

/// Durable installation identity persisted in `{dataDir}/installation.json`.
///
/// UUID v4 hyphenated grammar (same as pairing / credential ids). Cannot live
/// in Surreal because Surreal's password is itself keyed by this id.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct InstallationId(String);

impl InstallationId {
    pub fn parse(value: &str) -> Result<Self, IdentifierError> {
        if !is_uuid_hyphenated(value) {
            return Err(IdentifierError::new(
                "installation_id",
                "unsupported_syntax",
            ));
        }
        Ok(Self(value.to_ascii_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn storage_key(&self) -> StorageAuthorityKey {
        StorageAuthorityKey::derive("in1-", "installation", self.as_str())
    }
}

impl fmt::Display for InstallationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for InstallationId {
    type Err = IdentifierError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

/// Stable UUID for one integration connection row.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ConnectionId(String);

impl ConnectionId {
    pub fn parse(value: &str) -> Result<Self, IdentifierError> {
        if !is_uuid_hyphenated(value) {
            return Err(IdentifierError::new("connection_id", "unsupported_syntax"));
        }
        Ok(Self(value.to_ascii_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn storage_key(&self) -> StorageAuthorityKey {
        StorageAuthorityKey::derive("cn1-", "integration-connection", self.as_str())
    }
}

impl fmt::Display for ConnectionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ConnectionId {
    type Err = IdentifierError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

/// First-party local-auth client kinds (matches `medousa-local-credential` names).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case")]
pub enum LocalClientKind {
    HomeLocal,
    MedousaCli,
    MedousaTui,
}

impl LocalClientKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HomeLocal => "home-local",
            Self::MedousaCli => "medousa-cli",
            Self::MedousaTui => "medousa-tui",
        }
    }

    pub fn parse(value: &str) -> Result<Self, IdentifierError> {
        match value {
            "home-local" => Ok(Self::HomeLocal),
            "medousa-cli" => Ok(Self::MedousaCli),
            "medousa-tui" => Ok(Self::MedousaTui),
            _ => Err(IdentifierError::new(
                "local_client_kind",
                "unsupported_syntax",
            )),
        }
    }
}

impl fmt::Display for LocalClientKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Secret slot on an integration connection (never carries values on the wire).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum IntegrationSecretSlot {
    ApiKey,
    OauthBundle,
    BotToken,
    AppToken,
    AuthKey,
}

impl IntegrationSecretSlot {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ApiKey => "api_key",
            Self::OauthBundle => "oauth_bundle",
            Self::BotToken => "bot_token",
            Self::AppToken => "app_token",
            Self::AuthKey => "auth_key",
        }
    }

    pub fn parse(value: &str) -> Result<Self, IdentifierError> {
        match value {
            "api_key" => Ok(Self::ApiKey),
            "oauth_bundle" => Ok(Self::OauthBundle),
            "bot_token" => Ok(Self::BotToken),
            "app_token" => Ok(Self::AppToken),
            "auth_key" => Ok(Self::AuthKey),
            _ => Err(IdentifierError::new(
                "integration_secret_slot",
                "unsupported_syntax",
            )),
        }
    }
}

impl fmt::Display for IntegrationSecretSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for IntegrationSecretSlot {
    type Err = IdentifierError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

/// Slot-presence map — configured bools only, never secret values.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct IntegrationSecretStatus {
    #[serde(default)]
    pub api_key: bool,
    #[serde(default)]
    pub oauth_bundle: bool,
    #[serde(default)]
    pub bot_token: bool,
    #[serde(default)]
    pub app_token: bool,
    #[serde(default)]
    pub auth_key: bool,
}

impl IntegrationSecretStatus {
    pub fn set_slot(&mut self, slot: IntegrationSecretSlot, present: bool) {
        match slot {
            IntegrationSecretSlot::ApiKey => self.api_key = present,
            IntegrationSecretSlot::OauthBundle => self.oauth_bundle = present,
            IntegrationSecretSlot::BotToken => self.bot_token = present,
            IntegrationSecretSlot::AppToken => self.app_token = present,
            IntegrationSecretSlot::AuthKey => self.auth_key = present,
        }
    }

    pub fn slot(&self, slot: IntegrationSecretSlot) -> bool {
        match slot {
            IntegrationSecretSlot::ApiKey => self.api_key,
            IntegrationSecretSlot::OauthBundle => self.oauth_bundle,
            IntegrationSecretSlot::BotToken => self.bot_token,
            IntegrationSecretSlot::AppToken => self.app_token,
            IntegrationSecretSlot::AuthKey => self.auth_key,
        }
    }
}

/// Durable integration connection record (metadata + slot presence).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct IntegrationConnection {
    pub connection_id: ConnectionId,
    /// Catalog / routing slug (`openai`, `discord`, `chatgpt`, `apns`, …).
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default)]
    pub secrets: IntegrationSecretStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct IntegrationConnectionListResponse {
    pub connections: Vec<IntegrationConnection>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct CreateIntegrationConnectionRequest {
    /// Catalog slug (`openai`, `discord`, …).
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct PatchIntegrationConnectionRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct UpsertIntegrationSecretRequest {
    /// Secret material. Never echoed in responses.
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct DeleteIntegrationConnectionResponse {
    pub deleted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct IntegrationSecretWriteResponse {
    pub connection_id: ConnectionId,
    pub slot: IntegrationSecretSlot,
    pub configured: bool,
}

/// Daemon-owned keyring account path (`secrets.daemon` service).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DaemonSecretPath {
    SurrealPassword {
        installation_id: InstallationId,
    },
    Integration {
        installation_id: InstallationId,
        connection_id: ConnectionId,
        slot: IntegrationSecretSlot,
    },
    McpOAuth {
        installation_id: InstallationId,
        server_id: McpServerId,
    },
    LocalAuth {
        installation_id: InstallationId,
        client_kind: LocalClientKind,
        client_id: String,
    },
}

impl DaemonSecretPath {
    pub fn account(&self) -> String {
        match self {
            Self::SurrealPassword { installation_id } => {
                format!("v1/{}/runtime/surreal/password", installation_id.as_str())
            }
            Self::Integration {
                installation_id,
                connection_id,
                slot,
            } => format!(
                "v1/{}/integration/{}/{}",
                installation_id.as_str(),
                connection_id.as_str(),
                slot.as_str()
            ),
            Self::McpOAuth {
                installation_id,
                server_id,
            } => format!(
                "v1/{}/mcp/{}/oauth",
                installation_id.as_str(),
                server_id.as_str()
            ),
            Self::LocalAuth {
                installation_id,
                client_kind,
                client_id,
            } => format!(
                "v1/{}/local-auth/{}/{}/token",
                installation_id.as_str(),
                client_kind.as_str(),
                client_id
            ),
        }
    }

    /// H02 opaque file-fallback name (not a nested `v1/` directory tree).
    pub fn storage_key(&self) -> StorageAuthorityKey {
        StorageAuthorityKey::derive("sk1-", "daemon-secret-path", &self.account())
    }

    pub fn parse(account: &str) -> Result<Self, IdentifierError> {
        let parts: Vec<&str> = account.split('/').collect();
        if parts.first().copied() != Some("v1") {
            return Err(IdentifierError::new(
                "daemon_secret_path",
                "unsupported_syntax",
            ));
        }
        match parts.as_slice() {
            ["v1", installation, "runtime", "surreal", "password"] => Ok(Self::SurrealPassword {
                installation_id: InstallationId::parse(installation)?,
            }),
            ["v1", installation, "integration", connection, slot] => Ok(Self::Integration {
                installation_id: InstallationId::parse(installation)?,
                connection_id: ConnectionId::parse(connection)?,
                slot: IntegrationSecretSlot::parse(slot)?,
            }),
            ["v1", installation, "mcp", server_id, "oauth"] => Ok(Self::McpOAuth {
                installation_id: InstallationId::parse(installation)?,
                server_id: McpServerId::parse(server_id)?,
            }),
            ["v1", installation, "local-auth", kind, client_id, "token"] => {
                if client_id.is_empty() || !client_id.is_ascii() {
                    return Err(IdentifierError::new(
                        "daemon_secret_path",
                        "unsupported_syntax",
                    ));
                }
                Ok(Self::LocalAuth {
                    installation_id: InstallationId::parse(installation)?,
                    client_kind: LocalClientKind::parse(kind)?,
                    client_id: (*client_id).to_string(),
                })
            }
            _ => Err(IdentifierError::new(
                "daemon_secret_path",
                "unsupported_syntax",
            )),
        }
    }
}

impl fmt::Display for DaemonSecretPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.account())
    }
}

/// Client-owned keyring account path (`secrets.client` service).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ClientSecretPath {
    PairingToken {
        remote_id: WorkshopScopeId,
        session_id: String,
    },
}

impl ClientSecretPath {
    pub fn account(&self) -> String {
        match self {
            Self::PairingToken {
                remote_id,
                session_id,
            } => format!("v1/{}/pairing/{}/token", remote_id.as_str(), session_id),
        }
    }

    pub fn storage_key(&self) -> StorageAuthorityKey {
        StorageAuthorityKey::derive("cx1-", "client-secret-path", &self.account())
    }

    pub fn parse(account: &str) -> Result<Self, IdentifierError> {
        let parts: Vec<&str> = account.split('/').collect();
        match parts.as_slice() {
            ["v1", remote, "pairing", session, "token"] => {
                if session.is_empty() || !is_uuid_hyphenated(session) {
                    return Err(IdentifierError::new(
                        "client_secret_path",
                        "unsupported_syntax",
                    ));
                }
                Ok(Self::PairingToken {
                    remote_id: WorkshopScopeId::parse(remote)?,
                    session_id: (*session).to_ascii_lowercase(),
                })
            }
            _ => Err(IdentifierError::new(
                "client_secret_path",
                "unsupported_syntax",
            )),
        }
    }
}

impl fmt::Display for ClientSecretPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.account())
    }
}

fn is_uuid_hyphenated(value: &str) -> bool {
    // 8-4-4-4-12 lowercase/uppercase hex
    let bytes = value.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    let is_hex = |b: u8| b.is_ascii_hexdigit();
    for (i, &b) in bytes.iter().enumerate() {
        match i {
            8 | 13 | 18 | 23 => {
                if b != b'-' {
                    return false;
                }
            }
            _ => {
                if !is_hex(b) {
                    return false;
                }
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_path_round_trips() {
        let installation = InstallationId::parse("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let connection = ConnectionId::parse("6ba7b810-9dad-11d1-80b4-00c04fd430c8").unwrap();
        let path = DaemonSecretPath::Integration {
            installation_id: installation.clone(),
            connection_id: connection,
            slot: IntegrationSecretSlot::ApiKey,
        };
        let account = path.account();
        assert_eq!(
            account,
            "v1/550e8400-e29b-41d4-a716-446655440000/integration/6ba7b810-9dad-11d1-80b4-00c04fd430c8/api_key"
        );
        assert_eq!(DaemonSecretPath::parse(&account).unwrap(), path);
        assert!(path.storage_key().as_str().starts_with("sk1-"));
    }

    #[test]
    fn client_path_round_trips() {
        let path = ClientSecretPath::PairingToken {
            remote_id: WorkshopScopeId::parse("workshop-device-1").unwrap(),
            session_id: "550e8400-e29b-41d4-a716-446655440000".into(),
        };
        let account = path.account();
        assert_eq!(ClientSecretPath::parse(&account).unwrap(), path);
    }

    #[test]
    fn mcp_oauth_path_round_trips() {
        let path = DaemonSecretPath::McpOAuth {
            installation_id: InstallationId::parse("550e8400-e29b-41d4-a716-446655440000").unwrap(),
            server_id: McpServerId::parse("notion").unwrap(),
        };
        let account = path.account();
        assert_eq!(
            account,
            "v1/550e8400-e29b-41d4-a716-446655440000/mcp/notion/oauth"
        );
        assert_eq!(DaemonSecretPath::parse(&account).unwrap(), path);
    }
}
