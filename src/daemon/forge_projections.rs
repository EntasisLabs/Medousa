//! Home-facing Forge projections (allowed actions, review summary).
//!
//! Keeps unbounded patch/command payloads out of `WorkItem` responses.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use medousa_forge::events::{EventPayload, SideEffect};
use medousa_forge::forge::Forge;
use medousa_forge::model::{
    ActorKind, AttemptId, AttemptState, EvidenceId, EvidenceManifest, ReviewComment, WorkItem,
    WorkState,
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
    /// Reopen sealed review for human edits without an agent handoff.
    pub continue_editing: ActionAffordance,
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
            WorkState::Ready | WorkState::Executing => ActionAffordance::yes(),
            _ => ActionAffordance::no(format!("Cannot begin attempt in state {}", item.state)),
        },
        continue_editing: match item.state {
            WorkState::AwaitingReview if has_sealed_evidence => ActionAffordance::yes(),
            WorkState::AwaitingReview => ActionAffordance::no("No sealed evidence yet"),
            _ => ActionAffordance::no(format!("Cannot continue editing in state {}", item.state)),
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
        discard: match item.state {
            WorkState::Draft
            | WorkState::Ready
            | WorkState::Executing
            | WorkState::AwaitingReview => ActionAffordance::yes(),
            state if state.is_terminal() => ActionAffordance::no("Work is already terminal"),
            state => ActionAffordance::no(format!("Cannot discard in state {state}")),
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
pub struct ReviewSymbolScope {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub line_start: u32,
    pub line_end: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<String>,
    pub lines_added: u32,
    pub lines_removed: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub intents: Vec<String>,
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
    #[serde(default)]
    pub lines_added: u32,
    #[serde(default)]
    pub lines_removed: u32,
    /// Unique Coder tool intents that touched this path (skim without opening a diff).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub intents: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_intent: Option<String>,
    /// Number of nested symbol scopes (0 until World enrichment).
    #[serde(default)]
    pub symbol_count: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<ReviewSymbolScope>,
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
pub struct ReviewIssue {
    pub id: String,
    pub message: String,
    /// `high` | `attention` | `info`
    pub severity: String,
    pub blocks_approval: bool,
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
    /// Flat message list retained for older clients.
    pub unresolved_issues: Vec<String>,
    /// Severity-ranked issues; risk/status/blocking all derive from this list.
    #[serde(default)]
    pub issues: Vec<ReviewIssue>,
    /// True when any issue has `blocks_approval`.
    #[serde(default)]
    pub blocks_approval: bool,
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
    /// Number of attempts/contributions collapsed into this chip.
    #[serde(default, skip_serializing_if = "is_one")]
    pub count: u32,
}

fn is_one(value: &u32) -> bool {
    *value == 1
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
pub struct ReviewCommentProjection {
    pub id: String,
    pub thread_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub evidence_id: String,
    pub attempt_id: String,
    pub path: String,
    pub side: String,
    pub start_line: u32,
    pub end_line: u32,
    pub anchor_digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor_text: Option<String>,
    pub body: String,
    pub actor_kind: String,
    pub actor_id: String,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_by_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_by_id: Option<String>,
}

impl ReviewCommentProjection {
    pub fn from_comment(comment: &ReviewComment) -> Self {
        Self {
            id: comment.id.as_str().to_owned(),
            thread_id: comment.thread_id.as_str().to_owned(),
            parent_id: comment.parent_id.as_ref().map(|id| id.as_str().to_owned()),
            evidence_id: comment.evidence_id.as_str().to_owned(),
            attempt_id: comment.attempt_id.as_str().to_owned(),
            path: comment.path.clone(),
            side: comment.side.clone(),
            start_line: comment.start_line,
            end_line: comment.end_line,
            anchor_digest: comment.anchor_digest.clone(),
            anchor_text: comment.anchor_text.clone(),
            body: comment.body.clone(),
            actor_kind: match comment.actor.kind {
                ActorKind::User => "user",
                ActorKind::Profile => "profile",
                ActorKind::System => "system",
            }
            .into(),
            actor_id: comment.actor.id.clone(),
            created_at: comment.created_at,
            resolved_at: comment.resolved_at,
            resolved_by_kind: comment.resolved_by.as_ref().map(|actor| {
                match actor.kind {
                    ActorKind::User => "user",
                    ActorKind::Profile => "profile",
                    ActorKind::System => "system",
                }
                .into()
            }),
            resolved_by_id: comment.resolved_by.as_ref().map(|actor| actor.id.clone()),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ReviewProjection {
    pub work_id: String,
    pub title: String,
    pub state: String,
    pub human_phase: String,
    pub allowed_actions: AllowedActions,
    pub candidates: Vec<ReviewCandidateProjection>,
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
    pub comments: Vec<ReviewCommentProjection>,
    pub unresolved_comment_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision_brief: Option<String>,
    /// Paths that differ from the previously reviewed attempt (after request-changes).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changed_since_previous: Vec<String>,
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

#[derive(Debug, Clone, Serialize)]
pub struct ReviewCandidateProjection {
    pub attempt_id: String,
    pub attempt_seq: u32,
    pub executor: String,
    pub evidence_id: String,
    pub evidence_digest: String,
    pub baseline_oid: String,
    pub sealed_head_oid: String,
    pub branch: String,
    pub worktree: String,
    pub changed_file_count: usize,
    pub sealed_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_id: Option<String>,
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
    build_review_for_attempt(forge, item, None)
}

pub fn build_review_for_attempt(
    forge: &Forge,
    item: &WorkItem,
    selected_attempt_id: Option<&AttemptId>,
) -> ReviewProjection {
    let candidates = review_candidates(forge, item);
    let sealed_attempt = selected_attempt_id
        .and_then(|attempt_id| {
            item.attempt(attempt_id)
                .filter(|attempt| attempt.evidence_id.is_some())
        })
        .or_else(|| item.attempts.iter().rev().find(|a| a.evidence_id.is_some()));
    let env = sealed_attempt
        .and_then(|attempt| attempt.environment.as_ref())
        .or_else(|| item.workspace_environment());
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
    let mut intents_by_path: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    let mut patch_line_stats: std::collections::HashMap<String, (u32, u32)> =
        std::collections::HashMap::new();

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
                        medousa_forge::model::ChangeStatus::Unmerged => "unmerged",
                    }
                    .to_owned(),
                    old_path: f.old_path.clone(),
                    is_binary: f.is_binary,
                    byte_size: f.byte_size,
                    lines_added: 0,
                    lines_removed: 0,
                    intents: Vec::new(),
                    primary_intent: None,
                    symbol_count: 0,
                    scopes: Vec::new(),
                })
                .collect();
        }
        if let Ok(meta) = std::fs::metadata(dir.join("patch.diff")) {
            patch_byte_size = meta.len();
        }
        if let Ok(patch) = std::fs::read_to_string(dir.join("patch.diff")) {
            patch_line_stats = count_patch_line_stats(&patch);
        }
        if let Ok(commands) = std::fs::read_to_string(dir.join("commands.jsonl")) {
            command_log_lines = commands.lines().filter(|l| !l.trim().is_empty()).count();
            verification = commands.lines().rev().find_map(parse_verification);
            intents_by_path = collect_coder_intents_by_path(&commands);
        }
        if let Ok(bytes) = std::fs::read(dir.join("policy.json")) {
            policy = serde_json::from_slice(&bytes).ok();
        }
    }

    for file in &mut changed_files {
        if let Some((added, removed)) = patch_line_stats.get(&file.path) {
            file.lines_added = *added;
            file.lines_removed = *removed;
        }
        if let Some(intents) = intents_by_path.get(&file.path) {
            file.intents = intents.clone();
            file.primary_intent = intents.first().cloned();
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

    let empty_seal = evidence_id.is_some() && changed_files.is_empty();
    let issues = collect_review_issues(
        base_advanced,
        truncated,
        compact_receipt_rejections,
        policy_issues,
        verification.as_ref(),
        empty_seal,
    );
    let synthesis = synthesize_review(item.brief.clone(), verification, issues);

    let sealed_attempt_id = sealed_attempt.map(|attempt| attempt.id.as_str());
    let changed_paths: Vec<String> = changed_files.iter().map(|file| file.path.clone()).collect();
    let mut attribution = collect_attribution(item, sealed_attempt_id, &changed_paths);
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
            count: 1,
        });
    }
    let timeline = build_timeline(forge, item);

    let evidence_id_ref = evidence_id.as_ref();
    let comments: Vec<ReviewCommentProjection> = item
        .review_comments
        .iter()
        .filter(|comment| evidence_id_ref.is_none_or(|eid| &comment.evidence_id == eid))
        .map(ReviewCommentProjection::from_comment)
        .collect();
    let unresolved_comment_count = comments
        .iter()
        .filter(|comment| comment.resolved_at.is_none())
        .count();
    let revision_brief = item
        .changes_requested
        .iter()
        .rev()
        .find(|request| evidence_id_ref.is_none_or(|eid| &request.evidence_id == eid))
        .map(|request| request.revision_brief.clone())
        .or_else(|| {
            let unresolved: Vec<&ReviewComment> = item
                .review_comments
                .iter()
                .filter(|comment| {
                    comment.resolved_at.is_none()
                        && evidence_id_ref.is_none_or(|eid| &comment.evidence_id == eid)
                })
                .collect();
            if unresolved.is_empty() {
                None
            } else {
                let brief = compose_revision_brief(unresolved, None);
                if brief.trim().is_empty() {
                    None
                } else {
                    Some(brief)
                }
            }
        });

    let changed_since_previous = changed_since_previous_paths(
        forge,
        item,
        sealed_attempt.map(|a| a.id.as_str()),
        &changed_files,
    );

    ReviewProjection {
        work_id: item.id.as_str().to_owned(),
        title: item.title.clone(),
        state: item.state.to_string(),
        human_phase: human_phase(item.state).to_owned(),
        allowed_actions: allowed_actions(item),
        candidates,
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
        comments,
        unresolved_comment_count,
        revision_brief,
        changed_since_previous,
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
            .iter()
            .rev()
            .find(|decision| {
                sealed_attempt.is_some_and(|attempt| decision.attempt_id == attempt.id)
            })
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

fn issue(
    id: &str,
    message: impl Into<String>,
    severity: &str,
    blocks_approval: bool,
) -> ReviewIssue {
    ReviewIssue {
        id: id.into(),
        message: message.into(),
        severity: severity.into(),
        blocks_approval,
    }
}

/// Build a severity-ranked issue list. Risk, status, summary copy, and
/// approval-blocking all derive from this list — never from parallel if-chains.
pub fn collect_review_issues(
    base_advanced: bool,
    truncated: bool,
    compact_receipt_rejections: u64,
    policy_issues: usize,
    verification: Option<&ReviewVerification>,
    empty_seal: bool,
) -> Vec<ReviewIssue> {
    let mut issues = Vec::new();
    if empty_seal {
        issues.push(issue(
            "empty_seal",
            "This revision has no file changes to approve.",
            "attention",
            true,
        ));
    }
    if verification.is_some_and(|result| !result.success) {
        issues.push(issue(
            "verification_failed",
            "The latest project check did not pass.",
            "high",
            true,
        ));
    }
    if policy_issues > 0 {
        issues.push(issue(
            "policy",
            format!(
                "{policy_issues} policy or content risk{} need review.",
                if policy_issues == 1 { "" } else { "s" }
            ),
            "attention",
            true,
        ));
    }
    if base_advanced {
        issues.push(issue(
            "base_advanced",
            "The starting branch changed while this work was in progress.",
            "attention",
            true,
        ));
    }
    if truncated {
        issues.push(issue(
            "truncated",
            "Some evidence was shortened by the configured capture limits.",
            "attention",
            false,
        ));
    }
    if compact_receipt_rejections > 0 {
        issues.push(issue(
            "compact_receipts",
            format!(
                "{compact_receipt_rejections} compact evidence receipt{} could not be validated at seal.",
                if compact_receipt_rejections == 1 {
                    ""
                } else {
                    "s"
                }
            ),
            "attention",
            false,
        ));
    }
    if verification.is_none() && !empty_seal {
        issues.push(issue(
            "verification_missing",
            "No project check was recorded for this revision.",
            "info",
            false,
        ));
    }
    issues
}

pub fn synthesize_review(
    outcome: String,
    verification: Option<ReviewVerification>,
    issues: Vec<ReviewIssue>,
) -> ReviewSynthesis {
    let blocks_approval = issues.iter().any(|issue| issue.blocks_approval);
    let risk = if issues.iter().any(|issue| issue.severity == "high") {
        "high"
    } else if issues
        .iter()
        .any(|issue| issue.severity == "attention" || issue.blocks_approval)
    {
        "attention"
    } else {
        "low"
    };
    let status = if issues.is_empty() {
        "ready"
    } else if blocks_approval || issues.iter().any(|issue| issue.severity == "high") {
        "needs_attention"
    } else {
        "review"
    };
    let unresolved_issues: Vec<String> = issues.iter().map(|issue| issue.message.clone()).collect();
    ReviewSynthesis {
        outcome,
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
        issues,
        blocks_approval,
        recommended_next_action: match status {
            "ready" => "Approve the revision or inspect any file that matters to you.",
            "needs_attention" => {
                "Open the highlighted evidence, then revise or approve explicitly."
            }
            _ => "Inspect the changed files and decide whether to finish or revise.",
        }
        .into(),
    }
}

fn changed_since_previous_paths(
    forge: &Forge,
    item: &WorkItem,
    current_attempt_id: Option<&str>,
    current_files: &[ChangedFileSummary],
) -> Vec<String> {
    let Some(current_id) = current_attempt_id else {
        return Vec::new();
    };
    // Only meaningful after at least one ChangesRequested cycle.
    if item.changes_requested.is_empty() {
        return Vec::new();
    }
    let previous = item.attempts.iter().rev().find(|attempt| {
        attempt.id.as_str() != current_id
            && attempt.evidence_id.is_some()
            && item
                .changes_requested
                .iter()
                .any(|request| request.attempt_id == attempt.id)
    });
    let Some(previous) = previous else {
        return Vec::new();
    };
    let Some(evidence_id) = previous.evidence_id.as_ref() else {
        return Vec::new();
    };
    let Some(dir) = evidence_dir(forge, item, evidence_id) else {
        return Vec::new();
    };
    let Some(manifest) = load_manifest(&dir) else {
        return Vec::new();
    };
    let previous_paths: std::collections::HashSet<&str> = manifest
        .changed_files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    let current_paths: std::collections::HashSet<&str> = current_files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    let mut out: Vec<String> = current_paths
        .difference(&previous_paths)
        .chain(previous_paths.difference(&current_paths))
        .map(|path| (*path).to_owned())
        .collect();
    out.sort();
    out.dedup();
    out
}

fn collect_attribution(
    item: &WorkItem,
    sealed_attempt_id: Option<&str>,
    changed_paths: &[String],
) -> Vec<ReviewAttribution> {
    let mut grouped: Vec<ReviewAttribution> = Vec::new();
    for attempt in &item.attempts {
        let executor = attempt.executor.kind.trim().to_ascii_lowercase();
        let kind = match executor.as_str() {
            "human" => "human",
            "terminal" => "terminal",
            "codex" | "cursor" | "hermes" | "acp-codex" | "acp-cursor" | "acp-hermes"
            | "agent" | "script" => "agent",
            _ => "agent",
        };
        let label = match executor.as_str() {
            "human" => "You".into(),
            "terminal" => "Terminal".into(),
            "codex" | "acp-codex" => "Codex".into(),
            "cursor" | "acp-cursor" => "Cursor".into(),
            "hermes" | "acp-hermes" => "Hermes".into(),
            "script" => "Automation".into(),
            _ => attempt.executor.kind.clone(),
        };
        let state = match attempt.state {
            AttemptState::Running => "working",
            AttemptState::Completed => "finished",
            AttemptState::Interrupted => "paused",
            AttemptState::Failed => "needs attention",
        };
        let files = if sealed_attempt_id == Some(attempt.id.as_str()) {
            changed_paths.to_vec()
        } else {
            Vec::new()
        };
        if let Some(existing) = grouped
            .iter_mut()
            .find(|entry| entry.kind == kind && entry.label == label)
        {
            existing.count = existing.count.saturating_add(1);
            if attempt.started_at < existing.started_at {
                existing.started_at = attempt.started_at;
            }
            if let Some(ended) = attempt.ended_at {
                existing.ended_at = Some(match existing.ended_at {
                    Some(prev) if prev > ended => prev,
                    _ => ended,
                });
            }
            if !files.is_empty() {
                existing.files = files;
                existing.state = state.into();
                existing.id = attempt.id.as_str().to_owned();
            } else if existing.state != "working" && state == "working" {
                existing.state = state.into();
            }
            continue;
        }
        grouped.push(ReviewAttribution {
            id: attempt.id.as_str().to_owned(),
            kind: kind.into(),
            label,
            state: state.into(),
            started_at: attempt.started_at,
            ended_at: attempt.ended_at,
            files,
            count: 1,
        });
    }
    grouped
}

fn review_candidates(forge: &Forge, item: &WorkItem) -> Vec<ReviewCandidateProjection> {
    item.attempts
        .iter()
        .filter_map(|attempt| {
            let evidence_id = attempt.evidence_id.as_ref()?;
            let environment = attempt.environment.as_ref().or(item.environment.as_ref())?;
            let manifest = load_manifest(&evidence_dir(forge, item, evidence_id)?)?;
            let evidence_digest = manifest.bundle_digest.as_ref()?;
            Some(ReviewCandidateProjection {
                attempt_id: attempt.id.as_str().to_owned(),
                attempt_seq: attempt.seq,
                executor: attempt.executor.kind.clone(),
                evidence_id: evidence_id.as_str().to_owned(),
                evidence_digest: evidence_digest.as_str().to_owned(),
                baseline_oid: manifest.baseline_oid.as_str().to_owned(),
                sealed_head_oid: manifest.sealed_head_oid.as_str().to_owned(),
                branch: environment.branch.clone(),
                worktree: environment.worktree.display().to_string(),
                changed_file_count: manifest.changed_files.len(),
                sealed_at: manifest.sealed_at,
                decision_id: item
                    .review_decisions
                    .iter()
                    .rev()
                    .find(|decision| decision.attempt_id == attempt.id)
                    .map(|decision| decision.id.as_str().to_owned()),
            })
        })
        .collect()
}

fn collect_coder_intents_by_path(commands: &str) -> std::collections::HashMap<String, Vec<String>> {
    let mut by_path: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for line in commands.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) else {
            continue;
        };
        if value.get("kind").and_then(|v| v.as_str()) != Some("medousa_coder_tool") {
            continue;
        }
        let Some(intent) = value
            .get("intent")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
        else {
            continue;
        };
        let Some(path) = value
            .get("path")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.trim_start_matches("./").replace('\\', "/"))
        else {
            continue;
        };
        let entry = by_path.entry(path).or_default();
        if !entry.iter().any(|existing| existing == &intent) {
            entry.push(intent);
        }
    }
    by_path
}

/// Count +/- lines per path from a unified `patch.diff` (best-effort skim stats).
fn count_patch_line_stats(patch: &str) -> std::collections::HashMap<String, (u32, u32)> {
    let mut stats: std::collections::HashMap<String, (u32, u32)> = std::collections::HashMap::new();
    let mut current: Option<String> = None;
    for line in patch.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            // `diff --git a/path b/path` — prefer b/ path
            let path = rest
                .split_whitespace()
                .nth(1)
                .and_then(|token| token.strip_prefix("b/"))
                .or_else(|| {
                    rest.split_whitespace()
                        .next()
                        .and_then(|token| token.strip_prefix("a/"))
                })
                .unwrap_or("")
                .replace('\\', "/");
            if path.is_empty() {
                current = None;
            } else {
                current = Some(path.clone());
                stats.entry(path).or_insert((0, 0));
            }
            continue;
        }
        let Some(path) = current.as_ref() else {
            continue;
        };
        if line.starts_with("+++") || line.starts_with("---") || line.starts_with("@@") {
            continue;
        }
        if line.starts_with('+') {
            stats.entry(path.clone()).or_default().0 += 1;
        } else if line.starts_with('-') {
            stats.entry(path.clone()).or_default().1 += 1;
        }
    }
    stats
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
                EventPayload::ReviewCommentAdded { comment } => (
                    "comment",
                    "Comment added",
                    Some(format!("{}:{}", comment.path, comment.start_line)),
                ),
                EventPayload::ReviewCommentResolved { .. } => ("comment", "Comment resolved", None),
                EventPayload::ReviewCommentDeleted { .. } => ("comment", "Comment removed", None),
                EventPayload::ChangesRequested { request } => (
                    "decision",
                    "Changes requested",
                    if request.revision_brief.trim().is_empty() {
                        None
                    } else {
                        Some(request.revision_brief.chars().take(200).collect())
                    },
                ),
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
        "codex" | "acp-codex" => "Codex".into(),
        "cursor" | "acp-cursor" => "Cursor".into(),
        "hermes" | "acp-hermes" => "Hermes".into(),
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
    /// Whether the governed worktree currently exists on this workshop's disk.
    pub workspace_present: bool,
}

