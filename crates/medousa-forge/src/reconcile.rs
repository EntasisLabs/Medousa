//! Boot reconciliation: classify and preserve, roll forward idempotently,
//! never auto-start an executor and never auto-delete anything user-visible.
//!
//! Inputs are the per-item event logs (the operation journal), on-disk
//! reality (worktrees, refs), and a caller-provided [`LivenessProbe`]. Every
//! partial-operation boundary is handled: crash after `operation_started`,
//! crash after each side effect, crash before `operation_committed`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::ForgeError;
use crate::error::Result;
use crate::events::{EventPayload, OperationKind, SideEffect, TransitionEvent};
use crate::forge::Forge;
use crate::model::{AttemptId, AttemptState, OperationId, RecoveryDisposition, WorkId, WorkState};

/// Caller-provided liveness knowledge. Forge cannot know whether a provider's
/// subprocesses or sessions outlived a daemon restart — the host answers.
pub trait LivenessProbe {
    /// The booting daemon's instance identity.
    fn current_instance_id(&self) -> &str;
    /// Is this pid alive *and* the same process (start marker guards reuse)?
    fn is_process_alive(&self, pid: u32, process_start_marker: Option<&str>) -> bool;
}

/// What reconciliation found and did. Classification is always reported;
/// automatic action is limited to journaling (roll-forward) and attempt
/// interruption — environments and dirty work are preserved untouched.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReconcileReport {
    /// Operations completed (rolled forward) from their journal records.
    pub rolled_forward: Vec<RolledForward>,
    /// Running attempts whose lease belonged to a prior boot: interrupted,
    /// item returned to Ready, work preserved.
    pub interrupted_attempts: Vec<InterruptedAttempt>,
    /// Items whose environment is missing on disk (environment failure, not
    /// executor failure) — marked Failed.
    pub environment_failures: Vec<WorkId>,
    /// Forge worktrees on disk that no live item owns. Reported only — never
    /// auto-deleted.
    pub orphaned_worktrees: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RolledForward {
    pub work_id: WorkId,
    pub operation_id: OperationId,
    pub kind: OperationKind,
    pub outcome: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterruptedAttempt {
    pub work_id: WorkId,
    pub attempt_id: AttemptId,
    pub reason: String,
}

/// An operation that started but never committed or aborted.
struct OpenOperation {
    operation_id: OperationId,
    kind: OperationKind,
    attempt_id: Option<AttemptId>,
    effects: Vec<SideEffect>,
}

impl Forge {
    /// Reconcile after boot. Idempotent: running it twice is a no-op the
    /// second time.
    pub fn reconcile_on_boot(&self, probe: &dyn LivenessProbe) -> Result<ReconcileReport> {
        let mut report = ReconcileReport::default();
        for work_id in self.store.list_item_ids()? {
            let events = self.store.replay(&work_id)?;
            if events.is_empty() {
                continue;
            }
            // Clear the snapshot cache so every fold below starts from the
            // authoritative log.
            let snapshot = self.store.snapshot_path(&work_id);
            if snapshot.exists() {
                std::fs::remove_file(&snapshot)?;
            }
            self.reconcile_item(&work_id, &events, probe, &mut report)?;
        }
        report.orphaned_worktrees = self.scan_orphaned_worktrees()?;
        Ok(report)
    }

    fn reconcile_item(
        &self,
        work_id: &WorkId,
        events: &[TransitionEvent],
        probe: &dyn LivenessProbe,
        report: &mut ReconcileReport,
    ) -> Result<()> {
        let actor = Forge::system_actor();
        let item = crate::forge::fold(events)?;
        if item.state.is_terminal() {
            // Terminal items still may have an open discard operation.
            if let Some(open) = open_operation(events)
                && open.kind == OperationKind::Discard
            {
                self.roll_forward_discard(work_id, &open, &actor, report)?;
            }
            return Ok(());
        }

        // 1. Roll forward any open operation first — it owns the transition.
        if let Some(open) = open_operation(events) {
            match open.kind {
                OperationKind::Provision => {
                    self.roll_forward_provision(work_id, &item, &open, &actor, report)?
                }
                OperationKind::Seal => {
                    self.roll_forward_seal(work_id, &item, &open, &actor, report)?
                }
                OperationKind::Integrate => {
                    self.roll_forward_integrate(work_id, &item, &open, &actor, report)?
                }
                OperationKind::Discard => {
                    self.roll_forward_discard(work_id, &open, &actor, report)?
                }
            }
            return Ok(());
        }

        // 2. Executing attempt from a previous boot → interrupt, preserve work.
        if item.state == WorkState::Executing {
            let running: Vec<_> = item
                .active_attempt_ids()
                .into_iter()
                .filter_map(|id| item.attempt(id))
                .filter(|attempt| attempt.state == AttemptState::Running)
                .cloned()
                .collect();
            for attempt in running {
                let lease = attempt.lease.clone();
                let stale = match &lease {
                    Some(l) => {
                        l.owner_instance_id != probe.current_instance_id()
                            || !l
                                .pid
                                .map(|pid| {
                                    probe.is_process_alive(pid, l.process_start_marker.as_deref())
                                })
                                .unwrap_or(false)
                    }
                    None => true,
                };
                if stale {
                    let mut item = self.load(work_id)?;
                    let attempt_id = attempt.id.clone();
                    self.end_attempt(
                        &mut item,
                        &attempt_id,
                        AttemptState::Interrupted,
                        RecoveryDisposition::RestartAllowed,
                        &actor,
                    )?;
                    self.transition_to_attempt_state(
                        &mut item,
                        Some("executor lost across restart; work preserved".into()),
                        &actor,
                    )?;
                    report.interrupted_attempts.push(InterruptedAttempt {
                        work_id: work_id.clone(),
                        attempt_id,
                        reason: "lease owned by a prior boot or dead process".into(),
                    });
                }
            }
        }

        // 3. A missing private environment fails only its addressed attempt;
        //    healthy peers retain custody of their own worktrees.
        let item = self.load(work_id)?;
        let missing_attempts: Vec<_> = item
            .active_attempt_ids()
            .into_iter()
            .filter_map(|id| item.attempt(id))
            .filter(|attempt| {
                attempt.state == AttemptState::Running
                    && attempt
                        .environment
                        .as_ref()
                        .is_some_and(|env| !env.worktree.exists())
            })
            .map(|attempt| attempt.id.clone())
            .collect();
        for attempt_id in missing_attempts {
            let mut item = self.load(work_id)?;
            self.end_attempt(
                &mut item,
                &attempt_id,
                AttemptState::Failed,
                RecoveryDisposition::RestartAllowed,
                &actor,
            )?;
            self.transition_to_attempt_state(
                &mut item,
                Some(format!(
                    "attempt {attempt_id} environment missing; healthy peers preserved"
                )),
                &actor,
            )?;
            report.environment_failures.push(work_id.clone());
        }

        // 4. The staging anchor must exist on disk for non-terminal items past
        //    provisioning. Missing worktree = environment failure.
        let item = self.load(work_id)?;
        let missing_env = item
            .environment
            .as_ref()
            .filter(|env| {
                matches!(item.state, WorkState::Ready | WorkState::Executing)
                    && !env.worktree.exists()
            })
            .map(|env| env.worktree.display().to_string());
        if let Some(missing) = missing_env {
            let mut item = item;
            self.transition(
                &mut item,
                WorkState::Failed,
                Some(format!("environment worktree missing: {missing}")),
                &actor,
            )?;
            report.environment_failures.push(work_id.clone());
        }
        Ok(())
    }

    // ---------------------------------------------------------------
    // Roll-forward handlers
    // ---------------------------------------------------------------

    fn roll_forward_provision(
        &self,
        work_id: &WorkId,
        _item: &crate::model::WorkItem,
        open: &OpenOperation,
        actor: &crate::model::ActorRef,
        report: &mut ReconcileReport,
    ) -> Result<()> {
        let added = open.effects.iter().find_map(|e| match e {
            SideEffect::WorktreeAdded {
                path,
                branch,
                baseline_oid,
            } => Some((path.clone(), branch.clone(), baseline_oid.clone())),
            _ => None,
        });
        let mut item = self.load(work_id)?;
        match added {
            Some((path, branch, baseline_oid)) if path.exists() => {
                // Worktree exists: complete the provisioning record.
                let target = crate::forge::git_target(&item)?;
                let repo = self.git.repo_identity(&target.repo_path)?;
                let generation = 1;
                let env = crate::model::GovernedEnv {
                    kind: crate::model::EnvironmentKind::GitWorktree,
                    repo,
                    worktree: path,
                    branch,
                    baseline_oid,
                    generation,
                    derived_from: None,
                };
                item.environment = Some(env.clone());
                self.store.append(
                    work_id,
                    actor,
                    EventPayload::EnvironmentProvisioned { env: Box::new(env) },
                )?;
                self.store.append(
                    work_id,
                    actor,
                    EventPayload::OperationCommitted {
                        operation_id: open.operation_id.clone(),
                        resulting_state: WorkState::Ready,
                    },
                )?;
                self.transition(&mut item, WorkState::Ready, None, actor)?;
                report.rolled_forward.push(RolledForward {
                    work_id: work_id.clone(),
                    operation_id: open.operation_id.clone(),
                    kind: OperationKind::Provision,
                    outcome: "worktree existed; provisioning completed".into(),
                });
            }
            _ => {
                // No worktree on disk: nothing happened for real; back to Draft.
                self.store.append(
                    work_id,
                    actor,
                    EventPayload::OperationAborted {
                        operation_id: open.operation_id.clone(),
                        reason: "crashed before worktree creation".into(),
                    },
                )?;
                self.transition(&mut item, WorkState::Draft, None, actor)?;
                report.rolled_forward.push(RolledForward {
                    work_id: work_id.clone(),
                    operation_id: open.operation_id.clone(),
                    kind: OperationKind::Provision,
                    outcome: "no worktree; rolled back to draft".into(),
                });
            }
        }
        Ok(())
    }

    fn roll_forward_seal(
        &self,
        work_id: &WorkId,
        _item: &crate::model::WorkItem,
        open: &OpenOperation,
        actor: &crate::model::ActorRef,
        report: &mut ReconcileReport,
    ) -> Result<()> {
        let checkpoint = open.effects.iter().find_map(|e| match e {
            SideEffect::CheckpointCommitCreated { oid, .. } => Some(oid.clone()),
            _ => None,
        });
        let mut item = self.load(work_id)?;
        let attempt_id = open
            .attempt_id
            .clone()
            .or_else(|| {
                open.effects.iter().find_map(|effect| match effect {
                    SideEffect::CheckpointCommitCreated { branch, .. } => item
                        .attempts
                        .iter()
                        .find(|attempt| {
                            attempt.environment.as_ref().map(|env| env.branch.as_str())
                                == Some(branch.as_str())
                        })
                        .map(|attempt| attempt.id.clone()),
                    _ => None,
                })
            })
            .or_else(|| {
                item.latest_active_attempt()
                    .map(|attempt| attempt.id.clone())
            })
            .ok_or_else(|| {
                ForgeError::EnvironmentDrift("seal op has no addressed attempt".into())
            })?;
        match checkpoint {
            Some(sealed_head) => {
                // Commit exists: complete evidence + close the attempt.
                let env = item
                    .environment_for_attempt(&attempt_id)
                    .cloned()
                    .ok_or_else(|| ForgeError::EnvironmentDrift("no environment".into()))?;
                let attempt = item
                    .attempt(&attempt_id)
                    .ok_or_else(|| ForgeError::AttemptNotFound(attempt_id.clone()))?
                    .clone();
                // Re-derive evidence deterministically (idempotent: same
                // inputs, same digest, files overwritten).
                let pre_changed = self.worktree_changed_files(&env)?;
                let violations = crate::policy::evaluate_paths(&item.policy, &pre_changed)?;
                let evidence = self.capture_evidence(
                    &item,
                    &env,
                    &attempt,
                    &sealed_head,
                    violations,
                    Vec::new(),
                )?;
                if let Some(att) = item.attempt_mut(&attempt_id) {
                    att.evidence_id = Some(evidence.evidence_id.clone());
                }
                self.store.append(
                    work_id,
                    actor,
                    EventPayload::EvidenceSealed {
                        attempt_id: attempt_id.clone(),
                        evidence_id: evidence.evidence_id,
                        evidence_digest: evidence.bundle_digest.unwrap(),
                    },
                )?;
                self.end_attempt(
                    &mut item,
                    &attempt_id,
                    AttemptState::Completed,
                    RecoveryDisposition::NotResumable,
                    actor,
                )?;
                let resulting_state = item.state_after_attempts();
                self.store.append(
                    work_id,
                    actor,
                    EventPayload::OperationCommitted {
                        operation_id: open.operation_id.clone(),
                        resulting_state,
                    },
                )?;
                self.transition_to_attempt_state(&mut item, None, actor)?;
                report.rolled_forward.push(RolledForward {
                    work_id: work_id.clone(),
                    operation_id: open.operation_id.clone(),
                    kind: OperationKind::Seal,
                    outcome: "checkpoint existed; evidence captured, awaiting review".into(),
                });
            }
            None => {
                // Crash before the checkpoint: abort the seal, then classify
                // the still-Running attempt as interrupted (the executor was
                // sealing when the process died).
                self.store.append(
                    work_id,
                    actor,
                    EventPayload::OperationAborted {
                        operation_id: open.operation_id.clone(),
                        reason: "crashed before checkpoint commit".into(),
                    },
                )?;
                self.end_attempt(
                    &mut item,
                    &attempt_id,
                    AttemptState::Interrupted,
                    RecoveryDisposition::RestartAllowed,
                    actor,
                )?;
                self.transition_to_attempt_state(
                    &mut item,
                    Some("seal interrupted by crash; work preserved".into()),
                    actor,
                )?;
                report.rolled_forward.push(RolledForward {
                    work_id: work_id.clone(),
                    operation_id: open.operation_id.clone(),
                    kind: OperationKind::Seal,
                    outcome: "no checkpoint; seal aborted, attempt interrupted".into(),
                });
            }
        }
        Ok(())
    }

    fn roll_forward_integrate(
        &self,
        work_id: &WorkId,
        _item: &crate::model::WorkItem,
        open: &OpenOperation,
        actor: &crate::model::ActorRef,
        report: &mut ReconcileReport,
    ) -> Result<()> {
        let mut item = self.load(work_id)?;
        let advanced = open.effects.iter().find_map(|e| match e {
            SideEffect::BaseRefAdvanced {
                ref_name, new_oid, ..
            } => Some((ref_name.clone(), new_oid.clone())),
            _ => None,
        });
        let exported = open.effects.iter().find_map(|e| match e {
            SideEffect::PatchExported { path, digest } => Some((path.clone(), digest.clone())),
            _ => None,
        });
        let (disposition, outcome) = if let Some((ref_name, new_oid)) = advanced {
            // Verify the ref actually moved before claiming the disposition.
            let target = crate::forge::git_target(&item)?;
            let actual = self.git.ref_oid(&target.repo_path, &ref_name)?;
            if actual != new_oid {
                // Ref update did not land: abort, back to review.
                self.store.append(
                    work_id,
                    actor,
                    EventPayload::OperationAborted {
                        operation_id: open.operation_id.clone(),
                        reason: "base ref did not reach the recorded oid".into(),
                    },
                )?;
                self.transition(&mut item, WorkState::AwaitingReview, None, actor)?;
                report.rolled_forward.push(RolledForward {
                    work_id: work_id.clone(),
                    operation_id: open.operation_id.clone(),
                    kind: OperationKind::Integrate,
                    outcome: "ref update not visible; back to review".into(),
                });
                return Ok(());
            }
            (
                crate::model::AcceptedDisposition::BaseFastForwarded,
                format!("{ref_name} verified at {new_oid}"),
            )
        } else if let Some((path, digest)) = exported {
            if !path.exists() {
                self.store.append(
                    work_id,
                    actor,
                    EventPayload::OperationAborted {
                        operation_id: open.operation_id.clone(),
                        reason: "exported patch missing".into(),
                    },
                )?;
                self.transition(&mut item, WorkState::AwaitingReview, None, actor)?;
                report.rolled_forward.push(RolledForward {
                    work_id: work_id.clone(),
                    operation_id: open.operation_id.clone(),
                    kind: OperationKind::Integrate,
                    outcome: "patch missing; back to review".into(),
                });
                return Ok(());
            }
            let _ = digest;
            (
                crate::model::AcceptedDisposition::PatchExported,
                format!("patch verified at {}", path.display()),
            )
        } else {
            // No integration side effect landed: PreserveBranch was in flight
            // (or nothing happened). PreserveBranch has no side effects, so
            // any integrate op without effects completes as BranchPreserved.
            (
                crate::model::AcceptedDisposition::BranchPreserved,
                "no side effects; branch preserved".to_string(),
            )
        };

        item.disposition = Some(disposition);
        self.store.append(
            work_id,
            actor,
            EventPayload::DispositionApplied {
                disposition,
                detail: Some(outcome.clone()),
            },
        )?;
        self.store.append(
            work_id,
            actor,
            EventPayload::OperationCommitted {
                operation_id: open.operation_id.clone(),
                resulting_state: WorkState::Accepted,
            },
        )?;
        self.transition(&mut item, WorkState::Accepted, None, actor)?;
        report.rolled_forward.push(RolledForward {
            work_id: work_id.clone(),
            operation_id: open.operation_id.clone(),
            kind: OperationKind::Integrate,
            outcome,
        });
        Ok(())
    }

    fn roll_forward_discard(
        &self,
        work_id: &WorkId,
        open: &OpenOperation,
        actor: &crate::model::ActorRef,
        report: &mut ReconcileReport,
    ) -> Result<()> {
        let mut item = self.load(work_id)?;
        if let Some(env) = item.environment.clone() {
            let target = crate::forge::git_target(&item)?;
            if env.worktree.exists() {
                self.git.worktree_remove(&target.repo_path, &env.worktree)?;
            }
            if self.git.branch_exists(&target.repo_path, &env.branch) {
                self.git.branch_delete(&target.repo_path, &env.branch)?;
            }
        }
        self.store.append(
            work_id,
            actor,
            EventPayload::OperationCommitted {
                operation_id: open.operation_id.clone(),
                resulting_state: WorkState::Discarded,
            },
        )?;
        self.transition(&mut item, WorkState::Discarded, None, actor)?;
        report.rolled_forward.push(RolledForward {
            work_id: work_id.clone(),
            operation_id: open.operation_id.clone(),
            kind: OperationKind::Discard,
            outcome: "discard completed".into(),
        });
        Ok(())
    }

    /// Forge worktrees on disk that no live item owns. Reported, never
    /// auto-deleted.
    fn scan_orphaned_worktrees(&self) -> Result<Vec<PathBuf>> {
        let mut orphaned = Vec::new();
        let root = self.store.root().join("worktrees");
        if !root.exists() {
            return Ok(orphaned);
        }
        let live: std::collections::HashSet<String> = self
            .store
            .list_item_ids()?
            .iter()
            .filter_map(|id| self.load(id).ok())
            .flat_map(|item| {
                item.environment
                    .into_iter()
                    .chain(
                        item.attempts
                            .into_iter()
                            .filter_map(|attempt| attempt.environment),
                    )
                    .map(|env| env.worktree.to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
            })
            .collect();
        for repo_dir in std::fs::read_dir(&root)? {
            let repo_dir = repo_dir?;
            if !repo_dir.file_type()?.is_dir() {
                continue;
            }
            for wt in std::fs::read_dir(repo_dir.path())? {
                let wt = wt?;
                let path = wt.path();
                if !live.contains(&path.to_string_lossy().into_owned()) {
                    orphaned.push(path);
                }
            }
        }
        Ok(orphaned)
    }
}

/// Find an operation that started but never committed or aborted, collecting
/// its recorded side effects.
fn open_operation(events: &[TransitionEvent]) -> Option<OpenOperation> {
    let mut open: Option<OpenOperation> = None;
    for event in events {
        match &event.payload {
            EventPayload::OperationStarted {
                operation_id,
                kind,
                attempt_id,
            } => {
                open = Some(OpenOperation {
                    operation_id: operation_id.clone(),
                    kind: *kind,
                    attempt_id: attempt_id.clone(),
                    effects: Vec::new(),
                });
            }
            EventPayload::OperationSideEffect {
                operation_id,
                effect,
            } => {
                if let Some(o) = open.as_mut()
                    && o.operation_id == *operation_id
                {
                    o.effects.push(effect.clone());
                }
            }
            EventPayload::OperationCommitted { operation_id, .. }
            | EventPayload::OperationAborted { operation_id, .. } => {
                if let Some(o) = &open
                    && o.operation_id == *operation_id
                {
                    open = None;
                }
            }
            _ => {}
        }
    }
    open
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forge::SealOptions;
    use crate::git::{CheckpointAuthor, GitEngine};
    use crate::model::{ActorRef, ExecutorDescriptor};
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    struct Probe {
        instance: String,
        alive_pids: Vec<u32>,
    }

    impl LivenessProbe for Probe {
        fn current_instance_id(&self) -> &str {
            &self.instance
        }

        fn is_process_alive(&self, pid: u32, _marker: Option<&str>) -> bool {
            self.alive_pids.contains(&pid)
        }
    }

    struct Fx {
        _repo_tmp: TempDir,
        _forge_tmp: TempDir,
        repo: PathBuf,
        forge_root: PathBuf,
        git: GitEngine,
        baseline: crate::model::GitOid,
    }

    fn fixture() -> Fx {
        let repo_tmp = TempDir::new().unwrap();
        let forge_tmp = TempDir::new().unwrap();
        let git = GitEngine::detect().unwrap();
        git.run(repo_tmp.path(), &["init", "-b", "main"]).unwrap();
        fs::write(repo_tmp.path().join("a.txt"), "a\n").unwrap();
        git.run(repo_tmp.path(), &["add", "-A"]).unwrap();
        let baseline = git
            .commit_checkpoint(repo_tmp.path(), "init", &CheckpointAuthor::default())
            .unwrap();
        Fx {
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

    fn executor() -> ExecutorDescriptor {
        ExecutorDescriptor {
            kind: "script".into(),
            detail: serde_json::json!({}),
        }
    }

    /// Forge whose instance id looks like a *new* boot.
    fn reopened(forge_root: &Path) -> Forge {
        Forge::open(forge_root).unwrap()
    }

    fn probe() -> Probe {
        Probe {
            instance: "boot-new".into(),
            alive_pids: Vec::new(),
        }
    }

    #[test]
    fn executing_attempt_from_prior_boot_is_interrupted_and_preserved() {
        let fx = fixture();
        let forge = Forge::open(&fx.forge_root).unwrap();
        let item = forge
            .register("t", "b", &fx.repo, "main", "user-1", &actor())
            .unwrap();
        forge.provision(&item.id, &actor()).unwrap();
        let (item, lease) = forge
            .begin_attempt(&item.id, executor(), Some(u32::MAX), &actor())
            .unwrap();
        let env = item.environment.clone().unwrap();
        fs::write(env.worktree.join("wip.txt"), "unsaved work\n").unwrap();

        // New boot: reconcile. The lease belongs to the old instance and the
        // pid is dead.
        let forge2 = reopened(&fx.forge_root);
        assert_ne!(forge2.instance_id(), lease.owner_instance_id);
        let report = forge2.reconcile_on_boot(&probe()).unwrap();
        assert_eq!(report.interrupted_attempts.len(), 1);
        let item = forge2.load(&item.id).unwrap();
        assert_eq!(item.state, WorkState::Ready);
        let attempt = item.attempt(&lease.attempt_id).unwrap();
        assert_eq!(attempt.state, AttemptState::Interrupted);
        // Dirty work preserved, untouched.
        assert_eq!(
            fs::read_to_string(env.worktree.join("wip.txt")).unwrap(),
            "unsaved work\n"
        );
        // Idempotent: second reconcile is a no-op.
        let report2 = forge2.reconcile_on_boot(&probe()).unwrap();
        assert!(report2.interrupted_attempts.is_empty());
        assert!(report2.rolled_forward.is_empty());
    }

    #[test]
    fn missing_worktree_is_environment_failure_not_executor_failure() {
        let fx = fixture();
        let forge = Forge::open(&fx.forge_root).unwrap();
        let item = forge
            .register("t", "b", &fx.repo, "main", "user-1", &actor())
            .unwrap();
        forge.provision(&item.id, &actor()).unwrap();
        let env = forge.load(&item.id).unwrap().environment.unwrap();
        // The worktree vanishes while the daemon is down.
        fx.git.worktree_remove(&fx.repo, &env.worktree).unwrap();

        let forge2 = reopened(&fx.forge_root);
        let report = forge2.reconcile_on_boot(&probe()).unwrap();
        assert_eq!(report.environment_failures, vec![item.id.clone()]);
        assert!(report.interrupted_attempts.is_empty());
        assert_eq!(forge2.load(&item.id).unwrap().state, WorkState::Failed);
    }

    #[test]
    fn open_provision_with_worktree_rolls_forward_to_ready() {
        let fx = fixture();
        let forge = Forge::open(&fx.forge_root).unwrap();
        let item = forge
            .register("t", "b", &fx.repo, "main", "user-1", &actor())
            .unwrap();

        // Simulate a crash mid-provision: journal records the worktree add
        // (and the worktree exists), but no EnvironmentProvisioned / commit.
        let worktree = fx.forge_root.join("worktrees/r1/w1");
        fx.git
            .worktree_add(&fx.repo, &worktree, "worktree/w1", &fx.baseline)
            .unwrap();
        let op = OperationId::new();
        let store = forge.store();
        store
            .append(
                &item.id,
                &actor(),
                EventPayload::StateChanged {
                    from: WorkState::Draft,
                    to: WorkState::Provisioning,
                    reason: None,
                },
            )
            .unwrap();
        store
            .append(
                &item.id,
                &actor(),
                EventPayload::OperationStarted {
                    operation_id: op.clone(),
                    kind: OperationKind::Provision,
                    attempt_id: None,
                },
            )
            .unwrap();
        store
            .append(
                &item.id,
                &actor(),
                EventPayload::OperationSideEffect {
                    operation_id: op.clone(),
                    effect: SideEffect::WorktreeAdded {
                        path: worktree.clone(),
                        branch: "worktree/w1".into(),
                        baseline_oid: fx.baseline.clone(),
                    },
                },
            )
            .unwrap();
        let snapshot = store.snapshot_path(&item.id);
        if snapshot.exists() {
            fs::remove_file(&snapshot).unwrap();
        }

        let forge2 = reopened(&fx.forge_root);
        let report = forge2.reconcile_on_boot(&probe()).unwrap();
        assert_eq!(report.rolled_forward.len(), 1);
        assert_eq!(report.rolled_forward[0].kind, OperationKind::Provision);
        let item = forge2.load(&item.id).unwrap();
        assert_eq!(item.state, WorkState::Ready);
        assert_eq!(item.environment.unwrap().worktree, worktree);
    }

    #[test]
    fn open_provision_without_worktree_rolls_back_to_draft() {
        let fx = fixture();
        let forge = Forge::open(&fx.forge_root).unwrap();
        let item = forge
            .register("t", "b", &fx.repo, "main", "user-1", &actor())
            .unwrap();
        let op = OperationId::new();
        let store = forge.store();
        store
            .append(
                &item.id,
                &actor(),
                EventPayload::StateChanged {
                    from: WorkState::Draft,
                    to: WorkState::Provisioning,
                    reason: None,
                },
            )
            .unwrap();
        store
            .append(
                &item.id,
                &actor(),
                EventPayload::OperationStarted {
                    operation_id: op.clone(),
                    kind: OperationKind::Provision,
                    attempt_id: None,
                },
            )
            .unwrap();
        let snapshot = store.snapshot_path(&item.id);
        if snapshot.exists() {
            fs::remove_file(&snapshot).unwrap();
        }

        let forge2 = reopened(&fx.forge_root);
        let report = forge2.reconcile_on_boot(&probe()).unwrap();
        assert_eq!(report.rolled_forward.len(), 1);
        let item = forge2.load(&item.id).unwrap();
        assert_eq!(item.state, WorkState::Draft);
        assert!(item.environment.is_none());
        // Can provision cleanly afterwards.
        forge2.provision(&item.id, &actor()).unwrap();
        assert_eq!(forge2.load(&item.id).unwrap().state, WorkState::Ready);
    }

    #[test]
    fn open_integrate_with_advanced_ref_completes_as_fast_forwarded() {
        let fx = fixture();
        let forge = Forge::open(&fx.forge_root).unwrap();
        let item = forge
            .register("t", "b", &fx.repo, "main", "user-1", &actor())
            .unwrap();
        forge.provision(&item.id, &actor()).unwrap();
        let (item, lease) = forge
            .begin_attempt(&item.id, executor(), None, &actor())
            .unwrap();
        let env = item.environment.clone().unwrap();
        fs::write(env.worktree.join("f.txt"), "f\n").unwrap();
        let item = forge
            .complete_attempt(&lease, &SealOptions::default(), &actor())
            .unwrap();
        let sealed_head = fx.git.head_oid(&env.worktree).unwrap();

        // Simulate crash mid-integrate: the ref moved, but the operation was
        // never committed.
        fx.git
            .update_ref_cas(&fx.repo, "refs/heads/main", &sealed_head, &fx.baseline)
            .unwrap();
        let op = OperationId::new();
        let store = forge.store();
        store
            .append(
                &item.id,
                &actor(),
                EventPayload::StateChanged {
                    from: WorkState::AwaitingReview,
                    to: WorkState::ApplyingDecision,
                    reason: None,
                },
            )
            .unwrap();
        store
            .append(
                &item.id,
                &actor(),
                EventPayload::OperationStarted {
                    operation_id: op.clone(),
                    kind: OperationKind::Integrate,
                    attempt_id: None,
                },
            )
            .unwrap();
        store
            .append(
                &item.id,
                &actor(),
                EventPayload::OperationSideEffect {
                    operation_id: op.clone(),
                    effect: SideEffect::BaseRefAdvanced {
                        ref_name: "refs/heads/main".into(),
                        old_oid: fx.baseline.clone(),
                        new_oid: sealed_head.clone(),
                    },
                },
            )
            .unwrap();
        let snapshot = store.snapshot_path(&item.id);
        if snapshot.exists() {
            fs::remove_file(&snapshot).unwrap();
        }

        let forge2 = reopened(&fx.forge_root);
        let report = forge2.reconcile_on_boot(&probe()).unwrap();
        assert_eq!(report.rolled_forward.len(), 1);
        let item = forge2.load(&item.id).unwrap();
        assert_eq!(item.state, WorkState::Accepted);
        assert_eq!(
            item.disposition,
            Some(crate::model::AcceptedDisposition::BaseFastForwarded)
        );
    }

    #[test]
    fn open_discard_is_completed() {
        let fx = fixture();
        let forge = Forge::open(&fx.forge_root).unwrap();
        let item = forge
            .register("t", "b", &fx.repo, "main", "user-1", &actor())
            .unwrap();
        forge.provision(&item.id, &actor()).unwrap();
        let env = forge.load(&item.id).unwrap().environment.unwrap();

        // Crash right after the discard op started.
        let op = OperationId::new();
        forge
            .store()
            .append(
                &item.id,
                &actor(),
                EventPayload::OperationStarted {
                    operation_id: op.clone(),
                    kind: OperationKind::Discard,
                    attempt_id: None,
                },
            )
            .unwrap();

        let forge2 = reopened(&fx.forge_root);
        let report = forge2.reconcile_on_boot(&probe()).unwrap();
        assert_eq!(report.rolled_forward.len(), 1);
        let item = forge2.load(&item.id).unwrap();
        assert_eq!(item.state, WorkState::Discarded);
        assert!(!env.worktree.exists());
        assert!(!fx.git.branch_exists(&fx.repo, &env.branch));
    }

    #[test]
    fn orphaned_worktrees_are_reported_never_deleted() {
        let fx = fixture();
        let forge = Forge::open(&fx.forge_root).unwrap();
        // A live item owns its worktree.
        let item = forge
            .register("t", "b", &fx.repo, "main", "user-1", &actor())
            .unwrap();
        forge.provision(&item.id, &actor()).unwrap();
        let env = forge.load(&item.id).unwrap().environment.unwrap();
        // A ghost worktree from some forgotten life.
        let ghost = fx.forge_root.join("worktrees/ghost/ghost-gen1");
        fs::create_dir_all(&ghost).unwrap();

        let report = forge.reconcile_on_boot(&probe()).unwrap();
        assert_eq!(report.orphaned_worktrees, vec![ghost.clone()]);
        assert!(!report.orphaned_worktrees.contains(&env.worktree));
        // Reported, not deleted.
        assert!(ghost.exists());
        assert!(env.worktree.exists());
    }

    #[test]
    fn live_pid_keeps_attempt_running() {
        let fx = fixture();
        let forge = Forge::open(&fx.forge_root).unwrap();
        let item = forge
            .register("t", "b", &fx.repo, "main", "user-1", &actor())
            .unwrap();
        forge.provision(&item.id, &actor()).unwrap();
        // A live pid (init) stands in for the executor; the lease belongs to
        // the *current* boot instance.
        let (_item, lease) = forge
            .begin_attempt(&item.id, executor(), Some(1), &actor())
            .unwrap();
        let probe = Probe {
            instance: lease.owner_instance_id.clone(),
            alive_pids: vec![1],
        };
        let report = forge.reconcile_on_boot(&probe).unwrap();
        assert!(report.interrupted_attempts.is_empty());
        assert_eq!(forge.load(&item.id).unwrap().state, WorkState::Executing);
    }

    #[test]
    fn stale_peer_is_interrupted_without_disturbing_a_live_attempt() {
        let fx = fixture();
        let forge = Forge::open(&fx.forge_root).unwrap();
        let item = forge
            .register(
                "parallel",
                "recover one peer",
                &fx.repo,
                "main",
                "user-1",
                &actor(),
            )
            .unwrap();
        let item = forge.provision(&item.id, &actor()).unwrap();
        let (_, live) = forge
            .begin_isolated_attempt(&item.id, executor(), Some(1), &actor())
            .unwrap();
        let (_, stale) = forge
            .begin_isolated_attempt(&item.id, executor(), Some(2), &actor())
            .unwrap();
        let probe = Probe {
            instance: live.owner_instance_id.clone(),
            alive_pids: vec![1],
        };

        let report = forge.reconcile_on_boot(&probe).unwrap();
        assert_eq!(report.interrupted_attempts.len(), 1);
        assert_eq!(report.interrupted_attempts[0].attempt_id, stale.attempt_id);
        let item = forge.load(&item.id).unwrap();
        assert_eq!(item.state, WorkState::Executing);
        assert_eq!(item.active_attempt_ids(), vec![&live.attempt_id]);
        assert_eq!(
            item.attempt(&stale.attempt_id).unwrap().state,
            AttemptState::Interrupted
        );
        assert!(forge.heartbeat(&live).is_ok());
    }
}
