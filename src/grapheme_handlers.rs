use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use stasis::prelude::RuntimeComposition;

use crate::daemon::route_policy::{
    BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RoutePolicy,
};
use crate::daemon_api::{
    GraphemeModuleDetailResponse, GraphemeModuleOpsResponse, GraphemeModulesListResponse,
    GraphemeRunRequest, GraphemeRunResponse, GraphemeScriptDetailResponse,
    GraphemeScriptsListQuery, GraphemeScriptsListResponse,
};
use crate::grapheme_lsp_bridge::{get_lsp_workspace, grapheme_lsp_ws};
use crate::grapheme_workshop::{
    GraphemeAllowlistResponse, GraphemeAllowlistUpdateRequest, GraphemeCompileRequest,
    GraphemeCompileResponse, GraphemeLifecycleResponse, GraphemeModuleLoadRequest,
    GraphemeModuleLoadResponse, GraphemeScriptDeleteResponse, GraphemeScriptRenameRequest,
    GraphemeScriptSaveRequest, GraphemeScriptSaveResponse, compile_source, delete_script,
    get_allowlist, lifecycle_events, load_wasm_module, rename_script, save_script,
    update_allowlist,
};

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
    Json(crate::grapheme_api::list_modules())
}

pub async fn get_grapheme_module(
    Path(module_id): Path<String>,
) -> Result<Json<GraphemeModuleDetailResponse>, (StatusCode, String)> {
    crate::grapheme_api::get_module(&module_id)
        .map(Json)
        .map_err(|error| {
            let status = if error.starts_with("unknown grapheme module") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::BAD_REQUEST
            };
            (status, error)
        })
}

pub async fn get_grapheme_module_ops(
    Path(module_id): Path<String>,
    Query(query): Query<GraphemeModuleOpsQuery>,
) -> Json<GraphemeModuleOpsResponse> {
    Json(crate::grapheme_api::get_module_ops(
        &module_id,
        query.q.as_deref(),
    ))
}

pub async fn list_grapheme_scripts(
    Query(query): Query<GraphemeScriptsListQuery>,
) -> Json<GraphemeScriptsListResponse> {
    Json(crate::grapheme_api::list_scripts(query))
}

pub async fn get_grapheme_script(
    Path(script_id): Path<String>,
) -> Result<Json<GraphemeScriptDetailResponse>, (StatusCode, String)> {
    crate::grapheme_api::get_script(&script_id)
        .map(Json)
        .map_err(|error| {
            let status = if error.contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::BAD_REQUEST
            };
            (status, error)
        })
}

pub async fn run_grapheme_source(
    State(state): State<GraphemeApiState>,
    Json(request): Json<GraphemeRunRequest>,
) -> Result<Json<GraphemeRunResponse>, (StatusCode, String)> {
    crate::grapheme_api::run_source(&state.composition, &request.source)
        .await
        .map(Json)
        .map_err(|error| {
            let status = if error == "source is required" {
                StatusCode::BAD_REQUEST
            } else if error.contains("allowlist") {
                StatusCode::FORBIDDEN
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, error)
        })
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

pub async fn delete_grapheme_script(
    Path(script_id): Path<String>,
) -> Result<Json<GraphemeScriptDeleteResponse>, (StatusCode, String)> {
    let script_id = script_id.trim();
    if script_id.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "script_id is required".to_string()));
    }
    delete_script(script_id).map(Json).map_err(|err| {
        if err.contains("not found") {
            (StatusCode::NOT_FOUND, err)
        } else {
            (StatusCode::BAD_REQUEST, err)
        }
    })
}

pub async fn post_grapheme_script_rename(
    Path(script_id): Path<String>,
    Json(request): Json<GraphemeScriptRenameRequest>,
) -> Result<Json<GraphemeScriptSaveResponse>, (StatusCode, String)> {
    let script_id = script_id.trim();
    if script_id.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "script_id is required".to_string()));
    }
    rename_script(script_id, &request.name)
        .map(Json)
        .map_err(|err| {
            if err.contains("not found") {
                (StatusCode::NOT_FOUND, err)
            } else {
                (StatusCode::BAD_REQUEST, err)
            }
        })
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

