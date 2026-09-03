//! Persisted session selections and task-scoped agent-mode leases.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, RwLock};

use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::daemon_api::{
    AgentModeAutoAccept, AgentModeId, AgentModeLeaseResponse, AgentModeProposalListResponse,
    AgentModeProposalResolution, AgentModeProposalResponse, AgentModeProposalStatus,
    AgentModeScope, AgentModeSource, AgentModeTransitionPolicy, SessionAgentModeResponse,
    SessionCodeBindingResponse, SetSessionAgentModeRequest,
};

const MAX_TRANSITIONS_PER_SESSION: usize = 100;
const MAX_PROPOSALS_PER_SESSION: usize = 100;
pub const MIN_PROPOSAL_TTL_SECONDS: u64 = 5;
pub const MAX_PROPOSAL_TTL_SECONDS: u64 = 86_400;
static MODE_STATE_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));
static MODE_STATE_PATH: Lazy<RwLock<Option<PathBuf>>> = Lazy::new(|| RwLock::new(None));

fn validate_agent_mode(mode: AgentModeId) -> Result<(), String> {
    #[cfg(feature = "full-daemon")]
    {
        crate::agent_runtime::resolve_agent_mode(mode)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
    #[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
    {
        match mode {
            AgentModeId::General | AgentModeId::Teacher | AgentModeId::Instant => Ok(()),
            AgentModeId::Coder => Err(
                "Coder mode requires a workshop host with project, Forge, and shell authority"
                    .to_string(),
            ),
        }
    }
}

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
    #[serde(default)]
    proposals: Vec<ModeProposal>,
    #[serde(default)]
    bound_work_id: Option<String>,
    #[serde(default)]
    code_binding_updated_at_utc: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ModeProposal {
    proposal_id: String,
    session_id: String,
    from_mode: AgentModeId,
    to_mode: AgentModeId,
    scope: AgentModeScope,
    #[serde(default)]
    task_id: Option<String>,
    reason: String,
    status: AgentModeProposalStatus,
    #[serde(default)]
    resolution: Option<AgentModeProposalResolution>,
    created_at_utc: DateTime<Utc>,
    expires_at_utc: DateTime<Utc>,
    #[serde(default)]
    resolved_at_utc: Option<DateTime<Utc>>,
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
    #[serde(default)]
    transition_policy: AgentModeTransitionPolicy,
    sessions: HashMap<String, SessionModeState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentModeSelection {
    pub mode: AgentModeId,
    pub source: AgentModeSource,
}

fn index_path() -> std::path::PathBuf {
    if let Some(path) = MODE_STATE_PATH
        .read()
        .ok()
        .and_then(|path| path.as_ref().cloned())
    {
        return path;
    }
    crate::session::medousa_data_dir().join("agent_mode_state.json")
}

pub fn configure_agent_mode_state_path(path: impl Into<PathBuf>) -> Result<(), String> {
    let _guard = MODE_STATE_LOCK
        .lock()
        .map_err(|_| "agent mode state lock poisoned".to_string())?;
    *MODE_STATE_PATH
        .write()
        .map_err(|_| "agent mode state path lock poisoned".to_string())? = Some(path.into());
    Ok(())
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

#[cfg(test)]
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
    let (_session, _mutation) = crate::session_deletion::acquire_mutation_for_str(session_id)?;
    validate_agent_mode(request.mode)?;
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
    apply_mode(
        state,
        request.mode,
        request.scope,
        request.task_id.as_deref(),
        request.expires_at_utc,
        now,
    )?;
    write_index(&index)?;
    drop(_guard);
    get_session_mode(session_id)
}

pub fn get_transition_policy() -> AgentModeTransitionPolicy {
    let _guard = MODE_STATE_LOCK.lock().unwrap();
    read_index().transition_policy
}

pub fn session_ids_with_code_binding() -> Vec<String> {
    let _guard = MODE_STATE_LOCK.lock().unwrap();
    read_index()
        .sessions
        .iter()
        .filter(|(_, state)| state.bound_work_id.is_some())
        .map(|(session_id, _)| session_id.clone())
        .collect()
}

pub fn get_session_code_binding(session_id: &str) -> Result<SessionCodeBindingResponse, String> {
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return Err("session_id is required".to_string());
    }
    let _guard = MODE_STATE_LOCK.lock().unwrap();
    let index = read_index();
    let state = index.sessions.get(session_id);
    Ok(SessionCodeBindingResponse {
        session_id: session_id.to_string(),
        work_id: state.and_then(|value| value.bound_work_id.clone()),
        updated_at_utc: state.and_then(|value| value.code_binding_updated_at_utc),
    })
}

pub fn set_session_code_binding(
    session_id: &str,
    work_id: &str,
) -> Result<SessionCodeBindingResponse, String> {
    let session_id = session_id.trim();
    let work_id = work_id.trim();
    if session_id.is_empty() || work_id.is_empty() {
        return Err("session_id and work_id are required".to_string());
    }
    let (_session, _mutation) = crate::session_deletion::acquire_mutation_for_str(session_id)?;
    let _guard = MODE_STATE_LOCK.lock().unwrap();
    let mut index = read_index();
    let state = index.sessions.entry(session_id.to_string()).or_default();
    if state.bound_work_id.as_deref() == Some(work_id) {
        let response = SessionCodeBindingResponse {
            session_id: session_id.to_string(),
            work_id: state.bound_work_id.clone(),
            updated_at_utc: state.code_binding_updated_at_utc,
        };
        drop(_guard);
        crate::session_catalog::mark_has_code_work(session_id);
        return Ok(response);
    }
    let now = Utc::now();
    state.bound_work_id = Some(work_id.to_string());
    state.code_binding_updated_at_utc = Some(now);
    state.revision = state.revision.saturating_add(1);
    state.updated_at_utc = Some(now);
    write_index(&index)?;
    drop(_guard);
    crate::session_catalog::mark_has_code_work(session_id);
    Ok(SessionCodeBindingResponse {
        session_id: session_id.to_string(),
        work_id: Some(work_id.to_string()),
        updated_at_utc: Some(now),
    })
}

pub fn clear_session_code_binding(session_id: &str) -> Result<SessionCodeBindingResponse, String> {
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return Err("session_id is required".to_string());
    }
    let (_session, _mutation) = crate::session_deletion::acquire_mutation_for_str(session_id)?;
    let _guard = MODE_STATE_LOCK.lock().unwrap();
    let mut index = read_index();
    let state = index.sessions.entry(session_id.to_string()).or_default();
    if state.bound_work_id.is_none() {
        return Ok(SessionCodeBindingResponse {
            session_id: session_id.to_string(),
            work_id: None,
            updated_at_utc: state.code_binding_updated_at_utc,
        });
    }
    let now = Utc::now();
    state.bound_work_id = None;
    state.code_binding_updated_at_utc = Some(now);
    state.revision = state.revision.saturating_add(1);
    state.updated_at_utc = Some(now);
    write_index(&index)?;
    Ok(SessionCodeBindingResponse {
        session_id: session_id.to_string(),
        work_id: None,
        updated_at_utc: Some(now),
    })
}

