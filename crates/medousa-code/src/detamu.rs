//! Detamu bridge hooks (M5) — observers share Orchestrator handles later.
//!
//! Detamu is the versioned world model; it must not sit in the keystroke path.
//! When wired, Detamu may:
//! - request a snapshot of open document URIs / versions
//! - subscribe to file-event notifications from Forge worktrees
//! - obtain a server-session handle id for offline graph ingest
//!
//! Scores exposed to Medousa APIs must use `code_avec` (never bare `avec`).

use serde::{Deserialize, Serialize};

/// Opaque handle Detamu can hold without owning the LSP process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetamuServerHandle {
    pub workspace_root: String,
    pub language: String,
    pub session_key: String,
}

/// Snapshot of Orchestrator document state for Detamu observers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetamuDocumentSnapshot {
    pub uri: String,
    pub language_id: String,
    pub version: i32,
}

/// Placeholder observer API — returns empty until Detamu integration lands.
pub trait DetamuObserver: Send + Sync {
    fn on_documents_changed(&self, _docs: &[DetamuDocumentSnapshot]) {}
    fn on_session_ready(&self, _handle: &DetamuServerHandle) {}
}

/// No-op observer used until Detamu is linked.
#[derive(Debug, Default)]
pub struct NullDetamuObserver;

impl DetamuObserver for NullDetamuObserver {}
