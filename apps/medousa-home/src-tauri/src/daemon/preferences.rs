//! Small preference changes must preserve the rest of the engine's defaults.
use super::{window_request::WorkshopRequest, workshop_http};
use serde_json::Value;

pub(super) static WRITE_GATE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn merge_preferences(current: &mut Value, patch: Value) -> Result<(), String> {
    let mut patch = patch
        .as_object()
        .ok_or("Expected a preference object")?
        .clone();
    for key in patch.keys() {
        if !matches!(
            key.as_str(),
            "provider"
                | "model"
                | "response_depth_mode"
                | "reasoning_effort"
                | "stage_routing"
                | "favorite_models"
                | "active_voice_id"
                | "custom_voice_presets"
        ) {
            return Err(format!("Unsupported preference: {key}"));
        }
    }
    for key in [
        "provider",
        "model",
        "response_depth_mode",
        "reasoning_effort",
        "active_voice_id",
    ] {
        if let Some(value) = patch.get(key) {
            serde_json::from_value::<Option<String>>(value.clone())
                .map_err(|_| format!("Invalid preference: {key}"))?;
        }
    }
    if let Some(value) = patch.get_mut("favorite_models") {
        let models: Option<Vec<crate::medousa_paths::FavoriteModelDto>> =
            serde_json::from_value(value.clone()).map_err(|_| "Invalid favorite models")?;
        *value = serde_json::to_value(models.map(|models| {
            models
                .into_iter()
                .filter(|model| !model.provider.trim().is_empty() && !model.model.trim().is_empty())
                .take(8)
                .collect::<Vec<_>>()
        }))
        .map_err(|error| error.to_string())?;
    }
    if let Some(value) = patch.get_mut("custom_voice_presets") {
        let presets = serde_json::from_value(value.clone()).map_err(|_| "Invalid voice presets")?;
        *value = serde_json::to_value(crate::medousa_paths::normalize_voice_presets(presets))
            .map_err(|error| error.to_string())?;
    }
    if let Some(Value::String(value)) = patch.get_mut("active_voice_id") {
        *value = value.trim().to_string();
    }
    let current = current.as_object_mut().ok_or("Invalid workshop defaults")?;
    // Existing explicit inference profiles must agree with the flat chat fields.
    if let Some(main) = current
        .get_mut("inference_profiles")
        .and_then(|profiles| profiles.get_mut("main"))
        .and_then(Value::as_object_mut)
    {
        for key in ["provider", "model"] {
            if let Some(value) = patch.get(key) {
                main.insert(key.into(), value.clone());
            }
        }
    }
    current.extend(patch);
    Ok(())
}

#[tauri::command]
pub async fn runtime_patch_tui_preferences(
    state: WorkshopRequest,
    patch: Value,
) -> Result<(), String> {
    // Resolve before waiting: a main-window switch cannot redirect this write.
    state.config().await?;
    let _write = WRITE_GATE.lock().await;
    let mut current: Value = workshop_http::get_json(&state, "/v1/runtime/tui-defaults").await?;
    merge_preferences(&mut current, patch)?;
    let _: Value = workshop_http::put_json(&state, "/v1/runtime/tui-defaults", &current).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn voice_changes_preserve_runtime_and_unknown_fields() {
        let mut current = json!({"provider":"a","active_voice_id":"old","future_setting":42});
        merge_preferences(
            &mut current,
            json!({"active_voice_id":"new","custom_voice_presets":[]}),
        )
        .unwrap();
        assert_eq!(
            current,
            json!({"provider":"a","active_voice_id":"new","custom_voice_presets":null,"future_setting":42})
        );
    }

    #[test]
    fn model_changes_keep_explicit_profile_in_sync() {
        let mut current = json!({"inference_profiles":{"main":{"provider":"old","model":"old","base_url":"keep"}}});
        merge_preferences(&mut current, json!({"provider":"next","model":"next"})).unwrap();
        assert_eq!(
            current["inference_profiles"]["main"],
            json!({"provider":"next","model":"next","base_url":"keep"})
        );
    }

    #[test]
    fn invalid_patch_cannot_partially_mutate_defaults() {
        let mut current = json!({"provider":"keep"});
        assert!(merge_preferences(
            &mut current,
            json!({"provider":"wrong","shell_allowed_binaries":[]})
        )
        .is_err());
        assert_eq!(current, json!({"provider":"keep"}));
    }
}
