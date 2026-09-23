//! Vision routing for current-turn media and bounded replay of recent images.

use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use genai::chat::{ChatMessage, ContentPart, MessageContent};
use medousa_types::{ConversationTurn, TurnPart};

use crate::daemon_api::MediaRef;
use crate::media_store::{self, MediaPromptMergeOptions};

pub const MAX_MEDIA_REFS_PER_TURN: usize = 5;
pub const MAX_VISION_IMAGES_PER_TURN: usize = 5;
const MAX_VISION_IMAGE_BYTES: u64 = 10 * 1024 * 1024;
const MAX_RECENT_HISTORY_TURNS: usize = 20;
const MAX_RECENT_HISTORY_IMAGE_BYTES: u64 = 20 * 1024 * 1024;
const MAX_RECENT_HISTORY_IMAGE_BYTES_PER_READ: u64 = 8 * 1024 * 1024;
const MAX_RECENT_HISTORY_SCAN_IMAGES: usize = MAX_RECENT_HISTORY_TURNS * MAX_MEDIA_REFS_PER_TURN;
const RECENT_HISTORY_CAPTION_CHARS: usize = 160;
const RECENT_HISTORY_MARKER: &str = "[MEDOUSA_RECENT_HISTORY_IMAGES]";

type HistoryImageLoad =
    Pin<Box<dyn Future<Output = Option<(media_store::MediaRecord, Vec<u8>)>> + Send>>;

static MEDIA_EXECUTION: OnceLock<Arc<medousa_forge::execution::ForgeExecutionService>> =
    OnceLock::new();

pub fn media_execution_service() -> Arc<medousa_forge::execution::ForgeExecutionService> {
    Arc::clone(
        MEDIA_EXECUTION
            .get_or_init(|| Arc::new(medousa_forge::execution::ForgeExecutionService::new())),
    )
}

#[derive(Debug, Clone)]
pub struct TurnMediaVisionPlan {
    pub supports_vision: bool,
    pub vision_image_count: usize,
    image_parts: Vec<ContentPart>,
    pub merge_options: MediaPromptMergeOptions,
}

impl TurnMediaVisionPlan {
    pub fn empty() -> Self {
        Self {
            supports_vision: false,
            vision_image_count: 0,
            image_parts: Vec::new(),
            merge_options: MediaPromptMergeOptions {
                vision_active: false,
                vision_image_ids: HashSet::new(),
            },
        }
    }

    pub fn build_user_message(&self, prompt_text: &str) -> ChatMessage {
        if self.supports_vision && !self.image_parts.is_empty() {
            let mut parts = vec![ContentPart::from_text(prompt_text.to_string())];
            parts.extend(self.image_parts.clone());
            ChatMessage::user(MessageContent::from_parts(parts))
        } else {
            ChatMessage::user(prompt_text.to_string())
        }
    }

    pub fn stream_notice(&self, provider: &str, model: &str) -> Option<String> {
        if self.vision_image_count == 0 {
            return None;
        }
        if self.supports_vision {
            Some(format!(
                "◈ vision active images={} target={provider}:{model}",
                self.vision_image_count
            ))
        } else {
            Some(format!(
                "◈ vision unavailable images={} target={provider}:{model} (text fallback)",
                self.vision_image_count
            ))
        }
    }
}

pub fn supports_vision(provider: &str, model: &str) -> bool {
    crate::model_capability_registry::registry().supports_vision(provider, model)
}

