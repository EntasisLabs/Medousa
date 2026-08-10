//! Grapheme script library tools (Phase 8E.1).

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use stasis::domain::errors::Result as StasisResult;
use tokio::sync::mpsc;

use crate::events::TuiEvent;
use crate::grapheme_script::service::GraphemeScriptHit;
use crate::grapheme_script::{GraphemeScriptEntry, GraphemeScriptService};
use crate::semantic_values::{RequiredContent, TrimmedText};
use crate::tool_error::{DependencyResultExt, ToolError};
use crate::typed_tools::{CompatOption, ToolId, medousa_tool};

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

fn deserialize_compat_string_list<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
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
            #[serde(default)]
            id: CompatOption<String>,
            #[serde(default)]
            name: CompatOption<String>,
            #[serde(default)]
            body: CompatOption<String>,
            #[serde(default, deserialize_with = "deserialize_compat_string_list")]
            modules: Option<Vec<String>>,
            #[serde(default, deserialize_with = "deserialize_compat_string_list")]
            tags: Option<Vec<String>>,
            #[serde(default)]
            intent: CompatOption<String>,
            #[serde(default)]
            session_id: CompatOption<String>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            id: input.id.into_option(),
            name: input.name.into_option(),
            body: input.body.into_option(),
            modules: input.modules,
            tags: input.tags,
            intent: input.intent.into_option(),
            session_id: input.session_id.into_option(),
        })
    }
}

#[derive(Debug)]
struct GraphemeScriptSaveCommand {
    id: Option<TrimmedText>,
    name: TrimmedText,
    body: RequiredContent,
    modules: Vec<String>,
    tags: Vec<String>,
    intent: Option<TrimmedText>,
    session_id: Option<TrimmedText>,
}

impl TryFrom<GraphemeScriptSaveInput> for GraphemeScriptSaveCommand {
    type Error = ToolError;

    fn try_from(input: GraphemeScriptSaveInput) -> Result<Self, Self::Error> {
        let id = input.id.and_then(|value| TrimmedText::new(value).ok());
        let name = TrimmedText::new(input.name.unwrap_or_default()).map_err(|_| {
            ToolError::input(COGNITION_GRAPHEME_SCRIPT_SAVE, "name is required")
                .with_operation("normalize save input")
        })?;
        let body = RequiredContent::new(input.body.unwrap_or_default()).map_err(|_| {
            ToolError::input(COGNITION_GRAPHEME_SCRIPT_SAVE, "body is required")
                .with_operation("normalize save input")
        })?;
        let intent = input.intent.and_then(|value| TrimmedText::new(value).ok());
        let session_id = input
            .session_id
            .and_then(|value| TrimmedText::new(value).ok());

        Ok(Self {
            id,
            name,
            body,
            modules: input.modules.unwrap_or_default(),
            tags: input.tags.unwrap_or_default(),
            intent,
            session_id,
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
        let command = GraphemeScriptSaveCommand::try_from(input)?;
        let GraphemeScriptSaveCommand {
            id,
            name,
            body,
            modules,
            tags,
            intent,
            session_id,
        } = command;

        emit_invoked(
            &self.event_tx,
            COGNITION_GRAPHEME_SCRIPT_SAVE,
            name.as_str(),
        );
        let entry = GraphemeScriptService::save(
            id.as_ref().map(TrimmedText::as_str),
            name.as_str(),
            body.as_str(),
            modules,
            tags,
            intent.map(TrimmedText::into_string),
            session_id.map(TrimmedText::into_string),
        )
        .dependency_context(COGNITION_GRAPHEME_SCRIPT_SAVE, "save script")?;
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
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    module: CompatOption<String>,
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    tag: CompatOption<String>,
    #[serde(default)]
    #[schemars(
        with = "usize",
        range(min = 1, max = 200),
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    limit: CompatOption<usize>,
}

#[derive(Debug)]
struct GraphemeScriptListCommand {
    module: Option<TrimmedText>,
    tag: Option<TrimmedText>,
    limit: usize,
}

impl TryFrom<GraphemeScriptListInput> for GraphemeScriptListCommand {
    type Error = ToolError;

    fn try_from(input: GraphemeScriptListInput) -> Result<Self, Self::Error> {
        let module = input.module.into_option();
        let tag = input.tag.into_option();
        let limit = input.limit.into_option();
        Ok(Self {
            module: module.and_then(|value| TrimmedText::new(value).ok()),
            tag: tag.and_then(|value| TrimmedText::new(value).ok()),
            limit: limit.unwrap_or(20).clamp(1, 200),
        })
    }
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
        let command = GraphemeScriptListCommand::try_from(input)?;
        let module = command.module.as_ref().map(TrimmedText::as_str);
        let tag = command.tag.as_ref().map(TrimmedText::as_str);
        emit_invoked(
            &self.event_tx,
            COGNITION_GRAPHEME_SCRIPT_LIST_ID.as_str(),
            module.or(tag).unwrap_or("*"),
        );
        let entries = GraphemeScriptService::list(module, tag, command.limit);
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
            #[serde(default)]
            q: CompatOption<String>,
            #[serde(default)]
            module: CompatOption<String>,
            #[serde(default)]
            tag: CompatOption<String>,
            #[serde(default)]
            limit: CompatOption<usize>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            q: input.q.into_option(),
            module: input.module.into_option(),
            tag: input.tag.into_option(),
            limit: input.limit.into_option(),
        })
    }
}

