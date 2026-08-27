//! Shared OAuth lifecycle for remote MCP servers.
//!
//! Protocol mechanics come from the official MCP Rust SDK. Medousa owns the
//! per-server lifecycle, secret-store boundary, and secret-free receipts.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use medousa_types::mcp_gateway_api::{
    BeginMcpOAuthResponse, CompleteMcpOAuthResponse, DisconnectMcpOAuthResponse,
    McpOAuthStatusResponse,
};
use medousa_types::{DaemonSecretPath, InstallationId, McpServerId};
use rmcp::transport::auth::OAuthState;
use rmcp::transport::{
    AuthError, AuthorizationManager, AuthorizationRequest, CredentialStore, StoredCredentials,
};
use tokio::sync::Mutex;

/// Deployment binding for encrypted, opaque MCP OAuth bundles.
///
/// The broker owns the serialized schema. Hosts only select a secure backend
/// and scope values by MCP server id.
pub trait McpOAuthBundleStore: Send + Sync {
    fn load_bundle(&self, server_id: &str) -> Result<Option<String>, String>;
    fn save_bundle(&self, server_id: &str, bundle: Option<&str>) -> Result<(), String>;
}

/// Platform-backed daemon secret store shared by sidecar and embedded MCP.
pub struct SecureMcpOAuthBundleStore {
    data_dir: PathBuf,
    installation_id: InstallationId,
}

impl SecureMcpOAuthBundleStore {
    pub fn new(data_dir: impl AsRef<Path>) -> Result<Self, String> {
        let data_dir = data_dir.as_ref().to_path_buf();
        let installation_id = medousa_secrets::ensure_installation_id(&data_dir)
            .map_err(|error| error.to_string())?;
        Ok(Self {
            data_dir,
            installation_id,
        })
    }

    fn path(&self, server_id: &str) -> Result<DaemonSecretPath, String> {
        Ok(DaemonSecretPath::McpOAuth {
            installation_id: self.installation_id.clone(),
            server_id: McpServerId::parse(server_id).map_err(|error| error.to_string())?,
        })
    }
}

impl McpOAuthBundleStore for SecureMcpOAuthBundleStore {
    fn load_bundle(&self, server_id: &str) -> Result<Option<String>, String> {
        medousa_secrets::load_daemon_secret(&self.data_dir, &self.path(server_id)?)
            .map(|value| value.map(|value| value.value))
            .map_err(|error| error.to_string())
    }

    fn save_bundle(&self, server_id: &str, bundle: Option<&str>) -> Result<(), String> {
        let path = self.path(server_id)?;
        if let Some(bundle) = bundle.map(str::trim).filter(|value| !value.is_empty()) {
            medousa_secrets::save_daemon_secret(&self.data_dir, &path, bundle)
                .map(|_| ())
                .map_err(|error| error.to_string())
        } else {
            medousa_secrets::delete_daemon_secret(&self.data_dir, &path)
                .map_err(|error| error.to_string())
        }
    }
}

/// Inputs available when beginning authorization for one configured server.
#[derive(Clone)]
pub struct McpOAuthBeginRequest {
    pub server_id: String,
    pub server_url: String,
    pub redirect_uri: String,
    pub scopes: Vec<String>,
    pub client_metadata_url: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub challenge: Option<String>,
}

impl std::fmt::Debug for McpOAuthBeginRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("McpOAuthBeginRequest")
            .field("server_id", &self.server_id)
            .field("server_url", &self.server_url)
            .field("redirect_uri", &self.redirect_uri)
            .field("scopes", &self.scopes)
            .field("client_metadata_url", &self.client_metadata_url)
            .field("client_id", &self.client_id)
            .field(
                "client_secret",
                &self.client_secret.as_ref().map(|_| "<redacted>"),
            )
            .field("challenge", &self.challenge)
            .finish()
    }
}

struct PendingMcpLogin {
    server_id: String,
    state: OAuthState,
}

pub struct McpOAuthBroker {
    store: Arc<dyn McpOAuthBundleStore>,
    pending: Mutex<HashMap<String, PendingMcpLogin>>,
}

impl McpOAuthBroker {
    pub fn new(store: Arc<dyn McpOAuthBundleStore>) -> Self {
        Self {
            store,
            pending: Mutex::new(HashMap::new()),
        }
    }

    pub async fn status(&self, server_id: &str) -> Result<McpOAuthStatusResponse, McpOAuthError> {
        let server_id = required("server id", server_id)?;
        let credentials = self.credential_store(server_id).load().await?;
        Ok(status_for(server_id, credentials.as_ref()))
    }

