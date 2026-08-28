//! Location-neutral workshop worker request contract.

use schemars::JsonSchema;
use serde::Deserialize;

use crate::public_api::COGNITION_WORKSHOP_MUTATE;
use crate::schema_api::{TypedActionSchema, typed_action_schema};
use crate::typed_tools::ToolId;

const WORKSHOP_MUTATE_ID: ToolId = ToolId::new(COGNITION_WORKSHOP_MUTATE);

#[derive(Debug, Deserialize, JsonSchema)]
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
