//! MCP JSON-RPC client over HTTP (streamable) or legacy SSE transports.

use anyhow::{Context, Result, bail};
use futures_util::StreamExt;
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use reqwest::{Client, Response, Url};
use rmcp::model::{CallToolRequestParams, ClientInfo};
use rmcp::service::RunningService;
use rmcp::transport::{
    StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
};
use rmcp::{RoleClient, ServiceExt};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::time::{Duration, timeout};

use super::server_config::validate_remote_server_url;
use super::stdio_client::{
    McpToolDefinition, medousa_client_info, parse_tool_list, tool_definition_from_sdk,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteTransport {
    Http,
    Sse,
}

impl RemoteTransport {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "http" | "https" | "streamable" | "streamable-http" | "streamable_http" => {
                Some(Self::Http)
            }
            "sse" | "http-sse" | "http_sse" => Some(Self::Sse),
            _ => None,
        }
    }
}

pub struct RemoteMcpSession {
    inner: RemoteMcpSessionInner,
}

enum RemoteMcpSessionInner {
    StreamableHttp {
        client: RunningService<RoleClient, ClientInfo>,
        request_timeout: Duration,
    },
    LegacySse(LegacySseMcpSession),
}

impl RemoteMcpSession {
    pub async fn connect(
        url: &str,
        transport: RemoteTransport,
        bearer_token: Option<String>,
        request_timeout: Duration,
    ) -> Result<Self> {
        let _ =
            validate_remote_server_url(url, bearer_token.is_some()).map_err(anyhow::Error::msg)?;

        let inner = match transport {
            RemoteTransport::Http => {
                let mut config = StreamableHttpClientTransportConfig::with_uri(url.trim())
                    .reinit_on_expired_session(true);
                if let Some(token) = bearer_token {
                    config = config.auth_header(token);
                }
                let transport = StreamableHttpClientTransport::from_config(config);
                let client = timeout(request_timeout, medousa_client_info().serve(transport))
                    .await
                    .context("MCP initialize timed out")?
                    .context("MCP initialize failed")?;
                RemoteMcpSessionInner::StreamableHttp {
                    client,
                    request_timeout,
                }
            }
            RemoteTransport::Sse => RemoteMcpSessionInner::LegacySse(
                LegacySseMcpSession::connect(url, bearer_token, request_timeout).await?,
            ),
        };
        Ok(Self { inner })
    }

    pub async fn list_tools(&mut self) -> Result<Vec<McpToolDefinition>> {
        match &mut self.inner {
            RemoteMcpSessionInner::StreamableHttp {
                client,
                request_timeout,
            } => {
                let tools = timeout(*request_timeout, client.list_all_tools())
                    .await
                    .context("MCP tools/list timed out")?
                    .context("MCP tools/list failed")?;
                tools.into_iter().map(tool_definition_from_sdk).collect()
            }
            RemoteMcpSessionInner::LegacySse(session) => session.list_tools().await,
        }
    }

    pub async fn call_tool(&mut self, name: &str, arguments: Value) -> Result<Value> {
        match &mut self.inner {
            RemoteMcpSessionInner::StreamableHttp {
                client,
                request_timeout,
            } => {
                let arguments = match arguments {
                    Value::Object(arguments) => arguments,
                    Value::Null => Default::default(),
                    _ => bail!("MCP tool arguments must be a JSON object"),
                };
                let result = timeout(
                    *request_timeout,
                    client.call_tool(
                        CallToolRequestParams::new(name.to_string()).with_arguments(arguments),
                    ),
                )
                .await
                .context("MCP tools/call timed out")?
                .context("MCP tools/call failed")?;
                serde_json::to_value(result).context("failed to encode MCP tool result")
            }
            RemoteMcpSessionInner::LegacySse(session) => session.call_tool(name, arguments).await,
        }
    }
}

struct LegacySseMcpSession {
    client: Client,
    post_url: Url,
    bearer_token: Option<String>,
    session_id: Option<String>,
    next_id: u64,
    request_timeout: Duration,
    inbound: mpsc::UnboundedReceiver<Value>,
    _sse_task: tokio::task::JoinHandle<()>,
}

