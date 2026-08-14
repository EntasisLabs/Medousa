//! Git subprocess helpers for vault Versions.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};
use medousa_host::{find_command_in_path, hide_subprocess_window};
use medousa_install_support::shared_bin_dir;
use serde::{Deserialize, Serialize};

use crate::paths::medousa_data_dir;
use crate::store_root::StoreRoot;
use crate::vault::path::{VaultPath, user_vault_capability};
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
        bail!("Versions is off — enable it in Settings → Runtime Controls")
    }
}

fn platform_git_name() -> &'static str {
    if cfg!(windows) { "git.exe" } else { "git" }
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

fn run_command(git: &Path, cwd: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let mut command = Command::new(git);
    hide_subprocess_window(&mut command);
    let output = command
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
    Ok(output.stdout)
}

pub(crate) fn run_git(
    git: &Path,
    vault: &StoreRoot,
    ambient_root: &Path,
    args: &[&str],
) -> Result<String> {
    let output = run_git_bytes(git, vault, ambient_root, args)?;
    Ok(String::from_utf8_lossy(&output).into_owned())
}

fn run_git_bytes(
    git: &Path,
    vault: &StoreRoot,
    _ambient_root: &Path,
    args: &[&str],
) -> Result<Vec<u8>> {
    let mut command = Command::new(git);
    hide_subprocess_window(&mut command);
    command.args(args);
    #[cfg(unix)]
    vault.configure_command_current_dir(&mut command);
    #[cfg(not(unix))]
    command.current_dir(_ambient_root);

    let output = command
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
    Ok(output.stdout)
}

fn vault_authority() -> Result<(PathBuf, Arc<StoreRoot>)> {
    let ambient_root = active_vault_root();
    let vault = user_vault_capability()?;
    Ok((ambient_root, vault))
}

fn resolve_commit(git: &Path, vault: &StoreRoot, ambient_root: &Path, raw: &str) -> Result<String> {
    let commit = raw.trim();
    if commit.is_empty() {
        bail!("commit is required");
    }
    let revision = format!("{commit}^{{commit}}");
    Ok(run_git(
        git,
        vault,
        ambient_root,
        &["rev-parse", "--verify", "--end-of-options", &revision],
    )?
    .trim()
    .to_string())
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
    let version = run_command(&path, &std::env::temp_dir(), &["--version"])
        .ok()
        .map(|bytes| String::from_utf8_lossy(&bytes).trim().to_string());
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
    let vault = user_vault_capability()?;
    let git_dir = VaultPath::internal(".git")?;
    let is_repo = vault.is_dir(&git_dir)?
        || run_git(
            &git,
            &vault,
            &vault_root,
            &["rev-parse", "--is-inside-work-tree"],
        )
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

    let branch = run_git(
        &git,
        &vault,
        &vault_root,
        &["rev-parse", "--abbrev-ref", "HEAD"],
    )
    .ok()
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty() && s != "HEAD");

    let porcelain =
        run_git(&git, &vault, &vault_root, &["status", "--porcelain"]).unwrap_or_default();
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
    let (vault_root, vault) = vault_authority()?;
    let git_dir = VaultPath::internal(".git")?;
    if !vault.is_dir(&git_dir)? {
        run_git(&git, &vault, &vault_root, &["init"])?;
    }
    let ignore = VaultPath::internal(".gitignore")?;
    if !vault.is_file(&ignore)? {
        vault.atomic_write(&ignore, DEFAULT_GITIGNORE.as_bytes())?;
    }
    // Identity for local-only commits if unset
    let _ = run_git(
        &git,
        &vault,
        &vault_root,
        &["config", "user.email", "medousa@localhost"],
    );
    let _ = run_git(
        &git,
        &vault,
        &vault_root,
        &["config", "user.name", "Medousa"],
    );
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
    let (vault_root, vault) = vault_authority()?;
    let limit = query.limit.unwrap_or(40).clamp(1, 200);
    let mut args = vec![
        "log".to_string(),
        format!("-n{limit}"),
        "--pretty=format:%H%x09%h%x09%an%x09%aI%x09%s".to_string(),
    ];
    if let Some(path) = query
        .path
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        args.push("--".to_string());
        args.push(VaultPath::parse(path)?.to_string());
    }
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = run_git(&git, &vault, &vault_root, &arg_refs)?;
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
    let (vault_root, vault) = vault_authority()?;
    let git_dir = VaultPath::internal(".git")?;
    if !vault.is_dir(&git_dir)? {
        init_repo()?;
    }
    let message = request.message.trim();
    if message.is_empty() {
        bail!("version message is required");
    }
    if request.paths.is_empty() {
        run_git(&git, &vault, &vault_root, &["add", "-A"])?;
    } else {
        let mut args = vec!["add".to_string(), "--".to_string()];
        for path in &request.paths {
            let trimmed = path.trim();
            if !trimmed.is_empty() {
                args.push(VaultPath::parse(trimmed)?.to_string());
            }
        }
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        run_git(&git, &vault, &vault_root, &arg_refs)?;
    }
    // Nothing to commit?
    let staged = run_git(
        &git,
        &vault,
        &vault_root,
        &["diff", "--cached", "--name-only"],
    )?;
    if staged.trim().is_empty() {
        bail!("nothing to save — working tree matches the last version");
    }
    run_git(&git, &vault, &vault_root, &["commit", "-m", message])?;
    let id = run_git(&git, &vault, &vault_root, &["rev-parse", "HEAD"])?
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
    let (vault_root, vault) = vault_authority()?;
    let commit = resolve_commit(&git, &vault, &vault_root, &request.commit)?;
    let path = VaultPath::parse(&request.path)?;
    restore_blob(&git, &vault, &vault_root, &commit, &path)
}

