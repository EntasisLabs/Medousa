//! Provider keys, after medousa-home's Providers tab: each provider's API
//! key and base URL, kept by the workshop's engine. A custom provider's id
//! is a file on the workshop's machine, so it is set only for one this Mac
//! runs.
use medousa_desktop_host::provider_catalog::{ProviderCatalogEntry, providers_catalog};
use medousa_types::{IntegrationConnection, IntegrationSecretSlot};
use std::collections::HashMap;

pub(crate) const CUSTOM: &str = "custom";

pub(crate) fn allows_base_url(e: &ProviderCatalogEntry) -> bool {
    e.supports_custom_base_url || e.id == "ollama" || e.id == CUSTOM
}

pub(crate) fn allows_key(e: &ProviderCatalogEntry) -> bool {
    e.needs_api_key || e.id == CUSTOM
}

/// Providers with a key or a base URL to set, in catalog order.
pub(crate) fn entries() -> Vec<ProviderCatalogEntry> {
    providers_catalog()
        .providers
        .into_iter()
        .filter(|e| allows_base_url(e) || allows_key(e))
        .collect()
}

pub(crate) fn find(id: &str) -> Option<ProviderCatalogEntry> {
    entries().into_iter().find(|e| e.id == id)
}

/// medousa-home's `providerLabel`.
pub(crate) fn label(e: &ProviderCatalogEntry) -> String {
    if e.id == "openai" {
        "OpenAI API".into()
    } else {
        e.label.clone()
    }
}

/// One read: the engine's connections and, on this Mac, the custom id.
#[derive(Clone, Debug, Default)]
pub(crate) struct Read {
    pub connections: Vec<IntegrationConnection>,
    pub custom_id: Option<String>,
}

impl Read {
    /// The base URL saved for a provider, if any.
    pub(crate) fn base_url(&self, kind: &str) -> Option<String> {
        self.connections
            .iter()
            .filter(|c| c.kind == kind)
            .find_map(|c| c.base_url.clone().filter(|u| !u.trim().is_empty()))
    }

    pub(crate) fn has_key(&self, kind: &str) -> bool {
        self.connections
            .iter()
            .any(|c| c.kind == kind && c.secrets.slot(IntegrationSecretSlot::ApiKey))
    }

    /// Where a provider's key is kept: a custom provider's under its id.
    pub(crate) fn key_kind(&self, e: &ProviderCatalogEntry) -> String {
        if e.id == CUSTOM {
            self.custom_id.clone().unwrap_or_else(|| CUSTOM.into())
        } else {
            e.id.clone()
        }
    }

    /// medousa-home's `formatProviderSettingsSummary`.
    pub(crate) fn summary(&self, e: &ProviderCatalogEntry) -> String {
        let mut parts: Vec<String> = Vec::new();
        if e.id == CUSTOM {
            parts.push(
                self.custom_id
                    .clone()
                    .unwrap_or_else(|| "Not configured".into()),
            );
        }
        if allows_base_url(e) {
            match self.base_url(&e.id) {
                Some(url) if url.chars().count() > 28 => {
                    parts.push(format!("{}…", url.chars().take(25).collect::<String>()))
                }
                Some(url) => parts.push(url),
                None if e.default_base_url.is_some() => parts.push("Default URL".into()),
                None => {}
            }
        }
        if allows_key(e) {
            parts.push(
                if self.has_key(&self.key_kind(e)) {
                    "Key stored"
                } else {
                    "No key"
                }
                .into(),
            );
        }
        if parts.is_empty() {
            "Not set up".into()
        } else {
            parts.join(" · ")
        }
    }

    /// How many providers have a key kept.
    pub(crate) fn keyed(&self) -> usize {
        entries()
            .iter()
            .filter(|e| allows_key(e) && self.has_key(&self.key_kind(e)))
            .count()
    }
}

/// The form's fields as saved. The key starts empty: it is never read back.
pub(crate) fn fill(read: &Read, e: &ProviderCatalogEntry) -> Vec<(&'static str, String)> {
    vec![
        ("provider.id", read.custom_id.clone().unwrap_or_default()),
        (
            "provider.url",
            read.base_url(&e.id)
                .or_else(|| e.default_base_url.clone())
                .unwrap_or_default(),
        ),
        ("provider.key", String::new()),
    ]
}

pub(crate) const KEYS: [&str; 3] = ["provider.id", "provider.url", "provider.key"];

/// medousa-home's `normalizeCustomProviderId`.
pub(crate) fn normalize_id(raw: &str) -> String {
    raw.trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
}

/// An http(s) URL without its trailing slashes, as medousa-home keeps it.
fn base_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let url = reqwest::Url::parse(trimmed).ok()?;
    matches!(url.scheme(), "http" | "https").then(|| trimmed.trim_end_matches('/').to_string())
}

/// What one Save does.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Plan {
    /// A custom provider's id, written on this Mac.
    pub custom_id: Option<String>,
    /// The connection whose base URL changes, and the new one (`None`
    /// returns to the default).
    pub base_url: Option<(String, Option<String>)>,
    /// The connection a new key is kept under, and the key.
    pub key: Option<(String, String)>,
}

