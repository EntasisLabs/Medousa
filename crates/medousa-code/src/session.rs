//! Session pool: one language-server backend per governed project, resolved
//! language root, and language.
//!
//! HTTP agent callers share a backend; outbound broadcast fans notifications
//! (diagnostics, etc.) to every subscribed internal request listener.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use tokio::sync::{Mutex, RwLock, broadcast};

use crate::backend::{LanguageServerBackend, spawn_backend};
use crate::registry::{LanguageId, ServerRegistry};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionKey {
    pub project_root: PathBuf,
    pub language_root: PathBuf,
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
    last_used_millis: AtomicU64,
    active_requests: AtomicUsize,
    next_id: AtomicU64,
    write_lock: Mutex<()>,
    reader_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

pub(crate) fn initialization_options(language: &LanguageId) -> Value {
    if language.as_str() != "rust" {
        return Value::Null;
    }
    json!({
        "cachePriming": { "enable": false },
        "cargo": {
            "allTargets": false,
            "autoreload": false,
            "buildScripts": { "enable": false }
        },
        "checkOnSave": false,
        "lru": { "capacity": 64 },
        "numThreads": 2,
        "procMacro": { "enable": false }
    })
}

struct ActiveRequest<'a>(&'a LiveSession);

impl Drop for ActiveRequest<'_> {
    fn drop(&mut self) {
        self.0.active_requests.fetch_sub(1, Ordering::SeqCst);
        self.0.touch();
    }
}

impl LiveSession {
    fn touch(&self) {
        self.last_used_millis.store(now_millis(), Ordering::Relaxed);
    }

    fn idle_for(&self) -> Duration {
        Duration::from_millis(
            now_millis().saturating_sub(self.last_used_millis.load(Ordering::Relaxed)),
        )
    }

    fn is_idle(&self, max_idle: Duration) -> bool {
        self.active_requests.load(Ordering::SeqCst) == 0 && self.idle_for() >= max_idle
    }

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
        self.touch();
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
                "initializationOptions": initialization_options(&self.key.language),
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
        self.touch();
        self.active_requests.fetch_add(1, Ordering::SeqCst);
        let _active_request = ActiveRequest(self);
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
        self.touch();
        let _guard = self.write_lock.lock().await;
        if let Ok(v) = serde_json::from_str::<Value>(json_body)
            && v.get("method").and_then(|m| m.as_str()) == Some("initialized")
        {
            self.initialized.store(true, Ordering::SeqCst);
        }
        self.write_message(json_body).await
    }

    async fn shutdown(&self) {
        if self.closed.swap(true, Ordering::SeqCst) {
            return;
        }
        self.backend.shutdown().await;
        if let Some(reader) = self.reader_task.lock().await.take() {
            reader.abort();
        }
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
        project_root: PathBuf,
        language_root: PathBuf,
        language: LanguageId,
    ) -> anyhow::Result<Arc<LiveSession>> {
        let key = SessionKey {
            project_root,
            language_root: language_root.clone(),
            language: language.clone(),
        };
        {
            let guard = self.sessions.read().await;
            if let Some(existing) = guard.get(&key).filter(|session| !session.is_closed()) {
                existing.touch();
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
        let backend = spawn_backend(&spec, &language_root).await?;
        let (outbound, _) = broadcast::channel(256);
        let session = Arc::new(LiveSession {
            key: key.clone(),
            backend,
            outbound,
            diagnostics: RwLock::new(HashMap::new()),
            capabilities: RwLock::new(Value::Null),
            initialized: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            last_used_millis: AtomicU64::new(now_millis()),
            active_requests: AtomicUsize::new(0),
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

    pub async fn get_existing(
        &self,
        project_root: PathBuf,
        language_root: PathBuf,
        language: LanguageId,
    ) -> Option<Arc<LiveSession>> {
        let key = SessionKey {
            project_root,
            language_root,
            language,
        };
        self.sessions
            .read()
            .await
            .get(&key)
            .filter(|session| !session.is_closed())
            .map(|session| {
                session.touch();
                Arc::clone(session)
            })
    }

    pub async fn existing_for_workspace(&self, project_root: &Path) -> Vec<Arc<LiveSession>> {
        self.sessions
            .read()
            .await
            .values()
            .filter(|session| !session.is_closed() && session.key.project_root == project_root)
            .map(|session| {
                session.touch();
                Arc::clone(session)
            })
            .collect()
    }

    pub async fn existing_for_workspace_language(
        &self,
        project_root: &Path,
        language: &LanguageId,
    ) -> Vec<Arc<LiveSession>> {
        self.sessions
            .read()
            .await
            .values()
            .filter(|session| {
                !session.is_closed()
                    && session.key.project_root == project_root
                    && &session.key.language == language
            })
            .map(|session| {
                session.touch();
                Arc::clone(session)
            })
            .collect()
    }

    pub async fn shutdown_idle(&self, max_idle: Duration) -> usize {
        let stale = {
            let mut sessions = self.sessions.write().await;
            let keys = sessions
                .iter()
                .filter(|(_, session)| session.is_closed() || session.is_idle(max_idle))
                .map(|(key, _)| key.clone())
                .collect::<Vec<_>>();
            keys.into_iter()
                .filter_map(|key| sessions.remove(&key))
                .collect::<Vec<_>>()
        };
        let count = stale.len();
        for session in stale {
            session.shutdown().await;
        }
        count
    }

    pub async fn shutdown_all(&self) {
        let sessions = {
            let mut guard = self.sessions.write().await;
            guard
                .drain()
                .map(|(_, session)| session)
                .collect::<Vec<_>>()
        };
        for session in sessions {
            session.shutdown().await;
        }
    }

    pub async fn active_count(&self) -> usize {
        self.sessions
            .read()
            .await
            .values()
            .filter(|session| !session.is_closed())
            .count()
    }

    pub async fn list_keys(&self) -> Vec<SessionKey> {
        self.sessions.read().await.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_sessions_start_with_a_bounded_workload() {
        let options = initialization_options(&LanguageId::new("rust"));
        assert_eq!(options.pointer("/cachePriming/enable"), Some(&json!(false)));
        assert_eq!(
            options.pointer("/cargo/buildScripts/enable"),
            Some(&json!(false))
        );
        assert_eq!(options.pointer("/procMacro/enable"), Some(&json!(false)));
        assert_eq!(options.pointer("/checkOnSave"), Some(&json!(false)));
        assert_eq!(options.pointer("/lru/capacity"), Some(&json!(64)));
    }

    #[test]
    fn other_language_servers_keep_their_native_defaults() {
        assert_eq!(
            initialization_options(&LanguageId::new("typescript")),
            Value::Null
        );
    }
}
