//! Explicit human Git actions from review. Inputs are tied to a fresh repository snapshot.
use super::*;
use medousa_forge::execution::{ExecutionClass, supervise_command};
use std::collections::BTreeSet;

#[derive(Debug, Serialize)]
pub(super) struct GitState {
    head: String,
    branch: String,
    snapshot: String,
    paths: Vec<String>,
    base: String,
    can_branch: bool,
}
#[derive(Debug, Deserialize)]
pub(super) struct GitAction {
    lease_id: String,
    generation: u64,
    snapshot: String,
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default)]
    message: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    base: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    branch: String,
}

async fn command(
    program: &FsPath,
    root: &FsPath,
    args: Vec<String>,
    env: Vec<(String, String)>,
) -> ApiResult<Vec<u8>> {
    let (out, err, truncated, status) = supervise_command(
        program,
        Some(root.to_owned()),
        args,
        env,
        std::time::Duration::from_secs(120),
        8 * 1024 * 1024,
    )
    .await
    .map_err(map_err)?;
    if truncated {
        return Err(request_error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "Git output exceeds the review limit",
        ));
    }
    if !status.success() {
        return Err(request_error(
            StatusCode::CONFLICT,
            medousa_forge::execution::redact_git_text(&String::from_utf8_lossy(&err)),
        ));
    }
    Ok(out)
}
async fn git(binary: &FsPath, root: &FsPath, args: &[&str]) -> ApiResult<String> {
    let out = command(
        binary,
        root,
        args.iter().map(|v| (*v).to_owned()).collect(),
        vec![],
    )
    .await?;
    Ok(String::from_utf8_lossy(&out).trim().to_owned())
}
fn status_paths(status: &[u8]) -> ApiResult<Vec<String>> {
    let mut paths = BTreeSet::new();
    let mut records = status.split(|b| *b == 0).filter(|r| !r.is_empty());
    while let Some(record) = records.next() {
        if record.len() < 4 {
            return Err(request_error(StatusCode::CONFLICT, "Invalid Git status"));
        }
        let xy = &record[..2];
        if xy.contains(&b'U') || xy == b"AA" || xy == b"DD" {
            return Err(request_error(
                StatusCode::CONFLICT,
                "Resolve conflicts before committing or creating a PR",
            ));
        }
        let path = std::str::from_utf8(&record[3..])
            .map_err(|_| request_error(StatusCode::CONFLICT, "A filename is not UTF-8"))?;
        paths.insert(path.to_owned());
        if xy.contains(&b'R') || xy.contains(&b'C') {
            let old = records
                .next()
                .ok_or_else(|| request_error(StatusCode::CONFLICT, "Invalid rename status"))?;
            paths.insert(
                std::str::from_utf8(old)
                    .map_err(|_| request_error(StatusCode::CONFLICT, "A filename is not UTF-8"))?
                    .to_owned(),
            );
        }
    }
    Ok(paths.into_iter().collect())
}
async fn snapshot(binary: &FsPath, root: &FsPath, base: String) -> ApiResult<GitState> {
    let head = git(binary, root, &["rev-parse", "HEAD"]).await?;
    let branch = git(binary, root, &["symbolic-ref", "--short", "HEAD"]).await?;
    let status = command(
        binary,
        root,
        vec![
            "status".into(),
            "--porcelain=v1".into(),
            "-z".into(),
            "--untracked-files=all".into(),
        ],
        vec![],
    )
    .await?;
    let paths = status_paths(&status)?;
    let mut hash = Sha256::new();
    hash.update(head.as_bytes());
    hash.update(branch.as_bytes());
    hash.update(&status);
    // Include both staging and worktree content; neither may change after approval.
    for args in [
        vec!["diff", "--binary", "HEAD"],
        vec!["diff", "--binary", "--cached"],
    ] {
        hash.update(
            command(
                binary,
                root,
                args.into_iter().map(str::to_owned).collect(),
                vec![],
            )
            .await?,
        );
    }
    let mut total_bytes = 0u64;
    let canonical_root = tokio::fs::canonicalize(root)
        .await
        .map_err(|e| request_error(StatusCode::CONFLICT, e.to_string()))?;
    for path in &paths {
        let relative = FsPath::new(path);
        if relative.is_absolute()
            || relative
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err(request_error(
                StatusCode::BAD_REQUEST,
                "Invalid repository path",
            ));
        }
        // Hash untracked files too, without following symlinks outside the repository.
        hash.update((path.len() as u64).to_le_bytes());
        hash.update(path.as_bytes());
        let absolute = root.join(path);
        if let Some(parent) = absolute.parent() {
            match tokio::fs::canonicalize(parent).await {
                Ok(parent) if !parent.starts_with(&canonical_root) => {
                    return Err(request_error(
                        StatusCode::CONFLICT,
                        "Changed path leaves the workspace",
                    ));
                }
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                    return Err(request_error(StatusCode::CONFLICT, e.to_string()));
                }
                _ => {}
            }
        }
        match tokio::fs::symlink_metadata(&absolute).await {
            Ok(meta) if meta.file_type().is_symlink() => {
                hash.update(
                    tokio::fs::read_link(&absolute)
                        .await
                        .map_err(|e| request_error(StatusCode::CONFLICT, e.to_string()))?
                        .to_string_lossy()
                        .as_bytes(),
                );
            }
            Ok(meta) if meta.is_file() => {
                if meta.len() > 64 * 1024 * 1024 {
                    return Err(request_error(
                        StatusCode::PAYLOAD_TOO_LARGE,
                        "A changed file exceeds the 64 MB review limit",
                    ));
                }
                total_bytes = total_bytes.saturating_add(meta.len());
                if total_bytes > 128 * 1024 * 1024 {
                    return Err(request_error(
                        StatusCode::PAYLOAD_TOO_LARGE,
                        "Changed files exceed the 128 MB review limit",
                    ));
                }
                hash.update(meta.len().to_le_bytes());
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    hash.update(meta.permissions().mode().to_le_bytes());
                }
                hash.update(
                    tokio::fs::read(&absolute)
                        .await
                        .map_err(|e| request_error(StatusCode::CONFLICT, e.to_string()))?,
                );
            }
            Ok(_) => {
                return Err(request_error(
                    StatusCode::CONFLICT,
                    "Commit submodule changes separately",
                ));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(request_error(StatusCode::CONFLICT, e.to_string())),
        }
    }
    if git(binary, root, &["rev-parse", "HEAD"]).await? != head {
        return Err(request_error(
            StatusCode::CONFLICT,
            "Branch changed while preparing review; refresh",
        ));
    }
    Ok(GitState {
        head,
        branch,
        paths,
        base,
        can_branch: false,
        snapshot: format!("{:x}", hash.finalize()),
    })
}

