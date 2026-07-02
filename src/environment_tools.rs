//! Agent tools for environment spec and component canvas CRUD.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use medousa_types::environment::{
    ComponentDef, ComponentType, EnvironmentPendingProposal, EnvironmentSpec, SurfaceDef,
    SurfaceKind, SurfaceLayout, UiPresentation, activate_layout_preset,
};
use medousa_types::environment_validate::validate_environment_spec;
use serde_json::{Value, json};
use stasis::application::orchestration::tool_registry::StasisTool;
use stasis::prelude::{Result as StasisResult, StasisError};
use tokio::sync::RwLock;

use crate::environment_store::{environment_hub, resolve_profile_id};
use crate::turn_continuation::TurnContinuationScope;

pub const COGNITION_ENVIRONMENT_GET: &str = "cognition_environment_get";
pub const COGNITION_ENVIRONMENT_APPLY: &str = "cognition_environment_apply";
pub const COGNITION_ENVIRONMENT_ACTIVATE_PRESET: &str = "cognition_environment_activate_preset";
pub const COGNITION_ENVIRONMENT_PROPOSE: &str = "cognition_environment_propose";
pub const COGNITION_COMPONENT_LIST: &str = "cognition_component_list";
pub const COGNITION_COMPONENT_GET: &str = "cognition_component_get";
pub const COGNITION_COMPONENT_CREATE: &str = "cognition_component_create";
pub const COGNITION_COMPONENT_UPDATE: &str = "cognition_component_update";
pub const COGNITION_COMPONENT_DELETE: &str = "cognition_component_delete";

const ENVIRONMENT_SPEC_PATCH_HINT: &str =
    "Patch surfaces/components on the full spec. Custom surfaces must be listed in the active layout preset surfaces array. Components render only on kind=custom surfaces.";

fn component_def_schema() -> Value {
    json!({
        "type": "object",
        "required": ["id", "type", "surfaceId", "slot"],
        "properties": {
            "id": { "type": "string", "description": "Unique component id (kebab-case)" },
            "type": {
                "type": "string",
                "enum": ["presentation", "chrome_action", "artifact", "medousa_view", "builtin_panel"],
                "description": "presentation = HTML artifact frame on custom surfaces"
            },
            "surfaceId": {
                "type": "string",
                "description": "Target surface id — agent components MUST use kind=custom surfaces (not home/chat builtins)"
            },
            "slot": {
                "type": "string",
                "enum": ["main", "header", "fab", "sidebar", "inline"],
                "description": "Layout zone on the surface"
            },
            "label": { "type": "string" },
            "config": {
                "type": "object",
                "description": "Type-specific config — presentation uses { artifactId: string } where artifactId is the art:… id returned by cognition_ui_present (not the component id)"
            },
            "presentation": {
                "type": "string",
                "enum": ["inline", "panel", "fullscreen"]
            },
            "feeds": { "type": "array", "items": { "type": "string" } }
        },
        "example": {
            "id": "writing-manuscript",
            "type": "presentation",
            "surfaceId": "writing-studio",
            "slot": "main",
            "label": "Manuscript",
            "config": { "artifactId": "art-writing-demo" },
            "presentation": "inline"
        }
    })
}

pub fn register_environment_tools(
    registry: &mut stasis::application::orchestration::tool_registry::InMemoryToolRegistry,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
) -> StasisResult<()> {
    crate::environment_wiki_tools::register_environment_wiki_tools(registry)?;
    registry.register_tool(CognitionEnvironmentGetTool)?;
    registry.register_tool(CognitionEnvironmentProposeTool)?;
    registry.register_tool(CognitionEnvironmentApplyTool)?;
    registry.register_tool(CognitionEnvironmentActivatePresetTool)?;
    registry.register_tool(CognitionComponentListTool)?;
    registry.register_tool(CognitionComponentGetTool)?;
    registry.register_tool(CognitionComponentCreateTool::new(turn_scope.clone()))?;
    registry.register_tool(CognitionComponentUpdateTool::new(turn_scope.clone()))?;
    registry.register_tool(CognitionComponentDeleteTool)?;
    Ok(())
}

struct CognitionEnvironmentGetTool;

