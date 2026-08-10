//! Agent tools for stack-based environment layout on custom surface main bodies.

use medousa_types::environment::SurfaceKind;
use medousa_types::environment_validate::validate_environment_spec;
use medousa_types::layout::{LayoutNode, resolve_layout_root};
use schemars::JsonSchema;
use schemars::schema::{InstanceType, Schema, SchemaObject, SingleOrVec};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use stasis::prelude::{Result as StasisResult, StasisError};

use crate::environment_store::{environment_hub, resolve_profile_id};
use crate::semantic_values::TrimmedText;
use crate::typed_tools::{CompatOption, ToolId, medousa_tool};

pub const COGNITION_LAYOUT_GET: &str = "cognition_layout_get";
pub const COGNITION_LAYOUT_APPLY: &str = "cognition_layout_apply";
pub const COGNITION_LAYOUT_RESET: &str = "cognition_layout_reset";

const COGNITION_LAYOUT_GET_ID: ToolId = ToolId::new(COGNITION_LAYOUT_GET);
const COGNITION_LAYOUT_APPLY_ID: ToolId = ToolId::new(COGNITION_LAYOUT_APPLY);
const COGNITION_LAYOUT_RESET_ID: ToolId = ToolId::new(COGNITION_LAYOUT_RESET);

pub fn register_layout_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
) -> StasisResult<()> {
    registry.register_typed_tool(CognitionLayoutGetTool)?;
    registry.register_typed_tool(CognitionLayoutApplyTool)?;
    registry.register_typed_tool(CognitionLayoutResetTool)?;
    Ok(())
}

struct CognitionLayoutGetTool;

#[derive(Debug, Deserialize, JsonSchema)]
struct LayoutSurfaceInput {
    surface_id: String,
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    profile_id: CompatOption<String>,
}

#[derive(Debug)]
struct LayoutSurfaceCommand {
    surface_id: TrimmedText,
    profile_id: Option<TrimmedText>,
}

impl TryFrom<LayoutSurfaceInput> for LayoutSurfaceCommand {
    type Error = StasisError;

    fn try_from(input: LayoutSurfaceInput) -> Result<Self, Self::Error> {
        Ok(Self {
            surface_id: required_layout_identifier(input.surface_id, "surface_id")?,
            profile_id: input
                .profile_id
                .into_option()
                .and_then(|value| TrimmedText::new(value).ok()),
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
enum LayoutGetOutput {
    Success {
        ok: bool,
        surface_id: String,
        #[schemars(with = "serde_json::Value")]
        layout_root: Option<LayoutNode>,
        #[schemars(with = "serde_json::Value")]
        resolved_layout_root: LayoutNode,
        implicit_fallback: bool,
        main_component_ids: Vec<String>,
    },
    Failure {
        ok: bool,
        errors: Vec<String>,
    },
}

#[medousa_tool(id = COGNITION_LAYOUT_GET_ID)]
impl CognitionLayoutGetTool {
    /// Read the stack layout tree for a custom surface main body, including implicit fallback when layoutRoot is unset.
    async fn invoke_typed(
        &self,
        input: LayoutSurfaceInput,
    ) -> stasis::prelude::Result<LayoutGetOutput> {
        let command = LayoutSurfaceCommand::try_from(input)?;
        let profile_id = profile_from_typed(command.profile_id.as_ref().map(TrimmedText::as_str));
        let surface_id = command.surface_id.into_string();
        let record = environment_hub()
            .get(&profile_id)
            .await
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        let Some(surface) = record
            .spec
            .surfaces
            .iter()
            .find(|entry| entry.id == surface_id)
        else {
            return Ok(LayoutGetOutput::Failure {
                ok: false,
                errors: vec![format!("unknown surface '{surface_id}'")],
            });
        };
        if surface.kind != SurfaceKind::Custom {
            return Ok(LayoutGetOutput::Failure {
                ok: false,
                errors: vec![format!(
                    "surface '{surface_id}' is not custom — layout applies to custom surfaces only"
                )],
            });
        }
        let main_component_ids = record
            .spec
            .components
            .iter()
            .filter(|component| component.surface_id == surface_id && component.slot == "main")
            .map(|component| component.id.clone())
            .collect::<Vec<_>>();
        let resolved = resolve_layout_root(surface, &record.spec.components);
        Ok(LayoutGetOutput::Success {
            ok: true,
            surface_id,
            layout_root: surface.layout_root.clone(),
            resolved_layout_root: resolved,
            implicit_fallback: surface.layout_root.is_none(),
            main_component_ids,
        })
    }
}

struct CognitionLayoutApplyTool;

#[derive(Debug, Deserialize)]
#[serde(transparent)]
struct LayoutRootInput(LayoutNode);

impl JsonSchema for LayoutRootInput {
    fn schema_name() -> String {
        "LayoutRootInput".to_string()
    }

    fn is_referenceable() -> bool {
        false
    }

    fn json_schema(_: &mut schemars::r#gen::SchemaGenerator) -> Schema {
        Schema::Object(SchemaObject {
            instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Object))),
            ..SchemaObject::default()
        })
    }
}

fn layout_apply_example() -> Value {
    serde_json::json!({
        "surface_id": "adhd-guide",
        "layout_root": {
            "type": "hstack",
            "spacing": "md",
            "distribution": "fill_equally",
            "children": [
                { "type": "component", "id": "adhd-guide-tetris", "flex": 1 },
                { "type": "component", "id": "adhd-guide-original", "flex": 1 }
            ]
        }
    })
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(example = "layout_apply_example")]
struct LayoutApplyInput {
    surface_id: String,
    /// LayoutNode tree — type vstack|hstack|v_stack|h_stack|grid|component; distribution fill_equally|fillEqually
    layout_root: LayoutRootInput,
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    profile_id: CompatOption<String>,
}

#[derive(Debug)]
struct LayoutApplyCommand {
    surface_id: TrimmedText,
    layout_root: LayoutNode,
    profile_id: Option<TrimmedText>,
}

impl TryFrom<LayoutApplyInput> for LayoutApplyCommand {
    type Error = StasisError;