async fn context(state: &AppState, work_id: String) -> ApiResult<(WorkItem, PathBuf, PathBuf)> {
    admit_forge(state, ExecutionClass::RepositoryMetadata, 64 * 1024, {
        let state = state.clone();
        move || {
            let item = forge(&state)
                .load(&parse_work_id(&work_id)?)
                .map_err(map_err)?;
            let root = item
                .workspace_environment()
                .ok_or_else(|| request_error(StatusCode::CONFLICT, "Project has no workspace"))?
                .worktree
                .clone();
            Ok((item, root, forge(&state).git().binary().to_owned()))
        }
    })
    .await
}
pub(super) async fn state(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
) -> ApiResult<Json<GitState>> {
    let (item, root, binary) = context(&state, work_id).await?;
    let can_branch = item.uses_attached_checkout();
    let WorkTarget::Git(target) = item.target;
    state
        .forge_execution
        .run_async(
            ExecutionClass::Observation,
            8 * 1024 * 1024,
            Some(root.display().to_string()),
            async move {
                Ok(snapshot(&binary, &root, target.base_ref)
                    .await
                    .map(|mut snapshot| {
                        snapshot.can_branch = can_branch;
                        Json(snapshot)
                    }))
            },
        )
        .await
        .map_err(map_err)?
}

