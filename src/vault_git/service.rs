//! Git subprocess helpers for vault Versions.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, bail, Context, Result};
use medousa_host::find_command_in_path;
use medousa_install_support::shared_bin_dir;
use serde::{Deserialize, Serialize};

use crate::paths::medousa_data_dir;
use crate::vault::roots::active_vault_root;

pub fn vault_git_enabled() -> bool {
    crate::session::load_tui_defaults()
        .vault_git_enabled
        .unwrap_or(false)
}

pub fn ensure_enabled() -> Result<()> {
    if vault_git_enabled() {
        Ok(())
    } else {
        bail!("Versions is off — enable it in Settings → Versions")
    }
}

fn platform_git_name() -> &'static str {
    if cfg!(windows) {
        "git.exe"
    } else {
        "git"
    }
}

pub fn resolve_git_binary() -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var("GIT_BIN") {
        let path = PathBuf::from(explicit.trim());
        if path.is_file() {
            return Some(path);
        }
    }
    let shared = shared_bin_dir(&medousa_data_dir()).join(platform_git_name());
    if shared.is_file() {
        return Some(shared);
    }
    // Windows MinGit often lives under bin/mingw64/bin/git.exe after extract
    let mingw = shared_bin_dir(&medousa_data_dir())
        .join("mingw64")
        .join("bin")
        .join(platform_git_name());
    if mingw.is_file() {
        return Some(mingw);
    }
    let cmd_git = shared_bin_dir(&medousa_data_dir())
        .join("cmd")
        .join(platform_git_name());
    if cmd_git.is_file() {
        return Some(cmd_git);
    }
    find_command_in_path(platform_git_name())
}