#[async_trait]
impl StasisTool for CognitionEnvironmentGetTool {
    fn name(&self) -> &'static str {
        COGNITION_ENVIRONMENT_GET
    }

    fn description(&self) -> Option<&'static str> {
        Some("Read the persisted environment spec and component canvas for the active profile.")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "profile_id": { "type": "string" }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let profile_id = profile_from_input(&input);
        let record = environment_hub()
            .get(&profile_id)
            .await
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        Ok(json!({
            "ok": true,
            "revision": record.revision,
            "spec": record.spec,
        }))
    }
}

struct CognitionEnvironmentProposeTool;

#[async_trait]
impl StasisTool for CognitionEnvironmentProposeTool {
    fn name(&self) -> &'static str {
        COGNITION_ENVIRONMENT_PROPOSE
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Validate a proposed environment spec before applying. Returns errors[] on failure. \
             Add custom surfaces to spec.surfaces AND include their ids in the active preset surfaces list.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["spec"],
            "properties": {
                "spec": {
                    "type": "object",
                    "description": ENVIRONMENT_SPEC_PATCH_HINT,
                    "properties": {
                        "surfaces": { "type": "array", "items": { "type": "object" } },
                        "components": { "type": "array", "items": component_def_schema() },
                        "layoutPresets": { "type": "array" },
                        "activePresetId": { "type": "string" }
                    }
                }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let spec: EnvironmentSpec = parse_spec_input(&input)?;
        let profile_id = resolve_profile_id(Some(spec.profile_id.as_str()));
        let errors = validate_environment_spec(&spec);
        let diff_summary = format!(
            "surfaces={} components={} preset={}",
            spec.surfaces.len(),
            spec.components.len(),
            spec.active_preset_id.as_deref().unwrap_or("default")
        );
        environment_hub()
            .set_pending(
                &profile_id,
                EnvironmentPendingProposal {
                    proposed_spec: spec.clone(),
                    diff_summary: diff_summary.clone(),
                    errors: errors.clone(),
                    proposed_at: Utc::now(),
                    proposed_by: "agent".to_string(),
                },
            )
            .await;
        Ok(json!({
            "ok": errors.is_empty(),
            "errors": errors,
            "diff_summary": diff_summary,
            "proposed_spec": spec,
            "pending_operator_approval": true,
        }))
    }
}

struct CognitionEnvironmentApplyTool;

#[async_trait]
impl StasisTool for CognitionEnvironmentApplyTool {
    fn name(&self) -> &'static str {
        COGNITION_ENVIRONMENT_APPLY
    }

    fn description(&self) -> Option<&'static str> {
        Some("Apply an approved environment spec to the daemon store. Surfaces, components, and chrome sync to Home.")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["spec"],
            "properties": {
                "spec": {
                    "type": "object",
                    "description": ENVIRONMENT_SPEC_PATCH_HINT
                }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let spec: EnvironmentSpec = parse_spec_input(&input)?;
        let errors = validate_environment_spec(&spec);
        if !errors.is_empty() {
            return Ok(json!({ "ok": false, "errors": errors }));
        }
        let record = environment_hub()
            .put(spec, "agent")
            .await
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        environment_hub()
            .clear_pending(&record.spec.profile_id)
            .await;
        Ok(json!({
            "ok": true,
            "revision": record.revision,
            "spec": record.spec,
        }))
    }
}

struct CognitionEnvironmentActivatePresetTool;

#[async_trait]
impl StasisTool for CognitionEnvironmentActivatePresetTool {
    fn name(&self) -> &'static str {
        COGNITION_ENVIRONMENT_ACTIVATE_PRESET
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Switch the active layout preset (morning vs focus vs custom). Updates nav surfaces and shell chrome from the preset.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["preset_id"],
            "properties": {
                "preset_id": { "type": "string", "description": "Layout preset id from environment_get layoutPresets" },
                "profile_id": { "type": "string" }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let profile_id = profile_from_input(&input);
        let preset_id = input
            .get("preset_id")
            .and_then(Value::as_str)
            .ok_or_else(|| StasisError::PortFailure("preset_id required".to_string()))?;
        let mut record = environment_hub()
            .get(&profile_id)
            .await
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        activate_layout_preset(&mut record.spec, preset_id)
            .map_err(|err| StasisError::PortFailure(err))?;
        let updated = environment_hub()
            .put(record.spec, "agent")
            .await
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        Ok(json!({
            "ok": true,
            "revision": updated.revision,
            "active_preset_id": updated.spec.active_preset_id,
        }))
    }
}

struct CognitionComponentListTool;

