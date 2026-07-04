use anyhow::{Context, Result, bail};
use base64::Engine;
use chrono::Utc;
use medousa_types::environment::{ComponentDef, EnvironmentSpec, SurfaceDef, SurfaceKind, SurfaceLayout};

use crate::artifact_store::{fetch_artifact_at_id, list_ui_artifacts, persist_ui_artifact, FetchedArtifact};
use crate::environment_store::{EnvironmentHub, resolve_profile_id};
use crate::share::bundle::{
    ShareArtifactEntry, ShareBundle, ShareConflictStrategy, ShareExportRequest, ShareImportRequest,
    ShareImportResult, ShareSourceWorkshop, ShareVaultNoteEntry, SHARE_BUNDLE_VERSION,
};
use crate::vault::store::vault_store;

const SHARE_IMPORT_SESSION: &str = "__share_import__";

fn resolve_artifact_for_export(artifact_id: &str) -> Option<FetchedArtifact> {
    let id = artifact_id.trim();
    if id.is_empty() {
        return None;
    }
    if let Some(fetched) = fetch_artifact_at_id(SHARE_IMPORT_SESSION, id)
        .or_else(|| fetch_artifact_at_id("", id))
    {
        return Some(fetched);
    }
    let records = list_ui_artifacts(None, 500, Some(id));
    let record = records.into_iter().find(|entry| {
        entry.artifact_id == id || entry.artifact_id.starts_with(id) || id.starts_with(&entry.artifact_id)
    })?;
    fetch_artifact_at_id(&record.session_id, &record.artifact_id)
}

fn artifact_entry_from_fetched(fetched: FetchedArtifact) -> ShareArtifactEntry {
    ShareArtifactEntry {
        id: fetched.record.artifact_id.clone(),
        title: fetched
            .record
            .label
            .clone()
            .unwrap_or_else(|| "Shared artifact".to_string()),
        mime: fetched.mime.clone(),
        content_base64: base64::engine::general_purpose::STANDARD.encode(fetched.body.as_bytes()),
        presentation: fetched.record.presentation.clone(),
        height_px: fetched.record.height_px.map(|value| value as u32),
    }
}

/// Export a single UI artifact as a mini share bundle.
pub fn export_single_artifact(
    artifact_id: &str,
    source: ShareSourceWorkshop,
) -> Result<ShareBundle> {
    export_bundle(
        ShareExportRequest {
            artifact_ids: vec![artifact_id.trim().to_string()],
            ..Default::default()
        },
        source,
    )
}

/// Export a single vault note as a mini share bundle.
pub fn export_single_vault_note(path: &str, source: ShareSourceWorkshop) -> Result<ShareBundle> {
    export_bundle(
        ShareExportRequest {
            vault_paths: vec![path.trim().to_string()],
            ..Default::default()
        },
        source,
    )
}

pub fn export_bundle(
    request: ShareExportRequest,
    source: ShareSourceWorkshop,
) -> Result<ShareBundle> {
    let mut artifacts = Vec::new();
    for artifact_id in &request.artifact_ids {
        let id = artifact_id.trim();
        if id.is_empty() {
            continue;
        }
        let fetched = resolve_artifact_for_export(id)
            .with_context(|| format!("artifact '{id}' not found"))?;
        artifacts.push(artifact_entry_from_fetched(fetched));
    }

    let store = vault_store();
    let _ = store.refresh_from_disk();
    let mut vault_notes = Vec::new();
    for path in &request.vault_paths {
        let normalized = path.trim();
        if normalized.is_empty() {
            continue;
        }
        let content = store
            .read_content(normalized)
            .with_context(|| format!("vault note '{normalized}' not found"))?;
        vault_notes.push(ShareVaultNoteEntry {
            path: normalized.to_string(),
            content,
        });
    }

    let environment = if request.include_environment {
        let profile_id = resolve_profile_id(request.profile_id.as_deref());
        let runtime = tokio::runtime::Handle::try_current();
        let record = if let Ok(handle) = runtime {
            handle.block_on(async { EnvironmentHub::load_or_default(&profile_id).await })?
        } else {
            tokio::runtime::Runtime::new()?
                .block_on(async { EnvironmentHub::load_or_default(&profile_id).await })?
        };
        Some(filter_environment(
            &record.spec,
            &request.surface_ids,
            &request.component_ids,
        ))
    } else {
        None
    };

    Ok(ShareBundle {
        version: SHARE_BUNDLE_VERSION,
        exported_at: Utc::now(),
        source_workshop: source,
        artifacts,
        vault_notes,
        environment,
    })
}

