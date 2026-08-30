//! Daemon-owned ChatGPT OAuth credentials for native Medousa inference.
//!
//! This store is intentionally separate from both provider API keys and the
//! Codex CLI credential store. Public status objects never contain tokens.

use std::collections::HashMap;
#[cfg(feature = "full-daemon")]
use std::sync::OnceLock;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use base64::Engine;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use crate::daemon_api::{
    BeginChatGptOAuthResponse, ChatGptModelListResponse, ChatGptOAuthStatusResponse,
    CompleteChatGptOAuthResponse, DisconnectChatGptOAuthResponse,
};
use crate::openai_codex_chat_client::{
    CODEX_COMPAT_ORIGINATOR, CODEX_COMPAT_VERSION, codex_compat_user_agent,
};

const DEFAULT_ISSUER: &str = "https://auth.openai.com";
const DEFAULT_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
#[cfg(feature = "full-daemon")]
const CREDENTIAL_SERVICE: &str = "medousa.chatgpt";
#[cfg(feature = "full-daemon")]
const CREDENTIAL_ACCOUNT: &str = "native_oauth";
const DEVICE_CODE_LIFETIME_MINUTES: i64 = 15;
const REFRESH_WINDOW_MINUTES: i64 = 5;
const DEFAULT_MODELS_URL: &str = "https://chatgpt.com/backend-api/codex/models";

#[derive(Clone)]
struct OAuthConfig {
    issuer: String,
    client_id: String,
}

impl OAuthConfig {
    fn from_env() -> Self {
        Self {
            issuer: std::env::var("MEDOUSA_CHATGPT_OAUTH_ISSUER")
                .ok()
                .map(|value| value.trim_end_matches('/').to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| DEFAULT_ISSUER.to_string()),
            client_id: std::env::var("MEDOUSA_CHATGPT_OAUTH_CLIENT_ID")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| DEFAULT_CLIENT_ID.to_string()),
        }
    }

    fn user_code_url(&self) -> String {
        format!("{}/api/accounts/deviceauth/usercode", self.issuer)
    }

    fn device_token_url(&self) -> String {
        format!("{}/api/accounts/deviceauth/token", self.issuer)
    }

    fn token_url(&self) -> String {
        format!("{}/oauth/token", self.issuer)
    }

    fn revoke_url(&self) -> String {
        format!("{}/oauth/revoke", self.issuer)
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct CredentialEnvelope {
    access_token: String,
    refresh_token: String,
    id_token: String,
    account_id: String,
    expires_at_utc: DateTime<Utc>,
    #[serde(default)]
    reauth_required: bool,
}

impl std::fmt::Debug for CredentialEnvelope {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CredentialEnvelope")
            .field("access_token", &"<redacted>")
            .field("refresh_token", &"<redacted>")
            .field("id_token", &"<redacted>")
            .field("account_id", &self.account_id)
            .field("expires_at_utc", &self.expires_at_utc)
            .field("reauth_required", &self.reauth_required)
            .finish()
    }
}

/// Host binding for the encrypted ChatGPT credential bundle.
///
/// The OAuth broker owns the bundle schema and token lifecycle. Deployment
/// hosts only persist the opaque serialized value in their existing secret
/// authority (daemon secrets on full hosts, Keychain-backed secrets on iOS).
pub trait ChatGptCredentialStore: Send + Sync {
    fn load_bundle(&self) -> Result<Option<String>, String>;
    fn save_bundle(&self, bundle: Option<&str>) -> Result<(), String>;
}

#[cfg(feature = "full-daemon")]
struct DaemonCredentialStore;

#[cfg(feature = "full-daemon")]
impl DaemonCredentialStore {
    fn load_raw() -> Option<String> {
        crate::integration_connection::load_kind_secret(
            "chatgpt",
            medousa_types::secrets::IntegrationSecretSlot::OauthBundle,
        )
    }
}

#[cfg(feature = "full-daemon")]
impl ChatGptCredentialStore for DaemonCredentialStore {
    fn load_bundle(&self) -> Result<Option<String>, String> {
        Ok(Self::load_raw())
    }

    fn save_bundle(&self, bundle: Option<&str>) -> Result<(), String> {
        crate::integration_connection::save_kind_secret(
            "chatgpt",
            medousa_types::secrets::IntegrationSecretSlot::OauthBundle,
            bundle,
        );
        let _ = keyring::Entry::new(CREDENTIAL_SERVICE, CREDENTIAL_ACCOUNT)
            .ok()
            .map(|entry| entry.delete_password());
        let legacy = crate::session::medousa_data_dir()
            .join("secrets")
            .join("chatgpt_oauth.json");
        let _ = std::fs::remove_file(legacy);
        Ok(())
    }
}

#[derive(Clone)]
struct PendingDeviceLogin {
    device_auth_id: String,
    user_code: String,
    expires_at_utc: DateTime<Utc>,
    next_poll_at_utc: DateTime<Utc>,
    poll_interval_seconds: u64,
}