    pub async fn begin(
        &self,
        request: McpOAuthBeginRequest,
    ) -> Result<BeginMcpOAuthResponse, McpOAuthError> {
        let server_id = required("server id", &request.server_id)?.to_string();
        let server_url = required("server URL", &request.server_url)?;
        let redirect_uri = required("redirect URI", &request.redirect_uri)?;

        let mut state = OAuthState::new(server_url, None).await?;
        let OAuthState::Unauthorized(manager) = &mut state else {
            return Err(McpOAuthError::ProtocolState);
        };
        manager.set_credential_store(self.credential_store(&server_id));

        let mut authorization = AuthorizationRequest::new(redirect_uri)
            .with_client_name("Medousa")
            .with_application_type("native");
        if !request.scopes.is_empty() {
            authorization = authorization.with_scopes(request.scopes);
        }
        if let Some(url) = non_empty(request.client_metadata_url) {
            authorization = authorization.with_client_metadata_url(url);
        }
        if let Some(client_id) = non_empty(request.client_id) {
            authorization = authorization.with_preregistered_client(client_id);
        }
        if let Some(client_secret) = non_empty(request.client_secret) {
            authorization = authorization.with_client_secret(client_secret);
        }
        if let Some(challenge) = non_empty(request.challenge) {
            authorization = authorization.with_challenge(challenge);
        }

        state.start_authorization(authorization).await?;
        let authorization_url = state.get_authorization_url().await?;
        let login_id = uuid::Uuid::new_v4().to_string();
        let mut pending = self.pending.lock().await;
        pending.retain(|_, login| login.server_id != server_id);
        pending.insert(
            login_id.clone(),
            PendingMcpLogin {
                server_id: server_id.clone(),
                state,
            },
        );

        Ok(BeginMcpOAuthResponse {
            server_id,
            login_id,
            authorization_url,
        })
    }

    pub async fn complete(
        &self,
        login_id: &str,
        callback_url: &str,
    ) -> Result<CompleteMcpOAuthResponse, McpOAuthError> {
        let login_id = required("login id", login_id)?;
        let callback_url = required("callback URL", callback_url)?;
        let mut login = self
            .pending
            .lock()
            .await
            .remove(login_id)
            .ok_or(McpOAuthError::LoginNotFound)?;

        if let Err(error) = login.state.handle_callback_url(callback_url).await {
            self.pending
                .lock()
                .await
                .insert(login_id.to_string(), login);
            return Err(error.into());
        }

        Ok(CompleteMcpOAuthResponse {
            connection: self.status(&login.server_id).await?,
        })
    }

    pub async fn access_token(
        &self,
        server_id: &str,
        server_url: &str,
    ) -> Result<String, McpOAuthError> {
        let mut manager = self.manager(server_id, server_url).await?;
        if !manager.initialize_from_store().await? {
            return Err(McpOAuthError::NotConnected);
        }
        manager.get_access_token().await.map_err(Into::into)
    }

    pub async fn refresh(
        &self,
        server_id: &str,
        server_url: &str,
    ) -> Result<McpOAuthStatusResponse, McpOAuthError> {
        let mut manager = self.manager(server_id, server_url).await?;
        if !manager.initialize_from_store().await? {
            return Err(McpOAuthError::NotConnected);
        }
        manager.refresh_token().await?;
        self.status(server_id).await
    }

    pub async fn disconnect(
        &self,
        server_id: &str,
    ) -> Result<DisconnectMcpOAuthResponse, McpOAuthError> {
        let server_id = required("server id", server_id)?.to_string();
        self.credential_store(&server_id).clear().await?;
        self.pending
            .lock()
            .await
            .retain(|_, login| login.server_id != server_id);
        Ok(DisconnectMcpOAuthResponse {
            server_id,
            disconnected: true,
        })
    }

    async fn manager(
        &self,
        server_id: &str,
        server_url: &str,
    ) -> Result<AuthorizationManager, McpOAuthError> {
        let server_id = required("server id", server_id)?;
        let server_url = required("server URL", server_url)?;
        let mut manager = AuthorizationManager::new(server_url).await?;
        manager.set_credential_store(self.credential_store(server_id));
        Ok(manager)
    }

    fn credential_store(&self, server_id: &str) -> BrokerCredentialStore {
        BrokerCredentialStore {
            server_id: server_id.to_string(),
            store: self.store.clone(),
        }
    }
}

#[derive(Clone)]
struct BrokerCredentialStore {
    server_id: String,
    store: Arc<dyn McpOAuthBundleStore>,
}

#[async_trait]
impl CredentialStore for BrokerCredentialStore {
    async fn load(&self) -> Result<Option<StoredCredentials>, AuthError> {
        let bundle = self
            .store
            .load_bundle(&self.server_id)
            .map_err(|_| credential_storage_error())?;
        bundle
            .map(|value| serde_json::from_str(&value).map_err(|_| stored_credentials_error()))
            .transpose()
    }

