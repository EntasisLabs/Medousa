//! Host-bus vault tools: list, read, search, write, tags.

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use stasis::domain::errors::{Result as StasisResult, StasisError};
use tokio::sync::mpsc;

use crate::daemon_api::{
    VaultDeleteResponse, VaultNote, VaultNotesListResponse, VaultSearchResponse, VaultWriteRequest,
    VaultWriteResponse,
};
use crate::events::TuiEvent;
use crate::locus_semantic_tags::parse_semantic_tags_from_value;
use crate::semantic_values::{RequiredContent, TrimmedText};
use crate::typed_tools::{CompatOption, ToolId, medousa_tool};
use crate::vault::VaultService;

const READ_BUDGET_CHARS: usize = 12_000;
const COGNITION_VAULT_LIST_ID: ToolId = ToolId::new("cognition_vault_list");
const COGNITION_VAULT_READ_ID: ToolId = ToolId::new("cognition_vault_read");
const COGNITION_VAULT_GREP_ID: ToolId = ToolId::new("cognition_vault_grep");
const COGNITION_VAULT_SEARCH_ID: ToolId = ToolId::new("cognition_vault_search");
const COGNITION_VAULT_TAGS_ID: ToolId = ToolId::new("cognition_vault_tags");
const COGNITION_VAULT_WRITE_ID: ToolId = ToolId::new("cognition_vault_write");
const COGNITION_VAULT_DELETE_ID: ToolId = ToolId::new("cognition_vault_delete");
const COGNITION_VAULT_MOVE_ID: ToolId = ToolId::new("cognition_vault_move");

pub fn register_vault_tools(
    _registry: &mut impl crate::typed_tools::ToolRegistration,
    _event_tx: mpsc::Sender<TuiEvent>,
    _turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    _fallback_chat_session_id: String,
) -> StasisResult<()> {
    Ok(())
}

fn emit_invoked(event_tx: &mpsc::Sender<TuiEvent>, tool_name: &str, summary: &str) {
    let _ = event_tx.try_send(TuiEvent::ToolInvoked {
        tool_name: tool_name.to_string(),
        input_summary: summary.to_string(),
    });
}

pub struct CognitionVaultListTool {
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionVaultListTool {
    pub fn new(event_tx: mpsc::Sender<TuiEvent>) -> Self {
        Self { event_tx }
    }
}

#[derive(Debug, JsonSchema)]
pub struct VaultListInput {
    /// Optional path prefix filter
    #[serde(default)]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    #[serde(default)]
    #[schemars(
        with = "usize",
        range(min = 1, max = 200),
        skip_serializing_if = "Option::is_none"
    )]
    pub limit: Option<usize>,
    /// Indexed-style tag filter (match-all), aligned with Locus tags
    #[serde(default)]
    #[schemars(with = "Vec<String>", skip_serializing_if = "Option::is_none")]
    pub semantic_tags: Option<Vec<String>>,
    /// Filter notes with tags sharing this prefix
    #[serde(default)]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub tag_prefix: Option<String>,
}

impl<'de> Deserialize<'de> for VaultListInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(default)]
            prefix: CompatOption<String>,
            #[serde(default)]
            limit: CompatOption<usize>,
            #[serde(default, deserialize_with = "deserialize_compat_semantic_tags")]
            semantic_tags: Option<Vec<String>>,
            #[serde(default)]
            tag_prefix: CompatOption<String>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            prefix: input.prefix.into_option(),
            limit: input.limit.into_option(),
            semantic_tags: input.semantic_tags,
            tag_prefix: input.tag_prefix.into_option(),
        })
    }
}

fn deserialize_compat_semantic_tags<'de, D>(
    deserializer: D,
) -> Result<Option<Vec<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(parse_semantic_tags_from_value(Some(&value)))
}

#[medousa_tool(id = COGNITION_VAULT_LIST_ID)]
impl CognitionVaultListTool {
    /// List vault notes (path + title + semantic tags). Optional tag filter (match-all).
    pub(crate) async fn invoke_typed(
        &self,
        input: VaultListInput,
    ) -> stasis::prelude::Result<VaultNotesListResponse> {
        let tags = input.semantic_tags.map(|tags| tags.join(","));
        let tag_prefix = input
            .tag_prefix
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        emit_invoked(
            &self.event_tx,
            COGNITION_VAULT_LIST_ID.as_str(),
            input.prefix.as_deref().unwrap_or("*"),
        );
        let response = VaultService::list_notes(
            input.prefix.as_deref(),
            input.limit.unwrap_or(50),
            tags.as_deref(),
            tag_prefix,
        );
        Ok(response)
    }
}

