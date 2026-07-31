//! Policy evaluation and checkpoint-capture governance.
//!
//! Governance is audit, not sandbox. Violations and risks are recorded as
//! evidence; they never prove containment, and runtime sandboxing stays the
//! executor host's responsibility. All matching happens on *normalized git
//! paths* — never arbitrary OS path strings.

use std::path::{Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};

use crate::error::{ForgeError, Result};
use crate::model::{CaptureRisk, ChangedFile, PolicyViolation, PolicyViolationId, WorkPolicy};

/// Normalize to git-path form: forward slashes, no leading `./`, no trailing
/// `/`. `.git` internals can never be governed files — they normalize to a
/// sentinel that matches nothing and is filtered upstream.
pub fn normalize_git_path(raw: &str) -> String {
    let mut p = raw.replace('\\', "/");
    while let Some(rest) = p.strip_prefix("./") {
        p = rest.to_string();
    }
    while p.ends_with('/') && p.len() > 1 {
        p.pop();
    }
    p
}

pub fn is_git_internal(normalized: &str) -> bool {
    normalized == ".git" || normalized.starts_with(".git/")
}

fn build_set(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pat in patterns {
        let glob = Glob::new(pat)
            .map_err(|e| ForgeError::Store(format!("invalid policy glob '{pat}': {e}")))?;
        builder.add(glob);
    }
    builder
        .build()
        .map_err(|e| ForgeError::Store(format!("invalid policy globs: {e}")))
}

/// Evaluate allowed/denied path rules against the changed-file list.
/// Empty `allowed_paths` means everything is allowed (violations are still
/// computed for denied paths and `.git` internals).
pub fn evaluate_paths(policy: &WorkPolicy, changed: &[ChangedFile]) -> Result<Vec<PolicyViolation>> {
    let allowed = build_set(&policy.allowed_paths)?;
    let denied = build_set(&policy.denied_paths)?;
    let allow_all = policy.allowed_paths.is_empty();
    let mut violations = Vec::new();
    for file in changed {
        let path = normalize_git_path(&file.path);
        if is_git_internal(&path) {
            violations.push(PolicyViolation {
                id: PolicyViolationId::new(),
                path,
                rule: "git_internal".into(),
                detail: ".git internals can never be governed files".into(),
            });
            continue;
        }
        if denied.is_match(&path) {
            violations.push(PolicyViolation {
                id: PolicyViolationId::new(),
                path,
                rule: "denied_path".into(),
                detail: "path matched a denied_paths rule".into(),
            });
        } else if !allow_all && !allowed.is_match(&path) {
            violations.push(PolicyViolation {
                id: PolicyViolationId::new(),
                path,
                rule: "not_allowed".into(),
                detail: "path matched no allowed_paths rule".into(),
            });
        }
    }
    Ok(violations)
}

/// Paths excluded from checkpoint capture by policy.
pub fn capture_exclusions(policy: &WorkPolicy, changed: &[ChangedFile]) -> Result<Vec<String>> {
    let excluded = build_set(&policy.checkpoint_exclude_paths)?;
    Ok(changed
        .iter()
        .map(|f| normalize_git_path(&f.path))
        .filter(|p| !is_git_internal(p) && excluded.is_match(p))
        .collect())
}

/// Check capture candidates against size limits and the secret scan.
/// Returns the risks found; the caller decides block-vs-acknowledge.
pub fn assess_capture(
    policy: &WorkPolicy,
    worktree: &Path,
    candidates: &[String],
) -> Result<Vec<CaptureRisk>> {
    let mut risks = Vec::new();
    let mut total: u64 = 0;
    for rel in candidates {
        let path = worktree.join(rel);
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let size = meta.len();
        total = total.saturating_add(size);
        if policy.checkpoint_max_file_bytes > 0 && size > policy.checkpoint_max_file_bytes {
            risks.push(CaptureRisk::OversizeFile {
                path: rel.clone(),
                bytes: size,
                limit: policy.checkpoint_max_file_bytes,
            });
            continue; // don't secret-scan oversize files
        }
        if policy.checkpoint_secret_scan
            && let Some(pattern) = secret_scan_file(&path)?
        {
            risks.push(CaptureRisk::SecretPattern {
                path: rel.clone(),
                pattern,
            });
        }
    }
    if policy.checkpoint_max_total_bytes > 0 && total > policy.checkpoint_max_total_bytes {
        risks.push(CaptureRisk::OversizeTotal {
            bytes: total,
            limit: policy.checkpoint_max_total_bytes,
        });
    }
    Ok(risks)
}

