//! Transport-neutral Grapheme operations owned by the daemon.

use std::collections::BTreeSet;
use std::sync::Arc;

use medousa_types::daemon_api::{
    GraphemeModuleDetailResponse, GraphemeModuleOpsResponse, GraphemeModuleSummary,
    GraphemeModulesListResponse, GraphemeRunResponse, GraphemeScriptDetailResponse,
    GraphemeScriptEntryDto, GraphemeScriptsListQuery, GraphemeScriptsListResponse,
};
use stasis::prelude::RuntimeComposition;

use crate::grapheme_script::service::GraphemeScriptService;

#[cfg(feature = "full-daemon")]
use crate::grapheme_host_catalog::{
    discover_modules_with_host as discover_modules, examples_for_module as module_examples,
    modules_info_with_host as module_info, modules_ops_with_host as module_ops,
};

#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
fn discover_modules() -> Vec<grapheme_runtime::ModuleManifest> {
    grapheme_sdk::discover_module_manifests()
}

#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
fn module_info(module_id: &str) -> Option<grapheme_sdk::ModuleInfoPayload> {
    grapheme_sdk::modules_info_contract(module_id)
}

#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
fn module_examples(module_id: &str) -> Vec<String> {
    grapheme_sdk::curated_examples_for_module(module_id)
        .iter()
        .map(|path| (*path).to_string())
        .collect()
}

#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
fn module_ops(query: &str) -> grapheme_sdk::ModuleOpsPayload {
    grapheme_sdk::modules_ops_contract(query)
}

pub fn list_modules() -> GraphemeModulesListResponse {
    let modules = discover_modules()
        .into_iter()
        .map(|manifest| {
            let effects = manifest
                .exported_ops
                .iter()
                .filter_map(|op| {
                    serde_json::to_value(&op.effect)
                        .ok()
                        .and_then(|value| value.as_str().map(str::to_string))
                })
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();

            GraphemeModuleSummary {
                module_id: manifest.module_id,
                version: manifest.version,
                abi: serde_json::to_value(&manifest.abi)
                    .ok()
                    .and_then(|value| value.as_str().map(str::to_string))
                    .unwrap_or_else(|| "unknown".to_string()),
                entrypoint: manifest.entrypoint,
                op_count: manifest.exported_ops.len(),
                effects,
                required_capabilities: manifest.required_capabilities,
            }
        })
        .collect::<Vec<_>>();
    GraphemeModulesListResponse {
        count: modules.len(),
        modules,
    }
}

pub fn get_module(module_id: &str) -> Result<GraphemeModuleDetailResponse, String> {
    let module_id = module_id.trim();
    if module_id.is_empty() {
        return Err("module_id is required".to_string());
    }
    let info =
        module_info(module_id).ok_or_else(|| format!("unknown grapheme module '{module_id}'"))?;
    Ok(GraphemeModuleDetailResponse {
        info: serde_json::to_value(info).unwrap_or(serde_json::Value::Null),
        examples: module_examples(module_id),
    })
}

pub fn get_module_ops(module_id: &str, query: Option<&str>) -> GraphemeModuleOpsResponse {
    let module_id = module_id.trim();
    let search = query
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(module_id);
    let payload = module_ops(search);
    GraphemeModuleOpsResponse {
        module_id: module_id.to_string(),
        query: payload.query,
        matches: payload
            .matches
            .into_iter()
            .filter_map(|row| serde_json::to_value(row).ok())
            .collect(),
    }
}

pub fn list_scripts(query: GraphemeScriptsListQuery) -> GraphemeScriptsListResponse {
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let scripts: Vec<GraphemeScriptEntryDto> = if let Some(search) = query
        .query
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        GraphemeScriptService::search_ranked(
            search,
            query.module.as_deref(),
            query.tag.as_deref(),
            limit,
        )
        .into_iter()
        .map(|hit| GraphemeScriptEntryDto {
            id: hit.id,
            name: hit.name,
            modules: hit.modules,
            tags: hit.tags,
            intent: hit.intent,
            version: hit.version,
            score: Some(hit.score),
            line: Some(hit.line),
            body_path: None,
            body_hash: None,
            created_at_utc: None,
            updated_at_utc: None,
            source_session_id: None,
            body_preview: None,
        })
        .collect()
    } else {
        GraphemeScriptService::list(query.module.as_deref(), query.tag.as_deref(), limit)
            .into_iter()
            .map(script_entry_dto)
            .collect()
    };

    GraphemeScriptsListResponse {
        count: scripts.len(),
        scripts,
    }
}

pub fn get_script(script_id: &str) -> Result<GraphemeScriptDetailResponse, String> {
    let script_id = script_id.trim();
    if script_id.is_empty() {
        return Err("script_id is required".to_string());
    }
    let (entry, body) = GraphemeScriptService::load(script_id).map_err(|err| err.to_string())?;
    let body_preview = body.chars().take(4000).collect::<String>();
    Ok(GraphemeScriptDetailResponse {
        script: script_entry_dto(entry),
        body_preview,
        body_truncated: body.chars().count() > 4000,
    })
}

pub async fn run_source(
    runtime: &Arc<RuntimeComposition>,
    source: &str,
) -> Result<GraphemeRunResponse, String> {
    let source = source.trim();
    if source.is_empty() {
        return Err("source is required".to_string());
    }
    crate::grapheme_workshop::enforce_grapheme_allowlist(source)?;
    let result =
        crate::grapheme_runtime::run_grapheme_via_runtime(runtime, source, "workshop_grapheme_run")
            .await
            .map_err(|err| err.to_string())?;
    Ok(GraphemeRunResponse { result })
}

pub fn script_entry_dto(
    entry: crate::grapheme_script::entry::GraphemeScriptEntry,
) -> GraphemeScriptEntryDto {
    GraphemeScriptEntryDto {
        id: entry.id,
        name: entry.name,
        modules: entry.modules,
        tags: entry.tags,
        intent: entry.intent,
        version: entry.version,
        score: None,
        line: None,
        body_path: Some(entry.body_path),
        body_hash: Some(entry.body_hash),
        created_at_utc: Some(entry.created_at_utc),
        updated_at_utc: Some(entry.updated_at_utc),
        source_session_id: entry.source_session_id,
        body_preview: None,
    }
}
