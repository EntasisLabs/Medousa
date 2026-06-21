use crate::module_manifest::ModuleAbi;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompatibilityMode {
    Strict,
    Permissive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleLifecycleState {
    Loaded,
    Validated,
    Active,
    Draining,
    Retired,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleLifecycleEventKind {
    Loaded,
    Validated,
    Activated,
    ActivationFailed,
    Draining,
    Retired,
    Rollback,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleLifecycleEvent {
    pub kind: ModuleLifecycleEventKind,
    pub module_id: String,
    pub generation_id: u64,
    pub version: String,
    pub content_hash: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleGeneration {
    pub module_id: String,
    pub generation_id: u64,
    pub version: String,
    pub content_hash: String,
    pub wasm_path: PathBuf,
    pub abi: ModuleAbi,
    pub state: ModuleLifecycleState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadModuleRequest {
    pub module_id: String,
    pub wasm_path: PathBuf,
    pub compatibility_mode: CompatibilityMode,
    pub abi: ModuleAbi,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivationResult {
    pub module_id: String,
    pub generation_id: u64,
    pub version: String,
    pub content_hash: String,
}

#[derive(Debug, Error)]
pub enum ModuleLoadError {
    #[error("module id cannot be empty")]
    EmptyModuleId,
    #[error("module bytes could not be read from '{path}': {source}")]
    ReadFailed {
        path: String,
        source: std::io::Error,
    },
    #[error("incompatible ABI update for module '{module_id}': active={active:?}, candidate={candidate:?}")]
    AbiIncompatible {
        module_id: String,
        active: ModuleAbi,
        candidate: ModuleAbi,
    },
    #[error("module '{module_id}' has no prior generation to roll back to")]
    NoPriorGeneration { module_id: String },
    #[error("module '{module_id}' has no active generation")]
    NoActiveGeneration { module_id: String },
    #[error("module '{module_id}' is not registered in runtime module registry")]
    UnknownModule { module_id: String },
    #[error("module '{module_id}' activation missing required signature ops: {missing_ops:?}")]
    MissingRequiredOps {
        module_id: String,
        missing_ops: Vec<String>,
    },
    #[error(
        "module '{module_id}' activation denied by capability policy: {denied_capabilities:?}"
    )]
    PolicyDeniedCapabilities {
        module_id: String,
        denied_capabilities: Vec<String>,
    },
}

#[derive(Debug, Clone, Default)]
struct ModuleSlot {
    active_generation: Option<u64>,
    previous_generation: Option<u64>,
    generations: BTreeMap<u64, ModuleGeneration>,
}

#[derive(Debug, Clone, Default)]
pub struct ModuleManager {
    next_generation_id: u64,
    slots: HashMap<String, ModuleSlot>,
    events: Vec<ModuleLifecycleEvent>,
}

impl ModuleManager {
    pub fn new() -> Self {
        Self {
            next_generation_id: 1,
            slots: HashMap::new(),
            events: Vec::new(),
        }
    }

    pub fn load_and_activate(
        &mut self,
        req: LoadModuleRequest,
    ) -> Result<ActivationResult, ModuleLoadError> {
        if req.module_id.trim().is_empty() {
            return Err(ModuleLoadError::EmptyModuleId);
        }

        let version = req.version.unwrap_or_else(|| "0.0.0".to_string());
        let content_hash = hash_bytes_from_path(&req.wasm_path)?;
        let generation_id = self.next_generation_id;
        self.next_generation_id += 1;

        let mut generation = ModuleGeneration {
            module_id: req.module_id.clone(),
            generation_id,
            version: version.clone(),
            content_hash: content_hash.clone(),
            wasm_path: req.wasm_path,
            abi: req.abi.clone(),
            state: ModuleLifecycleState::Loaded,
        };

        self.emit_event(ModuleLifecycleEventKind::Loaded, &generation, None);

        let active_abi = self
            .slots
            .get(&req.module_id)
            .and_then(|slot| slot.active_generation)
            .and_then(|active_id| {
                self.slots
                    .get(&req.module_id)
                    .and_then(|slot| slot.generations.get(&active_id))
                    .map(|g| g.abi.clone())
            });

        if let Some(active_abi) = active_abi {
            if req.compatibility_mode == CompatibilityMode::Strict && active_abi != generation.abi {
                generation.state = ModuleLifecycleState::Failed;
                {
                    let slot = self.slots.entry(req.module_id.clone()).or_default();
                    slot.generations.insert(generation_id, generation.clone());
                }
                self.emit_event(
                    ModuleLifecycleEventKind::ActivationFailed,
                    &generation,
                    Some("abi_incompatible".to_string()),
                );
                return Err(ModuleLoadError::AbiIncompatible {
                    module_id: req.module_id,
                    active: active_abi,
                    candidate: req.abi,
                });
            }
        }

        generation.state = ModuleLifecycleState::Validated;
        self.emit_event(ModuleLifecycleEventKind::Validated, &generation, None);

        let mut drained_generation: Option<ModuleGeneration> = None;

        {
            let slot = self.slots.entry(req.module_id.clone()).or_default();

            if let Some(active_id) = slot.active_generation {
                if let Some(active_generation) = slot.generations.get_mut(&active_id) {
                    active_generation.state = ModuleLifecycleState::Draining;
                    drained_generation = Some(active_generation.clone());
                }
                slot.previous_generation = Some(active_id);
            }

            generation.state = ModuleLifecycleState::Active;
            slot.active_generation = Some(generation_id);
            slot.generations.insert(generation_id, generation.clone());
        }

        if let Some(draining) = drained_generation.as_ref() {
            self.emit_event(ModuleLifecycleEventKind::Draining, draining, None);
        }

        self.emit_event(ModuleLifecycleEventKind::Activated, &generation, None);
        self.retire_stale_draining_generations(&req.module_id);

        Ok(ActivationResult {
            module_id: generation.module_id,
            generation_id,
            version,
            content_hash,
        })
    }

    pub fn rollback(&mut self, module_id: &str) -> Result<ActivationResult, ModuleLoadError> {
        let rollback_result = {
            let Some(slot) = self.slots.get_mut(module_id) else {
                return Err(ModuleLoadError::NoActiveGeneration {
                    module_id: module_id.to_string(),
                });
            };

            let Some(active_id) = slot.active_generation else {
                return Err(ModuleLoadError::NoActiveGeneration {
                    module_id: module_id.to_string(),
                });
            };

            let Some(prior_id) = slot.previous_generation else {
                return Err(ModuleLoadError::NoPriorGeneration {
                    module_id: module_id.to_string(),
                });
            };

            if let Some(active_generation) = slot.generations.get_mut(&active_id) {
                active_generation.state = ModuleLifecycleState::Failed;
            }

            let prior = slot.generations.get_mut(&prior_id).ok_or_else(|| {
                ModuleLoadError::NoPriorGeneration {
                    module_id: module_id.to_string(),
                }
            })?;
            prior.state = ModuleLifecycleState::Active;

            let result = ActivationResult {
                module_id: prior.module_id.clone(),
                generation_id: prior.generation_id,
                version: prior.version.clone(),
                content_hash: prior.content_hash.clone(),
            };

            slot.active_generation = Some(prior_id);
            slot.previous_generation = None;

            result
        };

        self.events.push(ModuleLifecycleEvent {
            kind: ModuleLifecycleEventKind::Rollback,
            module_id: rollback_result.module_id.clone(),
            generation_id: rollback_result.generation_id,
            version: rollback_result.version.clone(),
            content_hash: rollback_result.content_hash.clone(),
            reason: None,
        });
        self.retire_stale_draining_generations(module_id);

        Ok(rollback_result)
    }

    pub fn active_generation(&self, module_id: &str) -> Option<u64> {
        self.slots
            .get(module_id)
            .and_then(|slot| slot.active_generation)
    }

    pub fn lifecycle_events(&self) -> &[ModuleLifecycleEvent] {
        &self.events
    }

    pub fn active_generation_record(&self, module_id: &str) -> Option<ModuleGeneration> {
        let slot = self.slots.get(module_id)?;
        let active_id = slot.active_generation?;
        slot.generations.get(&active_id).cloned()
    }

    fn emit_event(
        &mut self,
        kind: ModuleLifecycleEventKind,
        generation: &ModuleGeneration,
        reason: Option<String>,
    ) {
        self.events.push(ModuleLifecycleEvent {
            kind,
            module_id: generation.module_id.clone(),
            generation_id: generation.generation_id,
            version: generation.version.clone(),
            content_hash: generation.content_hash.clone(),
            reason,
        });
    }

    fn retire_stale_draining_generations(&mut self, module_id: &str) {
        let retired = {
            let Some(slot) = self.slots.get_mut(module_id) else {
                return;
            };

            let active = slot.active_generation;
            let previous = slot.previous_generation;
            let mut retired = Vec::new();

            for generation in slot.generations.values_mut() {
                if generation.state == ModuleLifecycleState::Draining
                    && Some(generation.generation_id) != active
                    && Some(generation.generation_id) != previous
                {
                    generation.state = ModuleLifecycleState::Retired;
                    retired.push(generation.clone());
                }
            }

            retired
        };

        for generation in retired {
            self.emit_event(ModuleLifecycleEventKind::Retired, &generation, None);
        }
    }

    /// Return all module ids tracked by the manager.
    pub fn module_ids(&self) -> Vec<String> {
        self.slots.keys().cloned().collect()
    }

    /// Export persistent hotload state for cross-command/session restore.
    pub fn export_hotload(&self) -> crate::module_hotload::HotloadStore {
        use crate::module_hotload::{HotloadGenerationRecord, HotloadSlotRecord, HotloadStore, HOTLOAD_SCHEMA};

        let mut slots = HashMap::new();
        for (module_id, slot) in &self.slots {
            let generations = slot
                .generations
                .values()
                .map(|generation| HotloadGenerationRecord {
                    generation_id: generation.generation_id,
                    module_id: generation.module_id.clone(),
                    version: generation.version.clone(),
                    content_hash: generation.content_hash.clone(),
                    wasm_path: generation.wasm_path.display().to_string(),
                    abi: generation.abi.clone(),
                    state: lifecycle_state_label(generation.state).to_string(),
                })
                .collect::<Vec<_>>();

            slots.insert(
                module_id.clone(),
                HotloadSlotRecord {
                    active_generation: slot.active_generation,
                    previous_generation: slot.previous_generation,
                    generations,
                },
            );
        }

        HotloadStore {
            schema: HOTLOAD_SCHEMA.to_string(),
            next_generation_id: self.next_generation_id,
            slots,
        }
    }

    /// Restore a module manager from a hotload store snapshot.
    pub fn import_hotload(store: crate::module_hotload::HotloadStore) -> Self {
        let mut manager = Self {
            next_generation_id: store.next_generation_id.max(1),
            slots: HashMap::new(),
            events: Vec::new(),
        };

        for (module_id, slot_record) in store.slots {
            let mut generations = BTreeMap::new();
            for record in slot_record.generations {
                generations.insert(
                    record.generation_id,
                    ModuleGeneration {
                        module_id: record.module_id,
                        generation_id: record.generation_id,
                        version: record.version,
                        content_hash: record.content_hash,
                        wasm_path: PathBuf::from(record.wasm_path),
                        abi: record.abi,
                        state: parse_lifecycle_state(&record.state),
                    },
                );
            }

            manager.slots.insert(
                module_id,
                ModuleSlot {
                    active_generation: slot_record.active_generation,
                    previous_generation: slot_record.previous_generation,
                    generations,
                },
            );
        }

        manager
    }
}

fn lifecycle_state_label(state: ModuleLifecycleState) -> &'static str {
    match state {
        ModuleLifecycleState::Loaded => "loaded",
        ModuleLifecycleState::Validated => "validated",
        ModuleLifecycleState::Active => "active",
        ModuleLifecycleState::Draining => "draining",
        ModuleLifecycleState::Retired => "retired",
        ModuleLifecycleState::Failed => "failed",
    }
}

fn parse_lifecycle_state(label: &str) -> ModuleLifecycleState {
    match label {
        "loaded" => ModuleLifecycleState::Loaded,
        "validated" => ModuleLifecycleState::Validated,
        "active" => ModuleLifecycleState::Active,
        "draining" => ModuleLifecycleState::Draining,
        "retired" => ModuleLifecycleState::Retired,
        "failed" => ModuleLifecycleState::Failed,
        _ => ModuleLifecycleState::Active,
    }
}

fn hash_bytes_from_path(path: &PathBuf) -> Result<String, ModuleLoadError> {
    let bytes = fs::read(path).map_err(|source| ModuleLoadError::ReadFailed {
        path: path.display().to_string(),
        source,
    })?;

    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn write_temp_wasm(tag: &str, bytes: &[u8]) -> PathBuf {
        let mut path = std::env::temp_dir();
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        path.push(format!("grapheme-{tag}-{ts}.wasm"));
        fs::write(&path, bytes).expect("write temp wasm bytes");
        path
    }

    #[test]
    fn load_and_activate_sets_active_generation_and_emits_events() {
        let wasm_path = write_temp_wasm("activate", b"wasm-a");
        let mut manager = ModuleManager::new();

        let result = manager
            .load_and_activate(LoadModuleRequest {
                module_id: "http".to_string(),
                wasm_path: wasm_path.clone(),
                compatibility_mode: CompatibilityMode::Strict,
                abi: ModuleAbi::WasixV1,
                version: Some("1.0.0".to_string()),
            })
            .expect("activation should succeed");

        assert_eq!(
            manager.active_generation("http"),
            Some(result.generation_id)
        );
        assert!(manager
            .lifecycle_events()
            .iter()
            .any(|e| e.kind == ModuleLifecycleEventKind::Activated));

        let _ = fs::remove_file(wasm_path);
    }

    #[test]
    fn strict_mode_rejects_incompatible_abi_updates() {
        let wasm_a = write_temp_wasm("abi-a", b"wasm-a");
        let wasm_b = write_temp_wasm("abi-b", b"wasm-b");
        let mut manager = ModuleManager::new();

        manager
            .load_and_activate(LoadModuleRequest {
                module_id: "http".to_string(),
                wasm_path: wasm_a.clone(),
                compatibility_mode: CompatibilityMode::Strict,
                abi: ModuleAbi::WasixV1,
                version: Some("1.0.0".to_string()),
            })
            .expect("first activation should succeed");

        let err = manager
            .load_and_activate(LoadModuleRequest {
                module_id: "http".to_string(),
                wasm_path: wasm_b.clone(),
                compatibility_mode: CompatibilityMode::Strict,
                abi: ModuleAbi::MirV1,
                version: Some("2.0.0".to_string()),
            })
            .expect_err("incompatible ABI should fail in strict mode");

        assert!(matches!(err, ModuleLoadError::AbiIncompatible { .. }));
        assert!(manager.lifecycle_events().iter().any(|e| {
            e.kind == ModuleLifecycleEventKind::ActivationFailed
                && e.reason.as_deref() == Some("abi_incompatible")
        }));

        let _ = fs::remove_file(wasm_a);
        let _ = fs::remove_file(wasm_b);
    }

    #[test]
    fn failed_activation_keeps_previous_generation_active() {
        let wasm_a = write_temp_wasm("active-on-fail-a", b"wasm-a");
        let wasm_b = write_temp_wasm("active-on-fail-b", b"wasm-b");
        let mut manager = ModuleManager::new();

        let first = manager
            .load_and_activate(LoadModuleRequest {
                module_id: "http".to_string(),
                wasm_path: wasm_a.clone(),
                compatibility_mode: CompatibilityMode::Strict,
                abi: ModuleAbi::WasixV1,
                version: Some("1.0.0".to_string()),
            })
            .expect("first activation should succeed");

        let err = manager
            .load_and_activate(LoadModuleRequest {
                module_id: "http".to_string(),
                wasm_path: wasm_b.clone(),
                compatibility_mode: CompatibilityMode::Strict,
                abi: ModuleAbi::MirV1,
                version: Some("2.0.0".to_string()),
            })
            .expect_err("incompatible ABI should fail");

        assert!(matches!(err, ModuleLoadError::AbiIncompatible { .. }));
        assert_eq!(manager.active_generation("http"), Some(first.generation_id));

        let _ = fs::remove_file(wasm_a);
        let _ = fs::remove_file(wasm_b);
    }

    #[test]
    fn rollback_restores_previous_generation() {
        let wasm_a = write_temp_wasm("rollback-a", b"wasm-a");
        let wasm_b = write_temp_wasm("rollback-b", b"wasm-b");
        let mut manager = ModuleManager::new();

        let first = manager
            .load_and_activate(LoadModuleRequest {
                module_id: "http".to_string(),
                wasm_path: wasm_a.clone(),
                compatibility_mode: CompatibilityMode::Strict,
                abi: ModuleAbi::WasixV1,
                version: Some("1.0.0".to_string()),
            })
            .expect("first activation should succeed");

        manager
            .load_and_activate(LoadModuleRequest {
                module_id: "http".to_string(),
                wasm_path: wasm_b.clone(),
                compatibility_mode: CompatibilityMode::Strict,
                abi: ModuleAbi::WasixV1,
                version: Some("1.1.0".to_string()),
            })
            .expect("second activation should succeed");

        let rolled_back = manager.rollback("http").expect("rollback should succeed");
        assert_eq!(rolled_back.generation_id, first.generation_id);
        assert_eq!(manager.active_generation("http"), Some(first.generation_id));
        assert!(manager
            .lifecycle_events()
            .iter()
            .any(|e| e.kind == ModuleLifecycleEventKind::Rollback));

        let _ = fs::remove_file(wasm_a);
        let _ = fs::remove_file(wasm_b);
    }

    #[test]
    fn superseded_draining_generation_transitions_to_retired() {
        let wasm_a = write_temp_wasm("retire-a", b"wasm-a");
        let wasm_b = write_temp_wasm("retire-b", b"wasm-b");
        let wasm_c = write_temp_wasm("retire-c", b"wasm-c");
        let mut manager = ModuleManager::new();

        let first = manager
            .load_and_activate(LoadModuleRequest {
                module_id: "http".to_string(),
                wasm_path: wasm_a.clone(),
                compatibility_mode: CompatibilityMode::Strict,
                abi: ModuleAbi::WasixV1,
                version: Some("1.0.0".to_string()),
            })
            .expect("first activation should succeed");

        manager
            .load_and_activate(LoadModuleRequest {
                module_id: "http".to_string(),
                wasm_path: wasm_b.clone(),
                compatibility_mode: CompatibilityMode::Strict,
                abi: ModuleAbi::WasixV1,
                version: Some("1.1.0".to_string()),
            })
            .expect("second activation should succeed");

        manager
            .load_and_activate(LoadModuleRequest {
                module_id: "http".to_string(),
                wasm_path: wasm_c.clone(),
                compatibility_mode: CompatibilityMode::Strict,
                abi: ModuleAbi::WasixV1,
                version: Some("1.2.0".to_string()),
            })
            .expect("third activation should succeed");

        assert!(manager.lifecycle_events().iter().any(|e| {
            e.kind == ModuleLifecycleEventKind::Retired
                && e.module_id == "http"
                && e.generation_id == first.generation_id
        }));

        let _ = fs::remove_file(wasm_a);
        let _ = fs::remove_file(wasm_b);
        let _ = fs::remove_file(wasm_c);
    }
}
