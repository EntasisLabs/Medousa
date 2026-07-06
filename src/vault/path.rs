//! Vault path normalization and root resolution.

use std::path::{Component, Path, PathBuf};

use anyhow::{Result, bail};

use crate::vault::roots::active_vault_root;

/// Active user vault root (multi-vault aware).
pub fn user_vault_root() -> PathBuf {
    active_vault_root()
}

/// Optional project overlay: `{root}/.medousa/vault/`.
pub fn project_vault_overlay_root() -> Option<PathBuf> {
    project_root().map(|root| root.join(".medousa").join("vault"))
}

fn project_root() -> Option<PathBuf> {
    if let Ok(raw) = std::env::var("MEDOUSA_PROJECT_ROOT") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }

    std::env::current_dir().ok()
}

pub fn normalize_vault_path(raw: &str) -> Result<String> {
    let trimmed = raw.trim().trim_start_matches('/');
    if trimmed.is_empty() {
        bail!("vault path is required");
    }
    if trimmed.contains('\\') {
        bail!("vault path must use forward slashes");
    }
    if trimmed.contains("..") {
        bail!("vault path must not contain '..'");
    }
    if trimmed.starts_with('.') {
        bail!("vault path must not start with '.'");
    }

    let mut normalized = Vec::new();
    for segment in trimmed.split('/') {
        let segment = segment.trim();
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." {
            bail!("vault path must not contain '..'");
        }
        normalized.push(segment);
    }
    if normalized.is_empty() {
        bail!("vault path is required");
    }
    Ok(normalized.join("/"))
}

pub fn resolve_user_note_path(relative: &str) -> Result<PathBuf> {
    let normalized = normalize_vault_path(relative)?;
    let root = user_vault_root();
    let absolute = root.join(&normalized);
    ensure_within_root(&root, &absolute)?;
    Ok(absolute)
}

pub fn resolve_overlay_note_path(relative: &str) -> Result<Option<PathBuf>> {
    let Some(overlay_root) = project_vault_overlay_root() else {
        return Ok(None);
    };
    let normalized = normalize_vault_path(relative)?;
    let absolute = overlay_root.join(&normalized);
    ensure_within_root(&overlay_root, &absolute)?;
    if absolute.is_file() {
        Ok(Some(absolute))
    } else {
        Ok(None)
    }
}

pub fn trash_path_for(relative: &str) -> Result<PathBuf> {
    let normalized = normalize_vault_path(relative)?;
    let root = user_vault_root();
    let trash = root.join(".trash").join(&normalized);
    ensure_within_root(&root, &trash)?;
    Ok(trash)
}

fn ensure_within_root(root: &Path, candidate: &Path) -> Result<()> {
    for component in candidate.components() {
        if matches!(component, Component::ParentDir) {
            bail!("vault path escapes root");
        }
    }

    let root_abs = absolutize_path_for_check(root);
    let enclosed = candidate
        .parent()
        .map(absolutize_path_for_check)
        .unwrap_or_else(|| absolutize_path_for_check(candidate));

    if !path_starts_with(&enclosed, &root_abs) {
        bail!("vault path escapes root");
    }
    Ok(())
}

/// Resolve to an absolute path for containment checks without requiring the leaf to exist.
fn absolutize_path_for_check(path: &Path) -> PathBuf {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    strip_verbatim_prefix(absolute)
}

#[cfg(windows)]
fn strip_verbatim_prefix(path: PathBuf) -> PathBuf {
    let display = path.display().to_string();
    if let Some(rest) = display.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path
    }
}

#[cfg(not(windows))]
fn strip_verbatim_prefix(path: PathBuf) -> PathBuf {
    path
}

fn path_starts_with(path: &Path, prefix: &Path) -> bool {
    let mut path_components = path.components();
    let mut prefix_components = prefix.components();

    loop {
        match (prefix_components.next(), path_components.next()) {
            (None, _) => return true,
            (Some(prefix_component), Some(path_component)) => {
                if !path_component_eq(path_component, prefix_component) {
                    return false;
                }
            }
            (Some(_), None) => return false,
        }
    }
}

fn path_component_eq(left: Component<'_>, right: Component<'_>) -> bool {
    match (left, right) {
        (Component::Prefix(left), Component::Prefix(right)) => {
            left.as_os_str().eq_ignore_ascii_case(right.as_os_str())
        }
        (Component::RootDir, Component::RootDir) => true,
        (Component::CurDir, Component::CurDir) => true,
        (Component::Normal(left), Component::Normal(right)) => {
            left.eq_ignore_ascii_case(right)
        }
        (left, right) => left.as_os_str() == right.as_os_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn normalize_rejects_traversal() {
        assert!(normalize_vault_path("../secret").is_err());
        assert!(normalize_vault_path("journal/2026-05-30.md").is_ok());
    }

    #[test]
    fn ensure_within_root_allows_nested_note_before_parent_exists() {
        let base = std::env::temp_dir().join(format!(
            "medousa-vault-path-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let root = base.join("vault");
        fs::create_dir_all(&root).expect("vault root");
        let note = root.join("journal").join("2026-05-30.md");

        ensure_within_root(&root, &note).expect("nested note should stay inside root");

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn ensure_within_root_rejects_paths_outside_root() {
        let base = std::env::temp_dir().join(format!(
            "medousa-vault-path-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let root = base.join("vault");
        fs::create_dir_all(&root).expect("vault root");
        let outside = base.join("outside.md");

        assert!(ensure_within_root(&root, &outside).is_err());

        let _ = fs::remove_dir_all(base);
    }
}
