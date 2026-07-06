use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use flate2::read::GzDecoder;
use medousa_install_support::manifest::{
    mark_package_installed, shared_bin_dir, unmark_package_installed, ReleaseManifest,
    ReleasePackage,
};
use medousa_install_support::packages::{
    catalog_entry, is_desktop_package, is_model_pack, is_tarball_package,
};
use medousa_install_support::release_config::release_manifest_url;
use medousa_install_support::{builtin_catalog, read_install_manifest, MODEL_STORE};
use reqwest::Client;
use sha2::{Digest, Sha256};
use tar::Archive;

pub fn install_manifest_path(install_root: &Path) -> PathBuf {
    install_root.join("install-manifest.json")
}

pub async fn fetch_release_manifest() -> Result<ReleaseManifest, String> {
    let url = release_manifest_url();
    let client = Client::builder()
        .build()
        .map_err(|err| err.to_string())?;
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(format!("failed to fetch release manifest from {url}: {}", response.status()));
    }
    let bytes = response.bytes().await.map_err(|err| err.to_string())?;
    serde_json::from_slice(&bytes).map_err(|err| err.to_string())
}

pub fn resolve_release_package<'a>(
    manifest: &'a ReleaseManifest,
    package_id: &str,
) -> Result<&'a ReleasePackage, String> {
    let target = std::env::var("MEDOUSA_INSTALL_TARGET")
        .unwrap_or_else(|_| medousa_install_support::host_target());
    let platform = medousa_install_support::host_platform_key();
    let keys = [
        format!("{package_id}-{target}"),
        format!("{package_id}-{platform}"),
        package_id.to_string(),
    ];
    for key in &keys {
        if let Some(pkg) = manifest.packages.get(key) {
            return Ok(pkg);
        }
    }
    manifest
        .packages
        .values()
        .find(|entry| entry.id == package_id)
        .ok_or_else(|| format!("release package not found for {package_id} ({target})"))
}

pub async fn install_package(
    install_root: &Path,
    data_dir: &Path,
    package_id: &str,
    mut progress: impl FnMut(f32, &str),
) -> Result<(), String> {
    if is_model_pack(package_id) {
        return install_model_pack(data_dir, package_id, progress).await;
    }

    if is_desktop_package(package_id) {
        let manifest = fetch_release_manifest().await?;
        let package = resolve_release_package(&manifest, package_id)?.clone();
        return install_desktop_package(install_root, &package, progress).await;
    }

    if !is_tarball_package(package_id) {
        return Err(format!("unsupported package type: {package_id}"));
    }

    let manifest = fetch_release_manifest().await?;
    let package = resolve_release_package(&manifest, package_id)?.clone();

    progress(5.0, "Downloading package…");
    let bytes = download_url(&package.url, |pct| progress(5.0 + pct * 0.65, "Downloading…")).await?;
    progress(72.0, "Verifying SHA256…");
    verify_sha256(&bytes, &package.sha256)?;
    progress(78.0, "Extracting…");

    let packages_dir = data_dir.join("packages").join(package_id);
    fs::create_dir_all(&packages_dir).map_err(|err| err.to_string())?;
    let archive_path = packages_dir.join("package.tar.gz");
    fs::write(&archive_path, &bytes).map_err(|err| err.to_string())?;
    extract_tar_gz(&bytes, &packages_dir)?;

    install_tarball_binaries(&packages_dir, data_dir, package_id)?;
    sync_binaries_to_app_bundle(&packages_dir, install_root, package_id)?;

    if package_id == "local-brain" {
        install_local_brain_sidecar(&packages_dir, install_root)?;
    }

    mark_package_installed(data_dir, package_id)?;
    progress(100.0, "Done");
    Ok(())
}

pub async fn remove_package(
    data_dir: &Path,
    package_id: &str,
) -> Result<(), String> {
    let entry = catalog_entry(package_id)
        .ok_or_else(|| format!("unknown package: {package_id}"))?;
    if !entry.optional {
        return Err(format!("cannot remove required package: {package_id}"));
    }

    let bin_dir = shared_bin_dir(data_dir);
    for bin in entry.binaries {
        let path = bin_dir.join(binary_filename(bin));
        if path.exists() {
            fs::remove_file(&path).map_err(|err| err.to_string())?;
        }
    }

    let packages_dir = data_dir.join("packages").join(package_id);
    if packages_dir.exists() {
        fs::remove_dir_all(&packages_dir).map_err(|err| err.to_string())?;
    }
    unmark_package_installed(data_dir, package_id)?;
    Ok(())
}

