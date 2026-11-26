//! `medousa mcp` — list the connector catalog and answer MCP elicitation prompts.

use anyhow::{Context, Result, anyhow};
use medousa_mcp_gateway::elicitation::channel_notice;
use medousa_types::mcp_gateway_api::{McpElicitationListResponse, McpElicitationReply};

#[derive(Debug, clap::Args)]
pub struct McpArgs {
    /// Gateway base URL. Defaults to MEDOUSA_MCP_GATEWAY_URL or http://127.0.0.1:7420.
    #[arg(long, global = true)]
    pub gateway_url: Option<String>,
    #[command(subcommand)]
    pub command: McpCommand,
}

#[derive(Debug, clap::Subcommand)]
pub enum McpCommand {
    /// List the built-in connector catalog.
    Catalog,
    /// List server confirmations waiting for a person.
    Elicitations,
    /// Answer one confirmation. Action is accept, decline, or cancel.
    ElicitationRespond {
        id: String,
        #[arg(long)]
        action: String,
        /// JSON object sent when the action is accept.
        #[arg(long)]
        content: Option<String>,
    },
}

pub fn run(args: McpArgs) -> Result<()> {
    match args.command {
        McpCommand::Catalog => print_catalog(),
        McpCommand::Elicitations => print_elicitations(&gateway_url(args.gateway_url.as_deref())),
        McpCommand::ElicitationRespond {
            id,
            action,
            content,
        } => respond(
            &gateway_url(args.gateway_url.as_deref()),
            &id,
            &action,
            content.as_deref(),
        ),
    }
}

fn print_catalog() -> Result<()> {
    let mut entries = medousa_mcp_gateway::builtin_catalog().map_err(anyhow::Error::msg)?;
    entries.sort_by(|left, right| left.manifest.title.cmp(&right.manifest.title));
    for entry in entries {
        let manifest = entry.manifest;
        let desktop = if medousa_mcp_gateway::connector::supports_platform(
            &manifest,
            medousa_mcp_gateway::ConnectorPlatform::Ios,
        ) {
            "desktop and mobile"
        } else {
            "desktop only"
        };
        println!(
            "{}\t{}\t{}\t{}",
            manifest.id, manifest.title, manifest.transport.kind, desktop
        );
    }
    Ok(())
}

fn print_elicitations(base: &str) -> Result<()> {
    let list: McpElicitationListResponse =
        gateway_json(base, reqwest::Method::GET, "/v1/mcp/elicitations", None)?;
    if list.prompts.is_empty() {
        println!("No confirmations are waiting.");
        return Ok(());
    }
    for prompt in &list.prompts {
        println!("{}", channel_notice(prompt));
        println!();
    }
    Ok(())
}

fn respond(base: &str, id: &str, action: &str, content: Option<&str>) -> Result<()> {
    let content = content
        .map(serde_json::from_str)
        .transpose()
        .context("content must be a JSON value")?;
    let reply = McpElicitationReply {
        action: action.to_string(),
        content,
    };
    let body = serde_json::to_vec(&reply)?;
    let response = gateway_send(
        base,
        reqwest::Method::POST,
        &format!("/v1/mcp/elicitations/{id}"),
        Some(body),
    )?;
    let status = response.status();
    if !status.is_success() {
        let text = response.text().unwrap_or_default();
        return Err(anyhow!("gateway returned {status}: {text}"));
    }
    println!("Recorded {action} for {id}.");
    Ok(())
}

fn gateway_url(explicit: Option<&str>) -> String {
    explicit
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| std::env::var("MEDOUSA_MCP_GATEWAY_URL").ok())
        .unwrap_or_else(|| "http://127.0.0.1:7420".to_string())
}

fn gateway_json<T: serde::de::DeserializeOwned>(
    base: &str,
    method: reqwest::Method,
    path: &str,
    body: Option<Vec<u8>>,
) -> Result<T> {
    let response = gateway_send(base, method, path, body)?;
    let status = response.status();
    let text = response.text().context("read gateway response")?;
    if !status.is_success() {
        return Err(anyhow!("gateway returned {status}: {text}"));
    }
    serde_json::from_str(&text).with_context(|| format!("decode gateway response: {text}"))
}

fn gateway_send(
    base: &str,
    method: reqwest::Method,
    path: &str,
    body: Option<Vec<u8>>,
) -> Result<reqwest::blocking::Response> {
    let url = format!("{}{}", base.trim_end_matches('/'), path);
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .context("build gateway client")?;
    let mut request = client.request(method, &url);
    if let Ok(token) = std::env::var("MEDOUSA_MCP_GATEWAY_TOKEN") {
        let token = token.trim();
        if !token.is_empty() {
            request = request.bearer_auth(token);
        }
    }
    if let Some(body) = body {
        request = request
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body);
    }
    request.send().with_context(|| format!("reach {url}"))
}
