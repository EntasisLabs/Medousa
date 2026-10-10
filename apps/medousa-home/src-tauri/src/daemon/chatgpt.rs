//! Native browser callback and authenticated workshop handoff. No OAuth codes or tokens enter JS.
use super::{DaemonState, workshop_http};
use crate::daemon::types::{
    BeginChatGptOAuthRequest, BeginChatGptOAuthResponse, CompleteChatGptOAuthRequest,
    SelectChatGptAccountRequest,
};
use crate::embedded_daemon::EmbeddedDaemonState;
use serde::Deserialize;
use serde_json::Value;
use tauri::{AppHandle, State};

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatGptOperation {
    Status,
    SignIn,
    Select,
    Refresh,
    Disconnect,
    Models,
}

#[tauri::command]
pub async fn chatgpt_oauth_request(
    app: AppHandle,
    state: State<'_, DaemonState>,
    embedded_state: State<'_, EmbeddedDaemonState>,
    operation: ChatGptOperation,
    client_id: Option<String>,
    enable_plan_usage: Option<bool>,
) -> Result<Value, String> {
    if matches!(operation, ChatGptOperation::SignIn) {
        let browser = crate::oauth_browser::OAuthBrowserSession::bind().await?;
        let request = BeginChatGptOAuthRequest {
            redirect_uri: browser.redirect_uri().to_string(),
            client_id,
            enable_plan_usage: enable_plan_usage.unwrap_or(false),
        };
        let begin: BeginChatGptOAuthResponse = serde_json::from_value(
            dispatch(
                &state,
                &embedded_state,
                "begin",
                Some(serde_json::to_value(request).map_err(|_| "encode sign-in request")?),
            )
            .await?,
        )
        .map_err(|_| "Invalid ChatGPT sign-in response")?;
        let callback = browser.authorize(&app, &begin.authorization_url).await?;
        let request = CompleteChatGptOAuthRequest {
            login_id: begin.login_id,
            callback_url: callback.url().to_string(),
        };
        let result = dispatch(
            &state,
            &embedded_state,
            "complete",
            Some(serde_json::to_value(request).map_err(|_| "encode sign-in callback")?),
        )
        .await;
        callback.finish(&app, result.is_ok()).await;
        return result;
    }
    let (action, request) = match operation {
        ChatGptOperation::Status => ("status", None),
        ChatGptOperation::Refresh => ("refresh", None),
        ChatGptOperation::Disconnect => ("disconnect", None),
        ChatGptOperation::Models => ("models", None),
        ChatGptOperation::Select => (
            "select",
            Some(
                serde_json::to_value(SelectChatGptAccountRequest {
                    client_id: client_id.ok_or("ChatGPT account is required")?,
                })
                .map_err(|_| "encode account selection")?,
            ),
        ),
        ChatGptOperation::SignIn => unreachable!(),
    };
    dispatch(&state, &embedded_state, action, request).await
}

async fn dispatch(
    state: &State<'_, DaemonState>,
    embedded_state: &EmbeddedDaemonState,
    action: &str,
    request: Option<Value>,
) -> Result<Value, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = embedded_state.client_if_active().await? {
        let value = match action {
            "status" => {
                serde_json::to_value(client.chatgpt_oauth_status().map_err(|e| e.to_string())?)
            }
            "begin" => serde_json::to_value(
                client
                    .begin_chatgpt_oauth(
                        serde_json::from_value(request.ok_or("missing request")?)
                            .map_err(|_| "invalid sign-in request")?,
                    )
                    .await
                    .map_err(|e| e.to_string())?,
            ),
            "complete" => serde_json::to_value(
                client
                    .complete_chatgpt_oauth(
                        serde_json::from_value(request.ok_or("missing request")?)
                            .map_err(|_| "invalid sign-in callback")?,
                    )
                    .await
                    .map_err(|e| e.to_string())?,
            ),
            "select" => {
                let request: SelectChatGptAccountRequest =
                    serde_json::from_value(request.ok_or("missing request")?)
                        .map_err(|_| "invalid account selection")?;
                serde_json::to_value(
                    client
                        .select_chatgpt_account(&request.client_id)
                        .await
                        .map_err(|e| e.to_string())?,
                )
            }
            "refresh" => serde_json::to_value(
                client
                    .refresh_chatgpt_oauth()
                    .await
                    .map_err(|e| e.to_string())?,
            ),
            "disconnect" => serde_json::to_value(
                client
                    .disconnect_chatgpt_oauth()
                    .await
                    .map_err(|e| e.to_string())?,
            ),
            "models" => serde_json::to_value(
                client
                    .list_chatgpt_models()
                    .await
                    .map_err(|e| e.to_string())?,
            ),
            _ => return Err("Unsupported ChatGPT operation".into()),
        };
        return value.map_err(|_| "Invalid ChatGPT response".into());
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let _ = embedded_state;
    match action {
        "status" => workshop_http::get_json(state, "/v1/auth/chatgpt").await,
        "begin" => workshop_http::post_json(state, "/v1/auth/chatgpt/begin", &request).await,
        "complete" => workshop_http::post_json(state, "/v1/auth/chatgpt/complete", &request).await,
        "select" => workshop_http::post_json(state, "/v1/auth/chatgpt/select", &request).await,
        "refresh" => workshop_http::post_empty_json(state, "/v1/auth/chatgpt/refresh").await,
        "disconnect" => workshop_http::delete_json(state, "/v1/auth/chatgpt").await,
        "models" => workshop_http::get_json(state, "/v1/auth/chatgpt/models").await,
        _ => Err("Unsupported ChatGPT operation".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn webview_cannot_submit_callback_codes_or_arbitrary_endpoints() {
        for name in ["begin", "complete", "arbitrary_path"] {
            assert!(
                serde_json::from_value::<ChatGptOperation>(Value::String(name.into())).is_err()
            );
        }
        assert!(
            serde_json::from_value::<ChatGptOperation>(Value::String("sign_in".into())).is_ok()
        );
    }
}
