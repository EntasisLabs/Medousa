//! Validation for environment specs — Frame / Chrome / Content design lock.

use crate::environment::{
    ComponentDef, ComponentType, EnvironmentSpec, EnvironmentTheme, SurfaceDef, SurfaceKind,
    CHROME_ACTION_OPEN_ACTIVITY, CHROME_ACTION_OPEN_ASK, COMPONENT_SLOT_FAB, COMPONENT_SLOT_HEADER,
    COMPONENT_SLOT_MAIN, COMPONENT_SLOT_SIDEBAR, SAFETY_SURFACE_RUNTIME, SAFETY_SURFACE_SETTINGS,
};
use crate::environment_icons::is_valid_surface_icon;
use crate::environment_themes::{is_valid_brand_color, is_valid_color_theme_id};

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

    if let Some(chrome) = &spec.shell_chrome
        && let Some(mobile) = &chrome.mobile
            && let Some(home) = &mobile.default_home
                && !surface_ids.contains(&home.as_str()) {
                    errors.push(format!(
                        "shellChrome.mobile.defaultHome references unknown surface '{home}'"
                    ));
                }

    if let Some(theme) = &spec.theme {
        validate_environment_theme(theme, &mut errors);
    }

    errors
}

fn validate_environment_theme(theme: &EnvironmentTheme, errors: &mut Vec<String>) {
    if let Some(id) = &theme.color_theme_id
        && !is_valid_color_theme_id(id) {
            errors.push(format!(
                "theme.colorThemeId '{id}' is invalid — use a known Room palette id"
            ));
        }
    if let Some(brand) = &theme.brand_color
        && !is_valid_brand_color(brand) {
            errors.push(
                "theme.brandColor must be a hex color (#RGB or #RRGGBB)".to_string(),
            );
        }
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
    if let Some(mobile_tab) = &surface.mobile_tab
        && !ALLOWED_MOBILE_TAB_SLUGS.contains(&mobile_tab.as_str()) {
            errors.push(format!(
                "surface '{}' mobileTab '{}' is invalid — use home|chat|notes|web",
                surface.id, mobile_tab
            ));
        }
    if !is_valid_surface_icon(&surface.icon) {
        errors.push(format!(
            "surface '{}' icon '{}' is not in the allowed icon catalog",
            surface.id, surface.icon
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
    if matches!(component.component_type, ComponentType::MediaEmbed) {
        validate_media_embed_component(component, errors);
    }
    if matches!(component.component_type, ComponentType::Scene) {
        // Presence-only: config.scene is an opaque payload the daemon never
        // interprets. Require a non-empty object so the client has something to
        // decode; do NOT validate the scene op shape here.
        let scene_ok = component
            .config
            .get("scene")
            .and_then(|v| v.as_object())
            .is_some_and(|obj| !obj.is_empty());
        if !scene_ok {
            errors.push(format!(
                "scene component '{}' requires a non-empty config.scene object (opaque scene payload, e.g. {{ ops: [...] }})",
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
                | ComponentType::MediaEmbed
                | ComponentType::Scene
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
            "component '{}' type '{:?}' is not rendered in Home Phase 1 — use type=presentation, medousa_view, media_embed, or chrome_action",
            component.id, component.component_type
        ));
    }
}

fn config_str<'a>(config: &'a serde_json::Value, camel: &str, snake: &str) -> Option<&'a str> {
    config
        .get(camel)
        .or_else(|| config.get(snake))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn validate_media_embed_component(component: &ComponentDef, errors: &mut Vec<String>) {
    let provider = config_str(&component.config, "provider", "provider").unwrap_or("");
    match provider {
        "spotify" | "apple_music" => {}
        _ => {
            errors.push(format!(
                "media_embed component '{}' requires config.provider spotify or apple_music (got '{provider}')",
                component.id
            ));
            return;
        }
    }
    let embed_url = config_str(&component.config, "embedUrl", "embed_url");
    let share_url = config_str(&component.config, "url", "url");
    let Some(url) = embed_url.or(share_url) else {
        errors.push(format!(
            "media_embed component '{}' requires config.embedUrl or config.url",
            component.id
        ));
        return;
    };
    if !url.starts_with("https://") {
        errors.push(format!(
            "media_embed component '{}' url must use https",
            component.id
        ));
        return;
    }
    match provider {
        "spotify" => {
            if !url.contains("open.spotify.com/embed/") && !url.contains("open.spotify.com/") {
                errors.push(format!(
                    "media_embed component '{}' spotify url must be open.spotify.com embed or share link",
                    component.id
                ));
            }
        }
        "apple_music"
            if !url.contains("embed.music.apple.com/") && !url.contains("music.apple.com/") => {
                errors.push(format!(
                    "media_embed component '{}' apple_music url must be embed.music.apple.com or music.apple.com link",
                    component.id
                ));
            }
        _ => {}
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
                icon: "pen-line".to_string(),
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

    fn push_custom_studio(spec: &mut crate::environment::EnvironmentSpec) {
        spec.surfaces.push(SurfaceDef {
            id: "studio".to_string(),
            label: "Studio".to_string(),
            icon: "pen-line".to_string(),
            kind: SurfaceKind::Custom,
            builtin_id: None,
            layout: crate::environment::SurfaceLayout::Dashboard,
            slots: vec![],
            mobile_tab: None,
            layout_root: None,
        });
    }

    #[test]
    fn accepts_scene_component_with_opaque_config() {
        let mut spec = default_environment_spec("personal");
        push_custom_studio(&mut spec);
        spec.components.push(ComponentDef {
            id: "trip-scene".to_string(),
            component_type: ComponentType::Scene,
            surface_id: "studio".to_string(),
            slot: "main".to_string(),
            label: Some("Trip".to_string()),
            // Opaque payload — daemon never inspects the op shape.
            config: serde_json::json!({ "scene": { "rev": 1, "ops": [{ "op": "plan_layout" }] } }),
            presentation: None,
            feeds: vec![],
            updated_at: None,
        });
        assert!(is_valid_environment_spec(&spec));
    }

    #[test]
    fn rejects_scene_component_without_scene_config() {
        let mut spec = default_environment_spec("personal");
        push_custom_studio(&mut spec);
        spec.components.push(ComponentDef {
            id: "empty-scene".to_string(),
            component_type: ComponentType::Scene,
            surface_id: "studio".to_string(),
            slot: "main".to_string(),
            label: None,
            config: serde_json::json!({}),
            presentation: None,
            feeds: vec![],
            updated_at: None,
        });
        let errors = validate_environment_spec(&spec);
        assert!(errors.iter().any(|e| e.contains("config.scene")));
    }

    #[test]
    fn rejects_scene_component_on_builtin_surface() {
        let mut spec = default_environment_spec("personal");
        spec.components.push(ComponentDef {
            id: "home-scene".to_string(),
            component_type: ComponentType::Scene,
            surface_id: "home".to_string(),
            slot: "main".to_string(),
            label: None,
            config: serde_json::json!({ "scene": { "ops": [{ "op": "plan_layout" }] } }),
            presentation: None,
            feeds: vec![],
            updated_at: None,
        });
        let errors = validate_environment_spec(&spec);
        assert!(errors.iter().any(|e| e.contains("custom surface")));
    }

    #[test]
    fn validates_hstack_layout_on_custom_surface() {
        let mut spec = default_environment_spec("personal");
        spec.surfaces.push(SurfaceDef {
            id: "studio".to_string(),
            label: "Studio".to_string(),
            icon: "layout-grid".to_string(),
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
