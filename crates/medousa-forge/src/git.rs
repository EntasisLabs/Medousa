//! Subprocess Git engine — Forge's only Git implementation. CWD-parameterized
//! (never mutates global git state), explicit OID comparisons (never symbolic
//! merge-base games for evidence), and explicit checkpoint identity via env
//! vars (Forge never impersonates the user).

use std::io::{Read as _, Write as _};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use sha2::{Digest as _, Sha256};

use crate::error::{ForgeError, Result};
use crate::execution::{
    MAX_CAPTURE_BYTES, capture_child_output_bounded, redact_git_text, run_command_bounded,
};
use crate::model::{GitOid, RepoId, RepoIdentity, SubmodulePin};

/// Streaming digest of a worktree binary diff (never materializes the patch).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamedDiffDigest {
    pub digest: String,
    pub bytes_hashed: u64,
    pub truncated: bool,
}

fn format_git_spawn_error(args: &[&str], err: &impl std::fmt::Display) -> String {
    format!(
        "failed to spawn git {}: {err}",
        redact_git_text(&args.join(" "))
    )
}

fn format_git_command_error(args: &[&str], detail: impl AsRef<str>) -> String {
    format!(
        "git {} failed: {}",
        redact_git_text(&args.join(" ")),
        redact_git_text(detail.as_ref())
    )
}

/// Committer identity stamped on every Forge-created commit. Authorship may
/// be attributed to the executor's identity where known; committer is always
/// recognizably Forge.
pub const FORGE_COMMITTER_NAME: &str = "Medousa Forge";
pub const FORGE_COMMITTER_EMAIL: &str = "forge@medousa.local";
static PORTABLE_BUNDLE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// Identity attributed as the *author* of a checkpoint commit.
#[derive(Debug, Clone)]
pub struct CheckpointAuthor {
    pub name: String,
    pub email: String,
}

impl Default for CheckpointAuthor {
    fn default() -> Self {
        Self {
            name: FORGE_COMMITTER_NAME.into(),
            email: FORGE_COMMITTER_EMAIL.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct GitEngine {
    git: PathBuf,
}

impl GitEngine {
    fn command(&self) -> Command {
        let mut command = Command::new(&self.git);
        medousa_host::hide_subprocess_window(&mut command);
        command
    }

    /// Resolve the git binary: `GIT_BIN` env override, then PATH.
    pub fn detect() -> Result<Self> {
        if let Ok(explicit) = std::env::var("GIT_BIN") {
            let path = PathBuf::from(explicit.trim());
            if path.is_file() {
                return Ok(Self { git: path });
            }
        }
        let name = if cfg!(windows) { "git.exe" } else { "git" };
        if let Some(path) = find_in_path(name) {
            return Ok(Self { git: path });
        }
        Err(ForgeError::Git(
            "git binary not found (set GIT_BIN or put git on PATH)".into(),
        ))
    }

    pub fn with_binary(git: PathBuf) -> Self {
        Self { git }
    }

    pub fn binary(&self) -> &Path {
        &self.git
    }

    pub(crate) fn run(&self, cwd: &Path, args: &[&str]) -> Result<String> {
        let (stdout, stderr, truncated, status) = self.run_capped(cwd, args)?;
        if truncated {
            return Err(ForgeError::Git(format_git_command_error(
                args,
                "output exceeded capture budget",
            )));
        }
        if !status.success() {
            let stderr = String::from_utf8_lossy(&stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&stdout).trim().to_string();
            let detail = if !stderr.is_empty() { stderr } else { stdout };
            return Err(ForgeError::Git(format_git_command_error(
                args,
                if detail.is_empty() {
                    status.to_string()
                } else {
                    detail
                },
            )));
        }
        Ok(String::from_utf8_lossy(&stdout).to_string())
    }

    fn run_bytes(&self, cwd: &Path, args: &[&str]) -> Result<Vec<u8>> {
        let (stdout, stderr, truncated, status) = self.run_capped(cwd, args)?;
        if truncated {
            return Err(ForgeError::Git(format_git_command_error(
                args,
                "output exceeded capture budget",
            )));
        }
        if !status.success() {
            let stderr = String::from_utf8_lossy(&stderr).trim().to_string();
            return Err(ForgeError::Git(format_git_command_error(
                args,
                if stderr.is_empty() {
                    status.to_string()
                } else {
                    stderr
                },
            )));
        }
        Ok(stdout)
    }

    /// Like `run_bytes`, but treats exit status 1 as success (git diff found
    /// differences, especially with `--no-index`).
    fn run_diff_bytes(&self, cwd: &Path, args: &[&str]) -> Result<Vec<u8>> {
        let (stdout, stderr, truncated, status) = self.run_capped(cwd, args)?;
        if truncated {
            return Err(ForgeError::Git(format_git_command_error(
                args,
                "output exceeded capture budget",
            )));
        }
        match status.code() {
            Some(0) | Some(1) => Ok(stdout),
            _ => {
                let stderr = String::from_utf8_lossy(&stderr).trim().to_string();
                Err(ForgeError::Git(format_git_command_error(
                    args,
                    if stderr.is_empty() {
                        status.to_string()
                    } else {
                        stderr
                    },
                )))
            }
        }
    }

    fn run_capped(
        &self,
        cwd: &Path,
        args: &[&str],
    ) -> Result<(Vec<u8>, Vec<u8>, bool, std::process::ExitStatus)> {
        let mut command = self.command();
        command
            .args(args)
            .current_dir(cwd)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env("GIT_TERMINAL_PROMPT", "0");
        run_command_bounded(command, MAX_CAPTURE_BYTES).map_err(|err| match err {
            ForgeError::Git(message) if message.contains("failed to spawn") => {
                ForgeError::Git(format_git_spawn_error(args, &message))
            }
            other => other,
        })
    }

    fn run_with_index_bytes(&self, cwd: &Path, index: &Path, args: &[&str]) -> Result<Vec<u8>> {
        let mut command = self.command();
        command
            .args(args)
            .current_dir(cwd)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env("GIT_INDEX_FILE", index)
            .env("GIT_TERMINAL_PROMPT", "0");
        let (stdout, stderr, truncated, status) =
            run_command_bounded(command, MAX_CAPTURE_BYTES)
                .map_err(|err| ForgeError::Git(format_git_spawn_error(args, &err.to_string())))?;
        if truncated {
            return Err(ForgeError::Git(format_git_command_error(
                args,
                "output exceeded capture budget",
            )));
        }
        if !status.success() {
            let stderr = String::from_utf8_lossy(&stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&stdout).trim().to_string();
            let detail = if !stderr.is_empty() { stderr } else { stdout };
            return Err(ForgeError::Git(format_git_command_error(
                args,
                if detail.is_empty() {
                    status.to_string()
                } else {
                    detail
                },
            )));
        }
        Ok(stdout)
    }

    fn run_with_index(&self, cwd: &Path, index: &Path, args: &[&str]) -> Result<String> {
        Ok(String::from_utf8_lossy(&self.run_with_index_bytes(cwd, index, args)?).to_string())
    }

    fn refresh_index(&self, cwd: &Path, index: &Path) -> Result<()> {
        let args = ["update-index", "--refresh"];
        let mut command = self.command();
        command
            .args(args)
            .current_dir(cwd)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env("GIT_INDEX_FILE", index)
            .env("GIT_TERMINAL_PROMPT", "0");
        let (_stdout, stderr, truncated, status) = run_command_bounded(command, MAX_CAPTURE_BYTES)
            .map_err(|err| ForgeError::Git(format_git_spawn_error(&args, &err.to_string())))?;
        if truncated {
            return Err(ForgeError::Git(format_git_command_error(
                &args,
                "output exceeded capture budget",
            )));
        }
        match status.code() {
            // Exit 1 only means at least one worktree file differs from the
            // temporary index, which is precisely what the following diff is
            // meant to report.
            Some(0) | Some(1) => Ok(()),
            _ => Err(ForgeError::Git(format_git_command_error(
                &args,
                String::from_utf8_lossy(&stderr).trim(),
            ))),
        }
    }

    fn run_with_stdin(&self, cwd: &Path, args: &[&str], stdin: &[u8]) -> Result<()> {
        let mut child = self
            .command()
            .args(args)
            .current_dir(cwd)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| ForgeError::Git(format_git_spawn_error(args, &e)))?;
        child
            .stdin
            .as_mut()
            .ok_or_else(|| ForgeError::Git("git stdin was unavailable".into()))?
            .write_all(stdin)?;
        // Drop stdin so git sees EOF.
        drop(child.stdin.take());
        let (stdout, stderr, truncated, status) =
            capture_child_output_bounded(child, MAX_CAPTURE_BYTES)?;
        let _ = stdout;
        if truncated {
            return Err(ForgeError::Git(format_git_command_error(
                args,
                "output exceeded capture budget",
            )));
        }
        if !status.success() {
            let stderr = String::from_utf8_lossy(&stderr).trim().to_string();
            return Err(ForgeError::Git(format_git_command_error(
                args,
                if stderr.is_empty() {
                    status.to_string()
                } else {
                    stderr
                },
            )));
        }
        Ok(())
    }

    /// True when `path` is inside a Git repository.
    pub fn is_repo(&self, path: &Path) -> bool {
        self.run(path, &["rev-parse", "--is-inside-work-tree"])
            .map(|s| s.trim() == "true")
            .unwrap_or(false)
    }

    /// Canonical root of the working tree containing `path`.
    pub fn worktree_root(&self, path: &Path) -> Result<PathBuf> {
        let root = self.run(path, &["rev-parse", "--show-toplevel"])?;
        Ok(PathBuf::from(root.trim()))
    }

    /// Checked-out branch, or `None` when HEAD is detached.
    pub fn current_branch(&self, path: &Path) -> Result<Option<String>> {
        let branch = self.run(path, &["branch", "--show-current"])?;
        Ok(Some(branch.trim().to_string()).filter(|value| !value.is_empty()))
    }

    /// Local branch names, without the `refs/heads/` prefix.
    pub fn local_branches(&self, path: &Path) -> Result<Vec<String>> {
        let output = self.run(
            path,
            &["for-each-ref", "--format=%(refname:short)", "refs/heads/"],
        )?;
        Ok(sorted_nonempty_lines(&output))
    }

    /// Configured Git remote names.
    pub fn remote_names(&self, path: &Path) -> Result<Vec<String>> {
        let output = self.run(path, &["remote"])?;
        Ok(sorted_nonempty_lines(&output))
    }

    /// Branch names for one remote, without the `<remote>/` prefix. The
    /// synthetic remote HEAD alias is intentionally excluded.
    pub fn remote_branches(&self, path: &Path, remote: &str) -> Result<Vec<String>> {
        let prefix = format!("refs/remotes/{remote}/");
        let output = self.run(
            path,
            &["for-each-ref", "--format=%(refname:short)", &prefix],
        )?;
        let short_prefix = format!("{remote}/");
        let mut branches = output
            .lines()
            .map(str::trim)
            .filter_map(|name| name.strip_prefix(&short_prefix))
            .filter(|name| !name.is_empty() && *name != "HEAD")
            .map(str::to_string)
            .collect::<Vec<_>>();
        branches.sort();
        branches.dedup();
        Ok(branches)
    }

    /// Default branch advertised by one remote, when its tracking HEAD exists.
    pub fn remote_default_branch(&self, path: &Path, remote: &str) -> Option<String> {
        let reference = format!("refs/remotes/{remote}/HEAD");
        let prefix = format!("{remote}/");
        self.run(path, &["symbolic-ref", "--quiet", "--short", &reference])
            .ok()
            .and_then(|value| value.trim().strip_prefix(&prefix).map(str::to_string))
            .filter(|value| !value.is_empty())
    }

    /// Best branch for new work: remote default, checked-out branch, then a
    /// conventional local default. This is advisory; callers still resolve it
    /// before creating a governed environment.
    pub fn suggested_base_ref(&self, path: &Path) -> Result<Option<String>> {
        if !self.has_commits(path)? {
            return Ok(None);
        }
        if let Ok(remote) = self.run(
            path,
            &[
                "symbolic-ref",
                "--quiet",
                "--short",
                "refs/remotes/origin/HEAD",
            ],
        ) {
            let branch = remote
                .trim()
                .strip_prefix("origin/")
                .unwrap_or(remote.trim());
            for candidate in [branch, remote.trim()] {
                if !candidate.is_empty() && self.resolve_oid(path, candidate).is_ok() {
                    return Ok(Some(candidate.to_string()));
                }
            }
        }
        if let Some(branch) = self.current_branch(path)?
            && self.resolve_oid(path, &branch).is_ok()
        {
            return Ok(Some(branch));
        }
        for candidate in ["main", "master"] {
            if self.resolve_oid(path, candidate).is_ok() {
                return Ok(Some(candidate.to_string()));
            }
        }
        if self.resolve_oid(path, "HEAD").is_ok() {
            return Ok(Some("HEAD".to_string()));
        }
        Ok(None)
    }

    /// Canonical repository identity, stable across worktrees of the same
    /// repository: derived from `git rev-parse --git-common-dir`.
    pub fn repo_identity(&self, path: &Path) -> Result<RepoIdentity> {
        let common_dir_raw = self.run(path, &["rev-parse", "--git-common-dir"])?;
        let common_dir = canonicalish(path, common_dir_raw.trim());
        let format = self
            .run(path, &["rev-parse", "--show-object-format"])
            .ok()
            .map(|s| s.trim().to_string());
        let remotes = self
            .run(path, &["remote", "-v"])
            .map(|s| {
                let mut urls: Vec<String> = s
                    .lines()
                    .filter(|l| l.ends_with("(fetch)"))
                    .filter_map(|l| l.split_whitespace().nth(1).map(str::to_string))
                    .collect();
                urls.sort();
                urls.dedup();
                urls
            })
            .unwrap_or_default();
        Ok(RepoIdentity {
            repo_id: RepoId::from(common_dir.to_string_lossy().into_owned()),
            requested_path: path.to_path_buf(),
            common_dir,
            format,
            remotes,
        })
    }

    /// Resolve any revision to a commit OID.
    pub fn resolve_oid(&self, cwd: &Path, rev: &str) -> Result<GitOid> {
        let revision = format!("{rev}^{{commit}}");
        let out = self.run(
            cwd,
            &["rev-parse", "--verify", "--end-of-options", &revision],
        )?;
        Ok(GitOid::new(out.trim()))
    }

    /// True when any commit exists in the repository, including on a branch
    /// other than the currently checked-out (possibly unborn) branch.
    pub fn has_commits(&self, cwd: &Path) -> Result<bool> {
        Ok(!self
            .run(cwd, &["rev-list", "--all", "--max-count=1"])?
            .trim()
            .is_empty())
    }

    /// Resolve a user-selected project base while preserving the distinction
    /// between an empty repository and a branch that disappeared.
    pub fn resolve_base_oid(&self, cwd: &Path, rev: &str) -> Result<GitOid> {
        let revision = rev.trim();
        if !revision.is_empty()
            && let Ok(oid) = self.resolve_oid(cwd, revision)
        {
            return Ok(oid);
        }
        if !self.has_commits(cwd)? {
            return Err(ForgeError::RepositoryEmpty(cwd.to_path_buf()));
        }
        Err(ForgeError::BaseRefMissing {
            repo_path: cwd.to_path_buf(),
            reference: revision.to_string(),
        })
    }

    pub fn head_oid(&self, cwd: &Path) -> Result<GitOid> {
        self.resolve_oid(cwd, "HEAD")
    }

    /// Add a worktree at `path` on a *new* branch `branch` starting at
    /// `baseline`. `repo_cwd` is any directory inside the repository.
    pub fn worktree_add(
        &self,
        repo_cwd: &Path,
        path: &Path,
        branch: &str,
        baseline: &GitOid,
    ) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        self.run(
            repo_cwd,
            &[
                "worktree",
                "add",
                "-b",
                branch,
                &path.to_string_lossy(),
                baseline.as_str(),
            ],
        )?;
        Ok(())
    }

    /// Fork a new worktree from `source` while preserving its tracked and
    /// untracked dirty starting state. The source is never mutated.
    pub fn worktree_add_from_worktree(
        &self,
        repo_cwd: &Path,
        source: &Path,
        destination: &Path,
        branch: &str,
    ) -> Result<GitOid> {
        let source_root = self.worktree_root(source)?;
        let source_root = std::fs::canonicalize(source_root)?;
        let requested_source = std::fs::canonicalize(source)?;
        if source_root != requested_source {
            return Err(ForgeError::EnvironmentDrift(
                "attempt source must be the root of its governed worktree".into(),
            ));
        }
        let head = self.head_oid(&source_root)?;
        self.worktree_add(repo_cwd, destination, branch, &head)?;
        let copied = self.copy_worktree_state(&source_root, destination, &head);
        if let Err(err) = copied {
            let _ = self.worktree_remove(repo_cwd, destination);
            let _ = self.branch_delete(repo_cwd, branch);
            return Err(err);
        }
        Ok(head)
    }

    fn copy_worktree_state(&self, source: &Path, destination: &Path, head: &GitOid) -> Result<()> {
        let patch = self.diff_binary_worktree(source, head)?;
        if !patch.is_empty() {
            self.run_with_stdin(destination, &["apply", "--binary", "-"], &patch)?;
        }
        for entry in self.status_porcelain(source)? {
            if entry.kind != PorcelainKind::Untracked {
                continue;
            }
            let relative = safe_worktree_relative_path(&entry.path)?;
            let source_path = source.join(&relative);
            let metadata = std::fs::symlink_metadata(&source_path)?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(ForgeError::EnvironmentDrift(format!(
                    "untracked attempt input must be a regular file: {}",
                    relative.display()
                )));
            }
            let destination_path = destination.join(&relative);
            let parent = destination_path.parent().ok_or_else(|| {
                ForgeError::EnvironmentDrift("untracked attempt path has no parent".into())
            })?;
            require_safe_worktree_parent(destination, parent)?;
            std::fs::create_dir_all(parent)?;
            require_safe_worktree_parent(destination, parent)?;
            std::fs::copy(source_path, destination_path)?;
        }
        Ok(())
    }