pub struct ChatGptOAuthBroker {
    client: reqwest::Client,
    config: OAuthConfig,
    store: Arc<dyn ChatGptCredentialStore>,
    cached: RwLock<Option<CredentialEnvelope>>,
    pending: Mutex<HashMap<String, PendingDeviceLogin>>,
    refresh_lock: Mutex<()>,
}

impl ChatGptOAuthBroker {
    /// Bind the canonical OAuth lifecycle to a deployment's secret authority.
    pub fn new(store: Arc<dyn ChatGptCredentialStore>) -> Self {
        Self::with_config(OAuthConfig::from_env(), store)
    }

    fn with_config(config: OAuthConfig, store: Arc<dyn ChatGptCredentialStore>) -> Self {
        let cached = store
            .load_bundle()
            .ok()
            .flatten()
            .and_then(|value| serde_json::from_str(&value).ok());
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            client,
            config,
            store,
            cached: RwLock::new(cached),
            pending: Mutex::new(HashMap::new()),
            refresh_lock: Mutex::new(()),
        }
    }

    pub fn status(&self) -> ChatGptOAuthStatusResponse {
        let credentials = self.cached.read().expect("ChatGPT credential cache lock");
        status_for(credentials.as_ref(), Utc::now())
    }

    pub async fn begin(&self) -> Result<BeginChatGptOAuthResponse, OAuthError> {
        #[derive(Serialize)]
        struct Request<'a> {
            client_id: &'a str,
        }
        #[derive(Deserialize)]
        struct Response {
            device_auth_id: String,
            #[serde(alias = "usercode")]
            user_code: String,
            #[serde(default, deserialize_with = "deserialize_u64_string_or_number")]
            interval: u64,
        }

        let response = self
            .client
            .post(self.config.user_code_url())
            .json(&Request {
                client_id: &self.config.client_id,
            })
            .send()
            .await
            .map_err(|_| OAuthError::Transport)?;
        if !response.status().is_success() {
            return Err(OAuthError::AuthorizationUnavailable(
                response.status().as_u16(),
            ));
        }
        let response: Response = response
            .json()
            .await
            .map_err(|_| OAuthError::InvalidAuthorizationResponse)?;
        if response.device_auth_id.trim().is_empty() || response.user_code.trim().is_empty() {
            return Err(OAuthError::InvalidAuthorizationResponse);
        }

        let now = Utc::now();
        let expires_at_utc = now + ChronoDuration::minutes(DEVICE_CODE_LIFETIME_MINUTES);
        let poll_interval_seconds = response.interval.clamp(1, 30);
        let login_id = uuid::Uuid::new_v4().to_string();
        self.pending.lock().await.insert(
            login_id.clone(),
            PendingDeviceLogin {
                device_auth_id: response.device_auth_id,
                user_code: response.user_code.clone(),
                expires_at_utc,
                next_poll_at_utc: now,
                poll_interval_seconds,
            },
        );
        Ok(BeginChatGptOAuthResponse {
            login_id,
            verification_url: format!("{}/codex/device", self.config.issuer),
            user_code: response.user_code,
            expires_at_utc,
            poll_interval_seconds,
        })
    }

    pub async fn complete(
        &self,
        login_id: &str,
    ) -> Result<CompleteChatGptOAuthResponse, OAuthError> {
        let pending = {
            let mut pending_logins = self.pending.lock().await;
            let Some(login) = pending_logins.get_mut(login_id) else {
                return Err(OAuthError::LoginNotFound);
            };
            let now = Utc::now();
            if now >= login.expires_at_utc {
                pending_logins.remove(login_id);
                return Err(OAuthError::LoginExpired);
            }
            if now < login.next_poll_at_utc {
                let retry_after_seconds =
                    (login.next_poll_at_utc - now).num_seconds().max(1) as u64;
                return Ok(CompleteChatGptOAuthResponse {
                    status: "pending".to_string(),
                    retry_after_seconds: Some(retry_after_seconds),
                    connection: None,
                });
            }
            login.next_poll_at_utc =
                now + ChronoDuration::seconds(login.poll_interval_seconds as i64);
            login.clone()
        };

        #[derive(Serialize)]
        struct PollRequest<'a> {
            device_auth_id: &'a str,
            user_code: &'a str,
        }
        #[derive(Deserialize)]
        struct PollResponse {
            authorization_code: String,
            code_challenge: String,
            code_verifier: String,
        }

        let response = self
            .client
            .post(self.config.device_token_url())
            .json(&PollRequest {
                device_auth_id: &pending.device_auth_id,
                user_code: &pending.user_code,
            })
            .send()
            .await
            .map_err(|_| OAuthError::Transport)?;
        if matches!(response.status().as_u16(), 403 | 404) {
            return Ok(CompleteChatGptOAuthResponse {
                status: "pending".to_string(),
                retry_after_seconds: Some(pending.poll_interval_seconds),
                connection: None,
            });
        }
        if !response.status().is_success() {
            return Err(OAuthError::AuthorizationFailed(response.status().as_u16()));
        }
        let code: PollResponse = response
            .json()
            .await
            .map_err(|_| OAuthError::InvalidAuthorizationResponse)?;
        validate_pkce(&code.code_verifier, &code.code_challenge)?;
        let tokens = self
            .exchange_authorization_code(&code.authorization_code, &code.code_verifier)
            .await?;
        let credentials = credentials_from_tokens(tokens, None)?;
        self.persist(credentials)?;
        self.pending.lock().await.remove(login_id);

        Ok(CompleteChatGptOAuthResponse {
            status: "connected".to_string(),
            retry_after_seconds: None,
            connection: Some(self.status()),
        })
    }

    async fn exchange_authorization_code(
        &self,
        code: &str,
        code_verifier: &str,
    ) -> Result<TokenResponse, OAuthError> {
        let redirect_uri = format!("{}/deviceauth/callback", self.config.issuer);
        let response = self
            .client
            .post(self.config.token_url())
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", redirect_uri.as_str()),
                ("client_id", self.config.client_id.as_str()),
                ("code_verifier", code_verifier),
            ])
            .send()
            .await
            .map_err(|_| OAuthError::Transport)?;
        parse_token_response(response).await
    }

    pub async fn refresh(&self) -> Result<ChatGptOAuthStatusResponse, OAuthError> {
        let _guard = self.refresh_lock.lock().await;
        let current = self
            .cached
            .read()
            .expect("ChatGPT credential cache lock")
            .clone()
            .ok_or(OAuthError::NotConnected)?;
        self.refresh_locked(current).await?;
        Ok(self.status())
    }

    /// Returns request credentials, refreshing once when expiry is within five
    /// minutes. Phase 3's transport consumes this method without owning tokens.
    pub(crate) async fn credentials_for_request(&self) -> Result<(String, String), OAuthError> {
        let snapshot = self
            .cached
            .read()
            .expect("ChatGPT credential cache lock")
            .clone()
            .ok_or(OAuthError::NotConnected)?;
        if snapshot.reauth_required {
            return Err(OAuthError::ReauthenticationRequired);
        }
        if !expires_within(&snapshot, Utc::now(), REFRESH_WINDOW_MINUTES) {
            return Ok((snapshot.access_token, snapshot.account_id));
        }

        let original_access_token = snapshot.access_token.clone();
        let _guard = self.refresh_lock.lock().await;
        let current = self
            .cached
            .read()
            .expect("ChatGPT credential cache lock")
            .clone()
            .ok_or(OAuthError::NotConnected)?;
        if current.access_token != original_access_token
            && !expires_within(&current, Utc::now(), REFRESH_WINDOW_MINUTES)
        {
            return Ok((current.access_token, current.account_id));
        }
        let refreshed = self.refresh_locked(current).await?;
        Ok((refreshed.access_token, refreshed.account_id))
    }

    /// Refreshes after one upstream authentication failure, but only if the
    /// failing token is still current. Concurrent 401s therefore share one
    /// refresh and one persisted token rotation.
    pub(crate) async fn refresh_after_unauthorized(
        &self,
        rejected_access_token: &str,
    ) -> Result<(String, String), OAuthError> {
        let _guard = self.refresh_lock.lock().await;
        let current = self
            .cached
            .read()
            .expect("ChatGPT credential cache lock")
            .clone()
            .ok_or(OAuthError::NotConnected)?;
        if current.access_token != rejected_access_token {
            return Ok((current.access_token, current.account_id));
        }
        let refreshed = self.refresh_locked(current).await?;
        Ok((refreshed.access_token, refreshed.account_id))
    }

    async fn refresh_locked(
        &self,
        current: CredentialEnvelope,
    ) -> Result<CredentialEnvelope, OAuthError> {
        let response = self
            .client
            .post(self.config.token_url())
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", current.refresh_token.as_str()),
                ("client_id", self.config.client_id.as_str()),
            ])
            .send()
            .await
            .map_err(|_| OAuthError::Transport)?;
        if matches!(response.status().as_u16(), 400 | 401) {
            let mut invalid = current;
            invalid.reauth_required = true;
            self.persist(invalid)?;
            return Err(OAuthError::ReauthenticationRequired);
        }
        let tokens = parse_token_response(response).await?;
        let refreshed = credentials_from_tokens(tokens, Some(&current))?;
        self.persist(refreshed.clone())?;
        Ok(refreshed)
    }

    pub async fn disconnect(&self) -> Result<DisconnectChatGptOAuthResponse, OAuthError> {
        let credentials = self
            .cached
            .read()
            .expect("ChatGPT credential cache lock")
            .clone();
        let revoked = if let Some(credentials) = credentials.as_ref() {
            self.client
                .post(self.config.revoke_url())
                .json(&serde_json::json!({
                    "token": credentials.refresh_token,
                    "token_type_hint": "refresh_token",
                    "client_id": self.config.client_id,
                }))
                .send()
                .await
                .map(|response| response.status().is_success())
                .unwrap_or(false)
        } else {
            false
        };
        self.store
            .save_bundle(None)
            .map_err(|_| OAuthError::CredentialStorage)?;
        *self.cached.write().expect("ChatGPT credential cache lock") = None;
        self.pending.lock().await.clear();
        Ok(DisconnectChatGptOAuthResponse {
            disconnected: true,
            revoked,
        })
    }

    pub async fn list_models(&self) -> Result<ChatGptModelListResponse, OAuthError> {
        let url = std::env::var("MEDOUSA_CHATGPT_MODELS_URL")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_MODELS_URL.to_string());
        self.list_models_from_url(&url).await
    }

    async fn list_models_from_url(
        &self,
        url: &str,
    ) -> Result<ChatGptModelListResponse, OAuthError> {
        let credentials = self.credentials_for_request().await?;
        let response = self
            .request_models_once(url, &credentials.0, &credentials.1)
            .await?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            let refreshed = self.refresh_after_unauthorized(&credentials.0).await?;
            return self
                .parse_models_response(
                    self.request_models_once(url, &refreshed.0, &refreshed.1)
                        .await?,
                )
                .await;
        }
        self.parse_models_response(response).await
    }

    async fn request_models_once(
        &self,
        url: &str,
        access_token: &str,
        account_id: &str,
    ) -> Result<reqwest::Response, OAuthError> {
        self.client
            .get(url)
            .query(&[("client_version", CODEX_COMPAT_VERSION)])
            .bearer_auth(access_token)
            .header("ChatGPT-Account-ID", account_id)
            .header("Originator", CODEX_COMPAT_ORIGINATOR)
            .header("User-Agent", codex_compat_user_agent())
            .header("Version", CODEX_COMPAT_VERSION)
            .send()
            .await
            .map_err(|_| OAuthError::Transport)
    }

    async fn parse_models_response(
        &self,
        response: reqwest::Response,
    ) -> Result<ChatGptModelListResponse, OAuthError> {
        #[derive(Deserialize)]
        struct ModelsResponse {
            models: Vec<ModelInfo>,
        }
        #[derive(Deserialize)]
        struct ModelInfo {
            slug: String,
            #[serde(default)]
            visibility: String,
            #[serde(default)]
            priority: i32,
        }

        if !response.status().is_success() {
            return Err(OAuthError::ModelCatalogUnavailable(
                response.status().as_u16(),
            ));
        }
        let mut models = response
            .json::<ModelsResponse>()
            .await
            .map_err(|_| OAuthError::InvalidModelCatalogResponse)?
            .models;
        models.retain(|model| model.visibility.is_empty() || model.visibility == "list");
        models.sort_by(|left, right| {
            right
                .priority
                .cmp(&left.priority)
                .then_with(|| left.slug.cmp(&right.slug))
        });
        models.dedup_by(|left, right| left.slug == right.slug);
        Ok(ChatGptModelListResponse {
            models: models
                .into_iter()
                .map(|model| model.slug.trim().to_string())
                .filter(|slug| !slug.is_empty())
                .collect(),
        })
    }

    fn persist(&self, credentials: CredentialEnvelope) -> Result<(), OAuthError> {
        let serialized = serde_json::to_string(&credentials)
            .map_err(|_| OAuthError::StoredCredentialsInvalid)?;
        self.store
            .save_bundle(Some(&serialized))
            .map_err(|_| OAuthError::CredentialStorage)?;
        *self.cached.write().expect("ChatGPT credential cache lock") = Some(credentials);
        Ok(())
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    id_token: Option<String>,
    expires_in: Option<i64>,
}

