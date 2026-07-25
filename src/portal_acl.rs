//! Shared-mode portal settings ACL.
//!
//! Personal mode keeps today's behavior (Peer allowlist only). In Shared mode,
//! remote callers need a valid Portal bearer; org-admin paths require the
//! bound `user:root` seat. Genuine loopback (not Iroh-proxied) remains root.

use axum::http::Method;

use crate::pairing::path_allowed_for_peer;
use crate::pairing::store::{PairedDeviceRecord, PairingRole};
use crate::shared_mode::root_profile_id;

/// Coarse path class for Shared-mode authorization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortalPathClass {
    /// Pair bootstrap + health — no bearer required.
    Public,
    /// Peer-role surfaces (inbox/share/heartbeat).
    PeerSurface,
    /// Bound Portal seat work (turns, sessions, vault content, …).
    Member,
    /// Org settings / host administration — root or loopback only.
    Admin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortalAclDecision {
    Allow,
    Deny(&'static str),
}

impl PortalAclDecision {
    pub fn is_allow(self) -> bool {
        matches!(self, Self::Allow)
    }
}

/// True when the pairing bind is the Shared-mode admin seat.
pub fn is_root_portal(record: &PairedDeviceRecord) -> bool {
    record.role.allows_full_portal()
        && record
            .profile_id
            .as_deref()
            .map(str::trim)
            .is_some_and(|id| id == root_profile_id())
}

pub fn classify_path(method: &Method, path: &str) -> PortalPathClass {
    let path = path.split('?').next().unwrap_or(path);

    if is_public_path(path) {
        return PortalPathClass::Public;
    }
    if path_allowed_for_peer(path) {
        return PortalPathClass::PeerSurface;
    }
    if is_admin_path(method, path) {
        return PortalPathClass::Admin;
    }
    PortalPathClass::Member
}

fn is_public_path(path: &str) -> bool {
    matches!(
        path,
        "/health" | "/pair/init" | "/pair/verify" | "/pair/code"
    )
}

fn is_admin_path(method: &Method, path: &str) -> bool {
    // Pairing invite / administration
    if path == "/qr"
        || path == "/qr.png"
        || path == "/qr/image"
        || path == "/qr/rotate"
        || path.starts_with("/pair/") && method == Method::DELETE
    {
        return true;
    }

    // Shared mode toggle (status GET is member-readable)
    if path == "/v1/shared-mode" && method != Method::GET {
        return true;
    }

    // Profile registry mutations / global switch / import-export
    if path == "/v1/identity/profiles" && method != Method::GET {
        return true;
    }
    if path == "/v1/identity/profiles/active"
        || path == "/v1/identity/profiles/export"
        || path == "/v1/identity/profiles/import"
        || path == "/v1/identity/rollback"
        || path == "/v1/identity/update/commit"
    {
        return true;
    }

    // Vault host configuration + Versions/Git (content/search stay member).
    if path == "/v1/vault/roots"
        || path == "/v1/vault/active"
        || path.starts_with("/v1/vault/git/")
    {
        return true;
    }

    // Runtime / environment / packages / MCP / maintenance host controls
    if path.starts_with("/v1/runtime/")
        || path.starts_with("/v1/environment/")
        || path.starts_with("/v1/maintenance/")
        || path.starts_with("/v1/mcp/")
        || path.starts_with("/v1/packages/")
        || path.starts_with("/v1/model/")
        || path.starts_with("/v1/inference/")
    {
        return true;
    }

    // Agent permission approvals + recurring schedule admin mutations
    if path.starts_with("/v1/agents/permission-requests/")
        || (path == "/v1/recurring/prompt" && method == Method::POST)
        || (path.starts_with("/v1/recurring/")
            && (method == Method::PATCH || method == Method::DELETE || method == Method::POST))
    {
        return true;
    }

    // Workspace rebuild / retry host ops
    if path.starts_with("/v1/workspace/") && method != Method::GET {
        return true;
    }

    false
}

/// Authorize a request under Personal vs Shared rules.
///
/// `trusted_local` must use [`crate::remote_trust::is_trusted_local`] (Iroh ≠ local).
/// `shared_mode` is passed explicitly so callers/tests avoid process-global races.
pub fn authorize_request(
    trusted_local: bool,
    shared_mode: bool,
    record: Option<&PairedDeviceRecord>,
    method: &Method,
    path: &str,
) -> PortalAclDecision {
    if trusted_local {
        return PortalAclDecision::Allow;
    }

    let class = classify_path(method, path);

    // Personal installs: keep Peer allowlist only; Portal (or no bearer) stays open.
    if !shared_mode {
        return match record.map(|r| r.role) {
            Some(PairingRole::Peer) if !path_allowed_for_peer(path) => {
                PortalAclDecision::Deny("Peer credentials can only use inbox and share surfaces")
            }
            _ => PortalAclDecision::Allow,
        };
    }

    // Shared mode — remote traffic is seat-scoped.
    match class {
        PortalPathClass::Public => PortalAclDecision::Allow,
        PortalPathClass::PeerSurface => match record {
            Some(r) if r.role.allows_peer_surface() => PortalAclDecision::Allow,
            Some(_) => PortalAclDecision::Deny("credentials cannot use this surface"),
            None => PortalAclDecision::Deny("Bearer session token required"),
        },
        PortalPathClass::Member => match record {
            Some(r) if r.role.allows_full_portal() => PortalAclDecision::Allow,
            Some(r) if r.role == PairingRole::Peer => {
                PortalAclDecision::Deny("Peer credentials can only use inbox and share surfaces")
            }
            Some(_) => PortalAclDecision::Deny("Portal credentials required"),
            None => PortalAclDecision::Deny("Bearer session token required"),
        },
        PortalPathClass::Admin => match record {
            Some(r) if is_root_portal(r) => PortalAclDecision::Allow,
            Some(_) => PortalAclDecision::Deny("Root seat required for portal settings"),
            None => PortalAclDecision::Deny("Bearer session token required"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn record(role: PairingRole, profile_id: Option<&str>) -> PairedDeviceRecord {
        PairedDeviceRecord {
            pairing_id: "p1".into(),
            phone_id: "phone".into(),
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
    fn classifies_admin_settings_paths() {
        assert_eq!(
            classify_path(&Method::PUT, "/v1/shared-mode"),
            PortalPathClass::Admin
        );
        assert_eq!(
            classify_path(&Method::GET, "/v1/shared-mode"),
            PortalPathClass::Member
        );
        assert_eq!(
            classify_path(&Method::POST, "/v1/identity/profiles"),
            PortalPathClass::Admin
        );
        assert_eq!(
            classify_path(&Method::GET, "/v1/identity/profiles"),
            PortalPathClass::Member
        );
        assert_eq!(
            classify_path(&Method::PUT, "/v1/identity/profiles/active"),
            PortalPathClass::Admin
        );
        assert_eq!(classify_path(&Method::GET, "/qr"), PortalPathClass::Admin);
        assert_eq!(
            classify_path(&Method::POST, "/v1/turns"),
            PortalPathClass::Member
        );
        assert_eq!(
            classify_path(&Method::GET, "/health"),
            PortalPathClass::Public
        );
    }

    #[test]
    fn loopback_always_allowed() {
        let alice = record(PairingRole::Portal, Some("user:alice"));
        assert!(
            authorize_request(true, true, Some(&alice), &Method::PUT, "/v1/shared-mode").is_allow()
        );
    }

    #[test]
    fn personal_mode_blocks_peer_escalation_only() {
        let peer = record(PairingRole::Peer, None);
        assert!(
            !authorize_request(false, false, Some(&peer), &Method::POST, "/v1/turns").is_allow()
        );
        assert!(
            authorize_request(false, false, Some(&peer), &Method::GET, "/v1/peer/messages")
                .is_allow()
        );
        let portal = record(PairingRole::Portal, None);
        assert!(
            authorize_request(false, false, Some(&portal), &Method::PUT, "/v1/shared-mode")
                .is_allow()
        );
    }

    #[test]
    fn shared_mode_requires_root_for_settings() {
        let alice = record(PairingRole::Portal, Some("user:alice"));
        let root = record(PairingRole::Portal, Some("user:root"));
        assert!(
            !authorize_request(false, true, Some(&alice), &Method::PUT, "/v1/shared-mode")
                .is_allow()
        );
        assert!(
            authorize_request(false, true, Some(&root), &Method::PUT, "/v1/shared-mode").is_allow()
        );
        assert!(
            authorize_request(false, true, Some(&alice), &Method::POST, "/v1/turns").is_allow()
        );
        assert!(!authorize_request(false, true, None, &Method::POST, "/v1/turns").is_allow());
        assert!(
            authorize_request(false, true, Some(&alice), &Method::GET, "/v1/shared-mode").is_allow()
        );
    }
}
