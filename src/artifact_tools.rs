//! Agent tools for listing, reading, grepping, and revising HTML UI artifacts.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use stasis::prelude::{Result as StasisResult, StasisError};
use tokio::sync::{RwLock, mpsc};

use crate::events::TuiEvent;
use crate::runtime_session::{require_active_chat_session_id_async, runtime_bootstrap_session_id};
use crate::turn_continuation::TurnContinuationScope;
use crate::typed_tools::{ToolId, medousa_tool};

pub const COGNITION_ARTIFACT_LIST: &str = "cognition_artifact_list";
pub const COGNITION_ARTIFACT_READ: &str = "cognition_artifact_read";
pub const COGNITION_ARTIFACT_GREP: &str = "cognition_artifact_grep";
pub const COGNITION_ARTIFACT_WRITE: &str = "cognition_artifact_write";
pub const COGNITION_ARTIFACT_DELETE: &str = "cognition_artifact_delete";
const COGNITION_ARTIFACT_LIST_ID: ToolId = ToolId::new(COGNITION_ARTIFACT_LIST);
const COGNITION_ARTIFACT_READ_ID: ToolId = ToolId::new(COGNITION_ARTIFACT_READ);
const COGNITION_ARTIFACT_GREP_ID: ToolId = ToolId::new(COGNITION_ARTIFACT_GREP);
const COGNITION_ARTIFACT_WRITE_ID: ToolId = ToolId::new(COGNITION_ARTIFACT_WRITE);
const COGNITION_ARTIFACT_DELETE_ID: ToolId = ToolId::new(COGNITION_ARTIFACT_DELETE);

pub const ARTIFACT_COGNITION_TOOLS: &[&str] = &[
    COGNITION_ARTIFACT_LIST,
    COGNITION_ARTIFACT_READ,
    COGNITION_ARTIFACT_GREP,
    COGNITION_ARTIFACT_WRITE,
    COGNITION_ARTIFACT_DELETE,
];

const READ_BUDGET_CHARS: usize = 12_000;

pub fn is_artifact_cognition_tool(name: &str) -> bool {
    ARTIFACT_COGNITION_TOOLS.contains(&name)
}

pub fn register_artifact_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    event_tx: mpsc::Sender<TuiEvent>,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
) -> StasisResult<()> {
    registry.register_typed_tool(CognitionArtifactListTool::new(
        event_tx.clone(),
        turn_scope.clone(),
    ))?;
    registry.register_typed_tool(CognitionArtifactReadTool::new(
        event_tx.clone(),
        turn_scope.clone(),
    ))?;
    registry.register_typed_tool(CognitionArtifactGrepTool::new(
        event_tx.clone(),
        turn_scope.clone(),
    ))?;
    registry.register_typed_tool(CognitionArtifactWriteTool::new(
        event_tx.clone(),
        turn_scope.clone(),
    ))?;
    registry.register_typed_tool(CognitionArtifactDeleteTool::new(event_tx, turn_scope))?;
    Ok(())
}

fn emit_invoked(event_tx: &mpsc::Sender<TuiEvent>, tool_name: &str, summary: &str) {
    let _ = event_tx.try_send(TuiEvent::ToolInvoked {
        tool_name: tool_name.to_string(),
        input_summary: summary.to_string(),
    });
}

struct ArtifactToolContext {
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
}

impl ArtifactToolContext {
    fn new(turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>) -> Self {
        Self { turn_scope }
    }

    async fn require_ui_artifacts(&self) -> StasisResult<()> {
        let supported = self
            .turn_scope
            .read()
            .await
            .as_ref()
            .is_some_and(|scope| scope.supports_ui_artifacts);
        if supported {
            Ok(())
        } else {
            Err(StasisError::PortFailure(
                "This channel does not support HTML UI artifacts (supports_ui_artifacts=false)."
                    .to_string(),
            ))
        }
    }