    pub fn worktree_remove(&self, repo_cwd: &Path, path: &Path) -> Result<()> {
        self.run(
            repo_cwd,
            &["worktree", "remove", "--force", &path.to_string_lossy()],
        )?;
        Ok(())
    }

    pub fn branch_delete(&self, repo_cwd: &Path, branch: &str) -> Result<()> {
        self.run(repo_cwd, &["branch", "-D", branch])?;
        Ok(())
    }

    pub fn branch_exists(&self, repo_cwd: &Path, branch: &str) -> bool {
        self.run(
            repo_cwd,
            &[
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/heads/{branch}"),
            ],
        )
        .is_ok()
    }

    /// `git worktree list --porcelain`, parsed as (path, checked-out branch).
    pub fn worktree_list(&self, repo_cwd: &Path) -> Result<Vec<(PathBuf, Option<String>)>> {
        let out = self.run(repo_cwd, &["worktree", "list", "--porcelain"])?;
        let mut entries = Vec::new();
        let mut path: Option<PathBuf> = None;
        let mut branch: Option<String> = None;
        for line in out.lines().chain(std::iter::once("")) {
            if let Some(p) = line.strip_prefix("worktree ") {
                path = Some(PathBuf::from(p));
            } else if let Some(b) = line.strip_prefix("branch ") {
                branch = Some(b.strip_prefix("refs/heads/").unwrap_or(b).to_string());
            } else if line.is_empty()
                && let Some(p) = path.take()
            {
                entries.push((p, branch.take()));
            }
        }
        Ok(entries)
    }

    /// Sync a checked-out working tree to a commit (integration follow-up for
    /// a base ref that is checked out somewhere).
    pub fn reset_hard(&self, cwd: &Path, oid: &GitOid) -> Result<()> {
        self.run(cwd, &["reset", "--hard", oid.as_str()])?;
        Ok(())
    }

    /// `git status --porcelain=v2 -z`, parsed. Includes untracked files.
    pub fn status_porcelain(&self, cwd: &Path) -> Result<Vec<PorcelainEntry>> {
        Ok(self.status_porcelain_with_branch(cwd)?.1)
    }

