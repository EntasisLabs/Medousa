//! Native authentication for first-party clients dialing the local daemon.

use std::net::IpAddr;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use reqwest::Url;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use zeroize::Zeroize;

/// Build an async client whose local-daemon requests carry the named bearer.
/// Remote URLs deliberately receive no local credential.
pub fn async_client(base_url: &str, credential_name: &str) -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .default_headers(default_headers(base_url, credential_name)?)
        .build()
        .context("build daemon HTTP client")
}

pub fn blocking_client_with_timeout(
    base_url: &str,
    credential_name: &str,
    timeout: Duration,
) -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .default_headers(default_headers(base_url, credential_name)?)
        .timeout(timeout)
        .build()
        .context("build daemon blocking HTTP client")
}

/// Return the native Authorization value for an HTTP or WebSocket handshake.
pub fn authorization_header(base_url: &str, credential_name: &str) -> Result<Option<HeaderValue>> {
    authorization_header_from_data_dir(base_url, credential_name, &crate::paths::medousa_data_dir())
}

fn authorization_header_from_data_dir(
    base_url: &str,
    credential_name: &str,
    data_dir: &Path,
) -> Result<Option<HeaderValue>> {
    if !is_loopback_url(base_url)? {
        return Ok(None);
    }
    let secret = medousa_local_credential::load_named_secret(data_dir, credential_name)
        .with_context(|| {
            format!(
                "load {credential_name} local daemon credential; start Medousa once to provision it"
            )
        })?;
    let mut encoded = Vec::with_capacity(7 + secret.token().len());
    encoded.extend_from_slice(b"Bearer ");
    encoded.extend_from_slice(secret.token().as_bytes());
    let value = HeaderValue::from_bytes(&encoded)
        .context("encode local daemon authorization header")
        .map(|mut value| {
            value.set_sensitive(true);
            value
        });
    encoded.zeroize();
    value.map(Some)
}

fn default_headers(base_url: &str, credential_name: &str) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    if let Some(value) = authorization_header(base_url, credential_name)? {
        headers.insert(AUTHORIZATION, value);
    }
    Ok(headers)
}

fn is_loopback_url(base_url: &str) -> Result<bool> {
    let url = Url::parse(base_url).with_context(|| format!("invalid daemon URL {base_url}"))?;
    if !matches!(url.scheme(), "http" | "https" | "ws" | "wss") {
        bail!("unsupported daemon URL scheme {}", url.scheme());
    }
    let Some(host) = url.host_str() else {
        bail!("daemon URL has no host");
    };
    if host.eq_ignore_ascii_case("localhost") {
        return Ok(true);
    }
    let ip_literal = host
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(host);
    Ok(ip_literal
        .parse::<IpAddr>()
        .is_ok_and(|ip| ip.is_loopback()))
}

#[cfg(test)]
mod tests {
    use super::{authorization_header_from_data_dir, is_loopback_url};

    #[test]
    fn local_authority_is_bound_to_loopback_urls() {
        assert!(is_loopback_url("http://127.0.0.1:7777").unwrap());
        assert!(is_loopback_url("ws://[::1]:7777/v1/terminal").unwrap());
        assert!(is_loopback_url("http://localhost:7777").unwrap());
        assert!(!is_loopback_url("https://daemon.example.com").unwrap());
        assert!(!is_loopback_url("http://192.168.1.10:7777").unwrap());
        assert!(!is_loopback_url("http://127.0.0.1.example.com").unwrap());
    }

    #[test]
    fn missing_loopback_credential_fails_with_remediation() {
        let missing = std::env::temp_dir().join(format!(
            "medousa-local-auth-missing-{}",
            uuid::Uuid::new_v4()
        ));
        let error = authorization_header_from_data_dir(
            "http://127.0.0.1:7419",
            medousa_local_credential::CLI_LOCAL_NAME,
            &missing,
        )
        .expect_err("missing local credential must fail closed");
        assert!(error.to_string().contains("start Medousa once"));
        assert!(
            authorization_header_from_data_dir(
                "https://daemon.example.com",
                medousa_local_credential::CLI_LOCAL_NAME,
                &missing,
            )
            .unwrap()
            .is_none()
        );
    }
}
