//! The Forge facade: the caller-driven lifecycle API. Executors are
//! replaceable callers of the lease API (`begin_attempt` / `heartbeat` /
//! `complete_attempt` / `interrupt_attempt` / `fail_attempt`); Forge owns
//! environments, evidence, review, dispositions, and recovery — it never runs
//! an executor and never resumes a provider.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;

use crate::catalog::{CatalogPage, ForgeCatalog, SlugReservationJournal};
use crate::compaction;
use crate::error::{ForgeError, Result};
use crate::events::{EventPayload, OperationKind, SideEffect, TransitionEvent};
use crate::execution::ForgeExecutionService;
use crate::git::{CheckpointAuthor, GitEngine};
use crate::model::{
    AcceptedDisposition, ActorKind, ActorRef, Attempt, AttemptId, AttemptState, CaptureRisk,
    ChangeStatus, ChangedFile, ChangesRequested, ChangesRequestedId, CompactEvidenceReceipt,
    CompactEvidenceRetention, Digest, EvidenceId, EvidenceManifest, ExecutionLease,
    ExecutorDescriptor, GitWorkTarget, GovernedEnv, IntegrationStrategy, LeaseId,
    MODEL_SCHEMA_VERSION, OperationId, PolicyReport, PolicyViolation, RawEvidenceDisposition,
    RecoveryDisposition, ReviewComment, ReviewCommentId, ReviewDecision, ReviewDecisionId, WorkId,
    WorkItem, WorkPolicy, WorkState, WorkTarget, anchor_digest_for, compose_revision_brief,
};
use crate::observation::{SharedWatcherFence, WorkspaceObserver};
use crate::owner::ForgeItemRegistry;
use crate::store::FsWorkStore;

const MAX_COMPACT_EVIDENCE_RECEIPTS: usize = 512;
const MAX_COMPACT_EVIDENCE_OBJECT_BYTES: u64 = 8 * 1024 * 1024;

fn compact_evidence_receipts(
    commands: &[u8],
    work_id: &WorkId,
) -> (Vec<CompactEvidenceReceipt>, u64, bool) {
    let mut receipts = Vec::new();
    let mut rejections = 0u64;
    let mut truncated = false;
    for line in String::from_utf8_lossy(commands).lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if value.get("kind").and_then(serde_json::Value::as_str)
            != Some("medousa_coder_ephemeral_evidence_receipt")
        {
            continue;
        }
        if receipts.len() >= MAX_COMPACT_EVIDENCE_RECEIPTS {
            rejections = rejections.saturating_add(1);
            truncated = true;
            continue;
        }
        match parse_compact_evidence_receipt(&value, work_id) {
            Some(receipt) => receipts.push(receipt),
            None => rejections = rejections.saturating_add(1),
        }
    }
    (receipts, rejections, truncated)
}

fn parse_compact_evidence_receipt(
    value: &serde_json::Value,
    expected_work_id: &WorkId,
) -> Option<CompactEvidenceReceipt> {
    let schema_version = value.get("schema_version")?.as_u64()?;
    if schema_version != 1 || value.get("work_id")?.as_str()? != expected_work_id.as_str() {
        return None;
    }
    let source_tool = bounded_receipt_text(value.get("source_tool")?.as_str()?, 128)?;
    let source_call_id = match value.get("source_call_id") {
        None | Some(serde_json::Value::Null) => None,
        Some(value) => Some(bounded_receipt_text(value.as_str()?, 200)?),
    };
    let digest = value.get("digest")?.as_str()?;
    let hex = digest.strip_prefix("sha256:")?;
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return None;
    }
    let reference = value.get("ephemeral_reference")?.as_str()?;
    if reference != format!("coder-evidence:sha256:{hex}") {
        return None;
    }
    let content_type = bounded_receipt_text(value.get("content_type")?.as_str()?, 128)?;
    let logical_bytes = value.get("logical_bytes")?.as_u64()?;
    let physical_bytes = value.get("physical_bytes")?.as_u64()?;
    if logical_bytes > MAX_COMPACT_EVIDENCE_OBJECT_BYTES
        || physical_bytes > MAX_COMPACT_EVIDENCE_OBJECT_BYTES
        || !value.get("redacted")?.as_bool()?
        || value.get("raw_promoted")?.as_bool()?
    {
        return None;
    }
    let retention = match value.get("retention")?.as_str()? {
        "successful_or_reproducible" => CompactEvidenceRetention::SuccessfulOrReproducible,
        "failed_or_non_reproducible" => CompactEvidenceRetention::FailedOrNonReproducible,
        _ => return None,
    };
    Some(CompactEvidenceReceipt {
        schema_version: 1,
        work_id: expected_work_id.as_str().to_owned(),
        source_tool,
        source_call_id,
        digest: digest.to_owned(),
        ephemeral_reference: reference.to_owned(),
        content_type,
        logical_bytes,
        physical_bytes,
        retention,
        expires_at_unix_seconds: value.get("expires_at_unix_seconds")?.as_u64()?,
        redacted: true,
        raw_evidence: RawEvidenceDisposition::EphemeralOnly,
        recorded_at: serde_json::from_value(value.get("recorded_at")?.clone()).ok()?,
    })
}

fn bounded_receipt_text(value: &str, max_chars: usize) -> Option<String> {
    let value = value.trim();
    (!value.is_empty() && value.chars().count() <= max_chars).then(|| value.to_owned())
}

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
    pub(crate) owners: ForgeItemRegistry,
    pub(crate) catalog: ForgeCatalog,
    pub(crate) slugs: SlugReservationJournal,
    pub(crate) observer: WorkspaceObserver,
    pub(crate) watcher_fence: SharedWatcherFence,
    pub(crate) execution: Option<Arc<ForgeExecutionService>>,
}

impl Forge {
    pub fn open(forge_root: impl AsRef<Path>) -> Result<Self> {
        let root = forge_root.as_ref();
        let store = FsWorkStore::open(root)?;
        let git = GitEngine::detect()?;
        let instance_id = format!("boot-{}", LeaseId::new().as_str());
        let canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let store_root = Arc::new(
            medousa_store::StoreRoot::open_or_create_nofollow(&canonical)
                .map_err(|err| ForgeError::Store(err.to_string()))?,
        );
        let slugs = SlugReservationJournal::open(Arc::clone(&store_root))?;
        let catalog = ForgeCatalog::open(store_root)?;
        let forge = Self {
            store,
            git,
            instance_id,
            owners: ForgeItemRegistry::new(),
            catalog,
            slugs,
            observer: WorkspaceObserver::default(),
            watcher_fence: SharedWatcherFence::new(),
            execution: None,
        };
        // Catalog rebuild is an unbounded snapshot scan — callers (daemon boot,
        // list fallback) must admit it through ForgeExecutionService rather than
        // running it inline on a Tokio worker during open.
        Ok(forge)
    }

    pub fn attach_execution(&mut self, execution: Arc<ForgeExecutionService>) {
        self.execution = Some(execution);
    }

    pub fn attach_watcher_fence(&mut self, fence: SharedWatcherFence) {
        self.watcher_fence = fence;
    }

    pub fn execution(&self) -> Option<Arc<ForgeExecutionService>> {
        self.execution.clone()
    }

    pub fn catalog(&self) -> &ForgeCatalog {
        &self.catalog
    }

    pub fn observer(&self) -> &WorkspaceObserver {
        &self.observer
    }

    pub fn watcher_fence(&self) -> &SharedWatcherFence {
        &self.watcher_fence
    }