pub fn project_item(mut item: WorkItem) -> ItemProjection {
    let human = human_phase(item.state).to_owned();
    let actions = allowed_actions(&item);
    let workspace_present = item
        .workspace_environment()
        .is_some_and(|environment| environment.worktree.is_dir());
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
        workspace_present,
    }
}

pub fn project_items(items: Vec<WorkItem>) -> Vec<ItemProjection> {
    items.into_iter().map(project_item).collect()
}

/// Re-export for callers/tests; builds a revision brief from comments + summary.
pub use medousa_forge::model::compose_revision_brief;

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

#[cfg(test)]
mod tests {
    use super::*;
    use medousa_forge::forge::SealOptions;
    use medousa_forge::git::{CheckpointAuthor, GitEngine};
    use medousa_forge::model::ExecutorDescriptor;

    #[test]
    fn review_candidates_select_exact_attempt_evidence() {
        let repo = tempfile::tempdir().unwrap();
        let forge_root = tempfile::tempdir().unwrap();
        let git = GitEngine::detect().unwrap();
        assert!(
            std::process::Command::new("git")
                .args(["init", "-b", "main", "--template="])
                .current_dir(repo.path())
                .status()
                .unwrap()
                .success()
        );
        std::fs::write(repo.path().join("base.txt"), "base\n").unwrap();
        assert!(
            std::process::Command::new("git")
                .args(["add", "-A"])
                .current_dir(repo.path())
                .status()
                .unwrap()
                .success()
        );
        git.commit_checkpoint(repo.path(), "initial", &CheckpointAuthor::default())
            .unwrap();
        let forge = Forge::open(forge_root.path()).unwrap();
        let item = forge
            .register(
                "parallel",
                "review candidates",
                repo.path(),
                "main",
                "user-1",
                &Forge::system_actor(),
            )
            .unwrap();
        let item = forge.provision(&item.id, &Forge::system_actor()).unwrap();
        let executor = || ExecutorDescriptor {
            kind: "agent".into(),
            detail: serde_json::json!({}),
        };
        let (_, first) = forge
            .begin_isolated_attempt(&item.id, executor(), None, &Forge::system_actor())
            .unwrap();
        let (item, second) = forge
            .begin_isolated_attempt(&item.id, executor(), None, &Forge::system_actor())
            .unwrap();
        let first_env = item.environment_for_attempt(&first.attempt_id).unwrap();
        let second_env = item.environment_for_attempt(&second.attempt_id).unwrap();
        std::fs::write(first_env.worktree.join("first.txt"), "first\n").unwrap();
        std::fs::write(second_env.worktree.join("second.txt"), "second\n").unwrap();
        forge
            .complete_attempt(&first, &SealOptions::default(), &Forge::system_actor())
            .unwrap();
        let item = forge
            .complete_attempt(&second, &SealOptions::default(), &Forge::system_actor())
            .unwrap();

        let latest = build_review(&forge, &item);
        assert_eq!(latest.candidates.len(), 2);
        assert_eq!(
            latest.attempt_id.as_deref(),
            Some(second.attempt_id.as_str())
        );
        assert!(
            latest
                .changed_files
                .iter()
                .any(|file| file.path == "second.txt")
        );
        assert!(
            !latest
                .changed_files
                .iter()
                .any(|file| file.path == "first.txt")
        );

        let selected = build_review_for_attempt(&forge, &item, Some(&first.attempt_id));
        assert_eq!(
            selected.attempt_id.as_deref(),
            Some(first.attempt_id.as_str())
        );
        assert!(
            selected
                .changed_files
                .iter()
                .any(|file| file.path == "first.txt")
        );
        assert!(
            !selected
                .changed_files
                .iter()
                .any(|file| file.path == "second.txt")
        );
        assert_ne!(selected.worktree, latest.worktree);
    }

