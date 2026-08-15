//! Agent tools for listing, reading, grepping, and revising HTML UI artifacts.


use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use stasis::prelude::{Result as StasisResult, StasisError};
use tokio::sync::mpsc;

use crate::events::TuiEvent;
use crate::runtime_session::{require_active_chat_session_id_async, runtime_bootstrap_session_id};
use crate::semantic_values::{RequiredContent, TrimmedText};
use crate::typed_tools::{CompatOption, ToolId, medousa_tool};

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

fn required_artifact_identifier(
    value: Option<String>,
    field: &str,
) -> stasis::prelude::Result<TrimmedText> {
    let value = value.ok_or_else(|| StasisError::PortFailure(format!("{field} is required")))?;
    TrimmedText::new(value).map_err(|_| StasisError::PortFailure(format!("{field} is required")))
}

pub fn is_artifact_cognition_tool(name: &str) -> bool {
    ARTIFACT_COGNITION_TOOLS.contains(&name)
}

pub fn register_artifact_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    event_tx: mpsc::Sender<TuiEvent>,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
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
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
}

impl ArtifactToolContext {
    fn new(turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess) -> Self {
        Self { turn_scope }
    }

    async fn require_ui_artifacts(&self) -> StasisResult<()> {
        let supported = crate::agent_runtime::execution_context::turn_continuation_scope(
            &self.turn_scope,
        )
            .await
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
        turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    ) -> Self {
        Self {
            event_tx,
            ctx: ArtifactToolContext::new(turn_scope),
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ArtifactListInput {
    #[serde(default)]
    #[schemars(
        with = "usize",
        range(min = 1, max = 100),
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    limit: CompatOption<usize>,
    /// Optional filter on title or artifact_id
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    query: CompatOption<String>,
}

#[derive(Debug)]
struct ArtifactListCommand {
    limit: usize,
    query: Option<TrimmedText>,
}

impl TryFrom<ArtifactListInput> for ArtifactListCommand {
    type Error = stasis::prelude::StasisError;

    fn try_from(input: ArtifactListInput) -> Result<Self, Self::Error> {
        Ok(Self {
            limit: input.limit.into_option().unwrap_or(20).clamp(1, 100),
            query: input
                .query
                .into_option()
                .as_deref()
                .and_then(|value| TrimmedText::new(value).ok()),
        })
    }
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
        let command = ArtifactListCommand::try_from(input)?;
        let limit = command.limit;
        let query_owned = command.query.map(TrimmedText::into_string);
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
        turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
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
            #[serde(default)]
            artifact_id: CompatOption<String>,
            #[serde(default)]
            line_start: CompatOption<usize>,
            #[serde(default)]
            line_end: CompatOption<usize>,
            #[serde(default)]
            max_chars: CompatOption<usize>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            artifact_id: input.artifact_id.into_option(),
            line_start: input.line_start.into_option(),
            line_end: input.line_end.into_option(),
            max_chars: input.max_chars.into_option(),
        })
    }
}

#[derive(Debug)]
struct ArtifactReadCommand {
    artifact_id: TrimmedText,
    line_start: Option<usize>,
    line_end: Option<usize>,
    max_chars: usize,
}

impl TryFrom<ArtifactReadInput> for ArtifactReadCommand {
    type Error = stasis::prelude::StasisError;

    fn try_from(input: ArtifactReadInput) -> Result<Self, Self::Error> {
        Ok(Self {
            artifact_id: required_artifact_identifier(input.artifact_id, "artifact_id")?,
            line_start: input.line_start,
            line_end: input.line_end,
            max_chars: input
                .max_chars
                .unwrap_or(READ_BUDGET_CHARS)
                .clamp(256, 20_000),
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
        let command = ArtifactReadCommand::try_from(input)?;
        let artifact_id = command.artifact_id.into_string();
        let line_start = command.line_start;
        let line_end = command.line_end;
        let max_chars = command.max_chars;
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
        turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
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
            #[serde(default)]
            artifact_id: CompatOption<String>,
            #[serde(default)]
            pattern: CompatOption<String>,
            #[serde(default)]
            context_lines: CompatOption<usize>,
            #[serde(default)]
            limit: CompatOption<usize>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            artifact_id: input.artifact_id.into_option(),
            pattern: input.pattern.into_option(),
            context_lines: input.context_lines.into_option(),
            limit: input.limit.into_option(),
        })
    }
}