    /// Porcelain status plus `# branch.*` tracking headers (`--branch`).
    pub fn status_porcelain_with_branch(
        &self,
        cwd: &Path,
    ) -> Result<(BranchTracking, Vec<PorcelainEntry>)> {
        let out = self.run_bytes(
            cwd,
            &[
                "status",
                "--porcelain=v2",
                "-z",
                "--branch",
                "--untracked-files=all",
            ],
        )?;
        Ok((parse_porcelain_v2_branch(&out), parse_porcelain_v2_z(&out)))
    }

    pub fn is_clean(&self, cwd: &Path) -> Result<bool> {
        Ok(self.status_porcelain(cwd)?.is_empty())
    }

    /// Direct two-dot binary diff between explicit OIDs — evidence compares the
    /// provisioned baseline against the sealed head, never through a symbolic
    /// merge base.
    pub fn diff_binary(&self, cwd: &Path, from: &GitOid, to: &GitOid) -> Result<Vec<u8>> {
        self.run_bytes(
            cwd,
            &[
                "diff",
                "--binary",
                "--full-index",
                from.as_str(),
                to.as_str(),
            ],
        )
    }

    /// Unified text diff for one normalized Git path between two exact
    /// revisions. Callers must validate the path before crossing this API.
    pub fn diff_path(&self, cwd: &Path, from: &GitOid, to: &GitOid, path: &str) -> Result<Vec<u8>> {
        self.run_bytes(
            cwd,
            &[
                "diff",
                "--no-ext-diff",
                "--no-color",
                "--unified=3",
                from.as_str(),
                to.as_str(),
                "--",
                path,
            ],
        )
    }

    /// Unified text diff for one path between an exact revision and the
    /// *working tree* (includes unstaged edits). Exit status 1 (differences)
    /// is treated as success and returns the patch bytes.
    pub fn diff_path_worktree(&self, cwd: &Path, from: &GitOid, path: &str) -> Result<Vec<u8>> {
        self.run_diff_bytes(
            cwd,
            &[
                "diff",
                "--no-ext-diff",
                "--no-color",
                "--unified=3",
                from.as_str(),
                "--",
                path,
            ],
        )
    }

    /// Diff an untracked worktree file against `/dev/null` (all additions).
    pub fn diff_untracked_path(&self, cwd: &Path, path: &str) -> Result<Vec<u8>> {
        self.run_diff_bytes(
            cwd,
            &[
                "diff",
                "--no-ext-diff",
                "--no-color",
                "--unified=3",
                "--no-index",
                "--",
                "/dev/null",
                path,
            ],
        )
    }

    /// `git diff --binary <baseline>` against the *working tree* (uncommitted
    /// state), used for pre-checkpoint inspection.
    pub fn diff_binary_worktree(&self, cwd: &Path, baseline: &GitOid) -> Result<Vec<u8>> {
        self.diff_binary_worktree_bounded(cwd, baseline, MAX_CAPTURE_BYTES)
    }

    /// Stream-friendly bounded worktree diff used for observation hashing.
    ///
    /// Truncation is signaled by returning a buffer shorter than the live
    /// stream together with callers that prefer [`Self::hash_diff_binary_worktree_streaming`]
    /// for Exact/Incomplete honesty.
    pub fn diff_binary_worktree_bounded(
        &self,
        cwd: &Path,
        baseline: &GitOid,
        max_bytes: usize,
    ) -> Result<Vec<u8>> {
        let max_bytes = max_bytes.min(MAX_CAPTURE_BYTES);
        let (stdout, stderr, truncated, status) = self.run_capped(
            cwd,
            &["diff", "--binary", "--full-index", baseline.as_str()],
        )?;
        match status.code() {
            Some(0) | Some(1) => {
                if truncated || stdout.len() > max_bytes {
                    return Ok(stdout[..stdout.len().min(max_bytes)].to_vec());
                }
                Ok(stdout)
            }
            _ => {
                let stderr = String::from_utf8_lossy(&stderr).trim().to_string();
                Err(ForgeError::Git(format_git_command_error(
                    &["diff", "--binary", "--full-index"],
                    if stderr.is_empty() {
                        status.to_string()
                    } else {
                        stderr
                    },
                )))
            }
        }
    }

