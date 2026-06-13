//! Unified capability catalog — intent layer above Grapheme ops and MCP tools.
//!
//! Design: docs/internal/capability-catalog-design.md

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Where a capability binding is implemented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilitySource {
    Grapheme,
    Mcp,
}

impl CapabilitySource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Grapheme => "grapheme",
            Self::Mcp => "mcp",
        }
    }
}

/// Stable capability identifier (snake_case), e.g. `document_search`.
pub type CapabilityId = String;

/// Canonical capability definition from manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityDefinition {
    pub id: CapabilityId,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
}

/// Explicit Grapheme binding from manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphemeCapabilityBindingSpec {
    pub module_op: String,
    #[serde(default)]
    pub priority: u16,
    #[serde(default = "default_binding_enabled")]
    pub enabled: bool,
}

/// Explicit MCP binding from manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpCapabilityBindingSpec {
    pub server_id: String,
    pub tool_name: String,
    #[serde(default)]
    pub priority: u16,
    #[serde(default = "default_binding_enabled")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect_class: Option<String>,
}

fn default_binding_enabled() -> bool {
    true
}

impl Default for GraphemeCapabilityBindingSpec {
    fn default() -> Self {
        Self {
            module_op: String::new(),
            priority: 0,
            enabled: true,
        }
    }
}

impl Default for McpCapabilityBindingSpec {
    fn default() -> Self {
        Self {
            server_id: String::new(),
            tool_name: String::new(),
            priority: 0,
            enabled: true,
            effect_class: None,
        }
    }
}

/// Operator-disabled binding stored in capabilities.toml overlay.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DisabledBindingRef {
    pub capability_id: String,
    pub source: String,
    pub reference: String,
}

/// Manifest entry: definition fields + declared bindings (TOML-friendly layout).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityManifestEntry {
    pub id: CapabilityId,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub bindings: CapabilityManifestBindings,
}

