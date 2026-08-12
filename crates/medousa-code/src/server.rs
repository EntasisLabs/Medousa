//! HTTP/WS surface for the Orchestrator.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tower_http::cors::CorsLayer;
use url::Url;

use crate::backend::{LanguageServerBackend, command_available, csharp_tooling_available, spawn_backend};
use crate::detamu::{DetamuDocumentSnapshot, DetamuServerHandle};
use crate::diagnostics::WorkspaceDiagnosticStore;
use crate::document::DocumentStore;
use crate::language_session::{LanguageSessionIdentity, LanguageSessionKind, LanguageSessionStore};
use crate::registry::{LanguageId, ServerRegistry};
use crate::session::{
    SessionPool, initialization_options, workspace_configuration_response, workspace_settings,
};
use crate::{ENGINE_API_REVISION, ENGINE_NAME, ENGINE_VERSION};

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
    pub workspace_diagnostics: Arc<WorkspaceDiagnosticStore>,
    pub language_sessions: Arc<LanguageSessionStore>,
    pub editor_sessions: Arc<AtomicUsize>,
    pub started: Instant,
}

impl OrchestratorState {
    pub fn new(config: OrchestratorConfig, registry: ServerRegistry) -> Self {
        let language_sessions = LanguageSessionStore::new();
        Self {
            config: config.clone(),
            pool: SessionPool::new(registry, Arc::clone(&language_sessions)),
            documents: DocumentStore::new(),
            workspace_diagnostics: WorkspaceDiagnosticStore::new(),
            language_sessions,
            editor_sessions: Arc::new(AtomicUsize::new(0)),
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
                workspace_root: key.language_root.display().to_string(),
                language: key.language.to_string(),
                session_key: format!("{}::{}", key.language_root.display(), key.language),
            })
            .collect()
    }
}

fn path_to_file_uri(path: &Path) -> String {
    Url::from_file_path(path)
        .map(|url| url.to_string())
        .unwrap_or_else(|_| format!("file:///{}", path.to_string_lossy().replace('\\', "/")))
}

#[derive(Serialize)]
pub struct HealthResponse {
    pub name: String,
    pub version: String,
    pub api_revision: u32,
    pub uptime_secs: u64,
    pub active_sessions: usize,
    pub editor_sessions: usize,
    pub agent_sessions: usize,
    pub workspace_root: String,
    pub languages: Vec<String>,
    pub allowed_roots: Vec<String>,
}

#[derive(Deserialize)]
pub struct LspQuery {
    /// Language id for this client connection (default grapheme).
    #[serde(default = "default_language")]
    pub language: String,
    /// Requested repository root. Must be under an orchestrator allowed root.
    #[serde(default)]
    pub workspace_root: Option<PathBuf>,
    /// Active document used to select the closest language root.
    #[serde(default)]
    pub document_uri: Option<String>,
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
        .route("/v1/code/workspace-diagnostics", get(workspace_diagnostics))
        .route("/v1/code/symbols", get(agent_symbols))
        .route("/v1/code/workspace-symbols", get(workspace_symbols))
        .route("/v1/code/capabilities", get(agent_capabilities))
        .route("/v1/code/conventions", get(agent_conventions))
        .route("/v1/code/language-root", get(language_root))
        .route("/v1/code/language-sessions", get(language_sessions))
        .route("/v1/code/language-matrix", get(language_matrix))
        .route("/v1/code/request", post(agent_request))
        .route("/v1/detamu/snapshot", get(detamu_snapshot))
        .route("/v1/detamu/handles", get(detamu_handles))
        .layer(CorsLayer::permissive())
        .with_state(Arc::new(state))
}

pub async fn serve(state: OrchestratorState) -> anyhow::Result<()> {
    let addr = state.config.bind;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "medousa-code orchestrator listening");
    let pool = Arc::clone(&state.pool);
    let reaper_pool = Arc::clone(&pool);
    let reaper = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        interval.tick().await;
        loop {
            interval.tick().await;
            let reaped = reaper_pool.shutdown_idle(Duration::from_secs(90)).await;
            if reaped > 0 {
                tracing::info!(reaped, "reclaimed idle language server sessions");
            }
        }
    });
    let result = axum::serve(listener, app(state)).await;
    reaper.abort();
    pool.shutdown_all().await;
    result?;
    Ok(())
}

