use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::authority_id::IdentifierError;
use crate::daemon_api::AgentModeId;

pub const BOT_PROFILE_SCHEMA_VERSION: u32 = 1;

/// Stable daemon-issued identity for one durable Bot profile.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct BotId(String);

impl BotId {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, IdentifierError> {
        let value = value.as_ref();
        let Some(suffix) = value.strip_prefix("bot_") else {
            return Err(IdentifierError::new("bot_id", "unsupported_syntax"));
        };
        if suffix.len() != 32
            || !suffix
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        {
            return Err(IdentifierError::new("bot_id", "unsupported_syntax"));
        }
        Ok(Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BotId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl AsRef<str> for BotId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl FromStr for BotId {
    type Err = IdentifierError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl TryFrom<String> for BotId {
    type Error = IdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl Serialize for BotId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for BotId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::try_from(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum BotSessionKind {
    Primary,
    Secondary,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct BotProfile {
    pub schema_version: u32,
    pub bot_id: BotId,
    pub owner_profile_id: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role_description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_ref: Option<String>,
    pub primary_manuscript_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_manuscript_ids: Vec<String>,
    pub memory_scope_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_mode: Option<AgentModeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_session_id: Option<String>,
    #[serde(default)]
    pub archived: bool,
    pub revision: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct BotSessionBinding {
    pub bot_id: BotId,
    pub session_id: String,
    pub kind: BotSessionKind,
    pub bot_revision_at_bind: u64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct CreateBotRequest {
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role_description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_ref: Option<String>,
    pub primary_manuscript_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_manuscript_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_mode: Option<AgentModeId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct UpdateBotRequest {
    pub expected_revision: u64,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role_description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_ref: Option<String>,
    pub primary_manuscript_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_manuscript_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_mode: Option<AgentModeId>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct DuplicateBotRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct SetBotArchivedRequest {
    pub archived: bool,
    pub expected_revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct SetSessionBotRequest {
    pub bot_id: BotId,
    #[serde(default = "default_secondary_kind")]
    pub kind: BotSessionKind,
}

fn default_secondary_kind() -> BotSessionKind {
    BotSessionKind::Secondary
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct BotListResponse {
    pub bots: Vec<BotProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct BotOpenResponse {
    pub bot: BotProfile,
    pub binding: BotSessionBinding,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct SessionBotResponse {
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding: Option<BotSessionBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bot: Option<BotProfile>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bot_id_requires_canonical_daemon_syntax() {
        let valid = "bot_0123456789abcdef0123456789abcdef";
        assert_eq!(BotId::parse(valid).unwrap().as_str(), valid);
        assert!(BotId::parse("bot_ABCDEF0123456789abcdef0123456789").is_err());
        assert!(BotId::parse("session_0123456789abcdef0123456789abcdef").is_err());
    }
}
