//! One durable, serialized MCP connection per configured server.

use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;

use super::remote_client::{RemoteMcpSession, RemoteTransport};
use super::server_config::McpServerConfig;
use super::stdio_client::{McpToolDefinition, StdioMcpSession};

const INITIAL_RECONNECT_DELAY: Duration = Duration::from_millis(250);
const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(4);

#[derive(Clone)]
pub(crate) struct ConnectionActorHandle {
    commands: mpsc::Sender<ConnectionCommand>,
}

enum ConnectionCommand {
    ListTools {
        bearer_token: Option<String>,
        reply: oneshot::Sender<Result<Vec<McpToolDefinition>>>,
    },
    CallTool {
        bearer_token: Option<String>,
        name: String,
        arguments: Value,
        reply: oneshot::Sender<Result<Value>>,
    },
    Invalidate {
        reply: oneshot::Sender<()>,
    },
}

enum LiveSession {
    Remote(RemoteMcpSession),
    Stdio(StdioMcpSession),
}

impl LiveSession {
    async fn list_tools(&mut self) -> Result<Vec<McpToolDefinition>> {
        match self {
            Self::Remote(session) => session.list_tools().await,
            Self::Stdio(session) => session.list_tools().await,
        }
    }

    async fn call_tool(&mut self, name: &str, arguments: Value) -> Result<Value> {
        match self {
            Self::Remote(session) => session.call_tool(name, arguments).await,
            Self::Stdio(session) => session.call_tool(name, arguments).await,
        }
    }

    async fn close(&mut self) {
        match self {
            Self::Remote(session) => session.close().await,
            Self::Stdio(session) => session.close().await,
        }
    }
}

struct ConnectionActor {
    server: McpServerConfig,
    request_timeout: Duration,
    session: Option<LiveSession>,
    bearer_token: Option<String>,
    consecutive_failures: u32,
    reconnect_not_before: Option<Instant>,
    local_tool_changes: mpsc::UnboundedSender<()>,
}

impl ConnectionActorHandle {
    pub(crate) fn spawn(
        server: McpServerConfig,
        request_timeout: Duration,
        catalog_changes: mpsc::UnboundedSender<String>,
    ) -> Self {
        let (commands, receiver) = mpsc::channel(32);
        let (local_tool_changes, tool_changes) = mpsc::unbounded_channel();
        let actor = ConnectionActor {
            server,
            request_timeout,
            session: None,
            bearer_token: None,
            consecutive_failures: 0,
            reconnect_not_before: None,
            local_tool_changes,
        };
        tokio::spawn(actor.run(receiver, tool_changes, catalog_changes));
        Self { commands }
    }

    pub(crate) async fn list_tools(
        &self,
        bearer_token: Option<String>,
    ) -> Result<Vec<McpToolDefinition>> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(ConnectionCommand::ListTools {
                bearer_token,
                reply,
            })
            .await
            .map_err(|_| anyhow!("MCP connection actor stopped"))?;
        response
            .await
            .map_err(|_| anyhow!("MCP connection actor dropped its reply"))?
    }

    pub(crate) async fn call_tool(
        &self,
        bearer_token: Option<String>,
        name: String,
        arguments: Value,
    ) -> Result<Value> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(ConnectionCommand::CallTool {
                bearer_token,
                name,
                arguments,
                reply,
            })
            .await
            .map_err(|_| anyhow!("MCP connection actor stopped"))?;
        response
            .await
            .map_err(|_| anyhow!("MCP connection actor dropped its reply"))?
    }

    pub(crate) async fn invalidate(&self) {
        let (reply, response) = oneshot::channel();
        if self
            .commands
            .send(ConnectionCommand::Invalidate { reply })
            .await
            .is_ok()
        {
            let _ = response.await;
        }
    }
}