async fn health(State(state): State<Arc<OrchestratorState>>) -> Json<HealthResponse> {
    let agent_sessions = state.pool.active_count().await;
    let editor_sessions = state.editor_sessions.load(Ordering::Relaxed);
    Json(HealthResponse {
        name: ENGINE_NAME.into(),
        version: ENGINE_VERSION.into(),
        api_revision: ENGINE_API_REVISION,
        uptime_secs: state.started.elapsed().as_secs(),
        active_sessions: editor_sessions + agent_sessions,
        editor_sessions,
        agent_sessions,
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
    ws.on_upgrade(move |socket| {
        handle_client(
            socket,
            state,
            query.language,
            query.workspace_root,
            query.document_uri,
        )
    })
}

fn requested_workspace_root(
    state: &OrchestratorState,
    requested: Option<PathBuf>,
) -> anyhow::Result<PathBuf> {
    let root = requested.unwrap_or_else(|| state.config.workspace_root.clone());
    let root = root.canonicalize()?;
    if !state.path_allowed(&root) {
        anyhow::bail!("workspace root is outside the coding engine allowlist");
    }
    Ok(root)
}

fn document_path_for_project(uri: &str, project_root: &Path) -> anyhow::Result<PathBuf> {
    let normalized_uri = uri.to_ascii_lowercase();
    if normalized_uri.contains("%2f") || normalized_uri.contains("%5c") {
        anyhow::bail!("document URI contains an encoded path separator");
    }
    let url = Url::parse(uri)?;
    if url.scheme() != "file" {
        anyhow::bail!("document URI must use the file scheme");
    }
    let path = url
        .to_file_path()
        .map_err(|_| anyhow::anyhow!("document URI is not a valid workshop file path"))?;
    let path = if path.exists() {
        path.canonicalize()?
    } else {
        let parent = path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("document path has no parent"))?
            .canonicalize()?;
        let name = path
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("document path has no file name"))?;
        parent.join(name)
    };
    if path == project_root || !path.starts_with(project_root) {
        anyhow::bail!("document is outside the governed coding workspace");
    }
    Ok(path)
}

fn resolve_language_root(
    state: &OrchestratorState,
    project_root: &Path,
    language: &LanguageId,
    document_uri: Option<&str>,
) -> anyhow::Result<PathBuf> {
    let Some(document_uri) = document_uri.filter(|uri| !uri.trim().is_empty()) else {
        return Ok(project_root.to_path_buf());
    };
    let document = document_path_for_project(document_uri, project_root)?;
    Ok(state
        .pool
        .registry()
        .resolve_root(language, &document, project_root))
}

fn advertise_editor_client_capabilities(capabilities: &mut serde_json::Map<String, Value>) {
    // CodeMirror's built-in client omits these. Advertise them here so servers
    // send configuration / progress / folder requests that this boundary answers.
    let workspace = capabilities
        .entry("workspace".to_string())
        .or_insert_with(|| json!({}))
        .as_object_mut();
    if let Some(workspace) = workspace {
        workspace.insert("configuration".to_string(), Value::Bool(true));
        workspace.insert("workspaceFolders".to_string(), Value::Bool(true));
    }
    let window = capabilities
        .entry("window".to_string())
        .or_insert_with(|| json!({}))
        .as_object_mut();
    if let Some(window) = window {
        window.insert("workDoneProgress".to_string(), Value::Bool(true));
    }
}

fn rewrite_initialize_params(
    params: &mut serde_json::Map<String, Value>,
    language: &LanguageId,
    language_root: &Path,
) {
    let language_root_uri = path_to_file_uri(language_root);
    params.insert(
        "initializationOptions".to_string(),
        initialization_options(language),
    );
    params.insert(
        "rootUri".to_string(),
        Value::String(language_root_uri.clone()),
    );
    params.insert(
        "rootPath".to_string(),
        Value::String(language_root.to_string_lossy().into_owned()),
    );
    params.insert(
        "workspaceFolders".to_string(),
        json!([{
            "uri": language_root_uri,
            "name": language_root
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("workspace")
        }]),
    );
    match params
        .entry("capabilities".to_string())
        .or_insert_with(|| json!({}))
        .as_object_mut()
    {
        Some(capabilities) => advertise_editor_client_capabilities(capabilities),
        None => {
            params.insert(
                "capabilities".to_string(),
                json!({
                    "workspace": {
                        "configuration": true,
                        "workspaceFolders": true
                    },
                    "window": { "workDoneProgress": true }
                }),
            );
        }
    }
}

fn language_server_response(id: Value, result: Value) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    })
    .to_string()
}

async fn handle_language_server_request(
    backend: &Arc<dyn LanguageServerBackend>,
    lifecycle: &crate::language_session::LanguageSessionHandle,
    language: &LanguageId,
    language_root: &Path,
    value: &Value,
) -> Result<bool, crate::backend::BackendError> {
    let Some(method) = value.get("method").and_then(Value::as_str) else {
        return Ok(false);
    };
    let Some(id) = value.get("id").cloned() else {
        return Ok(false);
    };
    let result = match method {
        "workspace/configuration" => {
            workspace_configuration_response(language, value.get("params").unwrap_or(&Value::Null))
        }
        "workspace/workspaceFolders" => json!([{
            "uri": path_to_file_uri(language_root),
            "name": language_root
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("workspace"),
        }]),
        "window/workDoneProgress/create" => {
            if let Some(token) = value.pointer("/params/token") {
                lifecycle.create_progress(token).await;
            }
            Value::Null
        }
        "client/registerCapability" | "client/unregisterCapability" => Value::Null,
        _ => return Ok(false),
    };
    backend
        .write_message(&language_server_response(id, result))
        .await?;
    Ok(true)
}

