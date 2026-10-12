//! Session ledger for wake grants, durable open gaps, and approvals.
//!
//! The grant is replaced on every turn so a peer wake cannot leak onto the
//! next human turn. Gaps and approvals are written to disk and reloaded after
//! a daemon restart. The tool loop reads [`hooks_for`]; the registry refuses
//! calls the grant does not allow.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use async_trait::async_trait;
use genai::chat::Tool;
use medousa_runtime::WakeLoopHooks;
use medousa_types::wake::{
    OpenGap, OpenGapKind, WakeGrant, WakeSource, is_approval_utterance, is_material_tool,
    is_safety_review_spawn, review_blocked_by_approval, tool_refusal,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use stasis::application::orchestration::tool_registry::ToolRegistry;
use stasis::domain::errors::StasisError;
use stasis::prelude::Result;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct DiskSession {
    #[serde(default)]
    gaps: Vec<OpenGap>,
    #[serde(default)]
    approvals: Vec<medousa_types::wake::WorkApproval>,
    #[serde(default)]
    generation: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct DiskLedger {
    #[serde(default)]
    sessions: HashMap<String, DiskSession>,
}

struct SessionState {
    grant: Option<WakeGrant>,
    disk: DiskSession,
}

struct Ledger {
    sessions: HashMap<String, SessionState>,
    dirty: bool,
}

fn ledger() -> &'static Mutex<Ledger> {
    static LEDGER: OnceLock<Mutex<Ledger>> = OnceLock::new();
    LEDGER.get_or_init(|| {
        let disk = load_disk();
        let sessions = disk
            .sessions
            .into_iter()
            .map(|(id, disk)| (id, SessionState { grant: None, disk }))
            .collect();
        Mutex::new(Ledger {
            sessions,
            dirty: false,
        })
    })
}

fn ledger_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("MEDOUSA_WAKE_LEDGER_PATH") {
        let path = PathBuf::from(path);
        if !path.as_os_str().is_empty() {
            return Some(path);
        }
    }
    Some(crate::paths::medousa_data_dir().join("wake_ledger.json"))
}

