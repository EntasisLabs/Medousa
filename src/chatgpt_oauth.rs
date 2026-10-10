//! Sign in with ChatGPT: daemon-owned public-client registration and plan usage.
//! Tokens and callback codes stay in native runtimes, never in the webview.

use crate::daemon_api::{
    BeginChatGptOAuthRequest, BeginChatGptOAuthResponse, ChatGptAccountProfile,
    ChatGptModelListResponse, ChatGptOAuthStatusResponse, CompleteChatGptOAuthRequest,
    CompleteChatGptOAuthResponse, DisconnectChatGptOAuthResponse,
};
use base64::Engine;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
#[cfg(feature = "full-daemon")]
use std::sync::OnceLock;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::Mutex;

const DEFAULT_ISSUER: &str = "https://auth.openai.com";
const RESOURCE: &str = "https://api.openai.com/v1";
const DIRECT_SCOPE: &str = "chatgpt.tokens.use.direct";
const SCOPES: &str =
    "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";
#[cfg(feature = "full-daemon")]
const CREDENTIAL_SERVICE: &str = "medousa.chatgpt";
#[cfg(feature = "full-daemon")]
const CREDENTIAL_ACCOUNT: &str = "native_oauth";

#[derive(Clone)]
struct OAuthConfig {
    issuer: String,
}
impl OAuthConfig {
    fn from_env() -> Self {
        Self {
            issuer: DEFAULT_ISSUER.into(),
        }
    }
    fn token_url(&self) -> String {
        format!("{}/api/accounts/oauth/token", self.issuer)
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct Credentials {
    access_token: String,
    refresh_token: String,
    id_token: String,
    scopes: Vec<String>,
    expires_at_utc: DateTime<Utc>,
    #[serde(default)]
    earliest_refresh_at: Option<serde_json::Value>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Registration {
    client_id: String,
    subject: Option<String>,
    email: Option<String>,
    credentials: Option<Credentials>,
    #[serde(default)]
    welcomed: bool,
    #[serde(default)]
    previous_access_digest: Option<String>,
    #[serde(default)]
    reauth_required: bool,
}
#[derive(Clone, Serialize, Deserialize)]
struct Connections {
    version: u8,
    issuer: String,
    ext_agent_host_id: String,
    active_client_id: Option<String>,
    profiles: Vec<Registration>,
}
impl Connections {
    fn new(issuer: &str) -> Self {
        Self {
            version: 2,
            issuer: issuer.into(),
            ext_agent_host_id: format!("urn:uuid:{}", uuid::Uuid::new_v4()),
            active_client_id: None,
            profiles: Vec::new(),
        }
    }
    fn active(&self) -> Option<&Registration> {
        self.profiles
            .iter()
            .find(|p| Some(&p.client_id) == self.active_client_id.as_ref())
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
impl ChatGptCredentialStore for DaemonCredentialStore {
    fn load_bundle(&self) -> Result<Option<String>, String> {
        crate::integration_connection::try_load_kind_secret(
            "chatgpt",
            medousa_types::secrets::IntegrationSecretSlot::OauthBundle,
        )
        .map_err(|_| "ChatGPT credential storage failed".into())
    }

    fn save_bundle(&self, bundle: Option<&str>) -> Result<(), String> {
        crate::integration_connection::try_save_kind_secret(
            "chatgpt",
            medousa_types::secrets::IntegrationSecretSlot::OauthBundle,
            bundle,
        )
        .map_err(|_| "ChatGPT credential storage failed")?;
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

struct PendingLogin {
    state: String,
    nonce: String,
    verifier: String,
    redirect_uri: String,
    client_id: Option<String>,
    expires_at_utc: DateTime<Utc>,
}

pub struct ChatGptOAuthBroker {
    client: reqwest::Client,
    config: OAuthConfig,
    store: Arc<dyn ChatGptCredentialStore>,
    cached: RwLock<Connections>,
    storage_invalid: bool,
    pending: Mutex<HashMap<String, PendingLogin>>,
    lifecycle: Mutex<()>,
}
impl ChatGptOAuthBroker {
    pub fn new(store: Arc<dyn ChatGptCredentialStore>) -> Self {
        Self::with_config(OAuthConfig::from_env(), store)
    }
    fn with_config(config: OAuthConfig, store: Arc<dyn ChatGptCredentialStore>) -> Self {
        let (cached, storage_invalid) = match store.load_bundle() {
            Ok(Some(raw)) => match serde_json::from_str::<Connections>(&raw) {
                Ok(value) if value.version == 2 && value.issuer == config.issuer => (value, false),
                // Previous Codex credentials cannot authorize the new public route.
                _ if serde_json::from_str::<serde_json::Value>(&raw)
                    .is_ok_and(|v| v.get("account_id").is_some()) =>
                {
                    (Connections::new(&config.issuer), false)
                }
                _ => (Connections::new(&config.issuer), true),
            },
            Ok(None) => (Connections::new(&config.issuer), false),
            Err(_) => (Connections::new(&config.issuer), true),
        };
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("OAuth HTTP client"),
            config,
            store,
            cached: RwLock::new(cached),
            storage_invalid,
            pending: Mutex::new(HashMap::new()),
            lifecycle: Mutex::new(()),
        }
    }
    fn snapshot(&self) -> Result<Connections, OAuthError> {
        if self.storage_invalid {
            return Err(OAuthError::StoredCredentialsInvalid);
        }
        Ok(self.cached.read().expect("ChatGPT cache").clone())
    }
    fn persist(&self, value: Connections) -> Result<(), OAuthError> {
        let raw =
            serde_json::to_string(&value).map_err(|_| OAuthError::StoredCredentialsInvalid)?;
        self.store
            .save_bundle(Some(&raw))
            .map_err(|_| OAuthError::CredentialStorage)?;
        *self.cached.write().expect("ChatGPT cache") = value;
        Ok(())
    }
    pub fn status(&self) -> ChatGptOAuthStatusResponse {
        let data = self.cached.read().expect("ChatGPT cache");
        let active = data.active();
        let credentials = active.and_then(|p| p.credentials.as_ref());
        let connected = credentials.is_some();
        let plan_usage_enabled =
            credentials.is_some_and(|c| c.scopes.iter().any(|s| s == DIRECT_SCOPE));
        let status = if !connected {
            if active.is_some_and(|p| p.reauth_required) {
                "reauth_required"
            } else {
                "signed_out"
            }
        } else if !plan_usage_enabled {
            "plan_usage_disabled"
        } else if credentials.is_some_and(near_expiry) {
            "refresh_required"
        } else {
            "connected"
        };
        ChatGptOAuthStatusResponse {
            status: status.into(),
            connected,
            account_id: active.and_then(|p| p.subject.clone()),
            expires_at_utc: credentials.map(|c| c.expires_at_utc),
            client_id: active.map(|p| p.client_id.clone()),
            email: active.and_then(|p| p.email.clone()),
            plan_usage_enabled,
            profiles: data
                .profiles
                .iter()
                .map(|p| ChatGptAccountProfile {
                    client_id: p.client_id.clone(),
                    account_id: p.subject.clone(),
                    email: p.email.clone(),
                    connected: p.credentials.is_some(),
                    plan_usage_enabled: p
                        .credentials
                        .as_ref()
                        .is_some_and(|c| c.scopes.iter().any(|s| s == DIRECT_SCOPE)),
                })
                .collect(),
        }
    }
    pub async fn begin(
        &self,
        request: BeginChatGptOAuthRequest,
    ) -> Result<BeginChatGptOAuthResponse, OAuthError> {
        validate_redirect(&request.redirect_uri)?;
        let _guard = self.lifecycle.lock().await;
        let data = self.snapshot()?;
        let profile = request
            .client_id
            .as_ref()
            .map(|id| {
                data.profiles
                    .iter()
                    .find(|p| &p.client_id == id)
                    .ok_or(OAuthError::LoginNotFound)
            })
            .transpose()?;
        // Persist the host before starting authorization, including a first failed/declined attempt.
        self.persist(data.clone())?;
        let state = random_value();
        let nonce = random_value();
        let verifier = random_value();
        let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(Sha256::digest(verifier.as_bytes()));
        let expires_at_utc = Utc::now() + ChronoDuration::minutes(10);
        let login_id = uuid::Uuid::new_v4().to_string();
        let mut url =
            reqwest::Url::parse(&format!("{}/api/accounts/authorize", self.config.issuer))
                .map_err(|_| OAuthError::InvalidAuthorizationResponse)?;
        {
            let mut query = url.query_pairs_mut();
            query.extend_pairs([
                (
                    "client_id",
                    profile
                        .map(|p| p.client_id.as_str())
                        .unwrap_or("dynamic_agent_client"),
                ),
                ("ext_agent_host_id", data.ext_agent_host_id.as_str()),
                ("response_type", "code"),
                ("redirect_uri", request.redirect_uri.as_str()),
                ("scope", SCOPES),
                ("resource", RESOURCE),
                ("state", state.as_str()),
                ("nonce", nonce.as_str()),
                ("code_challenge_method", "S256"),
                ("code_challenge", challenge.as_str()),
            ]);
            if let Some(profile) = profile {
                if let Some(credentials) = &profile.credentials {
                    query.append_pair("id_token_hint", &credentials.id_token);
                }
                if let Some(email) = &profile.email {
                    query.append_pair("login_hint", email);
                }
            } else {
                query.append_pair("agent_name_hint", "Medousa");
            }
            if request.enable_plan_usage {
                query.append_pair("prompt", "consent");
            }
        }
        let mut pending = self.pending.lock().await;
        pending.retain(|_, p| p.expires_at_utc > Utc::now());
        if pending.len() >= 16 {
            return Err(OAuthError::AuthorizationUnavailable(429));
        }
        pending.insert(
            login_id.clone(),
            PendingLogin {
                state,
                nonce,
                verifier,
                redirect_uri: request.redirect_uri,
                client_id: request.client_id,
                expires_at_utc,
            },
        );
        Ok(BeginChatGptOAuthResponse {
            login_id,
            authorization_url: url.into(),
            expires_at_utc,
        })
    }
    pub async fn complete(
        &self,
        request: CompleteChatGptOAuthRequest,
    ) -> Result<CompleteChatGptOAuthResponse, OAuthError> {
        let pending = self
            .pending
            .lock()
            .await
            .remove(&request.login_id)
            .ok_or(OAuthError::LoginNotFound)?;
        if pending.expires_at_utc <= Utc::now() {
            return Err(OAuthError::LoginExpired);
        }
        let callback = reqwest::Url::parse(&request.callback_url)
            .map_err(|_| OAuthError::PkceValidationFailed)?;
        let mut base = callback.clone();
        base.set_query(None);
        if base.as_str() != pending.redirect_uri {
            return Err(OAuthError::PkceValidationFailed);
        }
        let mut params = HashMap::new();
        for (key, value) in callback.query_pairs() {
            if params
                .insert(key.into_owned(), value.into_owned())
                .is_some()
            {
                return Err(OAuthError::PkceValidationFailed);
            }
        }
        if params.get("state") != Some(&pending.state) {
            return Err(OAuthError::PkceValidationFailed);
        }
        if params.contains_key("error") {
            return Err(OAuthError::AuthorizationFailed(403));
        }
        let issued = params
            .get("client_id")
            .or(pending.client_id.as_ref())
            .ok_or(OAuthError::AccountIdentityMissing)?;
        if !issued.starts_with("oaiapp_")
            || pending.client_id.as_ref().is_some_and(|id| id != issued)
        {
            return Err(OAuthError::AccountIdentityMissing);
        }
        let code = params
            .get("code")
            .filter(|c| !c.is_empty())
            .ok_or(OAuthError::InvalidAuthorizationResponse)?;
        let _guard = self.lifecycle.lock().await;
        let mut data = self.snapshot()?;
        // Retain registration before exchange so an expired code does not register another client.
        if !data.profiles.iter().any(|p| &p.client_id == issued) {
            data.profiles.push(Registration {
                client_id: issued.clone(),
                subject: None,
                email: None,
                credentials: None,
                welcomed: false,
                previous_access_digest: None,
                reauth_required: false,
            });
            self.persist(data.clone())?;
        }
        let tokens = parse_tokens(
            self.client
                .post(self.config.token_url())
                .form(&[
                    ("grant_type", "authorization_code"),
                    ("client_id", issued.as_str()),
                    ("code", code.as_str()),
                    ("code_verifier", pending.verifier.as_str()),
                    ("redirect_uri", pending.redirect_uri.as_str()),
                    ("resource", RESOURCE),
                ])
                .send()
                .await
                .map_err(|_| OAuthError::Transport)?,
        )
        .await?;
        let id_token = tokens
            .id_token
            .as_deref()
            .ok_or(OAuthError::InvalidAuthorizationResponse)?;
        let identity = self
            .verify_identity(id_token, issued, Some(&pending.nonce))
            .await?;
        let profile = data
            .profiles
            .iter_mut()
            .find(|p| &p.client_id == issued)
            .expect("registration");
        if profile.subject.as_ref().is_some_and(|s| s != &identity.sub) {
            return Err(OAuthError::AccountIdentityMissing);
        }
        profile.credentials = Some(credentials_from_tokens(tokens, None)?);
        profile.previous_access_digest = None;
        profile.reauth_required = false;
        profile.subject = Some(identity.sub);
        profile.email = identity.email;
        let first_connection = !profile.welcomed
            && profile
                .credentials
                .as_ref()
                .is_some_and(|c| c.scopes.iter().any(|s| s == DIRECT_SCOPE));
        profile.welcomed |= first_connection;
        data.active_client_id = Some(issued.clone());
        self.persist(data)?;
        update_catalog(Vec::new()).await?;
        let connection = self.status();
        Ok(CompleteChatGptOAuthResponse {
            status: connection.status.clone(),
            first_connection,
            connection,
        })
    }
    async fn discovery(&self) -> Result<serde_json::Value, OAuthError> {
        let response = self
            .client
            .get(format!(
                "{}/.well-known/openid-configuration",
                self.config.issuer
            ))
            .send()
            .await
            .map_err(|_| OAuthError::Transport)?;
        let value: serde_json::Value = response
            .error_for_status()
            .map_err(|_| OAuthError::InvalidAuthorizationResponse)?
            .json()
            .await
            .map_err(|_| OAuthError::InvalidAuthorizationResponse)?;
        if value["issuer"].as_str() != Some(&self.config.issuer) {
            return Err(OAuthError::InvalidAuthorizationResponse);
        }
        Ok(value)
    }
    fn discovery_endpoint(
        &self,
        discovery: &serde_json::Value,
        key: &str,
    ) -> Result<reqwest::Url, OAuthError> {
        let url = reqwest::Url::parse(
            discovery[key]
                .as_str()
                .ok_or(OAuthError::InvalidAuthorizationResponse)?,
        )
        .map_err(|_| OAuthError::InvalidAuthorizationResponse)?;
        let issuer = reqwest::Url::parse(&self.config.issuer)
            .map_err(|_| OAuthError::InvalidAuthorizationResponse)?;
        if url.origin() != issuer.origin() {
            return Err(OAuthError::InvalidAuthorizationResponse);
        }
        Ok(url)
    }
    async fn verify_identity(
        &self,
        token: &str,
        client_id: &str,
        nonce: Option<&str>,
    ) -> Result<Identity, OAuthError> {
        let discovery = self.discovery().await?;
        let jwks: jsonwebtoken::jwk::JwkSet = self
            .client
            .get(self.discovery_endpoint(&discovery, "jwks_uri")?)
            .send()
            .await
            .map_err(|_| OAuthError::Transport)?
            .error_for_status()
            .map_err(|_| OAuthError::InvalidAuthorizationResponse)?
            .json()
            .await
            .map_err(|_| OAuthError::InvalidAuthorizationResponse)?;
        validate_identity(token, client_id, &self.config.issuer, nonce, &jwks)
    }
    pub async fn select(&self, client_id: &str) -> Result<ChatGptOAuthStatusResponse, OAuthError> {
        let _guard = self.lifecycle.lock().await;
        let mut data = self.snapshot()?;
        if !data.profiles.iter().any(|p| p.client_id == client_id) {
            return Err(OAuthError::LoginNotFound);
        }
        data.active_client_id = Some(client_id.into());
        self.persist(data)?;
        update_catalog(Vec::new()).await?;
        Ok(self.status())
    }
    pub async fn refresh(&self) -> Result<ChatGptOAuthStatusResponse, OAuthError> {
        let _guard = self.lifecycle.lock().await;
        let data = self.snapshot()?;
        let id = data.active_client_id.ok_or(OAuthError::NotConnected)?;
        self.refresh_locked(&id).await?;
        Ok(self.status())
    }
    pub(crate) async fn credentials_for_request(&self) -> Result<(String, String), OAuthError> {
        let _guard = self.lifecycle.lock().await;
        let data = self.snapshot()?;
        let profile = data.active().ok_or(OAuthError::NotConnected)?;
        let current = profile
            .credentials
            .as_ref()
            .ok_or(OAuthError::ReauthenticationRequired)?;
        require_plan_usage(current)?;
        let token = if near_expiry(current) {
            self.refresh_locked(&profile.client_id).await?
        } else {
            current.clone()
        };
        require_plan_usage(&token)?;
        Ok((token.access_token, profile.client_id.clone()))
    }
    pub(crate) async fn refresh_after_unauthorized(
        &self,
        rejected: &str,
    ) -> Result<(String, String), OAuthError> {
        let _guard = self.lifecycle.lock().await;
        let data = self.snapshot()?;
        let profile = data.active().ok_or(OAuthError::NotConnected)?;
        let current = profile
            .credentials
            .as_ref()
            .ok_or(OAuthError::ReauthenticationRequired)?;
        // Never replay a request as a different account after a concurrent account switch.
        if current.access_token != rejected {
            if profile.previous_access_digest.as_deref() == Some(&access_digest(rejected)) {
                require_plan_usage(current)?;
                return Ok((current.access_token.clone(), profile.client_id.clone()));
            }
            return Err(OAuthError::ReauthenticationRequired);
        }
        let credentials = self.refresh_locked(&profile.client_id).await?;
        require_plan_usage(&credentials)?;
        Ok((credentials.access_token, profile.client_id.clone()))
    }
    async fn refresh_locked(&self, client_id: &str) -> Result<Credentials, OAuthError> {
        let mut data = self.snapshot()?;
        let profile = data
            .profiles
            .iter_mut()
            .find(|p| p.client_id == client_id)
            .ok_or(OAuthError::NotConnected)?;
        let current = profile
            .credentials
            .as_ref()
            .ok_or(OAuthError::ReauthenticationRequired)?
            .clone();
        let response = self
            .client
            .post(self.config.token_url())
            .form(&[
                ("grant_type", "refresh_token"),
                ("client_id", client_id),
                ("refresh_token", current.refresh_token.as_str()),
                ("resource", RESOURCE),
            ])
            .send()
            .await
            .map_err(|_| OAuthError::Transport)?;
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let error: serde_json::Value = response.json().await.unwrap_or_default();
            let code = error["error"]
                .as_str()
                .or(error["error"]["code"].as_str())
                .unwrap_or_default();
            if matches!(
                code,
                "invalid_grant"
                    | "invalid_refresh_token"
                    | "token_expired"
                    | "refresh_token_expired"
                    | "refresh_token_invalidated"
                    | "refresh_token_reused"
            ) {
                profile.credentials = None;
                profile.previous_access_digest = None;
                profile.reauth_required = true;
                self.persist(data)?;
                return Err(OAuthError::ReauthenticationRequired);
            }
            return Err(OAuthError::TokenExchangeFailed(status));
        }
        let tokens = parse_tokens(response).await?;
        if let Some(token) = tokens.id_token.as_deref() {
            let identity = self.verify_identity(token, client_id, None).await?;
            if Some(&identity.sub) != profile.subject.as_ref() {
                return Err(OAuthError::AccountIdentityMissing);
            }
        }
        let credentials = credentials_from_tokens(tokens, Some(&current))?;
        profile.previous_access_digest = Some(access_digest(&current.access_token));
        profile.credentials = Some(credentials.clone());
        self.persist(data)?;
        Ok(credentials)
    }
    pub async fn disconnect(&self) -> Result<DisconnectChatGptOAuthResponse, OAuthError> {
        let _guard = self.lifecycle.lock().await;
        let mut data = self.snapshot()?;
        let mut revoked = false;
        if let Some(id) = data.active_client_id.clone()
            && let Some(profile) = data.profiles.iter_mut().find(|p| p.client_id == id)
        {
            if let Some(credentials) = &profile.credentials
                && let Ok(discovery) = self.discovery().await
                && let Ok(url) = self.discovery_endpoint(&discovery, "revocation_endpoint")
            {
                for attempt in 0..3 {
                    match self
                        .client
                        .post(url.clone())
                        .form(&[
                            ("token", credentials.refresh_token.as_str()),
                            ("token_type_hint", "refresh_token"),
                            ("client_id", profile.client_id.as_str()),
                        ])
                        .send()
                        .await
                    {
                        Ok(response) if response.status() == reqwest::StatusCode::OK => {
                            revoked = true;
                            break;
                        }
                        Ok(response) if !response.status().is_server_error() => break,
                        _ => {
                            if attempt < 2 {
                                tokio::time::sleep(Duration::from_millis(250 << attempt)).await;
                            }
                        }
                    }
                }
            }
            profile.credentials = None;
            profile.previous_access_digest = None;
            profile.reauth_required = false;
        }
        self.persist(data)?;
        self.pending.lock().await.clear();
        update_catalog(Vec::new()).await?;
        Ok(DisconnectChatGptOAuthResponse {
            disconnected: true,
            revoked,
        })
    }
    pub async fn list_models(&self) -> Result<ChatGptModelListResponse, OAuthError> {
        self.list_models_from_url("https://api.openai.com/v1/models")
            .await
    }
    async fn list_models_from_url(
        &self,
        url: &str,
    ) -> Result<ChatGptModelListResponse, OAuthError> {
        let (token, client_id) = self.credentials_for_request().await?;
        let mut response = self
            .client
            .get(url)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|_| OAuthError::Transport)?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            let (refreshed, refreshed_client) = self.refresh_after_unauthorized(&token).await?;
            if refreshed_client != client_id {
                return Err(OAuthError::AccountIdentityMissing);
            }
            response = self
                .client
                .get(url)
                .bearer_auth(refreshed)
                .send()
                .await
                .map_err(|_| OAuthError::Transport)?;
        }
        if !response.status().is_success() {
            return Err(OAuthError::ModelCatalogUnavailable(
                response.status().as_u16(),
            ));
        }
        let response: serde_json::Value = response
            .json()
            .await
            .map_err(|_| OAuthError::InvalidModelCatalogResponse)?;
        let mut seen = HashSet::new();
        let mut models = Vec::new();
        let mut display_names = BTreeMap::new();
        let mut capabilities = Vec::new();
        for model in response["models"]
            .as_array()
            .ok_or(OAuthError::InvalidModelCatalogResponse)?
        {
            if model["visibility"].as_str() != Some("list") {
                continue;
            }
            if let Some(slug) = model["slug"].as_str().filter(|s| !s.trim().is_empty())
                && seen.insert(slug.to_string())
            {
                let reasoning = model["supported_reasoning_levels"]
                    .as_array()
                    .map(|levels| {
                        crate::reasoning_effort::ReasoningCapability::advertised(
                            &levels
                                .iter()
                                .filter_map(|level| level["effort"].as_str().map(str::to_owned))
                                .collect::<Vec<_>>(),
                            model["default_reasoning_level"].as_str().map(str::to_owned),
                        )
                    });
                capabilities.push((
                    slug.to_string(),
                    model["display_name"].as_str().map(str::to_owned),
                    reasoning,
                ));
                models.push(slug.to_string());
                display_names.insert(
                    slug.into(),
                    model["display_name"].as_str().unwrap_or(slug).into(),
                );
            }
        }
        let _guard = self.lifecycle.lock().await;
        if self.snapshot()?.active_client_id.as_deref() != Some(&client_id) {
            return Err(OAuthError::AccountIdentityMissing);
        }
        update_catalog(capabilities).await?;
        Ok(ChatGptModelListResponse {
            models,
            display_names,
        })
    }
}
type CatalogEntry = (
    String,
    Option<String>,
    Option<crate::reasoning_effort::ReasoningCapability>,
);
async fn update_catalog(models: Vec<CatalogEntry>) -> Result<(), OAuthError> {
    // Unit brokers must never mutate the installation's catalog on disk.
    #[cfg(not(test))]
    tokio::task::spawn_blocking(move || {
        crate::model_capability_registry::registry().record_chatgpt_catalog(models)
    })
    .await
    .map_err(|_| OAuthError::InvalidModelCatalogResponse)?;
    #[cfg(test)]
    let _ = models;
    Ok(())
}

#[derive(Clone, Deserialize)]
struct Identity {
    sub: String,
    nonce: Option<String>,
    email: Option<String>,
}
fn validate_identity(
    token: &str,
    client_id: &str,
    issuer: &str,
    nonce: Option<&str>,
    jwks: &jsonwebtoken::jwk::JwkSet,
) -> Result<Identity, OAuthError> {
    let header =
        jsonwebtoken::decode_header(token).map_err(|_| OAuthError::InvalidAuthorizationResponse)?;
    // Never accept unsigned/symmetric tokens, or an algorithm supplied only by the token.
    if header.alg != jsonwebtoken::Algorithm::RS256 {
        return Err(OAuthError::InvalidAuthorizationResponse);
    }
    let key = jwks
        .find(
            header
                .kid
                .as_deref()
                .ok_or(OAuthError::InvalidAuthorizationResponse)?,
        )
        .ok_or(OAuthError::InvalidAuthorizationResponse)?;
    let key = jsonwebtoken::DecodingKey::from_jwk(key)
        .map_err(|_| OAuthError::InvalidAuthorizationResponse)?;
    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);
    validation.set_issuer(&[issuer]);
    validation.set_audience(&[client_id]);
    validation.leeway = 5;
    validation.set_required_spec_claims(&["sub", "iss", "aud", "exp", "iat"]);
    let identity = jsonwebtoken::decode::<Identity>(token, &key, &validation)
        .map_err(|_| OAuthError::InvalidAuthorizationResponse)?
        .claims;
    if identity.sub.is_empty() || nonce.is_some_and(|n| identity.nonce.as_deref() != Some(n)) {
        return Err(OAuthError::AccountIdentityMissing);
    }
    Ok(identity)
}
#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    id_token: Option<String>,
    expires_in: i64,
    scope: Option<String>,
    token_type: String,
    #[serde(default)]
    earliest_refresh_at: Option<serde_json::Value>,
}
async fn parse_tokens(response: reqwest::Response) -> Result<TokenResponse, OAuthError> {
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
    previous: Option<&Credentials>,
) -> Result<Credentials, OAuthError> {
    if tokens.access_token.is_empty()
        || !tokens.token_type.eq_ignore_ascii_case("bearer")
        || tokens.expires_in <= 0
        || tokens.expires_in > 31536000
    {
        return Err(OAuthError::InvalidAuthorizationResponse);
    }
    Ok(Credentials {
        access_token: tokens.access_token,
        refresh_token: tokens
            .refresh_token
            .filter(|s| !s.is_empty())
            .or_else(|| previous.map(|c| c.refresh_token.clone()))
            .ok_or(OAuthError::InvalidAuthorizationResponse)?,
        id_token: tokens
            .id_token
            .filter(|s| !s.is_empty())
            .or_else(|| previous.map(|c| c.id_token.clone()))
            .ok_or(OAuthError::InvalidAuthorizationResponse)?,
        scopes: tokens
            .scope
            .map(|s| s.split_whitespace().map(str::to_owned).collect())
            .unwrap_or_else(|| previous.map(|c| c.scopes.clone()).unwrap_or_default()),
        expires_at_utc: Utc::now() + ChronoDuration::seconds(tokens.expires_in),
        earliest_refresh_at: tokens.earliest_refresh_at,
    })
}
fn access_digest(token: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(token.as_bytes()))
}
fn near_expiry(c: &Credentials) -> bool {
    c.expires_at_utc <= Utc::now() + ChronoDuration::minutes(5)
}
fn require_plan_usage(c: &Credentials) -> Result<(), OAuthError> {
    if !c.scopes.iter().any(|s| s == DIRECT_SCOPE) {
        return Err(OAuthError::PlanUsageDisabled);
    }
    Ok(())
}
fn random_value() -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
        [
            uuid::Uuid::new_v4().as_bytes().as_slice(),
            uuid::Uuid::new_v4().as_bytes().as_slice(),
        ]
        .concat(),
    )
}
fn validate_redirect(uri: &str) -> Result<(), OAuthError> {
    let url = reqwest::Url::parse(uri).map_err(|_| OAuthError::PkceValidationFailed)?;
    if url.scheme() != "http"
        || url.host_str() != Some("127.0.0.1")
        || url.port().is_none()
        || url.path() != "/oauth/callback"
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(OAuthError::PkceValidationFailed);
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
pub enum OAuthError {
    NotConnected,
    PlanUsageDisabled,
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
            Self::PlanUsageDisabled => "Enable ChatGPT plan usage in account settings".to_string(),
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
    broker().status().plan_usage_enabled
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
pub async fn begin(
    request: BeginChatGptOAuthRequest,
) -> Result<BeginChatGptOAuthResponse, OAuthError> {
    broker().begin(request).await
}

#[cfg(feature = "full-daemon")]
pub async fn complete(
    request: CompleteChatGptOAuthRequest,
) -> Result<CompleteChatGptOAuthResponse, OAuthError> {
    broker().complete(request).await
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

#[cfg(feature = "full-daemon")]
pub async fn select(client_id: &str) -> Result<ChatGptOAuthStatusResponse, OAuthError> {
    broker().select(client_id).await
}

#[cfg(test)]
mod tests;
