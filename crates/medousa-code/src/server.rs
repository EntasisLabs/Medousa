//! HTTP/WS surface for the Orchestrator.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tower_http::cors::CorsLayer;

use crate::detamu::{DetamuDocumentSnapshot, DetamuServerHandle};
use crate::document::DocumentStore;
use crate::registry::{LanguageId, ServerRegistry};
use crate::session::SessionPool;
use crate::{ENGINE_NAME, ENGINE_VERSION};

#[derive(Clone)]
pub struct OrchestratorConfig {
    pub bind: SocketAddr,
    pub workspace_root: PathBuf,
    /// Allowed path prefixes (scripts, forge worktrees, …). Empty = allow workspace_root only.
    pub allowed_roots: Vec<PathBuf>,
}

#[derive(Clone)]
pub struct OrchestratorState {
    pub config: OrchestratorConfig,
    pub pool: Arc<SessionPool>,
    pub documents: Arc<DocumentStore>,
    pub started: Instant,
}

impl OrchestratorState {
    pub fn new(config: OrchestratorConfig, registry: ServerRegistry) -> Self {
        Self {
            config: config.clone(),
            pool: SessionPool::new(registry),
            documents: DocumentStore::new(),
            started: Instant::now(),
        }
    }

    pub fn path_allowed(&self, path: &std::path::Path) -> bool {
        let roots = if self.config.allowed_roots.is_empty() {
            vec![self.config.workspace_root.clone()]
        } else {
            self.config.allowed_roots.clone()
        };
        roots.iter().any(|root| path.starts_with(root))
    }

    pub fn root_uri(&self) -> String {
        path_to_file_uri(&self.config.workspace_root)
    }

    /// Detamu bridge: list open documents (no keystroke coupling).
    pub async fn detamu_document_snapshot(&self) -> Vec<DetamuDocumentSnapshot> {
        let mut out = Vec::new();
        for uri in self.documents.list_uris().await {
            if let Some(doc) = self.documents.get(&uri).await {
                out.push(DetamuDocumentSnapshot {
                    uri: doc.uri,
                    language_id: doc.language_id,
                    version: doc.version,
                });
            }
        }
        out
    }

    pub async fn detamu_session_handles(&self) -> Vec<DetamuServerHandle> {
        self.pool
            .list_keys()
            .await
            .into_iter()
            .map(|key| DetamuServerHandle {
                workspace_root: key.workspace_root.display().to_string(),
                language: key.language.to_string(),
                session_key: format!("{}::{}", key.workspace_root.display(), key.language),
            })
            .collect()
    }
}

fn path_to_file_uri(path: &std::path::Path) -> String {
    let s = path.to_string_lossy();
    if s.starts_with('/') {
        format!("file://{s}")
    } else {
        format!("file:///{s}")
    }
}

#[derive(Serialize)]
pub struct HealthResponse {
    pub name: String,
    pub version: String,
    pub uptime_secs: u64,
    pub active_sessions: usize,
    pub workspace_root: String,
    pub languages: Vec<String>,
    pub allowed_roots: Vec<String>,
}

#[derive(Deserialize)]
pub struct LspQuery {
    /// Language id for this client connection (default grapheme).
    #[serde(default = "default_language")]
    pub language: String,
}

fn default_language() -> String {
    "grapheme".into()
}

pub fn app(state: OrchestratorState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/lsp", get(lsp_ws))
        .route("/v1/code/hover", get(agent_hover))
        .route("/v1/code/definition", get(agent_definition))
        .route("/v1/code/diagnostics", get(agent_diagnostics))
        .route("/v1/code/symbols", get(agent_symbols))
        .route("/v1/detamu/snapshot", get(detamu_snapshot))
        .route("/v1/detamu/handles", get(detamu_handles))
        .layer(CorsLayer::permissive())
        .with_state(Arc::new(state))
}

pub async fn serve(state: OrchestratorState) -> anyhow::Result<()> {
    let addr = state.config.bind;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "medousa-code orchestrator listening");
    axum::serve(listener, app(state)).await?;
    Ok(())
}

