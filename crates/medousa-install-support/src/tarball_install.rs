//! Download, verify, extract, and install tarball packages into `{dataDir}/bin`.

use std::fs;
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;
use futures_util::StreamExt;
use reqwest::Client;
use sha2::{Digest, Sha256};
use tar::Archive;

use crate::manifest::{
    ReleaseManifest, ReleasePackage, mark_package_installed, package_dir, resolve_release_package,
    shared_bin_dir, unmark_package_installed,
};
use crate::packages::{catalog_entry, is_tarball_package};
use crate::release_config::release_manifest_url;

pub async fn fetch_release_manifest() -> Result<ReleaseManifest, String> {
    let url = release_manifest_url();
    let client = Client::builder().build().map_err(|err| err.to_string())?;
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "failed to fetch release manifest from {url}: {}",
            response.status()
        ));
    }
    let bytes = response.bytes().await.map_err(|err| err.to_string())?;
    serde_json::from_slice(&bytes).map_err(|err| err.to_string())
}

/// Install a tarball package into `{data_dir}/packages/{id}` and `{data_dir}/bin`.
///
/// When `install_root` is Some, also sync engine/local-brain binaries into the
/// app bundle / install root `bin` (installer behavior). Home can pass None and
/// rely on `{data_dir}/bin` resolution.
pub async fn install_tarball_package(
    data_dir: &Path,
    package_id: &str,
    install_root: Option<&Path>,
    mut progress: impl FnMut(f32, &str),
) -> Result<ReleasePackage, String> {
    if !is_tarball_package(package_id) {
        return Err(format!("not a tarball package: {package_id}"));
    }

    let manifest = fetch_release_manifest().await?;
    let package = resolve_release_package(&manifest, package_id)?.clone();

    progress(5.0, "Downloading package…");
    let bytes = download_url(&package.url, |pct| {
        progress(5.0 + pct * 0.65, "Downloading…")
    })
    .await?;
    progress(72.0, "Verifying SHA256…");
    verify_sha256(&bytes, &package.sha256)?;
    progress(78.0, "Extracting…");

    let packages_dir = package_dir(data_dir, package_id)?;
    fs::create_dir_all(&packages_dir).map_err(|err| err.to_string())?;
    let archive_path = packages_dir.join("package.tar.gz");
    fs::write(&archive_path, &bytes).map_err(|err| err.to_string())?;
    extract_tar_gz(&bytes, &packages_dir)?;

    install_tarball_binaries(&packages_dir, data_dir, package_id)?;
    if let Some(root) = install_root {
        sync_binaries_to_app_bundle(&packages_dir, root, package_id)?;
        if package_id == "local-brain" {
            install_local_brain_sidecar(&packages_dir, root)?;
        }
    }

    mark_package_installed(data_dir, package_id)?;
    progress(100.0, "Done");
    Ok(package)
}

pub fn remove_tarball_package(data_dir: &Path, package_id: &str) -> Result<(), String> {
    let entry =
        catalog_entry(package_id).ok_or_else(|| format!("unknown package: {package_id}"))?;
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

    let packages_dir = package_dir(data_dir, package_id)?;
    if packages_dir.exists() {
        fs::remove_dir_all(&packages_dir).map_err(|err| err.to_string())?;
    }
    unmark_package_installed(data_dir, package_id)?;
    Ok(())
}

pub fn binary_filename(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

pub fn verify_sha256(bytes: &[u8], expected: &str) -> Result<(), String> {
    if expected.trim().len() != 64 || !expected.trim().bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("SHA256 digest is missing or invalid".to_string());
    }
    let digest = Sha256::digest(bytes);
    let actual = format!("{:x}", digest);
    if actual.eq_ignore_ascii_case(expected.trim()) {
        Ok(())
    } else {
        Err("SHA256 mismatch — download may be corrupt".to_string())
    }
}

pub async fn download_url(url: &str, mut progress: impl FnMut(f32)) -> Result<Vec<u8>, String> {
    let client = Client::builder().build().map_err(|err| err.to_string())?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(format!("download failed: {}", response.status()));
    }
    let total = response.content_length().unwrap_or(0);
    let mut reader = response.bytes_stream();
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

fn extract_tar_gz(bytes: &[u8], dest: &Path) -> Result<(), String> {
    let decoder = GzDecoder::new(bytes);
    let mut archive = Archive::new(decoder);
    archive.unpack(dest).map_err(|err| err.to_string())
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
    copy_binary_with(src.as_ref(), dest, |source, target| {
        fs::copy(source, target)
    })
}

fn copy_binary_with(
    src: &Path,
    dest: &Path,
    copy: impl FnOnce(&Path, &Path) -> std::io::Result<u64>,
) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let staged = dest.with_file_name(format!(
        ".{}.medousa-stage-{}",
        dest.file_name().unwrap_or_default().to_string_lossy(),
        uuid::Uuid::new_v4()
    ));
    let result = (|| {
        copy(src, &staged).map_err(|err| err.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&staged)
                .map_err(|err| err.to_string())?
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&staged, perms).map_err(|err| err.to_string())?;
        }
        fs::File::open(&staged)
            .and_then(|file| file.sync_all())
            .map_err(|err| err.to_string())?;
        #[cfg(windows)]
        {
            let backup = dest.with_extension("medousa-backup");
            if !dest.exists() && backup.exists() {
                fs::rename(&backup, dest).map_err(|err| err.to_string())?;
            }
            if dest.exists() {
                if backup.exists() {
                    fs::remove_file(&backup).map_err(|err| err.to_string())?;
                }
                fs::rename(dest, &backup).map_err(|err| err.to_string())?;
                if let Err(error) = fs::rename(&staged, dest) {
                    let _ = fs::rename(&backup, dest);
                    return Err(error.to_string());
                }
                let _ = fs::remove_file(&backup);
                return Ok(());
            }
        }
        fs::rename(&staged, dest).map_err(|err| err.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&staged);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::verify_sha256;

    #[test]
    fn sha256_verification_fails_closed() {
        let payload = b"medousa package";
        let digest = "432a0a959e33aca0f0d216b2c4aef42530ee3d14b892fcbee4dfc9390ba7db05";
        assert!(verify_sha256(payload, "").is_err());
        assert!(verify_sha256(payload, "not-a-digest").is_err());
        assert!(verify_sha256(payload, &"0".repeat(64)).is_err());
        assert!(verify_sha256(payload, digest).is_ok());
    }

    #[test]
    fn failed_binary_copy_preserves_live_binary() {
        let root = std::env::temp_dir().join(format!("medousa-copy-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let live = root.join("medousa_daemon");
        std::fs::write(&live, b"working binary").unwrap();
        assert!(super::copy_binary(root.join("missing"), &live).is_err());
        assert_eq!(std::fs::read(&live).unwrap(), b"working binary");
        let error = super::copy_binary_with(&live, &live, |_, staged| {
            std::fs::write(staged, b"partial download")?;
            Err(std::io::Error::from_raw_os_error(28))
        });
        assert!(error.is_err());
        assert_eq!(std::fs::read(&live).unwrap(), b"working binary");
        std::fs::remove_dir_all(root).unwrap();
    }
}
