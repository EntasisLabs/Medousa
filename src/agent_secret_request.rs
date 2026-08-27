//! Agent-initiated, out-of-transcript credential handoff.
//!
//! Public records contain metadata only. Fulfillment moves the value into an
//! OpenShell provider or a one-run Grapheme capability, then wakes the tool
//! with an opaque, session-bound, one-use grant.

use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use medousa_types::{
    AgentSecretRequestBackend, AgentSecretRequestRecord, AgentSecretRequestStatus,
};
use once_cell::sync::Lazy;
use tokio::sync::oneshot;
use uuid::Uuid;
use zeroize::Zeroizing;

static STORE: Lazy<AgentSecretRequestStore> = Lazy::new(AgentSecretRequestStore::new);

const MAX_PROVIDER_TYPE_LEN: usize = 64;
const MAX_CREDENTIAL_KEY_LEN: usize = 128;
const MAX_LABEL_CHARS: usize = 80;
const MAX_REASON_CHARS: usize = 500;
const MAX_SECRET_BYTES: usize = 16 * 1024;
const MAX_GRANTS_PER_RUN: usize = 8;
const MAX_ALLOWED_HOSTS: usize = 16;
const GRANT_TTL_SECS: i64 = 10 * 60;

pub fn agent_secret_request_store() -> &'static AgentSecretRequestStore {
    &STORE
}

pub struct CreateAgentSecretRequest {
    pub turn_id: String,
    pub session_id: String,
    pub provider_type: String,
    pub credential_key: String,
    pub backend: AgentSecretRequestBackend,
    pub allowed_hosts: Vec<String>,
    pub label: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretRequestResolution {
    Granted { grant_id: String },
    Denied,
}

struct RequestState {
    record: AgentSecretRequestRecord,
    waiter: Option<oneshot::Sender<SecretRequestResolution>>,
    grant_id: Option<String>,
}

enum SecretGrantMaterial {
    OpenshellProvider {
        provider_name: String,
    },
    GraphemeRuntime {
        secret_name: String,
        allowed_hosts: Vec<String>,
        value: Option<Zeroizing<String>>,
    },
}

struct SecretGrant {
    session_id: String,
    material: SecretGrantMaterial,
    expires_at_utc: chrono::DateTime<Utc>,
    consumed: bool,
}

/// Secret material moved directly into one Grapheme execution scope. This type
/// deliberately implements neither `Debug` nor serialization.
pub struct GraphemeSecretMaterial {
    pub grant_id: String,
    pub secret_name: String,
    pub allowed_hosts: Vec<String>,
    pub value: Zeroizing<String>,
}

struct FulfillmentReservation {
    backend: AgentSecretRequestBackend,
    provider_type: String,
    credential_key: String,
    allowed_hosts: Vec<String>,
    provider_name: Option<String>,
}

struct RequestWaitGuard<'a> {
    store: &'a AgentSecretRequestStore,
    request_id: String,
    armed: bool,
}

impl Drop for RequestWaitGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            let _ = self
                .store
                .expire(&self.request_id, Some("tool_cancelled".to_string()));
        }
    }
}

#[derive(Default)]
struct StoreState {
    requests: HashMap<String, RequestState>,
    grants: HashMap<String, SecretGrant>,
}

pub struct AgentSecretRequestStore {
    state: Arc<Mutex<StoreState>>,
}

