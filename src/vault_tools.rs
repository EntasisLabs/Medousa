//! Host-bus vault tools: list, read, search, write, tags.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use stasis::domain::errors::{Result as StasisResult, StasisError};
use tokio::sync::{RwLock, mpsc};

use crate::daemon_api::{
    VaultDeleteResponse, VaultNote, VaultNotesListResponse, VaultSearchResponse, VaultWriteRequest,
    VaultWriteResponse,
};
use crate::events::TuiEvent;
use crate::locus_semantic_tags::parse_semantic_tags_from_value;
use crate::turn_continuation::TurnContinuationScope;
use crate::typed_tools::{ToolId, medousa_tool};
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
    registry: &mut impl crate::typed_tools::ToolRegistration,
    event_tx: mpsc::Sender<TuiEvent>,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
    fallback_chat_session_id: String,
) -> StasisResult<()> {
    registry.register_typed_tool(CognitionVaultListTool::new(event_tx.clone()))?;
    registry.register_typed_tool(CognitionVaultReadTool::new(event_tx.clone()))?;
    registry.register_typed_tool(CognitionVaultGrepTool::new(event_tx.clone()))?;
    registry.register_typed_tool(CognitionVaultSearchTool::new(event_tx.clone()))?;
    registry.register_typed_tool(CognitionVaultTagsTool::new(event_tx.clone()))?;
    registry.register_typed_tool(CognitionVaultWriteTool::new(
        event_tx.clone(),
        turn_scope.clone(),
        fallback_chat_session_id.clone(),
    ))?;
    registry.register_typed_tool(CognitionVaultDeleteTool::new(event_tx.clone()))?;
    registry.register_typed_tool(CognitionVaultMoveTool::new(event_tx))?;
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

#[derive(Debug, Deserialize, JsonSchema)]
pub struct VaultListInput {
    /// Optional path prefix filter
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_usize"
    )]
    #[schemars(
        with = "usize",
        range(min = 1, max = 200),
        skip_serializing_if = "Option::is_none"
    )]
    pub limit: Option<usize>,
    /// Indexed-style tag filter (match-all), aligned with Locus tags
    #[serde(default, deserialize_with = "deserialize_lenient_semantic_tags")]
    #[schemars(with = "Vec<String>", skip_serializing_if = "Option::is_none")]
    pub semantic_tags: Option<Vec<String>>,
    /// Filter notes with tags sharing this prefix
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub tag_prefix: Option<String>,
}

fn deserialize_lenient_semantic_tags<'de, D>(
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
    async fn invoke_typed(
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
    path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        with = "usize",
        range(min = 256, max = 20000),
        skip_serializing_if = "Option::is_none"
    )]
    max_chars: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        with = "usize",
        range(min = 1),
        skip_serializing_if = "Option::is_none"
    )]
    line_start: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        with = "usize",
        range(min = 1),
        skip_serializing_if = "Option::is_none"
    )]
    line_end: Option<usize>,
}

impl<'de> Deserialize<'de> for VaultReadInput {
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
            path: Option<String>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_usize"
            )]
            max_chars: Option<usize>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_usize"
            )]
            line_start: Option<usize>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_usize"
            )]
            line_end: Option<usize>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            path: input.path,
            max_chars: input.max_chars,
            line_start: input.line_start,
            line_end: input.line_end,
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
    async fn invoke_typed(
        &self,
        input: VaultReadInput,
    ) -> stasis::prelude::Result<VaultReadOutput> {
        let path = input
            .path
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StasisError::PortFailure("path is required".to_string()))?;
        let max_chars = input
            .max_chars
            .unwrap_or(READ_BUDGET_CHARS)
            .clamp(256, 20_000);
        let line_start = input.line_start;
        let line_end = input.line_end;
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
    path: Option<String>,
    #[schemars(required, with = "String")]
    pattern: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        with = "usize",
        range(min = 0, max = 10),
        skip_serializing_if = "Option::is_none"
    )]
    context_lines: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        with = "usize",
        range(min = 1, max = 200),
        skip_serializing_if = "Option::is_none"
    )]
    limit: Option<usize>,
}

