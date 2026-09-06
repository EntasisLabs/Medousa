//! Inert, secret-free recipes derived from confirmed governed-world traces.

use serde::{Deserialize, Serialize};

pub const WORLD_RECIPE_SCHEMA_VERSION: u16 = 1;

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
