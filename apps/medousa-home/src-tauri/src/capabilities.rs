//! Read/write `capabilities.toml` overlay — disabled bindings + web search prefs.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct WebSearchOverlay {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    preferred_provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    try_fallbacks: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct DisabledBindingRef {
    capability_id: String,
    source: String,
    reference: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct CapabilitiesOverlayFile {
    #[serde(default)]
    web_search: WebSearchOverlay,
    #[serde(default)]
    disabled_bindings: Vec<DisabledBindingRef>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilitiesOverlayLoadResult {
    pub path: String,
    pub file_exists: bool,
    pub disabled_bindings: Vec<DisabledBindingDto>,
    pub web_search: WebSearchOverlayDto,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisabledBindingDto {
    pub capability_id: String,
    pub source: String,
    pub reference: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSearchOverlayDto {
    pub preferred_provider: Option<String>,
    pub try_fallbacks: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilitiesSetBindingRequest {
    pub capability_id: String,
    pub source: String,
    pub reference: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilitiesMutationResult {
    pub ok: bool,
    pub message: String,
    pub path: String,
}

fn capabilities_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("medousa")
        .join("capabilities.toml")
}

fn load_overlay_file() -> Result<(CapabilitiesOverlayFile, PathBuf, bool), String> {
    let path = capabilities_path();
    if !path.exists() {
        return Ok((CapabilitiesOverlayFile::default(), path, false));
    }
    let raw = fs::read_to_string(&path).map_err(|err| err.to_string())?;
    let overlay = toml::from_str::<CapabilitiesOverlayFile>(&raw).map_err(|err| {
        format!("failed to parse {}: {err}", path.display())
    })?;
    Ok((overlay, path, true))
}

fn persist_overlay_file(overlay: &CapabilitiesOverlayFile) -> Result<PathBuf, String> {
    let path = capabilities_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let encoded = toml::to_string_pretty(overlay).map_err(|err| err.to_string())?;
    fs::write(&path, encoded).map_err(|err| err.to_string())?;
    Ok(path)
}

fn normalize_source(raw: &str) -> Result<String, String> {
    let source = raw.trim().to_ascii_lowercase();
    if source != "grapheme" && source != "mcp" {
        return Err("source must be grapheme or mcp".to_string());
    }
    Ok(source)
}

fn to_dto(entry: &DisabledBindingRef) -> DisabledBindingDto {
    DisabledBindingDto {
        capability_id: entry.capability_id.clone(),
        source: entry.source.clone(),
        reference: entry.reference.clone(),
    }
}

#[tauri::command]
pub fn capabilities_load_overlay() -> Result<CapabilitiesOverlayLoadResult, String> {
    let (overlay, path, file_exists) = load_overlay_file()?;
    Ok(CapabilitiesOverlayLoadResult {
        path: path.display().to_string(),
        file_exists,
        disabled_bindings: overlay.disabled_bindings.iter().map(to_dto).collect(),
        web_search: WebSearchOverlayDto {
            preferred_provider: overlay.web_search.preferred_provider.clone(),
            try_fallbacks: overlay.web_search.try_fallbacks,
        },
    })
}

#[tauri::command]
pub fn capabilities_set_binding_enabled(
    request: CapabilitiesSetBindingRequest,
) -> Result<CapabilitiesMutationResult, String> {
    let capability_id = request.capability_id.trim();
    let reference = request.reference.trim();
    if capability_id.is_empty() || reference.is_empty() {
        return Err("capability_id and reference are required".to_string());
    }
    let source = normalize_source(&request.source)?;

    let (mut overlay, _, _) = load_overlay_file()?;
    overlay.disabled_bindings.retain(|entry| {
        !(entry.capability_id.eq_ignore_ascii_case(capability_id)
            && entry.source.eq_ignore_ascii_case(&source)
            && entry.reference == reference)
    });
    if !request.enabled {
        overlay.disabled_bindings.push(DisabledBindingRef {
            capability_id: capability_id.to_string(),
            source,
            reference: reference.to_string(),
        });
    }

    let path = persist_overlay_file(&overlay)?;
    Ok(CapabilitiesMutationResult {
        ok: true,
        message: if request.enabled {
            "Binding enabled — reindexing catalog…".to_string()
        } else {
            "Binding disabled — reindexing catalog…".to_string()
        },
        path: path.display().to_string(),
    })
}

#[tauri::command]
pub fn capabilities_save_web_search(
    preferred_provider: Option<String>,
    try_fallbacks: Option<bool>,
) -> Result<CapabilitiesMutationResult, String> {
    let (mut overlay, _, _) = load_overlay_file()?;
    overlay.web_search.preferred_provider = preferred_provider
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    overlay.web_search.try_fallbacks = try_fallbacks;

    let path = persist_overlay_file(&overlay)?;
    Ok(CapabilitiesMutationResult {
        ok: true,
        message: "Web search preferences saved to capabilities.toml".to_string(),
        path: path.display().to_string(),
    })
}
