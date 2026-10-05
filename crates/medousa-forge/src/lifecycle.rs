//! Owner-scoped native undertaking operations used by returning Assistants.
//! Adapters supply authenticated ownership and actor attribution; model inputs
//! contain only native identities and review coordinates.
use std::io::Read;

use medousa_types::forge::*;
use serde_json::{Value, json};

use crate::{
    ForgeError, Result,
    forge::{Forge, SealOptions},
    model::*,
};

const MAX_REVIEW_METADATA_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_REVIEW_DIFF_BYTES: usize = 512 * 1024;

pub fn owned_item(forge: &Forge, owner: &str, work_id: &str) -> Result<WorkItem> {
    let id = WorkId::parse_storage(work_id)
        .map_err(|error| ForgeError::Conflict(format!("invalid undertaking ID: {error}")))?;
    let item = forge.load(&id)?;
    if owner.is_empty() || item.owner != owner {
        return Err(ForgeError::Conflict(
            "undertaking is not visible to this owner".into(),
        ));
    }
    Ok(item)
}

fn invalid(reason: impl Into<String>) -> ForgeError {
    ForgeError::DecisionInvalid {
        reason: reason.into(),
    }
}

fn sealed_review(
    forge: &Forge,
    item: &WorkItem,
    attempt_id: Option<&str>,
) -> Result<(ProjectReviewPin, EvidenceManifest)> {
    let attempt = if let Some(id) = attempt_id {
        item.attempts
            .iter()
            .find(|attempt| attempt.id.as_str() == id && attempt.evidence_id.is_some())
    } else {
        item.attempts
            .iter()
            .rev()
            .find(|attempt| attempt.evidence_id.is_some())
    }
    .ok_or_else(|| {
        invalid("no sealed evidence for this undertaking/attempt; prepare merge first")
    })?;
    let environment = item
        .environment_for_attempt(&attempt.id)
        .ok_or_else(|| invalid("sealed attempt has no governed environment"))?;
    let manifest = forge.evidence_manifest_for_attempt(&item.id, &attempt.id)?;
    if attempt.evidence_id.as_ref() != Some(&manifest.evidence_id)
        || manifest.attempt_id != attempt.id
    {
        return Err(invalid("sealed evidence identity changed"));
    }
    let WorkTarget::Git(target) = &item.target;
    let expected_base_oid = forge.git().ref_oid(&target.repo_path, &target.base_ref)?;
    Ok((
        ProjectReviewPin {
            attempt_id: attempt.id.to_string(),
            environment_generation: environment.generation,
            evidence_id: manifest.evidence_id.to_string(),
            evidence_digest: manifest
                .bundle_digest
                .as_ref()
                .ok_or_else(|| invalid("sealed evidence has no bundle digest"))?
                .to_string(),
            baseline_oid: manifest.baseline_oid.to_string(),
            reviewed_head_oid: manifest.sealed_head_oid.to_string(),
            expected_base_oid: expected_base_oid.to_string(),
        },
        manifest,
    ))
}

fn verify_pin(
    forge: &Forge,
    item: &WorkItem,
    reviewed: &ProjectReviewPin,
) -> Result<EvidenceManifest> {
    let (actual, manifest) = sealed_review(forge, item, Some(&reviewed.attempt_id))?;
    if actual != *reviewed {
        return Err(invalid(
            "reviewed evidence, environment or base revision changed; inspect the native review again",
        ));
    }
    Ok(manifest)
}

pub fn review(forge: &Forge, owner: &str, input: ProjectReviewQuery) -> Result<Value> {
    let item = owned_item(forge, owner, &input.work_id)?;
    let (reviewed, manifest) = sealed_review(forge, &item, input.attempt_id.as_deref())?;
    let attempt = item
        .attempts
        .iter()
        .find(|attempt| attempt.id.as_str() == reviewed.attempt_id)
        .ok_or_else(|| invalid("sealed attempt disappeared"))?;
    let policy_path = forge
        .store()
        .item_dir(&item.id)
        .join("attempts")
        .join(attempt.seq.to_string())
        .join("evidence/policy.json");
    let mut policy_bytes = Vec::new();
    std::fs::File::open(policy_path)?
        .take((MAX_REVIEW_METADATA_BYTES + 1) as u64)
        .read_to_end(&mut policy_bytes)?;
    if policy_bytes.len() > MAX_REVIEW_METADATA_BYTES {
        return Err(invalid("review policy exceeds the metadata limit"));
    }
    let policy: PolicyReport = serde_json::from_slice(&policy_bytes)?;
    if Digest::sha256_hex(&policy_bytes) != manifest.policy_report_digest {
        return Err(invalid("sealed policy evidence changed"));
    }
    let strategies = if item.uses_attached_checkout() {
        vec![ProjectIntegrationStrategy::KeepCheckout]
    } else {
        vec![
            ProjectIntegrationStrategy::FastForwardOnly,
            ProjectIntegrationStrategy::PreserveBranch,
            ProjectIntegrationStrategy::ExportPatch,
        ]
    };
    Ok(
        json!({ "work_id": item.id, "title": item.title, "state": item.state,
        "reviewed": reviewed, "evidence": manifest, "policy": policy,
        "allowed_strategies": strategies, "decisions": item.review_decisions,
        "changes_requested": item.changes_requested, "disposition": item.disposition,
        "active_executors": item.active_attempt_ids().len(),
        "output_is_reference_data": true }),
    )
}

