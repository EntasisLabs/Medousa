//! Append-only event vocabulary. Every state change and every crash-visible
//! side effect is an event; the store assigns monotonic per-item `seq` on
//! append. Snapshots are strictly a cache — replay of `events.jsonl` is the
//! source of truth.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::model::{
    AcceptedDisposition, ActorRef, Attempt, AttemptId, AttemptState, EvidenceId, GitOid, LeaseId,
    OperationId, RecoveryDisposition, ReviewDecision, ReviewDecisionId, WorkId, WorkState,
};

pub const EVENT_SCHEMA_VERSION: u32 = 1;

/// One durable record in `items/<work_id>/events.jsonl`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransitionEvent {
    pub schema_version: u32,
    pub work_id: WorkId,
    /// Monotonic per work item, assigned by the store on append.
    pub seq: u64,
    pub actor: ActorRef,
    pub at: DateTime<Utc>,
    pub payload: EventPayload,
}

impl TransitionEvent {
    pub fn new(work_id: WorkId, seq: u64, actor: ActorRef, payload: EventPayload) -> Self {
        Self {
            schema_version: EVENT_SCHEMA_VERSION,
            work_id,
            seq,
            actor,
            at: Utc::now(),
            payload,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventPayload {
    /// First event in every log; carries the initial item so replay alone can
    /// rebuild full state.
    ItemRegistered { item: Box<crate::model::WorkItem> },
    /// A governed environment was provisioned (or re-provisioned) and is now
    /// the item's environment.
    EnvironmentProvisioned { env: Box<crate::model::GovernedEnv> },
    /// A mutating operation with Git/filesystem side effects has begun. If the
    /// process crashes before `operation_committed`, reconciliation rolls this
    /// operation forward (or classifies it) from its recorded side effects.
    OperationStarted {
        operation_id: OperationId,
        kind: OperationKind,
    },
    /// One completed irreversible-ish side effect within an operation.
    OperationSideEffect {
        operation_id: OperationId,
        effect: SideEffect,
    },
    OperationCommitted {
        operation_id: OperationId,
        resulting_state: WorkState,
    },
    OperationAborted {
        operation_id: OperationId,
        reason: String,
    },
    StateChanged {
        from: WorkState,
        to: WorkState,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    AttemptStarted {
        attempt: Box<Attempt>,
    },
    AttemptEnded {
        attempt_id: AttemptId,
        state: AttemptState,
        recovery: RecoveryDisposition,
    },
    LeaseAcquired {
        attempt_id: AttemptId,
        lease_id: LeaseId,
        generation: u64,
        owner_instance_id: String,
    },
    EvidenceSealed {
        attempt_id: AttemptId,
        evidence_id: EvidenceId,
        evidence_digest: crate::model::Digest,
    },
    ReviewDecided {
        decision: Box<ReviewDecision>,
    },
    DecisionInvalidated {
        decision_id: ReviewDecisionId,
        reason: String,
    },
    DispositionApplied {
        disposition: AcceptedDisposition,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    Provision,
    Seal,
    Integrate,
    Discard,
}

/// A completed side effect, typed so reconciliation can roll forward or
/// classify precisely without guessing from process state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SideEffect {
    WorktreeAdded {
        path: std::path::PathBuf,
        branch: String,
        baseline_oid: GitOid,
    },
    CheckpointCommitCreated {
        branch: String,
        oid: GitOid,
    },
    BaseRefAdvanced {
        ref_name: String,
        old_oid: GitOid,
        new_oid: GitOid,
    },
    PatchExported {
        path: std::path::PathBuf,
        digest: crate::model::Digest,
    },
    WorktreeRemoved {
        path: std::path::PathBuf,
    },
    BranchRemoved {
        branch: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ActorKind, Digest};

    fn actor() -> ActorRef {
        ActorRef {
            kind: ActorKind::System,
            id: "forge".into(),
        }
    }

    #[test]
    fn event_round_trip_all_payload_shapes() {
        let work_id = WorkId::new();
        let payloads = vec![
            EventPayload::ItemRegistered {
                item: Box::new(crate::model::WorkItem::new(
                    "t",
                    "b",
                    crate::model::WorkTarget::Git(crate::model::GitWorkTarget {
                        repo_path: std::path::PathBuf::from("/tmp/repo"),
                        base_ref: "main".into(),
                        base_oid: GitOid::new("a".repeat(40)),
                    }),
                    "user-1",
                )),
            },
            EventPayload::OperationStarted {
                operation_id: OperationId::new(),
                kind: OperationKind::Seal,
            },
            EventPayload::OperationSideEffect {
                operation_id: OperationId::new(),
                effect: SideEffect::CheckpointCommitCreated {
                    branch: "medousa/work/x".into(),
                    oid: GitOid::new("d".repeat(40)),
                },
            },
            EventPayload::OperationCommitted {
                operation_id: OperationId::new(),
                resulting_state: WorkState::AwaitingReview,
            },
            EventPayload::OperationAborted {
                operation_id: OperationId::new(),
                reason: "git failed".into(),
            },
            EventPayload::StateChanged {
                from: WorkState::Ready,
                to: WorkState::Executing,
                reason: None,
            },
            EventPayload::LeaseAcquired {
                attempt_id: AttemptId::new(),
                lease_id: LeaseId::new(),
                generation: 3,
                owner_instance_id: "boot-9".into(),
            },
            EventPayload::EvidenceSealed {
                attempt_id: AttemptId::new(),
                evidence_id: EvidenceId::new(),
                evidence_digest: Digest::sha256_hex(b"bundle"),
            },
            EventPayload::DecisionInvalidated {
                decision_id: ReviewDecisionId::new(),
                reason: "head moved".into(),
            },
            EventPayload::DispositionApplied {
                disposition: AcceptedDisposition::BranchPreserved,
                detail: Some("kept".into()),
            },
        ];

        for (i, payload) in payloads.into_iter().enumerate() {
            let event = TransitionEvent::new(work_id.clone(), i as u64 + 1, actor(), payload);
            let line = serde_json::to_string(&event).unwrap();
            let back: TransitionEvent = serde_json::from_str(&line).unwrap();
            assert_eq!(event, back);
            assert_eq!(back.schema_version, EVENT_SCHEMA_VERSION);
            assert_eq!(back.seq, i as u64 + 1);
        }
    }
}