fn run_git(git: &Path, cwd: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new(git)
        .args(args)
        .current_dir(cwd)
        .output()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if !stderr.is_empty() { stderr } else { stdout };
        bail!(
            "git {} failed: {}",
            args.join(" "),
            if detail.is_empty() {
                output.status.to_string()
            } else {
                detail
            }
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitDetectResponse {
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub enabled: bool,
    pub platform_hint: String,
}

pub fn detect_git() -> GitDetectResponse {
    let enabled = vault_git_enabled();
    let platform_hint = if cfg!(windows) {
        "Windows: Medousa can download portable Git into your data folder."
    } else if cfg!(target_os = "macos") {
        "macOS: install Git via Xcode Command Line Tools (xcode-select --install)."
    } else {
        "Linux: install Git with your package manager (e.g. apt install git)."
    }
    .to_string();

    let Some(path) = resolve_git_binary() else {
        return GitDetectResponse {
            available: false,
            path: None,
            version: None,
            enabled,
            platform_hint,
        };
    };
    let version = run_git(&path, &std::env::temp_dir(), &["--version"])
        .ok()
        .map(|s| s.trim().to_string());
    GitDetectResponse {
        available: true,
        path: Some(path.display().to_string()),
        version,
        enabled,
        platform_hint,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatusResponse {
    pub enabled: bool,
    pub available: bool,
    pub is_repo: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    pub dirty_count: usize,
    pub vault_root: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_path: Option<String>,
}

pub fn git_status() -> Result<GitStatusResponse> {
    let vault_root = active_vault_root();
    let enabled = vault_git_enabled();
    let git = resolve_git_binary();
    let available = git.is_some();
    let git_path = git.as_ref().map(|p| p.display().to_string());

    if !enabled || git.is_none() {
        return Ok(GitStatusResponse {
            enabled,
            available,
            is_repo: false,
            branch: None,
            dirty_count: 0,
            vault_root: vault_root.display().to_string(),
            git_path,
        });
    }
    let git = git.expect("checked");
    let is_repo = vault_root.join(".git").exists()
        || run_git(&git, &vault_root, &["rev-parse", "--is-inside-work-tree"])
            .map(|s| s.trim() == "true")
            .unwrap_or(false);

    if !is_repo {
        return Ok(GitStatusResponse {
            enabled,
            available: true,
            is_repo: false,
            branch: None,
            dirty_count: 0,
            vault_root: vault_root.display().to_string(),
            git_path,
        });
    }

    let branch = run_git(&git, &vault_root, &["rev-parse", "--abbrev-ref", "HEAD"])
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s != "HEAD");

    let porcelain = run_git(&git, &vault_root, &["status", "--porcelain"]).unwrap_or_default();
    let dirty_count = porcelain
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count();

    Ok(GitStatusResponse {
        enabled,
        available: true,
        is_repo: true,
        branch,
        dirty_count,
        vault_root: vault_root.display().to_string(),
        git_path,
    })
}

const DEFAULT_GITIGNORE: &str = "\
.trash/
.obsidian/
.DS_Store
*.swp
*~
";

pub fn init_repo() -> Result<GitStatusResponse> {
    ensure_enabled()?;
    let git = resolve_git_binary().ok_or_else(|| anyhow!("Git is not installed"))?;
    let vault_root = active_vault_root();
    std::fs::create_dir_all(&vault_root)?;
    if !vault_root.join(".git").exists() {
        run_git(&git, &vault_root, &["init"])?;
    }
    let ignore = vault_root.join(".gitignore");
    if !ignore.exists() {
        std::fs::write(&ignore, DEFAULT_GITIGNORE)?;
    }
    // Identity for local-only commits if unset
    let _ = run_git(
        &git,
        &vault_root,
        &["config", "user.email", "medousa@localhost"],
    );
    let _ = run_git(&git, &vault_root, &["config", "user.name", "Medousa"]);
    git_status()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitLogEntry {
    pub id: String,
    pub short_id: String,
    pub message: String,
    pub author: String,
    pub committed_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitLogQuery {
    pub path: Option<String>,
    pub limit: Option<usize>,
}

pub fn git_log(query: &GitLogQuery) -> Result<Vec<GitLogEntry>> {
    ensure_enabled()?;
    let git = resolve_git_binary().ok_or_else(|| anyhow!("Git is not installed"))?;
    let vault_root = active_vault_root();
    let limit = query.limit.unwrap_or(40).clamp(1, 200);
    let mut args = vec![
        "log".to_string(),
        format!("-n{limit}"),
        "--pretty=format:%H%x09%h%x09%an%x09%aI%x09%s".to_string(),
    ];
    if let Some(path) = query.path.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        args.push("--".to_string());
        args.push(path.to_string());
    }
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = run_git(&git, &vault_root, &arg_refs)?;
    Ok(out
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.splitn(5, '\t').collect();
            if parts.len() < 5 {
                return None;
            }
            Some(GitLogEntry {
                id: parts[0].to_string(),
                short_id: parts[1].to_string(),
                author: parts[2].to_string(),
                committed_at: parts[3].to_string(),
                message: parts[4].to_string(),
            })
        })
        .collect())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitCommitRequest {
    pub message: String,
    #[serde(default)]
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitCommitResponse {
    pub id: String,
    pub message: String,
}

pub fn commit_version(request: &GitCommitRequest) -> Result<GitCommitResponse> {
    ensure_enabled()?;
    let git = resolve_git_binary().ok_or_else(|| anyhow!("Git is not installed"))?;
    let vault_root = active_vault_root();
    if !vault_root.join(".git").exists() {
        init_repo()?;
    }
    let message = request.message.trim();
    if message.is_empty() {
        bail!("version message is required");
    }
    if request.paths.is_empty() {
        run_git(&git, &vault_root, &["add", "-A"])?;
    } else {
        let mut args = vec!["add".to_string(), "--".to_string()];
        for path in &request.paths {
            let trimmed = path.trim();
            if !trimmed.is_empty() {
                args.push(trimmed.to_string());
            }
        }
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        run_git(&git, &vault_root, &arg_refs)?;
    }
    // Nothing to commit?
    let staged = run_git(&git, &vault_root, &["diff", "--cached", "--name-only"])?;
    if staged.trim().is_empty() {
        bail!("nothing to save — working tree matches the last version");
    }
    run_git(&git, &vault_root, &["commit", "-m", message])?;
    let id = run_git(&git, &vault_root, &["rev-parse", "HEAD"])?
        .trim()
        .to_string();
    Ok(GitCommitResponse {
        id,
        message: message.to_string(),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitRestoreRequest {
    pub commit: String,
    pub path: String,
}

pub fn restore_note(request: &GitRestoreRequest) -> Result<()> {
    ensure_enabled()?;
    let git = resolve_git_binary().ok_or_else(|| anyhow!("Git is not installed"))?;
    let vault_root = active_vault_root();
    let commit = request.commit.trim();
    let path = request.path.trim().trim_start_matches('/');
    if commit.is_empty() || path.is_empty() {
        bail!("commit and path are required");
    }
    run_git(
        &git,
        &vault_root,
        &["checkout", commit, "--", path],
    )?;
    Ok(())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitDiffQuery {
    pub path: Option<String>,
    pub commit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitDiffResponse {
    pub path: String,
    pub patch: String,
}

pub fn diff_note(query: &GitDiffQuery) -> Result<GitDiffResponse> {
    ensure_enabled()?;
    let git = resolve_git_binary().ok_or_else(|| anyhow!("Git is not installed"))?;
    let vault_root = active_vault_root();
    let path = query
        .path
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .ok_or_else(|| anyhow!("path is required"))?;
    let commit = query
        .commit
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .unwrap_or("HEAD");
    let patch = run_git(
        &git,
        &vault_root,
        &["diff", commit, "--", path],
    )?;
    Ok(GitDiffResponse {
        path: path.to_string(),
        patch,
    })
}