pub fn plan_turn_media(
    session_id: &str,
    media_refs: &[MediaRef],
    provider: &str,
    model: &str,
) -> Result<TurnMediaVisionPlan, String> {
    if media_refs.len() > MAX_MEDIA_REFS_PER_TURN {
        return Err(format!(
            "too many attachments (max {MAX_MEDIA_REFS_PER_TURN})"
        ));
    }

    if media_refs.is_empty() {
        return Ok(TurnMediaVisionPlan::empty());
    }

    let vision_capable = supports_vision(provider, model);
    let mut image_parts = Vec::new();
    let mut vision_image_ids = HashSet::new();

    for media_ref in media_refs
        .iter()
        .filter(|media_ref| is_image_media_ref(media_ref))
    {
        if image_parts.len() >= MAX_VISION_IMAGES_PER_TURN {
            break;
        }
        if !vision_capable {
            continue;
        }
        let Some(record) = media_store::get_media_record(session_id, &media_ref.media_id) else {
            continue;
        };
        if record.byte_size > MAX_VISION_IMAGE_BYTES {
            continue;
        }
        let bytes = media_store::open_media_payload(&record).map_err(|err| err.to_string())?;
        let encoded = Arc::<str>::from(STANDARD.encode(bytes));
        let label = media_ref
            .label
            .clone()
            .or(record.label.clone())
            .filter(|value| !value.trim().is_empty());
        image_parts.push(ContentPart::from_binary_base64(record.mime, encoded, label));
        vision_image_ids.insert(media_ref.media_id.clone());
    }

    Ok(TurnMediaVisionPlan {
        supports_vision: vision_capable,
        vision_image_count: media_refs
            .iter()
            .filter(|media_ref| is_image_media_ref(media_ref))
            .count(),
        image_parts,
        merge_options: MediaPromptMergeOptions {
            vision_active: vision_capable && !vision_image_ids.is_empty(),
            vision_image_ids,
        },
    })
}

/// Return media references recorded in a transcript turn. The transcript is
/// metadata only; callers must resolve each reference against the session
/// media store before exposing any bytes to a model.
pub fn media_refs_from_turn(turn: &ConversationTurn) -> Vec<MediaRef> {
    let mut refs = Vec::new();
    for part in turn.parts.as_deref().unwrap_or_default() {
        let media_ref = match part {
            TurnPart::UserMedia {
                media_id,
                mime,
                label,
                ..
            } => Some(MediaRef {
                media_id: media_id.clone(),
                kind: media_store::media_kind_from_mime(mime).to_string(),
                mime: mime.clone(),
                label: label.clone(),
                source_media_id: None,
                generation_id: None,
                parent_generation_id: None,
            }),
            TurnPart::UserDrawing {
                preview_media_id,
                label,
                ..
            } => Some(MediaRef {
                media_id: preview_media_id.clone(),
                kind: "image".to_string(),
                mime: "image/png".to_string(),
                label: label.clone(),
                source_media_id: None,
                generation_id: None,
                parent_generation_id: None,
            }),
            TurnPart::GeneratedMedia {
                media_id,
                mime,
                label,
                generation_id,
                parent_generation_id,
                ..
            } => Some(MediaRef {
                media_id: media_id.clone(),
                kind: media_store::media_kind_from_mime(mime).to_string(),
                mime: mime.clone(),
                label: Some(label.clone()),
                source_media_id: None,
                generation_id: Some(generation_id.clone()),
                parent_generation_id: parent_generation_id.clone(),
            }),
            _ => None,
        };
        if let Some(media_ref) = media_ref {
            refs.push(media_ref);
        }
    }
    refs
}

/// Append one user message carrying a bounded set of recent transcript images.
/// Images are looked up by session-scoped durable IDs and the media store's
/// MIME/size metadata is authoritative. Missing or deleted media is skipped.
pub async fn append_recent_history_images(
    prior_messages: &mut Vec<ChatMessage>,
    session_id: &str,
    history: &[ConversationTurn],
    current_media_refs: &[MediaRef],
    provider: &str,
    model: &str,
) {
    if !supports_vision(provider, model) {
        return;
    }

    let service = media_execution_service();
    append_recent_history_images_with_loader(
        prior_messages,
        session_id,
        history,
        current_media_refs,
        move |read_session_id, read_media_id| {
            let service = Arc::clone(&service);
            Box::pin(async move {
                let result = service
                    .run(
                        medousa_forge::execution::ExecutionClass::Observation,
                        MAX_RECENT_HISTORY_IMAGE_BYTES_PER_READ as usize,
                        move || {
                            Ok((|| -> anyhow::Result<_> {
                                let Some(record) =
                                    media_store::get_media_record(&read_session_id, &read_media_id)
                                else {
                                    return Ok(None);
                                };
                                if record.session_id != read_session_id
                                    || record.media_id != read_media_id
                                    || !supported_replay_image_mime(&record.mime)
                                    || record.byte_size == 0
                                    || record.byte_size > MAX_RECENT_HISTORY_IMAGE_BYTES_PER_READ
                                {
                                    return Ok(None);
                                }
                                let bytes = media_store::open_media_payload(&record)
                                    .map_err(anyhow::Error::msg)?;
                                Ok(Some((record, bytes)))
                            })())
                        },
                    )
                    .await;
                match result {
                    Ok(Ok(Some(value))) => Some(value),
                    _ => None,
                }
            }) as HistoryImageLoad
        },
    )
    .await;
}