impl AgentSecretRequestStore {
    fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(StoreState::default())),
        }
    }

    pub fn create(
        &self,
        input: CreateAgentSecretRequest,
    ) -> Result<AgentSecretRequestRecord, String> {
        validate_provider_type(&input.provider_type)?;
        validate_credential_key(&input.credential_key)?;
        validate_text("label", &input.label, MAX_LABEL_CHARS)?;
        validate_text("reason", &input.reason, MAX_REASON_CHARS)?;
        let allowed_hosts = normalize_allowed_hosts(&input.allowed_hosts)?;
        if input.turn_id.trim().is_empty() || input.session_id.trim().is_empty() {
            return Err("secret request requires a turn and session".to_string());
        }

        let now = Utc::now();
        let record = AgentSecretRequestRecord {
            request_id: format!("asecret-{}", Uuid::new_v4().simple()),
            turn_id: input.turn_id,
            session_id: input.session_id,
            provider_type: input.provider_type,
            credential_key: input.credential_key,
            backend: input.backend,
            allowed_hosts,
            label: input.label,
            reason: input.reason,
            status: AgentSecretRequestStatus::Pending,
            created_at_utc: now,
            updated_at_utc: now,
            resolved_at_utc: None,
            resolved_by: None,
        };
        self.state
            .lock()
            .expect("agent secret request store")
            .requests
            .insert(
                record.request_id.clone(),
                RequestState {
                    record: record.clone(),
                    waiter: None,
                    grant_id: None,
                },
            );
        Ok(record)
    }

    pub fn list_pending(&self, limit: usize) -> Vec<AgentSecretRequestRecord> {
        self.list(limit, true)
    }

    pub fn list_all(&self, limit: usize) -> Vec<AgentSecretRequestRecord> {
        self.list(limit, false)
    }

    fn list(&self, limit: usize, pending_only: bool) -> Vec<AgentSecretRequestRecord> {
        let mut rows: Vec<_> = self
            .state
            .lock()
            .expect("agent secret request store")
            .requests
            .values()
            .filter(|state| {
                !pending_only
                    || matches!(
                        state.record.status,
                        AgentSecretRequestStatus::Pending | AgentSecretRequestStatus::Provisioning
                    )
            })
            .map(|state| state.record.clone())
            .collect();
        rows.sort_by_key(|row| Reverse(row.created_at_utc));
        rows.truncate(limit.clamp(1, 100));
        rows
    }

    fn timeout_secs() -> u64 {
        std::env::var("MEDOUSA_AGENT_SECRET_TIMEOUT_SECS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(300)
    }

    pub async fn wait_for_resolution(
        &self,
        request_id: &str,
    ) -> Result<SecretRequestResolution, String> {
        let (tx, rx) = oneshot::channel();
        let immediate = {
            let mut state = self.state.lock().expect("agent secret request store");
            let request = state
                .requests
                .get_mut(request_id)
                .ok_or_else(|| format!("secret request not found: {request_id}"))?;
            match request.record.status {
                AgentSecretRequestStatus::Pending | AgentSecretRequestStatus::Provisioning => {
                    request.waiter = Some(tx);
                    None
                }
                AgentSecretRequestStatus::Fulfilled => Some(SecretRequestResolution::Granted {
                    grant_id: request.grant_id.clone().ok_or_else(|| {
                        "fulfilled secret request is missing its grant".to_string()
                    })?,
                }),
                AgentSecretRequestStatus::Denied | AgentSecretRequestStatus::Expired => {
                    Some(SecretRequestResolution::Denied)
                }
            }
        };
        if let Some(resolution) = immediate {
            return Ok(resolution);
        }

        let mut guard = RequestWaitGuard {
            store: self,
            request_id: request_id.to_string(),
            armed: true,
        };
        let timeout = std::time::Duration::from_secs(Self::timeout_secs());
        let resolution = match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(resolution)) => Ok(resolution),
            Ok(Err(_)) => {
                let _ = self.expire(request_id, Some("waiter_dropped".to_string()));
                Ok(SecretRequestResolution::Denied)
            }
            Err(_) => {
                let _ = self.expire(request_id, Some("timeout".to_string()));
                Ok(SecretRequestResolution::Denied)
            }
        };
        guard.armed = false;
        resolution
    }

    /// Provision the value into OpenShell or move it into an ephemeral
    /// Grapheme grant. Raw material is always owned by a zeroizing buffer.
    pub async fn fulfill(
        &self,
        request_id: &str,
        value: String,
        resolved_by: Option<String>,
    ) -> Result<AgentSecretRequestRecord, String> {
        if value.len() > MAX_SECRET_BYTES {
            return Err(format!(
                "credential value exceeds the {MAX_SECRET_BYTES}-byte limit"
            ));
        }
        let secret = Zeroizing::new(value);
        if secret.trim().is_empty() {
            return Err("credential value must not be empty".to_string());
        }
        if secret.as_bytes().contains(&0) {
            return Err("credential value contains an unsupported NUL byte".to_string());
        }
        let reservation = self.reserve_fulfillment(request_id)?;

        if reservation.backend == AgentSecretRequestBackend::GraphemeRuntime {
            return self.complete_fulfillment(
                request_id,
                SecretGrantMaterial::GraphemeRuntime {
                    secret_name: reservation.credential_key,
                    allowed_hosts: reservation.allowed_hosts,
                    value: Some(secret),
                },
                resolved_by,
            );
        }

        #[cfg(not(feature = "full-daemon"))]
        {
            self.reopen_after_failed_fulfillment(request_id);
            return Err("OpenShell credentials require the desktop sidecar".to_string());
        }

        #[cfg(feature = "full-daemon")]
        {
        let provider_name = reservation
            .provider_name
            .ok_or_else(|| "OpenShell fulfillment is missing a provider name".to_string())?;
        let provider_type = reservation.provider_type;
        let credential_key = reservation.credential_key;
        let provisioned = match tokio::task::spawn_blocking({
            let provider_name = provider_name.clone();
            move || {
                crate::openshell_sandbox_run::provision_openshell_provider(
                    &provider_name,
                    &provider_type,
                    &credential_key,
                    secret,
                )
            }
        })
        .await
        {
            Ok(result) => result,
            Err(error) => {
                self.reopen_after_failed_fulfillment(request_id);
                return Err(format!("OpenShell provider task failed: {error}"));
            }
        };

        if let Err(error) = provisioned {
            self.reopen_after_failed_fulfillment(request_id);
            return Err(error);
        }
        let cleanup_provider_name = provider_name.clone();
        match self.complete_fulfillment(
            request_id,
            SecretGrantMaterial::OpenshellProvider { provider_name },
            resolved_by,
        ) {
            Ok(record) => Ok(record),
            Err(error) => {
                let cleanup = tokio::task::spawn_blocking(move || {
                    crate::openshell_sandbox_run::delete_openshell_provider(&cleanup_provider_name)
                })
                .await;
                match cleanup {
                    Ok(Ok(())) => Err(error),
                    Ok(Err(cleanup_error)) => Err(format!(
                        "{error}; OpenShell provider rollback also failed: {cleanup_error}"
                    )),
                    Err(cleanup_error) => Err(format!(
                        "{error}; OpenShell provider rollback task failed: {cleanup_error}"
                    )),
                }
            }
        }
        }
    }

    fn reserve_fulfillment(&self, request_id: &str) -> Result<FulfillmentReservation, String> {
        let mut state = self.state.lock().expect("agent secret request store");
        let request = state
            .requests
            .get_mut(request_id)
            .ok_or_else(|| format!("secret request not found: {request_id}"))?;
        if request.record.status != AgentSecretRequestStatus::Pending {
            return Err(format!("secret request {request_id} is not pending"));
        }
        request.record.status = AgentSecretRequestStatus::Provisioning;
        request.record.updated_at_utc = Utc::now();
        let provider_name =
            if request.record.backend == AgentSecretRequestBackend::OpenshellProvider {
                let suffix = Uuid::new_v4().simple().to_string();
                let provider_slug: String = request.record.provider_type.chars().take(32).collect();
                Some(format!("medousa-{provider_slug}-{}", &suffix[..12]))
            } else {
                None
            };
        Ok(FulfillmentReservation {
            backend: request.record.backend,
            provider_type: request.record.provider_type.clone(),
            credential_key: request.record.credential_key.clone(),
            allowed_hosts: request.record.allowed_hosts.clone(),
            provider_name,
        })
    }

    fn reopen_after_failed_fulfillment(&self, request_id: &str) {
        let mut state = self.state.lock().expect("agent secret request store");
        if let Some(request) = state.requests.get_mut(request_id)
            && request.record.status == AgentSecretRequestStatus::Provisioning
        {
            request.record.status = AgentSecretRequestStatus::Pending;
            request.record.updated_at_utc = Utc::now();
        }
    }

    fn complete_fulfillment(
        &self,
        request_id: &str,
        material: SecretGrantMaterial,
        resolved_by: Option<String>,
    ) -> Result<AgentSecretRequestRecord, String> {
        let grant_id = format!("sgrant-{}", Uuid::new_v4().simple());
        let (record, waiter) = {
            let mut state = self.state.lock().expect("agent secret request store");
            let request = state
                .requests
                .get_mut(request_id)
                .ok_or_else(|| format!("secret request not found: {request_id}"))?;
            if request.record.status != AgentSecretRequestStatus::Provisioning {
                return Err(format!("secret request {request_id} is not provisioning"));
            }
            let session_id = request.record.session_id.clone();
            let now = Utc::now();
            request.record.status = AgentSecretRequestStatus::Fulfilled;
            request.record.updated_at_utc = now;
            request.record.resolved_at_utc = Some(now);
            request.record.resolved_by = resolved_by;
            request.grant_id = Some(grant_id.clone());
            let waiter = request.waiter.take();
            let record = request.record.clone();
            state.grants.insert(
                grant_id.clone(),
                SecretGrant {
                    session_id,
                    material,
                    expires_at_utc: now + chrono::Duration::seconds(GRANT_TTL_SECS),
                    consumed: false,
                },
            );
            (record, waiter)
        };
        self.schedule_grant_expiration(grant_id.clone());
        if let Some(waiter) = waiter {
            let _ = waiter.send(SecretRequestResolution::Granted { grant_id });
        }
        Ok(record)
    }

    /// Ensure an approved-but-unused Grapheme value does not remain in memory
    /// indefinitely. Lazy purges still cover runtimes without an active Tokio
    /// handle (notably synchronous unit tests).
    fn schedule_grant_expiration(&self, grant_id: String) {
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let state = Arc::clone(&self.state);
        runtime.spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(GRANT_TTL_SECS as u64 + 1)).await;
            let mut state = state.lock().expect("agent secret request store");
            let expired = state
                .grants
                .get(&grant_id)
                .is_some_and(|grant| grant.expires_at_utc <= Utc::now());
            if expired {
                state.grants.remove(&grant_id);
            }
        });
    }

    pub fn deny(
        &self,
        request_id: &str,
        resolved_by: Option<String>,
    ) -> Result<AgentSecretRequestRecord, String> {
        self.resolve_without_value(request_id, AgentSecretRequestStatus::Denied, resolved_by)
    }

    fn expire(
        &self,
        request_id: &str,
        resolved_by: Option<String>,
    ) -> Result<AgentSecretRequestRecord, String> {
        self.resolve_without_value(request_id, AgentSecretRequestStatus::Expired, resolved_by)
    }

    fn resolve_without_value(
        &self,
        request_id: &str,
        status: AgentSecretRequestStatus,
        resolved_by: Option<String>,
    ) -> Result<AgentSecretRequestRecord, String> {
        let (record, waiter) = {
            let mut state = self.state.lock().expect("agent secret request store");
            let request = state
                .requests
                .get_mut(request_id)
                .ok_or_else(|| format!("secret request not found: {request_id}"))?;
            if !matches!(
                request.record.status,
                AgentSecretRequestStatus::Pending | AgentSecretRequestStatus::Provisioning
            ) {
                return Err(format!("secret request {request_id} is not pending"));
            }
            let now = Utc::now();
            request.record.status = status;
            request.record.updated_at_utc = now;
            request.record.resolved_at_utc = Some(now);
            request.record.resolved_by = resolved_by;
            (request.record.clone(), request.waiter.take())
        };
        if let Some(waiter) = waiter {
            let _ = waiter.send(SecretRequestResolution::Denied);
        }
        Ok(record)
    }

    /// Resolve opaque grants to provider names exactly once and only for their
    /// owning chat session. Validation is all-or-nothing.
    pub fn consume_openshell_grants(
        &self,
        grant_ids: &[String],
        session_id: &str,
    ) -> Result<Vec<String>, String> {
        if grant_ids.len() > MAX_GRANTS_PER_RUN {
            return Err(format!(
                "at most {MAX_GRANTS_PER_RUN} secret grants may be attached to one sandbox"
            ));
        }
        let mut seen = HashSet::new();
        for grant_id in grant_ids {
            if !grant_id.starts_with("sgrant-") || !seen.insert(grant_id.as_str()) {
                return Err("secret grants must be unique opaque grant ids".to_string());
            }
        }

        let mut state = self.state.lock().expect("agent secret request store");
        let now = Utc::now();
        state.grants.retain(|_, grant| grant.expires_at_utc > now);
        let mut providers = Vec::with_capacity(grant_ids.len());
        for grant_id in grant_ids {
            let grant = state
                .grants
                .get(grant_id)
                .ok_or_else(|| format!("secret grant is unknown or expired: {grant_id}"))?;
            if grant.session_id != session_id {
                return Err("secret grant belongs to a different chat session".to_string());
            }
            if grant.consumed {
                return Err("secret grant has already been used".to_string());
            }
            let SecretGrantMaterial::OpenshellProvider { provider_name } = &grant.material else {
                return Err("secret grant is not valid for an OpenShell sandbox".to_string());
            };
            providers.push(provider_name.clone());
        }
        for grant_id in grant_ids {
            if let Some(grant) = state.grants.get_mut(grant_id) {
                grant.consumed = true;
            }
        }
        Ok(providers)
    }

    /// Move secret material into exactly one Grapheme run. The value leaves the
    /// grant store here and is zeroized when the execution scope is dropped.
    pub fn consume_grapheme_grants(
        &self,
        grant_ids: &[String],
        session_id: &str,
    ) -> Result<Vec<GraphemeSecretMaterial>, String> {
        validate_grant_ids(grant_ids)?;
        let mut state = self.state.lock().expect("agent secret request store");
        let now = Utc::now();
        state.grants.retain(|_, grant| grant.expires_at_utc > now);

        for grant_id in grant_ids {
            let grant = state
                .grants
                .get(grant_id)
                .ok_or_else(|| format!("secret grant is unknown or expired: {grant_id}"))?;
            if grant.session_id != session_id {
                return Err("secret grant belongs to a different chat session".to_string());
            }
            if grant.consumed {
                return Err("secret grant has already been used".to_string());
            }
            match &grant.material {
                SecretGrantMaterial::GraphemeRuntime { value: Some(_), .. } => {}
                SecretGrantMaterial::GraphemeRuntime { value: None, .. } => {
                    return Err("Grapheme secret grant has no remaining value".to_string());
                }
                SecretGrantMaterial::OpenshellProvider { .. } => {
                    return Err("secret grant is not valid for Grapheme".to_string());
                }
            }
        }

        let mut materials = Vec::with_capacity(grant_ids.len());
        for grant_id in grant_ids {
            let grant = state.grants.get_mut(grant_id).expect("validated grant");
            let SecretGrantMaterial::GraphemeRuntime {
                secret_name,
                allowed_hosts,
                value,
            } = &mut grant.material
            else {
                unreachable!("validated Grapheme grant")
            };
            let value = value.take().expect("validated Grapheme value");
            grant.consumed = true;
            materials.push(GraphemeSecretMaterial {
                grant_id: grant_id.clone(),
                secret_name: secret_name.clone(),
                allowed_hosts: allowed_hosts.clone(),
                value,
            });
        }
        Ok(materials)
    }
}

