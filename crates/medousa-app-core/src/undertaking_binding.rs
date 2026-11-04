//! Undertaking ↔ shell-group ↔ chat/terminal session binding.
//!
//! Forge item lists/leases stay daemon authority. This module owns the
//! per-pane active undertaking projection and move-not-duplicate session
//! membership so Home and headless clients share one map.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[cfg(feature = "json-schema")]
use schemars::JsonSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(transparent)]
pub struct UndertakingBindingRevision(pub u64);

impl UndertakingBindingRevision {
    pub fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

/// Active undertaking projection for one shell group (pane).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct UndertakingContextFact {
    pub work_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_runtime_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_transport_runtime_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_id: Option<String>,
    pub title: String,
    pub human_phase: String,
    pub forge_state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_oid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sealed_oid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease_generation: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executor_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attempt_seq: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bound_chat_session_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bound_terminal_session_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct UndertakingGroupBinding {
    pub group_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<UndertakingContextFact>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct UndertakingBindingSnapshot {
    pub workshop_scope_id: String,
    pub revision: UndertakingBindingRevision,
    pub contexts: Vec<UndertakingGroupBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UndertakingBindingCommand {
    Reset {
        workshop_scope_id: String,
    },
    SetActive {
        group_id: String,
        context: UndertakingContextFact,
    },
    ClearActive {
        group_id: String,
    },
    /// Move `session_id` into `group_id` (remove from every other group).
    BindChat {
        session_id: String,
        group_id: String,
    },
    /// Detach `session_id` from every group.
    DetachChat {
        session_id: String,
    },
    BindTerminal {
        session_id: String,
        group_id: String,
    },
    Snapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UndertakingBindingUpdate {
    Snapshot {
        snapshot: UndertakingBindingSnapshot,
    },
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum UndertakingBindingCoreError {
    #[error("group `{0}` has no active undertaking")]
    GroupInactive(String),
    #[error("invalid group id")]
    InvalidGroupId,
    #[error("invalid session id")]
    InvalidSessionId,
}

#[derive(Debug, Clone, Default)]
pub struct UndertakingBindingCore {
    workshop_scope_id: String,
    revision: UndertakingBindingRevision,
    contexts: BTreeMap<String, Option<UndertakingContextFact>>,
}

impl UndertakingBindingCore {
    pub fn new(workshop_scope_id: impl Into<String>) -> Self {
        Self {
            workshop_scope_id: workshop_scope_id.into(),
            revision: UndertakingBindingRevision::default(),
            contexts: BTreeMap::new(),
        }
    }

    pub fn snapshot(&self) -> UndertakingBindingSnapshot {
        UndertakingBindingSnapshot {
            workshop_scope_id: self.workshop_scope_id.clone(),
            revision: self.revision,
            contexts: self
                .contexts
                .iter()
                .map(|(group_id, context)| UndertakingGroupBinding {
                    group_id: group_id.clone(),
                    context: context.clone(),
                })
                .collect(),
        }
    }

    fn bump(&mut self) {
        self.revision = self.revision.next();
    }

    pub fn dispatch(
        &mut self,
        command: UndertakingBindingCommand,
    ) -> Result<UndertakingBindingUpdate, UndertakingBindingCoreError> {
        match command {
            UndertakingBindingCommand::Reset { workshop_scope_id } => {
                self.workshop_scope_id = workshop_scope_id;
                self.contexts.clear();
                self.bump();
            }
            UndertakingBindingCommand::SetActive { group_id, context } => {
                let group_id = normalize_id(&group_id)
                    .ok_or(UndertakingBindingCoreError::InvalidGroupId)?;
                let mut next = context;
                if let Some(Some(prev)) = self.contexts.get(&group_id) {
                    if prev.work_id == next.work_id {
                        if next.execution_runtime_id.is_none() {
                            next.execution_runtime_id = prev.execution_runtime_id.clone();
                        }
                        if next.execution_transport_runtime_id.is_none() {
                            next.execution_transport_runtime_id =
                                prev.execution_transport_runtime_id.clone();
                        }
                        if next.repo_id.is_none() {
                            next.repo_id = prev.repo_id.clone();
                        }
                        if next.bound_chat_session_ids.is_empty() {
                            next.bound_chat_session_ids = prev.bound_chat_session_ids.clone();
                        }
                        if next.bound_terminal_session_ids.is_empty() {
                            next.bound_terminal_session_ids =
                                prev.bound_terminal_session_ids.clone();
                        }
                        if next.sealed_oid.is_none() {
                            next.sealed_oid = prev.sealed_oid.clone();
                        }
                        if next.executor_kind.is_none()
                            && next.lease_id.is_some()
                            && prev.executor_kind.is_some()
                        {
                            next.executor_kind = prev.executor_kind.clone();
                        }
                    }
                }
                self.contexts.insert(group_id, Some(next));
                self.bump();
            }
            UndertakingBindingCommand::ClearActive { group_id } => {
                let group_id = normalize_id(&group_id)
                    .ok_or(UndertakingBindingCoreError::InvalidGroupId)?;
                self.contexts.insert(group_id, None);
                self.bump();
            }
            UndertakingBindingCommand::BindChat {
                session_id,
                group_id,
            } => {
                let session_id = normalize_id(&session_id)
                    .ok_or(UndertakingBindingCoreError::InvalidSessionId)?;
                let group_id = normalize_id(&group_id)
                    .ok_or(UndertakingBindingCoreError::InvalidGroupId)?;
                if self.contexts.get(&group_id).and_then(|c| c.as_ref()).is_none() {
                    return Err(UndertakingBindingCoreError::GroupInactive(group_id));
                }
                self.move_chat_binding(&session_id, Some(&group_id));
                self.bump();
            }
            UndertakingBindingCommand::DetachChat { session_id } => {
                let session_id = normalize_id(&session_id)
                    .ok_or(UndertakingBindingCoreError::InvalidSessionId)?;
                self.move_chat_binding(&session_id, None);
                self.bump();
            }
            UndertakingBindingCommand::BindTerminal {
                session_id,
                group_id,
            } => {
                let session_id = normalize_id(&session_id)
                    .ok_or(UndertakingBindingCoreError::InvalidSessionId)?;
                let group_id = normalize_id(&group_id)
                    .ok_or(UndertakingBindingCoreError::InvalidGroupId)?;
                let entry = self
                    .contexts
                    .get_mut(&group_id)
                    .and_then(|c| c.as_mut())
                    .ok_or_else(|| UndertakingBindingCoreError::GroupInactive(group_id))?;
                if !entry.bound_terminal_session_ids.iter().any(|id| id == &session_id) {
                    entry.bound_terminal_session_ids.push(session_id);
                }
                self.bump();
            }
            UndertakingBindingCommand::Snapshot => {}
        }
        Ok(UndertakingBindingUpdate::Snapshot {
            snapshot: self.snapshot(),
        })
    }

    fn move_chat_binding(&mut self, session_id: &str, target_group: Option<&str>) {
        for (group_id, slot) in self.contexts.iter_mut() {
            let Some(context) = slot.as_mut() else {
                continue;
            };
            let should_bind = target_group == Some(group_id.as_str());
            let bound = context
                .bound_chat_session_ids
                .iter()
                .any(|id| id == session_id);
            if should_bind == bound {
                continue;
            }
            if should_bind {
                context.bound_chat_session_ids.push(session_id.to_string());
            } else {
                context
                    .bound_chat_session_ids
                    .retain(|id| id != session_id);
            }
        }
    }
}

fn normalize_id(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
