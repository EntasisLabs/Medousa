//! Default environment spec matching the current hardcoded Home experience.

use chrono::Utc;

use crate::environment::{
    ComponentDef, ComponentType, EnvironmentSpec, LayoutPreset, MobileAskEntry, MobileTabBar,
    ShellChromeDef, ShellChromeMobile, SurfaceDef, SurfaceKind, SurfaceLayout, UiPresentation,
    CHROME_ACTION_OPEN_ASK, ENVIRONMENT_SPEC_VERSION, SAFETY_SURFACE_RUNTIME,
    SAFETY_SURFACE_SETTINGS,
};

pub const DEFAULT_PROFILE_ID: &str = "personal";
pub const DEFAULT_PRESET_ID: &str = "default";
pub const FOCUS_PRESET_ID: &str = "focus";

pub fn default_environment_spec(profile_id: impl Into<String>) -> EnvironmentSpec {
    let profile_id = profile_id.into();
    let now = Utc::now();
    EnvironmentSpec {
        version: ENVIRONMENT_SPEC_VERSION,
        profile_id: profile_id.clone(),
        surfaces: default_surfaces(),
        components: Vec::new(),
        layout_presets: Some(vec![
            LayoutPreset {
                id: DEFAULT_PRESET_ID.to_string(),
                label: "Default".to_string(),
                active: true,
                surfaces: default_surface_ids(),
                shell_chrome: Some(default_shell_chrome()),
            },
            LayoutPreset {
                id: FOCUS_PRESET_ID.to_string(),
                label: "Focus".to_string(),
                active: false,
                surfaces: vec![
                    "chat".to_string(),
                    "peers".to_string(),
                    "work".to_string(),
                    "library".to_string(),
                    SAFETY_SURFACE_SETTINGS.to_string(),
                    SAFETY_SURFACE_RUNTIME.to_string(),
                ],
                shell_chrome: Some(default_shell_chrome()),
            },
        ]),
        active_preset_id: Some(DEFAULT_PRESET_ID.to_string()),
        shell_chrome: Some(default_shell_chrome()),
        theme: None,
        updated_at: now,
        updated_by: "system".to_string(),
    }
}

pub fn default_shell_chrome() -> ShellChromeDef {
    ShellChromeDef {
        mobile: Some(ShellChromeMobile {
            default_home: Some("home".to_string()),
            ask_entry: Some(MobileAskEntry::Inline),
            tab_bar: Some(MobileTabBar::Full),
        }),
        desktop: None,
    }
}

pub fn default_surface_ids() -> Vec<String> {
    vec![
        "home".to_string(),
        "chat".to_string(),
        "peers".to_string(),
        "work".to_string(),
        "library".to_string(),
        "calendar".to_string(),
        "web".to_string(),
        "context".to_string(),
        "workshop".to_string(),
        "automations".to_string(),
        "messaging".to_string(),
        SAFETY_SURFACE_RUNTIME.to_string(),
        SAFETY_SURFACE_SETTINGS.to_string(),
    ]
}

pub fn default_surfaces() -> Vec<SurfaceDef> {
    let builtin = [
        ("home", "Home", "home", Some("home"), Some("home")),
        ("chat", "Chat", "message-circle", Some("chat"), Some("chat")),
        ("peers", "Peers", "users", Some("peers"), None),
        ("work", "Work", "layout-grid", Some("work"), None),
        ("library", "Library", "book-open", Some("library"), Some("notes")),
        ("calendar", "Calendar", "calendar-days", Some("calendar"), None),
        ("web", "Web", "globe", Some("web"), Some("web")),
        ("context", "Context", "orbit", Some("context"), None),
        ("workshop", "Capabilities", "zap", Some("workshop"), None),
        ("automations", "Automations", "calendar", Some("automations"), None),
        ("messaging", "Messaging", "radio", Some("messaging"), None),
        (
            SAFETY_SURFACE_RUNTIME,
            "Runtime",
            "activity",
            Some(SAFETY_SURFACE_RUNTIME),
            None,
        ),
        (
            SAFETY_SURFACE_SETTINGS,
            "Settings",
            "settings",
            Some(SAFETY_SURFACE_SETTINGS),
            None,
        ),
    ];

    builtin
        .into_iter()
        .map(|(id, label, icon, builtin_id, mobile_tab)| SurfaceDef {
            id: id.to_string(),
            label: label.to_string(),
            icon: icon.to_string(),
            kind: SurfaceKind::Builtin,
            builtin_id: builtin_id.map(str::to_string),
            layout: SurfaceLayout::Single,
            slots: vec![],
            mobile_tab: mobile_tab.map(str::to_string),
            layout_root: None,
        })
        .collect()
}