fn validate_grant_ids(grant_ids: &[String]) -> Result<(), String> {
    if grant_ids.len() > MAX_GRANTS_PER_RUN {
        return Err(format!(
            "at most {MAX_GRANTS_PER_RUN} secret grants may be attached to one run"
        ));
    }
    let mut seen = HashSet::new();
    for grant_id in grant_ids {
        if !grant_id.starts_with("sgrant-") || !seen.insert(grant_id.as_str()) {
            return Err("secret grants must be unique opaque grant ids".to_string());
        }
    }
    Ok(())
}

pub fn validate_provider_type(value: &str) -> Result<(), String> {
    let valid = !value.is_empty()
        && value.len() <= MAX_PROVIDER_TYPE_LEN
        && value.is_ascii()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"-_.".contains(&byte)
        })
        && value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric);
    if valid {
        Ok(())
    } else {
        Err("provider_type must be a lowercase OpenShell profile id".to_string())
    }
}

pub fn validate_credential_key(value: &str) -> Result<(), String> {
    let valid = !value.is_empty()
        && value.len() <= MAX_CREDENTIAL_KEY_LEN
        && value.is_ascii()
        && value.as_bytes().first().is_some_and(u8::is_ascii_uppercase)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_');
    if valid {
        Ok(())
    } else {
        Err("credential_key must be an uppercase environment key".to_string())
    }
}

