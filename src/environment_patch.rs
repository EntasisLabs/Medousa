//! Incremental environment spec patches with hybrid approval gates.

use chrono::Utc;
use medousa_types::environment::{
    EnvironmentPatchOp, EnvironmentPatchResponse, EnvironmentPendingProposal, EnvironmentSpec,
    EnvironmentTheme, SurfaceKind, SurfaceLayout,
};
use medousa_types::environment_validate::validate_environment_spec;
use medousa_types::environment_icons::is_valid_surface_icon;
use medousa_types::environment_themes::{is_valid_brand_color, is_valid_color_theme_id};
use medousa_types::feed::is_valid_feed_id;
use serde_json::json;

use async_trait::async_trait;
use medousa_types::environment::EnvironmentPatchRequest;
use serde_json::Value;
use stasis::application::orchestration::tool_registry::StasisTool;
use stasis::prelude::{Result as StasisResult, StasisError};

use crate::custom_view_status::{active_preset_surface_ids, surface_nav_visible};
use crate::environment_store::{environment_hub, resolve_profile_id, EnvironmentHub};
use crate::environment_tools::make_custom_surface;

pub const COGNITION_ENVIRONMENT_PATCH: &str = "cognition_environment_patch";

pub fn patch_requires_proposal(ops: &[EnvironmentPatchOp]) -> bool {
    ops.iter()
        .any(|op| matches!(op, EnvironmentPatchOp::RewriteActivePresetSurfaces { .. }))
}

