use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest as _, Sha256};
use std::fmt;
use std::str::FromStr;

use crate::inference::InferenceProfilesConfig;
use crate::secrets::InstallationId;
use crate::stage_routing::StageRoutingMatrix;
use crate::turn::TurnPart;
use crate::turn::TurnSliceSummary;

pub const MAX_SESSION_ID_BYTES: usize = 128;
pub const MAX_EXECUTION_ID_BYTES: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidSessionId {
    reason: &'static str,
}

impl InvalidSessionId {
    pub fn reason(&self) -> &'static str {
        self.reason
    }
}

impl fmt::Display for InvalidSessionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid session id: {}", self.reason)
    }
}

impl std::error::Error for InvalidSessionId {}

/// Canonical chat session identifier. Construction never normalizes input.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct SessionId(String);

impl SessionId {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, InvalidSessionId> {
        let value = value.as_ref();
        validate_session_id(value)?;
        Ok(Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl AsRef<str> for SessionId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl FromStr for SessionId {
    type Err = InvalidSessionId;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl TryFrom<String> for SessionId {
    type Error = InvalidSessionId;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        validate_session_id(&value)?;
        Ok(Self(value))
    }
}

impl Serialize for SessionId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for SessionId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::try_from(value).map_err(serde::de::Error::custom)
    }
}

pub fn validate_session_id(session_id: &str) -> Result<&str, InvalidSessionId> {
    if session_id.is_empty() {
        return Err(InvalidSessionId { reason: "empty" });
    }
    if session_id.len() > MAX_SESSION_ID_BYTES {
        return Err(InvalidSessionId { reason: "too_long" });
    }
    if !session_id.is_ascii() {
        return Err(InvalidSessionId {
            reason: "non_ascii",
        });
    }
    if !session_id
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(InvalidSessionId {
            reason: "invalid_character",
        });
    }
    if is_windows_device_name(session_id) {
        return Err(InvalidSessionId {
            reason: "platform_alias",
        });
    }
    Ok(session_id)
}

fn is_windows_device_name(session_id: &str) -> bool {
    let upper = session_id.to_ascii_uppercase();
    matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || upper
            .strip_prefix("COM")
            .or_else(|| upper.strip_prefix("LPT"))
            .is_some_and(|suffix| suffix.len() == 1 && matches!(suffix.as_bytes()[0], b'1'..=b'9'))
}

macro_rules! prefixed_conversation_id {
    ($name:ident, $kind:literal, $prefix:literal, $hex_len:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        #[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
        pub struct $name(String);

        impl $name {
            pub fn parse(
                value: impl AsRef<str>,
            ) -> Result<Self, crate::authority_id::IdentifierError> {
                let value = value.as_ref();
                let Some(suffix) = value.strip_prefix($prefix) else {
                    return Err(crate::authority_id::IdentifierError::new(
                        $kind,
                        "unsupported_syntax",
                    ));
                };
                if suffix.len() != $hex_len
                    || !suffix
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
                {
                    return Err(crate::authority_id::IdentifierError::new(
                        $kind,
                        "unsupported_syntax",
                    ));
                }
                Ok(Self(value.to_string()))
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

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl FromStr for $name {
            type Err = crate::authority_id::IdentifierError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::parse(value)
            }
        }

        impl TryFrom<String> for $name {
            type Error = crate::authority_id::IdentifierError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::parse(value)
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::try_from(value).map_err(serde::de::Error::custom)
            }
        }
    };
}

prefixed_conversation_id!(AuthorityId, "authority_id", "auth_", 64);
prefixed_conversation_id!(TranscriptEntryId, "transcript_entry_id", "ent_", 32);
prefixed_conversation_id!(ContextManifestId, "context_manifest_id", "ctx_", 32);
prefixed_conversation_id!(DerivationId, "derivation_id", "drv_", 32);

impl AuthorityId {
    /// Stable logical workshop authority derived from the installation identity.
    /// Domain separation keeps the installation id itself off ordinary session APIs.
    pub fn from_installation_id(installation_id: &InstallationId) -> Self {
        let mut digest = Sha256::new();
        digest.update(b"medousa/workshop-authority/v1\0");
        digest.update(installation_id.as_str().as_bytes());
        Self(format!("auth_{:x}", digest.finalize()))
    }
}

/// Durable execution identity. Legacy execution ids predate a generated prefix,
/// so validation accepts bounded visible ASCII.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ExecutionId(String);

