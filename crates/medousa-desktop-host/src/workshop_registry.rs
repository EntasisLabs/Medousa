use medousa_types::daemon_api::DEFAULT_DAEMON_URL;
use serde::{Deserialize, Serialize};

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

pub fn validate_registry(registry: &WorkshopRegistry) -> Result<(), String> {
    if registry.version != REGISTRY_VERSION {
        return Err(format!(
            "Unsupported workshop registry version {}",
            registry.version
        ));
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

/// Apply the desktop registry migrations without writing config or touching pairing credentials.
/// The Tauri adapter persists this result; read-only hosts need not do so.
pub fn normalize_desktop_registry(registry: &mut WorkshopRegistry) -> Result<bool, String> {
    let mut migrated = false;
    for workshop in &mut registry.workshops {
        if workshop.kind == "paired" {
            workshop.kind = "portal".into();
            migrated = true;
        }
    }
    if registry
        .workshops
        .iter()
        .any(|w| w.id == registry.active_workshop_id && w.kind == "peer")
    {
        registry.active_workshop_id = PERSONAL_WORKSHOP_ID.into();
        migrated = true;
    }
    validate_registry(registry)?;
    crate::workshop_runtime::backfill_local_workshop_fields(registry);
    Ok(migrated)
}

use medousa_types::authority_id::WorkshopScopeId;
use std::{fs, path::PathBuf};
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

pub fn medousa_data_dir() -> PathBuf {
    crate::paths::medousa_data_dir()
}

pub fn workshops_registry_path() -> PathBuf {
    medousa_data_dir().join("workshops.json")
}

pub fn pairing_credentials_rel_path(workshop_id: &str) -> Result<String, String> {
    let workshop_id = WorkshopScopeId::parse(workshop_id).map_err(|err| err.to_string())?;
    Ok(format!(
        "workshops/{}/pairing.json",
        workshop_id.storage_key().as_str()
    ))
}

pub fn pairing_credentials_abs_path(workshop_id: &str) -> Result<PathBuf, String> {
    Ok(medousa_data_dir().join(pairing_credentials_rel_path(workshop_id)?))
}

pub fn legacy_pairing_credentials_abs_path(workshop_id: &str) -> Option<PathBuf> {
    let valid = !workshop_id.is_empty()
        && workshop_id.len() <= 128
        && workshop_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    valid.then(|| {
        medousa_data_dir()
            .join("workshops")
            .join(workshop_id)
            .join("pairing.json")
    })
}

pub fn paired_workshop_id(workshop_device_id: &str) -> String {
    format!("paired-{workshop_device_id}")
}

fn legacy_daemon_url_path() -> PathBuf {
    medousa_data_dir().join("home_daemon_url.txt")
}

#[cfg(feature = "legacy-client-network")]
fn legacy_pairing_credentials_path() -> PathBuf {
    medousa_data_dir().join("pairing_credentials.json")
}

fn normalize_url(raw: &str) -> String {
    raw.trim().trim_end_matches('/').to_string()
}

pub fn load_registry() -> Result<WorkshopRegistry, String> {
    let path = workshops_registry_path();
    let raw = fs::read_to_string(&path).map_err(|err| err.to_string())?;
    let mut registry: WorkshopRegistry =
        serde_json::from_str(&raw).map_err(|err| err.to_string())?;
    let embedded_migrated = normalize_embedded_personal(
        &mut registry,
        cfg!(any(target_os = "ios", target_os = "android")),
    );
    let migrated = normalize_desktop_registry(&mut registry)? || embedded_migrated;
    if migrated {
        save_registry(&registry)?;
    }
    Ok(registry)
}

#[doc(hidden)]
pub fn normalize_embedded_personal(
    registry: &mut WorkshopRegistry,
    embedded_personal: bool,
) -> bool {
    if !embedded_personal {
        return false;
    }
    let Some(personal) = registry
        .workshops
        .iter_mut()
        .find(|workshop| workshop.id == PERSONAL_WORKSHOP_ID)
    else {
        return false;
    };
    let mut changed = false;
    if personal.kind != "local" {
        personal.kind = "local".to_string();
        changed = true;
    }
    if normalize_url(&personal.url) != normalize_url(DEFAULT_DAEMON_URL) {
        personal.url = DEFAULT_DAEMON_URL.to_string();
        changed = true;
    }
    if changed {
        personal.updated_at = now_iso();
    }
    changed
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

#[cfg(feature = "legacy-client-network")]
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

#[cfg(feature = "legacy-client-network")]
fn read_legacy_pairing() -> Option<LegacyPairingCredentialsFile> {
    let raw = fs::read_to_string(legacy_pairing_credentials_path()).ok()?;
    serde_json::from_str(&raw).ok()
}

#[cfg(feature = "legacy-client-network")]
fn move_legacy_pairing_file(dest: &std::path::Path) -> Result<(), String> {
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

#[cfg(feature = "legacy-client-network")]
fn is_loopback_url(url: &str) -> bool {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|parsed| {
            parsed
                .host_str()
                .map(|host| host == "127.0.0.1" || host == "localhost" || host == "::1")
        })
        .unwrap_or(false)
}

pub fn ensure_migrated() -> Result<WorkshopRegistry, String> {
    if workshops_registry_path().is_file() {
        return load_registry();
    }

    let mut registry = default_registry();
    let resolved = if cfg!(any(target_os = "ios", target_os = "android")) {
        DEFAULT_DAEMON_URL.to_string()
    } else {
        std::env::var("MEDOUSA_DAEMON_URL")
            .or_else(|_| std::env::var("STASIS_DAEMON_URL"))
            .ok()
            .map(|v| normalize_url(&v))
            .filter(|v| !v.is_empty())
            .or_else(read_legacy_daemon_url)
            .unwrap_or_else(|| DEFAULT_DAEMON_URL.to_string())
    };
    if let Some(personal) = registry
        .workshops
        .iter_mut()
        .find(|workshop| workshop.id == PERSONAL_WORKSHOP_ID)
    {
        personal.url = resolved.clone();
        personal.updated_at = now_iso();
    }

    #[cfg(feature = "legacy-client-network")]
    if let Some(legacy) = read_legacy_pairing() {
        let workshop_id = paired_workshop_id(&legacy.workshop_device_id);
        let credentials_path = pairing_credentials_abs_path(&workshop_id)?;
        move_legacy_pairing_file(&credentials_path)?;
        crate::pairing_client::migrate_legacy_session_token(
            &legacy.workshop_device_id,
            &legacy.pairing_id,
        );

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
                credentials_rel_path: Some(pairing_credentials_rel_path(&workshop_id)?),
                has_iroh_ticket: Some(has_iroh),
                workshop_peer_name: None,
            }),
            client_state: None,
        });
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

pub fn active_local_data_dir_for_url(url: &str) -> Option<PathBuf> {
    let registry = ensure_migrated().ok()?;
    let workshop = active_workshop(&registry)?;
    if workshop.kind != "local"
        || normalize_url(&crate::workshop_runtime::resolve_workshop_url(workshop))
            != normalize_url(url)
    {
        return None;
    }
    Some(crate::workshop_runtime::resolve_workshop_data_dir(workshop))
}

pub fn register_paired_workshop(input: RegisterPairedInput) -> Result<String, String> {
    let mut registry = ensure_migrated()?;
    let role = normalize_connection_role(&input.role);
    let workshop_id = if role == "peer" {
        format!("peer-{}", input.workshop_device_id)
    } else {
        paired_workshop_id(&input.workshop_device_id)
    };
    let credentials_rel = pairing_credentials_rel_path(&workshop_id)?;
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
