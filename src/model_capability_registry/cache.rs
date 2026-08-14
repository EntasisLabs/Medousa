use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use medousa_types::authority_id::ProviderId;

use crate::store_root::{StorePath, StoreRoot};

use super::types::{CatalogIndex, ProviderCatalogSnapshot};

const MAX_CATALOG_FILE_BYTES: u64 = 32 * 1024 * 1024;

pub fn catalog_dir() -> PathBuf {
    crate::session::medousa_data_dir().join("model_catalog")
}

fn catalog_store() -> Result<StoreRoot> {
    Ok(StoreRoot::open_or_create_nofollow(&catalog_dir())?)
}

fn index_path() -> StorePath {
    StorePath::parse("index.json").expect("static model catalog index path")
}

fn provider_path(provider: &ProviderId) -> StorePath {
    StorePath::parse(&format!("{}.json", provider.storage_key().as_str()))
        .expect("opaque provider key is a valid store path")
}

fn legacy_provider_path(provider: &ProviderId) -> StorePath {
    StorePath::parse(&format!("{}.json", provider.as_str()))
        .expect("validated provider id is a valid legacy store path")
}

pub fn load_index() -> CatalogIndex {
    read_json(&index_path()).unwrap_or_default()
}

pub fn save_index(index: &CatalogIndex) -> Result<()> {
    write_json(&index_path(), index)
}

pub fn load_provider_snapshot(provider: &str) -> Option<ProviderCatalogSnapshot> {
    let provider = ProviderId::parse(provider).ok()?;
    let snapshot: ProviderCatalogSnapshot = read_json(&provider_path(&provider))
        .or_else(|_| read_json(&legacy_provider_path(&provider)))
        .ok()?;
    (snapshot.provider == provider.as_str()).then_some(snapshot)
}

pub fn save_provider_snapshot(snapshot: &ProviderCatalogSnapshot) -> Result<()> {
    let provider = ProviderId::parse(&snapshot.provider)?;
    write_json(&provider_path(&provider), snapshot)
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

fn read_json<T: serde::de::DeserializeOwned>(path: &StorePath) -> Result<T> {
    let raw = catalog_store()?.read_limited(path, MAX_CATALOG_FILE_BYTES)?;
    serde_json::from_slice(&raw).context("parse model catalog cache")
}

fn write_json<T: serde::Serialize>(path: &StorePath, value: &T) -> Result<()> {
    let encoded = serde_json::to_vec_pretty(value)?;
    catalog_store()?.atomic_write(path, &encoded)?;
    Ok(())
}
