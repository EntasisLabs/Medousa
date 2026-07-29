//! Home-facing Forge projections (allowed actions, review summary).
//!
//! Keeps unbounded patch/command payloads out of `WorkItem` responses.

use std::path::PathBuf;

use medousa_forge::forge::Forge;
use medousa_forge::model::{
    AttemptState, EvidenceId, EvidenceManifest, WorkItem, WorkState,
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
    let has_env = item.environment.is_some();
    let active = item
        .active_attempt
        .as_ref()
        .and_then(|id| item.attempt(id));
    let has_running = active.is_some_and(|a| a.state == AttemptState::Running && a.lease.is_some());
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
    pub truncated: bool,
    pub base_advanced: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy: Option<serde_json::Value>,
    pub command_log_lines: usize,
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
    let env = item.environment.as_ref();
    let sealed_attempt = item
        .attempts
        .iter()
        .rev()
        .find(|a| a.evidence_id.is_some());
    let evidence_id = sealed_attempt.and_then(|a| a.evidence_id.clone());
    let mut changed_files = Vec::new();
    let mut truncated = false;
    let mut base_advanced = false;
    let mut evidence_digest = None;
    let mut sealed_head = None;
    let mut policy = None;
    let mut command_log_lines = 0usize;
    let mut patch_byte_size = 0u64;

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
        }
        if let Ok(bytes) = std::fs::read(dir.join("policy.json")) {
            policy = serde_json::from_slice(&bytes).ok();
        }
    }

    let active = item
        .active_attempt
        .as_ref()
        .and_then(|id| item.attempt(id))
        .and_then(|a| a.lease.as_ref());

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
        truncated,
        base_advanced,
        policy,
        command_log_lines,
        patch_byte_size,
        decision: item
            .review_decisions
            .last()
            .and_then(|d| serde_json::to_value(d).ok()),
        disposition: item.disposition.map(|d| format!("{d:?}").to_ascii_lowercase()),
        worktree: env.map(|e| e.worktree.display().to_string()),
        active_lease_id: active.map(|l| l.lease_id.as_str().to_owned()),
        active_lease_generation: active.map(|l| l.generation),
        world: None,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ItemProjection {
    #[serde(flatten)]
    pub item: WorkItem,
    pub human_phase: String,
    pub allowed_actions: AllowedActions,
}

pub fn project_item(item: WorkItem) -> ItemProjection {
    let human = human_phase(item.state).to_owned();
    let actions = allowed_actions(&item);
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
