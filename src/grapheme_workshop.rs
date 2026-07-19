//! In-process Grapheme workshop session — compile hints, WASM hot-load, lifecycle events.

use std::path::PathBuf;
use std::sync::OnceLock;

use grapheme_compiler::{Compiler, CompilerOptions};
use grapheme_runtime::{CompatibilityMode, LoadModuleRequest, ModuleAbi, ModuleLifecycleEvent};
use grapheme_sdk::{GraphemeEngine, GraphemeRuntimeSession, GraphemeSdkError};
use tokio::sync::Mutex;

use crate::capability_catalog::{grapheme_allowed_modules, set_grapheme_allowed_modules};
use crate::tools::extract_module_ops_from_source;

pub use medousa_types::grapheme_extras::*;

static WORKSHOP_SESSION: OnceLock<Mutex<WorkshopGraphemeSession>> = OnceLock::new();

struct WorkshopGraphemeSession {
    engine: GraphemeEngine,
    session: GraphemeRuntimeSession,
}

fn workshop_session() -> &'static Mutex<WorkshopGraphemeSession> {
    WORKSHOP_SESSION.get_or_init(|| {
        let engine = crate::grapheme_medousa_bridge::configure_grapheme_engine_builder(
            GraphemeEngine::builder(),
        )
        .build();
        let session = engine.runtime_session();
        Mutex::new(WorkshopGraphemeSession { engine, session })
    })
}

pub fn enforce_grapheme_allowlist(source: &str) -> Result<(), String> {
    let allowed = grapheme_allowed_modules();
    if allowed.is_empty() {
        return Ok(());
    }
    let allowed_lower: Vec<String> = allowed.iter().map(|m| m.to_ascii_lowercase()).collect();
    for op in extract_module_ops_from_source(source) {
        let module = op.split('.').next().unwrap_or("").to_ascii_lowercase();
        if module.is_empty() {
            continue;
        }
        if !allowed_lower.iter().any(|entry| entry == &module) {
            return Err(format!(
                "Grapheme module '{module}' is not in the workshop allowlist"
            ));
        }
    }
    Ok(())
}

pub async fn get_allowlist() -> GraphemeAllowlistResponse {
    let allowed_modules = grapheme_allowed_modules();
    GraphemeAllowlistResponse {
        enforce: !allowed_modules.is_empty(),
        allowed_modules,
    }
}

pub async fn update_allowlist(
    request: GraphemeAllowlistUpdateRequest,
) -> Result<GraphemeAllowlistResponse, String> {
    set_grapheme_allowed_modules(request.allowed_modules)?;
    Ok(get_allowlist().await)
}

pub fn save_script(request: GraphemeScriptSaveRequest) -> Result<GraphemeScriptSaveResponse, String> {
    let name = request.name.trim();
    let body = request.body.trim();
    if name.is_empty() {
        return Err("name is required".to_string());
    }
    if body.is_empty() {
        return Err("body is required".to_string());
    }
    enforce_grapheme_allowlist(body)?;
    let entry = crate::grapheme_script::service::GraphemeScriptService::save(
        request.id.as_deref(),
        name,
        body,
        request.modules,
        request.tags,
        request.intent,
        request.source_session_id,
    )
    .map_err(|err| err.to_string())?;
    Ok(GraphemeScriptSaveResponse {
        script: crate::grapheme_handlers::script_entry_dto(entry),
    })
}

pub fn delete_script(script_id: &str) -> Result<GraphemeScriptDeleteResponse, String> {
    let entry = crate::grapheme_script::service::GraphemeScriptService::delete(script_id)
        .map_err(|err| err.to_string())?;
    Ok(GraphemeScriptDeleteResponse {
        deleted: true,
        id: entry.id,
        name: entry.name,
    })
}

pub fn rename_script(
    script_id: &str,
    name: &str,
) -> Result<GraphemeScriptSaveResponse, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("name is required".to_string());
    }
    let entry = crate::grapheme_script::service::GraphemeScriptService::rename(script_id, name)
        .map_err(|err| err.to_string())?;
    Ok(GraphemeScriptSaveResponse {
        script: crate::grapheme_handlers::script_entry_dto(entry),
    })
}

