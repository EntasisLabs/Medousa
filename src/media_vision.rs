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
    let provider = provider.trim().to_ascii_lowercase();
    let model = model.trim().to_ascii_lowercase();
    if model.is_empty() {
        return false;
    }

    if model.contains("vision") || model.contains("llava") || model.contains("bakllava") {
        return true;
    }

    match provider.as_str() {
        "openai" | "azure-openai" | "azure_openai" => {
            model.starts_with("gpt-4o")
                || model.starts_with("gpt-4.1")
                || model.starts_with("gpt-4-turbo")
                || model.starts_with("gpt-5")
                || model.contains("vision")
        }
        "anthropic" => {
            model.contains("claude-3")
                || model.contains("claude-4")
                || model.contains("claude-opus")
                || model.contains("claude-sonnet")
                || model.contains("claude-haiku")
        }
        "google" | "gemini" | "google-gemini" => model.contains("gemini"),
        "groq" => model.contains("vision"),
        "ollama" | "local" | "lmstudio" | "lm-studio" => {
            model.contains("llava")
                || model.contains("vision")
                || model.contains("moondream")
                || model.contains("minicpm-v")
        }
        _ => false,
    }
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

fn is_image_media_ref(media_ref: &MediaRef) -> bool {
    if media_ref.kind == "image" {
        return true;
    }
    media_ref.mime.trim().to_ascii_lowercase().starts_with("image/")
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
    fn empty_plan_builds_text_user_message() {
        let plan = TurnMediaVisionPlan::empty();
        let message = plan.build_user_message("hello");
        assert_eq!(message.content.first_text(), Some("hello"));
    }
}