pub struct CognitionVaultReadTool {
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionVaultReadTool {
    pub fn new(event_tx: mpsc::Sender<TuiEvent>) -> Self {
        Self { event_tx }
    }
}

#[derive(Debug, JsonSchema)]
pub struct VaultReadInput {
    #[schemars(required, with = "String")]
    pub(crate) path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        with = "usize",
        range(min = 256, max = 20000),
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) max_chars: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        with = "usize",
        range(min = 1),
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) line_start: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        with = "usize",
        range(min = 1),
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) line_end: Option<usize>,
}

#[derive(Debug)]
struct VaultReadCommand {
    path: TrimmedText,
    max_chars: usize,
    line_start: Option<usize>,
    line_end: Option<usize>,
}

impl TryFrom<VaultReadInput> for VaultReadCommand {
    type Error = stasis::prelude::StasisError;

    fn try_from(input: VaultReadInput) -> Result<Self, Self::Error> {
        Ok(Self {
            path: required_vault_identifier(input.path, "path")?,
            max_chars: input
                .max_chars
                .unwrap_or(READ_BUDGET_CHARS)
                .clamp(256, 20_000),
            line_start: input.line_start,
            line_end: input.line_end,
        })
    }
}

impl<'de> Deserialize<'de> for VaultReadInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(default)]
            path: CompatOption<String>,
            #[serde(default)]
            max_chars: CompatOption<usize>,
            #[serde(default)]
            line_start: CompatOption<usize>,
            #[serde(default)]
            line_end: CompatOption<usize>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            path: input.path.into_option(),
            max_chars: input.max_chars.into_option(),
            line_start: input.line_start.into_option(),
            line_end: input.line_end.into_option(),
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum VaultReadOutput {
    Excerpt {
        note: VaultNote,
        content: String,
        truncated: bool,
        total_lines: usize,
        total_chars: usize,
        line_start: usize,
        line_end: usize,
    },
    Whole {
        note: VaultNote,
        content: String,
        truncated: bool,
        total_lines: usize,
        total_chars: usize,
    },
}

#[medousa_tool(id = COGNITION_VAULT_READ_ID)]
impl CognitionVaultReadTool {
    /// Read a vault note body (budget-capped).
    pub(crate) async fn invoke_typed(
        &self,
        input: VaultReadInput,
    ) -> stasis::prelude::Result<VaultReadOutput> {
        let command = VaultReadCommand::try_from(input)?;
        let path = command.path.as_str();
        let max_chars = command.max_chars;
        let line_start = command.line_start;
        let line_end = command.line_end;
        emit_invoked(&self.event_tx, COGNITION_VAULT_READ_ID.as_str(), path);
        let note = VaultService::get_note(path)
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        if line_start.is_some() || line_end.is_some() {
            let excerpt =
                crate::line_grep::excerpt_lines(&note.content, line_start, line_end, max_chars);
            return Ok(VaultReadOutput::Excerpt {
                note: note.note,
                content: excerpt.content,
                truncated: excerpt.truncated,
                total_lines: excerpt.total_lines,
                total_chars: excerpt.total_chars,
                line_start: excerpt.line_start,
                line_end: excerpt.line_end,
            });
        }
        let total_lines = note.content.lines().count();
        let total_chars = note.content.chars().count();
        let truncated = truncate_chars(&note.content, max_chars);
        Ok(VaultReadOutput::Whole {
            note: note.note,
            content: truncated.body,
            truncated: truncated.truncated,
            total_lines,
            total_chars,
        })
    }
}

pub struct CognitionVaultGrepTool {
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionVaultGrepTool {
    pub fn new(event_tx: mpsc::Sender<TuiEvent>) -> Self {
        Self { event_tx }
    }
}

#[derive(Debug, JsonSchema)]
pub struct VaultGrepInput {
    #[schemars(required, with = "String")]
    pub(crate) path: Option<String>,
    #[schemars(required, with = "String")]
    pub(crate) pattern: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        with = "usize",
        range(min = 0, max = 10),
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) context_lines: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        with = "usize",
        range(min = 1, max = 200),
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) limit: Option<usize>,
}

