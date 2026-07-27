//! Shared persistence helpers for mesh registry / inbox / outbox / receipts.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::paths::medousa_data_dir;
use crate::session::atomic_write;

pub static MESH_IO_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

pub fn mesh_dir() -> PathBuf {
    medousa_data_dir().join("mesh")
}

pub fn peers_path() -> PathBuf {
    mesh_dir().join("peers.json")
}

pub fn inbox_path() -> PathBuf {
    mesh_dir().join("inbox.json")
}

pub fn outbox_path() -> PathBuf {
    mesh_dir().join("outbox.json")
}

pub fn receipts_path() -> PathBuf {
    mesh_dir().join("receipts.json")
}

pub fn intros_path() -> PathBuf {
    mesh_dir().join("intros.json")
}

pub fn read_json_default<T: Default + DeserializeOwned>(path: &PathBuf) -> Result<T> {
    if !path.is_file() {
        return Ok(T::default());
    }
    let raw = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    if raw.trim().is_empty() {
        return Ok(T::default());
    }
    serde_json::from_str(&raw).with_context(|| format!("parse {}", path.display()))
}

pub fn write_json<T: Serialize>(path: &PathBuf, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value).context("serialize mesh json")?;
    atomic_write(path, &bytes).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}
