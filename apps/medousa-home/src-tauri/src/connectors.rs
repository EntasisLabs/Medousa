//! Connector gallery commands. The catalog lives in the gateway crate; Home
//! draws it and runs the browser ceremony. A server file is written only after
//! the first `tools/list` succeeds.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use medousa_mcp_gateway::{
    catalog_entry, commit_install, prepare_install, probe_remote_tool_names, CatalogEntry,
    ConnectorPlatform, InstallInput, InstallProbe, InstallReport, InstallStore, McpServerConfig,
    SecureMcpOAuthBundleStore,
};
use serde::{Deserialize, Serialize};

use crate::mcp_gateway::{run_oauth_browser, OAuthBrowserPlan};
use crate::paths::medousa_data_dir;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorCardDto {
    pub id: String,
    pub title: String,
    pub publisher: String,
    pub categories: Vec<String>,
    pub transport: String,
    pub auth_kind: String,
    pub desktop_only: bool,
    pub needs_client_id: bool,
    pub google_agent_client: bool,
    pub elicitation_fallback_url: Option<String>,
    pub setup: Vec<ConnectorSetupFieldDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorSetupFieldDto {
    pub key: String,
    pub title: String,
    pub sensitive: bool,
    pub required: bool,
    pub link: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorInstallRequest {
    pub id: String,
    #[serde(default)]
    pub setup: HashMap<String, String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorInstallResult {
    pub ok: bool,
    pub connector_id: String,
    pub server_id: String,
    pub tool_names: Vec<String>,
    pub skill_imported: bool,
    pub message: String,
}

#[tauri::command]
pub fn mcp_connector_catalog() -> Result<Vec<ConnectorCardDto>, String> {
    let platform = ConnectorPlatform::current();
    let mut cards = medousa_mcp_gateway::builtin_catalog()?
        .into_iter()
        .map(|entry| card_for(&entry, platform))
        .collect::<Vec<_>>();
    cards.sort_by(|left, right| {
        left.title
            .to_ascii_lowercase()
            .cmp(&right.title.to_ascii_lowercase())
    });
    Ok(cards)
}

fn card_for(entry: &CatalogEntry, platform: ConnectorPlatform) -> ConnectorCardDto {
    let manifest = &entry.manifest;
    let needs_client_id = manifest.auth.kind == "oauth"
        && medousa_mcp_gateway::plan_oauth_begin(
            &manifest.auth,
            platform,
            &medousa_mcp_gateway::SuppliedOAuthClient::default(),
        )
        .is_err();
    ConnectorCardDto {
        id: manifest.id.clone(),
        title: manifest.title.clone(),
        publisher: manifest.publisher.clone(),
        categories: manifest.categories.clone(),
        transport: manifest.transport.kind.clone(),
        auth_kind: manifest.auth.kind.clone(),
        desktop_only: !medousa_mcp_gateway::connector::supports_platform(
            manifest,
            ConnectorPlatform::Ios,
        ),
        needs_client_id,
        google_agent_client: manifest.auth.google_agent_client,
        elicitation_fallback_url: manifest.transport.elicitation_fallback_url.clone(),
        setup: manifest
            .setup
            .iter()
            .map(|field| ConnectorSetupFieldDto {
                key: field.key.clone(),
                title: field.title.clone(),
                sensitive: field.sensitive,
                required: field.is_required(),
                link: field.link.clone(),
            })
            .collect(),
    }
}

#[tauri::command]
pub async fn mcp_connector_install(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::daemon::DaemonState>,
    embedded_state: tauri::State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
    request: ConnectorInstallRequest,
) -> Result<ConnectorInstallResult, String> {
    let entry = catalog_entry(&request.id)?;
    let platform = ConnectorPlatform::current();
    let input = InstallInput {
        platform,
        setup: request.setup.clone(),
        oauth_client_id: request.client_id.clone(),
        oauth_client_secret: request.client_secret.clone(),
        oauth_client_metadata_url: None,
        oauth_connected: false,
    };
    let prepared = prepare_install(&entry, &input)?;
    let mut oauth_connected = !prepared.needs_oauth;
    if prepared.needs_oauth {
        let plan = prepared
            .oauth_plan
            .clone()
            .ok_or_else(|| "connector is missing an OAuth plan".to_string())?;
        run_oauth_browser(
            app,
            state.clone(),
            embedded_state.clone(),
            OAuthBrowserPlan {
                server_id: prepared.server.id.clone(),
                server_url: prepared.server.url.clone(),
                scopes: plan.scopes,
                client_id: plan.client_id,
                client_secret: plan.client_secret,
                client_metadata_url: plan.client_metadata_url,
                access_type_offline: plan.access_type_offline,
            },
        )
        .await?;
        oauth_connected = true;
    }

    let probe = match prepared.probe {
        InstallProbe::SetupOnly => Ok(Vec::new()),
        InstallProbe::ToolsList => {
            let url = prepared
                .server
                .url
                .clone()
                .ok_or_else(|| "connector is missing a URL to probe".to_string())?;
            let listed = if prepared.needs_oauth {
                probe_oauth_tools(&state, &embedded_state, &prepared.server.id, &url).await
            } else {
                let bearer = prepared.sensitive.iter().find_map(|(key, value)| {
                    (key == "token" || key == "api_key").then_some(value.clone())
                });
                probe_remote_tool_names(&url, bearer).await
            };
            if listed.is_err() && prepared.needs_oauth {
                let _ = disconnect_oauth(&state, &embedded_state, &prepared.server.id).await;
            }
            listed
        }
    };

    let credentials = SecureMcpOAuthBundleStore::new(medousa_data_dir())
        .map_err(|error| format!("initialize MCP credential storage: {error}"))?;
    let mut store = HomeInstallStore { credentials };
    let report = match commit_install(&prepared, oauth_connected, probe, &mut store) {
        Ok(report) => report,
        Err(error) => {
            if prepared.needs_oauth {
                let _ = disconnect_oauth(&state, &embedded_state, &prepared.server.id).await;
            }
            return Err(error);
        }
    };
    if let Some(secret) = request
        .client_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        store.credentials.save_setup_secret(
            &report.server_id,
            "oauth_client_secret",
            Some(secret),
        )?;
    }
    let skill_imported = import_staged_skill(&state, &report).await;
    let _ = reload_gateway(&embedded_state).await;
    Ok(ConnectorInstallResult {
        ok: true,
        connector_id: report.connector_id,
        server_id: report.server_id,
        tool_names: report.tool_names,
        skill_imported,
        message: match prepared.probe {
            InstallProbe::ToolsList => {
                #[cfg(any(target_os = "ios", target_os = "android"))]
                {
                    "Connector saved after tools/list succeeded".to_string()
                }
                #[cfg(not(any(target_os = "ios", target_os = "android")))]
                {
                    "Connector saved after tools/list succeeded. Restart the MCP gateway if it does not appear yet.".to_string()
                }
            }
            InstallProbe::SetupOnly => {
                "Channel connector saved. Launching its adapter from this record comes later."
                    .to_string()
            }
        },
    })
}

struct HomeInstallStore {
    credentials: SecureMcpOAuthBundleStore,
}

impl InstallStore for HomeInstallStore {
    fn save_server(&mut self, server: &McpServerConfig) -> Result<(), String> {
        let path = medousa_mcp_gateway::install_starter_gateway_config_if_missing()
            .map_err(|error| error.to_string())?;
        let raw = fs::read_to_string(&path)
            .unwrap_or_else(|_| "[gateway]\nbind = \"127.0.0.1:7420\"\n\n".to_string());
        let mut config = toml::from_str::<medousa_mcp_gateway::McpGatewayFileConfig>(&raw)
            .map_err(|error| format!("read MCP config: {error}"))?;
        if let Some(existing) = config
            .servers
            .iter_mut()
            .find(|entry| entry.id.eq_ignore_ascii_case(&server.id))
        {
            *existing = server.clone();
        } else {
            config.servers.push(server.clone());
        }
        let encoded = toml::to_string_pretty(&config).map_err(|error| error.to_string())?;
        fs::write(&path, encoded).map_err(|error| error.to_string())?;
        Ok(())
    }

    fn save_secret(&mut self, server_id: &str, key: &str, value: &str) -> Result<(), String> {
        self.credentials
            .save_setup_secret(server_id, key, Some(value))?;
        if key == "token" || key == "api_key" {
            self.credentials.save_bearer_token(server_id, Some(value))?;
        }
        Ok(())
    }

    fn save_skill(&mut self, connector_id: &str, markdown: &str) -> Result<(), String> {
        let dir = skill_dir(connector_id)?;
        fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
        fs::write(dir.join("SKILL.md"), markdown).map_err(|error| error.to_string())?;
        Ok(())
    }
}

fn skill_dir(connector_id: &str) -> Result<PathBuf, String> {
    Ok(medousa_data_dir()
        .join("connector-skills")
        .join(connector_id))
}

async fn probe_oauth_tools(
    state: &tauri::State<'_, crate::daemon::DaemonState>,
    embedded_state: &tauri::State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
    server_id: &str,
    url: &str,
) -> Result<Vec<String>, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = embedded_state.client_if_active().await? {
        return client
            .probe_mcp_tools(server_id, url)
            .await
            .map(|response| response.tool_names)
            .map_err(|error| format!("list tools: {error:#}"));
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let _ = embedded_state;
    let response: medousa_types::McpProbeResponse = crate::daemon::workshop_http::post_json(
        state,
        "/v1/mcp/probe",
        &medousa_types::McpProbeRequest {
            server_id: server_id.to_string(),
            url: url.to_string(),
        },
    )
    .await?;
    Ok(response.tool_names)
}

async fn disconnect_oauth(
    state: &tauri::State<'_, crate::daemon::DaemonState>,
    embedded_state: &tauri::State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
    server_id: &str,
) -> Result<(), String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = embedded_state.client_if_active().await? {
        client
            .disconnect_mcp_oauth(server_id)
            .await
            .map_err(|error| error.to_string())?;
        return Ok(());
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let _ = embedded_state;
    let _: medousa_types::DisconnectMcpOAuthResponse =
        crate::daemon::workshop_http::delete_json(state, &format!("/v1/mcp/oauth/{server_id}"))
            .await?;
    Ok(())
}

async fn import_staged_skill(
    state: &tauri::State<'_, crate::daemon::DaemonState>,
    report: &InstallReport,
) -> bool {
    let path = match skill_dir(&report.connector_id) {
        Ok(path) if path.join("SKILL.md").is_file() => path,
        _ => return false,
    };
    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        let _ = state;
        return medousa::skill_import::import_skill(
            &path,
            medousa::identity_manuscript::ManuscriptScope::User,
            true,
            None,
        )
        .is_ok();
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        let request = medousa_types::daemon_api::ManuscriptImportRequest {
            path: Some(path.display().to_string()),
            preset: None,
            scope: Some("user".into()),
            force: Some(true),
        };
        let imported: Result<medousa_types::daemon_api::ManuscriptImportResponse, String> =
            crate::daemon::workshop_http::post_json(state, "/v1/manuscripts", &request).await;
        imported.is_ok()
    }
}

async fn reload_gateway(
    embedded_state: &tauri::State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
) -> Result<(), String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = embedded_state.client_if_active().await? {
        crate::mcp_gateway::reload_embedded_mcp(&client).await?;
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let _ = embedded_state;
    Ok(())
}

#[tauri::command]
pub async fn mcp_elicitation_pending(
    state: tauri::State<'_, crate::daemon::DaemonState>,
    embedded_state: tauri::State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
) -> Result<medousa_types::McpElicitationListResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = embedded_state.client_if_active().await? {
        return client
            .list_mcp_elicitations()
            .await
            .map_err(|error| format!("read confirmations: {error:#}"));
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let _ = embedded_state;
    crate::daemon::workshop_http::get_json(&state, "/v1/mcp/elicitations").await
}

#[tauri::command]
pub async fn mcp_elicitation_respond(
    state: tauri::State<'_, crate::daemon::DaemonState>,
    embedded_state: tauri::State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
    id: String,
    action: String,
    content: Option<serde_json::Value>,
) -> Result<(), String> {
    let reply = medousa_types::McpElicitationReply { action, content };
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = embedded_state.client_if_active().await? {
        return client
            .respond_mcp_elicitation(&id, reply)
            .await
            .map_err(|error| format!("send confirmation: {error:#}"));
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let _ = embedded_state;
    let _: serde_json::Value = crate::daemon::workshop_http::post_json(
        &state,
        &format!("/v1/mcp/elicitations/{id}"),
        &reply,
    )
    .await?;
    Ok(())
}