async fn send_workspace_configuration(
    backend: &Arc<dyn LanguageServerBackend>,
    language: &LanguageId,
) -> Result<(), crate::backend::BackendError> {
    backend
        .write_message(
            &json!({
                "jsonrpc": "2.0",
                "method": "workspace/didChangeConfiguration",
                "params": { "settings": workspace_settings(language) },
            })
            .to_string(),
        )
        .await
}

async fn handle_client(
    socket: WebSocket,
    state: Arc<OrchestratorState>,
    language: String,
    requested_root: Option<PathBuf>,
    document_uri: Option<String>,
) {
    let language = LanguageId::new(language);
    let project_root = match requested_workspace_root(&state, requested_root) {
        Ok(root) => root,
        Err(err) => {
            tracing::warn!(error = %err, "rejected coding workspace root");
            return;
        }
    };
    let language_root =
        match resolve_language_root(&state, &project_root, &language, document_uri.as_deref()) {
            Ok(root) => root,
            Err(err) => {
                tracing::warn!(error = %err, %language, "rejected coding document root");
                return;
            }
        };
    let spec = match state.pool.registry().get(&language) {
        Some(spec) => spec.clone(),
        None => {
            tracing::warn!(%language, "no language server registered");
            return;
        }
    };
    let lifecycle = state
        .language_sessions
        .begin(LanguageSessionIdentity {
            kind: LanguageSessionKind::Editor,
            project_root: project_root.clone(),
            language_root: language_root.clone(),
            language: language.to_string(),
        })
        .await;
    lifecycle
        .starting(format!("Starting {language} language server"))
        .await;
    // Editor connections are transparent one-client/one-server channels.
    // Sharing the agent session here would require JSON-RPC id translation and
    // initialize virtualization; blindly forwarding multiple clients corrupts
    // the protocol and lets their request ids collide.
    let backend = match spawn_backend(&spec, &language_root, lifecycle.logs()).await {
        Ok(backend) => backend,
        Err(err) => {
            tracing::warn!(error = %err, %language, "failed to spawn language session");
            lifecycle
                .failed(format!("Failed to start {language}: {err}"))
                .await;
            return;
        }
    };
    lifecycle
        .initializing(format!("Initializing {language}"))
        .await;
    state.editor_sessions.fetch_add(1, Ordering::Relaxed);
    let diagnostic_session_id = state
        .workspace_diagnostics
        .begin_session(project_root.clone(), language.to_string())
        .await;
    tracing::info!(
        %language,
        project_root = %project_root.display(),
        language_root = %language_root.display(),
        editor_sessions = state.editor_sessions.load(Ordering::Relaxed),
        "started editor language server"
    );
    let (mut ws_tx, mut ws_rx) = socket.split();
    // LSP framing reads are not cancellation-safe after a header or body has
    // been partially consumed. Keep one dedicated reader alive and select on
    // completed frames instead of selecting directly on `read_message`.
    let (server_tx, mut server_rx) = tokio::sync::mpsc::channel(64);
    let reader_backend = Arc::clone(&backend);
    let reader_task = tokio::spawn(async move {
        loop {
            let message = reader_backend.read_message().await;
            let failed = message.is_err();
            if server_tx.send(message).await.is_err() || failed {
                break;
            }
        }
    });
    let mut client_disconnected = false;
    loop {
        tokio::select! {
            server_message = server_rx.recv() => {
                let message = match server_message {
                    Some(Ok(message)) => message,
                    Some(Err(err)) => {
                        tracing::warn!(error = %err, %language, "editor language server stopped");
                        lifecycle
                            .failed(format!("{language} language server stopped: {err}"))
                            .await;
                        break;
                    }
                    None => {
                        lifecycle
                            .failed(format!("{language} language server reader stopped"))
                            .await;
                        break;
                    }
                };
                lifecycle.record_lsp_message(&message).await;
                if let Ok(value) = serde_json::from_str::<Value>(&message) {
                    match handle_language_server_request(
                        &backend,
                        &lifecycle,
                        &language,
                        &language_root,
                        &value,
                    )
                    .await
                    {
                        Ok(true) => continue,
                        Ok(false) => {}
                        Err(err) => {
                            lifecycle
                                .failed(format!("Could not answer {language} language server: {err}"))
                                .await;
                            break;
                        }
                    }
                }
                state
                    .workspace_diagnostics
                    .record_message(diagnostic_session_id, &message)
                    .await;
                if ws_tx.send(Message::Text(message.into())).await.is_err() {
                    client_disconnected = true;
                    break;
                }
            }
            client_message = ws_rx.next() => {
                let message = match client_message {
                    Some(Ok(message)) => message,
                    Some(Err(err)) => {
                        tracing::debug!(error = %err, %language, "editor LSP client disconnected");
                        client_disconnected = true;
                        break;
                    }
                    None => {
                        client_disconnected = true;
                        break;
                    }
                };
                let text = match message {
                    Message::Text(text) => text.to_string(),
                    Message::Binary(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
                    Message::Close(_) => {
                        client_disconnected = true;
                        break;
                    }
                    _ => continue,
                };
                let mut outbound = text;
                let mut initialized = false;
                if let Ok(mut value) = serde_json::from_str::<Value>(&outbound) {
                    track_document(&state, &value).await;
                    let initializing = value.get("method").and_then(Value::as_str)
                        == Some("initialize");
                    initialized = value.get("method").and_then(Value::as_str)
                        == Some("initialized");
                    if initializing {
                        if let Some(params) = value.get_mut("params").and_then(Value::as_object_mut) {
                            rewrite_initialize_params(params, &language, &language_root);
                        }
                        outbound = value.to_string();
                    }
                }
                if let Err(err) = backend.write_message(&outbound).await {
                    tracing::warn!(error = %err, %language, "backend write failed");
                    lifecycle
                        .failed(format!("Could not write to {language} language server: {err}"))
                        .await;
                    break;
                }
                if initialized {
                    if let Err(err) = send_workspace_configuration(&backend, &language).await {
                        lifecycle
                            .failed(format!("Could not configure {language} language server: {err}"))
                            .await;
                        break;
                    }
                    lifecycle
                        .ready(format!("{language} language server ready"))
                        .await;
                }
            }
        }
    }

    let _ = ws_tx.close().await;
    reader_task.abort();
    backend.shutdown().await;
    if client_disconnected {
        lifecycle.stopped("Editor connection closed").await;
    } else {
        lifecycle.stopped("Language session stopped").await;
    }
    state
        .workspace_diagnostics
        .end_session(diagnostic_session_id)
        .await;
    state.editor_sessions.fetch_sub(1, Ordering::Relaxed);
    tracing::info!(
        %language,
        project_root = %project_root.display(),
        language_root = %language_root.display(),
        editor_sessions = state.editor_sessions.load(Ordering::Relaxed),
        "stopped editor language server"
    );
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
    #[serde(default)]
    pub workspace_root: Option<PathBuf>,
}

#[derive(Deserialize)]
pub struct AgentWorkspaceQuery {
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub workspace_root: Option<PathBuf>,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub uri: Option<String>,
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
        "svelte" => "svelte",
        "rs" => "rust",
        "go" => "go",
        "c" | "h" => "c",
        "cc" | "cpp" | "cxx" | "hh" | "hpp" | "hxx" => "cpp",
        "cs" => "csharp",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "rb" => "ruby",
        "php" => "php",
        "swift" => "swift",
        "lua" => "lua",
        "grapheme" | "gr" => "grapheme",
        _ => "plaintext",
    })
}

