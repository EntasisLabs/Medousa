//! Home-facing Forge projections (allowed actions, review summary).
//!
//! Keeps unbounded patch/command payloads out of `WorkItem` responses.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use medousa_forge::events::{EventPayload, SideEffect};
use medousa_forge::forge::Forge;
use medousa_forge::model::{
    ActorKind, AttemptState, EvidenceId, EvidenceManifest, WorkItem, WorkState,
};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ActionAffordance {
    pub allowed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl ActionAffordance {
    pub fn yes() -> Self {
        Self {
            allowed: true,
            reason: None,
        }
    }

    pub fn no(reason: impl Into<String>) -> Self {
        Self {
            allowed: false,
            reason: Some(reason.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AllowedActions {
    pub provision: ActionAffordance,
    pub start_agent: ActionAffordance,
    pub open_terminal: ActionAffordance,
    pub begin_attempt: ActionAffordance,
    pub seal: ActionAffordance,
    pub review: ActionAffordance,
    pub apply: ActionAffordance,
    pub discard: ActionAffordance,
}

pub fn allowed_actions(item: &WorkItem) -> AllowedActions {
    let has_env = item.workspace_environment().is_some();
    let has_running = item.active_attempt_ids().into_iter().any(|id| {
        item.attempt(id).is_some_and(|attempt| {
            attempt.state == AttemptState::Running && attempt.lease.is_some()
        })
    });
    let has_sealed_evidence = item.attempts.iter().any(|a| a.evidence_id.is_some());
    let has_decision = !item.review_decisions.is_empty();

    AllowedActions {
        provision: match item.state {
            WorkState::Draft => ActionAffordance::yes(),
            _ if has_env => ActionAffordance::no("Already provisioned"),
            _ => ActionAffordance::no(format!("Cannot provision in state {}", item.state)),
        },
        start_agent: match item.state {
            WorkState::Ready | WorkState::Executing => ActionAffordance::yes(),
            WorkState::Draft | WorkState::Provisioning => {
                ActionAffordance::no("Provision the undertaking first")
            }
            _ => ActionAffordance::no(format!("Cannot start agent in state {}", item.state)),
        },
        open_terminal: if has_env {
            ActionAffordance::yes()
        } else {
            ActionAffordance::no("No governed worktree yet")
        },
        begin_attempt: match item.state {
            WorkState::Ready if !has_running => ActionAffordance::yes(),
            WorkState::Ready if has_running => ActionAffordance::no("Attempt already running"),
            WorkState::Executing => ActionAffordance::no("Attempt already running"),
            _ => ActionAffordance::no(format!("Cannot begin attempt in state {}", item.state)),
        },
        seal: if has_running {
            ActionAffordance::yes()
        } else {
            ActionAffordance::no("No active attempt")
        },
        review: match item.state {
            WorkState::AwaitingReview | WorkState::ApplyingDecision if has_sealed_evidence => {
                ActionAffordance::yes()
            }
            WorkState::AwaitingReview => ActionAffordance::no("No sealed evidence yet"),
            _ => ActionAffordance::no("Work is not sealed"),
        },
        apply: match item.state {
            WorkState::AwaitingReview if has_decision => ActionAffordance::yes(),
            WorkState::AwaitingReview => ActionAffordance::no("Review decision required"),
            WorkState::ApplyingDecision => ActionAffordance::yes(),
            _ => ActionAffordance::no(format!("Cannot apply in state {}", item.state)),
        },
        discard: if item.state.is_terminal() {
            ActionAffordance::no("Work is already terminal")
        } else {
            ActionAffordance::yes()
        },
    }
}

pub fn human_phase(state: WorkState) -> &'static str {
    match state {
        WorkState::Draft | WorkState::Provisioning => "prepare",
        WorkState::Ready | WorkState::Executing => "work",
        WorkState::Sealing | WorkState::AwaitingReview | WorkState::ApplyingDecision => "review",
        WorkState::Accepted | WorkState::Discarded => "complete",
        WorkState::Failed => "needs_attention",
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ChangedFileSummary {
    pub path: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_path: Option<String>,
    pub is_binary: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub byte_size: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReviewVerification {
    pub label: String,
    pub command: Vec<String>,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReviewSynthesis {
    pub outcome: String,
    pub status: String,
    pub status_summary: String,
    pub risk: String,
    pub risk_summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification: Option<ReviewVerification>,
    pub unresolved_issues: Vec<String>,
    pub recommended_next_action: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReviewAttribution {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub state: String,
    pub started_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<DateTime<Utc>>,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReviewTimelineEntry {
    pub id: String,
    pub at: DateTime<Utc>,
    pub kind: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub actor_kind: String,
    pub actor_label: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReviewProjection {
    pub work_id: String,
    pub title: String,
    pub state: String,
    pub human_phase: String,
    pub allowed_actions: AllowedActions,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline_oid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sealed_head_oid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt_seq: Option<u32>,
    pub changed_files: Vec<ChangedFileSummary>,
    pub synthesis: ReviewSynthesis,
    pub attribution: Vec<ReviewAttribution>,
    pub timeline: Vec<ReviewTimelineEntry>,
    pub truncated: bool,
    pub base_advanced: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy: Option<serde_json::Value>,
    pub command_log_lines: usize,
    pub compact_receipt_count: u64,
    pub compact_receipt_rejections: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compact_receipts_digest: Option<String>,
    pub patch_byte_size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disposition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_lease_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_lease_generation: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub world: Option<serde_json::Value>,
}

pub fn evidence_dir(forge: &Forge, item: &WorkItem, evidence_id: &EvidenceId) -> Option<PathBuf> {
    for attempt in &item.attempts {
        if attempt.evidence_id.as_ref() == Some(evidence_id) {
            return Some(
                forge
                    .store()
                    .item_dir(&item.id)
                    .join("attempts")
                    .join(attempt.seq.to_string())
                    .join("evidence"),
            );
        }
    }
    None
}

pub fn load_manifest(dir: &std::path::Path) -> Option<EvidenceManifest> {
    let bytes = std::fs::read(dir.join("manifest.json")).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub fn build_review(forge: &Forge, item: &WorkItem) -> ReviewProjection {
    let env = item.workspace_environment();
    let sealed_attempt = item.attempts.iter().rev().find(|a| a.evidence_id.is_some());
    let evidence_id = sealed_attempt.and_then(|a| a.evidence_id.clone());
    let mut changed_files = Vec::new();
    let mut truncated = false;
    let mut base_advanced = false;
    let mut evidence_digest = None;
    let mut sealed_head = None;
    let mut policy = None;
    let mut command_log_lines = 0usize;
    let mut patch_byte_size = 0u64;
    let mut compact_receipt_count = 0u64;
    let mut compact_receipt_rejections = 0u64;
    let mut compact_receipts_digest = None;
    let mut verification = None;

    if let Some(eid) = evidence_id.as_ref()
        && let Some(dir) = evidence_dir(forge, item, eid)
    {
        if let Some(manifest) = load_manifest(&dir) {
            truncated = manifest.truncated;
            base_advanced = manifest.base_advanced;
            evidence_digest = manifest
                .bundle_digest
                .as_ref()
                .map(|d| d.as_str().to_owned());
            sealed_head = Some(manifest.sealed_head_oid.as_str().to_owned());
            compact_receipt_count = manifest.compact_receipt_count;
            compact_receipt_rejections = manifest.compact_receipt_rejections;
            compact_receipts_digest = manifest
                .compact_receipts_digest
                .as_ref()
                .map(|digest| digest.as_str().to_owned());
            changed_files = manifest
                .changed_files
                .iter()
                .map(|f| ChangedFileSummary {
                    path: f.path.clone(),
                    status: match f.status {
                        medousa_forge::model::ChangeStatus::Added => "added",
                        medousa_forge::model::ChangeStatus::Modified => "modified",
                        medousa_forge::model::ChangeStatus::Deleted => "deleted",
                        medousa_forge::model::ChangeStatus::Renamed => "renamed",
                        medousa_forge::model::ChangeStatus::Copied => "copied",
                        medousa_forge::model::ChangeStatus::TypeChanged => "type_changed",
                        medousa_forge::model::ChangeStatus::Untracked => "untracked",
                    }
                    .to_owned(),
                    old_path: f.old_path.clone(),
                    is_binary: f.is_binary,
                    byte_size: f.byte_size,
                })
                .collect();
        }
        if let Ok(meta) = std::fs::metadata(dir.join("patch.diff")) {
            patch_byte_size = meta.len();
        }
        if let Ok(commands) = std::fs::read_to_string(dir.join("commands.jsonl")) {
            command_log_lines = commands.lines().filter(|l| !l.trim().is_empty()).count();
            verification = commands.lines().rev().find_map(parse_verification);
        }
        if let Ok(bytes) = std::fs::read(dir.join("policy.json")) {
            policy = serde_json::from_slice(&bytes).ok();
        }
    }

    let active = item.latest_active_attempt().and_then(|a| a.lease.as_ref());

    let policy_issues = policy
        .as_ref()
        .and_then(serde_json::Value::as_object)
        .map(|report| {
            report
                .get("violations")
                .and_then(serde_json::Value::as_array)
                .map(Vec::len)
                .unwrap_or(0)
                + report
                    .get("capture_risks")
                    .and_then(serde_json::Value::as_array)
                    .map(Vec::len)
                    .unwrap_or(0)
        })
        .unwrap_or(0);
    let mut unresolved_issues = Vec::new();
    if base_advanced {
        unresolved_issues
            .push("The starting branch changed while this work was in progress.".into());
    }
    if truncated {
        unresolved_issues
            .push("Some evidence was shortened by the configured capture limits.".into());
    }
    if compact_receipt_rejections > 0 {
        unresolved_issues.push(format!(
            "{compact_receipt_rejections} compact evidence receipt{} could not be validated at seal.",
            if compact_receipt_rejections == 1 { "" } else { "s" }
        ));
    }
    if policy_issues > 0 {
        unresolved_issues.push(format!(
            "{policy_issues} policy or content risk{} need review.",
            if policy_issues == 1 { "" } else { "s" }
        ));
    }
    if verification.as_ref().is_some_and(|result| !result.success) {
        unresolved_issues.push("The latest project check did not pass.".into());
    } else if verification.is_none() {
        unresolved_issues.push("No project check was recorded for this revision.".into());
    }
    let risk = if policy_issues > 0 || base_advanced {
        "attention"
    } else if verification.as_ref().is_some_and(|result| !result.success) {
        "high"
    } else {
        "low"
    };
    let status = if unresolved_issues.is_empty() {
        "ready"
    } else if verification.as_ref().is_some_and(|result| !result.success) || policy_issues > 0 {
        "needs_attention"
    } else {
        "review"
    };
    let synthesis = ReviewSynthesis {
        outcome: item.brief.clone(),
        status: status.into(),
        status_summary: match status {
            "ready" => "The intended change is ready for your decision.",
            "needs_attention" => "A recorded result deserves your attention before finishing.",
            _ => "The changes are ready to inspect; one confirmation remains.",
        }
        .into(),
        risk: risk.into(),
        risk_summary: match risk {
            "high" => "Verification failed; inspect the affected code before continuing.",
            "attention" => "Forge found a branch or policy condition that needs a human decision.",
            _ => "Forge found no recorded policy or branch risks.",
        }
        .into(),
        verification,
        unresolved_issues,
        recommended_next_action: match status {
            "ready" => "Approve the revision or inspect any file that matters to you.",
            "needs_attention" => {
                "Open the highlighted evidence, then revise or approve explicitly."
            }
            _ => "Inspect the changed files and decide whether to finish or revise.",
        }
        .into(),
    };
    let sealed_attempt_id = sealed_attempt.map(|attempt| attempt.id.as_str());
    let changed_paths: Vec<String> = changed_files.iter().map(|file| file.path.clone()).collect();
    let mut attribution: Vec<ReviewAttribution> = item
        .attempts
        .iter()
        .map(|attempt| {
            let executor = attempt.executor.kind.trim().to_ascii_lowercase();
            let kind = match executor.as_str() {
                "human" => "human",
                "terminal" => "terminal",
                "codex" | "cursor" | "agent" | "script" => "agent",
                _ => "agent",
            };
            ReviewAttribution {
                id: attempt.id.as_str().to_owned(),
                kind: kind.into(),
                label: match executor.as_str() {
                    "human" => "You".into(),
                    "terminal" => "Terminal".into(),
                    "codex" => "Codex".into(),
                    "cursor" => "Cursor".into(),
                    "script" => "Automation".into(),
                    _ => attempt.executor.kind.clone(),
                },
                state: match attempt.state {
                    AttemptState::Running => "working",
                    AttemptState::Completed => "finished",
                    AttemptState::Interrupted => "paused",
                    AttemptState::Failed => "needs attention",
                }
                .into(),
                started_at: attempt.started_at,
                ended_at: attempt.ended_at,
                files: if sealed_attempt_id == Some(attempt.id.as_str()) {
                    changed_paths.clone()
                } else {
                    Vec::new()
                },
            }
        })
        .collect();
    if synthesis.verification.is_some() {
        attribution.push(ReviewAttribution {
            id: "verification".into(),
            kind: "verification".into(),
            label: "Project check".into(),
            state: if synthesis
                .verification
                .as_ref()
                .is_some_and(|result| result.success)
            {
                "passed"
            } else {
                "failed"
            }
            .into(),
            started_at: sealed_attempt
                .and_then(|attempt| attempt.ended_at)
                .unwrap_or(item.updated_at),
            ended_at: None,
            files: Vec::new(),
        });
    }
    let timeline = build_timeline(forge, item);

    ReviewProjection {
        work_id: item.id.as_str().to_owned(),
        title: item.title.clone(),
        state: item.state.to_string(),
        human_phase: human_phase(item.state).to_owned(),
        allowed_actions: allowed_actions(item),
        baseline_oid: env.map(|e| e.baseline_oid.as_str().to_owned()),
        sealed_head_oid: sealed_head,
        evidence_id: evidence_id.map(|e| e.as_str().to_owned()),
        evidence_digest,
        attempt_id: sealed_attempt.map(|a| a.id.as_str().to_owned()),
        attempt_seq: sealed_attempt.map(|a| a.seq),
        changed_files,
        synthesis,
        attribution,
        timeline,
        truncated,
        base_advanced,
        policy,
        command_log_lines,
        compact_receipt_count,
        compact_receipt_rejections,
        compact_receipts_digest,
        patch_byte_size,
        decision: item
            .review_decisions
            .last()
            .and_then(|d| serde_json::to_value(d).ok()),
        disposition: item
            .disposition
            .map(|d| format!("{d:?}").to_ascii_lowercase()),
        worktree: env.map(|e| e.worktree.display().to_string()),
        active_lease_id: active.map(|l| l.lease_id.as_str().to_owned()),
        active_lease_generation: active.map(|l| l.generation),
        world: None,
    }
}

fn parse_verification(line: &str) -> Option<ReviewVerification> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    if value.get("kind")?.as_str()? != "project_task" {
        return None;
    }
    let task = value.get("task")?;
    Some(ReviewVerification {
        label: task.get("label")?.as_str()?.to_owned(),
        command: task
            .get("argv")
            .and_then(serde_json::Value::as_array)
            .map(|parts| {
                parts
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default(),
        success: value
            .get("success")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        exit_code: value.get("exit_code").and_then(serde_json::Value::as_i64),
        duration_ms: value.get("duration_ms").and_then(serde_json::Value::as_u64),
    })
}

fn build_timeline(forge: &Forge, item: &WorkItem) -> Vec<ReviewTimelineEntry> {
    forge
        .store()
        .replay(&item.id)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|event| {
            let (kind, label, detail) = match event.payload {
                EventPayload::ItemRegistered { .. } => ("intent", "Project created", None),
                EventPayload::EnvironmentProvisioned { .. } => {
                    ("workspace", "Isolated working copy prepared", None)
                }
                EventPayload::AttemptStarted { attempt } => {
                    let executor = attempt.executor.kind;
                    (
                        "attempt",
                        "Work started",
                        Some(format!("{} took the project", human_executor(&executor))),
                    )
                }
                EventPayload::AttemptEnded { state, .. } => match state {
                    AttemptState::Completed => ("attempt", "Work completed", None),
                    AttemptState::Interrupted => ("attempt", "Work paused safely", None),
                    AttemptState::Failed => ("attempt", "Work needs attention", None),
                    AttemptState::Running => ("attempt", "Work continued", None),
                },
                EventPayload::OperationSideEffect {
                    effect: SideEffect::CheckpointCommitCreated { oid, .. },
                    ..
                } => (
                    "checkpoint",
                    "Recovery point saved",
                    Some(oid.as_str().chars().take(10).collect()),
                ),
                EventPayload::EvidenceSealed { .. } => {
                    ("evidence", "Revision prepared for review", None)
                }
                EventPayload::ReviewDecided { .. } => ("decision", "Changes approved", None),
                EventPayload::DecisionInvalidated { reason, .. } => {
                    ("decision", "Approval set aside", Some(reason))
                }
                EventPayload::DispositionApplied { .. } => ("outcome", "Project finished", None),
                _ => return None,
            };
            Some(ReviewTimelineEntry {
                id: format!("event-{}", event.seq),
                at: event.at,
                kind: kind.into(),
                label: label.into(),
                detail,
                actor_kind: match event.actor.kind {
                    ActorKind::User => "human",
                    ActorKind::Profile => "agent",
                    ActorKind::System => "system",
                }
                .into(),
                actor_label: match event.actor.kind {
                    ActorKind::User => "You".into(),
                    ActorKind::Profile => "Agent".into(),
                    ActorKind::System => "Medousa".into(),
                },
            })
        })
        .collect()
}

fn human_executor(kind: &str) -> String {
    match kind.trim().to_ascii_lowercase().as_str() {
        "human" => "You".into(),
        "terminal" => "Terminal".into(),
        "codex" => "Codex".into(),
        "cursor" => "Cursor".into(),
        "script" => "Automation".into(),
        _ => kind.to_owned(),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ItemProjection {
    #[serde(flatten)]
    pub item: WorkItem,
    pub human_phase: String,
    pub allowed_actions: AllowedActions,
}

pub fn project_item(mut item: WorkItem) -> ItemProjection {
    let human = human_phase(item.state).to_owned();
    let actions = allowed_actions(&item);
    // `environment` is the long-lived staging anchor in durable Forge state.
    // Existing clients already understand that field, so project the current
    // lease-owned workspace through it without mutating the stored item.
    if let Some(environment) = item.workspace_environment().cloned() {
        item.environment = Some(environment);
    }
    ItemProjection {
        item,
        human_phase: human,
        allowed_actions: actions,
    }
}

pub fn project_items(items: Vec<WorkItem>) -> Vec<ItemProjection> {
    items.into_iter().map(project_item).collect()
}

/// Paginate a text file by line range (1-based inclusive end optional).
pub fn read_lines_page(
    path: &std::path::Path,
    offset: usize,
    limit: usize,
) -> Result<(Vec<String>, usize, bool), String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let all: Vec<&str> = text.lines().collect();
    let total = all.len();
    let start = offset.min(total);
    let end = (start + limit).min(total);
    let lines = all[start..end].iter().map(|s| (*s).to_owned()).collect();
    let truncated = end < total;
    Ok((lines, total, truncated))
}