#[derive(Debug)]
struct VaultGrepCommand {
    path: TrimmedText,
    pattern: TrimmedText,
    context_lines: usize,
    limit: usize,
}

impl TryFrom<VaultGrepInput> for VaultGrepCommand {
    type Error = stasis::prelude::StasisError;

    fn try_from(input: VaultGrepInput) -> Result<Self, Self::Error> {
        Ok(Self {
            path: required_vault_identifier(input.path, "path")?,
            pattern: required_vault_identifier(input.pattern, "pattern")?,
            context_lines: input.context_lines.unwrap_or(2).clamp(0, 10),
            limit: input.limit.unwrap_or(20).clamp(1, 200),
        })
    }
}

impl<'de> Deserialize<'de> for VaultGrepInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(default)]
            path: CompatOption<String>,
            #[serde(default)]
            pattern: CompatOption<String>,
            #[serde(default)]
            context_lines: CompatOption<usize>,
            #[serde(default)]
            limit: CompatOption<usize>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            path: input.path.into_option(),
            pattern: input.pattern.into_option(),
            context_lines: input.context_lines.into_option(),
            limit: input.limit.into_option(),
        })
    }
}

#[medousa_tool(id = COGNITION_VAULT_GREP_ID)]
impl CognitionVaultGrepTool {
    /// Search inside a vault note (literal case-insensitive match with line numbers). Use cognition_vault_search to discover notes; use grep for surgical edits.
    pub(crate) async fn invoke_typed(
        &self,
        input: VaultGrepInput,
    ) -> stasis::prelude::Result<crate::line_grep::LineGrepResult> {
        let command = VaultGrepCommand::try_from(input)?;
        let path = command.path.as_str();
        let pattern = command.pattern.as_str();
        let context_lines = command.context_lines;
        let limit = command.limit;
        emit_invoked(&self.event_tx, COGNITION_VAULT_GREP_ID.as_str(), path);
        let note = VaultService::get_note(path)
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        let result = crate::line_grep::grep_lines(&note.content, pattern, context_lines, limit)
            .map_err(StasisError::PortFailure)?;
        Ok(result)
    }
}

