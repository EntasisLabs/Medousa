//! Location-neutral workshop worker request contract.

use std::collections::BTreeSet;
use std::fmt;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use stasis::domain::runtime::placement::{PlacementConstraints, WorkerCapabilities};

use crate::public_api::COGNITION_WORKSHOP_MUTATE;
use crate::schema_api::{TypedActionSchema, typed_action_schema};
use crate::typed_tools::ToolId;

const WORKSHOP_MUTATE_ID: ToolId = ToolId::new(COGNITION_WORKSHOP_MUTATE);

pub const UNKNOWN_EXECUTION_RUNTIME_ID: &str = "unknown";

/// Public, location-neutral requirements used by automatic target selection.
/// Internally these compile directly into Stasis placement constraints.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ExecutionTargetRequirements {
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub required_capabilities: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub architecture: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
}

impl ExecutionTargetRequirements {
    pub fn to_placement_constraints(&self) -> PlacementConstraints {
        PlacementConstraints {
            required_capabilities: self.required_capabilities.clone(),
            platform: self.platform.clone(),
            architecture: self.architecture.clone(),
            region: self.region.clone(),
            target_node: None,
        }
    }
}

/// Where a worker should execute. Omission at the API boundary is equivalent
/// to `same_as_parent` unless a deployment supplies a documented migration
/// default for an older single-target route.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExecutionTargetSelection {
    #[default]
    SameAsParent,
    Exact {
        runtime_id: String,
    },
    Auto {
        #[serde(default)]
        requirements: ExecutionTargetRequirements,
    },
}

impl ExecutionTargetSelection {
    pub fn exact_runtime_id(&self) -> Option<&str> {
        match self {
            Self::Exact { runtime_id } => Some(runtime_id),
            Self::SameAsParent | Self::Auto { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionResolutionReason {
    SameAsParent,
    ExactTarget,
    AutoMatch,
    IngressDefault,
    #[default]
    LegacyUnknown,
}

fn legacy_resolution_time() -> DateTime<Utc> {
    DateTime::UNIX_EPOCH
}

/// Immutable placement provenance captured before a worker is enqueued.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ExecutionPlacementResolution {
    #[serde(default)]
    pub requested: ExecutionTargetSelection,
    #[serde(default = "default_unknown_runtime_id")]
    pub resolved_runtime_id: String,
    #[serde(default)]
    pub resolution_reason: ExecutionResolutionReason,
    #[serde(default = "legacy_resolution_time")]
    pub resolved_at: DateTime<Utc>,
}

impl Default for ExecutionPlacementResolution {
    fn default() -> Self {
        Self {
            requested: ExecutionTargetSelection::SameAsParent,
            resolved_runtime_id: default_unknown_runtime_id(),
            resolution_reason: ExecutionResolutionReason::LegacyUnknown,
            resolved_at: legacy_resolution_time(),
        }
    }
}

impl ExecutionPlacementResolution {
    pub fn resolved(
        requested: ExecutionTargetSelection,
        runtime_id: impl Into<String>,
        resolution_reason: ExecutionResolutionReason,
    ) -> Self {
        Self {
            requested,
            resolved_runtime_id: runtime_id.into(),
            resolution_reason,
            resolved_at: Utc::now(),
        }
    }
}

pub fn default_unknown_runtime_id() -> String {
    UNKNOWN_EXECUTION_RUNTIME_ID.to_string()
}

#[derive(Debug, Clone)]
pub struct ExecutionTargetCandidate {
    pub runtime_id: String,
    pub capabilities: WorkerCapabilities,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionTargetResolutionError {
    InvalidRuntimeId,
    ParentUnavailable { runtime_id: String },
    ExactUnavailable { runtime_id: String },
    NoCapableTarget,
    UnsupportedTarget { detail: String },
}

impl ExecutionTargetResolutionError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidRuntimeId => "execution_target_invalid",
            Self::ParentUnavailable { .. } | Self::ExactUnavailable { .. } => {
                "execution_target_unavailable"
            }
            Self::NoCapableTarget => "execution_target_no_capable_runtime",
            Self::UnsupportedTarget { .. } => "execution_target_unsupported",
        }
    }
}

impl fmt::Display for ExecutionTargetResolutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let detail = match self {
            Self::InvalidRuntimeId => "exact runtime_id is empty or invalid".to_string(),
            Self::ParentUnavailable { runtime_id } => {
                format!("parent runtime '{runtime_id}' is unavailable")
            }
            Self::ExactUnavailable { runtime_id } => {
                format!("exact runtime '{runtime_id}' is unavailable")
            }
            Self::NoCapableTarget => {
                "no available runtime satisfies the placement requirements".to_string()
            }
            Self::UnsupportedTarget { detail } => detail.clone(),
        };
        write!(formatter, "{}: {detail}", self.code())
    }
}

impl std::error::Error for ExecutionTargetResolutionError {}

