//! Production mesh transport for federated work-environment jobs.
//!
//! The HTTP route is only transport. Stasis owns the remote envelope and
//! terminal-result identities; `BlobTransferPort` owns immutable bytes; the
//! existing pairing identity signs both the mesh delivery and the inner Stasis
//! object. No bearer secret has to be copied between daemons.

use std::sync::Arc;

use async_trait::async_trait;
use axum::Json;
use axum::body::{Body, Bytes};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use base64::Engine as _;
use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};
use stasis::domain::runtime::blob_descriptor::BlobDescriptor;
use stasis::domain::runtime::federation::FederatedTerminalResult;
use stasis::domain::runtime::remote_job_envelope::{EnvelopeSignature, RemoteJobEnvelope};
use stasis::ports::outbound::runtime::blob_transfer::BlobTransferPort;
use stasis::prelude::{RuntimeComposition, StasisError};

use crate::daemon::route_policy::{
    BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RoutePolicy,
};
use crate::mesh::delivery;
use crate::mesh::envelope::{
    DEFAULT_ENVELOPE_TTL_SECS, MESH_ENVELOPE_HEADER, MeshCapability, MeshEnvelope,
    MeshEnvelopedRequest, encode_envelope_header, payload_hash_hex, sign_envelope,
    verify_enveloped_payload,
};
use crate::mesh::{record_has_capability, registry};
use crate::pairing::crypto::{base64url_encode, parse_verifying_key, sign_message, verify_message};
use crate::pairing::{PairedDeviceRecord, PairingService};
use crate::work_environment_federation::{
    SignedFederatedTerminalDelivery, accept_remote_work_environment_job,
};
use crate::work_environment_job::WorkEnvironmentJobPayload;
use medousa_runtime::WorkEnvironmentCheckpointManifest;

const ED25519_ALGORITHM: &str = "ed25519";
const FEDERATED_TERMINAL_MEDIA_TYPE: &str = "application/vnd.stasis.federated-terminal-result+json";
const BLOB_DESCRIPTOR_HEADER: &str = "x-medousa-blob-descriptor";
const MAX_FEDERATED_BLOB_BYTES: usize = 512 * 1024 * 1024;

