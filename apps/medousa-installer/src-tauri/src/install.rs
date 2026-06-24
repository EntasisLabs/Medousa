use std::fs;
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;
use medousa::install::manifest::{mark_package_installed, read_release_manifest, ReleaseManifest};
use medousa::install::packages::is_model_pack;
use medousa::local_inference::{builtin_catalog, MODEL_STORE};
use reqwest::Client;
use sha2::{Digest, Sha256};
use tar::Archive;

const DEFAULT_RELEASE_MANIFEST_URL: &str =
    "https://github.com/EntasisLabs/Medousa/releases/latest/download/release-manifest.json";

pub fn install_manifest_path(install_root: &Path) -> PathBuf {
    install_root.join("install-manifest.json")
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

    let manifest = fetch_release_manifest().await?;
    let target = std::env::var("MEDOUSA_INSTALL_TARGET").unwrap_or_else(|_| host_target());
    let key = format!("{package_id}-{target}");
    let package = manifest
        .packages
        .get(&key)
        .cloned()
        .or_else(|| {
            manifest
                .packages
                .values()
                .find(|entry| entry.id == package_id)
                .cloned()
        })
        .ok_or_else(|| format!("release package not found for {package_id} ({target})"))?;

    progress(5.0, "Downloading package…");
    let bytes = download_url(&package.url, |pct| progress(pct * 0.7, "Downloading…")).await?;
    progress(75.0, "Verifying SHA256…");
    verify_sha256(&bytes, &package.sha256)?;
    progress(85.0, "Extracting…");

    let packages_dir = data_dir.join("packages").join(package_id);
    fs::create_dir_all(&packages_dir).map_err(|err| err.to_string())?;
    let archive_path = packages_dir.join("package.tar.gz");
    fs::write(&archive_path, &bytes).map_err(|err| err.to_string())?;
    extract_tar_gz(&bytes, &packages_dir)?;

    if package_id == "local-brain" {
        install_local_brain_sidecar(&packages_dir, install_root)?;
    }

    mark_package_installed(data_dir, package_id)?;
    let _ = install_root;
    progress(100.0, "Done");
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
    let bin_dir = packages_dir.join("bin");
    if !bin_dir.is_dir() {
        return Ok(());
    }
    let dest_dir = if install_root.extension().is_some_and(|ext| ext == "app") {
        install_root.join("Contents/MacOS")
    } else {
        install_root.join("bin")
    };
    fs::create_dir_all(&dest_dir).map_err(|err| err.to_string())?;
    for entry in fs::read_dir(&bin_dir).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        if entry.file_type().map_err(|err| err.to_string())?.is_file() {
            let dest = dest_dir.join(entry.file_name());
            fs::copy(entry.path(), &dest).map_err(|err| err.to_string())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = fs::metadata(&dest).map_err(|err| err.to_string())?.permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&dest, perms).map_err(|err| err.to_string())?;
            }
        }
    }
    Ok(())
}

async fn fetch_release_manifest() -> Result<ReleaseManifest, String> {
    let url = std::env::var("MEDOUSA_RELEASE_MANIFEST_URL")
        .unwrap_or_else(|_| DEFAULT_RELEASE_MANIFEST_URL.to_string());
    let client = Client::builder()
        .build()
        .map_err(|err| err.to_string())?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(format!("failed to fetch release manifest: {}", response.status()));
    }
    let bytes = response.bytes().await.map_err(|err| err.to_string())?;
    serde_json::from_slice(&bytes).map_err(|err| err.to_string())
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

fn verify_sha256(bytes: &[u8], expected: &str) -> Result<(), String> {
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

fn host_target() -> String {
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
