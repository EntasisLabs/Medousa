//! Provider-neutral, daemon-owned image generation.

use std::sync::Arc;

use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use stasis::prelude::StasisError;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::inference_profiles::{InferenceProfile, InferenceTarget};
use crate::typed_tools::{ExternalJson, ToolId, medousa_tool};

pub const COGNITION_IMAGE_GENERATE: &str = "cognition_image_generate";
const COGNITION_IMAGE_GENERATE_ID: ToolId = ToolId::new(COGNITION_IMAGE_GENERATE);
const MAX_PROMPT_CHARS: usize = 8_000;
const MAX_COUNT: u8 = 4;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ImageAspect { Square, Portrait, Landscape }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ImageQuality { Low, Medium, High }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ImageBackground { Auto, Transparent, Opaque }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImageGenerationRequest {
    pub prompt: String,
    pub references: Vec<ImageInputRef>,
    pub aspect: ImageAspect,
    pub quality: ImageQuality,
    pub background: ImageBackground,
    pub count: u8,
    pub parent_generation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImageInputRef {
    pub bytes: Vec<u8>,
    pub mime: String,
    pub filename: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GeneratedImageBytes {
    pub bytes: Vec<u8>,
    pub mime: String,
    pub width_px: u32,
    pub height_px: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImageProviderReceipt {
    pub provider: String,
    pub model: String,
    pub credential_lane: String,
    pub usage: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImageGenerationResult {
    pub generation_id: String,
    pub outputs: Vec<GeneratedImageBytes>,
    pub revised_prompt: Option<String>,
    pub provider_receipt: ImageProviderReceipt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageGenerationFailure {
    Auth(String), Unsupported(String), Policy(String), RateLimit(String), InvalidInput(String),
    ProviderDown(String), Cancelled, MalformedOutput(String),
}

impl std::fmt::Display for ImageGenerationFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auth(message) | Self::Unsupported(message) | Self::Policy(message)
            | Self::RateLimit(message) | Self::InvalidInput(message)
            | Self::ProviderDown(message) | Self::MalformedOutput(message) => f.write_str(message),
            Self::Cancelled => f.write_str("image generation was cancelled"),
        }
    }
}

#[async_trait]
pub trait ImageGenerationPort: Send + Sync {
    async fn generate(
        &self,
        target: &InferenceTarget,
        request: ImageGenerationRequest,
        cancel: CancellationToken,
    ) -> Result<ImageGenerationResult, ImageGenerationFailure>;
}

#[derive(Clone)]
struct OpenAiImageGenerationPort {
    client: reqwest::Client,
}

impl Default for OpenAiImageGenerationPort {
    fn default() -> Self { Self { client: reqwest::Client::new() } }
}

#[derive(Deserialize)]
struct OpenAiImageResponse { data: Vec<OpenAiImageData>, usage: Option<Value> }

#[derive(Deserialize)]
struct OpenAiImageData { b64_json: Option<String>, revised_prompt: Option<String> }

fn dimensions(aspect: &ImageAspect) -> (&'static str, u32, u32) {
    match aspect {
        ImageAspect::Square => ("1024x1024", 1024, 1024),
        ImageAspect::Portrait => ("1024x1536", 1024, 1536),
        ImageAspect::Landscape => ("1536x1024", 1536, 1024),
    }
}

fn normalize_provider_error(status: reqwest::StatusCode, body: &str) -> ImageGenerationFailure {
    let message = serde_json::from_str::<Value>(body).ok()
        .and_then(|value| value.pointer("/error/message").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_else(|| format!("image provider returned HTTP {status}"));
    match status.as_u16() {
        401 | 403 => ImageGenerationFailure::Auth(message),
        400 if message.to_ascii_lowercase().contains("safety") => ImageGenerationFailure::Policy(message),
        400 | 404 | 422 => ImageGenerationFailure::InvalidInput(message),
        429 => ImageGenerationFailure::RateLimit(message),
        _ => ImageGenerationFailure::ProviderDown(message),
    }
}

#[async_trait]
impl ImageGenerationPort for OpenAiImageGenerationPort {
    async fn generate(&self, target: &InferenceTarget, request: ImageGenerationRequest, cancel: CancellationToken) -> Result<ImageGenerationResult, ImageGenerationFailure> {
        if target.provider != "openai" {
            return Err(ImageGenerationFailure::Unsupported(format!("{} does not have an image-generation adapter", target.provider)));
        }
        let key = crate::session::load_provider_api_key("openai")
            .ok_or_else(|| ImageGenerationFailure::Auth("OpenAI API key required for the Image API lane".to_string()))?;
        let (size, width_px, height_px) = dimensions(&request.aspect);
        let base = target.base_url.as_deref().unwrap_or("https://api.openai.com").trim_end_matches('/');
        let url = if base.ends_with("/v1") { format!("{base}/images/generations") } else { format!("{base}/v1/images/generations") };
        let quality = match request.quality { ImageQuality::Low => "low", ImageQuality::Medium => "medium", ImageQuality::High => "high" };
        let background = match request.background { ImageBackground::Auto => "auto", ImageBackground::Transparent => "transparent", ImageBackground::Opaque => "opaque" };
        let send = if request.references.is_empty() {
            let body = json!({
                "model": target.model,
                "prompt": request.prompt,
                "n": request.count,
                "size": size,
                "quality": quality,
                "background": background,
                "output_format": "png"
            });
            self.client.post(url).bearer_auth(key).json(&body).send()
        } else {
            let edit_url = if base.ends_with("/v1") { format!("{base}/images/edits") } else { format!("{base}/v1/images/edits") };
            let mut form = reqwest::multipart::Form::new()
                .text("model", target.model.clone())
                .text("prompt", request.prompt.clone())
                .text("n", request.count.to_string())
                .text("size", size.to_string())
                .text("quality", quality.to_string())
                .text("background", background.to_string())
                .text("output_format", "png");
            for reference in &request.references {
                let part = reqwest::multipart::Part::bytes(reference.bytes.clone())
                    .file_name(reference.filename.clone())
                    .mime_str(&reference.mime)
                    .map_err(|error| ImageGenerationFailure::InvalidInput(error.to_string()))?;
                form = form.part("image[]", part);
            }
            self.client.post(edit_url).bearer_auth(key).multipart(form).send()
        };
        let response = tokio::select! {
            _ = cancel.cancelled() => return Err(ImageGenerationFailure::Cancelled),
            result = send => result.map_err(|error| ImageGenerationFailure::ProviderDown(error.to_string()))?,
        };
        let status = response.status();
        let bytes = tokio::select! {
            _ = cancel.cancelled() => return Err(ImageGenerationFailure::Cancelled),
            result = response.bytes() => result.map_err(|error| ImageGenerationFailure::ProviderDown(error.to_string()))?,
        };
        if !status.is_success() { return Err(normalize_provider_error(status, &String::from_utf8_lossy(&bytes))); }
        let decoded: OpenAiImageResponse = serde_json::from_slice(&bytes)
            .map_err(|error| ImageGenerationFailure::MalformedOutput(error.to_string()))?;
        let revised_prompt = decoded.data.iter().find_map(|item| item.revised_prompt.clone());
        let outputs = decoded.data.into_iter().map(|item| {
            let encoded = item.b64_json.ok_or_else(|| ImageGenerationFailure::MalformedOutput("image result omitted b64_json".to_string()))?;
            let bytes = STANDARD.decode(encoded).map_err(|error| ImageGenerationFailure::MalformedOutput(error.to_string()))?;
            Ok(GeneratedImageBytes { bytes, mime: "image/png".to_string(), width_px, height_px })
        }).collect::<Result<Vec<_>, _>>()?;
        if outputs.is_empty() { return Err(ImageGenerationFailure::MalformedOutput("image result was empty".to_string())); }
        Ok(ImageGenerationResult {
            generation_id: format!("img:{}", Uuid::new_v4().simple()),
            outputs,
            revised_prompt,
            provider_receipt: ImageProviderReceipt { provider: target.provider.clone(), model: target.model.clone(), credential_lane: "api_key".to_string(), usage: decoded.usage },
        })
    }
}

fn configured_profile() -> Option<InferenceProfile> {
    crate::session::load_tui_defaults().inference_profiles
        .and_then(|profiles| profiles.image_generation)
        .and_then(|profile| profile.trimmed())
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ImageGenerateInput {
    /// A concise visual prompt describing the desired image.
    prompt: String,
    #[serde(default)] aspect: Option<ImageAspect>,
    #[serde(default)] quality: Option<ImageQuality>,
    #[serde(default)] background: Option<ImageBackground>,
    #[serde(default)] count: Option<u8>,
    /// Session media ids to edit or use as visual references.
    #[serde(default)] reference_media_ids: Vec<String>,
    #[serde(default)] parent_generation_id: Option<String>,
}

pub struct CognitionImageGenerateTool { session_id: String, port: Arc<dyn ImageGenerationPort> }

impl CognitionImageGenerateTool {
    pub fn new(session_id: impl Into<String>) -> Self {
        Self { session_id: session_id.into(), port: Arc::new(OpenAiImageGenerationPort::default()) }
    }
}

#[medousa_tool(id = COGNITION_IMAGE_GENERATE_ID)]
impl CognitionImageGenerateTool {
    /// Generate one or more images when a user explicitly asks to create or revise visual media. For an attached-image revision, pass its attachment id in reference_media_ids and its generation_id as parent_generation_id. Results are persisted by the workshop and appear directly in chat.
    async fn invoke_typed(
        &self,
        input: ImageGenerateInput,
    ) -> stasis::prelude::Result<ExternalJson> {
        let prompt = input.prompt.trim();
        if prompt.is_empty() || prompt.chars().count() > MAX_PROMPT_CHARS {
            return Err(StasisError::PortFailure(format!("prompt must be 1–{MAX_PROMPT_CHARS} characters")));
        }
        let count = input.count.unwrap_or(1);
        if !(1..=MAX_COUNT).contains(&count) {
            return Err(StasisError::PortFailure(format!("count must be 1–{MAX_COUNT}")));
        }
        let profile = configured_profile().ok_or_else(|| StasisError::PortFailure(
            "Image generation is not configured. Choose an Image generation model in Settings → Models.".to_string()
        ))?;
        if input.reference_media_ids.len() > crate::media_vision::MAX_MEDIA_REFS_PER_TURN {
            return Err(StasisError::PortFailure(format!(
                "at most {} reference images are allowed",
                crate::media_vision::MAX_MEDIA_REFS_PER_TURN
            )));
        }
        let mut references = Vec::new();
        for media_id in &input.reference_media_ids {
            let record = crate::media_store::get_media_record(&self.session_id, media_id)
                .ok_or_else(|| StasisError::PortFailure(format!("unknown reference media_id '{media_id}' for session")))?;
            if !record.mime.starts_with("image/") {
                return Err(StasisError::PortFailure(format!("reference media '{media_id}' is not an image")));
            }
            references.push(ImageInputRef {
                bytes: crate::media_store::open_media_payload(&record).map_err(StasisError::PortFailure)?,
                mime: record.mime,
                filename: record.label.unwrap_or_else(|| format!("reference-{}.png", references.len() + 1)),
            });
        }
        let request = ImageGenerationRequest {
            prompt: prompt.to_string(), references, aspect: input.aspect.unwrap_or(ImageAspect::Square),
            quality: input.quality.unwrap_or(ImageQuality::Medium), background: input.background.unwrap_or(ImageBackground::Auto),
            count, parent_generation_id: input.parent_generation_id.clone(),
        };
        let cancellation = crate::agent_runtime::execution_context::active_turn_execution_context()
            .map(|context| context.cancellation().clone())
            .unwrap_or_default();
        let mut targets = vec![profile.as_target()];
        targets.extend(profile.fallbacks.iter().cloned());
        let mut failures = Vec::new();
        let mut generated = None;
        for target in targets {
            if target.provider == "openai-codex" {
                failures.push(json!({
                    "provider": target.provider,
                    "model": target.model,
                    "credential_lane": "chatgpt_subscription",
                    "error": "This Medousa build cannot yet invoke the ChatGPT Codex image-generation backend directly."
                }));
                continue;
            }
            match self
                .port
                .generate(&target, request.clone(), cancellation.clone())
                .await
            {
                Ok(result) => {
                    generated = Some(result);
                    break;
                }
                Err(error) => failures.push(json!({
                    "provider": target.provider,
                    "model": target.model,
                    "error": error.to_string(),
                })),
            }
        }
        let Some(result) = generated else {
            return Ok(ExternalJson::new(json!({
                "ok": false,
                "error": "Image generation failed on every configured route.",
                "failures": failures,
            })));
        };
        let mut outputs = Vec::new();
        for (index, image) in result.outputs.iter().enumerate() {
            let label = if result.outputs.len() == 1 { "Generated image".to_string() } else { format!("Generated image {}", index + 1) };
            let upload = crate::media_store::persist_generated_media(&self.session_id, &image.bytes, &image.mime, Some(&label))
                .map_err(StasisError::PortFailure)?;
            outputs.push(json!({
                "media_id": upload.media_id, "mime": upload.mime, "label": label,
                "byte_size": upload.byte_size, "width_px": image.width_px, "height_px": image.height_px
            }));
        }
        Ok(ExternalJson::new(json!({
            "ok": true, "generation_id": result.generation_id,
            "parent_generation_id": request.parent_generation_id,
            "revised_prompt": result.revised_prompt, "outputs": outputs,
            "provider_receipt": result.provider_receipt
        })))
    }
}

pub fn generated_media_from_tool_output(output: &Value) -> Vec<medousa_types::TurnPart> {
    let Some(generation_id) = output.get("generation_id").and_then(Value::as_str) else { return Vec::new(); };
    let parent = output.get("parent_generation_id").and_then(Value::as_str).map(str::to_string);
    let provider = output.pointer("/provider_receipt/provider").and_then(Value::as_str).map(str::to_string);
    let model = output.pointer("/provider_receipt/model").and_then(Value::as_str).map(str::to_string);
    output.get("outputs").and_then(Value::as_array).into_iter().flatten().filter_map(|item| Some(medousa_types::TurnPart::GeneratedMedia {
        media_id: item.get("media_id")?.as_str()?.to_string(),
        mime: item.get("mime")?.as_str()?.to_string(),
        label: item.get("label")?.as_str()?.to_string(),
        generation_id: generation_id.to_string(), parent_generation_id: parent.clone(),
        width_px: item.get("width_px").and_then(Value::as_u64).and_then(|value| u32::try_from(value).ok()),
        height_px: item.get("height_px").and_then(Value::as_u64).and_then(|value| u32::try_from(value).ok()),
        byte_size: item.get("byte_size").and_then(Value::as_u64), provider: provider.clone(), model: model.clone(),
    })).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_generated_media_with_lineage() {
        let parts = generated_media_from_tool_output(&json!({
            "generation_id":"img:2", "parent_generation_id":"img:1",
            "provider_receipt":{"provider":"openai","model":"gpt-image-2"},
            "outputs":[{"media_id":"gen:s:1","mime":"image/png","label":"Generated image","byte_size":12,"width_px":1024,"height_px":1536}]
        }));
        assert!(matches!(&parts[0], medousa_types::TurnPart::GeneratedMedia { generation_id, parent_generation_id, .. } if generation_id == "img:2" && parent_generation_id.as_deref() == Some("img:1")));
    }

    #[test]
    fn normalizes_provider_errors() {
        assert!(matches!(normalize_provider_error(reqwest::StatusCode::UNAUTHORIZED, "{}"), ImageGenerationFailure::Auth(_)));
        assert!(matches!(normalize_provider_error(reqwest::StatusCode::TOO_MANY_REQUESTS, "{}"), ImageGenerationFailure::RateLimit(_)));
    }
}
