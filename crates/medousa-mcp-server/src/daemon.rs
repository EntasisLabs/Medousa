//! Thin HTTP client to the local Medousa daemon vault APIs.

use reqwest::blocking::Client;
use serde_json::{Value, json};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct DaemonClient {
    base_url: String,
    bearer: Option<String>,
    http: Client,
}

impl DaemonClient {
    pub fn from_env() -> Result<Self, String> {
        let base_url = std::env::var("MEDOUSA_DAEMON_URL")
            .or_else(|_| std::env::var("MEDOUSA_URL"))
            .unwrap_or_else(|_| "http://127.0.0.1:7419".to_string())
            .trim()
            .trim_end_matches('/')
            .to_string();
        if base_url.is_empty() {
            return Err("MEDOUSA_DAEMON_URL is empty".to_string());
        }
        let bearer = std::env::var("MEDOUSA_SESSION_TOKEN")
            .or_else(|_| std::env::var("MEDOUSA_BEARER_TOKEN"))
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|err| err.to_string())?;
        Ok(Self {
            base_url,
            bearer,
            http,
        })
    }

    fn get_json(&self, path: &str) -> Result<Value, String> {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.http.get(&url);
        if let Some(token) = &self.bearer {
            req = req.bearer_auth(token);
        }
        let response = req
            .send()
            .map_err(|err| format!("daemon GET {path} failed ({url}): {err}"))?;
        let status = response.status();
        let text = response.text().unwrap_or_default();
        if !status.is_success() {
            return Err(format!(
                "daemon GET {path} HTTP {status}: {}",
                text.chars().take(400).collect::<String>()
            ));
        }
        if text.trim().is_empty() {
            return Ok(json!({}));
        }
        serde_json::from_str(&text).map_err(|err| format!("daemon JSON parse: {err}"))
    }

    pub fn vault_list(&self, prefix: Option<&str>) -> Result<Value, String> {
        let mut path = "/v1/vault/notes?limit=200".to_string();
        if let Some(prefix) = prefix.map(str::trim).filter(|value| !value.is_empty()) {
            path.push_str(&format!("&prefix={}", urlencoding_path(prefix)));
        }
        self.get_json(&path)
    }

    pub fn vault_read(&self, note_path: &str) -> Result<Value, String> {
        let trimmed = note_path.trim().trim_start_matches('/');
        if trimmed.is_empty() {
            return Err("path is required".to_string());
        }
        let path = format!("/v1/vault/notes/{}", urlencoding_path(trimmed));
        self.get_json(&path)
    }

    pub fn vault_search(&self, query: &str) -> Result<Value, String> {
        let q = query.trim();
        if q.is_empty() {
            return Err("query is required".to_string());
        }
        let path = format!("/v1/vault/search?q={}&limit=50", urlencoding_path(q));
        self.get_json(&path)
    }

    pub fn calendar_list(&self, from: Option<&str>, to: Option<&str>) -> Result<Value, String> {
        let mut path = "/v1/calendar/events?".to_string();
        let mut parts = Vec::new();
        if let Some(from) = from.map(str::trim).filter(|v| !v.is_empty()) {
            parts.push(format!("from={}", urlencoding_path(from)));
        }
        if let Some(to) = to.map(str::trim).filter(|v| !v.is_empty()) {
            parts.push(format!("to={}", urlencoding_path(to)));
        }
        path.push_str(&parts.join("&"));
        self.get_json(&path)
    }

    pub fn artifacts_list(
        &self,
        session_id: Option<&str>,
        limit: Option<usize>,
        query: Option<&str>,
    ) -> Result<Value, String> {
        let body = json!({
            "session_id": session_id.map(str::trim).filter(|v| !v.is_empty()),
            "limit": limit.unwrap_or(50),
            "query": query.map(str::trim).filter(|v| !v.is_empty()),
        });
        self.post_json("/v1/runtime/artifact/list-ui", &body)
    }

    pub fn artifacts_fetch(
        &self,
        artifact_id: &str,
        session_id: Option<&str>,
    ) -> Result<Value, String> {
        let id = artifact_id.trim();
        if id.is_empty() {
            return Err("id is required".to_string());
        }
        let session = match session_id.map(str::trim).filter(|v| !v.is_empty()) {
            Some(s) => s.to_string(),
            None => {
                let listed = self.artifacts_list(None, Some(200), None)?;
                listed
                    .get("artifacts")
                    .and_then(|v| v.as_array())
                    .into_iter()
                    .flatten()
                    .find(|row| {
                        row.get("artifact_id")
                            .and_then(|v| v.as_str())
                            .is_some_and(|aid| aid == id)
                    })
                    .and_then(|row| {
                        row.get("session_id")
                            .and_then(|v| v.as_str())
                            .map(str::to_string)
                    })
                    .ok_or_else(|| {
                        format!("artifact '{id}' not found — pass session_id if known")
                    })?
            }
        };
        let body = json!({
            "session_id": session,
            "artifact_id": id,
        });
        self.post_json("/v1/runtime/artifact/fetch", &body)
    }

    fn post_json(&self, path: &str, body: &Value) -> Result<Value, String> {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.http.post(&url).json(body);
        if let Some(token) = &self.bearer {
            req = req.bearer_auth(token);
        }
        let response = req
            .send()
            .map_err(|err| format!("daemon POST {path} failed ({url}): {err}"))?;
        let status = response.status();
        let text = response.text().unwrap_or_default();
        if !status.is_success() {
            return Err(format!(
                "daemon POST {path} HTTP {status}: {}",
                text.chars().take(400).collect::<String>()
            ));
        }
        if text.trim().is_empty() {
            return Ok(json!({}));
        }
        serde_json::from_str(&text).map_err(|err| format!("daemon JSON parse: {err}"))
    }
}

/// Minimal query/path encoding (enough for vault note paths + search).
fn urlencoding_path(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for byte in raw.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::urlencoding_path;

    #[test]
    fn encodes_spaces_keeps_slashes() {
        assert_eq!(urlencoding_path("inbox/hi there.md"), "inbox/hi%20there.md");
    }
}