async fn language_root(
    State(state): State<Arc<OrchestratorState>>,
    Query(q): Query<AgentDocQuery>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let language = language_for_uri(&q.uri, q.language.as_deref());
    let project_root = requested_workspace_root(&state, q.workspace_root)
        .map_err(|err| (axum::http::StatusCode::BAD_REQUEST, err.to_string()))?;
    let root = resolve_language_root(&state, &project_root, &language, Some(&q.uri))
        .map_err(|err| (axum::http::StatusCode::BAD_REQUEST, err.to_string()))?;
    let relative_root = root
        .strip_prefix(&project_root)
        .unwrap_or(Path::new(""))
        .to_string_lossy()
        .replace('\\', "/");
    Ok(Json(json!({
        "ok": true,
        "language": language,
        "root_uri": path_to_file_uri(&root),
        "relative_root": relative_root,
    })))
}

async fn language_sessions(
    State(state): State<Arc<OrchestratorState>>,
    Query(q): Query<AgentDocQuery>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let language = language_for_uri(&q.uri, q.language.as_deref());
    let project_root = requested_workspace_root(&state, q.workspace_root)
        .map_err(|err| (axum::http::StatusCode::BAD_REQUEST, err.to_string()))?;
    let language_root = resolve_language_root(&state, &project_root, &language, Some(&q.uri))
        .map_err(|err| (axum::http::StatusCode::BAD_REQUEST, err.to_string()))?;
    let sessions = state
        .language_sessions
        .snapshots(&project_root, Some(language.as_str()), Some(&language_root))
        .await;
    Ok(Json(json!({
        "ok": true,
        "language": language,
        "root_uri": path_to_file_uri(&language_root),
        "sessions": sessions,
    })))
}