#[derive(Clone)]
pub struct MeshWorkEnvironmentFederationState {
    pub pairing: Arc<PairingService>,
    pub runtime: Arc<RuntimeComposition>,
    pub blobs: Arc<dyn BlobTransferPort>,
    pub accept_remote_jobs: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteWorkEnvironmentAdmission {
    pub envelope_id: String,
    pub job_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FederatedTerminalAdmission {
    pub result_id: String,
    pub stored: BlobDescriptor,
}

pub fn surface() -> DeclaredRouter<MeshWorkEnvironmentFederationState> {
    DeclaredRouter::default()
        .route(
            peer_policy(
                axum::http::Method::POST,
                "/v1/mesh/federation/work-environments",
                1024 * 1024,
            ),
            post(accept_remote_job),
        )
        .route(
            peer_policy(
                axum::http::Method::POST,
                "/v1/mesh/federation/terminal-results",
                1024 * 1024,
            ),
            post(accept_terminal_result),
        )
        .methods([
            (
                peer_policy(
                    axum::http::Method::PUT,
                    "/v1/mesh/federation/blobs/{digest}",
                    MAX_FEDERATED_BLOB_BYTES,
                ),
                put(put_blob),
            ),
            (
                peer_policy(
                    axum::http::Method::GET,
                    "/v1/mesh/federation/blobs/{digest}",
                    1024,
                ),
                get(get_blob),
            ),
        ])
}

fn peer_policy(method: axum::http::Method, path: &'static str, body_limit: usize) -> RoutePolicy {
    RoutePolicy {
        method,
        path,
        group: RouteGroup::PeerExchange,
        required_capability: Some(crate::request_principal::Capability::PeerExchange),
        bootstrap_public: false,
        browser_policy: BrowserPolicy::NativeOnly,
        body_limit,
        rate_limit_class: RateLimitClass::PeerExchange,
    }
}

fn signing_message(kind: &str, canonical: &[u8]) -> String {
    format!(
        "medousa-federation/v1|{kind}|{}",
        base64url_encode(canonical)
    )
}

pub fn sign_remote_job(
    envelope: &mut RemoteJobEnvelope,
    device_id: &str,
    signing_key: &SigningKey,
) -> Result<(), String> {
    let canonical = envelope.canonical_bytes()?;
    envelope.signature = EnvelopeSignature {
        algorithm: ED25519_ALGORITHM.to_string(),
        key_id: device_id.trim().to_string(),
        signature_hex: sign_message(signing_key, &signing_message("remote-job", &canonical)),
    };
    Ok(())
}

pub fn verify_remote_job(
    envelope: &RemoteJobEnvelope,
    expected_device_id: &str,
    public_key_b64: &str,
) -> Result<(), String> {
    envelope.validate_schema_version()?;
    verify_signature(
        &envelope.signature,
        expected_device_id,
        public_key_b64,
        "remote-job",
        &envelope.canonical_bytes()?,
    )
}

pub fn sign_terminal_result(
    result: &mut FederatedTerminalResult,
    device_id: &str,
    signing_key: &SigningKey,
) -> Result<(), String> {
    result.validate_schema_version()?;
    let canonical = result.canonical_bytes()?;
    result.signature = EnvelopeSignature {
        algorithm: ED25519_ALGORITHM.to_string(),
        key_id: device_id.trim().to_string(),
        signature_hex: sign_message(signing_key, &signing_message("terminal-result", &canonical)),
    };
    Ok(())
}

pub fn verify_terminal_result(
    result: &FederatedTerminalResult,
    expected_device_id: &str,
    public_key_b64: &str,
) -> Result<(), String> {
    result.validate_schema_version()?;
    verify_signature(
        &result.signature,
        expected_device_id,
        public_key_b64,
        "terminal-result",
        &result.canonical_bytes()?,
    )
}

fn verify_signature(
    signature: &EnvelopeSignature,
    expected_device_id: &str,
    public_key_b64: &str,
    kind: &str,
    canonical: &[u8],
) -> Result<(), String> {
    if signature.algorithm != ED25519_ALGORITHM
        || signature.key_id.trim() != expected_device_id.trim()
    {
        return Err("federated signature identity does not match the paired peer".to_string());
    }
    let key = parse_verifying_key(public_key_b64).map_err(|error| error.to_string())?;
    verify_message(
        &key,
        &signing_message(kind, canonical),
        &signature.signature_hex,
    )
    .map_err(|error| error.to_string())
}

async fn accept_remote_job(
    State(state): State<MeshWorkEnvironmentFederationState>,
    Json(wrapped): Json<MeshEnvelopedRequest<RemoteJobEnvelope>>,
) -> Result<Response, (StatusCode, String)> {
    if !state.accept_remote_jobs {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "this daemon does not advertise OCI work-environment execution".to_string(),
        ));
    }
    let (record, receipt) =
        authenticate_delivery(&state.pairing, &wrapped, MeshCapability::TaskRequest)?;
    verify_remote_job(&wrapped.payload, &record.phone_id, &record.phone_public_key)
        .map_err(|error| (StatusCode::UNAUTHORIZED, error))?;
    if wrapped.payload.origin_authority.runtime_id.trim() != record.phone_id.trim() {
        return Err((
            StatusCode::UNAUTHORIZED,
            "remote job origin does not match the authenticated peer".to_string(),
        ));
    }
    let job_id = accept_remote_work_environment_job(
        state.runtime.as_ref(),
        state.blobs.as_ref(),
        &wrapped.payload,
    )
    .await
    .map_err(map_stasis)?;
    delivery::bind_delivery_local_ref(&receipt.inbox_id, &job_id, &receipt.receipt.id)
        .map_err(internal)?;
    signed_json_response(
        &state.pairing,
        &record,
        MeshCapability::TaskResult,
        RemoteWorkEnvironmentAdmission {
            envelope_id: wrapped.payload.envelope_id,
            job_id,
        },
        &receipt.receipt,
    )
}