    /// Hash `git diff --binary` stdout incrementally without retaining the patch.
    pub fn hash_diff_binary_worktree_streaming(
        &self,
        cwd: &Path,
        baseline: &GitOid,
        max_bytes: u64,
    ) -> Result<StreamedDiffDigest> {
        let max_bytes = max_bytes.min(MAX_CAPTURE_BYTES as u64).max(1);
        let mut child = self
            .command()
            .args(["diff", "--binary", "--full-index", baseline.as_str()])
            .current_dir(cwd)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| {
                ForgeError::Git(format_git_spawn_error(
                    &["diff", "--binary", "--full-index"],
                    &err,
                ))
            })?;
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| ForgeError::Git("git stdout was unavailable".into()))?;
        let mut stderr = child
            .stderr
            .take()
            .ok_or_else(|| ForgeError::Git("git stderr was unavailable".into()))?;
        let stderr_thread = std::thread::spawn(move || {
            let mut buf = Vec::new();
            let mut chunk = [0u8; 8192];
            let mut truncated = false;
            loop {
                match stderr.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => {
                        if buf.len() < 64 * 1024 {
                            let take = (64 * 1024 - buf.len()).min(n);
                            buf.extend_from_slice(&chunk[..take]);
                            if take < n {
                                truncated = true;
                            }
                        } else {
                            truncated = true;
                        }
                    }
                    Err(_) => break,
                }
            }
            (buf, truncated)
        });
        let mut hasher = Sha256::new();
        let mut chunk = [0u8; 64 * 1024];
        let mut bytes_hashed = 0u64;
        let mut truncated = false;
        loop {
            let read = stdout.read(&mut chunk).map_err(ForgeError::Io)?;
            if read == 0 {
                break;
            }
            if bytes_hashed >= max_bytes {
                truncated = true;
                continue;
            }
            let remaining = (max_bytes - bytes_hashed) as usize;
            let take = remaining.min(read);
            hasher.update(&chunk[..take]);
            bytes_hashed = bytes_hashed.saturating_add(take as u64);
            if take < read {
                truncated = true;
            }
        }
        let status = child
            .wait()
            .map_err(|err| ForgeError::Git(format!("git wait failed: {err}")))?;
        let (stderr_bytes, stderr_trunc) = stderr_thread
            .join()
            .map_err(|_| ForgeError::Git("git stderr reader panicked".into()))?;
        let _ = stderr_trunc;
        match status.code() {
            Some(0) | Some(1) => Ok(StreamedDiffDigest {
                digest: format!("{:x}", hasher.finalize()),
                bytes_hashed,
                truncated,
            }),
            _ => {
                let stderr = String::from_utf8_lossy(&stderr_bytes).trim().to_string();
                Err(ForgeError::Git(format_git_command_error(
                    &["diff", "--binary", "--full-index"],
                    if stderr.is_empty() {
                        status.to_string()
                    } else {
                        stderr
                    },
                )))
            }
        }
    }

    /// Commits between baseline and head, oldest first.
    pub fn commit_list(&self, cwd: &Path, baseline: &GitOid, head: &GitOid) -> Result<Vec<GitOid>> {
        let out = self.run(
            cwd,
            &[
                "rev-list",
                "--reverse",
                &format!("{}..{}", baseline.as_str(), head.as_str()),
            ],
        )?;
        Ok(out
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| GitOid::new(l.trim()))
            .collect())
    }

    /// Fetch from a remote (default `origin`). Never force-updates locals.
    pub fn fetch(&self, cwd: &Path, remote: &str) -> Result<String> {
        let remote = if remote.trim().is_empty() {
            "origin"
        } else {
            remote.trim()
        };
        self.run(cwd, &["fetch", "--prune", remote])
    }

    /// Fast-forward only pull of the current branch. Refuses merge commits.
    pub fn pull_ff_only(&self, cwd: &Path, remote: &str) -> Result<String> {
        let remote = if remote.trim().is_empty() {
            "origin"
        } else {
            remote.trim()
        };
        self.run(cwd, &["pull", "--ff-only", remote])
    }

    /// Push the named branch to `remote` without force. Refuses `--force*`.
    pub fn push_branch(&self, cwd: &Path, remote: &str, branch: &str) -> Result<String> {
        let remote = if remote.trim().is_empty() {
            "origin"
        } else {
            remote.trim()
        };
        let branch = branch.trim();
        if branch.is_empty() {
            return Err(ForgeError::Git("push requires a branch name".into()));
        }
        self.run(
            cwd,
            &[
                "push",
                "--set-upstream",
                remote,
                &format!("refs/heads/{branch}"),
            ],
        )
    }

    /// Whether a merge or rebase is in progress in this worktree.
    pub fn merge_in_progress(&self, cwd: &Path) -> bool {
        let Ok(git_dir) = self.run(cwd, &["rev-parse", "--git-dir"]) else {
            return false;
        };
        let git_dir = PathBuf::from(git_dir.trim());
        let root = if git_dir.is_absolute() {
            git_dir
        } else {
            cwd.join(git_dir)
        };
        root.join("MERGE_HEAD").is_file()
            || root.join("rebase-merge").is_dir()
            || root.join("rebase-apply").is_dir()
    }

    /// Bounded commit history newest-first: oid, subject, author, timestamp.
    pub fn log_commits(&self, cwd: &Path, range: &str, limit: usize) -> Result<Vec<CommitSummary>> {
        let limit = limit.clamp(1, 200);
        let out = self.run(
            cwd,
            &[
                "log",
                &format!("--max-count={limit}"),
                "--format=%H%x00%an%x00%ae%x00%at%x00%s",
                range,
            ],
        )?;
        let mut commits = Vec::new();
        for line in out.lines().filter(|l| !l.is_empty()) {
            let mut parts = line.splitn(5, '\0');
            let Some(oid) = parts.next() else { continue };
            let author = parts.next().unwrap_or_default();
            let email = parts.next().unwrap_or_default();
            let ts = parts.next().unwrap_or_default();
            let subject = parts.next().unwrap_or_default();
            commits.push(CommitSummary {
                oid: GitOid::new(oid),
                author_name: author.to_string(),
                author_email: email.to_string(),
                authored_at: ts.parse().unwrap_or(0),
                subject: subject.to_string(),
            });
        }
        Ok(commits)
    }

    /// `git blame --porcelain` for one path, parsed into contiguous hunks.
    pub fn blame(&self, cwd: &Path, path: &str) -> Result<Vec<BlameHunk>> {
        let out = self.run(cwd, &["blame", "--porcelain", "--", path])?;
        Ok(parse_blame_porcelain(&out))
    }

    /// Check out one side of a conflict (`--ours` / `--theirs`) for a path.
    pub fn checkout_conflict_side(&self, cwd: &Path, path: &str, side: &str) -> Result<()> {
        let flag = match side {
            "ours" => "--ours",
            "theirs" => "--theirs",
            _ => {
                return Err(ForgeError::Git(format!(
                    "unsupported conflict side `{side}` (use ours or theirs)"
                )));
            }
        };
        self.run(cwd, &["checkout", flag, "--", path])?;
        Ok(())
    }

    /// Stage a path (`git add -- <path>`), clearing unmerged state after resolve.
    pub fn add_path(&self, cwd: &Path, path: &str) -> Result<()> {
        self.run(cwd, &["add", "--", path])?;
        Ok(())
    }

    /// Drop a path from the index and worktree (`git rm -f` when tracked).
    pub fn remove_path(&self, cwd: &Path, path: &str) -> Result<()> {
        self.run(cwd, &["rm", "-f", "--", path])?;
        Ok(())
    }

    /// `git diff --name-status -z <from> <to>`, parsed as (status, path,
    /// orig_path). Status is the single letter (A/M/D/T) or R/C for
    /// renames/copies.
    pub fn diff_name_status(
        &self,
        cwd: &Path,
        from: &GitOid,
        to: &GitOid,
    ) -> Result<Vec<NameStatus>> {
        let out = self.run_bytes(
            cwd,
            &["diff", "--name-status", "-z", from.as_str(), to.as_str()],
        )?;
        Ok(parse_name_status_z(&out))
    }

    pub fn is_ancestor(&self, cwd: &Path, ancestor: &GitOid, descendant: &GitOid) -> Result<bool> {
        let args = [
            "merge-base",
            "--is-ancestor",
            ancestor.as_str(),
            descendant.as_str(),
        ];
        let (_stdout, stderr, _truncated, status) = self.run_capped(cwd, &args)?;
        match status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => {
                let stderr = String::from_utf8_lossy(&stderr).trim().to_string();
                Err(ForgeError::Git(format_git_command_error(&args, stderr)))
            }
        }
    }

    /// Atomic compare-and-swap ref update — the integration guard. Fails
    /// (without touching the ref) if the ref is not exactly `expected_old`.
    pub fn update_ref_cas(
        &self,
        cwd: &Path,
        ref_name: &str,
        new_oid: &GitOid,
        expected_old: &GitOid,
    ) -> Result<()> {
        self.run(
            cwd,
            &[
                "update-ref",
                ref_name,
                new_oid.as_str(),
                expected_old.as_str(),
            ],
        )?;
        Ok(())
    }

    /// Create one Forge-owned ref only when it does not already exist.
    ///
    /// This is the publication primitive for remote result identities: a
    /// replay may observe the same value, but a stale or colliding attempt can
    /// never replace newer work under the same logical ref.
    pub fn create_forge_ref_cas(&self, cwd: &Path, ref_name: &str, new_oid: &GitOid) -> Result<()> {
        self.validate_forge_ref(cwd, ref_name)?;
        let object_id_width = new_oid.as_str().len();
        if !matches!(object_id_width, 40 | 64)
            || !new_oid
                .as_str()
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(ForgeError::Git(
                "Forge ref target must be a complete Git object id".to_string(),
            ));
        }
        let missing = "0".repeat(object_id_width);
        self.run(
            cwd,
            &["update-ref", ref_name, new_oid.as_str(), missing.as_str()],
        )?;
        Ok(())
    }

    /// Resolve a Forge-owned ref without treating absence as a Git failure.
    pub fn forge_ref_oid(&self, cwd: &Path, ref_name: &str) -> Result<Option<GitOid>> {
        self.validate_forge_ref(cwd, ref_name)?;
        let args = ["rev-parse", "--verify", "--quiet", ref_name];
        let (stdout, stderr, truncated, status) = self.run_capped(cwd, &args)?;
        if truncated {
            return Err(ForgeError::Git(format_git_command_error(
                &args,
                "output exceeded capture budget",
            )));
        }
        match status.code() {
            Some(0) => Ok(Some(GitOid::new(String::from_utf8_lossy(&stdout).trim()))),
            Some(1) => Ok(None),
            _ => Err(ForgeError::Git(format_git_command_error(
                &args,
                String::from_utf8_lossy(&stderr).trim(),
            ))),
        }
    }

    pub fn ref_oid(&self, cwd: &Path, ref_name: &str) -> Result<GitOid> {
        let out = self.run(cwd, &["rev-parse", ref_name])?;
        Ok(GitOid::new(out.trim()))
    }

    fn validate_forge_ref(&self, cwd: &Path, ref_name: &str) -> Result<()> {
        if !ref_name.starts_with("refs/medousa/forge/") {
            return Err(ForgeError::Git(format!(
                "ref is outside Forge's namespace: {ref_name}"
            )));
        }
        self.run(cwd, &["check-ref-format", ref_name])?;
        Ok(())
    }

    /// Move one Forge-owned namespaced ref. User branches are rejected.
    pub fn set_ref(&self, cwd: &Path, ref_name: &str, oid: &GitOid) -> Result<()> {
        self.validate_forge_ref(cwd, ref_name)?;
        self.run(cwd, &["update-ref", ref_name, oid.as_str()])?;
        Ok(())
    }

    pub fn delete_ref(&self, cwd: &Path, ref_name: &str) -> Result<()> {
        self.validate_forge_ref(cwd, ref_name)?;
        self.run(cwd, &["update-ref", "-d", ref_name])?;
        Ok(())
    }

    pub fn tree_oid(&self, cwd: &Path, commit: &GitOid) -> Result<GitOid> {
        let revision = format!("{}^{{tree}}", commit.as_str());
        let out = self.run(cwd, &["rev-parse", "--verify", &revision])?;
        Ok(GitOid::new(out.trim()))
    }

    /// Fingerprint the checkout's real index without changing it.
    pub fn index_tree_oid(&self, cwd: &Path) -> Result<GitOid> {
        let out = self.run(cwd, &["write-tree"])?;
        Ok(GitOid::new(out.trim()))
    }

    /// Compare the working tree to an arbitrary tree through a temporary
    /// index. Unlike `git status`, this does not compare that temporary index
    /// to the checkout's real `HEAD`, and unlike `git add`, it does not write
    /// working files into the repository's object database.
    pub fn status_against_tree(
        &self,
        cwd: &Path,
        baseline: &GitOid,
        temporary_index: &Path,
    ) -> Result<Vec<PorcelainEntry>> {
        if let Some(parent_dir) = temporary_index.parent() {
            std::fs::create_dir_all(parent_dir)?;
        }
        if temporary_index.exists() {
            std::fs::remove_file(temporary_index)?;
        }
        let result = (|| {
            self.run_with_index(cwd, temporary_index, &["read-tree", baseline.as_str()])?;
            self.refresh_index(cwd, temporary_index)?;
            let changed = self.run_with_index_bytes(
                cwd,
                temporary_index,
                &[
                    "diff-index",
                    "--name-status",
                    "-z",
                    "--find-renames",
                    baseline.as_str(),
                    "--",
                ],
            )?;
            let untracked = self.run_with_index_bytes(
                cwd,
                temporary_index,
                &["ls-files", "--others", "--exclude-standard", "-z"],
            )?;
            let mut entries = parse_name_status_z(&changed)
                .into_iter()
                .map(|entry| PorcelainEntry {
                    path: entry.path,
                    kind: match entry.status {
                        'R' | 'C' => PorcelainKind::RenameOrCopy,
                        'U' => PorcelainKind::Unmerged,
                        _ => PorcelainKind::Ordinary,
                    },
                    orig_path: entry.orig_path,
                    xy: Some(format!("{}.", entry.status)),
                })
                .collect::<Vec<_>>();
            entries.extend(
                String::from_utf8_lossy(&untracked)
                    .split('\0')
                    .filter(|path| !path.is_empty())
                    .map(|path| PorcelainEntry {
                        path: path.to_string(),
                        kind: PorcelainKind::Untracked,
                        orig_path: None,
                        xy: None,
                    }),
            );
            entries.sort_by(|left, right| left.path.cmp(&right.path));
            Ok(entries)
        })();
        let _ = std::fs::remove_file(temporary_index);
        result
    }

    /// Materialize the checkout's current non-ignored filesystem state into a
    /// temporary Git index and return its tree object. The real index, HEAD,
    /// branch, and working tree are never changed.
    pub fn write_worktree_tree(
        &self,
        cwd: &Path,
        parent: &GitOid,
        temporary_index: &Path,
        exclude: &[String],
    ) -> Result<GitOid> {
        if let Some(parent_dir) = temporary_index.parent() {
            std::fs::create_dir_all(parent_dir)?;
        }
        if temporary_index.exists() {
            std::fs::remove_file(temporary_index)?;
        }
        let result = (|| {
            self.run_with_index(cwd, temporary_index, &["read-tree", parent.as_str()])?;
            self.run_with_index(cwd, temporary_index, &["add", "-A"])?;
            if !exclude.is_empty() {
                let mut args: Vec<&str> = vec!["reset", "-q", parent.as_str(), "--"];
                args.extend(exclude.iter().map(String::as_str));
                self.run_with_index(cwd, temporary_index, &args)?;
            }
            let tree = self.run_with_index(cwd, temporary_index, &["write-tree"])?;
            Ok(GitOid::new(tree.trim()))
        })();
        let _ = std::fs::remove_file(temporary_index);
        result
    }

    /// Create a commit object representing the checkout without touching its
    /// real index or branch. The returned object remains unreachable until the
    /// caller pins it with a Forge-owned namespaced ref.
    pub fn snapshot_worktree(
        &self,
        cwd: &Path,
        parent: &GitOid,
        temporary_index: &Path,
        message: &str,
        author: &CheckpointAuthor,
        exclude: &[String],
    ) -> Result<GitOid> {
        let tree = self.write_worktree_tree(cwd, parent, temporary_index, exclude)?;
        if tree == self.tree_oid(cwd, parent)? {
            return Ok(parent.clone());
        }
        let mut command = self.command();
        command
            .args([
                "commit-tree",
                tree.as_str(),
                "-p",
                parent.as_str(),
                "-m",
                message,
            ])
            .current_dir(cwd)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_AUTHOR_NAME", &author.name)
            .env("GIT_AUTHOR_EMAIL", &author.email)
            .env("GIT_COMMITTER_NAME", FORGE_COMMITTER_NAME)
            .env("GIT_COMMITTER_EMAIL", FORGE_COMMITTER_EMAIL);
        let args = ["commit-tree"];
        let (stdout, stderr, truncated, status) =
            run_command_bounded(command, MAX_CAPTURE_BYTES)
                .map_err(|err| ForgeError::Git(format_git_spawn_error(&args, &err.to_string())))?;
        if truncated {
            return Err(ForgeError::Git(format_git_command_error(
                &args,
                "output exceeded capture budget",
            )));
        }
        if !status.success() {
            return Err(ForgeError::Git(format_git_command_error(
                &args,
                String::from_utf8_lossy(&stderr).trim(),
            )));
        }
        Ok(GitOid::new(String::from_utf8_lossy(&stdout).trim()))
    }

    /// Export one exact checkpoint and all reachable Git objects as a portable
    /// bundle. The caller persists and content-addresses the resulting file.
    pub fn export_checkpoint_bundle(
        &self,
        cwd: &Path,
        checkpoint: &GitOid,
        destination: &Path,
    ) -> Result<()> {
        if self.head_oid(cwd)? != *checkpoint {
            return Err(ForgeError::EnvironmentDrift(
                "checkpoint bundle head moved before export".to_string(),
            ));
        }
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let destination = destination.to_str().ok_or_else(|| {
            ForgeError::Store("checkpoint bundle path is not valid UTF-8".to_string())
        })?;
        let _ = std::fs::remove_file(destination);
        if let Err(error) = self.run(cwd, &["bundle", "create", destination, "HEAD"]) {
            let _ = std::fs::remove_file(destination);
            return Err(error);
        }
        self.run(cwd, &["bundle", "verify", destination])?;
        Ok(())
    }

    /// Export an arbitrary reachable checkpoint without moving HEAD or a user
    /// branch. Snapshot commits are normally unreachable until this method
    /// temporarily pins one beneath Forge's private namespace.
    pub fn export_reachable_checkpoint_bundle(
        &self,
        cwd: &Path,
        checkpoint: &GitOid,
        destination: &Path,
    ) -> Result<()> {
        let sequence = PORTABLE_BUNDLE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let checkpoint_prefix = checkpoint.as_str().chars().take(16).collect::<String>();
        let temporary_ref = format!(
            "refs/medousa/forge/portable/{}-{sequence}-{checkpoint_prefix}",
            std::process::id()
        );
        self.set_ref(cwd, &temporary_ref, checkpoint)?;
        let result = (|| {
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let destination = destination.to_str().ok_or_else(|| {
                ForgeError::Store("checkpoint bundle path is not valid UTF-8".to_string())
            })?;
            let _ = std::fs::remove_file(destination);
            if let Err(error) = self.run(cwd, &["bundle", "create", destination, &temporary_ref]) {
                let _ = std::fs::remove_file(destination);
                return Err(error);
            }
            self.run(cwd, &["bundle", "verify", destination])?;
            Ok(())
        })();
        let cleanup = self.delete_ref(cwd, &temporary_ref);
        match (result, cleanup) {
            (Err(error), _) => Err(error),
            (Ok(()), Err(error)) => Err(error),
            (Ok(()), Ok(())) => Ok(()),
        }
    }

    /// Import a previously verified checkpoint bundle and detach the worktree
    /// at the exact manifest commit.
    pub fn import_checkpoint_bundle(
        &self,
        cwd: &Path,
        bundle: &Path,
        checkpoint: &GitOid,
    ) -> Result<()> {
        self.import_checkpoint_objects(cwd, bundle, checkpoint)?;
        self.run(cwd, &["checkout", "--detach", checkpoint.as_str()])?;
        if self.head_oid(cwd)? != *checkpoint {
            return Err(ForgeError::EnvironmentDrift(
                "restored checkpoint head does not match its manifest".to_string(),
            ));
        }
        Ok(())
    }

    /// Import a verified checkpoint object graph without touching HEAD, the
    /// index, or any working-tree file. Origin-side reconciliation uses this
    /// before creating a private, compare-and-swap protected result ref.
    pub fn import_checkpoint_objects(
        &self,
        cwd: &Path,
        bundle: &Path,
        checkpoint: &GitOid,
    ) -> Result<()> {
        let bundle = bundle.to_str().ok_or_else(|| {
            ForgeError::Store("checkpoint bundle path is not valid UTF-8".to_string())
        })?;
        self.run(cwd, &["bundle", "verify", bundle])?;
        // `unbundle` imports the complete object graph without depending on a
        // particular advertised ref name. Portable Forge snapshots use an
        // ephemeral private ref while older bundles advertise HEAD.
        self.run(cwd, &["bundle", "unbundle", bundle])?;
        self.resolve_oid(cwd, checkpoint.as_str())?;
        Ok(())
    }

    /// Stage everything (respecting .gitignore) and create a checkpoint commit
    /// with explicit env-var identity. Returns the new head OID, or the
    /// unchanged head when the tree was already clean.
    pub fn commit_checkpoint(
        &self,
        cwd: &Path,
        message: &str,
        author: &CheckpointAuthor,
    ) -> Result<GitOid> {
        self.commit_checkpoint_with_exclusions(cwd, message, author, &[])
    }

    /// Like [`commit_checkpoint`], but un-stages the given normalized git
    /// paths after `add -A` (policy-driven capture exclusions).
    pub fn commit_checkpoint_with_exclusions(
        &self,
        cwd: &Path,
        message: &str,
        author: &CheckpointAuthor,
        exclude: &[String],
    ) -> Result<GitOid> {
        self.run(cwd, &["add", "-A"])?;
        if !exclude.is_empty() {
            let mut args: Vec<&str> = vec!["reset", "-q", "--"];
            args.extend(exclude.iter().map(String::as_str));
            self.run(cwd, &args)?;
        }
        let mut staged = self.command();
        staged
            .args(["diff", "--cached", "--quiet"])
            .current_dir(cwd)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env("GIT_TERMINAL_PROMPT", "0");
        let (_out, _err, _trunc, staged_status) = run_command_bounded(staged, MAX_CAPTURE_BYTES)
            .map_err(|e| {
                ForgeError::Git(format_git_spawn_error(
                    &["diff", "--cached", "--quiet"],
                    &e.to_string(),
                ))
            })?;
        if staged_status.success() {
            return self.head_oid(cwd);
        }
        let mut commit = self.command();
        commit
            .args(["commit", "--no-verify", "-m", message])
            .current_dir(cwd)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_AUTHOR_NAME", &author.name)
            .env("GIT_AUTHOR_EMAIL", &author.email)
            .env("GIT_COMMITTER_NAME", FORGE_COMMITTER_NAME)
            .env("GIT_COMMITTER_EMAIL", FORGE_COMMITTER_EMAIL);
        let (_stdout, stderr, truncated, status) = run_command_bounded(commit, MAX_CAPTURE_BYTES)
            .map_err(|e| {
            ForgeError::Git(format_git_spawn_error(&["commit"], &e.to_string()))
        })?;
        if truncated {
            return Err(ForgeError::Git(format_git_command_error(
                &["commit"],
                "output exceeded capture budget",
            )));
        }
        if !status.success() {
            let stderr = String::from_utf8_lossy(&stderr).trim().to_string();
            return Err(ForgeError::Git(format_git_command_error(
                &["commit"],
                stderr,
            )));
        }
        self.head_oid(cwd)
    }

    /// Read a file's bytes at a given commit (for evidence of binaries /
    /// untracked capture).
    pub fn show_bytes(&self, cwd: &Path, oid: &GitOid, path: &str) -> Result<Vec<u8>> {
        self.run_bytes(cwd, &["show", &format!("{}:{path}", oid.as_str())])
    }

    /// Submodule pins at HEAD of `cwd` (from the index, stage-0 gitlinks).
    pub fn submodule_pins(&self, cwd: &Path, baseline: &GitOid) -> Result<Vec<SubmodulePin>> {
        let out = self.run(cwd, &["ls-files", "--stage"])?;
        let mut pins = Vec::new();
        for line in out.lines() {
            // "160000 <oid> 0\t<path>" marks a gitlink (submodule).
            if let Some(rest) = line.strip_prefix("160000 ") {
                let mut parts = rest.split_whitespace();
                let oid = parts.next().unwrap_or_default();
                let path = line.split('\t').nth(1).unwrap_or_default().to_string();
                let changed = self
                    .show_bytes(cwd, baseline, &path)
                    .map(|b| String::from_utf8_lossy(&b).trim() != oid)
                    .unwrap_or(false);
                pins.push(SubmodulePin {
                    path,
                    oid: GitOid::new(oid),
                    changed,
                });
            }
        }
        Ok(pins)
    }
}

