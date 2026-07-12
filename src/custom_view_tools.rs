//! Agent tools for custom view ergonomics — doctor, compose, and registration.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use medousa_types::environment::{
    EnvironmentPatchOp, SurfaceKind, SurfaceLayout,
};
use medousa_types::environment_validate::validate_environment_spec;
use medousa_types::feed::is_valid_feed_id;
use medousa_types::layout::LayoutNode;
use serde_json::{json, Value};
use stasis::application::orchestration::tool_registry::StasisTool;
use stasis::prelude::{Result as StasisResult, RuntimeComposition, StasisError};
use tokio::sync::{mpsc, RwLock};

use crate::custom_view_status::{build_environment_status, surface_nav_visible, DoctorDiagnosticOptions};
use crate::environment_patch::execute_environment_patch;
use crate::environment_store::{environment_hub, resolve_profile_id};
use crate::environment_tools::make_presentation_component;
use crate::events::TuiEvent;
use crate::runtime_tools::CognitionRuntimeRecurringRegisterTool;
use crate::turn_continuation::TurnContinuationScope;
use crate::ui_present_tools::CognitionUiPresentTool;

pub const COGNITION_CUSTOM_VIEW_DOCTOR: &str = "cognition_custom_view_doctor";
pub const COGNITION_CUSTOM_VIEW_COMPOSE: &str = "cognition_custom_view_compose";

pub fn register_custom_view_tools(
    registry: &mut stasis::application::orchestration::tool_registry::InMemoryToolRegistry,
    runtime: Arc<RuntimeComposition>,
    event_tx: mpsc::Sender<TuiEvent>,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
) -> StasisResult<()> {
    crate::environment_patch::register_environment_patch_tool(registry)?;
    registry.register_tool(CognitionCustomViewDoctorTool::new(runtime.clone()))?;
    registry.register_tool(CognitionCustomViewComposeTool::new(
        runtime,
        event_tx,
        turn_scope,
    ))?;
    Ok(())
}

pub struct CognitionCustomViewDoctorTool {
    runtime: Arc<RuntimeComposition>,
}

impl CognitionCustomViewDoctorTool {
    pub fn new(runtime: Arc<RuntimeComposition>) -> Self {
        Self { runtime }
    }
}