async fn parse_token_response(response: reqwest::Response) -> Result<TokenResponse, OAuthError> {
    if !response.status().is_success() {
        return Err(OAuthError::TokenExchangeFailed(response.status().as_u16()));
    }
    response
        .json()
        .await
        .map_err(|_| OAuthError::InvalidAuthorizationResponse)
}

fn credentials_from_tokens(
    tokens: TokenResponse,
    previous: Option<&CredentialEnvelope>,
) -> Result<CredentialEnvelope, OAuthError> {
    if tokens.access_token.trim().is_empty() {
        return Err(OAuthError::InvalidAuthorizationResponse);
    }
    let id_token = tokens
        .id_token
        .filter(|value| !value.trim().is_empty())
        .or_else(|| previous.map(|value| value.id_token.clone()))
        .ok_or(OAuthError::InvalidAuthorizationResponse)?;
    let refresh_token = tokens
        .refresh_token
        .filter(|value| !value.trim().is_empty())
        .or_else(|| previous.map(|value| value.refresh_token.clone()))
        .ok_or(OAuthError::InvalidAuthorizationResponse)?;
    let account_id = jwt_string_claim(&id_token, "chatgpt_account_id")
        .or_else(|| jwt_string_claim(&tokens.access_token, "chatgpt_account_id"))
        .or_else(|| previous.map(|value| value.account_id.clone()))
        .ok_or(OAuthError::AccountIdentityMissing)?;
    let expires_at_utc = jwt_i64_claim(&tokens.access_token, "exp")
        .and_then(|value| DateTime::from_timestamp(value, 0))
        .or_else(|| {
            tokens
                .expires_in
                .map(|seconds| Utc::now() + ChronoDuration::seconds(seconds))
        })
        .ok_or(OAuthError::TokenExpiryMissing)?;
    Ok(CredentialEnvelope {
        access_token: tokens.access_token,
        refresh_token,
        id_token,
        account_id,
        expires_at_utc,
        reauth_required: false,
    })
}

