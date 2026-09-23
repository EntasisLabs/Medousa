//! Read-only, profile-scoped access to durable Medousa chat history.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use stasis::domain::errors::{Result as StasisResult, StasisError};

use crate::semantic_values::TrimmedText;
use crate::session_history::{ConversationTurn, SessionHistorySummary, load_history};
use crate::turn_parts::TurnPart;
use crate::typed_tools::{CompatOption, ToolId, medousa_tool};

pub const COGNITION_CHAT_HISTORY_SEARCH: &str = "cognition_chat_history_search";
pub const COGNITION_CHAT_HISTORY_READ: &str = "cognition_chat_history_read";

const COGNITION_CHAT_HISTORY_SEARCH_ID: ToolId = ToolId::new(COGNITION_CHAT_HISTORY_SEARCH);
const COGNITION_CHAT_HISTORY_READ_ID: ToolId = ToolId::new(COGNITION_CHAT_HISTORY_READ);
const DEFAULT_SEARCH_LIMIT: usize = 8;
const MAX_SEARCH_LIMIT: usize = 20;
const DEFAULT_SESSION_SCAN_LIMIT: usize = 80;
const MAX_SESSION_SCAN_LIMIT: usize = 200;
const MAX_TURNS_SCANNED_PER_SESSION: usize = 120;
const DEFAULT_READ_TURNS: usize = 12;
const MAX_READ_TURNS: usize = 40;
const DEFAULT_READ_CHARS: usize = 12_000;
const MAX_READ_CHARS: usize = 24_000;
const MAX_MESSAGE_CHARS: usize = 2_400;
const MAX_CHAT_HISTORY_IMAGE_BYTES: usize = 8 * 1024 * 1024;
const CHAT_HISTORY_IMAGE_MIMES: &[&str] = &["image/png", "image/jpeg", "image/webp", "image/gif"];
const SEARCH_EXCERPT_CHARS: usize = 420;

pub fn register_chat_history_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
) -> StasisResult<()> {
    registry.register_typed_tool(CognitionChatHistorySearchTool {
        turn_scope: turn_scope.clone(),
    })?;
    registry.register_typed_tool(CognitionChatHistoryReadTool { turn_scope })?;
    Ok(())
}

#[derive(Debug, Clone)]
struct HistoryAccess {
    source_session_id: String,
    profile_id: String,
}

async fn history_access(
    turn_scope: &crate::agent_runtime::execution_context::TurnScopeAccess,
    tool_name: &str,
) -> Result<HistoryAccess, StasisError> {
    let scope = crate::agent_runtime::execution_context::turn_continuation_scope(turn_scope)
        .await
        .ok_or_else(|| {
            StasisError::PortFailure(format!("{tool_name}: active turn scope required"))
        })?;
    let source_session_id = TrimmedText::new(scope.session_id)
        .map(TrimmedText::into_string)
        .map_err(|_| StasisError::PortFailure(format!("{tool_name}: active session required")))?;
    let profile_id = scope
        .identity_user_id
        .and_then(|value| TrimmedText::new(value).ok())
        .map(TrimmedText::into_string)
        .ok_or_else(|| {
            StasisError::PortFailure(format!(
                "{tool_name}: active principal identity required for cross-session reads"
            ))
        })?;
    Ok(HistoryAccess {
        source_session_id,
        profile_id,
    })
}

fn require_visible_session(
    access: &HistoryAccess,
    target_session_id: &str,
    tool_name: &str,
) -> Result<(), StasisError> {
    if crate::session_history::session_visible_to_profile(target_session_id, &access.profile_id) {
        return Ok(());
    }
    Err(StasisError::PortFailure(format!(
        "{tool_name}: session not found or not visible to the active profile"
    )))
}

fn active_history_access() -> Result<HistoryAccess, String> {
    let context = crate::agent_runtime::execution_context::active_turn_execution_context()
        .ok_or_else(|| "chat-history image requires an active turn".to_string())?;
    let scope = context.legacy_scope();
    let source_session_id = TrimmedText::new(scope.session_id.clone())
        .map(TrimmedText::into_string)
        .map_err(|_| "chat-history image requires an active session".to_string())?;
    let profile_id = scope
        .identity_user_id
        .clone()
        .and_then(|value| TrimmedText::new(value).ok())
        .map(TrimmedText::into_string)
        .ok_or_else(|| "chat-history image requires an active profile".to_string())?;
    Ok(HistoryAccess {
        source_session_id,
        profile_id,
    })
}