#[async_trait]
impl StasisTool for CognitionCustomViewDoctorTool {
    fn name(&self) -> &'static str {
        COGNITION_CUSTOM_VIEW_DOCTOR
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Diagnose custom environment surfaces: nav, feeds, recurring bindings, widget runtime logs, store lint, and static HTML checks.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "surface_id": {
                    "type": "string",
                    "description": "Optional single custom surface id; omit to inspect all"
                },
                "component_id": {
                    "type": "string",
                    "description": "Optional presentation component id to narrow runtime diagnostics"
                },
                "profile_id": { "type": "string" },
                "session_id": {
                    "type": "string",
                    "description": "Optional chat session for artifact HTML resolution"
                },
                "include_runtime": {
                    "type": "boolean",
                    "description": "Include MedousaStore lint and runtime log tail (default true)"
                },
                "include_static_lint": {
                    "type": "boolean",
                    "description": "Lint artifact HTML for sandbox anti-patterns (default true)"
                },
                "probe": {
                    "type": "boolean",
                    "description": "Run active store self-test when Home client is open (default false)"
                }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let profile_id = resolve_profile_id(
            input
                .get("profile_id")
                .and_then(Value::as_str),
        );
        let surface_filter = input
            .get("surface_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let component_id = input
            .get("component_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let include_runtime = input
            .get("include_runtime")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let include_static_lint = input
            .get("include_static_lint")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let probe = input.get("probe").and_then(Value::as_bool).unwrap_or(false);
        let session_id = input
            .get("session_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        let diagnostics = DoctorDiagnosticOptions {
            component_id_filter: component_id,
            include_runtime,
            include_static_lint,
            probe,
            session_id,
        };

        let status = build_environment_status(
            environment_hub(),
            &profile_id,
            surface_filter,
            Some(self.runtime.as_ref()),
            Some(&diagnostics),
        )
        .await
        .map_err(|err| StasisError::PortFailure(err.to_string()))?;

        serde_json::to_value(status).map_err(|err| {
            StasisError::PortFailure(format!("cognition_custom_view_doctor: encode error: {err}"))
        })
    }
}

pub struct CognitionCustomViewComposeTool {
    runtime: Arc<RuntimeComposition>,
    event_tx: mpsc::Sender<TuiEvent>,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
}

impl CognitionCustomViewComposeTool {
    pub fn new(
        runtime: Arc<RuntimeComposition>,
        event_tx: mpsc::Sender<TuiEvent>,
        turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
    ) -> Self {
        Self {
            runtime,
            event_tx,
            turn_scope,
        }
    }
}

#[async_trait]
impl StasisTool for CognitionCustomViewComposeTool {
    fn name(&self) -> &'static str {
        COGNITION_CUSTOM_VIEW_COMPOSE
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Orchestrate a custom view: surface + HTML component + feed subscribe + layout + recurring poll in one call.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["surface_id", "component_id"],
            "properties": {
                "surface_id": { "type": "string" },
                "label": { "type": "string" },
                "icon": { "type": "string" },
                "component_id": { "type": "string" },
                "html": { "type": "string" },
                "artifact_id": { "type": "string", "description": "Revise-only path when html omitted" },
                "title": { "type": "string" },
                "feed_ids": { "type": "array", "items": { "type": "string" } },
                "layout_root": { "type": "object" },
                "recurring": {
                    "type": "object",
                    "properties": {
                        "cron_expr": { "type": "string" },
                        "timezone": { "type": "string" },
                        "source": { "type": "string" },
                        "poll_url": { "type": "string" },
                        "job_type": { "type": "string" }
                    }
                },
                "nav": {
                    "type": "object",
                    "properties": {
                        "add_to_active_preset": { "type": "boolean", "default": true }
                    }
                },
                "preset_rewrite": {
                    "type": "object",
                    "properties": {
                        "surfaces": { "type": "array", "items": { "type": "string" } }
                    }
                },
                "profile_id": { "type": "string" }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let profile_id = resolve_profile_id(
            input
                .get("profile_id")
                .and_then(Value::as_str),
        );
        let surface_id = required_str(&input, "surface_id")?;
        let component_id = required_str(&input, "component_id")?;
        let html = input
            .get("html")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let artifact_id = input
            .get("artifact_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        if html.is_none() && artifact_id.is_none() {
            return Err(StasisError::PortFailure(
                "cognition_custom_view_compose: html or artifact_id is required".to_string(),
            ));
        }

        let feed_ids = parse_feed_ids(&input);
        for feed_id in &feed_ids {
            if !is_valid_feed_id(feed_id) {
                return Err(StasisError::PortFailure(format!(
                    "cognition_custom_view_compose: invalid feed_id '{feed_id}'"
                )));
            }
        }

        let add_to_preset = input
            .get("nav")
            .and_then(|nav| nav.get("add_to_active_preset"))
            .and_then(Value::as_bool)
            .unwrap_or(true);

        let mut pending_operator_approval = false;
        let mut patch_ops: Vec<EnvironmentPatchOp> = Vec::new();

        let record = environment_hub()
            .get(&profile_id)
            .await
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        let surface_exists = record
            .spec
            .surfaces
            .iter()
            .any(|surface| surface.id == surface_id);

        if !surface_exists {
            let label = input
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or(&surface_id)
                .to_string();
            let icon = input
                .get("icon")
                .and_then(Value::as_str)
                .unwrap_or("layout-grid")
                .to_string();
            patch_ops.push(EnvironmentPatchOp::AddCustomSurface {
                id: surface_id.clone(),
                label,
                icon,
                layout: Some(SurfaceLayout::Dashboard),
                add_to_active_preset: add_to_preset,
            });
        } else if add_to_preset && !surface_nav_visible(&record.spec, &surface_id) {
            patch_ops.push(EnvironmentPatchOp::AddToActivePreset {
                surface_id: surface_id.clone(),
            });
        }

        if let Some(rewrite) = input.get("preset_rewrite")
            && let Some(surfaces) = rewrite.get("surfaces").and_then(Value::as_array) {
                let surfaces: Vec<String> = surfaces
                    .iter()
                    .filter_map(|value| value.as_str().map(str::to_string))
                    .collect();
                patch_ops.push(EnvironmentPatchOp::RewriteActivePresetSurfaces { surfaces });
            }

        if !patch_ops.is_empty() {
            let patch_result = execute_environment_patch(
                environment_hub(),
                &profile_id,
                &patch_ops,
                "agent",
            )
            .await
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
            if patch_result.pending_operator_approval {
                pending_operator_approval = true;
            }
            if !patch_result.ok {
                return Ok(json!({
                    "ok": false,
                    "live": false,
                    "pending_operator_approval": pending_operator_approval,
                    "errors": patch_result.errors,
                }));
            }
        }

        let mut feeds_subscribed: Vec<String> = Vec::new();
        let mut feeds_bound_recurring: Vec<String> = Vec::new();
        let mut next_run_at_utc: Option<String> = None;

        if let Some(html) = html {
            let mut ui_input = json!({
                "title": input.get("title").and_then(Value::as_str).unwrap_or(&surface_id),
                "html": html,
                "persist": true,
                "component_id": component_id,
                "surface_id": surface_id,
                "slot": "main",
            });
            if let Some(presentation) = input.get("presentation") {
                ui_input["presentation"] = presentation.clone();
            }
            let ui_tool = CognitionUiPresentTool::new(self.turn_scope.clone());
            let ui_result = ui_tool.invoke(ui_input).await?;
            if ui_result
                .get("ok")
                .and_then(Value::as_bool)
                .is_some_and(|ok| !ok)
            {
                return merge_compose_status(
                    ui_result,
                    pending_operator_approval,
                    &surface_id,
                    &profile_id,
                    &feeds_subscribed,
                    &feeds_bound_recurring,
                    next_run_at_utc.as_deref(),
                    self.runtime.as_ref(),
                )
                .await;
            }
        } else if let Some(artifact_id) = artifact_id {
            let label = input
                .get("title")
                .or_else(|| input.get("label"))
                .and_then(Value::as_str)
                .unwrap_or(&component_id)
                .to_string();
            let component = make_presentation_component(
                &component_id,
                &surface_id,
                &artifact_id,
                &label,
            );
            let mut env_record = environment_hub()
                .get(&profile_id)
                .await
                .map_err(|err| StasisError::PortFailure(err.to_string()))?;
            if let Some(index) = env_record
                .spec
                .components
                .iter()
                .position(|entry| entry.id == component_id)
            {
                env_record.spec.components[index] = component.clone();
            } else {
                env_record.spec.components.push(component);
            }
            let errors = validate_environment_spec(&env_record.spec);
            if !errors.is_empty() {
                return Ok(json!({
                    "ok": false,
                    "live": false,
                    "pending_operator_approval": pending_operator_approval,
                    "errors": errors,
                }));
            }
            environment_hub()
                .put(env_record.spec, "agent")
                .await
                .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        }

        if !feed_ids.is_empty() {
            feeds_subscribed = subscribe_component_feeds(&profile_id, &component_id, &feed_ids)
                .await?;
        }

        if let Some(layout_root_value) = input.get("layout_root") {
            let layout_root: LayoutNode = serde_json::from_value(layout_root_value.clone())
                .map_err(|err| {
                    StasisError::PortFailure(format!(
                        "cognition_custom_view_compose: invalid layout_root: {err}"
                    ))
                })?;
            apply_layout_root(&profile_id, &surface_id, layout_root).await?;
        }

        if let Some(recurring) = input.get("recurring")
            && !feed_ids.is_empty() {
                let mut recurring_input = recurring.clone();
                if recurring_input.get("source").is_none()
                    && let Some(poll_url) = recurring.get("poll_url").and_then(Value::as_str) {
                        recurring_input["source"] = json!(format!(
                            "http_poll url=\"{}\"",
                            poll_url.replace('"', "\\\"")
                        ));
                    }
                if recurring_input.get("feeds").is_none() {
                    recurring_input["feeds"] = json!({ "feed_ids": feed_ids });
                }
                if recurring_input.get("recurring_id").is_none() {
                    recurring_input["recurring_id"] =
                        json!(format!("{surface_id}-{}", component_id));
                }
                let register_tool = CognitionRuntimeRecurringRegisterTool::new(
                    self.runtime.clone(),
                    self.event_tx.clone(),
                    self.turn_scope.clone(),
                );
                let register_result = register_tool.invoke(recurring_input).await?;
                if let Some(ids) = register_result
                    .get("feeds_bound")
                    .and_then(Value::as_array)
                {
                    feeds_bound_recurring = ids
                        .iter()
                        .filter_map(|value| value.as_str().map(str::to_string))
                        .collect();
                } else if register_result
                    .get("feeds_bound")
                    .and_then(Value::as_bool)
                    .is_some_and(|bound| bound)
                {
                    feeds_bound_recurring = feed_ids.clone();
                }
                next_run_at_utc = register_result
                    .get("next_run_at_utc")
                    .and_then(Value::as_str)
                    .map(str::to_string);
            }

        let nav_visible = environment_hub()
            .get(&profile_id)
            .await
            .map(|record| surface_nav_visible(&record.spec, &surface_id))
            .unwrap_or(false);

        let doctor = build_environment_status(
            environment_hub(),
            &profile_id,
            Some(&surface_id),
            Some(self.runtime.as_ref()),
            None,
        )
        .await
        .map_err(|err| StasisError::PortFailure(err.to_string()))?;

        Ok(json!({
            "ok": true,
            "live": !pending_operator_approval,
            "nav_visible": nav_visible,
            "pending_operator_approval": pending_operator_approval,
            "feeds_subscribed": feeds_subscribed,
            "feeds_bound_recurring": feeds_bound_recurring,
            "next_run_at_utc": next_run_at_utc,
            "surface_id": surface_id,
            "component_id": component_id,
            "doctor": doctor,
        }))
    }
}

async fn merge_compose_status(
    mut base: Value,
    pending_operator_approval: bool,
    surface_id: &str,
    profile_id: &str,
    feeds_subscribed: &[String],
    feeds_bound_recurring: &[String],
    next_run_at_utc: Option<&str>,
    runtime: &RuntimeComposition,
) -> StasisResult<Value> {
    if let Some(obj) = base.as_object_mut() {
        obj.insert("pending_operator_approval".to_string(), json!(pending_operator_approval));
        obj.insert("live".to_string(), json!(!pending_operator_approval));
        if let Ok(record) = environment_hub().get(profile_id).await {
            obj.insert(
                "nav_visible".to_string(),
                json!(surface_nav_visible(&record.spec, surface_id)),
            );
        }
        obj.insert("feeds_subscribed".to_string(), json!(feeds_subscribed));
        obj.insert(
            "feeds_bound_recurring".to_string(),
            json!(feeds_bound_recurring),
        );
        if let Some(next) = next_run_at_utc {
            obj.insert("next_run_at_utc".to_string(), json!(next));
        }
        if let Ok(doctor) = build_environment_status(
            environment_hub(),
            profile_id,
            Some(surface_id),
            Some(runtime),
            None,
        )
        .await
        {
            obj.insert(
                "doctor".to_string(),
                serde_json::to_value(doctor).unwrap_or(Value::Null),
            );
        }
    }
    Ok(base)
}

fn required_str(input: &Value, key: &str) -> StasisResult<String> {
    input
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            StasisError::PortFailure(format!(
                "cognition_custom_view_compose: {key} is required"
            ))
        })
}