impl ExecutionId {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, crate::authority_id::IdentifierError> {
        let value = value.as_ref();
        if value.is_empty()
            || value.len() > MAX_EXECUTION_ID_BYTES
            || !value.bytes().all(|byte| matches!(byte, 0x21..=0x7e))
        {
            return Err(crate::authority_id::IdentifierError::new(
                "execution_id",
                "unsupported_syntax",
            ));
        }
        Ok(Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ExecutionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Serialize for ExecutionId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ExecutionId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct SessionRef {
    pub authority_id: AuthorityId,
    pub session_id: SessionId,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct TranscriptEntryRef {
    pub session: SessionRef,
    pub entry_id: TranscriptEntryId,
    #[cfg_attr(feature = "json-schema", schemars(range(min = 1)))]
    pub entry_seq: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ExecutionRef {
    pub authority_id: AuthorityId,
    pub session_id: SessionId,
    pub execution_id: ExecutionId,
}

/// One immutable transcript payload at an ordered position in a session.
/// Session authority and session id are carried by the enclosing history
/// response; callers can combine them with these coordinates into a
/// `TranscriptEntryRef`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct TranscriptEntry {
    pub entry_id: TranscriptEntryId,
    #[cfg_attr(feature = "json-schema", schemars(range(min = 1)))]
    pub entry_seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caused_by: Option<ExecutionRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<TranscriptEntryRef>,
    pub content_digest: String,
    #[serde(flatten)]
    pub turn: ConversationTurn,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ConversationTurn {
    pub role: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub tool_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parts: Option<Vec<TurnPart>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slice_summary: Option<TurnSliceSummary>,
    /// Shared-room human speaker (`user:alice`). Absent on assistant turns / personal chats.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker_profile_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct TuiDefaults {
    pub backend: Option<String>,
    pub theme_id: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub env_overrides: Option<String>,
    pub allowed_modules: Option<Vec<String>>,
    pub tool_call_mode: Option<String>,
    pub max_tool_rounds: Option<usize>,
    pub host_bus_max_tool_rounds: Option<usize>,
    pub host_turn_bus_mode: Option<String>,
    pub activation_tool_intent_max_rounds: Option<usize>,
    pub activation_short_turn_max_tool_rounds: Option<usize>,
    pub continuation_max_tool_rounds: Option<usize>,
    pub max_text_only_stuck_continues: Option<usize>,
    pub classifier_restricted_max_tool_rounds: Option<usize>,
    pub thinking_capture: Option<bool>,
    #[serde(default)]
    pub stasis_otel_enabled: Option<bool>,
    pub thinking_max_lines: Option<usize>,
    pub activation_direct_answer_max_prompt_chars: Option<usize>,
    pub activation_long_session_turn_threshold: Option<usize>,
    pub activation_long_session_max_prompt_chars: Option<usize>,
    pub slice_hot_window_turns: Option<usize>,
    pub slice_cold_window_turns: Option<usize>,
    pub retry_runtime_max_retries: Option<usize>,
    pub retry_runtime_max_rounds: Option<usize>,
    pub verifier_min_citation_coverage: Option<f32>,
    pub verifier_min_avg_support_strength: Option<f32>,
    pub verifier_min_supported_claim_ratio: Option<f32>,
    pub verifier_min_claim_support_strength: Option<f32>,
    pub response_depth_mode: Option<String>,
    pub reasoning_effort: Option<String>,
    pub stage_routing: Option<StageRoutingMatrix>,
    pub command_usage_counts: Option<std::collections::HashMap<String, u64>>,
    pub web_search_preferred_provider: Option<String>,
    pub web_search_try_fallbacks: Option<bool>,
    #[serde(default)]
    pub work_card_hide_after_hours: Option<u32>,
    #[serde(default)]
    pub work_card_wipe_after_days: Option<u32>,
    pub surreal_endpoint: Option<String>,
    pub surreal_username: Option<String>,
    pub surreal_password: Option<String>,
    pub surreal_namespace: Option<String>,
    pub surreal_database: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inference_profiles: Option<InferenceProfilesConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stt_provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stt_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stt_base_url: Option<String>,
    /// Master switch for `cognition_shell_*` agent tools (default off — sensitive).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell_agent_tools_enabled: Option<bool>,
    /// Vault Versions (Git-backed). Off by default; product language is "Versions".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault_git_enabled: Option<bool>,
    /// Charter ceiling for shell network access (default off).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell_network_default: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell_timeout_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell_max_output_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell_allowed_binaries: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell_writable_roots: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct SessionHistorySummary {
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub turns: usize,
    pub verification_runs: usize,
    pub last_timestamp: Option<DateTime<Utc>>,
    pub last_verification_timestamp: Option<DateTime<Utc>>,
    pub last_verification_confidence: Option<f32>,
    pub last_verification_coverage: Option<f32>,
    pub last_verification_verified: Option<bool>,
    pub preview: String,
    /// `shared` when indexed in the multi-member catalog; omitted for single-seat chats.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog: Option<String>,
    /// First sticky non-home host surface (`vscode` | `neovim` | `obsidian` | `browser`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_surface: Option<String>,
    /// Sticky once a Forge code binding was set on the session.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub has_code_work: bool,
}

impl SessionHistorySummary {
    pub fn without_verification_fields(mut self) -> Self {
        self.verification_runs = 0;
        self.last_verification_timestamp = None;
        self.last_verification_confidence = None;
        self.last_verification_coverage = None;
        self.last_verification_verified = None;
        self
    }
}

impl ConversationTurn {
    pub fn plain(
        role: impl Into<String>,
        content: String,
        timestamp: DateTime<Utc>,
        tool_names: Vec<String>,
        answer_state: Option<String>,
    ) -> Self {
        Self {
            role: role.into(),
            content,
            timestamp,
            tool_names,
            answer_state,
            parts: None,
            slice_summary: None,
            speaker_profile_id: None,
        }
    }

    pub fn with_speaker_profile_id(mut self, profile_id: impl Into<String>) -> Self {
        let trimmed = profile_id.into();
        let trimmed = trimmed.trim();
        self.speaker_profile_id = (!trimmed.is_empty()).then(|| trimmed.to_string());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_id_serde_round_trips_as_a_string() {
        let session_id = SessionId::parse("ses_0123456789abcdef").unwrap();
        let encoded = serde_json::to_string(&session_id).unwrap();
        assert_eq!(encoded, "\"ses_0123456789abcdef\"");
        assert_eq!(
            serde_json::from_str::<SessionId>(&encoded).unwrap(),
            session_id
        );
    }

    #[test]
    fn session_id_deserialization_cannot_bypass_validation() {
        for encoded in ["\"../outside\"", "\" session\"", "\"nul\""] {
            assert!(serde_json::from_str::<SessionId>(encoded).is_err());
        }
    }

    #[test]
    fn conversation_coordinate_ids_validate_and_round_trip() {
        let authority = AuthorityId::parse(format!("auth_{}", "a".repeat(64))).unwrap();
        let entry = TranscriptEntryId::parse(format!("ent_{}", "b".repeat(32))).unwrap();
        let reference = TranscriptEntryRef {
            session: SessionRef {
                authority_id: authority,
                session_id: SessionId::parse("ses_0123456789abcdef").unwrap(),
            },
            entry_id: entry,
            entry_seq: 7,
        };
        let encoded = serde_json::to_string(&reference).unwrap();
        assert_eq!(
            serde_json::from_str::<TranscriptEntryRef>(&encoded).unwrap(),
            reference
        );
    }

    #[test]
    fn conversation_coordinate_deserialization_rejects_invalid_ids() {
        assert!(
            serde_json::from_str::<AuthorityId>(&format!("\"auth_{}\"", "A".repeat(64))).is_err()
        );
        assert!(serde_json::from_str::<TranscriptEntryId>("\"ent_not-hex\"").is_err());
        assert!(
            serde_json::from_str::<ContextManifestId>(&format!("\"ctx_{}\"", "0".repeat(31)))
                .is_err()
        );
    }

    #[test]
    fn authority_is_stable_and_domain_separated_from_installation_id() {
        let installation = InstallationId::parse("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let first = AuthorityId::from_installation_id(&installation);
        let second = AuthorityId::from_installation_id(&installation);
        assert_eq!(first, second);
        assert_ne!(first.as_str(), installation.as_str());
        assert!(first.as_str().starts_with("auth_"));
    }

    #[test]
    fn transcript_entry_keeps_legacy_turn_fields_flat_on_the_wire() {
        let entry = TranscriptEntry {
            entry_id: TranscriptEntryId::parse(format!("ent_{}", "c".repeat(32))).unwrap(),
            entry_seq: 2,
            caused_by: None,
            source: None,
            content_digest: "sha256:abc".to_string(),
            turn: ConversationTurn::plain(
                "user",
                "hello".to_string(),
                Utc::now(),
                Vec::new(),
                None,
            ),
        };
        let value = serde_json::to_value(&entry).unwrap();
        assert_eq!(value["role"], "user");
        assert_eq!(value["content"], "hello");
        assert_eq!(value["entry_seq"], 2);
        assert!(value.get("turn").is_none());
    }
}
