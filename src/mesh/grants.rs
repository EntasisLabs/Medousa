//! Capability grants attached to paired mesh/peer devices.

use crate::pairing::{PairedDeviceRecord, PairingRole};

pub const CAP_MESH_MESSAGE: &str = "mesh.message";
pub const CAP_MESH_BUNDLE_PUSH: &str = "mesh.bundle.push";
pub const CAP_TASK_REQUEST: &str = "task.request";
/// Signed terminal response correlated to an admitted task request.
pub const CAP_TASK_RESULT: &str = "task.result";
/// Client↔client introduce via this workshop (endpoint-hint exchange). Not implied by portal.
pub const CAP_CLIENT_RENDEZVOUS: &str = "client.rendezvous";
/// Reserved — scoped signaling/byte relay. Not issued in M4+ v1.
pub const CAP_CLIENT_RELAY: &str = "client.relay";

/// Default grants issued at peer/portal verify for mesh surfaces.
pub fn default_mesh_grants_for_role(role: PairingRole) -> Vec<String> {
    match role {
        PairingRole::Peer | PairingRole::Portal => vec![
            CAP_MESH_MESSAGE.to_string(),
            CAP_MESH_BUNDLE_PUSH.to_string(),
        ],
    }
}

/// Effective grants for a pairing record (legacy empty → role defaults).
pub fn effective_mesh_grants(record: &PairedDeviceRecord) -> Vec<String> {
    if record.mesh_grants.is_empty() {
        return default_mesh_grants_for_role(record.role);
    }
    record
        .mesh_grants
        .iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

pub fn record_has_capability(record: &PairedDeviceRecord, capability: &str) -> bool {
    let wanted = capability.trim();
    if wanted.is_empty() {
        return false;
    }
    let grants = effective_mesh_grants(record);
    grants
        .iter()
        .any(|grant| grant.eq_ignore_ascii_case(wanted))
        || (wanted.eq_ignore_ascii_case(CAP_TASK_RESULT)
            && grants
                .iter()
                .any(|grant| grant.eq_ignore_ascii_case(CAP_TASK_REQUEST)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn sample(role: PairingRole, grants: Vec<&str>) -> PairedDeviceRecord {
        PairedDeviceRecord {
            pairing_id: "p".into(),
            phone_id: "phone".into(),
            phone_name: "Phone".into(),
            phone_public_key: "key".into(),
            paired_at: Utc::now(),
            last_seen: Utc::now(),
            session_token_hash: "h".into(),
            session_token_expiry: Utc::now(),
            trust_expires_at: None,
            idle_timeout_seconds: None,
            credential_generation: 1,
            role,
            profile_id: None,
            mesh_grants: grants.into_iter().map(str::to_string).collect(),
            apns_device_token: None,
            push_platform: None,
            push_updated_at: None,
            live_activity_push_token: None,
            live_activity_push_updated_at: None,
        }
    }

    #[test]
    fn empty_grants_fall_back_to_role_defaults() {
        let peer = sample(PairingRole::Peer, vec![]);
        assert!(record_has_capability(&peer, CAP_MESH_MESSAGE));
        assert!(record_has_capability(&peer, CAP_MESH_BUNDLE_PUSH));
        assert!(!record_has_capability(&peer, CAP_TASK_REQUEST));
        assert!(!record_has_capability(&peer, CAP_CLIENT_RENDEZVOUS));
    }

    #[test]
    fn explicit_grants_narrow_surface() {
        let peer = sample(PairingRole::Peer, vec![CAP_MESH_MESSAGE]);
        assert!(record_has_capability(&peer, CAP_MESH_MESSAGE));
        assert!(!record_has_capability(&peer, CAP_MESH_BUNDLE_PUSH));
    }

    #[test]
    fn task_request_implies_terminal_result_but_no_other_capability() {
        let peer = sample(PairingRole::Peer, vec![CAP_TASK_REQUEST]);
        assert!(record_has_capability(&peer, CAP_TASK_REQUEST));
        assert!(record_has_capability(&peer, CAP_TASK_RESULT));
        assert!(!record_has_capability(&peer, CAP_MESH_MESSAGE));
    }
}
