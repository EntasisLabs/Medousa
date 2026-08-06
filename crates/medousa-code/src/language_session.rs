//! Bounded lifecycle, progress, and log history for workshop language sessions.

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value;
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

const LOG_CAPACITY: usize = 256;
const SESSION_CAPACITY: usize = 64;
const PROGRESS_CAPACITY: usize = 32;

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LanguageSessionKind {
    Editor,
    Agent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LanguageSessionPhase {
    Starting,
    Initializing,
    Ready,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct LanguageSessionLogEntry {
    pub sequence: u64,
    pub timestamp_ms: u64,
    pub level: String,
    pub source: String,
    pub message: String,
}

/// A process can keep writing stderr while the session record changes state, so
/// logs live in their own shared bounded buffer.
#[derive(Debug, Default)]
pub struct LanguageSessionLog {
    next_sequence: AtomicU64,
    entries: Mutex<VecDeque<LanguageSessionLogEntry>>,
}

impl LanguageSessionLog {
    pub async fn push(
        &self,
        level: impl Into<String>,
        source: impl Into<String>,
        message: impl Into<String>,
    ) {
        let message = message.into();
        if message.trim().is_empty() {
            return;
        }
        let entry = LanguageSessionLogEntry {
            sequence: self.next_sequence.fetch_add(1, Ordering::Relaxed) + 1,
            timestamp_ms: now_millis(),
            level: level.into(),
            source: source.into(),
            message,
        };
        let mut entries = self.entries.lock().await;
        entries.push_back(entry);
        while entries.len() > LOG_CAPACITY {
            entries.pop_front();
        }
    }

    async fn snapshot(&self) -> Vec<LanguageSessionLogEntry> {
        self.entries.lock().await.iter().cloned().collect()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LanguageSessionProgress {
    pub token: String,
    pub title: String,
    pub message: String,
    pub percentage: Option<f64>,
    pub done: bool,
}

#[derive(Debug, Clone)]
pub struct LanguageSessionIdentity {
    pub kind: LanguageSessionKind,
    pub project_root: PathBuf,
    pub language_root: PathBuf,
    pub language: String,
}

#[derive(Debug)]
struct LanguageSessionMutable {
    phase: LanguageSessionPhase,
    detail: String,
    updated_at_ms: u64,
    progress: BTreeMap<String, LanguageSessionProgress>,
}

#[derive(Debug)]
struct LanguageSessionRecord {
    id: String,
    identity: LanguageSessionIdentity,
    started_at_ms: u64,
    mutable: Mutex<LanguageSessionMutable>,
    logs: Arc<LanguageSessionLog>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LanguageSessionSnapshot {
    pub id: String,
    pub kind: LanguageSessionKind,
    pub language: String,
    pub project_root: String,
    pub language_root: String,
    pub relative_root: String,
    pub phase: LanguageSessionPhase,
    pub detail: String,
    pub started_at_ms: u64,
    pub updated_at_ms: u64,
    pub progress: Vec<LanguageSessionProgress>,
    pub logs: Vec<LanguageSessionLogEntry>,
}

#[derive(Clone, Debug)]
pub struct LanguageSessionHandle {
    record: Arc<LanguageSessionRecord>,
}

impl LanguageSessionHandle {
    pub fn id(&self) -> &str {
        &self.record.id
    }

    pub fn logs(&self) -> Arc<LanguageSessionLog> {
        Arc::clone(&self.record.logs)
    }

    pub async fn starting(&self, detail: impl Into<String>) {
        self.set_phase(LanguageSessionPhase::Starting, detail).await;
    }

    pub async fn initializing(&self, detail: impl Into<String>) {
        self.set_phase(LanguageSessionPhase::Initializing, detail)
            .await;
    }

    pub async fn ready(&self, detail: impl Into<String>) {
        self.set_phase(LanguageSessionPhase::Ready, detail).await;
    }

    pub async fn failed(&self, detail: impl Into<String>) {
        let detail = detail.into();
        self.record
            .logs
            .push("error", "lifecycle", detail.clone())
            .await;
        self.set_phase(LanguageSessionPhase::Failed, detail).await;
    }

    pub async fn stopped(&self, detail: impl Into<String>) {
        let mut mutable = self.record.mutable.lock().await;
        if mutable.phase == LanguageSessionPhase::Failed {
            return;
        }
        mutable.phase = LanguageSessionPhase::Stopped;
        mutable.detail = detail.into();
        mutable.updated_at_ms = now_millis();
    }

    async fn set_phase(&self, phase: LanguageSessionPhase, detail: impl Into<String>) {
        let mut mutable = self.record.mutable.lock().await;
        mutable.phase = phase;
        mutable.detail = detail.into();
        mutable.updated_at_ms = now_millis();
    }

    pub async fn record_lsp_message(&self, raw: &str) {
        let Ok(value) = serde_json::from_str::<Value>(raw) else {
            return;
        };
        let Some(method) = value.get("method").and_then(Value::as_str) else {
            return;
        };
        match method {
            "window/logMessage" | "window/showMessage" => {
                let message = value
                    .pointer("/params/message")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let level = match value.pointer("/params/type").and_then(Value::as_u64) {
                    Some(1) => "error",
                    Some(2) => "warning",
                    Some(3) => "info",
                    _ => "log",
                };
                self.record.logs.push(level, "lsp", message).await;
            }
            "$/progress" => self.record_progress(&value).await,
            _ => {}
        }
    }

    pub async fn create_progress(&self, token: &Value) {
        let token = progress_token(token);
        let mut mutable = self.record.mutable.lock().await;
        mutable.progress.insert(
            token.clone(),
            LanguageSessionProgress {
                token,
                title: String::new(),
                message: String::new(),
                percentage: None,
                done: false,
            },
        );
        trim_progress(&mut mutable.progress);
        mutable.updated_at_ms = now_millis();
    }

    async fn record_progress(&self, value: &Value) {
        let token = progress_token(value.pointer("/params/token").unwrap_or(&Value::Null));
        let payload = value.pointer("/params/value").unwrap_or(&Value::Null);
        let kind = payload
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("report");
        let mut mutable = self.record.mutable.lock().await;
        let progress =
            mutable
                .progress
                .entry(token.clone())
                .or_insert_with(|| LanguageSessionProgress {
                    token,
                    title: String::new(),
                    message: String::new(),
                    percentage: None,
                    done: false,
                });
        if let Some(title) = payload.get("title").and_then(Value::as_str) {
            progress.title = title.to_string();
        }
        if let Some(message) = payload.get("message").and_then(Value::as_str) {
            progress.message = message.to_string();
        }
        if let Some(percentage) = payload.get("percentage").and_then(Value::as_f64) {
            progress.percentage = Some(percentage.clamp(0.0, 100.0));
        }
        progress.done = kind == "end";
        trim_progress(&mut mutable.progress);
        mutable.updated_at_ms = now_millis();
    }

    async fn snapshot(&self) -> LanguageSessionSnapshot {
        let mutable = self.record.mutable.lock().await;
        let relative_root = self
            .record
            .identity
            .language_root
            .strip_prefix(&self.record.identity.project_root)
            .unwrap_or(Path::new(""))
            .to_string_lossy()
            .replace('\\', "/");
        LanguageSessionSnapshot {
            id: self.record.id.clone(),
            kind: self.record.identity.kind,
            language: self.record.identity.language.clone(),
            project_root: self
                .record
                .identity
                .project_root
                .to_string_lossy()
                .into_owned(),
            language_root: self
                .record
                .identity
                .language_root
                .to_string_lossy()
                .into_owned(),
            relative_root,
            phase: mutable.phase,
            detail: mutable.detail.clone(),
            started_at_ms: self.record.started_at_ms,
            updated_at_ms: mutable.updated_at_ms,
            progress: mutable.progress.values().cloned().collect(),
            logs: self.record.logs.snapshot().await,
        }
    }
}

fn progress_token(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| value.to_string())
}

fn trim_progress(progress: &mut BTreeMap<String, LanguageSessionProgress>) {
    while progress.len() > PROGRESS_CAPACITY {
        let Some(key) = progress.keys().next().cloned() else {
            break;
        };
        progress.remove(&key);
    }
}

#[derive(Debug, Default)]
pub struct LanguageSessionStore {
    records: RwLock<VecDeque<LanguageSessionHandle>>,
}

impl LanguageSessionStore {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub async fn begin(&self, identity: LanguageSessionIdentity) -> LanguageSessionHandle {
        let now = now_millis();
        let handle = LanguageSessionHandle {
            record: Arc::new(LanguageSessionRecord {
                id: Uuid::new_v4().to_string(),
                identity,
                started_at_ms: now,
                mutable: Mutex::new(LanguageSessionMutable {
                    phase: LanguageSessionPhase::Starting,
                    detail: "Starting language server".into(),
                    updated_at_ms: now,
                    progress: BTreeMap::new(),
                }),
                logs: Arc::new(LanguageSessionLog::default()),
            }),
        };
        let mut records = self.records.write().await;
        records.push_back(handle.clone());
        while records.len() > SESSION_CAPACITY {
            records.pop_front();
        }
        handle
    }

    pub async fn snapshots(
        &self,
        project_root: &Path,
        language: Option<&str>,
        language_root: Option<&Path>,
    ) -> Vec<LanguageSessionSnapshot> {
        let handles = self
            .records
            .read()
            .await
            .iter()
            .filter(|handle| {
                handle.record.identity.project_root == project_root
                    && language.is_none_or(|language| {
                        handle
                            .record
                            .identity
                            .language
                            .eq_ignore_ascii_case(language)
                    })
                    && language_root.is_none_or(|root| handle.record.identity.language_root == root)
            })
            .cloned()
            .collect::<Vec<_>>();
        let mut snapshots = Vec::with_capacity(handles.len());
        for handle in handles {
            snapshots.push(handle.snapshot().await);
        }
        snapshots.sort_by_key(|snapshot| std::cmp::Reverse(snapshot.started_at_ms));
        snapshots
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn lifecycle_keeps_bounded_logs_and_progress() {
        let store = LanguageSessionStore::new();
        let handle = store
            .begin(LanguageSessionIdentity {
                kind: LanguageSessionKind::Editor,
                project_root: PathBuf::from("/repo"),
                language_root: PathBuf::from("/repo/packages/app"),
                language: "typescript".into(),
            })
            .await;
        handle.initializing("Initializing").await;
        handle
            .record_lsp_message(
                r#"{"jsonrpc":"2.0","method":"window/logMessage","params":{"type":2,"message":"indexing"}}"#,
            )
            .await;
        handle
            .record_lsp_message(
                r#"{"jsonrpc":"2.0","method":"$/progress","params":{"token":"scan","value":{"kind":"begin","title":"Index","percentage":25}}}"#,
            )
            .await;
        handle.ready("Ready").await;

        let snapshots = store
            .snapshots(
                Path::new("/repo"),
                Some("typescript"),
                Some(Path::new("/repo/packages/app")),
            )
            .await;
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].phase, LanguageSessionPhase::Ready);
        assert_eq!(snapshots[0].relative_root, "packages/app");
        assert_eq!(snapshots[0].logs[0].level, "warning");
        assert_eq!(snapshots[0].progress[0].percentage, Some(25.0));
    }

    #[tokio::test]
    async fn failure_is_not_erased_by_cleanup() {
        let store = LanguageSessionStore::new();
        let handle = store
            .begin(LanguageSessionIdentity {
                kind: LanguageSessionKind::Agent,
                project_root: PathBuf::from("/repo"),
                language_root: PathBuf::from("/repo"),
                language: "rust".into(),
            })
            .await;
        handle.failed("server exited").await;
        handle.stopped("request released").await;
        let snapshot = store.snapshots(Path::new("/repo"), None, None).await;
        assert_eq!(snapshot[0].phase, LanguageSessionPhase::Failed);
        assert_eq!(snapshot[0].detail, "server exited");
    }
}
