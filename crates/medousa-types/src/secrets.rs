//! Typed secret coordinates and integration-connection wire DTOs.
//!
//! Keyring accounts are the `v1/…` grammar. HTTP never carries those account
//! strings or secret values — clients send `connection_id` + `slot`.

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::authority_id::{IdentifierError, StorageAuthorityKey, WorkshopScopeId};

pub const SECRET_PATH_VERSION: &str = "v1";

pub const KIND_DISCORD: &str = "discord";
pub const KIND_TELEGRAM: &str = "telegram";
pub const KIND_SLACK: &str = "slack";
pub const KIND_CHATGPT: &str = "chatgpt";
pub const KIND_APNS: &str = "apns";

macro_rules! uuid_id {
    ($name:ident, $kind:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $name(String);

        impl $name {
            pub fn generate() -> Self {
                Self(uuid::Uuid::new_v4().to_string())
            }

            pub fn parse(value: &str) -> Result<Self, IdentifierError> {
                if !hyphenated_uuid(value) {
                    return Err(IdentifierError::new($kind, "unsupported_syntax"));
                }
                Ok(Self(value.to_ascii_lowercase()))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = String::deserialize(deserializer)?;
                Self::parse(&value).map_err(serde::de::Error::custom)
            }
        }

        #[cfg(feature = "json-schema")]
        impl schemars::JsonSchema for $name {
            fn schema_name() -> String {
                stringify!($name).to_string()
            }

            fn json_schema(generator: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
                String::json_schema(generator)
            }
        }
    };
}

uuid_id!(InstallationId, "installation_id");
uuid_id!(ConnectionId, "connection_id");
uuid_id!(LocalClientId, "local_client_id");
uuid_id!(PairingSessionId, "pairing_session_id");

fn hyphenated_uuid(value: &str) -> bool {
    if value.len() != 36 {
        return false;
    }
    let bytes = value.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        match index {
            8 | 13 | 18 | 23 => {
                if *byte != b'-' {
                    return false;
                }
            }
            _ => {
                if !byte.is_ascii_hexdigit() {
                    return false;
                }
            }
        }
    }
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case")]
pub enum LocalClientKind {
    #[serde(rename = "home-local")]
    HomeLocal,
    #[serde(rename = "medousa-cli")]
    MedousaCli,
    #[serde(rename = "medousa-tui")]
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
            _ => Err(IdentifierError::new("local_client_kind", "unsupported_syntax")),
        }
    }
}

impl fmt::Display for LocalClientKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

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

    pub fn all() -> &'static [Self] {
        &[
            Self::ApiKey,
            Self::OauthBundle,
            Self::BotToken,
            Self::AppToken,
            Self::AuthKey,
        ]
    }
}

impl fmt::Display for IntegrationSecretSlot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DaemonSecretPath {
    RuntimeSurrealPassword {
        installation: InstallationId,
    },
    Integration {
        installation: InstallationId,
        connection: ConnectionId,
        slot: IntegrationSecretSlot,
    },
    LocalAuth {
        installation: InstallationId,
        client_kind: LocalClientKind,
        client_id: LocalClientId,
    },
}

impl DaemonSecretPath {
    pub fn surreal_password(installation: InstallationId) -> Self {
        Self::RuntimeSurrealPassword { installation }
    }

    pub fn integration(
        installation: InstallationId,
        connection: ConnectionId,
        slot: IntegrationSecretSlot,
    ) -> Self {
        Self::Integration {
            installation,
            connection,
            slot,
        }
    }

    pub fn local_auth(
        installation: InstallationId,
        client_kind: LocalClientKind,
        client_id: LocalClientId,
    ) -> Self {
        Self::LocalAuth {
            installation,
            client_kind,
            client_id,
        }
    }

    pub fn keyring_account(&self) -> String {
        match self {
            Self::RuntimeSurrealPassword { installation } => {
                format!("{SECRET_PATH_VERSION}/{installation}/runtime/surreal/password")
            }
            Self::Integration {
                installation,
                connection,
                slot,
            } => format!(
                "{SECRET_PATH_VERSION}/{installation}/integration/{connection}/{slot}"
            ),
            Self::LocalAuth {
                installation,
                client_kind,
                client_id,
            } => format!(
                "{SECRET_PATH_VERSION}/{installation}/local-auth/{client_kind}/{client_id}/token"
            ),
        }
    }

    pub fn storage_key(&self) -> StorageAuthorityKey {
        StorageAuthorityKey::for_secret_path(&self.keyring_account())
    }