impl LegacySseMcpSession {
    async fn connect(
        url: &str,
        bearer_token: Option<String>,
        request_timeout: Duration,
    ) -> Result<Self> {
        let base_url = Url::parse(url.trim()).context("invalid MCP server URL")?;
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(request_timeout + Duration::from_secs(10))
            .build()
            .context("failed to build HTTP client for MCP server")?;

        let (inbound_tx, inbound_rx) = mpsc::unbounded_channel();
        let post_url = discover_sse_post_url(
            &client,
            &base_url,
            bearer_token.as_deref(),
            request_timeout,
            inbound_tx.clone(),
        )
        .await?;

        let sse_task = {
            let client = client.clone();
            let sse_url = base_url.clone();
            let bearer_token = bearer_token.clone();
            let inbound_tx = inbound_tx.clone();
            tokio::spawn(async move {
                if let Err(error) = pump_legacy_sse(client, sse_url, bearer_token, inbound_tx).await
                {
                    eprintln!("medousa-mcp-gateway: legacy SSE stream ended: {error:#}");
                }
            })
        };

        let mut session = Self {
            client,
            post_url,
            bearer_token,
            session_id: None,
            next_id: 1,
            request_timeout,
            inbound: inbound_rx,
            _sse_task: sse_task,
        };
        session.initialize().await?;
        Ok(session)
    }

    async fn list_tools(&mut self) -> Result<Vec<McpToolDefinition>> {
        let id = self.next_id();
        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/list",
            "params": {}
        });
        let response = self.send_request(id, request).await?;
        parse_tool_list(&response)
    }

    async fn call_tool(&mut self, name: &str, arguments: Value) -> Result<Value> {
        let id = self.next_id();
        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {
                "name": name,
                "arguments": arguments
            }
        });
        let response = self.send_request(id, request).await?;
        if let Some(error) = response.get("error") {
            bail!("MCP tools/call error: {error}");
        }
        Ok(response.get("result").cloned().unwrap_or_else(|| json!({})))
    }

    async fn initialize(&mut self) -> Result<()> {
        let id = self.next_id();
        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "medousa-mcp-gateway",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }
        });
        let _ = self.send_request(id, request).await?;

        let initialized = json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });
        self.post_notification(&initialized).await?;
        Ok(())
    }

    fn next_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    async fn send_request(&mut self, id: u64, request: Value) -> Result<Value> {
        self.post_notification(&request).await?;
        self.wait_for_response(id).await
    }

    async fn post_notification(&mut self, payload: &Value) -> Result<()> {
        let response = self.post_payload(payload).await?;
        if response.status() == reqwest::StatusCode::ACCEPTED {
            return Ok(());
        }
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            bail!("MCP server returned HTTP {status}: {body}");
        }
        let _ = response.text().await;
        Ok(())
    }

    async fn post_payload(&mut self, payload: &Value) -> Result<Response> {
        let response = timeout(
            self.request_timeout,
            self.client
                .post(self.post_url.clone())
                .headers(self.base_headers(true))
                .json(payload)
                .send(),
        )
        .await
        .context("MCP request timed out")?
        .context("MCP POST failed")?;
        if let Some(session_id) = response.headers().get("mcp-session-id")
            && let Ok(value) = session_id.to_str()
        {
            self.session_id = Some(value.to_string());
        }
        Ok(response)
    }

    async fn wait_for_response(&mut self, id: u64) -> Result<Value> {
        timeout(self.request_timeout, self.wait_for_response_inner(id))
            .await
            .context("MCP response timed out")?
    }

    async fn wait_for_response_inner(&mut self, id: u64) -> Result<Value> {
        loop {
            let message = self
                .inbound
                .recv()
                .await
                .context("MCP response channel closed before reply")?;
            if message.get("method").is_some() {
                continue;
            }
            if message.get("id").and_then(Value::as_u64) == Some(id) {
                if let Some(error) = message.get("error") {
                    bail!("MCP error: {error}");
                }
                return Ok(message);
            }
        }
    }

    fn base_headers(&self, include_json_content_type: bool) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/json, text/event-stream"),
        );
        if include_json_content_type {
            headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        }
        if let Some(token) = self.bearer_token.as_deref()
            && let Ok(value) = HeaderValue::from_str(&format!("Bearer {token}"))
        {
            headers.insert(AUTHORIZATION, value);
        }
        if let Some(session_id) = self.session_id.as_deref()
            && let Ok(value) = HeaderValue::from_str(session_id)
        {
            headers.insert("mcp-session-id", value);
        }
        headers
    }
}

