use axum::{
    extract::Multipart,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};

use crate::stt::{self, SttStatusResponse, SttTranscribeResponse};

pub fn routes() -> Router {
    Router::new()
        .route("/v1/stt/status", get(stt_status))
        .route("/v1/stt/transcribe", post(stt_transcribe))
}

async fn stt_status() -> Json<SttStatusResponse> {
    Json(stt::stt_status())
}

async fn stt_transcribe(
    mut multipart: Multipart,
) -> Result<Json<SttTranscribeResponse>, (StatusCode, String)> {
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut mime: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?
    {
        match field.name().unwrap_or_default() {
            "file" => {
                mime = field.content_type().map(str::to_string);
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;
                file_bytes = Some(bytes.to_vec());
            }
            _ => {}
        }
    }

    let bytes = file_bytes.ok_or((
        StatusCode::BAD_REQUEST,
        "multipart field 'file' is required".to_string(),
    ))?;

    let mime_type = mime.as_deref().unwrap_or("audio/webm");
    stt::transcribe_audio(&bytes, mime_type)
        .await
        .map(Json)
        .map_err(|failure| (StatusCode::BAD_REQUEST, failure.operator_message))
}