pub fn apply_patch_ops(spec: &mut EnvironmentSpec, ops: &[EnvironmentPatchOp]) -> Result<Vec<String>, String> {
    let mut applied = Vec::new();
    for op in ops {
        match op {
            EnvironmentPatchOp::AddCustomSurface {
                id,
                label,
                icon,
                layout,
                add_to_active_preset,
            } => {
                let icon = icon.trim();
                if !is_valid_surface_icon(icon) {
                    return Err(format!("add_custom_surface: invalid icon '{icon}'"));
                }
                let id = id.trim();
                if id.is_empty() {
                    return Err("add_custom_surface: id is required".to_string());
                }
                if !spec.surfaces.iter().any(|surface| surface.id == id) {
                    let mut surface = make_custom_surface(id, label, icon);
                    if let Some(layout) = layout {
                        surface.layout = layout.clone();
                    }
                    spec.surfaces.push(surface);
                    applied.push(format!("add_custom_surface:{id}"));
                }
                if *add_to_active_preset {
                    add_surface_to_active_preset(spec, id)?;
                    applied.push(format!("add_to_active_preset:{id}"));
                }
            }
            EnvironmentPatchOp::AddToActivePreset { surface_id } => {
                let surface_id = surface_id.trim();
                add_surface_to_active_preset(spec, surface_id)?;
                applied.push(format!("add_to_active_preset:{surface_id}"));
            }
            EnvironmentPatchOp::AddComponent { component } => {
                if spec.components.iter().any(|entry| entry.id == component.id) {
                    return Err(format!("add_component: component '{}' already exists", component.id));
                }
                spec.components.push(component.clone());
                applied.push(format!("add_component:{}", component.id));
            }
            EnvironmentPatchOp::SetComponentFeeds {
                component_id,
                feed_ids,
            } => {
                for feed_id in feed_ids {
                    if !is_valid_feed_id(feed_id) {
                        return Err(format!("set_component_feeds: invalid feed_id '{feed_id}'"));
                    }
                }
                let Some(index) = spec
                    .components
                    .iter()
                    .position(|component| component.id == *component_id)
                else {
                    return Err(format!("set_component_feeds: unknown component '{component_id}'"));
                };
                spec.components[index].feeds = feed_ids.clone();
                spec.components[index].updated_at = Some(Utc::now());
                applied.push(format!("set_component_feeds:{component_id}"));
            }
            EnvironmentPatchOp::RewriteActivePresetSurfaces { surfaces } => {
                let presets = spec
                    .layout_presets
                    .as_mut()
                    .ok_or_else(|| "rewrite_active_preset_surfaces: spec has no layout presets".to_string())?;
                let active_index = presets
                    .iter()
                    .position(|preset| preset.active)
                    .ok_or_else(|| {
                        "rewrite_active_preset_surfaces: no active layout preset".to_string()
                    })?;
                presets[active_index].surfaces = surfaces.clone();
                applied.push("rewrite_active_preset_surfaces".to_string());
            }
            EnvironmentPatchOp::UpdateSurface { id, label, icon } => {
                let id = id.trim();
                let Some(surface) = spec.surfaces.iter_mut().find(|surface| surface.id == id) else {
                    return Err(format!("update_surface: unknown surface '{id}'"));
                };
                if let Some(label) = label {
                    let label = label.trim();
                    if label.is_empty() {
                        return Err("update_surface: label cannot be empty".to_string());
                    }
                    surface.label = label.to_string();
                }
                if let Some(icon) = icon {
                    let icon = icon.trim();
                    if !is_valid_surface_icon(icon) {
                        return Err(format!("update_surface: invalid icon '{icon}'"));
                    }
                    surface.icon = icon.to_string();
                }
                applied.push(format!("update_surface:{id}"));
            }
            EnvironmentPatchOp::SetEnvironmentTheme {
                color_theme_id,
                brand_color,
                tagline,
            } => {
                let theme = spec.theme.get_or_insert(EnvironmentTheme {
                    color_theme_id: None,
                    brand_color: None,
                    tagline: None,
                });
                if let Some(id) = color_theme_id {
                    let id = id.trim();
                    if id.is_empty() {
                        theme.color_theme_id = None;
                    } else if !is_valid_color_theme_id(id) {
                        return Err(format!("set_environment_theme: invalid colorThemeId '{id}'"));
                    } else {
                        theme.color_theme_id = Some(id.to_string());
                    }
                }
                if let Some(brand) = brand_color {
                    let brand = brand.trim();
                    if brand.is_empty() {
                        theme.brand_color = None;
                    } else if !is_valid_brand_color(brand) {
                        return Err(
                            "set_environment_theme: brandColor must be #RGB or #RRGGBB".to_string(),
                        );
                    } else {
                        let normalized = if brand.starts_with('#') {
                            brand.to_string()
                        } else {
                            format!("#{brand}")
                        };
                        theme.brand_color = Some(normalized);
                    }
                }
                if let Some(tagline) = tagline {
                    let tagline = tagline.trim();
                    theme.tagline = if tagline.is_empty() {
                        None
                    } else {
                        Some(tagline.to_string())
                    };
                }
                applied.push("set_environment_theme".to_string());
            }
            EnvironmentPatchOp::RemoveCustomSurface { id } => {
                let id = id.trim();
                let Some(surface) = spec.surfaces.iter().find(|surface| surface.id == id) else {
                    return Err(format!("remove_custom_surface: unknown surface '{id}'"));
                };
                if surface.kind != SurfaceKind::Custom {
                    return Err(format!("remove_custom_surface: '{id}' is not a custom surface"));
                }
                spec.surfaces.retain(|surface| surface.id != id);
                if let Some(presets) = spec.layout_presets.as_mut() {
                    for preset in presets.iter_mut() {
                        preset.surfaces.retain(|surface_id| surface_id != id);
                    }
                }
                spec.components.retain(|component| component.surface_id != id);
                applied.push(format!("remove_custom_surface:{id}"));
            }
            EnvironmentPatchOp::RemoveComponent { component_id } => {
                let component_id = component_id.trim();
                if !spec
                    .components
                    .iter()
                    .any(|component| component.id == component_id)
                {
                    return Err(format!("remove_component: unknown component '{component_id}'"));
                }
                spec.components.retain(|component| component.id != component_id);
                applied.push(format!("remove_component:{component_id}"));
            }
        }
    }
    Ok(applied)
}

