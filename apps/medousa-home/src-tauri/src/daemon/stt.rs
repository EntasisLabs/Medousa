use crate::workshop_transport::MultipartField;
use serde::{Deserialize, Serialize};
use tauri::State;

use super::workshop_http;
use super::DaemonState;

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposerSttResult {
    pub text: String,
}

#[tauri::command]
pub async fn composer_stt_status(
    state: State<'_, DaemonState>,
) -> Result<ComposerSttStatus, String> {
    match workshop_http::get_json(&state, "/v1/stt/status").await {
        Ok(status) => Ok(status),
        Err(err) if err.contains("404") => Ok(ComposerSttStatus {
            available: false,
            reason: Some(
                "Dictation requires a newer workshop daemon — rebuild and restart medousa_daemon."
                    .to_string(),
            ),
        }),
        Err(err) => Err(err),
    }
}

#[tauri::command]
pub async fn composer_stt_transcribe(
    state: State<'_, DaemonState>,
    request: ComposerSttRequest,
) -> Result<ComposerSttResult, String> {
    if request.audio_bytes.is_empty() {
        return Err("Recording was empty — try again.".into());
    }

    let mime_type = request.mime_type.trim();
    let mime_type = if mime_type.is_empty() {
        "audio/webm".to_string()
    } else {
        mime_type.to_string()
    };
    let extension = extension_for_mime(&mime_type);
    let filename = format!("composer-voice.{extension}");

    workshop_http::post_multipart(
        &state,
        "/v1/stt/transcribe",
        &[MultipartField {
            name: "file".to_string(),
            filename: Some(filename),
            mime: Some(mime_type),
            data: request.audio_bytes,
        }],
    )
    .await
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
