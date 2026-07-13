//! Editor/LSP catalog for Medousa host Grapheme modules (`shell`, `medousa`).
//!
//! Runtime registration lives in [`crate::grapheme_medousa_bridge`] /
//! [`crate::shell_grapheme`]. The Scripts Workbench and module APIs read a
//! separate static catalog (`grapheme_sdk::discover_module_manifests`) that does
//! not see host modules — this module bridges that gap.

use std::collections::{BTreeMap, BTreeSet};

use grapheme_runtime::{EffectKind, ExportedOp, ModuleAbi, ModuleManifest, ResourceLimits};
use grapheme_sdk::{
    CompactModuleOp, EffectGroup, ModuleInfoPayload, ModuleOpRow, ModuleOpSummary,
};
use serde_json::json;

use crate::shell_grapheme::{shell_host_module_manifest, SHELL_MODULE};

pub const MEDOUSA_MODULE: &str = "medousa";

/// All Medousa-owned host module manifests for the editor catalog.
pub fn host_module_manifests() -> Vec<ModuleManifest> {
    vec![medousa_host_module_manifest(), shell_host_module_manifest()]
}

pub fn medousa_host_module_manifest() -> ModuleManifest {
    ModuleManifest {
        module_id: MEDOUSA_MODULE.to_string(),
        version: "0.1.0".to_string(),
        abi: ModuleAbi::MirV1,
        entrypoint: "medousa.host".to_string(),
        exported_ops: vec![
            ExportedOp {
                op: "digest".to_string(),
                input_schema_ref: None,
                output_schema_ref: None,
                effect: EffectKind::Pure,
            },
            ExportedOp {
                op: "synthesize".to_string(),
                input_schema_ref: None,
                output_schema_ref: None,
                effect: EffectKind::Control,
            },
            ExportedOp {
                op: "deliver".to_string(),
                input_schema_ref: None,
                output_schema_ref: None,
                effect: EffectKind::Control,
            },
        ],
        required_capabilities: vec![],
        limits: ResourceLimits {
            max_cpu_ms: 30_000,
            max_memory_mb: 256,
            max_io_bytes: 16 * 1024 * 1024,
            max_network_calls: 8,
        },
    }
}

pub fn host_module_manifest_by_id(module_id: &str) -> Option<ModuleManifest> {
    let needle = module_id.trim().to_ascii_lowercase();
    host_module_manifests()
        .into_iter()
        .find(|manifest| manifest.module_id.eq_ignore_ascii_case(&needle))
}

/// Core catalog + Medousa host modules, deduped by `module_id` (host wins on clash).
pub fn discover_modules_with_host() -> Vec<ModuleManifest> {
    let mut by_id: BTreeMap<String, ModuleManifest> = BTreeMap::new();
    for manifest in grapheme_sdk::discover_module_manifests() {
        by_id.insert(manifest.module_id.to_ascii_lowercase(), manifest);
    }
    for manifest in host_module_manifests() {
        by_id.insert(manifest.module_id.to_ascii_lowercase(), manifest);
    }
    by_id.into_values().collect()
}

pub fn modules_info_with_host(module_id: &str) -> Option<ModuleInfoPayload> {
    if let Some(payload) = grapheme_sdk::modules_info_contract(module_id) {
        return Some(payload);
    }
    let manifest = host_module_manifest_by_id(module_id)?;
    Some(module_info_from_manifest(&manifest))
}

pub fn curated_examples_for_host_module(module_id: &str) -> Vec<String> {
    match module_id.trim().to_ascii_lowercase().as_str() {
        "shell" => vec![
            r#"query ShellStatus {
  shell.status() { available backend reason }
}"#
            .to_string(),
            r#"query ShellEcho {
  shell.run(
    command: "echo hello",
    network: false,
    timeout_ms: 5000
  ) { exit_code stdout stderr backend sandboxed timed_out duration_ms warning }
}"#
            .to_string(),
        ],
        "medousa" => vec![
            r#"query DigestPayload {
  medousa.digest(text: "summarize this turn") { ok }
}"#
            .to_string(),
        ],
        _ => Vec::new(),
    }
}

pub fn examples_for_module(module_id: &str) -> Vec<String> {
    let mut examples = grapheme_sdk::curated_examples_for_module(module_id)
        .iter()
        .map(|path| (*path).to_string())
        .collect::<Vec<_>>();
    examples.extend(curated_examples_for_host_module(module_id));
    examples
}

/// Search ops across core + host catalogs (same shape as `modules_ops_contract`).
pub fn modules_ops_with_host(query: &str) -> grapheme_sdk::ModuleOpsPayload {
    let q = query.to_lowercase();
    let mut matches = Vec::new();
    let mut seen = BTreeSet::new();

    for manifest in discover_modules_with_host() {
        let module_id = manifest.module_id.clone();
        let module_match = module_id.to_lowercase().contains(&q);
        for op in manifest.exported_ops {
            let full = format!("{}.{}", module_id, op.op);
            if !(module_match
                || op.op.to_lowercase().contains(&q)
                || full.to_lowercase().contains(&q))
            {
                continue;
            }
            let key = format!("{module_id}.{}", op.op);
            if !seen.insert(key) {
                continue;
            }
            // Prefer typed SDK row when available (args/output from signatures).
            let typed = grapheme_sdk::modules_ops_contract(&full)
                .matches
                .into_iter()
                .find(|row| {
                    row.module_id.eq_ignore_ascii_case(&module_id)
                        && row.op.eq_ignore_ascii_case(&op.op)
                });
            matches.push(typed.unwrap_or_else(|| host_op_row(&module_id, &op)));
        }
    }

    matches.sort_by(|a, b| a.module_id.cmp(&b.module_id).then(a.op.cmp(&b.op)));
    grapheme_sdk::ModuleOpsPayload {
        query: query.to_string(),
        matches,
    }
}

