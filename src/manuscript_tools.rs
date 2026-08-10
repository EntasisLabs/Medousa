//! Agent tools for the identity manuscript catalog.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use stasis::prelude::StasisError;

use crate::identity_manuscript::{ManuscriptScope, build_manuscript_context, list_manuscripts};
use crate::semantic_values::TrimmedText;
use crate::typed_tools::{ToolId, medousa_tool};

const COGNITION_MANUSCRIPT_LIST_ID: ToolId = ToolId::new("cognition_manuscript_list");
const COGNITION_MANUSCRIPT_RESOLVE_ID: ToolId = ToolId::new("cognition_manuscript_resolve");

pub fn register_manuscript_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
) -> stasis::prelude::Result<()> {
    registry.register_typed_tool(CognitionManuscriptListTool)?;
    registry.register_typed_tool(CognitionManuscriptResolveTool)?;
    Ok(())
}

pub struct CognitionManuscriptListTool;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ManuscriptListInput {
    /// Optional manuscript id prefix filter
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    /// Max entries (default 50)
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_usize"
    )]
    #[schemars(with = "i64", skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

#[derive(Debug)]
struct ManuscriptListCommand {
    prefix: Option<TrimmedText>,
    limit: usize,
}

impl TryFrom<ManuscriptListInput> for ManuscriptListCommand {
    type Error = stasis::prelude::StasisError;