fn filter_environment(
    spec: &EnvironmentSpec,
    surface_ids: &[String],
    component_ids: &[String],
) -> crate::share::bundle::ShareEnvironmentSection {
    use crate::share::bundle::ShareEnvironmentSection;

    let surface_filter: std::collections::HashSet<&str> = surface_ids
        .iter()
        .map(String::as_str)
        .filter(|value| !value.is_empty())
        .collect();
    let component_filter: std::collections::HashSet<&str> = component_ids
        .iter()
        .map(String::as_str)
        .filter(|value| !value.is_empty())
        .collect();

    let surfaces: Vec<SurfaceDef> = if surface_filter.is_empty() {
        spec.surfaces
            .iter()
            .filter(|surface| surface.kind == SurfaceKind::Custom)
            .cloned()
            .collect()
    } else {
        spec.surfaces
            .iter()
            .filter(|surface| surface_filter.contains(surface.id.as_str()))
            .cloned()
            .collect()
    };

    let surface_ids: std::collections::HashSet<String> =
        surfaces.iter().map(|surface| surface.id.clone()).collect();

    let components = if component_filter.is_empty() {
        spec.components
            .iter()
            .filter(|component| surface_ids.contains(&component.surface_id))
            .cloned()
            .collect()
    } else {
        spec.components
            .iter()
            .filter(|component| component_filter.contains(component.id.as_str()))
            .cloned()
            .collect()
    };

    let layout_presets = spec
        .layout_presets
        .clone()
        .unwrap_or_default()
        .into_iter()
        .map(|mut preset| {
            preset.surfaces.retain(|id| surface_ids.contains(id));
            preset.active = false;
            preset
        })
        .filter(|preset| !preset.surfaces.is_empty())
        .collect();

    ShareEnvironmentSection {
        surfaces,
        components,
        layout_presets,
    }
}

pub async fn import_bundle(
    hub: &EnvironmentHub,
    request: ShareImportRequest,
) -> Result<ShareImportResult> {
    let errors = request.bundle.validate();
    if !errors.is_empty() {
        bail!(errors.join("; "));
    }

    let mut result = ShareImportResult::default();
    let store = vault_store();
    let _ = store.refresh_from_disk();

    for artifact in &request.bundle.artifacts {
        match import_artifact(artifact, request.conflict_strategy) {
            Ok(Some(mapped)) => {
                result.artifacts_imported += 1;
                if mapped.0 != mapped.1 {
                    result.artifact_id_map.push(mapped);
                }
            }
            Ok(None) => result.artifacts_skipped += 1,
            Err(err) => result.warnings.push(format!("artifact {}: {err:#}", artifact.id)),
        }
    }

    for note in &request.bundle.vault_notes {
        match import_vault_note(note, request.conflict_strategy, store) {
            Ok(true) => result.vault_notes_imported += 1,
            Ok(false) => result.vault_notes_skipped += 1,
            Err(err) => result.warnings.push(format!("vault {}: {err:#}", note.path)),
        }
    }

    if let Some(environment) = &request.bundle.environment {
        let profile_id = resolve_profile_id(request.profile_id.as_deref());
        let record = hub.get(&profile_id).await?;
        let id_map: std::collections::HashMap<String, String> =
            result.artifact_id_map.iter().cloned().collect();
        let merged = merge_environment(
            &record.spec,
            environment,
            request.conflict_strategy,
            &id_map,
            &mut result,
        )?;
        let errors = medousa_types::environment_validate::validate_environment_spec(&merged);
        if !errors.is_empty() {
            bail!(errors.join("; "));
        }
        hub.put(merged, "share-import").await?;
    }

    Ok(result)
}

fn import_artifact(
    artifact: &ShareArtifactEntry,
    strategy: ShareConflictStrategy,
) -> Result<Option<(String, String)>> {
    let body = String::from_utf8(
        base64::engine::general_purpose::STANDARD
            .decode(artifact.content_base64.as_bytes())
            .context("decode artifact body")?,
    )
    .context("artifact body must be utf-8")?;

    let exists = fetch_artifact_at_id(SHARE_IMPORT_SESSION, &artifact.id).is_some();
    if exists && strategy == ShareConflictStrategy::Skip {
        return Ok(None);
    }

    let label = artifact.title.clone();
    let presentation = artifact
        .presentation
        .as_deref()
        .unwrap_or("inline");
    let record = persist_ui_artifact(
        SHARE_IMPORT_SESSION,
        &body,
        &label,
        presentation,
        artifact.height_px,
    )
    .map_err(|err| anyhow::anyhow!(err))?;

    Ok(Some((artifact.id.clone(), record.artifact_id)))
}

fn import_vault_note(
    note: &ShareVaultNoteEntry,
    strategy: ShareConflictStrategy,
    store: &crate::vault::store::VaultStore,
) -> Result<bool> {
    let exists = store.get_entry(&note.path).is_some();
    if exists && strategy == ShareConflictStrategy::Skip {
        return Ok(false);
    }

    let path = if exists && strategy == ShareConflictStrategy::Rename {
        rename_path(&note.path)
    } else {
        note.path.clone()
    };

    store.write_content(&path, &note.content, None)?;
    Ok(true)
}

fn rename_path(path: &str) -> String {
    let trimmed = path.trim().trim_end_matches(".md");
    format!("{trimmed}-imported.md")
}

