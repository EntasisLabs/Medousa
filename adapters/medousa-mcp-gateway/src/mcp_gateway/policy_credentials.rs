//! Installation-scoped credential for the gateway's local policy callback.
//! Only daemon startup creates it; gateways read it without minting authority.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use medousa_types::DaemonSecretPath;

pub fn data_dir() -> PathBuf {
    if let Some(path) = std::env::var("MEDOUSA_DATA_DIR")
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        return PathBuf::from(path.trim());
    }
    let default = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("medousa");
    std::fs::read_to_string(default.join("data_dir"))
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(|value| PathBuf::from(value.trim()))
        .unwrap_or(default)
}

/// Called off the async executor during daemon startup. The file lock serializes
/// first creation; the token itself lives in the existing typed secret store.
pub fn initialize_local_policy_token(root: &Path, configured: Option<String>) -> Result<String> {
    std::fs::create_dir_all(root)?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join("mcp-policy.lock"))?;
    lock.lock()
        .context("lock MCP policy credential initialization")?;
    let installation_id = medousa_secrets::ensure_installation_id(root)?;
    let path = DaemonSecretPath::McpPolicy { installation_id };
    let existing = medousa_secrets::load_daemon_secret(root, &path)?;
    let token = configured
        .filter(|token| !token.trim().is_empty())
        .or_else(|| existing.as_ref().map(|secret| secret.value.clone()))
        .unwrap_or_else(|| {
            format!(
                "{}{}",
                uuid::Uuid::new_v4().simple(),
                uuid::Uuid::new_v4().simple()
            )
        });
    if token.trim().is_empty() {
        bail!("stored MCP policy credential is empty");
    }
    if existing.as_ref().is_none_or(|secret| secret.value != token) {
        medousa_secrets::save_daemon_secret(root, &path, &token)
            .context("persist local MCP policy credential")?;
    }
    Ok(token)
}

pub fn load_local_policy_token(root: &Path) -> Result<Option<String>> {
    let Some(installation_id) = medousa_secrets::load_installation_id(root)? else {
        return Ok(None);
    };
    Ok(
        medousa_secrets::load_daemon_secret(
            root,
            &DaemonSecretPath::McpPolicy { installation_id },
        )?
        .map(|secret| secret.value)
        .filter(|token| !token.trim().is_empty()),
    )
}

pub fn is_local_policy_url(raw: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(raw) else {
        return false;
    };
    matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some_and(|host| {
            host.trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .is_ok_and(|address| address.is_loopback())
        })
        && url.username().is_empty()
        && url.password().is_none()
        && url.path() == "/v1/mcp/policy/evaluate"
        && url.query().is_none()
        && url.fragment().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installation_credential_is_restricted_to_the_loopback_policy_endpoint() {
        for url in [
            "http://127.0.0.1:7419/v1/mcp/policy/evaluate",
            "http://[::1]:7419/v1/mcp/policy/evaluate",
        ] {
            assert!(is_local_policy_url(url));
        }
        for url in [
            "https://example.com/v1/mcp/policy/evaluate",
            "http://192.0.2.1/v1/mcp/policy/evaluate",
            "http://127.0.0.1:7419/v1/runtime/admin",
            "http://127.0.0.1:7419/v1/mcp/policy/evaluate?redirect=1",
            "http://user@127.0.0.1:7419/v1/mcp/policy/evaluate",
        ] {
            assert!(!is_local_policy_url(url));
        }
        let path = DaemonSecretPath::McpPolicy {
            installation_id: medousa_types::InstallationId::parse(
                "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
            )
            .unwrap(),
        };
        assert_eq!(DaemonSecretPath::parse(&path.account()).unwrap(), path);
    }
}