fn safe_worktree_relative_path(value: &str) -> Result<PathBuf> {
    let path = Path::new(value);
    if value.is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(ForgeError::EnvironmentDrift(format!(
            "unsafe attempt worktree path: {value}"
        )));
    }
    Ok(path.to_path_buf())
}

fn require_safe_worktree_parent(root: &Path, parent: &Path) -> Result<()> {
    let relative = parent.strip_prefix(root).map_err(|_| {
        ForgeError::EnvironmentDrift("attempt copy destination escaped its worktree".into())
    })?;
    let mut cursor = root.to_path_buf();
    for component in relative.components() {
        cursor.push(component.as_os_str());
        match std::fs::symlink_metadata(&cursor) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(ForgeError::EnvironmentDrift(format!(
                    "attempt copy destination crosses a symlink: {}",
                    cursor.display()
                )));
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(ForgeError::EnvironmentDrift(format!(
                    "attempt copy destination parent is not a directory: {}",
                    cursor.display()
                )));
            }
            Ok(_) | Err(_) => {}
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameStatus {
    pub status: char,
    pub path: String,
    pub orig_path: Option<String>,
}

pub fn parse_name_status_z(data: &[u8]) -> Vec<NameStatus> {
    let text = String::from_utf8_lossy(data);
    let mut records = text.split('\0').filter(|s| !s.is_empty());
    let mut out = Vec::new();
    while let Some(rec) = records.next() {
        let status = rec.chars().next().unwrap_or('?');
        match status {
            'R' | 'C' => {
                // R<score>\0old\0new\0
                let old = records.next().unwrap_or_default().to_string();
                let new = records.next().unwrap_or_default().to_string();
                out.push(NameStatus {
                    status,
                    path: new,
                    orig_path: Some(old),
                });
            }
            'A' | 'M' | 'D' | 'T' | 'U' => {
                let path = records.next().unwrap_or_default().to_string();
                out.push(NameStatus {
                    status,
                    path,
                    orig_path: None,
                });
            }
            _ => {}
        }
    }
    out
}

/// One commit from `git log` for Changes history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitSummary {
    pub oid: GitOid,
    pub author_name: String,
    pub author_email: String,
    pub authored_at: i64,
    pub subject: String,
}