/// Phase 1 proof demo — custom home, FAB ask, presentation + chrome_action components.
pub fn writing_studio_demo_spec(profile_id: impl Into<String>) -> EnvironmentSpec {
    let mut spec = default_environment_spec(profile_id);
    spec.surfaces.push(SurfaceDef {
        id: "writing-studio".to_string(),
        label: "Writing studio".to_string(),
        icon: "pen-line".to_string(),
        kind: SurfaceKind::Custom,
        builtin_id: None,
        layout: SurfaceLayout::Dashboard,
        slots: vec![],
        mobile_tab: None,
        layout_root: None,
    });

    if let Some(presets) = &mut spec.layout_presets {
        for preset in presets.iter_mut() {
            if preset.id == DEFAULT_PRESET_ID && !preset.surfaces.iter().any(|id| id == "writing-studio") {
                preset.surfaces.push("writing-studio".to_string());
            }
        }
    }

    spec.shell_chrome = Some(ShellChromeDef {
        mobile: Some(ShellChromeMobile {
            default_home: Some("writing-studio".to_string()),
            ask_entry: Some(MobileAskEntry::Fab),
            tab_bar: Some(MobileTabBar::Full),
        }),
        desktop: None,
    });

    spec.components.push(ComponentDef {
        id: "writing-manuscript".to_string(),
        component_type: ComponentType::Presentation,
        surface_id: "writing-studio".to_string(),
        slot: "main".to_string(),
        label: Some("Manuscript".to_string()),
        config: serde_json::json!({ "artifactId": "art-writing-demo" }),
        presentation: Some(UiPresentation::Inline),
        feeds: vec![],
        updated_at: Some(spec.updated_at),
    });
    spec.components.push(ComponentDef {
        id: "writing-ask-fab".to_string(),
        component_type: ComponentType::ChromeAction,
        surface_id: "writing-studio".to_string(),
        slot: "fab".to_string(),
        label: Some("Ask".to_string()),
        config: serde_json::json!({ "action": CHROME_ACTION_OPEN_ASK }),
        presentation: None,
        feeds: vec![],
        updated_at: Some(spec.updated_at),
    });
    spec.updated_by = "demo".to_string();
    spec
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment_validate::is_valid_environment_spec;

    #[test]
    fn default_spec_includes_safety_surfaces() {
        let spec = default_environment_spec(DEFAULT_PROFILE_ID);
        let ids: Vec<_> = spec.surfaces.iter().map(|s| s.id.as_str()).collect();
        assert!(ids.contains(&SAFETY_SURFACE_SETTINGS));
        assert!(ids.contains(&SAFETY_SURFACE_RUNTIME));
    }

    #[test]
    fn writing_studio_demo_spec_validates() {
        let spec = writing_studio_demo_spec(DEFAULT_PROFILE_ID);
        assert!(is_valid_environment_spec(&spec));
        assert_eq!(
            spec.shell_chrome
                .as_ref()
                .and_then(|c| c.mobile.as_ref())
                .and_then(|m| m.default_home.as_deref()),
            Some("writing-studio")
        );
    }
}