pub struct CognitionVaultSearchTool {
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionVaultSearchTool {
    pub fn new(event_tx: mpsc::Sender<TuiEvent>) -> Self {
        Self { event_tx }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct VaultSearchInput {
    /// Full-text query (optional if semantic_tags set)
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    pub(crate) q: CompatOption<String>,
    #[serde(default, deserialize_with = "deserialize_compat_semantic_tags")]
    #[schemars(with = "Vec<String>", skip_serializing_if = "Option::is_none")]
    pub(crate) semantic_tags: Option<Vec<String>>,
    #[serde(default)]
    #[schemars(
        with = "usize",
        range(min = 1, max = 50),
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    pub(crate) limit: CompatOption<usize>,
}

#[derive(Debug)]
struct VaultSearchCommand {
    query: Option<TrimmedText>,
    semantic_tags: Option<Vec<String>>,
    limit: usize,
}

impl TryFrom<VaultSearchInput> for VaultSearchCommand {
    type Error = stasis::prelude::StasisError;

    fn try_from(input: VaultSearchInput) -> Result<Self, Self::Error> {
        let query_value = input.q.into_option();
        let limit = input.limit.into_option();
        let query = query_value
            .as_deref()
            .and_then(|value| TrimmedText::new(value).ok());
        if query.is_none() && input.semantic_tags.is_none() {
            return Err(StasisError::PortFailure(
                "q or semantic_tags is required".to_string(),
            ));
        }
        Ok(Self {
            query,
            semantic_tags: input.semantic_tags,
            limit: limit.unwrap_or(20),
        })
    }
}

#[medousa_tool(id = COGNITION_VAULT_SEARCH_ID)]
impl CognitionVaultSearchTool {
    /// Search vault notes by full text and/or semantic tags (match-all).
    pub(crate) async fn invoke_typed(
        &self,
        input: VaultSearchInput,
    ) -> stasis::prelude::Result<VaultSearchResponse> {
        let command = VaultSearchCommand::try_from(input)?;
        let query = command.query.as_ref().map(TrimmedText::as_str);
        let tags = command.semantic_tags.map(|tags| tags.join(","));
        let limit = command.limit;
        emit_invoked(
            &self.event_tx,
            COGNITION_VAULT_SEARCH_ID.as_str(),
            query.unwrap_or("tags-only"),
        );
        VaultService::search(query, limit, tags.as_deref())
            .map_err(|err| StasisError::PortFailure(err.to_string()))
    }
}

pub struct CognitionVaultTagsTool {
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionVaultTagsTool {
    pub fn new(event_tx: mpsc::Sender<TuiEvent>) -> Self {
        Self { event_tx }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct VaultTagsInput {
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    pub(crate) prefix: CompatOption<String>,
    #[serde(default)]
    #[schemars(
        with = "usize",
        range(min = 1, max = 500),
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    pub(crate) limit: CompatOption<usize>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct VaultTagsOutput {
    tags: Vec<String>,
    count: usize,
    usage: String,
}

#[medousa_tool(id = COGNITION_VAULT_TAGS_ID)]
impl CognitionVaultTagsTool {
    /// List semantic tags used across vault notes (shared vocabulary with Locus memory).
    pub(crate) async fn invoke_typed(
        &self,
        input: VaultTagsInput,
    ) -> stasis::prelude::Result<VaultTagsOutput> {
        let prefix_value = input.prefix.into_option();
        let prefix = prefix_value
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let limit = input.limit.into_option().unwrap_or(100).clamp(1, 500);
        emit_invoked(
            &self.event_tx,
            COGNITION_VAULT_TAGS_ID.as_str(),
            prefix.unwrap_or("all"),
        );
        let response = VaultService::list_tags(prefix, limit);
        Ok(VaultTagsOutput {
            tags: response.tags,
            count: response.count,
            usage: "Use semantic_tags on cognition_store_read/write action=vault.read|vault.write, or match Locus via cognition_memory_tags.".to_string(),
        })
    }
}

pub struct CognitionVaultWriteTool {
    event_tx: mpsc::Sender<TuiEvent>,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    fallback_chat_session_id: String,
}

impl CognitionVaultWriteTool {
    pub fn new(
        event_tx: mpsc::Sender<TuiEvent>,
        turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
        fallback_chat_session_id: String,
    ) -> Self {
        Self {
            event_tx,
            turn_scope,
            fallback_chat_session_id,
        }
    }
}

#[derive(Debug, Default)]
struct LenientStringPresence {
    provided: bool,
    value: Option<String>,
}

impl<'de> Deserialize<'de> for LenientStringPresence {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        Ok(Self {
            provided: true,
            value: value.as_str().map(str::to_string),
        })
    }
}

#[derive(Debug, JsonSchema)]
pub struct VaultWriteInput {
    #[schemars(required, with = "String")]
    pub(crate) path: Option<String>,
    #[schemars(required, with = "String")]
    pub(crate) content: Option<String>,
    /// Chat session for workshop linking tags (defaults to current turn session)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub(crate) session_id: Option<String>,
    #[schemars(skip)]
    pub(crate) session_id_provided: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Vec<String>", skip_serializing_if = "Option::is_none")]
    pub(crate) semantic_tags: Option<Vec<String>>,
    /// Merge medousa/vault/session/profile/chat defaults (default true)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "bool", skip_serializing_if = "Option::is_none")]
    pub(crate) auto_workshop_tags: Option<bool>,
    /// Optional content_hash for optimistic concurrency
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub(crate) if_match: Option<String>,
}

#[derive(Debug)]
struct VaultWriteCommand {
    path: TrimmedText,
    content: RequiredContent,
    session_id: Option<TrimmedText>,
    session_id_provided: bool,
    semantic_tags: Option<Vec<String>>,
    auto_workshop_tags: bool,
    if_match: Option<String>,
}

impl TryFrom<VaultWriteInput> for VaultWriteCommand {
    type Error = stasis::prelude::StasisError;

    fn try_from(input: VaultWriteInput) -> Result<Self, Self::Error> {
        let path = TrimmedText::new(
            input
                .path
                .ok_or_else(|| StasisError::PortFailure("path is required".to_string()))?,
        )
        .map_err(|_| StasisError::PortFailure("path is required".to_string()))?;
        let content = RequiredContent::new(
            input
                .content
                .ok_or_else(|| StasisError::PortFailure("content is required".to_string()))?,
        )
        .map_err(|_| StasisError::PortFailure("content is required".to_string()))?;
        let session_id = input
            .session_id
            .as_deref()
            .and_then(|value| TrimmedText::new(value).ok());

        Ok(Self {
            path,
            content,
            session_id,
            session_id_provided: input.session_id_provided,
            semantic_tags: input.semantic_tags,
            auto_workshop_tags: input.auto_workshop_tags.unwrap_or(true),
            if_match: input.if_match,
        })
    }
}

impl<'de> Deserialize<'de> for VaultWriteInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(default)]
            path: CompatOption<String>,
            #[serde(default)]
            content: CompatOption<String>,
            #[serde(default)]
            session_id: LenientStringPresence,
            #[serde(default, deserialize_with = "deserialize_compat_semantic_tags")]
            semantic_tags: Option<Vec<String>>,
            #[serde(default)]
            auto_workshop_tags: CompatOption<bool>,
            #[serde(default)]
            if_match: CompatOption<String>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            path: input.path.into_option(),
            content: input.content.into_option(),
            session_id: input.session_id.value,
            session_id_provided: input.session_id.provided,
            semantic_tags: input.semantic_tags,
            auto_workshop_tags: input.auto_workshop_tags.into_option(),
            if_match: input.if_match.into_option(),
        })
    }
}

#[medousa_tool(id = COGNITION_VAULT_WRITE_ID)]
impl CognitionVaultWriteTool {
    /// Create or update a vault markdown note. Merges Locus-aligned semantic tags into frontmatter.
    pub(crate) async fn invoke_typed(
        &self,
        input: VaultWriteInput,
    ) -> stasis::prelude::Result<VaultWriteResponse> {
        let command = VaultWriteCommand::try_from(input)?;
        let path = command.path.into_string();
        let content = command.content.into_string();
        let session_id = if command.session_id_provided {
            command.session_id.map(TrimmedText::into_string)
        } else {
            let chat_session_id = crate::runtime_session::resolve_active_chat_session_id_async(
                &self.turn_scope,
                &self.fallback_chat_session_id,
            )
            .await?;
            Some(crate::locus_memory::resolve_workshop_locus_session(
                &chat_session_id,
            ))
        };
        emit_invoked(&self.event_tx, COGNITION_VAULT_WRITE_ID.as_str(), &path);
        let request = VaultWriteRequest {
            path: Some(path.clone()),
            content,
            session_id,
            semantic_tags: command.semantic_tags,
            auto_workshop_tags: command.auto_workshop_tags,
        };
        VaultService::write_note_with_actor(
            Some(&path),
            &request,
            command.if_match.as_deref(),
            crate::daemon_api::WorkspaceEventActor::Agent,
            Some("cognition_store_write"),
        )
        .map_err(|err| StasisError::PortFailure(err.to_string()))
    }
}

struct TruncatedBody {
    body: String,
    truncated: bool,
}

fn truncate_chars(body: &str, max_chars: usize) -> TruncatedBody {
    if body.chars().count() <= max_chars {
        return TruncatedBody {
            body: body.to_string(),
            truncated: false,
        };
    }
    TruncatedBody {
        body: format!("{}…", body.chars().take(max_chars).collect::<String>()),
        truncated: true,
    }
}

pub struct CognitionVaultDeleteTool {
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionVaultDeleteTool {
    pub fn new(event_tx: mpsc::Sender<TuiEvent>) -> Self {
        Self { event_tx }
    }
}

#[derive(Debug, JsonSchema)]
pub struct VaultDeleteInput {
    /// Relative vault note path to delete
    #[schemars(required, with = "String")]
    pub(crate) path: Option<String>,
}

impl<'de> Deserialize<'de> for VaultDeleteInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(default)]
            path: CompatOption<String>,
        }
        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            path: input.path.into_option(),
        })
    }
}

