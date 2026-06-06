//! Vault path normalization and root resolution.

use std::path::{Component, Path, PathBuf};

use anyhow::{Result, bail};

use crate::session;

/// Default user vault root: `~/.local/share/medousa/vault/`.
pub fn user_vault_root() -> PathBuf {
    session::medousa_data_dir().join("vault")
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
    let root = root
        .canonicalize()
        .unwrap_or_else(|_| root.to_path_buf());
    let parent = candidate
        .parent()
        .map(|path| path.to_path_buf())
        .unwrap_or_else(|| candidate.to_path_buf());
    let parent = parent.canonicalize().unwrap_or(parent);
    if !parent.starts_with(&root) {
        bail!("vault path escapes root");
    }

    for component in candidate.components() {
        if matches!(component, Component::ParentDir) {
            bail!("vault path escapes root");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_rejects_traversal() {
        assert!(normalize_vault_path("../secret").is_err());
        assert!(normalize_vault_path("journal/2026-05-30.md").is_ok());
    }
}
