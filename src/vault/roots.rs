//! Multi-vault root registry (Phase 3).

use std::path::PathBuf;

use anyhow::{Context, Result, bail};

use crate::load_product_config;
use crate::product_config::{VaultProductConfig, VaultRootEntry, save_product_config};
use crate::vault::store::vault_store;

pub const DEFAULT_VAULT_ROOT_ID: &str = "personal";

pub fn default_vault_roots() -> Vec<VaultRootEntry> {
    vec![VaultRootEntry {
        id: DEFAULT_VAULT_ROOT_ID.to_string(),
        label: "Personal".to_string(),
        path: None,
    }]
}

pub fn normalize_vault_config(vault: &VaultProductConfig) -> VaultProductConfig {
    let mut normalized = vault.clone();
    if normalized.roots.is_empty() {
        normalized.roots = default_vault_roots();
    }
    if normalized.active_root_id.trim().is_empty()
        || !normalized
            .roots
            .iter()
            .any(|root| root.id == normalized.active_root_id)
    {
        normalized.active_root_id = normalized
            .roots
            .first()
            .map(|root| root.id.clone())
            .unwrap_or_else(|| DEFAULT_VAULT_ROOT_ID.to_string());
    }
    normalized
}

pub fn resolve_root_path(entry: &VaultRootEntry) -> PathBuf {
    match entry
        .path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(path) => PathBuf::from(path),
        None => crate::paths::user_vault_root(),
    }
}

pub fn active_vault_root() -> PathBuf {
    let config = normalize_vault_config(&load_product_config().vault);
    let entry = config
        .roots
        .iter()
        .find(|root| root.id == config.active_root_id)
        .cloned()
        .unwrap_or_else(|| default_vault_roots().remove(0));
    resolve_root_path(&entry)
}

pub fn list_vault_root_views() -> crate::daemon_api::VaultRootsResponse {
    let config = normalize_vault_config(&load_product_config().vault);
    let roots = config
        .roots
        .iter()
        .map(|entry| {
            let is_default = entry
                .path
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_none();
            crate::daemon_api::VaultRootView {
                id: entry.id.clone(),
                label: entry.label.clone(),
                path: resolve_root_path(entry).display().to_string(),
                is_default,
                active: entry.id == config.active_root_id,
            }
        })
        .collect();
    crate::daemon_api::VaultRootsResponse {
        active_root_id: config.active_root_id,
        roots,
    }
}

pub fn set_active_vault_root(root_id: &str) -> Result<crate::daemon_api::VaultRootsResponse> {
    let trimmed = root_id.trim();
    if trimmed.is_empty() {
        bail!("root_id is required");
    }

    let mut product = load_product_config();
    let vault = normalize_vault_config(&product.vault);
    if !vault.roots.iter().any(|root| root.id == trimmed) {
        bail!("vault root not found: {trimmed}");
    }

    product.vault = vault;
    product.vault.active_root_id = trimmed.to_string();
    save_product_config(&product).context("save product config")?;
    vault_store()
        .refresh_from_disk()
        .context("refresh vault after root switch")?;
    Ok(list_vault_root_views())
}

pub fn add_vault_root(
    label: &str,
    path: &str,
    id: Option<&str>,
) -> Result<crate::daemon_api::VaultRootsResponse> {
    let trimmed_label = label.trim();
    if trimmed_label.is_empty() {
        bail!("label is required");
    }

    let absolute = normalize_vault_root_path(path)?;
    std::fs::create_dir_all(&absolute).with_context(|| {
        format!(
            "create vault root directory {}",
            absolute.display()
        )
    })?;

    let root_id = match id.map(str::trim).filter(|value| !value.is_empty()) {
        Some(explicit) => validate_vault_root_id(explicit)?,
        None => slugify_vault_root_id(trimmed_label),
    };

    let mut product = load_product_config();
    let mut vault = normalize_vault_config(&product.vault);
    if vault.roots.iter().any(|root| root.id == root_id) {
        bail!("vault root id already exists: {root_id}");
    }

    vault.roots.push(VaultRootEntry {
        id: root_id,
        label: trimmed_label.to_string(),
        path: Some(absolute.display().to_string()),
    });
    product.vault = vault;
    save_product_config(&product).context("save product config")?;
    Ok(list_vault_root_views())
}

fn normalize_vault_root_path(raw: &str) -> Result<PathBuf> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!("path is required");
    }
    let path = PathBuf::from(trimmed);
    if !path.is_absolute() {
        bail!("vault root path must be absolute");
    }
    Ok(path)
}

fn validate_vault_root_id(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!("vault root id is required");
    }
    if trimmed == DEFAULT_VAULT_ROOT_ID {
        bail!("reserved vault root id: {DEFAULT_VAULT_ROOT_ID}");
    }
    if !trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        bail!("vault root id must use letters, numbers, hyphens, or underscores");
    }
    Ok(trimmed.to_string())
}

fn slugify_vault_root_id(label: &str) -> String {
    let slug: String = label
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let collapsed = slug
        .split('-')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if collapsed.is_empty() {
        "vault".to_string()
    } else {
        collapsed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_fills_default_root() {
        let normalized = normalize_vault_config(&VaultProductConfig::default());
        assert_eq!(normalized.roots.len(), 1);
        assert_eq!(normalized.active_root_id, DEFAULT_VAULT_ROOT_ID);
    }

    #[test]
    fn slugify_strips_spaces() {
        assert_eq!(slugify_vault_root_id("Work Notes"), "work-notes");
    }
}
