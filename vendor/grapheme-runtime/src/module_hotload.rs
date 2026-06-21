//! Persistent hotload store for Wasm module generations (v1).
//!
//! Persists `ModuleManager` slot state to `.grapheme/modules/hotload.json` so
//! activate/rollback survives CLI commands and run sessions.

use crate::module_manager::ModuleManager;
use crate::module_manifest::ModuleAbi;
use crate::module_registry::ModuleRegistry;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

pub const HOTLOAD_SCHEMA: &str = "grapheme.modules.hotload/v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HotloadStore {
    pub schema: String,
    pub next_generation_id: u64,
    #[serde(default)]
    pub slots: HashMap<String, HotloadSlotRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HotloadSlotRecord {
    pub active_generation: Option<u64>,
    pub previous_generation: Option<u64>,
    pub generations: Vec<HotloadGenerationRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HotloadGenerationRecord {
    pub generation_id: u64,
    pub module_id: String,
    pub version: String,
    pub content_hash: String,
    pub wasm_path: String,
    pub abi: ModuleAbi,
    pub state: String,
}

#[derive(Debug, Error)]
pub enum HotloadError {
    #[error("read hotload store '{path}': {source}")]
    ReadFailed {
        path: String,
        source: std::io::Error,
    },
    #[error("parse hotload store '{path}': {reason}")]
    ParseFailed { path: String, reason: String },
    #[error("write hotload store '{path}': {source}")]
    WriteFailed {
        path: String,
        source: std::io::Error,
    },
    #[error("unsupported hotload schema '{found}'; expected '{HOTLOAD_SCHEMA}'")]
    UnsupportedSchema { found: String },
}

impl Default for HotloadStore {
    fn default() -> Self {
        Self {
            schema: HOTLOAD_SCHEMA.to_string(),
            next_generation_id: 1,
            slots: HashMap::new(),
        }
    }
}

impl HotloadStore {
    pub fn validate(&self) -> Result<(), HotloadError> {
        if self.schema != HOTLOAD_SCHEMA {
            return Err(HotloadError::UnsupportedSchema {
                found: self.schema.clone(),
            });
        }
        Ok(())
    }

    pub fn active_bindings(&self) -> HashMap<String, PathBuf> {
        let mut out = HashMap::new();
        for (module_id, slot) in &self.slots {
            let Some(active_id) = slot.active_generation else {
                continue;
            };
            let Some(generation) = slot
                .generations
                .iter()
                .find(|g| g.generation_id == active_id)
            else {
                continue;
            };
            out.insert(module_id.clone(), PathBuf::from(&generation.wasm_path));
        }
        out
    }
}

pub fn default_hotload_store_path() -> PathBuf {
    PathBuf::from(".grapheme/modules/hotload.json")
}

pub fn load_hotload_store(path: &Path) -> Result<Option<HotloadStore>, HotloadError> {
    if !path.exists() {
        return Ok(None);
    }

    let raw = fs::read_to_string(path).map_err(|source| HotloadError::ReadFailed {
        path: path.display().to_string(),
        source,
    })?;

    let store: HotloadStore =
        serde_json::from_str(&raw).map_err(|err| HotloadError::ParseFailed {
            path: path.display().to_string(),
            reason: err.to_string(),
        })?;
    store.validate()?;
    Ok(Some(store))
}

pub fn save_hotload_store(path: &Path, store: &HotloadStore) -> Result<(), HotloadError> {
    store.validate()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| HotloadError::WriteFailed {
            path: parent.display().to_string(),
            source,
        })?;
    }

    let serialized = serde_json::to_string_pretty(store).map_err(|err| HotloadError::WriteFailed {
        path: path.display().to_string(),
        source: std::io::Error::new(std::io::ErrorKind::InvalidData, err.to_string()),
    })?;

    fs::write(path, serialized).map_err(|source| HotloadError::WriteFailed {
        path: path.display().to_string(),
        source,
    })
}

pub fn export_hotload(manager: &ModuleManager) -> HotloadStore {
    manager.export_hotload()
}

pub fn import_hotload(store: HotloadStore) -> Result<ModuleManager, HotloadError> {
    store.validate()?;
    Ok(ModuleManager::import_hotload(store))
}