#[medousa_tool(id = COGNITION_VAULT_DELETE_ID)]
impl CognitionVaultDeleteTool {
    /// Soft-delete a vault markdown note (moves to .trash). Use after confirming the path with list/read.
    pub(crate) async fn invoke_typed(
        &self,
        input: VaultDeleteInput,
    ) -> stasis::prelude::Result<VaultDeleteResponse> {
        let path = input
            .path
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StasisError::PortFailure("path is required".to_string()))?;
        emit_invoked(&self.event_tx, COGNITION_VAULT_DELETE_ID.as_str(), path);
        VaultService::delete_note(path).map_err(|err| StasisError::PortFailure(err.to_string()))
    }
}

pub struct CognitionVaultMoveTool {
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionVaultMoveTool {
    pub fn new(event_tx: mpsc::Sender<TuiEvent>) -> Self {
        Self { event_tx }
    }
}

#[derive(Debug, JsonSchema)]
pub struct VaultMoveInput {
    /// Existing note path
    #[schemars(required, with = "String")]
    pub(crate) from_path: Option<String>,
    /// Destination path
    #[schemars(required, with = "String")]
    pub(crate) to_path: Option<String>,
}

#[derive(Debug)]
struct VaultMoveCommand {
    from_path: TrimmedText,
    to_path: TrimmedText,
}

impl TryFrom<VaultMoveInput> for VaultMoveCommand {
    type Error = stasis::prelude::StasisError;

