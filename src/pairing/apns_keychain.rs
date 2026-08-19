//! Platform secret store for the APNs Auth Key (.p8 PEM).

use anyhow::{Context, Result};
use medousa_types::{IntegrationSecretSlot, KIND_APNS};

pub fn load_apns_key_pem() -> Option<String> {
    let _ = crate::secret_migration::migrate_legacy_secrets();
    crate::integration_store::load_kind_secret(KIND_APNS, IntegrationSecretSlot::AuthKey)
}

pub fn store_apns_key_pem(pem: &str) -> Result<()> {
    let trimmed = pem.trim();
    if trimmed.is_empty() {
        anyhow::bail!("APNs key PEM is empty");
    }
    crate::integration_store::save_kind_secret(KIND_APNS, IntegrationSecretSlot::AuthKey, Some(trimmed))
        .map_err(anyhow::Error::msg)
        .context("store APNs key")?;
    Ok(())
}

pub fn delete_apns_key_pem() -> Result<()> {
    crate::integration_store::save_kind_secret(KIND_APNS, IntegrationSecretSlot::AuthKey, None)
        .map_err(anyhow::Error::msg)
        .context("delete APNs key")?;
    Ok(())
}

pub fn keychain_available() -> bool {
    true
}
