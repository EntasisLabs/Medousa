use medousa_types::environment::EnvironmentSpec;

pub fn surface_nav_visible(spec: &EnvironmentSpec, surface_id: &str) -> bool {
    active_preset_surface_ids(spec)
        .iter()
        .any(|id| id == surface_id)
}

pub fn active_preset_surface_ids(spec: &EnvironmentSpec) -> Vec<String> {
    if let Some(presets) = &spec.layout_presets {
        if let Some(active) = presets.iter().find(|preset| preset.active) {
            return active.surfaces.clone();
        }
        if let Some(id) = spec.active_preset_id.as_deref()
            && let Some(preset) = presets.iter().find(|preset| preset.id == id)
        {
            return preset.surfaces.clone();
        }
    }
    spec.surfaces
        .iter()
        .map(|surface| surface.id.clone())
        .collect()
}

pub fn nav_visibility_hint(surface_id: &str, nav_visible: bool) -> Option<String> {
    (!nav_visible).then(|| {
        format!(
            "Surface '{surface_id}' is not in the active layout preset — call cognition_environment_patch with add_to_active_preset or cognition_custom_view_compose."
        )
    })
}
