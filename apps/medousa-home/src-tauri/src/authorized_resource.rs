//! Short-lived, trusted-shell-only delivery for bounded vault preview bytes.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{State, Webview};
use uuid::Uuid;

use crate::daemon::sdk::{client, sdk_error};
use crate::daemon::DaemonState;

const RESOURCE_TTL: Duration = Duration::from_secs(120);
const MAX_RESOURCES: usize = 64;
const MAX_RESOURCE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_ENCODED_BYTES: usize = 12 * 1024 * 1024;

struct AuthorizedResource {
    webview_label: String,
    content_type: String,
    base64: String,
    size: u64,
    expires_at: Instant,
}

#[derive(Default)]
pub struct AuthorizedResourceState {
    resources: Mutex<HashMap<String, AuthorizedResource>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorizedResourceAdmission {
    resource_id: String,
    content_type: String,
    size: u64,
    expires_in_ms: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorizedResourcePayload {
    content_type: String,
    base64: String,
    size: u64,
}

fn safe_preview_mime(content_type: &str) -> Option<&'static str> {
    match content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "image/png" => Some("image/png"),
        "image/jpeg" => Some("image/jpeg"),
        "image/gif" => Some("image/gif"),
        "image/webp" => Some("image/webp"),
        "image/avif" => Some("image/avif"),
        "image/bmp" => Some("image/bmp"),
        "image/x-icon" | "image/vnd.microsoft.icon" => Some("image/x-icon"),
        _ => None,
    }
}

fn validate_vault_relative_path(path: &str) -> Result<&str, String> {
    let path = path.trim();
    if path.is_empty() || path.len() > 4 * 1024 {
        return Err("resource path is missing or too long".to_string());
    }
    if path.starts_with('/')
        || path.starts_with('\\')
        || path.contains('\\')
        || path.contains('\0')
        || path
            .split(['/', '\\'])
            .any(|part| part.is_empty() || part == "." || part == "..")
        || path.as_bytes().get(1) == Some(&b':')
    {
        return Err("resource path must be vault-relative".to_string());
    }
    Ok(path)
}

fn encode_vault_path(path: &str) -> String {
    path.split('/')
        .map(urlencoding::encode)
        .map(|segment| segment.into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn retain_live(resources: &mut HashMap<String, AuthorizedResource>) {
    let now = Instant::now();
    resources.retain(|_, resource| resource.expires_at > now);
}

#[tauri::command]
pub async fn authorized_resource_admit(
    webview: Webview,
    state: State<'_, AuthorizedResourceState>,
    daemon: State<'_, DaemonState>,
    path: String,
    purpose: String,
) -> Result<AuthorizedResourceAdmission, String> {
    if purpose != "image-preview" {
        return Err("unsupported resource purpose".to_string());
    }
    let path = validate_vault_relative_path(&path)?;
    let file = client(&daemon)
        .vault()
        .get_file(&encode_vault_path(path))
        .await
        .map_err(sdk_error)?;
    let content_type = safe_preview_mime(&file.content_type)
        .ok_or_else(|| "resource format is not safe for inline preview".to_string())?;
    if file.size > MAX_RESOURCE_BYTES || file.base64.len() > MAX_ENCODED_BYTES {
        return Err("resource exceeds inline preview limit".to_string());
    }

    let resource_id = Uuid::new_v4().simple().to_string();
    let mut resources = state
        .resources
        .lock()
        .expect("authorized resource registry");
    retain_live(&mut resources);
    if resources.len() >= MAX_RESOURCES {
        return Err("too many authorized previews are pending".to_string());
    }
    resources.insert(
        resource_id.clone(),
        AuthorizedResource {
            webview_label: webview.label().to_string(),
            content_type: content_type.to_string(),
            base64: file.base64,
            size: file.size,
            expires_at: Instant::now() + RESOURCE_TTL,
        },
    );
    Ok(AuthorizedResourceAdmission {
        resource_id,
        content_type: content_type.to_string(),
        size: file.size,
        expires_in_ms: RESOURCE_TTL.as_millis() as u64,
    })
}

#[tauri::command]
pub fn authorized_resource_read(
    webview: Webview,
    state: State<'_, AuthorizedResourceState>,
    resource_id: String,
) -> Result<AuthorizedResourcePayload, String> {
    if resource_id.len() != 32 || !resource_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("invalid resource id".to_string());
    }
    let mut resources = state
        .resources
        .lock()
        .expect("authorized resource registry");
    retain_live(&mut resources);
    let resource = resources
        .get(&resource_id)
        .ok_or_else(|| "resource is unavailable or expired".to_string())?;
    if resource.webview_label != webview.label() {
        return Err("resource belongs to another webview".to_string());
    }
    let resource = resources
        .remove(&resource_id)
        .expect("authorized resource remained present");
    Ok(AuthorizedResourcePayload {
        content_type: resource.content_type,
        base64: resource.base64,
        size: resource.size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_paths_are_strictly_vault_relative() {
        assert_eq!(
            validate_vault_relative_path("images/cat.png").unwrap(),
            "images/cat.png"
        );
        for path in [
            "",
            "/etc/passwd",
            "../secret",
            "images/../secret",
            r"images\secret",
            r"C:\secret",
        ] {
            assert!(
                validate_vault_relative_path(path).is_err(),
                "accepted {path}"
            );
        }
    }

    #[test]
    fn active_formats_are_not_inline_preview_resources() {
        assert_eq!(
            safe_preview_mime("image/png; charset=binary"),
            Some("image/png")
        );
        assert_eq!(safe_preview_mime("image/svg+xml"), None);
        assert_eq!(safe_preview_mime("text/html"), None);
        assert_eq!(safe_preview_mime("application/pdf"), None);
    }
}