pub async fn compile_source(request: GraphemeCompileRequest) -> Result<GraphemeCompileResponse, String> {
    let source = request.source.trim();
    if source.is_empty() {
        return Err("source is required".to_string());
    }
    enforce_grapheme_allowlist(source)?;

    let mode = request
        .mode
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("check")
        .to_ascii_lowercase();

    let guard = workshop_session().lock().await;

    if mode == "aot" {
        let aot = guard
            .engine
            .compile_source_to_aot(source)
            .map_err(|err| err.to_string())?;
        return Ok(GraphemeCompileResponse {
            mode: "aot".to_string(),
            validated: true,
            artifact_id: Some(aot.base_artifact.artifact_id.clone()),
            lint_warnings: Vec::new(),
            compile_hints: vec![
                "AOT envelope ready — repeat runs can use execute_aot for lower latency.".to_string(),
                format!("Stage: {:?}", aot.stage),
            ],
            aot_stage: Some(format!("{:?}", aot.stage)),
        });
    }

    let compiled = Compiler::compile_source(source, CompilerOptions::default())
        .map_err(|err| err.to_string())?;
    let lint_warnings: Vec<String> = compiled
        .compilation
        .lint_warnings
        .iter()
        .map(|warning| format!("{warning:?}"))
        .collect();
    let mut compile_hints = Vec::new();
    if lint_warnings.is_empty() {
        compile_hints.push("Source compiles cleanly.".to_string());
    } else {
        compile_hints.push(format!(
            "{} lint warning(s) — review before scheduling.",
            lint_warnings.len()
        ));
    }
    compile_hints.push("Use mode=aot for repeat-run compile hints.".to_string());

    Ok(GraphemeCompileResponse {
        mode: "check".to_string(),
        validated: true,
        artifact_id: Some(compiled.artifact.artifact_id.clone()),
        lint_warnings,
        compile_hints,
        aot_stage: None,
    })
}

fn parse_module_abi(raw: Option<&str>) -> ModuleAbi {
    match raw.unwrap_or("wasix_v1").trim().to_ascii_lowercase().as_str() {
        "mir_v1" | "mir" => ModuleAbi::MirV1,
        "wasix_wit_v15" | "wit" => ModuleAbi::WasixWitV15,
        _ => ModuleAbi::WasixV1,
    }
}

fn parse_compatibility_mode(raw: Option<&str>) -> CompatibilityMode {
    match raw.unwrap_or("strict").trim().to_ascii_lowercase().as_str() {
        "permissive" => CompatibilityMode::Permissive,
        _ => CompatibilityMode::Strict,
    }
}

pub async fn load_wasm_module(
    request: GraphemeModuleLoadRequest,
) -> Result<GraphemeModuleLoadResponse, String> {
    let module_id = request.module_id.trim();
    let wasm_path = request.wasm_path.trim();
    if module_id.is_empty() {
        return Err("module_id is required".to_string());
    }
    if wasm_path.is_empty() {
        return Err("wasm_path is required".to_string());
    }
    let path = PathBuf::from(wasm_path);
    if !path.is_file() {
        return Err(format!("wasm file not found: {wasm_path}"));
    }

    let allowed = grapheme_allowed_modules();
    if !allowed.is_empty()
        && !allowed
            .iter()
            .any(|entry| entry.eq_ignore_ascii_case(module_id))
    {
        return Err(format!(
            "module '{module_id}' is not in the workshop allowlist"
        ));
    }

    let mut guard = workshop_session().lock().await;
    let activation = guard
        .session
        .activate_module_generation(LoadModuleRequest {
            module_id: module_id.to_string(),
            wasm_path: path,
            compatibility_mode: parse_compatibility_mode(request.compatibility_mode.as_deref()),
            abi: parse_module_abi(request.abi.as_deref()),
            version: request
                .version
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
        })
        .map_err(|err: GraphemeSdkError| err.to_string())?;

    Ok(GraphemeModuleLoadResponse {
        module_id: activation.module_id,
        generation_id: activation.generation_id,
        version: activation.version,
        content_hash: activation.content_hash,
    })
}

pub async fn lifecycle_events() -> GraphemeLifecycleResponse {
    let guard = workshop_session().lock().await;
    let events = guard
        .session
        .module_lifecycle_events()
        .into_iter()
        .map(lifecycle_event_to_dto)
        .collect();
    GraphemeLifecycleResponse { events }
}

fn lifecycle_event_to_dto(event: ModuleLifecycleEvent) -> GraphemeLifecycleEventDto {
    GraphemeLifecycleEventDto {
        kind: format!("{:?}", event.kind),
        module_id: event.module_id,
        generation_id: Some(event.generation_id),
        message: event.reason,
    }
}