fn add_surface_to_active_preset(spec: &mut EnvironmentSpec, surface_id: &str) -> Result<(), String> {
    if !spec.surfaces.iter().any(|surface| surface.id == surface_id) {
        return Err(format!(
            "add_to_active_preset: unknown surface '{surface_id}'"
        ));
    }
    let presets = spec
        .layout_presets
        .as_mut()
        .ok_or_else(|| "add_to_active_preset: spec has no layout presets".to_string())?;
    let active_index = presets
        .iter()
        .position(|preset| preset.active)
        .ok_or_else(|| "add_to_active_preset: no active layout preset".to_string())?;
    if !presets[active_index].surfaces.iter().any(|id| id == surface_id) {
        presets[active_index].surfaces.push(surface_id.to_string());
    }
    Ok(())
}

pub async fn execute_environment_patch(
    hub: &EnvironmentHub,
    profile_id: &str,
    ops: &[EnvironmentPatchOp],
    proposed_by: &str,
) -> anyhow::Result<EnvironmentPatchResponse> {
    if ops.is_empty() {
        return Ok(EnvironmentPatchResponse {
            ok: false,
            live: false,
            pending_operator_approval: false,
            revision: None,
            errors: vec!["ops must not be empty".to_string()],
            applied_ops: vec![],
        });
    }

    let mut record = hub.get(profile_id).await?;
    let requires_proposal = patch_requires_proposal(ops);

    if let Err(err) = apply_patch_ops(&mut record.spec, ops) {
        return Ok(EnvironmentPatchResponse {
            ok: false,
            live: false,
            pending_operator_approval: false,
            revision: None,
            errors: vec![err],
            applied_ops: vec![],
        });
    }

    let errors = validate_environment_spec(&record.spec);
    if !errors.is_empty() {
        return Ok(EnvironmentPatchResponse {
            ok: false,
            live: false,
            pending_operator_approval: false,
            revision: None,
            errors,
            applied_ops: vec![],
        });
    }

    let applied_ops: Vec<String> = ops
        .iter()
        .map(|op| match op {
            EnvironmentPatchOp::AddCustomSurface { id, .. } => format!("add_custom_surface:{id}"),
            EnvironmentPatchOp::AddToActivePreset { surface_id } => {
                format!("add_to_active_preset:{surface_id}")
            }
            EnvironmentPatchOp::AddComponent { component } => format!("add_component:{}", component.id),
            EnvironmentPatchOp::SetComponentFeeds { component_id, .. } => {
                format!("set_component_feeds:{component_id}")
            }
            EnvironmentPatchOp::RewriteActivePresetSurfaces { .. } => {
                "rewrite_active_preset_surfaces".to_string()
            }
            EnvironmentPatchOp::UpdateSurface { id, .. } => format!("update_surface:{id}"),
            EnvironmentPatchOp::SetEnvironmentTheme { .. } => "set_environment_theme".to_string(),
            EnvironmentPatchOp::RemoveCustomSurface { id } => format!("remove_custom_surface:{id}"),
            EnvironmentPatchOp::RemoveComponent { component_id } => {
                format!("remove_component:{component_id}")
            }
        })
        .collect();

    if requires_proposal {
        let diff_summary = format!(
            "Patch proposes preset rewrite ({} surfaces)",
            active_preset_surface_ids(&record.spec).len()
        );
        hub.set_pending(
            profile_id,
            EnvironmentPendingProposal {
                proposed_spec: record.spec.clone(),
                diff_summary,
                errors: vec![],
                proposed_at: Utc::now(),
                proposed_by: proposed_by.to_string(),
            },
        )
        .await;
        return Ok(EnvironmentPatchResponse {
            ok: true,
            live: false,
            pending_operator_approval: true,
            revision: None,
            errors: vec![],
            applied_ops,
        });
    }

    let updated = hub.put(record.spec, proposed_by).await?;
    Ok(EnvironmentPatchResponse {
        ok: true,
        live: true,
        pending_operator_approval: false,
        revision: Some(updated.revision),
        errors: vec![],
        applied_ops,
    })
}

