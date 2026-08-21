//! Stable logical authority for durable workshop data.
//!
//! The identity is derived once at daemon bootstrap from the persisted
//! installation id. Request handlers only read the in-memory value and never
//! perform blocking identity I/O on an async path.

use std::sync::OnceLock;

use medousa_types::secrets::InstallationId;
use medousa_types::session::AuthorityId;

static WORKSHOP_AUTHORITY_ID: OnceLock<AuthorityId> = OnceLock::new();

pub fn initialize(installation_id: &InstallationId) -> Result<&'static AuthorityId, String> {
    let authority_id = AuthorityId::from_installation_id(installation_id);
    if let Some(existing) = WORKSHOP_AUTHORITY_ID.get() {
        if existing == &authority_id {
            return Ok(existing);
        }
        return Err("workshop authority was already initialized with another identity".into());
    }
    let _ = WORKSHOP_AUTHORITY_ID.set(authority_id);
    WORKSHOP_AUTHORITY_ID
        .get()
        .ok_or_else(|| "workshop authority initialization failed".to_string())
}

pub fn current() -> Result<&'static AuthorityId, String> {
    WORKSHOP_AUTHORITY_ID
        .get()
        .ok_or_else(|| "workshop authority is not initialized".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialization_is_idempotent_for_the_same_installation() {
        let installation = InstallationId::parse("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let first = initialize(&installation).unwrap();
        let second = initialize(&installation).unwrap();
        assert_eq!(first, second);
        assert_eq!(current().unwrap(), first);
    }
}
