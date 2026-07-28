//! Subprocess Git engine — Forge's only Git implementation. CWD-parameterized
//! (never mutates global git state), explicit OID comparisons (never symbolic
//! merge-base games for evidence), and explicit checkpoint identity via env
//! vars (Forge never impersonates the user).

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{ForgeError, Result};
use crate::model::{GitOid, RepoId, RepoIdentity, SubmodulePin};

/// Committer identity stamped on every Forge-created commit. Authorship may
/// be attributed to the executor's identity where known; committer is always
/// recognizably Forge.
pub const FORGE_COMMITTER_NAME: &str = "Medousa Forge";
pub const FORGE_COMMITTER_EMAIL: &str = "forge@medousa.local";

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

    pub(crate) fn run(&self, cwd: &Path, args: &[&str]) -> Result<String> {
        let output = Command::new(&self.git)
            .args(args)
            .current_dir(cwd)
            // Defensive hygiene: a Forge subprocess must never inherit a
            // foreign repo context or block on a credential prompt.
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .map_err(|e| ForgeError::Git(format!("failed to spawn git {}: {e}", args.join(" "))))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let detail = if !stderr.is_empty() { stderr } else { stdout };
            return Err(ForgeError::Git(format!(
                "git {} failed: {}",
                args.join(" "),
                if detail.is_empty() {
                    output.status.to_string()
                } else {
                    detail
                }
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn run_bytes(&self, cwd: &Path, args: &[&str]) -> Result<Vec<u8>> {
        let output = Command::new(&self.git)
            .args(args)
            .current_dir(cwd)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .map_err(|e| ForgeError::Git(format!("failed to spawn git {}: {e}", args.join(" "))))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(ForgeError::Git(format!(
                "git {} failed: {}",
                args.join(" "),
                if stderr.is_empty() {
                    output.status.to_string()
                } else {
                    stderr
                }
            )));
        }
        Ok(output.stdout)
    }

    /// True when `path` is inside a Git repository.
    pub fn is_repo(&self, path: &Path) -> bool {
        self.run(path, &["rev-parse", "--is-inside-work-tree"])
            .map(|s| s.trim() == "true")
            .unwrap_or(false)
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
        let out = self.run(cwd, &["rev-parse", &format!("{rev}^{{commit}}")])?;
        Ok(GitOid::new(out.trim()))
    }

    pub fn head_oid(&self, cwd: &Path) -> Result<GitOid> {
        self.resolve_oid(cwd, "HEAD")
    }

    /// Add a worktree at `path` on a *new* branch `branch` starting at
    /// `baseline`. `repo_cwd` is any directory inside the repository.
    pub fn worktree_add(&self, repo_cwd: &Path, path: &Path, branch: &str, baseline: &GitOid) -> Result<()> {
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
        let out = self.run_bytes(cwd, &["status", "--porcelain=v2", "-z", "--untracked-files=all"])?;
        Ok(parse_porcelain_v2_z(&out))
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

    /// `git diff --binary <baseline>` against the *working tree* (uncommitted
    /// state), used for pre-checkpoint inspection.
    pub fn diff_binary_worktree(&self, cwd: &Path, baseline: &GitOid) -> Result<Vec<u8>> {
        self.run_bytes(cwd, &["diff", "--binary", "--full-index", baseline.as_str()])
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
        let status = Command::new(&self.git)
            .args(["merge-base", "--is-ancestor", ancestor.as_str(), descendant.as_str()])
            .current_dir(cwd)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .map_err(|e| ForgeError::Git(format!("failed to spawn git merge-base: {e}")))?;
        match status.status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => {
                let stderr = String::from_utf8_lossy(&status.stderr).trim().to_string();
                Err(ForgeError::Git(format!("git merge-base failed: {stderr}")))
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

    pub fn ref_oid(&self, cwd: &Path, ref_name: &str) -> Result<GitOid> {
        let out = self.run(cwd, &["rev-parse", ref_name])?;
        Ok(GitOid::new(out.trim()))
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
        let staged = Command::new(&self.git)
            .args(["diff", "--cached", "--quiet"])
            .current_dir(cwd)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .map_err(|e| ForgeError::Git(format!("failed to spawn git diff: {e}")))?;
        if staged.status.success() {
            return self.head_oid(cwd);
        }
        let output = Command::new(&self.git)
            .args(["commit", "--no-verify", "-m", message])
            .current_dir(cwd)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_AUTHOR_NAME", &author.name)
            .env("GIT_AUTHOR_EMAIL", &author.email)
            .env("GIT_COMMITTER_NAME", FORGE_COMMITTER_NAME)
            .env("GIT_COMMITTER_EMAIL", FORGE_COMMITTER_EMAIL)
            .output()
            .map_err(|e| ForgeError::Git(format!("failed to spawn git commit: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(ForgeError::Git(format!("git commit failed: {stderr}")));
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
        assert!(entries
            .iter()
            .any(|e| e.path == "new.txt" && e.kind == PorcelainKind::Untracked));
        assert!(entries
            .iter()
            .any(|e| e.path == "deep/nested.txt" && e.kind == PorcelainKind::Untracked));
    }

    #[test]
    fn worktree_add_commit_and_remove() {
        let (tmp, git, base) = init_repo();
        let wt = tmp.path().join("wt-1");
        git.worktree_add(tmp.path(), &wt, "medousa/work/test", &base).unwrap();
        assert!(wt.join("hello.txt").is_file());
        assert!(git.branch_exists(tmp.path(), "medousa/work/test"));

        fs::write(wt.join("work.txt"), "worked\n").unwrap();
        let sealed = git
            .commit_checkpoint(&wt, "forge: checkpoint test attempt 1", &CheckpointAuthor::default())
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
        git.branch_delete(tmp.path(), "medousa/work/test").unwrap();
        assert!(!git.branch_exists(tmp.path(), "medousa/work/test"));
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
        assert_eq!(git.ref_oid(tmp.path(), "refs/heads/cas-test").unwrap(), head);
        // Stale expectation fails and leaves the ref untouched.
        git.update_ref_cas(tmp.path(), "refs/heads/cas-test", &base, &base)
            .unwrap_err();
        assert_eq!(git.ref_oid(tmp.path(), "refs/heads/cas-test").unwrap(), head);
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
