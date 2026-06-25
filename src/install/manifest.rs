use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallManifest {
    pub schema_version: u32,
    pub product: String,
    pub version: String,
    pub target: String,
    pub built_at: String,
    #[serde(default)]
    pub binaries: Vec<String>,
    #[serde(default)]
    pub component_set_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_dir: Option<String>,
    #[serde(default)]
    pub packages: Vec<PackageInstallRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageInstallRecord {
    pub id: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default)]
    pub binaries: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseManifest {
    pub schema_version: u32,
    pub product: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    pub published_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    pub packages: HashMap<String, ReleasePackage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleasePackage {
    pub id: String,
    pub display_name: String,
    pub version: String,
    pub target: String,
    pub url: String,
    pub sha256: String,
    pub size_bytes: u64,
    #[serde(default)]
    pub depends: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default)]
    pub binaries: Vec<String>,
    #[serde(default)]
    pub workload_ids: Vec<String>,
}

pub fn read_install_manifest(path: &Path) -> Result<InstallManifest, String> {
    let raw = fs::read_to_string(path).map_err(|err| err.to_string())?;
    serde_json::from_str(&raw).map_err(|err| err.to_string())
}

pub fn write_install_manifest(path: &Path, manifest: &InstallManifest) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let raw = serde_json::to_string_pretty(manifest).map_err(|err| err.to_string())?;
    fs::write(path, raw).map_err(|err| err.to_string())
}

pub fn read_release_manifest(path: &Path) -> Result<ReleaseManifest, String> {
    let raw = fs::read_to_string(path).map_err(|err| err.to_string())?;
    serde_json::from_str(&raw).map_err(|err| err.to_string())
}

pub fn install_manifest_path(install_root: &Path) -> PathBuf {
    install_root.join("install-manifest.json")
}

pub fn user_packages_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("packages")
}

pub fn shared_bin_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("bin")
}

pub fn package_installed(data_dir: &Path, package_id: &str) -> bool {
    user_packages_dir(data_dir)
        .join(package_id)
        .join(".installed")
        .is_file()
}

pub fn mark_package_installed(data_dir: &Path, package_id: &str) -> Result<(), String> {
    let marker = user_packages_dir(data_dir).join(package_id).join(".installed");
    if let Some(parent) = marker.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    fs::write(marker, chrono::Utc::now().to_rfc3339()).map_err(|err| err.to_string())
}

pub fn unmark_package_installed(data_dir: &Path, package_id: &str) -> Result<(), String> {
    let marker = user_packages_dir(data_dir).join(package_id).join(".installed");
    if marker.exists() {
        fs::remove_file(marker).map_err(|err| err.to_string())?;
    }
    Ok(())
}
