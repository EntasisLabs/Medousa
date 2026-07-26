//! Desktop app update detection against the channel release-manifest.

use medousa_install_support::{
    fetch_release_manifest, release_base_url, release_channel, resolve_release_package,
};
use tauri::AppHandle;
use tauri_plugin_opener::open_url;

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateStatus {
    pub current_version: String,
    pub latest_version: Option<String>,
    pub update_available: bool,
    pub download_url: Option<String>,
    pub release_base_url: Option<String>,
    pub channel: String,
    pub error: Option<String>,
}

fn normalize_version(raw: &str) -> String {
    raw.trim().trim_start_matches('v').trim().to_string()
}

/// Parse `major.minor.patch` (extra suffixes ignored). Missing parts → 0.
fn parse_semver_tuple(raw: &str) -> (u64, u64, u64) {
    let cleaned = normalize_version(raw);
    let mut nums = cleaned
        .split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<u64>().ok());
    (
        nums.next().unwrap_or(0),
        nums.next().unwrap_or(0),
        nums.next().unwrap_or(0),
    )
}

fn version_is_newer(remote: &str, local: &str) -> bool {
    parse_semver_tuple(remote) > parse_semver_tuple(local)
}

#[tauri::command]
pub async fn app_update_status(app: AppHandle) -> Result<AppUpdateStatus, String> {
    let current_version = normalize_version(&app.package_info().version.to_string());
    let channel = release_channel();
    let base = release_base_url();

    let manifest = match fetch_release_manifest().await {
        Ok(manifest) => manifest,
        Err(err) => {
            return Ok(AppUpdateStatus {
                current_version,
                latest_version: None,
                update_available: false,
                download_url: None,
                release_base_url: base,
                channel,
                error: Some(err),
            });
        }
    };

    match resolve_release_package(&manifest, "desktop") {
        Ok(pkg) => {
            let latest_version = normalize_version(&pkg.version);
            let update_available = version_is_newer(&latest_version, &current_version);
            Ok(AppUpdateStatus {
                current_version,
                latest_version: Some(latest_version),
                update_available,
                download_url: Some(pkg.url.clone()),
                release_base_url: base,
                channel,
                error: None,
            })
        }
        Err(err) => Ok(AppUpdateStatus {
            current_version,
            latest_version: Some(normalize_version(&manifest.version)),
            update_available: false,
            download_url: None,
            release_base_url: base,
            channel,
            error: Some(err),
        }),
    }
}

#[tauri::command]
pub async fn app_update_open_download(app: AppHandle) -> Result<(), String> {
    let status = app_update_status(app).await?;
    let Some(url) = status.download_url.filter(|value| !value.trim().is_empty()) else {
        return Err(status
            .error
            .unwrap_or_else(|| "No desktop installer URL for this host.".to_string()));
    };
    open_url(url, None::<&str>).map_err(|err| err.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_newer_patch() {
        assert!(version_is_newer("0.5.1", "0.5.0"));
        assert!(!version_is_newer("0.5.0", "0.5.0"));
        assert!(!version_is_newer("0.4.9", "0.5.0"));
    }

    #[test]
    fn strips_v_prefix() {
        assert!(version_is_newer("v0.6.0", "0.5.0"));
        assert_eq!(parse_semver_tuple("v1.2.3-beta"), (1, 2, 3));
    }
}
