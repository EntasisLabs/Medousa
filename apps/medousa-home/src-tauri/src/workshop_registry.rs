//! Client-side registry of Medousa Engine connections (ADR-003).

use crate::daemon::{apply_daemon_url, resolve_daemon_url, DaemonState};
use crate::workshop_runtime::resolve_workshop_url;
use crate::daemon::types::DEFAULT_DAEMON_URL;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::State;

pub const REGISTRY_VERSION: u32 = 1;
pub const PERSONAL_WORKSHOP_ID: &str = "personal";
pub const MAX_WORKSHOPS: usize = 10;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkshopPairingRef {
    pub pairing_id: String,
    pub phone_id: String,
    pub workshop_device_id: String,
    pub paired_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credentials_rel_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_iroh_ticket: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workshop_peer_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkshopClientState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_theme_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkshopServer {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_connected_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brand_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tagline: Option<String>,
    /// Absolute engine storage root for local workshops (`MEDOUSA_DATA_DIR`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_dir: Option<String>,
    /// Loopback bind address for local engine spawn, e.g. `127.0.0.1:7420`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pairing: Option<WorkshopPairingRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_state: Option<WorkshopClientState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkshopRegistry {
    pub version: u32,
    pub active_workshop_id: String,
    pub workshops: Vec<WorkshopServer>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterPairedInput {
    pub pairing_id: String,
    pub phone_id: String,
    pub workshop_device_id: String,
    pub workshop_peer_name: String,
    pub daemon_url: String,
    pub paired_at: String,
    pub has_iroh_ticket: bool,
    /// `portal` (full client) or `peer` (inbox/share only).
    pub role: String,
}

pub fn normalize_connection_role(raw: &str) -> &'static str {
    match raw.trim().to_ascii_lowercase().as_str() {
        "peer" => "peer",
        _ => "portal",
    }
}

pub fn is_portal_kind(kind: &str) -> bool {
    matches!(kind, "local" | "portal" | "paired")
}

pub fn is_peer_kind(kind: &str) -> bool {
    kind == "peer"
}

fn migrate_kind(kind: &str) -> String {
    if kind == "paired" {
        "portal".to_string()
    } else {
        kind.to_string()
    }
}

pub fn medousa_data_dir() -> PathBuf {
    crate::paths::medousa_data_dir()
}

pub fn workshops_registry_path() -> PathBuf {
    medousa_data_dir().join("workshops.json")
}

pub fn pairing_credentials_rel_path(workshop_id: &str) -> String {
    format!("workshops/{workshop_id}/pairing.json")
}

pub fn pairing_credentials_abs_path(workshop_id: &str) -> PathBuf {
    medousa_data_dir().join(pairing_credentials_rel_path(workshop_id))
}

pub fn paired_workshop_id(workshop_device_id: &str) -> String {
    format!("paired-{workshop_device_id}")
}

fn legacy_daemon_url_path() -> PathBuf {
    medousa_data_dir().join("home_daemon_url.txt")
}

fn legacy_pairing_credentials_path() -> PathBuf {
    medousa_data_dir().join("pairing_credentials.json")
}

fn normalize_url(raw: &str) -> String {
    raw.trim().trim_end_matches('/').to_string()
}

pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

pub fn default_personal_workshop() -> WorkshopServer {
    let iso = now_iso();
    WorkshopServer {
        id: PERSONAL_WORKSHOP_ID.to_string(),
        label: "Personal".to_string(),
        kind: "local".to_string(),
        url: DEFAULT_DAEMON_URL.to_string(),
        icon: Some("home".to_string()),
        created_at: iso.clone(),
        updated_at: iso,
        last_connected_at: None,
        brand_color: None,
        tagline: None,
        data_dir: None,
        bind: Some(crate::workshop_runtime::DEFAULT_LOCAL_BIND.to_string()),
        pairing: None,
        client_state: None,
    }
}

pub fn default_registry() -> WorkshopRegistry {
    WorkshopRegistry {
        version: REGISTRY_VERSION,
        active_workshop_id: PERSONAL_WORKSHOP_ID.to_string(),
        workshops: vec![default_personal_workshop()],
    }
}

pub fn load_registry() -> Result<WorkshopRegistry, String> {
    let path = workshops_registry_path();
    let raw = fs::read_to_string(&path).map_err(|err| err.to_string())?;
    let mut registry: WorkshopRegistry = serde_json::from_str(&raw).map_err(|err| err.to_string())?;
    let mut migrated = false;
    for workshop in &mut registry.workshops {
        let next_kind = migrate_kind(&workshop.kind);
        if next_kind != workshop.kind {
            workshop.kind = next_kind;
            migrated = true;
        }
    }
    // Peer credentials must never be the active workshop.
    if let Some(active) = registry
        .workshops
        .iter()
        .find(|workshop| workshop.id == registry.active_workshop_id)
    {
        if is_peer_kind(&active.kind) {
            registry.active_workshop_id = PERSONAL_WORKSHOP_ID.to_string();
            migrated = true;
        }
    }
    validate_registry(&registry)?;
    crate::workshop_runtime::backfill_local_workshop_fields(&mut registry);
    if migrated {
        save_registry(&registry)?;
    }
    Ok(registry)
}

fn validate_registry(registry: &WorkshopRegistry) -> Result<(), String> {
    if registry.version != REGISTRY_VERSION {
        return Err(format!("Unsupported workshop registry version {}", registry.version));
    }
    if registry.workshops.is_empty() {
        return Err("Workshop registry must contain at least one workshop".to_string());
    }
    if !registry
        .workshops
        .iter()
        .any(|workshop| workshop.id == registry.active_workshop_id)
    {
        return Err("Active workshop id not found in registry".to_string());
    }
    Ok(())
}

pub fn save_registry(registry: &WorkshopRegistry) -> Result<(), String> {
    validate_registry(registry)?;
    let path = workshops_registry_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let body = serde_json::to_string_pretty(registry).map_err(|err| err.to_string())?;
    fs::write(path, body).map_err(|err| err.to_string())
}

fn read_legacy_daemon_url() -> Option<String> {
    let raw = fs::read_to_string(legacy_daemon_url_path()).ok()?;
    let trimmed = normalize_url(&raw);
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyPairingCredentialsFile {
    pairing_id: String,
    phone_id: String,
    workshop_device_id: String,
    daemon_url: String,
    paired_at: String,
    #[serde(default)]
    iroh_ticket: Option<String>,
}

fn read_legacy_pairing() -> Option<LegacyPairingCredentialsFile> {
    let raw = fs::read_to_string(legacy_pairing_credentials_path()).ok()?;
    serde_json::from_str(&raw).ok()
}

fn move_legacy_pairing_file(dest: &Path) -> Result<(), String> {
    let legacy = legacy_pairing_credentials_path();
    if !legacy.is_file() {
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    if dest.is_file() {
        fs::remove_file(&legacy).map_err(|err| err.to_string())?;
        return Ok(());
    }
    fs::rename(&legacy, dest).or_else(|_| {
        fs::copy(&legacy, dest).map_err(|err| err.to_string())?;
        fs::remove_file(&legacy).map_err(|err| err.to_string())
    })?;
    Ok(())
}

fn is_loopback_url(url: &str) -> bool {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(|host| {
            host == "127.0.0.1" || host == "localhost" || host == "::1"
        }))
        .unwrap_or(false)
}

pub fn ensure_migrated() -> Result<WorkshopRegistry, String> {
    if workshops_registry_path().is_file() {
        return load_registry();
    }

    let mut registry = default_registry();
    let resolved = normalize_url(&resolve_daemon_url());
    if let Some(personal) = registry
        .workshops
        .iter_mut()
        .find(|workshop| workshop.id == PERSONAL_WORKSHOP_ID)
    {
        personal.url = resolved.clone();
        personal.updated_at = now_iso();
    }

    if let Some(legacy) = read_legacy_pairing() {
        let workshop_id = paired_workshop_id(&legacy.workshop_device_id);
        let credentials_path = pairing_credentials_abs_path(&workshop_id);
        move_legacy_pairing_file(&credentials_path)?;
        crate::pairing_client::migrate_legacy_session_token(&legacy.workshop_device_id);

        let url = normalize_url(&legacy.daemon_url);
        let label = if legacy.daemon_url.trim().is_empty() {
            "Paired workshop".to_string()
        } else if is_loopback_url(&url) {
            "Personal".to_string()
        } else {
            format!("Workshop {}", legacy.workshop_device_id)
        };

        let has_iroh = legacy
            .iroh_ticket
            .as_ref()
            .is_some_and(|ticket| !ticket.trim().is_empty());

        registry.workshops.push(WorkshopServer {
            id: workshop_id.clone(),
            label,
            kind: "portal".to_string(),
            url: url.clone(),
            icon: Some("building".to_string()),
            created_at: legacy.paired_at.clone(),
            updated_at: now_iso(),
            last_connected_at: None,
            brand_color: None,
            tagline: None,
            data_dir: None,
            bind: None,
            pairing: Some(WorkshopPairingRef {
                pairing_id: legacy.pairing_id,
                phone_id: legacy.phone_id,
                workshop_device_id: legacy.workshop_device_id,
                paired_at: legacy.paired_at,
                credentials_rel_path: Some(pairing_credentials_rel_path(&workshop_id)),
                has_iroh_ticket: Some(has_iroh),
                workshop_peer_name: None,
            }),
            client_state: None,
        });

        if !is_loopback_url(&url) || is_loopback_url(&resolved) && url != resolved {
            registry.active_workshop_id = workshop_id;
        }
    }

    save_registry(&registry)?;
    Ok(registry)
}

pub fn active_workshop(registry: &WorkshopRegistry) -> Option<&WorkshopServer> {
    registry
        .workshops
        .iter()
        .find(|workshop| workshop.id == registry.active_workshop_id)
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
    workshop.url = normalize_url(url);
    workshop.updated_at = now_iso();
    save_registry(&registry)
}

pub fn register_paired_workshop(input: RegisterPairedInput) -> Result<String, String> {
    let mut registry = ensure_migrated()?;
    let role = normalize_connection_role(&input.role);
    let workshop_id = if role == "peer" {
        format!("peer-{}", input.workshop_device_id)
    } else {
        paired_workshop_id(&input.workshop_device_id)
    };
    let credentials_rel = pairing_credentials_rel_path(&workshop_id);
    let label = if input.workshop_peer_name.trim().is_empty() {
        format!("Workshop {}", input.workshop_device_id)
    } else {
        input.workshop_peer_name.trim().to_string()
    };
    let url = normalize_url(&input.daemon_url);
    let pairing = WorkshopPairingRef {
        pairing_id: input.pairing_id,
        phone_id: input.phone_id,
        workshop_device_id: input.workshop_device_id,
        paired_at: input.paired_at,
        credentials_rel_path: Some(credentials_rel),
        has_iroh_ticket: Some(input.has_iroh_ticket),
        workshop_peer_name: Some(label.clone()),
    };

    let icon = if role == "peer" {
        Some("users".to_string())
    } else {
        Some("building".to_string())
    };

    if let Some(existing) = registry
        .workshops
        .iter_mut()
        .find(|workshop| workshop.id == workshop_id)
    {
        existing.label = label;
        existing.url = url;
        existing.kind = role.to_string();
        existing.icon = icon;
        existing.updated_at = now_iso();
        existing.pairing = Some(pairing);
    } else {
        if registry.workshops.len() >= MAX_WORKSHOPS {
            return Err(format!(
                "Maximum of {MAX_WORKSHOPS} workshops reached — remove one before adding another."
            ));
        }
        let iso = now_iso();
        registry.workshops.push(WorkshopServer {
            id: workshop_id.clone(),
            label,
            kind: role.to_string(),
            url,
            icon,
            created_at: iso.clone(),
            updated_at: iso,
            last_connected_at: None,
            brand_color: None,
            tagline: None,
            data_dir: None,
            bind: None,
            pairing: Some(pairing),
            client_state: None,
        });
    }

    save_registry(&registry)?;
    Ok(workshop_id)
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
        let ensure = crate::workshop_runtime::ensure_local_engine(
            &workshop,
            crate::workshop_runtime::should_load_private_brain(false),
        )
        .await?;
        if !ensure.ok {
            return Err(ensure.message);
        }
    }

    registry.active_workshop_id = trimmed.to_string();
    let now = now_iso();
    if let Some(workshop) = registry.workshops.iter_mut().find(|entry| entry.id == trimmed) {
        workshop.last_connected_at = Some(now.clone());
        workshop.updated_at = now;
    }
    save_registry(&registry)?;
    apply_daemon_url(&state, &resolve_workshop_url(&workshop))?;
    crate::workshop_transport::invalidate_workshop_route_cache();
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
    let Some(workshop) = registry.workshops.iter_mut().find(|entry| entry.id == trimmed_id) else {
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
        registry.active_workshop_id = PERSONAL_WORKSHOP_ID.to_string();
    }
    save_registry(&registry)?;
    if switched_active {
        sync_daemon_state_from_registry(&state)?;
        crate::workshop_transport::invalidate_workshop_route_cache();
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

    let mut client_state = workshop.client_state.clone().unwrap_or(WorkshopClientState {
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
    fn default_registry_has_personal() {
        let registry = default_registry();
        assert_eq!(registry.active_workshop_id, PERSONAL_WORKSHOP_ID);
        assert_eq!(registry.workshops.len(), 1);
    }
}
