//! The Forge facade: the caller-driven lifecycle API. Executors are
//! replaceable callers of the lease API (`begin_attempt` / `heartbeat` /
//! `complete_attempt` / `interrupt_attempt` / `fail_attempt`); Forge owns
//! environments, evidence, review, dispositions, and recovery — it never runs
//! an executor and never resumes a provider.

use std::path::{Path, PathBuf};

use chrono::Utc;

use crate::error::{ForgeError, Result};
use crate::events::{EventPayload, OperationKind, SideEffect, TransitionEvent};
use crate::git::{CheckpointAuthor, GitEngine};
use crate::model::{
    AcceptedDisposition, ActorKind, ActorRef, Attempt, AttemptState, CaptureRisk, ChangeStatus,
    ChangedFile, Digest, EvidenceId, EvidenceManifest, ExecutionLease, ExecutorDescriptor,
    GitWorkTarget, GovernedEnv, IntegrationStrategy, LeaseId, OperationId, PolicyReport,
    PolicyViolation, RecoveryDisposition, ReviewDecision, ReviewDecisionId, WorkId, WorkItem,
    WorkPolicy, WorkState, WorkTarget, MODEL_SCHEMA_VERSION,
};
use crate::store::FsWorkStore;

/// Per-seal options: risk acknowledgment and checkpoint authorship.
#[derive(Debug, Clone, Default)]
pub struct SealOptions {
    /// Acknowledge capture risks (only honored when the item's policy sets
    /// `checkpoint_allow_risky_with_ack`).
    pub ack_risks: bool,
    /// Author attribution for the checkpoint commit (committer is always
    /// Medousa Forge).
    pub author: Option<CheckpointAuthor>,
}

fn file_facts(path: &Path) -> (bool, Option<u64>) {
    let Ok(meta) = std::fs::metadata(path) else {
        return (false, None);
    };
    if !meta.is_file() {
        return (false, None);
    }
    let is_binary = std::fs::read(path)
        .map(|b| b.iter().take(8192).any(|c| *c == 0))
        .unwrap_or(false);
    (is_binary, Some(meta.len()))
}

pub struct Forge {
    pub(crate) store: FsWorkStore,
    pub(crate) git: GitEngine,
    /// Boot identity stamped into every lease; reconciliation across restarts
    /// keys on this.
    pub(crate) instance_id: String,
}

impl Forge {
    pub fn open(forge_root: impl AsRef<Path>) -> Result<Self> {
        let store = FsWorkStore::open(forge_root)?;
        let git = GitEngine::detect()?;
        let instance_id = format!("boot-{}", LeaseId::new().as_str());
        Ok(Self {
            store,
            git,
            instance_id,
        })
    }

    pub fn with_git(forge_root: impl AsRef<Path>, git: GitEngine) -> Result<Self> {
        let mut forge = Self::open(forge_root)?;
        forge.git = git;
        Ok(forge)
    }

    pub fn store(&self) -> &FsWorkStore {
        &self.store
    }

