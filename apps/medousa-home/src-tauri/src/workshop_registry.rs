//! Client-side registry of Medousa Engine connections (ADR-003).

#[cfg(test)]
use crate::daemon::types::DEFAULT_DAEMON_URL;
use crate::daemon::{DaemonState, apply_daemon_url};
use crate::workshop_runtime::resolve_workshop_url;

use std::fs;
use tauri::State;

#[cfg(test)]
use medousa_desktop_host::workshop_registry::default_personal_workshop;
#[allow(unused_imports)] // Preserve the renderer adapter public API.
pub use medousa_desktop_host::workshop_registry::{
    MAX_WORKSHOPS, PERSONAL_WORKSHOP_ID, WorkshopClientState, WorkshopPairingRef, WorkshopRegistry,
    WorkshopServer, default_registry, now_iso, validate_registry,
};

#[cfg(test)]
use medousa_desktop_host::workshop_registry::normalize_embedded_personal;
#[allow(unused_imports)] // Preserve the renderer adapter public API.
pub use medousa_desktop_host::workshop_registry::{
    RegisterPairedInput, active_local_data_dir_for_url, active_workshop, ensure_migrated,
    is_peer_kind, is_portal_kind, legacy_pairing_credentials_abs_path, load_registry,
    medousa_data_dir, normalize_connection_role, paired_workshop_id, pairing_credentials_abs_path,
    pairing_credentials_rel_path, register_paired_workshop, save_registry, workshops_registry_path,
};

fn normalize_url(raw: &str) -> String {
    raw.trim().trim_end_matches('/').to_string()
}

pub fn sync_daemon_state_from_registry(state: &DaemonState) -> Result<(), String> {
    let registry = ensure_migrated()?;
    let Some(workshop) = active_workshop(&registry) else {
        return Err("No active workshop in registry".to_string());
    };
    apply_daemon_url(state, &resolve_workshop_url(workshop))
}

pub fn update_active_workshop_url(url: &str) -> Result<(), String> {
    let mut registry = ensure_migrated()?;
    let Some(workshop) = registry
        .workshops
        .iter_mut()
        .find(|entry| entry.id == registry.active_workshop_id)
    else {
        return Err("Active workshop not found".to_string());
    };
    if cfg!(any(target_os = "ios", target_os = "android"))
        && workshop.id == PERSONAL_WORKSHOP_ID
        && workshop.kind == "local"
    {
        return Err(
            "Personal uses the embedded daemon; add and select a remote workshop instead"
                .to_string(),
        );
    }
    workshop.url = normalize_url(url);
    workshop.updated_at = now_iso();
    save_registry(&registry)
}

#[tauri::command]
pub fn workshops_load() -> Result<WorkshopRegistry, String> {
    ensure_migrated()
}

#[tauri::command]
pub async fn workshops_set_active(
    state: State<'_, DaemonState>,
    workshop_id: String,
) -> Result<WorkshopRegistry, String> {
    let trimmed = workshop_id.trim();
    if trimmed.is_empty() {
        return Err("Workshop id is required".to_string());
    }

    let mut registry = ensure_migrated()?;
    let Some(workshop) = registry
        .workshops
        .iter()
        .find(|entry| entry.id == trimmed)
        .cloned()
    else {
        return Err("Workshop not found".to_string());
    };

    if is_peer_kind(&workshop.kind) {
        return Err(
            "Peer connections are inbox-only — use Peers, not the workshop switcher.".to_string(),
        );
    }

    if workshop.kind == "local" {
        #[cfg(any(target_os = "ios", target_os = "android"))]
        if workshop.id != PERSONAL_WORKSHOP_ID {
            return Err("Mobile supports only the personal embedded local workshop".to_string());
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            let ensure = crate::workshop_runtime::ensure_local_engine(
                &workshop,
                crate::workshop_runtime::should_load_private_brain(false),
            )
            .await?;
            if !ensure.ok {
                return Err(ensure.message);
            }
        }
    }

    state.cancel_selected_streams();
    registry.active_workshop_id = trimmed.to_string();
    let now = now_iso();
    if let Some(workshop) = registry
        .workshops
        .iter_mut()
        .find(|entry| entry.id == trimmed)
    {
        workshop.last_connected_at = Some(now.clone());
        workshop.updated_at = now;
    }
    save_registry(&registry)?;
    apply_daemon_url(&state, &resolve_workshop_url(&workshop))?;
    crate::workshop_transport::invalidate_all_route_caches();
    load_registry()
}