    fn try_from(input: LayoutApplyInput) -> Result<Self, Self::Error> {
        Ok(Self {
            surface_id: required_layout_identifier(input.surface_id, "surface_id")?,
            layout_root: input.layout_root.0,
            profile_id: input
                .profile_id
                .into_option()
                .and_then(|value| TrimmedText::new(value).ok()),
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
enum LayoutApplyOutput {
    Success {
        ok: bool,
        revision: u64,
        surface_id: String,
        #[schemars(with = "serde_json::Value")]
        layout_root: Option<LayoutNode>,
    },
    Failure {
        ok: bool,
        errors: Vec<String>,
    },
}

#[medousa_tool(id = COGNITION_LAYOUT_APPLY_ID)]
impl CognitionLayoutApplyTool {
    /// Apply a stack layout tree (vstack/hstack/grid/component) to a custom surface main body. Changes go live immediately.
    async fn invoke_typed(
        &self,
        input: LayoutApplyInput,
    ) -> stasis::prelude::Result<LayoutApplyOutput> {
        let command = LayoutApplyCommand::try_from(input)?;
        let profile_id = profile_from_typed(command.profile_id.as_ref().map(TrimmedText::as_str));
        let surface_id = command.surface_id.into_string();
        let layout_root = command.layout_root;
        let mut record = environment_hub()
            .get(&profile_id)
            .await
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        let Some(index) = record
            .spec
            .surfaces
            .iter()
            .position(|entry| entry.id == surface_id)
        else {
            return Ok(LayoutApplyOutput::Failure {
                ok: false,
                errors: vec![format!("unknown surface '{surface_id}'")],
            });
        };
        if record.spec.surfaces[index].kind != SurfaceKind::Custom {
            return Ok(LayoutApplyOutput::Failure {
                ok: false,
                errors: vec![format!("surface '{surface_id}' is not custom")],
            });
        }
        let previous = record.spec.surfaces[index].layout_root.clone();
        record.spec.surfaces[index].layout_root = Some(layout_root);
        let errors = validate_environment_spec(&record.spec);
        if !errors.is_empty() {
            record.spec.surfaces[index].layout_root = previous;
            return Ok(LayoutApplyOutput::Failure { ok: false, errors });
        }
        let updated = environment_hub()
            .put(record.spec, "agent")
            .await
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        Ok(LayoutApplyOutput::Success {
            ok: true,
            revision: updated.revision,
            layout_root: updated
                .spec
                .surfaces
                .iter()
                .find(|surface| surface.id == surface_id)
                .and_then(|surface| surface.layout_root.clone()),
            surface_id,
        })
    }
}

struct CognitionLayoutResetTool;

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
enum LayoutResetOutput {
    Success {
        ok: bool,
        revision: u64,
        surface_id: String,
        implicit_fallback: bool,
    },
    Failure {
        ok: bool,
        errors: Vec<String>,
    },
}

#[medousa_tool(id = COGNITION_LAYOUT_RESET_ID)]
impl CognitionLayoutResetTool {
    /// Clear layoutRoot on a custom surface so main components fall back to implicit vertical stack order.
    async fn invoke_typed(
        &self,
        input: LayoutSurfaceInput,
    ) -> stasis::prelude::Result<LayoutResetOutput> {
        let command = LayoutSurfaceCommand::try_from(input)?;
        let profile_id = profile_from_typed(command.profile_id.as_ref().map(TrimmedText::as_str));
        let surface_id = command.surface_id.into_string();
        let mut record = environment_hub()
            .get(&profile_id)
            .await
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        let Some(index) = record
            .spec
            .surfaces
            .iter()
            .position(|entry| entry.id == surface_id)
        else {
            return Ok(LayoutResetOutput::Failure {
                ok: false,
                errors: vec![format!("unknown surface '{surface_id}'")],
            });
        };
        record.spec.surfaces[index].layout_root = None;
        let updated = environment_hub()
            .put(record.spec, "agent")
            .await
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        Ok(LayoutResetOutput::Success {
            ok: true,
            revision: updated.revision,
            surface_id,
            implicit_fallback: true,
        })
    }
}

fn profile_from_typed(profile_id: Option<&str>) -> String {
    resolve_profile_id(profile_id.map(str::trim).filter(|value| !value.is_empty()))
}

fn required_layout_identifier(value: String, key: &str) -> StasisResult<TrimmedText> {
    TrimmedText::new(value).map_err(|_| StasisError::PortFailure(format!("{key} is required")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_commands_normalize_surface_and_profile_identifiers() {
        let surface = LayoutSurfaceCommand::try_from(LayoutSurfaceInput {
            surface_id: " custom-surface ".to_string(),
            profile_id: Some(" profile-a ".to_string()).into(),
        })
        .expect("surface command");
        assert_eq!(surface.surface_id.as_str(), "custom-surface");
        assert_eq!(
            surface.profile_id.as_ref().map(TrimmedText::as_str),
            Some("profile-a")
        );

        let apply = LayoutApplyCommand::try_from(LayoutApplyInput {
            surface_id: " custom-surface ".to_string(),
            layout_root: LayoutRootInput(LayoutNode::Component {
                id: "component-a".to_string(),
                flex: Some(2),
            }),
            profile_id: None.into(),
        })
        .expect("apply command");
        assert_eq!(apply.surface_id.as_str(), "custom-surface");
        assert!(matches!(apply.layout_root, LayoutNode::Component { .. }));
    }

    #[test]
    fn layout_surface_command_rejects_blank_surface_id() {
        let error = LayoutSurfaceCommand::try_from(LayoutSurfaceInput {
            surface_id: " \n\t".to_string(),
            profile_id: None.into(),
        })
        .expect_err("blank surface id should fail");
        assert!(error.to_string().contains("surface_id is required"));
    }

    #[test]
    fn layout_wire_profiles_remain_lenient_for_legacy_values() {
        let surface: LayoutSurfaceInput = serde_json::from_value(serde_json::json!({
            "surface_id": "custom-surface",
            "profile_id": 42,
        }))
        .expect("surface input");
        assert!(surface.profile_id.into_option().is_none());

        let apply: LayoutApplyInput = serde_json::from_value(serde_json::json!({
            "surface_id": "custom-surface",
            "layout_root": {"type": "component", "id": "component-a"},
            "profile_id": false,
        }))
        .expect("apply input");
        assert!(apply.profile_id.into_option().is_none());
    }
}