    /// Rebuild the listing catalog from on-disk snapshots. Must run under
    /// [`ForgeExecutionService`] admission when invoked from async contexts.
    pub fn rebuild_catalog_from_snapshots(&self) {
        let mut rebuilt = Vec::new();
        if let Ok(ids) = self.store.list_item_ids() {
            for id in ids {
                if let Ok(Some(envelope)) = self.store.read_snapshot(&id) {
                    rebuilt.push((envelope.item, envelope.applied_seq));
                }
            }
        }
        let _ = self.catalog.rebuild_from(rebuilt);
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

    /// Authoritative per-item mutation path. Production Forge/reconcile code
    /// must use this instead of `FsWorkStore::append`.
    pub(crate) fn commit_event(
        &self,
        work_id: &WorkId,
        actor: &ActorRef,
        payload: EventPayload,
    ) -> Result<TransitionEvent> {
        let (event, _receipt) =
            crate::owner::append_owned(&self.store, &self.owners, work_id, actor, payload, None)?;
        Ok(event)
    }

    pub(crate) fn commit_event_receipt(
        &self,
        work_id: &WorkId,
        actor: &ActorRef,
        payload: EventPayload,
        expected_item_generation: Option<u64>,
    ) -> Result<(TransitionEvent, crate::owner::ForgeCommitReceipt)> {
        crate::owner::append_owned(
            &self.store,
            &self.owners,
            work_id,
            actor,
            payload,
            expected_item_generation,
        )
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
        let base_oid = self.git.resolve_base_oid(repo_path, &base_ref)?;
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
        let taken = self.slugs.taken_slugs()?;
        item.slug = crate::slug::allocate_unique_slug(&item.slug, taken.iter().map(String::as_str));
        let operation_id = OperationId::new();
        self.slugs.reserve(&item.slug, operation_id.as_str())?;
        let _item_lock = self.store.lock_item(&item.id)?;
        let (event, receipt) = match self.commit_event_receipt(
            &item.id,
            actor,
            EventPayload::ItemRegistered {
                item: Box::new(item.clone()),
            },
            None,
        ) {
            Ok(result) => result,
            Err(err) => {
                let _ = self.slugs.release(&item.slug);
                return Err(err);
            }
        };
        self.persist(&item, event.seq)?;
        if let Err(err) = self.slugs.commit(&item.slug, item.id.clone(), event.seq) {
            // Item is already durable — never release the slug.
            return Err(err);
        }
        self.catalog.publish(&item, &receipt)?;
        Ok(item)
    }

    /// Load an item: snapshot cache when fresh, snapshot+tail when compacted,
    /// or bounded fold-from-events otherwise.
    pub fn load(&self, work_id: &WorkId) -> Result<WorkItem> {
        if !self.store.item_exists(work_id) {
            return Err(ForgeError::WorkNotFound(work_id.clone()));
        }
        let last_seq = self.store.cached_last_seq(work_id)?;
        if last_seq == 0 {
            return Err(ForgeError::WorkNotFound(work_id.clone()));
        }
        if let Some(envelope) = self.store.read_snapshot(work_id)? {
            compaction::validate_snapshot_log_pair(&self.store, work_id, &envelope)?;
            if envelope.item.schema_version == MODEL_SCHEMA_VERSION {
                if envelope.applied_seq == last_seq {
                    return Ok(envelope.item);
                }
                if envelope.applied_seq < last_seq
                    && (envelope.next_log_offset.is_some() || envelope.anchor_hash.is_some())
                {
                    let mut item = envelope.item;
                    let tail =
                        compaction::replay_after(&self.store, work_id, envelope.applied_seq)?;
                    for event in &tail {
                        apply_payload(&mut item, event)?;
                    }
                    return Ok(item);
                }
            }
        }
        let events = compaction::replay_bounded(&self.store, work_id)?;
        if events.is_empty() {
            return Err(ForgeError::WorkNotFound(work_id.clone()));
        }
        let item = fold(&events)?;
        self.persist(&item, last_seq)?;
        Ok(item)
    }

    /// Load the immutable evidence manifest for one exact sealed attempt.
    ///
    /// Callers receive only the canonical manifest projection; live sibling
    /// worktrees and mutable attempt state are never inspected by this API.
    pub fn evidence_manifest_for_attempt(
        &self,
        work_id: &WorkId,
        attempt_id: &AttemptId,
    ) -> Result<EvidenceManifest> {
        let item = self.load(work_id)?;
        self.read_attempt_evidence_manifest(&item, attempt_id)
    }

    pub fn list(&self) -> Result<Vec<WorkItem>> {
        let entries = self.catalog.all_entries()?;
        if !entries.is_empty() {
            return self.load_catalog_entries(&entries);
        }
        self.rebuild_catalog_from_snapshots();
        let entries = self.catalog.all_entries()?;
        if !entries.is_empty() {
            return self.load_catalog_entries(&entries);
        }
        let mut items = Vec::new();
        for id in self.store.list_item_ids()? {
            items.push(self.load(&id)?);
        }
        Ok(items)
    }

    pub fn list_page(&self, limit: Option<usize>, cursor: Option<&str>) -> Result<CatalogPage> {
        if self.catalog.all_entries()?.is_empty() {
            self.rebuild_catalog_from_snapshots();
        }
        self.catalog.page(limit, cursor)
    }

    fn load_catalog_entries(
        &self,
        entries: &[crate::catalog::CatalogEntry],
    ) -> Result<Vec<WorkItem>> {
        let mut items = Vec::new();
        for entry in entries {
            match self.load(&entry.work_id) {
                Ok(item) => items.push(item),
                Err(err) => {
                    return Err(ForgeError::CatalogStale(format!(
                        "catalog entry {} unloadable: {err}",
                        entry.work_id.as_str()
                    )));
                }
            }
        }
        Ok(items)
    }

    pub fn find_lease(&self, lease_id: &LeaseId, generation: u64) -> Result<ExecutionLease> {
        // Complete scan — never limited to the unparameterized list cap.
        let ids = self.store.list_item_ids()?;
        for id in ids {
            let item = self.load(&id)?;
            for active_id in item.active_attempt_ids() {
                let Some(attempt) = item.attempt(active_id) else {
                    continue;
                };
                let Some(lease) = &attempt.lease else {
                    continue;
                };
                if &lease.lease_id == lease_id {
                    if lease.generation != generation {
                        return Err(ForgeError::StaleLease {
                            presented: lease_id.clone(),
                            presented_generation: generation,
                            active: lease.lease_id.clone(),
                            active_generation: lease.generation,
                        });
                    }
                    return Ok(lease.clone());
                }
            }
        }
        Err(ForgeError::Store(format!(
            "active lease not found: {}",
            lease_id.as_str()
        )))
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
        self.commit_event(
            work_id,
            actor,
            EventPayload::OperationStarted {
                operation_id: operation_id.clone(),
                kind: OperationKind::Provision,
                attempt_id: None,
            },
        )?;

        match self.provision_inner(&mut item, &target, &operation_id, actor) {
            Ok(()) => {
                self.commit_event(
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
                let _ = self.commit_event(
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
        let baseline_oid = self
            .git
            .resolve_base_oid(&target.repo_path, &target.base_ref)?;
        let generation = item
            .environment
            .as_ref()
            .map(|e| e.generation + 1)
            .unwrap_or(1);
        let slug = item_slug(item);
        let worktree = self.worktree_path(&repo.repo_id, &slug, generation);
        let branch = crate::slug::staging_branch(&slug, generation);
        self.git
            .worktree_add(&target.repo_path, &worktree, &branch, &baseline_oid)?;
        self.commit_event(
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
            derived_from: None,
        };
        item.environment = Some(env.clone());
        self.commit_event(
            &item.id,
            actor,
            EventPayload::EnvironmentProvisioned { env: Box::new(env) },
        )?;
        Ok(())
    }

    fn worktree_path(
        &self,
        repo_id: &crate::model::RepoId,
        slug: &str,
        generation: u32,
    ) -> PathBuf {
        let digest = Digest::sha256_hex(repo_id.as_str().as_bytes());
        let repo_short = &digest.as_str()[..12];
        self.store
            .root()
            .join("worktrees")
            .join(repo_short)
            .join(crate::slug::staging_worktree_leaf(slug, generation))
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
        self.begin_attempt_inner(work_id, executor, pid, actor, false, None)
    }

    /// Begin an attempt with a private worktree that preserves the staging
    /// worktree's current dirty state. Integrations migrate to this entry point
    /// in Slice 5C.
    pub fn begin_isolated_attempt(
        &self,
        work_id: &WorkId,
        executor: ExecutorDescriptor,
        pid: Option<u32>,
        actor: &ActorRef,
    ) -> Result<(WorkItem, ExecutionLease)> {
        self.begin_attempt_inner(work_id, executor, pid, actor, true, None)
    }

    /// Resume work from one exact preserved attempt environment. Used when a
    /// reviewer selects an older concurrent candidate for another pass.
    pub fn begin_isolated_attempt_from(
        &self,
        work_id: &WorkId,
        source_attempt_id: &crate::model::AttemptId,
        executor: ExecutorDescriptor,
        pid: Option<u32>,
        actor: &ActorRef,
    ) -> Result<(WorkItem, ExecutionLease)> {
        self.begin_attempt_inner(work_id, executor, pid, actor, true, Some(source_attempt_id))
    }

    fn begin_attempt_inner(
        &self,
        work_id: &WorkId,
        executor: ExecutorDescriptor,
        pid: Option<u32>,
        actor: &ActorRef,
        isolated: bool,
        source_attempt_id: Option<&crate::model::AttemptId>,
    ) -> Result<(WorkItem, ExecutionLease)> {
        let probe = self.load(work_id)?;
        let target = git_target(&probe)?;
        let _repo_lock = self
            .store
            .lock_repo(&self.repo_lock_key(&target.repo_path)?)?;
        let _item_lock = self.store.lock_item(work_id)?;
        let mut item = self.load(work_id)?;
        if !matches!(item.state, WorkState::Ready | WorkState::Executing) {
            return Err(ForgeError::InvalidState {
                work_id: work_id.clone(),
                state: item.state,
                action: "begin attempt",
            });
        }
        let attempt_id = crate::model::AttemptId::new();
        let attempt_seq = item.next_attempt_seq();
        let attempt_environment = if isolated {
            Some(self.create_attempt_environment(
                &item,
                &target,
                &attempt_id,
                attempt_seq,
                source_attempt_id,
            )?)
        } else {
            None
        };
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
            seq: attempt_seq,
            executor,
            state: AttemptState::Running,
            environment: attempt_environment,
            lease: Some(lease.clone()),
            recovery: None,
            evidence_id: None,
            started_at: Utc::now(),
            ended_at: None,
        };
        item.activate_attempt(attempt_id.clone());
        item.attempts.push(attempt.clone());
        self.commit_event(
            work_id,
            actor,
            EventPayload::AttemptStarted {
                attempt: Box::new(attempt),
            },
        )?;
        self.commit_event(
            work_id,
            actor,
            EventPayload::LeaseAcquired {
                attempt_id,
                lease_id: lease.lease_id.clone(),
                generation: lease.generation,
                owner_instance_id: lease.owner_instance_id.clone(),
            },
        )?;
        if item.state == WorkState::Executing {
            self.persist_fresh(&mut item)?;
        } else {
            self.transition(&mut item, WorkState::Executing, None, actor)?;
        }
        Ok((item, lease))
    }

    fn create_attempt_environment(
        &self,
        item: &WorkItem,
        target: &GitWorkTarget,
        _attempt_id: &crate::model::AttemptId,
        attempt_seq: u32,
        source_attempt_id: Option<&crate::model::AttemptId>,
    ) -> Result<GovernedEnv> {
        let preserved = if item.has_active_attempts() {
            None
        } else if let Some(source_attempt_id) = source_attempt_id {
            Some(
                item.attempt(source_attempt_id)
                    .ok_or_else(|| ForgeError::AttemptNotFound(source_attempt_id.clone()))?
                    .environment
                    .as_ref()
                    .ok_or_else(|| {
                        ForgeError::EnvironmentDrift(format!(
                            "attempt {source_attempt_id} has no preserved environment"
                        ))
                    })?,
            )
        } else {
            item.latest_attempt_environment()
        };
        if let Some(environment) = preserved {
            if !environment.worktree.exists() {
                return Err(ForgeError::EnvironmentDrift(format!(
                    "preserved attempt worktree is missing: {}",
                    environment.worktree.display()
                )));
            }
            let expected_root = std::fs::canonicalize(&environment.worktree)?;
            let actual_root =
                std::fs::canonicalize(self.git.worktree_root(&environment.worktree)?)?;
            if actual_root != expected_root {
                return Err(ForgeError::EnvironmentDrift(format!(
                    "preserved attempt worktree root changed: expected {}, found {}",
                    expected_root.display(),
                    actual_root.display()
                )));
            }
            let branch = self.git.current_branch(&environment.worktree)?;
            if branch.as_deref() != Some(environment.branch.as_str()) {
                return Err(ForgeError::EnvironmentDrift(format!(
                    "preserved attempt branch changed: expected {}, found {}",
                    environment.branch,
                    branch.as_deref().unwrap_or("detached HEAD")
                )));
            }
            return Ok(environment.clone());
        }
        let staging = item
            .environment
            .as_ref()
            .ok_or_else(|| ForgeError::EnvironmentDrift("no staging environment".into()))?;
        let digest = Digest::sha256_hex(staging.repo.repo_id.as_str().as_bytes());
        let repo_short = &digest.as_str()[..12];
        let slug = item_slug(item);
        let worktree = self
            .store
            .root()
            .join("worktrees")
            .join(repo_short)
            .join(crate::slug::attempt_worktree_leaf(&slug, attempt_seq));
        let branch = crate::slug::attempt_branch(&slug, attempt_seq);
        // Capture the inheritance boundary before Git starts cloning the
        // source state. Parent memory written during the fork must not leak
        // into the child's immutable snapshot.
        let forked_at = Utc::now();
        self.git.worktree_add_from_worktree(
            &target.repo_path,
            &staging.worktree,
            &worktree,
            &branch,
        )?;
        Ok(GovernedEnv {
            kind: crate::model::EnvironmentKind::GitWorktree,
            repo: staging.repo.clone(),
            worktree,
            branch,
            baseline_oid: staging.baseline_oid.clone(),
            generation: staging.generation,
            derived_from: Some(crate::model::EnvironmentLineage {
                branch: staging.branch.clone(),
                generation: staging.generation,
                forked_at,
            }),
        })
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
            if let Some(RecoveryDisposition::ResumeSupported { provider_token }) = &attempt.recovery
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
        self.fence(&item, lease)?;
        let attempt_id = lease.attempt_id.clone();
        self.end_attempt(
            &mut item,
            &attempt_id,
            AttemptState::Interrupted,
            recovery,
            actor,
        )?;
        self.transition_to_attempt_state(&mut item, None, actor)?;
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
        self.fence(&item, lease)?;
        let attempt_id = lease.attempt_id.clone();
        self.end_attempt(
            &mut item,
            &attempt_id,
            AttemptState::Failed,
            RecoveryDisposition::RestartAllowed,
            actor,
        )?;
        self.transition_to_attempt_state(
            &mut item,
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
        self.fence(&item, lease)?;

        let operation_id = OperationId::new();
        self.commit_event(
            &lease.work_id,
            actor,
            EventPayload::OperationStarted {
                operation_id: operation_id.clone(),
                kind: OperationKind::Seal,
                attempt_id: Some(lease.attempt_id.clone()),
            },
        )?;

        match self.seal_inner(&mut item, lease, options, &operation_id, actor) {
            Ok(()) => {
                self.end_attempt(
                    &mut item,
                    &lease.attempt_id,
                    AttemptState::Completed,
                    RecoveryDisposition::NotResumable,
                    actor,
                )?;
                let resulting_state = item.state_after_attempts();
                self.commit_event(
                    &lease.work_id,
                    actor,
                    EventPayload::OperationCommitted {
                        operation_id,
                        resulting_state,
                    },
                )?;
                self.transition_to_attempt_state(&mut item, None, actor)?;
                Ok(item)
            }
            Err(err) => {
                let _ = self.commit_event(
                    &lease.work_id,
                    actor,
                    EventPayload::OperationAborted {
                        operation_id,
                        reason: err.to_string(),
                    },
                );
                let _ = self.persist_fresh(&mut item);
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
            .environment_for_attempt(&lease.attempt_id)
            .cloned()
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
        self.commit_event(
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
        self.commit_event(
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
                crate::git::PorcelainKind::Unmerged => ChangeStatus::Unmerged,
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
        let (compact_receipts, compact_receipt_rejections, receipts_truncated) =
            compact_evidence_receipts(&commands, &item.id);
        let compact_receipts_json = serde_json::to_vec_pretty(&compact_receipts)?;
        std::fs::write(evidence_dir.join("receipts.json"), &compact_receipts_json)?;

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
        let name_status =
            self.git
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
            compact_receipts_digest: Some(Digest::sha256_hex(&compact_receipts_json)),
            compact_receipt_count: u64::try_from(compact_receipts.len()).unwrap_or(u64::MAX),
            compact_receipt_rejections,
            changed_files,
            submodule_state,
            truncated: receipts_truncated,
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
        self.commit_event(
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
        self.commit_event(
            work_id,
            actor,
            EventPayload::OperationStarted {
                operation_id: operation_id.clone(),
                kind: OperationKind::Integrate,
                attempt_id: None,
            },
        )?;

        let (disposition, detail) = match decision.strategy {
            IntegrationStrategy::PreserveBranch => (AcceptedDisposition::BranchPreserved, None),
            IntegrationStrategy::FastForwardOnly => {
                match self.integrate_fast_forward(&item, &decision, &operation_id, actor) {
                    Ok(detail) => (AcceptedDisposition::BaseFastForwarded, Some(detail)),
                    Err(err) => {
                        let _ = self.commit_event(
                            work_id,
                            actor,
                            EventPayload::OperationAborted {
                                operation_id,
                                reason: err.to_string(),
                            },
                        );
                        // Base advanced or conflict: back to review, approval
                        // did not leak.
                        let _ = self.transition(&mut item, WorkState::AwaitingReview, None, actor);
                        return Err(err);
                    }
                }
            }
            IntegrationStrategy::ExportPatch => {
                match self.integrate_export_patch(&item, &decision, &operation_id, actor) {
                    Ok(detail) => (AcceptedDisposition::PatchExported, Some(detail)),
                    Err(err) => {
                        let _ = self.commit_event(
                            work_id,
                            actor,
                            EventPayload::OperationAborted {
                                operation_id,
                                reason: err.to_string(),
                            },
                        );
                        let _ = self.transition(&mut item, WorkState::AwaitingReview, None, actor);
                        return Err(err);
                    }
                }
            }
        };

        item.disposition = Some(disposition);
        self.commit_event(
            work_id,
            actor,
            EventPayload::DispositionApplied {
                disposition,
                detail,
            },
        )?;
        self.commit_event(
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
            .environment_for_attempt(&decision.attempt_id)
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
        if !self.git.is_ancestor(
            &target.repo_path,
            &current_base,
            &decision.reviewed_head_oid,
        )? {
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
        self.commit_event(
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
            ref_name, decision.expected_base_oid, decision.reviewed_head_oid
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
            .environment_for_attempt(&decision.attempt_id)
            .ok_or_else(|| ForgeError::EnvironmentDrift("no environment".into()))?;
        let patch = self.git.diff_binary(
            &env.worktree,
            &decision.baseline_oid,
            &decision.reviewed_head_oid,
        )?;
        let dir = self.store.item_dir(&item.id).join("dispositions");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("patch-{}.diff", decision.id.storage_key()));
        std::fs::write(&path, &patch)?;
        let digest = Digest::sha256_hex(&patch);
        self.commit_event(
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

    /// Guarded discard: release any running attempts, remove worktrees/branches,
    /// then mark Discarded. Allowed from Draft, Ready, Executing, or AwaitingReview.
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
            WorkState::Draft
            | WorkState::Ready
            | WorkState::Executing
            | WorkState::AwaitingReview => {}
            state => {
                return Err(ForgeError::InvalidState {
                    work_id: work_id.clone(),
                    state,
                    action: "discard",
                });
            }
        }

        // Owner-initiated teardown: interrupt active attempts before deleting
        // worktrees so leases cannot keep Executing after discard.
        let active_ids: Vec<_> = item.active_attempt_ids().into_iter().cloned().collect();
        for attempt_id in &active_ids {
            if item.attempt(attempt_id).is_none() {
                continue;
            }
            self.end_attempt(
                &mut item,
                attempt_id,
                AttemptState::Interrupted,
                RecoveryDisposition::NotResumable,
                actor,
            )?;
        }
        if !active_ids.is_empty() {
            // Persist attempt endings without requiring Ready first — discard
            // continues below into Discarded.
            self.persist_fresh(&mut item)?;
        }

        let operation_id = OperationId::new();
        self.commit_event(
            work_id,
            actor,
            EventPayload::OperationStarted {
                operation_id: operation_id.clone(),
                kind: OperationKind::Discard,
                attempt_id: None,
            },
        )?;
        let target = git_target(&item)?;
        let mut environments: Vec<GovernedEnv> = item
            .attempts
            .iter()
            .filter_map(|attempt| attempt.environment.clone())
            .collect();
        if let Some(environment) = item.environment.clone() {
            environments.push(environment);
        }
        environments.sort_by(|left, right| left.worktree.cmp(&right.worktree));
        environments.dedup_by(|left, right| left.worktree == right.worktree);
        for env in environments {
            if env.worktree.exists() {
                self.git.worktree_remove(&target.repo_path, &env.worktree)?;
            }
            self.commit_event(
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
            self.commit_event(
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
        self.commit_event(
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
        self.commit_event(
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

    /// Return reviewed work to Ready so the user can make a follow-up change.
    /// The sealed commit and evidence remain immutable and therefore provide a
    /// recovery point for anything changed during the new attempt.
    pub fn reopen_for_changes(
        &self,
        work_id: &WorkId,
        reason: &str,
        actor: &ActorRef,
    ) -> Result<WorkItem> {
        let _item_lock = self.store.lock_item(work_id)?;
        self.reopen_for_changes_locked(work_id, reason, actor)
    }

    fn reopen_for_changes_locked(
        &self,
        work_id: &WorkId,
        reason: &str,
        actor: &ActorRef,
    ) -> Result<WorkItem> {
        let mut item = self.load(work_id)?;
        expect_state(&item, WorkState::AwaitingReview, "request review changes")?;
        for decision in std::mem::take(&mut item.review_decisions) {
            self.commit_event(
                work_id,
                actor,
                EventPayload::DecisionInvalidated {
                    decision_id: decision.id,
                    reason: reason.to_string(),
                },
            )?;
        }
        self.transition(&mut item, WorkState::Ready, Some(reason.to_string()), actor)?;
        Ok(item)
    }

    /// Add a line-anchored review comment while the item awaits review.
    #[allow(clippy::too_many_arguments)] // comment placement fields travel together
    pub fn add_review_comment(
        &self,
        work_id: &WorkId,
        evidence_id: EvidenceId,
        attempt_id: Option<AttemptId>,
        path: impl Into<String>,
        side: impl Into<String>,
        start_line: u32,
        end_line: u32,
        anchor_text: Option<String>,
        body: impl Into<String>,
        parent_id: Option<ReviewCommentId>,
        actor: &ActorRef,
    ) -> Result<WorkItem> {
        let _item_lock = self.store.lock_item(work_id)?;
        let mut item = self.load(work_id)?;
        expect_state(&item, WorkState::AwaitingReview, "add review comment")?;

        let path = path.into();
        let side = side.into();
        let body = body.into();
        if path.trim().is_empty() {
            return Err(ForgeError::Store("review comment path is required".into()));
        }
        if side != "new" && side != "old" {
            return Err(ForgeError::Store(
                "review comment side must be \"new\" or \"old\"".into(),
            ));
        }
        if start_line == 0 || end_line == 0 || end_line < start_line {
            return Err(ForgeError::Store(
                "review comment line range is invalid".into(),
            ));
        }
        if body.trim().is_empty() {
            return Err(ForgeError::Store("review comment body is required".into()));
        }

        let attempt = match attempt_id {
            Some(id) => item.attempt(&id).ok_or(ForgeError::AttemptNotFound(id))?,
            None => item
                .attempts
                .iter()
                .rev()
                .find(|a| a.evidence_id.as_ref() == Some(&evidence_id))
                .ok_or_else(|| {
                    ForgeError::Store("evidence_id does not match a sealed attempt".into())
                })?,
        };
        if attempt.evidence_id.as_ref() != Some(&evidence_id) {
            return Err(ForgeError::Store(
                "attempt does not own the given evidence_id".into(),
            ));
        }
        let attempt_id = attempt.id.clone();

        let id = ReviewCommentId::new();
        let (thread_id, parent_id) = if let Some(parent_id) = parent_id {
            let parent = item
                .review_comments
                .iter()
                .find(|c| c.id == parent_id)
                .ok_or_else(|| {
                    ForgeError::Store(format!("parent comment {parent_id} not found"))
                })?;
            if parent.evidence_id != evidence_id {
                return Err(ForgeError::Store(
                    "parent comment belongs to different evidence".into(),
                ));
            }
            (parent.thread_id.clone(), Some(parent_id))
        } else {
            (id.clone(), None)
        };

        let anchor_digest = anchor_text
            .as_deref()
            .map(anchor_digest_for)
            .unwrap_or_default();
        let comment = ReviewComment {
            id,
            thread_id,
            parent_id,
            evidence_id,
            attempt_id,
            path,
            side,
            start_line,
            end_line,
            anchor_digest,
            anchor_text,
            body,
            actor: actor.clone(),
            created_at: Utc::now(),
            resolved_at: None,
            resolved_by: None,
        };
        item.review_comments.push(comment.clone());
        self.commit_event(
            &item.id,
            actor,
            EventPayload::ReviewCommentAdded {
                comment: Box::new(comment),
            },
        )?;
        self.persist_fresh(&mut item)?;
        Ok(item)
    }

    /// Mark a review comment resolved.
    pub fn resolve_review_comment(
        &self,
        work_id: &WorkId,
        comment_id: &ReviewCommentId,
        actor: &ActorRef,
    ) -> Result<WorkItem> {
        let _item_lock = self.store.lock_item(work_id)?;
        let mut item = self.load(work_id)?;
        let comment = item
            .review_comments
            .iter_mut()
            .find(|c| &c.id == comment_id)
            .ok_or_else(|| ForgeError::Store(format!("review comment {comment_id} not found")))?;
        let resolved_at = Utc::now();
        comment.resolved_at = Some(resolved_at);
        comment.resolved_by = Some(actor.clone());
        self.commit_event(
            &item.id,
            actor,
            EventPayload::ReviewCommentResolved {
                comment_id: comment_id.clone(),
                resolved_by: actor.clone(),
                resolved_at,
            },
        )?;
        self.persist_fresh(&mut item)?;
        Ok(item)
    }

    /// Update a review comment body (re-appends as ReviewCommentAdded upsert).
    pub fn update_review_comment_body(
        &self,
        work_id: &WorkId,
        comment_id: &ReviewCommentId,
        body: impl Into<String>,
        actor: &ActorRef,
    ) -> Result<WorkItem> {
        let _item_lock = self.store.lock_item(work_id)?;
        let mut item = self.load(work_id)?;
        let body = body.into();
        if body.trim().is_empty() {
            return Err(ForgeError::Store("review comment body is required".into()));
        }
        let comment = item
            .review_comments
            .iter_mut()
            .find(|c| &c.id == comment_id)
            .ok_or_else(|| ForgeError::Store(format!("review comment {comment_id} not found")))?;
        comment.body = body;
        let updated = comment.clone();
        self.commit_event(
            &item.id,
            actor,
            EventPayload::ReviewCommentAdded {
                comment: Box::new(updated),
            },
        )?;
        self.persist_fresh(&mut item)?;
        Ok(item)
    }

    /// Delete a review comment.
    pub fn delete_review_comment(
        &self,
        work_id: &WorkId,
        comment_id: &ReviewCommentId,
        actor: &ActorRef,
    ) -> Result<WorkItem> {
        let _item_lock = self.store.lock_item(work_id)?;
        let mut item = self.load(work_id)?;
        let before = item.review_comments.len();
        item.review_comments.retain(|c| &c.id != comment_id);
        if item.review_comments.len() == before {
            return Err(ForgeError::Store(format!(
                "review comment {comment_id} not found"
            )));
        }
        self.commit_event(
            &item.id,
            actor,
            EventPayload::ReviewCommentDeleted {
                comment_id: comment_id.clone(),
            },
        )?;
        self.persist_fresh(&mut item)?;
        Ok(item)
    }

    /// Record that changes were requested, then reopen the item to Ready.
    pub fn request_changes(
        &self,
        work_id: &WorkId,
        evidence_id: EvidenceId,
        evidence_digest: Digest,
        summary: Option<String>,
        comment_ids: Option<Vec<ReviewCommentId>>,
        actor: &ActorRef,
    ) -> Result<WorkItem> {
        let _item_lock = self.store.lock_item(work_id)?;
        let mut item = self.load(work_id)?;
        expect_state(&item, WorkState::AwaitingReview, "request changes")?;

        let attempt = item
            .attempts
            .iter()
            .rev()
            .find(|a| a.evidence_id.as_ref() == Some(&evidence_id))
            .ok_or_else(|| {
                ForgeError::Store("evidence_id does not match a sealed attempt".into())
            })?;
        let attempt_id = attempt.id.clone();

        if let Some(ids) = comment_ids.as_ref() {
            for id in ids {
                let found = item
                    .review_comments
                    .iter()
                    .any(|c| &c.id == id && c.evidence_id == evidence_id);
                if !found {
                    return Err(ForgeError::Store(format!(
                        "review comment {id} not found for evidence"
                    )));
                }
            }
        }

        let unresolved: Vec<&ReviewComment> = item
            .review_comments
            .iter()
            .filter(|c| c.evidence_id == evidence_id && c.resolved_at.is_none())
            .collect();
        let revision_brief = compose_revision_brief(unresolved, summary.as_deref());
        let selected_ids = comment_ids.unwrap_or_else(|| {
            item.review_comments
                .iter()
                .filter(|c| c.evidence_id == evidence_id && c.resolved_at.is_none())
                .map(|c| c.id.clone())
                .collect()
        });

        let request = ChangesRequested {
            id: ChangesRequestedId::new(),
            actor: actor.clone(),
            attempt_id,
            evidence_id,
            evidence_digest,
            comment_ids: selected_ids,
            summary,
            revision_brief: revision_brief.clone(),
            decided_at: Utc::now(),
        };
        item.changes_requested.push(request.clone());
        self.commit_event(
            &item.id,
            actor,
            EventPayload::ChangesRequested {
                request: Box::new(request),
            },
        )?;
        self.persist_fresh(&mut item)?;

        let reason = if revision_brief.trim().is_empty() {
            "Changes requested".to_owned()
        } else {
            revision_brief
        };
        self.reopen_for_changes_locked(work_id, &reason, actor)
    }

    /// Evidence-bound re-verification — the authorization boundary. Approval
    /// authorizes exactly one sealed state, never "whatever is there now".
    fn verify_decision(&self, item: &WorkItem, decision: &ReviewDecision) -> Result<()> {
        if item.has_active_attempts() {
            return Err(ForgeError::DecisionInvalid {
                reason: "an attempt is still active".into(),
            });
        }
        let env = item
            .environment_for_attempt(&decision.attempt_id)
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
        let stored = manifest
            .bundle_digest
            .clone()
            .ok_or_else(|| ForgeError::DecisionInvalid {
                reason: "evidence manifest has no digest".into(),
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
        let report: PolicyReport = serde_json::from_str(&std::fs::read_to_string(&policy_path)?)?;
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
        self.read_attempt_evidence_manifest(item, &decision.attempt_id)
    }

    fn read_attempt_evidence_manifest(
        &self,
        item: &WorkItem,
        attempt_id: &AttemptId,
    ) -> Result<EvidenceManifest> {
        let attempt = item
            .attempt(attempt_id)
            .ok_or_else(|| ForgeError::AttemptNotFound(attempt_id.clone()))?;
        let evidence_id = attempt.evidence_id.as_ref().ok_or_else(|| {
            ForgeError::Store(format!("attempt {attempt_id} has no sealed evidence"))
        })?;
        let path = self
            .store
            .item_dir(&item.id)
            .join("attempts")
            .join(attempt.seq.to_string())
            .join("evidence")
            .join("manifest.json");
        let raw = std::fs::read_to_string(&path)?;
        let manifest: EvidenceManifest = serde_json::from_str(&raw)?;
        if &manifest.attempt_id != attempt_id || &manifest.evidence_id != evidence_id {
            return Err(ForgeError::Store(format!(
                "sealed evidence identity does not match attempt {attempt_id}"
            )));
        }
        if manifest.bundle_digest.is_none() {
            return Err(ForgeError::Store(format!(
                "sealed evidence for attempt {attempt_id} has no bundle digest"
            )));
        }
        Ok(manifest)
    }

    // ------------------------------------------------------------------
    // Internals
    // ------------------------------------------------------------------

    /// Stable lock key for a repository path: hash of the canonicalized path,
    /// so all work items targeting the same repo share one lock.
    fn repo_lock_key(&self, repo_path: &Path) -> Result<String> {
        let canonical =
            std::fs::canonicalize(repo_path).unwrap_or_else(|_| repo_path.to_path_buf());
        let digest = Digest::sha256_hex(canonical.to_string_lossy().as_bytes());
        Ok(digest.as_str()[..16].to_string())
    }

    /// Next fencing generation for a work item: one more than the number of
    /// leases ever acquired (the event log is the source of truth, so this
    /// survives restarts).
    fn next_lease_generation(&self, work_id: &WorkId) -> Result<u64> {
        Ok(self.store.cached_tail(work_id)?.lease_acquisitions + 1)
    }

    /// Fencing: the presented lease must be the active lease of its addressed
    /// attempt. A stale adapter cannot write into a newer lease or a peer.
    fn fence(&self, item: &WorkItem, presented: &ExecutionLease) -> Result<()> {
        if !item.active_attempt_ids().contains(&&presented.attempt_id) {
            return Err(ForgeError::AttemptNotFound(presented.attempt_id.clone()));
        }
        let attempt = item
            .attempt(&presented.attempt_id)
            .ok_or_else(|| ForgeError::AttemptNotFound(presented.attempt_id.clone()))?;
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
        item.deactivate_attempt(attempt_id);
        self.commit_event(
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
        let event = self.commit_event(
            &item.id,
            actor,
            EventPayload::StateChanged { from, to, reason },
        )?;
        self.persist(item, event.seq)?;
        self.compact_if_needed(&item.id)?;
        Ok(())
    }

    pub(crate) fn transition_to_attempt_state(
        &self,
        item: &mut WorkItem,
        reason: Option<String>,
        actor: &ActorRef,
    ) -> Result<()> {
        let next = item.state_after_attempts();
        if item.state == next {
            self.persist_fresh(item)
        } else {
            self.transition(item, next, reason, actor)
        }
    }

    fn persist_fresh(&self, item: &mut WorkItem) -> Result<()> {
        item.updated_at = Utc::now();
        let seq = self.store.cached_last_seq(&item.id)?;
        self.persist(item, seq)
    }

    fn persist(&self, item: &WorkItem, applied_seq: u64) -> Result<()> {
        self.store.write_snapshot(item, applied_seq)?;
        if let Ok(handle) = self.owners.get_or_open(&self.store, &item.id)
            && let Ok(mut owner) = handle.lock()
        {
            crate::owner::mark_projection_clean(&mut owner, applied_seq);
            let _ = owner.sync_projection(item.clone());
            owner.dirty = false;
        }
        Ok(())
    }

    fn compact_if_needed(&self, work_id: &WorkId) -> Result<()> {
        let _ = compaction::compact_if_needed(&self.store, &self.owners, work_id)?;
        Ok(())
    }
}

pub(crate) fn git_target(item: &WorkItem) -> Result<GitWorkTarget> {
    match &item.target {
        WorkTarget::Git(t) => Ok(t.clone()),
    }
}

fn item_slug(item: &WorkItem) -> String {
    if item.slug.is_empty() {
        crate::slug::project_slug(&item.title)
    } else {
        item.slug.clone()
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

/// Fold / apply helpers live in [`crate::fold`]; re-exported for callers.
pub use crate::fold::{apply_payload, fold};

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
            .register(
                "Fix app",
                "make app v2",
                &fx.repo,
                "main",
                "user-1",
                &actor(),
            )
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
        let item = forge
            .complete_attempt(&lease, &SealOptions::default(), &actor())
            .unwrap();
        assert_eq!(item.state, WorkState::AwaitingReview);
        assert!(item.active_attempt.is_none());
        let attempt = item.attempt(&attempt_id).unwrap();
        assert_eq!(attempt.state, AttemptState::Completed);
        let evidence_id = attempt.evidence_id.clone().unwrap();

        // Checkpoint commit sealed the work.
        let sealed_head = fx.git.head_oid(&env.worktree).unwrap();
        assert_ne!(sealed_head, fx.baseline);
        assert!(fx.git.is_clean(&env.worktree).unwrap());
        let log = fx
            .git
            .run(&env.worktree, &["log", "-1", "--format=%s"])
            .unwrap();
        assert!(log.contains("forge: checkpoint"));

        // Evidence on disk, digests defined.
        let evidence_dir = forge.store().item_dir(&work_id).join("attempts/1/evidence");
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
        let item = forge
            .apply_decision(&work_id, &decision_id, &actor())
            .unwrap();
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
        assert_eq!(
            reopened.disposition,
            Some(AcceptedDisposition::BranchPreserved)
        );
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
    fn seal_promotes_only_valid_compact_receipt_metadata() {
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
        fs::write(env.worktree.join("app.txt"), "v2\n").unwrap();
        let digest = format!("sha256:{}", "a".repeat(64));
        for call_id in ["agent-a-call", "agent-b-call"] {
            forge
                .append_command_log(
                    &lease,
                    &serde_json::json!({
                        "kind": "medousa_coder_ephemeral_evidence_receipt",
                        "schema_version": 1,
                        "work_id": item.id.as_str(),
                        "source_tool": "cognition_shell_session_run",
                        "source_call_id": call_id,
                        "digest": digest,
                        "ephemeral_reference": format!("coder-evidence:sha256:{}", "a".repeat(64)),
                        "content_type": "application/json",
                        "logical_bytes": 4096,
                        "physical_bytes": 1024,
                        "retention": "successful_or_reproducible",
                        "expires_at_unix_seconds": 9_999_999_999u64,
                        "redacted": true,
                        "raw_promoted": false,
                        "recorded_at": Utc::now(),
                    }),
                )
                .unwrap();
        }
        forge
            .append_command_log(
                &lease,
                &serde_json::json!({
                    "kind": "medousa_coder_ephemeral_evidence_receipt",
                    "schema_version": 1,
                    "work_id": item.id.as_str(),
                    "source_tool": "cognition_shell_session_run",
                    "digest": digest,
                    "ephemeral_reference": format!("coder-evidence:sha256:{}", "a".repeat(64)),
                    "content_type": "application/json",
                    "logical_bytes": 4096,
                    "physical_bytes": 1024,
                    "retention": "failed_or_non_reproducible",
                    "expires_at_unix_seconds": 9_999_999_999u64,
                    "redacted": true,
                    "raw_promoted": true,
                    "recorded_at": Utc::now(),
                }),
            )
            .unwrap();

        let item = forge
            .complete_attempt(&lease, &SealOptions::default(), &actor())
            .unwrap();
        let evidence_dir = forge.store().item_dir(&item.id).join("attempts/1/evidence");
        let receipts_bytes = fs::read(evidence_dir.join("receipts.json")).unwrap();
        let receipts: Vec<CompactEvidenceReceipt> =
            serde_json::from_slice(&receipts_bytes).unwrap();
        assert_eq!(receipts.len(), 2);
        assert_eq!(receipts[0].source_call_id.as_deref(), Some("agent-a-call"));
        assert_eq!(receipts[1].source_call_id.as_deref(), Some("agent-b-call"));
        assert!(
            receipts
                .iter()
                .all(|receipt| receipt.raw_evidence == RawEvidenceDisposition::EphemeralOnly)
        );
        let rendered = String::from_utf8(receipts_bytes.clone()).unwrap();
        assert!(!rendered.contains("payload"));
        assert!(!rendered.contains("coder-evidence/objects"));

        let manifest: EvidenceManifest =
            serde_json::from_slice(&fs::read(evidence_dir.join("manifest.json")).unwrap()).unwrap();
        assert_eq!(manifest.compact_receipt_count, 2);
        assert_eq!(manifest.compact_receipt_rejections, 1);
        assert_eq!(
            manifest.compact_receipts_digest,
            Some(Digest::sha256_hex(&receipts_bytes))
        );
        assert!(!manifest.truncated);
        assert!(
            fs::read_dir(&evidence_dir)
                .unwrap()
                .flatten()
                .all(|entry| entry.path().extension().and_then(|ext| ext.to_str()) != Some("gz"))
        );
    }

    #[test]
    fn isolated_attempt_seals_its_private_dirty_state_and_discard_reclaims_it() {
        let fx = fixture();
        let forge = Forge::open(&fx.forge_root).unwrap();
        let item = forge
            .register("t", "b", &fx.repo, "main", "user-1", &actor())
            .unwrap();
        let item = forge.provision(&item.id, &actor()).unwrap();
        let staging = item.environment.clone().unwrap();
        fs::write(staging.worktree.join("app.txt"), "staging dirty\n").unwrap();
        fs::write(staging.worktree.join("notes.txt"), "untracked input\n").unwrap();

        let (item, lease) = forge
            .begin_isolated_attempt(&item.id, script_executor(), None, &actor())
            .unwrap();
        let attempt_environment = item
            .environment_for_attempt(&lease.attempt_id)
            .unwrap()
            .clone();
        assert_ne!(attempt_environment.worktree, staging.worktree);
        assert_ne!(attempt_environment.branch, staging.branch);
        assert_eq!(
            fs::read_to_string(attempt_environment.worktree.join("app.txt")).unwrap(),
            "staging dirty\n"
        );
        assert_eq!(
            fs::read_to_string(attempt_environment.worktree.join("notes.txt")).unwrap(),
            "untracked input\n"
        );

        fs::write(
            attempt_environment.worktree.join("app.txt"),
            "attempt-only result\n",
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(staging.worktree.join("app.txt")).unwrap(),
            "staging dirty\n"
        );

        let item = forge
            .complete_attempt(&lease, &SealOptions::default(), &actor())
            .unwrap();
        let sealed = fx.git.head_oid(&attempt_environment.worktree).unwrap();
        assert_ne!(sealed, fx.git.head_oid(&staging.worktree).unwrap());
        assert!(
            item.attempt(&lease.attempt_id)
                .unwrap()
                .evidence_id
                .is_some()
        );

        let attempt_branch = attempt_environment.branch.clone();
        let staging_branch = staging.branch.clone();
        forge.discard(&item.id, &actor()).unwrap();
        assert!(!attempt_environment.worktree.exists());
        assert!(!staging.worktree.exists());
        assert!(!fx.git.branch_exists(&fx.repo, &attempt_branch));
        assert!(!fx.git.branch_exists(&fx.repo, &staging_branch));
    }

    #[test]
    fn interrupted_isolated_attempt_reuses_its_workspace_with_edits_intact() {
        let fx = fixture();
        let forge = Forge::open(&fx.forge_root).unwrap();
        let item = forge
            .register("t", "b", &fx.repo, "main", "user-1", &actor())
            .unwrap();
        let item = forge.provision(&item.id, &actor()).unwrap();
        let staging = item.environment.clone().unwrap();
        let staging_contents = fs::read_to_string(staging.worktree.join("app.txt")).unwrap();
        let (item, first_lease) = forge
            .begin_isolated_attempt(&item.id, script_executor(), None, &actor())
            .unwrap();
        let first_environment = item
            .environment_for_attempt(&first_lease.attempt_id)
            .unwrap()
            .clone();
        let lineage = first_environment
            .derived_from
            .as_ref()
            .expect("isolated environment lineage");
        assert_eq!(lineage.branch, staging.branch);
        assert_eq!(lineage.generation, staging.generation);
        assert!(
            lineage.forked_at
                <= item
                    .attempt(&first_lease.attempt_id)
                    .expect("first attempt")
                    .started_at
        );
        fs::write(
            first_environment.worktree.join("app.txt"),
            "unfinished agent edit\n",
        )
        .unwrap();

        let item = forge
            .interrupt_attempt(&first_lease, RecoveryDisposition::RestartAllowed, &actor())
            .unwrap();
        assert_eq!(item.state, WorkState::Ready);
        let (item, second_lease) = forge
            .begin_isolated_attempt(&item.id, script_executor(), None, &actor())
            .unwrap();
        let second_environment = item
            .environment_for_attempt(&second_lease.attempt_id)
            .unwrap();

        assert_eq!(second_environment.worktree, first_environment.worktree);
        assert_eq!(second_environment.branch, first_environment.branch);
        assert_eq!(
            second_environment.derived_from,
            first_environment.derived_from
        );
        assert_eq!(
            fs::read_to_string(second_environment.worktree.join("app.txt")).unwrap(),
            "unfinished agent edit\n"
        );
        assert_eq!(
            fs::read_to_string(staging.worktree.join("app.txt")).unwrap(),
            staging_contents
        );
    }

    #[test]
    fn concurrent_attempts_seal_independently_without_interrupting_peers() {
        let fx = fixture();
        let forge = Forge::open(&fx.forge_root).unwrap();
        let item = forge
            .register(
                "parallel",
                "two agents",
                &fx.repo,
                "main",
                "user-1",
                &actor(),
            )
            .unwrap();
        let item = forge.provision(&item.id, &actor()).unwrap();
        let (item, first) = forge
            .begin_isolated_attempt(&item.id, script_executor(), None, &actor())
            .unwrap();
        let (item, second) = forge
            .begin_isolated_attempt(&item.id, script_executor(), None, &actor())
            .unwrap();
        assert_eq!(item.active_attempt_ids().len(), 2);
        let first_env = item
            .environment_for_attempt(&first.attempt_id)
            .unwrap()
            .clone();
        let second_env = item
            .environment_for_attempt(&second.attempt_id)
            .unwrap()
            .clone();
        assert_ne!(first_env.worktree, second_env.worktree);
        assert_ne!(first_env.branch, second_env.branch);
        assert_eq!(
            first_env
                .derived_from
                .as_ref()
                .map(|lineage| lineage.branch.as_str()),
            item.environment
                .as_ref()
                .map(|environment| environment.branch.as_str())
        );
        assert_eq!(
            second_env
                .derived_from
                .as_ref()
                .map(|lineage| (lineage.branch.as_str(), lineage.generation)),
            item.environment
                .as_ref()
                .map(|environment| (environment.branch.as_str(), environment.generation))
        );

        fs::write(first_env.worktree.join("first.txt"), "first agent\n").unwrap();
        fs::write(second_env.worktree.join("second.txt"), "second agent\n").unwrap();
        let item = forge
            .complete_attempt(&first, &SealOptions::default(), &actor())
            .unwrap();
        assert_eq!(item.state, WorkState::Executing);
        assert_eq!(item.active_attempt_ids(), vec![&second.attempt_id]);
        assert!(
            item.attempt(&first.attempt_id)
                .unwrap()
                .evidence_id
                .is_some()
        );
        assert!(forge.heartbeat(&second).is_ok());
        assert!(second_env.worktree.join("second.txt").exists());

        let item = forge
            .complete_attempt(&second, &SealOptions::default(), &actor())
            .unwrap();
        assert_eq!(item.state, WorkState::AwaitingReview);
        assert!(item.active_attempt_ids().is_empty());
        assert_eq!(
            item.attempts
                .iter()
                .filter(|attempt| attempt.evidence_id.is_some())
                .count(),
            2
        );

        let first_manifest: EvidenceManifest = serde_json::from_slice(
            &fs::read(
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
            actor: actor(),
            attempt_id: first.attempt_id.clone(),
            environment_generation: first_env.generation,
            evidence_id: first_manifest.evidence_id.clone(),
            evidence_digest: first_manifest.bundle_digest.clone().unwrap(),
            baseline_oid: first_manifest.baseline_oid.clone(),
            reviewed_head_oid: first_manifest.sealed_head_oid.clone(),
            expected_base_oid: fx.baseline.clone(),
            acknowledged_violations: Vec::new(),
            strategy: IntegrationStrategy::PreserveBranch,
            rationale: Some("select the first candidate".into()),
            decided_at: Utc::now(),
        };
        let decision_id = decision.id.clone();
        forge.decide(&item.id, decision, &actor()).unwrap();
        let item = forge
            .apply_decision(&item.id, &decision_id, &actor())
            .unwrap();
        assert_eq!(item.state, WorkState::Accepted);
        assert_eq!(item.disposition, Some(AcceptedDisposition::BranchPreserved));
    }

    #[test]
    fn reviewed_work_can_reopen_without_losing_its_checkpoint() {
        let fx = fixture();
        let forge = Forge::open(&fx.forge_root).unwrap();
        let item = forge
            .register(
                "Revise app",
                "make app v2",
                &fx.repo,
                "main",
                "user-1",
                &actor(),
            )
            .unwrap();
        let item = forge.provision(&item.id, &actor()).unwrap();
        let worktree = item.environment.as_ref().unwrap().worktree.clone();
        let (item, lease) = forge
            .begin_attempt(&item.id, script_executor(), None, &actor())
            .unwrap();
        fs::write(worktree.join("app.txt"), "v2\n").unwrap();
        let reviewed = forge
            .complete_attempt(&lease, &SealOptions::default(), &actor())
            .unwrap();
        let checkpoint = forge.git().head_oid(&worktree).unwrap();
        assert_eq!(reviewed.state, WorkState::AwaitingReview);
        assert!(reviewed.attempts.last().unwrap().evidence_id.is_some());

        let reopened = forge
            .reopen_for_changes(&item.id, "restore one file", &actor())
            .unwrap();
        assert_eq!(reopened.state, WorkState::Ready);
        assert_eq!(forge.git().head_oid(&worktree).unwrap(), checkpoint);
        assert!(reopened.attempts.last().unwrap().evidence_id.is_some());
    }

    #[test]
    fn review_comment_folds_onto_item() {
        let fx = fixture();
        let forge = Forge::open(&fx.forge_root).unwrap();
        let (item, _env, _sealed_head, manifest) = to_awaiting_review(&fx, &forge);
        let evidence_id = manifest.evidence_id.clone();

        let item = forge
            .add_review_comment(
                &item.id,
                evidence_id.clone(),
                None,
                "feature.txt",
                "new",
                1,
                1,
                Some("shipped".into()),
                "Please expand this",
                None,
                &actor(),
            )
            .unwrap();
        assert_eq!(item.review_comments.len(), 1);
        assert_eq!(item.review_comments[0].path, "feature.txt");
        assert_eq!(
            item.review_comments[0].anchor_digest,
            anchor_digest_for("shipped")
        );
        assert_eq!(
            item.review_comments[0].thread_id,
            item.review_comments[0].id
        );

        let events = forge.store().replay(&item.id).unwrap();
        let folded = fold(&events).unwrap();
        assert_eq!(folded.review_comments.len(), 1);
        assert_eq!(folded.review_comments[0].body, "Please expand this");
    }

    #[test]
    fn request_changes_records_and_reopens_to_ready() {
        let fx = fixture();
        let forge = Forge::open(&fx.forge_root).unwrap();
        let (item, _env, _sealed_head, manifest) = to_awaiting_review(&fx, &forge);
        let evidence_id = manifest.evidence_id.clone();
        let evidence_digest = manifest.bundle_digest.clone().unwrap();

        let item = forge
            .add_review_comment(
                &item.id,
                evidence_id.clone(),
                None,
                "feature.txt",
                "new",
                1,
                1,
                Some("shipped".into()),
                "Needs a test",
                None,
                &actor(),
            )
            .unwrap();
        let comment_id = item.review_comments[0].id.clone();

        let item = forge
            .request_changes(
                &item.id,
                evidence_id.clone(),
                evidence_digest,
                Some("Please revise".into()),
                Some(vec![comment_id.clone()]),
                &actor(),
            )
            .unwrap();

        assert_eq!(item.state, WorkState::Ready);
        assert_eq!(item.changes_requested.len(), 1);
        assert_eq!(item.changes_requested[0].comment_ids, vec![comment_id]);
        assert!(
            item.changes_requested[0]
                .revision_brief
                .contains("Please revise")
        );
        assert!(
            item.changes_requested[0]
                .revision_brief
                .contains("feature.txt:1")
        );
        assert!(item.review_decisions.is_empty());

        let events = forge.store().replay(&item.id).unwrap();
        let folded = fold(&events).unwrap();
        assert_eq!(folded.state, WorkState::Ready);
        assert_eq!(folded.changes_requested.len(), 1);
        assert_eq!(folded.review_comments.len(), 1);
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
        let err = forge
            .complete_attempt(&stale, &SealOptions::default(), &actor())
            .unwrap_err();
        assert!(matches!(err, ForgeError::StaleLease { .. }));

        // A lease pointing at a different attempt must be rejected.
        let mut alien = lease.clone();
        alien.attempt_id = AttemptId::new();
        let err = forge
            .complete_attempt(&alien, &SealOptions::default(), &actor())
            .unwrap_err();
        assert!(matches!(err, ForgeError::AttemptNotFound(_)));

        // The real lease still works.
        forge
            .complete_attempt(&lease, &SealOptions::default(), &actor())
            .unwrap();
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
        let item = forge
            .complete_attempt(&lease, &SealOptions::default(), &actor())
            .unwrap();
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
        assert_eq!(
            forge.load(&item.id).unwrap().state,
            WorkState::AwaitingReview
        );
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
        assert_eq!(forge.store().replay(&item.id).unwrap().len(), events_before);
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
            .append_command_log(&lease, &serde_json::json!({"kind": "prompt", "chars": 12}))
            .unwrap();
        forge
            .append_command_log(&lease, &serde_json::json!({"kind": "tool", "name": "read"}))
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
        assert!(
            forge
                .append_command_log(&stale, &serde_json::json!({"kind": "x"}))
                .is_err()
        );
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
        let err = forge
            .complete_attempt(&lease, &SealOptions::default(), &actor())
            .unwrap_err();
        assert!(matches!(
            err,
            ForgeError::StaleLease { .. } | ForgeError::AttemptNotFound(_)
        ));
        forge
            .complete_attempt(&lease2, &SealOptions::default(), &actor())
            .unwrap();
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
        let item = forge
            .fail_attempt(&lease, "adapter exploded", &actor())
            .unwrap();
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
        let paths: Vec<&str> = manifest
            .changed_files
            .iter()
            .map(|f| f.path.as_str())
            .collect();
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
    ) -> (
        WorkItem,
        crate::model::GovernedEnv,
        crate::model::GitOid,
        EvidenceManifest,
    ) {
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
        let item = forge
            .apply_decision(&item.id, &decision_id, &actor())
            .unwrap();

        assert_eq!(item.state, WorkState::Accepted);
        assert_eq!(
            item.disposition,
            Some(AcceptedDisposition::BaseFastForwarded)
        );
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
        assert_eq!(
            forge.load(&item.id).unwrap().state,
            WorkState::AwaitingReview
        );
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
        let item = forge
            .apply_decision(&item.id, &decision_id, &actor())
            .unwrap();

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
    fn discard_while_executing_interrupts_and_removes_worktree() {
        let fx = fixture();
        let forge = Forge::open(&fx.forge_root).unwrap();
        let item = forge
            .register(
                "Close me",
                "teardown while editing",
                &fx.repo,
                "main",
                "user-1",
                &actor(),
            )
            .unwrap();
        let item = forge.provision(&item.id, &actor()).unwrap();
        let staging = item.environment.clone().unwrap();
        let (item, _lease) = forge
            .begin_attempt(&item.id, script_executor(), None, &actor())
            .unwrap();
        assert_eq!(item.state, WorkState::Executing);
        assert!(item.has_active_attempts());

        let discarded = forge.discard(&item.id, &actor()).unwrap();
        assert_eq!(discarded.state, WorkState::Discarded);
        assert!(!discarded.has_active_attempts());
        assert!(!staging.worktree.exists());
        assert!(!fx.git.branch_exists(&fx.repo, &staging.branch));
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
