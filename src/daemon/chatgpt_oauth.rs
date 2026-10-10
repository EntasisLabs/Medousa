//! Native-only daemon HTTP surface for ChatGPT sign-in and account metadata.

use axum::Json;
use axum::http::StatusCode;

use crate::chatgpt_oauth::OAuthError;
use crate::daemon_api::{
    BeginChatGptOAuthRequest, BeginChatGptOAuthResponse, ChatGptModelListResponse,
    ChatGptOAuthStatusResponse, CompleteChatGptOAuthRequest, CompleteChatGptOAuthResponse,
    DisconnectChatGptOAuthResponse, SelectChatGptAccountRequest,
};

type ApiError = (StatusCode, String);

pub async fn status() -> Json<ChatGptOAuthStatusResponse> {
    Json(crate::chatgpt_oauth::status())
}

pub async fn begin(
    Json(request): Json<BeginChatGptOAuthRequest>,
) -> Result<Json<BeginChatGptOAuthResponse>, ApiError> {
    crate::chatgpt_oauth::begin(request)
        .await
        .map(Json)
        .map_err(api_error)
}

pub async fn complete(
    Json(request): Json<CompleteChatGptOAuthRequest>,
) -> Result<Json<CompleteChatGptOAuthResponse>, ApiError> {
    crate::chatgpt_oauth::complete(request)
        .await
        .map(Json)
        .map_err(api_error)
}

pub async fn refresh() -> Result<Json<ChatGptOAuthStatusResponse>, ApiError> {
    crate::chatgpt_oauth::refresh()
        .await
        .map(Json)
        .map_err(api_error)
}

pub async fn disconnect() -> Result<Json<DisconnectChatGptOAuthResponse>, ApiError> {
    crate::chatgpt_oauth::disconnect()
        .await
        .map(Json)
        .map_err(api_error)
}

pub async fn models() -> Result<Json<ChatGptModelListResponse>, ApiError> {
    crate::chatgpt_oauth::list_models()
        .await
        .map(Json)
        .map_err(api_error)
}

pub async fn select(
    Json(request): Json<SelectChatGptAccountRequest>,
) -> Result<Json<ChatGptOAuthStatusResponse>, ApiError> {
    crate::chatgpt_oauth::select(&request.client_id)
        .await
        .map(Json)
        .map_err(api_error)
}

fn api_error(error: OAuthError) -> ApiError {
    let status = match error {
        OAuthError::LoginNotFound => StatusCode::NOT_FOUND,
        OAuthError::LoginExpired
        | OAuthError::PkceValidationFailed
        | OAuthError::InvalidAuthorizationResponse
        | OAuthError::AccountIdentityMissing
        | OAuthError::TokenExpiryMissing => StatusCode::BAD_REQUEST,
        OAuthError::NotConnected
        | OAuthError::ReauthenticationRequired
        | OAuthError::PlanUsageDisabled => StatusCode::UNAUTHORIZED,
        OAuthError::AuthorizationUnavailable(_)
        | OAuthError::AuthorizationFailed(_)
        | OAuthError::TokenExchangeFailed(_)
        | OAuthError::ModelCatalogUnavailable(_)
        | OAuthError::InvalidModelCatalogResponse
        | OAuthError::Transport => StatusCode::BAD_GATEWAY,
        OAuthError::CredentialStorage | OAuthError::StoredCredentialsInvalid => {
            StatusCode::INTERNAL_SERVER_ERROR
        }
    };
    (status, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oauth_errors_expose_no_secret_material() {
        for error in [
            OAuthError::Transport,
            OAuthError::TokenExchangeFailed(401),
            OAuthError::StoredCredentialsInvalid,
        ] {
            let (_, message) = api_error(error);
            assert!(!message.contains("token="));
            assert!(!message.contains("Bearer"));
        }
    }
}