fn validate_image_receipt_shape(receipt: &ChatHistoryImageReceipt) -> Result<(), String> {
    let valid_media_id =
        receipt.media_id.starts_with("usr:") || receipt.media_id.starts_with("gen:");
    let valid_digest =
        receipt.sha256.len() == 64 && receipt.sha256.bytes().all(|byte| byte.is_ascii_hexdigit());
    crate::session_storage::SessionId::parse(&receipt.session_id)
        .map_err(|_| "chat-history image session id is invalid".to_string())?;
    if !valid_media_id
        || receipt.media_id.len() > 256
        || !CHAT_HISTORY_IMAGE_MIMES.contains(&receipt.mime.as_str())
        || receipt.byte_size == 0
        || receipt.byte_size > MAX_CHAT_HISTORY_IMAGE_BYTES
        || !valid_digest
    {
        return Err("chat-history image receipt failed validation".to_string());
    }
    Ok(())
}

fn transcript_attaches_media(turns: &[ConversationTurn], media_id: &str) -> bool {
    turns.iter().any(|turn| {
        crate::media_vision::media_refs_from_turn(turn)
            .iter()
            .any(|media_ref| media_ref.media_id == media_id)
    })
}

fn validate_image_record_and_bytes(
    session_id: &str,
    media_id: &str,
    record: &crate::media_store::MediaRecord,
    bytes: Vec<u8>,
) -> Result<(ChatHistoryImageReceipt, Vec<u8>), String> {
    if record.session_id != session_id
        || record.media_id != media_id
        || !CHAT_HISTORY_IMAGE_MIMES.contains(&record.mime.as_str())
        || record.byte_size == 0
        || record.byte_size > MAX_CHAT_HISTORY_IMAGE_BYTES as u64
    {
        return Err("stored image metadata failed validation".to_string());
    }
    if bytes.is_empty()
        || bytes.len() > MAX_CHAT_HISTORY_IMAGE_BYTES
        || bytes.len() as u64 != record.byte_size
    {
        return Err("stored image size does not match its media record".to_string());
    }
    let receipt = ChatHistoryImageReceipt {
        session_id: session_id.to_string(),
        media_id: media_id.to_string(),
        mime: record.mime.clone(),
        byte_size: bytes.len(),
        sha256: format!("{:x}", Sha256::digest(&bytes)),
    };
    Ok((receipt, bytes))
}

fn receipts_match(left: &ChatHistoryImageReceipt, right: &ChatHistoryImageReceipt) -> bool {
    left.session_id == right.session_id
        && left.media_id == right.media_id
        && left.mime == right.mime
        && left.byte_size == right.byte_size
        && left.sha256.eq_ignore_ascii_case(&right.sha256)
}

fn load_chat_history_image_payload(
    source_session_id: &str,
    session_id: &str,
    media_id: &str,
    profile_id: &str,
) -> Result<(ChatHistoryImageReceipt, Vec<u8>), String> {
    if !crate::session_history::session_visible_to_profile(source_session_id, profile_id)
        || !crate::session_history::session_visible_to_profile(session_id, profile_id)
    {
        return Err("session is not visible to the active profile".to_string());
    }
    let parsed_session_id = crate::session_storage::SessionId::parse(session_id)
        .map_err(|_| "chat-history image session id is invalid".to_string())?;
    let turns = crate::session_store::get_session_store().load_history(&parsed_session_id);
    if !transcript_attaches_media(&turns, media_id) {
        return Err("image is not attached to the durable session transcript".to_string());
    }
    let record = crate::media_store::get_media_record(session_id, media_id)
        .ok_or_else(|| "image record is unavailable in this session".to_string())?;
    if record.session_id != session_id
        || record.media_id != media_id
        || !CHAT_HISTORY_IMAGE_MIMES.contains(&record.mime.as_str())
        || record.byte_size == 0
        || record.byte_size > MAX_CHAT_HISTORY_IMAGE_BYTES as u64
    {
        return Err("stored image metadata failed validation".to_string());
    }
    let bytes = crate::media_store::open_media_payload(&record)?;
    validate_image_record_and_bytes(session_id, media_id, &record, bytes)
}