#[async_trait]
impl StasisTool for CognitionComponentListTool {
    fn name(&self) -> &'static str {
        COGNITION_COMPONENT_LIST
    }

    fn description(&self) -> Option<&'static str> {
        Some("List all persisted components on the environment canvas.")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "profile_id": { "type": "string" }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let profile_id = profile_from_input(&input);
        let record = environment_hub()
            .get(&profile_id)
            .await
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        Ok(json!({
            "ok": true,
            "components": record.spec.components,
        }))
    }
}

struct CognitionComponentGetTool;

#[async_trait]
impl StasisTool for CognitionComponentGetTool {
    fn name(&self) -> &'static str {
        COGNITION_COMPONENT_GET
    }

    fn description(&self) -> Option<&'static str> {
        Some("Read one component by id from the canvas.")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["component_id"],
            "properties": {
                "component_id": { "type": "string" },
                "profile_id": { "type": "string" }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let profile_id = profile_from_input(&input);
        let component_id = input
            .get("component_id")
            .and_then(Value::as_str)
            .ok_or_else(|| StasisError::PortFailure("component_id required".to_string()))?;
        let record = environment_hub()
            .get(&profile_id)
            .await
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        let component = record
            .spec
            .components
            .iter()
            .find(|c| c.id == component_id)
            .cloned()
            .ok_or_else(|| StasisError::PortFailure(format!("component not found: {component_id}")))?;
        Ok(json!({ "ok": true, "component": component }))
    }
}

struct CognitionComponentCreateTool {
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
}

impl CognitionComponentCreateTool {
    fn new(turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>) -> Self {
        Self { turn_scope }
    }
}

#[async_trait]
impl StasisTool for CognitionComponentCreateTool {
    fn name(&self) -> &'static str {
        COGNITION_COMPONENT_CREATE
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Add a presentation or chrome_action component to a custom surface slot. \
             Use camelCase fields (surfaceId, type). Verify with cognition_component_list.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["component"],
            "properties": {
                "component": component_def_schema(),
                "profile_id": { "type": "string" }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let profile_id = profile_from_input(&input);
        let component = match parse_component_input(&input) {
            Ok(component) => component,
            Err(err) => {
                return Ok(json!({ "ok": false, "errors": [err] }));
            }
        };
        let session_id = tool_session_id(&self.turn_scope).await;
        if let Some(err) = validate_presentation_component_artifact(session_id.as_deref(), &component)
        {
            return Ok(json!({ "ok": false, "errors": [err] }));
        }
        let mut record = environment_hub()
            .get(&profile_id)
            .await
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        if record.spec.components.iter().any(|c| c.id == component.id) {
            return Ok(json!({
                "ok": false,
                "errors": [format!("component already exists: {}", component.id)]
            }));
        }
        record.spec.components.push(component.clone());
        let errors = validate_environment_spec(&record.spec);
        if !errors.is_empty() {
            record.spec.components.pop();
            return Ok(json!({ "ok": false, "errors": errors }));
        }
        let updated = environment_hub()
            .put(record.spec, "agent")
            .await
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        if let Some(session_id) = session_id.as_deref() {
            register_presentation_aliases(session_id, &component);
        }
        Ok(json!({
            "ok": true,
            "revision": updated.revision,
            "component": updated.spec.components.last(),
        }))
    }
}

struct CognitionComponentUpdateTool {
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
}

impl CognitionComponentUpdateTool {
    fn new(turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>) -> Self {
        Self { turn_scope }
    }
}