async fn accept_terminal_result(
    State(state): State<MeshWorkEnvironmentFederationState>,
    Json(wrapped): Json<MeshEnvelopedRequest<FederatedTerminalResult>>,
) -> Result<Response, (StatusCode, String)> {
    let (record, receipt) =
        authenticate_delivery(&state.pairing, &wrapped, MeshCapability::TaskResult)?;
    verify_terminal_result(&wrapped.payload, &record.phone_id, &record.phone_public_key)
        .map_err(|error| (StatusCode::UNAUTHORIZED, error))?;
    if wrapped.payload.origin_authority.runtime_id.trim() != state.pairing.device_id().trim() {
        return Err((
            StatusCode::UNAUTHORIZED,
            "terminal result is not addressed to this runtime".to_string(),
        ));
    }
    let bytes = serde_json::to_vec(&wrapped.payload).map_err(internal)?;
    let stored = state
        .blobs
        .put(&bytes, Some(FEDERATED_TERMINAL_MEDIA_TYPE))
        .await
        .map_err(map_stasis)?;
    let local_ref = serde_json::to_string(&stored).map_err(internal)?;
    delivery::bind_delivery_local_ref(&receipt.inbox_id, &local_ref, &receipt.receipt.id)
        .map_err(internal)?;
    signed_json_response(
        &state.pairing,
        &record,
        MeshCapability::TaskResult,
        FederatedTerminalAdmission {
            result_id: wrapped.payload.result_id,
            stored,
        },
        &receipt.receipt,
    )
}

async fn put_blob(
    State(state): State<MeshWorkEnvironmentFederationState>,
    Path(digest): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, (StatusCode, String)> {
    let (record, envelope, descriptor) = authenticate_blob_request(&state, &headers, &digest)?;
    if body.len() > MAX_FEDERATED_BLOB_BYTES || !descriptor.verify(&body) {
        return Err((
            StatusCode::BAD_REQUEST,
            "federated blob failed size or digest verification".to_string(),
        ));
    }
    let payload_hash = payload_hash_hex(&descriptor)
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
    let accepted = delivery::accept_inbound_delivery(
        state.pairing.identity().signing_key(),
        state.pairing.device_id(),
        &envelope,
        &payload_hash,
    )
    .map_err(internal)?;
    let stored = state
        .blobs
        .put(&body, descriptor.media_type.as_deref())
        .await
        .map_err(map_stasis)?;
    if stored.digest != descriptor.digest || stored.size_bytes != descriptor.size_bytes {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "blob store changed the federated content identity".to_string(),
        ));
    }
    let local_ref = serde_json::to_string(&stored).map_err(internal)?;
    delivery::bind_delivery_local_ref(&accepted.inbox_id, &local_ref, &accepted.receipt.id)
        .map_err(internal)?;
    let receipt = delivery::receipt_header_value(&accepted.receipt).map_err(internal)?;
    let mut response = StatusCode::NO_CONTENT.into_response();
    response.headers_mut().insert(
        "x-medousa-mesh-receipt",
        HeaderValue::from_str(&receipt).map_err(internal)?,
    );
    let _ = record;
    Ok(response)
}

async fn get_blob(
    State(state): State<MeshWorkEnvironmentFederationState>,
    Path(digest): Path<String>,
    headers: HeaderMap,
) -> Result<Response, (StatusCode, String)> {
    let (_record, envelope, descriptor) = authenticate_blob_request(&state, &headers, &digest)?;
    let payload_hash = payload_hash_hex(&descriptor)
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
    let accepted = delivery::accept_inbound_delivery(
        state.pairing.identity().signing_key(),
        state.pairing.device_id(),
        &envelope,
        &payload_hash,
    )
    .map_err(internal)?;
    let bytes = state.blobs.get(&descriptor).await.map_err(map_stasis)?;
    let receipt = delivery::receipt_header_value(&accepted.receipt).map_err(internal)?;
    let mut response = Response::new(Body::from(bytes));
    response.headers_mut().insert(
        "x-medousa-mesh-receipt",
        HeaderValue::from_str(&receipt).map_err(internal)?,
    );
    if let Some(media_type) = descriptor.media_type.as_deref() {
        response.headers_mut().insert(
            axum::http::header::CONTENT_TYPE,
            HeaderValue::from_str(media_type).map_err(internal)?,
        );
    }
    Ok(response)
}