    async fn session_id(&self, tool_name: &str) -> StasisResult<String> {
        require_active_chat_session_id_async(
            &self.turn_scope,
            runtime_bootstrap_session_id(),
            tool_name,
        )
        .await
    }
}

pub struct CognitionArtifactListTool {
    event_tx: mpsc::Sender<TuiEvent>,
    ctx: ArtifactToolContext,
}

impl CognitionArtifactListTool {
    pub fn new(
        event_tx: mpsc::Sender<TuiEvent>,
        turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
    ) -> Self {
        Self {
            event_tx,
            ctx: ArtifactToolContext::new(turn_scope),
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ArtifactListInput {
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_usize"
    )]
    #[schemars(
        with = "usize",
        range(min = 1, max = 100),
        skip_serializing_if = "Option::is_none"
    )]
    limit: Option<usize>,
    /// Optional filter on title or artifact_id
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    query: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
struct ArtifactListItem {
    artifact_id: String,
    label: Option<String>,
    presentation: Option<String>,
    byte_size: usize,
    #[schemars(with = "String")]
    stored_at_utc: DateTime<Utc>,
    supersedes_artifact_id: Option<String>,
    root_artifact_id: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ArtifactListOutput {
    artifacts: Vec<ArtifactListItem>,
    count: usize,
}

#[medousa_tool(id = COGNITION_ARTIFACT_LIST_ID)]
impl CognitionArtifactListTool {
    /// List HTML presentation artifacts for the current chat session (newest first). Workflow: list → grep/read → cognition_artifact_write to revise.
    async fn invoke_typed(
        &self,
        input: ArtifactListInput,
    ) -> stasis::prelude::Result<ArtifactListOutput> {
        self.ctx.require_ui_artifacts().await?;
        let session_id = self.ctx.session_id(COGNITION_ARTIFACT_LIST).await?;
        let limit = input.limit.unwrap_or(20).clamp(1, 100);
        let query_owned = input
            .query
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        emit_invoked(
            &self.event_tx,
            COGNITION_ARTIFACT_LIST,
            query_owned.as_deref().unwrap_or("*"),
        );
        let records = tokio::task::spawn_blocking(move || {
            crate::artifact_store::list_ui_artifacts(
                Some(&session_id),
                limit,
                query_owned.as_deref(),
            )
        })
        .await
        .map_err(|err| StasisError::PortFailure(format!("artifact list join error: {err}")))?;
        let artifacts: Vec<ArtifactListItem> = records
            .into_iter()
            .map(|record| ArtifactListItem {
                artifact_id: record.artifact_id,
                label: record.label,
                presentation: record.presentation,
                byte_size: record.byte_size,
                stored_at_utc: record.stored_at_utc,
                supersedes_artifact_id: record.supersedes_artifact_id,
                root_artifact_id: record.root_artifact_id,
            })
            .collect();
        let count = artifacts.len();
        Ok(ArtifactListOutput { artifacts, count })
    }
}

pub struct CognitionArtifactReadTool {
    event_tx: mpsc::Sender<TuiEvent>,
    ctx: ArtifactToolContext,
}

impl CognitionArtifactReadTool {
    pub fn new(
        event_tx: mpsc::Sender<TuiEvent>,
        turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
    ) -> Self {
        Self {
            event_tx,
            ctx: ArtifactToolContext::new(turn_scope),
        }
    }
}

#[derive(Debug, JsonSchema)]
pub struct ArtifactReadInput {
    #[schemars(required, with = "String")]
    artifact_id: Option<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        with = "usize",
        range(min = 256, max = 20000),
        skip_serializing_if = "Option::is_none"
    )]
    max_chars: Option<usize>,
}

impl<'de> Deserialize<'de> for ArtifactReadInput {
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
            artifact_id: Option<String>,
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
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_usize"
            )]
            max_chars: Option<usize>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            artifact_id: input.artifact_id,
            line_start: input.line_start,
            line_end: input.line_end,
            max_chars: input.max_chars,
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ArtifactReadOutput {
    artifact_id: String,
    content: String,
    truncated: bool,
    total_lines: usize,
    total_chars: usize,
    line_start: usize,
    line_end: usize,
}