    pub fn parse(account: &str) -> Result<Self, IdentifierError> {
        let parts: Vec<&str> = account.split('/').collect();
        match parts.as_slice() {
            [SECRET_PATH_VERSION, installation, "runtime", "surreal", "password"] => {
                Ok(Self::RuntimeSurrealPassword {
                    installation: InstallationId::parse(installation)?,
                })
            }
            [SECRET_PATH_VERSION, installation, "integration", connection, slot] => {
                Ok(Self::Integration {
                    installation: InstallationId::parse(installation)?,
                    connection: ConnectionId::parse(connection)?,
                    slot: IntegrationSecretSlot::parse(slot)?,
                })
            }
            [SECRET_PATH_VERSION, installation, "local-auth", client_kind, client_id, "token"] => {
                Ok(Self::LocalAuth {
                    installation: InstallationId::parse(installation)?,
                    client_kind: LocalClientKind::parse(client_kind)?,
                    client_id: LocalClientId::parse(client_id)?,
                })
            }
            _ => Err(IdentifierError::new("daemon_secret_path", "unsupported_syntax")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ClientSecretPath {
    Pairing {
        remote: WorkshopScopeId,
        session: PairingSessionId,
    },
}

impl ClientSecretPath {
    pub fn pairing(remote: WorkshopScopeId, session: PairingSessionId) -> Self {
        Self::Pairing { remote, session }
    }

    pub fn keyring_account(&self) -> String {
        match self {
            Self::Pairing { remote, session } => {
                format!("{SECRET_PATH_VERSION}/{remote}/pairing/{session}/token")
            }
        }
    }

    pub fn storage_key(&self) -> StorageAuthorityKey {
        StorageAuthorityKey::for_secret_path(&self.keyring_account())
    }

    pub fn parse(account: &str) -> Result<Self, IdentifierError> {
        let parts: Vec<&str> = account.split('/').collect();
        match parts.as_slice() {
            [SECRET_PATH_VERSION, remote, "pairing", session, "token"] => {
                if *remote == "." || *remote == ".." {
                    return Err(IdentifierError::new(
                        "client_secret_path",
                        "unsupported_syntax",
                    ));
                }
                Ok(Self::Pairing {
                    remote: WorkshopScopeId::parse(remote)?,
                    session: PairingSessionId::parse(session)?,
                })
            }
            _ => Err(IdentifierError::new("client_secret_path", "unsupported_syntax")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
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
    pub fn slot(&self, slot: IntegrationSecretSlot) -> bool {
        match slot {
            IntegrationSecretSlot::ApiKey => self.api_key,
            IntegrationSecretSlot::OauthBundle => self.oauth_bundle,
            IntegrationSecretSlot::BotToken => self.bot_token,
            IntegrationSecretSlot::AppToken => self.app_token,
            IntegrationSecretSlot::AuthKey => self.auth_key,
        }
    }

    pub fn set_slot(&mut self, slot: IntegrationSecretSlot, present: bool) {
        match slot {
            IntegrationSecretSlot::ApiKey => self.api_key = present,
            IntegrationSecretSlot::OauthBundle => self.oauth_bundle = present,
            IntegrationSecretSlot::BotToken => self.bot_token = present,
            IntegrationSecretSlot::AppToken => self.app_token = present,
            IntegrationSecretSlot::AuthKey => self.auth_key = present,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct IntegrationConnection {
    pub connection_id: String,
    pub kind: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    pub secrets: IntegrationSecretStatus,
    pub created_at_utc: DateTime<Utc>,
    pub updated_at_utc: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct IntegrationListResponse {
    pub connections: Vec<IntegrationConnection>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct CreateIntegrationRequest {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct PatchIntegrationRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct DeleteIntegrationResponse {
    pub deleted: bool,
    pub connection_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct UpsertIntegrationSecretRequest {
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct IntegrationSecretMutationResponse {
    pub connection_id: String,
    pub slot: IntegrationSecretSlot,
    pub configured: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_paths_round_trip_and_stay_readable() {
        let installation = InstallationId::parse("01234567-89ab-cdef-0123-456789abcdef").unwrap();
        let connection = ConnectionId::parse("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee").unwrap();
        let path = DaemonSecretPath::integration(
            installation.clone(),
            connection,
            IntegrationSecretSlot::ApiKey,
        );
        let account = path.keyring_account();
        assert_eq!(
            account,
            "v1/01234567-89ab-cdef-0123-456789abcdef/integration/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee/api_key"
        );
        assert_eq!(DaemonSecretPath::parse(&account).unwrap(), path);
        assert!(path.storage_key().as_str().starts_with("sp1-"));
        assert!(!path.storage_key().as_str().contains("api_key"));
    }

    #[test]
    fn local_auth_and_pairing_paths_reject_hostile_segments() {
        assert!(DaemonSecretPath::parse("v1/not-a-uuid/runtime/surreal/password").is_err());
        assert!(ClientSecretPath::parse("v1/../pairing/01234567-89ab-cdef-0123-456789abcdef/token").is_err());
        assert!(InstallationId::parse("OPENAI").is_err());
        assert!(IntegrationSecretSlot::parse("openai").is_err());
    }

    #[test]
    fn uuid_ids_are_case_normalized() {
        let id = InstallationId::parse("01234567-89AB-CDEF-0123-456789ABCDEF").unwrap();
        assert_eq!(id.as_str(), "01234567-89ab-cdef-0123-456789abcdef");
    }
}