fn binary_filename(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

fn install_tarball_binaries(
    packages_dir: &Path,
    data_dir: &Path,
    package_id: &str,
) -> Result<(), String> {
    let source_bin = find_extracted_bin_dir(packages_dir)?;
    let dest_bin = shared_bin_dir(data_dir);
    fs::create_dir_all(&dest_bin).map_err(|err| err.to_string())?;

    let binaries: Vec<&str> = catalog_entry(package_id)
        .map(|entry| entry.binaries.to_vec())
        .unwrap_or_default();

    if binaries.is_empty() {
        if let Ok(entries) = fs::read_dir(&source_bin) {
            for entry in entries.flatten() {
                if entry.file_type().map_err(|err| err.to_string())?.is_file() {
                    copy_binary(entry.path(), &dest_bin.join(entry.file_name()))?;
                }
            }
        }
        return Ok(());
    }

    for bin in binaries {
        let file = binary_filename(bin);
        let src = source_bin.join(&file);
        if !src.is_file() {
            return Err(format!("missing binary {file} in package {package_id}"));
        }
        copy_binary(src, &dest_bin.join(&file))?;
    }
    Ok(())
}

fn sync_binaries_to_app_bundle(
    packages_dir: &Path,
    install_root: &Path,
    package_id: &str,
) -> Result<(), String> {
    if package_id != "engine" && package_id != "local-brain" {
        return Ok(());
    }
    let source_bin = find_extracted_bin_dir(packages_dir)?;
    let dest_dir = app_bundle_macos_dir(install_root);
    if !dest_dir.exists() {
        return Ok(());
    }
    if let Ok(entries) = fs::read_dir(&source_bin) {
        for entry in entries.flatten() {
            if entry.file_type().map_err(|err| err.to_string())?.is_file() {
                copy_binary(entry.path(), &dest_dir.join(entry.file_name()))?;
            }
        }
    }
    Ok(())
}

fn app_bundle_macos_dir(install_root: &Path) -> PathBuf {
    if install_root.extension().is_some_and(|ext| ext == "app") {
        install_root.join("Contents/MacOS")
    } else {
        install_root.join("bin")
    }
}

fn find_extracted_bin_dir(packages_dir: &Path) -> Result<PathBuf, String> {
    let direct = packages_dir.join("bin");
    if direct.is_dir() {
        return Ok(direct);
    }
    if let Ok(entries) = fs::read_dir(packages_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let nested = path.join("bin");
                if nested.is_dir() {
                    return Ok(nested);
                }
            }
        }
    }
    Err("could not find bin/ in extracted package".to_string())
}

fn copy_binary(src: impl AsRef<Path>, dest: &Path) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    fs::copy(src.as_ref(), dest).map_err(|err| err.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(dest).map_err(|err| err.to_string())?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(dest, perms).map_err(|err| err.to_string())?;
    }
    Ok(())
}

async fn install_desktop_package(
    install_root: &Path,
    package: &ReleasePackage,
    mut progress: impl FnMut(f32, &str),
) -> Result<(), String> {
    progress(5.0, "Downloading desktop app…");
    let bytes = download_url(&package.url, |pct| progress(5.0 + pct * 0.6, "Downloading…")).await?;
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
        return Err(format!("unsupported desktop package format: {}", package.url));
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
    if dest.exists() {
        fs::remove_dir_all(&dest).map_err(|err| err.to_string())?;
    }
    copy_dir_recursive(&app_src, &dest)?;

    let _ = Command::new("hdiutil")
        .args(["detach", mount_point.to_str().unwrap_or_default()])
        .status();
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
    fs::copy(appimage_path, &dest).map_err(|err| err.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&dest).map_err(|err| err.to_string())?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&dest, perms).map_err(|err| err.to_string())?;
    }
    Ok(())
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
        .find(|entry| entry.id.replace('-', "") == model_id.replace('-', "") || entry.id == model_id)
        .or_else(|| catalog.models.iter().find(|entry| entry.id.contains(model_id)))
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

fn install_local_brain_sidecar(packages_dir: &Path, install_root: &Path) -> Result<(), String> {
    let bin_dir = find_extracted_bin_dir(packages_dir)?;
    let dest_dir = app_bundle_macos_dir(install_root);
    if !dest_dir.exists() {
        return Ok(());
    }
    fs::create_dir_all(&dest_dir).map_err(|err| err.to_string())?;
    for entry in fs::read_dir(&bin_dir).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        if entry.file_type().map_err(|err| err.to_string())?.is_file() {
            copy_binary(entry.path(), &dest_dir.join(entry.file_name()))?;
        }
    }
    Ok(())
}

async fn download_url(url: &str, mut progress: impl FnMut(f32)) -> Result<Vec<u8>, String> {
    let client = Client::builder()
        .build()
        .map_err(|err| err.to_string())?;
    let response = client.get(url).send().await.map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(format!("download failed: {}", response.status()));
    }
    let total = response.content_length().unwrap_or(0);
    let mut reader = response.bytes_stream();
    use futures_util::StreamExt;
    let mut out = Vec::new();
    let mut downloaded = 0u64;
    while let Some(chunk) = reader.next().await {
        let chunk = chunk.map_err(|err| err.to_string())?;
        downloaded += chunk.len() as u64;
        out.extend_from_slice(&chunk);
        if total > 0 {
            progress((downloaded as f32 / total as f32) * 100.0);
        }
    }
    Ok(out)
}

pub fn verify_sha256(bytes: &[u8], expected: &str) -> Result<(), String> {
    if expected.trim().is_empty() {
        return Ok(());
    }
    let digest = Sha256::digest(bytes);
    let actual = format!("{:x}", digest);
    if actual.eq_ignore_ascii_case(expected.trim()) {
        Ok(())
    } else {
        Err("SHA256 mismatch — download may be corrupt".to_string())
    }
}

fn extract_tar_gz(bytes: &[u8], dest: &Path) -> Result<(), String> {
    let decoder = GzDecoder::new(bytes);
    let mut archive = Archive::new(decoder);
    archive.unpack(dest).map_err(|err| err.to_string())
}

pub fn read_local_install_manifest(install_root: &Path) -> Option<medousa_install_support::InstallManifest> {
    let path = install_manifest_path(install_root);
    read_install_manifest(&path).ok()
}
