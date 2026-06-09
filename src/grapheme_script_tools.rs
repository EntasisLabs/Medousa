//! Grapheme script library tools (Phase 8E.1).

use async_trait::async_trait;
use serde_json::{Value, json};
use stasis::application::orchestration::tool_registry::StasisTool;
use stasis::domain::errors::{Result as StasisResult, StasisError};
use tokio::sync::mpsc;

use crate::events::TuiEvent;
use crate::grapheme_script::GraphemeScriptService;

pub const COGNITION_GRAPHEME_SCRIPT_SAVE: &str = "cognition_grapheme_script_save";
pub const COGNITION_GRAPHEME_SCRIPT_LIST: &str = "cognition_grapheme_script_list";
pub const COGNITION_GRAPHEME_SCRIPT_SEARCH: &str = "cognition_grapheme_script_search";
pub const COGNITION_GRAPHEME_SCRIPT_LOAD: &str = "cognition_grapheme_script_load";

pub fn register_grapheme_script_tools(
    registry: &mut stasis::application::orchestration::tool_registry::InMemoryToolRegistry,
    event_tx: mpsc::Sender<TuiEvent>,
) -> StasisResult<()> {
    registry.register_tool(CognitionGraphemeScriptSaveTool {
        event_tx: event_tx.clone(),
    })?;
    registry.register_tool(CognitionGraphemeScriptListTool {
        event_tx: event_tx.clone(),
    })?;
    registry.register_tool(CognitionGraphemeScriptSearchTool {
        event_tx: event_tx.clone(),
    })?;
    registry.register_tool(CognitionGraphemeScriptLoadTool { event_tx })?;
    Ok(())
}

fn emit_invoked(event_tx: &mpsc::Sender<TuiEvent>, tool_name: &str, summary: &str) {
    let _ = event_tx.try_send(TuiEvent::ToolInvoked {
        tool_name: tool_name.to_string(),
        input_summary: summary.to_string(),
    });
}

fn string_list(input: &Value, key: &str) -> Vec<String> {
    input
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub struct CognitionGraphemeScriptSaveTool {
    event_tx: mpsc::Sender<TuiEvent>,
}

#[async_trait]
impl StasisTool for CognitionGraphemeScriptSaveTool {
    fn name(&self) -> &'static str {
        COGNITION_GRAPHEME_SCRIPT_SAVE
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Save a reusable Grapheme script to the workshop library with module tags and intent metadata. \
             Turn start may inject [MEDOUSA_GRAPHEME_SCRIPTS] matches — load full body with cognition_grapheme_script_load.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["name", "body"],
            "properties": {
                "id": { "type": "string", "description": "Optional stable id (slug derived from name when omitted)" },
                "name": { "type": "string" },
                "body": { "type": "string", "description": "Full Grapheme script source" },
                "modules": { "type": "array", "items": { "type": "string" }, "description": "Module tags e.g. web, http, core" },
                "tags": { "type": "array", "items": { "type": "string" } },
                "intent": { "type": "string", "description": "Short intent label for search/recall" },
                "session_id": { "type": "string", "description": "Optional source session for provenance" }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let name = input
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StasisError::PortFailure("name is required".to_string()))?;
        let body = input
            .get("body")
            .and_then(Value::as_str)
            .ok_or_else(|| StasisError::PortFailure("body is required".to_string()))?;
        let id = input.get("id").and_then(Value::as_str);
        let modules = string_list(&input, "modules");
        let tags = string_list(&input, "tags");
        let intent = input.get("intent").and_then(Value::as_str).map(str::to_string);
        let session_id = input
            .get("session_id")
            .and_then(Value::as_str)
            .map(str::to_string);

        emit_invoked(&self.event_tx, self.name(), name);
        let entry = GraphemeScriptService::save(id, name, body, modules, tags, intent, session_id)
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;

        Ok(json!({
            "ok": true,
            "id": entry.id,
            "name": entry.name,
            "version": entry.version,
            "modules": entry.modules,
            "tags": entry.tags,
            "intent": entry.intent,
            "line": entry.summary_line(),
        }))
    }
}

pub struct CognitionGraphemeScriptListTool {
    event_tx: mpsc::Sender<TuiEvent>,
}

#[async_trait]
impl StasisTool for CognitionGraphemeScriptListTool {
    fn name(&self) -> &'static str {
        COGNITION_GRAPHEME_SCRIPT_LIST
    }

    fn description(&self) -> Option<&'static str> {
        Some("List saved Grapheme scripts by optional module or tag filter.")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "module": { "type": "string" },
                "tag": { "type": "string" },
                "limit": { "type": "integer", "minimum": 1, "maximum": 200 }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let module = input.get("module").and_then(Value::as_str);
        let tag = input.get("tag").and_then(Value::as_str);
        let limit = input
            .get("limit")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(20);
        emit_invoked(
            &self.event_tx,
            self.name(),
            module.unwrap_or(tag.unwrap_or("*")),
        );
        let entries = GraphemeScriptService::list(module, tag, limit);
        let lines: Vec<String> = entries.iter().map(|entry| entry.summary_line()).collect();
        Ok(json!({
            "ok": true,
            "count": entries.len(),
            "scripts": entries,
            "block": lines.join("\n"),
        }))
    }
}

pub struct CognitionGraphemeScriptSearchTool {
    event_tx: mpsc::Sender<TuiEvent>,
}

#[async_trait]
impl StasisTool for CognitionGraphemeScriptSearchTool {
    fn name(&self) -> &'static str {
        COGNITION_GRAPHEME_SCRIPT_SEARCH
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Keyword search over saved Grapheme scripts (name, modules, tags, intent, body). \
             Use before authoring when [MEDOUSA_GRAPHEME_SCRIPTS] suggests a match.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["q"],
            "properties": {
                "q": { "type": "string" },
                "module": { "type": "string" },
                "tag": { "type": "string" },
                "limit": { "type": "integer", "minimum": 1, "maximum": 50 }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let query = input
            .get("q")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StasisError::PortFailure("q is required".to_string()))?;
        let module = input.get("module").and_then(Value::as_str);
        let tag = input.get("tag").and_then(Value::as_str);
        let limit = input
            .get("limit")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(10);
        emit_invoked(&self.event_tx, self.name(), query);
        let hits = GraphemeScriptService::search_ranked(query, module, tag, limit);
        let lines: Vec<String> = hits.iter().map(|hit| hit.line.clone()).collect();
        Ok(json!({
            "ok": true,
            "query": query,
            "hits": hits,
            "block": lines.join("\n"),
        }))
    }
}

pub struct CognitionGraphemeScriptLoadTool {
    event_tx: mpsc::Sender<TuiEvent>,
}

#[async_trait]
impl StasisTool for CognitionGraphemeScriptLoadTool {
    fn name(&self) -> &'static str {
        COGNITION_GRAPHEME_SCRIPT_LOAD
    }

    fn description(&self) -> Option<&'static str> {
        Some("Load a saved Grapheme script body and metadata by id for run or edit.")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["id"],
            "properties": {
                "id": { "type": "string" }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let id = input
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StasisError::PortFailure("id is required".to_string()))?;
        emit_invoked(&self.event_tx, self.name(), id);
        let (entry, body) = GraphemeScriptService::load(id)
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        Ok(json!({
            "ok": true,
            "id": entry.id,
            "name": entry.name,
            "version": entry.version,
            "modules": entry.modules,
            "tags": entry.tags,
            "intent": entry.intent,
            "body": body,
            "body_hash": entry.body_hash,
        }))
    }
}