fn visible_turn_text(turn: &ConversationTurn) -> Option<String> {
    if !matches!(turn.role.as_str(), "user" | "assistant" | "agent") {
        return None;
    }
    let content = turn.content.trim();
    let media_refs = crate::media_vision::media_refs_from_turn(turn);
    let attachment_text = media_refs
        .iter()
        .map(|media_ref| {
            let label = bounded_text(media_ref.label.as_deref().unwrap_or("attachment"), 120);
            let media_id = bounded_text(&media_ref.media_id, 160);
            format!("[attachment label={label} media_id={media_id}]")
        })
        .collect::<Vec<_>>()
        .join("\n");
    if !content.is_empty() {
        return Some(if attachment_text.is_empty() {
            content.to_string()
        } else {
            format!("{content}\n{attachment_text}")
        });
    }
    let parts = turn.parts.as_deref()?;
    let visible = parts
        .iter()
        .filter_map(|part| match part {
            TurnPart::Text { markdown, .. } | TurnPart::Progress { markdown } => {
                Some(markdown.trim())
            }
            TurnPart::Handoff { text, .. } => Some(text.trim()),
            TurnPart::Reasoning { .. }
            | TurnPart::ModelReceipt { .. }
            | TurnPart::ToolRun { .. }
            | TurnPart::UserMedia { .. }
            | TurnPart::UserDrawing { .. }
            | TurnPart::GeneratedMedia { .. }
            | TurnPart::HostContext { .. }
            | TurnPart::AttachmentRef { .. }
            | TurnPart::Unknown => None,
        })
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    if !visible.is_empty() {
        return Some(if attachment_text.is_empty() {
            visible
        } else {
            format!("{visible}\n{attachment_text}")
        });
    }
    if media_refs.is_empty() {
        return None;
    }
    let has_image = media_refs.iter().any(|media_ref| {
        media_ref
            .mime
            .trim()
            .to_ascii_lowercase()
            .starts_with("image/")
    });
    Some(format!(
        "{}\n{attachment_text}",
        if has_image {
            "[image attachment]"
        } else {
            "[attachment]"
        }
    ))
}

fn visible_turn_window(
    turns: &[ConversationTurn],
    before_turn: usize,
    last_k: usize,
) -> (Vec<(usize, &ConversationTurn, String)>, bool) {
    let mut visible = turns
        .iter()
        .enumerate()
        .rev()
        .filter(|(index, _)| index.saturating_add(1) < before_turn)
        .filter_map(|(index, turn)| visible_turn_text(turn).map(|content| (index, turn, content)))
        .take(last_k + 1)
        .collect::<Vec<_>>();
    let truncated = visible.len() > last_k;
    visible.truncate(last_k);
    visible.reverse();
    (visible, truncated)
}

fn matching_history_turn(
    entries: &[medousa_types::session::TranscriptEntry],
    query: &str,
) -> Option<(usize, String, String)> {
    let query = query.to_ascii_lowercase();
    entries.iter().rev().find_map(|entry| {
        let content = visible_turn_text(&entry.turn)?;
        content.to_ascii_lowercase().contains(&query).then(|| {
            (
                entry.entry_seq as usize,
                display_role(&entry.turn.role),
                search_excerpt(&content, &query),
            )
        })
    })
}

fn display_role(role: &str) -> String {
    if role == "agent" {
        "assistant".to_string()
    } else {
        role.to_string()
    }
}

fn bounded_text(text: &str, max_chars: usize) -> String {
    crate::agent_runtime::prompt_prep::truncate_text_for_budget(text, max_chars)
}

fn search_excerpt(text: &str, query: &str) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = collapsed.to_ascii_lowercase();
    let needle = query.to_ascii_lowercase();
    let start = lower
        .find(&needle)
        .map(|byte| collapsed[..byte].chars().count().saturating_sub(90))
        .unwrap_or(0);
    let excerpt = collapsed
        .chars()
        .skip(start)
        .take(SEARCH_EXCERPT_CHARS)
        .collect::<String>();
    if start > 0 {
        format!("…{excerpt}")
    } else {
        excerpt
    }
}

