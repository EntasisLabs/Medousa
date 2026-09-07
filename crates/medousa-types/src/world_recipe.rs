//! Inert, secret-free recipes derived from confirmed governed-world traces.

use serde::{Deserialize, Serialize};

pub const WORLD_RECIPE_SCHEMA_VERSION: u16 = 1;
pub const WORLD_RECIPE_RUN_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum WorldRecipeInputKind {
    Text,
    Selection,
    Key,
    ScrollDelta,
    WaitDuration,
}

/// A semantic operation template. Values, selectors, coordinates, handles,
/// permits, and grants are deliberately absent and must be resolved afresh.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct WorldRecipeOperation {
    pub verb: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_kind: Option<WorldRecipeInputKind>,
    pub requires_operator_confirmation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct WorldRecipeStep {
    pub ordinal: u32,
    pub source_admission_sequence: u64,
    pub source_completion_sequence: u64,
    pub source_intent_id: String,
    pub surface: String,
    pub effect_class: String,
    pub operations: Vec<WorldRecipeOperation>,
    pub requires_fresh_observation: bool,
    pub requires_fresh_admission: bool,
    pub requires_operator_confirmation: bool,
}

/// A recipe is reviewable guidance, never a reusable action permit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct WorldRecipe {
    pub schema_version: u16,
    pub recipe_id: String,
    pub source_trace_id: String,
    pub source_start_sequence: u64,
    pub source_end_sequence: u64,
    pub steps: Vec<WorldRecipeStep>,
    pub execution_model: String,
    pub carries_authority: bool,
    pub automatic_dispatch_allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct WorldRecipeDeriveResponse {
    pub recipe: WorldRecipe,
}

/// Bind one source recipe step to an exact live world. World ids remain opaque;
/// the destination daemon resolves them against its own registered drivers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct WorldRecipeRunTarget {
    pub step_ordinal: u32,
    pub world_id: String,
}

/// Fresh operator-provided material for one semantic operation. These values
/// are never copied into a recipe, receipt, timeline summary, or run response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorldRecipeRunInputValue {
    Text { text: String },
    Selection { value: String },
    Key { key: String },
    ScrollDelta { delta_y: i64 },
    WaitDuration { milliseconds: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct WorldRecipeRunInput {
    pub step_ordinal: u32,
    /// Zero-based operation index within the recipe step.
    pub operation_index: u32,
    pub input: WorldRecipeRunInputValue,
}

/// Explicit confirmation for one operation whose source or freshly resolved
/// target requires operator review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct WorldRecipeRunConfirmation {
    pub step_ordinal: u32,
    /// Zero-based operation index within the recipe step.
    pub operation_index: u32,
}

/// Execute a server-derived recipe as a new governed run. The daemon
/// re-derives `recipe_id` from `source_trace_id`; callers cannot submit an
/// arbitrary action template. `run_id` is a durable idempotency boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct WorldRecipeRunRequest {
    pub recipe_id: String,
    pub source_trace_id: String,
    pub run_id: String,
    pub operator_approved: bool,
    pub targets: Vec<WorldRecipeRunTarget>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<WorldRecipeRunInput>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub confirmations: Vec<WorldRecipeRunConfirmation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum WorldRecipeRunStatus {
    Completed,
    Stopped,
}

/// One successfully acknowledged replay step. This intentionally contains no
/// fresh input values or driver-native target handles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct WorldRecipeRunStepResult {
    pub step_ordinal: u32,
    pub world_id: String,
    pub surface: String,
    pub operation_count: u32,
    pub intent_id: String,
    pub committed_revision: u64,
    pub completion_sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct WorldRecipeRunStop {
    pub step_ordinal: u32,
    pub code: String,
    pub reason: String,
    pub effect_may_have_applied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct WorldRecipeRunResponse {
    pub schema_version: u16,
    pub recipe_id: String,
    pub run_id: String,
    pub trace_id: String,
    pub status: WorldRecipeRunStatus,
    pub completed_steps: u32,
    pub steps: Vec<WorldRecipeRunStepResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop: Option<WorldRecipeRunStop>,
}
