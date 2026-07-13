//! SurrealDB connection settings (endpoint, namespace, database, optional auth).

use crate::product_config::ProductConfig;
use crate::session::TuiDefaults;
use stasis::prelude::{RuntimeBackend, SurrealAuth};

const DEFAULT_NAMESPACE: &str = "medousa";
const DEFAULT_DATABASE: &str = "runtime";

#[derive(Debug, Clone, Default)]
pub struct SurrealConnectionSettings {
    pub endpoint: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub namespace: Option<String>,
    pub database: Option<String>,
}

/// Merge product config + TUI defaults + environment into connection settings.
pub fn resolve_surreal_connection_settings(
    product: &ProductConfig,
    defaults: &TuiDefaults,
) -> SurrealConnectionSettings {
    SurrealConnectionSettings {
        endpoint: product
            .surreal
            .endpoint
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
            .or_else(|| {
                defaults
                    .surreal_endpoint
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToString::to_string)
            })
            .or_else(|| std::env::var("MEDOUSA_SURREAL_ENDPOINT").ok())
            .or_else(|| std::env::var("STASIS_SURREAL_ENDPOINT").ok())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        username: resolve_optional_string(
            product.surreal.username.as_deref(),
            defaults.surreal_username.as_deref(),
            "MEDOUSA_SURREAL_USERNAME",
            "STASIS_SURREAL_USERNAME",
        ),
        password: resolve_optional_string(
            product.surreal.password.as_deref(),
            defaults.surreal_password.as_deref(),
            "MEDOUSA_SURREAL_PASSWORD",
            "STASIS_SURREAL_PASSWORD",
        )
        .or_else(crate::session::load_surreal_password),
        namespace: resolve_optional_string(
            product.surreal.namespace.as_deref(),
            defaults.surreal_namespace.as_deref(),
            "MEDOUSA_SURREAL_NAMESPACE",
            "STASIS_SURREAL_NAMESPACE",
        ),
        database: resolve_optional_string(
            product.surreal.database.as_deref(),
            defaults.surreal_database.as_deref(),
            "MEDOUSA_SURREAL_DATABASE",
            "STASIS_SURREAL_DATABASE",
        ),
    }
}

fn resolve_optional_string(
    product: Option<&str>,
    defaults: Option<&str>,
    medousa_env: &str,
    stasis_env: &str,
) -> Option<String> {
    product
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            defaults
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
        })
        .or_else(|| std::env::var(medousa_env).ok())
        .or_else(|| std::env::var(stasis_env).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn resolve_surreal_namespace(settings: &SurrealConnectionSettings) -> String {
    settings
        .namespace
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(str::trim)
        .map(ToString::to_string)
        .unwrap_or_else(|| DEFAULT_NAMESPACE.to_string())
}

pub fn resolve_surreal_database(settings: &SurrealConnectionSettings) -> String {
    settings
        .database
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(str::trim)
        .map(ToString::to_string)
        .unwrap_or_else(|| DEFAULT_DATABASE.to_string())
}

/// Root/database credentials for Stasis `RuntimeBackend::with_surreal_auth`.
pub fn resolve_surreal_auth(settings: &SurrealConnectionSettings) -> Option<SurrealAuth> {
    let username = settings
        .username
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let password = settings
        .password
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    Some(SurrealAuth::new(username, password))
}

/// Attach resolved Surreal auth to any surreal-backed runtime backend.
pub fn apply_surreal_auth_to_backend(
    backend: RuntimeBackend,
    settings: &SurrealConnectionSettings,
) -> RuntimeBackend {
    match resolve_surreal_auth(settings) {
        Some(auth) => backend.with_surreal_auth(auth),
        None => backend,
    }
}

/// Strip `surreal-ws:` prefix; credentials are passed via `SurrealAuth`, not the URL.
///
/// `product_config.json` / `MEDOUSA_SURREAL_ENDPOINT` (via [`resolve_surreal_connection_settings`])
/// wins over a URL embedded in `--backend` or `onboard_profile.daemon_backend`, so stale profile
/// values cannot override a corrected Surreal endpoint.
pub fn resolve_surreal_ws_endpoint(raw_backend: &str, settings: &SurrealConnectionSettings) -> String {
    if let Some(endpoint) = settings
        .endpoint
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return strip_endpoint_userinfo(endpoint).0;
    }

    let endpoint = raw_backend
        .strip_prefix("surreal-ws:")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| "ws://127.0.0.1:8000/rpc".to_string());

    strip_endpoint_userinfo(&endpoint).0
}

