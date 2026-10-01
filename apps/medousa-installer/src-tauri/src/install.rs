use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use medousa_install_support::manifest::{ReleaseManifest, ReleasePackage, mark_package_installed};
use medousa_install_support::packages::{
    catalog_entry, is_desktop_package, is_model_pack, is_tarball_package,
};
use medousa_install_support::tarball_install::{
    download_url, install_tarball_package, remove_tarball_package, verify_sha256,
};
use medousa_install_support::{MODEL_STORE, builtin_catalog, read_install_manifest};

pub fn install_manifest_path(install_root: &Path) -> PathBuf {
    install_root.join("install-manifest.json")
}

pub async fn fetch_release_manifest() -> Result<ReleaseManifest, String> {
    medousa_install_support::fetch_release_manifest().await
}

pub fn resolve_release_package<'a>(
    manifest: &'a ReleaseManifest,
    package_id: &str,
) -> Result<&'a ReleasePackage, String> {
    medousa_install_support::resolve_release_package(manifest, package_id)
}

pub async fn install_package(
    install_root: &Path,
    data_dir: &Path,
    package_id: &str,
    progress: impl FnMut(f32, &str),
) -> Result<Option<ReleasePackage>, String> {
    if is_model_pack(package_id) {
        install_model_pack(data_dir, package_id, progress).await?;
        return Ok(None);
    }

    if is_desktop_package(package_id) {
        let manifest = fetch_release_manifest().await?;
        let package = resolve_release_package(&manifest, package_id)?.clone();
        install_desktop_package(install_root, &package, progress).await?;
        return Ok(Some(package));
    }

    if !is_tarball_package(package_id) {
        return Err(format!("unsupported package type: {package_id}"));
    }

    install_tarball_package(data_dir, package_id, Some(install_root), progress)
        .await
        .map(Some)
}

pub async fn remove_package(data_dir: &Path, package_id: &str) -> Result<(), String> {
    let entry =
        catalog_entry(package_id).ok_or_else(|| format!("unknown package: {package_id}"))?;
    if !entry.optional {
        return Err(format!("cannot remove required package: {package_id}"));
    }
    remove_tarball_package(data_dir, package_id)
}

