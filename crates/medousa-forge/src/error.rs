use thiserror::Error;

use crate::model::{AttemptId, Digest, GitOid, LeaseId, WorkId, WorkState};

#[derive(Debug, Error)]
pub enum ForgeError {
    #[error("work item not found: {0}")]
    WorkNotFound(WorkId),

    #[error("attempt not found: {0}")]
    AttemptNotFound(AttemptId),

    #[error("invalid transition: work {work_id} is {state}, cannot {action}")]
    InvalidState {
        work_id: WorkId,
        state: WorkState,
        action: &'static str,
    },

    /// Fencing token mismatch: the caller presents a lease that is not the
    /// active lease for this attempt (stale adapter, superseded generation).
    #[error("stale lease: presented {presented} (gen {presented_generation}), active is {active} (gen {active_generation})")]
    StaleLease {
        presented: LeaseId,
        presented_generation: u64,
        active: LeaseId,
        active_generation: u64,
    },

    #[error("an attempt is already running for work {0}")]
    AttemptAlreadyRunning(WorkId),

    /// Base ref moved between decision and integration — the approval no
    /// longer authorizes the current state.
    #[error("base advanced: expected {expected}, found {found}")]
    BaseAdvanced { expected: GitOid, found: GitOid },

    /// Evidence-bound re-verification failed at disposition time.
    #[error("review decision invalid: {reason}")]
    DecisionInvalid { reason: String },

    /// The checkpoint/evidence digest no longer matches the sealed bundle.
    #[error("evidence digest mismatch: expected {expected}, found {found}")]
    EvidenceMismatch { expected: Digest, found: Digest },

    /// Worktree state changed after sealing (dirty when it must be clean,
    /// head moved past the reviewed head).
    #[error("environment drifted after seal: {0}")]
    EnvironmentDrift(String),

    #[error("policy violation requires acknowledgment: {0}")]
    PolicyViolation(String),

    /// Checkpoint capture refused by policy (oversize/secret without ack).
    #[error("checkpoint capture blocked: {0}")]
    CaptureBlocked(String),

    #[error("git error: {0}")]
    Git(String),

    #[error("store error: {0}")]
    Store(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, ForgeError>;
