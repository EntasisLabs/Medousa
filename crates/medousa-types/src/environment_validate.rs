//! Validation for environment specs — Frame / Chrome / Content design lock.

use crate::environment::{
    ComponentDef, ComponentType, EnvironmentSpec, SurfaceDef, SurfaceKind,
    CHROME_ACTION_OPEN_ACTIVITY, CHROME_ACTION_OPEN_ASK, COMPONENT_SLOT_FAB, COMPONENT_SLOT_HEADER,
    COMPONENT_SLOT_MAIN, COMPONENT_SLOT_SIDEBAR, SAFETY_SURFACE_RUNTIME, SAFETY_SURFACE_SETTINGS,
};

const ALLOWED_SLOTS: &[&str] = &[
    COMPONENT_SLOT_MAIN,
    COMPONENT_SLOT_HEADER,
    COMPONENT_SLOT_FAB,
    COMPONENT_SLOT_SIDEBAR,
    "inline",
];

const ALLOWED_CHROME_ACTIONS: &[&str] = &[CHROME_ACTION_OPEN_ASK, CHROME_ACTION_OPEN_ACTIVITY];

pub fn validate_environment_spec(spec: &EnvironmentSpec) -> Vec<String> {
    let mut errors = Vec::new();

    if spec.version == 0 {
        errors.push("version must be >= 1".to_string());
    }
    if spec.profile_id.trim().is_empty() {
        errors.push("profile_id is required".to_string());
    }

    let surface_ids: Vec<&str> = spec.surfaces.iter().map(|s| s.id.as_str()).collect();
    if !surface_ids.contains(&SAFETY_SURFACE_SETTINGS) {
        errors.push(format!(
            "safety floor: '{SAFETY_SURFACE_SETTINGS}' surface must be present"
        ));
    }
    if !surface_ids.contains(&SAFETY_SURFACE_RUNTIME) {
        errors.push(format!(
            "safety floor: '{SAFETY_SURFACE_RUNTIME}' surface must be present"
        ));
    }

    let mut seen_surfaces = std::collections::HashSet::new();
    for surface in &spec.surfaces {
        if !seen_surfaces.insert(surface.id.clone()) {
            errors.push(format!("duplicate surface id '{}'", surface.id));
        }
        validate_surface(surface, &mut errors);
    }

    let mut seen_components = std::collections::HashSet::new();
    for component in &spec.components {
        if !seen_components.insert(component.id.clone()) {
            errors.push(format!("duplicate component id '{}'", component.id));
        }
        validate_component(component, &surface_ids, &spec.surfaces, &mut errors);
    }

    if let Some(presets) = &spec.layout_presets {
        let mut active_count = 0usize;
        for preset in presets {
            if preset.active {
                active_count += 1;
            }
            for sid in &preset.surfaces {
                if !surface_ids.contains(&sid.as_str()) {
                    errors.push(format!(
                        "layout preset '{}' references unknown surface '{}'",
                        preset.id, sid
                    ));
                }
            }
        }
        if active_count > 1 {
            errors.push("only one layout preset may be active".to_string());
        }
    }

    if let Some(chrome) = &spec.shell_chrome {
        if let Some(mobile) = &chrome.mobile {
            if let Some(home) = &mobile.default_home {
                if !surface_ids.contains(&home.as_str()) {
                    errors.push(format!(
                        "shellChrome.mobile.defaultHome references unknown surface '{home}'"
                    ));
                }
            }
        }
    }

    errors
}

fn validate_surface(surface: &SurfaceDef, errors: &mut Vec<String>) {
    if surface.id.trim().is_empty() {
        errors.push("surface id is required".to_string());
    }
    if surface.label.trim().is_empty() {
        errors.push(format!("surface '{}' requires a label", surface.id));
    }
    if matches!(surface.kind, SurfaceKind::Builtin) && surface.builtin_id.is_none() {
        errors.push(format!(
            "builtin surface '{}' requires builtin_id",
            surface.id
        ));
    }
}

