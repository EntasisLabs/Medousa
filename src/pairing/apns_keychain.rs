//! Platform secret store for the APNs Auth Key (.p8 PEM).

use anyhow::Result;
use medousa_types::secrets::IntegrationSecretSlot;

pub fn load_apns_key_pem() -> Option<String> {
    crate::integration_connection::load_kind_secret("apns", IntegrationSecretSlot::AuthKey)
}

pub fn store_apns_key_pem(pem: &str) -> Result<()> {
    let trimmed = pem.trim();
    if trimmed.is_empty() {
        anyhow::bail!("APNs key PEM is empty");
    }
    crate::integration_connection::save_kind_secret(
        "apns",
        IntegrationSecretSlot::AuthKey,
        Some(trimmed),
    );
    Ok(())
}

pub fn delete_apns_key_pem() -> Result<()> {
    crate::integration_connection::save_kind_secret("apns", IntegrationSecretSlot::AuthKey, None);
    Ok(())
}

pub fn keychain_available() -> bool {
    true
}

/// Legacy probe kept for callers that only check whether a keyring backend exists.
pub fn apns_key_keyring_entry() -> Result<keyring::Entry, keyring::Error> {
    keyring::Entry::new("medousa.apns", "auth_key")
}
