//! Session ledger for wake grants, durable open gaps, approvals, and deviations.
//!
//! The grant is replaced on every turn so a peer wake cannot leak onto the
//! next human turn. Gaps, approvals, and deviations are written to disk and
//! reloaded after a daemon restart. The tool loop reads [`hooks_for`]. The
//! registry keeps the full toolset and records a call that goes past the
//! expected response.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use async_trait::async_trait;
use genai::chat::Tool;
use medousa_runtime::WakeLoopHooks;
use medousa_types::wake::{
    OpenGap, OpenGapKind, WakeGrant, WakeSource, beyond_expected_note, is_approval_utterance,
    is_material_tool, is_safety_review_spawn, review_context,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use stasis::application::orchestration::tool_registry::ToolRegistry;
use stasis::prelude::Result;

const MAX_DEVIATIONS: usize = 200;

/// A call that went past the wake's expected response. The tool still ran.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WakeDeviation {
    pub at: String,
    pub source: String,
    pub expected: String,
    pub tool: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct DiskSession {
    #[serde(default)]
    gaps: Vec<OpenGap>,
    #[serde(default)]
    approvals: Vec<medousa_types::wake::WorkApproval>,
    #[serde(default)]
    generation: u64,
    #[serde(default)]
    deviations: Vec<WakeDeviation>,
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
                approved_at: Some(chrono::Utc::now().to_rfc3339()),
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

pub fn deviations(session_id: &str) -> Vec<WakeDeviation> {
    let guard = ledger().lock().unwrap_or_else(|err| err.into_inner());
    guard
        .sessions
        .get(session_id)
        .map(|state| state.disk.deviations.clone())
        .unwrap_or_default()
}

fn stated_reason(input: &Value) -> Option<String> {
    for key in ["reason", "why", "rationale"] {
        if let Some(text) = input.get(key).and_then(Value::as_str) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.chars().take(500).collect());
            }
        }
    }
    None
}

fn record_deviation(session_id: &str, grant: &WakeGrant, tool: &str, reason: Option<String>) {
    with_session(session_id, |state| {
        state.disk.deviations.push(WakeDeviation {
            at: chrono::Utc::now().to_rfc3339(),
            source: grant.source.as_str().to_string(),
            expected: grant.authority.expected_response().to_string(),
            tool: tool.to_string(),
            reason,
        });
        if state.disk.deviations.len() > MAX_DEVIATIONS {
            let overflow = state.disk.deviations.len() - MAX_DEVIATIONS;
            state.disk.deviations.drain(0..overflow);
        }
    });
}

/// Approval context for a review spawn. `None` when this session has no approval.
pub fn review_note(session_id: &str) -> Option<String> {
    let guard = ledger().lock().unwrap_or_else(|err| err.into_inner());
    let state = guard.sessions.get(session_id)?;
    let digest = state.disk.generation.to_string();
    review_context(state.disk.approvals.last(), &digest)
}

pub fn hooks_for(session_id: &str) -> Arc<WakeLoopHooks> {
    hooks(session_id)
}

pub fn worker_hooks(session_id: &str) -> Arc<WakeLoopHooks> {
    hooks(session_id)
}

fn hooks(session_id: &str) -> Arc<WakeLoopHooks> {
    let gaps_session = session_id.to_string();
    let success_session = session_id.to_string();
    Arc::new(WakeLoopHooks {
        round_cap: None,
        open_gaps: Arc::new(move || gap_lines(&gaps_session)),
        on_tool_success: Arc::new(move |tool| note_material(&success_session, tool)),
    })
}

fn attach_field(mut output: Value, key: &str, note: String) -> Value {
    if let Some(map) = output.as_object_mut() {
        map.insert(key.to_string(), Value::String(note));
        output
    } else {
        serde_json::json!({"result": output, key: note})
    }
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
        self.inner.list_tools().await
    }

    async fn invoke_tool(&self, tool_name: &str, input: Value) -> Result<Value> {
        let grant = effective_grant(&self.session_id);
        let beyond = beyond_expected_note(&grant, tool_name, &input);
        let review = is_safety_review_spawn(tool_name, &input);
        if beyond.is_some() {
            let reason = stated_reason(&input);
            tracing::info!(
                session_id = %self.session_id,
                source = grant.source.as_str(),
                tool = tool_name,
                reason = reason.as_deref().unwrap_or(""),
                "wake deviation: call is past the expected response"
            );
            record_deviation(&self.session_id, &grant, tool_name, reason);
        }
        let mut output = self.inner.invoke_tool(tool_name, input).await?;
        if let Some(note) = beyond {
            output = attach_field(output, "wake_note", note);
        }
        if review && let Some(context) = review_note(&self.session_id) {
            output = attach_field(output, "review_context", context);
        }
        Ok(output)
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
    async fn peer_terminal_registry_logs_a_review_spawn_and_still_runs_it() {
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
        let tools = registry.list_tools().await.expect("full toolset");
        assert!(
            tools
                .iter()
                .any(|tool| tool.name.as_str() == "cognition_shell_run"),
            "a peer wake keeps shell"
        );
        let value = registry
            .invoke_tool(
                "cognition_workshop_mutate",
                json!({
                    "action": "workshop.spawn",
                    "intent": "Stasis safety review",
                    "reason": "the peer result looks wrong"
                }),
            )
            .await
            .expect("peer wake keeps the tool");
        assert_eq!(value["ok"], true);
        let note = value["wake_note"].as_str().expect("deviation note");
        assert!(note.contains("peer finished"), "{note}");
        assert!(note.contains("The call is allowed"), "{note}");
        let logged = deviations(&session);
        assert_eq!(logged.len(), 1);
        assert_eq!(logged[0].tool, "cognition_workshop_mutate");
        assert_eq!(logged[0].source, "peer_terminal");
        assert_eq!(
            logged[0].reason.as_deref(),
            Some("the peer result looks wrong")
        );
        assert_eq!(logged[0].expected, "report the outcome, then finish");
        let query = registry
            .invoke_tool("cognition_runtime_query", json!({}))
            .await
            .expect("report tools stay available");
        assert!(query.get("wake_note").is_none());
        assert_eq!(deviations(&session).len(), 1);
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
        let note = review_note(&session).expect("approval is context");
        assert!(note.contains("approved at "), "{note}");
        assert!(note.contains("nothing material changed since"), "{note}");
        assert!(note.contains("(unchanged)"), "{note}");
        note_material(&session, "cognition_shell_run");
        let changed = review_note(&session).expect("changed evidence is still context");
        assert!(changed.contains("(changed)"), "{changed}");
    }
}