async fn install_desktop_package(
    install_root: &Path,
    package: &ReleasePackage,
    mut progress: impl FnMut(f32, &str),
) -> Result<(), String> {
    progress(5.0, "Downloading desktop app…");
    let bytes = download_url(&package.url, |pct| {
        progress(5.0 + pct * 0.6, "Downloading…")
    })
    .await?;
    progress(68.0, "Verifying SHA256…");
    verify_sha256(&bytes, &package.sha256)?;

    let url_lower = package.url.to_lowercase();
    let tmp = std::env::temp_dir().join(format!(
        "medousa-desktop-{}",
        chrono::Utc::now().timestamp_millis()
    ));
    fs::create_dir_all(&tmp).map_err(|err| err.to_string())?;

    if url_lower.ends_with(".dmg") {
        #[cfg(target_os = "macos")]
        {
            let dmg_path = tmp.join("Medousa.dmg");
            fs::write(&dmg_path, &bytes).map_err(|err| err.to_string())?;
            progress(75.0, "Mounting disk image…");
            install_desktop_from_dmg(&dmg_path, install_root)?;
        }
        #[cfg(not(target_os = "macos"))]
        {
            return Err("DMG install is only supported on macOS".to_string());
        }
    } else if url_lower.ends_with(".msi") {
        #[cfg(target_os = "windows")]
        {
            let msi_path = tmp.join("Medousa.msi");
            fs::write(&msi_path, &bytes).map_err(|err| err.to_string())?;
            progress(75.0, "Running installer…");
            install_desktop_from_msi(&msi_path)?;
        }
        #[cfg(not(target_os = "windows"))]
        {
            return Err("MSI install is only supported on Windows".to_string());
        }
    } else if url_lower.ends_with(".exe") {
        #[cfg(target_os = "windows")]
        {
            let exe_path = tmp.join("Medousa-setup.exe");
            fs::write(&exe_path, &bytes).map_err(|err| err.to_string())?;
            progress(75.0, "Running installer…");
            let status = Command::new(&exe_path)
                .args(["/passive", "/norestart"])
                .status()
                .map_err(|err| err.to_string())?;
            if !status.success() {
                return Err(format!("desktop installer failed: {status}"));
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            return Err("EXE install is only supported on Windows".to_string());
        }
    } else if url_lower.ends_with(".appimage") {
        let appimage_path = tmp.join("Medousa.AppImage");
        fs::write(&appimage_path, &bytes).map_err(|err| err.to_string())?;
        progress(75.0, "Installing AppImage…");
        install_desktop_appimage(&appimage_path, install_root)?;
    } else if url_lower.ends_with(".deb") {
        let deb_path = tmp.join("Medousa.deb");
        fs::write(&deb_path, &bytes).map_err(|err| err.to_string())?;
        progress(75.0, "Installing package…");
        install_desktop_deb(&deb_path)?;
    } else {
        return Err(format!(
            "unsupported desktop package format: {}",
            package.url
        ));
    }

    let _ = fs::remove_dir_all(&tmp);
    progress(100.0, "Desktop installed");
    Ok(())
}

#[cfg(target_os = "macos")]
fn install_desktop_from_dmg(dmg_path: &Path, install_root: &Path) -> Result<(), String> {
    let mount_point = std::env::temp_dir().join(format!(
        "medousa-dmg-{}",
        chrono::Utc::now().timestamp_millis()
    ));
    fs::create_dir_all(&mount_point).map_err(|err| err.to_string())?;

    let attach = Command::new("hdiutil")
        .args([
            "attach",
            dmg_path.to_str().unwrap_or_default(),
            "-nobrowse",
            "-mountpoint",
            mount_point.to_str().unwrap_or_default(),
        ])
        .output()
        .map_err(|err| err.to_string())?;
    if !attach.status.success() {
        return Err(format!(
            "hdiutil attach failed: {}",
            String::from_utf8_lossy(&attach.stderr)
        ));
    }

    let result = (|| {
        let app_src = fs::read_dir(&mount_point)
            .map_err(|err| err.to_string())?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .find(|path| path.extension().is_some_and(|ext| ext == "app"))
            .ok_or_else(|| "no .app found in DMG".to_string())?;

        let dest_parent = install_root
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("/Applications"));
        fs::create_dir_all(&dest_parent).map_err(|err| err.to_string())?;
        let dest = dest_parent.join(
            install_root
                .file_name()
                .unwrap_or_else(|| std::ffi::OsStr::new("Medousa.app")),
        );
        replace_app_bundle(&app_src, &dest)
    })();
    let _ = Command::new("hdiutil")
        .args(["detach", mount_point.to_str().unwrap_or_default()])
        .status();
    result
}