impl<'de> Deserialize<'de> for VaultGrepInput {
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
            path: Option<String>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            pattern: Option<String>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_usize"
            )]
            context_lines: Option<usize>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_usize"
            )]
            limit: Option<usize>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            path: input.path,
            pattern: input.pattern,
            context_lines: input.context_lines,
            limit: input.limit,
        })
    }
}

#[medousa_tool(id = COGNITION_VAULT_GREP_ID)]
impl CognitionVaultGrepTool {
    /// Search inside a vault note (literal case-insensitive match with line numbers). Use cognition_vault_search to discover notes; use grep for surgical edits.
    async fn invoke_typed(
        &self,
        input: VaultGrepInput,
    ) -> stasis::prelude::Result<crate::line_grep::LineGrepResult> {
        let path = input
            .path
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StasisError::PortFailure("path is required".to_string()))?;
        let pattern = input
            .pattern
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StasisError::PortFailure("pattern is required".to_string()))?;
        let context_lines = input.context_lines.unwrap_or(2).clamp(0, 10);
        let limit = input.limit.unwrap_or(20).clamp(1, 200);
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
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    q: Option<String>,
    #[serde(default, deserialize_with = "deserialize_lenient_semantic_tags")]
    #[schemars(with = "Vec<String>", skip_serializing_if = "Option::is_none")]
    semantic_tags: Option<Vec<String>>,
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_usize"
    )]
    #[schemars(
        with = "usize",
        range(min = 1, max = 50),
        skip_serializing_if = "Option::is_none"
    )]
    limit: Option<usize>,
}

#[medousa_tool(id = COGNITION_VAULT_SEARCH_ID)]
impl CognitionVaultSearchTool {
    /// Search vault notes by full text and/or semantic tags (match-all).
    async fn invoke_typed(
        &self,
        input: VaultSearchInput,
    ) -> stasis::prelude::Result<VaultSearchResponse> {
        let query = input
            .q
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let tags = input.semantic_tags.map(|tags| tags.join(","));
        if query.is_none() && tags.is_none() {
            return Err(StasisError::PortFailure(
                "q or semantic_tags is required".to_string(),
            ));
        }
        let limit = input.limit.unwrap_or(20);
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
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    prefix: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_usize"
    )]
    #[schemars(
        with = "usize",
        range(min = 1, max = 500),
        skip_serializing_if = "Option::is_none"
    )]
    limit: Option<usize>,
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
    async fn invoke_typed(
        &self,
        input: VaultTagsInput,
    ) -> stasis::prelude::Result<VaultTagsOutput> {
        let prefix = input
            .prefix
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let limit = input.limit.unwrap_or(100).clamp(1, 500);
        emit_invoked(
            &self.event_tx,
            COGNITION_VAULT_TAGS_ID.as_str(),
            prefix.unwrap_or("all"),
        );
        let response = VaultService::list_tags(prefix, limit);
        Ok(VaultTagsOutput {
            tags: response.tags,
            count: response.count,
            usage: "Use semantic_tags on cognition_vault_list/search/write or match Locus via cognition_memory_tags.".to_string(),
        })
    }
}

pub struct CognitionVaultWriteTool {
    event_tx: mpsc::Sender<TuiEvent>,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
    fallback_chat_session_id: String,
}

