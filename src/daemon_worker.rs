//! Daemon-owned outbound worker pairing and delegated-task transport.
use crate::{
    delegated_task::{
        DelegatedTaskControlObservation, DelegatedTaskControlRequest, DelegatedTaskError,
        DelegatedTaskObservation, DelegatedTaskRequest, DelegatedTaskTransport, delegated_work_id,
        validate_task_control_observation,
    },
    delegation::{AuthorizedDelegationTarget, DelegationTarget},
    mesh::{
        DEFAULT_ENVELOPE_TTL_SECS, MeshCapability, MeshEnvelopedRequest, payload_hash_hex,
        sign_envelope, verify_enveloped_payload,
    },
    workshop_contract::{
        ACTIVE_WORK_INVENTORY_SCHEMA_VERSION, ActiveWorkInventoryProbeRequest,
        ActiveWorkInventoryProbeResponse, EXECUTION_TARGET_INVENTORY_SCHEMA_VERSION,
        ExecutionTargetCandidate, ExecutionTargetProbeRequest, ExecutionTargetProbeResponse,
    },
};
use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use base64::Engine as _;
use chrono::Utc;
use ed25519_dalek::SigningKey;
use rand::{RngCore as _, rngs::OsRng};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest as _, Sha256};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
const VERSION: u32 = 1;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DaemonWorkerConnection {
    pub id: String,
    pub label: String,
    pub workshop_device_id: String,
    pub daemon_url: String,
    pub pairing_id: String,
    pub daemon_public_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iroh_ticket: Option<String>,
    pub connected_at: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Stored {
    #[serde(flatten)]
    summary: DaemonWorkerConnection,
    session_token: String,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Store {
    #[serde(default = "version")]
    version: u32,
    #[serde(default)]
    connections: Vec<Stored>,
}
const fn version() -> u32 {
    VERSION
}
struct Identity {
    device_id: String,
    key: SigningKey,
}
#[derive(Debug, Clone)]
pub struct DaemonWorkerPairing {
    root: PathBuf,
    client: reqwest::Client,
}
impl Default for DaemonWorkerPairing {
    fn default() -> Self {
        Self::new(crate::paths::medousa_data_dir().join("delegation"))
    }
}
impl DaemonWorkerPairing {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(20))
                .build()
                .expect("valid HTTP client"),
        }
    }
    pub fn storage_root(&self) -> &Path {
        &self.root
    }
    pub fn list(&self) -> Result<Vec<DaemonWorkerConnection>> {
        Ok(self
            .load()?
            .connections
            .into_iter()
            .map(|x| x.summary)
            .collect())
    }
    pub fn remove(&self, id: &str) -> Result<bool> {
        let mut s = self.load()?;
        let n = s.connections.len();
        s.connections.retain(|x| {
            x.summary.id != id
                && !x.summary.label.eq_ignore_ascii_case(id)
                && !x.summary.workshop_device_id.starts_with(id)
        });
        if n != s.connections.len() {
            self.save(&s)?;
        }
        Ok(n != s.connections.len())
    }
    pub async fn revoke_and_remove(&self, id: &str) -> Result<Option<DaemonWorkerConnection>> {
        let pairing = self.clone();
        let id = id.to_string();
        let stored = tokio::task::spawn_blocking(move || -> Result<Option<Stored>> {
            Ok(pairing.load()?.connections.into_iter().find(|connection| {
                connection.summary.id == id
                    || connection.summary.label.eq_ignore_ascii_case(&id)
                    || connection.summary.workshop_device_id.starts_with(&id)
            }))
        })
        .await
        .context("join worker credential lookup")??;
        let Some(stored) = stored else {
            return Ok(None);
        };
        let response = self
            .client
            .delete(format!(
                "{}/pair/{}",
                stored.summary.daemon_url, stored.summary.pairing_id
            ))
            .bearer_auth(&stored.session_token)
            .send()
            .await
            .context("revoke destination worker credential")?;
        if !response.status().is_success() && response.status() != reqwest::StatusCode::NOT_FOUND {
            bail!(
                "destination rejected worker credential revocation: {}: {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )
        }
        let summary = stored.summary;
        let pairing = self.clone();
        let remove_id = summary.id.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let mut store = pairing.load()?;
            store
                .connections
                .retain(|connection| connection.summary.id != remove_id);
            pairing.save(&store)
        })
        .await
        .context("join worker credential removal")??;
        Ok(Some(summary))
    }

    pub fn transport(&self) -> Arc<dyn DelegatedTaskTransport> {
        Arc::new(DaemonWorkerTransport::new(
            self.root.clone(),
            self.client.clone(),
        ))
    }
    pub async fn pair_from_qr(
        &self,
        raw: &str,
        base: Option<&str>,
        label: Option<&str>,
    ) -> Result<DaemonWorkerConnection> {
        let qr = Qr::parse(raw)?;
        let base = base
            .map(norm)
            .unwrap_or_else(|| url_from_address(&qr.address));
        let (remote_id, remote_name, remote_key) = if let Some(k) = qr.public_key.clone() {
            (qr.device_id.clone(), qr.name.clone(), k)
        } else {
            let s: Status = self.get(&base, "/pair/status", None).await?;
            (s.device_id, s.peer_name, s.daemon_public_key)
        };
        if remote_id != qr.device_id {
            bail!("pairing link does not match workshop device id")
        }
        qr.verify(&remote_key)?;
        let pairing = self.clone();
        let ident = tokio::task::spawn_blocking(move || pairing.identity())
            .await
            .context("join worker identity load")??;
        let public_key = crate::pairing::crypto::verifying_key_to_b64(&ident.key.verifying_key());
        let init:Init=self.client.post(format!("{base}/pair/init")).json(&serde_json::json!({"qrToken":qr.token,"phoneId":ident.device_id,"phoneName":label.filter(|x|!x.trim().is_empty()).unwrap_or("Medousa worker daemon"),"publicKey":public_key,"role":"portal"})).send().await?.error_for_status()?.json().await?;
        if init.status != "challenge" {
            bail!(
                "pair init failed: {}",
                init.reason.unwrap_or_else(|| "rejected".into())
            )
        }
        let nonce = init.server_nonce.context("pair init omitted nonce")?;
        let session = init.session_id.context("pair init omitted session")?;
        let signed = crate::pairing::crypto::sign_message(&ident.key, &nonce);
        let mut phone = [0; 32];
        OsRng.fill_bytes(&mut phone);
        let phone = b64(&phone);
        let done: Verify = self
            .client
            .post(format!("{base}/pair/verify"))
            .json(&serde_json::json!({"sessionId":session,"signedNonce":signed,"phoneNonce":phone}))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        if done.status != "paired" {
            bail!(
                "pair verify failed: {}",
                done.reason.unwrap_or_else(|| "rejected".into())
            )
        }
        crate::pairing::crypto::verify_message(
            &crate::pairing::crypto::parse_verifying_key(&remote_key)?,
            &phone,
            done.server_signed_nonce
                .as_deref()
                .context("pair verify omitted signature")?,
        )?;
        let token = done.session_token.context("pair verify omitted token")?;
        let pairing_id = done.pairing_id.context("pair verify omitted pairing id")?;
        let ticket = self
            .get::<Ticket>(&base, "/pair/iroh-ticket", Some(&token))
            .await
            .ok()
            .and_then(|x| x.ticket)
            .filter(|x| !x.trim().is_empty())
            .or(qr.ticket);
        let summary = DaemonWorkerConnection {
            id: format!("worker-{remote_id}"),
            label: label
                .filter(|x| !x.trim().is_empty())
                .unwrap_or(&remote_name)
                .to_string(),
            workshop_device_id: remote_id,
            daemon_url: base,
            pairing_id,
            daemon_public_key: remote_key,
            iroh_ticket: ticket,
            connected_at: Utc::now().to_rfc3339(),
        };
        let pairing = self.clone();
        let stored_summary = summary.clone();
        tokio::task::spawn_blocking(move || {
            let mut store = pairing.load()?;
            store
                .connections
                .retain(|connection| connection.summary.id != stored_summary.id);
            store.connections.push(Stored {
                summary: stored_summary,
                session_token: token,
            });
            pairing.save(&store)
        })
        .await
        .context("join worker credential store")??;
        Ok(summary)
    }
    async fn get<T: DeserializeOwned>(&self, b: &str, p: &str, t: Option<&str>) -> Result<T> {
        let mut r = self.client.get(format!("{b}{p}"));
        if let Some(t) = t {
            r = r.bearer_auth(t)
        }
        Ok(r.send().await?.error_for_status()?.json().await?)
    }
    fn load(&self) -> Result<Store> {
        let p = self.root.join("workers.json");
        if !p.is_file() {
            return Ok(Store::default());
        }
        let s: Store = serde_json::from_slice(&fs::read(&p)?)?;
        if s.version != VERSION {
            bail!("unsupported worker store version {}", s.version)
        }
        Ok(s)
    }
    fn save(&self, s: &Store) -> Result<()> {
        private_dir(&self.root)?;
        private_write(
            &self.root.join("workers.json"),
            &serde_json::to_vec_pretty(s)?,
        )
    }
    fn identity(&self) -> Result<Identity> {
        private_dir(&self.root)?;
        let p = self.root.join("identity.secret");
        let seed = if p.is_file() {
            decode_seed(fs::read_to_string(&p)?.trim())?
        } else {
            let mut x = [0; 32];
            OsRng.fill_bytes(&mut x);
            private_write(&p, format!("{}\n", hex(&x)).as_bytes())?;
            x
        };
        let key = SigningKey::from_bytes(&seed);
        let d = Sha256::digest(key.verifying_key().as_bytes());
        Ok(Identity {
            device_id: hex(&d[..4]),
            key,
        })
    }
}
#[derive(Debug)]
pub struct DaemonWorkerTransport {
    root: PathBuf,
    client: reqwest::Client,
    seq: AtomicU64,
}
impl DaemonWorkerTransport {
    pub fn new(root: PathBuf, client: reqwest::Client) -> Self {
        Self {
            root,
            client,
            seq: AtomicU64::new(Utc::now().timestamp_millis().max(0) as u64),
        }
    }
    fn all(&self) -> Result<(Identity, Vec<Stored>)> {
        let p = DaemonWorkerPairing {
            root: self.root.clone(),
            client: self.client.clone(),
        };
        Ok((p.identity()?, p.load()?.connections))
    }
    async fn all_async(&self) -> Result<(Identity, Vec<Stored>), DelegatedTaskError> {
        let transport = Self::new(self.root.clone(), self.client.clone());
        tokio::task::spawn_blocking(move || transport.all())
            .await
            .map_err(|error| DelegatedTaskError::internal(error.to_string()))?
            .map_err(|error| DelegatedTaskError::transport(error.to_string()))
    }