async fn health(State(state): State<Arc<OrchestratorState>>) -> Json<HealthResponse> {
    Json(HealthResponse {
        name: ENGINE_NAME.into(),
        version: ENGINE_VERSION.into(),
        uptime_secs: state.started.elapsed().as_secs(),
        active_sessions: state.pool.active_count().await,
        workspace_root: state.config.workspace_root.display().to_string(),
        languages: state
            .pool
            .registry()
            .languages()
            .map(|l| l.to_string())
            .collect(),
        allowed_roots: state
            .config
            .allowed_roots
            .iter()
            .map(|p| p.display().to_string())
            .collect(),
    })
}

async fn lsp_ws(
    ws: WebSocketUpgrade,
    State(state): State<Arc<OrchestratorState>>,
    Query(query): Query<LspQuery>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_client(socket, state, query.language))
}

async fn handle_client(socket: WebSocket, state: Arc<OrchestratorState>, language: String) {
    let language = LanguageId::new(language);
    let session = match state
        .pool
        .get_or_spawn(state.config.workspace_root.clone(), language.clone())
        .await
    {
        Ok(s) => s,
        Err(err) => {
            tracing::warn!(error = %err, %language, "failed to spawn language session");
            return;
        }
    };

    let mut outbound_rx = session.outbound.subscribe();
    let (mut ws_tx, mut ws_rx) = socket.split();

    let fanout = tokio::spawn(async move {
        while let Ok(msg) = outbound_rx.recv().await {
            if ws_tx.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    while let Some(Ok(msg)) = ws_rx.next().await {
        let text = match msg {
            Message::Text(t) => t.to_string(),
            Message::Binary(b) => String::from_utf8_lossy(&b).into_owned(),
            Message::Close(_) => break,
            _ => continue,
        };
        if let Ok(value) = serde_json::from_str::<Value>(&text) {
            track_document(&state, &value).await;
        }
        if let Err(err) = session.write_raw(&text).await {
            tracing::warn!(error = %err, "backend write failed");
            break;
        }
    }

    fanout.abort();
}

async fn track_document(state: &OrchestratorState, value: &Value) {
    let method = value.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let params = value.get("params");
    match method {
        "textDocument/didOpen" => {
            if let Some(p) = params {
                let uri = p
                    .pointer("/textDocument/uri")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let language_id = p
                    .pointer("/textDocument/languageId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("plaintext")
                    .to_string();
                let text = p
                    .pointer("/textDocument/text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let version = p
                    .pointer("/textDocument/version")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(1) as i32;
                if !uri.is_empty() {
                    state.documents.open(uri, language_id, text, version).await;
                }
            }
        }
        "textDocument/didChange" => {
            if let Some(p) = params {
                let uri = p
                    .pointer("/textDocument/uri")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let version = p
                    .pointer("/textDocument/version")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0) as i32;
                let text = p
                    .pointer("/contentChanges/0/text")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                if let Some(text) = text {
                    let _ = state.documents.change(uri, text, version).await;
                }
            }
        }
        "textDocument/didClose" => {
            if let Some(uri) = params
                .and_then(|p| p.pointer("/textDocument/uri"))
                .and_then(|v| v.as_str())
            {
                state.documents.close(uri).await;
            }
        }
        _ => {}
    }
}

#[derive(Deserialize)]
pub struct AgentDocQuery {
    pub uri: String,
    #[serde(default)]
    pub line: Option<u32>,
    #[serde(default)]
    pub character: Option<u32>,
    #[serde(default)]
    pub language: Option<String>,
}

fn language_for_uri(uri: &str, override_lang: Option<&str>) -> LanguageId {
    if let Some(lang) = override_lang.filter(|s| !s.is_empty()) {
        return LanguageId::new(lang);
    }
    let path = uri.strip_prefix("file://").unwrap_or(uri);
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    LanguageId::new(match ext {
        "py" => "python",
        "ts" | "tsx" => "typescript",
        "js" | "jsx" | "mjs" => "javascript",
        "rs" => "rust",
        "grapheme" | "gr" => "grapheme",
        _ => "grapheme",
    })
}

async fn session_for_doc(
    state: &OrchestratorState,
    q: &AgentDocQuery,
) -> anyhow::Result<Arc<crate::session::LiveSession>> {
    let language = language_for_uri(&q.uri, q.language.as_deref());
    let session = state
        .pool
        .get_or_spawn(state.config.workspace_root.clone(), language)
        .await?;
    session.ensure_initialized(&state.root_uri()).await?;
    // Sync open doc from orchestrator store if present (notification, not request).
    if let Some(doc) = state.documents.get(&q.uri).await {
        session
            .write_raw(
                &json!({
                    "jsonrpc": "2.0",
                    "method": "textDocument/didOpen",
                    "params": {
                        "textDocument": {
                            "uri": doc.uri,
                            "languageId": doc.language_id,
                            "version": doc.version,
                            "text": doc.text
                        }
                    }
                })
                .to_string(),
            )
            .await?;
    }
    Ok(session)
}

fn position(q: &AgentDocQuery) -> Value {
    json!({
        "line": q.line.unwrap_or(0),
        "character": q.character.unwrap_or(0)
    })
}

async fn agent_hover(
    State(state): State<Arc<OrchestratorState>>,
    Query(q): Query<AgentDocQuery>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let session = session_for_doc(&state, &q)
        .await
        .map_err(|e| (axum::http::StatusCode::BAD_GATEWAY, e.to_string()))?;
    let result = session
        .request(
            "textDocument/hover",
            json!({
                "textDocument": { "uri": q.uri },
                "position": position(&q)
            }),
        )
        .await
        .map_err(|e| (axum::http::StatusCode::BAD_GATEWAY, e.to_string()))?;
    Ok(Json(json!({ "ok": true, "result": result })))
}

async fn agent_definition(
    State(state): State<Arc<OrchestratorState>>,
    Query(q): Query<AgentDocQuery>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let session = session_for_doc(&state, &q)
        .await
        .map_err(|e| (axum::http::StatusCode::BAD_GATEWAY, e.to_string()))?;
    let result = session
        .request(
            "textDocument/definition",
            json!({
                "textDocument": { "uri": q.uri },
                "position": position(&q)
            }),
        )
        .await
        .map_err(|e| (axum::http::StatusCode::BAD_GATEWAY, e.to_string()))?;
    Ok(Json(json!({ "ok": true, "result": result })))
}

async fn agent_diagnostics(
    State(state): State<Arc<OrchestratorState>>,
    Query(q): Query<AgentDocQuery>,
) -> Json<Value> {
    let language = language_for_uri(&q.uri, q.language.as_deref());
    let diagnostics = if let Ok(session) = state
        .pool
        .get_or_spawn(state.config.workspace_root.clone(), language)
        .await
    {
        session
            .diagnostics
            .read()
            .await
            .get(&q.uri)
            .cloned()
            .unwrap_or(json!({ "uri": q.uri, "diagnostics": [] }))
    } else {
        json!({ "uri": q.uri, "diagnostics": [] })
    };
    let doc = state.documents.get(&q.uri).await;
    Json(json!({
        "ok": true,
        "uri": q.uri,
        "open": doc.is_some(),
        "version": doc.as_ref().map(|d| d.version),
        "diagnostics": diagnostics.get("diagnostics").cloned().unwrap_or(json!([])),
    }))
}

async fn agent_symbols(
    State(state): State<Arc<OrchestratorState>>,
    Query(q): Query<AgentDocQuery>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let session = session_for_doc(&state, &q)
        .await
        .map_err(|e| (axum::http::StatusCode::BAD_GATEWAY, e.to_string()))?;
    let result = session
        .request(
            "textDocument/documentSymbol",
            json!({ "textDocument": { "uri": q.uri } }),
        )
        .await
        .map_err(|e| (axum::http::StatusCode::BAD_GATEWAY, e.to_string()))?;
    Ok(Json(json!({ "ok": true, "result": result })))
}

async fn detamu_snapshot(State(state): State<Arc<OrchestratorState>>) -> Json<Value> {
    let docs = state.detamu_document_snapshot().await;
    Json(json!({ "ok": true, "documents": docs }))
}

async fn detamu_handles(State(state): State<Arc<OrchestratorState>>) -> Json<Value> {
    let handles = state.detamu_session_handles().await;
    Json(json!({ "ok": true, "handles": handles }))
}