#[derive(Debug)]
struct GraphemeScriptSearchCommand {
    query: TrimmedText,
    module: Option<TrimmedText>,
    tag: Option<TrimmedText>,
    limit: usize,
}

impl TryFrom<GraphemeScriptSearchInput> for GraphemeScriptSearchCommand {
    type Error = ToolError;

    fn try_from(input: GraphemeScriptSearchInput) -> Result<Self, Self::Error> {
        let query = TrimmedText::new(input.q.unwrap_or_default()).map_err(|_| {
            ToolError::input(COGNITION_GRAPHEME_SCRIPT_SEARCH, "q is required")
                .with_operation("normalize search input")
        })?;
        Ok(Self {
            query,
            module: input.module.and_then(|value| TrimmedText::new(value).ok()),
            tag: input.tag.and_then(|value| TrimmedText::new(value).ok()),
            limit: input.limit.unwrap_or(10).clamp(1, 50),
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
        let command = GraphemeScriptSearchCommand::try_from(input)?;
        let query = command.query.as_str();
        let module = command.module.as_ref().map(TrimmedText::as_str);
        let tag = command.tag.as_ref().map(TrimmedText::as_str);
        emit_invoked(
            &self.event_tx,
            COGNITION_GRAPHEME_SCRIPT_SEARCH_ID.as_str(),
            query,
        );
        let hits = GraphemeScriptService::search_ranked(query, module, tag, command.limit);
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
            #[serde(default)]
            id: CompatOption<String>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            id: input.id.into_option(),
        })
    }
}

#[derive(Debug)]
struct GraphemeScriptLoadCommand {
    id: TrimmedText,
}

impl TryFrom<GraphemeScriptLoadInput> for GraphemeScriptLoadCommand {
    type Error = ToolError;