pub fn apply_hotload_store(store: &HotloadStore, manager: &mut ModuleManager, registry: &mut ModuleRegistry) {
    *manager = ModuleManager::import_hotload(store.clone());
    sync_registry_from_manager(manager, registry);
}

pub fn sync_registry_from_manager(manager: &ModuleManager, registry: &mut ModuleRegistry) {
    for module_id in manager.module_ids() {
        if let Some(generation) = manager.active_generation_record(&module_id) {
            registry.set_wasm_generation(
                &module_id,
                generation.wasm_path,
                generation.generation_id,
                generation.content_hash,
            );
        }
    }
}

pub fn hotload_status_payload(store: &HotloadStore) -> serde_json::Value {
    let mut modules = serde_json::Map::new();

    for (module_id, slot) in &store.slots {
        let active = slot.active_generation.and_then(|active_id| {
            slot.generations.iter().find(|g| g.generation_id == active_id)
        });
        let previous = slot.previous_generation.and_then(|previous_id| {
            slot.generations.iter().find(|g| g.generation_id == previous_id)
        });

        modules.insert(
            module_id.clone(),
            serde_json::json!({
                "active_generation": slot.active_generation,
                "previous_generation": slot.previous_generation,
                "generation_count": slot.generations.len(),
                "active": active.map(|g| serde_json::json!({
                    "generation_id": g.generation_id,
                    "version": g.version,
                    "wasm_path": g.wasm_path,
                    "content_hash": g.content_hash,
                    "state": g.state,
                })),
                "previous": previous.map(|g| serde_json::json!({
                    "generation_id": g.generation_id,
                    "version": g.version,
                    "wasm_path": g.wasm_path,
                    "content_hash": g.content_hash,
                    "state": g.state,
                })),
            }),
        );
    }

    serde_json::json!({
        "schema": store.schema,
        "next_generation_id": store.next_generation_id,
        "module_count": store.slots.len(),
        "modules": modules,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::module_manager::{CompatibilityMode, LoadModuleRequest, ModuleLifecycleEventKind};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn write_temp_wasm(tag: &str, bytes: &[u8]) -> PathBuf {
        let mut path = std::env::temp_dir();
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        path.push(format!("grapheme-hotload-{tag}-{ts}.wasm"));
        fs::write(&path, bytes).expect("write temp wasm bytes");
        path
    }

    #[test]
    fn export_import_roundtrip_preserves_active_generation() {
        let wasm = write_temp_wasm("roundtrip", b"wasm-a");
        let mut manager = ModuleManager::new();
        manager
            .load_and_activate(LoadModuleRequest {
                module_id: "pdf".to_string(),
                wasm_path: wasm.clone(),
                compatibility_mode: CompatibilityMode::Strict,
                abi: ModuleAbi::WasixV1,
                version: Some("0.1.0".to_string()),
            })
            .expect("activate");

        let store = export_hotload(&manager);
        let restored = import_hotload(store).expect("import");
        assert_eq!(restored.active_generation("pdf"), manager.active_generation("pdf"));

        let _ = fs::remove_file(wasm);
    }

    #[test]
    fn restored_manager_supports_rollback_after_second_activation() {
        let wasm_a = write_temp_wasm("a", b"wasm-a");
        let wasm_b = write_temp_wasm("b", b"wasm-b");

        let mut manager = ModuleManager::new();
        manager
            .load_and_activate(LoadModuleRequest {
                module_id: "pdf".to_string(),
                wasm_path: wasm_a,
                compatibility_mode: CompatibilityMode::Strict,
                abi: ModuleAbi::WasixV1,
                version: Some("0.1.0".to_string()),
            })
            .expect("first activate");
        manager
            .load_and_activate(LoadModuleRequest {
                module_id: "pdf".to_string(),
                wasm_path: wasm_b.clone(),
                compatibility_mode: CompatibilityMode::Strict,
                abi: ModuleAbi::WasixV1,
                version: Some("0.2.0".to_string()),
            })
            .expect("second activate");

        let store = export_hotload(&manager);
        let mut restored = import_hotload(store).expect("import");
        let rollback = restored.rollback("pdf").expect("rollback");
        assert_eq!(rollback.version, "0.1.0");
        assert!(restored
            .lifecycle_events()
            .iter()
            .any(|e| e.kind == ModuleLifecycleEventKind::Rollback));

        let _ = fs::remove_file(wasm_b);
    }
}