/// Normalize exact Grapheme egress authorities (`host` or `host:port`). Schemes,
/// paths, credentials, and wildcards are rejected so UI approval maps to one
/// unambiguous request boundary.
pub fn normalize_allowed_hosts(values: &[String]) -> Result<Vec<String>, String> {
    if values.len() > MAX_ALLOWED_HOSTS {
        return Err(format!(
            "at most {MAX_ALLOWED_HOSTS} credential destination hosts may be approved"
        ));
    }
    let mut normalized = Vec::with_capacity(values.len());
    let mut seen = HashSet::new();
    for raw in values {
        let value = raw.trim().to_ascii_lowercase();
        if value.is_empty()
            || value.len() > 253
            || !value.is_ascii()
            || value.contains("://")
            || value.bytes().any(|byte| b"/?#@*\\".contains(&byte))
        {
            return Err(format!(
                "allowed host must be an exact host or host:port authority: {raw}"
            ));
        }
        let (host, port) = match value.rsplit_once(':') {
            Some((host, port)) if !host.contains(':') => {
                let port = port
                    .parse::<u16>()
                    .map_err(|_| format!("allowed host has an invalid port: {raw}"))?;
                if port == 0 {
                    return Err(format!("allowed host has an invalid port: {raw}"));
                }
                (host, Some(port))
            }
            _ => (value.as_str(), None),
        };
        if host.is_empty()
            || host.starts_with('.')
            || host.ends_with('.')
            || host.contains("..")
            || !host
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'-')
        {
            return Err(format!("allowed host has invalid syntax: {raw}"));
        }
        let authority = match port {
            Some(port) => format!("{host}:{port}"),
            None => host.to_string(),
        };
        if seen.insert(authority.clone()) {
            normalized.push(authority);
        }
    }
    Ok(normalized)
}