#[tauri::command]
pub fn workshops_rename(workshop_id: String, label: String) -> Result<WorkshopRegistry, String> {
    let trimmed_id = workshop_id.trim();
    let trimmed_label = label.trim();
    if trimmed_id.is_empty() {
        return Err("Workshop id is required".to_string());
    }
    if trimmed_label.is_empty() {
        return Err("Label is required".to_string());
    }

    let mut registry = ensure_migrated()?;
    let Some(workshop) = registry
        .workshops
        .iter_mut()
        .find(|entry| entry.id == trimmed_id)
    else {
        return Err("Workshop not found".to_string());
    };
    workshop.label = trimmed_label.to_string();
    workshop.updated_at = now_iso();
    save_registry(&registry)?;
    load_registry()
}

#[tauri::command]
pub fn workshops_remove(
    state: State<'_, DaemonState>,
    workshop_id: String,
) -> Result<WorkshopRegistry, String> {
    let trimmed = workshop_id.trim();
    if trimmed.is_empty() {
        return Err("Workshop id is required".to_string());
    }
    if trimmed == PERSONAL_WORKSHOP_ID {
        return Err("The Personal workshop cannot be removed".to_string());
    }

    let mut registry = ensure_migrated()?;
    let previous_active = registry.active_workshop_id.clone();
    let removed = registry
        .workshops
        .iter()
        .find(|workshop| workshop.id == trimmed)
        .cloned();
    registry.workshops.retain(|workshop| workshop.id != trimmed);
    if registry.workshops.is_empty() {
        return Err("Cannot remove the last workshop".to_string());
    }

    if let Some(workshop) = removed {
        if workshop.kind == "local" {
            crate::workshop_runtime::stop_local_engine(&workshop.id);
        }
        if workshop.kind != "local" {
            let device_id = workshop
                .pairing
                .as_ref()
                .map(|pairing| pairing.workshop_device_id.as_str());
            crate::pairing_client::remove_workshop_credentials(&workshop.id, device_id)?;
        }
    }

    let switched_active = previous_active == trimmed;
    if switched_active {
        state.cancel_selected_streams();
        registry.active_workshop_id = PERSONAL_WORKSHOP_ID.to_string();
    }
    save_registry(&registry)?;
    if switched_active {
        sync_daemon_state_from_registry(&state)?;
        crate::workshop_transport::invalidate_all_route_caches();
    }
    load_registry()
}

#[tauri::command]
pub fn workshops_update_client_state(
    workshop_id: String,
    last_session_id: Option<String>,
    color_theme_id: Option<String>,
) -> Result<WorkshopRegistry, String> {
    let trimmed_id = workshop_id.trim();
    if trimmed_id.is_empty() {
        return Err("Workshop id is required".to_string());
    }

    let mut registry = ensure_migrated()?;
    let Some(workshop) = registry
        .workshops
        .iter_mut()
        .find(|entry| entry.id == trimmed_id)
    else {
        return Err("Workshop not found".to_string());
    };

    let mut client_state = workshop
        .client_state
        .clone()
        .unwrap_or(WorkshopClientState {
            last_session_id: None,
            color_theme_id: None,
        });
    if let Some(session_id) = last_session_id {
        let trimmed = session_id.trim().to_string();
        client_state.last_session_id = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        };
    }
    if let Some(theme_id) = color_theme_id {
        let trimmed = theme_id.trim().to_string();
        client_state.color_theme_id = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        };
    }
    workshop.client_state = Some(client_state);
    workshop.updated_at = now_iso();
    save_registry(&registry)?;
    load_registry()
}

fn normalize_brand_color(raw: &str) -> Result<Option<String>, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let upper = trimmed.to_uppercase();
    if !upper.starts_with('#') {
        return Err("Brand color must be a hex value like #7C3AED".to_string());
    }
    let hex = &upper[1..];
    if hex.len() != 3 && hex.len() != 6 {
        return Err("Brand color must be #RGB or #RRGGBB".to_string());
    }
    if !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err("Brand color must be a valid hex value".to_string());
    }
    Ok(Some(format!("#{hex}")))
}

