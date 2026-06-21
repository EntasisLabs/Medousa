use std::collections::HashMap;
use std::path::PathBuf;

use crate::module_manifest::{core_v1_manifests, ExportedOp, ModuleAbi, ModuleManifest};

#[derive(Debug, Clone)]
pub struct ModuleBinding {
    pub manifest: ModuleManifest,
    pub wasm_path: Option<PathBuf>,
    pub generation_id: Option<u64>,
    pub content_hash: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ModuleRegistry {
    bindings: HashMap<String, ModuleBinding>,
}

#[derive(Debug, Clone)]
pub struct ResolvedModuleCall {
    pub module_id: String,
    pub op: String,
    pub abi: ModuleAbi,
    pub wasm_path: Option<PathBuf>,
    pub generation_id: Option<u64>,
    pub content_hash: Option<String>,
}

impl ModuleRegistry {
    pub fn from_core_v1() -> Self {
        let mut bindings = HashMap::new();
        for manifest in core_v1_manifests() {
            let module_id = manifest.module_id.clone();
            bindings.insert(
                module_id,
                ModuleBinding {
                    manifest,
                    wasm_path: None,
                    generation_id: None,
                    content_hash: None,
                },
            );
        }

        Self { bindings }
    }

    pub fn set_wasm_path(&mut self, module_id: &str, wasm_path: PathBuf) {
        if let Some(binding) = self.bindings.get_mut(module_id) {
            binding.wasm_path = Some(wasm_path);
            binding.generation_id = None;
            binding.content_hash = None;
        }
    }

    pub fn has_module(&self, module_id: &str) -> bool {
        self.bindings.contains_key(module_id)
    }

    pub fn manifest_for(&self, module_id: &str) -> Option<ModuleManifest> {
        self.bindings
            .get(&module_id.to_lowercase())
            .map(|binding| binding.manifest.clone())
    }

    pub fn exported_ops_for(&self, module_id: &str) -> Option<Vec<ExportedOp>> {
        self.bindings
            .get(&module_id.to_lowercase())
            .map(|binding| binding.manifest.exported_ops.clone())
    }

    pub fn set_wasm_generation(
        &mut self,
        module_id: &str,
        wasm_path: PathBuf,
        generation_id: u64,
        content_hash: String,
    ) {
        if let Some(binding) = self.bindings.get_mut(module_id) {
            binding.wasm_path = Some(wasm_path);
            binding.generation_id = Some(generation_id);
            binding.content_hash = Some(content_hash);
        }
    }

    pub fn resolve_call(
        &self,
        module: Option<&str>,
        op: &str,
        capability: &str,
    ) -> Option<ResolvedModuleCall> {
        let module_id = module
            .map(|m| m.to_lowercase())
            .or_else(|| capability.split('.').next().map(|m| m.to_lowercase()))?;

        let binding = self.bindings.get(&module_id)?;

        let op_exists = binding.manifest.exported_ops.iter().any(|e| e.op == op);
        if !op_exists {
            return None;
        }

        Some(ResolvedModuleCall {
            module_id,
            op: op.to_string(),
            abi: effective_abi(binding),
            wasm_path: binding.wasm_path.clone(),
            generation_id: binding.generation_id,
            content_hash: binding.content_hash.clone(),
        })
    }

    /// Register a MirV1 host module (no Wasm path) for capability dispatch via `CapabilityHost`.
    pub fn register_host_module(&mut self, manifest: ModuleManifest) {
        let module_id = manifest.module_id.to_lowercase();
        self.bindings.insert(
            module_id,
            ModuleBinding {
                manifest,
                wasm_path: None,
                generation_id: None,
                content_hash: None,
            },
        );
    }
}

fn effective_abi(binding: &ModuleBinding) -> ModuleAbi {
    if binding.wasm_path.is_none() {
        return binding.manifest.abi.clone();
    }

    match binding.manifest.abi {
        ModuleAbi::MirV1 => ModuleAbi::WasixV1,
        ModuleAbi::WasixV1 => ModuleAbi::WasixV1,
        ModuleAbi::WasixWitV15 => ModuleAbi::WasixWitV15,
    }
}

impl Default for ModuleRegistry {
    fn default() -> Self {
        Self::from_core_v1()
    }
}