    pub fn git(&self) -> &GitEngine {
        &self.git
    }

    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }

    pub fn system_actor() -> ActorRef {
        ActorRef {
            kind: ActorKind::System,
            id: "forge".into(),
        }
    }

    // ------------------------------------------------------------------
    // Registration
    // ------------------------------------------------------------------

    /// Register a new work item against a git repository. Captures the base
    /// OID at registration — integration later binds to exact OIDs.
    pub fn register(
        &self,
        title: impl Into<String>,
        brief: impl Into<String>,
        repo_path: impl AsRef<Path>,
        base_ref: impl Into<String>,
        owner: impl Into<String>,
        actor: &ActorRef,
    ) -> Result<WorkItem> {
        self.register_with_policy(
            title,
            brief,
            repo_path,
            base_ref,
            owner,
            WorkPolicy::default(),
            actor,
        )
    }

    /// Register with an explicit work policy.
    #[allow(clippy::too_many_arguments)] // registration fields travel together
    pub fn register_with_policy(
        &self,
        title: impl Into<String>,
        brief: impl Into<String>,
        repo_path: impl AsRef<Path>,
        base_ref: impl Into<String>,
        owner: impl Into<String>,
        policy: WorkPolicy,
        actor: &ActorRef,
    ) -> Result<WorkItem> {
        let repo_path = repo_path.as_ref();
        let base_ref = base_ref.into();
        if !self.git.is_repo(repo_path) {
            return Err(ForgeError::Git(format!(
                "{} is not inside a git repository",
                repo_path.display()
            )));
        }
        let base_oid = self.git.resolve_oid(repo_path, &base_ref)?;
        let mut item = WorkItem::new(
            title,
            brief,
            WorkTarget::Git(GitWorkTarget {
                repo_path: repo_path.to_path_buf(),
                base_ref,
                base_oid,
            }),
            owner,
        );
        item.policy = policy;
        let _item_lock = self.store.lock_item(&item.id)?;
        self.store.append(
            &item.id,
            actor,
            EventPayload::ItemRegistered {
                item: Box::new(item.clone()),
            },
        )?;
        self.persist(&item, 1)?;
        Ok(item)
    }

    /// Load an item: snapshot cache when fresh, fold-from-events otherwise.
    pub fn load(&self, work_id: &WorkId) -> Result<WorkItem> {
        let events = self.store.replay(work_id)?;
        if events.is_empty() {
            return Err(ForgeError::WorkNotFound(work_id.clone()));
        }
        let last_seq = events.last().map(|e| e.seq).unwrap_or(0);
        if let Some(envelope) = self.store.read_snapshot(work_id)?
            && envelope.applied_seq == last_seq
            && envelope.item.schema_version == MODEL_SCHEMA_VERSION
        {
            return Ok(envelope.item);
        }
        let item = fold(&events)?;
        self.persist(&item, last_seq)?;
        Ok(item)
    }

    pub fn list(&self) -> Result<Vec<WorkItem>> {
        let mut items = Vec::new();
        for id in self.store.list_item_ids()? {
            items.push(self.load(&id)?);
        }
        Ok(items)
    }

    // ------------------------------------------------------------------
    // Provisioning
    // ------------------------------------------------------------------

    /// Provision the governed environment (one per environment generation).
    /// Draft → Provisioning → Ready.
    pub fn provision(&self, work_id: &WorkId, actor: &ActorRef) -> Result<WorkItem> {
        // Unlocked read to discover the repo; locks are then taken in
        // repo → item order and the item is re-read authoritatively.
        let probe = self.load(work_id)?;
        let target = git_target(&probe)?;
        let _repo_lock = self
            .store
            .lock_repo(&self.repo_lock_key(&target.repo_path)?)?;
        let _item_lock = self.store.lock_item(work_id)?;
        let mut item = self.load(work_id)?;
        expect_state(&item, WorkState::Draft, "provision")?;

        let operation_id = OperationId::new();
        self.transition(&mut item, WorkState::Provisioning, None, actor)?;
        self.store.append(
            work_id,
            actor,
            EventPayload::OperationStarted {
                operation_id: operation_id.clone(),
                kind: OperationKind::Provision,
            },
        )?;

        match self.provision_inner(&mut item, &target, &operation_id, actor) {
            Ok(()) => {
                self.store.append(
                    work_id,
                    actor,
                    EventPayload::OperationCommitted {
                        operation_id,
                        resulting_state: WorkState::Ready,
                    },
                )?;
                self.transition(&mut item, WorkState::Ready, None, actor)?;
                Ok(item)
            }
            Err(err) => {
                let _ = self.store.append(
                    work_id,
                    actor,
                    EventPayload::OperationAborted {
                        operation_id,
                        reason: err.to_string(),
                    },
                );
                let _ = self.transition(
                    &mut item,
                    WorkState::Failed,
                    Some(format!("provision failed: {err}")),
                    actor,
                );
                Err(err)
            }
        }
    }

    fn provision_inner(
        &self,
        item: &mut WorkItem,
        target: &GitWorkTarget,
        operation_id: &OperationId,
        actor: &ActorRef,
    ) -> Result<()> {
        let repo = self.git.repo_identity(&target.repo_path)?;
        // Baseline binds to the *current* base OID at provision time; the
        // immutable OID is what evidence and integration compare against.
        let baseline_oid = self.git.resolve_oid(&target.repo_path, &target.base_ref)?;
        let generation = item.environment.as_ref().map(|e| e.generation + 1).unwrap_or(1);
        let worktree = self.worktree_path(&repo.repo_id, &item.id, generation);
        let branch = format!("medousa/work/{}", item.id.as_str());
        self.git
            .worktree_add(&target.repo_path, &worktree, &branch, &baseline_oid)?;
        self.store.append(
            &item.id,
            actor,
            EventPayload::OperationSideEffect {
                operation_id: operation_id.clone(),
                effect: SideEffect::WorktreeAdded {
                    path: worktree.clone(),
                    branch: branch.clone(),
                    baseline_oid: baseline_oid.clone(),
                },
            },
        )?;
        let env = GovernedEnv {
            kind: crate::model::EnvironmentKind::GitWorktree,
            repo,
            worktree,
            branch,
            baseline_oid,
            generation,
        };
        item.environment = Some(env.clone());
        self.store.append(
            &item.id,
            actor,
            EventPayload::EnvironmentProvisioned {
                env: Box::new(env),
            },
        )?;
        Ok(())
    }

    fn worktree_path(
        &self,
        repo_id: &crate::model::RepoId,
        work_id: &WorkId,
        generation: u32,
    ) -> PathBuf {
        let digest = Digest::sha256_hex(repo_id.as_str().as_bytes());
        let repo_short = &digest.as_str()[..12];
        self.store
            .root()
            .join("worktrees")
            .join(repo_short)
            .join(format!("{}-gen{generation}", work_id.as_str()))
    }

    // ------------------------------------------------------------------
    // Attempts (lease-fenced)
    // ------------------------------------------------------------------

    /// Begin an attempt: Ready → Executing. Returns the fenced lease the
    /// executor must present back to mutate the attempt.
    pub fn begin_attempt(
        &self,
        work_id: &WorkId,
        executor: ExecutorDescriptor,
        pid: Option<u32>,
        actor: &ActorRef,
    ) -> Result<(WorkItem, ExecutionLease)> {
        let _item_lock = self.store.lock_item(work_id)?;
        let mut item = self.load(work_id)?;
        expect_state(&item, WorkState::Ready, "begin attempt")?;
        if item.active_attempt.is_some() {
            return Err(ForgeError::AttemptAlreadyRunning(work_id.clone()));
        }
        let attempt_id = crate::model::AttemptId::new();
        let lease = ExecutionLease {
            lease_id: LeaseId::new(),
            // Fencing tokens are monotonic across the whole work item, derived
            // from the event log — a superseded adapter can never present a
            // current token.
            generation: self.next_lease_generation(work_id)?,
            work_id: work_id.clone(),
            attempt_id: attempt_id.clone(),
            owner_instance_id: self.instance_id.clone(),
            acquired_at: Utc::now(),
            heartbeat_at: Utc::now(),
            pid,
            process_start_marker: None,
        };
        let attempt = Attempt {
            id: attempt_id.clone(),
            seq: item.next_attempt_seq(),
            executor,
            state: AttemptState::Running,
            lease: Some(lease.clone()),
            recovery: None,
            evidence_id: None,
            started_at: Utc::now(),
            ended_at: None,
        };
        item.active_attempt = Some(attempt_id.clone());
        item.attempts.push(attempt.clone());
        self.store.append(
            work_id,
            actor,
            EventPayload::AttemptStarted {
                attempt: Box::new(attempt),
            },
        )?;
        self.store.append(
            work_id,
            actor,
            EventPayload::LeaseAcquired {
                attempt_id,
                lease_id: lease.lease_id.clone(),
                generation: lease.generation,
                owner_instance_id: lease.owner_instance_id.clone(),
            },
        )?;
        self.transition(&mut item, WorkState::Executing, None, actor)?;
        Ok((item, lease))
    }

    /// Liveness signal from the executor. Updates the lease record in the
    /// snapshot only — heartbeats are never appended to the JSONL event log.
    pub fn heartbeat(&self, lease: &ExecutionLease) -> Result<()> {
        let _item_lock = self.store.lock_item(&lease.work_id)?;
        let mut item = self.load(&lease.work_id)?;
        self.fence(&item, lease)?;
        let attempt = item
            .attempt_mut(&lease.attempt_id)
            .ok_or_else(|| ForgeError::AttemptNotFound(lease.attempt_id.clone()))?;
        if let Some(active) = attempt.lease.as_mut() {
            active.heartbeat_at = Utc::now();
        }
        self.persist_fresh(&mut item)?;
        Ok(())
    }

    /// Lease-fenced append of one JSONL line under
    /// `attempts/{seq}/evidence/commands.jsonl`. Holds the item lock so a
    /// concurrent seal cannot read a torn file. Adapters own the schema;
    /// Forge only guarantees the file exists and is digestable at seal.
    pub fn append_command_log(
        &self,
        lease: &ExecutionLease,
        line: &serde_json::Value,
    ) -> Result<()> {
        let _item_lock = self.store.lock_item(&lease.work_id)?;
        let item = self.load(&lease.work_id)?;
        self.fence(&item, lease)?;
        let attempt = item
            .attempt(&lease.attempt_id)
            .ok_or_else(|| ForgeError::AttemptNotFound(lease.attempt_id.clone()))?;
        let evidence_dir = self
            .store
            .item_dir(&lease.work_id)
            .join("attempts")
            .join(attempt.seq.to_string())
            .join("evidence");
        std::fs::create_dir_all(&evidence_dir)?;
        let path = evidence_dir.join("commands.jsonl");
        let mut bytes = serde_json::to_vec(line)?;
        bytes.push(b'\n');
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        Ok(())
    }

    /// Latest `ResumeSupported` provider token on this work item (most recent
    /// interrupted attempt wins). Adapters use this to reattach; Forge never
    /// resumes providers itself.
    pub fn latest_resume_token(&self, work_id: &WorkId) -> Result<Option<String>> {
        let item = self.load(work_id)?;
        for attempt in item.attempts.iter().rev() {
            if let Some(RecoveryDisposition::ResumeSupported { provider_token }) =
                &attempt.recovery
                && !provider_token.trim().is_empty()
            {
                return Ok(Some(provider_token.clone()));
            }
        }
        Ok(None)
    }

    /// Interrupt a running attempt (crash, cancel, operator stop). The work
    /// environment and any uncommitted work are preserved untouched; the item
    /// returns to Ready for a future attempt. `recovery` records what a later
    /// adapter may do — Forge never resumes providers itself.
    pub fn interrupt_attempt(
        &self,
        lease: &ExecutionLease,
        recovery: RecoveryDisposition,
        actor: &ActorRef,
    ) -> Result<WorkItem> {
        let _item_lock = self.store.lock_item(&lease.work_id)?;
        let mut item = self.load(&lease.work_id)?;
        expect_state(&item, WorkState::Executing, "interrupt attempt")?;
        self.fence(&item, lease)?;
        let attempt_id = lease.attempt_id.clone();
        self.end_attempt(&mut item, &attempt_id, AttemptState::Interrupted, recovery, actor)?;
        self.transition(&mut item, WorkState::Ready, None, actor)?;
        Ok(item)
    }

    /// Fail a running attempt (executor reported an error outcome).
    pub fn fail_attempt(
        &self,
        lease: &ExecutionLease,
        error: &str,
        actor: &ActorRef,
    ) -> Result<WorkItem> {
        let _item_lock = self.store.lock_item(&lease.work_id)?;
        let mut item = self.load(&lease.work_id)?;
        expect_state(&item, WorkState::Executing, "fail attempt")?;
        self.fence(&item, lease)?;
        let attempt_id = lease.attempt_id.clone();
        self.end_attempt(
            &mut item,
            &attempt_id,
            AttemptState::Failed,
            RecoveryDisposition::RestartAllowed,
            actor,
        )?;
        self.transition(
            &mut item,
            WorkState::Ready,
            Some(format!("attempt failed: {error}")),
            actor,
        )?;
        Ok(item)
    }

    /// Complete an attempt: seal the environment (checkpoint commit + evidence)
    /// and move to AwaitingReview. Fencing: the presented lease must be the
    /// active lease for the active attempt.
    pub fn complete_attempt(
        &self,
        lease: &ExecutionLease,
        options: &SealOptions,
        actor: &ActorRef,
    ) -> Result<WorkItem> {
        let _item_lock = self.store.lock_item(&lease.work_id)?;
        let mut item = self.load(&lease.work_id)?;
        expect_state(&item, WorkState::Executing, "complete attempt")?;
        self.fence(&item, lease)?;

        let operation_id = OperationId::new();
        self.transition(&mut item, WorkState::Sealing, None, actor)?;
        self.store.append(
            &lease.work_id,
            actor,
            EventPayload::OperationStarted {
                operation_id: operation_id.clone(),
                kind: OperationKind::Seal,
            },
        )?;

        match self.seal_inner(&mut item, lease, options, &operation_id, actor) {
            Ok(()) => {
                self.store.append(
                    &lease.work_id,
                    actor,
                    EventPayload::OperationCommitted {
                        operation_id,
                        resulting_state: WorkState::AwaitingReview,
                    },
                )?;
                self.end_attempt(
                    &mut item,
                    &lease.attempt_id,
                    AttemptState::Completed,
                    RecoveryDisposition::NotResumable,
                    actor,
                )?;
                self.transition(&mut item, WorkState::AwaitingReview, None, actor)?;
                Ok(item)
            }
            Err(err) => {
                let _ = self.store.append(
                    &lease.work_id,
                    actor,
                    EventPayload::OperationAborted {
                        operation_id,
                        reason: err.to_string(),
                    },
                );
                let _ = self.transition(
                    &mut item,
                    WorkState::Executing,
                    Some(format!("seal failed: {err}")),
                    actor,
                );
                Err(err)
            }
        }
    }

    fn seal_inner(
        &self,
        item: &mut WorkItem,
        lease: &ExecutionLease,
        options: &SealOptions,
        operation_id: &OperationId,
        actor: &ActorRef,
    ) -> Result<()> {
        let env = item
            .environment
            .clone()
            .ok_or_else(|| ForgeError::EnvironmentDrift("no environment".into()))?;
        let attempt = item
            .attempt(&lease.attempt_id)
            .ok_or_else(|| ForgeError::AttemptNotFound(lease.attempt_id.clone()))?
            .clone();
        let policy = &item.policy;
        if !policy.checkpoint_capture_all {
            return Err(ForgeError::CaptureBlocked(
                "selective capture is not implemented; set checkpoint_capture_all = true".into(),
            ));
        }

        // Pre-commit view: what the executor changed (tracked + untracked).
        let pre_changed = self.worktree_changed_files(&env)?;
        let violations = crate::policy::evaluate_paths(policy, &pre_changed)?;
        let exclusions = crate::policy::capture_exclusions(policy, &pre_changed)?;
        let candidates: Vec<String> = pre_changed
            .iter()
            .map(|f| crate::policy::normalize_git_path(&f.path))
            .filter(|p| !exclusions.contains(p))
            .collect();
        let risks = crate::policy::assess_capture(policy, &env.worktree, &candidates)?;
        if !risks.is_empty() && (!policy.checkpoint_allow_risky_with_ack || !options.ack_risks) {
            return Err(ForgeError::CaptureBlocked(format!(
                "{} capture risk(s) require acknowledgment (first: {:?})",
                risks.len(),
                risks[0]
            )));
        }

        let message = format!(
            "forge: checkpoint {} attempt {}",
            item.id.as_str(),
            attempt.seq
        );
        let author = options.author.clone().unwrap_or_default();
        let sealed_head = self.git.commit_checkpoint_with_exclusions(
            &env.worktree,
            &message,
            &author,
            &exclusions,
        )?;
        self.store.append(
            &item.id,
            actor,
            EventPayload::OperationSideEffect {
                operation_id: operation_id.clone(),
                effect: SideEffect::CheckpointCommitCreated {
                    branch: env.branch.clone(),
                    oid: sealed_head.clone(),
                },
            },
        )?;

        let evidence =
            self.capture_evidence(item, &env, &attempt, &sealed_head, violations, risks)?;
        if let Some(att) = item.attempt_mut(&lease.attempt_id) {
            att.evidence_id = Some(evidence.evidence_id.clone());
        }
        self.store.append(
            &item.id,
            actor,
            EventPayload::EvidenceSealed {
                attempt_id: lease.attempt_id.clone(),
                evidence_id: evidence.evidence_id.clone(),
                evidence_digest: evidence.bundle_digest.clone().unwrap(),
            },
        )?;
        Ok(())
    }

    /// Changed files in the worktree right now (porcelain view), normalized
    /// and with `.git` internals filtered out.
    pub(crate) fn worktree_changed_files(&self, env: &GovernedEnv) -> Result<Vec<ChangedFile>> {
        let entries = self.git.status_porcelain(&env.worktree)?;
        let mut files = Vec::new();
        for entry in entries {
            if entry.kind == crate::git::PorcelainKind::Ignored {
                continue;
            }
            let path = crate::policy::normalize_git_path(&entry.path);
            if crate::policy::is_git_internal(&path) {
                continue;
            }
            let status = match entry.kind {
                crate::git::PorcelainKind::Untracked => ChangeStatus::Untracked,
                crate::git::PorcelainKind::RenameOrCopy => {
                    if entry.xy.as_deref().unwrap_or_default().starts_with('C') {
                        ChangeStatus::Copied
                    } else {
                        ChangeStatus::Renamed
                    }
                }
                _ => {
                    let xy = entry.xy.as_deref().unwrap_or_default();
                    if xy.contains('A') {
                        ChangeStatus::Added
                    } else if xy.contains('D') {
                        ChangeStatus::Deleted
                    } else if xy.contains('T') {
                        ChangeStatus::TypeChanged
                    } else {
                        ChangeStatus::Modified
                    }
                }
            };
            let (is_binary, byte_size) = file_facts(&env.worktree.join(&path));
            files.push(ChangedFile {
                path,
                status,
                old_path: entry.orig_path,
                is_binary,
                byte_size,
            });
        }
        Ok(files)
    }

    pub(crate) fn capture_evidence(
        &self,
        item: &WorkItem,
        env: &GovernedEnv,
        attempt: &Attempt,
        sealed_head: &crate::model::GitOid,
        violations: Vec<PolicyViolation>,
        risks: Vec<CaptureRisk>,
    ) -> Result<EvidenceManifest> {
        let evidence_dir = self
            .store
            .item_dir(&item.id)
            .join("attempts")
            .join(attempt.seq.to_string())
            .join("evidence");
        std::fs::create_dir_all(&evidence_dir)?;

        let patch = self
            .git
            .diff_binary(&env.worktree, &env.baseline_oid, sealed_head)?;
        std::fs::write(evidence_dir.join("patch.diff"), &patch)?;

        // Adapters own command/transcript capture; Forge only guarantees the
        // files exist so digests are well-defined.
        let commands_path = evidence_dir.join("commands.jsonl");
        if !commands_path.exists() {
            std::fs::write(&commands_path, b"")?;
        }
        let commands = std::fs::read(&commands_path)?;

        let (symlinks, nested_repos) = crate::policy::scan_worktree(&env.worktree)?;
        let submodule_state = self
            .git
            .submodule_pins(&env.worktree, &env.baseline_oid)
            .unwrap_or_default();
        let submodules: Vec<String> = submodule_state.iter().map(|s| s.path.clone()).collect();
        let policy = PolicyReport {
            violations,
            capture_risks: risks,
            symlinks,
            submodules,
            nested_repos,
        };
        let policy_json = serde_json::to_vec_pretty(&policy)?;
        std::fs::write(evidence_dir.join("policy.json"), &policy_json)?;

        // Post-commit committed view: exact baseline → sealed head.
        let name_status = self
            .git
            .diff_name_status(&env.worktree, &env.baseline_oid, sealed_head)?;
        let changed_files: Vec<ChangedFile> = name_status
            .into_iter()
            .map(|ns| {
                let path = crate::policy::normalize_git_path(&ns.path);
                let (is_binary, byte_size) = file_facts(&env.worktree.join(&path));
                ChangedFile {
                    path,
                    status: match ns.status {
                        'A' => ChangeStatus::Added,
                        'D' => ChangeStatus::Deleted,
                        'T' => ChangeStatus::TypeChanged,
                        'R' => ChangeStatus::Renamed,
                        'C' => ChangeStatus::Copied,
                        _ => ChangeStatus::Modified,
                    },
                    old_path: ns.orig_path,
                    is_binary,
                    byte_size,
                }
            })
            .collect();

        let current_base_oid = match &item.target {
            WorkTarget::Git(t) => self.git.ref_oid(&t.repo_path, &t.base_ref)?,
        };
        let mut manifest = EvidenceManifest {
            schema_version: MODEL_SCHEMA_VERSION,
            evidence_id: EvidenceId::new(),
            attempt_id: attempt.id.clone(),
            baseline_oid: env.baseline_oid.clone(),
            sealed_head_oid: sealed_head.clone(),
            current_base_oid: current_base_oid.clone(),
            base_advanced: current_base_oid != env.baseline_oid,
            patch_digest: Digest::sha256_hex(&patch),
            command_log_digest: Digest::sha256_hex(&commands),
            policy_report_digest: Digest::sha256_hex(&policy_json),
            changed_files,
            submodule_state,
            truncated: false,
            sealed_at: Utc::now(),
            bundle_digest: None,
        };
        // Canonical digest: fixed-order serialization with the digest field
        // absent, SHA-256.
        let canonical = serde_json::to_vec(&manifest)?;
        manifest.bundle_digest = Some(Digest::sha256_hex(&canonical));
        std::fs::write(
            evidence_dir.join("manifest.json"),
            serde_json::to_vec_pretty(&manifest)?,
        )?;
        Ok(manifest)
    }

    // ------------------------------------------------------------------
    // Review & dispositions
    // ------------------------------------------------------------------

    /// Record a review decision. The decision binds to exact evidence and
    /// exact Git state; it is re-verified at disposition time.
    pub fn decide(
        &self,
        work_id: &WorkId,
        decision: ReviewDecision,
        actor: &ActorRef,
    ) -> Result<WorkItem> {
        let _item_lock = self.store.lock_item(work_id)?;
        let mut item = self.load(work_id)?;
        expect_state(&item, WorkState::AwaitingReview, "record review decision")?;
        item.review_decisions.push(decision.clone());
        self.store.append(
            &item.id,
            actor,
            EventPayload::ReviewDecided {
                decision: Box::new(decision),
            },
        )?;
        self.persist_fresh(&mut item)?;
        Ok(item)
    }

    /// Apply an accepted decision. PreserveBranch touches nothing upstream —
    /// the reviewable branch is the durable outcome.
    pub fn apply_decision(
        &self,
        work_id: &WorkId,
        decision_id: &ReviewDecisionId,
        actor: &ActorRef,
    ) -> Result<WorkItem> {
        // Integration mutates the repository: repo lock before item lock.
        let probe = self.load(work_id)?;
        let repo_key = match &probe.environment {
            Some(env) => self.repo_lock_key(&env.repo.common_dir)?,
            None => self.repo_lock_key(&git_target(&probe)?.repo_path)?,
        };
        let _repo_lock = self.store.lock_repo(&repo_key)?;
        let _item_lock = self.store.lock_item(work_id)?;
        let mut item = self.load(work_id)?;
        expect_state(&item, WorkState::AwaitingReview, "apply decision")?;
        let decision = item
            .review_decisions
            .iter()
            .find(|d| &d.id == decision_id)
            .cloned()
            .ok_or_else(|| ForgeError::DecisionInvalid {
                reason: format!("no decision {decision_id}"),
            })?;

        self.verify_decision(&item, &decision)?;

        let operation_id = OperationId::new();
        self.transition(&mut item, WorkState::ApplyingDecision, None, actor)?;
        self.store.append(
            work_id,
            actor,
            EventPayload::OperationStarted {
                operation_id: operation_id.clone(),
                kind: OperationKind::Integrate,
            },
        )?;

        let (disposition, detail) = match decision.strategy {
            IntegrationStrategy::PreserveBranch => (AcceptedDisposition::BranchPreserved, None),
            IntegrationStrategy::FastForwardOnly => {
                match self.integrate_fast_forward(&item, &decision, &operation_id, actor) {
                    Ok(detail) => (AcceptedDisposition::BaseFastForwarded, Some(detail)),
                    Err(err) => {
                        let _ = self.store.append(
                            work_id,
                            actor,
                            EventPayload::OperationAborted {
                                operation_id,
                                reason: err.to_string(),
                            },
                        );
                        // Base advanced or conflict: back to review, approval
                        // did not leak.
                        let _ =
                            self.transition(&mut item, WorkState::AwaitingReview, None, actor);
                        return Err(err);
                    }
                }
            }
            IntegrationStrategy::ExportPatch => {
                match self.integrate_export_patch(&item, &decision, &operation_id, actor) {
                    Ok(detail) => (AcceptedDisposition::PatchExported, Some(detail)),
                    Err(err) => {
                        let _ = self.store.append(
                            work_id,
                            actor,
                            EventPayload::OperationAborted {
                                operation_id,
                                reason: err.to_string(),
                            },
                        );
                        let _ =
                            self.transition(&mut item, WorkState::AwaitingReview, None, actor);
                        return Err(err);
                    }
                }
            }
        };

        item.disposition = Some(disposition);
        self.store.append(
            work_id,
            actor,
            EventPayload::DispositionApplied { disposition, detail },
        )?;
        self.store.append(
            work_id,
            actor,
            EventPayload::OperationCommitted {
                operation_id,
                resulting_state: WorkState::Accepted,
            },
        )?;
        self.transition(&mut item, WorkState::Accepted, None, actor)?;
        Ok(item)
    }

    /// Fast-forward the base ref to the reviewed head. Guarded by the
    /// decision's expected base OID + an atomic CAS ref update, and safe for a
    /// base that is checked out in another worktree (that checkout must be
    /// clean; it is synced after the ref moves).
    fn integrate_fast_forward(
        &self,
        item: &WorkItem,
        decision: &ReviewDecision,
        operation_id: &OperationId,
        actor: &ActorRef,
    ) -> Result<String> {
        let target = git_target(item)?;
        let env = item
            .environment
            .as_ref()
            .ok_or_else(|| ForgeError::EnvironmentDrift("no environment".into()))?;

        // Expected base OID: the approval authorizes integration against
        // exactly this base state.
        let current_base = self.git.ref_oid(&target.repo_path, &target.base_ref)?;
        if current_base != decision.expected_base_oid {
            return Err(ForgeError::BaseAdvanced {
                expected: decision.expected_base_oid.clone(),
                found: current_base,
            });
        }
        // The reviewed head must strictly descend from the base.
        if !self
            .git
            .is_ancestor(&target.repo_path, &current_base, &decision.reviewed_head_oid)?
        {
            return Err(ForgeError::DecisionInvalid {
                reason: "reviewed head does not descend from base — not fast-forwardable".into(),
            });
        }
        // Checked-out-base safety: if the base branch is checked out anywhere,
        // that checkout must be clean before we move its ref.
        let base_checkout = self
            .git
            .worktree_list(&target.repo_path)?
            .into_iter()
            .find(|(_, branch)| branch.as_deref() == Some(target.base_ref.as_str()))
            .map(|(path, _)| path);
        if let Some(checkout) = &base_checkout
            && !self.git.is_clean(checkout)?
        {
            return Err(ForgeError::EnvironmentDrift(format!(
                "base ref {} is checked out at {} with uncommitted changes",
                target.base_ref,
                checkout.display()
            )));
        }

        // Atomic compare-and-swap: fails without touching the ref if the base
        // moved between our check and the update.
        let ref_name = format!("refs/heads/{}", target.base_ref);
        self.git
            .update_ref_cas(
                &target.repo_path,
                &ref_name,
                &decision.reviewed_head_oid,
                &decision.expected_base_oid,
            )
            .map_err(|_| ForgeError::BaseAdvanced {
                expected: decision.expected_base_oid.clone(),
                found: self
                    .git
                    .ref_oid(&target.repo_path, &target.base_ref)
                    .unwrap_or_else(|_| decision.expected_base_oid.clone()),
            })?;
        self.store.append(
            &item.id,
            actor,
            EventPayload::OperationSideEffect {
                operation_id: operation_id.clone(),
                effect: SideEffect::BaseRefAdvanced {
                    ref_name: ref_name.clone(),
                    old_oid: decision.expected_base_oid.clone(),
                    new_oid: decision.reviewed_head_oid.clone(),
                },
            },
        )?;
        // Sync the checked-out working tree to the moved ref.
        if let Some(checkout) = &base_checkout {
            self.git.reset_hard(checkout, &decision.reviewed_head_oid)?;
        }
        let _ = env; // environment is retained after fast-forward
        Ok(format!(
            "{} fast-forwarded {} → {}",
            ref_name,
            decision.expected_base_oid,
            decision.reviewed_head_oid
        ))
    }

    /// Export the reviewed work as a portable patch artifact under the item's
    /// disposition directory. Touches no refs.
    fn integrate_export_patch(
        &self,
        item: &WorkItem,
        decision: &ReviewDecision,
        operation_id: &OperationId,
        actor: &ActorRef,
    ) -> Result<String> {
        let env = item
            .environment
            .as_ref()
            .ok_or_else(|| ForgeError::EnvironmentDrift("no environment".into()))?;
        let patch = self.git.diff_binary(
            &env.worktree,
            &decision.baseline_oid,
            &decision.reviewed_head_oid,
        )?;
        let dir = self.store.item_dir(&item.id).join("dispositions");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("patch-{}.diff", decision.id.as_str()));
        std::fs::write(&path, &patch)?;
        let digest = Digest::sha256_hex(&patch);
        self.store.append(
            &item.id,
            actor,
            EventPayload::OperationSideEffect {
                operation_id: operation_id.clone(),
                effect: SideEffect::PatchExported {
                    path: path.clone(),
                    digest: digest.clone(),
                },
            },
        )?;
        Ok(format!("patch exported to {} ({digest})", path.display()))
    }

    /// Guarded discard: remove the worktree, then the branch, then terminal.
    /// Allowed from Ready or AwaitingReview; never while an attempt runs.
    pub fn discard(&self, work_id: &WorkId, actor: &ActorRef) -> Result<WorkItem> {
        let probe = self.load(work_id)?;
        let repo_key = match &probe.environment {
            Some(env) => self.repo_lock_key(&env.repo.common_dir)?,
            None => self.repo_lock_key(&git_target(&probe)?.repo_path)?,
        };
        let _repo_lock = self.store.lock_repo(&repo_key)?;
        let _item_lock = self.store.lock_item(work_id)?;
        let mut item = self.load(work_id)?;
        match item.state {
            WorkState::Ready | WorkState::AwaitingReview => {}
            state => {
                return Err(ForgeError::InvalidState {
                    work_id: work_id.clone(),
                    state,
                    action: "discard",
                })
            }
        }

        let operation_id = OperationId::new();
        self.store.append(
            work_id,
            actor,
            EventPayload::OperationStarted {
                operation_id: operation_id.clone(),
                kind: OperationKind::Discard,
            },
        )?;
        if let Some(env) = item.environment.clone() {
            let target = git_target(&item)?;
            if env.worktree.exists() {
                self.git.worktree_remove(&target.repo_path, &env.worktree)?;
            }
            self.store.append(
                work_id,
                actor,
                EventPayload::OperationSideEffect {
                    operation_id: operation_id.clone(),
                    effect: SideEffect::WorktreeRemoved {
                        path: env.worktree.clone(),
                    },
                },
            )?;
            if self.git.branch_exists(&target.repo_path, &env.branch) {
                self.git.branch_delete(&target.repo_path, &env.branch)?;
            }
            self.store.append(
                work_id,
                actor,
                EventPayload::OperationSideEffect {
                    operation_id: operation_id.clone(),
                    effect: SideEffect::BranchRemoved {
                        branch: env.branch.clone(),
                    },
                },
            )?;
        }
        self.store.append(
            work_id,
            actor,
            EventPayload::OperationCommitted {
                operation_id,
                resulting_state: WorkState::Discarded,
            },
        )?;
        self.transition(&mut item, WorkState::Discarded, None, actor)?;
        Ok(item)
    }

    /// Invalidate a recorded decision (head moved, evidence superseded, human
    /// reversal). The item stays in AwaitingReview.
    pub fn invalidate_decision(
        &self,
        work_id: &WorkId,
        decision_id: &ReviewDecisionId,
        reason: &str,
        actor: &ActorRef,
    ) -> Result<WorkItem> {
        let _item_lock = self.store.lock_item(work_id)?;
        let mut item = self.load(work_id)?;
        item.review_decisions.retain(|d| &d.id != decision_id);
        self.store.append(
            work_id,
            actor,
            EventPayload::DecisionInvalidated {
                decision_id: decision_id.clone(),
                reason: reason.to_string(),
            },
        )?;
        self.persist_fresh(&mut item)?;
        Ok(item)
    }

    /// Evidence-bound re-verification — the authorization boundary. Approval
    /// authorizes exactly one sealed state, never "whatever is there now".
    fn verify_decision(&self, item: &WorkItem, decision: &ReviewDecision) -> Result<()> {
        if item.active_attempt.is_some() {
            return Err(ForgeError::DecisionInvalid {
                reason: "an attempt is still active".into(),
            });
        }
        let env = item
            .environment
            .as_ref()
            .ok_or_else(|| ForgeError::DecisionInvalid {
                reason: "no environment".into(),
            })?;
        if env.generation != decision.environment_generation {
            return Err(ForgeError::DecisionInvalid {
                reason: "environment generation changed since decision".into(),
            });
        }
        let head = self.git.head_oid(&env.worktree)?;
        if head != decision.reviewed_head_oid {
            return Err(ForgeError::EnvironmentDrift(format!(
                "head moved since review: {} != {}",
                head.as_str(),
                decision.reviewed_head_oid.as_str()
            )));
        }
        if !self.git.is_clean(&env.worktree)? {
            return Err(ForgeError::EnvironmentDrift(
                "worktree dirty after seal".into(),
            ));
        }
        let manifest = self.read_evidence_manifest(item, decision)?;
        let stored = manifest.bundle_digest.clone().ok_or_else(|| {
            ForgeError::DecisionInvalid {
                reason: "evidence manifest has no digest".into(),
            }
        })?;
        if stored != decision.evidence_digest {
            return Err(ForgeError::EvidenceMismatch {
                expected: decision.evidence_digest.clone(),
                found: stored,
            });
        }
        // Every policy violation in the sealed evidence must be explicitly
        // acknowledged by the decision.
        let attempt = item
            .attempt(&decision.attempt_id)
            .ok_or_else(|| ForgeError::AttemptNotFound(decision.attempt_id.clone()))?;
        let policy_path = self
            .store
            .item_dir(&item.id)
            .join("attempts")
            .join(attempt.seq.to_string())
            .join("evidence")
            .join("policy.json");
        let report: PolicyReport =
            serde_json::from_str(&std::fs::read_to_string(&policy_path)?)?;
        for violation in &report.violations {
            if !decision.acknowledged_violations.contains(&violation.id) {
                return Err(ForgeError::PolicyViolation(format!(
                    "violation {} ({}) is not acknowledged by the decision",
                    violation.id, violation.rule
                )));
            }
        }
        Ok(())
    }

    fn read_evidence_manifest(
        &self,
        item: &WorkItem,
        decision: &ReviewDecision,
    ) -> Result<EvidenceManifest> {
        let attempt = item
            .attempt(&decision.attempt_id)
            .ok_or_else(|| ForgeError::AttemptNotFound(decision.attempt_id.clone()))?;
        let path = self
            .store
            .item_dir(&item.id)
            .join("attempts")
            .join(attempt.seq.to_string())
            .join("evidence")
            .join("manifest.json");
        let raw = std::fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&raw)?)
    }

    // ------------------------------------------------------------------
    // Internals
    // ------------------------------------------------------------------

    /// Stable lock key for a repository path: hash of the canonicalized path,
    /// so all work items targeting the same repo share one lock.
    fn repo_lock_key(&self, repo_path: &Path) -> Result<String> {
        let canonical = std::fs::canonicalize(repo_path).unwrap_or_else(|_| repo_path.to_path_buf());
        let digest = Digest::sha256_hex(canonical.to_string_lossy().as_bytes());
        Ok(digest.as_str()[..16].to_string())
    }

    /// Next fencing generation for a work item: one more than the number of
    /// leases ever acquired (the event log is the source of truth, so this
    /// survives restarts).
    fn next_lease_generation(&self, work_id: &WorkId) -> Result<u64> {        let events = self.store.replay(work_id)?;
        let acquired = events
            .iter()
            .filter(|e| matches!(e.payload, EventPayload::LeaseAcquired { .. }))
            .count() as u64;
        Ok(acquired + 1)
    }

    /// Fencing: the presented lease must be the active lease of the active
    /// attempt. A stale adapter cannot write into a newer attempt.
    fn fence(&self, item: &WorkItem, presented: &ExecutionLease) -> Result<()> {        let active_id = item
            .active_attempt
            .as_ref()
            .ok_or_else(|| ForgeError::InvalidState {
                work_id: item.id.clone(),
                state: item.state,
                action: "mutate attempt (none active)",
            })?;
        if active_id != &presented.attempt_id {
            return Err(ForgeError::AttemptNotFound(presented.attempt_id.clone()));
        }
        let attempt = item
            .attempt(active_id)
            .ok_or_else(|| ForgeError::AttemptNotFound(active_id.clone()))?;
        let active = attempt
            .lease
            .as_ref()
            .ok_or_else(|| ForgeError::InvalidState {
                work_id: item.id.clone(),
                state: item.state,
                action: "mutate attempt (no active lease)",
            })?;
        if active.lease_id != presented.lease_id || active.generation != presented.generation {
            return Err(ForgeError::StaleLease {
                presented: presented.lease_id.clone(),
                presented_generation: presented.generation,
                active: active.lease_id.clone(),
                active_generation: active.generation,
            });
        }
        Ok(())
    }

    pub(crate) fn end_attempt(
        &self,
        item: &mut WorkItem,
        attempt_id: &crate::model::AttemptId,
        state: AttemptState,
        recovery: RecoveryDisposition,
        actor: &ActorRef,
    ) -> Result<()> {
        {
            let attempt = item
                .attempt_mut(attempt_id)
                .ok_or_else(|| ForgeError::AttemptNotFound(attempt_id.clone()))?;
            attempt.state = state;
            attempt.recovery = Some(recovery.clone());
            attempt.ended_at = Some(Utc::now());
            attempt.lease = None;
        }
        item.active_attempt = None;
        self.store.append(
            &item.id,
            actor,
            EventPayload::AttemptEnded {
                attempt_id: attempt_id.clone(),
                state,
                recovery,
            },
        )?;
        Ok(())
    }

    pub(crate) fn transition(
        &self,
        item: &mut WorkItem,
        to: WorkState,
        reason: Option<String>,
        actor: &ActorRef,
    ) -> Result<()> {
        let from = item.state;
        item.state = to;
        item.updated_at = Utc::now();
        self.store.append(
            &item.id,
            actor,
            EventPayload::StateChanged { from, to, reason },
        )?;
        let seq = self.store.replay(&item.id)?.last().map(|e| e.seq).unwrap_or(0);
        self.persist(item, seq)?;
        Ok(())
    }

    fn persist_fresh(&self, item: &mut WorkItem) -> Result<()> {
        item.updated_at = Utc::now();
        let seq = self.store.replay(&item.id)?.last().map(|e| e.seq).unwrap_or(0);
        self.persist(item, seq)
    }

    fn persist(&self, item: &WorkItem, applied_seq: u64) -> Result<()> {
        self.store.write_snapshot(item, applied_seq)
    }
}