#[medousa_tool(id = COGNITION_ARTIFACT_READ_ID)]
impl CognitionArtifactReadTool {
    /// Read HTML source for a presentation artifact (budget-capped). Optional line_start/line_end for surgical edits.
    async fn invoke_typed(
        &self,
        input: ArtifactReadInput,
    ) -> stasis::prelude::Result<ArtifactReadOutput> {
        self.ctx.require_ui_artifacts().await?;
        let session_id = self.ctx.session_id(COGNITION_ARTIFACT_READ).await?;
        let artifact_id = input
            .artifact_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StasisError::PortFailure("artifact_id is required".to_string()))?
            .to_string();
        let line_start = input.line_start;
        let line_end = input.line_end;
        let max_chars = input
            .max_chars
            .unwrap_or(READ_BUDGET_CHARS)
            .clamp(256, 20_000);
        emit_invoked(&self.event_tx, COGNITION_ARTIFACT_READ, &artifact_id);
        let artifact_id_for_response = artifact_id.clone();
        let excerpt = tokio::task::spawn_blocking(move || {
            crate::artifact_store::read_ui_artifact_excerpt(
                &session_id,
                &artifact_id,
                line_start,
                line_end,
                max_chars,
            )
        })
        .await
        .map_err(|err| StasisError::PortFailure(format!("artifact read join error: {err}")))?
        .map_err(StasisError::PortFailure)?;
        Ok(ArtifactReadOutput {
            artifact_id: artifact_id_for_response,
            content: excerpt.content,
            truncated: excerpt.truncated,
            total_lines: excerpt.total_lines,
            total_chars: excerpt.total_chars,
            line_start: excerpt.line_start,
            line_end: excerpt.line_end,
        })
    }
}

pub struct CognitionArtifactGrepTool {
    event_tx: mpsc::Sender<TuiEvent>,
    ctx: ArtifactToolContext,
}

impl CognitionArtifactGrepTool {
    pub fn new(
        event_tx: mpsc::Sender<TuiEvent>,
        turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
    ) -> Self {
        Self {
            event_tx,
            ctx: ArtifactToolContext::new(turn_scope),
        }
    }
}

#[derive(Debug, JsonSchema)]
pub struct ArtifactGrepInput {
    #[schemars(required, with = "String")]
    artifact_id: Option<String>,
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

impl<'de> Deserialize<'de> for ArtifactGrepInput {
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
            artifact_id: Option<String>,
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
            artifact_id: input.artifact_id,
            pattern: input.pattern,
            context_lines: input.context_lines,
            limit: input.limit,
        })
    }
}

#[medousa_tool(id = COGNITION_ARTIFACT_GREP_ID)]
impl CognitionArtifactGrepTool {
    /// Search inside an HTML artifact source (literal case-insensitive match with line numbers). Use before cognition_artifact_write to locate CSS/HTML snippets.
    async fn invoke_typed(
        &self,
        input: ArtifactGrepInput,
    ) -> stasis::prelude::Result<crate::line_grep::LineGrepResult> {
        self.ctx.require_ui_artifacts().await?;
        let session_id = self.ctx.session_id(COGNITION_ARTIFACT_GREP).await?;
        let artifact_id = input
            .artifact_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StasisError::PortFailure("artifact_id is required".to_string()))?
            .to_string();
        let pattern = input
            .pattern
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StasisError::PortFailure("pattern is required".to_string()))?
            .to_string();
        let context_lines = input.context_lines.unwrap_or(2).clamp(0, 10);
        let limit = input.limit.unwrap_or(20).clamp(1, 200);
        emit_invoked(&self.event_tx, COGNITION_ARTIFACT_GREP, &artifact_id);
        let result = tokio::task::spawn_blocking(move || {
            crate::artifact_store::grep_ui_artifact(
                &session_id,
                &artifact_id,
                &pattern,
                context_lines,
                limit,
            )
        })
        .await
        .map_err(|err| StasisError::PortFailure(format!("artifact grep join error: {err}")))?
        .map_err(StasisError::PortFailure)?;
        Ok(result)
    }
}

