//! Default environment spec matching the current hardcoded Home experience.

use chrono::Utc;

use crate::environment::{
    ComponentType, EnvironmentSpec, LayoutPreset, MobileAskEntry, MobileTabBar, ShellChromeDef,
    ShellChromeMobile, SurfaceDef, SurfaceKind, SurfaceLayout, ENVIRONMENT_SPEC_VERSION,
    SAFETY_SURFACE_RUNTIME, SAFETY_SURFACE_SETTINGS,
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
        "work".to_string(),
        "library".to_string(),
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
        ("work", "Work", "layout-grid", Some("work"), None),
        ("library", "Library", "book-open", Some("library"), Some("notes")),
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
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_spec_includes_safety_surfaces() {
        let spec = default_environment_spec(DEFAULT_PROFILE_ID);
        let ids: Vec<_> = spec.surfaces.iter().map(|s| s.id.as_str()).collect();
        assert!(ids.contains(&SAFETY_SURFACE_SETTINGS));
        assert!(ids.contains(&SAFETY_SURFACE_RUNTIME));
    }
}