async fn append_recent_history_images_with_loader<F>(
    prior_messages: &mut Vec<ChatMessage>,
    session_id: &str,
    history: &[ConversationTurn],
    current_media_refs: &[MediaRef],
    mut load: F,
) where
    F: FnMut(String, String) -> HistoryImageLoad + Send,
{
    let current_image_count = current_media_refs
        .iter()
        .filter(|media_ref| is_image_media_ref(media_ref))
        .count();
    let current_ids: HashSet<String> = current_media_refs
        .iter()
        .filter(|media_ref| is_image_media_ref(media_ref))
        .map(|media_ref| media_ref.media_id.clone())
        .collect();
    let image_slots = MAX_VISION_IMAGES_PER_TURN
        .saturating_sub(current_image_count.min(MAX_VISION_IMAGES_PER_TURN));
    if image_slots == 0 {
        return;
    }

    let mut seen = current_ids;
    let mut candidates = Vec::new();
    'turns: for turn in history.iter().rev().take(MAX_RECENT_HISTORY_TURNS) {
        let caption = short_caption(&turn.content);
        for media_ref in media_refs_from_turn(turn)
            .into_iter()
            .rev()
            .filter(is_image_media_ref)
        {
            if candidates.len() >= MAX_RECENT_HISTORY_SCAN_IMAGES {
                break 'turns;
            }
            if !seen.insert(media_ref.media_id.clone()) {
                continue;
            }
            candidates.push((
                media_ref.media_id,
                media_ref.label,
                turn.timestamp.to_rfc3339(),
                caption.clone(),
            ));
        }
    }

    let mut used_bytes = 0u64;
    let mut entries = Vec::new();
    for (media_id, reference_label, attached_at, caption) in candidates {
        if entries.len() >= image_slots {
            break;
        }
        let Some((record, bytes)) = load(session_id.to_string(), media_id.clone()).await else {
            continue;
        };
        if record.session_id != session_id
            || record.media_id != media_id
            || !supported_replay_image_mime(&record.mime)
            || record.byte_size == 0
            || record.byte_size > MAX_RECENT_HISTORY_IMAGE_BYTES_PER_READ
        {
            continue;
        }
        let byte_count = bytes.len() as u64;
        if byte_count == 0
            || byte_count > MAX_RECENT_HISTORY_IMAGE_BYTES_PER_READ
            || byte_count != record.byte_size
            || used_bytes.saturating_add(byte_count) > MAX_RECENT_HISTORY_IMAGE_BYTES
        {
            continue;
        }
        used_bytes = used_bytes.saturating_add(byte_count);
        let label = reference_label
            .or(record.label.clone())
            .filter(|value| !value.trim().is_empty())
            .map(|value| value.chars().take(160).collect::<String>());
        let caption_text = if caption.is_empty() {
            String::new()
        } else {
            format!("\ncaption={caption}")
        };
        let metadata = format!(
            "media_id={} attached_at={} label={}{}",
            record.media_id,
            attached_at,
            label.as_deref().unwrap_or("(none)"),
            caption_text,
        );
        let part = ContentPart::from_binary_base64(
            record.mime,
            Arc::<str>::from(STANDARD.encode(bytes)),
            label,
        );
        entries.push((metadata, part));
    }
    append_replayed_images(prior_messages, entries);
}