    fn try_from(input: VaultMoveInput) -> Result<Self, Self::Error> {
        Ok(Self {
            from_path: required_vault_identifier(input.from_path, "from_path")?,
            to_path: required_vault_identifier(input.to_path, "to_path")?,
        })
    }
}

fn required_vault_identifier(
    value: Option<String>,
    field: &str,
) -> stasis::prelude::Result<TrimmedText> {
    let value = value.ok_or_else(|| StasisError::PortFailure(format!("{field} is required")))?;
    TrimmedText::new(value).map_err(|_| StasisError::PortFailure(format!("{field} is required")))
}

impl<'de> Deserialize<'de> for VaultMoveInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(default)]
            from_path: CompatOption<String>,
            #[serde(default)]
            to_path: CompatOption<String>,
        }
        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            from_path: input.from_path.into_option(),
            to_path: input.to_path.into_option(),
        })
    }
}

#[medousa_tool(id = COGNITION_VAULT_MOVE_ID)]
impl CognitionVaultMoveTool {
    /// Move/rename a vault note to a new relative path. Creates parent folders as needed and removes the source note.
    pub(crate) async fn invoke_typed(
        &self,
        input: VaultMoveInput,
    ) -> stasis::prelude::Result<VaultWriteResponse> {
        let command = VaultMoveCommand::try_from(input)?;
        let from_path = command.from_path.as_str();
        let to_path = command.to_path.as_str();
        emit_invoked(
            &self.event_tx,
            COGNITION_VAULT_MOVE_ID.as_str(),
            &format!("{from_path} -> {to_path}"),
        );
        VaultService::relocate_note(from_path, to_path)
            .map_err(|err| StasisError::PortFailure(err.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use stasis::application::orchestration::tool_registry::StasisTool;

    use super::{COGNITION_VAULT_LIST_ID, CognitionVaultListTool, VaultListInput, VaultWriteInput};
    use crate::events::TuiEvent;
    use crate::semantic_values::TrimmedText;

    #[test]
    fn typed_vault_list_keeps_injected_events_and_lenient_wire_inputs() {
        crate::vault::service::with_temp_vault(|| {
            let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(1);
            let tool = CognitionVaultListTool::new(event_tx);
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("test runtime");

            let input = crate::typed_tools::deserialize_input::<VaultListInput>(
                COGNITION_VAULT_LIST_ID,
                json!({
                    "prefix": 42,
                    "limit": "many",
                    "semantic_tags": "alpha, beta",
                    "tag_prefix": false
                }),
            )
            .expect("legacy optional forms");
            assert_eq!(input.prefix, None);
            assert_eq!(input.limit, None);
            assert_eq!(
                input.semantic_tags,
                Some(vec!["alpha".to_string(), "beta".to_string()])
            );
            assert_eq!(input.tag_prefix, None);

            let output = runtime
                .block_on(tool.invoke_typed(input))
                .expect("typed list response");
            assert!(output.notes.is_empty());

            match event_rx.try_recv().expect("injected event sender used") {
                TuiEvent::ToolInvoked {
                    tool_name,
                    input_summary,
                } => {
                    assert_eq!(tool_name, COGNITION_VAULT_LIST_ID.as_str());
                    assert_eq!(input_summary, "*");
                }
                other => panic!("unexpected event: {other:?}"),
            }

            let boundary = runtime
                .block_on(StasisTool::invoke(&tool, json!({ "limit": 1 })))
                .expect("generated Stasis boundary");
            assert!(boundary["notes"].is_array());
        });
    }

    #[test]
    fn vault_write_input_preserves_explicit_session_presence() {
        let omitted: VaultWriteInput = serde_json::from_value(json!({
            "path": "notes/typed.md",
            "content": "hello"
        }))
        .expect("omitted session input");
        assert!(!omitted.session_id_provided);
        assert_eq!(omitted.session_id, None);

        let explicit_null: VaultWriteInput = serde_json::from_value(json!({
            "path": "notes/typed.md",
            "content": "hello",
            "session_id": null
        }))
        .expect("explicit null session input");
        assert!(explicit_null.session_id_provided);
        assert_eq!(explicit_null.session_id, None);

        let explicit: VaultWriteInput = serde_json::from_value(json!({
            "path": "notes/typed.md",
            "content": "hello",
            "session_id": "chat-1"
        }))
        .expect("explicit session input");
        assert!(explicit.session_id_provided);
        assert_eq!(explicit.session_id.as_deref(), Some("chat-1"));
    }

    #[test]
    fn vault_write_command_preserves_content_and_normalizes_identifiers() {
        let command = super::VaultWriteCommand::try_from(VaultWriteInput {
            path: Some("  notes/typed.md  ".to_string()),
            content: Some("  # Heading\n\nbody  \n".to_string()),
            session_id: Some("  chat-1  ".to_string()),
            session_id_provided: true,
            semantic_tags: None,
            auto_workshop_tags: None,
            if_match: Some("  digest  ".to_string()),
        })
        .expect("command");

        assert_eq!(command.path.as_str(), "notes/typed.md");
        assert_eq!(command.content.as_str(), "  # Heading\n\nbody  \n");
        assert_eq!(
            command.session_id.as_ref().map(TrimmedText::as_str),
            Some("chat-1")
        );
        assert!(command.auto_workshop_tags);
        assert_eq!(command.if_match.as_deref(), Some("  digest  "));
    }

    #[test]
    fn vault_write_command_rejects_blank_content() {
        let error = super::VaultWriteCommand::try_from(VaultWriteInput {
            path: Some("notes/blank.md".to_string()),
            content: Some(" \n\t".to_string()),
            session_id: None,
            session_id_provided: false,
            semantic_tags: None,
            auto_workshop_tags: None,
            if_match: None,
        })
        .expect_err("blank content should fail");
        assert!(error.to_string().contains("content is required"));
    }

    #[test]
    fn vault_read_and_grep_commands_normalize_paths_and_clamp_bounds() {
        let read = super::VaultReadCommand::try_from(super::VaultReadInput {
            path: Some("  notes/read.md  ".to_string()),
            max_chars: Some(99_999),
            line_start: Some(3),
            line_end: Some(8),
        })
        .expect("read command");
        assert_eq!(read.path.as_str(), "notes/read.md");
        assert_eq!(read.max_chars, 20_000);
        assert_eq!(read.line_start, Some(3));

        let grep = super::VaultGrepCommand::try_from(super::VaultGrepInput {
            path: Some("  notes/read.md  ".to_string()),
            pattern: Some("  heading  ".to_string()),
            context_lines: Some(99),
            limit: Some(999),
        })
        .expect("grep command");
        assert_eq!(grep.path.as_str(), "notes/read.md");
        assert_eq!(grep.pattern.as_str(), "heading");
        assert_eq!(grep.context_lines, 10);
        assert_eq!(grep.limit, 200);
    }

    #[test]
    fn vault_search_and_move_commands_validate_cross_field_inputs() {
        let search = super::VaultSearchCommand::try_from(super::VaultSearchInput {
            q: Some("  async rust  ".to_string()).into(),
            semantic_tags: None,
            limit: Some(7).into(),
        })
        .expect("search command");
        assert_eq!(
            search.query.as_ref().map(TrimmedText::as_str),
            Some("async rust")
        );
        assert_eq!(search.limit, 7);

        let move_command = super::VaultMoveCommand::try_from(super::VaultMoveInput {
            from_path: Some("  old.md  ".to_string()),
            to_path: Some("  new.md  ".to_string()),
        })
        .expect("move command");
        assert_eq!(move_command.from_path.as_str(), "old.md");
        assert_eq!(move_command.to_path.as_str(), "new.md");

        let error = super::VaultSearchCommand::try_from(super::VaultSearchInput {
            q: Some(" \n".to_string()).into(),
            semantic_tags: None,
            limit: None::<usize>.into(),
        })
        .expect_err("empty search command should fail");
        assert!(error.to_string().contains("q or semantic_tags is required"));
    }
}