fn restore_blob(
    git: &Path,
    vault: &StoreRoot,
    ambient_root: &Path,
    commit: &str,
    path: &VaultPath,
) -> Result<()> {
    let object = format!("{commit}:{}", path.as_str());
    let bytes = run_git_bytes(git, vault, ambient_root, &["show", &object])?;
    const MAX_RESTORED_NOTE_BYTES: usize = 8 * 1024 * 1024;
    if bytes.len() > MAX_RESTORED_NOTE_BYTES {
        bail!("restored note exceeds the 8 MiB limit");
    }
    vault.atomic_write(path, &bytes)?;
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
    let (vault_root, vault) = vault_authority()?;
    let path = VaultPath::parse(
        query
            .path
            .as_deref()
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .ok_or_else(|| anyhow!("path is required"))?,
    )?;
    let commit = resolve_commit(
        &git,
        &vault,
        &vault_root,
        query
            .commit
            .as_deref()
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .unwrap_or("HEAD"),
    )?;
    let patch = run_git(
        &git,
        &vault,
        &vault_root,
        &["diff", &commit, "--", path.as_str()],
    )?;
    Ok(GitDiffResponse {
        path: path.to_string(),
        patch,
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn committed_note() -> (tempfile::TempDir, PathBuf, StoreRoot, String, VaultPath) {
        let temp = tempfile::tempdir().unwrap();
        let root_path = temp.path().canonicalize().unwrap().join("vault");
        let vault = StoreRoot::open_or_create_nofollow(&root_path).unwrap();
        let path = VaultPath::parse("notes/proof.md").unwrap();
        vault.atomic_write(&path, b"committed").unwrap();
        let git = Path::new("git");
        run_git(git, &vault, &root_path, &["init"]).unwrap();
        run_git(
            git,
            &vault,
            &root_path,
            &["config", "user.email", "test@localhost"],
        )
        .unwrap();
        run_git(
            git,
            &vault,
            &root_path,
            &["config", "user.name", "Medousa Test"],
        )
        .unwrap();
        run_git(git, &vault, &root_path, &["add", "--", path.as_str()]).unwrap();
        run_git(git, &vault, &root_path, &["commit", "-m", "proof"]).unwrap();
        let commit = run_git(git, &vault, &root_path, &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_string();
        (temp, root_path, vault, commit, path)
    }

    #[test]
    fn restore_writes_through_held_root_after_ambient_replacement() {
        let (_temp, root_path, vault, commit, path) = committed_note();
        let held_path = root_path.with_file_name("held-vault");
        vault.atomic_write(&path, b"newer").unwrap();
        std::fs::rename(&root_path, &held_path).unwrap();
        std::fs::create_dir(&root_path).unwrap();

        restore_blob(Path::new("git"), &vault, &root_path, &commit, &path).unwrap();

        assert_eq!(
            std::fs::read(held_path.join(path.as_str())).unwrap(),
            b"committed"
        );
        assert!(!root_path.join(path.as_str()).exists());
    }

    #[test]
    fn restore_refuses_a_link_ancestor_and_preserves_the_outside_canary() {
        use std::os::unix::fs::symlink;

        let (temp, root_path, vault, commit, path) = committed_note();
        let notes = root_path.join("notes");
        let outside = temp.path().join("outside");
        std::fs::remove_dir_all(&notes).unwrap();
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("proof.md"), b"outside").unwrap();
        symlink(&outside, &notes).unwrap();

        assert!(restore_blob(Path::new("git"), &vault, &root_path, &commit, &path).is_err());
        assert_eq!(std::fs::read(outside.join("proof.md")).unwrap(), b"outside");
    }
}
