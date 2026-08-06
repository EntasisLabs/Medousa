//! Project-scoped diagnostics captured from transparent editor LSP sessions.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;
use serde_json::Value;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
struct DiagnosticEntry {
    revision: u64,
    params: Value,
}

#[derive(Debug)]
struct DiagnosticSession {
    workspace_root: PathBuf,
    language: String,
    documents: HashMap<String, DiagnosticEntry>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct WorkspaceDiagnosticDocument {
    pub uri: String,
    pub language: String,
    pub diagnostics: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct WorkspaceDiagnosticSnapshot {
    pub languages: Vec<String>,
    pub documents: Vec<WorkspaceDiagnosticDocument>,
}

/// Keeps editor-session diagnostics separate so closing one split cannot clear
/// a newer result published by another split of the same project and language.
#[derive(Debug, Default)]
pub struct WorkspaceDiagnosticStore {
    next_session_id: AtomicU64,
    next_revision: AtomicU64,
    sessions: RwLock<HashMap<u64, DiagnosticSession>>,
}

impl WorkspaceDiagnosticStore {
    pub fn new() -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self::default())
    }

    pub async fn begin_session(&self, workspace_root: PathBuf, language: String) -> u64 {
        let session_id = self.next_session_id.fetch_add(1, Ordering::SeqCst) + 1;
        self.sessions.write().await.insert(
            session_id,
            DiagnosticSession {
                workspace_root,
                language,
                documents: HashMap::new(),
            },
        );
        session_id
    }

    /// Record a `textDocument/publishDiagnostics` notification. Returns true
    /// only when the message was a valid diagnostic update for a live session.
    pub async fn record_message(&self, session_id: u64, raw: &str) -> bool {
        let Ok(value) = serde_json::from_str::<Value>(raw) else {
            return false;
        };
        if value.get("method").and_then(Value::as_str) != Some("textDocument/publishDiagnostics") {
            return false;
        }
        let Some(params) = value.get("params").filter(|value| value.is_object()) else {
            return false;
        };
        let Some(uri) = params
            .get("uri")
            .and_then(Value::as_str)
            .filter(|uri| !uri.is_empty())
        else {
            return false;
        };
        if !params.get("diagnostics").is_some_and(Value::is_array) {
            return false;
        }
        let revision = self.next_revision.fetch_add(1, Ordering::SeqCst) + 1;
        let mut sessions = self.sessions.write().await;
        let Some(session) = sessions.get_mut(&session_id) else {
            return false;
        };
        session.documents.insert(
            uri.to_string(),
            DiagnosticEntry {
                revision,
                params: params.clone(),
            },
        );
        true
    }

    pub async fn end_session(&self, session_id: u64) {
        self.sessions.write().await.remove(&session_id);
    }

    pub async fn snapshot(
        &self,
        workspace_root: &Path,
        language: Option<&str>,
    ) -> WorkspaceDiagnosticSnapshot {
        let sessions = self.sessions.read().await;
        let mut languages = BTreeSet::new();
        let mut latest = HashMap::<(String, String), DiagnosticEntry>::new();
        for session in sessions.values().filter(|session| {
            session.workspace_root == workspace_root
                && language.is_none_or(|language| session.language == language)
        }) {
            languages.insert(session.language.clone());
            for (uri, entry) in &session.documents {
                let key = (session.language.clone(), uri.clone());
                if latest
                    .get(&key)
                    .is_none_or(|current| current.revision < entry.revision)
                {
                    latest.insert(key, entry.clone());
                }
            }
        }
        let mut documents = latest
            .into_iter()
            .map(|((language, uri), entry)| WorkspaceDiagnosticDocument {
                diagnostics: entry
                    .params
                    .get("diagnostics")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default(),
                version: entry.params.get("version").and_then(Value::as_i64),
                uri,
                language,
            })
            .collect::<Vec<_>>();
        documents.sort_by(|left, right| {
            left.uri
                .cmp(&right.uri)
                .then_with(|| left.language.cmp(&right.language))
        });
        WorkspaceDiagnosticSnapshot {
            languages: languages.into_iter().collect(),
            documents,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn publish(uri: &str, message: &str, version: i64) -> String {
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": {
                "uri": uri,
                "version": version,
                "diagnostics": [{
                    "message": message,
                    "severity": 1,
                    "range": {
                        "start": { "line": 0, "character": 0 },
                        "end": { "line": 0, "character": 1 }
                    }
                }]
            }
        })
        .to_string()
    }

    #[tokio::test]
    async fn snapshot_keeps_projects_and_languages_isolated() {
        let store = WorkspaceDiagnosticStore::new();
        let rust = store
            .begin_session(PathBuf::from("/repo"), "rust".into())
            .await;
        let typescript = store
            .begin_session(PathBuf::from("/repo"), "typescript".into())
            .await;
        let other = store
            .begin_session(PathBuf::from("/other"), "rust".into())
            .await;
        assert!(
            store
                .record_message(rust, &publish("file:///repo/src/lib.rs", "rust issue", 1))
                .await
        );
        assert!(
            store
                .record_message(
                    typescript,
                    &publish("file:///repo/src/app.ts", "ts issue", 2),
                )
                .await
        );
        assert!(
            store
                .record_message(other, &publish("file:///other/lib.rs", "other", 1))
                .await
        );

        let snapshot = store.snapshot(Path::new("/repo"), None).await;
        assert_eq!(snapshot.languages, vec!["rust", "typescript"]);
        assert_eq!(snapshot.documents.len(), 2);
        assert_eq!(snapshot.documents[0].uri, "file:///repo/src/app.ts");
        assert_eq!(snapshot.documents[1].uri, "file:///repo/src/lib.rs");
    }

    #[tokio::test]
    async fn newest_split_wins_and_closing_it_reveals_the_remaining_split() {
        let store = WorkspaceDiagnosticStore::new();
        let first = store
            .begin_session(PathBuf::from("/repo"), "rust".into())
            .await;
        let second = store
            .begin_session(PathBuf::from("/repo"), "rust".into())
            .await;
        store
            .record_message(first, &publish("file:///repo/lib.rs", "first", 1))
            .await;
        store
            .record_message(second, &publish("file:///repo/lib.rs", "second", 2))
            .await;

        let snapshot = store.snapshot(Path::new("/repo"), Some("rust")).await;
        assert_eq!(snapshot.documents[0].diagnostics[0]["message"], "second");
        assert_eq!(snapshot.documents[0].version, Some(2));

        store.end_session(second).await;
        let snapshot = store.snapshot(Path::new("/repo"), Some("rust")).await;
        assert_eq!(snapshot.documents[0].diagnostics[0]["message"], "first");
    }

    #[tokio::test]
    async fn invalid_messages_and_closed_sessions_are_ignored() {
        let store = WorkspaceDiagnosticStore::new();
        let session = store
            .begin_session(PathBuf::from("/repo"), "rust".into())
            .await;
        assert!(!store.record_message(session, "not json").await);
        assert!(
            !store
                .record_message(session, r#"{"method":"window/logMessage"}"#)
                .await
        );
        store.end_session(session).await;
        assert!(
            !store
                .record_message(session, &publish("file:///repo/lib.rs", "late", 1))
                .await
        );
        assert!(
            store
                .snapshot(Path::new("/repo"), None)
                .await
                .documents
                .is_empty()
        );
    }
}
