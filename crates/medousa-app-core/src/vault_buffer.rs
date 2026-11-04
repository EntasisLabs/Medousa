//! Vault note buffer application core — unsaved identity + dirty reconcile.
//!
//! Filesystem bytes and version tokens stay daemon authority. This module owns
//! path-keyed in-memory buffers (multi-pane stash), dirty vs baseline, and
//! conflict detection when disk changes under a dirty edit.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[cfg(feature = "json-schema")]
use schemars::JsonSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(transparent)]
pub struct VaultBufferRevision(pub u64);

impl VaultBufferRevision {
    pub fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

/// Path-keyed note buffer (chat sessionRuntime analogue for vault panes).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct NoteBufferFact {
    pub path: String,
    pub content: String,
    pub baseline_content: String,
    /// If-Match token: opaque note_version when available, else digest hash.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    pub title: String,
    pub dirty: bool,
    pub content_revision: u64,
    /// Disk changed under a dirty local edit — host should surface conflict UI.
    #[serde(default)]
    pub conflict: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct VaultBufferSnapshot {
    pub workshop_scope_id: String,
    pub revision: VaultBufferRevision,
    pub buffers: Vec<NoteBufferFact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VaultBufferCommand {
    Reset {
        workshop_scope_id: String,
    },
    /// Insert or replace a buffer from disk / absolute file / test seed.
    SeedBuffer {
        buffer: NoteBufferFact,
    },
    /// Apply an editor edit. Marks dirty when content ≠ baseline.
    ApplyEdit {
        path: String,
        content: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
    },
    /// Successful save — content becomes the new baseline.
    MarkSaved {
        path: String,
        content: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        content_hash: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
    },
    /// Disk/agent bytes arrived. Clean buffers adopt; dirty buffers conflict.
    ReconcileFromDisk {
        path: String,
        content: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        content_hash: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
    },
    DisposeBuffer {
        path: String,
    },
    Snapshot {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        path: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VaultBufferUpdate {
    Snapshot { snapshot: VaultBufferSnapshot },
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum VaultBufferCoreError {
    #[error("buffer `{0}` not found")]
    BufferNotFound(String),
    #[error("invalid vault buffer path")]
    InvalidPath,
}

#[derive(Debug, Clone, Default)]
pub struct VaultBufferCore {
    workshop_scope_id: String,
    revision: VaultBufferRevision,
    buffers: BTreeMap<String, NoteBufferFact>,
}

impl VaultBufferCore {
    pub fn new(workshop_scope_id: impl Into<String>) -> Self {
        Self {
            workshop_scope_id: workshop_scope_id.into(),
            revision: VaultBufferRevision(0),
            buffers: BTreeMap::new(),
        }
    }

    pub fn workshop_scope_id(&self) -> &str {
        &self.workshop_scope_id
    }

    pub fn revision(&self) -> VaultBufferRevision {
        self.revision
    }

    pub fn get(&self, path: &str) -> Option<&NoteBufferFact> {
        self.buffers.get(path)
    }

    pub fn snapshot(&self, path: Option<&str>) -> VaultBufferSnapshot {
        let buffers = if let Some(path) = path {
            self.buffers
                .get(path)
                .cloned()
                .into_iter()
                .collect()
        } else {
            self.buffers.values().cloned().collect()
        };
        VaultBufferSnapshot {
            workshop_scope_id: self.workshop_scope_id.clone(),
            revision: self.revision,
            buffers,
        }
    }

    pub fn dispatch(
        &mut self,
        command: VaultBufferCommand,
    ) -> Result<VaultBufferUpdate, VaultBufferCoreError> {
        match command {
            VaultBufferCommand::Reset { workshop_scope_id } => {
                *self = Self::new(workshop_scope_id);
                self.ok_snapshot(None)
            }
            VaultBufferCommand::SeedBuffer { buffer } => {
                let path = normalize_path(&buffer.path)?;
                let mut next = buffer;
                next.path = path.clone();
                next.dirty = next.content != next.baseline_content || next.dirty;
                if next.content == next.baseline_content {
                    next.dirty = false;
                    next.conflict = false;
                }
                let prev_rev = self.buffers.get(&path).map(|b| b.content_revision).unwrap_or(0);
                if next.content_revision <= prev_rev {
                    next.content_revision = prev_rev.saturating_add(1);
                }
                self.buffers.insert(path, next);
                self.bump();
                self.ok_snapshot(None)
            }
            VaultBufferCommand::ApplyEdit {
                path,
                content,
                title,
            } => {
                let key = normalize_path(&path)?;
                let buffer = self
                    .buffers
                    .get_mut(&key)
                    .ok_or_else(|| VaultBufferCoreError::BufferNotFound(key.clone()))?;
                if buffer.content == content
                    && title
                        .as_ref()
                        .is_none_or(|t| t == &buffer.title)
                {
                    return self.ok_snapshot(Some(&key));
                }
                buffer.content = content;
                buffer.dirty = buffer.content != buffer.baseline_content;
                if let Some(title) = title {
                    buffer.title = title;
                }
                buffer.content_revision = buffer.content_revision.saturating_add(1);
                if !buffer.dirty {
                    buffer.conflict = false;
                }
                self.bump();
                self.ok_snapshot(Some(&key))
            }
            VaultBufferCommand::MarkSaved {
                path,
                content,
                content_hash,
                title,
            } => {
                let key = normalize_path(&path)?;
                let buffer = self
                    .buffers
                    .get_mut(&key)
                    .ok_or_else(|| VaultBufferCoreError::BufferNotFound(key.clone()))?;
                buffer.content = content.clone();
                buffer.baseline_content = content;
                buffer.content_hash = content_hash;
                if let Some(title) = title {
                    buffer.title = title;
                }
                buffer.dirty = false;
                buffer.conflict = false;
                buffer.content_revision = buffer.content_revision.saturating_add(1);
                self.bump();
                self.ok_snapshot(Some(&key))
            }
            VaultBufferCommand::ReconcileFromDisk {
                path,
                content,
                content_hash,
                title,
            } => {
                let key = normalize_path(&path)?;
                if let Some(buffer) = self.buffers.get_mut(&key) {
                    if !buffer.dirty {
                        buffer.content = content.clone();
                        buffer.baseline_content = content;
                        buffer.content_hash = content_hash;
                        if let Some(title) = title {
                            buffer.title = title;
                        }
                        buffer.conflict = false;
                        buffer.content_revision = buffer.content_revision.saturating_add(1);
                    } else if content != buffer.content {
                        // Keep local edits; refresh If-Match + flag conflict.
                        buffer.content_hash = content_hash;
                        if let Some(title) = title {
                            buffer.title = title;
                        }
                        buffer.conflict = true;
                        buffer.content_revision = buffer.content_revision.saturating_add(1);
                    } else {
                        // Dirty but matches disk text — treat as clean adoption.
                        buffer.baseline_content = content;
                        buffer.content_hash = content_hash;
                        buffer.dirty = false;
                        buffer.conflict = false;
                        buffer.content_revision = buffer.content_revision.saturating_add(1);
                    }
                } else {
                    let title = title.unwrap_or_default();
                    self.buffers.insert(
                        key.clone(),
                        NoteBufferFact {
                            path: key.clone(),
                            content: content.clone(),
                            baseline_content: content,
                            content_hash,
                            title,
                            dirty: false,
                            content_revision: 1,
                            conflict: false,
                        },
                    );
                }
                self.bump();
                self.ok_snapshot(Some(&key))
            }
            VaultBufferCommand::DisposeBuffer { path } => {
                let key = normalize_path(&path)?;
                self.buffers.remove(&key);
                self.bump();
                self.ok_snapshot(None)
            }
            VaultBufferCommand::Snapshot { path } => {
                self.ok_snapshot(path.as_deref())
            }
        }
    }

    fn bump(&mut self) {
        self.revision = self.revision.next();
    }

    fn ok_snapshot(
        &self,
        path: Option<&str>,
    ) -> Result<VaultBufferUpdate, VaultBufferCoreError> {
        Ok(VaultBufferUpdate::Snapshot {
            snapshot: self.snapshot(path),
        })
    }
}

fn normalize_path(path: &str) -> Result<String, VaultBufferCoreError> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(VaultBufferCoreError::InvalidPath);
    }
    Ok(trimmed.to_string())
}