/// Resolve against an already-authorized candidate inventory. Candidate order
/// is preference order for `auto`; exact and parent selection never fall back.
pub fn resolve_execution_target(
    requested: ExecutionTargetSelection,
    parent_runtime_id: &str,
    candidates: &[ExecutionTargetCandidate],
) -> Result<ExecutionPlacementResolution, ExecutionTargetResolutionError> {
    let parent_runtime_id = parent_runtime_id.trim();
    let (runtime_id, reason, requested) = match requested {
        ExecutionTargetSelection::SameAsParent => {
            let candidate = candidates
                .iter()
                .find(|candidate| candidate.runtime_id == parent_runtime_id)
                .ok_or_else(|| ExecutionTargetResolutionError::ParentUnavailable {
                    runtime_id: parent_runtime_id.to_string(),
                })?;
            (
                candidate.runtime_id.clone(),
                ExecutionResolutionReason::SameAsParent,
                ExecutionTargetSelection::SameAsParent,
            )
        }
        ExecutionTargetSelection::Exact { runtime_id } => {
            let runtime_id = runtime_id.trim();
            if runtime_id.is_empty()
                || runtime_id.len() > 256
                || !runtime_id
                    .bytes()
                    .all(|byte| matches!(byte, 0x21..=0x7e))
            {
                return Err(ExecutionTargetResolutionError::InvalidRuntimeId);
            }
            let candidate = candidates
                .iter()
                .find(|candidate| candidate.runtime_id == runtime_id)
                .ok_or_else(|| ExecutionTargetResolutionError::ExactUnavailable {
                    runtime_id: runtime_id.to_string(),
                })?;
            let runtime_id = candidate.runtime_id.clone();
            (
                runtime_id.clone(),
                ExecutionResolutionReason::ExactTarget,
                ExecutionTargetSelection::Exact { runtime_id },
            )
        }
        ExecutionTargetSelection::Auto { requirements } => {
            let placement = requirements.to_placement_constraints();
            let candidate = candidates
                .iter()
                .find(|candidate| placement.matches(&candidate.capabilities))
                .ok_or(ExecutionTargetResolutionError::NoCapableTarget)?;
            (
                candidate.runtime_id.clone(),
                ExecutionResolutionReason::AutoMatch,
                ExecutionTargetSelection::Auto { requirements },
            )
        }
    };
    Ok(ExecutionPlacementResolution::resolved(
        requested, runtime_id, reason,
    ))
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct WorkshopSpawn {
    /// Worker profile: memory.avec_calibrate | memory.context | research | general
    #[serde(default)]
    pub(crate) intent: Option<String>,
    /// Focused task for the worker
    pub(crate) task: String,
    /// Short message for the user while the worker runs
    pub(crate) user_ack: String,
    /// Optional YAML specialty
    #[serde(default)]
    pub(crate) manuscript_id: Option<String>,
    /// Optional StageRoutingMatrix role
    #[serde(default)]
    pub(crate) stage_role: Option<String>,
    /// Prefer omit or auto; only set provider:model when explicitly requested
    #[serde(default)]
    pub(crate) model_hint: Option<String>,
    /// Optional execution placement. Omitted means same as the parent turn.
    #[serde(default)]
    pub(crate) execution_target: Option<ExecutionTargetSelection>,
}

pub fn workshop_spawn_type_schema() -> TypedActionSchema {
    typed_action_schema::<WorkshopSpawn>(
        WORKSHOP_MUTATE_ID,
        "workshop.spawn",
        "Delegate heavy work to a background turn worker",
    )
}

#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub fn workshop_type_schemas() -> Vec<TypedActionSchema> {
    vec![workshop_spawn_type_schema()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(runtime_id: &str) -> ExecutionTargetCandidate {
        ExecutionTargetCandidate {
            runtime_id: runtime_id.to_string(),
            capabilities: WorkerCapabilities::any()
                .node_id(runtime_id)
                .platform("macos")
                .architecture("aarch64")
                .with_capability("assistant.work"),
        }
    }

    #[test]
    fn omitted_spawn_target_remains_same_as_parent() {
        let spawn: WorkshopSpawn = serde_json::from_value(serde_json::json!({
            "task": "research",
            "user_ack": "On it"
        }))
        .expect("spawn");
        assert!(spawn.execution_target.is_none());
    }

    #[test]
    fn exact_target_never_falls_back() {
        let candidates = vec![candidate("runtime-local")];
        let error = resolve_execution_target(
            ExecutionTargetSelection::Exact {
                runtime_id: "runtime-offline".to_string(),
            },
            "runtime-local",
            &candidates,
        )
        .expect_err("exact offline target");
        assert_eq!(error.code(), "execution_target_unavailable");
    }

    #[test]
    fn auto_uses_stasis_capability_matching() {
        let candidates = vec![candidate("runtime-local")];
        let resolution = resolve_execution_target(
            ExecutionTargetSelection::Auto {
                requirements: ExecutionTargetRequirements {
                    required_capabilities: BTreeSet::from(["assistant.work".to_string()]),
                    platform: Some("macos".to_string()),
                    architecture: Some("aarch64".to_string()),
                    region: None,
                },
            },
            "runtime-parent",
            &candidates,
        )
        .expect("auto match");
        assert_eq!(resolution.resolved_runtime_id, "runtime-local");
        assert_eq!(
            resolution.resolution_reason,
            ExecutionResolutionReason::AutoMatch
        );
    }
}