fn authenticate_blob_request(
    state: &MeshWorkEnvironmentFederationState,
    headers: &HeaderMap,
    path_digest: &str,
) -> Result<(PairedDeviceRecord, MeshEnvelope, BlobDescriptor), (StatusCode, String)> {
    let envelope = mesh_envelope_header(headers)?;
    let descriptor = blob_descriptor_header(headers)?;
    if descriptor.digest.hex.trim() != path_digest.trim() {
        return Err((
            StatusCode::BAD_REQUEST,
            "blob route digest does not match its descriptor".to_string(),
        ));
    }
    let record = paired_record(&state.pairing, &envelope.sender_device_id)?;
    verify_enveloped_payload(
        &MeshEnvelopedRequest {
            envelope: envelope.clone(),
            payload: descriptor.clone(),
        },
        &record.phone_public_key,
        &record.phone_id,
        state.pairing.device_id(),
        MeshCapability::BundlePush,
        record_has_capability(&record, MeshCapability::BundlePush.as_str()),
    )
    .map_err(|error| (StatusCode::UNAUTHORIZED, error.to_string()))?;
    Ok((record, envelope, descriptor))
}

fn mesh_envelope_header(headers: &HeaderMap) -> Result<MeshEnvelope, (StatusCode, String)> {
    let raw = headers
        .get(MESH_ENVELOPE_HEADER)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                "signed mesh envelope header required".to_string(),
            )
        })?;
    crate::mesh::decode_envelope_header(raw)
        .map_err(|error| (StatusCode::UNAUTHORIZED, error.to_string()))
}

fn blob_descriptor_header(headers: &HeaderMap) -> Result<BlobDescriptor, (StatusCode, String)> {
    let raw = headers
        .get(BLOB_DESCRIPTOR_HEADER)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                "federated blob descriptor header required".to_string(),
            )
        })?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(raw.trim())
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
    serde_json::from_slice(&bytes).map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))
}

fn authenticate_delivery<T: Serialize>(
    pairing: &PairingService,
    wrapped: &MeshEnvelopedRequest<T>,
    capability: MeshCapability,
) -> Result<(PairedDeviceRecord, delivery::InboundMeshAccept), (StatusCode, String)> {
    let record = paired_record(pairing, &wrapped.envelope.sender_device_id)?;
    verify_enveloped_payload(
        wrapped,
        &record.phone_public_key,
        &record.phone_id,
        pairing.device_id(),
        capability,
        record_has_capability(&record, capability.as_str()),
    )
    .map_err(|error| (StatusCode::UNAUTHORIZED, error.to_string()))?;
    let payload_hash = payload_hash_hex(&wrapped.payload)
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
    let accepted = delivery::accept_inbound_delivery(
        pairing.identity().signing_key(),
        pairing.device_id(),
        &wrapped.envelope,
        &payload_hash,
    )
    .map_err(internal)?;
    Ok((record, accepted))
}

fn paired_record(
    pairing: &PairingService,
    sender: &str,
) -> Result<PairedDeviceRecord, (StatusCode, String)> {
    pairing
        .find_by_phone_id(sender)
        .map_err(internal)?
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                "federated sender is not paired".to_string(),
            )
        })
}

fn signed_json_response<T: Serialize>(
    pairing: &PairingService,
    recipient: &PairedDeviceRecord,
    capability: MeshCapability,
    payload: T,
    receipt: &crate::mesh::MeshReceipt,
) -> Result<Response, (StatusCode, String)> {
    let payload_hash = payload_hash_hex(&payload)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    let seq = registry::allocate_outbound_seq(&recipient.phone_id).map_err(internal)?;
    let envelope = sign_envelope(
        pairing.identity().signing_key(),
        pairing.device_id(),
        &recipient.phone_id,
        seq,
        capability,
        &payload_hash,
        chrono::Duration::seconds(DEFAULT_ENVELOPE_TTL_SECS),
    );
    let receipt = delivery::receipt_header_value(receipt).map_err(internal)?;
    Ok((
        [("x-medousa-mesh-receipt", receipt)],
        Json(MeshEnvelopedRequest { envelope, payload }),
    )
        .into_response())
}