    fn try_from(input: GraphemeScriptLoadInput) -> Result<Self, Self::Error> {
        let id = TrimmedText::new(input.id.unwrap_or_default()).map_err(|_| {
            ToolError::input(COGNITION_GRAPHEME_SCRIPT_LOAD, "id is required")
                .with_operation("normalize load input")
        })?;
        Ok(Self { id })
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
        let command = GraphemeScriptLoadCommand::try_from(input)?;
        let id = command.id.as_str();
        emit_invoked(
            &self.event_tx,
            COGNITION_GRAPHEME_SCRIPT_LOAD_ID.as_str(),
            id,
        );
        let (entry, body) = GraphemeScriptService::load(id).map_err(|err| {
            let message = err.to_string();
            if message.starts_with("grapheme script not found:") {
                ToolError::not_found(COGNITION_GRAPHEME_SCRIPT_LOAD, message)
            } else {
                ToolError::dependency(COGNITION_GRAPHEME_SCRIPT_LOAD, message)
            }
            .with_operation("load script")
        })?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn save_command_normalizes_metadata_and_preserves_script_bytes() {
        let command = GraphemeScriptSaveCommand::try_from(GraphemeScriptSaveInput {
            id: Some(" script-a ".into()),
            name: Some(" Script A ".into()),
            body: Some("  query Core.echo(message: \"hi\")  \n".into()),
            modules: Some(vec!["core".into()]),
            tags: Some(vec!["demo".into()]),
            intent: Some(" example ".into()),
            session_id: Some(" session-a ".into()),
        })
        .expect("save command");

        assert_eq!(command.id.as_ref().unwrap().as_str(), "script-a");
        assert_eq!(command.name.as_str(), "Script A");
        assert_eq!(
            command.body.as_str(),
            "  query Core.echo(message: \"hi\")  \n"
        );
        assert_eq!(command.intent.as_ref().unwrap().as_str(), "example");
        assert_eq!(command.session_id.as_ref().unwrap().as_str(), "session-a");
    }

    #[test]
    fn script_query_commands_normalize_filters_and_bounds() {
        let list = GraphemeScriptListCommand::try_from(GraphemeScriptListInput {
            module: Some(" web ".into()).into(),
            tag: Some(" \n\t".into()).into(),
            limit: Some(999).into(),
        })
        .expect("list command");
        assert_eq!(list.module.as_ref().unwrap().as_str(), "web");
        assert!(list.tag.is_none());
        assert_eq!(list.limit, 200);

        let search = GraphemeScriptSearchCommand::try_from(GraphemeScriptSearchInput {
            q: Some("  hello  ".into()),
            module: Some(" core ".into()),
            tag: None,
            limit: Some(999),
        })
        .expect("search command");
        assert_eq!(search.query.as_str(), "hello");
        assert_eq!(search.module.as_ref().unwrap().as_str(), "core");
        assert_eq!(search.limit, 50);

        let error = GraphemeScriptLoadCommand::try_from(GraphemeScriptLoadInput {
            id: Some(" \n\t".into()),
        })
        .expect_err("load id is required");
        assert!(error.to_string().contains("id is required"));
    }

    #[test]
    fn script_wire_optionals_absorb_wrong_legacy_types() {
        let save: GraphemeScriptSaveInput = serde_json::from_value(json!({
            "id": 7,
            "name": "script",
            "body": "body",
            "modules": ["web", 9],
            "tags": "legacy",
            "intent": false,
            "session_id": []
        }))
        .expect("save input should deserialize");
        assert_eq!(save.id, None);
        assert_eq!(save.name, Some("script".to_string()));
        assert_eq!(save.body, Some("body".to_string()));
        assert_eq!(save.modules, Some(vec!["web".to_string()]));
        assert_eq!(save.tags, Some(Vec::new()));
        assert_eq!(save.intent, None);
        assert_eq!(save.session_id, None);

        let list: GraphemeScriptListInput = serde_json::from_value(json!({
            "module": 9,
            "tag": true,
            "limit": "many"
        }))
        .expect("list input should deserialize");
        let list = GraphemeScriptListCommand::try_from(list).expect("list command");
        assert!(list.module.is_none());
        assert!(list.tag.is_none());
        assert_eq!(list.limit, 20);

        let search: GraphemeScriptSearchInput = serde_json::from_value(json!({
            "q": false,
            "module": [],
            "tag": {},
            "limit": []
        }))
        .expect("search input should deserialize");
        let error = GraphemeScriptSearchCommand::try_from(search)
            .expect_err("wrong query type should become the required-field error");
        assert!(error.to_string().contains("q is required"));

        let load: GraphemeScriptLoadInput = serde_json::from_value(json!({"id": ["script"]}))
            .expect("load input should deserialize");
        let error = GraphemeScriptLoadCommand::try_from(load)
            .expect_err("wrong load id type should become the required-field error");
        assert!(error.to_string().contains("id is required"));
    }
}