fn status_for(
    credentials: Option<&CredentialEnvelope>,
    now: DateTime<Utc>,
) -> ChatGptOAuthStatusResponse {
    let Some(credentials) = credentials else {
        return ChatGptOAuthStatusResponse {
            status: "signed_out".to_string(),
            connected: false,
            account_id: None,
            expires_at_utc: None,
        };
    };
    let status = if credentials.reauth_required {
        "reauth_required"
    } else if expires_within(credentials, now, REFRESH_WINDOW_MINUTES) {
        "refresh_required"
    } else {
        "connected"
    };
    ChatGptOAuthStatusResponse {
        status: status.to_string(),
        connected: !credentials.reauth_required,
        account_id: Some(credentials.account_id.clone()),
        expires_at_utc: Some(credentials.expires_at_utc),
    }
}

fn expires_within(credentials: &CredentialEnvelope, now: DateTime<Utc>, minutes: i64) -> bool {
    credentials.expires_at_utc <= now + ChronoDuration::minutes(minutes)
}

fn validate_pkce(verifier: &str, expected_challenge: &str) -> Result<(), OAuthError> {
    if verifier.is_empty() || expected_challenge.is_empty() {
        return Err(OAuthError::PkceValidationFailed);
    }
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(Sha256::digest(verifier.as_bytes()));
    if challenge == expected_challenge {
        Ok(())
    } else {
        Err(OAuthError::PkceValidationFailed)
    }
}

