//! Grapheme script library tools (Phase 8E.1).

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
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
const COGNITION_GRAPHEME_SCRIPT_SAVE_ID: ToolId = ToolId::new(COGNITION_GRAPHEME_SCRIPT_SAVE);

pub fn register_grapheme_script_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    event_tx: mpsc::Sender<TuiEvent>,
) -> StasisResult<()> {
    registry.register_typed_tool(CognitionGraphemeScriptSaveTool {
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

fn deserialize_lenient_string_list<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    let values = value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    Ok(Some(values))
}

pub struct CognitionGraphemeScriptSaveTool {
    event_tx: mpsc::Sender<TuiEvent>,
}

#[derive(Debug, JsonSchema)]
pub struct GraphemeScriptSaveInput {
    /// Optional stable id (slug derived from name when omitted)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    #[schemars(required, with = "String")]
    name: Option<String>,
    /// Full Grapheme script source
    #[schemars(required, with = "String")]
    body: Option<String>,
    /// Module tags e.g. web, http, core
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Vec<String>", skip_serializing_if = "Option::is_none")]
    modules: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Vec<String>", skip_serializing_if = "Option::is_none")]
    tags: Option<Vec<String>>,
    /// Short intent label for search/recall
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    intent: Option<String>,
    /// Optional source session for provenance
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
}

impl<'de> Deserialize<'de> for GraphemeScriptSaveInput {
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
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            name: Option<String>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            body: Option<String>,
            #[serde(default, deserialize_with = "deserialize_lenient_string_list")]
            modules: Option<Vec<String>>,
            #[serde(default, deserialize_with = "deserialize_lenient_string_list")]
            tags: Option<Vec<String>>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            intent: Option<String>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            session_id: Option<String>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            id: input.id,
            name: input.name,
            body: input.body,
            modules: input.modules,
            tags: input.tags,
            intent: input.intent,
            session_id: input.session_id,
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct GraphemeScriptSaveOutput {
    ok: bool,
    id: String,
    name: String,
    version: u32,
    modules: Vec<String>,
    tags: Vec<String>,
    intent: Option<String>,
    line: String,
}

#[medousa_tool(id = COGNITION_GRAPHEME_SCRIPT_SAVE_ID)]
impl CognitionGraphemeScriptSaveTool {
    /// Save a reusable Grapheme script to the workshop library with module tags and intent metadata. Turn start may inject [MEDOUSA_GRAPHEME_SCRIPTS] matches — load full body with cognition_grapheme_script_load.
    async fn invoke_typed(
        &self,
        input: GraphemeScriptSaveInput,
    ) -> stasis::prelude::Result<GraphemeScriptSaveOutput> {
        let name = input
            .name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StasisError::PortFailure("name is required".to_string()))?;
        let body = input
            .body
            .as_deref()
            .ok_or_else(|| StasisError::PortFailure("body is required".to_string()))?;

        emit_invoked(&self.event_tx, COGNITION_GRAPHEME_SCRIPT_SAVE, name);
        let entry = GraphemeScriptService::save(
            input.id.as_deref(),
            name,
            body,
            input.modules.unwrap_or_default(),
            input.tags.unwrap_or_default(),
            input.intent,
            input.session_id,
        )
        .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        let line = entry.summary_line();

        Ok(GraphemeScriptSaveOutput {
            ok: true,
            id: entry.id,
            name: entry.name,
            version: entry.version,
            modules: entry.modules,
            tags: entry.tags,
            intent: entry.intent,
            line,
        })
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