fn metadata_matches(summary: &SessionHistorySummary, query: &str) -> bool {
    let query = query.to_ascii_lowercase();
    summary.session_id.to_ascii_lowercase().contains(&query)
        || summary.preview.to_ascii_lowercase().contains(&query)
        || summary
            .display_name
            .as_ref()
            .is_some_and(|name| name.to_ascii_lowercase().contains(&query))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ChatHistorySearchInput {
    /// Optional text to search across visible chat prose. Omit to list recent chats.
    #[serde(default)]
    #[schemars(with = "String", skip_serializing_if = "CompatOption::is_none")]
    query: CompatOption<String>,
    /// Maximum results (default 8, maximum 20).
    #[serde(default)]
    #[schemars(
        with = "usize",
        range(min = 1, max = 20),
        skip_serializing_if = "CompatOption::is_none"
    )]
    limit: CompatOption<usize>,
    /// Maximum recent sessions searched when query is present (default 80, maximum 200).
    #[serde(default)]
    #[schemars(
        with = "usize",
        range(min = 1, max = 200),
        skip_serializing_if = "CompatOption::is_none"
    )]
    session_scan_limit: CompatOption<usize>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ChatHistorySearchMatch {
    session_id: String,
    display_name: Option<String>,
    preview: String,
    last_activity_at: Option<String>,
    turn_count: usize,
    turn_index: Option<usize>,
    role: Option<String>,
    excerpt: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ChatHistorySearchOutput {
    ok: bool,
    source_session_id: String,
    query: Option<String>,
    scanned_sessions: usize,
    results: Vec<ChatHistorySearchMatch>,
}

pub struct CognitionChatHistorySearchTool {
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
}

#[medousa_tool(id = COGNITION_CHAT_HISTORY_SEARCH_ID)]
impl CognitionChatHistorySearchTool {
    /// Search prior Medousa chats. Omit query to list recent chats. Searches user and assistant messages.
    async fn invoke_typed(
        &self,
        input: ChatHistorySearchInput,
    ) -> stasis::prelude::Result<ChatHistorySearchOutput> {
        let access = history_access(&self.turn_scope, COGNITION_CHAT_HISTORY_SEARCH).await?;
        let query = input
            .query
            .into_option()
            .and_then(|value| TrimmedText::new(value).ok())
            .map(TrimmedText::into_string);
        let limit = input
            .limit
            .into_option()
            .unwrap_or(DEFAULT_SEARCH_LIMIT)
            .clamp(1, MAX_SEARCH_LIMIT);
        let scan_limit = input
            .session_scan_limit
            .into_option()
            .unwrap_or(DEFAULT_SESSION_SCAN_LIMIT)
            .clamp(1, MAX_SESSION_SCAN_LIMIT);

        let sessions = crate::session_history::list_sessions_for_profile(
            &access.profile_id,
            if query.is_some() { scan_limit } else { limit },
        );
        let scanned_sessions = sessions.len();
        let mut results = Vec::new();
        if query.is_none() {
            for summary in sessions {
                results.push(search_match_from_summary(summary, None, None, None));
            }
        } else if let Some(query_text) = query.as_deref() {
            let mut summaries = sessions
                .into_iter()
                .map(|summary| (summary.session_id.clone(), summary))
                .collect::<std::collections::HashMap<_, _>>();
            let session_ids = summaries.keys().cloned().collect::<Vec<_>>();
            let candidate_limit = scan_limit.saturating_mul(MAX_TURNS_SCANNED_PER_SESSION);
            let transcript_hits = crate::session_store::get_session_store()
                .search_transcripts(&session_ids, query_text, candidate_limit)
                .map_err(|error| {
                    StasisError::PortFailure(format!("{COGNITION_CHAT_HISTORY_SEARCH}: {error}"))
                })?;
            for hit in transcript_hits {
                if results.len() >= limit {
                    break;
                }
                let Some(summary) = summaries.remove(&hit.session_id) else {
                    continue;
                };
                results.push(search_match_from_summary(
                    summary,
                    None,
                    Some(hit.role),
                    Some(hit.excerpt),
                ));
            }

            if results.len() < limit {
                let mut attachment_candidates = summaries.values().cloned().collect::<Vec<_>>();
                attachment_candidates
                    .sort_by_key(|summary| std::cmp::Reverse(summary.last_timestamp));
                let remaining = limit - results.len();
                let query = query_text.to_string();
                let matches = crate::media_vision::media_execution_service()
                    .run(
                        medousa_forge::execution::ExecutionClass::StoreIo,
                        medousa_forge::execution::MAX_STORE_PAYLOAD_BYTES,
                        move || {
                            let mut matches = Vec::new();
                            for summary in attachment_candidates {
                                if matches.len() >= remaining {
                                    break;
                                }
                                let Ok(target_session_id) =
                                    crate::session_storage::SessionId::parse(&summary.session_id)
                                else {
                                    continue;
                                };
                                let entries = crate::session_store::get_session_store()
                                    .load_transcript_entries_page(
                                        &target_session_id,
                                        MAX_TURNS_SCANNED_PER_SESSION,
                                        None,
                                    )
                                    .entries;
                                let Some((turn_index, role, excerpt)) =
                                    matching_history_turn(&entries, &query)
                                else {
                                    continue;
                                };
                                matches.push(search_match_from_summary(
                                    summary,
                                    Some(turn_index),
                                    Some(role),
                                    Some(excerpt),
                                ));
                            }
                            Ok(matches)
                        },
                    )
                    .await
                    .map_err(|error| {
                        StasisError::PortFailure(format!(
                            "{COGNITION_CHAT_HISTORY_SEARCH}: attachment scan failed: {error}"
                        ))
                    })?;
                let matched_sessions = matches
                    .iter()
                    .map(|item| item.session_id.as_str())
                    .collect::<std::collections::HashSet<_>>();
                for session_id in matched_sessions {
                    summaries.remove(session_id);
                }
                results.extend(matches);
            }

            if results.len() < limit {
                let mut metadata_hits = summaries
                    .into_values()
                    .filter(|summary| metadata_matches(summary, query_text))
                    .collect::<Vec<_>>();
                metadata_hits.sort_by_key(|hit| std::cmp::Reverse(hit.last_timestamp));
                for summary in metadata_hits.into_iter().take(limit - results.len()) {
                    results.push(search_match_from_summary(summary, None, None, None));
                }
            }
        }

        Ok(ChatHistorySearchOutput {
            ok: true,
            source_session_id: access.source_session_id,
            query,
            scanned_sessions,
            results,
        })
    }
}

fn search_match_from_summary(
    summary: SessionHistorySummary,
    turn_index: Option<usize>,
    role: Option<String>,
    excerpt: Option<String>,
) -> ChatHistorySearchMatch {
    ChatHistorySearchMatch {
        session_id: summary.session_id,
        display_name: summary.display_name,
        preview: bounded_text(&summary.preview, 160),
        last_activity_at: summary.last_timestamp.map(|value| value.to_rfc3339()),
        turn_count: summary.turns,
        turn_index,
        role,
        excerpt,
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ChatHistoryReadInput {
    /// Session returned by cognition_chat_history_search.
    session_id: String,
    /// Number of most recent visible turns (default 12, maximum 40).
    #[serde(default)]
    #[schemars(
        with = "usize",
        range(min = 1, max = 40),
        skip_serializing_if = "CompatOption::is_none"
    )]
    last_k: CompatOption<usize>,
    /// Total prose budget (default 12000, maximum 24000 characters).
    #[serde(default)]
    #[schemars(
        with = "usize",
        range(min = 512, max = 24000),
        skip_serializing_if = "CompatOption::is_none"
    )]
    max_chars: CompatOption<usize>,
    /// Return turns before this exclusive, one-based transcript turn index.
    /// Use the oldest returned message's turn_index to continue paging backward.
    #[serde(default)]
    #[schemars(with = "usize", skip_serializing_if = "CompatOption::is_none")]
    before_turn: CompatOption<usize>,
    /// Reopen an attached image by its returned media_id so a vision-capable model can inspect its pixels.
    #[serde(default)]
    #[schemars(with = "String", skip_serializing_if = "CompatOption::is_none")]
    media_id: CompatOption<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) struct ChatHistoryImageReceipt {
    pub(crate) session_id: String,
    pub(crate) media_id: String,
    pub(crate) mime: String,
    pub(crate) byte_size: usize,
    pub(crate) sha256: String,
}

