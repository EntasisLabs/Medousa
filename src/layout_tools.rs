//! Agent tools for stack-based environment layout on custom surface main bodies.

use medousa_types::environment::SurfaceKind;
use medousa_types::environment_validate::validate_environment_spec;
use medousa_types::layout::{resolve_layout_root, LayoutNode};
use serde_json::{json, Value};
use stasis::application::orchestration::tool_registry::StasisTool;
use stasis::prelude::{Result as StasisResult, StasisError};

use crate::environment_store::{environment_hub, resolve_profile_id};

pub const COGNITION_LAYOUT_GET: &str = "cognition_layout_get";
pub const COGNITION_LAYOUT_APPLY: &str = "cognition_layout_apply";
pub const COGNITION_LAYOUT_RESET: &str = "cognition_layout_reset";

pub fn register_layout_tools(
    registry: &mut stasis::application::orchestration::tool_registry::InMemoryToolRegistry,
) -> StasisResult<()> {
    registry.register_tool(CognitionLayoutGetTool)?;
    registry.register_tool(CognitionLayoutApplyTool)?;
    registry.register_tool(CognitionLayoutResetTool)?;
    Ok(())
}

struct CognitionLayoutGetTool;

#[async_trait::async_trait]
impl StasisTool for CognitionLayoutGetTool {
    fn name(&self) -> &'static str {
        COGNITION_LAYOUT_GET
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Read the stack layout tree for a custom surface main body, including implicit fallback when layoutRoot is unset.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["surface_id"],
            "properties": {
                "surface_id": { "type": "string" },
                "profile_id": { "type": "string" }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let profile_id = profile_from_input(&input);
        let surface_id = required_string(&input, "surface_id")?;
        let record = environment_hub()
            .get(&profile_id)
            .await
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        let Some(surface) = record.spec.surfaces.iter().find(|entry| entry.id == surface_id) else {
            return Ok(json!({
                "ok": false,
                "errors": [format!("unknown surface '{surface_id}'")]
            }));
        };
        if surface.kind != SurfaceKind::Custom {
            return Ok(json!({
                "ok": false,
                "errors": [format!("surface '{surface_id}' is not custom — layout applies to custom surfaces only")]
            }));
        }
        let main_component_ids = record
            .spec
            .components
            .iter()
            .filter(|component| component.surface_id == surface_id && component.slot == "main")
            .map(|component| component.id.clone())
            .collect::<Vec<_>>();
        let resolved = resolve_layout_root(surface, &record.spec.components);
        Ok(json!({
            "ok": true,
            "surface_id": surface_id,
            "layout_root": surface.layout_root,
            "resolved_layout_root": resolved,
            "implicit_fallback": surface.layout_root.is_none(),
            "main_component_ids": main_component_ids,
        }))
    }
}

struct CognitionLayoutApplyTool;

#[async_trait::async_trait]
impl StasisTool for CognitionLayoutApplyTool {
    fn name(&self) -> &'static str {
        COGNITION_LAYOUT_APPLY
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Apply a stack layout tree (vstack/hstack/grid/component) to a custom surface main body. Changes go live immediately.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["surface_id", "layout_root"],
            "properties": {
                "surface_id": { "type": "string" },
                "layout_root": {
                    "type": "object",
                    "description": "LayoutNode tree — type vstack|hstack|v_stack|h_stack|grid|component; distribution fill_equally|fillEqually"
                },
                "profile_id": { "type": "string" }
            },
            "example": {
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
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let profile_id = profile_from_input(&input);
        let surface_id = required_string(&input, "surface_id")?;
        let layout_root: LayoutNode = input
            .get("layout_root")
            .cloned()
            .ok_or_else(|| StasisError::PortFailure("layout_root is required".to_string()))
            .and_then(|value| {
                serde_json::from_value(value).map_err(|err| {
                    StasisError::PortFailure(format!("invalid layout_root: {err}"))
                })
            })?;
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
            return Ok(json!({
                "ok": false,
                "errors": [format!("unknown surface '{surface_id}'")]
            }));
        };
        if record.spec.surfaces[index].kind != SurfaceKind::Custom {
            return Ok(json!({
                "ok": false,
                "errors": [format!("surface '{surface_id}' is not custom")]
            }));
        }
        let previous = record.spec.surfaces[index].layout_root.clone();
        record.spec.surfaces[index].layout_root = Some(layout_root);
        let errors = validate_environment_spec(&record.spec);
        if !errors.is_empty() {
            record.spec.surfaces[index].layout_root = previous;
            return Ok(json!({ "ok": false, "errors": errors }));
        }
        let updated = environment_hub()
            .put(record.spec, "agent")
            .await
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        Ok(json!({
            "ok": true,
            "revision": updated.revision,
            "surface_id": surface_id,
            "layout_root": updated.spec.surfaces.iter().find(|s| s.id == surface_id).and_then(|s| s.layout_root.clone()),
        }))
    }
}

struct CognitionLayoutResetTool;

#[async_trait::async_trait]
impl StasisTool for CognitionLayoutResetTool {
    fn name(&self) -> &'static str {
        COGNITION_LAYOUT_RESET
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Clear layoutRoot on a custom surface so main components fall back to implicit vertical stack order.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["surface_id"],
            "properties": {
                "surface_id": { "type": "string" },
                "profile_id": { "type": "string" }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let profile_id = profile_from_input(&input);
        let surface_id = required_string(&input, "surface_id")?;
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
            return Ok(json!({
                "ok": false,
                "errors": [format!("unknown surface '{surface_id}'")]
            }));
        };
        record.spec.surfaces[index].layout_root = None;
        let updated = environment_hub()
            .put(record.spec, "agent")
            .await
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        Ok(json!({
            "ok": true,
            "revision": updated.revision,
            "surface_id": surface_id,
            "implicit_fallback": true,
        }))
    }
}

fn profile_from_input(input: &Value) -> String {
    resolve_profile_id(
        input
            .get("profile_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty()),
    )
}

fn required_string(input: &Value, key: &str) -> StasisResult<String> {
    input
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| StasisError::PortFailure(format!("{key} is required")))
}
