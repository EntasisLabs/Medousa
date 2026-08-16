//! Daemon-side Forge host helpers: open the forge root under the workshop data
//! dir and provide a process-backed [`LivenessProbe`] for boot reconciliation.

use medousa_forge::forge::Forge;
use medousa_forge::reconcile::LivenessProbe;
use sysinfo::{Pid, ProcessesToUpdate, System};

use crate::paths::medousa_data_dir;

/// `{medousa_data_dir()}/forge` — custody metadata lives here, never inside
/// the user's vault/worktree.
pub fn forge_root() -> std::path::PathBuf {
    medousa_data_dir().join("forge")
}

/// Open Forge at the workshop forge root. Caller attaches execution admission,
/// rebuilds the catalog through that service, then runs `reconcile_on_boot`
/// before serving HTTP.
pub fn open_forge() -> anyhow::Result<Forge> {
    Forge::open(forge_root()).map_err(|err| anyhow::anyhow!("failed to open forge store: {err}"))
}

/// Process liveness for Forge lease reconciliation. Instance id is the Forge
/// boot id; PIDs are probed via sysinfo (existence only — not a security
/// boundary). Start markers are accepted but not yet verified in this host.
pub struct DaemonLivenessProbe {
    instance_id: String,
}

impl DaemonLivenessProbe {
    pub fn new(instance_id: impl Into<String>) -> Self {
        Self {
            instance_id: instance_id.into(),
        }
    }
}

impl LivenessProbe for DaemonLivenessProbe {
    fn current_instance_id(&self) -> &str {
        &self.instance_id
    }

    fn is_process_alive(&self, pid: u32, _process_start_marker: Option<&str>) -> bool {
        if pid == 0 {
            return false;
        }
        let mut sys = System::new();
        let pid = Pid::from_u32(pid);
        sys.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
        sys.process(pid).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use medousa_forge::adapter::ScriptAdapter;
    use medousa_forge::forge::Forge;
    use medousa_forge::git::{CheckpointAuthor, GitEngine};
    use medousa_forge::model::{
        ActorKind, ActorRef, EvidenceManifest, IntegrationStrategy, ReviewDecision,
        ReviewDecisionId, WorkState,
    };
    use std::fs;
    use tempfile::TempDir;

    fn actor() -> ActorRef {
        ActorRef {
            kind: ActorKind::System,
            id: "test".into(),
        }
    }

    fn init_repo(dir: &std::path::Path) -> medousa_forge::model::GitOid {
        let git = GitEngine::detect().unwrap();
        std::process::Command::new("git")
            .args(["init", "-b", "main"])
            .current_dir(dir)
            .status()
            .unwrap();
        fs::write(dir.join("note.md"), "# hello\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(dir)
            .status()
            .unwrap();
        git.commit_checkpoint(dir, "initial", &CheckpointAuthor::default())
            .unwrap()
    }

    #[test]
    fn forge_root_is_under_data_dir() {
        let root = forge_root();
        assert!(root.ends_with("forge"));
    }

    #[test]
    fn probe_reports_current_process_alive_and_zero_dead() {
        let probe = DaemonLivenessProbe::new("boot-test");
        assert_eq!(probe.current_instance_id(), "boot-test");
        assert!(probe.is_process_alive(std::process::id(), None));
        assert!(!probe.is_process_alive(0, None));
    }

    #[test]
    fn reconcile_on_boot_is_idempotent_for_empty_store() {
        let tmp = TempDir::new().unwrap();
        let forge = Forge::open(tmp.path()).unwrap();
        let probe = DaemonLivenessProbe::new(forge.instance_id());
        let report = forge.reconcile_on_boot(&probe).unwrap();
        assert!(report.interrupted_attempts.is_empty());
        assert!(report.rolled_forward.is_empty());
        let report2 = forge.reconcile_on_boot(&probe).unwrap();
        assert!(report2.interrupted_attempts.is_empty());
        assert!(report2.rolled_forward.is_empty());
    }

    #[test]
    fn host_lifecycle_survives_reopen_after_preserve_branch() {
        let repo_tmp = TempDir::new().unwrap();
        let forge_tmp = TempDir::new().unwrap();
        let baseline = init_repo(repo_tmp.path());

        let forge = Forge::open(forge_tmp.path()).unwrap();
        let probe = DaemonLivenessProbe::new(forge.instance_id());
        forge.reconcile_on_boot(&probe).unwrap();

        let item = forge
            .register(
                "Update note",
                "vault-style undertaking",
                repo_tmp.path(),
                "main",
                "user-1",
                &actor(),
            )
            .unwrap();
        forge.provision(&item.id, &actor()).unwrap();
        let script = vec![
            "sh".to_string(),
            "-c".to_string(),
            "echo permanent >> note.md".to_string(),
        ];
        let item = ScriptAdapter::new(&forge)
            .run_script(&item.id, &script)
            .unwrap();
        assert_eq!(item.state, WorkState::AwaitingReview);

        let attempt_id = item.attempts[0].id.clone();
        let env = item.environment_for_attempt(&attempt_id).cloned().unwrap();
        let sealed_head = forge.git().head_oid(&env.worktree).unwrap();
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
            actor: actor(),
            attempt_id,
            environment_generation: env.generation,
            evidence_id: manifest.evidence_id.clone(),
            evidence_digest: manifest.bundle_digest.clone().unwrap(),
            baseline_oid: baseline.clone(),
            reviewed_head_oid: sealed_head,
            expected_base_oid: baseline,
            acknowledged_violations: Vec::new(),
            strategy: IntegrationStrategy::PreserveBranch,
            rationale: Some("keep pocket".into()),
            decided_at: chrono::Utc::now(),
        };
        let decision_id = decision.id.clone();
        forge.decide(&item.id, decision, &actor()).unwrap();
        let item = forge
            .apply_decision(&item.id, &decision_id, &actor())
            .unwrap();
        assert_eq!(item.state, WorkState::Accepted);

        // Simulate daemon restart: new Forge instance, reconcile, load.
        let forge2 = Forge::open(forge_tmp.path()).unwrap();
        let probe2 = DaemonLivenessProbe::new(forge2.instance_id());
        forge2.reconcile_on_boot(&probe2).unwrap();
        let reopened = forge2.load(&item.id).unwrap();
        assert_eq!(reopened.state, WorkState::Accepted);
        assert_eq!(
            reopened.disposition,
            Some(medousa_forge::model::AcceptedDisposition::BranchPreserved)
        );
    }
}
