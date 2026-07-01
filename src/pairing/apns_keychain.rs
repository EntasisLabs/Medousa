//! macOS Keychain / platform secret store for the APNs Auth Key (.p8 PEM).

use anyhow::{Context, Result};

const APNS_KEY_SERVICE: &str = "medousa.apns";
const APNS_KEY_ACCOUNT: &str = "auth_key";

pub fn apns_key_keyring_entry() -> Result<keyring::Entry, keyring::Error> {
    keyring::Entry::new(APNS_KEY_SERVICE, APNS_KEY_ACCOUNT)
}

pub fn load_apns_key_pem() -> Option<String> {
    let entry = apns_key_keyring_entry().ok()?;
    entry
        .get_password()
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn store_apns_key_pem(pem: &str) -> Result<()> {
    let trimmed = pem.trim();
    if trimmed.is_empty() {
        anyhow::bail!("APNs key PEM is empty");
    }
    let entry = apns_key_keyring_entry().context("open keychain entry for APNs key")?;
    entry
        .set_password(trimmed)
        .context("store APNs key in keychain")?;
    Ok(())
}

pub fn delete_apns_key_pem() -> Result<()> {
    let entry = apns_key_keyring_entry().context("open keychain entry for APNs key")?;
    match entry.delete_password() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(err).context("delete APNs key from keychain"),
    }
}

pub fn keychain_available() -> bool {
    apns_key_keyring_entry().is_ok()
}
