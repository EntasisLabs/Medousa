//! Multi-vault root registry (Phase 3).

use std::path::PathBuf;
use std::sync::OnceLock;

#[cfg(feature = "full-daemon")]
use anyhow::Context;
use anyhow::{Result, bail};

#[cfg(feature = "full-daemon")]
use crate::load_product_config;
#[cfg(feature = "full-daemon")]
use crate::product_config::{VaultProductConfig, VaultRootEntry, save_product_config};
#[cfg(feature = "full-daemon")]
use crate::vault::path::VaultPath;
#[cfg(feature = "full-daemon")]
use crate::vault::path::vault_capability_for_root;
#[cfg(feature = "full-daemon")]
use crate::vault::store::vault_store;

pub const DEFAULT_VAULT_ROOT_ID: &str = "personal";

static DEPLOYMENT_VAULT_ROOT: OnceLock<PathBuf> = OnceLock::new();

/// Bind the daemon's default vault to a deployment-owned filesystem root.
/// Embedded hosts call this during boot before the lazy vault store opens.
pub fn configure_deployment_vault_root(root: PathBuf) -> Result<()> {
    if let Some(existing) = DEPLOYMENT_VAULT_ROOT.get() {
        if existing == &root {
            return Ok(());
        }
        bail!(
            "vault root already configured for another daemon deployment: {}",
            existing.display()
        );
    }
    DEPLOYMENT_VAULT_ROOT
        .set(root)
        .map_err(|_| anyhow::anyhow!("vault root configuration raced"))
}

pub fn deployment_vault_root_configured() -> bool {
    DEPLOYMENT_VAULT_ROOT.get().is_some()
}

mod root_override {
    use std::cell::RefCell;
    use std::path::PathBuf;

    thread_local! {
        static OVERRIDE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    }

    pub fn set(path: Option<PathBuf>) {
        OVERRIDE.with(|cell| *cell.borrow_mut() = path);
    }

    pub fn get() -> Option<PathBuf> {
        OVERRIDE.with(|cell| cell.borrow().clone())
    }
}

/// Redirect active vault root for tests and retained harnesses (P06).
/// Does not touch product config. Clear with `None` after use.
pub fn set_test_vault_root_override(path: Option<PathBuf>) {
    root_override::set(path);
}

#[cfg(feature = "full-daemon")]
pub fn default_vault_roots() -> Vec<VaultRootEntry> {
    vec![VaultRootEntry {
        id: DEFAULT_VAULT_ROOT_ID.to_string(),
        label: "Personal".to_string(),
        path: None,
    }]
}

#[cfg(feature = "full-daemon")]
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

#[cfg(feature = "full-daemon")]
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
    if let Some(path) = root_override::get() {
        return path;
    }
    if let Some(path) = DEPLOYMENT_VAULT_ROOT.get() {
        return path.clone();
    }
    #[cfg(feature = "full-daemon")]
    {
        let config = normalize_vault_config(&load_product_config().vault);
        let entry = config
            .roots
            .iter()
            .find(|root| root.id == config.active_root_id)
            .cloned()
            .unwrap_or_else(|| default_vault_roots().remove(0));
        resolve_root_path(&entry)
    }
    #[cfg(not(feature = "full-daemon"))]
    panic!("embedded vault root must be configured before use")
}