pub struct CognitionArtifactWriteTool {
    event_tx: mpsc::Sender<TuiEvent>,
    ctx: ArtifactToolContext,
}

impl CognitionArtifactWriteTool {
    pub fn new(
        event_tx: mpsc::Sender<TuiEvent>,
        turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
    ) -> Self {
        Self {
            event_tx,
            ctx: ArtifactToolContext::new(turn_scope),
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, JsonSchema)]
#[serde(rename_all = "lowercase")]
enum ArtifactPresentationSchema {
    Inline,
    Panel,
    Fullscreen,
}

#[derive(Debug, JsonSchema)]
pub struct ArtifactWriteInput {
    #[schemars(required, with = "String")]
    title: Option<String>,
    /// HTML fragment or document. MedousaStore get/set/delete are async — await in async init and handlers (cognition_environment_wiki topic=artifact_runtime).
    #[schemars(required, with = "String")]
    html: Option<String>,
    #[schemars(required, with = "ArtifactPresentationSchema")]
    presentation: Option<String>,
    /// When set, supersedes this artifact revision
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    artifact_id: Option<String>,
    /// Optional hash64 of the artifact being revised
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    if_match_hash64: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        with = "u64",
        range(min = 120, max = 1200),
        skip_serializing_if = "Option::is_none"
    )]
    height: Option<u64>,
}

impl<'de> Deserialize<'de> for ArtifactWriteInput {
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
            title: Option<String>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            html: Option<String>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            presentation: Option<String>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            artifact_id: Option<String>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            if_match_hash64: Option<String>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_u64"
            )]
            height: Option<u64>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            title: input.title,
            html: input.html,
            presentation: input.presentation,
            artifact_id: input.artifact_id,
            if_match_hash64: input.if_match_hash64,
            height: input.height,
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ArtifactWriteOutput {
    ok: bool,
    artifact_id: String,
    previous_artifact_id: Option<String>,
    root_artifact_id: Option<String>,
    label: Option<String>,
    mime: String,
    presentation: Option<String>,
    height_px: Option<u32>,
    byte_size: usize,
    hash64: String,
}