/// Builds a save from the form, as medousa-home's provider sheet does.
pub(crate) fn plan(
    e: &ProviderCatalogEntry,
    read: &Read,
    drafts: &HashMap<&'static str, String>,
) -> Result<Plan, String> {
    let d = |k: &str| drafts.get(k).map(String::as_str).unwrap_or_default();
    let mut plan = Plan::default();
    let custom = e.id == CUSTOM;
    let mut key_kind = read.key_kind(e);
    if custom {
        let id = normalize_id(d("provider.id"));
        if id.is_empty() {
            return Err("Enter a provider id (e.g. openai, vllm).".into());
        }
        if read.custom_id.as_deref() != Some(id.as_str()) {
            plan.custom_id = Some(id.clone());
        }
        key_kind = id;
    }
    if allows_base_url(e) {
        let raw = d("provider.url").trim();
        let url = match raw {
            "" if custom => return Err("Custom providers require an API base URL.".into()),
            "" => None,
            raw => Some(base_url(raw).ok_or("Enter a valid http(s) API base URL.")?),
        };
        // The default needs no override.
        let url = url.filter(|u| {
            e.default_base_url
                .as_deref()
                .map(|d| d.trim_end_matches('/'))
                != Some(u.as_str())
        });
        if url != read.base_url(&e.id) {
            plan.base_url = Some((e.id.clone(), url));
        }
    }
    if allows_key(e) {
        let key = d("provider.key").trim();
        if !key.is_empty() {
            plan.key = Some((key_kind, key.to_string()));
        }
    }
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(connections: serde_json::Value, custom_id: Option<&str>) -> Read {
        Read {
            connections: serde_json::from_value(connections).unwrap(),
            custom_id: custom_id.map(str::to_string),
        }
    }

    fn conn(kind: &str, base_url: Option<&str>, key: bool) -> serde_json::Value {
        serde_json::json!({
            "connection_id": format!("conn_{}", "a".repeat(32)), "kind": kind,
            "base_url": base_url, "secrets": {"api_key": key},
            "created_at": "2026-11-01T00:00:00Z", "updated_at": "2026-11-01T00:00:00Z"
        })
    }

    #[test]
    fn rows_read_as_medousa_home_shows_them() {
        let r = read(
            serde_json::json!([
                conn("openai", None, true),
                conn(
                    "ollama",
                    Some("http://studio.local:11434/v1/very/long/path"),
                    false
                ),
            ]),
            None,
        );
        assert_eq!(
            r.summary(&find("openai").unwrap()),
            "Default URL · Key stored"
        );
        assert_eq!(r.summary(&find("anthropic").unwrap()), "No key");
        assert_eq!(
            r.summary(&find("ollama").unwrap()),
            "http://studio.local:11434…"
        );
        assert_eq!(r.summary(&find(CUSTOM).unwrap()), "Not configured · No key");
        assert_eq!(r.keyed(), 1);
        assert_eq!(label(&find("openai").unwrap()), "OpenAI API");
        assert!(find("openai-codex").is_none(), "signs in instead of a key");
    }

    #[test]
    fn a_save_changes_only_what_was_edited() {
        let r = read(serde_json::json!([conn("openai", None, true)]), None);
        let openai = find("openai").unwrap();
        let mut drafts: HashMap<&'static str, String> = fill(&r, &openai).into_iter().collect();
        assert_eq!(drafts["provider.url"], "https://api.openai.com/v1");
        assert_eq!(plan(&openai, &r, &drafts).unwrap(), Plan::default());

        drafts.insert("provider.key", " sk-new ".into());
        drafts.insert("provider.url", "https://proxy.example/v1/".into());
        let p = plan(&openai, &r, &drafts).unwrap();
        assert_eq!(p.key, Some(("openai".into(), "sk-new".into())));
        assert_eq!(
            p.base_url,
            Some(("openai".into(), Some("https://proxy.example/v1".into())))
        );

        drafts.insert("provider.url", "ftp://nope".into());
        assert!(plan(&openai, &r, &drafts).unwrap_err().contains("http(s)"));
    }

    #[test]
    fn a_custom_provider_needs_an_id_and_a_url_and_keeps_its_key_under_the_id() {
        let r = read(serde_json::json!([]), None);
        let custom = find(CUSTOM).unwrap();
        let mut drafts: HashMap<&'static str, String> = HashMap::new();
        assert!(
            plan(&custom, &r, &drafts)
                .unwrap_err()
                .contains("provider id")
        );
        drafts.insert("provider.id", " My VLLM ".into());
        assert!(plan(&custom, &r, &drafts).unwrap_err().contains("base URL"));
        drafts.insert("provider.url", "http://127.0.0.1:8000/v1".into());
        drafts.insert("provider.key", "k".into());
        let p = plan(&custom, &r, &drafts).unwrap();
        assert_eq!(p.custom_id.as_deref(), Some("my-vllm"));
        assert_eq!(p.key, Some(("my-vllm".into(), "k".into())));
        assert_eq!(
            p.base_url,
            Some(("custom".into(), Some("http://127.0.0.1:8000/v1".into())))
        );
    }
}