impl ConnectionActor {
    async fn run(
        mut self,
        mut commands: mpsc::Receiver<ConnectionCommand>,
        mut tool_changes: mpsc::UnboundedReceiver<()>,
        catalog_changes: mpsc::UnboundedSender<String>,
    ) {
        loop {
            tokio::select! {
                command = commands.recv() => {
                    let Some(command) = command else { break };
                    match command {
                        ConnectionCommand::ListTools { bearer_token, reply } => {
                            let _ = reply.send(self.list_tools(bearer_token).await);
                        }
                        ConnectionCommand::CallTool { bearer_token, name, arguments, reply } => {
                            let _ = reply.send(self.call_tool(bearer_token, &name, arguments).await);
                        }
                        ConnectionCommand::Invalidate { reply } => {
                            self.invalidate().await;
                            let _ = reply.send(());
                        }
                    }
                }
                change = tool_changes.recv() => {
                    if change.is_some() {
                        let _ = catalog_changes.send(self.server.id.clone());
                    }
                }
            }
        }
        self.invalidate().await;
    }

    async fn list_tools(&mut self, bearer_token: Option<String>) -> Result<Vec<McpToolDefinition>> {
        self.prepare_credentials(bearer_token).await;
        let mut last_error = None;
        for _ in 0..2 {
            if let Err(error) = self.ensure_session().await {
                last_error = Some(error);
            } else {
                let result = self
                    .session
                    .as_mut()
                    .expect("session established")
                    .list_tools()
                    .await;
                match result {
                    Ok(tools) => {
                        self.record_success();
                        return Ok(tools);
                    }
                    Err(error) => last_error = Some(error),
                }
            }
            self.record_failure().await;
        }
        Err(last_error.unwrap_or_else(|| anyhow!("MCP tool discovery failed")))
    }

    async fn call_tool(
        &mut self,
        bearer_token: Option<String>,
        name: &str,
        arguments: Value,
    ) -> Result<Value> {
        self.prepare_credentials(bearer_token).await;
        if let Err(error) = self.ensure_session().await {
            self.record_failure().await;
            return Err(error);
        }
        match self
            .session
            .as_mut()
            .expect("session established")
            .call_tool(name, arguments)
            .await
        {
            Ok(output) => {
                self.record_success();
                Ok(output)
            }
            Err(error) => {
                // A failed mutating request may already have reached the server.
                // Reconnect for the next command, but never replay this call here.
                self.record_failure().await;
                Err(error)
            }
        }
    }

    async fn prepare_credentials(&mut self, bearer_token: Option<String>) {
        if self.bearer_token != bearer_token {
            self.invalidate().await;
            self.bearer_token = bearer_token;
        }
    }

    async fn ensure_session(&mut self) -> Result<()> {
        if self.session.is_some() {
            return Ok(());
        }
        if let Some(not_before) = self.reconnect_not_before.take() {
            tokio::time::sleep_until(not_before).await;
        }
        let session = self.connect().await?;
        self.session = Some(session);
        Ok(())
    }

    async fn connect(&self) -> Result<LiveSession> {
        if let Some(transport) = RemoteTransport::parse(&self.server.transport) {
            let url = self
                .server
                .url
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .context("remote MCP server missing URL")?;
            let session = RemoteMcpSession::connect_with_events(
                url,
                transport,
                self.bearer_token.clone(),
                self.request_timeout,
                Some(self.local_tool_changes.clone()),
            )
            .await?;
            return Ok(LiveSession::Remote(session));
        }

        let command = self
            .server
            .command
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .context("stdio MCP server missing command")?;
        if self.server.transport.trim() != "stdio" {
            bail!("unsupported MCP transport '{}'", self.server.transport);
        }
        let session = StdioMcpSession::spawn_with_events(
            command,
            &self.server.args,
            self.request_timeout,
            Some(self.local_tool_changes.clone()),
        )
        .await?;
        Ok(LiveSession::Stdio(session))
    }

    async fn record_failure(&mut self) {
        if let Some(session) = self.session.as_mut() {
            session.close().await;
        }
        self.session = None;
        self.consecutive_failures = self.consecutive_failures.saturating_add(1).min(8);
        let multiplier = 1_u32 << self.consecutive_failures.saturating_sub(1);
        let delay = INITIAL_RECONNECT_DELAY
            .saturating_mul(multiplier)
            .min(MAX_RECONNECT_DELAY);
        self.reconnect_not_before = Some(Instant::now() + delay);
    }

    fn record_success(&mut self) {
        self.consecutive_failures = 0;
        self.reconnect_not_before = None;
    }