fn validate_text(field: &str, value: &str, max_chars: usize) -> Result<(), String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{field} is required"));
    }
    if trimmed.chars().count() > max_chars {
        return Err(format!("{field} exceeds {max_chars} characters"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(store: &AgentSecretRequestStore) -> AgentSecretRequestRecord {
        store
            .create(CreateAgentSecretRequest {
                turn_id: "turn-1".to_string(),
                session_id: "session-1".to_string(),
                provider_type: "github".to_string(),
                credential_key: "GITHUB_TOKEN".to_string(),
                backend: AgentSecretRequestBackend::OpenshellProvider,
                allowed_hosts: Vec::new(),
                label: "GitHub token".to_string(),
                reason: "Read a private repository".to_string(),
            })
            .expect("create request")
    }

    #[test]
    fn records_never_have_a_secret_value_field() {
        let store = AgentSecretRequestStore::new();
        let record = request(&store);
        let json = serde_json::to_value(record).expect("record json");
        assert!(json.get("value").is_none());
        assert!(json.get("secret").is_none());
    }

    #[tokio::test]
    async fn fulfillment_before_wait_is_race_safe_and_grant_is_one_use() {
        let store = AgentSecretRequestStore::new();
        let record = request(&store);
        let provider_name = store
            .reserve_fulfillment(&record.request_id)
            .expect("reserve")
            .provider_name
            .expect("provider name");
        store
            .complete_fulfillment(
                &record.request_id,
                SecretGrantMaterial::OpenshellProvider { provider_name },
                Some("test".to_string()),
            )
            .expect("complete");
        let resolution = store
            .wait_for_resolution(&record.request_id)
            .await
            .expect("resolution");
        let SecretRequestResolution::Granted { grant_id } = resolution else {
            panic!("expected grant");
        };
        let first = store
            .consume_openshell_grants(std::slice::from_ref(&grant_id), "session-1")
            .expect("first consume");
        assert_eq!(first.len(), 1);
        assert!(
            store
                .consume_openshell_grants(&[grant_id], "session-1")
                .unwrap_err()
                .contains("already been used")
        );
    }

    #[test]
    fn grants_are_session_bound() {
        let store = AgentSecretRequestStore::new();
        let record = request(&store);
        let provider_name = store
            .reserve_fulfillment(&record.request_id)
            .expect("reserve")
            .provider_name
            .expect("provider name");
        store
            .complete_fulfillment(
                &record.request_id,
                SecretGrantMaterial::OpenshellProvider { provider_name },
                None,
            )
            .expect("complete");
        let grant_id = store
            .state
            .lock()
            .unwrap()
            .requests
            .get(&record.request_id)
            .unwrap()
            .grant_id
            .clone()
            .unwrap();
        assert!(
            store
                .consume_openshell_grants(&[grant_id], "session-2")
                .unwrap_err()
                .contains("different chat session")
        );
    }

    #[test]
    fn cancelled_wait_expires_the_pending_request() {
        let store = AgentSecretRequestStore::new();
        let record = request(&store);
        drop(RequestWaitGuard {
            store: &store,
            request_id: record.request_id.clone(),
            armed: true,
        });
        let state = store.state.lock().unwrap();
        assert_eq!(
            state.requests[&record.request_id].record.status,
            AgentSecretRequestStatus::Expired
        );
    }

    #[test]
    fn denial_can_win_a_provisioning_race_without_issuing_a_grant() {
        let store = AgentSecretRequestStore::new();
        let record = request(&store);
        let provider_name = store
            .reserve_fulfillment(&record.request_id)
            .expect("reserve")
            .provider_name
            .expect("provider name");
        store
            .deny(&record.request_id, Some("test".to_string()))
            .expect("deny");
        assert!(
            store
                .complete_fulfillment(
                    &record.request_id,
                    SecretGrantMaterial::OpenshellProvider { provider_name },
                    None,
                )
                .unwrap_err()
                .contains("not provisioning")
        );
        assert!(store.state.lock().unwrap().grants.is_empty());
    }

    #[test]
    fn expired_grants_cannot_be_consumed() {
        let store = AgentSecretRequestStore::new();
        let record = request(&store);
        let provider_name = store
            .reserve_fulfillment(&record.request_id)
            .expect("reserve")
            .provider_name
            .expect("provider name");
        store
            .complete_fulfillment(
                &record.request_id,
                SecretGrantMaterial::OpenshellProvider { provider_name },
                None,
            )
            .expect("complete");
        let grant_id = store.state.lock().unwrap().requests[&record.request_id]
            .grant_id
            .clone()
            .unwrap();
        store
            .state
            .lock()
            .unwrap()
            .grants
            .get_mut(&grant_id)
            .unwrap()
            .expires_at_utc = Utc::now() - chrono::Duration::seconds(1);
        assert!(
            store
                .consume_openshell_grants(&[grant_id], "session-1")
                .unwrap_err()
                .contains("unknown or expired")
        );
    }

    #[test]
    fn provider_and_credential_identifiers_are_closed() {
        assert!(validate_provider_type("openai-compatible").is_ok());
        assert!(validate_provider_type("../../host").is_err());
        assert!(validate_credential_key("OPENAI_API_KEY").is_ok());
        assert!(validate_credential_key("bad-key").is_err());
        assert_eq!(
            normalize_allowed_hosts(&[
                "API.Example.com:443".to_string(),
                "api.example.com:443".to_string()
            ])
            .unwrap(),
            vec!["api.example.com:443"]
        );
        assert!(normalize_allowed_hosts(&["https://example.com".to_string()]).is_err());
    }

    #[test]
    fn grapheme_grant_moves_zeroizing_material_once() {
        let store = AgentSecretRequestStore::new();
        let record = store
            .create(CreateAgentSecretRequest {
                turn_id: "turn-1".to_string(),
                session_id: "session-1".to_string(),
                provider_type: "grapheme".to_string(),
                credential_key: "EXAMPLE_API_KEY".to_string(),
                backend: AgentSecretRequestBackend::GraphemeRuntime,
                allowed_hosts: vec!["api.example.com".to_string()],
                label: "Example API key".to_string(),
                reason: "Call the approved API".to_string(),
            })
            .expect("create request");
        store
            .reserve_fulfillment(&record.request_id)
            .expect("reserve");
        store
            .complete_fulfillment(
                &record.request_id,
                SecretGrantMaterial::GraphemeRuntime {
                    secret_name: "EXAMPLE_API_KEY".to_string(),
                    allowed_hosts: vec!["api.example.com".to_string()],
                    value: Some(Zeroizing::new("super-secret".to_string())),
                },
                None,
            )
            .expect("complete");
        let grant_id = store.state.lock().unwrap().requests[&record.request_id]
            .grant_id
            .clone()
            .unwrap();
        let mut material = store
            .consume_grapheme_grants(std::slice::from_ref(&grant_id), "session-1")
            .expect("consume");
        assert_eq!(material.len(), 1);
        assert_eq!(material[0].value.as_str(), "super-secret");
        material.clear();
        let error = match store.consume_grapheme_grants(&[grant_id], "session-1") {
            Ok(_) => panic!("grant should be one-use"),
            Err(error) => error,
        };
        assert!(error.contains("already been used"));
    }
}