pub fn list_vault_root_views() -> crate::daemon_api::VaultRootsResponse {
    if let Some(path) = DEPLOYMENT_VAULT_ROOT.get() {
        return crate::daemon_api::VaultRootsResponse {
            active_root_id: DEFAULT_VAULT_ROOT_ID.to_string(),
            roots: vec![crate::daemon_api::VaultRootView {
                id: DEFAULT_VAULT_ROOT_ID.to_string(),
                label: "Personal".to_string(),
                path: path.display().to_string(),
                is_default: true,
                active: true,
                is_obsidian: false,
            }],
        };
    }
    #[cfg(feature = "full-daemon")]
    {
        let config = normalize_vault_config(&load_product_config().vault);
        let roots = config
            .roots
            .iter()
            .map(|entry| {
                let absolute = resolve_root_path(entry);
                let is_default = entry
                    .path
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .is_none();
                crate::daemon_api::VaultRootView {
                    id: entry.id.clone(),
                    label: entry.label.clone(),
                    path: absolute.display().to_string(),
                    is_default,
                    active: entry.id == config.active_root_id,
                    is_obsidian: vault_capability_for_root(absolute.clone())
                        .and_then(|root| {
                            root.is_dir(&VaultPath::internal(".obsidian")?)
                                .map_err(Into::into)
                        })
                        .unwrap_or(false),
                }
            })
            .collect();
        crate::daemon_api::VaultRootsResponse {
            active_root_id: config.active_root_id,
            roots,
        }
    }
    #[cfg(not(feature = "full-daemon"))]
    unreachable!("embedded vault root must be configured before use")
}

/// True when the active vault root is an external (non-default) folder and/or Obsidian.
pub fn active_root_skips_auto_workshop_tags() -> bool {
    if DEPLOYMENT_VAULT_ROOT.get().is_some() {
        return false;
    }
    #[cfg(feature = "full-daemon")]
    let config = normalize_vault_config(&load_product_config().vault);
    #[cfg(feature = "full-daemon")]
    let entry = config
        .roots
        .iter()
        .find(|root| root.id == config.active_root_id)
        .cloned()
        .unwrap_or_else(|| default_vault_roots().remove(0));
    #[cfg(feature = "full-daemon")]
    let is_external = entry
        .path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some();
    #[cfg(feature = "full-daemon")]
    if is_external {
        return true;
    }
    #[cfg(feature = "full-daemon")]
    return vault_capability_for_root(resolve_root_path(&entry))
        .and_then(|root| {
            root.is_dir(&VaultPath::internal(".obsidian")?)
                .map_err(Into::into)
        })
        .unwrap_or(false);
    #[cfg(not(feature = "full-daemon"))]
    false
}

pub fn set_active_vault_root(root_id: &str) -> Result<crate::daemon_api::VaultRootsResponse> {
    if DEPLOYMENT_VAULT_ROOT.get().is_some() {
        if root_id.trim() == DEFAULT_VAULT_ROOT_ID {
            return Ok(list_vault_root_views());
        }
        bail!("embedded deployment owns one app-sandbox vault root");
    }
    #[cfg(not(feature = "full-daemon"))]
    bail!("embedded vault root is not configured");
    #[cfg(feature = "full-daemon")]
    {
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
}

pub fn add_vault_root(
    label: &str,
    path: &str,
    id: Option<&str>,
) -> Result<crate::daemon_api::VaultRootsResponse> {
    if DEPLOYMENT_VAULT_ROOT.get().is_some() {
        bail!("embedded deployment cannot add an external vault root");
    }
    #[cfg(not(feature = "full-daemon"))]
    {
        let _ = (label, path, id);
        bail!("embedded vault root is not configured");
    }
    #[cfg(feature = "full-daemon")]
    {
        let trimmed_label = label.trim();
        if trimmed_label.is_empty() {
            bail!("label is required");
        }

        let absolute = normalize_vault_root_path(path)?;
        vault_capability_for_root(absolute.clone())
            .with_context(|| format!("create vault root directory {}", absolute.display()))?;

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
}

#[cfg(feature = "full-daemon")]
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

#[cfg(feature = "full-daemon")]
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

#[cfg(feature = "full-daemon")]
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

#[cfg(all(test, feature = "full-daemon"))]
mod tests {
    use super::*;
    use std::fs;

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

    #[test]
    fn detects_obsidian_dir_on_root_path() {
        let dir =
            std::env::temp_dir().join(format!("medousa-obsidian-detect-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join(".obsidian")).expect("obsidian");
        let entry = VaultRootEntry {
            id: "obs".to_string(),
            label: "Notes".to_string(),
            path: Some(dir.display().to_string()),
        };
        assert!(resolve_root_path(&entry).join(".obsidian").is_dir());
        let _ = fs::remove_dir_all(&dir);
    }
}