impl CognitionVaultWriteTool {
    pub fn new(
        event_tx: mpsc::Sender<TuiEvent>,
        turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
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
    path: Option<String>,
    #[schemars(required, with = "String")]
    content: Option<String>,
    /// Chat session for workshop linking tags (defaults to current turn session)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
    #[schemars(skip)]
    session_id_provided: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Vec<String>", skip_serializing_if = "Option::is_none")]
    semantic_tags: Option<Vec<String>>,
    /// Merge medousa/vault/session/profile/chat defaults (default true)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "bool", skip_serializing_if = "Option::is_none")]
    auto_workshop_tags: Option<bool>,
    /// Optional content_hash for optimistic concurrency
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    if_match: Option<String>,
}

impl<'de> Deserialize<'de> for VaultWriteInput {
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
            path: Option<String>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            content: Option<String>,
            #[serde(default)]
            session_id: LenientStringPresence,
            #[serde(default, deserialize_with = "deserialize_lenient_semantic_tags")]
            semantic_tags: Option<Vec<String>>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_bool"
            )]
            auto_workshop_tags: Option<bool>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            if_match: Option<String>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            path: input.path,
            content: input.content,
            session_id: input.session_id.value,
            session_id_provided: input.session_id.provided,
            semantic_tags: input.semantic_tags,
            auto_workshop_tags: input.auto_workshop_tags,
            if_match: input.if_match,
        })
    }
}

#[medousa_tool(id = COGNITION_VAULT_WRITE_ID)]
impl CognitionVaultWriteTool {
    /// Create or update a vault markdown note. Merges Locus-aligned semantic tags into frontmatter.
    async fn invoke_typed(
        &self,
        input: VaultWriteInput,
    ) -> stasis::prelude::Result<VaultWriteResponse> {
        let path = input
            .path
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StasisError::PortFailure("path is required".to_string()))?
            .to_string();
        let content = input
            .content
            .ok_or_else(|| StasisError::PortFailure("content is required".to_string()))?;
        let session_id = if input.session_id_provided {
            input
                .session_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        } else {
            let chat_session_id = crate::runtime_session::resolve_active_chat_session_id_async(
                &self.turn_scope,
                &self.fallback_chat_session_id,
            )
            .await;
            Some(crate::locus_memory::resolve_workshop_locus_session(
                &chat_session_id,
            ))
        };
        emit_invoked(&self.event_tx, COGNITION_VAULT_WRITE_ID.as_str(), &path);
        let request = VaultWriteRequest {
            path: Some(path.clone()),
            content,
            session_id,
            semantic_tags: input.semantic_tags,
            auto_workshop_tags: input.auto_workshop_tags.unwrap_or(true),
        };
        VaultService::write_note_with_actor(
            Some(&path),
            &request,
            input.if_match.as_deref(),
            crate::daemon_api::WorkspaceEventActor::Agent,
            Some("cognition_vault_write"),
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
    path: Option<String>,
}

impl<'de> Deserialize<'de> for VaultDeleteInput {
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
            path: Option<String>,
        }
        let input = WireInput::deserialize(deserializer)?;
        Ok(Self { path: input.path })
    }
}

#[medousa_tool(id = COGNITION_VAULT_DELETE_ID)]
impl CognitionVaultDeleteTool {
    /// Soft-delete a vault markdown note (moves to .trash). Use after confirming the path with list/read.
    async fn invoke_typed(
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
    from_path: Option<String>,
    /// Destination path
    #[schemars(required, with = "String")]
    to_path: Option<String>,
}

impl<'de> Deserialize<'de> for VaultMoveInput {
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
            from_path: Option<String>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            to_path: Option<String>,
        }
        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            from_path: input.from_path,
            to_path: input.to_path,
        })
    }
}

#[medousa_tool(id = COGNITION_VAULT_MOVE_ID)]
impl CognitionVaultMoveTool {
    /// Move/rename a vault note to a new relative path. Creates parent folders as needed and removes the source note.
    async fn invoke_typed(
        &self,
        input: VaultMoveInput,
    ) -> stasis::prelude::Result<VaultWriteResponse> {
        let from_path = input
            .from_path
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StasisError::PortFailure("from_path is required".to_string()))?;
        let to_path = input
            .to_path
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StasisError::PortFailure("to_path is required".to_string()))?;
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
}