pub(super) async fn commit(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<GitAction>,
) -> ApiResult<Json<serde_json::Value>> {
    execute(state, work_id, body, false).await
}
pub(super) async fn pull_request(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<GitAction>,
) -> ApiResult<Json<serde_json::Value>> {
    execute(state, work_id, body, true).await
}
async fn execute(
    state: AppState,
    work_id: String,
    body: GitAction,
    pr: bool,
) -> ApiResult<Json<serde_json::Value>> {
    let (_, root, binary) = context(&state, work_id.clone()).await?;
    let execution = state.forge_execution.clone();
    execution.run_async(if pr { ExecutionClass::NetworkGit } else { ExecutionClass::LocalMutation }, 8 * 1024 * 1024,
        Some(root.display().to_string()), async move {
        let result = async {
            // Revalidate custody after waiting for the repository admission lane.
            let item = tokio::task::spawn_blocking({
                let state = state.clone(); let work_id = work_id.clone(); let lease_id = body.lease_id.clone(); let generation = body.generation; let expected_root = root.clone();
                move || {
                    let (item, lease) = require_work_lease(&state, &parse_work_id(&work_id)?, &lease_id, generation)?;
                    if item.attempts.iter().find(|a| a.id == lease.attempt_id).is_none_or(|a| a.executor.kind != "human") {
                        return Err(request_error(StatusCode::CONFLICT, "Wait for the agent to finish before changing Git history"));
                    }
                    let env = item.environment_for_attempt(&lease.attempt_id).ok_or_else(|| request_error(StatusCode::CONFLICT, "Project has no workspace"))?;
                    if env.worktree != expected_root || item.active_attempt_ids().iter().any(|id| **id != lease.attempt_id) {
                        return Err(request_error(StatusCode::CONFLICT, "Workspace or active workers changed; refresh before continuing"));
                    }
                    changes_sync_preflight(forge(&state).as_ref(), env)?;
                    Ok(item)
                }
            }).await.map_err(|e| request_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))??;
            let WorkTarget::Git(target) = &item.target;
            let before = snapshot(&binary, &root, target.base_ref.clone()).await?;
            if before.snapshot != body.snapshot { return Err(request_error(StatusCode::CONFLICT, "Changes moved since you reviewed them. Refresh and review again.")); }
            let requested_branch = body.branch.trim();
            let new_branch = !requested_branch.is_empty() && requested_branch != before.branch;
            if new_branch {
                if !item.uses_attached_checkout() || requested_branch.starts_with('-') {
                    return Err(request_error(StatusCode::BAD_REQUEST, "A new branch can only be created for an attached checkout"));
                }
                git(&binary, &root, &["check-ref-format", "--branch", requested_branch]).await?;
            }
            let final_branch = if new_branch { requested_branch.to_owned() } else { before.branch.clone() };
            if pr {
                if !before.paths.is_empty() { return Err(request_error(StatusCode::CONFLICT, "Commit or remove working changes before creating a PR")); }
                if body.title.trim().is_empty() || body.base.trim().is_empty() || final_branch == body.base || matches!(final_branch.as_str(), "main" | "master") {
                    return Err(request_error(StatusCode::BAD_REQUEST, "Choose a feature branch and a different PR base branch"));
                }
                if body.base.starts_with('-') { return Err(request_error(StatusCode::BAD_REQUEST, "Invalid base branch")); }
                git(&binary, &root, &["check-ref-format", "--branch", &body.base]).await?;
                if new_branch {
                    git(&binary, &root, &["switch", "-c", &final_branch]).await?;
                    record_commit(&state, &item, &before.head, &before.head, &final_branch).await?;
                }
                let origin = git(&binary, &root, &["remote", "get-url", "--push", "--all", "origin"]).await?;
                let repository = github_repository(&origin)?;
                // Look up first: retrying after an uncertain response must not create duplicates.
                let listed = command(FsPath::new("gh"), &root, vec!["pr", "list", "--repo", &repository, "--head", &final_branch, "--base", &body.base, "--state", "open", "--json", "url"].into_iter().map(str::to_owned).collect(), vec![]).await?;
                let existing: serde_json::Value = serde_json::from_slice(&listed).map_err(|e| request_error(StatusCode::CONFLICT, e.to_string()))?;
                git(&binary, &root, &["push", "origin", &format!("{}:refs/heads/{}", before.head, final_branch)]).await?;
                if let Some(url) = existing.get(0).and_then(|v| v.get("url")).and_then(|v| v.as_str()) {
                    return Ok(Json(serde_json::json!({"url": url, "existing": true})));
                }
                let mut args = vec!["pr".into(), "create".into(), "--repo".into(), repository, "--head".into(), final_branch, "--base".into(), body.base, "--title".into(), body.title, "--body".into(), body.body];
                if body.draft { args.push("--draft".into()); }
                let url = command(FsPath::new("gh"), &root, args, vec![]).await.map_err(|_| request_error(StatusCode::CONFLICT, "The branch was pushed, but GitHub did not confirm PR creation. Check GitHub authentication and retry; an existing PR will be reused."))?;
                return Ok(Json(serde_json::json!({"url": String::from_utf8_lossy(&url).trim(), "existing": false})));
            }
            if body.message.trim().is_empty() || body.paths.is_empty() || body.paths.iter().any(|p| !before.paths.contains(p)) {
                return Err(request_error(StatusCode::BAD_REQUEST, "Choose changed files and enter a commit message"));
            }
            let temp = prepare_commit(&binary, &root, &before, &body.paths).await?;
            if new_branch {
                git(&binary, &root, &["switch", "-c", &final_branch]).await?;
                record_commit(&state, &item, &before.head, &before.head, &final_branch).await?;
            }
            let (head, warning) = finish_commit(&binary, &root, &temp, &body.message, &body.paths).await?;
            record_commit(&state, &item, &before.head, &head, &final_branch).await?;
            Ok(Json(serde_json::json!({"head": head, "warning": warning})))
        }.await;
        Ok(result)
    }).await.map_err(map_err)?
}

