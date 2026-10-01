use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::authority_id::IdentifierError;
use crate::coordination::ExternalPeerRuntime;
use crate::daemon_api::AgentModeId;

pub const BOT_PROFILE_SCHEMA_VERSION: u32 = 3;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ExternalAgentSessionContract {
    FreshPerJob,
}

/// Owner-pinned execution authority for an external-agent Bot. `forge_work_id`
/// is resolved on the home workshop; caller-supplied host paths are forbidden.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ExternalAgentExecutor {
    /// Installed ACP runtime, distinct from an inference provider.
    pub runtime: ExternalPeerRuntime,
    pub home_workshop_id: String,
    pub forge_work_id: String,
    pub forge_repo_id: String,
    pub session_contract: ExternalAgentSessionContract,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_tools: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_capabilities: Vec<String>,
}

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

/// Durable world shape currently supported by Bot continuity.
///
/// This deliberately excludes attached human browser and desktop worlds. A
/// Bot may retain only a daemon-owned isolated browser profile whose runtime
/// can re-establish authority after a restart.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum BotWorldBindingKind {
    PersistentBrowser,
}

/// Explicit durable world selected by the Bot owner.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct BotWorldBinding {
    pub kind: BotWorldBindingKind,
    pub world_id: String,
    pub execution_runtime_id: String,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world_binding: Option<BotWorldBinding>,
    /// Configuration only. Each execution still requires placement, pairing,
    /// destination policy, and runtime admission.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_agent: Option<ExternalAgentExecutor>,
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
    /// Opt-in durable world continuity. Omission grants no world.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world_binding: Option<BotWorldBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_agent: Option<ExternalAgentExecutor>,
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
    /// Set or replace durable world continuity. Omission preserves the current
    /// binding for compatibility with clients predating schema v2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world_binding: Option<BotWorldBinding>,
    /// Explicitly clear durable continuity. This cannot be combined with
    /// `world_binding`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub clear_world_binding: bool,
    /// Omission preserves the current executor for older clients.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_agent: Option<ExternalAgentExecutor>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub clear_external_agent: bool,
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

    #[test]
    fn external_executor_is_an_installed_runtime_with_pinned_authority() {
        let executor = ExternalAgentExecutor {
            runtime: ExternalPeerRuntime::Codex,
            home_workshop_id: "workshop-mini".into(),
            forge_work_id: "work-project".into(),
            forge_repo_id: "repo-project".into(),
            session_contract: ExternalAgentSessionContract::FreshPerJob,
            allowed_tools: vec!["code".into()],
            allowed_capabilities: vec!["forge".into()],
        };
        let value = serde_json::to_value(&executor).unwrap();
        assert_eq!(value["runtime"], "codex");
        assert_eq!(value["home_workshop_id"], "workshop-mini");
        assert_eq!(value["session_contract"], "fresh_per_job");
        assert!(
            serde_json::from_value::<ExternalAgentExecutor>(serde_json::json!({
                "runtime": "openai-codex",
                "home_workshop_id": "workshop-mini",
            "forge_work_id": "work-project",
            "forge_repo_id": "repo-project",
                "session_contract": "fresh_per_job"
            }))
            .is_err()
        );
        assert_eq!(
            serde_json::from_value::<ExternalAgentExecutor>(value).unwrap(),
            executor
        );
    }
}
