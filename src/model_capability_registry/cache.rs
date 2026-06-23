use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::types::{CatalogIndex, ProviderCatalogSnapshot};

pub fn catalog_dir() -> PathBuf {
    crate::session::medousa_data_dir().join("model_catalog")
}

fn index_path() -> PathBuf {
    catalog_dir().join("index.json")
}

fn provider_path(provider: &str) -> PathBuf {
    catalog_dir().join(format!("{}.json", provider.trim().to_ascii_lowercase()))
}

pub fn load_index() -> CatalogIndex {
    read_json(&index_path()).unwrap_or_default()
}

pub fn save_index(index: &CatalogIndex) -> Result<()> {
    write_json(&index_path(), index)
}

pub fn load_provider_snapshot(provider: &str) -> Option<ProviderCatalogSnapshot> {
    read_json(&provider_path(provider)).ok()
}

pub fn save_provider_snapshot(snapshot: &ProviderCatalogSnapshot) -> Result<()> {
    write_json(&provider_path(&snapshot.provider), snapshot)
}

pub fn load_all_snapshots(index: &CatalogIndex) -> HashMap<String, ProviderCatalogSnapshot> {
    let mut out = HashMap::new();
    for provider in index.providers.keys() {
        if let Some(snapshot) = load_provider_snapshot(provider) {
            out.insert(provider.clone(), snapshot);
        }
    }
    out
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("parse {}", path.display()))
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create dir {}", parent.display()))?;
    }
    let encoded = serde_json::to_string_pretty(value)?;
    fs::write(path, encoded).with_context(|| format!("write {}", path.display()))
}
