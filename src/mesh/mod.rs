//! Peer-mesh signed envelopes and capability grants (0.6.0 M2).
//!
//! Portal ACL and pairing bearers remain the outer door. Mesh envelopes bind
//! each remote share/message delivery to sender identity, capability, expiry,
//! and a payload hash — Iroh tickets stay routing-only.

pub mod envelope;
pub mod grants;

pub use envelope::{
    DEFAULT_ENVELOPE_TTL_SECS, MeshCapability, MeshEnvelope, MeshEnvelopeError,
    MeshEnvelopedRequest, MeshInboundBody, payload_hash_hex, require_remote_envelope,
    require_remote_envelope_json, sign_envelope, verify_enveloped_payload,
};
pub use grants::{
    CAP_MESH_BUNDLE_PUSH, CAP_MESH_MESSAGE, CAP_TASK_REQUEST, default_mesh_grants_for_role,
    effective_mesh_grants, record_has_capability,
};
