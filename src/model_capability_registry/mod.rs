pub mod adapters;
pub mod cache;
pub mod handlers;
pub mod heuristic;
pub mod types;

use std::collections::{HashMap, HashSet};
use std::sync::{OnceLock, RwLock};

use chrono::{Duration, Utc};

use self::adapters::{fetch_provider_catalog, http_client, openrouter_slug_for};
use self::cache::{
    load_all_snapshots, load_index, save_index, save_provider_snapshot,
};
use self::heuristic::{infer_capability, infer_supports_vision};
use self::types::{
    CatalogFreshnessResponse, CatalogIndex, CatalogIndexEntry, CatalogProviderFreshness,
    ModelCapabilitiesLookupResponse, ModelCatalogListQuery, ModelCatalogListResponse,
    ModelCatalogRefreshFailure, ModelCatalogRefreshResponse, ModelCapabilityRecord,
    ProviderCatalogSnapshot,
};

static REGISTRY: OnceLock<ModelCapabilityRegistry> = OnceLock::new();

pub struct ModelCapabilityRegistry {
    index: RwLock<CatalogIndex>,
    snapshots: RwLock<HashMap<String, ProviderCatalogSnapshot>>,
}

pub fn registry() -> &'static ModelCapabilityRegistry {
    ModelCapabilityRegistry::global()
}

