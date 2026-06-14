use crate::medousa_paths::load_tui_defaults;
use crate::messaging::secrets::load_secret_value;
use crate::provider_catalog::{self, ProviderValidation};
use reqwest::multipart;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const MAX_AUDIO_BYTES: usize = 25 * 1024 * 1024;
const STT_SETTINGS_HINT: &str = "Configure speech input in Settings → Voice → Speech input.";

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

struct SttConfig {
    model: String,
    base_url: String,
    needs_api_key: bool,
}

fn stt_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|err| err.to_string())
}

fn default_whisper_model(provider_id: &str) -> &'static str {
    if provider_id == "groq" {
        "whisper-large-v3"
    } else {
        "whisper-1"
    }
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

fn provider_supports_whisper(provider_id: &str) -> bool {
    let Some(spec) = provider_catalog::find_provider(provider_id) else {
        return false;
    };
    match spec.validation {
        ProviderValidation::OpenAiCompatible => true,
        ProviderValidation::AcceptKey if spec.supports_custom_base_url => true,
        _ => false,
    }
}

fn provider_label(provider_id: &str) -> &str {
    provider_catalog::find_provider(provider_id)
        .map(|spec| spec.label)
        .unwrap_or(provider_id)
}

fn load_stt_config() -> Result<SttConfig, String> {
    let defaults = load_tui_defaults();
    let provider_id = defaults
        .stt_provider
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_lowercase)
        .unwrap_or_else(|| "openai".to_string());

    if !provider_supports_whisper(&provider_id) {
        return Err(format!(
            "{} does not support Whisper transcription — pick an OpenAI-compatible provider there (e.g. OpenAI, Groq). Current: {}.",
            STT_SETTINGS_HINT,
            provider_label(&provider_id)
        ));
    }

    let model = defaults
        .stt_model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| default_whisper_model(&provider_id).to_string());

    let base_url = resolve_base_url(&provider_id, defaults.stt_base_url.as_deref()).ok_or_else(
        || {
            format!(
                "No transcription API base URL for {} — set one under Speech input in Settings → Voice.",
                provider_label(&provider_id)
            )
        },
    )?;

    let needs_api_key = provider_catalog::find_provider(&provider_id)
        .map(|spec| spec.needs_api_key)
        .unwrap_or(true);

    Ok(SttConfig {
        model,
        base_url,
        needs_api_key,
    })
}

fn resolve_stt_api_key(needs_key: bool) -> Result<String, String> {
    if !needs_key {
        return Ok(String::new());
    }

    if let Some(key) = load_secret_value("stt_api_key")?
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        return Ok(key);
    }

    if let Some(key) = load_secret_value("api_key")?
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        return Ok(key);
    }

    Err(format!(
        "Add a speech input API key in Settings → Voice → Speech input (or reuse your chat API key)."
    ))
}

fn stt_unavailable_reason(config: &Result<SttConfig, String>) -> Option<String> {
    match config {
        Ok(cfg) => match resolve_stt_api_key(cfg.needs_api_key) {
            Ok(_) => None,
            Err(reason) => Some(reason),
        },
        Err(reason) => Some(reason.clone()),
    }
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

#[tauri::command]
pub fn composer_stt_status() -> ComposerSttStatus {
    let config = load_stt_config();
    if let Some(reason) = stt_unavailable_reason(&config) {
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

    let config = load_stt_config()?;
    let api_key = resolve_stt_api_key(config.needs_api_key)?;

    let mime_type = request.mime_type.trim();
    let mime_type = if mime_type.is_empty() {
        "audio/webm"
    } else {
        mime_type
    };
    let extension = extension_for_mime(mime_type);
    let filename = format!("composer-voice.{extension}");

    let form = multipart::Form::new()
        .text("model", config.model.clone())
        .part(
            "file",
            multipart::Part::bytes(request.audio_bytes)
                .file_name(filename)
                .mime_str(mime_type)
                .map_err(|err| err.to_string())?,
        );

    let client = stt_client()?;
    let mut req = client
        .post(transcription_url(&config.base_url))
        .multipart(form);
    if config.needs_api_key {
        req = req.bearer_auth(api_key);
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
            return Err(
                "Invalid speech input API key — update it in Settings → Voice → Speech input."
                    .into(),
            );
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
