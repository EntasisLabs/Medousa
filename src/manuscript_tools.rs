//! Agent tools for the identity manuscript catalog.

use async_trait::async_trait;
use serde_json::{Value, json};
use stasis::application::orchestration::tool_registry::StasisTool;
use stasis::prelude::{Result as StasisResult, StasisError};

use crate::identity_manuscript::{
    ManuscriptScope, build_manuscript_context, list_manuscripts, manuscript_catalog_entry,
};

pub fn register_manuscript_tools(
    registry: &mut stasis::application::orchestration::tool_registry::InMemoryToolRegistry,
) -> stasis::prelude::Result<()> {
    registry.register_tool(CognitionManuscriptListTool)?;
    registry.register_tool(CognitionManuscriptResolveTool)?;
    Ok(())
}

pub struct CognitionManuscriptListTool;

#[async_trait]
impl StasisTool for CognitionManuscriptListTool {
    fn name(&self) -> &'static str {
        "cognition_manuscript_list"
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "List YAML identity manuscripts (specialty packs) from project and user dirs. \
             Use before spawn, recurring register, or ingest /brief.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "prefix": {
                    "type": "string",
                    "description": "Optional manuscript id prefix filter"
                },
                "limit": {
                    "type": "integer",
                    "description": "Max entries (default 50)"
                }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let prefix = input
            .get("prefix")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let limit = input
            .get("limit")
            .and_then(|value| value.as_u64())
            .unwrap_or(50)
            .clamp(1, 200) as usize;

        let mut entries = list_manuscripts().map_err(|err| StasisError::PortFailure(err.to_string()))?;
        if let Some(prefix) = prefix {
            entries.retain(|entry| entry.id.starts_with(prefix));
        }
        entries.truncate(limit);

        let manuscripts = entries
            .into_iter()
            .map(|entry| {
                json!({
                    "id": entry.id,
                    "name": entry.name,
                    "description": entry.description,
                    "scope": match entry.scope {
                        ManuscriptScope::Project => "project",
                        ManuscriptScope::User => "user",
                    },
                    "path": entry.path.display().to_string(),
                })
            })
            .collect::<Vec<_>>();

        Ok(json!({
            "count": manuscripts.len(),
            "manuscripts": manuscripts,
            "dirs": {
                "project": crate::identity_manuscript::project_manuscripts_dir().display().to_string(),
                "user": crate::identity_manuscript::user_manuscripts_dir().display().to_string(),
            }
        }))
    }
}

pub struct CognitionManuscriptResolveTool;

#[async_trait]
impl StasisTool for CognitionManuscriptResolveTool {
    fn name(&self) -> &'static str {
        "cognition_manuscript_resolve"
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Resolve a manuscript id to its merged YAML specialty summary (tools, worker intent, pins). \
             Read-only catalog inspect — does not run a turn.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Manuscript id (e.g. morning-brief)"
                },
                "include_prompt_preview": {
                    "type": "boolean",
                    "description": "Include truncated voice/system/task preview (default false)"
                }
            },
            "required": ["id"]
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let id = input
            .get("id")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                StasisError::PortFailure("cognition_manuscript_resolve: id is required".to_string())
            })?;
        let include_prompt_preview = input
            .get("include_prompt_preview")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);

        let context = build_manuscript_context(id)
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        let mut entry = manuscript_catalog_entry(&context);
        if include_prompt_preview {
            entry["prompt_preview"] = json!({
                "voice_appendix": truncate_preview(context.voice_appendix.as_deref()),
                "system_appendix": truncate_preview(context.system_appendix.as_deref()),
                "task_template": truncate_preview(context.task_template.as_deref()),
            });
        }
        Ok(json!({
            "ok": true,
            "manuscript": entry,
        }))
    }
}

fn truncate_preview(value: Option<&str>) -> Value {
    match value.map(str::trim).filter(|text| !text.is_empty()) {
        Some(text) => {
            let preview: String = text.chars().take(240).collect();
            json!({
                "chars": text.chars().count(),
                "preview": preview,
            })
        }
        None => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn list_tool_returns_dirs() {
        let tool = CognitionManuscriptListTool;
        let output = tool.invoke(json!({})).await.expect("list");
        assert!(output["dirs"]["project"].is_string());
        assert!(output["manuscripts"].is_array());
    }
}