fn jwt_payload(token: &str) -> Option<serde_json::Value> {
    let payload = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn jwt_string_claim(token: &str, name: &str) -> Option<String> {
    let payload = jwt_payload(token)?;
    payload
        .get(name)
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            payload
                .get("https://api.openai.com/auth")
                .and_then(|auth| auth.get(name))
                .and_then(serde_json::Value::as_str)
        })
        .map(str::to_string)
}

fn jwt_i64_claim(token: &str, name: &str) -> Option<i64> {
    jwt_payload(token)?.get(name)?.as_i64()
}

fn deserialize_u64_string_or_number<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Value {
        Number(u64),
        String(String),
    }
    match Value::deserialize(deserializer)? {
        Value::Number(value) => Ok(value),
        Value::String(value) => value.trim().parse().map_err(serde::de::Error::custom),
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum OAuthError {
    NotConnected,
    ReauthenticationRequired,
    AuthorizationUnavailable(u16),
    AuthorizationFailed(u16),
    TokenExchangeFailed(u16),
    InvalidAuthorizationResponse,
    PkceValidationFailed,
    LoginExpired,
    LoginNotFound,
    AccountIdentityMissing,
    TokenExpiryMissing,
    CredentialStorage,
    StoredCredentialsInvalid,
    ModelCatalogUnavailable(u16),
    InvalidModelCatalogResponse,
    Transport,
}

impl std::fmt::Display for OAuthError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::NotConnected => "ChatGPT account is not connected".to_string(),
            Self::ReauthenticationRequired => "ChatGPT sign-in must be completed again".to_string(),
            Self::AuthorizationUnavailable(status) => {
                format!("ChatGPT sign-in is not available (HTTP {status})")
            }
            Self::AuthorizationFailed(status) => {
                format!("ChatGPT sign-in failed (HTTP {status})")
            }
            Self::TokenExchangeFailed(status) => {
                format!("ChatGPT token exchange failed (HTTP {status})")
            }
            Self::InvalidAuthorizationResponse => {
                "ChatGPT sign-in response was invalid".to_string()
            }
            Self::PkceValidationFailed => "ChatGPT sign-in PKCE validation failed".to_string(),
            Self::LoginExpired => "ChatGPT sign-in expired; start again".to_string(),
            Self::LoginNotFound => "ChatGPT sign-in was not found; start again".to_string(),
            Self::AccountIdentityMissing => "ChatGPT account identity was missing".to_string(),
            Self::TokenExpiryMissing => "ChatGPT token expiry was missing".to_string(),
            Self::CredentialStorage => "ChatGPT credential storage failed".to_string(),
            Self::StoredCredentialsInvalid => "stored ChatGPT credentials were invalid".to_string(),
            Self::ModelCatalogUnavailable(status) => {
                format!("ChatGPT model catalog is unavailable (HTTP {status})")
            }
            Self::InvalidModelCatalogResponse => {
                "ChatGPT model catalog response was invalid".to_string()
            }
            Self::Transport => "ChatGPT authentication service could not be reached".to_string(),
        };
        formatter.write_str(&message)
    }
}

