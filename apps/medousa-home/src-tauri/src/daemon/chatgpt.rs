//! Closed native dispatch for the daemon-owned ChatGPT OAuth lifecycle.

use serde::Deserialize;
use serde_json::Value;
use tauri::State;

use super::{workshop_http, DaemonState};
use crate::embedded_daemon::EmbeddedDaemonState;

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
    embedded_state: State<'_, EmbeddedDaemonState>,
    operation: ChatGptOperation,
    login_id: Option<String>,
) -> Result<Value, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = embedded_state.client_if_active().await? {
        let value = match operation {
            ChatGptOperation::Status => serde_json::to_value(
                client
                    .chatgpt_oauth_status()
                    .map_err(|error| format!("read ChatGPT connection: {error:#}"))?,
            ),
            ChatGptOperation::Begin => serde_json::to_value(
                client
                    .begin_chatgpt_oauth()
                    .await
                    .map_err(|error| format!("begin ChatGPT sign-in: {error:#}"))?,
            ),
            ChatGptOperation::Complete => {
                let login_id = required_login_id(login_id.as_deref())?;
                serde_json::to_value(
                    client
                        .complete_chatgpt_oauth(login_id)
                        .await
                        .map_err(|error| format!("complete ChatGPT sign-in: {error:#}"))?,
                )
            }
            ChatGptOperation::Refresh => serde_json::to_value(
                client
                    .refresh_chatgpt_oauth()
                    .await
                    .map_err(|error| format!("refresh ChatGPT connection: {error:#}"))?,
            ),
            ChatGptOperation::Disconnect => serde_json::to_value(
                client
                    .disconnect_chatgpt_oauth()
                    .await
                    .map_err(|error| format!("disconnect ChatGPT account: {error:#}"))?,
            ),
            ChatGptOperation::Models => serde_json::to_value(
                client
                    .list_chatgpt_models()
                    .await
                    .map_err(|error| format!("list ChatGPT models: {error:#}"))?,
            ),
        };
        return value.map_err(|error| format!("encode ChatGPT response: {error}"));
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let _ = embedded_state;

    match operation {
        ChatGptOperation::Status => workshop_http::get_json(&state, "/v1/auth/chatgpt").await,
        ChatGptOperation::Begin => {
            workshop_http::post_empty_json(&state, "/v1/auth/chatgpt/begin").await
        }
        ChatGptOperation::Complete => {
            let login_id = required_login_id(login_id.as_deref())?;
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

fn required_login_id(login_id: Option<&str>) -> Result<&str, String> {
    login_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "login id is required".to_string())
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
