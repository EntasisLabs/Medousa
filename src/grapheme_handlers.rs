use std::collections::BTreeSet;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use grapheme_sdk::{curated_examples_for_module, discover_module_manifests, modules_info_contract, modules_ops_contract};
use serde::Deserialize;
use stasis::prelude::RuntimeComposition;

use crate::daemon_api::{
    GraphemeModuleDetailResponse, GraphemeModuleSummary, GraphemeModulesListResponse,
    GraphemeModuleOpsResponse, GraphemeRunRequest, GraphemeRunResponse, GraphemeScriptDetailResponse,
    GraphemeScriptEntryDto, GraphemeScriptsListQuery, GraphemeScriptsListResponse,
};
use crate::grapheme_lsp_bridge::{get_lsp_workspace, grapheme_lsp_ws};
use crate::grapheme_script::service::GraphemeScriptService;
use crate::grapheme_workshop::{
    compile_source, enforce_grapheme_allowlist, get_allowlist, lifecycle_events, load_wasm_module,
    save_script, update_allowlist, GraphemeAllowlistResponse, GraphemeAllowlistUpdateRequest,
    GraphemeCompileRequest, GraphemeCompileResponse, GraphemeLifecycleResponse,
    GraphemeModuleLoadRequest, GraphemeModuleLoadResponse, GraphemeScriptSaveRequest,
    GraphemeScriptSaveResponse,
};
use crate::tools::run_grapheme_via_runtime;

#[derive(Clone)]
pub struct GraphemeApiState {
    pub composition: Arc<RuntimeComposition>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GraphemeModuleOpsQuery {
    #[serde(default)]
    pub q: Option<String>,
}

pub async fn list_grapheme_modules() -> Json<GraphemeModulesListResponse> {
    let modules = discover_module_manifests()
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
    let count = modules.len();
    Json(GraphemeModulesListResponse { count, modules })
}

pub async fn get_grapheme_module(
    Path(module_id): Path<String>,
) -> Result<Json<GraphemeModuleDetailResponse>, (StatusCode, String)> {
    let module_id = module_id.trim();
    if module_id.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "module_id is required".to_string()));
    }

    let info = modules_info_contract(module_id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            format!("unknown grapheme module '{module_id}'"),
        )
    })?;

    let examples = curated_examples_for_module(module_id)
        .iter()
        .map(|path| (*path).to_string())
        .collect();

    Ok(Json(GraphemeModuleDetailResponse {
        info: serde_json::to_value(info).unwrap_or(serde_json::Value::Null),
        examples,
    }))
}

pub async fn get_grapheme_module_ops(
    Path(module_id): Path<String>,
    Query(query): Query<GraphemeModuleOpsQuery>,
) -> Json<GraphemeModuleOpsResponse> {
    let module_id = module_id.trim();
    let search = query
        .q
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(module_id);
    let payload = modules_ops_contract(search);
    Json(GraphemeModuleOpsResponse {
        module_id: module_id.to_string(),
        query: payload.query,
        matches: payload
            .matches
            .into_iter()
            .filter_map(|row| serde_json::to_value(row).ok())
            .collect(),
    })
}

pub async fn list_grapheme_scripts(
    Query(query): Query<GraphemeScriptsListQuery>,
) -> Json<GraphemeScriptsListResponse> {
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

    Json(GraphemeScriptsListResponse {
        count: scripts.len(),
        scripts,
    })
}

pub async fn get_grapheme_script(
    Path(script_id): Path<String>,
) -> Result<Json<GraphemeScriptDetailResponse>, (StatusCode, String)> {
    let script_id = script_id.trim();
    if script_id.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "script_id is required".to_string()));
    }

    let (entry, body) = GraphemeScriptService::load(script_id).map_err(|err| {
        (
            StatusCode::NOT_FOUND,
            err.to_string(),
        )
    })?;

    let body_preview = truncate_body(&body, 4000);
    Ok(Json(GraphemeScriptDetailResponse {
        script: script_entry_dto(entry),
        body_preview,
        body_truncated: body.len() > 4000,
    }))
}

