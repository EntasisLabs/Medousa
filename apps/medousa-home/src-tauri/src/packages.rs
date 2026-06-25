use std::path::PathBuf;
use std::process::Command;

use crate::workshop_runtime;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageStatusSummary {
    pub local_brain_installed: bool,
    pub installer_available: bool,
}

#[tauri::command]
pub fn packages_status() -> PackageStatusSummary {
    PackageStatusSummary {
        local_brain_installed: workshop_runtime::local_brain_installed(),
        installer_available: resolve_installer_app().is_some(),
    }
}

#[tauri::command]
pub fn packages_open_installer() -> Result<(), String> {
    let Some(path) = resolve_installer_app() else {
        return Err(
            "Medousa Installer not found. Download it from the latest GitHub release.".to_string(),
        );
    };

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|err| err.to_string())?;
    }

    #[cfg(target_os = "windows")]
    {
        Command::new(path)
            .spawn()
            .map_err(|err| err.to_string())?;
    }

    #[cfg(target_os = "linux")]
    {
        Command::new(path)
            .spawn()
            .map_err(|err| err.to_string())?;
    }

    Ok(())
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
