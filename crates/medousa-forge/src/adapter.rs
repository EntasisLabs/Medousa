//! Reference adapter: proves the public lifecycle API is executor-independent.
//! The blocking `run_script` shape is intentionally *not* the core Executor
//! contract — adapters own execution and report into Forge through the lease
//! API (`begin_attempt` / `heartbeat` / `complete_attempt` / `fail_attempt`).

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};

use serde::Serialize;

use crate::error::{ForgeError, Result};
use crate::forge::{Forge, SealOptions};
use crate::model::{ExecutorDescriptor, WorkId, WorkItem};

/// Cap on captured output per stream (bounded evidence).
const OUTPUT_CAP: usize = 64 * 1024;

#[derive(Debug, Serialize)]
struct CommandRecord {
    argv: Vec<String>,
    cwd: String,
    started_at: String,
    ended_at: String,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
    truncated: bool,
}

pub struct ScriptAdapter<'a> {
    forge: &'a Forge,
}

impl<'a> ScriptAdapter<'a> {
    pub fn new(forge: &'a Forge) -> Self {
        Self { forge }
    }

    /// Run `argv` inside the item's governed worktree, then complete (exit 0)
    /// or fail (non-zero) the attempt. The command log is staged into the
    /// attempt's evidence directory before sealing, so it becomes part of the
    /// sealed bundle and its digest.
    pub fn run_script(&self, work_id: &WorkId, argv: &[String]) -> Result<WorkItem> {
        if argv.is_empty() {
            return Err(ForgeError::Git("script argv is empty".into()));
        }
        let actor = Forge::system_actor();
        let executor = ExecutorDescriptor {
            kind: "script".into(),
            detail: serde_json::json!({ "argv": argv }),
        };
        let (item, lease) = self
            .forge
            .begin_attempt(work_id, executor, None, &actor)?;
        let env = item
            .environment
            .clone()
            .ok_or_else(|| ForgeError::EnvironmentDrift("no environment".into()))?;
        let attempt_seq = item
            .attempt(&lease.attempt_id)
            .map(|a| a.seq)
            .unwrap_or(1);

        let started = chrono::Utc::now();
        let mut child = Command::new(&argv[0])
            .args(&argv[1..])
            .current_dir(&env.worktree)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("GIT_TERMINAL_PROMPT", "0")
            .spawn()
            .map_err(|e| ForgeError::Git(format!("failed to spawn {}: {e}", argv[0])))?;
        let pid = child.id();
        let lease = crate::model::ExecutionLease { pid: Some(pid), ..lease };
        self.forge.heartbeat(&lease)?;

        let mut stdout = child.stdout.take().expect("piped");
        let mut stderr = child.stderr.take().expect("piped");
        let mut out_buf = Vec::new();
        let mut err_buf = Vec::new();
        let _ = stdout.read_to_end(&mut out_buf);
        let _ = stderr.read_to_end(&mut err_buf);
        let status = child
            .wait()
            .map_err(|e| ForgeError::Git(format!("failed to wait on {}: {e}", argv[0])))?;
        let ended = chrono::Utc::now();

        let truncated = out_buf.len() > OUTPUT_CAP || err_buf.len() > OUTPUT_CAP;
        out_buf.truncate(OUTPUT_CAP);
        err_buf.truncate(OUTPUT_CAP);
        let record = CommandRecord {
            argv: argv.to_vec(),
            cwd: env.worktree.to_string_lossy().into_owned(),
            started_at: started.to_rfc3339(),
            ended_at: ended.to_rfc3339(),
            exit_code: status.code(),
            stdout: String::from_utf8_lossy(&out_buf).into_owned(),
            stderr: String::from_utf8_lossy(&err_buf).into_owned(),
            truncated,
        };

        // Stage the command log where capture_evidence will pick it up.
        let evidence_dir = self
            .forge
            .store()
            .item_dir(work_id)
            .join("attempts")
            .join(attempt_seq.to_string())
            .join("evidence");
        std::fs::create_dir_all(&evidence_dir)?;
        let mut line = serde_json::to_vec(&record)?;
        line.push(b'\n');
        std::fs::write(evidence_dir.join("commands.jsonl"), &line)?;

        if status.success() {
            self.forge
                .complete_attempt(&lease, &SealOptions::default(), &actor)
        } else {
            let tail: String = String::from_utf8_lossy(&err_buf)
                .lines()
                .last()
                .unwrap_or("script failed")
                .chars()
                .take(200)
                .collect();
            self.forge.fail_attempt(&lease, &tail, &actor)
        }
    }
}