#[cfg(any(test, target_os = "macos"))]
fn replace_app_bundle(source: &Path, destination: &Path) -> Result<(), String> {
    let parent = destination
        .parent()
        .ok_or_else(|| "app destination has no parent".to_string())?;
    fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    let name = destination
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    let backup = parent.join(format!(".{name}.medousa-backup"));
    if !destination.exists() && backup.exists() {
        fs::rename(&backup, destination).map_err(|err| err.to_string())?;
    }
    let stage = parent.join(format!(
        ".{name}.medousa-stage-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    if let Err(error) = copy_dir_recursive(source, &stage) {
        let _ = fs::remove_dir_all(&stage);
        return Err(error);
    }
    let had_old = destination.exists();
    if had_old {
        if backup.exists() {
            fs::remove_dir_all(&backup).map_err(|err| err.to_string())?;
        }
        if let Err(error) = fs::rename(destination, &backup) {
            let _ = fs::remove_dir_all(&stage);
            return Err(error.to_string());
        }
    }
    if let Err(error) = fs::rename(&stage, destination) {
        if had_old {
            let _ = fs::rename(&backup, destination);
        }
        let _ = fs::remove_dir_all(&stage);
        return Err(error.to_string());
    }
    if had_old {
        let _ = fs::remove_dir_all(&backup);
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn install_desktop_from_msi(msi_path: &Path) -> Result<(), String> {
    let status = Command::new("msiexec")
        .args([
            "/i",
            msi_path.to_str().unwrap_or_default(),
            "/passive",
            "/norestart",
        ])
        .status()
        .map_err(|err| err.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("msiexec failed: {status}"))
    }
}

fn install_desktop_appimage(appimage_path: &Path, install_root: &Path) -> Result<(), String> {
    let dest = if install_root.extension().is_some_and(|ext| ext == "app") {
        install_root.to_path_buf()
    } else {
        install_root.join("Medousa.AppImage")
    };
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let stage = dest.with_extension(format!(
        "AppImage.medousa-stage-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let result = (|| {
        fs::copy(appimage_path, &stage).map_err(|err| err.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&stage)
                .map_err(|err| err.to_string())?
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&stage, perms).map_err(|err| err.to_string())?;
        }
        fs::File::open(&stage)
            .and_then(|file| file.sync_all())
            .map_err(|err| err.to_string())?;
        fs::rename(&stage, &dest).map_err(|err| err.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(stage);
    }
    result
}

fn install_desktop_deb(deb_path: &Path) -> Result<(), String> {
    let status = Command::new("dpkg")
        .args(["-i", deb_path.to_str().unwrap_or_default()])
        .status()
        .map_err(|err| err.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("dpkg install failed: {status}"))
    }
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<(), String> {
    fs::create_dir_all(dest).map_err(|err| err.to_string())?;
    for entry in fs::read_dir(src).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        let target = dest.join(entry.file_name());
        if entry.file_type().map_err(|err| err.to_string())?.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target).map_err(|err| err.to_string())?;
        }
    }
    Ok(())
}

async fn install_model_pack(
    data_dir: &Path,
    package_id: &str,
    mut progress: impl FnMut(f32, &str),
) -> Result<(), String> {
    let model_id = package_id.strip_prefix("model-").unwrap_or(package_id);
    let catalog = builtin_catalog();
    let entry = catalog
        .models
        .iter()
        .find(|entry| {
            entry.id.replace('-', "") == model_id.replace('-', "") || entry.id == model_id
        })
        .or_else(|| {
            catalog
                .models
                .iter()
                .find(|entry| entry.id.contains(model_id))
        })
        .cloned()
        .ok_or_else(|| format!("unknown model pack: {package_id}"))?;

    progress(10.0, "Queuing model download…");
    std::env::set_var("MEDOUSA_DATA_DIR", data_dir);
    let job = MODEL_STORE
        .start_download(entry)
        .await
        .map_err(|err| err.to_string())?;
    let job_id = job.job_id.clone();

    loop {
        if let Some(current) = MODEL_STORE.get_job_progress(&job_id).await {
            progress(current.percent.max(10.0), &current.message);
            if current.phase == "ready" {
                mark_package_installed(data_dir, package_id)?;
                return Ok(());
            }
            if current.phase == "failed" {
                return Err(current
                    .error
                    .unwrap_or_else(|| "model download failed".to_string()));
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(750)).await;
    }
}

pub fn read_local_install_manifest(
    install_root: &Path,
) -> Option<medousa_install_support::InstallManifest> {
    let path = install_manifest_path(install_root);
    read_install_manifest(&path).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_or_interrupted_app_copy_preserves_previous_bundle() {
        let root = std::env::temp_dir().join(format!(
            "medousa-app-replace-test-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let live = root.join("Medousa.app");
        fs::create_dir_all(&live).unwrap();
        fs::write(live.join("old"), b"working app").unwrap();
        assert!(replace_app_bundle(&root.join("missing.app"), &live).is_err());
        assert_eq!(fs::read(live.join("old")).unwrap(), b"working app");

        let backup = root.join(".Medousa.app.medousa-backup");
        fs::rename(&live, &backup).unwrap();
        assert!(replace_app_bundle(&root.join("missing.app"), &live).is_err());
        assert_eq!(fs::read(live.join("old")).unwrap(), b"working app");
        fs::remove_dir_all(root).unwrap();
    }
}