    async fn save(&self, credentials: StoredCredentials) -> Result<(), AuthError> {
        let bundle = serde_json::to_string(&credentials).map_err(|_| stored_credentials_error())?;
        self.store
            .save_bundle(&self.server_id, Some(&bundle))
            .map_err(|_| credential_storage_error())
    }

    async fn clear(&self) -> Result<(), AuthError> {
        self.store
            .save_bundle(&self.server_id, None)
            .map_err(|_| credential_storage_error())
    }
}

fn status_for(server_id: &str, credentials: Option<&StoredCredentials>) -> McpOAuthStatusResponse {
    let connected = credentials.is_some_and(|value| value.token_response.is_some());
    McpOAuthStatusResponse {
        server_id: server_id.to_string(),
        status: if connected { "connected" } else { "signed_out" }.to_string(),
        connected,
        issuer: credentials.and_then(|value| value.issuer.clone()),
        scopes: credentials
            .map(|value| value.granted_scopes.clone())
            .unwrap_or_default(),
    }
}

fn required<'a>(label: &'static str, value: &'a str) -> Result<&'a str, McpOAuthError> {
    let value = value.trim();
    if value.is_empty() {
        Err(McpOAuthError::InvalidInput(label))
    } else {
        Ok(value)
    }
}

fn non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn credential_storage_error() -> AuthError {
    AuthError::InternalError("MCP OAuth credential storage failed".to_string())
}

fn stored_credentials_error() -> AuthError {
    AuthError::InternalError("stored MCP OAuth credentials were invalid".to_string())
}

#[derive(Debug)]
pub enum McpOAuthError {
    InvalidInput(&'static str),
    LoginNotFound,
    NotConnected,
    ProtocolState,
    OAuth(AuthError),
}

impl std::fmt::Display for McpOAuthError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput(label) => write!(formatter, "{label} is required"),
            Self::LoginNotFound => {
                formatter.write_str("MCP OAuth login was not found; start again")
            }
            Self::NotConnected => formatter.write_str("MCP server OAuth is not connected"),
            Self::ProtocolState => formatter.write_str("MCP OAuth entered an invalid state"),
            Self::OAuth(error) => write!(formatter, "MCP OAuth failed: {error}"),
        }
    }
}

impl std::error::Error for McpOAuthError {}

impl From<AuthError> for McpOAuthError {
    fn from(error: AuthError) -> Self {
        match error {
            AuthError::AuthorizationRequired | AuthError::TokenRefreshRejected(_) => {
                Self::NotConnected
            }
            error => Self::OAuth(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    #[derive(Default)]
    struct MemoryBundleStore(StdMutex<HashMap<String, String>>);

    impl McpOAuthBundleStore for MemoryBundleStore {
        fn load_bundle(&self, server_id: &str) -> Result<Option<String>, String> {
            Ok(self.0.lock().unwrap().get(server_id).cloned())
        }

        fn save_bundle(&self, server_id: &str, bundle: Option<&str>) -> Result<(), String> {
            let mut values = self.0.lock().unwrap();
            if let Some(bundle) = bundle {
                values.insert(server_id.to_string(), bundle.to_string());
            } else {
                values.remove(server_id);
            }
            Ok(())
        }
    }

    #[tokio::test]
    async fn credential_bundles_are_scoped_by_server() {
        let store = Arc::new(MemoryBundleStore::default());
        let broker = McpOAuthBroker::new(store);
        let first = broker.credential_store("first");
        first
            .save(StoredCredentials::new(
                "client-first".to_string(),
                None,
                vec!["read".to_string()],
                None,
            ))
            .await
            .unwrap();

        assert_eq!(
            first.load().await.unwrap().unwrap().client_id,
            "client-first"
        );
        assert!(
            broker
                .credential_store("second")
                .load()
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn status_is_secret_free_and_disconnect_is_server_scoped() {
        let store = Arc::new(MemoryBundleStore::default());
        let broker = McpOAuthBroker::new(store);
        broker
            .credential_store("notion")
            .save(StoredCredentials::new(
                "client-notion".to_string(),
                None,
                vec!["read".to_string()],
                None,
            ))
            .await
            .unwrap();

        let status = broker.status("notion").await.unwrap();
        assert!(!status.connected);
        assert_eq!(status.status, "signed_out");
        broker.disconnect("notion").await.unwrap();
        assert!(
            broker
                .credential_store("notion")
                .load()
                .await
                .unwrap()
                .is_none()
        );
    }
}