/// Contiguous blame attribution for one or more lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlameHunk {
    pub oid: GitOid,
    pub author_name: String,
    pub author_email: String,
    pub authored_at: i64,
    pub summary: String,
    pub start_line: u32,
    pub line_count: u32,
}

/// One parsed `git status --porcelain=v2 -z` record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PorcelainEntry {
    pub path: String,
    pub kind: PorcelainKind,
    /// Original path for renames/copies.
    pub orig_path: Option<String>,
    /// Raw two-char XY status for ordinary entries (e.g. ".M", "A.", "M.").
    pub xy: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PorcelainKind {
    Ordinary,
    RenameOrCopy,
    Unmerged,
    Untracked,
    Ignored,
}

/// Branch / upstream tracking from `git status --porcelain=v2 --branch`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BranchTracking {
    /// Checked-out branch name, or `None` when detached / unknown.
    pub head: Option<String>,
    pub oid: Option<String>,
    pub upstream: Option<String>,
    pub ahead: Option<u64>,
    pub behind: Option<u64>,
    pub detached: bool,
}

/// Parse `# branch.*` headers from porcelain v2 (with or without `-z`).
pub fn parse_porcelain_v2_branch(data: &[u8]) -> BranchTracking {
    let text = String::from_utf8_lossy(data);
    let mut tracking = BranchTracking::default();
    for record in text.split(['\0', '\n']).filter(|s| !s.is_empty()) {
        let Some(rest) = record.strip_prefix("# branch.") else {
            continue;
        };
        let mut parts = rest.splitn(2, ' ');
        let key = parts.next().unwrap_or_default();
        let value = parts.next().unwrap_or_default().trim();
        match key {
            "oid" if value != "(initial)" && !value.is_empty() => {
                tracking.oid = Some(value.to_string());
            }
            "head" => {
                if value == "(detached)" {
                    tracking.detached = true;
                    tracking.head = None;
                } else if !value.is_empty() {
                    tracking.head = Some(value.to_string());
                    tracking.detached = false;
                }
            }
            "upstream" if !value.is_empty() => {
                tracking.upstream = Some(value.to_string());
            }
            "ab" => {
                // Format: +<ahead> -<behind>
                let mut ahead = None;
                let mut behind = None;
                for token in value.split_whitespace() {
                    if let Some(n) = token.strip_prefix('+') {
                        ahead = n.parse().ok();
                    } else if let Some(n) = token.strip_prefix('-') {
                        behind = n.parse().ok();
                    }
                }
                tracking.ahead = ahead;
                tracking.behind = behind;
            }
            _ => {}
        }
    }
    tracking
}

pub fn parse_porcelain_v2_z(data: &[u8]) -> Vec<PorcelainEntry> {
    let text = String::from_utf8_lossy(data);
    let mut records = text.split('\0').filter(|s| !s.is_empty());
    let mut entries = Vec::new();
    while let Some(record) = records.next() {
        let mut parts = record.splitn(2, ' ');
        let tag = parts.next().unwrap_or_default();
        match tag {
            "1" => {
                if let Some(entry) = parse_ordinary(record, PorcelainKind::Ordinary) {
                    entries.push(entry);
                }
            }
            "2" => {
                if let Some(mut entry) = parse_ordinary(record, PorcelainKind::RenameOrCopy) {
                    // With -z, the original path is the *next* record.
                    entry.orig_path = records.next().map(str::to_string);
                    entries.push(entry);
                }
            }
            "u" => {
                if let Some(entry) = parse_ordinary(record, PorcelainKind::Unmerged) {
                    entries.push(entry);
                }
            }
            "?" => {
                if let Some(path) = parts.next() {
                    entries.push(PorcelainEntry {
                        path: path.to_string(),
                        kind: PorcelainKind::Untracked,
                        orig_path: None,
                        xy: None,
                    });
                }
            }
            "!" => {
                if let Some(path) = parts.next() {
                    entries.push(PorcelainEntry {
                        path: path.to_string(),
                        kind: PorcelainKind::Ignored,
                        orig_path: None,
                        xy: None,
                    });
                }
            }
            // "#" branch headers: not status entries.
            _ => {}
        }
    }
    entries
}

fn parse_ordinary(record: &str, kind: PorcelainKind) -> Option<PorcelainEntry> {
    let mut it = record.split(' ');
    it.next()?; // tag
    let xy = it.next()?.to_string();
    // Remaining fields are key/value-ish; path is always last.
    let path = record.rsplit(' ').next()?.to_string();
    Some(PorcelainEntry {
        path,
        kind,
        orig_path: None,
        xy: Some(xy),
    })
}

/// Parse `git blame --porcelain` into contiguous hunks.
pub fn parse_blame_porcelain(text: &str) -> Vec<BlameHunk> {
    let mut hunks = Vec::new();
    let mut current_oid = String::new();
    let mut author_name = String::new();
    let mut author_email = String::new();
    let mut authored_at: i64 = 0;
    let mut summary = String::new();
    let mut pending_start: Option<u32> = None;
    let mut pending_count: u32 = 0;
    let mut remaining_in_group: u32 = 0;

    let flush = |hunks: &mut Vec<BlameHunk>,
                 oid: &str,
                 author_name: &str,
                 author_email: &str,
                 authored_at: i64,
                 summary: &str,
                 start: Option<u32>,
                 count: u32| {
        if let Some(start_line) = start.filter(|_| count > 0 && !oid.is_empty()) {
            hunks.push(BlameHunk {
                oid: GitOid::new(oid),
                author_name: author_name.to_string(),
                author_email: author_email.to_string(),
                authored_at,
                summary: summary.to_string(),
                start_line,
                line_count: count,
            });
        }
    };

    for line in text.lines() {
        if line.starts_with('\t') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("author ") {
            author_name = rest.to_string();
            continue;
        }
        if let Some(rest) = line.strip_prefix("author-mail ") {
            author_email = rest
                .trim()
                .trim_start_matches('<')
                .trim_end_matches('>')
                .to_string();
            continue;
        }
        if let Some(rest) = line.strip_prefix("author-time ") {
            authored_at = rest.parse().unwrap_or(0);
            continue;
        }
        if let Some(rest) = line.strip_prefix("summary ") {
            summary = rest.to_string();
            continue;
        }
        // Header: <oid> <orig> <final> [<num>]
        let mut parts = line.split_whitespace();
        let Some(oid) = parts.next() else { continue };
        if oid.len() < 7 || !oid.chars().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }
        let _orig = parts.next();
        let Some(final_line) = parts.next().and_then(|v| v.parse::<u32>().ok()) else {
            continue;
        };
        let group = parts.next().and_then(|v| v.parse::<u32>().ok());
        if let Some(group_size) = group {
            flush(
                &mut hunks,
                &current_oid,
                &author_name,
                &author_email,
                authored_at,
                &summary,
                pending_start,
                pending_count,
            );
            current_oid = oid.to_string();
            pending_start = Some(final_line);
            pending_count = 1;
            remaining_in_group = group_size.saturating_sub(1);
        } else if remaining_in_group > 0 && oid == current_oid {
            pending_count += 1;
            remaining_in_group -= 1;
        } else if oid == current_oid
            && pending_start.is_some_and(|start| final_line == start + pending_count)
        {
            pending_count += 1;
        } else {
            flush(
                &mut hunks,
                &current_oid,
                &author_name,
                &author_email,
                authored_at,
                &summary,
                pending_start,
                pending_count,
            );
            current_oid = oid.to_string();
            pending_start = Some(final_line);
            pending_count = 1;
            remaining_in_group = 0;
        }
    }
    flush(
        &mut hunks,
        &current_oid,
        &author_name,
        &author_email,
        authored_at,
        &summary,
        pending_start,
        pending_count,
    );
    hunks
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn canonicalish(base: &Path, raw: &str) -> PathBuf {
    let p = PathBuf::from(raw);
    let abs = if p.is_absolute() { p } else { base.join(p) };
    std::fs::canonicalize(&abs).unwrap_or(abs)
}