fn parse_feed_ids(input: &Value) -> Vec<String> {
    input
        .get("feed_ids")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|value| value.as_str().map(str::trim).filter(|id| !id.is_empty()))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

async fn subscribe_component_feeds(
    profile_id: &str,
    component_id: &str,
    feed_ids: &[String],
) -> StasisResult<Vec<String>> {
    let mut record = environment_hub()
        .get(profile_id)
        .await
        .map_err(|err| StasisError::PortFailure(err.to_string()))?;
    let Some(index) = record
        .spec
        .components
        .iter()
        .position(|component| component.id == component_id)
    else {
        return Err(StasisError::PortFailure(format!(
            "cognition_custom_view_compose: component not found: {component_id}"
        )));
    };
    let surface_id = record.spec.components[index].surface_id.clone();
    let surface_kind = record
        .spec
        .surfaces
        .iter()
        .find(|surface| surface.id == surface_id)
        .map(|surface| surface.kind.clone());
    if surface_kind != Some(SurfaceKind::Custom) {
        return Err(StasisError::PortFailure(format!(
            "cognition_custom_view_compose: component '{component_id}' must be on a custom surface"
        )));
    }
    record.spec.components[index].feeds = feed_ids.to_vec();
    record.spec.components[index].updated_at = Some(Utc::now());
    let errors = validate_environment_spec(&record.spec);
    if !errors.is_empty() {
        return Err(StasisError::PortFailure(errors.join("; ")));
    }
    environment_hub()
        .put(record.spec, "agent")
        .await
        .map_err(|err| StasisError::PortFailure(err.to_string()))?;
    Ok(feed_ids.to_vec())
}