impl CapabilityManifestEntry {
    pub fn definition(&self) -> CapabilityDefinition {
        CapabilityDefinition {
            id: self.id.clone(),
            title: self.title.clone(),
            description: self.description.clone(),
            aliases: self.aliases.clone(),
            keywords: self.keywords.clone(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CapabilityManifestBindings {
    #[serde(default)]
    pub grapheme: Vec<GraphemeCapabilityBindingSpec>,
    #[serde(default)]
    pub mcp: Vec<McpCapabilityBindingSpec>,
}

/// Operator preferences for first-class `cognition_web_search` (optional overlay in capabilities.toml).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WebSearchSettings {
    /// Preferred `web.<provider>` binding when the model omits `provider` (e.g. duckduckgo, tavily, google).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred_provider: Option<String>,
    /// When true, try lower-priority bindings after the preferred one fails.
    #[serde(default = "default_web_search_try_fallbacks")]
    pub try_fallbacks: bool,
}

fn default_web_search_try_fallbacks() -> bool {
    true
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CapabilityManifest {
    #[serde(default)]
    pub capabilities: Vec<CapabilityManifestEntry>,
    #[serde(default)]
    pub web_search: WebSearchSettings,
    #[serde(default)]
    pub disabled_bindings: Vec<DisabledBindingRef>,
}

/// Resolved binding ready for agent consumption.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityBinding {
    pub source: CapabilitySource,
    /// Canonical ref: `module.op` for Grapheme, `server_id.tool_name` for MCP.
    pub reference: String,
    pub priority: u16,
    pub available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invoke_via: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub op: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect_class: Option<String>,
}

impl CapabilityBinding {
    pub fn grapheme(module_op: &str, priority: u16, available: bool) -> Self {
        let (module, op) = split_module_op(module_op);
        Self {
            source: CapabilitySource::Grapheme,
            reference: module_op.to_string(),
            priority,
            available,
            unavailable_reason: None,
            invoke_via: Some("cognition_grapheme_run".to_string()),
            module: Some(module),
            op: Some(op),
            server_id: None,
            tool_name: None,
            effect_class: None,
        }
    }

    pub fn mcp(
        server_id: &str,
        tool_name: &str,
        priority: u16,
        available: bool,
        effect_class: Option<String>,
    ) -> Self {
        Self {
            source: CapabilitySource::Mcp,
            reference: format!("{server_id}.{tool_name}"),
            priority,
            available,
            unavailable_reason: None,
            invoke_via: Some("cognition_mcp_invoke".to_string()),
            module: None,
            op: None,
            server_id: Some(server_id.to_string()),
            tool_name: Some(tool_name.to_string()),
            effect_class,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityResolveRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability: Option<CapabilityId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityImplementations {
    #[serde(default)]
    pub grapheme: Vec<CapabilityBinding>,
    #[serde(default)]
    pub mcp: Vec<CapabilityBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityRecommendation {
    pub source: CapabilitySource,
    pub reference: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityResolveResponse {
    pub capability: CapabilityId,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub implementations: CapabilityImplementations,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommended: Option<CapabilityRecommendation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gateway_unreachable: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilitySearchMatch {
    pub capability: CapabilityId,
    pub title: String,
    pub score: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matched_on: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilitySearchRequest {
    pub query: String,
    #[serde(default = "default_search_limit")]
    pub limit: usize,
}

fn default_search_limit() -> usize {
    10
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilitySearchResponse {
    pub query: String,
    pub matches: Vec<CapabilitySearchMatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityBindingSummary {
    pub source: String,
    pub reference: String,
    pub available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect_class: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invoke_via: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityListEntry {
    pub id: CapabilityId,
    pub title: String,
    pub binding_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub domain: String,
    pub has_grapheme: bool,
    pub has_mcp: bool,
    #[serde(default)]
    pub bindings_summary: Vec<CapabilityBindingSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityListResponse {
    pub capabilities: Vec<CapabilityListEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityReindexResponse {
    pub capability_count: usize,
    pub binding_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_path: Option<String>,
    pub manifest_loaded_from_file: bool,
    pub gateway_synced: bool,
    pub now_utc: DateTime<Utc>,
}

/// MCP catalog row synced from gateway (includes capability tags).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpCatalogSyncEntry {
    pub server_id: String,
    pub tool_name: String,
    pub title: String,
    #[serde(default)]
    pub capability_ids: Vec<CapabilityId>,
    pub available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpCatalogSyncResponse {
    pub entries: Vec<McpCatalogSyncEntry>,
    pub now_utc: chrono::DateTime<chrono::Utc>,
}

/// In-memory capability registry with inverted index.
#[derive(Debug, Clone, Default)]
pub struct CapabilityRegistry {
    definitions: HashMap<CapabilityId, CapabilityDefinition>,
    bindings: HashMap<CapabilityId, Vec<CapabilityBinding>>,
}

impl CapabilityManifest {
    pub fn is_binding_disabled(
        &self,
        capability_id: &str,
        source: CapabilitySource,
        reference: &str,
    ) -> bool {
        self.disabled_bindings.iter().any(|entry| {
            entry.capability_id.eq_ignore_ascii_case(capability_id)
                && entry.source.eq_ignore_ascii_case(source.as_str())
                && entry.reference == reference
        })
    }
}

impl CapabilityRegistry {
    pub fn from_manifest(manifest: &CapabilityManifest) -> Self {
        let mut registry = Self::default();
        for entry in &manifest.capabilities {
            let id = entry.id.clone();
            let definition = entry.definition();
            registry.definitions.insert(id.clone(), definition);

            let mut resolved = Vec::new();
            for binding in &entry.bindings.grapheme {
                if !binding.enabled {
                    continue;
                }
                if manifest.is_binding_disabled(&id, CapabilitySource::Grapheme, &binding.module_op) {
                    continue;
                }
                resolved.push(CapabilityBinding::grapheme(
                    &binding.module_op,
                    binding.priority,
                    true,
                ));
            }
            for binding in &entry.bindings.mcp {
                if !binding.enabled {
                    continue;
                }
                let reference = format!("{}.{}", binding.server_id, binding.tool_name);
                if manifest.is_binding_disabled(&id, CapabilitySource::Mcp, &reference) {
                    continue;
                }
                resolved.push(CapabilityBinding::mcp(
                    &binding.server_id,
                    &binding.tool_name,
                    binding.priority,
                    false,
                    binding.effect_class.clone(),
                ));
            }
            resolved.sort_by_key(|b| b.priority);
            registry.bindings.insert(id, resolved);
        }
        registry
    }

    pub fn with_embedded_seed() -> Self {
        Self::from_manifest(&embedded_capability_manifest())
    }

    pub fn with_loaded_manifest() -> Self {
        let (manifest, _) = load_capability_manifest();
        Self::from_manifest(&manifest)
    }

    pub fn binding_count(&self) -> usize {
        self.bindings.values().map(|bindings| bindings.len()).sum()
    }

    pub fn list(&self) -> CapabilityListResponse {
        let mut capabilities = self
            .definitions
            .iter()
            .map(|(id, def)| {
                let bindings = self.bindings.get(id).cloned().unwrap_or_default();
                let bindings_summary = bindings
                    .iter()
                    .map(|binding| CapabilityBindingSummary {
                        source: binding.source.as_str().to_string(),
                        reference: binding.reference.clone(),
                        available: binding.available,
                        effect_class: binding.effect_class.clone(),
                        invoke_via: binding.invoke_via.clone(),
                    })
                    .collect::<Vec<_>>();
                let has_grapheme = bindings_summary
                    .iter()
                    .any(|binding| binding.source == "grapheme");
                let has_mcp = bindings_summary.iter().any(|binding| binding.source == "mcp");
                CapabilityListEntry {
                    id: id.clone(),
                    title: def.title.clone(),
                    description: def.description.clone(),
                    domain: capability_domain(id),
                    binding_count: bindings.len(),
                    has_grapheme,
                    has_mcp,
                    bindings_summary,
                }
            })
            .collect::<Vec<_>>();
        capabilities.sort_by(|a, b| a.id.cmp(&b.id));
        CapabilityListResponse { capabilities }
    }

    pub fn resolve(&self, capability_id: &str) -> Option<CapabilityResolveResponse> {
        let definition = self.definitions.get(capability_id)?;
        let bindings = self.bindings.get(capability_id)?.clone();

        let mut grapheme = Vec::new();
        let mut mcp = Vec::new();
        for binding in bindings {
            match binding.source {
                CapabilitySource::Grapheme => grapheme.push(binding),
                CapabilitySource::Mcp => mcp.push(binding),
            }
        }

        let recommended = select_recommended(&grapheme, &mcp);

        Some(CapabilityResolveResponse {
            capability: capability_id.to_string(),
            title: definition.title.clone(),
            description: definition.description.clone(),
            implementations: CapabilityImplementations { grapheme, mcp },
            recommended,
            gateway_unreachable: None,
        })
    }

    pub fn search(&self, query: &str, limit: usize) -> CapabilitySearchResponse {
        let normalized_query = normalize_tokens(query);
        let mut matches = Vec::new();

        for (id, def) in &self.definitions {
            let (score, matched_on) = score_capability_match(id, def, &normalized_query);
            if score > 0 {
                matches.push(CapabilitySearchMatch {
                    capability: id.clone(),
                    title: def.title.clone(),
                    score,
                    matched_on,
                });
            }
        }

        matches.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.capability.cmp(&b.capability)));
        matches.truncate(limit);

        CapabilitySearchResponse {
            query: query.to_string(),
            matches,
        }
    }

    /// Merge MCP catalog availability from gateway sync.
    pub fn apply_mcp_catalog_sync(&mut self, sync: &McpCatalogSyncResponse) {
        let catalog_index = sync
            .entries
            .iter()
            .map(|entry| {
                (
                    format!("{}.{}", entry.server_id, entry.tool_name),
                    entry,
                )
            })
            .collect::<HashMap<_, _>>();

        for bindings in self.bindings.values_mut() {
            for binding in bindings.iter_mut() {
                if binding.source != CapabilitySource::Mcp {
                    continue;
                }
                let Some(entry) = catalog_index.get(&binding.reference) else {
                    binding.available = false;
                    binding.unavailable_reason =
                        Some("tool not present in gateway catalog".to_string());
                    continue;
                };
                binding.available = entry.available;
                binding.unavailable_reason = entry.unavailable_reason.clone();
            }
        }

        for entry in &sync.entries {
            for capability_id in &entry.capability_ids {
                if !self.definitions.contains_key(capability_id) {
                    continue;
                }
                let reference = format!("{}.{}", entry.server_id, entry.tool_name);
                let bindings = self.bindings.entry(capability_id.clone()).or_default();
                if bindings.iter().any(|b| b.reference == reference) {
                    continue;
                }
                let mut binding = CapabilityBinding::mcp(
                    &entry.server_id,
                    &entry.tool_name,
                    100,
                    entry.available,
                    None,
                );
                binding.unavailable_reason = entry.unavailable_reason.clone();
                bindings.push(binding);
                bindings.sort_by_key(|b| b.priority);
            }
        }
    }
}

fn select_recommended(
    grapheme: &[CapabilityBinding],
    mcp: &[CapabilityBinding],
) -> Option<CapabilityRecommendation> {
    let best = grapheme
        .iter()
        .filter(|b| b.available)
        .chain(mcp.iter().filter(|b| b.available))
        .min_by_key(|b| b.priority)?;

    Some(CapabilityRecommendation {
        source: best.source,
        reference: best.reference.clone(),
        reason: "lowest priority number among available bindings".to_string(),
    })
}

fn capability_domain(id: &str) -> String {
    id.split('_')
        .next()
        .filter(|segment| !segment.is_empty())
        .unwrap_or(id)
        .to_uppercase()
}

fn split_module_op(module_op: &str) -> (String, String) {
    match module_op.split_once('.') {
        Some((module, op)) => (module.to_string(), op.to_string()),
        None => (module_op.to_string(), String::new()),
    }
}

fn normalize_tokens(input: &str) -> Vec<String> {
    input
        .to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect()
}

fn score_capability_match(
    id: &str,
    def: &CapabilityDefinition,
    query_tokens: &[String],
) -> (u8, Option<String>) {
    if query_tokens.is_empty() {
        return (0, None);
    }

    let query_joined = query_tokens.join(" ");
    if id.eq_ignore_ascii_case(&query_joined) || id.eq_ignore_ascii_case(query_tokens[0].as_str())
    {
        return (100, Some("id".to_string()));
    }

    for alias in &def.aliases {
        if alias.eq_ignore_ascii_case(&query_joined) {
            return (90, Some(format!("alias:{alias}")));
        }
    }

    let corpus_tokens = collect_definition_tokens(def);
    let overlap = query_tokens
        .iter()
        .filter(|token| corpus_tokens.contains(*token))
        .count();
    if overlap == 0 {
        return (0, None);
    }

    let score = ((overlap as f32 / query_tokens.len() as f32) * 80.0).round() as u8;
    (score.max(1), Some("keywords".to_string()))
}

fn collect_definition_tokens(def: &CapabilityDefinition) -> HashSet<String> {
    let mut tokens = HashSet::new();
    for source in [&def.title]
        .into_iter()
        .chain(def.aliases.iter())
        .chain(def.keywords.iter())
        .chain(def.description.iter())
    {
        for token in normalize_tokens(source) {
            tokens.insert(token);
        }
    }
    tokens
}

pub fn capabilities_manifest_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("medousa")
        .join("capabilities.toml")
}

/// Load embedded seed merged with optional `~/.config/medousa/capabilities.toml`.
/// File entries override same-id capabilities and append new ones.
/// Example overlay: `config/capabilities.toml.example` in the repo (copy to config dir).
pub fn load_capability_manifest() -> (CapabilityManifest, bool) {
    let mut manifest = embedded_capability_manifest();
    let path = capabilities_manifest_path();
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return (manifest, false);
    };

    match toml::from_str::<CapabilityManifest>(&raw) {
        Ok(file_manifest) => {
            merge_capability_manifests(&mut manifest, file_manifest);
            (manifest, true)
        }
        Err(error) => {
            eprintln!(
                "medousa: failed to parse {}: {error}",
                path.display()
            );
            (manifest, false)
        }
    }
}

fn merge_capability_manifests(base: &mut CapabilityManifest, overlay: CapabilityManifest) {
    for entry in overlay.capabilities {
        if let Some(existing) = base.capabilities.iter_mut().find(|item| item.id == entry.id) {
            *existing = entry;
        } else {
            base.capabilities.push(entry);
        }
    }
    if overlay.web_search.preferred_provider.is_some() {
        base.web_search.preferred_provider = overlay.web_search.preferred_provider;
    }
    for disabled in overlay.disabled_bindings {
        if !base.disabled_bindings.iter().any(|entry| entry == &disabled) {
            base.disabled_bindings.push(disabled);
        }
    }
}

/// Resolved web-search operator prefs (capabilities.toml + env override).
pub fn web_search_settings() -> WebSearchSettings {
    let (manifest, _) = load_capability_manifest();
    let mut settings = manifest.web_search;
    if let Ok(from_env) = std::env::var("MEDOUSA_WEB_SEARCH_PROVIDER") {
        let trimmed = from_env.trim();
        if !trimmed.is_empty() {
            settings.preferred_provider = Some(trimmed.to_string());
        }
    }
    let defaults = crate::session::load_tui_defaults();
    if settings.preferred_provider.is_none() {
        settings.preferred_provider = defaults
            .web_search_preferred_provider
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
    }
    if let Some(try_fallbacks) = defaults.web_search_try_fallbacks {
        settings.try_fallbacks = try_fallbacks;
    }
    settings
}

/// Embedded seed manifest — overridden/extended by `~/.config/medousa/capabilities.toml`.
pub fn embedded_capability_manifest() -> CapabilityManifest {
    CapabilityManifest {
        capabilities: vec![
            CapabilityManifestEntry {
                id: "document_search".to_string(),
                title: "Search documents and knowledge bases".to_string(),
                description: Some(
                    "Find pages, files, and docs across connected knowledge stores".to_string(),
                ),
                aliases: vec![
                    "doc search".to_string(),
                    "find documents".to_string(),
                    "wiki search".to_string(),
                ],
                keywords: vec![
                    "document".to_string(),
                    "page".to_string(),
                    "wiki".to_string(),
                    "notion".to_string(),
                    "confluence".to_string(),
                    "drive".to_string(),
                ],
                bindings: CapabilityManifestBindings {
                    grapheme: vec![
                        GraphemeCapabilityBindingSpec {
                            module_op: "websearch.research_materials".to_string(),
                            priority: 10,
                            ..Default::default()
                        },
                        GraphemeCapabilityBindingSpec {
                            module_op: "docs.search".to_string(),
                            priority: 20,
                            ..Default::default()
                        },
                    ],
                    mcp: vec![
                        McpCapabilityBindingSpec {
                            server_id: "notion".to_string(),
                            tool_name: "search_pages".to_string(),
                            priority: 30,
                            effect_class: None,
                            ..Default::default()
                        },
                        McpCapabilityBindingSpec {
                            server_id: "confluence".to_string(),
                            tool_name: "search".to_string(),
                            priority: 40,
                            effect_class: None,
                            ..Default::default()
                        },
                        McpCapabilityBindingSpec {
                            server_id: "google_drive".to_string(),
                            tool_name: "search_docs".to_string(),
                            priority: 50,
                            effect_class: None,
                            ..Default::default()
                        },
                    ],
                },
            },
            CapabilityManifestEntry {
                id: "web_research".to_string(),
                title: "Research the public web".to_string(),
                description: Some(
                    "Provider-native retrieval via web.<provider>; websearch.* for multi-step research pipelines.".to_string(),
                ),
                aliases: vec!["web search".to_string(), "look up online".to_string()],
                keywords: vec![
                    "web".to_string(),
                    "internet".to_string(),
                    "news".to_string(),
                    "articles".to_string(),
                    "provider".to_string(),
                    "tavily".to_string(),
                ],
                bindings: CapabilityManifestBindings {
                    grapheme: vec![
                        GraphemeCapabilityBindingSpec {
                            module_op: "web.providers".to_string(),
                            priority: 5,
                            ..Default::default()
                        },
                        GraphemeCapabilityBindingSpec {
                            module_op: "web.capabilities".to_string(),
                            priority: 8,
                            ..Default::default()
                        },
                        GraphemeCapabilityBindingSpec {
                            module_op: "web.duckduckgo".to_string(),
                            priority: 10,
                            ..Default::default()
                        },
                        GraphemeCapabilityBindingSpec {
                            module_op: "web.google".to_string(),
                            priority: 15,
                            ..Default::default()
                        },
                        GraphemeCapabilityBindingSpec {
                            module_op: "websearch.search".to_string(),
                            priority: 30,
                            ..Default::default()
                        },
                        GraphemeCapabilityBindingSpec {
                            module_op: "websearch.research_materials".to_string(),
                            priority: 35,
                            ..Default::default()
                        },
                        GraphemeCapabilityBindingSpec {
                            module_op: "websearch.research_report".to_string(),
                            priority: 40,
                            ..Default::default()
                        },
                    ],
                    mcp: vec![],
                },
            },
            CapabilityManifestEntry {
                id: "http_fetch".to_string(),
                title: "Fetch a URL or API endpoint".to_string(),
                description: None,
                aliases: vec![],
                keywords: vec![
                    "http".to_string(),
                    "api".to_string(),
                    "rest".to_string(),
                    "url".to_string(),
                ],
                bindings: CapabilityManifestBindings {
                    grapheme: vec![GraphemeCapabilityBindingSpec {
                        module_op: "http.fetch".to_string(),
                        priority: 10,
                        ..Default::default()
                    }],
                    mcp: vec![],
                },
            },
            CapabilityManifestEntry {
                id: "send_email".to_string(),
                title: "Send email".to_string(),
                description: None,
                aliases: vec![],
                keywords: vec![
                    "email".to_string(),
                    "smtp".to_string(),
                    "mail".to_string(),
                ],
                bindings: CapabilityManifestBindings {
                    grapheme: vec![GraphemeCapabilityBindingSpec {
                        module_op: "smtp.send".to_string(),
                        priority: 10,
                        ..Default::default()
                    }],
                    mcp: vec![McpCapabilityBindingSpec {
                        server_id: "gmail".to_string(),
                        tool_name: "send_message".to_string(),
                        priority: 20,
                        effect_class: Some("external_side_effect".to_string()),
                        ..Default::default()
                    }],
                },
            },
        ],
        web_search: WebSearchSettings::default(),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_document_search_groups_grapheme_and_mcp() {
        let registry = CapabilityRegistry::with_embedded_seed();
        let response = registry
            .resolve("document_search")
            .expect("document_search capability");

        assert_eq!(response.capability, "document_search");
        assert_eq!(response.implementations.grapheme.len(), 2);
        assert_eq!(
            response.implementations.grapheme[0].reference,
            "websearch.research_materials"
        );
        assert_eq!(response.implementations.mcp.len(), 3);
        assert!(
            response
                .implementations
                .mcp
                .iter()
                .any(|b| b.reference == "notion.search_pages")
        );
        assert!(response.recommended.is_some());
    }

    #[test]
    fn search_finds_document_search_by_alias() {
        let registry = CapabilityRegistry::with_embedded_seed();
        let results = registry.search("wiki search", 5);
        assert!(!results.matches.is_empty());
        assert_eq!(results.matches[0].capability, "document_search");
    }

    #[test]
    fn file_manifest_merges_with_embedded_seed() {
        let mut manifest = embedded_capability_manifest();
        let file_manifest = CapabilityManifest {
            capabilities: vec![CapabilityManifestEntry {
                id: "custom_capability".to_string(),
                title: "Custom test capability".to_string(),
                description: None,
                aliases: vec![],
                keywords: vec!["custom".to_string()],
                bindings: CapabilityManifestBindings::default(),
            }],
            web_search: WebSearchSettings::default(),
            ..Default::default()
        };
        merge_capability_manifests(&mut manifest, file_manifest);

        assert!(
            manifest
                .capabilities
                .iter()
                .any(|entry| entry.id == "custom_capability")
        );
        assert!(
            manifest
                .capabilities
                .iter()
                .any(|entry| entry.id == "document_search")
        );
    }

    #[test]
    fn mcp_catalog_sync_marks_availability() {
        let mut registry = CapabilityRegistry::with_embedded_seed();
        let sync = McpCatalogSyncResponse {
            entries: vec![McpCatalogSyncEntry {
                server_id: "notion".to_string(),
                tool_name: "search_pages".to_string(),
                title: "Search pages".to_string(),
                capability_ids: vec!["document_search".to_string()],
                available: true,
                unavailable_reason: None,
            }],
            now_utc: chrono::Utc::now(),
        };
        registry.apply_mcp_catalog_sync(&sync);

        let response = registry.resolve("document_search").unwrap();
        let notion = response
            .implementations
            .mcp
            .iter()
            .find(|b| b.reference == "notion.search_pages")
            .expect("notion binding");
        assert!(notion.available);
    }
}