#[derive(Debug, Serialize, JsonSchema)]
struct ChatHistoryAttachment {
    media_id: String,
    kind: String,
    mime: String,
    label: Option<String>,
    source_media_id: Option<String>,
    generation_id: Option<String>,
    parent_generation_id: Option<String>,
}

impl From<crate::daemon_api::MediaRef> for ChatHistoryAttachment {
    fn from(media_ref: crate::daemon_api::MediaRef) -> Self {
        Self {
            media_id: media_ref.media_id,
            kind: media_ref.kind,
            mime: media_ref.mime,
            label: media_ref.label,
            source_media_id: media_ref.source_media_id,
            generation_id: media_ref.generation_id,
            parent_generation_id: media_ref.parent_generation_id,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ChatHistoryMessage {
    turn_index: usize,
    role: String,
    timestamp: String,
    content: String,
    tool_names: Vec<String>,
    attachments: Vec<ChatHistoryAttachment>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ChatHistoryReadOutput {
    ok: bool,
    source_session_id: String,
    session_id: String,
    display_name: Option<String>,
    total_turns: usize,
    returned_turns: usize,
    truncated: bool,
    messages: Vec<ChatHistoryMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image: Option<ChatHistoryImageReceipt>,
}

/// Reopens a transcript image only after checking the active profile, durable
/// attachment membership, the media record, and the payload digest.
#[derive(Default)]
pub struct ChatHistoryMediaHydrationPort;

impl medousa_runtime::ToolObservationHydrationPort for ChatHistoryMediaHydrationPort {
    fn accepts(&self, tool_name: &str) -> bool {
        tool_name == COGNITION_CHAT_HISTORY_READ
    }

    fn hydrate(
        &self,
        request: medousa_runtime::ToolObservationHydrationRequest,
    ) -> medousa_runtime::RuntimePortFuture<
        Result<Option<medousa_runtime::HydratedToolObservation>, String>,
    > {
        Box::pin(async move {
            if request.tool_name != COGNITION_CHAT_HISTORY_READ {
                return Ok(None);
            }
            let Some(image_value) = request.tool_output.get("image") else {
                return Ok(None);
            };
            if image_value.is_null() {
                return Ok(None);
            }
            let receipt: ChatHistoryImageReceipt = serde_json::from_value(image_value.clone())
                .map_err(|_| "chat-history image receipt is malformed".to_string())?;
            let output_session_id = request
                .tool_output
                .get("session_id")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| "chat-history image source session is missing".to_string())?;
            if output_session_id != receipt.session_id {
                return Err("chat-history image does not match the returned session".to_string());
            }
            validate_image_receipt_shape(&receipt)?;

            let access = active_history_access()?;
            let output_source_session_id = request
                .tool_output
                .get("source_session_id")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| "chat-history image source scope is missing".to_string())?;
            if output_source_session_id != access.source_session_id {
                return Err("chat-history image does not match the active source scope".to_string());
            }
            let source_session_id = access.source_session_id;
            let session_id = receipt.session_id.clone();
            let media_id = receipt.media_id.clone();
            let profile_id = access.profile_id;
            let expected = receipt.clone();
            let execution = crate::media_vision::media_execution_service();
            let observation = execution
                .run(
                    medousa_forge::execution::ExecutionClass::Observation,
                    receipt.byte_size.max(1),
                    move || {
                        let (verified, bytes) = load_chat_history_image_payload(
                            &source_session_id,
                            &session_id,
                            &media_id,
                            &profile_id,
                        )
                        .map_err(medousa_forge::error::ForgeError::Store)?;
                        if !receipts_match(&verified, &expected) {
                            return Err(medousa_forge::error::ForgeError::Store(
                                "chat-history image does not match its durable receipt".to_string(),
                            ));
                        }
                        let sha256 = format!("{:x}", Sha256::digest(&bytes));
                        Ok(medousa_runtime::HydratedToolObservation {
                            tool_name: request.tool_name,
                            source_call_id: request.source_call_id,
                            artifact_id: verified.media_id,
                            content_type: verified.mime,
                            bytes,
                            sha256,
                            untrusted_content: true,
                        })
                    },
                )
                .await
                .map_err(|error| format!("chat-history image lookup failed: {error}"))?;
            Ok(Some(observation))
        })
    }
}

pub struct CognitionChatHistoryReadTool {
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
}

#[medousa_tool(id = COGNITION_CHAT_HISTORY_READ_ID)]
impl CognitionChatHistoryReadTool {
    /// Read messages and attachment IDs from a Medousa chat. Use before_turn to page backward, or media_id to reopen an attached image for visual inspection.
    async fn invoke_typed(
        &self,
        input: ChatHistoryReadInput,
    ) -> stasis::prelude::Result<ChatHistoryReadOutput> {
        let access = history_access(&self.turn_scope, COGNITION_CHAT_HISTORY_READ).await?;
        let session_id = TrimmedText::new(input.session_id)
            .map(TrimmedText::into_string)
            .map_err(|_| {
                StasisError::PortFailure(
                    "cognition_chat_history_read: session_id is required".to_string(),
                )
            })?;
        require_visible_session(&access, &session_id, COGNITION_CHAT_HISTORY_READ)?;
        let last_k = input
            .last_k
            .into_option()
            .unwrap_or(DEFAULT_READ_TURNS)
            .clamp(1, MAX_READ_TURNS);
        let max_chars = input
            .max_chars
            .into_option()
            .unwrap_or(DEFAULT_READ_CHARS)
            .clamp(512, MAX_READ_CHARS);
        let before_turn = input.before_turn.into_option().unwrap_or(usize::MAX).max(1);

        let turns = load_history(&session_id);
        let total_turns = turns.len();
        let (visible, mut truncated) = visible_turn_window(&turns, before_turn, last_k);

        let mut remaining = max_chars;
        let mut messages = Vec::new();
        for (index, turn, content) in visible {
            if remaining == 0 {
                truncated = true;
                break;
            }
            let per_message = remaining.min(MAX_MESSAGE_CHARS);
            let bounded = bounded_text(&content, per_message);
            truncated |= bounded.chars().count() < content.chars().count();
            remaining = remaining.saturating_sub(bounded.chars().count());
            messages.push(ChatHistoryMessage {
                turn_index: index + 1,
                role: display_role(&turn.role),
                timestamp: turn.timestamp.to_rfc3339(),
                content: bounded,
                tool_names: turn.tool_names.iter().take(16).cloned().collect(),
                attachments: crate::media_vision::media_refs_from_turn(turn)
                    .into_iter()
                    .take(crate::media_vision::MAX_MEDIA_REFS_PER_TURN)
                    .map(ChatHistoryAttachment::from)
                    .collect(),
            });
        }

        let image = if let Some(media_id) = input.media_id.into_option() {
            let media_id = TrimmedText::new(media_id)
                .map(TrimmedText::into_string)
                .map_err(|_| {
                    StasisError::PortFailure(
                        "cognition_chat_history_read: media_id must not be empty".to_string(),
                    )
                })?;
            let image_session_id = session_id.clone();
            let image_media_id = media_id.clone();
            let source_session_id = access.source_session_id.clone();
            let profile_id = access.profile_id.clone();
            Some(
                crate::media_vision::media_execution_service()
                    .run(
                        medousa_forge::execution::ExecutionClass::Observation,
                        MAX_CHAT_HISTORY_IMAGE_BYTES,
                        move || {
                            load_chat_history_image_payload(
                                &source_session_id,
                                &image_session_id,
                                &image_media_id,
                                &profile_id,
                            )
                            .map(|(receipt, _bytes)| receipt)
                            .map_err(medousa_forge::error::ForgeError::Store)
                        },
                    )
                    .await
                    .map_err(|error| {
                        StasisError::PortFailure(format!(
                            "{COGNITION_CHAT_HISTORY_READ}: image lookup failed: {error}"
                        ))
                    })?,
            )
        } else {
            None
        };

        let display_name = crate::session_history::display_name(&session_id);
        let returned_turns = messages.len();
        Ok(ChatHistoryReadOutput {
            ok: true,
            source_session_id: access.source_session_id,
            session_id,
            display_name,
            total_turns,
            returned_turns,
            truncated,
            messages,
            image,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn turn(role: &str, content: &str, parts: Option<Vec<TurnPart>>) -> ConversationTurn {
        ConversationTurn {
            role: role.to_string(),
            content: content.to_string(),
            timestamp: Utc::now(),
            tool_names: Vec::new(),
            answer_state: None,
            parts,
            slice_summary: None,
            speaker_profile_id: None,
        }
    }

    fn image_turn(media_id: &str, label: &str) -> ConversationTurn {
        turn(
            "user",
            "",
            Some(vec![TurnPart::UserMedia {
                media_id: media_id.to_string(),
                mime: "image/png".to_string(),
                label: Some(label.to_string()),
                byte_size: Some(4),
            }]),
        )
    }

    #[test]
    fn visible_text_never_returns_reasoning_or_tool_receipts() {
        let only_private = turn(
            "assistant",
            "",
            Some(vec![
                TurnPart::Reasoning {
                    markdown: "private chain".to_string(),
                },
                TurnPart::ToolRun {
                    run_id: "run-1".to_string(),
                    tool_name: "secret_tool".to_string(),
                    status: "succeeded".to_string(),
                    input_summary: "private input".to_string(),
                    input_params: Vec::new(),
                    output_summary: Some("private output".to_string()),
                    artifact_refs: Vec::new(),
                    tool_round: Some(1),
                    started_at: Utc::now(),
                    finished_at: Some(Utc::now()),
                },
            ]),
        );
        assert_eq!(visible_turn_text(&only_private), None);

        let mixed = turn(
            "assistant",
            "",
            Some(vec![
                TurnPart::Reasoning {
                    markdown: "private chain".to_string(),
                },
                TurnPart::Text {
                    markdown: "visible answer".to_string(),
                    segment_id: None,
                    model_round: None,
                },
            ]),
        );
        assert_eq!(visible_turn_text(&mixed).as_deref(), Some("visible answer"));
    }

    #[test]
    fn excerpt_centers_the_matching_text_and_stays_bounded() {
        let text = format!(
            "{} pager sentinel {}",
            "before ".repeat(80),
            "after ".repeat(80)
        );
        let excerpt = search_excerpt(&text, "pager sentinel");
        assert!(excerpt.contains("pager sentinel"));
        assert!(excerpt.chars().count() <= SEARCH_EXCERPT_CHARS + 1);
    }

    #[test]
    fn image_only_history_turn_is_visible_with_searchable_attachment_metadata() {
        let image = image_turn("usr:session-a:photo-1", "pager screenshot");
        let visible = visible_turn_text(&image).expect("image-only turn should be visible");
        assert!(visible.contains("[image attachment]"));
        assert!(visible.contains("pager screenshot"));
        assert!(visible.contains("usr:session-a:photo-1"));
        assert!(transcript_attaches_media(
            std::slice::from_ref(&image),
            "usr:session-a:photo-1"
        ));
        assert!(!transcript_attaches_media(
            &[turn("user", "text only", None)],
            "usr:session-a:photo-1"
        ));
    }

    #[test]
    fn attachment_labels_and_ids_match_history_search_queries() {
        let image = image_turn("usr:session-a:photo-1", "pager screenshot");
        let entries = [medousa_types::session::TranscriptEntry {
            entry_id: medousa_types::TranscriptEntryId::parse(
                "ent_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            )
            .expect("entry id"),
            entry_seq: 7,
            caused_by: None,
            source: None,
            content_digest: "sha256:test".to_string(),
            turn: image,
        }];
        assert_eq!(
            matching_history_turn(&entries, "pager screenshot")
                .map(|(index, role, _)| (index, role)),
            Some((7, "user".to_string()))
        );
        assert!(matching_history_turn(&entries, "usr:session-a:photo-1").is_some());
    }

    #[test]
    fn history_reader_pages_older_than_the_returned_window() {
        let turns = vec![
            turn("user", "first", None),
            image_turn("usr:session-a:photo-1", "middle image"),
            turn("assistant", "last", None),
        ];
        let (recent, truncated) = visible_turn_window(&turns, usize::MAX, 2);
        assert!(truncated);
        assert_eq!(recent[0].0 + 1, 2);
        assert_eq!(recent[1].0 + 1, 3);
        let (older, has_more) = visible_turn_window(&turns, recent[0].0 + 1, 2);
        assert!(!has_more);
        assert_eq!(older.len(), 1);
        assert_eq!(older[0].0 + 1, 1);
    }

    #[test]
    fn history_image_receipt_rejects_foreign_records_oversize_payload_and_tampering() {
        let bytes = b"png!".to_vec();
        let record = crate::media_store::MediaRecord {
            media_id: "usr:session-a:photo-1".to_string(),
            session_id: "session-a".to_string(),
            mime: "image/png".to_string(),
            kind: "image".to_string(),
            byte_size: bytes.len() as u64,
            stored_at_utc: Utc::now(),
            payload_path: "ignored-by-test".to_string(),
            label: Some("photo".to_string()),
            extract_path: None,
            extract_chars: None,
            extract_truncated: false,
        };
        let (receipt, observed) = validate_image_record_and_bytes(
            "session-a",
            "usr:session-a:photo-1",
            &record,
            bytes.clone(),
        )
        .expect("valid image receipt");
        assert_eq!(observed, bytes);
        validate_image_receipt_shape(&receipt).expect("valid receipt shape");

        let mut foreign_record = record.clone();
        foreign_record.session_id = "session-b".to_string();
        assert!(
            validate_image_record_and_bytes(
                "session-a",
                &foreign_record.media_id,
                &foreign_record,
                bytes.clone(),
            )
            .is_err()
        );
        assert!(
            validate_image_record_and_bytes(
                "session-a",
                &record.media_id,
                &record,
                b"png!!".to_vec(),
            )
            .is_err()
        );
        let mut tampered = receipt.clone();
        tampered.sha256 = "0".repeat(64);
        assert!(!receipts_match(&receipt, &tampered));
    }

    #[tokio::test]
    async fn cross_session_access_requires_a_turn_principal() {
        let scope = crate::agent_runtime::execution_context::TurnScopeAccess::for_test(
            crate::turn_continuation::TurnContinuationScope {
                turn_correlation_id: "turn-1".to_string(),
                session_id: "session-a".to_string(),
                identity_user_id: None,
                original_prompt: "find the pager chat".to_string(),
                delivery_target: None,
                provider: "test".to_string(),
                model: "test".to_string(),
                response_depth_mode: "standard".to_string(),
                supports_ui_artifacts: false,
                supports_liquid_markdown: false,
                supports_browser_host: false,
                browser_driver_id: None,
                selected_worlds: Vec::new(),
                channel_surface: None,
            },
        );
        let error = history_access(&scope, COGNITION_CHAT_HISTORY_SEARCH)
            .await
            .expect_err("missing principal must be denied");
        assert!(error.to_string().contains("principal identity required"));
    }
}
