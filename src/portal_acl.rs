//! Shared-mode portal settings ACL.
//!
//! Remote callers need a valid scoped bearer in both Personal and Shared mode.
//! In Shared mode, org-admin paths require the bound `user:root` seat. Genuine
//! loopback (not Iroh-proxied) retains the temporary H01 compatibility path.

use axum::http::Method;

use crate::pairing::store::PairedDeviceRecord;
use crate::request_principal::{Capability, RequestPrincipal};
use crate::shared_mode::root_profile_id;

/// Coarse path class for Shared-mode authorization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortalPathClass {
    /// Pair bootstrap + health — no bearer required.
    Public,
    /// Bound Portal seat work (turns, sessions, vault content, …).
    Member,
    /// Org settings / host administration — root or loopback only.
    Admin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortalAclDecision {
    Allow,
    Deny,
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
    if is_admin_path(method, path) {
        return PortalPathClass::Admin;
    }
    PortalPathClass::Member
}

fn is_public_path(path: &str) -> bool {
    matches!(path, "/health" | "/pair/init" | "/pair/verify")
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

    // Vault host configuration + Versions/Git (content/search stay member).
    if path == "/v1/vault/roots" || path == "/v1/vault/active" || path.starts_with("/v1/vault/git/")
    {
        return true;
    }

    // Packages / MCP / model host controls
    if path.starts_with("/v1/mcp/")
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

/// Authorize a principal on the protected router during the H01.2 migration.
/// Bootstrap routes never call this function; a public-looking path mounted on
/// the protected router therefore remains inaccessible.
pub fn authorize_request(
    principal: &RequestPrincipal,
    method: &Method,
    path: &str,
) -> PortalAclDecision {
    let required = match classify_path(method, path) {
        PortalPathClass::Public => return PortalAclDecision::Deny,
        // H01.2 replaces these coarse transitional classes with per-route
        // capability metadata at router assembly.
        PortalPathClass::Member => Capability::WorkshopRead,
        PortalPathClass::Admin => Capability::AdminRuntime,
    };
    if principal.capabilities().contains(required) {
        PortalAclDecision::Allow
    } else {
        PortalAclDecision::Deny
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pairing::store::PairingRole;
    use crate::request_principal::TransportClass;
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

    fn principal(
        role: PairingRole,
        profile_id: Option<&str>,
        shared_mode: bool,
    ) -> RequestPrincipal {
        RequestPrincipal::from_pairing_record(
            record(role, profile_id),
            TransportClass::Direct,
            shared_mode,
        )
    }

    #[test]
    fn classifies_remaining_legacy_paths() {
        assert_eq!(classify_path(&Method::GET, "/qr"), PortalPathClass::Admin);
        assert_eq!(
            classify_path(&Method::POST, "/v1/mcp/policy/evaluate"),
            PortalPathClass::Admin
        );
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
    fn legacy_local_retains_operator_capabilities() {
        let local = RequestPrincipal::legacy_local();
        assert!(authorize_request(&local, &Method::POST, "/v1/mcp/policy/evaluate").is_allow());
    }

    #[test]
    fn issued_capabilities_bound_personal_mode_access() {
        let peer = principal(PairingRole::Peer, None, false);
        assert!(!authorize_request(&peer, &Method::POST, "/v1/turns").is_allow());
        assert!(!authorize_request(&peer, &Method::GET, "/v1/peer/messages").is_allow());
        let portal = principal(PairingRole::Portal, None, false);
        assert!(authorize_request(&portal, &Method::POST, "/v1/mcp/policy/evaluate").is_allow());
        let anonymous = RequestPrincipal::anonymous(TransportClass::Direct);
        assert!(!authorize_request(&anonymous, &Method::POST, "/v1/turns").is_allow());
        assert!(!authorize_request(&anonymous, &Method::GET, "/health").is_allow());
        assert!(!authorize_request(&anonymous, &Method::GET, "/pair/code").is_allow());
        assert!(!authorize_request(&portal, &Method::GET, "/health").is_allow());
    }

    #[test]
    fn shared_mode_issues_admin_capability_only_to_root() {
        let alice = principal(PairingRole::Portal, Some("user:alice"), true);
        let root = principal(PairingRole::Portal, Some("user:root"), true);
        assert!(!authorize_request(&alice, &Method::POST, "/v1/mcp/policy/evaluate").is_allow());
        assert!(authorize_request(&root, &Method::POST, "/v1/mcp/policy/evaluate").is_allow());
        assert!(authorize_request(&alice, &Method::POST, "/v1/turns").is_allow());
        assert!(authorize_request(&alice, &Method::GET, "/v1/turns").is_allow());
    }
}