async fn language_matrix(
    State(state): State<Arc<OrchestratorState>>,
) -> Json<Value> {
    let languages: Vec<Value> = state
        .pool
        .registry()
        .specs()
        .iter()
        .map(|spec| {
            let command = spec.kind.command_name().map(str::to_string);
            let binary_available = match &spec.kind {
                crate::registry::ServerKind::Grapheme => true,
                crate::registry::ServerKind::Stdio { command } => {
                    if spec.language.as_str() == "csharp" {
                        csharp_tooling_available()
                    } else {
                        command_available(command)
                    }
                }
            };
            json!({
                "language": spec.language,
                "command": command,
                "binary_available": binary_available,
                "usable": binary_available,
                "package_id": spec.package_id,
                "root_markers": spec.root_markers,
                "extensions": spec.extensions,
                "args": spec.args,
            })
        })
        .collect();
    Json(json!({
        "ok": true,
        "languages": languages,
    }))
}

async fn session_for_doc(
    state: &OrchestratorState,
    q: &AgentDocQuery,
) -> anyhow::Result<Arc<crate::session::LiveSession>> {
    let language = language_for_uri(&q.uri, q.language.as_deref());
    let project_root = requested_workspace_root(state, q.workspace_root.clone())?;
    let language_root = resolve_language_root(state, &project_root, &language, Some(&q.uri))?;
    let session = state
        .pool
        .get_or_spawn(project_root, language_root.clone(), language)
        .await?;
    session
        .ensure_initialized(&path_to_file_uri(&language_root))
        .await?;
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
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let language = language_for_uri(&q.uri, q.language.as_deref());
    let project_root = requested_workspace_root(&state, q.workspace_root.clone())
        .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e.to_string()))?;
    let language_root = resolve_language_root(&state, &project_root, &language, Some(&q.uri))
        .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e.to_string()))?;
    let diagnostics = if let Some(session) = state
        .pool
        .get_existing(project_root, language_root, language)
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
    Ok(Json(json!({
        "ok": true,
        "uri": q.uri,
        "open": doc.is_some(),
        "version": doc.as_ref().map(|d| d.version),
        "diagnostics": diagnostics.get("diagnostics").cloned().unwrap_or(json!([])),
    })))
}

async fn workspace_diagnostics(
    State(state): State<Arc<OrchestratorState>>,
    Query(q): Query<AgentWorkspaceQuery>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let project_root = requested_workspace_root(&state, q.workspace_root)
        .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e.to_string()))?;
    let requested_language = q.language.filter(|language| !language.trim().is_empty());
    let sessions = if let Some(language) = requested_language.as_deref() {
        let language = LanguageId::new(language);
        let mut sessions = state
            .pool
            .existing_for_workspace_language(&project_root, &language)
            .await;
        if sessions.is_empty() {
            let session = state
                .pool
                .get_or_spawn(project_root.clone(), project_root.clone(), language)
                .await
                .map_err(|e| (axum::http::StatusCode::BAD_GATEWAY, e.to_string()))?;
            session
                .ensure_initialized(&path_to_file_uri(&project_root))
                .await
                .map_err(|e| (axum::http::StatusCode::BAD_GATEWAY, e.to_string()))?;
            sessions.push(session);
        }
        sessions
    } else {
        state.pool.existing_for_workspace(&project_root).await
    };

    let mut languages = std::collections::BTreeSet::new();
    let mut documents = std::collections::BTreeMap::<(String, String), Value>::new();
    for session in sessions {
        let language = session.key.language.to_string();
        languages.insert(language.clone());
        for diagnostic in session.diagnostics.read().await.values() {
            let Some(uri) = diagnostic.get("uri").and_then(Value::as_str) else {
                continue;
            };
            let mut diagnostic = diagnostic.clone();
            if let Some(object) = diagnostic.as_object_mut() {
                object.insert("language".into(), Value::String(language.clone()));
            }
            documents.insert((language.clone(), uri.to_string()), diagnostic);
        }
    }

    let editor_snapshot = state
        .workspace_diagnostics
        .snapshot(&project_root, requested_language.as_deref())
        .await;
    languages.extend(editor_snapshot.languages);
    for diagnostic in editor_snapshot.documents {
        let key = (diagnostic.language.clone(), diagnostic.uri.clone());
        documents.insert(key, serde_json::to_value(diagnostic).unwrap_or(Value::Null));
    }
    Ok(Json(json!({
        "ok": true,
        "scope": if requested_language.is_some() { "language" } else { "active_sessions" },
        "languages": languages,
        "documents": documents.into_values().collect::<Vec<_>>(),
    })))
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

