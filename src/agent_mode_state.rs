//! Persisted session selections and task-scoped agent-mode leases.

use std::collections::HashMap;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::daemon_api::{
    AgentModeId, AgentModeLeaseResponse, AgentModeScope, AgentModeSource, SessionAgentModeResponse,
    SetSessionAgentModeRequest,
};

const MAX_TRANSITIONS_PER_SESSION: usize = 100;
static MODE_STATE_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TaskModeLease {
    lease_id: String,
    task_id: String,
    mode: AgentModeId,
    acquired_at_utc: DateTime<Utc>,
    #[serde(default)]
    expires_at_utc: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct SessionModeState {
    #[serde(default)]
    selected_mode: Option<AgentModeId>,
    #[serde(default)]
    task_lease: Option<TaskModeLease>,
    #[serde(default)]
    revision: u64,
    #[serde(default)]
    updated_at_utc: Option<DateTime<Utc>>,
    #[serde(default)]
    transitions: Vec<ModeTransition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ModeTransition {
    transition_id: String,
    scope: AgentModeScope,
    #[serde(default)]
    from_mode: Option<AgentModeId>,
    #[serde(default)]
    to_mode: Option<AgentModeId>,
    occurred_at_utc: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ModeStateIndex {
    sessions: HashMap<String, SessionModeState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentModeSelection {
    pub mode: AgentModeId,
    pub source: AgentModeSource,
}

fn index_path() -> std::path::PathBuf {
    crate::session::medousa_data_dir().join("agent_mode_state.json")
}

fn read_index() -> ModeStateIndex {
    std::fs::read_to_string(index_path())
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn write_index(index: &ModeStateIndex) -> Result<(), String> {
    let path = index_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let json = serde_json::to_string_pretty(index).map_err(|err| err.to_string())?;
    crate::session::atomic_write(&path, json.as_bytes()).map_err(|err| err.to_string())
}

fn live_lease(state: &SessionModeState, now: DateTime<Utc>) -> Option<&TaskModeLease> {
    state
        .task_lease
        .as_ref()
        .filter(|lease| lease.expires_at_utc.is_none_or(|expires| expires > now))
}

fn selection_from_state(
    state: Option<&SessionModeState>,
    now: DateTime<Utc>,
) -> AgentModeSelection {
    if let Some(lease) = state.and_then(|state| live_lease(state, now)) {
        return AgentModeSelection {
            mode: lease.mode,
            source: AgentModeSource::Task,
        };
    }
    if let Some(mode) = state.and_then(|state| state.selected_mode) {
        return AgentModeSelection {
            mode,
            source: AgentModeSource::Session,
        };
    }
    AgentModeSelection {
        mode: AgentModeId::General,
        source: AgentModeSource::Default,
    }
}

fn select_mode(
    turn_override: Option<AgentModeId>,
    state: Option<&SessionModeState>,
    now: DateTime<Utc>,
) -> AgentModeSelection {
    turn_override.map_or_else(
        || selection_from_state(state, now),
        |mode| AgentModeSelection {
            mode,
            source: AgentModeSource::Turn,
        },
    )
}

pub fn resolve_for_turn(
    session_id: &str,
    turn_override: Option<AgentModeId>,
) -> AgentModeSelection {
    if let Some(mode) = turn_override {
        return AgentModeSelection {
            mode,
            source: AgentModeSource::Turn,
        };
    }
    let _guard = MODE_STATE_LOCK.lock().unwrap();
    let index = read_index();
    selection_from_state(index.sessions.get(session_id.trim()), Utc::now())
}

pub fn get_session_mode(session_id: &str) -> Result<SessionAgentModeResponse, String> {
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return Err("session_id is required".to_string());
    }
    let _guard = MODE_STATE_LOCK.lock().unwrap();
    let index = read_index();
    let state = index.sessions.get(session_id);
    let now = Utc::now();
    let selection = selection_from_state(state, now);
    Ok(SessionAgentModeResponse {
        session_id: session_id.to_string(),
        selected_mode: state.and_then(|value| value.selected_mode),
        task_lease: state
            .and_then(|value| live_lease(value, now))
            .map(TaskModeLease::to_response),
        effective_mode: selection.mode,
        effective_source: selection.source,
        revision: state.map_or(0, |value| value.revision),
        updated_at_utc: state.and_then(|value| value.updated_at_utc),
    })
}

pub fn set_session_mode(
    session_id: &str,
    request: SetSessionAgentModeRequest,
) -> Result<SessionAgentModeResponse, String> {
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return Err("session_id is required".to_string());
    }
    crate::agent_runtime::resolve_agent_mode(request.mode).map_err(|err| err.to_string())?;
    if request
        .expires_at_utc
        .is_some_and(|expires| expires <= Utc::now())
    {
        return Err("expires_at_utc must be in the future".to_string());
    }

    let _guard = MODE_STATE_LOCK.lock().unwrap();
    let mut index = read_index();
    let state = index.sessions.entry(session_id.to_string()).or_default();
    let now = Utc::now();
    let from_mode = match request.scope {
        AgentModeScope::Session => state.selected_mode,
        AgentModeScope::Task => state.task_lease.as_ref().map(|lease| lease.mode),
    };
    match request.scope {
        AgentModeScope::Session => state.selected_mode = Some(request.mode),
        AgentModeScope::Task => {
            let task_id = request
                .task_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "task_id is required for task-scoped mode".to_string())?;
            state.task_lease = Some(TaskModeLease {
                lease_id: Uuid::new_v4().to_string(),
                task_id: task_id.to_string(),
                mode: request.mode,
                acquired_at_utc: now,
                expires_at_utc: request.expires_at_utc,
            });
        }
    }
    record_transition(state, request.scope, from_mode, Some(request.mode), now);
    write_index(&index)?;
    drop(_guard);
    get_session_mode(session_id)
}

pub fn clear_session_mode(
    session_id: &str,
    scope: AgentModeScope,
) -> Result<SessionAgentModeResponse, String> {
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return Err("session_id is required".to_string());
    }
    let _guard = MODE_STATE_LOCK.lock().unwrap();
    let mut index = read_index();
    if let Some(state) = index.sessions.get_mut(session_id) {
        let now = Utc::now();
        let from_mode = match scope {
            AgentModeScope::Session => state.selected_mode.take(),
            AgentModeScope::Task => state.task_lease.take().map(|lease| lease.mode),
        };
        if from_mode.is_some() {
            record_transition(state, scope, from_mode, None, now);
            write_index(&index)?;
        }
    }
    drop(_guard);
    get_session_mode(session_id)
}

pub fn delete_session_mode_state(session_id: &str) {
    let _guard = MODE_STATE_LOCK.lock().unwrap();
    let mut index = read_index();
    if index.sessions.remove(session_id.trim()).is_some() {
        let _ = write_index(&index);
    }
}

fn record_transition(
    state: &mut SessionModeState,
    scope: AgentModeScope,
    from_mode: Option<AgentModeId>,
    to_mode: Option<AgentModeId>,
    now: DateTime<Utc>,
) {
    state.revision = state.revision.saturating_add(1);
    state.updated_at_utc = Some(now);
    state.transitions.push(ModeTransition {
        transition_id: Uuid::new_v4().to_string(),
        scope,
        from_mode,
        to_mode,
        occurred_at_utc: now,
    });
    if state.transitions.len() > MAX_TRANSITIONS_PER_SESSION {
        state
            .transitions
            .drain(..state.transitions.len() - MAX_TRANSITIONS_PER_SESSION);
    }
}

impl TaskModeLease {
    fn to_response(&self) -> AgentModeLeaseResponse {
        AgentModeLeaseResponse {
            lease_id: self.lease_id.clone(),
            task_id: self.task_id.clone(),
            mode: self.mode,
            acquired_at_utc: self.acquired_at_utc,
            expires_at_utc: self.expires_at_utc,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(session: Option<AgentModeId>, task: Option<AgentModeId>) -> SessionModeState {
        SessionModeState {
            selected_mode: session,
            task_lease: task.map(|mode| TaskModeLease {
                lease_id: "lease".into(),
                task_id: "task".into(),
                mode,
                acquired_at_utc: Utc::now(),
                expires_at_utc: None,
            }),
            ..Default::default()
        }
    }

    #[test]
    fn deterministic_precedence_is_turn_then_task_then_session_then_default() {
        let now = Utc::now();
        assert_eq!(
            selection_from_state(None, now).source,
            AgentModeSource::Default
        );
        assert_eq!(
            selection_from_state(Some(&state(Some(AgentModeId::Coder), None)), now).source,
            AgentModeSource::Session
        );
        assert_eq!(
            selection_from_state(
                Some(&state(Some(AgentModeId::General), Some(AgentModeId::Coder))),
                now,
            )
            .source,
            AgentModeSource::Task
        );
        assert_eq!(
            select_mode(
                Some(AgentModeId::General),
                Some(&state(Some(AgentModeId::Coder), Some(AgentModeId::Coder))),
                now,
            )
            .source,
            AgentModeSource::Turn
        );
    }

    #[test]
    fn expired_task_lease_falls_back_to_session() {
        let now = Utc::now();
        let mut value = state(Some(AgentModeId::General), Some(AgentModeId::Coder));
        value.task_lease.as_mut().unwrap().expires_at_utc =
            Some(now - chrono::Duration::seconds(1));
        let selection = selection_from_state(Some(&value), now);
        assert_eq!(selection.mode, AgentModeId::General);
        assert_eq!(selection.source, AgentModeSource::Session);
    }
}