    #[test]
    fn coder_intents_and_patch_stats_attach_to_paths() {
        let commands = r#"
{"kind":"medousa_coder_tool","intent":"Add helper","path":"src/lib.rs","ok":true}
{"kind":"medousa_coder_tool","intent":"Add helper","path":"src/lib.rs","ok":true}
{"kind":"medousa_coder_tool","intent":"Fix test","path":"src/lib.rs","ok":true}
{"kind":"project_task","success":true}
{"kind":"medousa_coder_tool","intent":"No path"}
"#;
        let intents = collect_coder_intents_by_path(commands);
        assert_eq!(
            intents.get("src/lib.rs"),
            Some(&vec!["Add helper".to_string(), "Fix test".to_string()])
        );

        let patch = "\
diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,4 @@
 keep
-old
+new
+also
";
        let stats = count_patch_line_stats(patch);
        assert_eq!(stats.get("src/lib.rs"), Some(&(2, 1)));
    }

    #[test]
    fn continue_editing_is_allowed_only_while_awaiting_review_with_evidence() {
        use medousa_forge::model::{
            Attempt, AttemptId, AttemptState, EvidenceId, ExecutorDescriptor, GitOid,
            GitWorkTarget, WorkItem, WorkState, WorkTarget,
        };

        let mut item = WorkItem::new(
            "t",
            "b",
            WorkTarget::Git(GitWorkTarget {
                repo_path: std::path::PathBuf::from("/tmp/repo"),
                base_ref: "main".into(),
                base_oid: GitOid::new("a".repeat(40)),
            }),
            "user-1",
        );
        item.state = WorkState::AwaitingReview;
        assert!(!allowed_actions(&item).continue_editing.allowed);

        item.attempts.push(Attempt {
            id: AttemptId::new(),
            seq: 1,
            state: AttemptState::Completed,
            executor: ExecutorDescriptor {
                kind: "human".into(),
                detail: serde_json::json!({}),
            },
            environment: None,
            lease: None,
            evidence_id: Some(EvidenceId::new()),
            started_at: chrono::Utc::now(),
            ended_at: Some(chrono::Utc::now()),
            recovery: None,
        });
        assert!(allowed_actions(&item).continue_editing.allowed);

        item.state = WorkState::Ready;
        assert!(!allowed_actions(&item).continue_editing.allowed);
        assert!(allowed_actions(&item).begin_attempt.allowed);
    }