fn map_stasis(error: StasisError) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, error.to_string())
}

fn internal(error: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}

/// Destination-side durable terminal sender. The origin peer is selected from
/// the signed Stasis result, then resolved through the existing mesh registry.
pub struct MeshSignedFederatedTerminalDelivery {
    pairing: Arc<PairingService>,
    client: reqwest::Client,
}

/// Source-side sender used by coordinators. It moves the immutable input graph
/// first, then submits the signed Stasis envelope. Destination selection stays
/// outside this adapter.
pub struct MeshWorkEnvironmentFederationTransport {
    pairing: Arc<PairingService>,
    blobs: Arc<dyn BlobTransferPort>,
    client: reqwest::Client,
}

impl MeshWorkEnvironmentFederationTransport {
    pub fn new(pairing: Arc<PairingService>, blobs: Arc<dyn BlobTransferPort>) -> Self {
        Self {
            pairing,
            blobs,
            client: reqwest::Client::new(),
        }
    }

    pub async fn submit_remote_job(
        &self,
        target_runtime_id: &str,
        mut envelope: RemoteJobEnvelope,
    ) -> stasis::prelude::Result<RemoteWorkEnvironmentAdmission> {
        let local_runtime_id = self.pairing.device_id();
        if envelope.origin_authority.runtime_id.trim() != local_runtime_id.trim()
            || envelope.terminal_delivery.protocol != "medousa-mesh-v1"
            || envelope.terminal_delivery.address.trim() != local_runtime_id.trim()
        {
            return Err(StasisError::PortFailure(
                "remote job origin or terminal endpoint does not match this runtime".to_string(),
            ));
        }
        self.transfer_input_graph(target_runtime_id, &envelope.payload)
            .await?;
        sign_remote_job(
            &mut envelope,
            local_runtime_id,
            self.pairing.identity().signing_key(),
        )
        .map_err(StasisError::PortFailure)?;
        self.post_signed(
            target_runtime_id,
            MeshCapability::TaskRequest,
            "/v1/mesh/federation/work-environments",
            envelope,
        )
        .await
    }

    pub async fn fetch_blob(
        &self,
        target_runtime_id: &str,
        descriptor: &BlobDescriptor,
    ) -> stasis::prelude::Result<BlobDescriptor> {
        let peer = self.peer(target_runtime_id)?;
        let base = peer_base(&peer)?;
        let envelope =
            self.sign_mesh_payload(target_runtime_id, MeshCapability::BundlePush, descriptor)?;
        let response = self
            .client
            .get(format!(
                "{}/v1/mesh/federation/blobs/{}",
                base, descriptor.digest.hex
            ))
            .header(
                MESH_ENVELOPE_HEADER,
                encode_envelope_header(&envelope)
                    .map_err(|error| StasisError::PortFailure(error.to_string()))?,
            )
            .header(
                BLOB_DESCRIPTOR_HEADER,
                encode_blob_descriptor_header(descriptor).map_err(StasisError::PortFailure)?,
            )
            .send()
            .await
            .map_err(|error| StasisError::PortFailure(error.to_string()))?;
        if !response.status().is_success() {
            return Err(http_failure("blob fetch", response).await);
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|error| StasisError::PortFailure(error.to_string()))?;
        if bytes.len() > MAX_FEDERATED_BLOB_BYTES || !descriptor.verify(&bytes) {
            return Err(StasisError::PortFailure(
                "fetched blob failed size or digest verification".to_string(),
            ));
        }
        let stored = self
            .blobs
            .put(&bytes, descriptor.media_type.as_deref())
            .await?;
        if stored.digest != descriptor.digest || stored.size_bytes != descriptor.size_bytes {
            return Err(StasisError::PortFailure(
                "local blob identity changed after fetch".to_string(),
            ));
        }
        Ok(stored)
    }

