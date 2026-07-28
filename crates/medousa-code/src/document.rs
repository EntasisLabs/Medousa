//! Orchestrator-authoritative document versions for multi-client sync.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct DocumentState {
    pub uri: String,
    pub language_id: String,
    pub version: i32,
    pub text: String,
}

#[derive(Debug, Default)]
pub struct DocumentStore {
    inner: RwLock<HashMap<String, DocumentState>>,
}

impl DocumentStore {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub async fn open(&self, uri: String, language_id: String, text: String, version: i32) {
        let mut guard = self.inner.write().await;
        guard.insert(
            uri.clone(),
            DocumentState {
                uri,
                language_id,
                version,
                text,
            },
        );
    }

    pub async fn change(&self, uri: &str, text: String, version: i32) -> bool {
        let mut guard = self.inner.write().await;
        let Some(doc) = guard.get_mut(uri) else {
            return false;
        };
        if version < doc.version {
            return false;
        }
        doc.version = version;
        doc.text = text;
        true
    }

    pub async fn close(&self, uri: &str) {
        self.inner.write().await.remove(uri);
    }

    pub async fn get(&self, uri: &str) -> Option<DocumentState> {
        self.inner.read().await.get(uri).cloned()
    }

    pub async fn list_uris(&self) -> Vec<String> {
        self.inner.read().await.keys().cloned().collect()
    }
}
