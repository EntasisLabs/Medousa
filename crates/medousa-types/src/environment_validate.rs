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

const SAFETY_FLOOR_SURFACES: &[&str] = &[SAFETY_SURFACE_SETTINGS, SAFETY_SURFACE_RUNTIME];

const ALLOWED_MOBILE_TAB_SLUGS: &[&str] = &["home", "chat", "notes", "web"];

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
        errors.extend(crate::layout::validate_layout_tree(
            surface,
            &spec.components,
        ));
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
            if preset.label.trim().is_empty() {
                errors.push(format!("layout preset '{}' requires a label", preset.id));
            }
            for sid in &preset.surfaces {
                if !surface_ids.contains(&sid.as_str()) {
                    errors.push(format!(
                        "layout preset '{}' references unknown surface '{}'",
                        preset.id, sid
                    ));
                }
            }
            for safety in SAFETY_FLOOR_SURFACES {
                if !preset.surfaces.iter().any(|sid| sid == safety) {
                    errors.push(format!(
                        "layout preset '{}' must include safety surface '{safety}'",
                        preset.id
                    ));
                }
            }
        }
        if active_count > 1 {
            errors.push("only one layout preset may be active".to_string());
        }
        if active_count == 0 && !presets.is_empty() {
            errors.push("at least one layout preset must be active".to_string());
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
    if let Some(mobile_tab) = &surface.mobile_tab {
        if !ALLOWED_MOBILE_TAB_SLUGS.contains(&mobile_tab.as_str()) {
            errors.push(format!(
                "surface '{}' mobileTab '{}' is invalid — use home|chat|notes|web",
                surface.id, mobile_tab
            ));
        }
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
    if matches!(component.component_type, ComponentType::MedousaView) {
        let note_path = component
            .config
            .get("notePath")
            .or_else(|| component.config.get("note_path"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if note_path.trim().is_empty() {
            errors.push(format!(
                "medousa_view component '{}' requires config.notePath",
                component.id
            ));
        }
    }
    if matches!(component.component_type, ComponentType::Presentation) {
        let artifact_id = component
            .config
            .get("artifactId")
            .or_else(|| component.config.get("artifact_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if artifact_id.trim().is_empty() {
            errors.push(format!(
                "presentation component '{}' requires config.artifactId",
                component.id
            ));
        }
    }
    for feed_id in &component.feeds {
        if !crate::feed::is_valid_feed_id(feed_id) {
            errors.push(format!(
                "component '{}' feed id '{feed_id}' is invalid — use lowercase letters, digits, '.', '_', '-'",
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
            ComponentType::Presentation
                | ComponentType::MedousaView
                | ComponentType::Artifact
                | ComponentType::BuiltinPanel
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
            "component '{}' type '{:?}' is not rendered in Home Phase 1 — use type=presentation, medousa_view, or chrome_action",
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
                layout_root: None,
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
    fn writing_studio_demo_spec_validates() {
        use crate::environment_default::writing_studio_demo_spec;

        let spec = writing_studio_demo_spec("personal");
        assert!(is_valid_environment_spec(&spec));
    }

    #[test]
    fn rejects_presentation_component_on_builtin_home() {
        let mut spec = default_environment_spec("personal");
        spec.components.push(ComponentDef {
            id: "bad-home-widget".to_string(),
            component_type: ComponentType::Presentation,
            surface_id: "home".to_string(),
            slot: "main".to_string(),
            label: Some("Dashboard".to_string()),
            config: serde_json::json!({ "artifactId": "art-demo" }),
            presentation: Some(crate::environment::UiPresentation::Inline),
            feeds: vec![],
            updated_at: None,
        });
        let errors = validate_environment_spec(&spec);
        assert!(errors.iter().any(|e| e.contains("custom surface")));
        assert!(errors.iter().any(|e| e.contains("home")));
    }

    #[test]
    fn validates_hstack_layout_on_custom_surface() {
        let mut spec = default_environment_spec("personal");
        spec.surfaces.push(SurfaceDef {
            id: "studio".to_string(),
            label: "Studio".to_string(),
            icon: "grid".to_string(),
            kind: SurfaceKind::Custom,
            builtin_id: None,
            layout: crate::environment::SurfaceLayout::Dashboard,
            slots: vec![],
            mobile_tab: None,
            layout_root: Some(crate::layout::LayoutNode::HStack {
                spacing: crate::layout::StackSpacing::Md,
                align: crate::layout::StackAlign::Start,
                distribution: crate::layout::StackDistribution::FillEqually,
                children: vec![
                    crate::layout::LayoutNode::Component {
                        id: "left".to_string(),
                        flex: Some(1),
                    },
                    crate::layout::LayoutNode::Component {
                        id: "right".to_string(),
                        flex: Some(1),
                    },
                ],
            }),
        });
        spec.components.push(ComponentDef {
            id: "left".to_string(),
            component_type: ComponentType::Presentation,
            surface_id: "studio".to_string(),
            slot: "main".to_string(),
            label: None,
            config: serde_json::json!({ "artifactId": "art-left" }),
            presentation: Some(crate::environment::UiPresentation::Panel),
            feeds: vec![],
            updated_at: None,
        });
        spec.components.push(ComponentDef {
            id: "right".to_string(),
            component_type: ComponentType::Presentation,
            surface_id: "studio".to_string(),
            slot: "main".to_string(),
            label: None,
            config: serde_json::json!({ "artifactId": "art-right" }),
            presentation: Some(crate::environment::UiPresentation::Panel),
            feeds: vec![],
            updated_at: None,
        });
        assert!(is_valid_environment_spec(&spec));
    }

    #[test]
    fn rejects_missing_settings_surface() {
        let mut spec = default_environment_spec("personal");
        spec.surfaces.retain(|s| s.id != SAFETY_SURFACE_SETTINGS);
        assert!(!is_valid_environment_spec(&spec));
    }

    #[test]
    fn rejects_active_preset_missing_safety_floor() {
        let mut spec = default_environment_spec("personal");
        if let Some(presets) = &mut spec.layout_presets {
            for preset in presets.iter_mut() {
                if preset.active {
                    preset
                        .surfaces
                        .retain(|id| id != SAFETY_SURFACE_SETTINGS);
                }
            }
        }
        let errors = validate_environment_spec(&spec);
        assert!(errors.iter().any(|e| e.contains("safety surface")));
    }
}
