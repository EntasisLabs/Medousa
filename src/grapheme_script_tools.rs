//! Grapheme script library tools (Phase 8E.1).

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};
use stasis::application::orchestration::tool_registry::StasisTool;
use stasis::domain::errors::{Result as StasisResult, StasisError};
use tokio::sync::mpsc;

use crate::events::TuiEvent;
use crate::grapheme_script::service::GraphemeScriptHit;
use crate::grapheme_script::{GraphemeScriptEntry, GraphemeScriptService};
use crate::typed_tools::{ToolId, medousa_tool};

pub const COGNITION_GRAPHEME_SCRIPT_SAVE: &str = "cognition_grapheme_script_save";
pub const COGNITION_GRAPHEME_SCRIPT_LIST: &str = "cognition_grapheme_script_list";
pub const COGNITION_GRAPHEME_SCRIPT_SEARCH: &str = "cognition_grapheme_script_search";
pub const COGNITION_GRAPHEME_SCRIPT_LOAD: &str = "cognition_grapheme_script_load";
const COGNITION_GRAPHEME_SCRIPT_LIST_ID: ToolId = ToolId::new(COGNITION_GRAPHEME_SCRIPT_LIST);
const COGNITION_GRAPHEME_SCRIPT_SEARCH_ID: ToolId = ToolId::new(COGNITION_GRAPHEME_SCRIPT_SEARCH);
const COGNITION_GRAPHEME_SCRIPT_LOAD_ID: ToolId = ToolId::new(COGNITION_GRAPHEME_SCRIPT_LOAD);

pub fn register_grapheme_script_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    event_tx: mpsc::Sender<TuiEvent>,
) -> StasisResult<()> {
    registry.register_tool(CognitionGraphemeScriptSaveTool {
        event_tx: event_tx.clone(),
    })?;
    registry.register_typed_tool(CognitionGraphemeScriptListTool {
        event_tx: event_tx.clone(),
    })?;
    registry.register_typed_tool(CognitionGraphemeScriptSearchTool {
        event_tx: event_tx.clone(),
    })?;
    registry.register_typed_tool(CognitionGraphemeScriptLoadTool { event_tx })?;
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

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GraphemeScriptListInput {
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    module: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    tag: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_usize"
    )]
    #[schemars(
        with = "usize",
        range(min = 1, max = 200),
        skip_serializing_if = "Option::is_none"
    )]
    limit: Option<usize>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct GraphemeScriptListOutput {
    ok: bool,
    count: usize,
    scripts: Vec<GraphemeScriptEntry>,
    block: String,
}

#[medousa_tool(id = COGNITION_GRAPHEME_SCRIPT_LIST_ID)]
impl CognitionGraphemeScriptListTool {
    /// List saved Grapheme scripts by optional module or tag filter.
    async fn invoke_typed(
        &self,
        input: GraphemeScriptListInput,
    ) -> stasis::prelude::Result<GraphemeScriptListOutput> {
        let module = input.module.as_deref();
        let tag = input.tag.as_deref();
        let limit = input.limit.unwrap_or(20);
        emit_invoked(
            &self.event_tx,
            COGNITION_GRAPHEME_SCRIPT_LIST_ID.as_str(),
            module.unwrap_or(tag.unwrap_or("*")),
        );
        let entries = GraphemeScriptService::list(module, tag, limit);
        let lines: Vec<String> = entries.iter().map(|entry| entry.summary_line()).collect();
        Ok(GraphemeScriptListOutput {
            ok: true,
            count: entries.len(),
            scripts: entries,
            block: lines.join("\n"),
        })
    }
}

pub struct CognitionGraphemeScriptSearchTool {
    event_tx: mpsc::Sender<TuiEvent>,
}

#[derive(Debug, JsonSchema)]
pub struct GraphemeScriptSearchInput {
    #[schemars(required, with = "String")]
    q: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    module: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        with = "usize",
        range(min = 1, max = 50),
        skip_serializing_if = "Option::is_none"
    )]
    limit: Option<usize>,
}

impl<'de> Deserialize<'de> for GraphemeScriptSearchInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            q: Option<String>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            module: Option<String>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            tag: Option<String>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_usize"
            )]
            limit: Option<usize>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            q: input.q,
            module: input.module,
            tag: input.tag,
            limit: input.limit,
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct GraphemeScriptSearchOutput {
    ok: bool,
    query: String,
    hits: Vec<GraphemeScriptHit>,
    block: String,
}

#[medousa_tool(id = COGNITION_GRAPHEME_SCRIPT_SEARCH_ID)]
impl CognitionGraphemeScriptSearchTool {
    /// Keyword search over saved Grapheme scripts (name, modules, tags, intent, body). Use before authoring when [MEDOUSA_GRAPHEME_SCRIPTS] suggests a match.
    async fn invoke_typed(
        &self,
        input: GraphemeScriptSearchInput,
    ) -> stasis::prelude::Result<GraphemeScriptSearchOutput> {
        let query = input
            .q
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StasisError::PortFailure("q is required".to_string()))?;
        let module = input.module.as_deref();
        let tag = input.tag.as_deref();
        let limit = input.limit.unwrap_or(10);
        emit_invoked(
            &self.event_tx,
            COGNITION_GRAPHEME_SCRIPT_SEARCH_ID.as_str(),
            query,
        );
        let hits = GraphemeScriptService::search_ranked(query, module, tag, limit);
        let lines: Vec<String> = hits.iter().map(|hit| hit.line.clone()).collect();
        Ok(GraphemeScriptSearchOutput {
            ok: true,
            query: query.to_string(),
            hits,
            block: lines.join("\n"),
        })
    }
}

pub struct CognitionGraphemeScriptLoadTool {
    event_tx: mpsc::Sender<TuiEvent>,
}

#[derive(Debug, JsonSchema)]
pub struct GraphemeScriptLoadInput {
    #[schemars(required, with = "String")]
    id: Option<String>,
}

impl<'de> Deserialize<'de> for GraphemeScriptLoadInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            id: Option<String>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self { id: input.id })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct GraphemeScriptLoadOutput {
    ok: bool,
    id: String,
    name: String,
    version: u32,
    modules: Vec<String>,
    tags: Vec<String>,
    intent: Option<String>,
    body: String,
    body_hash: String,
}

#[medousa_tool(id = COGNITION_GRAPHEME_SCRIPT_LOAD_ID)]
impl CognitionGraphemeScriptLoadTool {
    /// Load a saved Grapheme script body and metadata by id for run or edit.
    async fn invoke_typed(
        &self,
        input: GraphemeScriptLoadInput,
    ) -> stasis::prelude::Result<GraphemeScriptLoadOutput> {
        let id = input
            .id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StasisError::PortFailure("id is required".to_string()))?;
        emit_invoked(
            &self.event_tx,
            COGNITION_GRAPHEME_SCRIPT_LOAD_ID.as_str(),
            id,
        );
        let (entry, body) = GraphemeScriptService::load(id)
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        Ok(GraphemeScriptLoadOutput {
            ok: true,
            id: entry.id,
            name: entry.name,
            version: entry.version,
            modules: entry.modules,
            tags: entry.tags,
            intent: entry.intent,
            body,
            body_hash: entry.body_hash,
        })
    }
}
