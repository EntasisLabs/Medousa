use std::fs;
use std::path::PathBuf;
use std::process::Command;

use crate::workshop_runtime;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageStatusSummary {
    pub local_brain_installed: bool,
    pub installer_available: bool,
    pub installed_packages: Vec<String>,
    pub installed_version: Option<String>,
    pub release_base_url: Option<String>,
    pub update_available: bool,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstallManifestLite {
    version: String,
    packages: Vec<PackageRecordLite>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct PackageRecordLite {
    id: String,
}

#[tauri::command]
pub fn packages_status() -> PackageStatusSummary {
    let install_root = default_install_root();
    let data_dir = default_data_dir();
    let manifest = read_install_manifest_lite(&install_root.join("install-manifest.json"))
        .or_else(|| read_install_manifest_lite(&data_dir.join("install-manifest.json")));

    let installed_packages: Vec<String> = manifest
        .as_ref()
        .map(|m| m.packages.iter().map(|p| p.id.clone()).collect())
        .unwrap_or_default();

    PackageStatusSummary {
        local_brain_installed: workshop_runtime::local_brain_installed()
            || package_installed_marker(&data_dir, "local-brain"),
        installer_available: resolve_installer_app().is_some(),
        installed_packages,
        installed_version: manifest.as_ref().map(|m| m.version.clone()),
        release_base_url: release_base_url(),
        update_available: false,
    }
}

#[tauri::command]
pub fn packages_open_installer() -> Result<(), String> {
    let Some(path) = resolve_installer_app() else {
        if let Some(base) = release_base_url() {
            let bootstrap = format!("{base}/stable/installer-bootstrap.json");
            #[cfg(target_os = "windows")]
            {
                return Err(format!(
                    "Medousa Installer not found locally. For add-ons, download it from {bootstrap} (see installerUrl on Windows)."
                ));
            }
            #[cfg(not(target_os = "windows"))]
            {
                return Err(format!(
                    "Medousa Installer not found locally. Download it from {bootstrap}"
                ));
            }
        }
        return Err(
            "Medousa Installer not found. Set MEDOUSA_RELEASE_BASE_URL or install the installer app."
                .to_string(),
        );
    };

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg("-a")
            .arg(&path)
            .arg("--args")
            .arg("--modify")
            .spawn()
            .or_else(|_| Command::new("open").arg(path).spawn())
            .map_err(|err| err.to_string())?;
    }

    #[cfg(target_os = "windows")]
    {
        Command::new(&path)
            .arg("--modify")
            .spawn()
            .map_err(|err| err.to_string())?;
    }

    #[cfg(target_os = "linux")]
    {
        Command::new(&path)
            .arg("--modify")
            .spawn()
            .map_err(|err| err.to_string())?;
    }

    Ok(())
}

fn read_install_manifest_lite(path: &std::path::Path) -> Option<InstallManifestLite> {
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

fn package_installed_marker(data_dir: &std::path::Path, package_id: &str) -> bool {
    data_dir
        .join("packages")
        .join(package_id)
        .join(".installed")
        .is_file()
}

fn release_base_url() -> Option<String> {
    std::env::var("MEDOUSA_RELEASE_BASE_URL")
        .ok()
        .map(|value| value.trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
}

fn default_install_root() -> PathBuf {
    if cfg!(target_os = "macos") {
        PathBuf::from("/Applications/Medousa.app")
    } else if cfg!(windows) {
        PathBuf::from(r"C:\Program Files\Medousa")
    } else {
        PathBuf::from("/opt/medousa")
    }
}

fn default_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("medousa")
}

fn resolve_installer_app() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from("/Applications/Medousa Installer.app"),
        PathBuf::from(r"C:\Program Files\Medousa\MedousaInstaller.exe"),
        PathBuf::from("/opt/medousa/MedousaInstaller"),
        dirs::data_local_dir()
            .unwrap_or_default()
            .join("medousa")
            .join("MedousaInstaller"),
    ];
    candidates.into_iter().find(|path| path.exists())
}