pub fn patch_response_extras(spec: &EnvironmentSpec, surface_id: &str) -> serde_json::Value {
    let nav_visible = surface_nav_visible(spec, surface_id);
    crate::custom_view_status::nav_visibility_fields(spec, surface_id, nav_visible)
}

pub fn register_environment_patch_tool(
    registry: &mut stasis::application::orchestration::tool_registry::InMemoryToolRegistry,
) -> StasisResult<()> {
    registry.register_tool(CognitionEnvironmentPatchTool)
}

struct CognitionEnvironmentPatchTool;

#[async_trait]
impl StasisTool for CognitionEnvironmentPatchTool {
    fn name(&self) -> &'static str {
        COGNITION_ENVIRONMENT_PATCH
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Apply incremental environment spec ops. New custom surfaces, update_surface, set_environment_theme, and preset membership go live immediately; preset rewrites require operator approval.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(serde_json::json!({
            "type": "object",
            "required": ["ops"],
            "properties": {
                "profile_id": { "type": "string" },
                "ops": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "description": "Tagged op: add_custom_surface | add_to_active_preset | add_component | set_component_feeds | rewrite_active_preset_surfaces | update_surface | set_environment_theme | remove_custom_surface | remove_component",
                        "required": ["op"],
                        "properties": {
                            "op": { "type": "string" }
                        }
                    }
                }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let request: EnvironmentPatchRequest = serde_json::from_value(input).map_err(|err| {
            StasisError::PortFailure(format!("cognition_environment_patch: invalid input: {err}"))
        })?;
        let profile_id = resolve_profile_id(request.profile_id.as_deref());
        let response = execute_environment_patch(
            environment_hub(),
            &profile_id,
            &request.ops,
            "agent",
        )
        .await
        .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        let mut value = serde_json::to_value(response).map_err(|err| {
            StasisError::PortFailure(format!("cognition_environment_patch: encode error: {err}"))
        })?;
        if let Some(first_surface) = request.ops.iter().find_map(|op| match op {
            medousa_types::environment::EnvironmentPatchOp::AddCustomSurface { id, .. } => {
                Some(id.clone())
            }
            medousa_types::environment::EnvironmentPatchOp::AddToActivePreset { surface_id } => {
                Some(surface_id.clone())
            }
            _ => None,
        }) {
            if let Ok(record) = environment_hub().get(&profile_id).await {
                if let Some(obj) = value.as_object_mut() {
                    if let Some(extra) = patch_response_extras(&record.spec, &first_surface).as_object() {
                        for (key, val) in extra {
                            obj.insert(key.clone(), val.clone());
                        }
                    }
                }
            }
        }
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use medousa_types::environment_default::default_environment_spec;

    #[test]
    fn add_custom_surface_appends_to_active_preset() {
        let mut spec = default_environment_spec("default");
        let ops = vec![EnvironmentPatchOp::AddCustomSurface {
            id: "trip-london".to_string(),
            label: "Trip".to_string(),
            icon: "train-front".to_string(),
            layout: Some(SurfaceLayout::Dashboard),
            add_to_active_preset: true,
        }];
        apply_patch_ops(&mut spec, &ops).expect("patch");
        assert!(spec.surfaces.iter().any(|s| s.id == "trip-london"));
        assert!(surface_nav_visible(&spec, "trip-london"));
    }

    #[test]
    fn rewrite_preset_requires_proposal_flag() {
        let ops = vec![EnvironmentPatchOp::RewriteActivePresetSurfaces {
            surfaces: vec!["chat".to_string()],
        }];
        assert!(patch_requires_proposal(&ops));
    }
}