fn module_info_from_manifest(manifest: &ModuleManifest) -> ModuleInfoPayload {
    let module_id = manifest.module_id.clone();
    ModuleInfoPayload {
        module_id: module_id.clone(),
        version: manifest.version.clone(),
        abi: manifest.abi.clone(),
        entrypoint: manifest.entrypoint.clone(),
        required_capabilities: manifest.required_capabilities.clone(),
        limits: manifest.limits.clone(),
        op_summary: ModuleOpSummary {
            total_ops: manifest.exported_ops.len(),
            typed_ops: 0,
            untyped_ops: manifest.exported_ops.len(),
            input_schema_refs: 0,
            output_schema_refs: 0,
        },
        exported_ops_by_effect: group_ops_by_effect(&manifest.exported_ops),
        exported_ops: manifest
            .exported_ops
            .iter()
            .map(|op| CompactModuleOp {
                op: op.op.clone(),
                stability: "stable".to_string(),
                effect: op.effect.clone(),
                args: host_op_arg_rows(&module_id, &op.op),
                input_object_type: None,
                output_type: host_output_type(&module_id, &op.op),
                output_object_type: None,
                input_schema_ref: op.input_schema_ref.clone(),
                output_schema_ref: op.output_schema_ref.clone(),
            })
            .collect(),
    }
}

fn host_op_row(module_id: &str, op: &ExportedOp) -> ModuleOpRow {
    ModuleOpRow {
        module_id: module_id.to_string(),
        stability: "stable".to_string(),
        args: host_op_arg_rows(module_id, &op.op),
        input_object_type: None,
        output_type: host_output_type(module_id, &op.op),
        output_object_type: None,
        op: op.op.clone(),
        effect: op.effect.clone(),
        input_schema_ref: op.input_schema_ref.clone(),
        output_schema_ref: op.output_schema_ref.clone(),
    }
}

fn host_op_arg_rows(module_id: &str, op: &str) -> Vec<grapheme_sdk::OperationArgRow> {
    match (module_id, op) {
        (SHELL_MODULE, "run") => vec![
            arg("command", "string", false),
            arg("argv", "array", false),
            arg("cwd", "string", false),
            arg("network", "boolean", false),
            arg("timeout_ms", "number", false),
            arg("writable_roots", "array", false),
        ],
        (SHELL_MODULE, "status") => Vec::new(),
        (MEDOUSA_MODULE, "digest") => vec![arg("text", "string", false)],
        (MEDOUSA_MODULE, "synthesize") | (MEDOUSA_MODULE, "deliver") => {
            vec![arg("payload", "any", false)]
        }
        _ => Vec::new(),
    }
}

fn arg(name: &str, ty: &str, required: bool) -> grapheme_sdk::OperationArgRow {
    grapheme_sdk::OperationArgRow {
        name: name.to_string(),
        ty: ty.to_string(),
        required,
    }
}

fn host_output_type(module_id: &str, op: &str) -> String {
    match (module_id, op) {
        (SHELL_MODULE, "run") => "object".to_string(),
        (SHELL_MODULE, "status") => "object".to_string(),
        _ => "any".to_string(),
    }
}

fn group_ops_by_effect(ops: &[ExportedOp]) -> Vec<EffectGroup> {
    let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for op in ops {
        let effect = effect_label(&op.effect).to_string();
        map.entry(effect).or_default().push(op.op.clone());
    }
    map.into_iter()
        .map(|(effect, ops)| EffectGroup { effect, ops })
        .collect()
}

fn effect_label(effect: &EffectKind) -> &'static str {
    match effect {
        EffectKind::Pure => "pure",
        EffectKind::Network => "network",
        EffectKind::Io => "io",
        EffectKind::State => "state",
        EffectKind::Secrets => "secrets",
        EffectKind::Control => "control",
    }
}

/// JSON-friendly host op catalog for a future CodeMirror completion source / docs.
pub fn host_completion_entries() -> serde_json::Value {
    let entries: Vec<_> = host_module_manifests()
        .into_iter()
        .flat_map(|manifest| {
            let module_id = manifest.module_id.clone();
            manifest
                .exported_ops
                .into_iter()
                .map(move |op| {
                    json!({
                        "label": format!("{}.{}", module_id, op.op),
                        "module_id": module_id,
                        "op": op.op,
                        "effect": effect_label(&op.effect),
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect();
    json!({ "entries": entries })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discover_includes_shell_and_medousa() {
        let ids: BTreeSet<_> = discover_modules_with_host()
            .into_iter()
            .map(|m| m.module_id)
            .collect();
        assert!(ids.contains("shell"));
        assert!(ids.contains("medousa"));
        assert!(ids.contains("core"));
    }

    #[test]
    fn shell_info_lists_run_and_status() {
        let info = modules_info_with_host("shell").expect("shell info");
        let ops: BTreeSet<_> = info.exported_ops.into_iter().map(|op| op.op).collect();
        assert!(ops.contains("run"));
        assert!(ops.contains("status"));
    }

    #[test]
    fn ops_search_finds_shell_run() {
        let payload = modules_ops_with_host("shell.run");
        assert!(payload.matches.iter().any(|row| row.module_id == "shell" && row.op == "run"));
    }
}
