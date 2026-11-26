//! Connector manifests (`connector.toml`) and the built-in catalog.
//!
//! A connector is the install-time shape: transport, auth, setup fields, tool
//! trims, effect pins, and an optional usage skill. The gateway's
//! `McpServerConfig` remains the runtime shape.

mod oauth_plan;

pub use oauth_plan::{MOBILE_OAUTH_REDIRECT, OAuthBeginPlan};
pub use oauth_plan::{
    SuppliedOAuthClient, plan_oauth_begin, scopes_for_authorization, with_access_type_offline,
};

use std::collections::HashMap;

use serde::Deserialize;

use super::server_config::{
    McpServerConfig, McpServerOAuthConfig, default_allowed_effects, default_allowed_lanes,
    tool_name_matches_exclusion,
};

pub const CONNECTOR_SCHEMA: &str = "medousa.connector/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorPlatform {
    Desktop,
    Ios,
    Android,
}

impl ConnectorPlatform {
    pub fn is_mobile(self) -> bool {
        matches!(self, Self::Ios | Self::Android)
    }

    pub fn current() -> Self {
        #[cfg(target_os = "ios")]
        {
            Self::Ios
        }
        #[cfg(target_os = "android")]
        {
            Self::Android
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            Self::Desktop
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OAuthClientMode {
    Preregistered,
    Cimd,
    Dcr,
    Byo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallProbe {
    /// Remote and local MCP: persist only after `tools/list` succeeds.
    ToolsList,
    /// Channel connectors have no MCP tool list. Required setup fields are the probe.
    SetupOnly,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConnectorManifest {
    pub schema: String,
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub publisher: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub source: Option<String>,
    pub transport: ConnectorTransport,
    #[serde(default)]
    pub auth: ConnectorAuth,
    #[serde(default)]
    pub setup: Vec<SetupField>,
    #[serde(default)]
    pub tools: ConnectorTools,
    #[serde(default)]
    pub skill: Option<ConnectorSkill>,
    #[serde(default)]
    pub suggest: ConnectorSuggest,
    #[serde(default)]
    pub platforms: Option<ConnectorPlatforms>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConnectorTransport {
    pub kind: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub elicitation_fallback_url: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub adapter: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConnectorAuth {
    #[serde(default = "default_auth_kind")]
    pub kind: String,
    #[serde(default)]
    pub clients: Vec<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub client_metadata_url: Option<String>,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub client_ids: HashMap<String, String>,
    #[serde(default)]
    pub offline_access: bool,
    #[serde(default)]
    pub access_type_offline: bool,
    #[serde(default)]
    pub google_agent_client: bool,
}

impl Default for ConnectorAuth {
    fn default() -> Self {
        Self {
            kind: default_auth_kind(),
            clients: Vec::new(),
            scopes: Vec::new(),
            client_metadata_url: None,
            client_id: None,
            client_ids: HashMap::new(),
            offline_access: false,
            access_type_offline: false,
            google_agent_client: false,
        }
    }
}

fn default_auth_kind() -> String {
    "none".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct SetupField {
    pub key: String,
    pub title: String,
    #[serde(default)]
    pub sensitive: bool,
    #[serde(default)]
    pub link: Option<String>,
    /// Missing means required.
    #[serde(default)]
    pub required: Option<bool>,
}

impl SetupField {
    pub fn is_required(&self) -> bool {
        self.required.unwrap_or(true)
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ConnectorTools {
    #[serde(default)]
    pub default_excluded: Vec<String>,
    #[serde(default)]
    pub effects: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConnectorSkill {
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ConnectorSuggest {
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub hosts: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConnectorPlatforms {
    #[serde(default = "default_true")]
    pub desktop: bool,
    #[serde(default = "default_true")]
    pub mobile: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone)]
pub struct CatalogEntry {
    pub manifest: ConnectorManifest,
    pub skill_markdown: Option<String>,
}

#[derive(Clone)]
pub struct InstallInput {
    pub platform: ConnectorPlatform,
    pub setup: HashMap<String, String>,
    pub oauth_client_id: Option<String>,
    pub oauth_client_secret: Option<String>,
    pub oauth_client_metadata_url: Option<String>,
    /// True after the host has finished the browser ceremony for this server id.
    pub oauth_connected: bool,
}

impl std::fmt::Debug for InstallInput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("InstallInput")
            .field("platform", &self.platform)
            .field("setup_keys", &self.setup.keys().collect::<Vec<_>>())
            .field("oauth_client_id", &self.oauth_client_id)
            .field(
                "oauth_client_secret",
                &self.oauth_client_secret.as_ref().map(|_| "<redacted>"),
            )
            .field("oauth_connected", &self.oauth_connected)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct PreparedInstall {
    pub connector_id: String,
    pub server: McpServerConfig,
    pub sensitive: Vec<(String, String)>,
    pub skill_markdown: Option<String>,
    pub needs_oauth: bool,
    pub oauth_plan: Option<OAuthBeginPlan>,
    pub probe: InstallProbe,
}

pub trait InstallStore {
    fn save_server(&mut self, server: &McpServerConfig) -> Result<(), String>;
    fn save_secret(&mut self, server_id: &str, key: &str, value: &str) -> Result<(), String>;
    fn save_skill(&mut self, connector_id: &str, markdown: &str) -> Result<(), String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallReport {
    pub connector_id: String,
    pub server_id: String,
    pub persisted: bool,
    pub tool_names: Vec<String>,
    pub skill_saved: bool,
}

pub fn parse_connector_manifest(raw: &str) -> Result<ConnectorManifest, String> {
    let mut manifest: ConnectorManifest =
        toml::from_str(raw).map_err(|error| format!("connector manifest: {error}"))?;
    if manifest.schema.trim() != CONNECTOR_SCHEMA {
        return Err(format!(
            "unsupported connector schema '{}' (expected {CONNECTOR_SCHEMA})",
            manifest.schema
        ));
    }
    manifest.id = manifest.id.trim().to_ascii_lowercase();
    if !valid_connector_id(&manifest.id) {
        return Err(
            "connector id must be a lowercase slug (letters, digits, hyphens, underscores)"
                .to_string(),
        );
    }
    if manifest.title.trim().is_empty() {
        return Err("connector title is required".to_string());
    }
    manifest.title = manifest.title.trim().to_string();
    let kind = manifest.transport.kind.trim().to_ascii_lowercase();
    if !matches!(kind.as_str(), "remote" | "local" | "channel" | "grapheme") {
        return Err(format!("unsupported transport kind '{kind}'"));
    }
    manifest.transport.kind = kind;
    manifest.auth.kind = manifest.auth.kind.trim().to_ascii_lowercase();
    manifest.auth.client_ids = oauth_plan::platform_client_ids(manifest.auth.client_ids.clone());
    if manifest.platforms.is_none() {
        let local = manifest.transport.kind == "local";
        manifest.platforms = Some(ConnectorPlatforms {
            desktop: true,
            mobile: !local,
        });
    }
    Ok(manifest)
}

pub fn supports_platform(manifest: &ConnectorManifest, platform: ConnectorPlatform) -> bool {
    let platforms = manifest.platforms.as_ref();
    match platform {
        ConnectorPlatform::Desktop => platforms.is_none_or(|value| value.desktop),
        ConnectorPlatform::Ios | ConnectorPlatform::Android => {
            platforms.is_none_or(|value| value.mobile) && manifest.transport.kind != "local"
        }
    }
}

include!(concat!(env!("OUT_DIR"), "/connector_catalog_gen.rs"));

pub fn builtin_catalog() -> Result<Vec<CatalogEntry>, String> {
    EMBEDDED_CONNECTORS
        .iter()
        .map(|entry| {
            let manifest = parse_connector_manifest(entry.manifest)?;
            if manifest.id != entry.id {
                return Err(format!(
                    "connector folder '{}' declares id '{}'",
                    entry.id, manifest.id
                ));
            }
            Ok(CatalogEntry {
                manifest,
                skill_markdown: entry.skill.map(str::to_string),
            })
        })
        .collect()
}

pub fn catalog_entry(id: &str) -> Result<CatalogEntry, String> {
    let id = id.trim();
    builtin_catalog()?
        .into_iter()
        .find(|entry| entry.manifest.id.eq_ignore_ascii_case(id))
        .ok_or_else(|| format!("unknown connector '{id}'"))
}

pub fn prepare_install(
    entry: &CatalogEntry,
    input: &InstallInput,
) -> Result<PreparedInstall, String> {
    let manifest = &entry.manifest;
    if !supports_platform(manifest, input.platform) {
        return Err(match manifest.transport.kind.as_str() {
            "local" => format!(
                "{} runs a local process and is desktop-only. On a phone, use a remote connector.",
                manifest.title
            ),
            _ => format!("{} is not available on this device", manifest.title),
        });
    }

    let mut sensitive = Vec::new();
    for field in &manifest.setup {
        let value = input
            .setup
            .get(&field.key)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty());
        if field.is_required() && value.is_none() {
            return Err(format!("missing setup field '{}'", field.title));
        }
        if field.sensitive
            && let Some(value) = value
        {
            sensitive.push((field.key.clone(), value.to_string()));
        }
    }

    let needs_oauth = manifest.auth.kind == "oauth";
    let oauth_plan = if needs_oauth {
        Some(plan_oauth_begin(
            &manifest.auth,
            input.platform,
            &SuppliedOAuthClient {
                client_id: input.oauth_client_id.clone(),
                client_secret: input.oauth_client_secret.clone(),
                client_metadata_url: input.oauth_client_metadata_url.clone(),
            },
        )?)
    } else {
        None
    };

    let server = server_from_manifest(manifest, input, oauth_plan.as_ref(), &sensitive)?;
    let probe = if manifest.transport.kind == "channel" {
        InstallProbe::SetupOnly
    } else {
        InstallProbe::ToolsList
    };
    Ok(PreparedInstall {
        connector_id: manifest.id.clone(),
        server,
        sensitive,
        skill_markdown: entry.skill_markdown.clone(),
        needs_oauth,
        oauth_plan,
        probe,
    })
}

pub fn commit_install<S: InstallStore>(
    prepared: &PreparedInstall,
    oauth_connected: bool,
    probe: Result<Vec<String>, String>,
    store: &mut S,
) -> Result<InstallReport, String> {
    if prepared.needs_oauth && !oauth_connected {
        return Err("authorize the connector before saving it. Nothing was written.".to_string());
    }
    let tool_names = match prepared.probe {
        InstallProbe::ToolsList => probe.map_err(|error| {
            format!("tools/list failed, so the connector was not saved: {error}")
        })?,
        InstallProbe::SetupOnly => probe.map_err(|error| {
            format!("connector setup was not confirmed, so it was not saved: {error}")
        })?,
    };

    store.save_server(&prepared.server)?;
    for (key, value) in &prepared.sensitive {
        store.save_secret(&prepared.server.id, key, value)?;
    }
    let skill_saved = if let Some(markdown) = prepared.skill_markdown.as_deref() {
        store.save_skill(&prepared.connector_id, markdown)?;
        true
    } else {
        false
    };
    Ok(InstallReport {
        connector_id: prepared.connector_id.clone(),
        server_id: prepared.server.id.clone(),
        persisted: true,
        tool_names,
        skill_saved,
    })
}

pub async fn probe_remote_tool_names(
    url: &str,
    bearer_token: Option<String>,
) -> Result<Vec<String>, String> {
    let mut session = super::remote_client::RemoteMcpSession::connect_with_events(
        url,
        super::remote_client::RemoteTransport::Http,
        bearer_token,
        std::time::Duration::from_secs(20),
        None,
    )
    .await
    .map_err(|error| error.to_string())?;
    let tools = session
        .list_tools()
        .await
        .map_err(|error| error.to_string())?;
    Ok(tools.into_iter().map(|tool| tool.name).collect())
}

/// Probe `tools/list`, then persist. A failed probe leaves the store untouched.
pub async fn install_after_tools_list<S: InstallStore>(
    entry: &CatalogEntry,
    input: &InstallInput,
    bearer_token: Option<String>,
    store: &mut S,
) -> Result<InstallReport, String> {
    let prepared = prepare_install(entry, input)?;
    if prepared.needs_oauth && !input.oauth_connected {
        return Err("authorize the connector before saving it. Nothing was written.".to_string());
    }
    let probe = match prepared.probe {
        InstallProbe::ToolsList => {
            let url = prepared
                .server
                .url
                .clone()
                .ok_or_else(|| "remote connector is missing a URL".to_string())?;
            probe_remote_tool_names(&url, bearer_token).await
        }
        InstallProbe::SetupOnly => Ok(Vec::new()),
    };
    commit_install(&prepared, input.oauth_connected, probe, store)
}

fn server_from_manifest(
    manifest: &ConnectorManifest,
    input: &InstallInput,
    plan: Option<&OAuthBeginPlan>,
    sensitive: &[(String, String)],
) -> Result<McpServerConfig, String> {
    let resolve = |raw: &str| resolve_setup_template(raw, &input.setup);
    let (transport, url, command, args) = match manifest.transport.kind.as_str() {
        "remote" => {
            let url = manifest
                .transport
                .url
                .as_deref()
                .map(resolve)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "remote connector is missing a URL".to_string())?;
            super::server_config::validate_remote_server_url(&url, false)?;
            ("http".to_string(), Some(url), None, Vec::new())
        }
        "local" => {
            let command = manifest
                .transport
                .command
                .as_deref()
                .map(resolve)
                .filter(|value| !value.is_empty())
                .or_else(|| {
                    input
                        .setup
                        .get("command")
                        .map(|value| value.trim().to_string())
                        .filter(|value| !value.is_empty())
                })
                .ok_or_else(|| "local connector is missing a command".to_string())?;
            let args = if manifest.transport.args.is_empty() {
                input
                    .setup
                    .get("args")
                    .map(|value| split_args(value))
                    .unwrap_or_default()
            } else {
                manifest
                    .transport
                    .args
                    .iter()
                    .map(|arg| resolve(arg))
                    .collect()
            };
            ("stdio".to_string(), None, Some(command), args)
        }
        "channel" => {
            let adapter = manifest
                .transport
                .adapter
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "channel connector is missing an adapter".to_string())?
                .to_string();
            ("channel".to_string(), None, Some(adapter), Vec::new())
        }
        "grapheme" => {
            return Err(
                "grapheme connectors are not installed through the MCP gateway yet".to_string(),
            );
        }
        other => return Err(format!("unsupported transport kind '{other}'")),
    };

    let token_auth = matches!(manifest.auth.kind.as_str(), "token" | "api_key");
    let bearer_configured =
        token_auth && manifest.transport.kind == "remote" && !sensitive.is_empty()
            || (manifest.transport.kind == "channel" && !sensitive.is_empty());

    let oauth = plan
        .map(|plan| McpServerOAuthConfig {
            clients: manifest.auth.clients.clone(),
            scopes: plan.scopes.clone(),
            client_metadata_url: plan.client_metadata_url.clone(),
            client_id: plan.client_id.clone(),
            offline_access: manifest.auth.offline_access,
            access_type_offline: plan.access_type_offline,
            google_agent_client: plan.google_agent_client,
        })
        .unwrap_or_default();

    Ok(McpServerConfig {
        id: manifest.id.clone(),
        title: manifest.title.clone(),
        enabled: true,
        transport,
        command,
        args,
        url,
        bearer_token: None,
        bearer_token_configured: bearer_configured,
        allowed_lanes: default_allowed_lanes(),
        allowed_effect_classes: default_allowed_effects(),
        tool_tags: HashMap::new(),
        disabled_tools: manifest.tools.default_excluded.clone(),
        use_mock: false,
        connector_id: Some(manifest.id.clone()),
        oauth,
        tool_effects: manifest.tools.effects.clone(),
    })
}

pub fn resolve_setup_template(raw: &str, setup: &HashMap<String, String>) -> String {
    let mut output = String::new();
    let mut rest = raw;
    while let Some(start) = rest.find("${setup.") {
        output.push_str(&rest[..start]);
        let after = &rest[start + "${setup.".len()..];
        if let Some(end) = after.find('}') {
            let key = &after[..end];
            if let Some(value) = setup.get(key) {
                output.push_str(value.trim());
            } else {
                output.push_str(&rest[start..start + "${setup.".len() + end + 1]);
            }
            rest = &after[end + 1..];
        } else {
            output.push_str(&rest[start..]);
            return output;
        }
    }
    output.push_str(rest);
    output
}

fn split_args(raw: &str) -> Vec<String> {
    raw.split_whitespace()
        .map(str::to_string)
        .filter(|value| !value.is_empty())
        .collect()
}

fn valid_connector_id(id: &str) -> bool {
    let mut chars = id.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_lowercase()
        && chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' || ch == '_')
}

pub fn excluded_tool_names<'a>(
    patterns: &[String],
    tool_names: impl IntoIterator<Item = &'a str>,
) -> Vec<String> {
    tool_names
        .into_iter()
        .filter(|name| {
            patterns
                .iter()
                .any(|pattern| tool_name_matches_exclusion(pattern, name))
        })
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MemoryStore {
        servers: Vec<McpServerConfig>,
        secrets: Vec<(String, String, String)>,
        skills: Vec<(String, String)>,
    }

    impl InstallStore for MemoryStore {
        fn save_server(&mut self, server: &McpServerConfig) -> Result<(), String> {
            self.servers.push(server.clone());
            Ok(())
        }

        fn save_secret(&mut self, server_id: &str, key: &str, value: &str) -> Result<(), String> {
            self.secrets
                .push((server_id.to_string(), key.to_string(), value.to_string()));
            Ok(())
        }

        fn save_skill(&mut self, connector_id: &str, markdown: &str) -> Result<(), String> {
            self.skills
                .push((connector_id.to_string(), markdown.to_string()));
            Ok(())
        }
    }

    fn input(platform: ConnectorPlatform) -> InstallInput {
        InstallInput {
            platform,
            setup: HashMap::new(),
            oauth_client_id: None,
            oauth_client_secret: None,
            oauth_client_metadata_url: None,
            oauth_connected: false,
        }
    }

    #[test]
    fn parses_remote_local_and_channel_manifests() {
        let linear = catalog_entry("linear").expect("linear");
        assert_eq!(linear.manifest.transport.kind, "remote");
        assert_eq!(
            linear.manifest.transport.url.as_deref(),
            Some("https://mcp.linear.app/mcp")
        );
        assert!(
            linear
                .manifest
                .auth
                .clients
                .iter()
                .any(|mode| mode == "dcr")
        );
        assert!(
            linear
                .manifest
                .tools
                .default_excluded
                .iter()
                .any(|pattern| pattern == "delete_*")
        );
        assert_eq!(
            linear
                .manifest
                .tools
                .effects
                .get("create_issue")
                .map(String::as_str),
            Some("external_write")
        );
        assert!(linear.skill_markdown.is_some());

        let local = catalog_entry("desktop-local").expect("local");
        assert_eq!(local.manifest.transport.kind, "local");
        assert!(!supports_platform(&local.manifest, ConnectorPlatform::Ios));
        assert!(supports_platform(
            &local.manifest,
            ConnectorPlatform::Desktop
        ));

        let telegram = catalog_entry("telegram").expect("telegram");
        assert_eq!(telegram.manifest.transport.kind, "channel");
        assert_eq!(
            telegram.manifest.transport.adapter.as_deref(),
            Some("medousa-telegram")
        );
        assert!(
            telegram
                .manifest
                .setup
                .iter()
                .any(|field| field.key == "bot_token" && field.sensitive)
        );
    }

    #[test]
    fn google_entries_prefer_mcp_v1_and_keep_v1self_as_fallback() {
        let gmail = catalog_entry("gmail").expect("gmail");
        assert_eq!(
            gmail.manifest.transport.url.as_deref(),
            Some("https://gmailmcp.googleapis.com/mcp/v1")
        );
        assert_eq!(
            gmail.manifest.transport.elicitation_fallback_url.as_deref(),
            Some("https://gmailmcp.googleapis.com/mcp/v1self")
        );
        assert!(gmail.manifest.auth.google_agent_client);
        assert!(gmail.manifest.auth.access_type_offline);
        assert!(gmail.manifest.auth.client_id.is_none());
        assert!(supports_platform(
            &gmail.manifest,
            ConnectorPlatform::Android
        ));
    }

    #[test]
    fn catalog_ships_no_client_secrets() {
        let catalog = builtin_catalog().expect("catalog");
        let ids = catalog
            .iter()
            .map(|entry| entry.manifest.id.as_str())
            .collect::<Vec<_>>();
        for required in [
            "gmail",
            "google-drive",
            "google-calendar",
            "notion",
            "linear",
            "figma",
            "atlassian",
            "stripe",
            "github",
            "slack",
            "asana",
            "hubspot",
            "context7",
            "home-assistant",
            "telegram",
        ] {
            assert!(ids.contains(&required), "missing {required}");
        }
        for entry in &catalog {
            assert!(
                entry.manifest.auth.client_id.is_none(),
                "{}",
                entry.manifest.id
            );
            let raw = toml::to_string(&serde_json::json!({
                "id": entry.manifest.id,
                "client_id": entry.manifest.auth.client_id,
            }))
            .unwrap_or_default();
            assert!(!raw.to_ascii_lowercase().contains("client_secret"));
            if let Some(skill) = &entry.skill_markdown {
                assert!(!skill.contains("client_secret"));
                assert!(!skill.to_ascii_lowercase().contains("begin private"));
            }
        }
    }

    #[test]
    fn local_connector_is_rejected_on_mobile_before_anything_is_saved() {
        let entry = catalog_entry("desktop-local").unwrap();
        let mut setup_input = input(ConnectorPlatform::Ios);
        setup_input.setup.insert("command".into(), "npx".into());
        let error = prepare_install(&entry, &setup_input).expect_err("mobile local");
        assert!(error.contains("desktop-only"));
    }

    #[test]
    fn failed_tools_list_does_not_persist() {
        let entry = catalog_entry("context7").unwrap();
        let mut install_input = input(ConnectorPlatform::Desktop);
        install_input.oauth_connected = true;
        let prepared = prepare_install(&entry, &install_input).expect("prepare");
        assert_eq!(prepared.probe, InstallProbe::ToolsList);
        let mut store = MemoryStore {
            servers: Vec::new(),
            secrets: Vec::new(),
            skills: Vec::new(),
        };
        let error = commit_install(
            &prepared,
            true,
            Err("connection refused".into()),
            &mut store,
        )
        .expect_err("probe");
        assert!(error.contains("not saved"));
        assert!(store.servers.is_empty());
        assert!(store.secrets.is_empty());
        assert!(store.skills.is_empty());
    }

    #[test]
    fn successful_tools_list_persists_trims_and_keeps_secrets_out_of_config() {
        let entry = catalog_entry("home-assistant").unwrap();
        let mut install_input = input(ConnectorPlatform::Desktop);
        install_input
            .setup
            .insert("mcp_url".into(), "https://home.example.test/mcp".into());
        install_input
            .setup
            .insert("token".into(), "long-lived-token".into());
        let prepared = prepare_install(&entry, &install_input).expect("prepare");
        let mut store = MemoryStore {
            servers: Vec::new(),
            secrets: Vec::new(),
            skills: Vec::new(),
        };
        let report = commit_install(
            &prepared,
            true,
            Ok(vec!["list_devices".into(), "delete_device".into()]),
            &mut store,
        )
        .expect("commit");
        assert!(report.persisted);
        assert!(report.skill_saved);
        let server = &store.servers[0];
        assert_eq!(server.url.as_deref(), Some("https://home.example.test/mcp"));
        assert!(server.bearer_token.is_none());
        assert!(server.bearer_token_configured);
        let encoded = toml::to_string(server).expect("toml");
        assert!(!encoded.contains("long-lived-token"));
        assert_eq!(store.secrets[0].1, "token");
        assert_eq!(store.secrets[0].2, "long-lived-token");
        assert!(!server.tool_enabled("delete_device"));
        assert!(server.tool_enabled("list_devices"));
    }

    #[test]
    fn oauth_install_refuses_to_save_before_authorization() {
        let entry = catalog_entry("linear").unwrap();
        let prepared =
            prepare_install(&entry, &input(ConnectorPlatform::Desktop)).expect("prepare");
        assert!(prepared.needs_oauth);
        assert_eq!(
            prepared.oauth_plan.as_ref().map(|plan| plan.mode),
            Some(OAuthClientMode::Dcr)
        );
        let mut store = MemoryStore {
            servers: Vec::new(),
            secrets: Vec::new(),
            skills: Vec::new(),
        };
        let error = commit_install(&prepared, false, Ok(vec!["list_issues".into()]), &mut store)
            .expect_err("oauth");
        assert!(error.contains("authorize"));
        assert!(store.servers.is_empty());
    }

    #[tokio::test]
    async fn install_persists_only_after_a_live_tools_list() {
        use axum::Json;
        use axum::http::StatusCode;
        use axum::response::IntoResponse;
        use serde_json::json;
        use std::sync::atomic::{AtomicBool, Ordering};

        #[derive(Clone)]
        struct ProbeState {
            fail: std::sync::Arc<AtomicBool>,
        }

        async fn fixture(
            axum::extract::State(state): axum::extract::State<ProbeState>,
            Json(request): Json<serde_json::Value>,
        ) -> axum::response::Response {
            let id = request
                .get("id")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            match request.get("method").and_then(|value| value.as_str()) {
                Some("initialize") => Json(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "protocolVersion": "2025-11-25",
                        "capabilities": { "tools": {} },
                        "serverInfo": { "name": "probe", "version": "1" }
                    }
                }))
                .into_response(),
                Some("notifications/initialized") => StatusCode::ACCEPTED.into_response(),
                Some("tools/list") if state.fail.load(Ordering::SeqCst) => {
                    StatusCode::INTERNAL_SERVER_ERROR.into_response()
                }
                Some("tools/list") => Json(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "tools": [{
                            "name": "search",
                            "description": "Search docs",
                            "inputSchema": { "type": "object" }
                        }]
                    }
                }))
                .into_response(),
                _ => StatusCode::ACCEPTED.into_response(),
            }
        }

        let fail = std::sync::Arc::new(AtomicBool::new(true));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let address = listener.local_addr().unwrap();
        let state = ProbeState { fail: fail.clone() };
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                axum::Router::new()
                    .route("/mcp", axum::routing::post(fixture))
                    .with_state(state),
            )
            .await
        });

        let mut entry = catalog_entry("context7").unwrap();
        entry.manifest.transport.url = Some(format!("http://{address}/mcp"));
        let install_input = input(ConnectorPlatform::Desktop);
        let mut store = MemoryStore {
            servers: Vec::new(),
            secrets: Vec::new(),
            skills: Vec::new(),
        };
        let failed = install_after_tools_list(&entry, &install_input, None, &mut store)
            .await
            .expect_err("failed probe");
        assert!(failed.contains("not saved") || failed.contains("tools/list"));
        assert!(store.servers.is_empty());

        fail.store(false, Ordering::SeqCst);
        let report = install_after_tools_list(&entry, &install_input, None, &mut store)
            .await
            .expect("saved");
        assert!(report.persisted);
        assert_eq!(report.tool_names, ["search"]);
        assert_eq!(store.servers.len(), 1);
        server.abort();
    }
}