#[medousa_tool(id = COGNITION_ARTIFACT_WRITE_ID)]
impl CognitionArtifactWriteTool {
    /// Create or revise an HTML presentation artifact. Pass artifact_id to publish a new revision (content-addressed). Use if_match_hash64 for optimistic concurrency. First-time publish: use cognition_ui_present. Canvas widgets using MedousaStore: get/set/delete return Promises — use async/await (wiki topic artifact_runtime).
    async fn invoke_typed(
        &self,
        input: ArtifactWriteInput,
    ) -> stasis::prelude::Result<ArtifactWriteOutput> {
        self.ctx.require_ui_artifacts().await?;
        let session_id = self.ctx.session_id(COGNITION_ARTIFACT_WRITE).await?;
        let title = input
            .title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StasisError::PortFailure("title is required".to_string()))?
            .to_string();
        let html = input
            .html
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StasisError::PortFailure("html is required".to_string()))?
            .to_string();
        let presentation = input.presentation.unwrap_or_else(|| "inline".to_string());
        let height_px = input.height.map(|value| value.clamp(120, 1200) as u32);
        let artifact_id = input
            .artifact_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let if_match_hash64 = input
            .if_match_hash64
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        emit_invoked(
            &self.event_tx,
            COGNITION_ARTIFACT_WRITE,
            artifact_id.as_deref().unwrap_or("new"),
        );

        let record = tokio::task::spawn_blocking(move || {
            if let Some(previous_id) = artifact_id.as_deref() {
                if let Some(expected) = if_match_hash64.as_deref() {
                    let previous =
                        crate::artifact_store::fetch_artifact_at_id(&session_id, previous_id)
                            .ok_or_else(|| format!("artifact not found: {previous_id}"))?;
                    if previous.record.hash64 != expected {
                        return Err(format!(
                            "if_match_hash64 mismatch (expected {expected}, got {})",
                            previous.record.hash64
                        ));
                    }
                }
                crate::artifact_store::persist_ui_artifact_revision(
                    &session_id,
                    &html,
                    &title,
                    &presentation,
                    height_px,
                    Some(previous_id),
                )
            } else {
                crate::artifact_store::persist_ui_artifact(
                    &session_id,
                    &html,
                    &title,
                    &presentation,
                    height_px,
                )
            }
        })
        .await
        .map_err(|err| StasisError::PortFailure(format!("artifact write join error: {err}")))?
        .map_err(StasisError::PortFailure)?;

        let previous_artifact_id = record.supersedes_artifact_id.clone();
        if let Some(ref old_id) = previous_artifact_id {
            let _ = crate::artifact_store::rebind_artifact_aliases(
                &record.session_id,
                old_id,
                &record.artifact_id,
            );
        }
        Ok(ArtifactWriteOutput {
            ok: true,
            artifact_id: record.artifact_id,
            previous_artifact_id,
            root_artifact_id: record.root_artifact_id,
            label: record.label,
            mime: record.content_type,
            presentation: record.presentation,
            height_px: record.height_px,
            byte_size: record.byte_size,
            hash64: record.hash64,
        })
    }
}

pub struct CognitionArtifactDeleteTool {
    event_tx: mpsc::Sender<TuiEvent>,
    ctx: ArtifactToolContext,
}

impl CognitionArtifactDeleteTool {
    pub fn new(
        event_tx: mpsc::Sender<TuiEvent>,
        turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
    ) -> Self {
        Self {
            event_tx,
            ctx: ArtifactToolContext::new(turn_scope),
        }
    }
}

#[derive(Debug, JsonSchema)]
pub struct ArtifactDeleteInput {
    /// Presentation artifact id or alias to delete
    #[schemars(required, with = "String")]
    artifact_id: Option<String>,
}

impl<'de> Deserialize<'de> for ArtifactDeleteInput {
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
            artifact_id: Option<String>,
        }
        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            artifact_id: input.artifact_id,
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ArtifactDeleteOutput {
    ok: bool,
    deleted_artifact_ids: Vec<String>,
    count: usize,
}

#[medousa_tool(id = COGNITION_ARTIFACT_DELETE_ID)]
impl CognitionArtifactDeleteTool {
    /// Delete an HTML presentation artifact and its revision chain from the session store. Use cognition_artifact_list to discover artifact_id values first.
    async fn invoke_typed(
        &self,
        input: ArtifactDeleteInput,
    ) -> stasis::prelude::Result<ArtifactDeleteOutput> {
        self.ctx.require_ui_artifacts().await?;
        let session_id = self.ctx.session_id(COGNITION_ARTIFACT_DELETE).await?;
        let artifact_id = input
            .artifact_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StasisError::PortFailure("artifact_id is required".to_string()))?
            .to_string();
        emit_invoked(&self.event_tx, COGNITION_ARTIFACT_DELETE, &artifact_id);

        let deleted = tokio::task::spawn_blocking(move || {
            crate::artifact_store::delete_ui_artifact(&session_id, &artifact_id)
        })
        .await
        .map_err(|err| StasisError::PortFailure(format!("artifact delete join error: {err}")))?
        .map_err(StasisError::PortFailure)?;
        let count = deleted.len();

        Ok(ArtifactDeleteOutput {
            ok: true,
            deleted_artifact_ids: deleted,
            count,
        })
    }
}