fn load_disk() -> DiskLedger {
    let Some(path) = ledger_path() else {
        return DiskLedger::default();
    };
    let Ok(raw) = fs::read_to_string(&path) else {
        return DiskLedger::default();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

fn persist(ledger: &Ledger) {
    if !ledger.dirty {
        return;
    }
    let Some(path) = ledger_path() else {
        return;
    };
    let disk = DiskLedger {
        sessions: ledger
            .sessions
            .iter()
            .map(|(id, state)| (id.clone(), state.disk.clone()))
            .collect(),
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(raw) = serde_json::to_string_pretty(&disk) {
        let _ = fs::write(path, raw);
    }
}

fn with_session<T>(session_id: &str, update: impl FnOnce(&mut SessionState) -> T) -> T {
    let mut guard = ledger().lock().unwrap_or_else(|err| err.into_inner());
    let state = guard
        .sessions
        .entry(session_id.to_string())
        .or_insert_with(|| SessionState {
            grant: None,
            disk: DiskSession::default(),
        });
    let value = update(state);
    guard.dirty = true;
    persist(&guard);
    value
}

pub fn begin_turn(
    session_id: &str,
    wake: Option<WakeGrant>,
    prompt: &str,
    persist_user_turn: bool,
) {
    with_session(session_id, |state| {
        if persist_user_turn && is_approval_utterance(prompt) {
            record_approval_locked(state, "human");
        }
        state.grant = wake;
    });
}

pub fn install_grant(session_id: &str, grant: WakeGrant) {
    with_session(session_id, |state| {
        state.grant = Some(grant);
    });
}

pub fn current_grant(session_id: &str) -> Option<WakeGrant> {
    let guard = ledger().lock().unwrap_or_else(|err| err.into_inner());
    guard
        .sessions
        .get(session_id)
        .and_then(|state| state.grant.clone())
}

pub fn register_gap(session_id: &str, gap: OpenGap) {
    with_session(session_id, |state| {
        if let Some(existing) = state.disk.gaps.iter_mut().find(|item| item.id == gap.id) {
            *existing = gap;
        } else {
            state.disk.gaps.push(gap);
        }
    });
}

pub fn close_gap(session_id: &str, id: &str) {
    let mut guard = ledger().lock().unwrap_or_else(|err| err.into_inner());
    let Some(state) = guard.sessions.get_mut(session_id) else {
        return;
    };
    let before = state.disk.gaps.len();
    state.disk.gaps.retain(|gap| gap.id != id);
    if state.disk.gaps.len() != before {
        guard.dirty = true;
        persist(&guard);
    }
}

pub fn gap_lines(session_id: &str) -> Vec<String> {
    let guard = ledger().lock().unwrap_or_else(|err| err.into_inner());
    guard
        .sessions
        .get(session_id)
        .map(|state| state.disk.gaps.iter().map(OpenGap::line).collect())
        .unwrap_or_default()
}

fn record_approval_locked(state: &mut SessionState, approved_by: &str) {
    let digest = state.disk.generation.to_string();
    let pending: Vec<_> = state
        .disk
        .gaps
        .iter()
        .filter(|gap| gap.kind == OpenGapKind::PendingApproval)
        .cloned()
        .collect();
    let targets = if pending.is_empty() {
        vec![("session".to_string(), "session".to_string())]
    } else {
        pending
            .iter()
            .map(|gap| (gap.id.clone(), gap.id.clone()))
            .collect()
    };
    for (work_id, question_id) in targets {
        state
            .disk
            .approvals
            .retain(|approval| approval.question_id != question_id);
        state
            .disk
            .approvals
            .push(medousa_types::wake::WorkApproval {
                work_id,
                question_id,
                evidence_digest: digest.clone(),
                approved_by: approved_by.to_string(),
            });
    }
    state
        .disk
        .gaps
        .retain(|gap| gap.kind != OpenGapKind::PendingApproval);
}

fn note_material(session_id: &str, tool: &str) {
    if !is_material_tool(tool) {
        return;
    }
    let mut guard = ledger().lock().unwrap_or_else(|err| err.into_inner());
    let Some(state) = guard.sessions.get_mut(session_id) else {
        return;
    };
    state.disk.generation = state.disk.generation.saturating_add(1);
    guard.dirty = true;
    persist(&guard);
}

fn approval_refusal(session_id: &str, source: WakeSource) -> Option<String> {
    let guard = ledger().lock().unwrap_or_else(|err| err.into_inner());
    let state = guard.sessions.get(session_id)?;
    let digest = state.disk.generation.to_string();
    let approval = state.disk.approvals.last();
    review_blocked_by_approval(source, approval, &digest)
}

pub fn review_block(session_id: &str) -> Option<String> {
    let source = current_grant(session_id)
        .map(|grant| grant.source)
        .unwrap_or(WakeSource::Human);
    approval_refusal(session_id, source)
}

pub fn hooks_for(session_id: &str) -> Arc<WakeLoopHooks> {
    hooks(session_id, true)
}

pub fn worker_hooks(session_id: &str) -> Arc<WakeLoopHooks> {
    hooks(session_id, false)
}

fn hooks(session_id: &str, apply_round_cap: bool) -> Arc<WakeLoopHooks> {
    let session = session_id.to_string();
    let round_cap = if apply_round_cap {
        current_grant(session_id).and_then(|grant| {
            grant
                .remaining_rounds
                .or_else(|| grant.authority.round_cap())
                .map(|rounds| rounds as usize)
        })
    } else {
        None
    };
    let gaps_session = session.clone();
    let success_session = session;
    Arc::new(WakeLoopHooks {
        round_cap,
        open_gaps: Arc::new(move || gap_lines(&gaps_session)),
        on_tool_success: Arc::new(move |tool| note_material(&success_session, tool)),
    })
}

pub struct WakeToolRegistry {
    inner: Arc<dyn ToolRegistry>,
    session_id: String,
}

impl WakeToolRegistry {
    pub fn wrap(
        inner: Arc<dyn ToolRegistry>,
        session_id: impl Into<String>,
    ) -> Arc<dyn ToolRegistry> {
        Arc::new(Self {
            inner,
            session_id: session_id.into(),
        })
    }
}

fn effective_grant(session_id: &str) -> WakeGrant {
    current_grant(session_id).unwrap_or_else(|| WakeGrant::for_source(WakeSource::Human))
}

#[async_trait]
impl ToolRegistry for WakeToolRegistry {
    async fn list_tools(&self) -> Result<Vec<Tool>> {
        let grant = effective_grant(&self.session_id);
        let tools = self.inner.list_tools().await?;
        if current_grant(&self.session_id).is_none() {
            return Ok(tools);
        }
        Ok(tools
            .into_iter()
            .filter(|tool| {
                tool_refusal(
                    &grant,
                    tool.name.as_str(),
                    &Value::Object(Default::default()),
                )
                .is_none()
            })
            .collect())
    }

    async fn invoke_tool(&self, tool_name: &str, input: Value) -> Result<Value> {
        let grant = effective_grant(&self.session_id);
        if let Some(refusal) = tool_refusal(&grant, tool_name, &input) {
            return Err(StasisError::PortFailure(refusal.message));
        }
        if is_safety_review_spawn(tool_name, &input)
            && let Some(message) = approval_refusal(&self.session_id, grant.source)
        {
            return Err(StasisError::PortFailure(message));
        }
        self.inner.invoke_tool(tool_name, input).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct EchoRegistry;

    #[async_trait]
    impl ToolRegistry for EchoRegistry {
        async fn list_tools(&self) -> Result<Vec<Tool>> {
            Ok(vec![
                Tool {
                    name: "cognition_shell_run".into(),
                    description: None,
                    schema: None,
                    strict: None,
                    config: None,
                },
                Tool {
                    name: "cognition_runtime_query".into(),
                    description: None,
                    schema: None,
                    strict: None,
                    config: None,
                },
            ])
        }

        async fn invoke_tool(&self, tool_name: &str, input: Value) -> Result<Value> {
            Ok(json!({"ok": true, "tool": tool_name, "input": input}))
        }
    }

    #[tokio::test]
    async fn peer_terminal_registry_refuses_a_review_spawn() {
        let dir = tempfile::tempdir().unwrap();
        // SAFETY: unit tests set a private ledger path before touching the store.
        unsafe {
            std::env::set_var("MEDOUSA_WAKE_LEDGER_PATH", dir.path().join("wake.json"));
        }
        let session = format!("ses_{}", uuid::Uuid::new_v4().simple());
        begin_turn(
            &session,
            Some(WakeGrant::for_source(WakeSource::PeerTerminal)),
            "peer terminal",
            false,
        );
        let registry = WakeToolRegistry::wrap(Arc::new(EchoRegistry), &session);
        let error = registry
            .invoke_tool(
                "cognition_workshop_mutate",
                json!({"action": "workshop.spawn", "intent": "Stasis safety review"}),
            )
            .await
            .expect_err("peer wake must not spawn a review");
        let message = error.to_string();
        assert!(
            message.contains("cannot spawn a safety review"),
            "{message}"
        );
        assert!(message.contains("Allowed action:"), "{message}");
        registry
            .invoke_tool("cognition_runtime_query", json!({}))
            .await
            .expect("report tools stay available");
    }

    #[test]
    fn approval_closes_until_a_material_tool_changes_the_digest() {
        let dir = tempfile::tempdir().unwrap();
        // SAFETY: unit tests set a private ledger path before touching the store.
        unsafe {
            std::env::set_var("MEDOUSA_WAKE_LEDGER_PATH", dir.path().join("wake.json"));
        }
        let session = format!("ses_{}", uuid::Uuid::new_v4().simple());
        register_gap(
            &session,
            OpenGap {
                id: "review-1".into(),
                kind: OpenGapKind::PendingApproval,
                summary: "deploy review".into(),
            },
        );
        begin_turn(&session, None, "approved", true);
        assert!(gap_lines(&session).is_empty());
        let blocked = review_block(&session).expect("approval holds");
        assert!(blocked.contains("already approved"), "{blocked}");
        note_material(&session, "cognition_shell_run");
        assert!(review_block(&session).is_none());
    }
}