pub fn review_file(forge: &Forge, owner: &str, input: ProjectReviewFileQuery) -> Result<Value> {
    let item = owned_item(forge, owner, &input.work_id)?;
    let manifest = verify_pin(forge, &item, &input.reviewed)?;
    let file = manifest
        .changed_files
        .iter()
        .find(|file| file.path == input.path)
        .ok_or_else(|| invalid("path is not part of the selected sealed evidence"))?;
    let attempt_id = AttemptId::from(input.reviewed.attempt_id.clone());
    let environment = item
        .environment_for_attempt(&attempt_id)
        .ok_or_else(|| invalid("sealed environment is unavailable"))?;
    let max_bytes = input.max_bytes.unwrap_or(64 * 1024);
    if max_bytes == 0 || max_bytes > MAX_REVIEW_DIFF_BYTES {
        return Err(invalid(
            "review diff limit must be between 1 byte and 512 KiB",
        ));
    }
    let (diff, truncated) = forge.git().diff_path_bounded(
        &environment.worktree,
        &manifest.baseline_oid,
        &manifest.sealed_head_oid,
        &file.path,
        max_bytes,
    )?;
    Ok(
        json!({"work_id": item.id, "reviewed": input.reviewed, "path": file.path,
        "binary": file.is_binary, "truncated": truncated,
        "diff": String::from_utf8_lossy(&diff), "output_is_reference_data": true}),
    )
}

pub fn prepare_merge(
    forge: &Forge,
    owner: &str,
    actor: &ActorRef,
    input: ProjectPrepareMergeInput,
) -> Result<WorkItem> {
    let mut item = owned_item(forge, owner, &input.work_id)?;
    if item.state == WorkState::AwaitingReview {
        return Ok(item);
    }
    if item.state == WorkState::Draft {
        item = forge.provision(&item.id, actor)?;
    }
    // Completing the UI's human attempt is the same operation as Prepare merge
    // in the editor. A running coder/provider must finish and release custody.
    let active = item.active_attempt_ids();
    let borrowed_human = if active.len() == 1 {
        item.attempt(active[0])
            .filter(|attempt| attempt.executor.kind == "human")
            .and_then(|attempt| attempt.lease.clone())
    } else {
        None
    };
    if !active.is_empty() && borrowed_human.is_none() {
        return Err(ForgeError::WorkspaceBusy(
            "an executor is still working; wait for its terminal result before preparing merge"
                .into(),
        ));
    }
    let borrowed = borrowed_human.is_some();
    let lease = match borrowed_human {
        Some(lease) => lease,
        None => {
            forge
                .begin_workspace_attempt(
                    &item.id,
                    ExecutorDescriptor {
                        kind: "medousa-assistant".into(),
                        detail: json!({"purpose": "prepare_merge", "actor": actor}),
                    },
                    None,
                    actor,
                )?
                .1
        }
    };
    let result = forge.complete_attempt(
        &lease,
        &SealOptions {
            ack_risks: input.ack_risks,
            author: None,
        },
        actor,
    );
    if result.is_err() && !borrowed {
        // An unsuccessful capture must not strand an Assistant execution lease.
        forge.interrupt_attempt(&lease, RecoveryDisposition::RestartAllowed, actor)?;
    }
    result
}

