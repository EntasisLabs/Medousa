//! Authenticated request identity produced once at the daemon boundary.

use std::sync::Arc;

#[cfg(feature = "full-daemon")]
use axum::http::HeaderMap;
#[cfg(feature = "full-daemon")]
use std::net::IpAddr;

#[cfg(feature = "full-daemon")]
use crate::pairing::store::{PairedDeviceRecord, PairingRole};
#[cfg(feature = "full-daemon")]
use crate::shared_mode::root_profile_id;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrincipalKind {
    Anonymous,
    Continuation,
    LocalApp,
    McpGateway,
    Portal,
    Peer,
    Root,
    Worker,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportClass {
    Loopback,
    Direct,
    Iroh,
}

impl TransportClass {
    #[cfg(feature = "full-daemon")]
    pub fn from_request(ip: IpAddr, headers: &HeaderMap) -> Self {
        if crate::remote_trust::transport_is_iroh(headers) {
            Self::Iroh
        } else if ip.is_loopback() {
            Self::Loopback
        } else {
            Self::Direct
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialId(Arc<str>);

impl CredentialId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Capability {
    WorkshopRead,
    WorkshopInteract,
    ContentRead,
    ContentWrite,
    WorkspaceWrite,
    PeerExchange,
    ProfileSelf,
    AdminIdentity,
    AdminRuntime,
    AdminExecute,
    McpPolicyEvaluate,
}

impl Capability {
    const fn bit(self) -> u16 {
        1 << self as u16
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WorkshopRead => "workshop.read",
            Self::WorkshopInteract => "workshop.interact",
            Self::ContentRead => "content.read",
            Self::ContentWrite => "content.write",
            Self::WorkspaceWrite => "workspace.write",
            Self::PeerExchange => "peer.exchange",
            Self::ProfileSelf => "profile.self",
            Self::AdminIdentity => "admin.identity",
            Self::AdminRuntime => "admin.runtime",
            Self::AdminExecute => "admin.execute",
            Self::McpPolicyEvaluate => "mcp.policy.evaluate",
        }
    }
}

/// Compact capability representation copied with the request principal.
/// Capability names never allocate on the request path.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CapabilitySet(u16);

impl CapabilitySet {
    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn contains(self, capability: Capability) -> bool {
        self.0 & capability.bit() != 0
    }

    const fn with(self, capability: Capability) -> Self {
        Self(self.0 | capability.bit())
    }

    const fn member() -> Self {
        Self::empty()
            .with(Capability::WorkshopRead)
            .with(Capability::WorkshopInteract)
            .with(Capability::ContentRead)
            .with(Capability::ContentWrite)
            .with(Capability::WorkspaceWrite)
            .with(Capability::PeerExchange)
            .with(Capability::ProfileSelf)
    }

    const fn operator() -> Self {
        Self::member()
            .with(Capability::PeerExchange)
            .with(Capability::AdminIdentity)
            .with(Capability::AdminRuntime)
            .with(Capability::AdminExecute)
    }

    #[cfg(feature = "full-daemon")]
    const fn peer() -> Self {
        Self::empty().with(Capability::PeerExchange)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestPrincipal {
    kind: PrincipalKind,
    credential_id: Option<CredentialId>,
    profile_id: Option<String>,
    capabilities: CapabilitySet,
    transport: TransportClass,
    revocation_generation: u64,
}

impl RequestPrincipal {
    pub fn anonymous(transport: TransportClass) -> Self {
        Self {
            kind: PrincipalKind::Anonymous,
            credential_id: None,
            profile_id: None,
            capabilities: CapabilitySet::empty(),
            transport,
            revocation_generation: 0,
        }
    }

    pub fn local_app(credential_id: Arc<str>, transport: TransportClass) -> Self {
        Self::local_app_with_generation(credential_id, transport, 1)
    }

    /// Reauthorized internal principal for one durable continuation replay.
    /// It receives member capabilities only; replay can never recover operator
    /// authority from process-global state.
    pub fn continuation(profile_id: impl Into<String>) -> Self {
        Self {
            kind: PrincipalKind::Continuation,
            credential_id: None,
            profile_id: Some(profile_id.into()),
            capabilities: CapabilitySet::member(),
            transport: TransportClass::Loopback,
            revocation_generation: 0,
        }
    }

    /// Reauthorized durable worker principal. Workers inherit member authority
    /// only and cannot recover host operator capabilities from ambient state.
    pub fn worker(profile_id: impl Into<String>) -> Self {
        Self {
            kind: PrincipalKind::Worker,
            credential_id: None,
            profile_id: Some(profile_id.into()),
            capabilities: CapabilitySet::member(),
            transport: TransportClass::Loopback,
            revocation_generation: 0,
        }
    }

    pub fn local_app_with_generation(
        credential_id: Arc<str>,
        transport: TransportClass,
        revocation_generation: u64,
    ) -> Self {
        Self {
            kind: PrincipalKind::LocalApp,
            credential_id: Some(CredentialId(credential_id)),
            profile_id: None,
            capabilities: CapabilitySet::operator(),
            transport,
            revocation_generation,
        }
    }

    pub fn mcp_policy_service(transport: TransportClass) -> Self {
        Self {
            kind: PrincipalKind::McpGateway,
            credential_id: Some(CredentialId(Arc::from("mcp-policy"))),
            profile_id: None,
            capabilities: CapabilitySet::empty().with(Capability::McpPolicyEvaluate),
            transport,
            revocation_generation: 0,
        }
    }

    #[cfg(feature = "full-daemon")]
    pub fn from_pairing_record(
        record: PairedDeviceRecord,
        transport: TransportClass,
        shared_mode: bool,
    ) -> Self {
        let root = shared_mode && is_root_portal(&record);
        let (kind, capabilities) = match record.role {
            PairingRole::Peer => (PrincipalKind::Peer, CapabilitySet::peer()),
            PairingRole::Portal if root => (PrincipalKind::Root, CapabilitySet::operator()),
            PairingRole::Portal if shared_mode => (PrincipalKind::Portal, CapabilitySet::member()),
            PairingRole::Portal => (PrincipalKind::Portal, CapabilitySet::operator()),
        };
        Self {
            kind,
            credential_id: Some(CredentialId(Arc::from(record.pairing_id))),
            profile_id: record
                .profile_id
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            capabilities,
            transport,
            revocation_generation: record.credential_generation,
        }
    }

    /// A peer authenticated by a signed mesh request header receives only the
    /// peer-exchange surface. Possessing the pairing key must never recover
    /// portal/member/operator authority without the bearer credential.
    #[cfg(feature = "full-daemon")]
    pub fn from_signed_mesh_record(record: PairedDeviceRecord, transport: TransportClass) -> Self {
        Self {
            kind: PrincipalKind::Peer,
            credential_id: Some(CredentialId(Arc::from(record.pairing_id))),
            profile_id: record
                .profile_id
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            capabilities: CapabilitySet::peer(),
            transport,
            revocation_generation: record.credential_generation,
        }
    }

    pub fn kind(&self) -> PrincipalKind {
        self.kind
    }

    pub fn credential_id(&self) -> Option<&CredentialId> {
        self.credential_id.as_ref()
    }

    pub fn profile_id(&self) -> Option<&str> {
        self.profile_id.as_deref()
    }

    pub fn capabilities(&self) -> CapabilitySet {
        self.capabilities
    }

    pub fn transport(&self) -> TransportClass {
        self.transport
    }

    pub fn revocation_generation(&self) -> u64 {
        self.revocation_generation
    }
}

#[cfg(feature = "full-daemon")]
fn is_root_portal(record: &PairedDeviceRecord) -> bool {
    record.role.allows_full_portal()
        && record
            .profile_id
            .as_deref()
            .map(str::trim)
            .is_some_and(|id| id == root_profile_id())
}

#[cfg(all(test, feature = "full-daemon"))]
mod tests {
    use chrono::Utc;

    use super::*;

    fn record(role: PairingRole, profile_id: Option<&str>) -> PairedDeviceRecord {
        PairedDeviceRecord {
            pairing_id: "pairing-1".into(),
            phone_id: "phone-1".into(),
            phone_name: "Phone".into(),
            phone_public_key: "key".into(),
            paired_at: Utc::now(),
            last_seen: Utc::now(),
            session_token_hash: "hash".into(),
            session_token_expiry: Utc::now(),
            trust_expires_at: None,
            idle_timeout_seconds: None,
            credential_generation: 1,
            role,
            profile_id: profile_id.map(str::to_string),
            mesh_grants: Vec::new(),
            apns_device_token: None,
            push_platform: None,
            push_updated_at: None,
            live_activity_push_token: None,
            live_activity_push_updated_at: None,
        }
    }

    #[test]
    fn principal_contains_no_bearer_and_uses_opaque_credential_id() {
        let principal = RequestPrincipal::from_pairing_record(
            record(PairingRole::Portal, Some("user:alice")),
            TransportClass::Direct,
            true,
        );
        assert_eq!(principal.kind(), PrincipalKind::Portal);
        assert_eq!(
            principal.credential_id().map(CredentialId::as_str),
            Some("pairing-1")
        );
        assert_eq!(principal.profile_id(), Some("user:alice"));
        assert_eq!(principal.transport(), TransportClass::Direct);
        assert!(
            principal
                .capabilities()
                .contains(Capability::WorkshopInteract)
        );
        assert!(!principal.capabilities().contains(Capability::AdminRuntime));
    }

    #[test]
    fn signed_mesh_portal_is_confined_to_peer_exchange() {
        let principal = RequestPrincipal::from_signed_mesh_record(
            record(PairingRole::Portal, Some("user:alice")),
            TransportClass::Direct,
        );
        assert_eq!(principal.kind(), PrincipalKind::Peer);
        assert!(principal.capabilities().contains(Capability::PeerExchange));
        assert!(!principal.capabilities().contains(Capability::WorkshopRead));
        assert!(!principal.capabilities().contains(Capability::AdminRuntime));
    }

    #[test]
    fn shared_mode_root_seat_receives_operator_capabilities() {
        let root = RequestPrincipal::from_pairing_record(
            record(PairingRole::Portal, Some("user:root")),
            TransportClass::Direct,
            true,
        );
        assert_eq!(root.kind(), PrincipalKind::Root);
        assert!(root.capabilities().contains(Capability::AdminRuntime));

        let personal = RequestPrincipal::from_pairing_record(
            record(PairingRole::Portal, Some("user:root")),
            TransportClass::Direct,
            false,
        );
        assert_eq!(personal.kind(), PrincipalKind::Portal);
    }

    #[test]
    fn peer_gets_only_peer_exchange() {
        let principal = RequestPrincipal::from_pairing_record(
            record(PairingRole::Peer, None),
            TransportClass::Iroh,
            false,
        );
        assert_eq!(principal.kind(), PrincipalKind::Peer);
        assert!(principal.capabilities().contains(Capability::PeerExchange));
        assert!(!principal.capabilities().contains(Capability::ContentRead));
    }

    #[test]
    fn mcp_policy_service_has_only_callback_authority() {
        let principal = RequestPrincipal::mcp_policy_service(TransportClass::Loopback);
        assert_eq!(principal.kind(), PrincipalKind::McpGateway);
        assert_eq!(
            principal.credential_id().map(CredentialId::as_str),
            Some("mcp-policy")
        );
        assert!(
            principal
                .capabilities()
                .contains(Capability::McpPolicyEvaluate)
        );
        assert!(!principal.capabilities().contains(Capability::AdminRuntime));
    }

    #[test]
    fn continuation_principal_is_member_scoped() {
        let principal = RequestPrincipal::continuation("user:alice");
        assert_eq!(principal.kind(), PrincipalKind::Continuation);
        assert_eq!(principal.profile_id(), Some("user:alice"));
        assert!(
            principal
                .capabilities()
                .contains(Capability::WorkshopInteract)
        );
        assert!(!principal.capabilities().contains(Capability::AdminExecute));
    }

    #[test]
    fn worker_principal_is_member_scoped() {
        let principal = RequestPrincipal::worker("user:alice");
        assert_eq!(principal.kind(), PrincipalKind::Worker);
        assert_eq!(principal.profile_id(), Some("user:alice"));
        assert!(principal.capabilities().contains(Capability::ContentWrite));
        assert!(!principal.capabilities().contains(Capability::AdminRuntime));
    }
}