    async fn transfer_input_graph(
        &self,
        target_runtime_id: &str,
        payload_descriptor: &BlobDescriptor,
    ) -> stasis::prelude::Result<()> {
        self.put_blob(target_runtime_id, payload_descriptor).await?;
        let payload_bytes = self.blobs.get(payload_descriptor).await?;
        let payload: WorkEnvironmentJobPayload = serde_json::from_slice(&payload_bytes)
            .map_err(|error| StasisError::PortFailure(error.to_string()))?;
        let Some(checkpoint) = payload.spec.checkpoint_ref.as_ref() else {
            return Ok(());
        };
        checkpoint
            .validate()
            .map_err(|error| StasisError::PortFailure(error.to_string()))?;
        self.put_blob(target_runtime_id, &checkpoint.manifest)
            .await?;
        let manifest_bytes = self.blobs.get(&checkpoint.manifest).await?;
        let manifest: WorkEnvironmentCheckpointManifest =
            serde_json::from_slice(&manifest_bytes)
                .map_err(|error| StasisError::PortFailure(error.to_string()))?;
        manifest
            .validate()
            .map_err(|error| StasisError::PortFailure(error.to_string()))?;
        self.put_blob(target_runtime_id, &manifest.source_bundle)
            .await?;
        for artifact in &manifest.artifacts {
            self.put_blob(target_runtime_id, &artifact.blob).await?;
        }
        Ok(())
    }

    async fn put_blob(
        &self,
        target_runtime_id: &str,
        descriptor: &BlobDescriptor,
    ) -> stasis::prelude::Result<()> {
        let bytes = self.blobs.get(descriptor).await?;
        if bytes.len() > MAX_FEDERATED_BLOB_BYTES || !descriptor.verify(&bytes) {
            return Err(StasisError::PortFailure(
                "outbound blob failed size or digest verification".to_string(),
            ));
        }
        let peer = self.peer(target_runtime_id)?;
        let base = peer_base(&peer)?;
        let envelope =
            self.sign_mesh_payload(target_runtime_id, MeshCapability::BundlePush, descriptor)?;
        let response = self
            .client
            .put(format!(
                "{}/v1/mesh/federation/blobs/{}",
                base, descriptor.digest.hex
            ))
            .header(
                MESH_ENVELOPE_HEADER,
                encode_envelope_header(&envelope)
                    .map_err(|error| StasisError::PortFailure(error.to_string()))?,
            )
            .header(
                BLOB_DESCRIPTOR_HEADER,
                encode_blob_descriptor_header(descriptor).map_err(StasisError::PortFailure)?,
            )
            .body(bytes)
            .send()
            .await
            .map_err(|error| StasisError::PortFailure(error.to_string()))?;
        if !response.status().is_success() {
            return Err(http_failure("blob upload", response).await);
        }
        Ok(())
    }

    async fn post_signed<T, R>(
        &self,
        target_runtime_id: &str,
        capability: MeshCapability,
        path: &str,
        payload: T,
    ) -> stasis::prelude::Result<R>
    where
        T: Serialize,
        R: serde::de::DeserializeOwned + Serialize,
    {
        let peer = self.peer(target_runtime_id)?;
        let base = peer_base(&peer)?;
        let envelope = self.sign_mesh_payload(target_runtime_id, capability, &payload)?;
        let response = self
            .client
            .post(format!("{base}{path}"))
            .header(
                MESH_ENVELOPE_HEADER,
                encode_envelope_header(&envelope)
                    .map_err(|error| StasisError::PortFailure(error.to_string()))?,
            )
            .json(&MeshEnvelopedRequest { envelope, payload })
            .send()
            .await
            .map_err(|error| StasisError::PortFailure(error.to_string()))?;
        if !response.status().is_success() {
            return Err(http_failure("federated post", response).await);
        }
        let wrapped: MeshEnvelopedRequest<R> = response
            .json()
            .await
            .map_err(|error| StasisError::PortFailure(error.to_string()))?;
        verify_enveloped_payload(
            &wrapped,
            &peer.public_key_b64,
            target_runtime_id,
            self.pairing.device_id(),
            MeshCapability::TaskResult,
            true,
        )
        .map_err(|error| StasisError::PortFailure(error.to_string()))?;
        Ok(wrapped.payload)
    }

