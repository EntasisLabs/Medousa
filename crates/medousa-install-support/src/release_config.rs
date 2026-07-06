//! Self-hosted release endpoint URL resolution.

use std::sync::OnceLock;

const DEFAULT_RELEASE_CHANNEL: &str = "stable";

static EMBEDDED_BASE_URL: OnceLock<String> = OnceLock::new();
static EMBEDDED_CHANNEL: OnceLock<String> = OnceLock::new();

/// Register compile-time or installer-config defaults (called once at app startup).
pub fn set_embedded_release_defaults(base_url: String, channel: String) {
    let _ = EMBEDDED_BASE_URL.set(base_url);
    let _ = EMBEDDED_CHANNEL.set(channel);
}

/// Resolve the release manifest URL from environment.
///
/// Priority: `MEDOUSA_RELEASE_MANIFEST_URL` > `MEDOUSA_RELEASE_BASE_URL` + channel + path >
/// embedded defaults (installer build) > legacy GitHub Releases fallback (dev/reference only).
pub fn release_manifest_url() -> String {
    if let Ok(url) = std::env::var("MEDOUSA_RELEASE_MANIFEST_URL") {
        if !url.trim().is_empty() {
            return url;
        }
    }

    if let Some(base) = release_base_url() {
        return compose_manifest_url(&base, &release_channel(), pinned_release_version());
    }

    // Dev/reference fallback — production should set MEDOUSA_RELEASE_BASE_URL or embed at build.
    "https://github.com/EntasisLabs/Medousa/releases/latest/download/release-manifest.json"
        .to_string()
}

fn compose_manifest_url(base: &str, channel: &str, version: Option<String>) -> String {
    if let Some(version) = version.filter(|v| !v.trim().is_empty()) {
        let version = version.trim_start_matches('v');
        return format!("{base}/{channel}/v{version}/release-manifest.json");
    }
    format!("{base}/{channel}/release-manifest.json")
}

fn pinned_release_version() -> Option<String> {
    std::env::var("MEDOUSA_RELEASE_VERSION")
        .ok()
        .filter(|value| !value.trim().is_empty())
}

pub fn installer_bootstrap_url() -> String {
    if let Ok(url) = std::env::var("MEDOUSA_INSTALLER_BOOTSTRAP_URL") {
        if !url.trim().is_empty() {
            return url;
        }
    }
    if let Some(base) = release_base_url() {
        let channel = release_channel();
        return format!("{base}/{channel}/installer-bootstrap.json");
    }
    String::new()
}

pub fn release_base_url() -> Option<String> {
    std::env::var("MEDOUSA_RELEASE_BASE_URL")
        .ok()
        .map(|value| value.trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| EMBEDDED_BASE_URL.get().cloned())
}

pub fn release_channel() -> String {
    std::env::var("MEDOUSA_RELEASE_CHANNEL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| EMBEDDED_CHANNEL.get().cloned())
        .unwrap_or_else(|| DEFAULT_RELEASE_CHANNEL.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_manifest_url_stable_channel() {
        assert_eq!(
            compose_manifest_url("https://releases.example.com/medousa", "stable", None),
            "https://releases.example.com/medousa/stable/release-manifest.json"
        );
    }

    #[test]
    fn compose_manifest_url_versioned_channel() {
        assert_eq!(
            compose_manifest_url(
                "https://releases.example.com/medousa",
                "stable",
                Some("0.2.0".to_string())
            ),
            "https://releases.example.com/medousa/stable/v0.2.0/release-manifest.json"
        );
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallerBootstrap {
    pub schema_version: u32,
    pub product: String,
    pub version: String,
    pub channel: String,
    pub published_at: String,
    pub manifest_url: String,
    pub platforms: std::collections::HashMap<String, InstallerBootstrapPlatform>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallerBootstrapPlatform {
    pub platform: String,
    pub version: String,
    pub file_name: String,
    pub url: String,
    pub sha256: String,
    pub size_bytes: u64,
}

pub fn host_platform_key() -> &'static str {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "macos-aarch64"
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "macos-x64"
    } else if cfg!(windows) {
        "windows-x64"
    } else if cfg!(target_os = "linux") {
        "linux-x64"
    } else {
        "unknown"
    }
}

pub fn host_target() -> String {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "aarch64-apple-darwin".to_string()
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "x86_64-apple-darwin".to_string()
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "x86_64-unknown-linux-gnu".to_string()
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        "aarch64-unknown-linux-gnu".to_string()
    } else if cfg!(windows) {
        "x86_64-pc-windows-msvc".to_string()
    } else {
        "unknown".to_string()
    }
}