pub(crate) fn git_target(item: &WorkItem) -> Result<GitWorkTarget> {
    match &item.target {
        WorkTarget::Git(t) => Ok(t.clone()),
    }
}

fn expect_state(item: &WorkItem, expected: WorkState, action: &'static str) -> Result<()> {
    if item.state != expected {
        return Err(ForgeError::InvalidState {
            work_id: item.id.clone(),
            state: item.state,
            action,
        });
    }
    Ok(())
}

/// Fold events into state. The first event must be `item_registered`; all
/// other payloads update the item deterministically. Operation-journal events
/// are crash-recovery records, not state.
pub fn fold(events: &[TransitionEvent]) -> Result<WorkItem> {
    let mut iter = events.iter();
    let mut item = match iter.next().map(|e| &e.payload) {
        Some(EventPayload::ItemRegistered { item }) => (**item).clone(),
        _ => {
            return Err(ForgeError::Store(
                "event log does not start with item_registered".into(),
            ))
        }
    };
    for event in iter {
        match &event.payload {
            EventPayload::ItemRegistered { .. } => {
                return Err(ForgeError::Store("duplicate item_registered".into()));
            }
            EventPayload::EnvironmentProvisioned { env } => {
                item.environment = Some((**env).clone());
            }
            EventPayload::StateChanged { to, .. } => {
                item.state = *to;
                item.updated_at = event.at;
            }
            EventPayload::AttemptStarted { attempt } => {
                item.active_attempt = Some(attempt.id.clone());
                item.attempts.push((**attempt).clone());
            }
            EventPayload::AttemptEnded {
                attempt_id,
                state,
                recovery,
            } => {
                if let Some(att) = item.attempt_mut(attempt_id) {
                    att.state = *state;
                    att.recovery = Some(recovery.clone());
                    att.ended_at = Some(event.at);
                    att.lease = None;
                }
                if item.active_attempt.as_ref() == Some(attempt_id) {
                    item.active_attempt = None;
                }
            }
            EventPayload::LeaseAcquired {
                attempt_id,
                lease_id,
                generation,
                owner_instance_id,
            } => {
                if let Some(att) = item.attempt_mut(attempt_id)
                    && let Some(lease) = att.lease.as_mut()
                {
                    lease.lease_id = lease_id.clone();
                    lease.generation = *generation;
                    lease.owner_instance_id = owner_instance_id.clone();
                }
            }
            EventPayload::EvidenceSealed {
                attempt_id,
                evidence_id,
                ..
            } => {
                if let Some(att) = item.attempt_mut(attempt_id) {
                    att.evidence_id = Some(evidence_id.clone());
                }
            }
            EventPayload::ReviewDecided { decision } => {
                item.review_decisions.push((**decision).clone());
            }
            EventPayload::DecisionInvalidated { decision_id, .. } => {
                item.review_decisions.retain(|d| &d.id != decision_id);
            }
            EventPayload::DispositionApplied { disposition, .. } => {
                item.disposition = Some(*disposition);
            }
            // Operation journal: crash-recovery records, not state.
            EventPayload::OperationStarted { .. }
            | EventPayload::OperationSideEffect { .. }
            | EventPayload::OperationCommitted { .. }
            | EventPayload::OperationAborted { .. } => {}
        }
    }
    Ok(item)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::CheckpointAuthor;
    use crate::model::{AttemptId, Digest};
    use std::fs;
    use tempfile::TempDir;

    struct Fixture {
        _repo_tmp: TempDir,
        _forge_tmp: TempDir,
        repo: PathBuf,
        forge_root: PathBuf,
        git: GitEngine,
        baseline: crate::model::GitOid,
    }

    fn fixture() -> Fixture {
        let repo_tmp = TempDir::new().unwrap();
        let forge_tmp = TempDir::new().unwrap();
        let git = GitEngine::detect().unwrap();
        git.run(repo_tmp.path(), &["init", "-b", "main", "--template="])
            .or_else(|_| git.run(repo_tmp.path(), &["init", "-b", "main"]))
            .unwrap();
        fs::write(repo_tmp.path().join("app.txt"), "v1\n").unwrap();
        git.run(repo_tmp.path(), &["add", "-A"]).unwrap();
        let baseline = git
            .commit_checkpoint(repo_tmp.path(), "initial", &CheckpointAuthor::default())
            .unwrap();
        Fixture {
            repo: repo_tmp.path().to_path_buf(),
            forge_root: forge_tmp.path().to_path_buf(),
            git,
            baseline,
            _repo_tmp: repo_tmp,
            _forge_tmp: forge_tmp,
        }
    }

    fn actor() -> ActorRef {
        Forge::system_actor()
    }

    fn script_executor() -> ExecutorDescriptor {
        ExecutorDescriptor {
            kind: "script".into(),
            detail: serde_json::json!({"argv": ["sh", "-c", "echo done"]}),
        }
    }

    /// The thin vertical slice, end to end:
    /// create → provision → attempt → complete → checkpoint → evidence →
    /// review → PreserveBranch → reopen → verify.
    #[test]
    fn vertical_slice_full_lifecycle() {
        let fx = fixture();
        let forge = Forge::open(&fx.forge_root).unwrap();

        // Register
        let item = forge
            .register("Fix app", "make app v2", &fx.repo, "main", "user-1", &actor())
            .unwrap();
        assert_eq!(item.state, WorkState::Draft);
        let work_id = item.id.clone();

        // Provision
        let item = forge.provision(&work_id, &actor()).unwrap();
        assert_eq!(item.state, WorkState::Ready);
        let env = item.environment.clone().unwrap();
        assert_eq!(env.generation, 1);
        assert_eq!(env.baseline_oid, fx.baseline);
        assert!(env.worktree.join("app.txt").is_file());
        assert!(fx.git.branch_exists(&fx.repo, &env.branch));

        // Attempt
        let (item, lease) = forge
            .begin_attempt(&work_id, script_executor(), Some(1234), &actor())
            .unwrap();
        assert_eq!(item.state, WorkState::Executing);
        assert_eq!(lease.generation, 1);
        let attempt_id = lease.attempt_id.clone();

        // Executor does its work (Forge is not involved in execution).
        fs::write(env.worktree.join("app.txt"), "v2\n").unwrap();
        fs::write(env.worktree.join("notes.md"), "# notes\n").unwrap();

        // Complete → seal
        let item = forge.complete_attempt(&lease, &SealOptions::default(), &actor()).unwrap();
        assert_eq!(item.state, WorkState::AwaitingReview);
        assert!(item.active_attempt.is_none());
        let attempt = item.attempt(&attempt_id).unwrap();
        assert_eq!(attempt.state, AttemptState::Completed);
        let evidence_id = attempt.evidence_id.clone().unwrap();

        // Checkpoint commit sealed the work.
        let sealed_head = fx.git.head_oid(&env.worktree).unwrap();
        assert_ne!(sealed_head, fx.baseline);
        assert!(fx.git.is_clean(&env.worktree).unwrap());
        let log = fx.git.run(&env.worktree, &["log", "-1", "--format=%s"]).unwrap();
        assert!(log.contains("forge: checkpoint"));

        // Evidence on disk, digests defined.
        let evidence_dir = forge
            .store()
            .item_dir(&work_id)
            .join("attempts/1/evidence");
        let manifest_raw = fs::read_to_string(evidence_dir.join("manifest.json")).unwrap();
        let manifest: EvidenceManifest = serde_json::from_str(&manifest_raw).unwrap();
        assert_eq!(manifest.evidence_id, evidence_id);
        assert_eq!(manifest.baseline_oid, fx.baseline);
        assert_eq!(manifest.sealed_head_oid, sealed_head);
        assert!(!manifest.base_advanced);
        assert!(manifest.bundle_digest.is_some());
        let patch = fs::read(evidence_dir.join("patch.diff")).unwrap();
        assert!(String::from_utf8_lossy(&patch).contains("notes.md"));

        // Review: binds to exact evidence and exact head.
        let decision = ReviewDecision {
            id: ReviewDecisionId::new(),
            actor: ActorRef {
                kind: ActorKind::User,
                id: "user-1".into(),
            },
            attempt_id: attempt_id.clone(),
            environment_generation: 1,
            evidence_id: evidence_id.clone(),
            evidence_digest: manifest.bundle_digest.clone().unwrap(),
            baseline_oid: fx.baseline.clone(),
            reviewed_head_oid: sealed_head.clone(),
            expected_base_oid: fx.baseline.clone(),
            acknowledged_violations: Vec::new(),
            strategy: IntegrationStrategy::PreserveBranch,
            rationale: Some("ship it".into()),
            decided_at: Utc::now(),
        };
        let decision_id = decision.id.clone();
        let item = forge.decide(&work_id, decision, &actor()).unwrap();
        assert_eq!(item.review_decisions.len(), 1);

        // Apply → accepted, base untouched, branch preserved.
        let item = forge.apply_decision(&work_id, &decision_id, &actor()).unwrap();
        assert_eq!(item.state, WorkState::Accepted);
        assert_eq!(item.disposition, Some(AcceptedDisposition::BranchPreserved));
        assert_eq!(
            fx.git.ref_oid(&fx.repo, "main").unwrap(),
            fx.baseline,
            "PreserveBranch must not touch the base ref"
        );
        assert!(fx.git.branch_exists(&fx.repo, &env.branch));

        // Reopen and verify: state survives a restart, work is durable.
        let forge2 = Forge::open(&fx.forge_root).unwrap();
        let reopened = forge2.load(&work_id).unwrap();
        assert_eq!(reopened.state, WorkState::Accepted);
        assert_eq!(reopened.disposition, Some(AcceptedDisposition::BranchPreserved));
        assert!(env.worktree.join("notes.md").is_file());
        let patch = fx
            .git
            .diff_binary(&env.worktree, &fx.baseline, &sealed_head)
            .unwrap();
        let text = String::from_utf8_lossy(&patch);
        assert!(text.contains("app.txt"));
        assert!(text.contains("notes.md"));
    }

    #[test]
    fn stale_lease_is_fenced_out() {
        let fx = fixture();
        let forge = Forge::open(&fx.forge_root).unwrap();
        let item = forge
            .register("t", "b", &fx.repo, "main", "user-1", &actor())
            .unwrap();
        forge.provision(&item.id, &actor()).unwrap();
        let (_item, lease) = forge
            .begin_attempt(&item.id, script_executor(), None, &actor())
            .unwrap();

        // A forged lease with the wrong generation must be rejected.
        let mut stale = lease.clone();
        stale.generation = 99;
        let err = forge.complete_attempt(&stale, &SealOptions::default(), &actor()).unwrap_err();
        assert!(matches!(err, ForgeError::StaleLease { .. }));

        // A lease pointing at a different attempt must be rejected.
        let mut alien = lease.clone();
        alien.attempt_id = AttemptId::new();
        let err = forge.complete_attempt(&alien, &SealOptions::default(), &actor()).unwrap_err();
        assert!(matches!(err, ForgeError::AttemptNotFound(_)));

        // The real lease still works.
        forge.complete_attempt(&lease, &SealOptions::default(), &actor()).unwrap();
    }

    #[test]
    fn fsm_rejects_out_of_order_calls() {
        let fx = fixture();
        let forge = Forge::open(&fx.forge_root).unwrap();
        let item = forge
            .register("t", "b", &fx.repo, "main", "user-1", &actor())
            .unwrap();

        // Cannot begin an attempt from Draft.
        let err = forge
            .begin_attempt(&item.id, script_executor(), None, &actor())
            .unwrap_err();
        assert!(matches!(err, ForgeError::InvalidState { .. }));

        // Cannot re-provision once Ready.
        forge.provision(&item.id, &actor()).unwrap();
        let err = forge.provision(&item.id, &actor()).unwrap_err();
        assert!(matches!(err, ForgeError::InvalidState { .. }));
    }

    #[test]
    fn drift_after_review_invalidates_decision() {
        let fx = fixture();
        let forge = Forge::open(&fx.forge_root).unwrap();
        let item = forge
            .register("t", "b", &fx.repo, "main", "user-1", &actor())
            .unwrap();
        forge.provision(&item.id, &actor()).unwrap();
        let (item, lease) = forge
            .begin_attempt(&item.id, script_executor(), None, &actor())
            .unwrap();
        let env = item.environment.clone().unwrap();
        fs::write(env.worktree.join("a.txt"), "a\n").unwrap();
        let item = forge.complete_attempt(&lease, &SealOptions::default(), &actor()).unwrap();
        let attempt = item.attempt(&lease.attempt_id).unwrap();
        let sealed_head = fx.git.head_oid(&env.worktree).unwrap();
        let manifest: EvidenceManifest = serde_json::from_str(
            &fs::read_to_string(
                forge
                    .store()
                    .item_dir(&item.id)
                    .join("attempts/1/evidence/manifest.json"),
            )
            .unwrap(),
        )
        .unwrap();
        let decision = ReviewDecision {
            id: ReviewDecisionId::new(),
            actor: ActorRef {
                kind: ActorKind::User,
                id: "user-1".into(),
            },
            attempt_id: lease.attempt_id.clone(),
            environment_generation: 1,
            evidence_id: attempt.evidence_id.clone().unwrap(),
            evidence_digest: manifest.bundle_digest.unwrap(),
            baseline_oid: fx.baseline.clone(),
            reviewed_head_oid: sealed_head,
            expected_base_oid: fx.baseline.clone(),
            acknowledged_violations: Vec::new(),
            strategy: IntegrationStrategy::PreserveBranch,
            rationale: None,
            decided_at: Utc::now(),
        };
        let decision_id = decision.id.clone();
        forge.decide(&item.id, decision, &actor()).unwrap();

        // TOCTOU: something moves the branch head after the review.
        fs::write(env.worktree.join("evil.txt"), "x\n").unwrap();
        fx.git
            .commit_checkpoint(&env.worktree, "forge: sneaky", &CheckpointAuthor::default())
            .unwrap();

        let err = forge
            .apply_decision(&item.id, &decision_id, &actor())
            .unwrap_err();
        assert!(matches!(err, ForgeError::EnvironmentDrift(_)));
        // The item stays in AwaitingReview — approval did not leak.
        assert_eq!(forge.load(&item.id).unwrap().state, WorkState::AwaitingReview);
    }

    #[test]
    fn heartbeat_updates_lease_record_without_logging_events() {
        let fx = fixture();
        let forge = Forge::open(&fx.forge_root).unwrap();
        let item = forge
            .register("t", "b", &fx.repo, "main", "user-1", &actor())
            .unwrap();
        forge.provision(&item.id, &actor()).unwrap();
        let (_item, lease) = forge
            .begin_attempt(&item.id, script_executor(), None, &actor())
            .unwrap();
        let events_before = forge.store().replay(&item.id).unwrap().len();

        forge.heartbeat(&lease).unwrap();
        forge.heartbeat(&lease).unwrap();

        // Heartbeats never touch the JSONL log.
        assert_eq!(
            forge.store().replay(&item.id).unwrap().len(),
            events_before
        );
        // But the lease record (snapshot) reflects them.
        let loaded = forge.load(&item.id).unwrap();
        let hb = loaded
            .attempt(&lease.attempt_id)
            .unwrap()
            .lease
            .as_ref()
            .unwrap()
            .heartbeat_at;
        assert!(hb >= lease.heartbeat_at);
    }

    #[test]
    fn append_command_log_is_lease_fenced_and_digestable() {
        let fx = fixture();
        let forge = Forge::open(&fx.forge_root).unwrap();
        let item = forge
            .register("t", "b", &fx.repo, "main", "user-1", &actor())
            .unwrap();
        forge.provision(&item.id, &actor()).unwrap();
        let (item, lease) = forge
            .begin_attempt(&item.id, script_executor(), None, &actor())
            .unwrap();
        let seq = item.attempt(&lease.attempt_id).unwrap().seq;

        forge
            .append_command_log(
                &lease,
                &serde_json::json!({"kind": "prompt", "chars": 12}),
            )
            .unwrap();
        forge
            .append_command_log(
                &lease,
                &serde_json::json!({"kind": "tool", "name": "read"}),
            )
            .unwrap();

        let path = forge
            .store()
            .item_dir(&item.id)
            .join("attempts")
            .join(seq.to_string())
            .join("evidence/commands.jsonl");
        let raw = fs::read_to_string(&path).unwrap();
        let lines: Vec<_> = raw.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("\"prompt\""));
        assert!(lines[1].contains("\"tool\""));

        // Stale lease cannot append.
        let stale = ExecutionLease {
            generation: lease.generation + 99,
            ..lease.clone()
        };
        assert!(forge
            .append_command_log(&stale, &serde_json::json!({"kind": "x"}))
            .is_err());
    }

    #[test]
    fn latest_resume_token_returns_most_recent_resume_supported() {
        let fx = fixture();
        let forge = Forge::open(&fx.forge_root).unwrap();
        let item = forge
            .register("t", "b", &fx.repo, "main", "user-1", &actor())
            .unwrap();
        forge.provision(&item.id, &actor()).unwrap();
        assert!(forge.latest_resume_token(&item.id).unwrap().is_none());

        let (_, lease) = forge
            .begin_attempt(&item.id, script_executor(), None, &actor())
            .unwrap();
        forge
            .interrupt_attempt(
                &lease,
                RecoveryDisposition::ResumeSupported {
                    provider_token: "wire-abc".into(),
                },
                &actor(),
            )
            .unwrap();
        assert_eq!(
            forge.latest_resume_token(&item.id).unwrap().as_deref(),
            Some("wire-abc")
        );

        let (_, lease2) = forge
            .begin_attempt(&item.id, script_executor(), None, &actor())
            .unwrap();
        forge
            .interrupt_attempt(
                &lease2,
                RecoveryDisposition::ResumeSupported {
                    provider_token: "wire-xyz".into(),
                },
                &actor(),
            )
            .unwrap();
        assert_eq!(
            forge.latest_resume_token(&item.id).unwrap().as_deref(),
            Some("wire-xyz")
        );
    }

    #[test]
    fn interrupt_preserves_environment_and_returns_to_ready() {
        let fx = fixture();
        let forge = Forge::open(&fx.forge_root).unwrap();
        let item = forge
            .register("t", "b", &fx.repo, "main", "user-1", &actor())
            .unwrap();
        forge.provision(&item.id, &actor()).unwrap();
        let (item, lease) = forge
            .begin_attempt(&item.id, script_executor(), None, &actor())
            .unwrap();
        let env = item.environment.clone().unwrap();
        // Executor leaves uncommitted work behind.
        fs::write(env.worktree.join("wip.txt"), "work in progress\n").unwrap();

        let item = forge
            .interrupt_attempt(
                &lease,
                RecoveryDisposition::ResumeSupported {
                    provider_token: "sess-abc".into(),
                },
                &actor(),
            )
            .unwrap();
        assert_eq!(item.state, WorkState::Ready);
        assert!(item.active_attempt.is_none());
        let attempt = item.attempt(&lease.attempt_id).unwrap();
        assert_eq!(attempt.state, AttemptState::Interrupted);
        assert_eq!(
            attempt.recovery,
            Some(RecoveryDisposition::ResumeSupported {
                provider_token: "sess-abc".into()
            })
        );
        // Dirty work is preserved, untouched.
        assert_eq!(
            fs::read_to_string(env.worktree.join("wip.txt")).unwrap(),
            "work in progress\n"
        );

        // A replacement executor can pick up the same environment.
        let (item, lease2) = forge
            .begin_attempt(&item.id, script_executor(), None, &actor())
            .unwrap();
        assert_eq!(item.state, WorkState::Executing);
        assert!(
            lease2.generation > lease.generation,
            "fencing tokens must be monotonic across attempts"
        );
        // The old lease is fenced out.
        let err = forge.complete_attempt(&lease, &SealOptions::default(), &actor()).unwrap_err();
        assert!(matches!(
            err,
            ForgeError::StaleLease { .. } | ForgeError::AttemptNotFound(_)
        ));
        forge.complete_attempt(&lease2, &SealOptions::default(), &actor()).unwrap();
    }

    #[test]
    fn fail_attempt_marks_failed_and_returns_to_ready() {
        let fx = fixture();
        let forge = Forge::open(&fx.forge_root).unwrap();
        let item = forge
            .register("t", "b", &fx.repo, "main", "user-1", &actor())
            .unwrap();
        forge.provision(&item.id, &actor()).unwrap();
        let (_item, lease) = forge
            .begin_attempt(&item.id, script_executor(), None, &actor())
            .unwrap();
        let item = forge.fail_attempt(&lease, "adapter exploded", &actor()).unwrap();
        assert_eq!(item.state, WorkState::Ready);
        let attempt = item.attempt(&lease.attempt_id).unwrap();
        assert_eq!(attempt.state, AttemptState::Failed);
        assert_eq!(attempt.recovery, Some(RecoveryDisposition::RestartAllowed));
    }

    /// Helper: drive an item to Executing with a custom policy.
    fn to_executing_with_policy(
        fx: &Fixture,
        forge: &Forge,
        policy: crate::model::WorkPolicy,
    ) -> (WorkItem, ExecutionLease, crate::model::GovernedEnv) {
        let item = forge
            .register_with_policy("t", "b", &fx.repo, "main", "user-1", policy, &actor())
            .unwrap();
        forge.provision(&item.id, &actor()).unwrap();
        let (item, lease) = forge
            .begin_attempt(&item.id, script_executor(), None, &actor())
            .unwrap();
        let env = item.environment.clone().unwrap();
        (item, lease, env)
    }

    #[test]
    fn secret_capture_is_blocked_then_allowed_with_ack() {
        let fx = fixture();
        let forge = Forge::open(&fx.forge_root).unwrap();
        let policy = crate::model::WorkPolicy {
            checkpoint_allow_risky_with_ack: true,
            ..Default::default()
        };
        let (item, lease, env) = to_executing_with_policy(&fx, &forge, policy);
        fs::write(
            env.worktree.join("id_rsa"),
            "-----BEGIN RSA PRIVATE KEY-----\nMII\n-----END RSA PRIVATE KEY-----\n",
        )
        .unwrap();

        // Without acknowledgment: blocked, item stays Executing.
        let err = forge
            .complete_attempt(&lease, &SealOptions::default(), &actor())
            .unwrap_err();
        assert!(matches!(err, ForgeError::CaptureBlocked(_)));
        assert_eq!(forge.load(&item.id).unwrap().state, WorkState::Executing);

        // With acknowledgment: sealed, risk recorded in the policy report.
        let options = SealOptions {
            ack_risks: true,
            author: None,
        };
        let item = forge.complete_attempt(&lease, &options, &actor()).unwrap();
        assert_eq!(item.state, WorkState::AwaitingReview);
        let report: crate::model::PolicyReport = serde_json::from_str(
            &fs::read_to_string(
                forge
                    .store()
                    .item_dir(&item.id)
                    .join("attempts/1/evidence/policy.json"),
            )
            .unwrap(),
        )
        .unwrap();
        assert!(report.capture_risks.iter().any(|r| matches!(
            r,
            CaptureRisk::SecretPattern { path, pattern }
                if path == "id_rsa" && pattern == "rsa_private_key"
        )));
    }

    #[test]
    fn risky_capture_without_policy_escape_hatch_is_hard_blocked() {
        let fx = fixture();
        let forge = Forge::open(&fx.forge_root).unwrap();
        // Default policy: checkpoint_allow_risky_with_ack = false.
        let (item, lease, env) =
            to_executing_with_policy(&fx, &forge, crate::model::WorkPolicy::default());
        fs::write(
            env.worktree.join("key.pem"),
            "-----BEGIN RSA PRIVATE KEY-----\nMII\n-----END RSA PRIVATE KEY-----\n",
        )
        .unwrap();
        let options = SealOptions {
            ack_risks: true, // ack alone is not enough — policy forbids risky capture
            author: None,
        };
        let err = forge
            .complete_attempt(&lease, &options, &actor())
            .unwrap_err();
        assert!(matches!(err, ForgeError::CaptureBlocked(_)));
        assert_eq!(forge.load(&item.id).unwrap().state, WorkState::Executing);
    }

    #[test]
    fn excluded_paths_stay_out_of_checkpoint_and_evidence() {
        let fx = fixture();
        let forge = Forge::open(&fx.forge_root).unwrap();
        let policy = crate::model::WorkPolicy {
            checkpoint_exclude_paths: vec!["logs/**".into()],
            ..Default::default()
        };
        let (_item, lease, env) = to_executing_with_policy(&fx, &forge, policy);
        fs::write(env.worktree.join("keep.txt"), "keep\n").unwrap();
        fs::create_dir_all(env.worktree.join("logs")).unwrap();
        fs::write(env.worktree.join("logs/debug.log"), "noisy\n").unwrap();

        let item = forge
            .complete_attempt(&lease, &SealOptions::default(), &actor())
            .unwrap();
        assert_eq!(item.state, WorkState::AwaitingReview);
        // The excluded file is not committed: it remains untracked in the
        // worktree, and the sealed patch does not contain it.
        let manifest: EvidenceManifest = serde_json::from_str(
            &fs::read_to_string(
                forge
                    .store()
                    .item_dir(&item.id)
                    .join("attempts/1/evidence/manifest.json"),
            )
            .unwrap(),
        )
        .unwrap();
        let paths: Vec<&str> = manifest.changed_files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&"keep.txt"));
        assert!(!paths.iter().any(|p| p.starts_with("logs/")));
        let patch = fs::read(
            forge
                .store()
                .item_dir(&item.id)
                .join("attempts/1/evidence/patch.diff"),
        )
        .unwrap();
        assert!(!String::from_utf8_lossy(&patch).contains("debug.log"));
    }

    #[test]
    fn path_violations_are_recorded_as_evidence_not_blocked() {
        let fx = fixture();
        let forge = Forge::open(&fx.forge_root).unwrap();
        let policy = crate::model::WorkPolicy {
            allowed_paths: vec!["src/**".into()],
            ..Default::default()
        };
        let (_item, lease, env) = to_executing_with_policy(&fx, &forge, policy);
        fs::create_dir_all(env.worktree.join("src")).unwrap();
        fs::write(env.worktree.join("src/ok.rs"), "fn main() {}\n").unwrap();
        fs::write(env.worktree.join("outside.txt"), "not allowed\n").unwrap();

        // Violations are evidence — sealing proceeds and records them.
        let item = forge
            .complete_attempt(&lease, &SealOptions::default(), &actor())
            .unwrap();
        let report: crate::model::PolicyReport = serde_json::from_str(
            &fs::read_to_string(
                forge
                    .store()
                    .item_dir(&item.id)
                    .join("attempts/1/evidence/policy.json"),
            )
            .unwrap(),
        )
        .unwrap();
        assert!(!report.is_clean());
        assert_eq!(report.violations.len(), 1);
        assert_eq!(report.violations[0].path, "outside.txt");
        assert_eq!(report.violations[0].rule, "not_allowed");
        assert_eq!(item.state, WorkState::AwaitingReview);
    }

    /// Drive an item to AwaitingReview with one committed change; returns
    /// (item, env, sealed_head, manifest, decision skeleton).
    fn to_awaiting_review(
        fx: &Fixture,
        forge: &Forge,
    ) -> (WorkItem, crate::model::GovernedEnv, crate::model::GitOid, EvidenceManifest) {
        let item = forge
            .register("t", "b", &fx.repo, "main", "user-1", &actor())
            .unwrap();
        forge.provision(&item.id, &actor()).unwrap();
        let (item, lease) = forge
            .begin_attempt(&item.id, script_executor(), None, &actor())
            .unwrap();
        let env = item.environment.clone().unwrap();
        fs::write(env.worktree.join("feature.txt"), "shipped\n").unwrap();
        let item = forge
            .complete_attempt(&lease, &SealOptions::default(), &actor())
            .unwrap();
        let sealed_head = fx.git.head_oid(&env.worktree).unwrap();
        let manifest: EvidenceManifest = serde_json::from_str(
            &fs::read_to_string(
                forge
                    .store()
                    .item_dir(&item.id)
                    .join("attempts/1/evidence/manifest.json"),
            )
            .unwrap(),
        )
        .unwrap();
        (item, env, sealed_head, manifest)
    }

    fn decision_for(
        item: &WorkItem,
        sealed_head: &crate::model::GitOid,
        manifest: &EvidenceManifest,
        strategy: IntegrationStrategy,
        fx: &Fixture,
    ) -> ReviewDecision {
        ReviewDecision {
            id: ReviewDecisionId::new(),
            actor: ActorRef {
                kind: ActorKind::User,
                id: "user-1".into(),
            },
            attempt_id: item.attempts[0].id.clone(),
            environment_generation: 1,
            evidence_id: manifest.evidence_id.clone(),
            evidence_digest: manifest.bundle_digest.clone().unwrap(),
            baseline_oid: fx.baseline.clone(),
            reviewed_head_oid: sealed_head.clone(),
            expected_base_oid: fx.baseline.clone(),
            acknowledged_violations: Vec::new(),
            strategy,
            rationale: None,
            decided_at: Utc::now(),
        }
    }

    #[test]
    fn fast_forward_advances_base_and_syncs_checked_out_worktree() {
        let fx = fixture();
        let forge = Forge::open(&fx.forge_root).unwrap();
        let (item, _env, sealed_head, manifest) = to_awaiting_review(&fx, &forge);
        let decision = decision_for(
            &item,
            &sealed_head,
            &manifest,
            IntegrationStrategy::FastForwardOnly,
            &fx,
        );
        let decision_id = decision.id.clone();
        forge.decide(&item.id, decision, &actor()).unwrap();
        let item = forge.apply_decision(&item.id, &decision_id, &actor()).unwrap();

        assert_eq!(item.state, WorkState::Accepted);
        assert_eq!(item.disposition, Some(AcceptedDisposition::BaseFastForwarded));
        assert_eq!(fx.git.ref_oid(&fx.repo, "main").unwrap(), sealed_head);
        // Checked-out-base safety: the main checkout was synced to the moved ref.
        assert_eq!(
            fs::read_to_string(fx.repo.join("feature.txt")).unwrap(),
            "shipped\n"
        );
        assert!(fx.git.is_clean(&fx.repo).unwrap());
    }

    #[test]
    fn base_advancing_between_decision_and_apply_is_a_typed_error() {
        let fx = fixture();
        let forge = Forge::open(&fx.forge_root).unwrap();
        let (item, _env, sealed_head, manifest) = to_awaiting_review(&fx, &forge);
        let decision = decision_for(
            &item,
            &sealed_head,
            &manifest,
            IntegrationStrategy::FastForwardOnly,
            &fx,
        );
        let decision_id = decision.id.clone();
        forge.decide(&item.id, decision, &actor()).unwrap();

        // Someone else lands a commit on main after the review.
        fs::write(fx.repo.join("other.txt"), "raced\n").unwrap();
        let raced = fx
            .git
            .commit_checkpoint(&fx.repo, "racing commit", &CheckpointAuthor::default())
            .unwrap();
        assert_ne!(raced, fx.baseline);

        let err = forge
            .apply_decision(&item.id, &decision_id, &actor())
            .unwrap_err();
        assert!(matches!(err, ForgeError::BaseAdvanced { .. }));
        // Approval did not leak; the item is back at review.
        assert_eq!(forge.load(&item.id).unwrap().state, WorkState::AwaitingReview);
        // The forge branch still points at its sealed head.
        let env = forge.load(&item.id).unwrap().environment.unwrap();
        assert_eq!(fx.git.head_oid(&env.worktree).unwrap(), sealed_head);
    }

    #[test]
    fn export_patch_produces_artifact_and_touches_no_refs() {
        let fx = fixture();
        let forge = Forge::open(&fx.forge_root).unwrap();
        let (item, _env, sealed_head, manifest) = to_awaiting_review(&fx, &forge);
        let decision = decision_for(
            &item,
            &sealed_head,
            &manifest,
            IntegrationStrategy::ExportPatch,
            &fx,
        );
        let decision_id = decision.id.clone();
        forge.decide(&item.id, decision, &actor()).unwrap();
        let item = forge.apply_decision(&item.id, &decision_id, &actor()).unwrap();

        assert_eq!(item.disposition, Some(AcceptedDisposition::PatchExported));
        let dispositions = forge.store().item_dir(&item.id).join("dispositions");
        let patch = fs::read_dir(&dispositions)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let content = fs::read_to_string(&patch).unwrap();
        assert!(content.contains("feature.txt"));
        // Base untouched.
        assert_eq!(fx.git.ref_oid(&fx.repo, "main").unwrap(), fx.baseline);
    }

    #[test]
    fn discard_removes_worktree_and_branch() {
        let fx = fixture();
        let forge = Forge::open(&fx.forge_root).unwrap();
        let (item, env, _sealed_head, _manifest) = to_awaiting_review(&fx, &forge);
        let worktree = env.worktree.clone();
        let branch = env.branch.clone();

        let item = forge.discard(&item.id, &actor()).unwrap();
        assert_eq!(item.state, WorkState::Discarded);
        assert!(!worktree.exists());
        assert!(!fx.git.branch_exists(&fx.repo, &branch));
        // Base untouched, terminal state survives reopen.
        assert_eq!(fx.git.ref_oid(&fx.repo, "main").unwrap(), fx.baseline);
        let forge2 = Forge::open(&fx.forge_root).unwrap();
        assert_eq!(forge2.load(&item.id).unwrap().state, WorkState::Discarded);
    }

    #[test]
    fn unacknowledged_violations_block_apply_until_acknowledged() {
        let fx = fixture();
        let forge = Forge::open(&fx.forge_root).unwrap();
        let policy = crate::model::WorkPolicy {
            allowed_paths: vec!["src/**".into()],
            ..Default::default()
        };
        let (_item, lease, env) = to_executing_with_policy(&fx, &forge, policy);
        fs::write(env.worktree.join("rogue.txt"), "outside policy\n").unwrap();
        let item = forge
            .complete_attempt(&lease, &SealOptions::default(), &actor())
            .unwrap();
        let sealed_head = fx.git.head_oid(&env.worktree).unwrap();
        let manifest: EvidenceManifest = serde_json::from_str(
            &fs::read_to_string(
                forge
                    .store()
                    .item_dir(&item.id)
                    .join("attempts/1/evidence/manifest.json"),
            )
            .unwrap(),
        )
        .unwrap();
        let report: crate::model::PolicyReport = serde_json::from_str(
            &fs::read_to_string(
                forge
                    .store()
                    .item_dir(&item.id)
                    .join("attempts/1/evidence/policy.json"),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(report.violations.len(), 1);

        // Decision without acknowledgment → blocked.
        let mut decision = decision_for(
            &item,
            &sealed_head,
            &manifest,
            IntegrationStrategy::PreserveBranch,
            &fx,
        );
        decision.attempt_id = lease.attempt_id.clone();
        let unack_id = decision.id.clone();
        forge.decide(&item.id, decision, &actor()).unwrap();
        let err = forge
            .apply_decision(&item.id, &unack_id, &actor())
            .unwrap_err();
        assert!(matches!(err, ForgeError::PolicyViolation(_)));

        // Decision with acknowledgment → applies.
        let mut acked = decision_for(
            &item,
            &sealed_head,
            &manifest,
            IntegrationStrategy::PreserveBranch,
            &fx,
        );
        acked.attempt_id = lease.attempt_id.clone();
        acked.acknowledged_violations = vec![report.violations[0].id.clone()];
        let acked_id = acked.id.clone();
        forge.decide(&item.id, acked, &actor()).unwrap();
        let item = forge.apply_decision(&item.id, &acked_id, &actor()).unwrap();
        assert_eq!(item.state, WorkState::Accepted);
    }

    #[test]
    fn invalidate_decision_removes_it_from_review() {
        let fx = fixture();
        let forge = Forge::open(&fx.forge_root).unwrap();
        let (item, _env, sealed_head, manifest) = to_awaiting_review(&fx, &forge);
        let decision = decision_for(
            &item,
            &sealed_head,
            &manifest,
            IntegrationStrategy::PreserveBranch,
            &fx,
        );
        let decision_id = decision.id.clone();
        forge.decide(&item.id, decision, &actor()).unwrap();
        let item = forge
            .invalidate_decision(&item.id, &decision_id, "superseded", &actor())
            .unwrap();
        assert!(item.review_decisions.is_empty());
        // Fold agrees after a restart.
        let forge2 = Forge::open(&fx.forge_root).unwrap();
        assert!(forge2.load(&item.id).unwrap().review_decisions.is_empty());
    }

    #[test]
    fn lease_generations_are_monotonic_across_attempts() {
        let fx = fixture();
        let forge = Forge::open(&fx.forge_root).unwrap();
        let item = forge
            .register("t", "b", &fx.repo, "main", "user-1", &actor())
            .unwrap();
        forge.provision(&item.id, &actor()).unwrap();
        let mut generations = Vec::new();
        for _ in 0..3 {
            let (_item, lease) = forge
                .begin_attempt(&item.id, script_executor(), None, &actor())
                .unwrap();
            generations.push(lease.generation);
            forge
                .interrupt_attempt(&lease, RecoveryDisposition::NotResumable, &actor())
                .unwrap();
        }
        assert_eq!(generations, vec![1, 2, 3]);
        // And they survive a restart (derived from the log, not memory).
        let forge2 = Forge::open(&fx.forge_root).unwrap();
        let (_item, lease) = forge2
            .begin_attempt(&item.id, script_executor(), None, &actor())
            .unwrap();
        assert_eq!(lease.generation, 4);
    }

    #[test]
    fn snapshot_cache_is_rebuilt_from_events_when_stale() {
        let fx = fixture();
        let forge = Forge::open(&fx.forge_root).unwrap();
        let item = forge
            .register("t", "b", &fx.repo, "main", "user-1", &actor())
            .unwrap();
        forge.provision(&item.id, &actor()).unwrap();

        // Delete the cache entirely — fold-from-events must rebuild it.
        fs::remove_file(forge.store().snapshot_path(&item.id)).unwrap();
        let loaded = forge.load(&item.id).unwrap();
        assert_eq!(loaded.state, WorkState::Ready);
        assert!(loaded.environment.is_some());
        // And the cache is refreshed on load.
        let envelope = forge.store().read_snapshot(&item.id).unwrap().unwrap();
        assert_eq!(envelope.item.state, WorkState::Ready);
        let _ = Digest::sha256_hex(b"unused-import-guard");
    }
}