impl std::error::Error for OAuthError {}

#[cfg(feature = "full-daemon")]
pub(crate) fn broker() -> &'static ChatGptOAuthBroker {
    #[cfg(test)]
    crate::test_env::panic_if_hermetic_host("chatgpt_oauth::broker (keyring)");
    static BROKER: OnceLock<ChatGptOAuthBroker> = OnceLock::new();
    BROKER.get_or_init(|| {
        ChatGptOAuthBroker::with_config(OAuthConfig::from_env(), Arc::new(DaemonCredentialStore))
    })
}

#[cfg(feature = "full-daemon")]
pub fn configured() -> bool {
    broker().status().connected
}

// Embedded hosts own their OAuth broker instance so credentials never pass
// through a process-global desktop singleton. Portable inference profiles use
// API-key/local targets; direct ChatGPT turns are routed by the embedded host.
#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub fn configured() -> bool {
    false
}

#[cfg(feature = "full-daemon")]
pub fn status() -> ChatGptOAuthStatusResponse {
    broker().status()
}

#[cfg(feature = "full-daemon")]
pub async fn begin() -> Result<BeginChatGptOAuthResponse, OAuthError> {
    broker().begin().await
}

#[cfg(feature = "full-daemon")]
pub async fn complete(login_id: &str) -> Result<CompleteChatGptOAuthResponse, OAuthError> {
    broker().complete(login_id).await
}

#[cfg(feature = "full-daemon")]
pub async fn refresh() -> Result<ChatGptOAuthStatusResponse, OAuthError> {
    broker().refresh().await
}

#[cfg(feature = "full-daemon")]
pub async fn disconnect() -> Result<DisconnectChatGptOAuthResponse, OAuthError> {
    broker().disconnect().await
}

#[cfg(feature = "full-daemon")]
pub async fn list_models() -> Result<ChatGptModelListResponse, OAuthError> {
    broker().list_models().await
}

#[cfg(feature = "full-daemon")]
pub(crate) async fn request_credentials() -> Result<(String, String), OAuthError> {
    broker().credentials_for_request().await
}

#[cfg(feature = "full-daemon")]
pub(crate) async fn refresh_request_credentials(
    rejected_access_token: &str,
) -> Result<(String, String), OAuthError> {
    broker()
        .refresh_after_unauthorized(rejected_access_token)
        .await
}

#[cfg(all(test, feature = "full-daemon"))]
mod tests {
    use super::*;
    use axum::{Json, http::StatusCode};
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn hermetic_suite_refuses_the_live_oauth_broker() {
        if !crate::test_env::hermetic() {
            return;
        }
        let panicked = std::panic::catch_unwind(|| {
            let _ = configured();
        })
        .is_err();
        assert!(
            panicked,
            "live ChatGPT OAuth broker must not initialize during hermetic tests"
        );
    }

    #[derive(Default)]
    struct MemoryStore(RwLock<Option<String>>);

    impl ChatGptCredentialStore for MemoryStore {
        fn load_bundle(&self) -> Result<Option<String>, String> {
            Ok(self.0.read().unwrap().clone())
        }

        fn save_bundle(&self, bundle: Option<&str>) -> Result<(), String> {
            *self.0.write().unwrap() = bundle.map(str::to_string);
            Ok(())
        }
    }

    async fn mock_server(
        refresh_count: Arc<AtomicUsize>,
        revoke_count: Arc<AtomicUsize>,
    ) -> String {
        async fn token(
            axum::extract::State(count): axum::extract::State<Arc<AtomicUsize>>,
        ) -> Json<serde_json::Value> {
            count.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(40)).await;
            let access_token = jwt(serde_json::json!({
                "exp": (Utc::now() + ChronoDuration::hours(1)).timestamp(),
                "chatgpt_account_id": "acct_123"
            }));
            Json(serde_json::json!({
                "access_token": access_token,
                "refresh_token": "rotated-refresh"
            }))
        }

        async fn revoke(
            axum::extract::State(count): axum::extract::State<Arc<AtomicUsize>>,
        ) -> StatusCode {
            count.fetch_add(1, Ordering::SeqCst);
            StatusCode::OK
        }