pub fn set_transition_policy(
    policy: AgentModeTransitionPolicy,
) -> Result<AgentModeTransitionPolicy, String> {
    if !(MIN_PROPOSAL_TTL_SECONDS..=MAX_PROPOSAL_TTL_SECONDS).contains(&policy.proposal_ttl_seconds)
    {
        return Err(format!(
            "proposal_ttl_seconds must be between {MIN_PROPOSAL_TTL_SECONDS} and {MAX_PROPOSAL_TTL_SECONDS}"
        ));
    }
    let _guard = MODE_STATE_LOCK.lock().unwrap();
    let mut index = read_index();
    index.transition_policy = policy.clone();
    write_index(&index)?;
    Ok(policy)
}

pub fn propose_mode_transition(
    session_id: &str,
    to_mode: AgentModeId,
    scope: AgentModeScope,
    task_id: Option<&str>,
    reason: &str,
) -> Result<AgentModeProposalResponse, String> {
    let session_id = session_id.trim();
    let reason = reason.trim();
    if session_id.is_empty() {
        return Err("session_id is required".to_string());
    }
    if reason.is_empty() {
        return Err("reason is required".to_string());
    }
    let (_session, _mutation) = crate::session_deletion::acquire_mutation_for_str(session_id)?;
    validate_agent_mode(to_mode)?;
    let task_id = normalize_task_id(scope, task_id)?;

    let _guard = MODE_STATE_LOCK.lock().unwrap();
    let mut index = read_index();
    let policy = index.transition_policy.clone();
    let state = index.sessions.entry(session_id.to_string()).or_default();
    let now = Utc::now();
    expire_proposals(state, now);
    let from_mode = selection_from_state(Some(state), now).mode;
    if from_mode == to_mode {
        return Err(format!("{to_mode:?} mode is already active"));
    }
    let auto_accept = policy_auto_accepts(policy.auto_accept, scope);
    let mut proposal = ModeProposal {
        proposal_id: Uuid::new_v4().to_string(),
        session_id: session_id.to_string(),
        from_mode,
        to_mode,
        scope,
        task_id: task_id.clone(),
        reason: reason.chars().take(500).collect(),
        status: AgentModeProposalStatus::Pending,
        resolution: None,
        created_at_utc: now,
        expires_at_utc: now + chrono::Duration::seconds(policy.proposal_ttl_seconds as i64),
        resolved_at_utc: None,
    };
    if auto_accept {
        apply_mode(state, to_mode, scope, task_id.as_deref(), None, now)?;
        proposal.status = AgentModeProposalStatus::Accepted;
        proposal.resolution = Some(AgentModeProposalResolution::AutoAccepted);
        proposal.resolved_at_utc = Some(now);
    }
    state.proposals.push(proposal.clone());
    trim_proposals(state);
    write_index(&index)?;
    Ok(proposal.to_response())
}

