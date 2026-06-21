//! Dynamic discovery for Wasm capability modules with v1 manifest sidecars.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

pub const MANIFEST_SCHEMA: &str = "grapheme.module.manifest/v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WasmModuleManifest {
    pub schema: String,
    pub module_id: String,
    pub version: String,
    pub abi: String,
    pub wasm: String,
    #[serde(default)]
    pub entrypoint: Option<String>,
    pub exported_ops: Vec<WasmExportedOp>,
    pub required_capabilities: Vec<String>,
    #[serde(default)]
    pub limits: Option<WasmModuleLimits>,
    #[serde(default)]
    pub content_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WasmExportedOp {
    pub op: String,
    pub effect: String,
    #[serde(default)]
    pub input_schema_ref: Option<String>,
    #[serde(default)]
    pub output_schema_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WasmModuleLimits {
    pub max_cpu_ms: u64,
    pub max_memory_mb: u64,
    pub max_io_bytes: u64,
    pub max_network_calls: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveredWasmModule {
    pub module_id: String,
    pub version: String,
    pub abi: String,
    pub wasm_path: PathBuf,
    pub manifest_path: PathBuf,
    pub exported_ops: Vec<String>,
    pub required_capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WasmDiscoveryReport {
    pub count: usize,
    pub modules: Vec<DiscoveredWasmModule>,
    #[serde(default)]
    pub errors: Vec<WasmDiscoveryErrorRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WasmDiscoveryErrorRecord {
    pub path: String,
    pub error: String,
}

#[derive(Debug, Error)]
pub enum WasmDiscoveryError {
    #[error("scan path does not exist: {0}")]
    ScanPathMissing(String),
    #[error("failed to read directory '{path}': {source}")]
    ReadDirFailed {
        path: String,
        source: std::io::Error,
    },
    #[error("invalid manifest at '{path}': {reason}")]
    InvalidManifest { path: String, reason: String },
    #[error("manifest references missing wasm file '{wasm}' (manifest: {manifest})")]
    MissingWasm { manifest: String, wasm: String },
}

pub fn discover_wasm_modules(scan_roots: &[PathBuf]) -> WasmDiscoveryReport {
    let mut modules = Vec::new();
    let mut errors = Vec::new();

    for root in scan_roots {
        if !root.exists() {
            errors.push(WasmDiscoveryErrorRecord {
                path: root.display().to_string(),
                error: WasmDiscoveryError::ScanPathMissing(root.display().to_string()).to_string(),
            });
            continue;
        }

        discover_in_directory(root, &mut modules, &mut errors);
    }

    modules.sort_by(|a, b| a.module_id.cmp(&b.module_id));

    WasmDiscoveryReport {
        count: modules.len(),
        modules,
        errors,
    }
}

fn discover_in_directory(
    dir: &Path,
    modules: &mut Vec<DiscoveredWasmModule>,
    errors: &mut Vec<WasmDiscoveryErrorRecord>,
) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) => {
            errors.push(WasmDiscoveryErrorRecord {
                path: dir.display().to_string(),
                error: WasmDiscoveryError::ReadDirFailed {
                    path: dir.display().to_string(),
                    source: err,
                }
                .to_string(),
            });
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            discover_in_directory(&path, modules, errors);
            continue;
        }

        if path.extension().and_then(|ext| ext.to_str()) != Some("wasm") {
            continue;
        }

        match load_discovered_module(&path) {
            Ok(module) => modules.push(module),
            Err(err) => errors.push(WasmDiscoveryErrorRecord {
                path: path.display().to_string(),
                error: err.to_string(),
            }),
        }
    }
}

fn load_discovered_module(wasm_path: &Path) -> Result<DiscoveredWasmModule, WasmDiscoveryError> {
    let manifest_path = manifest_path_for_wasm(wasm_path);
    if !manifest_path.exists() {
        return Err(WasmDiscoveryError::InvalidManifest {
            path: manifest_path.display().to_string(),
            reason: format!(
                "missing sidecar manifest (expected {})",
                manifest_path.display()
            ),
        });
    }

    let raw = fs::read_to_string(&manifest_path).map_err(|err| WasmDiscoveryError::InvalidManifest {
        path: manifest_path.display().to_string(),
        reason: format!("read failed: {err}"),
    })?;

    let manifest: WasmModuleManifest =
        serde_json::from_str(&raw).map_err(|err| WasmDiscoveryError::InvalidManifest {
            path: manifest_path.display().to_string(),
            reason: format!("json parse failed: {err}"),
        })?;

    validate_manifest(&manifest, &manifest_path)?;

    let wasm_relative = Path::new(&manifest.wasm);
    let resolved_wasm = manifest_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(wasm_relative);

    if !resolved_wasm.exists() {
        return Err(WasmDiscoveryError::MissingWasm {
            manifest: manifest_path.display().to_string(),
            wasm: resolved_wasm.display().to_string(),
        });
    }

    Ok(DiscoveredWasmModule {
        module_id: manifest.module_id,
        version: manifest.version,
        abi: manifest.abi,
        wasm_path: resolved_wasm,
        manifest_path,
        exported_ops: manifest
            .exported_ops
            .into_iter()
            .map(|op| op.op)
            .collect(),
        required_capabilities: manifest.required_capabilities,
    })
}

fn validate_manifest(manifest: &WasmModuleManifest, path: &Path) -> Result<(), WasmDiscoveryError> {
    if manifest.schema != MANIFEST_SCHEMA {
        return Err(WasmDiscoveryError::InvalidManifest {
            path: path.display().to_string(),
            reason: format!(
                "unsupported schema '{}'; expected '{MANIFEST_SCHEMA}'",
                manifest.schema
            ),
        });
    }

    if manifest.module_id.trim().is_empty() {
        return Err(WasmDiscoveryError::InvalidManifest {
            path: path.display().to_string(),
            reason: "module_id cannot be empty".to_string(),
        });
    }

    if manifest.abi != "wasix_v1" {
        return Err(WasmDiscoveryError::InvalidManifest {
            path: path.display().to_string(),
            reason: format!("unsupported abi '{}'", manifest.abi),
        });
    }

    if manifest.exported_ops.is_empty() {
        return Err(WasmDiscoveryError::InvalidManifest {
            path: path.display().to_string(),
            reason: "exported_ops cannot be empty".to_string(),
        });
    }

    Ok(())
}

/// Build a module activation request from a discovery scan record.
pub fn discovered_module_to_load_request(module: &DiscoveredWasmModule) -> crate::module_manager::LoadModuleRequest {
    use crate::module_manager::CompatibilityMode;
    use crate::module_manifest::ModuleAbi;

    crate::module_manager::LoadModuleRequest {
        module_id: module.module_id.clone(),
        wasm_path: module.wasm_path.clone(),
        compatibility_mode: CompatibilityMode::Strict,
        abi: ModuleAbi::WasixV1,
        version: Some(module.version.clone()),
    }
}

fn manifest_path_for_wasm(wasm_path: &Path) -> PathBuf {
    let stem = wasm_path.file_stem().and_then(|s| s.to_str()).unwrap_or("module");
    wasm_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!("{stem}.module.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_file(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(path, contents).expect("write file");
    }

    #[test]
    fn discovers_module_with_valid_sidecar() {
        let base = std::env::temp_dir().join(format!(
            "grapheme-discover-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);

        let wasm_path = base.join("pdf.wasm");
        write_file(&wasm_path, "wasm-bytes");
        write_file(
            &base.join("pdf.module.json"),
            r#"{
              "schema": "grapheme.module.manifest/v1",
              "module_id": "pdf",
              "version": "0.1.0",
              "abi": "wasix_v1",
              "wasm": "pdf.wasm",
              "exported_ops": [{ "op": "generate", "effect": "io" }],
              "required_capabilities": ["pdf.generate"]
            }"#,
        );

        let report = discover_wasm_modules(&[base.clone()]);
        assert_eq!(report.count, 1);
        assert_eq!(report.modules[0].module_id, "pdf");
        assert_eq!(report.modules[0].exported_ops, vec!["generate".to_string()]);

        let _ = fs::remove_dir_all(base);
    }
}