#[async_trait]
impl StasisTool for CognitionComponentUpdateTool {
    fn name(&self) -> &'static str {
        COGNITION_COMPONENT_UPDATE
    }

    fn description(&self) -> Option<&'static str> {
        Some("Patch an existing canvas component by id.")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["component_id"],
            "properties": {
                "component_id": { "type": "string" },
                "patch": {
                    "type": "object",
                    "description": "Partial update — label, surfaceId|surface_id, slot, config, presentation",
                    "properties": {
                        "label": { "type": "string" },
                        "surfaceId": { "type": "string" },
                        "surface_id": { "type": "string" },
                        "slot": { "type": "string" },
                        "config": { "type": "object" },
                        "presentation": {
                            "type": "string",
                            "enum": ["inline", "panel", "fullscreen"]
                        }
                    }
                },
                "profile_id": { "type": "string" }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let profile_id = profile_from_input(&input);
        let component_id = input
            .get("component_id")
            .and_then(Value::as_str)
            .ok_or_else(|| StasisError::PortFailure("component_id required".to_string()))?;
        let patch = input.get("patch").cloned().unwrap_or(json!({}));
        let mut record = environment_hub()
            .get(&profile_id)
            .await
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        let Some(index) = record
            .spec
            .components
            .iter()
            .position(|c| c.id == component_id)
        else {
            return Ok(json!({
                "ok": false,
                "errors": [format!("component not found: {component_id}")]
            }));
        };
        let previous = record.spec.components[index].clone();
        let mut existing = previous.clone();
        apply_component_patch(&mut existing, &patch);
        existing.updated_at = Some(Utc::now());
        let session_id = tool_session_id(&self.turn_scope).await;
        if let Some(err) =
            validate_presentation_component_artifact(session_id.as_deref(), &existing)
        {
            return Ok(json!({ "ok": false, "errors": [err] }));
        }
        record.spec.components[index] = existing.clone();
        let errors = validate_environment_spec(&record.spec);
        if !errors.is_empty() {
            record.spec.components[index] = previous;
            return Ok(json!({ "ok": false, "errors": errors }));
        }
        let updated = environment_hub()
            .put(record.spec, "agent")
            .await
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        if let Some(session_id) = session_id.as_deref() {
            register_presentation_aliases(session_id, &existing);
        }
        Ok(json!({
            "ok": true,
            "revision": updated.revision,
            "component": existing,
        }))
    }
}

struct CognitionComponentDeleteTool;

#[async_trait]
impl StasisTool for CognitionComponentDeleteTool {
    fn name(&self) -> &'static str {
        COGNITION_COMPONENT_DELETE
    }

    fn description(&self) -> Option<&'static str> {
        Some("Remove a component from the canvas.")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["component_id"],
            "properties": {
                "component_id": { "type": "string" },
                "profile_id": { "type": "string" }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let profile_id = profile_from_input(&input);
        let component_id = input
            .get("component_id")
            .and_then(Value::as_str)
            .ok_or_else(|| StasisError::PortFailure("component_id required".to_string()))?;
        let mut record = environment_hub()
            .get(&profile_id)
            .await
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        let before = record.spec.components.len();
        record
            .spec
            .components
            .retain(|c| c.id != component_id);
        if record.spec.components.len() == before {
            return Err(StasisError::PortFailure(format!(
                "component not found: {component_id}"
            )));
        }
        let updated = environment_hub()
            .put(record.spec, "agent")
            .await
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        Ok(json!({ "ok": true, "revision": updated.revision }))
    }
}

fn profile_from_input(input: &Value) -> String {
    resolve_profile_id(
        input
            .get("profile_id")
            .and_then(Value::as_str),
    )
}

fn parse_component_input(input: &Value) -> Result<ComponentDef, String> {
    let value = input
        .get("component")
        .cloned()
        .ok_or_else(|| "component required".to_string())?;
    serde_json::from_value(value).map_err(|err| format!("invalid component: {err}"))
}

async fn tool_session_id(
    turn_scope: &Arc<RwLock<Option<TurnContinuationScope>>>,
) -> Option<String> {
    turn_scope
        .read()
        .await
        .as_ref()
        .map(|scope| scope.session_id.clone())
        .filter(|id| !id.trim().is_empty())
}