fn normalize_icon(raw: Option<String>) -> Result<Option<String>, String> {
    let Some(value) = raw else {
        return Ok(None);
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    match trimmed {
        "home" | "building" | "team" => Ok(Some(trimmed.to_string())),
        _ => Err("Icon must be home, building, or team".to_string()),
    }
}

#[tauri::command]
pub fn workshops_update_branding(
    workshop_id: String,
    icon: Option<String>,
    brand_color: Option<String>,
    tagline: Option<String>,
) -> Result<WorkshopRegistry, String> {
    let trimmed_id = workshop_id.trim();
    if trimmed_id.is_empty() {
        return Err("Workshop id is required".to_string());
    }

    let mut registry = ensure_migrated()?;
    let Some(workshop) = registry
        .workshops
        .iter_mut()
        .find(|entry| entry.id == trimmed_id)
    else {
        return Err("Workshop not found".to_string());
    };

    if let Some(icon_value) = icon {
        workshop.icon = normalize_icon(Some(icon_value))?;
    }
    if let Some(color_value) = brand_color {
        workshop.brand_color = normalize_brand_color(&color_value)?;
    }
    if let Some(tagline_value) = tagline {
        let trimmed = tagline_value.trim();
        workshop.tagline = if trimmed.is_empty() {
            None
        } else if trimmed.len() > 80 {
            return Err("Tagline must be 80 characters or fewer".to_string());
        } else {
            Some(trimmed.to_string())
        };
    }
    workshop.updated_at = now_iso();
    save_registry(&registry)?;
    load_registry()
}

#[tauri::command]
pub fn workshops_add_local(label: String, data_dir: String) -> Result<WorkshopRegistry, String> {
    let trimmed_label = label.trim();
    if trimmed_label.is_empty() {
        return Err("Label is required".to_string());
    }
    let data_path = crate::workshop_runtime::validate_engine_data_dir(&data_dir)?;
    fs::create_dir_all(&data_path).map_err(|err| err.to_string())?;

    let mut registry = ensure_migrated()?;
    if registry.workshops.len() >= MAX_WORKSHOPS {
        return Err(format!(
            "Maximum of {MAX_WORKSHOPS} workshops reached — remove one before adding another."
        ));
    }

    let bind = crate::workshop_runtime::allocate_local_bind(&registry)?;
    let url = crate::workshop_runtime::url_from_bind(&bind);
    let workshop_id = format!("local-{}", uuid::Uuid::new_v4().simple());
    let iso = now_iso();
    registry.workshops.push(WorkshopServer {
        id: workshop_id,
        label: trimmed_label.to_string(),
        kind: "local".to_string(),
        url,
        icon: Some("building".to_string()),
        created_at: iso.clone(),
        updated_at: iso,
        last_connected_at: None,
        brand_color: None,
        tagline: None,
        data_dir: Some(data_path.display().to_string()),
        bind: Some(bind),
        pairing: None,
        client_state: None,
    });
    save_registry(&registry)?;
    load_registry()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paired_workshop_id_is_stable() {
        assert_eq!(paired_workshop_id("abcd1234"), "paired-abcd1234");
    }

    #[test]
    fn pairing_credentials_use_opaque_non_aliasing_directories() {
        let colon = pairing_credentials_rel_path("paired-a:b").unwrap();
        let underscore = pairing_credentials_rel_path("paired-a_b").unwrap();
        assert_ne!(colon, underscore);
        assert!(!colon.contains("a:b"));
        let traversal = pairing_credentials_rel_path("../../outside").unwrap();
        assert!(!traversal.contains(".."));
        assert_eq!(traversal.split('/').count(), 3);
    }

    #[test]
    fn default_registry_has_personal() {
        let registry = default_registry();
        assert_eq!(registry.active_workshop_id, PERSONAL_WORKSHOP_ID);
        assert_eq!(registry.workshops.len(), 1);
    }

    #[test]
    fn embedded_personal_repairs_a_remote_development_url_only_for_personal() {
        let mut registry = default_registry();
        registry.workshops[0].url = "http://dev-mac.invalid:7419".to_string();
        let mut remote = default_personal_workshop();
        remote.id = "paired-canary".to_string();
        remote.kind = "portal".to_string();
        remote.url = "https://remote-canary.invalid".to_string();
        registry.workshops.push(remote);

        assert!(normalize_embedded_personal(&mut registry, true));
        assert_eq!(registry.workshops[0].url, DEFAULT_DAEMON_URL);
        assert_eq!(registry.workshops[1].url, "https://remote-canary.invalid");
        assert_eq!(registry.active_workshop_id, PERSONAL_WORKSHOP_ID);
    }

    #[test]
    fn desktop_personal_keeps_its_configured_local_address() {
        let mut registry = default_registry();
        registry.workshops[0].url = "http://127.0.0.1:8123".to_string();
        assert!(!normalize_embedded_personal(&mut registry, false));
        assert_eq!(registry.workshops[0].url, "http://127.0.0.1:8123");
    }

    #[test]
    fn portal_kind_qualifies_for_saved_pairing_credentials() {
        assert!(is_portal_kind("portal"));
        assert!(is_portal_kind("paired"));
        assert!(is_portal_kind("local"));
        assert!(!is_portal_kind("peer"));
    }
}
