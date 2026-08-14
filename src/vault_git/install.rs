//! Platform helpers to obtain a Git binary when missing.

use std::path::PathBuf;
#[cfg(any(windows, test))]
use std::io::Cursor;

use anyhow::{Result, bail};
#[cfg(any(windows, test))]
use anyhow::Context;
#[cfg(windows)]
use medousa_install_support::shared_bin_dir;

#[cfg(windows)]
use crate::paths::medousa_data_dir;

use super::service::resolve_git_binary;

#[cfg(windows)]
const MAX_MINGIT_ARCHIVE_BYTES: u64 = 128 * 1024 * 1024;
#[cfg(any(windows, test))]
const MAX_MINGIT_EXTRACTED_BYTES: u64 = 512 * 1024 * 1024;
#[cfg(any(windows, test))]
const MAX_MINGIT_ENTRIES: usize = 100_000;

pub struct GitInstallProgress {
    pub phase: String,
    pub percent: f32,
}

/// Install portable Git when possible. macOS/Linux return guidance errors.
pub async fn install_portable_git(
    mut progress: impl FnMut(GitInstallProgress),
) -> Result<PathBuf> {
    if let Some(existing) = resolve_git_binary() {
        return Ok(existing);
    }

    if cfg!(windows) {
        return install_mingit_windows(&mut progress).await;
    }

    if cfg!(target_os = "macos") {
        let _ = std::process::Command::new("xcode-select")
            .arg("--install")
            .status();
        bail!(
            "Git is not installed. A Command Line Tools prompt may have opened — \
             finish that install, then try Versions again. Or install Git from https://git-scm.com/download/mac"
        );
    }

    bail!(
        "Git is not installed. Install it with your package manager \
         (e.g. sudo apt install git), then enable Versions again."
    );
}

#[cfg(not(windows))]
async fn install_mingit_windows(
    _progress: &mut impl FnMut(GitInstallProgress),
) -> Result<PathBuf> {
    bail!("portable Git download is only available on Windows")
}

#[cfg(windows)]
async fn install_mingit_windows(
    progress: &mut impl FnMut(GitInstallProgress),
) -> Result<PathBuf> {
    if !cfg!(windows) {
        bail!("portable Git download is only available on Windows");
    }

    progress(GitInstallProgress {
        phase: "Downloading portable Git…".into(),
        percent: 5.0,
    });
    let url = "https://github.com/git-for-windows/git/releases/download/v2.47.1.windows.1/MinGit-2.47.1-64-bit.zip";
    let client = reqwest::Client::builder()
        .build()
        .context("build HTTP client")?;
    let response = client
        .get(url)
        .send()
        .await
        .context("download MinGit")?;
    if !response.status().is_success() {
        bail!("download failed: {}", response.status());
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_MINGIT_ARCHIVE_BYTES)
    {
        bail!("MinGit archive exceeds the 128 MiB download limit");
    }
    let bytes = response.bytes().await.context("read MinGit bytes")?;
    if bytes.len() as u64 > MAX_MINGIT_ARCHIVE_BYTES {
        bail!("MinGit archive exceeds the 128 MiB download limit");
    }
    progress(GitInstallProgress {
        phase: "Extracting Git…".into(),
        percent: 70.0,
    });

    let bin_dir = shared_bin_dir(&medousa_data_dir());
    std::fs::create_dir_all(&bin_dir)?;
    let extract_dir = bin_dir.join("mingit");
    if extract_dir.exists() {
        std::fs::remove_dir_all(&extract_dir)?;
    }
    std::fs::create_dir_all(&extract_dir)?;

    extract_zip(&bytes, &extract_dir)?;

    let candidates = [
        extract_dir.join("cmd").join("git.exe"),
        extract_dir.join("mingw64").join("bin").join("git.exe"),
        extract_dir.join("bin").join("git.exe"),
    ];
    for candidate in &candidates {
        if candidate.is_file() {
            progress(GitInstallProgress {
                phase: "Done".into(),
                percent: 100.0,
            });
            return Ok(candidate.clone());
        }
    }
    if let Ok(entries) = std::fs::read_dir(&extract_dir) {
        for entry in entries.flatten() {
            let cmd = entry.path().join("cmd").join("git.exe");
            if cmd.is_file() {
                progress(GitInstallProgress {
                    phase: "Done".into(),
                    percent: 100.0,
                });
                return Ok(cmd);
            }
        }
    }
    bail!(
        "MinGit extracted but git.exe was not found under {}",
        extract_dir.display()
    );
}

#[cfg(any(windows, test))]
fn extract_zip(bytes: &[u8], extract_dir: &std::path::Path) -> Result<()> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).context("open MinGit zip")?;
    if archive.len() > MAX_MINGIT_ENTRIES {
        bail!("MinGit archive contains too many entries");
    }
    let mut extracted_bytes = 0_u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        if entry.enclosed_name().is_none() {
            bail!("MinGit archive contains an invalid path");
        }
        extracted_bytes = extracted_bytes
            .checked_add(entry.size())
            .ok_or_else(|| anyhow::anyhow!("MinGit extracted size overflow"))?;
        if extracted_bytes > MAX_MINGIT_EXTRACTED_BYTES {
            bail!("MinGit archive exceeds the 512 MiB extraction limit");
        }
    }
    archive
        .extract(extract_dir)
        .context("extract MinGit zip")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;
    use zip::write::SimpleFileOptions;

    #[test]
    fn in_process_zip_extraction_rejects_the_whole_archive_on_escape() {
        let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
        archive
            .start_file("cmd/git.exe", SimpleFileOptions::default())
            .unwrap();
        archive.write_all(b"git").unwrap();
        archive
            .start_file("../outside.txt", SimpleFileOptions::default())
            .unwrap();
        archive.write_all(b"escape").unwrap();
        let bytes = archive.finish().unwrap().into_inner();
        let temp = tempfile::tempdir().unwrap();
        let extract_dir = temp.path().join("extract");

        assert!(extract_zip(&bytes, &extract_dir).is_err());

        assert!(!temp.path().join("outside.txt").exists());
        assert!(!extract_dir.join("cmd/git.exe").exists());
    }

    #[test]
    fn in_process_zip_extraction_unpacks_valid_entries() {
        let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
        archive
            .start_file("cmd/git.exe", SimpleFileOptions::default())
            .unwrap();
        archive.write_all(b"git").unwrap();
        let bytes = archive.finish().unwrap().into_inner();
        let temp = tempfile::tempdir().unwrap();
        let extract_dir = temp.path().join("extract");

        extract_zip(&bytes, &extract_dir).unwrap();

        assert_eq!(
            std::fs::read(extract_dir.join("cmd/git.exe")).unwrap(),
            b"git"
        );
    }
}
