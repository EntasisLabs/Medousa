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
