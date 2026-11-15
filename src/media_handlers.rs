use axum::Json;
use axum::body::Body;
use axum::extract::{Multipart, Path, Query};
use axum::http::{StatusCode, header};
use axum::response::Response;

use crate::daemon_api::MediaUploadResponse;

#[derive(Debug, serde::Deserialize)]
pub struct MediaSessionQuery {
    pub session_id: String,
}

pub async fn upload_media(
    Query(query): Query<MediaSessionQuery>,
    mut multipart: Multipart,
) -> Result<Json<MediaUploadResponse>, (StatusCode, String)> {
    let session_id = crate::session_storage::validate_session_id(&query.session_id)
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn assistant_image_bytes_are_exact_session_scoped_and_not_prefix_resolved() {
        let session = "test-assistant-image-read";
        let png = b"\x89PNG\r\n\x1a\nopaque-pixel-payload";
        let record = crate::artifact_store::persist_binary_artifact(
            session,
            "cognition_image_view",
            "output",
            "image/png",
            Some("Concept"),
            png,
        )
        .unwrap();
        let response = get_media(
            Path(record.artifact_id.clone()),
            Query(MediaSessionQuery {
                session_id: session.into(),
            }),
        )
        .await
        .unwrap();
        assert_eq!(response.headers()[header::CONTENT_TYPE], "image/png");
        assert_eq!(
            response.headers()[header::X_CONTENT_TYPE_OPTIONS],
            "nosniff"
        );
        let bytes = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        assert_eq!(bytes.as_ref(), png);
        for (session, id) in [
            ("other-image-session", record.artifact_id.clone()),
            (session, "art:test".into()),
            (session, "art:missing".into()),
        ] {
            assert_eq!(
                get_media(
                    Path(id),
                    Query(MediaSessionQuery {
                        session_id: session.into()
                    })
                )
                .await
                .unwrap_err()
                .0,
                StatusCode::NOT_FOUND
            );
        }
    }
}

pub async fn get_media(
    Path(media_id): Path<String>,
    Query(query): Query<MediaSessionQuery>,
) -> Result<Response, (StatusCode, String)> {
    let session_id = crate::session_storage::validate_session_id(&query.session_id)
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
    let media_id = media_id.trim();
    if media_id.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "media_id is required".to_string()));
    }

    // Tool-produced images are binary artifacts, not user uploads. Resolve exact
    // IDs in this session only; never use the cross-chat presentation resolver.
    let session_id = session_id.to_string();
    let media_id = media_id.to_string();
    let (mime, bytes) = tokio::task::spawn_blocking(move || {
        if media_id.starts_with("art:") {
            let image = crate::artifact_store::fetch_binary_artifact(&session_id, &media_id)
                .ok_or((StatusCode::NOT_FOUND, "image not found".to_string()))?;
            Ok((image.mime, image.bytes))
        } else {
            let record = crate::media_store::get_media_record(&session_id, &media_id)
                .ok_or((StatusCode::NOT_FOUND, "media not found".to_string()))?;
            let bytes = crate::media_store::open_media_payload(&record)
                .map_err(|err| (StatusCode::NOT_FOUND, err))?;
            Ok((record.mime, bytes))
        }
    })
    .await
    .map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "media reader unavailable".to_string(),
        )
    })??;

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime.as_str())
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .header(header::CACHE_CONTROL, "private, max-age=3600")
        .header(header::CONTENT_LENGTH, bytes.len())
        .body(Body::from(bytes))
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))
}
