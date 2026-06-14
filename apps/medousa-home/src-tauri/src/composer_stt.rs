use crate::medousa_paths::load_tui_defaults;
use crate::messaging::secrets::load_secret_value;
use crate::provider_catalog::{self, ProviderValidation};
use reqwest::multipart;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const MAX_AUDIO_BYTES: usize = 25 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposerSttStatus {
    pub available: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposerSttRequest {
    pub audio_bytes: Vec<u8>,
    pub mime_type: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposerSttResult {
    pub text: String,
}

fn stt_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|err| err.to_string())
}

fn resolve_provider_id() -> String {
    load_tui_defaults()
        .provider
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "openai".to_string())
}

fn resolve_base_url(provider_id: &str, override_base: Option<&str>) -> Option<String> {
    override_base
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            provider_catalog::find_provider(provider_id)
                .and_then(|spec| spec.default_base_url.map(str::to_string))
        })
}

fn whisper_model(provider_id: &str) -> Option<&'static str> {
    match provider_id {
        "groq" => Some("whisper-large-v3"),
        "openai" | "openrouter" | "together" | "fireworks" | "mistral" | "azure-openai" => {
            Some("whisper-1")
        }
        _ => None,
    }
}

fn whisper_provider_label(provider_id: &str) -> &str {
    provider_catalog::find_provider(provider_id)
        .map(|spec| spec.label)
        .unwrap_or(provider_id)
}

fn transcription_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.ends_with("/v1") {
        format!("{trimmed}/audio/transcriptions")
    } else {
        format!("{trimmed}/v1/audio/transcriptions")
    }
}

fn extension_for_mime(mime_type: &str) -> &'static str {
    let mime = mime_type.trim().to_lowercase();
    if mime.contains("mp4") || mime.contains("m4a") || mime.contains("aac") {
        "m4a"
    } else if mime.contains("ogg") {
        "ogg"
    } else if mime.contains("wav") {
        "wav"
    } else {
        "webm"
    }
}

fn stt_unavailable_reason(provider_id: &str) -> Option<String> {
    let spec = provider_catalog::find_provider(provider_id);
    let validation = spec.map(|entry| entry.validation);

    if validation == Some(ProviderValidation::Ollama) {
        return Some(
            "Voice input needs a cloud provider with Whisper — Ollama does not support transcription."
                .into(),
        );
    }
    if matches!(
        validation,
        Some(ProviderValidation::Anthropic) | Some(ProviderValidation::Google)
    ) {
        let label = spec.map(|entry| entry.label).unwrap_or(provider_id);
        return Some(format!(
            "Voice input needs an OpenAI-compatible provider — {label} does not offer Whisper transcription."
        ));
    }
    if whisper_model(provider_id).is_none() {
        return Some(format!(
            "Voice input uses Whisper — switch to OpenAI or Groq in Settings → Voice (current: {}).",
            whisper_provider_label(provider_id)
        ));
    }

    let needs_key = spec.map(|entry| entry.needs_api_key).unwrap_or(true);
    if needs_key {
        let key = load_secret_value("api_key").map_err(|err| err.to_string()).ok().flatten();
        if key.is_none() {
            return Some(
                "Add an API key in Settings → Voice before using voice input.".into(),
            );
        }
    }

    None
}

#[tauri::command]
pub fn composer_stt_status() -> ComposerSttStatus {
    let provider_id = resolve_provider_id();
    if let Some(reason) = stt_unavailable_reason(&provider_id) {
        return ComposerSttStatus {
            available: false,
            reason: Some(reason),
        };
    }
    ComposerSttStatus {
        available: true,
        reason: None,
    }
}

#[tauri::command]
pub async fn composer_stt_transcribe(
    request: ComposerSttRequest,
) -> Result<ComposerSttResult, String> {
    if request.audio_bytes.is_empty() {
        return Err("Recording was empty — try again.".into());
    }
    if request.audio_bytes.len() > MAX_AUDIO_BYTES {
        return Err("Recording is too long — keep voice messages under a few minutes.".into());
    }

    let provider_id = resolve_provider_id();
    if let Some(reason) = stt_unavailable_reason(&provider_id) {
        return Err(reason);
    }

    let model = whisper_model(&provider_id)
        .ok_or_else(|| format!("Voice input is not configured for provider \"{provider_id}\"."))?;

    let defaults = load_tui_defaults();
    let base_url = resolve_base_url(&provider_id, defaults.base_url.as_deref()).ok_or_else(|| {
        format!("No API base URL configured for provider \"{provider_id}\".")
    })?;

    let spec = provider_catalog::find_provider(&provider_id);
    let needs_key = spec.map(|entry| entry.needs_api_key).unwrap_or(true);
    let api_key = if needs_key {
        load_secret_value("api_key")?
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "Add an API key in Settings → Voice before using voice input.".to_string())?
    } else {
        String::new()
    };

    let mime_type = request.mime_type.trim();
    let mime_type = if mime_type.is_empty() {
        "audio/webm"
    } else {
        mime_type
    };
    let extension = extension_for_mime(mime_type);
    let filename = format!("composer-voice.{extension}");

    let form = multipart::Form::new()
        .text("model", model)
        .part(
            "file",
            multipart::Part::bytes(request.audio_bytes)
                .file_name(filename)
                .mime_str(mime_type)
                .map_err(|err| err.to_string())?,
        );

    let client = stt_client()?;
    let mut req = client.post(transcription_url(&base_url)).multipart(form);
    if needs_key {
        req = req.bearer_auth(api_key.trim());
    }

    let response = req.send().await.map_err(|err| {
        if err.is_connect() || err.is_timeout() {
            "Could not reach the transcription service — check your network.".to_string()
        } else {
            err.to_string()
        }
    })?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let detail = body.trim();
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err("Invalid API key — update it in Settings → Voice.".into());
        }
        if detail.is_empty() {
            return Err(format!("Transcription failed ({status})."));
        }
        return Err(format!("Transcription failed ({status}): {detail}"));
    }

    let payload: serde_json::Value = response.json().await.map_err(|err| err.to_string())?;
    let text = payload
        .get("text")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim()
        .to_string();

    Ok(ComposerSttResult { text })
}
