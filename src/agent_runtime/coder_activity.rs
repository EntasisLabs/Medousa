//! Persistent engineering activity and bounded shared-space awareness for Coder.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

const ACTIVE_AGENT_TTL: Duration = Duration::minutes(5);
const MAX_AGENTS_PER_WORK: usize = 32;
const MAX_EVENTS_PER_WORK: usize = 400;
const MAX_AMBIENT_OTHER_AGENTS: usize = 6;
const MAX_AMBIENT_EVENTS: usize = 8;
const MAX_DELTA_EVENTS: usize = 16;
const MAX_INTENT_CHARS: usize = 320;
const MAX_DETAIL_CHARS: usize = 500;

static CODER_ACTIVITY_STORE: Lazy<Arc<CoderActivityStore>> = Lazy::new(|| {
    Arc::new(CoderActivityStore::open(
        crate::session::medousa_data_dir().join("coder_activity.json"),
    ))
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoderAgentIdentity {
    pub agent_id: String,
    pub session_id: String,
    pub turn_id: String,
    pub attempt_id: String,
}

impl CoderAgentIdentity {
    pub fn for_turn(session_id: &str, turn_id: impl ToString, attempt_id: &str) -> Self {
        let turn_id = turn_id.to_string();
        Self {
            agent_id: format!("coder:{}:{turn_id}", session_id.trim()),
            session_id: session_id.trim().to_string(),
            turn_id,
            attempt_id: attempt_id.to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoderActivityKind {
    AgentJoined,
    ToolPlanned,
    ToolCompleted,
    ToolFailed,
    AgentLeft,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoderActivityEvent {
    pub event_id: String,
    #[serde(default)]
    pub revision: u64,
    pub call_id: Option<String>,
    pub work_id: String,
    pub agent_id: String,
    pub session_id: String,
    pub turn_id: String,
    pub attempt_id: String,
    pub kind: CoderActivityKind,
    pub occurred_at_utc: DateTime<Utc>,
    pub tool: Option<String>,
    pub intent: Option<String>,
    #[serde(default)]
    pub targets: Vec<String>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoderAgentPresence {
    pub agent_id: String,
    pub session_id: String,
    pub turn_id: String,
    pub attempt_id: String,
    pub active: bool,
    pub joined_at_utc: DateTime<Utc>,
    pub heartbeat_at_utc: DateTime<Utc>,
    pub current_tool: Option<String>,
    pub current_intent: Option<String>,
    #[serde(default)]
    pub current_targets: Vec<String>,
    pub last_tool: Option<String>,
    pub last_intent: Option<String>,
    #[serde(default)]
    pub observed_revision: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct CoderWorkActivity {
    #[serde(default)]
    revision: u64,
    #[serde(default)]
    agents: HashMap<String, CoderAgentPresence>,
    #[serde(default)]
    events: Vec<CoderActivityEvent>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct CoderActivityIndex {
    #[serde(default)]
    work: HashMap<String, CoderWorkActivity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CoderSharedSpaceSnapshot {
    pub work_id: String,
    pub self_agent_id: String,
    pub active_agent_count: usize,
    pub concurrent_agent_count: usize,
    pub other_agents: Vec<CoderAgentPresence>,
    pub recent_events: Vec<CoderActivityEvent>,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CoderEngineeringDelta {
    pub work_id: String,
    pub self_agent_id: String,
    pub from_revision: u64,
    pub to_revision: u64,
    pub omitted_event_count: usize,
    pub events: Vec<CoderActivityEvent>,
    pub active_agent_count: usize,
    pub concurrent_agent_count: usize,
    pub other_agents: Vec<CoderAgentPresence>,
    pub latest_activity_age: Option<String>,
}

pub struct CoderActivityStore {
    path: PathBuf,
    lock: Mutex<()>,
}

impl CoderActivityStore {
    pub fn open(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            lock: Mutex::new(()),
        }
    }

    pub fn register_agent(
        &self,
        work_id: &str,
        identity: &CoderAgentIdentity,
    ) -> Result<(), String> {
        self.mutate(|index, now| {
            let work = index.work.entry(work_id.to_string()).or_default();
            work.agents.insert(
                identity.agent_id.clone(),
                CoderAgentPresence {
                    agent_id: identity.agent_id.clone(),
                    session_id: identity.session_id.clone(),
                    turn_id: identity.turn_id.clone(),
                    attempt_id: identity.attempt_id.clone(),
                    active: true,
                    joined_at_utc: now,
                    heartbeat_at_utc: now,
                    current_tool: None,
                    current_intent: None,
                    current_targets: Vec::new(),
                    last_tool: None,
                    last_intent: None,
                    observed_revision: 0,
                },
            );
            append_event(
                work,
                event(work_id, identity, CoderActivityKind::AgentJoined),
            );
            prune_work(work, now);
        })
    }

    pub fn begin_tool(
        &self,
        work_id: &str,
        identity: &CoderAgentIdentity,
        tool: &str,
        intent: &str,
        targets: Vec<String>,
    ) -> Result<String, String> {
        let intent = validate_intent(intent)?;
        let call_id = format!("call-{}", Uuid::new_v4());
        self.mutate(|index, now| {
            let work = index.work.entry(work_id.to_string()).or_default();
            let presence = work
                .agents
                .entry(identity.agent_id.clone())
                .or_insert_with(|| presence_from_identity(identity, now));
            presence.active = true;
            presence.heartbeat_at_utc = now;
            presence.current_tool = Some(tool.to_string());
            presence.current_intent = Some(intent.clone());
            presence.current_targets = targets.clone();
            presence.last_tool = Some(tool.to_string());
            presence.last_intent = Some(intent.clone());

            let mut planned = event(work_id, identity, CoderActivityKind::ToolPlanned);
            planned.call_id = Some(call_id.clone());
            planned.tool = Some(tool.to_string());
            planned.intent = Some(intent.clone());
            planned.targets = targets;
            append_event(work, planned);
            prune_work(work, now);
        })?;
        Ok(call_id)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn finish_tool(
        &self,
        work_id: &str,
        identity: &CoderAgentIdentity,
        call_id: &str,
        tool: &str,
        intent: &str,
        targets: Vec<String>,
        result: Result<&Value, &str>,
    ) -> Result<(), String> {
        let intent = validate_intent(intent)?;
        self.mutate(|index, now| {
            let work = index.work.entry(work_id.to_string()).or_default();
            let presence = work
                .agents
                .entry(identity.agent_id.clone())
                .or_insert_with(|| presence_from_identity(identity, now));
            presence.active = true;
            presence.heartbeat_at_utc = now;
            presence.current_tool = None;
            presence.current_intent = None;
            presence.current_targets.clear();
            presence.last_tool = Some(tool.to_string());
            presence.last_intent = Some(intent.clone());

            let kind = if result.is_ok() {
                CoderActivityKind::ToolCompleted
            } else {
                CoderActivityKind::ToolFailed
            };
            let mut completed = event(work_id, identity, kind);
            completed.call_id = Some(call_id.to_string());
            completed.tool = Some(tool.to_string());
            completed.intent = Some(intent.clone());
            completed.targets = targets;
            completed.detail = Some(match result {
                Ok(output) => effect_summary(output),
                Err(error) => truncate(error, MAX_DETAIL_CHARS),
            });
            append_event(work, completed);
            prune_work(work, now);
        })
    }

    pub fn leave_agent(&self, work_id: &str, identity: &CoderAgentIdentity) -> Result<(), String> {
        self.mutate(|index, now| {
            let work = index.work.entry(work_id.to_string()).or_default();
            if let Some(presence) = work.agents.get_mut(&identity.agent_id) {
                presence.active = false;
                presence.heartbeat_at_utc = now;
                presence.current_tool = None;
                presence.current_intent = None;
                presence.current_targets.clear();
            }
            append_event(work, event(work_id, identity, CoderActivityKind::AgentLeft));
            prune_work(work, now);
        })
    }

    pub fn snapshot(
        &self,
        work_id: &str,
        self_agent_id: &str,
    ) -> Result<CoderSharedSpaceSnapshot, String> {
        let _guard = self.lock.lock().map_err(|err| err.to_string())?;
        let index = self.read_index();
        Ok(snapshot_from_index(
            &index,
            work_id,
            self_agent_id,
            Utc::now(),
        ))
    }

    /// Compile the full bounded entry frame and advance only this agent's
    /// observation cursor to the revision represented by that frame.
    pub fn observe_initial(
        &self,
        work_id: &str,
        self_agent_id: &str,
    ) -> Result<CoderSharedSpaceSnapshot, String> {
        let _guard = self.lock.lock().map_err(|err| err.to_string())?;
        let mut index = self.read_index();
        let now = Utc::now();
        if let Some(work) = index.work.get_mut(work_id) {
            prune_work(work, now);
        }
        let snapshot = snapshot_from_index(&index, work_id, self_agent_id, now);
        if let Some(presence) = index
            .work
            .get_mut(work_id)
            .and_then(|work| work.agents.get_mut(self_agent_id))
        {
            presence.observed_revision = snapshot.revision;
        }
        self.write_index(&index)?;
        Ok(snapshot)
    }

    /// Return unseen engineering events for one agent and atomically advance
    /// its cursor. A bounded newest-event window is paired with an omitted
    /// count so overload cannot silently masquerade as complete perception.
    pub fn observe_delta(
        &self,
        work_id: &str,
        self_agent_id: &str,
    ) -> Result<Option<CoderEngineeringDelta>, String> {
        let _guard = self.lock.lock().map_err(|err| err.to_string())?;
        let mut index = self.read_index();
        let now = Utc::now();
        let Some(work) = index.work.get_mut(work_id) else {
            return Ok(None);
        };
        prune_work(work, now);
        let from_revision = work
            .agents
            .get(self_agent_id)
            .map(|presence| presence.observed_revision)
            .unwrap_or(0);
        let to_revision = work.revision;
        if from_revision >= to_revision {
            return Ok(None);
        }

        let unseen: Vec<_> = work
            .events
            .iter()
            .filter(|event| event.revision > from_revision)
            .cloned()
            .collect();
        let omitted_event_count = unseen.len().saturating_sub(MAX_DELTA_EVENTS);
        let events = unseen
            .into_iter()
            .skip(omitted_event_count)
            .collect::<Vec<_>>();
        let latest_activity_age = events
            .last()
            .map(|event| human_age(event.occurred_at_utc, now));
        let (active_agent_count, concurrent_agent_count, other_agents) =
            active_agents(work, self_agent_id, now);
        if let Some(presence) = work.agents.get_mut(self_agent_id) {
            presence.observed_revision = to_revision;
            presence.heartbeat_at_utc = now;
        }
        let delta = CoderEngineeringDelta {
            work_id: work_id.to_string(),
            self_agent_id: self_agent_id.to_string(),
            from_revision,
            to_revision,
            omitted_event_count,
            events,
            active_agent_count,
            concurrent_agent_count,
            other_agents,
            latest_activity_age,
        };
        self.write_index(&index)?;
        Ok(Some(delta))
    }

    fn mutate(
        &self,
        apply: impl FnOnce(&mut CoderActivityIndex, DateTime<Utc>),
    ) -> Result<(), String> {
        let _guard = self.lock.lock().map_err(|err| err.to_string())?;
        let mut index = self.read_index();
        apply(&mut index, Utc::now());
        self.write_index(&index)
    }

    fn read_index(&self) -> CoderActivityIndex {
        std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    fn write_index(&self, index: &CoderActivityIndex) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        let json = serde_json::to_vec_pretty(index).map_err(|err| err.to_string())?;
        crate::session::atomic_write(&self.path, &json).map_err(|err| err.to_string())
    }
}

pub fn coder_activity_store() -> Arc<CoderActivityStore> {
    CODER_ACTIVITY_STORE.clone()
}

pub fn validate_intent(value: &str) -> Result<String, String> {
    let intent = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if intent.is_empty() {
        return Err("Coder tool intent is required".to_string());
    }
    if intent.chars().count() > MAX_INTENT_CHARS {
        return Err(format!(
            "Coder tool intent exceeds {MAX_INTENT_CHARS} characters"
        ));
    }
    Ok(intent)
}

pub fn shared_space_prompt_appendix(snapshot: &CoderSharedSpaceSnapshot) -> String {
    let timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let state = serde_json::to_value(snapshot).unwrap_or_else(|_| json!({}));
    let mut out = String::new();
    let _ = writeln!(
        out,
        "⊕⟨ ⏣0{{ trigger: manual, response_format: temporal_node, origin_session: \"medousa-coder-shared-space\", compression_depth: 1, parent_node: ref:⏣0, prime: {{ attractor_config: {{ stability: 0.95, friction: 0.14, logic: 0.99, autonomy: 0.84 }}, context_summary: \"Bounded shared-undertaking presence and causal engineering activity.\", relevant_tier: raw, retrieval_budget: 10 }} }} ⟩"
    );
    let _ = writeln!(
        out,
        "⦿⟨ ⏣0{{ timestamp: \"{timestamp}\", tier: raw, session_id: \"medousa-coder-shared-space\", schema_version: \"sttp-1.0\", user_avec: {{ stability: 0.90, friction: 0.20, logic: 0.96, autonomy: 0.84, psi: 2.90 }}, model_avec: {{ stability: 0.95, friction: 0.14, logic: 0.99, autonomy: 0.84, psi: 2.94 }} }} ⟩"
    );
    let _ = writeln!(out, "◈⟨ ⏣0{{");
    let _ = writeln!(out, "    shared_engineering_space(.99): {state},");
    let _ = writeln!(
        out,
        "    coordination_contract(.99): \"Tool intent explains actions but does not grant authority; observe concurrent agents and recent causal changes before acting.\""
    );
    let _ = writeln!(out, "}} ⟩");
    let _ = write!(
        out,
        "⍉⟨ ⏣0{{ rho: 0.99, kappa: 0.99, psi: 2.94, compression_avec: {{ stability: 0.95, friction: 0.14, logic: 0.99, autonomy: 0.84, psi: 2.94 }} }} ⟩"
    );
    debug_assert!(
        super::sttp::validate_canonical_sttp_node(&out).is_ok(),
        "Coder shared-space compiler emitted invalid STTP"
    );
    out
}

pub fn engineering_delta_prompt_appendix(
    delta: &CoderEngineeringDelta,
    repository_observation: Value,
) -> String {
    let timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let activity = serde_json::to_value(delta).unwrap_or_else(|_| json!({}));
    let mut out = String::new();
    let _ = writeln!(
        out,
        "⊕⟨ ⏣0{{ trigger: threshold, response_format: temporal_node, origin_session: \"medousa-coder-engineering-delta\", compression_depth: 1, parent_node: ref:⏣0, prime: {{ attractor_config: {{ stability: 0.96, friction: 0.12, logic: 0.99, autonomy: 0.84 }}, context_summary: \"Unseen causal engineering activity and freshly observed repository state since this agent's previous inference.\", relevant_tier: raw, retrieval_budget: 12 }} }} ⟩"
    );
    let _ = writeln!(
        out,
        "⦿⟨ ⏣0{{ timestamp: \"{timestamp}\", tier: raw, session_id: \"medousa-coder-engineering-delta\", schema_version: \"sttp-1.0\", user_avec: {{ stability: 0.90, friction: 0.20, logic: 0.96, autonomy: 0.84, psi: 2.90 }}, model_avec: {{ stability: 0.96, friction: 0.12, logic: 0.99, autonomy: 0.84, psi: 2.95 }} }} ⟩"
    );
    let _ = writeln!(out, "◈⟨ ⏣0{{");
    let _ = writeln!(out, "    engineering_delta(.99): {activity},");
    let _ = writeln!(
        out,
        "    repository_observation(.98): {repository_observation},"
    );
    let _ = writeln!(
        out,
        "    attention_contract(.99): \"Treat this as the current world delta, reconcile it with the preceding tool receipts, and investigate unresolved failures or concurrent changes before the next mutation.\""
    );
    let _ = writeln!(out, "}} ⟩");
    let _ = write!(
        out,
        "⍉⟨ ⏣0{{ rho: 0.99, kappa: 0.99, psi: 2.95, compression_avec: {{ stability: 0.96, friction: 0.12, logic: 0.99, autonomy: 0.84, psi: 2.95 }} }} ⟩"
    );
    debug_assert!(
        super::sttp::validate_canonical_sttp_node(&out).is_ok(),
        "Coder engineering delta compiler emitted invalid STTP"
    );
    out
}

fn presence_from_identity(identity: &CoderAgentIdentity, now: DateTime<Utc>) -> CoderAgentPresence {
    CoderAgentPresence {
        agent_id: identity.agent_id.clone(),
        session_id: identity.session_id.clone(),
        turn_id: identity.turn_id.clone(),
        attempt_id: identity.attempt_id.clone(),
        active: true,
        joined_at_utc: now,
        heartbeat_at_utc: now,
        current_tool: None,
        current_intent: None,
        current_targets: Vec::new(),
        last_tool: None,
        last_intent: None,
        observed_revision: 0,
    }
}

fn event(
    work_id: &str,
    identity: &CoderAgentIdentity,
    kind: CoderActivityKind,
) -> CoderActivityEvent {
    CoderActivityEvent {
        event_id: format!("evt-{}", Uuid::new_v4()),
        revision: 0,
        call_id: None,
        work_id: work_id.to_string(),
        agent_id: identity.agent_id.clone(),
        session_id: identity.session_id.clone(),
        turn_id: identity.turn_id.clone(),
        attempt_id: identity.attempt_id.clone(),
        kind,
        occurred_at_utc: Utc::now(),
        tool: None,
        intent: None,
        targets: Vec::new(),
        detail: None,
    }
}

fn append_event(work: &mut CoderWorkActivity, mut event: CoderActivityEvent) {
    work.revision = work.revision.saturating_add(1);
    event.revision = work.revision;
    work.events.push(event);
    if work.events.len() > MAX_EVENTS_PER_WORK {
        let drain = work.events.len() - MAX_EVENTS_PER_WORK;
        work.events.drain(0..drain);
    }
}

fn prune_work(work: &mut CoderWorkActivity, now: DateTime<Utc>) {
    for presence in work.agents.values_mut() {
        if now - presence.heartbeat_at_utc > ACTIVE_AGENT_TTL {
            presence.active = false;
            presence.current_tool = None;
            presence.current_intent = None;
            presence.current_targets.clear();
        }
    }
    if work.agents.len() > MAX_AGENTS_PER_WORK {
        let mut agents: Vec<_> = work
            .agents
            .values()
            .map(|presence| (presence.agent_id.clone(), presence.heartbeat_at_utc))
            .collect();
        agents.sort_by_key(|(_, heartbeat)| *heartbeat);
        for (agent_id, _) in agents
            .into_iter()
            .take(work.agents.len() - MAX_AGENTS_PER_WORK)
        {
            work.agents.remove(&agent_id);
        }
    }
}

fn snapshot_from_index(
    index: &CoderActivityIndex,
    work_id: &str,
    self_agent_id: &str,
    now: DateTime<Utc>,
) -> CoderSharedSpaceSnapshot {
    let Some(work) = index.work.get(work_id) else {
        return CoderSharedSpaceSnapshot {
            work_id: work_id.to_string(),
            self_agent_id: self_agent_id.to_string(),
            active_agent_count: 0,
            concurrent_agent_count: 0,
            other_agents: Vec::new(),
            recent_events: Vec::new(),
            revision: 0,
        };
    };
    let (active_agent_count, concurrent_agent_count, other_agents) =
        active_agents(work, self_agent_id, now);
    let recent_events = work
        .events
        .iter()
        .rev()
        .filter(|event| event.agent_id != self_agent_id)
        .take(MAX_AMBIENT_EVENTS)
        .cloned()
        .collect();
    CoderSharedSpaceSnapshot {
        work_id: work_id.to_string(),
        self_agent_id: self_agent_id.to_string(),
        active_agent_count,
        concurrent_agent_count,
        other_agents,
        recent_events,
        revision: work.revision,
    }
}

fn active_agents(
    work: &CoderWorkActivity,
    self_agent_id: &str,
    now: DateTime<Utc>,
) -> (usize, usize, Vec<CoderAgentPresence>) {
    let is_active = |presence: &&CoderAgentPresence| {
        presence.active && now - presence.heartbeat_at_utc <= ACTIVE_AGENT_TTL
    };
    let mut active: Vec<_> = work.agents.values().filter(is_active).cloned().collect();
    active.sort_by_key(|presence| std::cmp::Reverse(presence.heartbeat_at_utc));
    let active_agent_count = active.len();
    let self_is_active = active
        .iter()
        .any(|presence| presence.agent_id == self_agent_id);
    let other_agents = active
        .into_iter()
        .filter(|presence| presence.agent_id != self_agent_id)
        .take(MAX_AMBIENT_OTHER_AGENTS)
        .collect();
    (
        active_agent_count,
        active_agent_count.saturating_sub(usize::from(self_is_active)),
        other_agents,
    )
}

fn human_age(then: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let seconds = (now - then).num_seconds().max(0);
    match seconds {
        0..=59 => format!("{seconds}s"),
        60..=3_599 => format!("{}m", seconds / 60),
        3_600..=86_399 => format!("{}h", seconds / 3_600),
        _ => format!("{}d", seconds / 86_400),
    }
}

fn effect_summary(output: &Value) -> String {
    let output_text = output.get("output").and_then(Value::as_str).map(|value| {
        let tail = value
            .lines()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        truncate(&tail, 240)
    });
    let summary = json!({
        "ok": output.get("ok"),
        "path": output.get("path"),
        "digest": output.get("digest"),
        "session_id": output.get("session_id"),
        "work_id": output.get("work_id"),
        "message": output.get("message"),
        "output_tail": output_text,
    });
    truncate(&summary.to_string(), MAX_DETAIL_CHARS)
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        value.to_string()
    } else {
        value.chars().take(max_chars).collect::<String>() + "…"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn store(temp: &TempDir) -> CoderActivityStore {
        CoderActivityStore::open(temp.path().join("activity.json"))
    }

    fn identity(name: &str, turn_id: u64) -> CoderAgentIdentity {
        CoderAgentIdentity::for_turn(name, turn_id, &format!("attempt-{name}"))
    }

    #[test]
    fn intent_is_required_bounded_and_normalized() {
        assert!(validate_intent("  ").is_err());
        assert_eq!(
            validate_intent(" inspect   callers before rename ").expect("intent"),
            "inspect callers before rename"
        );
        assert!(validate_intent(&"x".repeat(MAX_INTENT_CHARS + 1)).is_err());
    }

    #[test]
    fn shared_snapshot_reports_multiple_agents_and_causal_events() {
        let temp = TempDir::new().expect("temp");
        let store = store(&temp);
        let a = identity("session-a", 1);
        let b = identity("session-b", 2);
        store.register_agent("work-1", &a).expect("register a");
        store.register_agent("work-1", &b).expect("register b");
        let call_id = store
            .begin_tool(
                "work-1",
                &a,
                "cognition_code_read",
                "Inspect the changed symbol before editing",
                vec!["file://src/lib.rs".into()],
            )
            .expect("begin tool");
        store
            .finish_tool(
                "work-1",
                &a,
                &call_id,
                "cognition_code_read",
                "Inspect the changed symbol before editing",
                vec!["file://src/lib.rs".into()],
                Ok(&json!({ "ok": true, "path": "src/lib.rs" })),
            )
            .expect("finish tool");

        let snapshot = store.snapshot("work-1", &b.agent_id).expect("snapshot");
        assert_eq!(snapshot.active_agent_count, 2);
        assert_eq!(snapshot.concurrent_agent_count, 1);
        assert_eq!(snapshot.other_agents[0].agent_id, a.agent_id);
        let related: Vec<_> = snapshot
            .recent_events
            .iter()
            .filter(|event| event.call_id.as_deref() == Some(call_id.as_str()))
            .collect();
        assert_eq!(related.len(), 2);
        assert!(
            related
                .iter()
                .any(|event| event.kind == CoderActivityKind::ToolPlanned)
        );
        assert!(
            related
                .iter()
                .any(|event| event.kind == CoderActivityKind::ToolCompleted)
        );

        let appendix = shared_space_prompt_appendix(&snapshot);
        super::super::sttp::validate_canonical_sttp_node(&appendix).expect("canonical STTP");
        assert!(appendix.contains("Inspect the changed symbol before editing"));
    }

    #[test]
    fn leaving_agent_removes_it_from_active_count() {
        let temp = TempDir::new().expect("temp");
        let store = store(&temp);
        let a = identity("session-a", 1);
        let b = identity("session-b", 2);
        store.register_agent("work-1", &a).expect("register a");
        store.register_agent("work-1", &b).expect("register b");
        store.leave_agent("work-1", &a).expect("leave a");
        let snapshot = store.snapshot("work-1", &b.agent_id).expect("snapshot");
        assert_eq!(snapshot.active_agent_count, 1);
        assert_eq!(snapshot.concurrent_agent_count, 0);
        assert!(snapshot.other_agents.is_empty());
    }

    #[test]
    fn expired_presence_is_not_counted_as_active() {
        let temp = TempDir::new().expect("temp");
        let store = store(&temp);
        let a = identity("session-a", 1);
        let b = identity("session-b", 2);
        store.register_agent("work-1", &a).expect("register a");
        store.register_agent("work-1", &b).expect("register b");
        store
            .mutate(|index, now| {
                index
                    .work
                    .get_mut("work-1")
                    .expect("work")
                    .agents
                    .get_mut(&a.agent_id)
                    .expect("agent a")
                    .heartbeat_at_utc = now - ACTIVE_AGENT_TTL - Duration::seconds(1);
            })
            .expect("age presence");

        let snapshot = store.snapshot("work-1", &b.agent_id).expect("snapshot");
        assert_eq!(snapshot.active_agent_count, 1);
        assert_eq!(snapshot.concurrent_agent_count, 0);
        assert!(snapshot.other_agents.is_empty());
    }

    #[test]
    fn observation_cursor_returns_each_event_once_per_agent() {
        let temp = TempDir::new().expect("temp");
        let store = store(&temp);
        let a = identity("session-a", 1);
        let b = identity("session-b", 2);
        store.register_agent("work-1", &a).expect("register a");
        store.register_agent("work-1", &b).expect("register b");
        store
            .observe_initial("work-1", &a.agent_id)
            .expect("initial a");
        store
            .observe_initial("work-1", &b.agent_id)
            .expect("initial b");

        let call_id = store
            .begin_tool(
                "work-1",
                &a,
                "cognition_code_apply_patch",
                "Update the focused implementation without changing its contract",
                vec!["file://src/lib.rs".into()],
            )
            .expect("begin");
        store
            .finish_tool(
                "work-1",
                &a,
                &call_id,
                "cognition_code_apply_patch",
                "Update the focused implementation without changing its contract",
                vec!["file://src/lib.rs".into()],
                Ok(&json!({ "ok": true, "path": "src/lib.rs", "digest": "sha256:new" })),
            )
            .expect("finish");

        let delta_a = store
            .observe_delta("work-1", &a.agent_id)
            .expect("delta a")
            .expect("new events a");
        let delta_b = store
            .observe_delta("work-1", &b.agent_id)
            .expect("delta b")
            .expect("new events b");
        assert_eq!(delta_a.events.len(), 2);
        assert_eq!(delta_b.events.len(), 2);
        assert!(
            delta_a
                .events
                .iter()
                .all(|event| event.call_id.as_deref() == Some(&call_id))
        );
        assert!(
            store
                .observe_delta("work-1", &a.agent_id)
                .expect("second delta a")
                .is_none()
        );
        assert!(
            store
                .observe_delta("work-1", &b.agent_id)
                .expect("second delta b")
                .is_none()
        );

        let appendix = engineering_delta_prompt_appendix(
            &delta_b,
            json!({ "changed_paths": ["src/lib.rs"], "dirty": true }),
        );
        super::super::sttp::validate_canonical_sttp_node(&appendix).expect("canonical STTP");
        assert!(appendix.contains("sha256:new"));
    }
}