    fn target(&self, t: &DelegationTarget) -> Result<(Identity, Stored)> {
        let (i, s) = self.all()?;
        Ok((
            i,
            s.into_iter()
                .find(|x| {
                    x.summary.id == t.route_ref && x.summary.workshop_device_id == t.peer_device_id
                })
                .context("worker route is not paired by this daemon")?,
        ))
    }
    async fn target_async(
        &self,
        target: DelegationTarget,
    ) -> Result<(Identity, Stored), DelegatedTaskError> {
        let transport = Self::new(self.root.clone(), self.client.clone());
        tokio::task::spawn_blocking(move || transport.target(&target))
            .await
            .map_err(|error| DelegatedTaskError::internal(error.to_string()))?
            .map_err(|error| DelegatedTaskError::transport(error.to_string()))
    }

    async fn exchange<P: Serialize, R: Serialize + DeserializeOwned>(
        &self,
        s: &Stored,
        i: &Identity,
        path: &str,
        payload: P,
    ) -> Result<R, DelegatedTaskError> {
        let hash =
            payload_hash_hex(&payload).map_err(|e| DelegatedTaskError::internal(e.to_string()))?;
        let env = sign_envelope(
            &i.key,
            &i.device_id,
            &s.summary.workshop_device_id,
            self.seq.fetch_add(1, Ordering::Relaxed),
            MeshCapability::TaskRequest,
            &hash,
            chrono::Duration::seconds(DEFAULT_ENVELOPE_TTL_SECS),
        );
        let r = self
            .client
            .post(format!("{}{}", s.summary.daemon_url, path))
            .bearer_auth(&s.session_token)
            .json(&MeshEnvelopedRequest {
                envelope: env,
                payload,
            })
            .send()
            .await
            .map_err(|e| DelegatedTaskError::transport(e.to_string()))?;
        let status = r.status();
        if !status.is_success() {
            return Err(DelegatedTaskError::transport(format!(
                "worker HTTP {status}: {}",
                r.text().await.unwrap_or_default()
            )));
        }
        let wrapped: MeshEnvelopedRequest<R> = r
            .json()
            .await
            .map_err(|e| DelegatedTaskError::transport(e.to_string()))?;
        verify_enveloped_payload(
            &wrapped,
            &s.summary.daemon_public_key,
            &s.summary.workshop_device_id,
            &i.device_id,
            MeshCapability::TaskResult,
            true,
        )
        .map_err(|e| DelegatedTaskError::transport(e.to_string()))?;
        Ok(wrapped.payload)
    }
    async fn probe(
        &self,
        t: DelegationTarget,
    ) -> Result<AuthorizedDelegationTarget, DelegatedTaskError> {
        let (i, s) = self.target_async(t.clone()).await?;
        let r: ExecutionTargetProbeResponse = self
            .exchange(
                &s,
                &i,
                "/v1/mesh/execution-target",
                ExecutionTargetProbeRequest::default(),
            )
            .await?;
        if r.schema_version != EXECUTION_TARGET_INVENTORY_SCHEMA_VERSION
            || r.target.runtime_id.trim() != t.peer_device_id.trim()
        {
            return Err(DelegatedTaskError::transport(
                "execution target identity mismatch",
            ));
        }
        Ok(AuthorizedDelegationTarget {
            target: t,
            candidate: ExecutionTargetCandidate::from_inventory_entry(r.target),
            policy_revision: r.policy_revision,
        })
    }
}
#[async_trait]
impl DelegatedTaskTransport for DaemonWorkerTransport {
    async fn authorized_targets(
        &self,
    ) -> Result<Vec<AuthorizedDelegationTarget>, DelegatedTaskError> {
        let (_, s) = self.all_async().await?;
        let mut out = vec![];
        for x in s {
            let t = DelegationTarget {
                route_ref: x.summary.id,
                peer_device_id: x.summary.workshop_device_id,
                label: Some(x.summary.label),
            };
            if let Ok(Ok(p)) = tokio::time::timeout(Duration::from_secs(5), self.probe(t)).await {
                if p.candidate.user_selectable {
                    out.push(p)
                }
            }
        }
        out.sort_by(|a, b| {
            a.candidate
                .label
                .to_lowercase()
                .cmp(&b.candidate.label.to_lowercase())
        });
        Ok(out)
    }
    async fn active_work_inventories(
        &self,
        terminal: bool,
    ) -> Result<Vec<serde_json::Value>, DelegatedTaskError> {
        let (i, s) = self.all_async().await?;
        let mut out = vec![];
        for x in s {
            let target = serde_json::json!({"route_ref":x.summary.id,"execution_runtime_id":x.summary.workshop_device_id,"label":x.summary.label});
            let req = ActiveWorkInventoryProbeRequest {
                schema_version: ACTIVE_WORK_INVENTORY_SCHEMA_VERSION,
                include_terminal: terminal,
            };
            let row = match self
                .exchange::<_, ActiveWorkInventoryProbeResponse>(
                    &x,
                    &i,
                    "/v1/mesh/active-work",
                    req,
                )
                .await
            {
                Ok(r)
                    if r.schema_version == ACTIVE_WORK_INVENTORY_SCHEMA_VERSION
                        && r.inventory["coverage"]["execution_runtime_id"].as_str()
                            == Some(x.summary.workshop_device_id.as_str()) =>
                {
                    serde_json::json!({"available":true,"target":target,"inventory":r.inventory})
                }
                Ok(_) => {
                    serde_json::json!({"available":false,"target":target,"error":"active-work identity mismatch"})
                }
                Err(e) => {
                    serde_json::json!({"available":false,"target":target,"error":e.to_string()})
                }
            };
            out.push(row)
        }
        Ok(out)
    }
    async fn submit_or_observe(
        &self,
        t: &DelegationTarget,
        r: DelegatedTaskRequest,
    ) -> Result<DelegatedTaskObservation, DelegatedTaskError> {
        let (i, s) = self.target_async(t.clone()).await?;
        let expected = delegated_work_id(
            &i.device_id,
            r.grant
                .turn_id
                .as_deref()
                .ok_or_else(|| DelegatedTaskError::invalid("delegated turn id is missing"))?,
        );
        let o: DelegatedTaskObservation = self.exchange(&s, &i, "/v1/mesh/tasks", r).await?;
        if o.work_id != expected {
            return Err(DelegatedTaskError::transport(
                "delegated observation source mismatch",
            ));
        }
        if o.result.as_ref().is_some_and(|r| {
            r.terminal.participant_id.as_deref().map(str::trim)
                != Some(s.summary.workshop_device_id.trim())
        }) {
            return Err(DelegatedTaskError::transport(
                "terminal participant mismatch",
            ));
        }
        Ok(o)
    }
    async fn control(
        &self,
        t: &DelegationTarget,
        r: DelegatedTaskControlRequest,
    ) -> Result<DelegatedTaskControlObservation, DelegatedTaskError> {
        let (i, s) = self.target_async(t.clone()).await?;
        let p = format!("/v1/mesh/tasks/{}/control", r.work_id);
        let o = self.exchange(&s, &i, &p, r.clone()).await?;
        validate_task_control_observation(&r, &o)?;
        if o.destination_runtime_id.trim() != s.summary.workshop_device_id.trim() {
            return Err(DelegatedTaskError::transport(
                "control destination mismatch",
            ));
        }
        Ok(o)
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Status {
    device_id: String,
    peer_name: String,
    daemon_public_key: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Init {
    status: String,
    server_nonce: Option<String>,
    session_id: Option<String>,
    reason: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Verify {
    status: String,
    server_signed_nonce: Option<String>,
    session_token: Option<String>,
    pairing_id: Option<String>,
    reason: Option<String>,
}
#[derive(Deserialize)]
struct Ticket {
    ticket: Option<String>,
}
struct Qr {
    address: String,
    device_id: String,
    token: String,
    signature: String,
    name: String,
    public_key: Option<String>,
    ticket: Option<String>,
    profile: Option<String>,
}
impl Qr {
    fn parse(raw: &str) -> Result<Self> {
        if !raw.trim().starts_with("medousa://pair/") {
            bail!("pairing URL must use medousa://pair/")
        }
        let mut m = HashMap::new();
        for p in raw.split_once('?').map(|x| x.1).unwrap_or("").split('&') {
            if let Some((k, v)) = p.split_once('=') {
                m.insert(k, urlencoding::decode(v)?.into_owned());
            }
        }
        let get = |k| m.get(k).cloned().filter(|v| !v.trim().is_empty());
        Ok(Self {
            address: get("a").context("missing a")?,
            device_id: get("d").context("missing d")?,
            token: get("t").context("missing t")?,
            signature: get("s").context("missing s")?,
            name: get("n").unwrap_or_else(|| "Worker daemon".into()),
            public_key: get("u"),
            ticket: get("k"),
            profile: get("p"),
        })
    }
    fn verify(&self, key: &str) -> Result<()> {
        let msg = match (&self.ticket, &self.profile) {
            (Some(k), Some(p)) => format!(
                "{}|{}|{}|{}|p:{}",
                self.address, self.device_id, self.token, k, p
            ),
            (Some(k), None) => format!("{}|{}|{}|{}", self.address, self.device_id, self.token, k),
            (None, Some(p)) => {
                format!("{}|{}|{}|p:{}", self.address, self.device_id, self.token, p)
            }
            (None, None) => format!("{}|{}|{}", self.address, self.device_id, self.token),
        };
        crate::pairing::crypto::verify_message(
            &crate::pairing::crypto::parse_verifying_key(key)?,
            &msg,
            &self.signature,
        )
    }
}
fn norm(x: &str) -> String {
    x.trim().trim_end_matches('/').into()
}
fn url_from_address(x: &str) -> String {
    let x = x.trim().trim_end_matches('/');
    if x.starts_with("http://") || x.starts_with("https://") {
        x.into()
    } else {
        format!("http://{x}")
    }
}
fn b64(x: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(x)
}
fn hex(x: &[u8]) -> String {
    x.iter().map(|b| format!("{b:02x}")).collect()
}
fn decode_seed(x: &str) -> Result<[u8; 32]> {
    let b = if x.len() == 64 && x.chars().all(|c| c.is_ascii_hexdigit()) {
        (0..64)
            .step_by(2)
            .map(|i| u8::from_str_radix(&x[i..i + 2], 16))
            .collect::<std::result::Result<Vec<_>, _>>()?
    } else {
        base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(x)?
    };
    b.try_into()
        .map_err(|_| anyhow::anyhow!("identity seed must be 32 bytes"))
}
fn private_dir(p: &Path) -> Result<()> {
    fs::create_dir_all(p)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(p, fs::Permissions::from_mode(0o700))?
    }
    Ok(())
}
fn private_write(p: &Path, b: &[u8]) -> Result<()> {
    fs::write(p, b)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(p, fs::Permissions::from_mode(0o600))?
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairDaemonWorkerRequest {
    pub pairing_url: String,
    #[serde(default)]
    pub daemon_url: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
}

pub async fn list_daemon_workers(
    axum::extract::State(state): axum::extract::State<crate::daemon::state::AppState>,
) -> Result<axum::Json<Vec<DaemonWorkerConnection>>, (axum::http::StatusCode, String)> {
    let pairing = state.daemon_workers.clone();
    tokio::task::spawn_blocking(move || pairing.list())
        .await
        .map_err(|error| worker_api_error(anyhow::anyhow!(error.to_string())))?
        .map(axum::Json)
        .map_err(worker_api_error)
}

pub async fn pair_daemon_worker(
    axum::extract::State(state): axum::extract::State<crate::daemon::state::AppState>,
    axum::Json(request): axum::Json<PairDaemonWorkerRequest>,
) -> Result<axum::Json<DaemonWorkerConnection>, (axum::http::StatusCode, String)> {
    if request.pairing_url.trim().is_empty() {
        return Err((
            axum::http::StatusCode::BAD_REQUEST,
            "pairingUrl is required".to_string(),
        ));
    }
    state
        .daemon_workers
        .pair_from_qr(
            &request.pairing_url,
            request.daemon_url.as_deref(),
            request.label.as_deref(),
        )
        .await
        .map(axum::Json)
        .map_err(worker_api_error)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectDaemonWorkerRequest {
    #[serde(default)]
    pub runtime_id: Option<String>,
}

pub async fn select_daemon_worker(
    axum::extract::State(state): axum::extract::State<crate::daemon::state::AppState>,
    axum::Json(request): axum::Json<SelectDaemonWorkerRequest>,
) -> Result<axum::Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let service = state.platform.delegation_service().ok_or_else(|| {
        (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "daemon delegation runtime is unavailable".to_string(),
        )
    })?;
    let runtime_id = request
        .runtime_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty());
    let Some(runtime_id) = runtime_id else {
        let cleared = service.clear().await.map_err(worker_api_error)?;
        return Ok(axum::Json(serde_json::json!({ "cleared": cleared })));
    };
    let authorized = service
        .authorized_targets()
        .await
        .map_err(|error| worker_api_error(anyhow::anyhow!(error.to_string())))?;
    let target = authorized
        .into_iter()
        .find(|candidate| candidate.candidate.runtime_id == runtime_id)
        .ok_or_else(|| {
            (
                axum::http::StatusCode::NOT_FOUND,
                format!("authorized worker target not found: {runtime_id}"),
            )
        })?;
    let binding = service
        .bind(target.target)
        .await
        .map_err(worker_api_error)?;
    Ok(axum::Json(serde_json::json!({ "binding": binding })))
}

pub async fn remove_daemon_worker(
    axum::extract::State(state): axum::extract::State<crate::daemon::state::AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let removed = state
        .daemon_workers
        .revoke_and_remove(&id)
        .await
        .map_err(worker_api_error)?;
    let Some(removed) = removed else {
        return Err((
            axum::http::StatusCode::NOT_FOUND,
            format!("worker not found: {id}"),
        ));
    };
    if let Some(service) = state.platform.delegation_service()
        && service
            .binding()
            .await
            .ok()
            .flatten()
            .is_some_and(|binding| {
                binding.target.route_ref == removed.id
                    || binding.target.peer_device_id == removed.workshop_device_id
            })
    {
        service.clear().await.map_err(worker_api_error)?;
    }
    Ok(axum::Json(serde_json::json!({ "removed": true })))
}

fn worker_api_error(error: anyhow::Error) -> (axum::http::StatusCode, String) {
    (
        axum::http::StatusCode::BAD_GATEWAY,
        format!("daemon worker operation failed: {error:#}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    fn sample() -> DaemonWorkerConnection {
        DaemonWorkerConnection {
            id: "worker-abcd1234".into(),
            label: "Studio".into(),
            workshop_device_id: "abcd1234".into(),
            daemon_url: "http://x".into(),
            pairing_id: "p".into(),
            daemon_public_key: "k".into(),
            iroh_ticket: None,
            connected_at: "now".into(),
        }
    }
    #[test]
    fn store_redacts_token_and_is_private() {
        let d = tempdir().unwrap();
        let p = DaemonWorkerPairing::new(d.path().into());
        p.save(&Store {
            version: VERSION,
            connections: vec![Stored {
                summary: sample(),
                session_token: "secret".into(),
            }],
        })
        .unwrap();
        assert!(
            !serde_json::to_string(&p.list().unwrap())
                .unwrap()
                .contains("secret")
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(d.path().join("workers.json"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            )
        }
    }
    #[test]
    fn identity_is_stable() {
        let d = tempdir().unwrap();
        let p = DaemonWorkerPairing::new(d.path().into());
        assert_eq!(
            p.identity().unwrap().device_id,
            p.identity().unwrap().device_id
        )
    }
    #[test]
    fn remove_by_label() {
        let d = tempdir().unwrap();
        let p = DaemonWorkerPairing::new(d.path().into());
        p.save(&Store {
            version: VERSION,
            connections: vec![Stored {
                summary: sample(),
                session_token: "s".into(),
            }],
        })
        .unwrap();
        assert!(p.remove("studio").unwrap());
        assert!(p.list().unwrap().is_empty())
    }
}