fn merge_environment(
    existing: &EnvironmentSpec,
    incoming: &crate::share::bundle::ShareEnvironmentSection,
    strategy: ShareConflictStrategy,
    artifact_id_map: &std::collections::HashMap<String, String>,
    result: &mut ShareImportResult,
) -> Result<EnvironmentSpec> {
    let mut spec = existing.clone();
    let existing_surface_ids: std::collections::HashSet<String> =
        spec.surfaces.iter().map(|surface| surface.id.clone()).collect();
    let existing_component_ids: std::collections::HashSet<String> = spec
        .components
        .iter()
        .map(|component| component.id.clone())
        .collect();

    for surface in &incoming.surfaces {
        if existing_surface_ids.contains(&surface.id) {
            match strategy {
                ShareConflictStrategy::Skip => continue,
                ShareConflictStrategy::Overwrite => {
                    spec.surfaces.retain(|entry| entry.id != surface.id);
                    spec.surfaces.push(surface.clone());
                    result.surfaces_imported += 1;
                }
                ShareConflictStrategy::Rename => {
                    let mut renamed = surface.clone();
                    renamed.id = rename_surface_id(&surface.id, &existing_surface_ids);
                    spec.surfaces.push(renamed);
                    result.surfaces_imported += 1;
                }
            }
        } else {
            spec.surfaces.push(surface.clone());
            result.surfaces_imported += 1;
        }
    }

    for component in &incoming.components {
        let mut next = remap_component_artifacts(component.clone(), artifact_id_map);
        if existing_component_ids.contains(&next.id) {
            match strategy {
                ShareConflictStrategy::Skip => continue,
                ShareConflictStrategy::Overwrite => {
                    spec.components.retain(|entry| entry.id != next.id);
                    spec.components.push(next);
                    result.components_imported += 1;
                }
                ShareConflictStrategy::Rename => {
                    next.id = rename_component_id(&next.id, &existing_component_ids);
                    spec.components.push(next);
                    result.components_imported += 1;
                }
            }
        } else {
            spec.components.push(next);
            result.components_imported += 1;
        }
    }

    if !incoming.layout_presets.is_empty() {
        let presets = spec.layout_presets.get_or_insert_with(Vec::new);
        for preset in &incoming.layout_presets {
            if presets.iter().any(|entry| entry.id == preset.id) {
                if strategy == ShareConflictStrategy::Skip {
                    continue;
                }
                presets.retain(|entry| entry.id != preset.id);
            }
            presets.push(preset.clone());
            result.layout_presets_imported += 1;
        }
    }

    spec.updated_at = Utc::now();
    spec.updated_by = "share-import".to_string();
    Ok(spec)
}

fn remap_component_artifacts(
    mut component: ComponentDef,
    artifact_id_map: &std::collections::HashMap<String, String>,
) -> ComponentDef {
    let Some(artifact_id) = component
        .config
        .get("artifactId")
        .or_else(|| component.config.get("artifact_id"))
        .and_then(|value| value.as_str())
        .map(str::to_string)
    else {
        return component;
    };
    let Some(mapped) = artifact_id_map.get(&artifact_id) else {
        return component;
    };
    if let Some(config) = component.config.as_object_mut() {
        config.insert(
            "artifactId".to_string(),
            serde_json::Value::String(mapped.clone()),
        );
    }
    component
}

fn rename_surface_id(base: &str, existing: &std::collections::HashSet<String>) -> String {
    let root = format!("{base}-imported");
    if !existing.contains(&root) {
        return root;
    }
    let mut index = 2;
    loop {
        let candidate = format!("{root}-{index}");
        if !existing.contains(&candidate) {
            return candidate;
        }
        index += 1;
    }
}

fn rename_component_id(base: &str, existing: &std::collections::HashSet<String>) -> String {
    rename_surface_id(base, existing)
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::{
        export_single_artifact, export_single_vault_note, ShareBundle, ShareSourceWorkshop,
        SHARE_BUNDLE_VERSION,
    };

    fn sample_source() -> ShareSourceWorkshop {
        ShareSourceWorkshop {
            device_id: "abcd1234".to_string(),
            name: "Test Workshop".to_string(),
        }
    }

    #[test]
    fn share_bundle_validate_rejects_bad_version() {
        let bundle = ShareBundle {
            version: 99,
            exported_at: Utc::now(),
            source_workshop: sample_source(),
            artifacts: vec![],
            vault_notes: vec![],
            environment: None,
        };
        let errors = bundle.validate();
        assert!(errors.iter().any(|err| err.contains("unsupported share bundle version")));
    }

    #[test]
    fn share_bundle_validate_accepts_minimal_bundle() {
        let bundle = ShareBundle {
            version: SHARE_BUNDLE_VERSION,
            exported_at: Utc::now(),
            source_workshop: sample_source(),
            artifacts: vec![],
            vault_notes: vec![],
            environment: None,
        };
        assert!(bundle.validate().is_empty());
    }

    #[test]
    fn export_single_helpers_require_existing_items() {
        let err = export_single_artifact("art:missing", sample_source()).unwrap_err();
        assert!(err.to_string().contains("not found"));
        let err = export_single_vault_note("missing/note.md", sample_source()).unwrap_err();
        assert!(err.to_string().contains("not found"));
    }
}