    fn sign_mesh_payload<T: Serialize>(
        &self,
        target_runtime_id: &str,
        capability: MeshCapability,
        payload: &T,
    ) -> stasis::prelude::Result<MeshEnvelope> {
        let payload_hash = payload_hash_hex(payload)
            .map_err(|error| StasisError::PortFailure(error.to_string()))?;
        let seq = registry::allocate_outbound_seq(target_runtime_id)
            .map_err(|error| StasisError::PortFailure(error.to_string()))?;
        Ok(sign_envelope(
            self.pairing.identity().signing_key(),
            self.pairing.device_id(),
            target_runtime_id,
            seq,
            capability,
            &payload_hash,
            chrono::Duration::seconds(DEFAULT_ENVELOPE_TTL_SECS),
        ))
    }

    fn peer(
        &self,
        target_runtime_id: &str,
    ) -> stasis::prelude::Result<crate::mesh::MeshPeerRecord> {
        registry::get_peer(target_runtime_id)
            .map_err(|error| StasisError::PortFailure(error.to_string()))?
            .ok_or_else(|| {
                StasisError::PortFailure(format!("mesh peer not found: {target_runtime_id}"))
            })
    }
}

fn peer_base(peer: &crate::mesh::MeshPeerRecord) -> stasis::prelude::Result<String> {
    peer.endpoints
        .lan_base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.trim_end_matches('/').to_string())
        .ok_or_else(|| {
            StasisError::PortFailure(format!("mesh peer has no LAN endpoint: {}", peer.device_id))
        })
}

async fn http_failure(operation: &str, response: reqwest::Response) -> StasisError {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    StasisError::PortFailure(format!("{operation} HTTP {status}: {body}"))
}

impl MeshSignedFederatedTerminalDelivery {
    pub fn new(pairing: Arc<PairingService>) -> Self {
        Self {
            pairing,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl SignedFederatedTerminalDelivery for MeshSignedFederatedTerminalDelivery {
    async fn sign_and_deliver(
        &self,
        mut result: FederatedTerminalResult,
    ) -> stasis::prelude::Result<()> {
        if result.terminal_delivery.protocol != "medousa-mesh-v1"
            || result.terminal_delivery.address.trim() != result.origin_authority.runtime_id.trim()
        {
            return Err(StasisError::PortFailure(
                "unsupported or mismatched terminal delivery endpoint".to_string(),
            ));
        }
        sign_terminal_result(
            &mut result,
            self.pairing.device_id(),
            self.pairing.identity().signing_key(),
        )
        .map_err(StasisError::PortFailure)?;
        let target = result.origin_authority.runtime_id.clone();
        let peer = registry::get_peer(&target)
            .map_err(|error| StasisError::PortFailure(error.to_string()))?
            .ok_or_else(|| StasisError::PortFailure(format!("mesh peer not found: {target}")))?;
        let base = peer
            .endpoints
            .lan_base_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                StasisError::PortFailure(format!("mesh peer has no LAN endpoint: {target}"))
            })?;
        let payload_hash = payload_hash_hex(&result)
            .map_err(|error| StasisError::PortFailure(error.to_string()))?;
        let seq = registry::allocate_outbound_seq(&target)
            .map_err(|error| StasisError::PortFailure(error.to_string()))?;
        let envelope = sign_envelope(
            self.pairing.identity().signing_key(),
            self.pairing.device_id(),
            &target,
            seq,
            MeshCapability::TaskResult,
            &payload_hash,
            chrono::Duration::seconds(DEFAULT_ENVELOPE_TTL_SECS),
        );
        let header = encode_envelope_header(&envelope)
            .map_err(|error| StasisError::PortFailure(error.to_string()))?;
        let response = self
            .client
            .post(format!(
                "{}/v1/mesh/federation/terminal-results",
                base.trim_end_matches('/')
            ))
            .header(MESH_ENVELOPE_HEADER, header)
            .json(&MeshEnvelopedRequest {
                envelope,
                payload: result,
            })
            .send()
            .await
            .map_err(|error| StasisError::PortFailure(error.to_string()))?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(StasisError::PortFailure(format!(
                "terminal delivery HTTP {status}: {body}"
            )));
        }
        Ok(())
    }
}