pub fn grapheme_surface() -> DeclaredRouter<GraphemeApiState> {
    use crate::request_principal::Capability;
    use axum::routing::{delete, get, post, put};

    DeclaredRouter::default()
        .route(
            grapheme_read_policy("/v1/grapheme/modules"),
            get(list_grapheme_modules),
        )
        .route(
            grapheme_read_policy("/v1/grapheme/modules/{module_id}"),
            get(get_grapheme_module),
        )
        .route(
            grapheme_read_policy("/v1/grapheme/modules/{module_id}/ops"),
            get(get_grapheme_module_ops),
        )
        .methods([
            (
                grapheme_policy(
                    axum::http::Method::GET,
                    "/v1/grapheme/allowlist",
                    Capability::AdminRuntime,
                    1024,
                    RateLimitClass::Administration,
                ),
                get(get_grapheme_allowlist),
            ),
            (
                grapheme_policy(
                    axum::http::Method::PUT,
                    "/v1/grapheme/allowlist",
                    Capability::AdminRuntime,
                    256 * 1024,
                    RateLimitClass::Administration,
                ),
                put(put_grapheme_allowlist),
            ),
        ])
        .methods([
            (
                grapheme_content_read_policy("/v1/grapheme/scripts"),
                get(list_grapheme_scripts),
            ),
            (
                grapheme_content_write_policy(
                    axum::http::Method::POST,
                    "/v1/grapheme/scripts",
                    1024 * 1024,
                ),
                post(post_grapheme_script_save),
            ),
        ])
        .route(
            grapheme_policy(
                axum::http::Method::POST,
                "/v1/grapheme/compile",
                Capability::AdminExecute,
                1024 * 1024,
                RateLimitClass::Administration,
            ),
            post(post_grapheme_compile),
        )
        .route(
            grapheme_policy(
                axum::http::Method::POST,
                "/v1/grapheme/modules/load",
                Capability::AdminRuntime,
                8 * 1024 * 1024,
                RateLimitClass::Administration,
            ),
            post(post_grapheme_module_load),
        )
        .route(
            grapheme_read_policy("/v1/grapheme/lifecycle"),
            get(get_grapheme_lifecycle),
        )
        .route(
            grapheme_read_policy("/v1/grapheme/lsp/workspace"),
            get(get_lsp_workspace),
        )
        .route(
            grapheme_policy(
                axum::http::Method::GET,
                "/v1/grapheme/lsp",
                Capability::AdminExecute,
                1024,
                RateLimitClass::Stream,
            ),
            get(grapheme_lsp_ws),
        )
        .methods([
            (
                grapheme_content_read_policy("/v1/grapheme/scripts/{script_id}"),
                get(get_grapheme_script),
            ),
            (
                grapheme_content_write_policy(
                    axum::http::Method::DELETE,
                    "/v1/grapheme/scripts/{script_id}",
                    1024,
                ),
                delete(delete_grapheme_script),
            ),
        ])
        .route(
            grapheme_content_write_policy(
                axum::http::Method::POST,
                "/v1/grapheme/scripts/{script_id}/rename",
                64 * 1024,
            ),
            post(post_grapheme_script_rename),
        )
        .route(
            grapheme_policy(
                axum::http::Method::POST,
                "/v1/grapheme/run",
                Capability::AdminExecute,
                1024 * 1024,
                RateLimitClass::Administration,
            ),
            post(run_grapheme_source),
        )
}

fn grapheme_read_policy(path: &'static str) -> RoutePolicy {
    grapheme_policy(
        axum::http::Method::GET,
        path,
        crate::request_principal::Capability::WorkshopRead,
        1024,
        RateLimitClass::Read,
    )
}

fn grapheme_content_read_policy(path: &'static str) -> RoutePolicy {
    grapheme_policy(
        axum::http::Method::GET,
        path,
        crate::request_principal::Capability::ContentRead,
        1024,
        RateLimitClass::Read,
    )
}

fn grapheme_content_write_policy(
    method: axum::http::Method,
    path: &'static str,
    body_limit: usize,
) -> RoutePolicy {
    grapheme_policy(
        method,
        path,
        crate::request_principal::Capability::ContentWrite,
        body_limit,
        RateLimitClass::Mutation,
    )
}

fn grapheme_policy(
    method: axum::http::Method,
    path: &'static str,
    required_capability: crate::request_principal::Capability,
    body_limit: usize,
    rate_limit_class: RateLimitClass,
) -> RoutePolicy {
    RoutePolicy {
        method,
        path,
        group: RouteGroup::Portal,
        required_capability: Some(required_capability),
        bootstrap_public: false,
        browser_policy: BrowserPolicy::NativeOnly,
        body_limit,
        rate_limit_class,
    }
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