    fn try_from(input: ManuscriptListInput) -> Result<Self, Self::Error> {
        Ok(Self {
            prefix: input
                .prefix
                .as_deref()
                .and_then(|value| TrimmedText::new(value).ok()),
            limit: input.limit.unwrap_or(50).clamp(1, 200),
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ManuscriptListScope {
    Project,
    User,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ManuscriptListEntry {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub scope: ManuscriptListScope,
    pub path: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ManuscriptDirectories {
    pub project: String,
    pub user: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ManuscriptListOutput {
    pub count: usize,
    pub manuscripts: Vec<ManuscriptListEntry>,
    pub dirs: ManuscriptDirectories,
}

#[medousa_tool(id = COGNITION_MANUSCRIPT_LIST_ID)]
impl CognitionManuscriptListTool {
    /// List YAML identity manuscripts (specialty packs) from project and user dirs. Use before spawn, recurring register, or ingest /brief.
    async fn invoke_typed(
        &self,
        input: ManuscriptListInput,
    ) -> stasis::prelude::Result<ManuscriptListOutput> {
        let command = ManuscriptListCommand::try_from(input)?;

        let mut entries =
            list_manuscripts().map_err(|err| StasisError::PortFailure(err.to_string()))?;
        if let Some(prefix) = command.prefix.as_ref() {
            entries.retain(|entry| entry.id.starts_with(prefix.as_str()));
        }
        entries.truncate(command.limit);

        let manuscripts = entries
            .into_iter()
            .map(|entry| ManuscriptListEntry {
                id: entry.id,
                name: entry.name,
                description: entry.description,
                scope: match entry.scope {
                    ManuscriptScope::Project => ManuscriptListScope::Project,
                    ManuscriptScope::User => ManuscriptListScope::User,
                },
                path: entry.path.display().to_string(),
            })
            .collect::<Vec<_>>();

        Ok(ManuscriptListOutput {
            count: manuscripts.len(),
            manuscripts,
            dirs: ManuscriptDirectories {
                project: crate::identity_manuscript::project_manuscripts_dir()
                    .display()
                    .to_string(),
                user: crate::identity_manuscript::user_manuscripts_dir()
                    .display()
                    .to_string(),
            },
        })
    }
}

pub struct CognitionManuscriptResolveTool;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ManuscriptResolveInput {
    /// Manuscript id (e.g. morning-brief)
    pub id: String,
    /// Include truncated voice/system/task preview (default false)
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_bool"
    )]
    #[schemars(with = "bool", skip_serializing_if = "Option::is_none")]
    pub include_prompt_preview: Option<bool>,
}

#[derive(Debug)]
struct ManuscriptResolveCommand {
    id: TrimmedText,
    include_prompt_preview: bool,
}

impl TryFrom<ManuscriptResolveInput> for ManuscriptResolveCommand {
    type Error = stasis::prelude::StasisError;

    fn try_from(input: ManuscriptResolveInput) -> Result<Self, Self::Error> {
        let id = TrimmedText::new(input.id).map_err(|_| {
            StasisError::PortFailure("cognition_manuscript_resolve: id is required".to_string())
        })?;
        Ok(Self {
            id,
            include_prompt_preview: input.include_prompt_preview.unwrap_or(false),
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ManuscriptTextPreview {
    pub chars: usize,
    pub preview: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ManuscriptPromptPreview {
    pub voice_appendix: Option<ManuscriptTextPreview>,
    pub system_appendix: Option<ManuscriptTextPreview>,
    pub task_template: Option<ManuscriptTextPreview>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ManuscriptOpenshellSummary {
    pub enabled: bool,
    pub policy_template: Option<String>,
    pub sandbox_from: Option<String>,
    pub allow_scheduled: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ResolvedManuscript {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub extends_from: Option<String>,
    pub display_name: Option<String>,
    pub worker_intent: Option<String>,
    pub worker_stage_role: Option<String>,
    pub worker_model_hint: Option<String>,
    pub max_tool_rounds: Option<usize>,
    pub tools_allow: Vec<String>,
    pub pinned_preferences: Vec<String>,
    pub pinned_contacts: Vec<String>,
    pub recall_hints: Vec<String>,
    pub locus_session_id: Option<String>,
    pub delivery_mode: Option<String>,
    pub delivery_on_complete: Option<String>,
    pub schedule_cron: Option<String>,
    pub schedule_execution_mode: Option<String>,
    pub openshell: ManuscriptOpenshellSummary,
    pub source_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_preview: Option<ManuscriptPromptPreview>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ManuscriptResolveOutput {
    pub ok: bool,
    pub manuscript: ResolvedManuscript,
}

#[medousa_tool(id = COGNITION_MANUSCRIPT_RESOLVE_ID)]
impl CognitionManuscriptResolveTool {
    /// Resolve a manuscript id to its merged YAML specialty summary (tools, worker intent, pins). Read-only catalog inspect — does not run a turn.
    async fn invoke_typed(
        &self,
        input: ManuscriptResolveInput,
    ) -> stasis::prelude::Result<ManuscriptResolveOutput> {
        let command = ManuscriptResolveCommand::try_from(input)?;

        let context = build_manuscript_context(command.id.as_str())
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        let prompt_preview = command
            .include_prompt_preview
            .then(|| ManuscriptPromptPreview {
                voice_appendix: truncate_preview(context.voice_appendix.as_deref()),
                system_appendix: truncate_preview(context.system_appendix.as_deref()),
                task_template: truncate_preview(context.task_template.as_deref()),
            });
        Ok(ManuscriptResolveOutput {
            ok: true,
            manuscript: ResolvedManuscript {
                id: context.id,
                name: context.name,
                description: context.description,
                extends_from: context.extends_from,
                display_name: context.display_name,
                worker_intent: context.worker_intent,
                worker_stage_role: context.worker_stage_role,
                worker_model_hint: context.worker_model_hint,
                max_tool_rounds: context.max_tool_rounds,
                tools_allow: context.tools_allow,
                pinned_preferences: context.pinned_preferences,
                pinned_contacts: context.pinned_contact_ids,
                recall_hints: context.recall_hints,
                locus_session_id: context.locus_session_id,
                delivery_mode: context.delivery_mode,
                delivery_on_complete: context.delivery_on_complete,
                schedule_cron: context.schedule_cron,
                schedule_execution_mode: context.schedule_execution_mode,
                openshell: ManuscriptOpenshellSummary {
                    enabled: context.openshell_enabled,
                    policy_template: context.openshell_policy_template,
                    sandbox_from: context.openshell_sandbox_from,
                    allow_scheduled: context.openshell_allow_scheduled,
                },
                source_path: context.source_path.display().to_string(),
                prompt_preview,
            },
        })
    }
}

fn truncate_preview(value: Option<&str>) -> Option<ManuscriptTextPreview> {
    match value.map(str::trim).filter(|text| !text.is_empty()) {
        Some(text) => {
            let preview: String = text.chars().take(240).collect();
            Some(ManuscriptTextPreview {
                chars: text.chars().count(),
                preview,
            })
        }
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn list_tool_returns_dirs() {
        use serde_json::json;
        use stasis::application::orchestration::tool_registry::StasisTool;

        let tool = CognitionManuscriptListTool;
        let output = tool.invoke(json!({})).await.expect("list");
        assert!(output["dirs"]["project"].is_string());
        assert!(output["manuscripts"].is_array());
    }

    #[test]
    fn manuscript_list_command_normalizes_prefix_and_clamps_limit() {
        let command = ManuscriptListCommand::try_from(ManuscriptListInput {
            prefix: Some("  morning  ".to_string()),
            limit: Some(999),
        })
        .expect("command");
        assert_eq!(
            command.prefix.as_ref().map(TrimmedText::as_str),
            Some("morning")
        );
        assert_eq!(command.limit, 200);
    }

    #[test]
    fn manuscript_resolve_command_rejects_blank_id() {
        let error = ManuscriptResolveCommand::try_from(ManuscriptResolveInput {
            id: " \n\t".to_string(),
            include_prompt_preview: None,
        })
        .expect_err("blank manuscript id should fail");
        assert!(error.to_string().contains("id is required"));
    }
}