async fn discover_sse_post_url(
    client: &Client,
    sse_url: &Url,
    bearer_token: Option<&str>,
    request_timeout: Duration,
    inbound_tx: mpsc::UnboundedSender<Value>,
) -> Result<Url> {
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("text/event-stream"));
    if let Some(token) = bearer_token
        && let Ok(value) = HeaderValue::from_str(&format!("Bearer {token}"))
    {
        headers.insert(AUTHORIZATION, value);
    }

    let response = timeout(
        request_timeout,
        client.get(sse_url.clone()).headers(headers).send(),
    )
    .await
    .context("SSE connect timed out")?
    .context("SSE connect failed")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        bail!("MCP SSE connect returned HTTP {status}: {body}");
    }

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("SSE discovery read failed")?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));
        while let Some((event, rest)) = take_sse_event(&buffer) {
            buffer = rest.to_string();
            if event.event.as_deref() == Some("endpoint") {
                let endpoint = event.data.trim();
                if endpoint.is_empty() {
                    continue;
                }
                return resolve_endpoint_url(sse_url, endpoint);
            }
            if let Some(message) = parse_sse_event_message(&event) {
                let _ = inbound_tx.send(message);
            }
        }
    }

    bail!("MCP SSE stream closed before endpoint event")
}

async fn pump_legacy_sse(
    client: Client,
    sse_url: Url,
    bearer_token: Option<String>,
    inbound_tx: mpsc::UnboundedSender<Value>,
) -> Result<()> {
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("text/event-stream"));
    if let Some(token) = bearer_token.as_deref()
        && let Ok(value) = HeaderValue::from_str(&format!("Bearer {token}"))
    {
        headers.insert(AUTHORIZATION, value);
    }

    let response = client
        .get(sse_url)
        .headers(headers)
        .send()
        .await
        .context("legacy SSE reconnect failed")?;

    if !response.status().is_success() {
        bail!("legacy SSE reconnect returned HTTP {}", response.status());
    }

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("legacy SSE read failed")?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));
        while let Some((event, rest)) = take_sse_event(&buffer) {
            buffer = rest.to_string();
            if event.event.as_deref() == Some("endpoint") {
                continue;
            }
            if let Some(message) = parse_sse_event_message(&event) {
                let _ = inbound_tx.send(message);
            }
        }
    }
    Ok(())
}

fn resolve_endpoint_url(base: &Url, endpoint: &str) -> Result<Url> {
    if let Ok(absolute) = Url::parse(endpoint) {
        return Ok(absolute);
    }
    base.join(endpoint)
        .with_context(|| format!("invalid MCP SSE endpoint '{endpoint}'"))
}

#[derive(Debug, Clone)]
struct SseEvent {
    event: Option<String>,
    data: String,
}

fn take_sse_event(buffer: &str) -> Option<(SseEvent, &str)> {
    let (end, delimiter_len) = match (buffer.find("\n\n"), buffer.find("\r\n\r\n")) {
        (Some(lf), Some(crlf)) if lf <= crlf => (lf, 2),
        (Some(_), Some(crlf)) => (crlf, 4),
        (Some(lf), None) => (lf, 2),
        (None, Some(crlf)) => (crlf, 4),
        (None, None) => return None,
    };
    let normalized = buffer[..end].replace("\r\n", "\n");
    let block = normalized.as_str();
    let rest = &buffer[end + delimiter_len..];

    let mut event = None;
    let mut data_lines = Vec::new();
    for line in block.lines() {
        if let Some(value) = line.strip_prefix("event:") {
            event = Some(value.trim().to_string());
        } else if let Some(value) = line.strip_prefix("data:") {
            data_lines.push(value.trim_start().to_string());
        }
    }

    Some((
        SseEvent {
            event,
            data: data_lines.join("\n"),
        },
        rest,
    ))
}