const SECRET_NEEDLES: &[(&str, &str)] = &[
    ("-----BEGIN RSA PRIVATE KEY-----", "rsa_private_key"),
    ("-----BEGIN OPENSSH PRIVATE KEY-----", "openssh_private_key"),
    ("-----BEGIN PRIVATE KEY-----", "pkcs8_private_key"),
    ("-----BEGIN PGP PRIVATE KEY BLOCK-----", "pgp_private_key"),
];

/// Scan a file for likely secrets. Skips binaries (NUL sniff) and files too
/// large to read safely. Returns the pattern name on a hit.
pub fn secret_scan_file(path: &Path) -> Result<Option<String>> {
    const MAX_SCAN: u64 = 8 * 1024 * 1024;
    let meta = std::fs::metadata(path)?;
    if meta.len() > MAX_SCAN {
        return Ok(None);
    }
    let bytes = std::fs::read(path)?;
    if bytes.iter().take(8192).any(|b| *b == 0) {
        return Ok(None); // binary
    }
    let text = String::from_utf8_lossy(&bytes);
    for (needle, name) in SECRET_NEEDLES {
        if text.contains(needle) {
            return Ok(Some((*name).to_string()));
        }
    }
    // High-signal token prefixes.
    for line in text.lines() {
        for token in line.split_whitespace() {
            let t = token.trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '-');
            if t.starts_with("AKIA") && t.len() == 20 {
                return Ok(Some("aws_access_key_id".into()));
            }
            if t.starts_with("ghp_") && t.len() >= 36 {
                return Ok(Some("github_pat".into()));
            }
            if t.starts_with("xoxb-") {
                return Ok(Some("slack_bot_token".into()));
            }
        }
    }
    Ok(None)
}

/// Worktree hygiene scan: symlinks and nested repositories, skipping `.git`.
pub fn scan_worktree(worktree: &Path) -> Result<(Vec<String>, Vec<String>)> {
    let mut symlinks = Vec::new();
    let mut nested = Vec::new();
    walk(worktree, worktree, &mut symlinks, &mut nested)?;
    symlinks.sort();
    nested.sort();
    Ok((symlinks, nested))
}

fn walk(
    root: &Path,
    dir: &Path,
    symlinks: &mut Vec<String>,
    nested: &mut Vec<String>,
) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == ".git" {
            continue;
        }
        let ft = entry.file_type()?;
        if ft.is_symlink() {
            symlinks.push(rel(root, &path));
            continue;
        }
        if ft.is_dir() {
            if path.join(".git").exists() {
                nested.push(rel(root, &path));
                continue; // don't descend into nested repos
            }
            walk(root, &path, symlinks, nested)?;
        }
    }
    Ok(())
}

