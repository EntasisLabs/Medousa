use crate::daemon::types::{MediaUploadResponse, MediaRef};
use crate::daemon::{daemon_http_client, DaemonState};
use reqwest::multipart;
use tauri::State;

#[tauri::command]
pub async fn media_upload(
    state: State<'_, DaemonState>,
    session_id: String,
    filename: String,
    mime: String,
    bytes: Vec<u8>,
    label: Option<String>,
) -> Result<MediaUploadResponse, String> {
    let base = state.daemon_url.lock().expect("daemon url lock").clone();
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return Err("session_id is required".to_string());
    }
    if bytes.is_empty() {
        return Err("empty file".to_string());
    }

    let filename = filename.trim();
    let filename = if filename.is_empty() {
        "attachment".to_string()
    } else {
        filename.to_string()
    };
    let mime = mime.trim();
    let mime = if mime.is_empty() {
        "application/octet-stream".to_string()
    } else {
        mime.to_string()
    };

    let mut form = multipart::Form::new().part(
        "file",
        multipart::Part::bytes(bytes)
            .file_name(filename.clone())
            .mime_str(&mime)
            .map_err(|err| err.to_string())?,
    );
    if let Some(label) = label
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        form = form.text("label", label);
    }

    let client = daemon_http_client()?;
    let response = client
        .post(format!("{base}/v1/media/upload?session_id={session_id}"))
        .multipart(form)
        .send()
        .await
        .map_err(|err| err.to_string())?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("media upload failed ({status}): {body}"));
    }

    response
        .json::<MediaUploadResponse>()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn media_upload_path(
    state: State<'_, DaemonState>,
    session_id: String,
    path: String,
    label: Option<String>,
) -> Result<MediaUploadResponse, String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("path is required".to_string());
    }
    let bytes = std::fs::read(path).map_err(|err| err.to_string())?;
    let filename = std::path::Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("attachment")
        .to_string();
    media_upload(
        state,
        session_id,
        filename,
        guess_mime_from_path(path),
        bytes,
        label,
    )
    .await
}

fn guess_mime_from_path(path: &str) -> String {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".png") {
        "image/png".into()
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg".into()
    } else if lower.ends_with(".gif") {
        "image/gif".into()
    } else if lower.ends_with(".webp") {
        "image/webp".into()
    } else if lower.ends_with(".pdf") {
        "application/pdf".into()
    } else if lower.ends_with(".csv") {
        "text/csv".into()
    } else if lower.ends_with(".xlsx") {
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".into()
    } else if lower.ends_with(".txt") || lower.ends_with(".md") {
        "text/plain".into()
    } else {
        "application/octet-stream".into()
    }
}

pub fn media_ref_from_upload(response: &MediaUploadResponse, kind_hint: Option<&str>) -> MediaRef {
    MediaRef {
        media_id: response.media_id.clone(),
        kind: kind_hint
            .map(str::to_string)
            .unwrap_or_else(|| media_kind_from_mime(&response.mime).to_string()),
        mime: response.mime.clone(),
        label: response.label.clone(),
    }
}

fn media_kind_from_mime(mime: &str) -> &'static str {
    let mime = mime.to_ascii_lowercase();
    if mime.starts_with("image/") {
        "image"
    } else if mime.contains("spreadsheet")
        || mime.contains("excel")
        || mime == "text/csv"
        || mime == "text/tab-separated-values"
    {
        "spreadsheet"
    } else if mime.starts_with("audio/") {
        "audio"
    } else {
        "document"
    }
}
