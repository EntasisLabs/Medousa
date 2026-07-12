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

/// Whether a release manifest entry targets this machine (Rust triple or platform key).
pub fn release_package_matches_host(pkg: &ReleasePackage) -> bool {
    use crate::release_config::{host_platform_key, host_target};

    pkg.target == "any"
        || pkg.target == host_platform_key()
        || pkg.target == host_target()
}

/// Desktop bundles must match the host OS artifact type (guards against bad manifests).
pub fn desktop_artifact_url_matches_host(url: &str) -> bool {
    let url_lower = url.to_lowercase();
    #[cfg(windows)]
    {
        return url_lower.ends_with(".msi") || url_lower.ends_with(".exe");
    }
    #[cfg(target_os = "macos")]
    {
        url_lower.ends_with(".dmg")
    }
    #[cfg(target_os = "linux")]
    {
        return url_lower.ends_with(".appimage") || url_lower.ends_with(".deb");
    }
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        let _ = url_lower;
        false
    }
}

fn release_package_usable_on_host(pkg: &ReleasePackage) -> bool {
    if !release_package_matches_host(pkg) {
        return false;
    }
    if pkg.id == "desktop" && !desktop_artifact_url_matches_host(&pkg.url) {
        return false;
    }
    true
}

/// Resolve a catalog package id to the correct release manifest entry for this host.
pub fn resolve_release_package<'a>(
    manifest: &'a ReleaseManifest,
    package_id: &str,
) -> Result<&'a ReleasePackage, String> {
    use crate::release_config::{host_platform_key, host_target};

    let target = std::env::var("MEDOUSA_INSTALL_TARGET").unwrap_or_else(|_| host_target());
    let platform = host_platform_key();
    let keys = [
        format!("{package_id}-{target}"),
        format!("{package_id}-{platform}"),
        package_id.to_string(),
    ];
    for key in &keys {
        if let Some(pkg) = manifest.packages.get(key)
            && release_package_usable_on_host(pkg) {
                return Ok(pkg);
            }
    }
    manifest
        .packages
        .values()
        .find(|entry| entry.id == package_id && release_package_usable_on_host(entry))
        .ok_or_else(|| format!("release package not found for {package_id} ({target})"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn pkg(id: &str, target: &str, url: &str) -> ReleasePackage {
        ReleasePackage {
            id: id.to_string(),
            display_name: id.to_string(),
            version: "0.1.0".to_string(),
            target: target.to_string(),
            url: url.to_string(),
            sha256: String::new(),
            size_bytes: 1,
            depends: vec![],
            backend: None,
            description: None,
            category: None,
            icon: None,
            binaries: vec![],
            workload_ids: vec![],
        }
    }

    #[test]
    fn resolve_skips_wrong_desktop_artifact_for_host() {
        let mut packages = HashMap::new();
        packages.insert(
            "desktop-windows-x64".to_string(),
            pkg(
                "desktop",
                "windows-x64",
                "https://example.com/Medousa_0.1.0_aarch64.dmg",
            ),
        );
        let manifest = ReleaseManifest {
            schema_version: 2,
            product: "medousa".to_string(),
            version: "0.1.0".to_string(),
            channel: None,
            published_at: String::new(),
            base_url: None,
            packages,
        };

        #[cfg(windows)]
        assert!(resolve_release_package(&manifest, "desktop").is_err());

        #[cfg(target_os = "macos")]
        assert!(resolve_release_package(&manifest, "desktop").is_ok());
    }
}
