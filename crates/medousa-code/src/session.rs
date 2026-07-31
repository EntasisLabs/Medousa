//! Session pool: one language-server backend per (workspace_root, language).
//!
//! Multi-client: Home + agents share one backend; outbound broadcast fans
//! notifications (diagnostics, etc.) to every subscribed WebSocket client.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::sync::{Mutex, RwLock, broadcast};

use crate::backend::{LanguageServerBackend, spawn_backend};
use crate::registry::{LanguageId, ServerRegistry};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionKey {
    pub workspace_root: PathBuf,
    pub language: LanguageId,
}

pub struct LiveSession {
    pub key: SessionKey,
    pub backend: Arc<dyn LanguageServerBackend>,
    /// Fan-out of raw LSP JSON bodies from the server to all clients.
    pub outbound: broadcast::Sender<String>,
    /// Latest publishDiagnostics payloads keyed by document URI.
    pub diagnostics: RwLock<HashMap<String, Value>>,
    /// Capabilities advertised by the active language server.
    pub capabilities: RwLock<Value>,
    initialized: AtomicBool,
    closed: AtomicBool,
    next_id: AtomicU64,
    write_lock: Mutex<()>,
    reader_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl LiveSession {
    pub async fn start_reader(self: &Arc<Self>) {
        let mut guard = self.reader_task.lock().await;
        if guard.is_some() {
            return;
        }
        let session = Arc::clone(self);
        *guard = Some(tokio::spawn(async move {
            loop {
                let msg = match session.backend.read_message().await {
                    Ok(msg) => msg,
                    Err(err) => {
                        tracing::warn!(error = %err, "language server reader stopped");
                        break;
                    }
                };
                if let Ok(value) = serde_json::from_str::<Value>(&msg)
                    && value.get("method").and_then(|m| m.as_str())
                        == Some("textDocument/publishDiagnostics")
                    && let Some(uri) = value.pointer("/params/uri").and_then(|v| v.as_str())
                {
                    let params = value.get("params").cloned().unwrap_or(Value::Null);
                    session
                        .diagnostics
                        .write()
                        .await
                        .insert(uri.to_string(), params);
                }
                let _ = session.outbound.send(msg);
            }
            session.closed.store(true, Ordering::SeqCst);
        }));
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    fn ensure_open(&self) -> anyhow::Result<()> {
        if self.is_closed() {
            anyhow::bail!("language server session closed");
        }
        Ok(())
    }

    async fn write_message(&self, message: &str) -> anyhow::Result<()> {
        self.ensure_open()?;
        if let Err(err) = self.backend.write_message(message).await {
            self.closed.store(true, Ordering::SeqCst);
            return Err(anyhow::anyhow!(err));
        }
        Ok(())
    }

    pub async fn ensure_initialized(&self, root_uri: &str) -> anyhow::Result<()> {
        self.ensure_open()?;
        if self.initialized.load(Ordering::SeqCst) {
            return Ok(());
        }
        let _guard = self.write_lock.lock().await;
        if self.initialized.load(Ordering::SeqCst) {
            return Ok(());
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        let init = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "initialize",
            "params": {
                "processId": null,
                "rootUri": root_uri,
                "capabilities": {
                    "textDocument": {
                        "hover": { "contentFormat": ["markdown", "plaintext"] },
                        "definition": {},
                        "references": {},
                        "rename": { "prepareSupport": true },
                        "formatting": {},
                        "rangeFormatting": {},
                        "codeAction": {
                            "codeActionLiteralSupport": {
                                "codeActionKind": {
                                    "valueSet": ["", "quickfix", "refactor", "source", "source.organizeImports"]
                                }
                            }
                        },
                        "documentSymbol": {},
                        "publishDiagnostics": {}
                    },
                    "workspace": {
                        "symbol": {},
                        "diagnostics": {}
                    }
                },
                "clientInfo": { "name": "medousa-code", "version": env!("CARGO_PKG_VERSION") }
            }
        });
        // Subscribe before sending so a fast language server cannot race its
        // initialize response past this listener.
        let mut rx = self.outbound.subscribe();
        self.write_message(&init.to_string()).await?;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        let capabilities = loop {
            self.ensure_open()?;
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                anyhow::bail!("language server initialize timed out");
            }
            match tokio::time::timeout(remaining.min(Duration::from_millis(100)), rx.recv()).await {
                Ok(Ok(msg)) => {
                    if let Ok(v) = serde_json::from_str::<Value>(&msg)
                        && v.get("id").and_then(|x| x.as_u64()) == Some(id)
                    {
                        if let Some(error) = v.get("error") {
                            anyhow::bail!("language server initialize failed: {error}");
                        }
                        break v
                            .pointer("/result/capabilities")
                            .cloned()
                            .unwrap_or(Value::Null);
                    }
                }
                Ok(Err(_)) => anyhow::bail!("language server session closed"),
                Err(_) => continue,
            }
        };
        *self.capabilities.write().await = capabilities;
        let initialized = json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {}
        });
        self.write_message(&initialized.to_string()).await?;
        self.initialized.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Send a JSON-RPC request and wait for the matching response.
    pub async fn request(&self, method: &str, params: Value) -> anyhow::Result<Value> {
        self.ensure_open()?;
        let id = self.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        let msg = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });
        let mut rx = self.outbound.subscribe();
        {
            let _guard = self.write_lock.lock().await;
            self.write_message(&msg.to_string()).await?;
        }
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            self.ensure_open()?;
            if remaining.is_zero() {
                anyhow::bail!("LSP request {method} timed out");
            }
            match tokio::time::timeout(remaining.min(Duration::from_millis(100)), rx.recv()).await {
                Ok(Ok(raw)) => {
                    let Ok(v) = serde_json::from_str::<Value>(&raw) else {
                        continue;
                    };
                    if v.get("id").and_then(|x| x.as_u64()) == Some(id) {
                        if let Some(err) = v.get("error") {
                            anyhow::bail!("LSP error: {err}");
                        }
                        return Ok(v.get("result").cloned().unwrap_or(Value::Null));
                    }
                }
                Ok(Err(_)) => anyhow::bail!("LSP session closed"),
                Err(_) => continue,
            }
        }
    }

    pub async fn write_raw(&self, json_body: &str) -> anyhow::Result<()> {
        let _guard = self.write_lock.lock().await;
        if let Ok(v) = serde_json::from_str::<Value>(json_body)
            && v.get("method").and_then(|m| m.as_str()) == Some("initialized")
        {
            self.initialized.store(true, Ordering::SeqCst);
        }
        self.write_message(json_body).await
    }
}

