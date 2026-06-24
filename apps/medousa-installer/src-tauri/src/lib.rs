mod install;

use std::path::PathBuf;

use medousa::install::manifest::{read_install_manifest, write_install_manifest, InstallManifest, PackageInstallRecord};
use medousa::install::packages::{
    default_install_profiles, expand_package_dependencies, package_catalog,
    package_disk_estimate_bytes,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProfileSummary {
    id: String,
    display_name: String,
    packages: Vec<String>,
    size_label: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PackageSummary {
    id: String,
    display_name: String,
    depends: Vec<String>,
    size_label: String,
    optional: bool,
    selected: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BootstrapResponse {
    install_root: String,
    profiles: Vec<ProfileSummary>,
    packages: Vec<PackageSummary>,
    modify_mode: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstallRequest {
    install_root: String,
    package_ids: Vec<String>,
    modify_mode: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadProgressEvent {
    package_id: String,
    phase: String,
    percent: f32,
    message: String,
}

fn format_bytes(bytes: u64) -> String {
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    if bytes >= GB as u64 {
        format!("{:.1} GB", bytes as f64 / GB)
    } else {
        format!("{:.0} MB", bytes as f64 / MB)
    }
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

fn data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("medousa")
}

fn modify_mode_from_args() -> bool {
    std::env::args().any(|arg| arg == "--modify" || arg == "--repair" || arg == "--update")
}

#[tauri::command]
fn installer_bootstrap() -> Result<BootstrapResponse, String> {
    let install_root = default_install_root();
    let profiles = default_install_profiles()
        .into_iter()
        .map(|profile| {
            let packages: Vec<String> = profile
                .packages
                .iter()
                .map(|id| id.to_string())
                .collect();
            let bytes = package_disk_estimate_bytes(&packages);
            ProfileSummary {
                id: profile.id.to_string(),
                display_name: profile.display_name.to_string(),
                packages,
                size_label: format_bytes(bytes),
            }
        })
        .collect();

    let packages = package_catalog()
        .into_iter()
        .map(|entry| PackageSummary {
            id: entry.id.to_string(),
            display_name: entry.display_name.to_string(),
            depends: entry.depends.iter().map(|dep| dep.to_string()).collect(),
            size_label: format_bytes(entry.default_size_bytes),
            optional: entry.optional,
            selected: matches!(entry.id, "desktop" | "engine"),
        })
        .collect();

    Ok(BootstrapResponse {
        install_root: install_root.display().to_string(),
        profiles,
        packages,
        modify_mode: modify_mode_from_args()
            || install::install_manifest_path(&install_root).exists(),
    })
}

#[tauri::command]
async fn installer_run(app: AppHandle, request: InstallRequest) -> Result<(), String> {
    let install_root = PathBuf::from(request.install_root.trim());
    let expanded: Vec<String> = expand_package_dependencies(
        &request
            .package_ids
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
    );

    for package_id in expanded {
        emit_progress(
            &app,
            &package_id,
            "downloading",
            0.0,
            "Starting download…",
        );
        let result = install::install_package(
            &install_root,
            &data_dir(),
            &package_id,
            |percent, message| {
                emit_progress(&app, &package_id, "downloading", percent, message);
            },
        )
        .await;
        match result {
            Ok(()) => emit_progress(&app, &package_id, "ready", 100.0, "Installed"),
            Err(err) => {
                emit_progress(&app, &package_id, "failed", 0.0, &err);
                return Err(err);
            }
        }
    }

    let manifest_path = install::install_manifest_path(&install_root);
    let mut manifest = if manifest_path.exists() {
        read_install_manifest(&manifest_path)?
    } else {
        InstallManifest {
            schema_version: 2,
            product: "medousa".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            target: std::env::consts::ARCH.to_string(),
            built_at: chrono::Utc::now().to_rfc3339(),
            binaries: Vec::new(),
            component_set_id: String::new(),
            install_root: Some(install_root.display().to_string()),
            packages: Vec::new(),
        }
    };
    manifest.install_root = Some(install_root.display().to_string());
    for package_id in &request.package_ids {
        if manifest.packages.iter().any(|entry| entry.id == *package_id) {
            continue;
        }
        manifest.packages.push(PackageInstallRecord {
            id: package_id.clone(),
            version: manifest.version.clone(),
            install_path: Some(data_dir().join("packages").join(package_id).display().to_string()),
            sha256: None,
        });
    }
    write_install_manifest(&manifest_path, &manifest)?;
    Ok(())
}

fn emit_progress(app: &AppHandle, package_id: &str, phase: &str, percent: f32, message: &str) {
    let _ = app.emit(
        "install-progress",
        DownloadProgressEvent {
            package_id: package_id.to_string(),
            phase: phase.to_string(),
            percent,
            message: message.to_string(),
        },
    );
}

#[tauri::command]
fn installer_launch_medousa(app: AppHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let path = default_install_root();
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|err| err.to_string())?;
        let _ = app;
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        let path = default_install_root().join("Medousa.exe");
        std::process::Command::new(path)
            .spawn()
            .map_err(|err| err.to_string())?;
        let _ = app;
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        let candidates = [
            default_install_root().join("medousa-home"),
            PathBuf::from("/usr/bin/medousa-home"),
        ];
        for path in candidates {
            if path.exists() {
                std::process::Command::new(path)
                    .spawn()
                    .map_err(|err| err.to_string())?;
                let _ = app;
                return Ok(());
            }
        }
        Err("Medousa desktop app not found — install the desktop package first".to_string())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            installer_bootstrap,
            installer_run,
            installer_launch_medousa
        ])
        .run(tauri::generate_context!())
        .expect("error while running Medousa Installer");
}