impl ModelCapabilityRegistry {
    pub fn global() -> &'static Self {
        REGISTRY.get_or_init(Self::bootstrap)
    }

    fn bootstrap() -> Self {
        let index = load_index();
        let snapshots = load_all_snapshots(&index);
        Self {
            index: RwLock::new(index),
            snapshots: RwLock::new(snapshots),
        }
    }

    pub fn supports_vision(&self, provider: &str, model: &str) -> bool {
        match self.lookup_record(provider, model) {
            Some(record) => record.supports_vision,
            None => infer_supports_vision(provider, model),
        }
    }

    pub fn resolve(&self, provider: &str, model: &str) -> ModelCapabilitiesLookupResponse {
        if let Some(record) = self.lookup_record(provider, model) {
            return ModelCapabilitiesLookupResponse {
                found: true,
                model: Some(record),
                heuristic: false,
            };
        }

        ModelCapabilitiesLookupResponse {
            found: false,
            model: Some(infer_capability(provider, model)),
            heuristic: true,
        }
    }

    pub fn catalog_freshness(&self) -> CatalogFreshnessResponse {
        let index = self.index.read().expect("catalog index lock");
        let ttl_secs = index.ttl_secs;
        let providers = index
            .providers
            .iter()
            .map(|(provider, entry)| provider_freshness(provider, entry, ttl_secs))
            .collect();
        CatalogFreshnessResponse {
            ttl_secs,
            providers,
        }
    }

    pub fn list_catalog(&self, query: ModelCatalogListQuery) -> ModelCatalogListResponse {
        let snapshots = self.snapshots.read().expect("catalog snapshots lock");
        let provider_filter = query
            .provider
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_ascii_lowercase());
        let capability = query
            .capability
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_ascii_lowercase());
        let search = query
            .q
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_ascii_lowercase());

        let mut models = Vec::new();
        for (provider, snapshot) in snapshots.iter() {
            if let Some(filter) = provider_filter.as_deref()
                && provider != filter
            {
                continue;
            }
            for record in &snapshot.models {
                if let Some(cap) = capability.as_deref() {
                    match cap {
                        "vision" if !record.supports_vision => continue,
                        other if other != "vision" => continue,
                        _ => {}
                    }
                }
                if let Some(needle) = search.as_deref() {
                    let haystack = format!(
                        "{} {}",
                        record.model_id.to_ascii_lowercase(),
                        record
                            .display_name
                            .as_deref()
                            .unwrap_or_default()
                            .to_ascii_lowercase()
                    );
                    if !haystack.contains(needle) {
                        continue;
                    }
                }
                models.push(record.clone());
            }
        }
        models.sort_by(|left, right| left.model_id.cmp(&right.model_id));

        ModelCatalogListResponse {
            freshness: self.catalog_freshness(),
            models,
        }
    }

    pub fn any_stale(&self, providers: &[String]) -> bool {
        let index = self.index.read().expect("catalog index lock");
        providers.iter().any(|provider| {
            let provider = provider.trim().to_ascii_lowercase();
            match index.providers.get(&provider) {
                Some(entry) => is_entry_stale(entry, index.ttl_secs),
                None => true,
            }
        })
    }

    pub async fn refresh(
        &self,
        providers: Option<Vec<String>>,
    ) -> ModelCatalogRefreshResponse {
        let targets = normalize_provider_list(providers);
        let client = match http_client() {
            Ok(client) => client,
            Err(message) => {
                return ModelCatalogRefreshResponse {
                    refreshed: Vec::new(),
                    failures: targets
                        .into_iter()
                        .map(|provider| ModelCatalogRefreshFailure {
                            provider,
                            message: message.clone(),
                        })
                        .collect(),
                    freshness: self.catalog_freshness(),
                };
            }
        };

        let mut refreshed = Vec::new();
        let mut failures = Vec::new();
        let mut openrouter_overlay = self
            .snapshots
            .read()
            .expect("catalog snapshots lock")
            .get("openrouter")
            .map(|snapshot| snapshot.models.clone());

        if targets.iter().any(|provider| provider == "openrouter")
            || openrouter_overlay.is_none()
        {
            let snapshot = fetch_provider_catalog(&client, "openrouter", None, None, None).await;
            if snapshot.error.is_none() {
                openrouter_overlay = Some(snapshot.models.clone());
            }
            self.persist_snapshot(snapshot);
            refreshed.push("openrouter".to_string());
        }

        let overlay = openrouter_overlay.as_deref();
        for provider in targets {
            if provider == "openrouter" {
                continue;
            }
            let api_key = provider_api_key(&provider);
            let base_url = provider_base_url(&provider);
            let snapshot =
                fetch_provider_catalog(&client, &provider, api_key.as_deref(), base_url.as_deref(), overlay)
                    .await;
            if let Some(err) = snapshot.error.clone() {
                failures.push(ModelCatalogRefreshFailure {
                    provider: provider.clone(),
                    message: err,
                });
            } else {
                refreshed.push(provider.clone());
            }
            self.persist_snapshot(snapshot);
        }

        ModelCatalogRefreshResponse {
            refreshed,
            failures,
            freshness: self.catalog_freshness(),
        }
    }

    fn persist_snapshot(&self, snapshot: ProviderCatalogSnapshot) {
        let provider = snapshot.provider.trim().to_ascii_lowercase();
        if let Err(err) = save_provider_snapshot(&snapshot) {
            eprintln!("model catalog: failed to save {provider}: {err:#}");
        }

        {
            let mut snapshots = self.snapshots.write().expect("catalog snapshots lock");
            snapshots.insert(provider.clone(), snapshot.clone());
        }

        let mut index = self.index.write().expect("catalog index lock");
        index.providers.insert(
            provider,
            CatalogIndexEntry {
                fetched_at: Some(snapshot.fetched_at),
                model_count: snapshot.models.len(),
                source: snapshot.source.clone(),
                error: snapshot.error.clone(),
            },
        );
        if let Err(err) = save_index(&index) {
            eprintln!("model catalog: failed to save index: {err:#}");
        }
    }

    fn lookup_record(&self, provider: &str, model: &str) -> Option<ModelCapabilityRecord> {
        let provider = provider.trim().to_ascii_lowercase();
        let model = model.trim();
        if model.is_empty() {
            return None;
        }

        let snapshots = self.snapshots.read().expect("catalog snapshots lock");
        if let Some(snapshot) = snapshots.get(&provider) {
            if let Some(record) = find_model_record(&snapshot.models, model) {
                return Some(record);
            }
        }

        if provider != "openrouter" {
            let slug = openrouter_slug_for(&provider, model);
            if let Some(snapshot) = snapshots.get("openrouter") {
                if let Some(record) = find_model_record(&snapshot.models, &slug) {
                    return Some(enrich_record_provider(record, &provider, model));
                }
            }
        }

        None
    }
}