async fn workspace_symbols(
    State(state): State<Arc<OrchestratorState>>,
    Query(q): Query<AgentWorkspaceQuery>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let language = LanguageId::new(q.language.as_deref().unwrap_or("plaintext"));
    let project_root = requested_workspace_root(&state, q.workspace_root)
        .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e.to_string()))?;
    let language_root = resolve_language_root(&state, &project_root, &language, q.uri.as_deref())
        .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e.to_string()))?;
    let session = state
        .pool
        .get_or_spawn(project_root, language_root.clone(), language)
        .await
        .map_err(|e| (axum::http::StatusCode::BAD_GATEWAY, e.to_string()))?;
    session
        .ensure_initialized(&path_to_file_uri(&language_root))
        .await
        .map_err(|e| (axum::http::StatusCode::BAD_GATEWAY, e.to_string()))?;
    let result = session
        .request(
            "workspace/symbol",
            json!({ "query": q.query.unwrap_or_default() }),
        )
        .await
        .map_err(|e| (axum::http::StatusCode::BAD_GATEWAY, e.to_string()))?;
    Ok(Json(json!({ "ok": true, "result": result })))
}

async fn agent_capabilities(
    State(state): State<Arc<OrchestratorState>>,
    Query(q): Query<AgentDocQuery>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let session = session_for_doc(&state, &q)
        .await
        .map_err(|e| (axum::http::StatusCode::BAD_GATEWAY, e.to_string()))?;
    let capabilities = session.capabilities.read().await.clone();
    Ok(Json(json!({ "ok": true, "capabilities": capabilities })))
}

async fn agent_conventions(
    State(state): State<Arc<OrchestratorState>>,
    Query(q): Query<AgentDocQuery>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let workspace_root = requested_workspace_root(&state, q.workspace_root)
        .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e.to_string()))?;
    let path = q
        .uri
        .strip_prefix("file://")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(&q.uri));
    if !path.starts_with(&workspace_root) {
        return Err((
            axum::http::StatusCode::BAD_REQUEST,
            "document is outside the coding workspace".to_string(),
        ));
    }
    let mut files = Vec::new();
    let mut directory = path.parent();
    while let Some(current) = directory {
        if !current.starts_with(&workspace_root) {
            break;
        }
        let candidate = current.join(".editorconfig");
        if candidate.is_file() {
            files.push(candidate);
        }
        if current == workspace_root {
            break;
        }
        directory = current.parent();
    }
    files.reverse();
    let mut resolved = serde_json::Map::new();
    for file in files {
        let Ok(content) = tokio::fs::read_to_string(&file).await else {
            continue;
        };
        let relative = path
            .strip_prefix(file.parent().unwrap_or(&workspace_root))
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        apply_editorconfig(&content, &relative, &mut resolved);
    }
    Ok(Json(json!({ "ok": true, "conventions": resolved })))
}

fn apply_editorconfig(
    content: &str,
    relative_path: &str,
    resolved: &mut serde_json::Map<String, Value>,
) {
    let mut active = true;
    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            active = editorconfig_pattern_matches(&line[1..line.len() - 1], relative_path);
            continue;
        }
        if !active {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if matches!(
            key,
            "indent_style" | "indent_size" | "tab_width" | "end_of_line" | "insert_final_newline"
        ) {
            resolved.insert(key.to_string(), Value::String(value.trim().to_string()));
        }
    }
}

fn editorconfig_pattern_matches(pattern: &str, relative_path: &str) -> bool {
    let name = relative_path.rsplit('/').next().unwrap_or(relative_path);
    pattern == "*"
        || pattern == relative_path
        || pattern == name
        || pattern
            .strip_prefix("*.")
            .is_some_and(|extension| name.ends_with(&format!(".{extension}")))
        || pattern
            .strip_prefix("**/*.")
            .is_some_and(|extension| name.ends_with(&format!(".{extension}")))
}

#[derive(Deserialize)]
pub struct AgentRequest {
    pub action: String,
    pub uri: String,
    #[serde(default)]
    pub line: Option<u32>,
    #[serde(default)]
    pub character: Option<u32>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub workspace_root: Option<PathBuf>,
    #[serde(default)]
    pub new_name: Option<String>,
    #[serde(default)]
    pub range: Option<Value>,
    #[serde(default)]
    pub diagnostics: Vec<Value>,
    #[serde(default)]
    pub options: Option<Value>,
}

