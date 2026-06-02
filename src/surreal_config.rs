//! SurrealDB connection settings (endpoint credentials, namespace, database).

use crate::product_config::ProductConfig;
use crate::session::TuiDefaults;

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
        .or_else(|| crate::session::load_surreal_password()),
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

/// Strip `surreal-ws:` prefix and apply saved credentials when the URL has no userinfo.
pub fn resolve_surreal_ws_endpoint(raw_backend: &str, settings: &SurrealConnectionSettings) -> String {
    let endpoint = raw_backend
        .strip_prefix("surreal-ws:")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            settings
                .endpoint
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
        })
        .unwrap_or_else(|| "ws://127.0.0.1:8000/rpc".to_string());

    apply_credentials_to_endpoint(
        &endpoint,
        settings.username.as_deref(),
        settings.password.as_deref(),
    )
}

pub fn apply_credentials_to_endpoint(
    endpoint: &str,
    username: Option<&str>,
    password: Option<&str>,
) -> String {
    let endpoint = endpoint.trim();
    if endpoint.is_empty() {
        return "ws://127.0.0.1:8000/rpc".to_string();
    }
    if endpoint.contains('@') {
        return endpoint.to_string();
    }

    let user = username.map(str::trim).filter(|value| !value.is_empty());
    let pass = password.map(str::trim).filter(|value| !value.is_empty());
    if user.is_none() && pass.is_none() {
        return endpoint.to_string();
    }

    let (scheme, rest) = if let Some(rest) = endpoint.strip_prefix("wss://") {
        ("wss://", rest)
    } else if let Some(rest) = endpoint.strip_prefix("ws://") {
        ("ws://", rest)
    } else {
        return endpoint.to_string();
    };

    let auth = match (user, pass) {
        (Some(user), Some(pass)) => {
            format!(
                "{}:{}@",
                encode_userinfo_component(user),
                encode_userinfo_component(pass)
            )
        }
        (Some(user), None) => format!("{}@", encode_userinfo_component(user)),
        _ => return endpoint.to_string(),
    };

    format!("{scheme}{auth}{rest}")
}

fn encode_userinfo_component(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '~') {
            out.push(ch);
        } else {
            let mut buf = [0u8; 4];
            let encoded = ch.encode_utf8(&mut buf);
            for byte in encoded.bytes() {
                out.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injects_credentials_into_ws_endpoint() {
        let out = apply_credentials_to_endpoint(
            "ws://127.0.0.1:8000/rpc",
            Some("root"),
            Some("s3cret"),
        );
        assert_eq!(out, "ws://root:s3cret@127.0.0.1:8000/rpc");
    }

    #[test]
    fn leaves_endpoint_with_existing_auth_unchanged() {
        let out = apply_credentials_to_endpoint(
            "ws://root:pass@127.0.0.1:8000/rpc",
            Some("other"),
            Some("ignored"),
        );
        assert_eq!(out, "ws://root:pass@127.0.0.1:8000/rpc");
    }
}
