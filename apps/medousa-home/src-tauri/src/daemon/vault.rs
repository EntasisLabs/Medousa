use crate::daemon::types::{
    VaultBacklinksResponse, VaultNoteContentResponse, VaultNotesListResponse, VaultSearchResponse,
    VaultWriteResponse,
};
use crate::daemon::DaemonState;
use reqwest::Client;
use tauri::State;

fn encode_note_path(path: &str) -> String {
    path.split('/')
        .map(urlencoding::encode)
        .map(|segment| segment.into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn daemon_base(state: &State<'_, DaemonState>) -> Result<String, String> {
    Ok(state
        .daemon_url
        .lock()
        .expect("daemon url lock")
        .clone())
}

async fn map_http_error(response: reqwest::Response) -> String {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    format!("HTTP {status}: {body}")
}

#[tauri::command]
pub async fn vault_list_notes(
    state: State<'_, DaemonState>,
    prefix: Option<String>,
    limit: Option<usize>,
) -> Result<VaultNotesListResponse, String> {
    let base = daemon_base(&state)?;
    let client = Client::new();
    let mut url = format!("{base}/v1/vault/notes");
    let mut params = Vec::new();
    if let Some(prefix) = prefix.filter(|value| !value.trim().is_empty()) {
        params.push(format!("prefix={}", urlencoding::encode(prefix.trim())));
    }
    if let Some(limit) = limit {
        params.push(format!("limit={limit}"));
    }
    if !params.is_empty() {
        url.push('?');
        url.push_str(&params.join("&"));
    }

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(map_http_error(response).await);
    }
    response.json().await.map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn vault_get_note(
    state: State<'_, DaemonState>,
    path: String,
) -> Result<VaultNoteContentResponse, String> {
    let base = daemon_base(&state)?;
    let encoded = encode_note_path(path.trim());
    let client = Client::new();
    let response = client
        .get(format!("{base}/v1/vault/notes/{encoded}"))
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(map_http_error(response).await);
    }
    response.json().await.map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn vault_save_note(
    state: State<'_, DaemonState>,
    path: String,
    content: String,
    content_hash: Option<String>,
) -> Result<VaultWriteResponse, String> {
    let base = daemon_base(&state)?;
    let encoded = encode_note_path(path.trim());
    let client = Client::new();
    let mut request = client
        .put(format!("{base}/v1/vault/notes/{encoded}"))
        .header("content-type", "text/markdown; charset=utf-8")
        .body(content);
    if let Some(hash) = content_hash.filter(|value| !value.trim().is_empty()) {
        request = request.header("if-match", hash);
    }
    let response = request.send().await.map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(map_http_error(response).await);
    }
    response.json().await.map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn vault_create_note(
    state: State<'_, DaemonState>,
    path: String,
    content: String,
) -> Result<VaultWriteResponse, String> {
    let base = daemon_base(&state)?;
    let client = Client::new();
    let body = serde_json::json!({
        "path": path.trim(),
        "content": content,
    });
    let response = client
        .post(format!("{base}/v1/vault/notes"))
        .json(&body)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(map_http_error(response).await);
    }
    response.json().await.map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn vault_delete_note(
    state: State<'_, DaemonState>,
    path: String,
) -> Result<serde_json::Value, String> {
    let base = daemon_base(&state)?;
    let encoded = encode_note_path(path.trim());
    let client = Client::new();
    let response = client
        .delete(format!("{base}/v1/vault/notes/{encoded}"))
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(map_http_error(response).await);
    }
    response.json().await.map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn vault_search(
    state: State<'_, DaemonState>,
    query: String,
    limit: Option<usize>,
) -> Result<VaultSearchResponse, String> {
    let base = daemon_base(&state)?;
    let encoded = urlencoding::encode(query.trim());
    let limit = limit.unwrap_or(20);
    let client = Client::new();
    let response = client
        .get(format!("{base}/v1/vault/search?q={encoded}&limit={limit}"))
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(map_http_error(response).await);
    }
    response.json().await.map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn vault_backlinks(
    state: State<'_, DaemonState>,
    path: String,
) -> Result<VaultBacklinksResponse, String> {
    let base = daemon_base(&state)?;
    let encoded = urlencoding::encode(path.trim());
    let client = Client::new();
    let response = client
        .get(format!("{base}/v1/vault/backlinks?path={encoded}"))
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(map_http_error(response).await);
    }
    response.json().await.map_err(|err| err.to_string())
}