fn sorted_nonempty_lines(output: &str) -> Vec<String> {
    let mut values = output
        .lines()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn init_repo() -> (TempDir, GitEngine, GitOid) {
        let tmp = TempDir::new().unwrap();
        let git = GitEngine::detect().unwrap();
        git.run(tmp.path(), &["init", "-b", "main"]).unwrap();
        fs::write(tmp.path().join("hello.txt"), "hello\n").unwrap();
        git.run(tmp.path(), &["add", "-A"]).unwrap();
        let _ = git
            .commit_checkpoint(tmp.path(), "initial", &CheckpointAuthor::default())
            .unwrap();
        let head = git.head_oid(tmp.path()).unwrap();
        (tmp, git, head)
    }

    #[test]
    fn repository_inspection_resolves_nested_root_and_branch() {
        let (tmp, git, _) = init_repo();
        git.run(tmp.path(), &["branch", "feature/local"]).unwrap();
        let nested = tmp.path().join("src/nested");
        std::fs::create_dir_all(&nested).unwrap();
        assert_eq!(
            git.worktree_root(&nested).unwrap(),
            std::fs::canonicalize(tmp.path()).unwrap()
        );
        assert_eq!(
            git.current_branch(&nested).unwrap().as_deref(),
            Some("main")
        );
        assert_eq!(
            git.suggested_base_ref(&nested).unwrap().as_deref(),
            Some("main")
        );
        assert_eq!(
            git.local_branches(&nested).unwrap(),
            vec!["feature/local".to_string(), "main".to_string()]
        );
    }

    #[test]
    fn remote_branch_listing_separates_remote_and_hides_head_alias() {
        let (tmp, git, head) = init_repo();
        git.run(tmp.path(), &["remote", "add", "origin", "."])
            .unwrap();
        git.run(
            tmp.path(),
            &["update-ref", "refs/remotes/origin/main", head.as_str()],
        )
        .unwrap();
        git.run(
            tmp.path(),
            &[
                "update-ref",
                "refs/remotes/origin/feature/remote",
                head.as_str(),
            ],
        )
        .unwrap();
        git.run(
            tmp.path(),
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/main",
            ],
        )
        .unwrap();

        assert_eq!(git.remote_names(tmp.path()).unwrap(), vec!["origin"]);
        assert_eq!(
            git.remote_branches(tmp.path(), "origin").unwrap(),
            vec!["feature/remote".to_string(), "main".to_string()]
        );
        assert_eq!(
            git.remote_default_branch(tmp.path(), "origin").as_deref(),
            Some("main")
        );
    }

    #[test]
    fn unborn_repository_has_no_base_commit() {
        let tmp = TempDir::new().unwrap();
        let git = GitEngine::detect().unwrap();
        git.run(tmp.path(), &["init", "-b", "master"]).unwrap();

        assert_eq!(
            git.current_branch(tmp.path()).unwrap().as_deref(),
            Some("master")
        );
        assert!(!git.has_commits(tmp.path()).unwrap());
        assert_eq!(git.suggested_base_ref(tmp.path()).unwrap(), None);
        assert!(matches!(
            git.resolve_base_oid(tmp.path(), "master"),
            Err(ForgeError::RepositoryEmpty(_))
        ));
    }

    #[test]
    fn missing_base_is_distinct_from_empty_repository() {
        let (tmp, git, _) = init_repo();
        assert!(matches!(
            git.resolve_base_oid(tmp.path(), "master"),
            Err(ForgeError::BaseRefMissing { reference, .. }) if reference == "master"
        ));
    }

    #[test]
    fn detects_git_and_reports_identity() {
        let (tmp, git, head) = init_repo();
        let identity = git.repo_identity(tmp.path()).unwrap();
        assert!(identity.common_dir.ends_with(".git"));
        assert_eq!(identity.format.as_deref(), Some("sha1"));
        // Same identity from a nested directory.
        let nested = tmp.path().join("sub/dir");
        fs::create_dir_all(&nested).unwrap();
        let identity2 = git.repo_identity(&nested).unwrap();
        assert_eq!(identity.repo_id, identity2.repo_id);
        let _ = head;
    }

    #[test]
    fn porcelain_parses_modified_and_untracked() {
        let (tmp, git, _) = init_repo();
        fs::write(tmp.path().join("hello.txt"), "changed\n").unwrap();
        fs::write(tmp.path().join("new.txt"), "new\n").unwrap();
        fs::create_dir_all(tmp.path().join("deep")).unwrap();
        fs::write(tmp.path().join("deep/nested.txt"), "n\n").unwrap();
        let entries = git.status_porcelain(tmp.path()).unwrap();
        let modified = entries
            .iter()
            .find(|e| e.path == "hello.txt")
            .expect("modified entry");
        assert_eq!(modified.kind, PorcelainKind::Ordinary);
        assert!(
            entries
                .iter()
                .any(|e| e.path == "new.txt" && e.kind == PorcelainKind::Untracked)
        );
        assert!(
            entries
                .iter()
                .any(|e| e.path == "deep/nested.txt" && e.kind == PorcelainKind::Untracked)
        );
    }

    #[test]
    fn porcelain_branch_headers_parse_ahead_behind() {
        let raw = b"# branch.oid abcdef0123456789abcdef0123456789abcdef01\0\
# branch.head feature\0\
# branch.upstream origin/main\0\
# branch.ab +2 -1\0\
1 .M N... 100644 100644 100644 oid oid hello.txt\0";
        let tracking = parse_porcelain_v2_branch(raw);
        assert_eq!(tracking.head.as_deref(), Some("feature"));
        assert_eq!(tracking.upstream.as_deref(), Some("origin/main"));
        assert_eq!(tracking.ahead, Some(2));
        assert_eq!(tracking.behind, Some(1));
        assert!(!tracking.detached);
        let entries = parse_porcelain_v2_z(raw);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "hello.txt");
    }

    #[test]
    fn porcelain_with_branch_reports_current_head() {
        let (tmp, git, _) = init_repo();
        let (tracking, _) = git.status_porcelain_with_branch(tmp.path()).unwrap();
        assert_eq!(tracking.head.as_deref(), Some("main"));
        assert!(!tracking.detached);
        assert!(tracking.upstream.is_none());
    }

    #[test]
    fn diff_path_worktree_and_untracked() {
        let (tmp, git, head) = init_repo();
        fs::write(tmp.path().join("hello.txt"), "changed\n").unwrap();
        let patch = git
            .diff_path_worktree(tmp.path(), &head, "hello.txt")
            .unwrap();
        let text = String::from_utf8_lossy(&patch);
        assert!(text.contains("-hello") || text.contains("-hello\n"));
        assert!(text.contains("+changed"));

        fs::write(tmp.path().join("brand-new.txt"), "fresh\n").unwrap();
        let untracked = git
            .diff_untracked_path(tmp.path(), "brand-new.txt")
            .unwrap();
        let untracked_text = String::from_utf8_lossy(&untracked);
        assert!(untracked_text.contains("+fresh"));
    }

    #[test]
    fn snapshot_worktree_captures_files_without_touching_head_or_index() {
        let (tmp, git, head) = init_repo();
        let scratch = TempDir::new().unwrap();
        fs::write(tmp.path().join("hello.txt"), "staged\n").unwrap();
        git.run(tmp.path(), &["add", "--", "hello.txt"]).unwrap();
        fs::write(tmp.path().join("hello.txt"), "working\n").unwrap();
        fs::write(tmp.path().join("new.txt"), "new\n").unwrap();
        let index_before = git.run(tmp.path(), &["write-tree"]).unwrap();
        let status_before = git
            .run_bytes(
                tmp.path(),
                &["status", "--porcelain=v2", "-z", "--untracked-files=all"],
            )
            .unwrap();
        let temporary_index = scratch.path().join("forge-snapshot.index");

        let snapshot = git
            .snapshot_worktree(
                tmp.path(),
                &head,
                &temporary_index,
                "snapshot",
                &CheckpointAuthor::default(),
                &[],
            )
            .unwrap();

        assert_eq!(git.head_oid(tmp.path()).unwrap(), head);
        assert_eq!(
            git.current_branch(tmp.path()).unwrap().as_deref(),
            Some("main")
        );
        assert_eq!(git.run(tmp.path(), &["write-tree"]).unwrap(), index_before);
        assert_eq!(
            git.run_bytes(
                tmp.path(),
                &["status", "--porcelain=v2", "-z", "--untracked-files=all"],
            )
            .unwrap(),
            status_before
        );
        assert_eq!(
            git.show_bytes(tmp.path(), &snapshot, "hello.txt").unwrap(),
            b"working\n"
        );
        assert_eq!(
            git.show_bytes(tmp.path(), &snapshot, "new.txt").unwrap(),
            b"new\n"
        );
        assert!(!temporary_index.exists());
        assert!(
            git.set_ref(tmp.path(), "refs/heads/main", &snapshot)
                .is_err()
        );
        assert_eq!(git.head_oid(tmp.path()).unwrap(), head);
    }

    #[test]
    fn reachable_snapshot_bundle_reconstructs_without_a_shared_origin() {
        let (tmp, git, head) = init_repo();
        let scratch = TempDir::new().unwrap();
        fs::write(tmp.path().join("hello.txt"), "portable\n").unwrap();
        fs::write(tmp.path().join("new.txt"), "captured\n").unwrap();
        let snapshot = git
            .snapshot_worktree(
                tmp.path(),
                &head,
                &scratch.path().join("snapshot.index"),
                "portable snapshot",
                &CheckpointAuthor::default(),
                &[],
            )
            .unwrap();
        let bundle = scratch.path().join("checkpoint.bundle");
        git.export_reachable_checkpoint_bundle(tmp.path(), &snapshot, &bundle)
            .unwrap();

        let restored = TempDir::new().unwrap();
        git.run(restored.path(), &["init", "--quiet", "--template="])
            .or_else(|_| git.run(restored.path(), &["init", "--quiet"]))
            .unwrap();
        git.import_checkpoint_bundle(restored.path(), &bundle, &snapshot)
            .unwrap();

        assert_eq!(git.head_oid(restored.path()).unwrap(), snapshot);
        assert_eq!(
            fs::read_to_string(restored.path().join("hello.txt")).unwrap(),
            "portable\n"
        );
        assert_eq!(
            fs::read_to_string(restored.path().join("new.txt")).unwrap(),
            "captured\n"
        );
        assert_eq!(git.head_oid(tmp.path()).unwrap(), head);
    }

    #[test]
    fn blame_and_log_summaries() {
        let (tmp, git, head) = init_repo();
        let log = git.log_commits(tmp.path(), "HEAD", 5).unwrap();
        assert!(!log.is_empty());
        assert_eq!(log[0].oid, head);
        assert!(!log[0].subject.is_empty());
        let blame = git.blame(tmp.path(), "hello.txt").unwrap();
        assert!(!blame.is_empty());
        assert_eq!(blame[0].start_line, 1);
        assert!(blame[0].line_count >= 1);
    }

    #[test]
    fn parse_blame_groups_contiguous_lines() {
        let raw = "\
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa 1 1 2
author Ada
author-mail <ada@example.com>
author-time 1700000000
summary first
\tline one
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa 2 2
author Ada
author-mail <ada@example.com>
author-time 1700000000
summary first
\tline two
bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb 3 3 1
author Bea
author-mail <bea@example.com>
author-time 1700000001
summary second
\tline three
";
        let hunks = parse_blame_porcelain(raw);
        assert_eq!(hunks.len(), 2);
        assert_eq!(hunks[0].start_line, 1);
        assert_eq!(hunks[0].line_count, 2);
        assert_eq!(hunks[0].author_name, "Ada");
        assert_eq!(hunks[1].start_line, 3);
        assert_eq!(hunks[1].line_count, 1);
        assert_eq!(hunks[1].summary, "second");
    }

    #[test]
    fn worktree_add_commit_and_remove() {
        let (tmp, git, base) = init_repo();
        let wt = tmp.path().join("wt-1");
        git.worktree_add(tmp.path(), &wt, "worktree/test", &base)
            .unwrap();
        assert!(wt.join("hello.txt").is_file());
        assert!(git.branch_exists(tmp.path(), "worktree/test"));

        fs::write(wt.join("work.txt"), "worked\n").unwrap();
        let sealed = git
            .commit_checkpoint(
                &wt,
                "forge: checkpoint test attempt 1",
                &CheckpointAuthor::default(),
            )
            .unwrap();
        assert_ne!(sealed, base);
        assert!(git.is_clean(&wt).unwrap());

        // Committer is Forge even if the user has global git config.
        let log = git
            .run(&wt, &["log", "-1", "--format=%an|%ae|%cn|%ce"])
            .unwrap();
        assert_eq!(
            log.trim(),
            "Medousa Forge|forge@medousa.local|Medousa Forge|forge@medousa.local"
        );

        git.worktree_remove(tmp.path(), &wt).unwrap();
        assert!(!wt.exists());
        git.branch_delete(tmp.path(), "worktree/test").unwrap();
        assert!(!git.branch_exists(tmp.path(), "worktree/test"));
    }

    #[test]
    fn worktree_fork_preserves_dirty_state_without_mutating_source() {
        let (tmp, git, _base) = init_repo();
        let source = tmp.path();
        fs::write(source.join("hello.txt"), "changed\n").unwrap();
        fs::write(source.join("delete-me.txt"), "delete\n").unwrap();
        git.run(source, &["add", "delete-me.txt"]).unwrap();
        git.commit_checkpoint(source, "add delete target", &CheckpointAuthor::default())
            .unwrap();
        let source_head = git.head_oid(source).unwrap();
        fs::remove_file(source.join("delete-me.txt")).unwrap();
        fs::create_dir_all(source.join("nested")).unwrap();
        fs::write(source.join("nested/new.txt"), "untracked\n").unwrap();
        fs::write(source.join("binary.bin"), [0, 1, 2, 255]).unwrap();

        let destination_root = TempDir::new().unwrap();
        let destination = destination_root.path().join("attempt-wt");
        let fork_head = git
            .worktree_add_from_worktree(source, source, &destination, "worktree/attempt-fork")
            .unwrap();
        assert_eq!(fork_head, source_head);
        assert_eq!(
            fs::read_to_string(destination.join("hello.txt")).unwrap(),
            "changed\n"
        );
        assert!(!destination.join("delete-me.txt").exists());
        assert_eq!(
            fs::read_to_string(destination.join("nested/new.txt")).unwrap(),
            "untracked\n"
        );
        assert_eq!(
            fs::read(destination.join("binary.bin")).unwrap(),
            [0, 1, 2, 255]
        );
        assert_eq!(
            fs::read_to_string(source.join("hello.txt")).unwrap(),
            "changed\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn worktree_fork_rejects_untracked_symlinks_and_cleans_partial_fork() {
        use std::os::unix::fs::symlink;

        let (tmp, git, _) = init_repo();
        symlink("hello.txt", tmp.path().join("link.txt")).unwrap();
        let destination_root = TempDir::new().unwrap();
        let destination = destination_root.path().join("attempt-wt");
        let branch = "worktree/rejected-attempt-fork";

        let error = git
            .worktree_add_from_worktree(tmp.path(), tmp.path(), &destination, branch)
            .unwrap_err();
        assert!(error.to_string().contains("regular file"));
        assert!(!destination.exists());
        assert!(!git.branch_exists(tmp.path(), branch));
        assert!(tmp.path().join("link.txt").is_symlink());
    }

    #[test]
    fn clean_tree_checkpoint_is_noop() {
        let (tmp, git, base) = init_repo();
        let again = git
            .commit_checkpoint(tmp.path(), "forge: noop", &CheckpointAuthor::default())
            .unwrap();
        assert_eq!(again, base);
    }

    #[test]
    fn hash_diff_binary_worktree_streaming_truncates_without_materializing() {
        let (tmp, git, head) = init_repo();
        fs::write(tmp.path().join("hello.txt"), "x".repeat(32 * 1024)).unwrap();
        let digest = git
            .hash_diff_binary_worktree_streaming(tmp.path(), &head, 128)
            .unwrap();
        assert!(digest.truncated);
        assert!(digest.bytes_hashed <= 128);
        assert_eq!(digest.digest.len(), 64);
    }

    #[test]
    fn diff_binary_and_commit_list_between_oids() {
        let (tmp, git, base) = init_repo();
        fs::write(tmp.path().join("hello.txt"), "v2\n").unwrap();
        fs::write(tmp.path().join("added.txt"), "a\n").unwrap();
        let head = git
            .commit_checkpoint(tmp.path(), "second", &CheckpointAuthor::default())
            .unwrap();
        let patch = git.diff_binary(tmp.path(), &base, &head).unwrap();
        let text = String::from_utf8_lossy(&patch);
        assert!(text.contains("hello.txt"));
        assert!(text.contains("added.txt"));
        let commits = git.commit_list(tmp.path(), &base, &head).unwrap();
        assert_eq!(commits, vec![head.clone()]);
    }

    #[test]
    fn checkpoint_bundle_restores_exact_commit_without_source_worktree() {
        let (source, git, _) = init_repo();
        fs::write(source.path().join("portable.txt"), "portable\n").unwrap();
        let checkpoint = git
            .commit_checkpoint(source.path(), "portable", &CheckpointAuthor::default())
            .unwrap();
        let durable = TempDir::new().unwrap();
        let bundle = durable.path().join("checkpoint.bundle");
        git.export_checkpoint_bundle(source.path(), &checkpoint, &bundle)
            .unwrap();

        let restored = TempDir::new().unwrap();
        git.run(restored.path(), &["init", "--quiet"]).unwrap();
        git.import_checkpoint_bundle(restored.path(), &bundle, &checkpoint)
            .unwrap();
        assert_eq!(git.head_oid(restored.path()).unwrap(), checkpoint);
        assert_eq!(
            fs::read_to_string(restored.path().join("portable.txt")).unwrap(),
            "portable\n"
        );
    }

    #[test]
    fn object_only_import_and_create_only_forge_ref_preserve_checkout() {
        let (source, git, _) = init_repo();
        fs::write(source.path().join("portable.txt"), "portable\n").unwrap();
        let checkpoint = git
            .commit_checkpoint(source.path(), "portable", &CheckpointAuthor::default())
            .unwrap();
        let durable = TempDir::new().unwrap();
        let bundle = durable.path().join("checkpoint.bundle");
        git.export_checkpoint_bundle(source.path(), &checkpoint, &bundle)
            .unwrap();

        let (origin, _, origin_head) = init_repo();
        let origin_index = git.index_tree_oid(origin.path()).unwrap();
        let origin_file = fs::read_to_string(origin.path().join("hello.txt")).unwrap();
        git.import_checkpoint_objects(origin.path(), &bundle, &checkpoint)
            .unwrap();
        assert_eq!(git.head_oid(origin.path()).unwrap(), origin_head);
        assert_eq!(git.index_tree_oid(origin.path()).unwrap(), origin_index);
        assert_eq!(
            fs::read_to_string(origin.path().join("hello.txt")).unwrap(),
            origin_file
        );

        let reference = "refs/medousa/forge/remote/work/op/result";
        git.create_forge_ref_cas(origin.path(), reference, &checkpoint)
            .unwrap();
        assert_eq!(
            git.forge_ref_oid(origin.path(), reference).unwrap(),
            Some(checkpoint.clone())
        );
        assert!(
            git.create_forge_ref_cas(origin.path(), reference, &origin_head)
                .is_err()
        );
        assert_eq!(
            git.forge_ref_oid(origin.path(), reference).unwrap(),
            Some(checkpoint)
        );
    }

    #[test]
    fn update_ref_cas_guards_integration() {
        let (tmp, git, base) = init_repo();
        fs::write(tmp.path().join("next.txt"), "n\n").unwrap();
        let head = git
            .commit_checkpoint(tmp.path(), "second", &CheckpointAuthor::default())
            .unwrap();
        // Correct expectation succeeds.
        git.update_ref_cas(tmp.path(), "refs/heads/cas-test", &head, &base)
            .unwrap_err(); // ref doesn't exist at base → must fail, not create
        git.run(tmp.path(), &["branch", "cas-test", base.as_str()])
            .unwrap();
        git.update_ref_cas(tmp.path(), "refs/heads/cas-test", &head, &base)
            .unwrap();
        assert_eq!(
            git.ref_oid(tmp.path(), "refs/heads/cas-test").unwrap(),
            head
        );
        // Stale expectation fails and leaves the ref untouched.
        git.update_ref_cas(tmp.path(), "refs/heads/cas-test", &base, &base)
            .unwrap_err();
        assert_eq!(
            git.ref_oid(tmp.path(), "refs/heads/cas-test").unwrap(),
            head
        );
    }

    #[test]
    fn is_ancestor_orders_commits() {
        let (tmp, git, base) = init_repo();
        fs::write(tmp.path().join("b.txt"), "b\n").unwrap();
        let head = git
            .commit_checkpoint(tmp.path(), "second", &CheckpointAuthor::default())
            .unwrap();
        assert!(git.is_ancestor(tmp.path(), &base, &head).unwrap());
        assert!(!git.is_ancestor(tmp.path(), &head, &base).unwrap());
    }
}