fn presentation_artifact_id(config: &Value) -> Option<String> {
    config
        .get("artifactId")
        .or_else(|| config.get("artifact_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn validate_presentation_component_artifact(
    session_id: Option<&str>,
    component: &ComponentDef,
) -> Option<String> {
    if component.component_type != ComponentType::Presentation {
        return None;
    }
    let Some(artifact_ref) = presentation_artifact_id(&component.config) else {
        return Some(
            "presentation components require config.artifactId from cognition_ui_present"
                .to_string(),
        );
    };
    let Some(session_id) = session_id.filter(|id| !id.trim().is_empty()) else {
        if artifact_ref.starts_with("art:") {
            return None;
        }
        return Some(format!(
            "config.artifactId '{artifact_ref}' must be a canonical art:… id from cognition_ui_present"
        ));
    };
    if crate::artifact_store::presentation_artifact_exists(session_id, &artifact_ref) {
        return None;
    }
    Some(format!(
        "artifact not found for session '{session_id}': {artifact_ref}. \
         Call cognition_ui_present first and set config.artifactId to the returned artifact_id \
         (component id '{}' is not an artifact id).",
        component.id
    ))
}

fn register_presentation_aliases(session_id: &str, component: &ComponentDef) {
    if component.component_type != ComponentType::Presentation {
        return;
    }
    let Some(artifact_ref) = presentation_artifact_id(&component.config) else {
        return;
    };
    let resolved = crate::artifact_store::resolve_artifact_reference(session_id, &artifact_ref);
    if !resolved.starts_with("art:") {
        return;
    }
    let _ = crate::artifact_store::register_artifact_alias(session_id, &component.id, &resolved);
    if artifact_ref != resolved && artifact_ref != component.id {
        let _ =
            crate::artifact_store::register_artifact_alias(session_id, &artifact_ref, &resolved);
    }
}

fn parse_spec_input(input: &Value) -> StasisResult<EnvironmentSpec> {
    let value = input
        .get("spec")
        .cloned()
        .ok_or_else(|| StasisError::PortFailure("spec required".to_string()))?;
    serde_json::from_value(value).map_err(|err| StasisError::PortFailure(err.to_string()))
}

fn apply_component_patch(component: &mut ComponentDef, patch: &Value) {
    if let Some(label) = patch.get("label").and_then(Value::as_str) {
        component.label = Some(label.to_string());
    }
    if let Some(surface_id) = patch
        .get("surfaceId")
        .or_else(|| patch.get("surface_id"))
        .and_then(Value::as_str)
    {
        component.surface_id = surface_id.to_string();
    }
    if let Some(slot) = patch.get("slot").and_then(Value::as_str) {
        component.slot = slot.to_string();
    }
    if let Some(config) = patch.get("config") {
        component.config = config.clone();
    }
    if let Some(presentation) = patch.get("presentation").and_then(Value::as_str) {
        component.presentation = match presentation {
            "panel" => Some(UiPresentation::Panel),
            "fullscreen" => Some(UiPresentation::Fullscreen),
            _ => Some(UiPresentation::Inline),
        };
    }
}

/// Helper for agent-driven custom surface creation.
pub fn make_custom_surface(id: &str, label: &str, icon: &str) -> SurfaceDef {
    SurfaceDef {
        id: id.to_string(),
        label: label.to_string(),
        icon: icon.to_string(),
        kind: SurfaceKind::Custom,
        builtin_id: None,
        layout: SurfaceLayout::Dashboard,
        slots: vec![],
        mobile_tab: None,
    }
}

pub fn make_presentation_component(
    id: &str,
    surface_id: &str,
    artifact_id: &str,
    label: &str,
) -> ComponentDef {
    ComponentDef {
        id: id.to_string(),
        component_type: ComponentType::Presentation,
        surface_id: surface_id.to_string(),
        slot: "main".to_string(),
        label: Some(label.to_string()),
        config: json!({ "artifactId": artifact_id }),
        presentation: Some(UiPresentation::Inline),
        feeds: vec![],
        updated_at: Some(Utc::now()),
    }
}

pub fn make_chrome_action_component(
    id: &str,
    surface_id: &str,
    slot: &str,
    action: &str,
    label: &str,
) -> ComponentDef {
    ComponentDef {
        id: id.to_string(),
        component_type: ComponentType::ChromeAction,
        surface_id: surface_id.to_string(),
        slot: slot.to_string(),
        label: Some(label.to_string()),
        config: json!({ "action": action }),
        presentation: None,
        feeds: vec![],
        updated_at: Some(Utc::now()),
    }
}

#[cfg(test)]
mod demo_tests {
    use medousa_types::environment_default::writing_studio_demo_spec;
    use medousa_types::environment_validate::is_valid_environment_spec;

    use super::*;

    #[test]
    fn writing_studio_demo_spec_validates() {
        let spec = writing_studio_demo_spec("personal");
        assert!(is_valid_environment_spec(&spec));
    }

    #[test]
    fn presentation_component_rejects_missing_artifact_id() {
        let component = ComponentDef {
            id: "demo".to_string(),
            component_type: ComponentType::Presentation,
            surface_id: "writing-studio".to_string(),
            slot: "main".to_string(),
            label: Some("Demo".to_string()),
            config: json!({}),
            presentation: Some(UiPresentation::Inline),
            feeds: vec![],
            updated_at: None,
        };
        let err = validate_presentation_component_artifact(Some("sess-1"), &component)
            .expect("missing artifactId");
        assert!(err.contains("artifactId"));
    }
}