    #[test]
    fn compose_revision_brief_formats_path_anchor_and_summary() {
        use medousa_forge::model::{
            ActorKind, ActorRef, AttemptId, EvidenceId, ReviewComment, ReviewCommentId,
        };

        let comment = ReviewComment {
            id: ReviewCommentId::new(),
            thread_id: ReviewCommentId::new(),
            parent_id: None,
            evidence_id: EvidenceId::new(),
            attempt_id: AttemptId::new(),
            path: "src/lib.rs".into(),
            side: "new".into(),
            start_line: 42,
            end_line: 42,
            anchor_digest: medousa_forge::model::anchor_digest_for("fn main() {}"),
            anchor_text: Some("fn main() {}".into()),
            body: "Add error handling".into(),
            actor: ActorRef {
                kind: ActorKind::User,
                id: "user-1".into(),
            },
            created_at: Utc::now(),
            resolved_at: None,
            resolved_by: None,
        };
        let brief = compose_revision_brief([&comment], Some("Please revise"));
        assert!(brief.starts_with("Please revise"));
        assert!(brief.contains("src/lib.rs:42 \"fn main() {}\""));
        assert!(brief.contains("Add error handling"));
    }

    #[test]
    fn base_advanced_alone_is_attention_and_blocks() {
        let issues = collect_review_issues(true, false, 0, 0, None, false);
        let synthesis = synthesize_review("brief".into(), None, issues);
        assert_eq!(synthesis.risk, "attention");
        assert_eq!(synthesis.status, "needs_attention");
        assert!(synthesis.blocks_approval);
        assert!(synthesis.status_summary.contains("deserves your attention"));
    }