pub struct SessionPool {
    registry: ServerRegistry,
    sessions: RwLock<HashMap<SessionKey, Arc<LiveSession>>>,
}

impl SessionPool {
    pub fn new(registry: ServerRegistry) -> Arc<Self> {
        Arc::new(Self {
            registry,
            sessions: RwLock::new(HashMap::new()),
        })
    }

    pub fn registry(&self) -> &ServerRegistry {
        &self.registry
    }

    pub async fn get_or_spawn(
        &self,
        workspace_root: PathBuf,
        language: LanguageId,
    ) -> anyhow::Result<Arc<LiveSession>> {
        let key = SessionKey {
            workspace_root: workspace_root.clone(),
            language: language.clone(),
        };
        {
            let guard = self.sessions.read().await;
            if let Some(existing) = guard.get(&key).filter(|session| !session.is_closed()) {
                return Ok(Arc::clone(existing));
            }
        }
        {
            let mut guard = self.sessions.write().await;
            if guard.get(&key).is_some_and(|session| session.is_closed()) {
                guard.remove(&key);
            }
        }
        let spec = self
            .registry
            .get(&language)
            .ok_or_else(|| anyhow::anyhow!("no language server registered for {language}"))?
            .clone();
        let backend = spawn_backend(&spec, &workspace_root).await?;
        let (outbound, _) = broadcast::channel(256);
        let session = Arc::new(LiveSession {
            key: key.clone(),
            backend,
            outbound,
            diagnostics: RwLock::new(HashMap::new()),
            capabilities: RwLock::new(Value::Null),
            initialized: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            next_id: AtomicU64::new(1),
            write_lock: Mutex::new(()),
            reader_task: Mutex::new(None),
        });
        session.start_reader().await;
        let existing = {
            let mut guard = self.sessions.write().await;
            let existing = guard
                .get(&key)
                .filter(|session| !session.is_closed())
                .cloned();
            if existing.is_none() {
                guard.insert(key, Arc::clone(&session));
            }
            existing
        };
        if let Some(existing) = existing {
            session.backend.shutdown().await;
            return Ok(existing);
        }
        Ok(session)
    }

    pub async fn active_count(&self) -> usize {
        self.sessions.read().await.len()
    }

    pub async fn list_keys(&self) -> Vec<SessionKey> {
        self.sessions.read().await.keys().cloned().collect()
    }
}
