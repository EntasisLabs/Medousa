use axum::body::Body;
use axum::extract::{Multipart, Path, Query};
use axum::http::{header, StatusCode};
use axum::response::Response;
use axum::Json;

use crate::daemon_api::MediaUploadResponse;

#[derive(Debug, serde::Deserialize)]
pub struct MediaSessionQuery {
    pub session_id: String,
}

pub async fn upload_media(
    Query(query): Query<MediaSessionQuery>,
    mut multipart: Multipart,
) -> Result<Json<MediaUploadResponse>, (StatusCode, String)> {
    let session_id = query.session_id.trim();
    if session_id.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "session_id is required".to_string()));
    }

    let mut file_bytes: Option<Vec<u8>> = None;
    let mut mime: Option<String> = None;
    let mut label: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?
    {
        match field.name().unwrap_or_default() {
            "file" => {
                mime = field.content_type().map(str::to_string);
                label = field.file_name().map(str::to_string);
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;
                file_bytes = Some(bytes.to_vec());
            }
            "label" => {
                let text = field
                    .text()
                    .await
                    .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;
                if !text.trim().is_empty() {
                    label = Some(text.trim().to_string());
                }
            }
            _ => {}
        }
    }

    let bytes = file_bytes.ok_or((
        StatusCode::BAD_REQUEST,
        "multipart field 'file' is required".to_string(),
    ))?;

    let response = crate::media_store::persist_user_media(
        session_id,
        &bytes,
        mime.as_deref().unwrap_or("application/octet-stream"),
        label.as_deref(),
    )
    .map_err(|err| (StatusCode::BAD_REQUEST, err))?;

    Ok(Json(response))
}

pub async fn get_media(
    Path(media_id): Path<String>,
    Query(query): Query<MediaSessionQuery>,
) -> Result<Response, (StatusCode, String)> {
    let session_id = query.session_id.trim();
    let media_id = media_id.trim();
    if session_id.is_empty() || media_id.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "session_id and media_id are required".to_string()));
    }

    let record = crate::media_store::get_media_record(session_id, media_id).ok_or((
        StatusCode::NOT_FOUND,
        "media not found".to_string(),
    ))?;

    let bytes = crate::media_store::open_media_payload(&record).map_err(|err| {
        (
            StatusCode::NOT_FOUND,
            err,
        )
    })?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, record.mime.as_str())
        .header(header::CACHE_CONTROL, "private, max-age=3600")
        .header(header::CONTENT_LENGTH, bytes.len())
        .body(Body::from(bytes))
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?)
}
