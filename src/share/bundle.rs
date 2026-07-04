use chrono::{DateTime, Utc};
use medousa_types::environment::{ComponentDef, EnvironmentSpec, LayoutPreset, SurfaceDef};
use serde::{Deserialize, Serialize};

pub const SHARE_BUNDLE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareSourceWorkshop {
    pub device_id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareArtifactEntry {
    pub id: String,
    pub title: String,
    pub mime: String,
    pub content_base64: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height_px: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareVaultNoteEntry {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ShareEnvironmentSection {
    #[serde(default)]
    pub surfaces: Vec<SurfaceDef>,
    #[serde(default)]
    pub components: Vec<ComponentDef>,
    #[serde(default)]
    pub layout_presets: Vec<LayoutPreset>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareBundle {
    pub version: u32,
    pub exported_at: DateTime<Utc>,
    pub source_workshop: ShareSourceWorkshop,
    #[serde(default)]
    pub artifacts: Vec<ShareArtifactEntry>,
    #[serde(default)]
    pub vault_notes: Vec<ShareVaultNoteEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<ShareEnvironmentSection>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ShareExportRequest {
    #[serde(default)]
    pub artifact_ids: Vec<String>,
    #[serde(default)]
    pub vault_paths: Vec<String>,
    #[serde(default)]
    pub include_environment: bool,
    #[serde(default)]
    pub surface_ids: Vec<String>,
    #[serde(default)]
    pub component_ids: Vec<String>,
    #[serde(default)]
    pub profile_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ShareConflictStrategy {
    #[default]
    Skip,
    Rename,
    Overwrite,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareImportRequest {
    pub bundle: ShareBundle,
    #[serde(default)]
    pub conflict_strategy: ShareConflictStrategy,
    #[serde(default)]
    pub profile_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ShareImportResult {
    pub artifacts_imported: usize,
    pub artifacts_skipped: usize,
    pub vault_notes_imported: usize,
    pub vault_notes_skipped: usize,
    pub surfaces_imported: usize,
    pub components_imported: usize,
    pub layout_presets_imported: usize,
    #[serde(default)]
    pub artifact_id_map: Vec<(String, String)>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareCapabilitiesResponse {
    pub version: u32,
    pub sections: Vec<&'static str>,
    pub max_artifact_bytes: usize,
}

impl ShareCapabilitiesResponse {
    pub fn current() -> Self {
        Self {
            version: SHARE_BUNDLE_VERSION,
            sections: vec!["artifacts", "vaultNotes", "environment"],
            max_artifact_bytes: crate::artifact_store::UI_ARTIFACT_MAX_BYTES,
        }
    }
}

impl ShareBundle {
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        if self.version != SHARE_BUNDLE_VERSION {
            errors.push(format!(
                "unsupported share bundle version {} (expected {SHARE_BUNDLE_VERSION})",
                self.version
            ));
        }
        if self.source_workshop.device_id.trim().is_empty() {
            errors.push("sourceWorkshop.deviceId is required".to_string());
        }
        for artifact in &self.artifacts {
            if artifact.id.trim().is_empty() {
                errors.push("artifact id is required".to_string());
            }
            if base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                artifact.content_base64.as_bytes(),
            )
            .is_err()
            {
                errors.push(format!("artifact '{}' has invalid base64", artifact.id));
            }
        }
        if let Some(environment) = &self.environment {
            let spec = EnvironmentSpec {
                version: 1,
                profile_id: "share-preview".to_string(),
                surfaces: environment.surfaces.clone(),
                components: environment.components.clone(),
                layout_presets: Some(environment.layout_presets.clone()),
                active_preset_id: None,
                shell_chrome: None,
                theme: None,
                updated_at: Utc::now(),
                updated_by: "share".to_string(),
            };
            errors.extend(
                medousa_types::environment_validate::validate_environment_spec(&spec)
                    .into_iter()
                    .map(|err| format!("environment: {err}")),
            );
        }
        errors
    }
}
