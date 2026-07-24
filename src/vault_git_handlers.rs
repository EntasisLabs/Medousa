//! HTTP handlers for vault Versions (`/v1/vault/git/*`).

use axum::extract::Query;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::session::{load_tui_defaults, save_tui_defaults};
use crate::vault_git::{
    commit_version, detect_git, diff_note, git_log, git_status, init_repo, install_portable_git,
    restore_note, GitCommitRequest, GitDiffQuery, GitLogQuery,
};

fn map_err(err: anyhow::Error) -> (StatusCode, String) {
    let message = err.to_string();
    if message.contains("Versions is off")
        || message.contains("required")
        || message.contains("nothing to save")
        || message.contains("not installed")
    {
        (StatusCode::BAD_REQUEST, message)
    } else {
        (StatusCode::INTERNAL_SERVER_ERROR, message)
    }
}

pub async fn vault_git_detect() -> Json<crate::vault_git::service::GitDetectResponse> {
    Json(detect_git())
}

pub async fn vault_git_status(
) -> Result<Json<crate::vault_git::service::GitStatusResponse>, (StatusCode, String)> {
    git_status().map(Json).map_err(map_err)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultGitEnableRequest {
    pub enabled: bool,
    #[serde(default)]
    pub init_if_needed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultGitEnableResponse {
    pub enabled: bool,
    pub status: crate::vault_git::service::GitStatusResponse,
}

pub async fn vault_git_enable(
    Json(body): Json<VaultGitEnableRequest>,
) -> Result<Json<VaultGitEnableResponse>, (StatusCode, String)> {
    let mut defaults = load_tui_defaults();
    defaults.vault_git_enabled = Some(body.enabled);
    save_tui_defaults(&defaults);

    if body.enabled && body.init_if_needed {
        let detect = detect_git();
        if detect.available {
            let status = git_status().map_err(map_err)?;
            if !status.is_repo {
                let _ = init_repo().map_err(map_err)?;
            }
        }
    }

    let status = git_status().map_err(map_err)?;
    Ok(Json(VaultGitEnableResponse {
        enabled: body.enabled,
        status,
    }))
}

pub async fn vault_git_init(
) -> Result<Json<crate::vault_git::service::GitStatusResponse>, (StatusCode, String)> {
    init_repo().map(Json).map_err(map_err)
}

pub async fn vault_git_install(
) -> Result<Json<crate::vault_git::service::GitDetectResponse>, (StatusCode, String)> {
    install_portable_git(|_| {}).await.map_err(map_err)?;
    Ok(Json(detect_git()))
}

pub async fn vault_git_log(
    Query(query): Query<GitLogQuery>,
) -> Result<Json<Vec<crate::vault_git::service::GitLogEntry>>, (StatusCode, String)> {
    git_log(&query).map(Json).map_err(map_err)
}

pub async fn vault_git_commit(
    Json(body): Json<GitCommitRequest>,
) -> Result<Json<crate::vault_git::service::GitCommitResponse>, (StatusCode, String)> {
    commit_version(&body).map(Json).map_err(map_err)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultGitRestoreRequest {
    pub commit: String,
    pub path: String,
}

pub async fn vault_git_restore(
    Json(body): Json<VaultGitRestoreRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    restore_note(&crate::vault_git::service::GitRestoreRequest {
        commit: body.commit,
        path: body.path,
    })
    .map(|_| StatusCode::NO_CONTENT)
    .map_err(map_err)
}

pub async fn vault_git_diff(
    Query(query): Query<GitDiffQuery>,
) -> Result<Json<crate::vault_git::service::GitDiffResponse>, (StatusCode, String)> {
    diff_note(&query).map(Json).map_err(map_err)
}

// --- Advanced: worktrees ---

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeEntry {
    pub path: String,
    pub head: String,
    pub branch: Option<String>,
}

pub async fn vault_git_worktrees_list(
) -> Result<Json<Vec<WorktreeEntry>>, (StatusCode, String)> {
    crate::vault_git::ensure_enabled().map_err(map_err)?;
    let git = crate::vault_git::service::resolve_git_binary()
        .ok_or_else(|| map_err(anyhow::anyhow!("Git is not installed")))?;
    let vault_root = crate::vault::roots::active_vault_root();
    let output = std::process::Command::new(&git)
        .args(["worktree", "list", "--porcelain"])
        .current_dir(&vault_root)
        .output()
        .map_err(|e| map_err(e.into()))?;
    if !output.status.success() {
        return Err(map_err(anyhow::anyhow!(
            "git worktree list failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut entries = Vec::new();
    let mut current_path = None::<String>;
    let mut current_head = None::<String>;
    let mut current_branch = None::<String>;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("worktree ") {
            if let (Some(path), Some(head)) = (current_path.take(), current_head.take()) {
                entries.push(WorktreeEntry {
                    path,
                    head,
                    branch: current_branch.take(),
                });
            }
            current_path = Some(rest.to_string());
        } else if let Some(rest) = line.strip_prefix("HEAD ") {
            current_head = Some(rest.to_string());
        } else if let Some(rest) = line.strip_prefix("branch ") {
            current_branch = Some(rest.trim_start_matches("refs/heads/").to_string());
        } else if line.is_empty() {
            if let (Some(path), Some(head)) = (current_path.take(), current_head.take()) {
                entries.push(WorktreeEntry {
                    path,
                    head,
                    branch: current_branch.take(),
                });
            }
        }
    }
    if let (Some(path), Some(head)) = (current_path, current_head) {
        entries.push(WorktreeEntry {
            path,
            head,
            branch: current_branch,
        });
    }
    Ok(Json(entries))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeAddRequest {
    pub path: String,
    pub branch: Option<String>,
}

pub async fn vault_git_worktrees_add(
    Json(body): Json<WorktreeAddRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    crate::vault_git::ensure_enabled().map_err(map_err)?;
    let git = crate::vault_git::service::resolve_git_binary()
        .ok_or_else(|| map_err(anyhow::anyhow!("Git is not installed")))?;
    let vault_root = crate::vault::roots::active_vault_root();
    let path = body.path.trim();
    if path.is_empty() {
        return Err(map_err(anyhow::anyhow!("path is required")));
    }
    let mut args = vec!["worktree".to_string(), "add".to_string()];
    if let Some(branch) = body.branch.as_deref().map(str::trim).filter(|b| !b.is_empty()) {
        args.push("-b".to_string());
        args.push(branch.to_string());
    }
    args.push(path.to_string());
    let status = std::process::Command::new(&git)
        .args(&args)
        .current_dir(&vault_root)
        .status()
        .map_err(|e| map_err(e.into()))?;
    if !status.success() {
        return Err(map_err(anyhow::anyhow!("git worktree add failed")));
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeRemoveRequest {
    pub path: String,
}

pub async fn vault_git_worktrees_remove(
    Json(body): Json<WorktreeRemoveRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    crate::vault_git::ensure_enabled().map_err(map_err)?;
    let git = crate::vault_git::service::resolve_git_binary()
        .ok_or_else(|| map_err(anyhow::anyhow!("Git is not installed")))?;
    let vault_root = crate::vault::roots::active_vault_root();
    let path = body.path.trim();
    if path.is_empty() {
        return Err(map_err(anyhow::anyhow!("path is required")));
    }
    let status = std::process::Command::new(&git)
        .args(["worktree", "remove", "--force", path])
        .current_dir(&vault_root)
        .status()
        .map_err(|e| map_err(e.into()))?;
    if !status.success() {
        return Err(map_err(anyhow::anyhow!("git worktree remove failed")));
    }
    Ok(StatusCode::NO_CONTENT)
}
