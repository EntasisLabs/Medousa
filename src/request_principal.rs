//! Authenticated request identity produced once at the daemon boundary.

use std::net::IpAddr;

use axum::http::HeaderMap;

use crate::pairing::store::{PairedDeviceRecord, PairingRole};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrincipalKind {
    Anonymous,
    LegacyLocal,
    Portal,
    Peer,
    Root,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportClass {
    Loopback,
    Direct,
    Iroh,
}

impl TransportClass {
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
pub struct CredentialId(String);

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
}

impl Capability {
    const fn bit(self) -> u16 {
        1 << self as u16
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

    pub fn legacy_local() -> Self {
        Self {
            kind: PrincipalKind::LegacyLocal,
            credential_id: None,
            profile_id: None,
            capabilities: CapabilitySet::operator(),
            transport: TransportClass::Loopback,
            revocation_generation: 0,
        }
    }

    pub fn from_pairing_record(
        record: PairedDeviceRecord,
        transport: TransportClass,
        shared_mode: bool,
    ) -> Self {
        let root = shared_mode && crate::portal_acl::is_root_portal(&record);
        let (kind, capabilities) = match record.role {
            PairingRole::Peer => (PrincipalKind::Peer, CapabilitySet::peer()),
            PairingRole::Portal if root => (PrincipalKind::Root, CapabilitySet::operator()),
            PairingRole::Portal if shared_mode => (PrincipalKind::Portal, CapabilitySet::member()),
            PairingRole::Portal => (PrincipalKind::Portal, CapabilitySet::operator()),
        };
        Self {
            kind,
            credential_id: Some(CredentialId(record.pairing_id)),
            profile_id: record
                .profile_id
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            capabilities,
            transport,
            // The current pairing store has boolean revocation. H01.5 replaces
            // this sentinel with the store's monotonic revocation generation.
            revocation_generation: 0,
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

#[cfg(test)]
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
    fn legacy_local_is_explicit_and_temporary() {
        let principal = RequestPrincipal::legacy_local();
        assert_eq!(principal.kind(), PrincipalKind::LegacyLocal);
        assert!(principal.credential_id().is_none());
        assert_eq!(principal.transport(), TransportClass::Loopback);
        assert!(principal.capabilities().contains(Capability::AdminExecute));
    }
}