fn parse_sse_event_message(event: &SseEvent) -> Option<Value> {
    if event.data.is_empty() {
        return None;
    }
    if event.event.as_deref() == Some("endpoint") {
        return None;
    }
    serde_json::from_str(&event.data).ok()
}

#[cfg(test)]
mod tests {
    use axum::Json;
    use axum::http::StatusCode;
    use axum::response::{IntoResponse, Response as AxumResponse};
    use axum::routing::post;
    use serde_json::json;

    use super::*;

    async fn streamable_fixture(Json(request): Json<Value>) -> AxumResponse {
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        match request.get("method").and_then(Value::as_str) {
            Some("initialize") => Json(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": "2025-11-25",
                    "capabilities": { "tools": { "listChanged": true } },
                    "serverInfo": { "name": "medousa-test", "version": "1.0.0" }
                }
            }))
            .into_response(),
            Some("notifications/initialized") => StatusCode::ACCEPTED.into_response(),
            Some("tools/list") => {
                let cursor = request
                    .get("params")
                    .and_then(|params| params.get("cursor"))
                    .and_then(Value::as_str);
                let (tools, next_cursor) = if cursor == Some("page-2") {
                    (
                        json!([{
                            "name": "second",
                            "description": "Second page",
                            "inputSchema": { "type": "object" }
                        }]),
                        Value::Null,
                    )
                } else {
                    (
                        json!([{
                            "name": "first",
                            "title": "First tool",
                            "description": "First page",
                            "inputSchema": { "type": "object" },
                            "annotations": { "readOnlyHint": true }
                        }]),
                        Value::String("page-2".to_string()),
                    )
                };
                Json(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": { "tools": tools, "nextCursor": next_cursor }
                }))
                .into_response()
            }
            Some("tools/call") => Json(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": [{ "type": "text", "text": "fixture complete" }],
                    "structuredContent": { "ok": true },
                    "isError": false
                }
            }))
            .into_response(),
            _ => StatusCode::NOT_FOUND.into_response(),
        }
    }

    #[test]
    fn parses_sse_event_block() {
        let input = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1}\n\nmore";
        let (event, rest) = take_sse_event(input).expect("event");
        assert_eq!(event.event.as_deref(), Some("message"));
        assert_eq!(event.data, "{\"jsonrpc\":\"2.0\",\"id\":1}");
        assert_eq!(rest, "more");
    }

    #[test]
    fn parses_crlf_sse_event_without_corrupting_remainder() {
        let input = "event: message\r\ndata: {\"jsonrpc\":\"2.0\",\"id\":1}\r\n\r\nmore";
        let (event, rest) = take_sse_event(input).expect("event");
        assert_eq!(event.event.as_deref(), Some("message"));
        assert_eq!(event.data, "{\"jsonrpc\":\"2.0\",\"id\":1}");
        assert_eq!(rest, "more");
    }

    #[test]
    fn remote_transport_aliases() {
        assert_eq!(RemoteTransport::parse("http"), Some(RemoteTransport::Http));
        assert_eq!(
            RemoteTransport::parse("streamable-http"),
            Some(RemoteTransport::Http)
        );
        assert_eq!(RemoteTransport::parse("sse"), Some(RemoteTransport::Sse));
        assert!(RemoteTransport::parse("stdio").is_none());
    }

    #[tokio::test]
    async fn official_streamable_transport_negotiates_paginates_and_calls_tools() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind fixture");
        let address = listener.local_addr().expect("fixture address");
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                axum::Router::new().route("/mcp", post(streamable_fixture)),
            )
            .await
        });

        let mut session = RemoteMcpSession::connect(
            &format!("http://{address}/mcp"),
            RemoteTransport::Http,
            None,
            Duration::from_secs(5),
        )
        .await
        .expect("connect fixture");
        let tools = session.list_tools().await.expect("list fixture tools");
        assert_eq!(
            tools
                .iter()
                .map(|tool| tool.name.as_str())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );
        let output = session
            .call_tool("first", json!({ "value": 1 }))
            .await
            .expect("call fixture tool");
        assert_eq!(
            output
                .get("structuredContent")
                .and_then(|value| value.get("ok"))
                .and_then(Value::as_bool),
            Some(true)
        );

        drop(session);
        server.abort();
    }
}
