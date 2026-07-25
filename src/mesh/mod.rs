//! Peer-mesh signed envelopes, grants, and delivery durability (0.6.0 M2–M3).
//!
//! Portal ACL and pairing bearers remain the outer door. Mesh envelopes bind
//! each remote share/message delivery to sender identity, capability, expiry,
//! and a payload hash — Iroh tickets stay routing-only.
//!
//! M3 adds a daemon registry, inbox (sender+seq dedupe), outbox, and signed
//! receipts around those deliveries.

pub mod delivery;
pub mod envelope;
pub mod grants;
pub mod handlers;
pub mod inbox;
pub mod outbox;
pub mod receipts;
pub mod registry;
mod store_io;

pub use envelope::{
    DEFAULT_ENVELOPE_TTL_SECS, MeshCapability, MeshEnvelope, MeshEnvelopeError,
    MeshEnvelopedRequest, MeshInboundBody, payload_hash_hex, require_remote_envelope,
    require_remote_envelope_json, sign_envelope, verify_enveloped_payload,
};
pub use grants::{
    CAP_MESH_BUNDLE_PUSH, CAP_MESH_MESSAGE, CAP_TASK_REQUEST, default_mesh_grants_for_role,
    effective_mesh_grants, record_has_capability,
};
pub use handlers::{MeshApiState, mesh_router};
pub use receipts::{CAP_MESH_RECEIPT, MeshReceipt, MeshReceiptStatus};
pub use registry::{MeshPeerEndpoints, MeshPeerRecord};