fn rel(root: &Path, path: &Path) -> String {
    let rel = path
        .strip_prefix(root)
        .map(PathBuf::from)
        .unwrap_or_else(|_| path.to_path_buf());
    normalize_git_path(&rel.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ChangeStatus;
    use tempfile::TempDir;

    fn changed(paths: &[&str]) -> Vec<ChangedFile> {
        paths
            .iter()
            .map(|p| ChangedFile {
                path: (*p).to_string(),
                status: ChangeStatus::Modified,
                old_path: None,
                is_binary: false,
                byte_size: None,
            })
            .collect()
    }

    #[test]
    fn normalizes_git_paths() {
        assert_eq!(normalize_git_path("./src/main.rs"), "src/main.rs");
        assert_eq!(normalize_git_path("src\\win.rs"), "src/win.rs");
        assert_eq!(normalize_git_path("docs/"), "docs");
        assert!(is_git_internal(".git/config"));
        assert!(is_git_internal(".git"));
        assert!(!is_git_internal(".gitignore"));
    }

    #[test]
    fn denied_and_allowed_globs_produce_violations() {
        let policy = WorkPolicy {
            allowed_paths: vec!["src/**".into(), "docs/**".into()],
            denied_paths: vec!["**/*.pem".into(), ".env*".into()],
            ..Default::default()
        };
        let files = changed(&["src/a.rs", "docs/b.md", "scripts/c.sh", "keys/d.pem", ".env.local"]);
        let violations = evaluate_paths(&policy, &files).unwrap();
        let paths: Vec<&str> = violations.iter().map(|v| v.path.as_str()).collect();
        assert!(paths.contains(&"scripts/c.sh")); // not allowed
        assert!(paths.contains(&"keys/d.pem")); // denied
        assert!(paths.contains(&".env.local")); // denied
        assert!(!paths.contains(&"src/a.rs"));
        assert!(!paths.contains(&"docs/b.md"));
    }

    #[test]
    fn git_internals_are_always_violations() {
        let policy = WorkPolicy::default();
        let files = changed(&[".git/hooks/evil"]);
        let violations = evaluate_paths(&policy, &files).unwrap();
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].rule, "git_internal");
    }

    #[test]
    fn capture_limits_and_secret_scan() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("small.txt"), "hi").unwrap();
        std::fs::write(tmp.path().join("big.bin"), vec![0u8; 4096]).unwrap();
        std::fs::write(
            tmp.path().join("key.pem"),
            "-----BEGIN RSA PRIVATE KEY-----\nMII...\n-----END RSA PRIVATE KEY-----\n",
        )
        .unwrap();

        let policy = WorkPolicy {
            checkpoint_max_file_bytes: 1024,
            ..Default::default()
        };
        let risks = assess_capture(
            &policy,
            tmp.path(),
            &["small.txt".into(), "big.bin".into(), "key.pem".into()],
        )
        .unwrap();
        assert!(risks.iter().any(|r| matches!(
            r,
            CaptureRisk::OversizeFile { path, .. } if path == "big.bin"
        )));
        assert!(risks.iter().any(|r| matches!(
            r,
            CaptureRisk::SecretPattern { path, pattern } if path == "key.pem" && pattern == "rsa_private_key"
        )));
        // small.txt is clean.
        assert!(!risks.iter().any(|r| matches!(
            r,
            CaptureRisk::OversizeFile { path, .. } if path == "small.txt"
        )));
    }

    #[test]
    fn total_limit_aggregates() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("a"), vec![0u8; 700]).unwrap();
        std::fs::write(tmp.path().join("b"), vec![0u8; 700]).unwrap();
        let policy = WorkPolicy {
            checkpoint_max_total_bytes: 1000,
            checkpoint_secret_scan: false,
            ..Default::default()
        };
        let risks = assess_capture(&policy, tmp.path(), &["a".into(), "b".into()]).unwrap();
        assert!(risks
            .iter()
            .any(|r| matches!(r, CaptureRisk::OversizeTotal { bytes: 1400, limit: 1000 })));
    }

    #[cfg(unix)]
    #[test]
    fn worktree_scan_finds_symlinks_and_nested_repos() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("real.txt"), "x").unwrap();
        std::os::unix::fs::symlink(tmp.path().join("real.txt"), tmp.path().join("link.txt"))
            .unwrap();
        std::fs::create_dir_all(tmp.path().join("vendor/dep/.git")).unwrap();
        std::fs::create_dir_all(tmp.path().join(".git")).unwrap(); // root .git is skipped
        let (symlinks, nested) = scan_worktree(tmp.path()).unwrap();
        assert_eq!(symlinks, vec!["link.txt"]);
        assert_eq!(nested, vec!["vendor/dep"]);
    }
}