fn validate_component(
    component: &ComponentDef,
    surface_ids: &[&str],
    surfaces: &[SurfaceDef],
    errors: &mut Vec<String>,
) {
    if component.id.trim().is_empty() {
        errors.push("component id is required".to_string());
    }
    if !surface_ids.contains(&component.surface_id.as_str()) {
        errors.push(format!(
            "component '{}' references unknown surface '{}'",
            component.id, component.surface_id
        ));
    }
    if !ALLOWED_SLOTS.contains(&component.slot.as_str()) {
        errors.push(format!(
            "component '{}' slot '{}' is not in allowed zones",
            component.id, component.slot
        ));
    }
    if matches!(component.component_type, ComponentType::ChromeAction) {
        let action = component
            .config
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if !ALLOWED_CHROME_ACTIONS.contains(&action) {
            errors.push(format!(
                "chrome-action component '{}' has unknown action '{action}'",
                component.id
            ));
        }
    }
    validate_component_surface_kind(component, surfaces, errors);
}

fn validate_component_surface_kind(
    component: &ComponentDef,
    surfaces: &[SurfaceDef],
    errors: &mut Vec<String>,
) {
    let Some(surface) = surfaces
        .iter()
        .find(|entry| entry.id == component.surface_id)
    else {
        return;
    };
    if surface.kind != SurfaceKind::Custom
        && matches!(
            component.component_type,
            ComponentType::Presentation | ComponentType::Artifact | ComponentType::BuiltinPanel
        )
    {
        errors.push(format!(
            "component '{}' type '{:?}' must target a custom surface (got builtin '{}') — use kind=custom surfaces only",
            component.id, component.component_type, surface.id
        ));
    }
    if matches!(
        component.component_type,
        ComponentType::Artifact | ComponentType::BuiltinPanel
    ) {
        errors.push(format!(
            "component '{}' type '{:?}' is not rendered in Home Phase 1 — use type=presentation or chrome_action",
            component.id, component.component_type
        ));
    }
}

pub fn is_valid_environment_spec(spec: &EnvironmentSpec) -> bool {
    validate_environment_spec(spec).is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment_default::default_environment_spec;
    use crate::environment::{activate_layout_preset, ComponentDef, ComponentType, SurfaceKind};

    #[test]
    fn default_spec_is_valid() {
        let spec = default_environment_spec("personal");
        assert!(is_valid_environment_spec(&spec));
    }

    #[test]
    fn rejects_unsupported_component_types() {
        let mut spec = default_environment_spec("personal");
        spec.surfaces
            .push(crate::environment::SurfaceDef {
                id: "studio".to_string(),
                label: "Studio".to_string(),
                icon: "pen".to_string(),
                kind: SurfaceKind::Custom,
                builtin_id: None,
                layout: crate::environment::SurfaceLayout::Dashboard,
                slots: vec![],
                mobile_tab: None,
            });
        spec.components.push(ComponentDef {
            id: "bad".to_string(),
            component_type: ComponentType::BuiltinPanel,
            surface_id: "studio".to_string(),
            slot: "main".to_string(),
            label: None,
            config: serde_json::json!({}),
            presentation: None,
            feeds: vec![],
            updated_at: None,
        });
        let errors = validate_environment_spec(&spec);
        assert!(errors.iter().any(|e| e.contains("not rendered in Home Phase 1")));
    }

    #[test]
    fn activate_layout_preset_marks_active() {
        let mut spec = default_environment_spec("personal");
        activate_layout_preset(&mut spec, crate::environment_default::FOCUS_PRESET_ID)
            .expect("focus preset");
        assert_eq!(spec.active_preset_id.as_deref(), Some("focus"));
        let presets = spec.layout_presets.as_ref().unwrap();
        assert!(presets.iter().any(|p| p.id == "focus" && p.active));
    }

    #[test]
    fn rejects_missing_settings() {
        let mut spec = default_environment_spec("personal");
        spec.surfaces.retain(|s| s.id != SAFETY_SURFACE_SETTINGS);
        assert!(!is_valid_environment_spec(&spec));
    }
}