pub async fn run_grapheme_source(
    State(state): State<GraphemeApiState>,
    Json(request): Json<GraphemeRunRequest>,
) -> Result<Json<GraphemeRunResponse>, (StatusCode, String)> {
    let source = request.source.trim();
    if source.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "source is required".to_string()));
    }
    enforce_grapheme_allowlist(source).map_err(|err| (StatusCode::FORBIDDEN, err))?;

    let result = run_grapheme_via_runtime(&state.composition, source, "workshop_grapheme_run")
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    Ok(Json(GraphemeRunResponse { result }))
}

pub fn script_entry_dto(entry: crate::grapheme_script::entry::GraphemeScriptEntry) -> GraphemeScriptEntryDto {
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

fn truncate_body(body: &str, max_len: usize) -> String {
    if body.len() <= max_len {
        return body.to_string();
    }
    format!("{}…", &body[..max_len])
}

pub async fn get_grapheme_allowlist() -> Json<GraphemeAllowlistResponse> {
    Json(get_allowlist().await)
}

pub async fn put_grapheme_allowlist(
    Json(request): Json<GraphemeAllowlistUpdateRequest>,
) -> Result<Json<GraphemeAllowlistResponse>, (StatusCode, String)> {
    update_allowlist(request)
        .await
        .map(Json)
        .map_err(|err| (StatusCode::BAD_REQUEST, err))
}

pub async fn post_grapheme_script_save(
    Json(request): Json<GraphemeScriptSaveRequest>,
) -> Result<Json<GraphemeScriptSaveResponse>, (StatusCode, String)> {
    save_script(request)
        .map(Json)
        .map_err(|err| (StatusCode::BAD_REQUEST, err))
}

pub async fn post_grapheme_compile(
    Json(request): Json<GraphemeCompileRequest>,
) -> Result<Json<GraphemeCompileResponse>, (StatusCode, String)> {
    compile_source(request)
        .await
        .map(Json)
        .map_err(|err| (StatusCode::BAD_REQUEST, err))
}

pub async fn post_grapheme_module_load(
    Json(request): Json<GraphemeModuleLoadRequest>,
) -> Result<Json<GraphemeModuleLoadResponse>, (StatusCode, String)> {
    load_wasm_module(request)
        .await
        .map(Json)
        .map_err(|err| (StatusCode::BAD_REQUEST, err))
}

pub async fn get_grapheme_lifecycle() -> Json<GraphemeLifecycleResponse> {
    Json(lifecycle_events().await)
}

pub fn grapheme_router(state: GraphemeApiState) -> axum::Router {
    use axum::routing::{get, post};

    axum::Router::new()
        .route(
            "/v1/grapheme/modules",
            get(list_grapheme_modules),
        )
        .route(
            "/v1/grapheme/modules/{module_id}",
            get(get_grapheme_module),
        )
        .route(
            "/v1/grapheme/modules/{module_id}/ops",
            get(get_grapheme_module_ops),
        )
        .route(
            "/v1/grapheme/allowlist",
            get(get_grapheme_allowlist).put(put_grapheme_allowlist),
        )
        .route(
            "/v1/grapheme/scripts",
            get(list_grapheme_scripts).post(post_grapheme_script_save),
        )
        .route(
            "/v1/grapheme/compile",
            post(post_grapheme_compile),
        )
        .route(
            "/v1/grapheme/modules/load",
            post(post_grapheme_module_load),
        )
        .route(
            "/v1/grapheme/lifecycle",
            get(get_grapheme_lifecycle),
        )
        .route(
            "/v1/grapheme/lsp/workspace",
            get(get_lsp_workspace),
        )
        .route(
            "/v1/grapheme/lsp",
            get(grapheme_lsp_ws),
        )
        .route(
            "/v1/grapheme/scripts/{script_id}",
            get(get_grapheme_script),
        )
        .route(
            "/v1/grapheme/run",
            post(run_grapheme_source),
        )
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use grapheme_sdk::discover_module_manifests;

    #[test]
    fn discover_modules_includes_core() {
        let modules = discover_module_manifests();
        assert!(modules.iter().any(|manifest| manifest.module_id == "core"));
    }
}
