//! HTTP handlers for manuscript / skill catalog (`GET /v1/manuscripts`).

use axum::extract::Query;
use axum::http::StatusCode;
use axum::Json;

use crate::daemon_api::{
    ManuscriptCatalogEntry, ManuscriptCatalogQuery, ManuscriptCatalogResponse,
    ManuscriptScriptEntry,
};
use crate::identity_manuscript::{self, ManuscriptScope};
use crate::skill_execution::discover_skill_for_manuscript;
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

        let openshell_enabled = identity_manuscript::build_manuscript_context(&entry.id)
            .ok()
            .map(|ctx| ctx.openshell_enabled)
            .unwrap_or(false);

        manuscripts.push(ManuscriptCatalogEntry {
            id: entry.id,
            name: entry.name,
            description: entry.description,
            scope: match entry.scope {
                ManuscriptScope::Project => "project".to_string(),
                ManuscriptScope::User => "user".to_string(),
            },
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