/// Backend string passed to `medousa_daemon --backend` (CLI override, then product surreal endpoint).
pub fn resolve_daemon_launch_backend(
    cli_backend: Option<&str>,
    profile_daemon_backend: Option<&str>,
    product: &ProductConfig,
    defaults: &TuiDefaults,
) -> String {
    if let Some(backend) = cli_backend.map(str::trim).filter(|value| !value.is_empty()) {
        return backend.to_string();
    }

    let settings = resolve_surreal_connection_settings(product, defaults);
    if let Some(endpoint) = settings.endpoint.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        return format!("surreal-ws:{endpoint}");
    }

    if let Some(backend) = profile_daemon_backend.map(str::trim).filter(|value| !value.is_empty()) {
        return backend.to_string();
    }

    defaults
        .backend
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| "in-memory".to_string())
}

/// Keep `onboard_profile.daemon_backend` aligned with the canonical surreal endpoint.
pub fn sync_profile_daemon_backend(
    profile_daemon_backend: &mut Option<String>,
    product: &ProductConfig,
    defaults: &TuiDefaults,
) {
    let settings = resolve_surreal_connection_settings(product, defaults);
    if let Some(endpoint) = settings.endpoint.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        *profile_daemon_backend = Some(format!("surreal-ws:{endpoint}"));
    }
}

/// Split `ws://user:pass@host/...` into a credential-free endpoint and optional auth.
pub fn split_endpoint_userinfo(endpoint: &str) -> (String, Option<SurrealAuth>) {
    let (clean, user, pass) = strip_endpoint_userinfo(endpoint);
    match (user, pass) {
        (Some(username), Some(password)) => (clean, Some(SurrealAuth::new(username, password))),
        _ => (clean, None),
    }
}

fn strip_endpoint_userinfo(endpoint: &str) -> (String, Option<String>, Option<String>) {
    let endpoint = endpoint.trim();
    if endpoint.is_empty() {
        return ("ws://127.0.0.1:8000/rpc".to_string(), None, None);
    }

    let (scheme, rest) = if let Some(rest) = endpoint.strip_prefix("wss://") {
        ("wss://", rest)
    } else if let Some(rest) = endpoint.strip_prefix("ws://") {
        ("ws://", rest)
    } else {
        return (endpoint.to_string(), None, None);
    };

    let Some((auth, host_rest)) = rest.split_once('@') else {
        return (endpoint.to_string(), None, None);
    };

    let (username, password) = auth
        .split_once(':')
        .map(|(user, pass)| (decode_userinfo_component(user), decode_userinfo_component(pass)))
        .unwrap_or_else(|| (decode_userinfo_component(auth), String::new()));

    let username = (!username.is_empty()).then_some(username);
    let password = (!password.is_empty()).then_some(password);
    (
        format!("{scheme}{host_rest}"),
        username,
        password,
    )
}

fn decode_userinfo_component(value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch == '%' {
            let hi = chars.next();
            let lo = chars.next();
            if let (Some(hi), Some(lo)) = (hi, lo) {
                let hex = format!("{hi}{lo}");
                if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                    out.push(byte as char);
                    continue;
                }
            }
            out.push('%');
            if let Some(hi) = hi {
                out.push(hi);
            }
            if let Some(lo) = lo {
                out.push(lo);
            }
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_auth_requires_username_and_password() {
        let settings = SurrealConnectionSettings {
            username: Some("root".to_string()),
            password: Some("secret".to_string()),
            ..SurrealConnectionSettings::default()
        };
        let auth = resolve_surreal_auth(&settings).expect("auth");
        assert_eq!(auth.username, "root");
        assert_eq!(auth.password, "secret");
    }

    #[test]
    fn strips_userinfo_from_ws_endpoint() {
        let (endpoint, auth) =
            split_endpoint_userinfo("ws://root:pass@127.0.0.1:8000/rpc");
        assert_eq!(endpoint, "ws://127.0.0.1:8000/rpc");
        let auth = auth.expect("auth");
        assert_eq!(auth.username, "root");
        assert_eq!(auth.password, "pass");
    }

    #[test]
    fn resolve_ws_endpoint_does_not_embed_credentials() {
        let settings = SurrealConnectionSettings {
            endpoint: Some("ws://127.0.0.1:8000/rpc".to_string()),
            username: Some("root".to_string()),
            password: Some("s3cret".to_string()),
            ..SurrealConnectionSettings::default()
        };
        let out = resolve_surreal_ws_endpoint("surreal-ws:", &settings);
        assert_eq!(out, "ws://127.0.0.1:8000/rpc");
    }

    #[test]
    fn product_endpoint_overrides_stale_backend_url() {
        let settings = SurrealConnectionSettings {
            endpoint: Some("ws://10.12.0.13:9096/rpc".to_string()),
            ..SurrealConnectionSettings::default()
        };
        let out = resolve_surreal_ws_endpoint("surreal-ws:ws://10.12.0.11:906/rpc", &settings);
        assert_eq!(out, "ws://10.12.0.13:9096/rpc");
    }
}