async fn agent_request(
    State(state): State<Arc<OrchestratorState>>,
    Json(request): Json<AgentRequest>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let query = AgentDocQuery {
        uri: request.uri.clone(),
        line: request.line,
        character: request.character,
        language: request.language,
        workspace_root: request.workspace_root,
    };
    let session = session_for_doc(&state, &query)
        .await
        .map_err(|e| (axum::http::StatusCode::BAD_GATEWAY, e.to_string()))?;
    let position = position(&query);
    let (method, params) = match request.action.as_str() {
        "references" => (
            "textDocument/references",
            json!({
                "textDocument": { "uri": request.uri },
                "position": position,
                "context": { "includeDeclaration": true }
            }),
        ),
        "rename" => {
            let new_name = request
                .new_name
                .filter(|name| !name.trim().is_empty())
                .ok_or((
                    axum::http::StatusCode::BAD_REQUEST,
                    "rename requires new_name".to_string(),
                ))?;
            (
                "textDocument/rename",
                json!({
                    "textDocument": { "uri": request.uri },
                    "position": position,
                    "newName": new_name
                }),
            )
        }
        "format" => (
            "textDocument/formatting",
            json!({
                "textDocument": { "uri": request.uri },
                "options": request.options.unwrap_or_else(|| json!({
                    "tabSize": 2,
                    "insertSpaces": true
                }))
            }),
        ),
        "code_actions" | "organize_imports" => {
            let only = if request.action == "organize_imports" {
                json!(["source.organizeImports"])
            } else {
                Value::Null
            };
            (
                "textDocument/codeAction",
                json!({
                    "textDocument": { "uri": request.uri },
                    "range": request.range.unwrap_or_else(|| json!({
                        "start": position,
                        "end": position
                    })),
                    "context": {
                        "diagnostics": request.diagnostics,
                        "only": only
                    }
                }),
            )
        }
        _ => {
            return Err((
                axum::http::StatusCode::BAD_REQUEST,
                "unsupported language action".to_string(),
            ));
        }
    };
    let result = session
        .request(method, params)
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

#[cfg(test)]
mod tests {
    use super::{
        AgentDocQuery, OrchestratorConfig, OrchestratorState, apply_editorconfig,
        document_path_for_project, language_matrix, language_root, language_sessions,
        path_to_file_uri, resolve_language_root, rewrite_initialize_params,
    };
    use crate::language_session::{LanguageSessionIdentity, LanguageSessionKind};
    use crate::registry::{LanguageId, ServerRegistry};
    use serde_json::{Map, Value};

    #[test]
    fn editorconfig_applies_matching_language_section() {
        let mut resolved = Map::<String, Value>::new();
        apply_editorconfig(
            "indent_style = space\n[*]\nindent_size = 4\n[*.rs]\nindent_size = 2\n",
            "src/main.rs",
            &mut resolved,
        );
        assert_eq!(resolved["indent_style"], "space");
        assert_eq!(resolved["indent_size"], "2");
    }

    #[test]
    fn document_uri_resolution_is_bounded_to_the_canonical_project() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("project");
        std::fs::create_dir_all(project.join("src")).unwrap();
        let file = project.join("src/a b.ts");
        std::fs::write(&file, "export {};").unwrap();
        let project = project.canonicalize().unwrap();

        assert_eq!(
            document_path_for_project(&path_to_file_uri(&file), &project).unwrap(),
            file.canonicalize().unwrap(),
        );
        let outside = dir.path().join("outside.ts");
        std::fs::write(&outside, "").unwrap();
        assert!(document_path_for_project(&path_to_file_uri(&outside), &project).is_err());
        let encoded_separator = format!("{}/src%2Fmain.ts", path_to_file_uri(&project));
        assert!(document_path_for_project(&encoded_separator, &project).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn document_uri_resolution_rejects_a_symlink_escape() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("project");
        std::fs::create_dir_all(&project).unwrap();
        let outside = dir.path().join("outside.ts");
        std::fs::write(&outside, "").unwrap();
        let linked = project.join("linked.ts");
        symlink(&outside, &linked).unwrap();
        let project = project.canonicalize().unwrap();

        assert!(document_path_for_project(&path_to_file_uri(&linked), &project).is_err());
    }

    #[test]
    fn language_root_resolution_uses_the_closest_marker() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("project");
        let package = project.join("packages/app");
        std::fs::create_dir_all(package.join("src")).unwrap();
        std::fs::write(project.join("package.json"), "{}").unwrap();
        std::fs::write(package.join("package.json"), "{}").unwrap();
        let file = package.join("src/main.ts");
        std::fs::write(&file, "export {};").unwrap();
        let project = project.canonicalize().unwrap();
        let state = OrchestratorState::new(
            OrchestratorConfig {
                bind: "127.0.0.1:0".parse().unwrap(),
                workspace_root: project.clone(),
                allowed_roots: vec![project.clone()],
            },
            ServerRegistry::with_defaults(),
        );

        let root = resolve_language_root(
            &state,
            &project,
            &LanguageId::new("typescript"),
            Some(&path_to_file_uri(&file)),
        )
        .unwrap();
        assert_eq!(root, package.canonicalize().unwrap());
    }

    #[test]
    fn editor_initialize_is_fenced_to_the_resolved_language_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("packages/app");
        std::fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let mut params = serde_json::json!({
            "rootUri": "file:///wrong",
            "rootPath": "/wrong",
            "capabilities": {
                "textDocument": { "hover": {} },
                "window": { "showMessage": {} }
            }
        })
        .as_object()
        .unwrap()
        .clone();

        rewrite_initialize_params(&mut params, &LanguageId::new("typescript"), &root);

        assert_eq!(params["rootUri"], path_to_file_uri(&root));
        assert_eq!(params["rootPath"], root.to_string_lossy().as_ref());
        assert_eq!(
            params["workspaceFolders"][0]["uri"],
            path_to_file_uri(&root)
        );
        assert_eq!(params["workspaceFolders"][0]["name"], "app");
        assert_eq!(params["capabilities"]["workspace"]["configuration"], true);
        assert_eq!(
            params["capabilities"]["workspace"]["workspaceFolders"],
            true
        );
        assert_eq!(params["capabilities"]["window"]["workDoneProgress"], true);
        assert_eq!(params["capabilities"]["window"]["showMessage"], serde_json::json!({}));
        assert_eq!(params["capabilities"]["textDocument"]["hover"], serde_json::json!({}));
    }

    #[tokio::test]
    async fn language_root_route_reports_a_project_relative_root() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("project");
        let package = project.join("packages/app");
        std::fs::create_dir_all(package.join("src")).unwrap();
        std::fs::write(package.join("package.json"), "{}").unwrap();
        let file = package.join("src/main.ts");
        std::fs::write(&file, "export {};").unwrap();
        let project = project.canonicalize().unwrap();
        let state = std::sync::Arc::new(OrchestratorState::new(
            OrchestratorConfig {
                bind: "127.0.0.1:0".parse().unwrap(),
                workspace_root: project.clone(),
                allowed_roots: vec![project.clone()],
            },
            ServerRegistry::with_defaults(),
        ));

        let response = language_root(
            axum::extract::State(state),
            axum::extract::Query(AgentDocQuery {
                uri: path_to_file_uri(&file),
                line: None,
                character: None,
                language: Some("typescript".into()),
                workspace_root: Some(project),
            }),
        )
        .await
        .unwrap()
        .0;

        assert_eq!(response["relative_root"], "packages/app");
        assert_eq!(
            response["root_uri"],
            path_to_file_uri(&package.canonicalize().unwrap())
        );
    }

    #[tokio::test]
    async fn language_sessions_route_returns_only_the_resolved_root_history() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("project");
        let package = project.join("packages/app");
        std::fs::create_dir_all(package.join("src")).unwrap();
        std::fs::write(package.join("package.json"), "{}").unwrap();
        let file = package.join("src/main.ts");
        std::fs::write(&file, "export {};").unwrap();
        let project = project.canonicalize().unwrap();
        let package = package.canonicalize().unwrap();
        let state = std::sync::Arc::new(OrchestratorState::new(
            OrchestratorConfig {
                bind: "127.0.0.1:0".parse().unwrap(),
                workspace_root: project.clone(),
                allowed_roots: vec![project.clone()],
            },
            ServerRegistry::with_defaults(),
        ));
        let session = state
            .language_sessions
            .begin(LanguageSessionIdentity {
                kind: LanguageSessionKind::Editor,
                project_root: project.clone(),
                language_root: package.clone(),
                language: "typescript".into(),
            })
            .await;
        session.ready("Ready").await;

        let response = language_sessions(
            axum::extract::State(state),
            axum::extract::Query(AgentDocQuery {
                uri: path_to_file_uri(&file),
                line: None,
                character: None,
                language: Some("typescript".into()),
                workspace_root: Some(project),
            }),
        )
        .await
        .unwrap()
        .0;

        assert_eq!(response["root_uri"], path_to_file_uri(&package));
        assert_eq!(response["sessions"][0]["phase"], "ready");
        assert_eq!(response["sessions"][0]["relative_root"], "packages/app");
    }

    #[tokio::test]
    async fn language_matrix_reports_package_identity_without_claiming_usability() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().canonicalize().unwrap();
        let state = std::sync::Arc::new(OrchestratorState::new(
            OrchestratorConfig {
                bind: "127.0.0.1:0".parse().unwrap(),
                workspace_root: project.clone(),
                allowed_roots: vec![project],
            },
            ServerRegistry::with_defaults(),
        ));

        let response = language_matrix(axum::extract::State(state)).await.0;
        let languages = response["languages"].as_array().unwrap();
        let svelte = languages
            .iter()
            .find(|entry| entry["language"] == "svelte")
            .expect("svelte row");
        assert_eq!(svelte["command"], "svelteserver");
        assert_eq!(svelte["package_id"], "langservers");
        assert!(svelte["extensions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|ext| ext == "svelte"));
        // Registry membership alone is not usability — probe the binary.
        assert!(svelte["usable"].is_boolean());
        assert_eq!(svelte["usable"], svelte["binary_available"]);

        let grapheme = languages
            .iter()
            .find(|entry| entry["language"] == "grapheme")
            .expect("grapheme row");
        assert_eq!(grapheme["usable"], true);
        assert!(grapheme["command"].is_null());
    }
}