fn append_replayed_images(
    prior_messages: &mut Vec<ChatMessage>,
    entries: Vec<(String, ContentPart)>,
) {
    if entries.is_empty() {
        return;
    }
    let mut parts = vec![ContentPart::from_text(format!(
        "{RECENT_HISTORY_MARKER}\nRecent images from this conversation (newest first). These are prior attachments, not current-turn uploads:\n{}",
        entries
            .iter()
            .map(|(metadata, _)| metadata.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    ))];
    parts.extend(entries.into_iter().map(|(_, part)| part));
    prior_messages.push(ChatMessage::user(MessageContent::from_parts(parts)));
}

/// Drop only the synthetic recent-image replay message for a target without
/// vision support. Ordinary transcript messages remain untouched.
pub fn prior_messages_for_target(
    prior_messages: Vec<ChatMessage>,
    provider: &str,
    model: &str,
) -> Vec<ChatMessage> {
    if supports_vision(provider, model) {
        return prior_messages;
    }
    prior_messages
        .into_iter()
        .filter(|message| {
            !(message
                .content
                .parts()
                .first()
                .and_then(ContentPart::as_text)
                .is_some_and(|text| text.starts_with(RECENT_HISTORY_MARKER))
                && message
                    .content
                    .parts()
                    .iter()
                    .any(|part| matches!(part, ContentPart::Binary(_))))
        })
        .collect()
}

fn short_caption(content: &str) -> String {
    let normalized = content.split_whitespace().collect::<Vec<_>>().join(" ");
    normalized
        .chars()
        .take(RECENT_HISTORY_CAPTION_CHARS)
        .collect()
}

fn supported_replay_image_mime(mime: &str) -> bool {
    matches!(
        mime.trim().to_ascii_lowercase().as_str(),
        "image/png" | "image/jpeg" | "image/webp" | "image/gif"
    )
}

pub fn is_image_media_ref(media_ref: &MediaRef) -> bool {
    if media_ref.kind == "image" {
        return true;
    }
    media_ref
        .mime
        .trim()
        .to_ascii_lowercase()
        .starts_with("image/")
}

pub fn has_vision_media(media_refs: &[MediaRef]) -> bool {
    media_refs.iter().any(is_image_media_ref)
}

pub fn has_document_media(media_refs: &[MediaRef]) -> bool {
    media_refs.iter().any(|media_ref| {
        if is_image_media_ref(media_ref) {
            return false;
        }
        is_extractable_document_mime(&media_ref.mime)
    })
}

fn is_extractable_document_mime(mime: &str) -> bool {
    matches!(
        mime.trim().to_ascii_lowercase().as_str(),
        "text/plain"
            | "text/markdown"
            | "text/csv"
            | "text/tab-separated-values"
            | "application/pdf"
            | "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
            | "application/vnd.ms-excel"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use medousa_types::{ConversationTurn, TurnPart};

    #[test]
    fn text_only_fallback_drops_replay_pixels_but_keeps_real_history() {
        let real_text = format!("{RECENT_HISTORY_MARKER} explain this marker");
        let mut messages = vec![ChatMessage::user(real_text.clone())];
        append_replayed_images(
            &mut messages,
            vec![(
                "media_id=usr:session:photo".into(),
                ContentPart::from_binary_base64("image/png", "cGl4ZWxz", None),
            )],
        );
        assert_eq!(
            prior_messages_for_target(messages.clone(), "openai", "gpt-4o-mini").len(),
            2
        );
        let messages = prior_messages_for_target(messages, "openai", "gpt-3.5-turbo");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content.first_text(), Some(real_text.as_str()));
    }

    #[test]
    fn openai_vision_models_detected() {
        assert!(supports_vision("openai", "gpt-4o-mini"));
        assert!(supports_vision("openai", "gpt-4.1-mini"));
        assert!(supports_vision("openai-codex", "gpt-5.6-sol"));
        assert!(!supports_vision("openai", "gpt-3.5-turbo"));
    }

    #[test]
    fn anthropic_vision_models_detected() {
        assert!(supports_vision("anthropic", "claude-3-5-sonnet-20241022"));
        assert!(!supports_vision("anthropic", "claude-2.1"));
    }

    #[test]
    fn openrouter_gpt4o_mini_detected() {
        assert!(supports_vision("openrouter", "openai/gpt-4o-mini"));
    }

    #[test]
    fn empty_plan_builds_text_user_message() {
        let plan = TurnMediaVisionPlan::empty();
        let message = plan.build_user_message("hello");
        assert_eq!(message.content.first_text(), Some("hello"));
    }

    #[test]
    fn document_only_refs_are_not_vision_media() {
        let refs = vec![MediaRef {
            media_id: "usr:s1:abc".to_string(),
            kind: "document".to_string(),
            mime: "application/pdf".to_string(),
            label: Some("report.pdf".to_string()),
            source_media_id: None,
            generation_id: None,
            parent_generation_id: None,
        }];
        assert!(!has_vision_media(&refs));
        assert!(has_document_media(&refs));
    }

    #[test]
    fn mixed_image_and_pdf_requires_vision_for_image_only() {
        let refs = vec![
            MediaRef {
                media_id: "usr:s1:img".to_string(),
                kind: "image".to_string(),
                mime: "image/png".to_string(),
                label: None,
                source_media_id: None,
                generation_id: None,
                parent_generation_id: None,
            },
            MediaRef {
                media_id: "usr:s1:pdf".to_string(),
                kind: "document".to_string(),
                mime: "application/pdf".to_string(),
                label: Some("notes.pdf".to_string()),
                source_media_id: None,
                generation_id: None,
                parent_generation_id: None,
            },
        ];
        assert!(has_vision_media(&refs));
        assert!(has_document_media(&refs));
        let plan = plan_turn_media("session-1", &refs, "openai", "gpt-3.5-turbo").expect("plan");
        assert_eq!(plan.vision_image_count, 1);
        assert!(!plan.supports_vision);
    }

    #[test]
    fn document_only_plan_has_no_vision_images() {
        let refs = vec![MediaRef {
            media_id: "usr:s1:csv".to_string(),
            kind: "spreadsheet".to_string(),
            mime: "text/csv".to_string(),
            label: None,
            source_media_id: None,
            generation_id: None,
            parent_generation_id: None,
        }];
        let plan = plan_turn_media("session-1", &refs, "openai", "gpt-4o-mini").expect("plan");
        assert_eq!(plan.vision_image_count, 0);
        assert!(!plan.merge_options.vision_active);
    }

    fn history_turn(
        media_id: &str,
        mime: &str,
        label: Option<&str>,
        content: &str,
    ) -> ConversationTurn {
        ConversationTurn {
            role: "user".to_string(),
            content: content.to_string(),
            timestamp: Utc.with_ymd_and_hms(2026, 9, 23, 10, 0, 0).unwrap(),
            tool_names: Vec::new(),
            answer_state: None,
            parts: Some(vec![TurnPart::UserMedia {
                media_id: media_id.to_string(),
                mime: mime.to_string(),
                label: label.map(str::to_string),
                byte_size: Some(3),
            }]),
            slice_summary: None,
            speaker_profile_id: None,
        }
    }

    fn stored_image(media_id: &str, session_id: &str, byte_size: u64) -> media_store::MediaRecord {
        media_store::MediaRecord {
            media_id: media_id.to_string(),
            session_id: session_id.to_string(),
            mime: "image/png".to_string(),
            kind: "image".to_string(),
            byte_size,
            stored_at_utc: Utc::now(),
            payload_path: "unused.png".to_string(),
            label: Some("stored.png".to_string()),
            extract_path: None,
            extract_chars: None,
            extract_truncated: false,
        }
    }

    #[test]
    fn media_refs_from_turn_uses_drawing_preview_and_generated_media_ids() {
        let mut turn = history_turn("user-image", "image/png", None, "caption");
        turn.parts = Some(vec![
            TurnPart::UserDrawing {
                media_id: "drawing-source".into(),
                preview_media_id: "drawing-preview".into(),
                mime: "image/webp".into(),
                label: Some("sketch".into()),
                byte_size: None,
            },
            TurnPart::GeneratedMedia {
                media_id: "generated-image".into(),
                mime: "image/png".into(),
                label: "render".into(),
                generation_id: "gen-1".into(),
                parent_generation_id: None,
                width_px: None,
                height_px: None,
                byte_size: None,
                provider: None,
                model: None,
            },
        ]);
        let refs = media_refs_from_turn(&turn);
        assert_eq!(
            refs.iter()
                .map(|item| item.media_id.as_str())
                .collect::<Vec<_>>(),
            ["drawing-preview", "generated-image"]
        );
        assert_eq!(refs[1].generation_id.as_deref(), Some("gen-1"));
    }

    #[tokio::test]
    async fn recent_history_replay_adds_session_scoped_image_pixels_and_skips_missing_foreign_and_nonvision()
     {
        let history = vec![
            history_turn("old", "image/png", Some("older"), "old caption"),
            history_turn("foreign", "image/png", None, "foreign caption"),
            history_turn("deleted", "image/png", None, "deleted caption"),
            history_turn("current", "image/png", None, "current caption"),
            history_turn(
                "newest",
                "image/png",
                Some("latest"),
                "please compare these",
            ),
        ];
        let current = vec![MediaRef {
            media_id: "current".into(),
            kind: "image".into(),
            mime: "image/png".into(),
            label: None,
            source_media_id: None,
            generation_id: None,
            parent_generation_id: None,
        }];
        let mut messages = Vec::new();
        append_recent_history_images_with_loader(
            &mut messages,
            "session-a",
            &history,
            &current,
            |session_id, media_id| {
                Box::pin(async move {
                    let record = match media_id.as_str() {
                        "foreign" => stored_image(&media_id, "session-b", 3),
                        "deleted" => return None,
                        _ => stored_image(&media_id, &session_id, 3),
                    };
                    Some((record, vec![1u8, 2, 3]))
                }) as HistoryImageLoad
            },
        )
        .await;
        assert_eq!(messages.len(), 1);
        let parts = messages[0].content.parts();
        assert_eq!(
            parts
                .iter()
                .filter(|part| matches!(part, ContentPart::Binary(_)))
                .count(),
            2
        );
        let metadata = parts.iter().find_map(ContentPart::as_text).unwrap();
        assert!(metadata.contains("media_id=newest "));
        assert!(metadata.contains("media_id=old "));
        assert!(!metadata.contains("media_id=foreign "));
        assert!(!metadata.contains("media_id=deleted "));
        assert!(!metadata.contains("media_id=current "));
        assert!(
            parts
                .iter()
                .any(|part| matches!(part, ContentPart::Binary(_)))
        );

        let mut nonvision = Vec::new();
        append_recent_history_images(
            &mut nonvision,
            "session-a",
            &history,
            &[],
            "openai",
            "gpt-3.5-turbo",
        )
        .await;
        assert!(nonvision.is_empty());
    }

    #[tokio::test]
    async fn recent_history_replay_caps_images_to_five_and_twenty_megabytes() {
        let history = (0..8)
            .map(|index| history_turn(&format!("image-{index}"), "image/png", None, "caption"))
            .collect::<Vec<_>>();
        let mut messages = Vec::new();
        append_recent_history_images_with_loader(
            &mut messages,
            "session-a",
            &history,
            &[],
            |session_id, media_id| {
                Box::pin(
                    async move { Some((stored_image(&media_id, &session_id, 3), vec![1u8, 2, 3])) },
                ) as HistoryImageLoad
            },
        )
        .await;
        assert_eq!(
            messages[0]
                .content
                .parts()
                .iter()
                .filter(|part| matches!(part, ContentPart::Binary(_)))
                .count(),
            5
        );

        let mut aggregate_bounded = Vec::new();
        append_recent_history_images_with_loader(
            &mut aggregate_bounded,
            "session-a",
            &history[..3],
            &[],
            |session_id, media_id| {
                Box::pin(async move {
                    Some((
                        stored_image(
                            &media_id,
                            &session_id,
                            MAX_RECENT_HISTORY_IMAGE_BYTES_PER_READ,
                        ),
                        vec![1u8; MAX_RECENT_HISTORY_IMAGE_BYTES_PER_READ as usize],
                    ))
                }) as HistoryImageLoad
            },
        )
        .await;
        assert_eq!(
            aggregate_bounded[0]
                .content
                .parts()
                .iter()
                .filter(|part| matches!(part, ContentPart::Binary(_)))
                .count(),
            2
        );
    }
}
