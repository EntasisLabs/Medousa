//! HTTP handlers for manuscript / specialist catalog and editor-lite.

use axum::extract::Path;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use axum::extract::Query;

use crate::daemon_api::{
    ManuscriptCatalogEntry, ManuscriptCatalogQuery, ManuscriptCatalogResponse,
    ManuscriptDetailResponse, ManuscriptImportRequest, ManuscriptImportResponse,
    ManuscriptImportResultEntry, ManuscriptOpenshellSummary, ManuscriptScriptEntry,
    UpdateManuscriptRequest,
};
use crate::identity_manuscript::{
    self, ManuscriptScope, build_manuscript_context, palette_tools_for_editor,
    scheduled_tool_preview, validate_manuscript_for_scheduled_lane,
};
use crate::skill_execution::discover_skill_for_manuscript;
use crate::skill_import::{SkillImportPreset, import_skills_at_path, import_skills_from_roots, preset_skill_roots};
use crate::skill_ingest::risk_class_label;

pub async fn list_manuscripts_catalog(
    Query(query): Query<ManuscriptCatalogQuery>,
) -> Result<Json<ManuscriptCatalogResponse>, (StatusCode, String)> {
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let skills_only = query.skills_only.unwrap_or(false);
    let prefix = query
        .prefix
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let mut entries =
        identity_manuscript::list_manuscripts().map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    if let Some(prefix) = prefix {
        entries.retain(|entry| entry.id.starts_with(prefix));
    }

    let mut manuscripts = Vec::new();
    for entry in entries {
        let discovery = discover_skill_for_manuscript(&entry.id).ok();
        let has_scripts = discovery.as_ref().is_some_and(|item| item.has_scripts);
        if skills_only && !has_scripts {
            continue;
        }

        let scripts = discovery
            .map(|item| {
                item.scripts
                    .into_iter()
                    .map(|script| ManuscriptScriptEntry {
                        relative_path: script.relative_path,
                        risk_class: risk_class_label(script.risk_class).to_string(),
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let openshell_enabled = build_manuscript_context(&entry.id)
            .ok()
            .map(|ctx| ctx.openshell_enabled)
            .unwrap_or(false);

        manuscripts.push(ManuscriptCatalogEntry {
            id: entry.id,
            name: entry.name,
            description: entry.description,
            scope: scope_label(entry.scope),
            path: entry.path.display().to_string(),
            has_scripts,
            scripts,
            openshell_enabled,
        });

        if manuscripts.len() >= limit {
            break;
        }
    }

    let count = manuscripts.len();
    Ok(Json(ManuscriptCatalogResponse { count, manuscripts }))
}

pub async fn get_manuscript_detail(
    Path(manuscript_id): Path<String>,
) -> Result<Json<ManuscriptDetailResponse>, (StatusCode, String)> {
    let manuscript_id = manuscript_id.trim();
    if manuscript_id.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "manuscript_id is required".to_string()));
    }

    let context = build_manuscript_context(manuscript_id).map_err(|err| {
        (
            StatusCode::NOT_FOUND,
            err.to_string(),
        )
    })?;

    let listing = identity_manuscript::list_manuscripts()
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
        .into_iter()
        .find(|entry| entry.id == manuscript_id);

    let scope = listing
        .as_ref()
        .map(|entry| scope_label(entry.scope))
        .unwrap_or_else(|| "user".to_string());

    let discovery = discover_skill_for_manuscript(manuscript_id).ok();
    let has_scripts = discovery.as_ref().is_some_and(|item| item.has_scripts);
    let scripts = discovery
        .map(|item| {
            item.scripts
                .into_iter()
                .map(|script| ManuscriptScriptEntry {
                    relative_path: script.relative_path,
                    risk_class: risk_class_label(script.risk_class).to_string(),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let schedule_validation_error = validate_manuscript_for_scheduled_lane(&context)
        .err()
        .map(|err| err.to_string());
    let schedule_ready = schedule_validation_error.is_none();

    Ok(Json(build_detail_response(
        context,
        scope,
        has_scripts,
        scripts,
        schedule_ready,
        schedule_validation_error,
    )))
}

pub async fn patch_manuscript_detail(
    Path(manuscript_id): Path<String>,
    Json(request): Json<UpdateManuscriptRequest>,
) -> Result<Json<ManuscriptDetailResponse>, (StatusCode, String)> {
    let manuscript_id = manuscript_id.trim();
    if manuscript_id.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "manuscript_id is required".to_string()));
    }

    let context = identity_manuscript::apply_editor_lite_update(manuscript_id, &request).map_err(
        |err| (StatusCode::BAD_REQUEST, err.to_string()),
    )?;

    let listing = identity_manuscript::list_manuscripts()
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
        .into_iter()
        .find(|entry| entry.id == manuscript_id);

    let scope = listing
        .as_ref()
        .map(|entry| scope_label(entry.scope))
        .unwrap_or_else(|| "user".to_string());

    let discovery = discover_skill_for_manuscript(manuscript_id).ok();
    let has_scripts = discovery.as_ref().is_some_and(|item| item.has_scripts);
    let scripts = discovery
        .map(|item| {
            item.scripts
                .into_iter()
                .map(|script| ManuscriptScriptEntry {
                    relative_path: script.relative_path,
                    risk_class: risk_class_label(script.risk_class).to_string(),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let schedule_validation_error = validate_manuscript_for_scheduled_lane(&context)
        .err()
        .map(|err| err.to_string());
    let schedule_ready = schedule_validation_error.is_none();

    Ok(Json(build_detail_response(
        context,
        scope,
        has_scripts,
        scripts,
        schedule_ready,
        schedule_validation_error,
    )))
}

pub async fn import_manuscripts(
    Json(request): Json<ManuscriptImportRequest>,
) -> Result<Json<ManuscriptImportResponse>, (StatusCode, String)> {
    let scope = parse_scope(request.scope.as_deref())?;
    let force = request.force.unwrap_or(false);

    let results = if let Some(preset) = request.preset.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        let preset = parse_preset(preset)?;
        let roots = preset_skill_roots(preset);
        import_skills_from_roots(&roots, scope, force, None).map_err(|err| {
            (StatusCode::BAD_REQUEST, err.to_string())
        })?
    } else if let Some(path) = request.path.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        import_skills_at_path(std::path::Path::new(path), scope, force, None).map_err(|err| {
            (StatusCode::BAD_REQUEST, err.to_string())
        })?
    } else {
        return Err((
            StatusCode::BAD_REQUEST,
            "path or preset is required".to_string(),
        ));
    };

    let imported = results
        .into_iter()
        .map(|result| ManuscriptImportResultEntry {
            id: result.id,
            name: result.name,
            yaml_path: result.yaml_path.display().to_string(),
            source: result.source.display().to_string(),
        })
        .collect::<Vec<_>>();
    let count = imported.len();
    Ok(Json(ManuscriptImportResponse { count, imported }))
}

fn build_detail_response(
    context: identity_manuscript::ManuscriptContext,
    scope: String,
    has_scripts: bool,
    scripts: Vec<ManuscriptScriptEntry>,
    schedule_ready: bool,
    schedule_validation_error: Option<String>,
) -> ManuscriptDetailResponse {
    ManuscriptDetailResponse {
        id: context.id.clone(),
        name: context.name.clone(),
        description: context.description.clone(),
        scope,
        path: context.source_path.display().to_string(),
        extends_from: context.extends_from.clone(),
        task_template: context.task_template.clone(),
        tools_allow: context.tools_allow.clone(),
        schedule_cron: context.schedule_cron.clone(),
        schedule_execution_mode: context.schedule_execution_mode.clone(),
        delivery_mode: context.delivery_mode.clone(),
        delivery_on_complete: context.delivery_on_complete.clone(),
        openshell: ManuscriptOpenshellSummary {
            enabled: context.openshell_enabled,
            policy_template: context.openshell_policy_template.clone(),
            sandbox_from: context.openshell_sandbox_from.clone(),
            allow_scheduled: context.openshell_allow_scheduled,
            default_path: "Grapheme modules (Workshop → Modules)".to_string(),
        },
        has_scripts,
        scripts,
        scheduled_tools: scheduled_tool_preview(&context),
        schedule_ready,
        schedule_validation_error,
        palette_tools: palette_tools_for_editor(),
    }
}

fn scope_label(scope: ManuscriptScope) -> String {
    match scope {
        ManuscriptScope::Project => "project".to_string(),
        ManuscriptScope::User => "user".to_string(),
    }
}

fn parse_scope(raw: Option<&str>) -> Result<ManuscriptScope, (StatusCode, String)> {
    match raw.unwrap_or("user").trim().to_ascii_lowercase().as_str() {
        "project" => Ok(ManuscriptScope::Project),
        "user" => Ok(ManuscriptScope::User),
        other => Err((
            StatusCode::BAD_REQUEST,
            format!("unsupported scope '{other}' (expected user or project)"),
        )),
    }
}

fn parse_preset(raw: &str) -> Result<SkillImportPreset, (StatusCode, String)> {
    match raw.to_ascii_lowercase().as_str() {
        "hermes" => Ok(SkillImportPreset::Hermes),
        "openclaw" => Ok(SkillImportPreset::OpenClaw),
        "cursor" => Ok(SkillImportPreset::Cursor),
        other => Err((
            StatusCode::BAD_REQUEST,
            format!("unsupported preset '{other}' (expected hermes, openclaw, or cursor)"),
        )),
    }
}

pub fn manuscript_router() -> Router {
    Router::new()
        .route(
            "/v1/manuscripts",
            get(list_manuscripts_catalog).post(import_manuscripts),
        )
        .route(
            "/v1/manuscripts/{manuscript_id}",
            get(get_manuscript_detail).patch(patch_manuscript_detail),
        )
}
