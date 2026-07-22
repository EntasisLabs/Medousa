//! Platform helpers to obtain a Git binary when missing.

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use medousa_install_support::shared_bin_dir;

use crate::paths::medousa_data_dir;

use super::service::resolve_git_binary;

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
    let bytes = response.bytes().await.context("read MinGit bytes")?;
    progress(GitInstallProgress {
        phase: "Extracting Git…".into(),
        percent: 70.0,
    });

    let bin_dir = shared_bin_dir(&medousa_data_dir());
    std::fs::create_dir_all(&bin_dir)?;
    let zip_path = bin_dir.join("mingit-download.zip");
    std::fs::write(&zip_path, &bytes)?;

    let extract_dir = bin_dir.join("mingit");
    if extract_dir.exists() {
        std::fs::remove_dir_all(&extract_dir)?;
    }
    std::fs::create_dir_all(&extract_dir)?;

    #[cfg(not(windows))]
    {
        let _ = (&zip_path, &extract_dir, progress);
        bail!("portable Git download is only available on Windows");
    }

    #[cfg(windows)]
    {
        let expanded = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "Expand-Archive -Path '{}' -DestinationPath '{}' -Force",
                    zip_path.display(),
                    extract_dir.display()
                ),
            ])
            .status()
            .context("expand MinGit zip")?;
        if !expanded.success() {
            bail!("failed to extract MinGit archive");
        }

        let _ = std::fs::remove_file(&zip_path);

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
}
