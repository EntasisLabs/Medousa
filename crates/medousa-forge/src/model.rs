//! Core domain model for Forge work items.
//!
//! Serde conventions follow the house style: snake_case everywhere, enums as
//! externally-tagged unit/newtype variants, `schema_version` on anything that
//! hits disk.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

pub const MODEL_SCHEMA_VERSION: u32 = 1;

macro_rules! id_type {
    ($name:ident, $prefix:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub struct $name(String);

        impl $name {
            pub fn new() -> Self {
                Self(format!("{}-{}", $prefix, uuid_v4_lite()))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }
    };
}

id_type!(WorkId, "work");
id_type!(AttemptId, "att");
id_type!(LeaseId, "lease");
id_type!(OperationId, "op");
id_type!(EvidenceId, "ev");
id_type!(ReviewDecisionId, "rev");
id_type!(ReviewCommentId, "rc");
id_type!(ChangesRequestedId, "cr");
id_type!(RepoId, "repo");
id_type!(PolicyViolationId, "pv");

/// RFC 4122 v4-shaped id without taking a uuid dependency (matches the
/// medousa-acp-client approach).
fn uuid_v4_lite() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let rand: u64 = rand_u64();
    format!("{nanos:08x}{rand:016x}")
}

fn rand_u64() -> u64 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let state = RandomState::new();
    let mut hasher = state.build_hasher();
    hasher.write_u64(std::process::id() as u64);
    hasher.finish()
}

/// SHA-256 hex digest over canonical content.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Digest(String);