pub fn default_refresh_providers() -> Vec<String> {
    let mut providers = vec!["openrouter".to_string()];
    let defaults = crate::session::load_tui_defaults();
    if let Some(provider) = defaults
        .provider
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        providers.push(provider.to_ascii_lowercase());
    }
    dedupe_providers(providers)
}

fn normalize_provider_list(providers: Option<Vec<String>>) -> Vec<String> {
    match providers {
        Some(list) if !list.is_empty() => dedupe_providers(
            list.into_iter()
                .map(|provider| provider.trim().to_ascii_lowercase())
                .filter(|provider| !provider.is_empty())
                .collect(),
        ),
        _ => default_refresh_providers(),
    }
}

fn dedupe_providers(providers: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    providers
        .into_iter()
        .filter(|provider| seen.insert(provider.clone()))
        .collect()
}

fn find_model_record(models: &[ModelCapabilityRecord], model: &str) -> Option<ModelCapabilityRecord> {
    models
        .iter()
        .find(|record| {
            record.model_id.eq_ignore_ascii_case(model)
                || record.model_id.ends_with(&format!("/{model}"))
        })
        .cloned()
}

fn enrich_record_provider(
    record: ModelCapabilityRecord,
    provider: &str,
    model: &str,
) -> ModelCapabilityRecord {
    ModelCapabilityRecord {
        provider: provider.to_string(),
        model_id: model.to_string(),
        source: format!("{}+{}", record.source, provider),
        ..record
    }
}

fn provider_freshness(
    provider: &str,
    entry: &CatalogIndexEntry,
    ttl_secs: u64,
) -> CatalogProviderFreshness {
    CatalogProviderFreshness {
        provider: provider.to_string(),
        fetched_at: entry.fetched_at,
        model_count: entry.model_count,
        source: entry.source.clone(),
        stale: is_entry_stale(entry, ttl_secs),
        error: entry.error.clone(),
    }
}

fn is_entry_stale(entry: &CatalogIndexEntry, ttl_secs: u64) -> bool {
    match entry.fetched_at {
        None => true,
        Some(fetched_at) => {
            Utc::now().signed_duration_since(fetched_at)
                > Duration::seconds(ttl_secs as i64)
        }
    }
}

fn provider_api_key(provider: &str) -> Option<String> {
    crate::session::load_provider_api_key(provider)
}

fn provider_base_url(provider: &str) -> Option<String> {
    let provider = provider.trim().to_ascii_lowercase();
    let defaults = crate::session::load_tui_defaults();
    if defaults
        .provider
        .as_deref()
        .is_some_and(|saved| saved.trim().eq_ignore_ascii_case(&provider))
    {
        if let Some(url) = defaults
            .base_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Some(url.to_string());
        }
    }

    let normalized = provider.to_ascii_uppercase().replace('-', "_");
    for key in [
        format!("MEDOUSA_{normalized}_BASE_URL"),
        format!("STASIS_{normalized}_BASE_URL"),
    ] {
        if let Ok(value) = std::env::var(&key) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openrouter_slug_lookup_uses_heuristic_when_empty_cache() {
        let registry = ModelCapabilityRegistry {
            index: RwLock::new(CatalogIndex::default()),
            snapshots: RwLock::new(HashMap::new()),
        };
        assert!(registry.supports_vision("openrouter", "openai/gpt-4o-mini"));
    }

    #[test]
    fn resolve_without_cache_returns_heuristic_record() {
        let registry = ModelCapabilityRegistry {
            index: RwLock::new(CatalogIndex::default()),
            snapshots: RwLock::new(HashMap::new()),
        };
        let response = registry.resolve("openrouter", "openai/gpt-4o-mini");
        assert!(!response.found);
        assert!(response.heuristic);
        assert!(response.model.unwrap().supports_vision);
    }
}