/// Produce a portable work package: the item, its full event log, and every
/// attempt's sealed evidence, copied to `destination`.
pub fn export_bundle(forge: &Forge, work_id: &WorkId, destination: &Path) -> Result<()> {
    let item = forge.load(work_id)?;
    std::fs::create_dir_all(destination)?;
    std::fs::write(
        destination.join("item.json"),
        serde_json::to_vec_pretty(&item)?,
    )?;
    let events = forge.store().replay(work_id)?;
    let mut log = Vec::new();
    for event in &events {
        log.extend_from_slice(&serde_json::to_vec(event)?);
        log.push(b'\n');
    }
    std::fs::write(destination.join("events.jsonl"), &log)?;

    let item_dir = forge.store().item_dir(work_id);
    let attempts_src = item_dir.join("attempts");
    if attempts_src.exists() {
        copy_dir(&attempts_src, &destination.join("attempts"))?;
    }
    let dispositions = item_dir.join("dispositions");
    if dispositions.exists() {
        copy_dir(&dispositions, &destination.join("dispositions"))?;
    }
    Ok(())
}

fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let target = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::{CheckpointAuthor, GitEngine};
    use crate::model::{AttemptState, WorkState};
    use tempfile::TempDir;

    struct Fx {
        _repo_tmp: TempDir,
        _forge_tmp: TempDir,
        repo: std::path::PathBuf,
        forge_root: std::path::PathBuf,
    }

    fn fixture() -> Fx {
        let repo_tmp = TempDir::new().unwrap();
        let forge_tmp = TempDir::new().unwrap();
        let git = GitEngine::detect().unwrap();
        git.run(repo_tmp.path(), &["init", "-b", "main"]).unwrap();
        std::fs::write(repo_tmp.path().join("a.txt"), "a\n").unwrap();
        git.run(repo_tmp.path(), &["add", "-A"]).unwrap();
        git.commit_checkpoint(repo_tmp.path(), "init", &CheckpointAuthor::default())
            .unwrap();
        Fx {
            repo: repo_tmp.path().to_path_buf(),
            forge_root: forge_tmp.path().to_path_buf(),
            _repo_tmp: repo_tmp,
            _forge_tmp: forge_tmp,
        }
    }

    fn ready_item(forge: &Forge, fx: &Fx) -> WorkItem {
        let actor = Forge::system_actor();
        let item = forge
            .register("t", "b", &fx.repo, "main", "user-1", &actor)
            .unwrap();
        forge.provision(&item.id, &actor).unwrap()
    }

    #[test]
    fn script_success_completes_attempt_and_seals_command_log() {
        let fx = fixture();
        let forge = Forge::open(&fx.forge_root).unwrap();
        let item = ready_item(&forge, &fx);
        let adapter = ScriptAdapter::new(&forge);

        let script = vec![
            "sh".to_string(),
            "-c".to_string(),
            "echo hello-from-script > script-output.txt && echo ran".to_string(),
        ];
        let item = adapter.run_script(&item.id, &script).unwrap();
        assert_eq!(item.state, WorkState::AwaitingReview);
        let attempt = &item.attempts[0];
        assert_eq!(attempt.state, AttemptState::Completed);

        // The command log is inside the sealed evidence and its digest.
        let commands = std::fs::read_to_string(
            forge
                .store()
                .item_dir(&item.id)
                .join("attempts/1/evidence/commands.jsonl"),
        )
        .unwrap();
        assert!(commands.contains("hello-from-script"));
        assert!(commands.contains("\"exit_code\":0"));
        // The script's work was checkpointed.
        let env = item.environment.unwrap();
        assert!(env.worktree.join("script-output.txt").exists());
    }

    #[test]
    fn script_failure_marks_attempt_failed_and_returns_to_ready() {
        let fx = fixture();
        let forge = Forge::open(&fx.forge_root).unwrap();
        let item = ready_item(&forge, &fx);
        let adapter = ScriptAdapter::new(&forge);

        let script = vec![
            "sh".to_string(),
            "-c".to_string(),
            "echo boom >&2 && exit 3".to_string(),
        ];
        let item = adapter.run_script(&item.id, &script).unwrap();
        assert_eq!(item.state, WorkState::Ready);
        let attempt = &item.attempts[0];
        assert_eq!(attempt.state, AttemptState::Failed);
    }

    #[test]
    fn export_bundle_produces_portable_package() {
        let fx = fixture();
        let forge = Forge::open(&fx.forge_root).unwrap();
        let item = ready_item(&forge, &fx);
        let adapter = ScriptAdapter::new(&forge);
        let script = vec![
            "sh".to_string(),
            "-c".to_string(),
            "echo bundled > bundled.txt".to_string(),
        ];
        let item = adapter.run_script(&item.id, &script).unwrap();

        let dest = TempDir::new().unwrap();
        let bundle = dest.path().join("bundle");
        export_bundle(&forge, &item.id, &bundle).unwrap();

        let exported: WorkItem =
            serde_json::from_str(&std::fs::read_to_string(bundle.join("item.json")).unwrap())
                .unwrap();
        assert_eq!(exported.id, item.id);
        assert_eq!(exported.state, WorkState::AwaitingReview);
        let log = std::fs::read_to_string(bundle.join("events.jsonl")).unwrap();
        assert!(log.lines().count() > 3);
        assert!(bundle.join("attempts/1/evidence/manifest.json").exists());
        assert!(bundle.join("attempts/1/evidence/patch.diff").exists());
        assert!(bundle.join("attempts/1/evidence/commands.jsonl").exists());
    }
}