impl Digest {
    pub fn sha256_hex(bytes: &[u8]) -> Self {
        use sha2::{Digest as _, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        Self(format!("{:x}", hasher.finalize()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Wrap an already-computed hex digest (no re-hash).
    pub fn from_hex(hex: impl Into<String>) -> Self {
        Self(hex.into())
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// SHA-256 hex digest of anchored line content for review comment stability.
pub fn anchor_digest_for(content: &str) -> String {
    Digest::sha256_hex(content.as_bytes()).as_str().to_owned()
}

/// A full Git object id (commit/tree/blob hash).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GitOid(String);

impl GitOid {
    pub fn new(oid: impl Into<String>) -> Self {
        Self(oid.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for GitOid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Who performed a governed action. Forge records actors; it never impersonates
/// them in Git authorship (checkpoint identity is explicit, via env vars).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActorRef {
    pub kind: ActorKind,
    pub id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorKind {
    User,
    Profile,
    System,
}

/// What a work item mutates. Extensible; Git-only in v1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkTarget {
    Git(GitWorkTarget),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitWorkTarget {
    /// The working-tree path the user pointed at (may be any worktree of the repo).
    pub repo_path: PathBuf,
    /// Base ref the work integrates toward (always protected, regardless of name).
    pub base_ref: String,
    /// Base OID captured at registration; integration binds to expected OIDs.
    pub base_oid: GitOid,
}

/// Repository identity derived from the canonical `git rev-parse
/// --git-common-dir`, stable across worktrees of the same repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoIdentity {
    pub repo_id: RepoId,
    pub requested_path: PathBuf,
    pub common_dir: PathBuf,
    pub format: Option<String>,
    /// Diagnostic identity only (e.g. origin URL); never a security boundary.
    pub remotes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentKind {
    GitWorktree,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernedEnv {
    pub kind: EnvironmentKind,
    pub repo: RepoIdentity,
    pub worktree: PathBuf,
    /// Forge-owned branch (medousa/work/<work_id>).
    pub branch: String,
    /// Immutable commit the environment was provisioned from. Evidence and
    /// integration compare against this OID, never a symbolic ref.
    pub baseline_oid: GitOid,
    /// Bumped on explicit environment restart/fork.
    pub generation: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkState {
    Draft,
    Provisioning,
    Ready,
    Executing,
    Sealing,
    AwaitingReview,
    ApplyingDecision,
    Accepted,
    Discarded,
    Failed,
}

impl WorkState {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            WorkState::Accepted | WorkState::Discarded | WorkState::Failed
        )
    }
}

impl fmt::Display for WorkState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            WorkState::Draft => "draft",
            WorkState::Provisioning => "provisioning",
            WorkState::Ready => "ready",
            WorkState::Executing => "executing",
            WorkState::Sealing => "sealing",
            WorkState::AwaitingReview => "awaiting_review",
            WorkState::ApplyingDecision => "applying_decision",
            WorkState::Accepted => "accepted",
            WorkState::Discarded => "discarded",
            WorkState::Failed => "failed",
        };
        f.write_str(s)
    }
}

/// "Accepted outcome" is deliberately not "merged into base" — each disposition
/// is a distinct durable result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcceptedDisposition {
    BranchPreserved,
    BaseFastForwarded,
    PatchExported,
}

/// v1 integration strategies. Merge commits and squash are deferred until
/// base-checkout and identity behavior are thoroughly tested.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationStrategy {
    /// Safest default: keep the reviewable branch, touch nothing upstream.
    PreserveBranch,
    /// Explicitly selected: advance the base ref (checked-out-base safe).
    FastForwardOnly,
    /// Produce a portable patch artifact instead of mutating refs.
    ExportPatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptState {
    Running,
    Completed,
    Interrupted,
    Failed,
}

/// Separate from state: an interrupted attempt records what a future adapter
/// may do about it. Forge never resumes providers itself — it exposes durable
/// information so the next adapter attempt can start correctly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecoveryDisposition {
    NotResumable,
    ResumeSupported { provider_token: String },
    RestartAllowed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutorDescriptor {
    /// "script" reference adapter now; "cursor" / "codex" / "medousa" later.
    pub kind: String,
    /// Opaque adapter config (argv for script, session hints for ACP adapters).
    #[serde(default)]
    pub detail: serde_json::Value,
}

/// Fenced ownership of an attempt. Every mutating attempt call presents
/// `lease_id` + `generation`; Forge rejects callers that no longer hold the
/// active lease, so a stale adapter cannot write into a newer attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionLease {
    pub lease_id: LeaseId,
    /// Monotonic per attempt; presented as the fencing token.
    pub generation: u64,
    pub work_id: WorkId,
    pub attempt_id: AttemptId,
    /// Daemon boot identity — distinguishes leases across restarts.
    pub owner_instance_id: String,
    pub acquired_at: DateTime<Utc>,
    /// Latest heartbeat. Updated in the lease record only — never appended to
    /// the JSONL event log.
    pub heartbeat_at: DateTime<Utc>,
    pub pid: Option<u32>,
    /// Guards PID reuse (platform process start token when available).
    pub process_start_marker: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attempt {
    pub id: AttemptId,
    pub seq: u32,
    pub executor: ExecutorDescriptor,
    pub state: AttemptState,
    /// Isolated mutation environment owned by this attempt. Older/shared
    /// attempts omit it and fall back to the undertaking environment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<GovernedEnv>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease: Option<ExecutionLease>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery: Option<RecoveryDisposition>,
    /// Evidence contributed by this attempt (set at seal).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_id: Option<EvidenceId>,
    pub started_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<DateTime<Utc>>,
}

/// Governance over paths an executor may touch, and rules for checkpoint
/// capture. Violations are evidence — they never prove containment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkPolicy {
    /// Normalized git-path globs the executor is allowed to modify. Empty =
    /// everything allowed (violations still computed for report completeness).
    #[serde(default)]
    pub allowed_paths: Vec<String>,
    /// Normalized git-path globs that are always violations.
    #[serde(default)]
    pub denied_paths: Vec<String>,
    /// Additional protected refs beyond the always-protected base ref.
    #[serde(default)]
    pub protected_refs: Vec<String>,
    // --- checkpoint capture rules ---
    /// Capture all non-ignored changes into the checkpoint commit.
    #[serde(default = "default_true")]
    pub checkpoint_capture_all: bool,
    /// Maximum bytes for any single captured file (0 = unlimited).
    #[serde(default)]
    pub checkpoint_max_file_bytes: u64,
    /// Maximum total bytes captured (0 = unlimited).
    #[serde(default)]
    pub checkpoint_max_total_bytes: u64,
    /// Path classes excluded from checkpoint capture (normalized git-path globs).
    #[serde(default)]
    pub checkpoint_exclude_paths: Vec<String>,
    /// Include ignored/untracked files in checkpoints (usually false).
    #[serde(default)]
    pub checkpoint_include_ignored: bool,
    /// Scan captured content for likely secrets before committing.
    #[serde(default = "default_true")]
    pub checkpoint_secret_scan: bool,
    /// Risky checkpoints (oversize/secret hits) require explicit acknowledgment
    /// instead of being blocked outright.
    #[serde(default)]
    pub checkpoint_allow_risky_with_ack: bool,
}

fn default_true() -> bool {
    true
}

impl Default for WorkPolicy {
    fn default() -> Self {
        Self {
            allowed_paths: Vec::new(),
            denied_paths: Vec::new(),
            protected_refs: Vec::new(),
            checkpoint_capture_all: true,
            checkpoint_max_file_bytes: 0,
            checkpoint_max_total_bytes: 0,
            checkpoint_exclude_paths: Vec::new(),
            checkpoint_include_ignored: false,
            checkpoint_secret_scan: true,
            checkpoint_allow_risky_with_ack: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyViolation {
    pub id: PolicyViolationId,
    /// Normalized git path that tripped policy.
    pub path: String,
    pub rule: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyReport {
    /// Clean when no violations were found.
    pub violations: Vec<PolicyViolation>,
    /// Capture hazards recorded at seal time (oversize content, likely
    /// secrets). Empty when the checkpoint was clean or risks were blocked.
    #[serde(default)]
    pub capture_risks: Vec<CaptureRisk>,
    #[serde(default)]
    pub symlinks: Vec<String>,
    #[serde(default)]
    pub submodules: Vec<String>,
    #[serde(default)]
    pub nested_repos: Vec<String>,
}

/// A checkpoint-capture hazard: not a path violation, but risky content
/// (oversize, likely secret). Risky checkpoints require explicit
/// acknowledgment when the policy allows them at all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CaptureRisk {
    OversizeFile {
        path: String,
        bytes: u64,
        limit: u64,
    },
    OversizeTotal {
        bytes: u64,
        limit: u64,
    },
    SecretPattern {
        path: String,
        pattern: String,
    },
}

impl PolicyReport {
    pub fn is_clean(&self) -> bool {
        self.violations.is_empty() && self.capture_risks.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangedFile {
    /// Normalized git path.
    pub path: String,
    pub status: ChangeStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_path: Option<String>,
    #[serde(default)]
    pub is_binary: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_size: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
    Untracked,
    /// Unmerged / conflicted path (`git status` porcelain `u`).
    Unmerged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmodulePin {
    pub path: String,
    pub oid: GitOid,
    pub changed: bool,
}

/// Durable provenance for a redacted ephemeral Coder object. The raw object is
/// deliberately absent: sealing promotes identity and lifecycle metadata only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompactEvidenceReceipt {
    pub schema_version: u32,
    pub work_id: String,
    pub source_tool: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_call_id: Option<String>,
    pub digest: String,
    pub ephemeral_reference: String,
    pub content_type: String,
    pub logical_bytes: u64,
    pub physical_bytes: u64,
    pub retention: CompactEvidenceRetention,
    pub expires_at_unix_seconds: u64,
    pub redacted: bool,
    pub raw_evidence: RawEvidenceDisposition,
    pub recorded_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompactEvidenceRetention {
    SuccessfulOrReproducible,
    FailedOrNonReproducible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RawEvidenceDisposition {
    /// Raw bytes remain in the bounded TTL store and are never copied by seal.
    EphemeralOnly,
}

/// Canonical evidence manifest. The `bundle_digest` is computed over the
/// canonical serialization of this manifest *without* the digest field —
/// deterministic field ordering, normalized paths, stable list ordering,
/// SHA-256.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvidenceManifest {
    pub schema_version: u32,
    pub evidence_id: EvidenceId,
    pub attempt_id: AttemptId,
    pub baseline_oid: GitOid,
    pub sealed_head_oid: GitOid,
    pub current_base_oid: GitOid,
    pub base_advanced: bool,
    pub patch_digest: Digest,
    pub command_log_digest: Digest,
    pub policy_report_digest: Digest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compact_receipts_digest: Option<Digest>,
    #[serde(default)]
    pub compact_receipt_count: u64,
    #[serde(default)]
    pub compact_receipt_rejections: u64,
    pub changed_files: Vec<ChangedFile>,
    #[serde(default)]
    pub submodule_state: Vec<SubmodulePin>,
    /// Patch/command capture was truncated at policy limits.
    #[serde(default)]
    pub truncated: bool,
    pub sealed_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle_digest: Option<Digest>,
}

/// Authorization boundary: binds a human decision to exact evidence and exact
/// Git state. Re-verified before any disposition is applied.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewDecision {
    pub id: ReviewDecisionId,
    pub actor: ActorRef,
    pub attempt_id: AttemptId,
    pub environment_generation: u32,
    pub evidence_id: EvidenceId,
    pub evidence_digest: Digest,
    pub baseline_oid: GitOid,
    pub reviewed_head_oid: GitOid,
    /// Expected base OID at decision time.
    pub expected_base_oid: GitOid,
    #[serde(default)]
    pub acknowledged_violations: Vec<PolicyViolationId>,
    pub strategy: IntegrationStrategy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    pub decided_at: DateTime<Utc>,
}

/// A line-anchored review comment (or reply) on sealed evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewComment {
    pub id: ReviewCommentId,
    /// Root comment id; for a root comment this equals `id`.
    pub thread_id: ReviewCommentId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<ReviewCommentId>,
    pub evidence_id: EvidenceId,
    pub attempt_id: AttemptId,
    pub path: String,
    /// Diff side: `"new"` or `"old"`.
    pub side: String,
    pub start_line: u32,
    pub end_line: u32,
    /// SHA-256 hex of the anchored line content at comment time.
    pub anchor_digest: String,
    /// Optional original anchor text (aids revision briefs; may be omitted).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_text: Option<String>,
    pub body: String,
    pub actor: ActorRef,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_by: Option<ActorRef>,
}

/// Durable record that review was returned for another pass.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangesRequested {
    pub id: ChangesRequestedId,
    pub actor: ActorRef,
    pub attempt_id: AttemptId,
    pub evidence_id: EvidenceId,
    pub evidence_digest: Digest,
    #[serde(default)]
    pub comment_ids: Vec<ReviewCommentId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub revision_brief: String,
    pub decided_at: DateTime<Utc>,
}

/// Build a revision brief from review comments plus an optional summary.
///
/// Each comment becomes `path:line` (plus a quoted anchor when available) and
/// its body. A non-empty summary is placed first.
pub fn compose_revision_brief<'a, I>(comments: I, summary: Option<&str>) -> String
where
    I: IntoIterator<Item = &'a ReviewComment>,
{
    let mut parts: Vec<String> = Vec::new();
    if let Some(summary) = summary.map(str::trim).filter(|s| !s.is_empty()) {
        parts.push(summary.to_owned());
    }
    for comment in comments {
        let mut header = format!("{}:{}", comment.path, comment.start_line);
        if let Some(anchor) = comment
            .anchor_text
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            let clipped: String = anchor.chars().take(120).collect();
            header.push(' ');
            header.push('"');
            header.push_str(&clipped);
            header.push('"');
        }
        let body = comment.body.trim();
        if body.is_empty() {
            parts.push(header);
        } else {
            parts.push(format!("{header}\n{body}"));
        }
    }
    parts.join("\n\n")
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkItem {
    pub schema_version: u32,
    pub id: WorkId,
    pub title: String,
    /// User intent, user-owned.
    pub brief: String,
    pub target: WorkTarget,
    pub policy: WorkPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<GovernedEnv>,
    #[serde(default)]
    pub attempts: Vec<Attempt>,
    /// Canonical set of running attempts. Every active attempt owns a fenced,
    /// attempt-scoped mutation environment.
    #[serde(default)]
    pub active_attempts: Vec<AttemptId>,
    /// Legacy projection retained while existing snapshots and clients migrate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_attempt: Option<AttemptId>,
    pub state: WorkState,
    /// Disposition recorded when state reaches Accepted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disposition: Option<AcceptedDisposition>,
    /// Every review decision ever recorded (valid or later invalidated);
    /// validity is re-verified at disposition time, never assumed.
    #[serde(default)]
    pub review_decisions: Vec<ReviewDecision>,
    #[serde(default)]
    pub review_comments: Vec<ReviewComment>,
    #[serde(default)]
    pub changes_requested: Vec<ChangesRequested>,
    pub owner: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl WorkItem {
    pub fn new(
        title: impl Into<String>,
        brief: impl Into<String>,
        target: WorkTarget,
        owner: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            schema_version: MODEL_SCHEMA_VERSION,
            id: WorkId::new(),
            title: title.into(),
            brief: brief.into(),
            target,
            policy: WorkPolicy::default(),
            environment: None,
            attempts: Vec::new(),
            active_attempts: Vec::new(),
            active_attempt: None,
            state: WorkState::Draft,
            disposition: None,
            review_decisions: Vec::new(),
            review_comments: Vec::new(),
            changes_requested: Vec::new(),
            owner: owner.into(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn attempt(&self, id: &AttemptId) -> Option<&Attempt> {
        self.attempts.iter().find(|a| &a.id == id)
    }

    pub fn attempt_mut(&mut self, id: &AttemptId) -> Option<&mut Attempt> {
        self.attempts.iter_mut().find(|a| &a.id == id)
    }

    pub fn next_attempt_seq(&self) -> u32 {
        self.attempts.iter().map(|a| a.seq).max().unwrap_or(0) + 1
    }

    /// Active ids from the canonical set, with a legacy snapshot fallback.
    pub fn active_attempt_ids(&self) -> Vec<&AttemptId> {
        if self.active_attempts.is_empty() {
            self.active_attempt.iter().collect()
        } else {
            self.active_attempts.iter().collect()
        }
    }

    pub fn has_active_attempts(&self) -> bool {
        !self.active_attempts.is_empty() || self.active_attempt.is_some()
    }

    pub fn latest_active_attempt(&self) -> Option<&Attempt> {
        self.active_attempt_ids()
            .into_iter()
            .filter_map(|id| self.attempt(id))
            .max_by_key(|attempt| attempt.seq)
    }

    pub fn environment_for_attempt(&self, id: &AttemptId) -> Option<&GovernedEnv> {
        self.attempt(id)
            .and_then(|attempt| attempt.environment.as_ref())
            .or(self.environment.as_ref())
    }

    pub fn latest_attempt_environment(&self) -> Option<&GovernedEnv> {
        self.attempts
            .iter()
            .filter_map(|attempt| {
                attempt
                    .environment
                    .as_ref()
                    .map(|environment| (attempt.seq, environment))
            })
            .max_by_key(|(seq, _)| *seq)
            .map(|(_, environment)| environment)
    }

    /// The worktree users and tools should see between attempts. Once an
    /// isolated attempt exists, its preserved environment carries continuity;
    /// the original undertaking environment remains the staging anchor.
    pub fn workspace_environment(&self) -> Option<&GovernedEnv> {
        if self.state == WorkState::Discarded {
            return None;
        }
        self.latest_active_attempt()
            .and_then(|attempt| attempt.environment.as_ref())
            .or_else(|| self.latest_attempt_environment())
            .or(self.environment.as_ref())
    }

    /// Aggregate state after one attempt changes lifecycle. A healthy peer
    /// keeps the undertaking executing; sealed evidence becomes reviewable
    /// only after the last active attempt releases custody.
    pub fn state_after_attempts(&self) -> WorkState {
        if self.has_active_attempts() {
            WorkState::Executing
        } else if self
            .attempts
            .iter()
            .any(|attempt| attempt.evidence_id.is_some())
        {
            WorkState::AwaitingReview
        } else {
            WorkState::Ready
        }
    }

    pub fn activate_attempt(&mut self, id: AttemptId) {
        if !self.active_attempts.contains(&id) {
            self.active_attempts.push(id.clone());
        }
        self.active_attempt = Some(id);
    }

    pub fn deactivate_attempt(&mut self, id: &AttemptId) {
        self.active_attempts.retain(|active| active != id);
        self.active_attempt = self
            .active_attempts
            .iter()
            .filter_map(|active| self.attempt(active).map(|attempt| (attempt.seq, active)))
            .max_by_key(|(seq, _)| *seq)
            .map(|(_, active)| active.clone());
        if self.active_attempts.is_empty() && self.active_attempt.as_ref() == Some(id) {
            self.active_attempt = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_target() -> WorkTarget {
        WorkTarget::Git(GitWorkTarget {
            repo_path: PathBuf::from("/tmp/repo"),
            base_ref: "main".into(),
            base_oid: GitOid::new("a".repeat(40)),
        })
    }

    #[test]
    fn work_item_serde_round_trip() {
        let mut item = WorkItem::new("Fix the thing", "make it work", sample_target(), "user-1");
        item.policy.allowed_paths.push("src/**".into());
        let json = serde_json::to_string_pretty(&item).unwrap();
        let back: WorkItem = serde_json::from_str(&json).unwrap();
        assert_eq!(item, back);
        assert_eq!(back.schema_version, MODEL_SCHEMA_VERSION);
    }

    #[test]
    fn legacy_active_attempt_snapshot_projects_into_canonical_set() {
        let mut item = WorkItem::new("t", "b", sample_target(), "user-1");
        let active = AttemptId::new();
        item.active_attempt = Some(active.clone());
        let mut value = serde_json::to_value(&item).unwrap();
        value.as_object_mut().unwrap().remove("active_attempts");

        let restored: WorkItem = serde_json::from_value(value).unwrap();
        assert!(restored.has_active_attempts());
        assert_eq!(restored.active_attempt_ids(), vec![&active]);
    }

    #[test]
    fn environment_and_attempt_round_trip() {
        let mut item = WorkItem::new("t", "b", sample_target(), "user-1");
        let env = GovernedEnv {
            kind: EnvironmentKind::GitWorktree,
            repo: RepoIdentity {
                repo_id: RepoId::new(),
                requested_path: PathBuf::from("/tmp/repo"),
                common_dir: PathBuf::from("/tmp/repo/.git"),
                format: Some("0".into()),
                remotes: vec!["git@example.com:r.git".into()],
            },
            worktree: PathBuf::from("/tmp/forge/worktrees/repo-x/work-1"),
            branch: format!("medousa/work/{}", item.id),
            baseline_oid: GitOid::new("b".repeat(40)),
            generation: 1,
        };
        item.environment = Some(env);
        let lease = ExecutionLease {
            lease_id: LeaseId::new(),
            generation: 1,
            work_id: item.id.clone(),
            attempt_id: AttemptId::new(),
            owner_instance_id: "boot-1".into(),
            acquired_at: Utc::now(),
            heartbeat_at: Utc::now(),
            pid: Some(4242),
            process_start_marker: Some("marker".into()),
        };
        let attempt = Attempt {
            id: lease.attempt_id.clone(),
            seq: 1,
            executor: ExecutorDescriptor {
                kind: "script".into(),
                detail: serde_json::json!({"argv": ["echo", "hi"]}),
            },
            state: AttemptState::Running,
            environment: None,
            lease: Some(lease),
            recovery: None,
            evidence_id: None,
            started_at: Utc::now(),
            ended_at: None,
        };
        item.activate_attempt(attempt.id.clone());
        item.attempts.push(attempt);
        item.state = WorkState::Executing;

        let json = serde_json::to_string(&item).unwrap();
        let back: WorkItem = serde_json::from_str(&json).unwrap();
        assert_eq!(item, back);
        assert_eq!(back.active_attempts.len(), 1);
        let att = back.latest_active_attempt().unwrap();
        assert_eq!(att.lease.as_ref().unwrap().generation, 1);
    }

    #[test]
    fn review_decision_round_trip() {
        let decision = ReviewDecision {
            id: ReviewDecisionId::new(),
            actor: ActorRef {
                kind: ActorKind::User,
                id: "user-1".into(),
            },
            attempt_id: AttemptId::new(),
            environment_generation: 1,
            evidence_id: EvidenceId::new(),
            evidence_digest: Digest::sha256_hex(b"canonical"),
            baseline_oid: GitOid::new("a".repeat(40)),
            reviewed_head_oid: GitOid::new("c".repeat(40)),
            expected_base_oid: GitOid::new("a".repeat(40)),
            acknowledged_violations: vec![PolicyViolationId::new()],
            strategy: IntegrationStrategy::PreserveBranch,
            rationale: Some("looks right".into()),
            decided_at: Utc::now(),
        };
        let json = serde_json::to_string(&decision).unwrap();
        let back: ReviewDecision = serde_json::from_str(&json).unwrap();
        assert_eq!(decision, back);
    }

    #[test]
    fn terminal_states() {
        assert!(WorkState::Accepted.is_terminal());
        assert!(WorkState::Discarded.is_terminal());
        assert!(WorkState::Failed.is_terminal());
        assert!(!WorkState::Executing.is_terminal());
    }

    #[test]
    fn digest_is_stable_sha256_hex() {
        let d = Digest::sha256_hex(b"forge");
        assert_eq!(d.as_str().len(), 64);
        assert_eq!(d, Digest::sha256_hex(b"forge"));
        assert_ne!(d, Digest::sha256_hex(b"other"));
    }
}
