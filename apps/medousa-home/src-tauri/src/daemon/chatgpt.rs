//! Closed native dispatch for the daemon-owned ChatGPT OAuth lifecycle.

use serde::Deserialize;
use serde_json::Value;
use tauri::State;

use super::{DaemonState, workshop_http};

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatGptOperation {
    Status,
    Begin,
    Complete,
    Refresh,
    Disconnect,
    Models,
}

#[tauri::command]
pub async fn chatgpt_oauth_request(
    state: State<'_, DaemonState>,
    operation: ChatGptOperation,
    login_id: Option<String>,
) -> Result<Value, String> {
    match operation {
        ChatGptOperation::Status => workshop_http::get_json(&state, "/v1/auth/chatgpt").await,
        ChatGptOperation::Begin => {
            workshop_http::post_empty_json(&state, "/v1/auth/chatgpt/begin").await
        }
        ChatGptOperation::Complete => {
            let login_id = login_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "login id is required".to_string())?;
            workshop_http::post_json(
                &state,
                "/v1/auth/chatgpt/complete",
                &crate::daemon::types::CompleteChatGptOAuthRequest {
                    login_id: login_id.to_string(),
                },
            )
            .await
        }
        ChatGptOperation::Refresh => {
            workshop_http::post_empty_json(&state, "/v1/auth/chatgpt/refresh").await
        }
        ChatGptOperation::Disconnect => {
            workshop_http::delete_json(&state, "/v1/auth/chatgpt").await
        }
        ChatGptOperation::Models => {
            workshop_http::get_json(&state, "/v1/auth/chatgpt/models").await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_decoder_rejects_arbitrary_endpoint_names() {
        assert!(serde_json::from_str::<ChatGptOperation>(r#""status""#).is_ok());
        assert!(serde_json::from_str::<ChatGptOperation>(r#""arbitrary_path""#).is_err());
    }
}