    async fn invalidate(&mut self) {
        if let Some(session) = self.session.as_mut() {
            session.close().await;
        }
        self.session = None;
        self.bearer_token = None;
        self.consecutive_failures = 0;
        self.reconnect_not_before = None;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use axum::Json;
    use axum::extract::State;
    use axum::http::StatusCode;
    use axum::response::{IntoResponse, Response};
    use axum::routing::post;
    use serde_json::json;

    use super::*;

    #[derive(Default)]
    struct FixtureState {
        initializes: AtomicUsize,
        calls: AtomicUsize,
        fail_calls: AtomicBool,
    }

    async fn fixture(
        State(state): State<Arc<FixtureState>>,
        Json(request): Json<Value>,
    ) -> Response {
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        match request.get("method").and_then(Value::as_str) {
            Some("initialize") => {
                state.initializes.fetch_add(1, Ordering::SeqCst);
                Json(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "protocolVersion": "2025-11-25",
                        "capabilities": { "tools": { "listChanged": true } },
                        "serverInfo": { "name": "actor-fixture", "version": "1.0.0" }
                    }
                }))
                .into_response()
            }
            Some("notifications/initialized") => StatusCode::ACCEPTED.into_response(),
            Some("tools/list") => Json(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "tools": [{
                        "name": "remember",
                        "description": "Uses server session state",
                        "inputSchema": { "type": "object" }
                    }]
                }
            }))
            .into_response(),
            Some("tools/call") => {
                state.calls.fetch_add(1, Ordering::SeqCst);
                if state.fail_calls.load(Ordering::SeqCst) {
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                }
                Json(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": { "content": [{ "type": "text", "text": "remembered" }] }
                }))
                .into_response()
            }
            _ => StatusCode::ACCEPTED.into_response(),
        }
    }

    fn fixture_config(address: std::net::SocketAddr) -> McpServerConfig {
        McpServerConfig {
            id: "stateful".into(),
            title: "Stateful fixture".into(),
            enabled: true,
            transport: "http".into(),
            command: None,
            args: Vec::new(),
            url: Some(format!("http://{address}/mcp")),
            bearer_token: None,
            bearer_token_configured: false,
            allowed_lanes: vec!["interactive".into()],
            allowed_effect_classes: vec!["external_read".into()],
            tool_tags: Default::default(),
            disabled_tools: Vec::new(),
            use_mock: false,
        }
    }

    #[tokio::test]
    async fn reuses_session_and_invalidates_when_credentials_change() {
        let state = Arc::new(FixtureState::default());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind fixture");
        let address = listener.local_addr().expect("fixture address");
        let app = axum::Router::new()
            .route("/mcp", post(fixture))
            .with_state(state.clone());
        let server_task = tokio::spawn(async move { axum::serve(listener, app).await });
        let (changes, _change_rx) = mpsc::unbounded_channel();
        let actor =
            ConnectionActorHandle::spawn(fixture_config(address), Duration::from_secs(3), changes);

        actor.list_tools(None).await.expect("first list");
        actor.list_tools(None).await.expect("second list");
        actor
            .call_tool(None, "remember".into(), json!({}))
            .await
            .expect("stateful call");
        assert_eq!(state.initializes.load(Ordering::SeqCst), 1);
        assert_eq!(state.calls.load(Ordering::SeqCst), 1);

        actor
            .list_tools(Some("replacement-token".into()))
            .await
            .expect("list after credential replacement");
        assert_eq!(state.initializes.load(Ordering::SeqCst), 2);

        actor.invalidate().await;
        drop(actor);
        server_task.abort();
    }

    #[tokio::test]
    async fn never_replays_an_ambiguous_failed_tool_call() {
        let state = Arc::new(FixtureState {
            fail_calls: AtomicBool::new(true),
            ..Default::default()
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind fixture");
        let address = listener.local_addr().expect("fixture address");
        let app = axum::Router::new()
            .route("/mcp", post(fixture))
            .with_state(state.clone());
        let server_task = tokio::spawn(async move { axum::serve(listener, app).await });
        let (changes, _change_rx) = mpsc::unbounded_channel();
        let actor =
            ConnectionActorHandle::spawn(fixture_config(address), Duration::from_secs(3), changes);

        let error = actor
            .call_tool(None, "remember".into(), json!({}))
            .await
            .expect_err("fixture call should fail");
        assert!(!error.to_string().is_empty());
        assert_eq!(state.calls.load(Ordering::SeqCst), 1);

        drop(actor);
        server_task.abort();
    }
}