    #[test]
    fn verification_failure_is_high_risk() {
        let verification = ReviewVerification {
            label: "check".into(),
            command: vec!["npm".into(), "test".into()],
            success: false,
            exit_code: Some(1),
            duration_ms: Some(10),
        };
        let issues = collect_review_issues(false, false, 0, 0, Some(&verification), false);
        let synthesis = synthesize_review("brief".into(), Some(verification), issues);
        assert_eq!(synthesis.risk, "high");
        assert_eq!(synthesis.status, "needs_attention");
        assert!(synthesis.blocks_approval);
    }

    #[test]
    fn missing_verification_alone_does_not_block() {
        let issues = collect_review_issues(false, false, 0, 0, None, false);
        let synthesis = synthesize_review("brief".into(), None, issues);
        assert_eq!(synthesis.risk, "low");
        assert_eq!(synthesis.status, "review");
        assert!(!synthesis.blocks_approval);
    }

    #[test]
    fn empty_seal_blocks_approval() {
        let issues = collect_review_issues(false, false, 0, 0, None, true);
        let synthesis = synthesize_review("brief".into(), None, issues);
        assert!(synthesis.blocks_approval);
        assert_eq!(synthesis.risk, "attention");
        assert!(
            synthesis
                .unresolved_issues
                .iter()
                .any(|msg| msg.contains("no file changes"))
        );
    }
}
