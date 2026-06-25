//! Self-hosted release endpoint URL resolution.

const DEFAULT_RELEASE_CHANNEL: &str = "stable";

/// Resolve the release manifest URL from environment.
///
/// Priority: `MEDOUSA_RELEASE_MANIFEST_URL` > `MEDOUSA_RELEASE_BASE_URL` + channel + path >
/// legacy GitHub Releases fallback (dev/reference only).
pub fn release_manifest_url() -> String {
    if let Ok(url) = std::env::var("MEDOUSA_RELEASE_MANIFEST_URL") {
        if !url.trim().is_empty() {
            return url;
        }
    }

    if let Some(base) = release_base_url() {
        let channel = release_channel();
        let version = std::env::var("MEDOUSA_RELEASE_VERSION").ok();
        if let Some(version) = version.filter(|v| !v.trim().is_empty()) {
            let version = version.trim_start_matches('v');
            return format!("{base}/{channel}/v{version}/release-manifest.json");
        }
        return format!("{base}/{channel}/release-manifest.json");
    }

    // Dev/reference fallback — production should set MEDOUSA_RELEASE_BASE_URL.
    "https://github.com/EntasisLabs/Medousa/releases/latest/download/release-manifest.json"
        .to_string()
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
}

pub fn release_channel() -> String {
    std::env::var("MEDOUSA_RELEASE_CHANNEL")
        .unwrap_or_else(|_| DEFAULT_RELEASE_CHANNEL.to_string())
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
