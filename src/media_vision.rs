//! Vision routing for current-turn user media (P5b — images only, no history replay).

use std::collections::HashSet;
use std::sync::Arc;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use genai::chat::{ChatMessage, ContentPart, MessageContent};

use crate::daemon_api::MediaRef;
use crate::media_store::{self, MediaPromptMergeOptions};

pub const MAX_MEDIA_REFS_PER_TURN: usize = 5;
pub const MAX_VISION_IMAGES_PER_TURN: usize = 5;
const MAX_VISION_IMAGE_BYTES: u64 = 10 * 1024 * 1024;

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

    for media_ref in media_refs.iter().filter(|media_ref| is_image_media_ref(media_ref)) {
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
        image_parts.push(ContentPart::from_binary_base64(
            record.mime,
            encoded,
            label,
        ));
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

pub fn is_image_media_ref(media_ref: &MediaRef) -> bool {
    if media_ref.kind == "image" {
        return true;
    }
    media_ref.mime.trim().to_ascii_lowercase().starts_with("image/")
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

    #[test]
    fn openai_vision_models_detected() {
        assert!(supports_vision("openai", "gpt-4o-mini"));
        assert!(supports_vision("openai", "gpt-4.1-mini"));
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
            },
            MediaRef {
                media_id: "usr:s1:pdf".to_string(),
                kind: "document".to_string(),
                mime: "application/pdf".to_string(),
                label: Some("notes.pdf".to_string()),
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
        }];
        let plan = plan_turn_media("session-1", &refs, "openai", "gpt-4o-mini").expect("plan");
        assert_eq!(plan.vision_image_count, 0);
        assert!(!plan.merge_options.vision_active);
    }
}