pub fn list_mode_proposals(session_id: &str) -> Result<AgentModeProposalListResponse, String> {
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return Err("session_id is required".to_string());
    }
    let (_session, _mutation) = crate::session_deletion::acquire_mutation_for_str(session_id)?;
    let _guard = MODE_STATE_LOCK.lock().unwrap();
    let mut index = read_index();
    let now = Utc::now();
    let changed = index
        .sessions
        .get_mut(session_id)
        .is_some_and(|state| expire_proposals(state, now));
    if changed {
        write_index(&index)?;
    }
    let proposals = index
        .sessions
        .get(session_id)
        .map(|state| {
            state
                .proposals
                .iter()
                .rev()
                .map(ModeProposal::to_response)
                .collect()
        })
        .unwrap_or_default();
    Ok(AgentModeProposalListResponse { proposals })
}

pub fn decide_mode_proposal(
    session_id: &str,
    proposal_id: &str,
    accept: bool,
) -> Result<AgentModeProposalResponse, String> {
    let session_id = session_id.trim();
    let proposal_id = proposal_id.trim();
    if session_id.is_empty() || proposal_id.is_empty() {
        return Err("session_id and proposal_id are required".to_string());
    }
    let (_session, _mutation) = crate::session_deletion::acquire_mutation_for_str(session_id)?;
    let _guard = MODE_STATE_LOCK.lock().unwrap();
    let mut index = read_index();
    let state = index
        .sessions
        .get_mut(session_id)
        .ok_or_else(|| "mode proposal not found".to_string())?;
    let now = Utc::now();
    expire_proposals(state, now);
    let position = state
        .proposals
        .iter()
        .position(|proposal| proposal.proposal_id == proposal_id)
        .ok_or_else(|| "mode proposal not found".to_string())?;
    if state.proposals[position].status != AgentModeProposalStatus::Pending {
        let response = state.proposals[position].to_response();
        write_index(&index)?;
        return Ok(response);
    }
    if accept {
        let proposal = state.proposals[position].clone();
        apply_mode(
            state,
            proposal.to_mode,
            proposal.scope,
            proposal.task_id.as_deref(),
            None,
            now,
        )?;
        state.proposals[position].status = AgentModeProposalStatus::Accepted;
        state.proposals[position].resolution = Some(AgentModeProposalResolution::UserAccepted);
    } else {
        state.proposals[position].status = AgentModeProposalStatus::Denied;
        state.proposals[position].resolution = Some(AgentModeProposalResolution::UserDenied);
    }
    state.proposals[position].resolved_at_utc = Some(now);
    let response = state.proposals[position].to_response();
    write_index(&index)?;
    Ok(response)
}

