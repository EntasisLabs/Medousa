//! Persistent engineering activity and bounded shared-space awareness for Coder.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use once_cell::sync::Lazy;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use super::coder_claims::{CoderClaimMode, CoderClaimScope};

const ACTIVE_AGENT_TTL: Duration = Duration::minutes(5);
const ACTIVE_CLAIM_TTL: Duration = Duration::minutes(2);
const MAX_AGENTS_PER_WORK: usize = 32;
const MAX_EVENTS_PER_WORK: usize = 400;
const MAX_ACTIVE_CLAIMS_PER_WORK: usize = 256;
const MAX_AMBIENT_OTHER_AGENTS: usize = 6;
const MAX_AMBIENT_EVENTS: usize = 8;
const MAX_DELTA_EVENTS: usize = 16;
const MAX_INTENT_CHARS: usize = 320;
const MAX_DETAIL_CHARS: usize = 500;
const MAX_AMBIENT_OVERLAPS: usize = 12;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CoderActivityKind {
    AgentJoined,
    ToolPlanned,
    ToolBlocked,
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
    #[serde(default)]
    pub claims: Vec<CoderClaimScope>,
    #[serde(default)]
    pub overlaps: Vec<CoderClaimOverlap>,
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
    #[serde(default)]
    pub current_claims: Vec<CoderClaimScope>,
    pub last_tool: Option<String>,
    pub last_intent: Option<String>,
    #[serde(default)]
    pub observed_revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CoderWorkActivity {
    #[serde(default = "new_activity_epoch")]
    epoch: String,
    #[serde(default)]
    revision: u64,
    #[serde(default)]
    agents: HashMap<String, CoderAgentPresence>,
    #[serde(default)]
    events: Vec<CoderActivityEvent>,
    #[serde(default)]
    active_claims: HashMap<String, CoderActiveClaim>,
}

fn new_activity_epoch() -> String {
    Uuid::new_v4().to_string()
}

impl Default for CoderWorkActivity {
    fn default() -> Self {
        Self {
            epoch: new_activity_epoch(),
            revision: 0,
            agents: HashMap::new(),
            events: Vec::new(),
            active_claims: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoderActiveClaim {
    pub claim_id: String,
    pub call_id: String,
    pub agent_id: String,
    pub session_id: String,
    pub turn_id: String,
    pub attempt_id: String,
    pub tool: String,
    pub intent: String,
    pub scope: CoderClaimScope,
    pub acquired_at_utc: DateTime<Utc>,
    pub heartbeat_at_utc: DateTime<Utc>,
    pub expires_at_utc: DateTime<Utc>,
    #[serde(default)]
    pub retained_after_call: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoderClaimOverlap {
    pub target: String,
    pub requested_mode: CoderClaimMode,
    pub held_mode: CoderClaimMode,
    pub hazardous: bool,
    pub blocked: bool,
    pub holder_agent_id: String,
    pub holder_attempt_id: String,
    pub holder_tool: String,
    pub holder_intent: String,
    pub holder_expires_at_utc: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CoderToolActivityAdmission {
    pub call_id: String,
    pub claims: Vec<CoderClaimScope>,
    pub overlaps: Vec<CoderClaimOverlap>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CoderClaimConflictResult {
    pub ok: bool,
    pub code: &'static str,
    pub error: &'static str,
    pub work_id: String,
    pub call_id: String,
    pub requested_claims: Vec<CoderClaimScope>,
    pub conflicts: Vec<CoderClaimOverlap>,
    pub retry_after_utc: Option<DateTime<Utc>>,
    pub next_decision: &'static str,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct CoderActivityIndex {
    #[serde(default)]
    work: HashMap<String, CoderWorkActivity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CoderSharedSpaceSnapshot {
    pub epoch: String,
    pub work_id: String,
    pub self_agent_id: String,
    pub active_agent_count: usize,
    pub concurrent_agent_count: usize,
    pub other_agents: Vec<CoderAgentPresence>,
    pub recent_events: Vec<CoderActivityEvent>,
    pub active_claims: Vec<CoderActiveClaim>,
    pub overlaps: Vec<CoderClaimOverlap>,
    pub revision: u64,
}

/// Explicit cursors are independent of agent presence, so reads are retryable and
/// a resumed agent cannot accidentally inherit another reader's progress.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoderDeltaQuery {
    pub epoch: String,
    pub since_revision: u64,
    pub through_revision: Option<u64>,
    pub agent_id: Option<String>,
    pub target: Option<String>,
    pub kind: Option<CoderActivityKind>,
    pub limit: Option<usize>,
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
                    current_claims: Vec::new(),
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
        claims: Vec<CoderClaimScope>,
    ) -> Result<CoderToolActivityAdmission, String> {
        let intent = validate_intent(intent)?;
        let call_id = format!("call-{}", Uuid::new_v4());
        let _guard = self.lock.lock().map_err(|err| err.to_string())?;
        let mut index = self.read_index();
        let now = Utc::now();
        let work = index.work.entry(work_id.to_string()).or_default();
        prune_work(work, now);
        let overlaps = claim_overlaps(work, &identity.agent_id, &claims);
        let blocked = overlaps.iter().any(|overlap| overlap.blocked);
        if blocked {
            let mut event = event(work_id, identity, CoderActivityKind::ToolBlocked);
            event.call_id = Some(call_id.clone());
            event.tool = Some(tool.to_string());
            event.intent = Some(intent);
            event.targets = targets;
            event.claims = claims.clone();
            event.overlaps = overlaps.clone();
            event.detail = Some("hazardous shared resource is already claimed".into());
            append_event(work, event);
            let conflict = CoderClaimConflictResult {
                ok: false,
                code: "coder_claim_conflict",
                error: "Hazardous shared resource is already claimed by another agent.",
                work_id: work_id.to_string(),
                call_id,
                requested_claims: claims,
                retry_after_utc: overlaps
                    .iter()
                    .filter(|overlap| overlap.blocked)
                    .map(|overlap| overlap.holder_expires_at_utc)
                    .max(),
                conflicts: overlaps,
                next_decision: "Inspect the conflicting agent intent in the ambient frame, choose a different non-overlapping action, or retry after the claim expires.",
            };
            self.write_index(&index)?;
            return Err(serde_json::to_string(&conflict).map_err(|err| err.to_string())?);
        }

        let replacements = work
            .active_claims
            .values()
            .filter(|active| {
                active.agent_id == identity.agent_id
                    && claims.iter().any(|claim| {
                        claim.target == active.scope.target && claim.mode == active.scope.mode
                    })
            })
            .count();
        if work
            .active_claims
            .len()
            .saturating_sub(replacements)
            .saturating_add(claims.len())
            > MAX_ACTIVE_CLAIMS_PER_WORK
        {
            let mut event = event(work_id, identity, CoderActivityKind::ToolBlocked);
            event.call_id = Some(call_id.clone());
            event.tool = Some(tool.to_string());
            event.intent = Some(intent);
            event.targets = targets;
            event.claims = claims;
            event.detail = Some("active coordination claim capacity reached".into());
            append_event(work, event);
            self.write_index(&index)?;
            return Err(json!({
                "ok": false,
                "code": "coder_claim_capacity",
                "error": "The bounded active coordination claim capacity has been reached.",
                "work_id": work_id,
                "call_id": call_id,
                "max_active_claims": MAX_ACTIVE_CLAIMS_PER_WORK,
                "next_decision": "Finish or release an active agent, or retry after its short claim TTL expires."
            })
            .to_string());
        }
        work.active_claims.retain(|_, active| {
            active.agent_id != identity.agent_id
                || !claims.iter().any(|claim| {
                    claim.target == active.scope.target && claim.mode == active.scope.mode
                })
        });

        let presence = work
            .agents
            .entry(identity.agent_id.clone())
            .or_insert_with(|| presence_from_identity(identity, now));
        presence.active = true;
        presence.heartbeat_at_utc = now;
        presence.current_tool = Some(tool.to_string());
        presence.current_intent = Some(intent.clone());
        presence.current_targets = targets.clone();
        presence.current_claims = claims.clone();
        presence.last_tool = Some(tool.to_string());
        presence.last_intent = Some(intent.clone());
        refresh_agent_claims(work, &identity.agent_id, now);

        for (index, scope) in claims.iter().cloned().enumerate() {
            let claim_id = format!("{call_id}:{index}");
            work.active_claims.insert(
                claim_id.clone(),
                CoderActiveClaim {
                    claim_id,
                    call_id: call_id.clone(),
                    agent_id: identity.agent_id.clone(),
                    session_id: identity.session_id.clone(),
                    turn_id: identity.turn_id.clone(),
                    attempt_id: identity.attempt_id.clone(),
                    tool: tool.to_string(),
                    intent: intent.clone(),
                    retained_after_call: scope.mode == CoderClaimMode::Write,
                    scope,
                    acquired_at_utc: now,
                    heartbeat_at_utc: now,
                    expires_at_utc: now + ACTIVE_CLAIM_TTL,
                },
            );
        }

        let mut planned = event(work_id, identity, CoderActivityKind::ToolPlanned);
        planned.call_id = Some(call_id.clone());
        planned.tool = Some(tool.to_string());
        planned.intent = Some(intent);
        planned.targets = targets;
        planned.claims = claims.clone();
        planned.overlaps = overlaps.clone();
        append_event(work, planned);
        self.write_index(&index)?;
        Ok(CoderToolActivityAdmission {
            call_id,
            claims,
            overlaps,
        })
    }

    pub fn heartbeat_claims(
        &self,
        work_id: &str,
        agent_id: &str,
        _call_id: &str,
    ) -> Result<(), String> {
        self.mutate(|index, now| {
            let Some(work) = index.work.get_mut(work_id) else {
                return;
            };
            if let Some(presence) = work.agents.get_mut(agent_id) {
                presence.heartbeat_at_utc = now;
            }
            refresh_agent_claims(work, agent_id, now);
            prune_work(work, now);
        })
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
            presence.current_claims.clear();
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
            let completed_claims = work
                .active_claims
                .values()
                .filter(|claim| claim.call_id == call_id)
                .map(|claim| claim.scope.clone())
                .collect::<Vec<_>>();
            completed.claims = completed_claims;
            completed.detail = Some(match result {
                Ok(output) => effect_summary(output),
                Err(error) => truncate(error, MAX_DETAIL_CHARS),
            });
            append_event(work, completed);
            work.active_claims.retain(|_, claim| {
                claim.call_id != call_id || (result.is_ok() && claim.retained_after_call)
            });
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
                presence.current_claims.clear();
            }
            work.active_claims
                .retain(|_, claim| claim.agent_id != identity.agent_id);
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

    pub(crate) fn events_for_work(&self, work_id: &str) -> Result<Vec<CoderActivityEvent>, String> {
        let _guard = self.lock.lock().map_err(|err| err.to_string())?;
        let index = self.read_index();
        Ok(index
            .work
            .get(work_id)
            .map(|work| work.events.clone())
            .unwrap_or_default())
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
        prune_work(index.work.entry(work_id.to_string()).or_default(), now);
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

    /// A small current pointer, never an acknowledgement of unseen events.
    pub fn context_pointer(&self, work_id: &str, self_agent_id: &str) -> Result<Value, String> {
        let _guard = self.lock.lock().map_err(|err| err.to_string())?;
        let index = self.read_index();
        let Some(work) = index.work.get(work_id) else {
            return Ok(json!({"status": "snapshot_required", "reason": "activity_unavailable"}));
        };
        let overlaps = active_overlaps(work, Utc::now());
        let mut overlap_targets: Vec<_> = overlaps
            .iter()
            .map(|overlap| overlap.target.chars().take(120).collect::<String>())
            .collect();
        overlap_targets.sort();
        overlap_targets.dedup();
        overlap_targets.truncate(3);
        Ok(json!({
            "epoch": work.epoch,
            "latest_revision": work.revision,
            "oldest_available_revision": oldest_revision(work),
            "peer_revision": work.events.iter().rev()
                .find(|event| event.agent_id != self_agent_id).map(|event| event.revision),
            "blocked_revision": work.events.iter().rev()
                .find(|event| event.kind == CoderActivityKind::ToolBlocked).map(|event| event.revision),
            "overlap_targets": overlap_targets,
            "overlap_count_at_least": overlaps.len(),
        }))
    }

    /// Read oldest-first within an immutable high-water mark. Only scanned
    /// revisions advance the returned cursor, including when filters match nothing.
    /// No mutable observation cursor is advanced by this read.
    pub fn read_delta(&self, work_id: &str, query: &CoderDeltaQuery) -> Result<Value, String> {
        let limit = query.limit.unwrap_or(MAX_DELTA_EVENTS);
        if !(1..=MAX_DELTA_EVENTS).contains(&limit) {
            return Err(format!("limit must be between 1 and {MAX_DELTA_EVENTS}"));
        }
        let _guard = self.lock.lock().map_err(|err| err.to_string())?;
        let index = self.read_index();
        let Some(work) = index.work.get(work_id) else {
            return Ok(
                json!({"ok": false, "status": "snapshot_required", "reason": "activity_unavailable"}),
            );
        };
        let through = query.through_revision.unwrap_or(work.revision);
        let reason = if query.epoch != work.epoch {
            Some("epoch_changed")
        } else if query.since_revision > work.revision || through > work.revision {
            Some("revision_discontinuity")
        } else if query.since_revision < oldest_revision(work).saturating_sub(1) {
            Some("retention_gap")
        } else {
            None
        };
        if let Some(reason) = reason {
            return Ok(
                json!({"ok": false, "status": "snapshot_required", "reason": reason,
                "epoch": work.epoch, "latest_revision": work.revision,
                "next_decision": "Read cognition_coder_context_read with mode snapshot before relying on this context."}),
            );
        }
        if through < query.since_revision {
            return Err("through_revision must be at least since_revision".into());
        }
        let mut events = Vec::new();
        let mut bytes = 0;
        let mut to_revision = query.since_revision;
        for event in work
            .events
            .iter()
            .filter(|event| event.revision > query.since_revision && event.revision <= through)
        {
            let matches = query
                .agent_id
                .as_ref()
                .is_none_or(|id| event.agent_id == *id)
                && query.kind.is_none_or(|kind| event.kind == kind)
                && query
                    .target
                    .as_ref()
                    .is_none_or(|target| event.targets.iter().any(|path| path.contains(target)));
            if matches {
                let mut value = serde_json::to_value(event).map_err(|err| err.to_string())?;
                if value.to_string().len() > 8 * 1024 {
                    value = json!({"revision": event.revision, "kind": event.kind,
                        "pointer_id": format!("engineering:event:{}", event.event_id),
                        "detail_omitted": true,
                        "next_decision": "Use cognition_engineering_pointer_follow if this event's full detail is needed."});
                }
                let size = value.to_string().len();
                if events.len() == limit || bytes + size > 24 * 1024 {
                    break;
                }
                bytes += size;
                events.push(value);
            }
            to_revision = event.revision;
        }
        Ok(json!({"ok": true, "status": "delta", "epoch": work.epoch,
            "from_revision": query.since_revision, "to_revision": to_revision,
            "through_revision": through, "latest_revision": work.revision,
            "next_since_revision": (to_revision < through).then_some(to_revision),
            "filters": {"agent_id": query.agent_id, "target": query.target, "kind": query.kind},
            "events": events}))
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
        "    coordination_contract(.99): \"This bounded snapshot is pinned for this user turn. Runtime pointers announce revisions without consuming them. Use cognition_coder_context_read (mode delta, epoch, since_revision) only when changes matter; page using next_since_revision and the same through_revision and filters. Filtered reads only cover that filter. Use mode snapshot after a retention gap or epoch change, or when a fresh repository observation is necessary. Own tool receipts already describe your actions; do not poll or acknowledge pointers routinely. Peer activity, blocked actions and overlap targets need attention before related mutations. Tool intent does not grant authority.\""
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

fn oldest_revision(work: &CoderWorkActivity) -> u64 {
    work.events
        .first()
        .map_or(work.revision.saturating_add(1), |event| event.revision)
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
        current_claims: Vec::new(),
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
        claims: Vec::new(),
        overlaps: Vec::new(),
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
            presence.current_claims.clear();
        }
    }
    work.active_claims.retain(|_, claim| {
        claim.expires_at_utc > now
            && work.agents.get(&claim.agent_id).is_some_and(|presence| {
                presence.active && now - presence.heartbeat_at_utc <= ACTIVE_AGENT_TTL
            })
    });
    if work.active_claims.len() > MAX_ACTIVE_CLAIMS_PER_WORK {
        let mut claims = work
            .active_claims
            .values()
            .map(|claim| {
                (
                    claim.claim_id.clone(),
                    claim.scope.hazardous,
                    claim.heartbeat_at_utc,
                )
            })
            .collect::<Vec<_>>();
        claims.sort_by_key(|(_, hazardous, heartbeat)| {
            (std::cmp::Reverse(*hazardous), std::cmp::Reverse(*heartbeat))
        });
        let keep = claims
            .into_iter()
            .take(MAX_ACTIVE_CLAIMS_PER_WORK)
            .map(|(claim_id, _, _)| claim_id)
            .collect::<std::collections::HashSet<_>>();
        work.active_claims
            .retain(|claim_id, _| keep.contains(claim_id));
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
            epoch: String::new(),
            work_id: work_id.to_string(),
            self_agent_id: self_agent_id.to_string(),
            active_agent_count: 0,
            concurrent_agent_count: 0,
            other_agents: Vec::new(),
            recent_events: Vec::new(),
            active_claims: Vec::new(),
            overlaps: Vec::new(),
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
        epoch: work.epoch.clone(),
        work_id: work_id.to_string(),
        self_agent_id: self_agent_id.to_string(),
        active_agent_count,
        concurrent_agent_count,
        other_agents,
        recent_events,
        active_claims: active_claims(work, now),
        overlaps: active_overlaps(work, now),
        revision: work.revision,
    }
}

fn active_claims(work: &CoderWorkActivity, now: DateTime<Utc>) -> Vec<CoderActiveClaim> {
    let mut claims = work
        .active_claims
        .values()
        .filter(|claim| claim.expires_at_utc > now)
        .cloned()
        .collect::<Vec<_>>();
    claims.sort_by(|left, right| {
        right
            .heartbeat_at_utc
            .cmp(&left.heartbeat_at_utc)
            .then_with(|| left.claim_id.cmp(&right.claim_id))
    });
    claims.truncate(MAX_AMBIENT_OVERLAPS * 2);
    claims
}

fn refresh_agent_claims(work: &mut CoderWorkActivity, agent_id: &str, now: DateTime<Utc>) {
    for claim in work
        .active_claims
        .values_mut()
        .filter(|claim| claim.agent_id == agent_id)
    {
        claim.heartbeat_at_utc = now;
        claim.expires_at_utc = now + ACTIVE_CLAIM_TTL;
    }
}

fn active_overlaps(work: &CoderWorkActivity, now: DateTime<Utc>) -> Vec<CoderClaimOverlap> {
    let claims = active_claims(work, now);
    let mut overlaps = Vec::new();
    for (index, left) in claims.iter().enumerate() {
        for right in claims.iter().skip(index + 1) {
            if left.agent_id == right.agent_id
                || !left.scope.mode.conflicts_with(right.scope.mode)
                || !targets_overlap(&left.scope.target, &right.scope.target)
            {
                continue;
            }
            overlaps.push(CoderClaimOverlap {
                target: left.scope.target.clone(),
                requested_mode: left.scope.mode,
                held_mode: right.scope.mode,
                hazardous: left.scope.hazardous || right.scope.hazardous,
                blocked: false,
                holder_agent_id: right.agent_id.clone(),
                holder_attempt_id: right.attempt_id.clone(),
                holder_tool: right.tool.clone(),
                holder_intent: right.intent.clone(),
                holder_expires_at_utc: right.expires_at_utc,
            });
            if overlaps.len() >= MAX_AMBIENT_OVERLAPS {
                return overlaps;
            }
        }
    }
    overlaps
}

fn claim_overlaps(
    work: &CoderWorkActivity,
    requesting_agent_id: &str,
    requested: &[CoderClaimScope],
) -> Vec<CoderClaimOverlap> {
    let mut overlaps = Vec::new();
    for request in requested {
        for held in work.active_claims.values() {
            if held.agent_id == requesting_agent_id
                || !request.mode.conflicts_with(held.scope.mode)
                || !targets_overlap(&request.target, &held.scope.target)
            {
                continue;
            }
            let hazardous = request.hazardous || held.scope.hazardous;
            overlaps.push(CoderClaimOverlap {
                target: request.target.clone(),
                requested_mode: request.mode,
                held_mode: held.scope.mode,
                hazardous,
                blocked: hazardous,
                holder_agent_id: held.agent_id.clone(),
                holder_attempt_id: held.attempt_id.clone(),
                holder_tool: held.tool.clone(),
                holder_intent: held.intent.clone(),
                holder_expires_at_utc: held.expires_at_utc,
            });
            if overlaps.len() >= MAX_AMBIENT_OVERLAPS {
                return overlaps;
            }
        }
    }
    overlaps
}

fn targets_overlap(left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }
    let prefix_overlap = |ancestor: &str, descendant: &str| {
        descendant
            .strip_prefix(ancestor)
            .is_some_and(|suffix| suffix.starts_with('/'))
    };
    prefix_overlap(left, right) || prefix_overlap(right, left)
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
    let stable_object_id = output
        .pointer("/change_set/id")
        .or_else(|| output.pointer("/action/id"))
        .or_else(|| output.pointer("/selection/id"))
        .or_else(|| output.get("workflow"));
    let stable_object_kind = output
        .pointer("/change_set/kind")
        .or_else(|| output.pointer("/action/kind"))
        .or_else(|| output.pointer("/selection/kind"))
        .or_else(|| output.get("query"));
    let summary = json!({
        "ok": output.get("ok"),
        "stable_object_id": stable_object_id,
        "stable_object_kind": stable_object_kind,
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

    fn claim(target: &str, mode: CoderClaimMode, hazardous: bool) -> CoderClaimScope {
        CoderClaimScope {
            target: target.into(),
            mode,
            hazardous,
            reason: "test claim".into(),
        }
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
    fn effect_summary_retains_stable_semantic_object_without_source_payload() {
        let summary = effect_summary(&json!({
            "ok": true,
            "change_set": {
                "id": "changeset:sha256:abc",
                "kind": "symbol_rename",
                "files": [{ "path": "src/lib.rs", "after": "private source" }]
            }
        }));
        assert!(summary.contains("changeset:sha256:abc"));
        assert!(summary.contains("symbol_rename"));
        assert!(!summary.contains("private source"));
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
                vec![CoderClaimScope {
                    target: "file://src/lib.rs".into(),
                    mode: CoderClaimMode::Read,
                    hazardous: false,
                    reason: "test".into(),
                }],
            )
            .expect("begin tool")
            .call_id;
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
    fn delta_pages_pin_boundary_and_survive_reopen_without_consuming_other_readers() {
        let temp = TempDir::new().unwrap();
        let activity = store(&temp);
        let a = identity("session-a", 1);
        activity.register_agent("work-1", &a).unwrap();
        let snapshot = activity.observe_initial("work-1", &a.agent_id).unwrap();
        activity
            .mutate(|index, _| {
                for _ in 0..5 {
                    append_event(
                        index.work.get_mut("work-1").unwrap(),
                        event("work-1", &a, CoderActivityKind::ToolCompleted),
                    );
                }
            })
            .unwrap();
        let mut query = CoderDeltaQuery {
            epoch: snapshot.epoch,
            since_revision: snapshot.revision,
            limit: Some(2),
            ..Default::default()
        };
        let first = activity.read_delta("work-1", &query).unwrap();
        assert_eq!(first["events"].as_array().unwrap().len(), 2);
        assert_eq!(first["to_revision"], 3);
        assert_eq!(first["through_revision"], 6);
        activity
            .register_agent("work-1", &identity("peer", 2))
            .unwrap();
        // Another snapshot/read must not acknowledge this reader's pending events.
        activity.observe_initial("work-1", &a.agent_id).unwrap();
        query.through_revision = first["through_revision"].as_u64();
        let reopened = store(&temp);
        assert_eq!(
            reopened.read_delta("work-1", &query).unwrap()["events"],
            first["events"]
        );
        query.since_revision = first["next_since_revision"].as_u64().unwrap();
        let second = reopened.read_delta("work-1", &query).unwrap();
        assert_eq!(second["to_revision"], 5);
        query.since_revision = second["next_since_revision"].as_u64().unwrap();
        let third = reopened.read_delta("work-1", &query).unwrap();
        assert_eq!(third["to_revision"], 6);
        assert_eq!(third["next_since_revision"], Value::Null);
        assert_eq!(third["latest_revision"], 7);
        query.since_revision = 6;
        query.through_revision = None;
        let pending = reopened.read_delta("work-1", &query).unwrap();
        assert_eq!(pending["events"][0]["revision"], 7);
        query.agent_id = Some("absent".into());
        let filtered = reopened.read_delta("work-1", &query).unwrap();
        assert_eq!(filtered["events"], json!([]));
        assert_eq!(filtered["to_revision"], 7);
        query.agent_id = None;
        query.target = Some("no-match".into());
        assert_eq!(
            reopened.read_delta("work-1", &query).unwrap()["events"],
            json!([])
        );
        query.target = None;
        query.kind = Some(CoderActivityKind::AgentJoined);
        assert_eq!(
            reopened.read_delta("work-1", &query).unwrap()["events"][0]["revision"],
            7
        );
    }

    #[test]
    fn delta_retention_and_epoch_gaps_require_explicit_snapshot() {
        let temp = TempDir::new().unwrap();
        let activity = store(&temp);
        let a = identity("a", 1);
        activity.register_agent("work-1", &a).unwrap();
        let snapshot = activity.observe_initial("work-1", &a.agent_id).unwrap();
        activity
            .mutate(|index, _| {
                for _ in 0..=MAX_EVENTS_PER_WORK {
                    append_event(
                        index.work.get_mut("work-1").unwrap(),
                        event("work-1", &a, CoderActivityKind::ToolCompleted),
                    );
                }
            })
            .unwrap();
        let mut query = CoderDeltaQuery {
            epoch: snapshot.epoch.clone(),
            since_revision: snapshot.revision,
            ..Default::default()
        };
        assert_eq!(
            activity.read_delta("work-1", &query).unwrap()["reason"],
            "retention_gap"
        );
        query.since_revision = 2;
        assert_eq!(
            activity.read_delta("work-1", &query).unwrap()["events"][0]["revision"],
            3
        );
        query.since_revision = 999;
        assert_eq!(
            activity.read_delta("work-1", &query).unwrap()["reason"],
            "revision_discontinuity"
        );
        std::fs::write(&activity.path, "corrupted").unwrap();
        assert_eq!(
            activity.read_delta("work-1", &query).unwrap()["reason"],
            "activity_unavailable"
        );
        let refreshed = activity.observe_initial("work-1", &a.agent_id).unwrap();
        assert_ne!(refreshed.epoch, snapshot.epoch);
        assert!(!refreshed.epoch.is_empty());
        assert_eq!(
            activity.read_delta("work-1", &query).unwrap()["reason"],
            "epoch_changed"
        );
        query.epoch = refreshed.epoch;
        query.since_revision = 0;
        assert_eq!(
            activity.read_delta("work-1", &query).unwrap()["next_since_revision"],
            Value::Null
        );
    }

    #[test]
    fn legacy_activity_gets_a_persisted_epoch_on_entry() {
        let temp = TempDir::new().unwrap();
        let activity = store(&temp);
        let a = identity("a", 1);
        activity.register_agent("work-1", &a).unwrap();
        let mut legacy: Value =
            serde_json::from_slice(&std::fs::read(&activity.path).unwrap()).unwrap();
        legacy["work"]["work-1"]
            .as_object_mut()
            .unwrap()
            .remove("epoch");
        std::fs::write(&activity.path, legacy.to_string()).unwrap();
        let snapshot = activity.observe_initial("work-1", &a.agent_id).unwrap();
        assert!(!snapshot.epoch.is_empty());
        assert_eq!(
            activity.context_pointer("work-1", &a.agent_id).unwrap()["epoch"],
            snapshot.epoch
        );
        assert_eq!(
            store(&temp).snapshot("work-1", &a.agent_id).unwrap().epoch,
            snapshot.epoch
        );
    }

    #[test]
    fn delta_payload_is_bounded_without_skipping_large_events() {
        let temp = TempDir::new().unwrap();
        let activity = store(&temp);
        let a = identity("a", 1);
        activity.register_agent("work-1", &a).unwrap();
        let snapshot = activity.observe_initial("work-1", &a.agent_id).unwrap();
        activity
            .mutate(|index, _| {
                for size in [50_000, 6_000, 6_000, 6_000, 6_000, 6_000] {
                    let mut ev = event("work-1", &a, CoderActivityKind::ToolCompleted);
                    ev.targets = vec!["x".repeat(size)];
                    append_event(index.work.get_mut("work-1").unwrap(), ev);
                }
            })
            .unwrap();
        let query = CoderDeltaQuery {
            epoch: snapshot.epoch,
            since_revision: snapshot.revision,
            ..Default::default()
        };
        let page = activity.read_delta("work-1", &query).unwrap();
        assert!(page.to_string().len() < 25 * 1024);
        assert_eq!(page["events"][0]["detail_omitted"], true);
        assert!(
            page["events"][0]["pointer_id"]
                .as_str()
                .unwrap()
                .starts_with("engineering:event:")
        );
        assert!(page["next_since_revision"].is_number());
        assert_eq!(
            page["to_revision"],
            page["events"].as_array().unwrap().last().unwrap()["revision"]
        );
    }

    #[test]
    fn ordinary_write_overlap_is_visible_but_does_not_block_isolated_agents() {
        let temp = TempDir::new().expect("temp");
        let store = store(&temp);
        let a = identity("session-a", 1);
        let b = identity("session-b", 2);
        store.register_agent("work-1", &a).expect("register a");
        store.register_agent("work-1", &b).expect("register b");
        let file_claim = claim("file://src/lib.rs", CoderClaimMode::Write, false);
        let first = store
            .begin_tool(
                "work-1",
                &a,
                "cognition_code_apply_patch",
                "Refactor the parser entry point",
                vec!["file://src/lib.rs".into()],
                vec![file_claim.clone()],
            )
            .expect("first admission");
        store
            .finish_tool(
                "work-1",
                &a,
                &first.call_id,
                "cognition_code_apply_patch",
                "Refactor the parser entry point",
                vec!["file://src/lib.rs".into()],
                Ok(&json!({ "ok": true })),
            )
            .expect("finish first");

        let second = store
            .begin_tool(
                "work-1",
                &b,
                "cognition_code_apply_patch",
                "Update the same parser for the new protocol",
                vec!["file://src/lib.rs".into()],
                vec![file_claim],
            )
            .expect("isolated overlap remains admissible");
        assert_eq!(second.overlaps.len(), 1);
        assert!(!second.overlaps[0].blocked);
        assert_eq!(second.overlaps[0].holder_agent_id, a.agent_id);

        let snapshot = store.snapshot("work-1", &a.agent_id).expect("snapshot");
        assert!(
            snapshot
                .overlaps
                .iter()
                .any(|overlap| { overlap.target == "file://src/lib.rs" && !overlap.blocked })
        );
    }

    #[test]
    fn hazardous_claim_conflict_is_structured_and_visible_to_both_agents() {
        let temp = TempDir::new().expect("temp");
        let store = store(&temp);
        let a = identity("session-a", 1);
        let b = identity("session-b", 2);
        store.register_agent("work-1", &a).expect("register a");
        store.register_agent("work-1", &b).expect("register b");
        store
            .observe_initial("work-1", &a.agent_id)
            .expect("initial a");
        let lock_claim = claim(
            "resource://lockfile/cargo.lock",
            CoderClaimMode::Write,
            true,
        );
        let first = store
            .begin_tool(
                "work-1",
                &a,
                "cognition_code_apply_patch",
                "Update the dependency lockfile",
                vec!["file://Cargo.lock".into()],
                vec![lock_claim.clone()],
            )
            .expect("first admission");
        store
            .finish_tool(
                "work-1",
                &a,
                &first.call_id,
                "cognition_code_apply_patch",
                "Update the dependency lockfile",
                vec!["file://Cargo.lock".into()],
                Ok(&json!({ "ok": true })),
            )
            .expect("finish first");

        let error = store
            .begin_tool(
                "work-1",
                &b,
                "cognition_code_apply_patch",
                "Regenerate the dependency lockfile",
                vec!["file://Cargo.lock".into()],
                vec![lock_claim],
            )
            .expect_err("hazard must serialize");
        let conflict: serde_json::Value = serde_json::from_str(&error).expect("structured error");
        assert_eq!(conflict["code"], "coder_claim_conflict");
        assert_eq!(conflict["conflicts"][0]["holder_agent_id"], a.agent_id);
        assert!(conflict["retry_after_utc"].is_string());

        let pointer = store.context_pointer("work-1", &a.agent_id).unwrap();
        assert!(pointer["blocked_revision"].is_number());
        assert!(pointer["peer_revision"].is_number());
        assert!(pointer.to_string().len() < 1024);
        let events = store.events_for_work("work-1").unwrap();
        assert!(events.iter().any(|event| {
            event.kind == CoderActivityKind::ToolBlocked
                && event.agent_id == b.agent_id
                && event.overlaps.iter().any(|overlap| overlap.blocked)
        }));
        let b_snapshot = store.snapshot("work-1", &b.agent_id).expect("snapshot b");
        assert!(b_snapshot.recent_events.iter().any(|event| {
            event.kind == CoderActivityKind::ToolCompleted && event.agent_id == a.agent_id
        }));
    }

    #[test]
    fn expired_hazardous_claim_releases_serialization_slot() {
        let temp = TempDir::new().expect("temp");
        let store = store(&temp);
        let a = identity("session-a", 1);
        let b = identity("session-b", 2);
        store.register_agent("work-1", &a).expect("register a");
        store.register_agent("work-1", &b).expect("register b");
        let deployment = claim("resource://deployment/default", CoderClaimMode::Write, true);
        let first = store
            .begin_tool(
                "work-1",
                &a,
                "cognition_shell_session_run",
                "Deploy the candidate build",
                vec!["attempt://a".into()],
                vec![deployment.clone()],
            )
            .expect("first admission");
        store
            .mutate(|index, now| {
                let work = index.work.get_mut("work-1").expect("work");
                for active in work
                    .active_claims
                    .values_mut()
                    .filter(|active| active.call_id == first.call_id)
                {
                    active.expires_at_utc = now - Duration::seconds(1);
                }
            })
            .expect("expire claim");

        let second = store
            .begin_tool(
                "work-1",
                &b,
                "cognition_shell_session_run",
                "Deploy after the prior claim expired",
                vec!["attempt://b".into()],
                vec![deployment],
            )
            .expect("expired claim should not block");
        assert!(second.overlaps.is_empty());
    }
}
