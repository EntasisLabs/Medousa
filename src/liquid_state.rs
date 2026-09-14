//! Daemon-owned durable state for inert Liquid component instances.

use std::path::PathBuf;
use std::sync::RwLock;

use medousa_types::{
    LiquidComponentStateRecord, LiquidComponentStateResponse, PutLiquidComponentStateRequest,
};
use once_cell::sync::Lazy;
use sha2::{Digest, Sha256};

use crate::store_root::StorePath;

const MAX_STATE_BYTES: usize = 32 * 1024;
static ROOT: Lazy<RwLock<Option<PathBuf>>> = Lazy::new(|| RwLock::new(None));
static WRITES: Lazy<RwLock<()>> = Lazy::new(|| RwLock::new(()));

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiquidStateError {
    Invalid(String),
    Conflict { expected: Option<u64>, actual: u64 },
    Store(String),
}

impl std::fmt::Display for LiquidStateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(message) | Self::Store(message) => formatter.write_str(message),
            Self::Conflict { expected, actual } => write!(
                formatter,
                "Liquid component state revision conflict: expected {expected:?}, current {actual}"
            ),
        }
    }
}

pub fn configure_root(root: impl Into<PathBuf>) -> Result<(), String> {
    *ROOT
        .write()
        .map_err(|_| "Liquid state root lock poisoned".to_string())? = Some(root.into());
    Ok(())
}

fn store() -> crate::session_storage::SessionDirectoryStore {
    let root = ROOT
        .read()
        .ok()
        .and_then(|root| root.as_ref().cloned())
        .unwrap_or_else(|| crate::paths::medousa_data_dir().join("liquid-state"));
    crate::session_storage::SessionDirectoryStore::new(root)
}

fn bounded_id(name: &str, value: &str) -> Result<String, LiquidStateError> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > 512 || trimmed.chars().any(char::is_control) {
        return Err(LiquidStateError::Invalid(format!("{name} is invalid")));
    }
    Ok(trimmed.to_string())
}

fn path(message_id: &str, node_id: &str, instance_id: &str) -> StorePath {
    let mut hash = Sha256::new();
    for value in [message_id, node_id, instance_id] {
        hash.update(value.as_bytes());
        hash.update([0]);
    }
    StorePath::parse(&format!("{:x}.json", hash.finalize())).expect("sha256 path is confined")
}

pub fn get(
    session_id: &str,
    message_id: &str,
    node_id: &str,
    instance_id: &str,
) -> Result<LiquidComponentStateResponse, LiquidStateError> {
    let session_id = crate::session_storage::SessionId::parse(session_id)
        .map_err(|error| LiquidStateError::Invalid(error.to_string()))?;
    let message_id = bounded_id("message_id", message_id)?;
    let node_id = bounded_id("node_id", node_id)?;
    let instance_id = bounded_id("instance_id", instance_id)?;
    let relative = path(&message_id, &node_id, &instance_id);
    let store = store();
    if !store
        .is_file(&session_id, &relative)
        .map_err(|error| LiquidStateError::Store(error.to_string()))?
    {
        return Ok(LiquidComponentStateResponse { state: None });
    }
    let bytes = store
        .read_limited(&session_id, &relative, MAX_STATE_BYTES as u64)
        .map_err(|error| LiquidStateError::Store(error.to_string()))?;
    let record = serde_json::from_slice::<LiquidComponentStateRecord>(&bytes)
        .map_err(|error| LiquidStateError::Store(format!("Liquid state is corrupt: {error}")))?;
    Ok(LiquidComponentStateResponse {
        state: Some(record),
    })
}

pub fn put(
    session_id: &str,
    message_id: &str,
    node_id: &str,
    instance_id: &str,
    request: PutLiquidComponentStateRequest,
) -> Result<LiquidComponentStateResponse, LiquidStateError> {
    let _guard = WRITES
        .write()
        .map_err(|_| LiquidStateError::Store("Liquid state write lock poisoned".to_string()))?;
    let session_id = crate::session_storage::SessionId::parse(session_id)
        .map_err(|error| LiquidStateError::Invalid(error.to_string()))?;
    let message_id = bounded_id("message_id", message_id)?;
    let node_id = bounded_id("node_id", node_id)?;
    let instance_id = bounded_id("instance_id", instance_id)?;
    let component_type = bounded_id("component_type", &request.component_type)?;
    if request.schema_version == 0 {
        return Err(LiquidStateError::Invalid(
            "schema_version must be positive".to_string(),
        ));
    }
    let state_bytes = serde_json::to_vec(&request.state)
        .map_err(|error| LiquidStateError::Invalid(error.to_string()))?;
    if state_bytes.len() > MAX_STATE_BYTES {
        return Err(LiquidStateError::Invalid(format!(
            "component state exceeds {MAX_STATE_BYTES} bytes"
        )));
    }
    let relative = path(&message_id, &node_id, &instance_id);
    let store = store();
    let existing = if store
        .is_file(&session_id, &relative)
        .map_err(|error| LiquidStateError::Store(error.to_string()))?
    {
        let bytes = store
            .read_limited(&session_id, &relative, MAX_STATE_BYTES as u64)
            .map_err(|error| LiquidStateError::Store(error.to_string()))?;
        Some(
            serde_json::from_slice::<LiquidComponentStateRecord>(&bytes).map_err(|error| {
                LiquidStateError::Store(format!("Liquid state is corrupt: {error}"))
            })?,
        )
    } else {
        None
    };
    let actual = existing.as_ref().map_or(0, |record| record.revision);
    if request.expected_revision.unwrap_or(0) != actual {
        return Err(LiquidStateError::Conflict {
            expected: request.expected_revision,
            actual,
        });
    }
    let record = LiquidComponentStateRecord {
        schema_version: request.schema_version,
        session_id: session_id.to_string(),
        message_id,
        node_id,
        instance_id,
        component_type,
        revision: actual + 1,
        state: request.state,
        updated_at_utc: chrono::Utc::now(),
    };
    let bytes = serde_json::to_vec_pretty(&record)
        .map_err(|error| LiquidStateError::Store(error.to_string()))?;
    store
        .atomic_write(&session_id, &relative, &bytes)
        .map_err(|error| LiquidStateError::Store(error.to_string()))?;
    Ok(LiquidComponentStateResponse {
        state: Some(record),
    })
}

pub fn delete_for_session(session_id: &str) -> Result<(), String> {
    let session_id =
        crate::session_storage::SessionId::parse(session_id).map_err(|error| error.to_string())?;
    store()
        .remove_session(&session_id)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn persists_and_rejects_stale_revisions() {
        let temp = tempfile::tempdir().unwrap();
        configure_root(temp.path()).unwrap();
        let request = PutLiquidComponentStateRequest {
            schema_version: 1,
            component_type: "timer".into(),
            expected_revision: Some(0),
            state: json!({"status":"running","deadlineAt":123}),
        };
        let first = put("session-a", "message-a", "recipe", "step-1", request).unwrap();
        assert_eq!(first.state.as_ref().unwrap().revision, 1);
        let stale = put(
            "session-a",
            "message-a",
            "recipe",
            "step-1",
            PutLiquidComponentStateRequest {
                schema_version: 1,
                component_type: "timer".into(),
                expected_revision: Some(0),
                state: json!({"status":"paused"}),
            },
        );
        assert!(matches!(
            stale,
            Err(LiquidStateError::Conflict { actual: 1, .. })
        ));
        assert_eq!(
            get("session-a", "message-a", "recipe", "step-1")
                .unwrap()
                .state
                .unwrap()
                .state["status"],
            "running"
        );
    }
}