fn apply_mode(
    state: &mut SessionModeState,
    mode: AgentModeId,
    scope: AgentModeScope,
    task_id: Option<&str>,
    expires_at_utc: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Result<(), String> {
    let from_mode = match scope {
        AgentModeScope::Session => state.selected_mode,
        AgentModeScope::Task => state.task_lease.as_ref().map(|lease| lease.mode),
    };
    match scope {
        AgentModeScope::Session => state.selected_mode = Some(mode),
        AgentModeScope::Task => {
            let task_id =
                normalize_task_id(scope, task_id)?.expect("task scope normalization returns an id");
            state.task_lease = Some(TaskModeLease {
                lease_id: Uuid::new_v4().to_string(),
                task_id,
                mode,
                acquired_at_utc: now,
                expires_at_utc,
            });
        }
    }
    record_transition(state, scope, from_mode, Some(mode), now);
    Ok(())
}

fn normalize_task_id(
    scope: AgentModeScope,
    task_id: Option<&str>,
) -> Result<Option<String>, String> {
    let task_id = task_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    if scope == AgentModeScope::Task && task_id.is_none() {
        return Err("task_id is required for task-scoped mode".to_string());
    }
    Ok(task_id)
}

fn policy_auto_accepts(policy: AgentModeAutoAccept, scope: AgentModeScope) -> bool {
    match policy {
        AgentModeAutoAccept::Never => false,
        AgentModeAutoAccept::Task => scope == AgentModeScope::Task,
        AgentModeAutoAccept::All => true,
    }
}

fn expire_proposals(state: &mut SessionModeState, now: DateTime<Utc>) -> bool {
    let mut changed = false;
    for proposal in &mut state.proposals {
        if proposal.status == AgentModeProposalStatus::Pending && proposal.expires_at_utc <= now {
            proposal.status = AgentModeProposalStatus::Expired;
            proposal.resolution = Some(AgentModeProposalResolution::Expired);
            proposal.resolved_at_utc = Some(now);
            changed = true;
        }
    }
    changed
}

fn trim_proposals(state: &mut SessionModeState) {
    if state.proposals.len() > MAX_PROPOSALS_PER_SESSION {
        state
            .proposals
            .drain(..state.proposals.len() - MAX_PROPOSALS_PER_SESSION);
    }
}

pub fn clear_session_mode(
    session_id: &str,
    scope: AgentModeScope,
) -> Result<SessionAgentModeResponse, String> {
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return Err("session_id is required".to_string());
    }
    let (_session, _mutation) = crate::session_deletion::acquire_mutation_for_str(session_id)?;
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

pub fn delete_session_mode_state(session_id: &str) -> Result<(), String> {
    let _guard = MODE_STATE_LOCK.lock().unwrap();
    let mut index = read_index();
    if index.sessions.remove(session_id.trim()).is_some() {
        write_index(&index)?;
    }
    if read_index().sessions.contains_key(session_id.trim()) {
        return Err("agent mode state remains after deletion".to_string());
    }
    Ok(())
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

impl ModeProposal {
    fn to_response(&self) -> AgentModeProposalResponse {
        AgentModeProposalResponse {
            proposal_id: self.proposal_id.clone(),
            session_id: self.session_id.clone(),
            from_mode: self.from_mode,
            to_mode: self.to_mode,
            scope: self.scope,
            task_id: self.task_id.clone(),
            reason: self.reason.clone(),
            status: self.status,
            resolution: self.resolution,
            created_at_utc: self.created_at_utc,
            expires_at_utc: self.expires_at_utc,
            resolved_at_utc: self.resolved_at_utc,
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

    #[test]
    fn proposal_expiry_is_a_denial_without_changing_mode() {
        let now = Utc::now();
        let mut value = state(Some(AgentModeId::General), None);
        value.proposals.push(ModeProposal {
            proposal_id: "proposal".into(),
            session_id: "session".into(),
            from_mode: AgentModeId::General,
            to_mode: AgentModeId::Coder,
            scope: AgentModeScope::Session,
            task_id: None,
            reason: "Repository work".into(),
            status: AgentModeProposalStatus::Pending,
            resolution: None,
            created_at_utc: now - chrono::Duration::seconds(31),
            expires_at_utc: now - chrono::Duration::seconds(1),
            resolved_at_utc: None,
        });

        assert!(expire_proposals(&mut value, now));
        assert_eq!(value.proposals[0].status, AgentModeProposalStatus::Expired);
        assert_eq!(
            value.proposals[0].resolution,
            Some(AgentModeProposalResolution::Expired)
        );
        assert_eq!(
            selection_from_state(Some(&value), now).mode,
            AgentModeId::General
        );
    }

    #[test]
    fn task_scope_requires_a_task_id() {
        assert!(normalize_task_id(AgentModeScope::Task, None).is_err());
        assert_eq!(
            normalize_task_id(AgentModeScope::Session, None).expect("session scope"),
            None
        );
    }

    #[test]
    fn auto_accept_policy_only_matches_its_configured_scope() {
        assert!(!policy_auto_accepts(
            AgentModeAutoAccept::Never,
            AgentModeScope::Task
        ));
        assert!(!policy_auto_accepts(
            AgentModeAutoAccept::Task,
            AgentModeScope::Session
        ));
        assert!(policy_auto_accepts(
            AgentModeAutoAccept::Task,
            AgentModeScope::Task
        ));
        assert!(policy_auto_accepts(
            AgentModeAutoAccept::All,
            AgentModeScope::Session
        ));
    }
}