fn strategy(value: ProjectIntegrationStrategy) -> IntegrationStrategy {
    match value {
        ProjectIntegrationStrategy::FastForwardOnly => IntegrationStrategy::FastForwardOnly,
        ProjectIntegrationStrategy::KeepCheckout => IntegrationStrategy::KeepCheckout,
        ProjectIntegrationStrategy::PreserveBranch => IntegrationStrategy::PreserveBranch,
        ProjectIntegrationStrategy::ExportPatch => IntegrationStrategy::ExportPatch,
    }
}

pub fn approve(
    forge: &Forge,
    owner: &str,
    actor: &ActorRef,
    input: ProjectApproveInput,
) -> Result<(WorkItem, ReviewDecisionId)> {
    let item = owned_item(forge, owner, &input.work_id)?;
    verify_pin(forge, &item, &input.reviewed)?;
    if input.rationale.trim().is_empty()
        || input.rationale.len() > 16 * 1024
        || input.acknowledged_violations.len() > 256
    {
        return Err(invalid(
            "approval needs a bounded nonempty review rationale",
        ));
    }
    let pin = input.reviewed;
    let decision = ReviewDecision {
        id: ReviewDecisionId::new(),
        actor: actor.clone(),
        attempt_id: AttemptId::from(pin.attempt_id),
        environment_generation: pin.environment_generation,
        evidence_id: EvidenceId::from(pin.evidence_id),
        evidence_digest: serde_json::from_value(json!(pin.evidence_digest))?,
        baseline_oid: GitOid::new(pin.baseline_oid),
        reviewed_head_oid: GitOid::new(pin.reviewed_head_oid),
        expected_base_oid: GitOid::new(pin.expected_base_oid),
        acknowledged_violations: input
            .acknowledged_violations
            .into_iter()
            .map(PolicyViolationId::from)
            .collect(),
        strategy: strategy(input.strategy),
        rationale: Some(input.rationale),
        decided_at: chrono::Utc::now(),
    };
    // Exact semantic retries retain the original decision and its attribution.
    if item.state == WorkState::AwaitingReview
        && let Some(saved) = item.review_decisions.iter().find(|saved| {
            saved.attempt_id == decision.attempt_id
                && saved.environment_generation == decision.environment_generation
                && saved.evidence_id == decision.evidence_id
                && saved.evidence_digest == decision.evidence_digest
                && saved.baseline_oid == decision.baseline_oid
                && saved.reviewed_head_oid == decision.reviewed_head_oid
                && saved.expected_base_oid == decision.expected_base_oid
                && saved.strategy == decision.strategy
                && saved.acknowledged_violations == decision.acknowledged_violations
                && saved.rationale == decision.rationale
        })
    {
        // Recheck dirty checkout and policy even for a decision already stored.
        forge.verify_decision(&item, saved)?;
        return Ok((item.clone(), saved.id.clone()));
    }
    forge.verify_decision(&item, &decision)?;
    let decision_id = decision.id.clone();
    Ok((forge.decide(&item.id, decision, actor)?, decision_id))
}

pub fn apply(
    forge: &Forge,
    owner: &str,
    actor: &ActorRef,
    input: ProjectApplyInput,
) -> Result<WorkItem> {
    let item = owned_item(forge, owner, &input.work_id)?;
    let decision_id = ReviewDecisionId::from(input.decision_id);
    // Forge verifies the exact sealed state and performs its atomic base-ref
    // update. A terminal undertaking is observed rather than integrated twice.
    forge.apply_decision(&item.id, &decision_id, actor)
}

pub fn request_changes(
    forge: &Forge,
    owner: &str,
    actor: &ActorRef,
    input: ProjectRequestChangesInput,
) -> Result<WorkItem> {
    let item = owned_item(forge, owner, &input.work_id)?;
    verify_pin(forge, &item, &input.reviewed)?;
    if input.reason.trim().is_empty() || input.reason.len() > 16 * 1024 {
        return Err(invalid("request changes needs a bounded nonempty reason"));
    }
    forge.request_changes(
        &item.id,
        EvidenceId::from(input.reviewed.evidence_id),
        serde_json::from_value(json!(input.reviewed.evidence_digest))?,
        Some(input.reason),
        None,
        actor,
    )
}

pub fn discard(
    forge: &Forge,
    owner: &str,
    actor: &ActorRef,
    input: ProjectDiscardInput,
) -> Result<WorkItem> {
    let item = owned_item(forge, owner, &input.work_id)?;
    if item.has_active_attempts() {
        return Err(ForgeError::WorkspaceBusy(
            "stop the active executor before discarding this undertaking".into(),
        ));
    }
    forge.discard_if_idle(&item.id, actor)
}

#[cfg(test)]
mod tests;