pub fn encode_blob_descriptor_header(descriptor: &BlobDescriptor) -> Result<String, String> {
    let bytes = serde_json::to_vec(descriptor).map_err(|error| error.to_string())?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;
    use stasis::domain::runtime::blob_descriptor::BlobDescriptor;
    use stasis::domain::runtime::federation::{
        FEDERATED_TERMINAL_RESULT_SCHEMA_VERSION_V1, FederatedTerminalResult,
    };
    use stasis::domain::runtime::placement::PlacementConstraints;
    use stasis::domain::runtime::remote_job_envelope::{
        EnvelopeSignature, OriginAuthority, REMOTE_JOB_ENVELOPE_SCHEMA_VERSION_V1,
        RemoteJobEnvelope, TerminalDeliveryEndpoint,
    };

    use crate::pairing::crypto::{device_id_from_public_key, verifying_key_to_b64};

    use super::*;

    fn identity() -> (String, SigningKey, String) {
        let signing = SigningKey::generate(&mut OsRng);
        let verifying = signing.verifying_key();
        (
            device_id_from_public_key(verifying.as_bytes()),
            signing,
            verifying_key_to_b64(&verifying),
        )
    }

    fn unsigned_signature() -> EnvelopeSignature {
        EnvelopeSignature {
            algorithm: String::new(),
            key_id: String::new(),
            signature_hex: String::new(),
        }
    }

    #[test]
    fn pairing_identity_signs_stasis_job_and_terminal_result() {
        let (device_id, signing, public_key) = identity();
        let origin = OriginAuthority {
            runtime_id: device_id.clone(),
            authority_id: "authority".to_string(),
            realm: None,
        };
        let endpoint = TerminalDeliveryEndpoint {
            endpoint_id: "terminal".to_string(),
            protocol: "medousa-mesh-v1".to_string(),
            address: device_id.clone(),
        };
        let mut envelope = RemoteJobEnvelope {
            schema_version: REMOTE_JOB_ENVELOPE_SCHEMA_VERSION_V1,
            envelope_id: "envelope".to_string(),
            job_type: crate::work_environment_job::WORK_ENVIRONMENT_JOB_TYPE.to_string(),
            payload: BlobDescriptor::from_bytes(b"payload"),
            idempotency_key: "idempotency".to_string(),
            correlation_id: "correlation".to_string(),
            causation_id: "causation".to_string(),
            deadline: Utc::now() + Duration::minutes(5),
            origin_authority: origin.clone(),
            terminal_delivery: endpoint.clone(),
            placement: PlacementConstraints::unrestricted(),
            signature: unsigned_signature(),
        };
        sign_remote_job(&mut envelope, &device_id, &signing).unwrap();
        verify_remote_job(&envelope, &device_id, &public_key).unwrap();
        let mut tampered = envelope.clone();
        tampered.idempotency_key.push_str("-tampered");
        assert!(verify_remote_job(&tampered, &device_id, &public_key).is_err());

        let mut result = FederatedTerminalResult {
            schema_version: FEDERATED_TERMINAL_RESULT_SCHEMA_VERSION_V1,
            result_id: "result".to_string(),
            envelope_id: envelope.envelope_id,
            job_id: "remote-job".to_string(),
            job_type: envelope.job_type,
            succeeded: true,
            output: Some(BlobDescriptor::from_bytes(b"result")),
            output_provenance: None,
            error_message: None,
            origin_authority: origin,
            terminal_delivery: endpoint,
            correlation_id: "correlation".to_string(),
            causation_id: "causation".to_string(),
            occurred_at: Utc::now(),
            signature: unsigned_signature(),
        };
        sign_terminal_result(&mut result, &device_id, &signing).unwrap();
        verify_terminal_result(&result, &device_id, &public_key).unwrap();
        result.succeeded = false;
        assert!(verify_terminal_result(&result, &device_id, &public_key).is_err());
    }
}