/// Bind PR creation to the actual push destination, including GitHub Enterprise.
fn github_repository(origin: &str) -> ApiResult<String> {
    let invalid = || {
        request_error(
            StatusCode::BAD_REQUEST,
            "Origin must have one GitHub SSH or HTTPS push URL",
        )
    };
    if origin.lines().count() != 1 {
        return Err(invalid());
    }
    let normalized = if let Some(scp) = origin.strip_prefix("git@") {
        format!("ssh://git@{}", scp.replacen(':', "/", 1))
    } else {
        origin.to_owned()
    };
    let url = reqwest::Url::parse(&normalized).map_err(|_| invalid())?;
    if !matches!(url.scheme(), "https" | "ssh") || url.query().is_some() || url.fragment().is_some()
    {
        return Err(invalid());
    }
    let host = url.host_str().ok_or_else(invalid)?;
    let path = url.path().trim_matches('/').trim_end_matches(".git");
    let parts = path.split('/').collect::<Vec<_>>();
    if parts.len() != 2
        || parts.iter().any(|p| {
            p.is_empty()
                || !p
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        })
    {
        return Err(invalid());
    }
    Ok(format!("{host}/{path}"))
}

async fn record_commit(
    state: &AppState,
    item: &WorkItem,
    before: &str,
    after: &str,
    branch: &str,
) -> ApiResult<()> {
    let state = state.clone();
    let item = item.clone();
    let before = before.to_owned();
    let after = after.to_owned();
    let branch = branch.to_owned();
    tokio::task::spawn_blocking(move || {
        if item.uses_attached_checkout() {
            forge(&state).record_review_commit(&item.id, &GitOid::new(before), &GitOid::new(after.clone()), &branch, &actor_from_state(&state))
                .map_err(|_| request_error(StatusCode::CONFLICT, format!("Git moved to {after}, but the project could not be refreshed. Reattach the checkout before continuing.")))?;
        }
        publish_project_change(&state, &item, ForgeProjectEventKind::GitStatus, None, None, None);
        Ok(())
    }).await.map_err(|e| request_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
}

async fn prepare_commit(
    binary: &FsPath,
    root: &FsPath,
    before: &GitState,
    paths: &[String],
) -> ApiResult<tempfile::TempDir> {
    if paths.is_empty() || paths.iter().any(|path| !before.paths.contains(path)) {
        return Err(request_error(
            StatusCode::BAD_REQUEST,
            "Select changed files",
        ));
    }
    let temp = tokio::task::spawn_blocking(tempfile::tempdir)
        .await
        .map_err(|e| request_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(|e| request_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let env = vec![(
        "GIT_INDEX_FILE".into(),
        temp.path().join("index").to_string_lossy().into_owned(),
    )];
    command(
        binary,
        root,
        vec!["read-tree".into(), before.head.clone()],
        env.clone(),
    )
    .await?;
    let mut args = vec![
        "--literal-pathspecs".into(),
        "add".into(),
        "--all".into(),
        "--".into(),
    ];
    args.extend_from_slice(paths);
    command(binary, root, args, env).await?;
    if snapshot(binary, root, before.base.clone()).await?.snapshot != before.snapshot {
        return Err(request_error(
            StatusCode::CONFLICT,
            "Files changed while preparing the commit; review again",
        ));
    }
    Ok(temp)
}
async fn finish_commit(
    binary: &FsPath,
    root: &FsPath,
    temp: &tempfile::TempDir,
    message: &str,
    paths: &[String],
) -> ApiResult<(String, Option<String>)> {
    let index_path = git(
        binary,
        root,
        &["rev-parse", "--path-format=absolute", "--git-path", "index"],
    )
    .await?;
    let index_before = tokio::fs::read(&index_path)
        .await
        .map_err(|e| request_error(StatusCode::CONFLICT, e.to_string()))?;
    let env = vec![(
        "GIT_INDEX_FILE".into(),
        temp.path().join("index").to_string_lossy().into_owned(),
    )];
    // Normal commit runs hooks/signing. Unrelated staging stays in the principal index.
    command(
        binary,
        root,
        vec!["commit".into(), "-m".into(), message.into()],
        env,
    )
    .await?;
    let head = git(binary, root, &["rev-parse", "HEAD"]).await?;
    if tokio::fs::read(&index_path).await.ok().as_ref() != Some(&index_before) {
        return Ok((head, Some("The commit succeeded. Staging changed during the commit and was preserved; inspect Git status before continuing.".into())));
    }
    let mut reset = vec![
        "--literal-pathspecs".into(),
        "reset".into(),
        "--quiet".into(),
        "HEAD".into(),
        "--".into(),
    ];
    reset.extend_from_slice(paths);
    let warning = command(binary, root, reset, vec![]).await.err().map(|_| "The commit succeeded, but staging could not be refreshed. Inspect Git status before continuing.".to_owned());
    Ok((head, warning))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pull_request_repository_matches_push_destination() {
        for origin in [
            "git@github.com:owner/repo.git",
            "https://github.com/owner/repo.git",
            "ssh://git@github.com/owner/repo",
        ] {
            assert_eq!(github_repository(origin).unwrap(), "github.com/owner/repo");
        }
        assert_eq!(
            github_repository("https://github.internal/team/repo.git").unwrap(),
            "github.internal/team/repo"
        );
        for invalid in [
            "/tmp/repository",
            "https://github.com/owner/repo.git\nhttps://github.com/other/repo.git",
            "https://github.com/owner/repo/extra",
            "file:///repo",
        ] {
            assert!(github_repository(invalid).is_err());
        }
    }
    async fn repository() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let bin = FsPath::new("git");
        git(bin, root, &["init", "-b", "main"]).await.unwrap();
        git(bin, root, &["config", "user.name", "Review test"])
            .await
            .unwrap();
        git(
            bin,
            root,
            &["config", "user.email", "review@example.invalid"],
        )
        .await
        .unwrap();
        git(bin, root, &["config", "commit.gpgsign", "false"])
            .await
            .unwrap();
        tokio::fs::write(root.join("one.txt"), "one").await.unwrap();
        tokio::fs::write(root.join("two.txt"), "two").await.unwrap();
        git(bin, root, &["add", "."]).await.unwrap();
        git(bin, root, &["commit", "-m", "initial"]).await.unwrap();
        dir
    }
    #[test]
    fn status_handles_renames_spaces_and_conflicts() {
        assert_eq!(
            status_paths(b"R  new name old name ?? untracked ").unwrap(),
            vec!["new name", "old name", "untracked"]
        );
        assert!(status_paths(b"UU conflict ").is_err());
    }
    #[tokio::test]
    async fn selected_commit_preserves_unrelated_staged_files_and_runs_hooks() {
        let dir = repository().await;
        let root = dir.path();
        let bin = FsPath::new("git");
        tokio::fs::write(root.join("one.txt"), "updated")
            .await
            .unwrap();
        tokio::fs::write(root.join("two.txt"), "staged separately")
            .await
            .unwrap();
        tokio::fs::write(root.join("new file.txt"), "new")
            .await
            .unwrap();
        git(bin, root, &["add", "two.txt"]).await.unwrap();
        let before = snapshot(bin, root, "main".into()).await.unwrap();
        let paths = vec!["one.txt".into(), "new file.txt".into()];
        let prepared = prepare_commit(bin, root, &before, &paths).await.unwrap();
        finish_commit(bin, root, &prepared, "selected files", &paths)
            .await
            .unwrap();
        assert_eq!(
            git(bin, root, &["show", "HEAD:two.txt"]).await.unwrap(),
            "two"
        );
        assert_eq!(
            git(bin, root, &["diff", "--cached", "--name-only"])
                .await
                .unwrap(),
            "two.txt"
        );
        assert_eq!(
            git(bin, root, &["show", "HEAD:new file.txt"])
                .await
                .unwrap(),
            "new"
        );
    }
    #[tokio::test]
    async fn stale_snapshot_rejects_commit_without_changing_head_or_index() {
        let dir = repository().await;
        let root = dir.path();
        let bin = FsPath::new("git");
        tokio::fs::write(root.join("one.txt"), "reviewed")
            .await
            .unwrap();
        let before = snapshot(bin, root, "main".into()).await.unwrap();
        let index = git(bin, root, &["write-tree"]).await.unwrap();
        tokio::fs::write(root.join("one.txt"), "changed again")
            .await
            .unwrap();
        assert!(
            prepare_commit(bin, root, &before, &["one.txt".into()])
                .await
                .is_err()
        );
        assert_eq!(
            git(bin, root, &["rev-parse", "HEAD"]).await.unwrap(),
            before.head
        );
        assert_eq!(git(bin, root, &["write-tree"]).await.unwrap(), index);
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn rejecting_hook_leaves_head_and_principal_index_unchanged() {
        use std::os::unix::fs::PermissionsExt;
        let dir = repository().await;
        let root = dir.path();
        let bin = FsPath::new("git");
        tokio::fs::write(root.join("one.txt"), "reviewed")
            .await
            .unwrap();
        let before = snapshot(bin, root, "main".into()).await.unwrap();
        let index = git(bin, root, &["write-tree"]).await.unwrap();
        let hook = root.join(".git/hooks/pre-commit");
        tokio::fs::write(&hook, "#!/bin/sh\nexit 1\n")
            .await
            .unwrap();
        tokio::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755))
            .await
            .unwrap();
        let paths = vec!["one.txt".into()];
        let prepared = prepare_commit(bin, root, &before, &paths).await.unwrap();
        assert!(
            finish_commit(bin, root, &prepared, "blocked by hook", &paths)
                .await
                .is_err()
        );
        assert_eq!(
            git(bin, root, &["rev-parse", "HEAD"]).await.unwrap(),
            before.head
        );
        assert_eq!(git(bin, root, &["write-tree"]).await.unwrap(), index);
    }
}