        let token_router = axum::Router::new()
            .route("/oauth/token", axum::routing::post(token))
            .with_state(refresh_count);
        let revoke_router = axum::Router::new()
            .route("/oauth/revoke", axum::routing::post(revoke))
            .with_state(revoke_count);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, token_router.merge(revoke_router))
                .await
                .unwrap();
        });
        format!("http://{address}")
    }

    async fn mock_device_server() -> String {
        #[derive(Clone)]
        struct State {
            challenge: String,
            id_token: String,
            access_token: String,
        }

        async fn user_code() -> Json<serde_json::Value> {
            Json(serde_json::json!({
                "device_auth_id": "device-secret",
                "user_code": "ABCD-EFGH",
                "interval": "1"
            }))
        }

        async fn device_token(
            axum::extract::State(state): axum::extract::State<State>,
        ) -> Json<serde_json::Value> {
            Json(serde_json::json!({
                "authorization_code": "authorization-secret",
                "code_challenge": state.challenge,
                "code_verifier": "remote-workshop-verifier"
            }))
        }

        async fn exchange(
            axum::extract::State(state): axum::extract::State<State>,
        ) -> Json<serde_json::Value> {
            Json(serde_json::json!({
                "access_token": state.access_token,
                "refresh_token": "refresh-secret",
                "id_token": state.id_token
            }))
        }

        let verifier = "remote-workshop-verifier";
        let state = State {
            challenge: base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(Sha256::digest(verifier.as_bytes())),
            id_token: jwt(serde_json::json!({
                "https://api.openai.com/auth": { "chatgpt_account_id": "acct_remote" }
            })),
            access_token: jwt(serde_json::json!({
                "exp": (Utc::now() + ChronoDuration::hours(1)).timestamp()
            })),
        };
        let router = axum::Router::new()
            .route(
                "/api/accounts/deviceauth/usercode",
                axum::routing::post(user_code),
            )
            .route(
                "/api/accounts/deviceauth/token",
                axum::routing::post(device_token),
            )
            .route("/oauth/token", axum::routing::post(exchange))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        format!("http://{address}")
    }

    fn jwt(payload: serde_json::Value) -> String {
        let encode = |bytes: &[u8]| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
        format!(
            "{}.{}.sig",
            encode(br#"{"alg":"none"}"#),
            encode(serde_json::to_string(&payload).unwrap().as_bytes())
        )
    }

    fn credentials(expiry: DateTime<Utc>) -> CredentialEnvelope {
        CredentialEnvelope {
            access_token: "access-secret".to_string(),
            refresh_token: "refresh-secret".to_string(),
            id_token: "id-secret".to_string(),
            account_id: "acct_123".to_string(),
            expires_at_utc: expiry,
            reauth_required: false,
        }
    }

    fn save_credentials(store: &MemoryStore, credentials: &CredentialEnvelope) {
        store
            .save_bundle(Some(&serde_json::to_string(credentials).unwrap()))
            .unwrap();
    }

    #[test]
    fn validates_device_flow_pkce_pair() {
        let verifier = "correct-horse-battery-staple";
        let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(Sha256::digest(verifier.as_bytes()));
        assert_eq!(validate_pkce(verifier, &challenge), Ok(()));
        assert_eq!(
            validate_pkce(verifier, "wrong"),
            Err(OAuthError::PkceValidationFailed)
        );
    }

    #[test]
    fn expiry_boundary_enters_refresh_window() {
        let now = Utc::now();
        assert!(!expires_within(
            &credentials(now + ChronoDuration::minutes(6)),
            now,
            REFRESH_WINDOW_MINUTES
        ));
        assert!(expires_within(
            &credentials(now + ChronoDuration::minutes(5)),
            now,
            REFRESH_WINDOW_MINUTES
        ));
    }

    #[test]
    fn refresh_rotation_preserves_omitted_refresh_and_id_tokens() {
        let now = Utc::now();
        let previous = credentials(now);
        let access_token = jwt(serde_json::json!({
            "exp": (now + ChronoDuration::hours(1)).timestamp(),
            "chatgpt_account_id": "acct_123"
        }));
        let refreshed = credentials_from_tokens(
            TokenResponse {
                access_token,
                refresh_token: None,
                id_token: None,
                expires_in: None,
            },
            Some(&previous),
        )
        .unwrap();
        assert_eq!(refreshed.refresh_token, "refresh-secret");
        assert_eq!(refreshed.id_token, "id-secret");
        assert_eq!(refreshed.account_id, "acct_123");
    }

    #[test]
    fn debug_output_redacts_every_token() {
        let rendered = format!("{:?}", credentials(Utc::now()));
        assert!(!rendered.contains("access-secret"));
        assert!(!rendered.contains("refresh-secret"));
        assert!(!rendered.contains("id-secret"));
        assert!(rendered.contains("<redacted>"));
    }

    #[test]
    fn nested_openai_account_claim_is_supported() {
        let token = jwt(serde_json::json!({
            "https://api.openai.com/auth": { "chatgpt_account_id": "acct_nested" }
        }));
        assert_eq!(
            jwt_string_claim(&token, "chatgpt_account_id").as_deref(),
            Some("acct_nested")
        );
    }

    #[tokio::test]
    async fn account_model_catalog_uses_oauth_identity_and_picker_visibility() {
        async fn models(
            headers: axum::http::HeaderMap,
            axum::extract::Query(query): axum::extract::Query<HashMap<String, String>>,
        ) -> Json<serde_json::Value> {
            assert_eq!(
                headers.get("authorization").unwrap(),
                "Bearer access-secret"
            );
            assert_eq!(headers.get("chatgpt-account-id").unwrap(), "acct_123");
            assert_eq!(headers.get("originator").unwrap(), CODEX_COMPAT_ORIGINATOR);
            assert_eq!(headers.get("version").unwrap(), CODEX_COMPAT_VERSION);
            assert_eq!(
                headers.get("user-agent").unwrap(),
                codex_compat_user_agent().as_str()
            );
            assert_eq!(
                query.get("client_version").map(String::as_str),
                Some(CODEX_COMPAT_VERSION)
            );
            Json(serde_json::json!({
                "models": [
                    { "slug": "gpt-visible-slow", "visibility": "list", "priority": 10 },
                    { "slug": "gpt-hidden", "visibility": "hide", "priority": 100 },
                    { "slug": "gpt-visible-fast", "visibility": "list", "priority": 20 }
                ]
            }))
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let router = axum::Router::new().route("/models", axum::routing::get(models));
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });

        let store = Arc::new(MemoryStore::default());
        save_credentials(&store, &credentials(Utc::now() + ChronoDuration::hours(1)));
        let broker = ChatGptOAuthBroker::with_config(
            OAuthConfig {
                issuer: "http://unused".to_string(),
                client_id: "test-client".to_string(),
            },
            store,
        );
        let result = broker
            .list_models_from_url(&format!("http://{address}/models"))
            .await
            .unwrap();
        assert_eq!(result.models, vec!["gpt-visible-fast", "gpt-visible-slow"]);
    }

    #[tokio::test]
    async fn concurrent_expiry_refreshes_share_one_rotation() {
        let refresh_count = Arc::new(AtomicUsize::new(0));
        let issuer = mock_server(refresh_count.clone(), Arc::new(AtomicUsize::new(0))).await;
        let store = Arc::new(MemoryStore::default());
        let stale = credentials(Utc::now() - ChronoDuration::minutes(1));
        save_credentials(&store, &stale);
        let broker = ChatGptOAuthBroker::with_config(
            OAuthConfig {
                issuer,
                client_id: "test-client".to_string(),
            },
            store,
        );

        let (first, second) = tokio::join!(
            broker.credentials_for_request(),
            broker.credentials_for_request()
        );
        assert_eq!(first.unwrap().1, "acct_123");
        assert_eq!(second.unwrap().1, "acct_123");
        assert_eq!(refresh_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn disconnect_revokes_refresh_token_and_clears_local_store() {
        let revoke_count = Arc::new(AtomicUsize::new(0));
        let issuer = mock_server(Arc::new(AtomicUsize::new(0)), revoke_count.clone()).await;
        let store = Arc::new(MemoryStore::default());
        save_credentials(&store, &credentials(Utc::now() + ChronoDuration::hours(1)));
        let broker = ChatGptOAuthBroker::with_config(
            OAuthConfig {
                issuer,
                client_id: "test-client".to_string(),
            },
            store.clone(),
        );

        let response = broker.disconnect().await.unwrap();
        assert!(response.disconnected);
        assert!(response.revoked);
        assert_eq!(revoke_count.load(Ordering::SeqCst), 1);
        assert!(store.load_bundle().unwrap().is_none());
        assert_eq!(broker.status().status, "signed_out");
    }

    #[tokio::test]
    async fn device_flow_completes_through_daemon_without_exposing_tokens() {
        let issuer = mock_device_server().await;
        let store = Arc::new(MemoryStore::default());
        let broker = ChatGptOAuthBroker::with_config(
            OAuthConfig {
                issuer: issuer.clone(),
                client_id: "test-client".to_string(),
            },
            store.clone(),
        );

        let started = broker.begin().await.unwrap();
        assert_eq!(started.verification_url, format!("{issuer}/codex/device"));
        assert_eq!(started.user_code, "ABCD-EFGH");
        let completed = broker.complete(&started.login_id).await.unwrap();
        assert_eq!(completed.status, "connected");
        assert_eq!(
            completed.connection.as_ref().unwrap().account_id.as_deref(),
            Some("acct_remote")
        );
        let public_json = serde_json::to_string(&(started, completed)).unwrap();
        assert!(!public_json.contains("device-secret"));
        assert!(!public_json.contains("authorization-secret"));
        assert!(!public_json.contains("refresh-secret"));
        assert!(store.load_bundle().unwrap().is_some());
    }
}
