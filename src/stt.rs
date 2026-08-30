//! Speech-to-text via Whisper-compatible APIs (Phase 4).

use std::sync::Arc;
use std::time::Duration;

use reqwest::multipart;

use crate::inference_profiles::InferenceProfileKind;
use crate::inference_profiles::InferenceTarget;
use crate::inference_router::{self, CapabilityRequirement};
use crate::inference_router::{CredentialAvailability, ProviderCredentialRequirement};
use crate::session::load_provider_api_key;
use crate::turn_failure::TurnFailure;
use medousa_runtime::{CredentialProvider, ProviderCredential};

pub const MAX_AUDIO_BYTES: usize = 25 * 1024 * 1024;
const STT_SETTINGS_HINT: &str = "Configure speech input in Settings → Voice → Speech input.";

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SttStatusResponse {
    pub available: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SttTranscribeResponse {
    pub text: String,
}

pub fn stt_status() -> SttStatusResponse {
    let targets = inference_router::profile_targets(InferenceProfileKind::Stt);
    if targets.is_empty() {
        return SttStatusResponse {
            available: false,
            reason: Some(format!(
                "No speech input profile configured — set one under {STT_SETTINGS_HINT}"
            )),
        };
    }

    let mut last_reason: Option<String> = None;
    for target in targets {
        if !inference_router::target_is_eligible(&target, CapabilityRequirement::None) {
            last_reason = Some(missing_target_reason(&target));
            continue;
        }
        if resolve_stt_base_url(&target).is_none() {
            last_reason = Some(format!(
                "No transcription API base URL for {} — set one under Speech input in Settings → Voice.",
                target.provider
            ));
            continue;
        }
        return SttStatusResponse {
            available: true,
            reason: None,
        };
    }

    SttStatusResponse {
        available: false,
        reason: last_reason.or_else(|| {
            Some(format!(
                "Speech input is not ready — check provider, model, and API key under {STT_SETTINGS_HINT}"
            ))
        }),
    }
}

/// Resolve speech-input availability through the native host's credential
/// authority. This keeps Personal/mobile deployments on the same secure
/// Keychain-backed provider configuration as chat.
pub async fn stt_status_with_credentials(
    credentials: &dyn CredentialProvider,
) -> SttStatusResponse {
    let targets = inference_router::profile_targets(InferenceProfileKind::Stt);
    if targets.is_empty() {
        return SttStatusResponse {
            available: false,
            reason: Some(format!(
                "No speech input profile configured — set one under {STT_SETTINGS_HINT}"
            )),
        };
    }

    let mut last_reason: Option<String> = None;
    for target in targets {
        match inference_router::provider_credential_requirement(&target.provider) {
            ProviderCredentialRequirement::None => {}
            ProviderCredentialRequirement::ApiKey => {
                if credentials.credential_for(&target.provider).await.is_err() {
                    last_reason = Some(missing_api_key_reason());
                    continue;
                }
            }
            ProviderCredentialRequirement::ChatGptOAuth => {
                last_reason = Some(
                    "ChatGPT account inference does not provide speech transcription.".to_string(),
                );
                continue;
            }
        }
        if resolve_stt_base_url(&target).is_none() {
            last_reason = Some(format!(
                "No transcription API base URL for {} — set one under Speech input in Settings → Voice.",
                target.provider
            ));
            continue;
        }
        return SttStatusResponse {
            available: true,
            reason: None,
        };
    }

    SttStatusResponse {
        available: false,
        reason: last_reason.or_else(|| {
            Some(format!(
                "Speech input is not ready — check provider, model, and API key under {STT_SETTINGS_HINT}"
            ))
        }),
    }
}

pub async fn transcribe_audio(
    audio_bytes: &[u8],
    mime_type: &str,
) -> Result<SttTranscribeResponse, TurnFailure> {
    validate_audio(audio_bytes)?;
    let mime_type = normalized_mime_type(mime_type);

    let execution = inference_router::execute_with_fallbacks(
        InferenceProfileKind::Stt,
        CapabilityRequirement::None,
        |_notice| {},
        |target| transcribe_with_target(target, audio_bytes, mime_type),
    )
    .await?;

    Ok(execution.result)
}

/// Transcribe through credentials supplied by the deployment host rather than
/// consulting a second daemon-local secret store.
pub async fn transcribe_audio_with_credentials(
    audio_bytes: &[u8],
    mime_type: &str,
    credentials: Arc<dyn CredentialProvider>,
) -> Result<SttTranscribeResponse, TurnFailure> {
    validate_audio(audio_bytes)?;
    let mime_type = normalized_mime_type(mime_type);

    let execution = inference_router::execute_with_fallbacks_and_eligibility(
        InferenceProfileKind::Stt,
        CapabilityRequirement::None,
        |_notice| {},
        |target, required| {
            inference_router::target_ineligibility_with_credentials(
                target,
                required,
                CredentialAvailability {
                    // The operation resolves this from the host authority. A
                    // missing key becomes an auth failure and advances to the
                    // next configured fallback.
                    api_key_configured: true,
                    chatgpt_oauth_configured: false,
                },
            )
        },
        |target| {
            transcribe_with_credential_provider(target, audio_bytes, mime_type, credentials.clone())
        },
    )
    .await?;

    Ok(execution.result)
}

fn validate_audio(audio_bytes: &[u8]) -> Result<(), TurnFailure> {
    if audio_bytes.is_empty() {
        return Err(TurnFailure::validation(
            "Recording was empty — try again.",
            "empty audio payload",
        ));
    }
    if audio_bytes.len() > MAX_AUDIO_BYTES {
        return Err(TurnFailure::validation(
            "Recording is too long — keep voice messages under a few minutes.",
            format!("audio payload exceeds {MAX_AUDIO_BYTES} bytes"),
        ));
    }
    Ok(())
}

fn normalized_mime_type(mime_type: &str) -> &str {
    let mime_type = mime_type.trim();
    if mime_type.is_empty() {
        "audio/webm"
    } else {
        mime_type
    }
}

async fn transcribe_with_target(
    target: InferenceTarget,
    audio_bytes: &[u8],
    mime_type: &str,
) -> Result<SttTranscribeResponse, String> {
    let credential = match inference_router::provider_credential_requirement(&target.provider) {
        ProviderCredentialRequirement::None => None,
        ProviderCredentialRequirement::ApiKey => Some(
            ProviderCredential::new(load_provider_api_key(&target.provider).ok_or_else(|| {
                "Add a speech input API key in Settings → Voice → Speech input (or reuse your chat API key)."
                    .to_string()
            })?)
            .map_err(|error| error.to_string())?,
        ),
        ProviderCredentialRequirement::ChatGptOAuth => {
            return Err("ChatGPT account inference does not provide speech transcription.".into());
        }
    };
    transcribe_with_credential(target, audio_bytes, mime_type, credential).await
}

async fn transcribe_with_credential_provider(
    target: InferenceTarget,
    audio_bytes: &[u8],
    mime_type: &str,
    credentials: Arc<dyn CredentialProvider>,
) -> Result<SttTranscribeResponse, String> {
    let credential = match inference_router::provider_credential_requirement(&target.provider) {
        ProviderCredentialRequirement::None => None,
        ProviderCredentialRequirement::ApiKey => Some(
            credentials
                .credential_for(&target.provider)
                .await
                .map_err(|_| missing_api_key_reason())?,
        ),
        ProviderCredentialRequirement::ChatGptOAuth => {
            return Err("ChatGPT account inference does not provide speech transcription.".into());
        }
    };
    transcribe_with_credential(target, audio_bytes, mime_type, credential).await
}

async fn transcribe_with_credential(
    target: InferenceTarget,
    audio_bytes: &[u8],
    mime_type: &str,
    credential: Option<ProviderCredential>,
) -> Result<SttTranscribeResponse, String> {
    let base_url = resolve_stt_base_url(&target).ok_or_else(|| {
        format!(
            "No transcription API base URL for {} — set one under Speech input in Settings → Voice.",
            target.provider
        )
    })?;

    let extension = extension_for_mime(mime_type);
    let filename = format!("composer-voice.{extension}");
    let form = multipart::Form::new()
        .text("model", target.model.clone())
        .part(
            "file",
            multipart::Part::bytes(audio_bytes.to_vec())
                .file_name(filename)
                .mime_str(mime_type)
                .map_err(|err| err.to_string())?,
        );

    let client = stt_client()?;
    let request = client.post(transcription_url(&base_url)).multipart(form);
    let request = match credential.as_ref() {
        Some(credential) => credential.with_secret(|secret| request.bearer_auth(secret)),
        None => request,
    };
    parse_transcription_response(request.send().await).await
}

async fn parse_transcription_response(
    response: Result<reqwest::Response, reqwest::Error>,
) -> Result<SttTranscribeResponse, String> {
    let response = response.map_err(|err| {
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
    Ok(SttTranscribeResponse { text })
}

fn missing_api_key_reason() -> String {
    "Authentication is required — add a speech input API key in Settings → Voice → Speech input (or reuse your chat API key)."
        .to_string()
}

fn resolve_stt_base_url(target: &InferenceTarget) -> Option<String> {
    crate::resolve_llm_base_url(Some(target.provider.as_str()), target.base_url.as_deref())
}

fn missing_target_reason(target: &InferenceTarget) -> String {
    match inference_router::target_ineligibility_reason(target, CapabilityRequirement::None) {
        Some("missing_api_key") => {
            "Add a speech input API key in Settings → Voice → Speech input (or reuse your chat API key)."
                .to_string()
        }
        Some("missing_chatgpt_oauth") => {
            "Connect a ChatGPT account before using this inference route.".to_string()
        }
        _ => format!(
            "Speech input target {}:{} is not ready — check Settings → Voice → Speech input.",
            target.provider, target.model
        ),
    }
}

fn stt_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|err| err.to_string())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcription_url_appends_openai_path() {
        assert_eq!(
            transcription_url("https://api.openai.com/v1"),
            "https://api.openai.com/v1/audio/transcriptions"
        );
        assert_eq!(
            transcription_url("https://api.groq.com/openai"),
            "https://api.groq.com/openai/v1/audio/transcriptions"
        );
    }
}