#[derive(Debug)]
struct ArtifactGrepCommand {
    artifact_id: TrimmedText,
    pattern: TrimmedText,
    context_lines: usize,
    limit: usize,
}

impl TryFrom<ArtifactGrepInput> for ArtifactGrepCommand {
    type Error = stasis::prelude::StasisError;

    fn try_from(input: ArtifactGrepInput) -> Result<Self, Self::Error> {
        Ok(Self {
            artifact_id: required_artifact_identifier(input.artifact_id, "artifact_id")?,
            pattern: required_artifact_identifier(input.pattern, "pattern")?,
            context_lines: input.context_lines.unwrap_or(2).clamp(0, 10),
            limit: input.limit.unwrap_or(20).clamp(1, 200),
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
        let command = ArtifactGrepCommand::try_from(input)?;
        let artifact_id = command.artifact_id.into_string();
        let pattern = command.pattern.into_string();
        let context_lines = command.context_lines;
        let limit = command.limit;
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
        turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
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
            #[serde(default)]
            title: CompatOption<String>,
            #[serde(default)]
            html: CompatOption<String>,
            #[serde(default)]
            presentation: CompatOption<String>,
            #[serde(default)]
            artifact_id: CompatOption<String>,
            #[serde(default)]
            if_match_hash64: CompatOption<String>,
            #[serde(default)]
            height: CompatOption<u64>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            title: input.title.into_option(),
            html: input.html.into_option(),
            presentation: input.presentation.into_option(),
            artifact_id: input.artifact_id.into_option(),
            if_match_hash64: input.if_match_hash64.into_option(),
            height: input.height.into_option(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArtifactPresentation {
    Inline,
    Panel,
    Fullscreen,
}

impl ArtifactPresentation {
    fn parse(value: Option<String>) -> stasis::prelude::Result<Self> {
        let value = value.unwrap_or_else(|| "inline".to_string());
        match value.trim().to_ascii_lowercase().as_str() {
            "" | "inline" => Ok(Self::Inline),
            "panel" => Ok(Self::Panel),
            "fullscreen" => Ok(Self::Fullscreen),
            other => Err(StasisError::PortFailure(format!(
                "presentation must be inline, panel, or fullscreen (got {other})"
            ))),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Inline => "inline",
            Self::Panel => "panel",
            Self::Fullscreen => "fullscreen",
        }
    }
}

#[derive(Debug)]
struct ArtifactWriteCommand {
    title: TrimmedText,
    html: RequiredContent,
    presentation: ArtifactPresentation,
    artifact_id: Option<TrimmedText>,
    if_match_hash64: Option<TrimmedText>,
    height_px: Option<u32>,
}

impl TryFrom<ArtifactWriteInput> for ArtifactWriteCommand {
    type Error = stasis::prelude::StasisError;

    fn try_from(input: ArtifactWriteInput) -> Result<Self, Self::Error> {
        let title = required_artifact_identifier(input.title, "title")?;
        let html = RequiredContent::new(
            input
                .html
                .ok_or_else(|| StasisError::PortFailure("html is required".to_string()))?,
        )
        .map_err(|_| StasisError::PortFailure("html is required".to_string()))?;
        let artifact_id = input
            .artifact_id
            .and_then(|value| TrimmedText::new(value).ok());
        let if_match_hash64 = input
            .if_match_hash64
            .and_then(|value| TrimmedText::new(value).ok());

        Ok(Self {
            title,
            html,
            presentation: ArtifactPresentation::parse(input.presentation)?,
            artifact_id,
            if_match_hash64,
            height_px: input.height.map(|value| value.clamp(120, 1200) as u32),
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
        let command = ArtifactWriteCommand::try_from(input)?;
        let title = command.title.into_string();
        let html = command.html.into_string();
        let presentation = command.presentation.as_str().to_string();
        let height_px = command.height_px;
        let artifact_id = command.artifact_id.map(TrimmedText::into_string);
        let if_match_hash64 = command.if_match_hash64.map(TrimmedText::into_string);
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
        turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    ) -> Self {
        Self {
            event_tx,
            ctx: ArtifactToolContext::new(turn_scope),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic_values::TrimmedText;

    #[test]
    fn artifact_commands_normalize_identifiers_and_preserve_html() {
        let write = ArtifactWriteCommand::try_from(ArtifactWriteInput {
            title: Some("  Chart  ".to_string()),
            html: Some("  <div>Chart</div>  \n".to_string()),
            presentation: Some(" PANEL ".to_string()),
            artifact_id: Some("  art:old  ".to_string()),
            if_match_hash64: Some("  hash  ".to_string()),
            height: Some(9999),
        })
        .expect("write command");
        assert_eq!(write.title.as_str(), "Chart");
        assert_eq!(write.html.as_str(), "  <div>Chart</div>  \n");
        assert_eq!(write.presentation, ArtifactPresentation::Panel);
        assert_eq!(
            write.artifact_id.as_ref().map(TrimmedText::as_str),
            Some("art:old")
        );
        assert_eq!(write.height_px, Some(1200));

        let read = ArtifactReadCommand::try_from(ArtifactReadInput {
            artifact_id: Some("  art:old  ".to_string()),
            line_start: Some(2),
            line_end: Some(4),
            max_chars: Some(99_999),
        })
        .expect("read command");
        assert_eq!(read.artifact_id.as_str(), "art:old");
        assert_eq!(read.max_chars, 20_000);

        let grep = ArtifactGrepCommand::try_from(ArtifactGrepInput {
            artifact_id: Some("  art:old  ".to_string()),
            pattern: Some("  Chart  ".to_string()),
            context_lines: Some(99),
            limit: Some(999),
        })
        .expect("grep command");
        assert_eq!(grep.pattern.as_str(), "Chart");
        assert_eq!(grep.context_lines, 10);
        assert_eq!(grep.limit, 200);
    }

    #[test]
    fn artifact_write_command_rejects_blank_html() {
        let error = ArtifactWriteCommand::try_from(ArtifactWriteInput {
            title: Some("Chart".to_string()),
            html: Some(" \n\t".to_string()),
            presentation: None,
            artifact_id: None,
            if_match_hash64: None,
            height: None,
        })
        .expect_err("blank html should fail");
        assert!(error.to_string().contains("html is required"));
    }

    #[test]
    fn artifact_wire_optionals_remain_lenient_for_legacy_values() {
        let list: ArtifactListInput = serde_json::from_value(serde_json::json!({
            "limit": "100",
            "query": 42,
        }))
        .expect("list input");
        assert!(list.limit.into_option().is_none());
        assert!(list.query.into_option().is_none());

        let write: ArtifactWriteInput = serde_json::from_value(serde_json::json!({
            "title": 42,
            "html": false,
            "presentation": ["panel"],
            "height": "1200",
        }))
        .expect("write input");
        assert!(write.title.is_none());
        assert!(write.html.is_none());
        assert!(write.presentation.is_none());
        assert!(write.height.is_none());

        let delete: ArtifactDeleteInput = serde_json::from_value(serde_json::json!({
            "artifact_id": false,
        }))
        .expect("delete input");
        assert!(delete.artifact_id.is_none());
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
            #[serde(default)]
            artifact_id: CompatOption<String>,
        }
        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            artifact_id: input.artifact_id.into_option(),
        })
    }
}

#[derive(Debug)]
struct ArtifactDeleteCommand {
    artifact_id: TrimmedText,
}

impl TryFrom<ArtifactDeleteInput> for ArtifactDeleteCommand {
    type Error = stasis::prelude::StasisError;

    fn try_from(input: ArtifactDeleteInput) -> Result<Self, Self::Error> {
        Ok(Self {
            artifact_id: required_artifact_identifier(input.artifact_id, "artifact_id")?,
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
        let command = ArtifactDeleteCommand::try_from(input)?;
        let artifact_id = command.artifact_id.into_string();
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