async fn apply_layout_root(
    profile_id: &str,
    surface_id: &str,
    layout_root: LayoutNode,
) -> StasisResult<()> {
    let mut record = environment_hub()
        .get(profile_id)
        .await
        .map_err(|err| StasisError::PortFailure(err.to_string()))?;
    let Some(index) = record
        .spec
        .surfaces
        .iter()
        .position(|entry| entry.id == surface_id)
    else {
        return Err(StasisError::PortFailure(format!(
            "cognition_custom_view_compose: unknown surface '{surface_id}'"
        )));
    };
    if record.spec.surfaces[index].kind != SurfaceKind::Custom {
        return Err(StasisError::PortFailure(format!(
            "cognition_custom_view_compose: surface '{surface_id}' is not custom"
        )));
    }
    record.spec.surfaces[index].layout_root = Some(layout_root);
    let errors = validate_environment_spec(&record.spec);
    if !errors.is_empty() {
        return Err(StasisError::PortFailure(errors.join("; ")));
    }
    environment_hub()
        .put(record.spec, "agent")
        .await
        .map_err(|err| StasisError::PortFailure(err.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment_patch::patch_requires_proposal;
    use medousa_types::environment_default::default_environment_spec;

    #[test]
    fn patch_requires_proposal_when_preset_rewrite_in_compose_ops() {
        let ops = vec![EnvironmentPatchOp::RewriteActivePresetSurfaces {
            surfaces: vec!["chat".to_string()],
        }];
        assert!(patch_requires_proposal(&ops));
    }

    #[test]
    fn surface_nav_visible_after_add_custom_surface() {
        let mut spec = default_environment_spec("default");
        let ops = vec![EnvironmentPatchOp::AddCustomSurface {
            id: "trip-london".to_string(),
            label: "Trip".to_string(),
            icon: "train-front".to_string(),
            layout: Some(SurfaceLayout::Dashboard),
            add_to_active_preset: true,
        }];
        crate::environment_patch::apply_patch_ops(&mut spec, &ops).expect("patch");
        assert!(surface_nav_visible(&spec, "trip-london"));
    }
}
