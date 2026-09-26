//! Typed, provider-independent perception decisions for foreground turns.
//!
//! System 1 may recommend an execution posture, but it never grants authority.
//! Admission, policy, tool allowlists, and runtime limits remain authoritative.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

pub const SYSTEM_ONE_SCHEMA_VERSION: u8 = 1;
pub const SYSTEM_ONE_MODE_ENV: &str = "MEDOUSA_SYSTEM_ONE_MODE";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnIntent {
    Conversational,
    ToolRequired,
    Clarify,
    Mixed,
}

impl TurnIntent {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Conversational => "conversational",
            Self::ToolRequired => "tool_required",
            Self::Clarify => "clarify",
            Self::Mixed => "mixed",
        }
    }
}

impl FromStr for TurnIntent {
    type Err = SystemOneError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "conversational" => Ok(Self::Conversational),
            "tool_required" => Ok(Self::ToolRequired),
            "clarify" => Ok(Self::Clarify),
            "mixed" => Ok(Self::Mixed),
            other => Err(SystemOneError::UnknownIntent(other.to_string())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemOneInput {
    pub current_user_message: String,
    #[serde(default)]
    pub recent_context: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemOneDecision {
    pub schema_version: u8,
    pub intent: TurnIntent,
    pub confidence: f32,
    pub reason: String,
    pub engine: String,
}

impl SystemOneDecision {
    pub fn new(
        intent: TurnIntent,
        confidence: f32,
        reason: impl Into<String>,
        engine: impl Into<String>,
    ) -> Result<Self, SystemOneError> {
        if !confidence.is_finite() || !(0.0..=1.0).contains(&confidence) {
            return Err(SystemOneError::InvalidConfidence);
        }
        let reason = reason
            .into()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(160)
            .collect();
        let engine: String = engine
            .into()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join("-")
            .chars()
            .take(64)
            .collect();
        if engine.is_empty() {
            return Err(SystemOneError::MissingEngine);
        }
        Ok(Self {
            schema_version: SYSTEM_ONE_SCHEMA_VERSION,
            intent,
            confidence,
            reason,
            engine,
        })
    }

    pub fn is_valid(&self) -> bool {
        self.schema_version == SYSTEM_ONE_SCHEMA_VERSION
            && self.confidence.is_finite()
            && (0.0..=1.0).contains(&self.confidence)
            && !self.engine.trim().is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemOneRecommendation {
    KeepHeuristic,
    PreferNoTools,
    PreferTools,
    AskClarification,
}

impl SystemOneRecommendation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::KeepHeuristic => "keep_heuristic",
            Self::PreferNoTools => "prefer_no_tools",
            Self::PreferTools => "prefer_tools",
            Self::AskClarification => "ask_clarification",
        }
    }
}

pub const SYSTEM_ONE_EVALUATION_SCHEMA_VERSION: u8 = 1;

/// A bounded, locally persisted comparison row for calibrating System 1.
///
/// `reference_intent` is a weak label from the pre-existing activation heuristic,
/// not human-adjudicated ground truth. Keeping the source explicit prevents
/// calibration tooling from silently treating it as such.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemOneEvaluationRecord {
    pub schema_version: u8,
    pub recorded_at: DateTime<Utc>,
    pub session_id: String,
    pub turn_id: u64,
    pub mode: SystemOneMode,
    pub input: SystemOneInput,
    pub decision: SystemOneDecision,
    pub recommendation: SystemOneRecommendation,
    pub reference_intent: TurnIntent,
    pub reference_source: String,
    pub reference_reason: String,
    pub agrees_with_reference: bool,
}

impl SystemOneEvaluationRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        recorded_at: DateTime<Utc>,
        session_id: impl Into<String>,
        turn_id: u64,
        mode: SystemOneMode,
        input: SystemOneInput,
        decision: SystemOneDecision,
        recommendation: SystemOneRecommendation,
        reference_intent: TurnIntent,
        reference_reason: impl Into<String>,
    ) -> Self {
        let agrees_with_reference = decision.intent == reference_intent;
        Self {
            schema_version: SYSTEM_ONE_EVALUATION_SCHEMA_VERSION,
            recorded_at,
            session_id: session_id.into(),
            turn_id,
            mode,
            input,
            decision,
            recommendation,
            reference_intent,
            reference_source: "activation_heuristic".to_string(),
            reference_reason: reference_reason.into(),
            agrees_with_reference,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SystemOnePolicy {
    pub low_confidence: f32,
    pub conversational: f32,
    pub tool_required: f32,
}

impl Default for SystemOnePolicy {
    fn default() -> Self {
        Self {
            low_confidence: 0.45,
            conversational: 0.55,
            tool_required: 0.60,
        }
    }
}

impl SystemOnePolicy {
    pub fn recommend(self, decision: &SystemOneDecision) -> SystemOneRecommendation {
        if !decision.is_valid() || decision.confidence < self.low_confidence {
            return SystemOneRecommendation::KeepHeuristic;
        }
        match decision.intent {
            TurnIntent::Conversational if decision.confidence >= self.conversational => {
                SystemOneRecommendation::PreferNoTools
            }
            TurnIntent::ToolRequired if decision.confidence >= self.tool_required => {
                SystemOneRecommendation::PreferTools
            }
            TurnIntent::Clarify => SystemOneRecommendation::AskClarification,
            TurnIntent::Conversational | TurnIntent::ToolRequired | TurnIntent::Mixed => {
                SystemOneRecommendation::KeepHeuristic
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemOneMode {
    #[default]
    Shadow,
    Active,
    Disabled,
}

impl SystemOneMode {
    pub fn from_env() -> Self {
        std::env::var(SYSTEM_ONE_MODE_ENV)
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or_default()
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Shadow => "shadow",
            Self::Active => "active",
            Self::Disabled => "disabled",
        }
    }

    pub const fn may_mutate_execution(self) -> bool {
        matches!(self, Self::Active)
    }
}

impl FromStr for SystemOneMode {
    type Err = SystemOneError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "shadow" | "" => Ok(Self::Shadow),
            "active" => Ok(Self::Active),
            "disabled" | "off" => Ok(Self::Disabled),
            other => Err(SystemOneError::UnknownMode(other.to_string())),
        }
    }
}

#[async_trait]
pub trait SystemOneEngine: Send + Sync {
    fn id(&self) -> &str;

    async fn decide(&self, input: &SystemOneInput) -> Result<SystemOneDecision, SystemOneError>;
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SystemOneError {
    #[error("unknown System 1 intent: {0}")]
    UnknownIntent(String),
    #[error("unknown System 1 mode: {0}")]
    UnknownMode(String),
    #[error("System 1 confidence must be finite and between zero and one")]
    InvalidConfidence,
    #[error("System 1 engine id is required")]
    MissingEngine,
    #[error("System 1 engine failed: {0}")]
    Engine(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decision(intent: TurnIntent, confidence: f32) -> SystemOneDecision {
        SystemOneDecision::new(intent, confidence, "test", "test-engine").unwrap()
    }

    #[test]
    fn policy_is_conservative_below_thresholds() {
        let policy = SystemOnePolicy::default();
        assert_eq!(
            policy.recommend(&decision(TurnIntent::ToolRequired, 0.59)),
            SystemOneRecommendation::KeepHeuristic
        );
        assert_eq!(
            policy.recommend(&decision(TurnIntent::Conversational, 0.54)),
            SystemOneRecommendation::KeepHeuristic
        );
    }

    #[test]
    fn policy_maps_confident_typed_intents() {
        let policy = SystemOnePolicy::default();
        assert_eq!(
            policy.recommend(&decision(TurnIntent::ToolRequired, 0.60)),
            SystemOneRecommendation::PreferTools
        );
        assert_eq!(
            policy.recommend(&decision(TurnIntent::Conversational, 0.55)),
            SystemOneRecommendation::PreferNoTools
        );
        assert_eq!(
            policy.recommend(&decision(TurnIntent::Clarify, 0.45)),
            SystemOneRecommendation::AskClarification
        );
    }

    #[test]
    fn defaults_to_shadow_and_requires_explicit_active_mode() {
        assert_eq!(SystemOneMode::default(), SystemOneMode::Shadow);
        assert!(!SystemOneMode::Shadow.may_mutate_execution());
        assert!(
            "active"
                .parse::<SystemOneMode>()
                .unwrap()
                .may_mutate_execution()
        );
    }

    #[test]
    fn decision_rejects_invalid_confidence() {
        assert_eq!(
            SystemOneDecision::new(TurnIntent::Mixed, f32::NAN, "bad", "test"),
            Err(SystemOneError::InvalidConfidence)
        );
    }

    #[test]
    fn policy_fails_closed_for_unvalidated_decisions() {
        let malformed = SystemOneDecision {
            schema_version: SYSTEM_ONE_SCHEMA_VERSION,
            intent: TurnIntent::Clarify,
            confidence: f32::NAN,
            reason: "bad".to_string(),
            engine: "test".to_string(),
        };
        assert_eq!(
            SystemOnePolicy::default().recommend(&malformed),
            SystemOneRecommendation::KeepHeuristic
        );
    }

    #[test]
    fn decision_normalizes_observability_fields() {
        let normalized =
            SystemOneDecision::new(TurnIntent::Mixed, 0.5, "two\n lines", "model engine").unwrap();
        assert_eq!(normalized.reason, "two lines");
        assert_eq!(normalized.engine, "model-engine");
    }
}
