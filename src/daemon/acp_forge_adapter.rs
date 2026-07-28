//! Forge lease adapter for ACP-backed agent sessions.
//!
//! An ACP session bound to a Forge undertaking (`CreateAgentSessionRequest.work_id`)
//! reports executor lifecycle through these helpers. Chat SSE streaming is untouched —
//! this adapter runs beside the pump, never instead of it. Sealing stays explicit via
//! `POST /v1/forge/.../complete`; `AcpEvent::Done` is never a seal.

use medousa_forge::error::ForgeError;
use medousa_forge::forge::Forge;
use medousa_forge::model::{
    ActorKind, ActorRef, ExecutionLease, ExecutorDescriptor, RecoveryDisposition, WorkId, WorkItem,
};

pub struct AcpForgeAdapter<'a> {
    forge: &'a Forge,
}

pub struct AcpForgeContext<'a> {
    pub agent_session_id: &'a str,
    pub acp_session_id: &'a str,
    pub chat_session_id: &'a str,
    pub runtime: &'a str,
    pub pid: Option<u32>,
}

impl<'a> AcpForgeAdapter<'a> {
    pub fn new(forge: &'a Forge) -> Self {
        Self { forge }
    }

    /// Begin an attempt for a bound session's first prompt.
    pub fn begin_attempt(
        &self,
        work_id: &WorkId,
        ctx: &AcpForgeContext<'_>,
    ) -> Result<(WorkItem, ExecutionLease), ForgeError> {
        let mut detail = serde_json::Map::new();
        detail.insert(
            "agent_session_id".into(),
            serde_json::Value::String(ctx.agent_session_id.to_string()),
        );
        detail.insert(
            "acp_session_id".into(),
            serde_json::Value::String(ctx.acp_session_id.to_string()),
        );
        detail.insert(
            "chat_session_id".into(),
            serde_json::Value::String(ctx.chat_session_id.to_string()),
        );
        if let Some(pid) = ctx.pid {
            detail.insert("pid".into(), serde_json::Value::Number(pid.into()));
        }
        let executor = ExecutorDescriptor {
            kind: format!("acp-{}", ctx.runtime),
            detail: serde_json::Value::Object(detail),
        };
        self.forge
            .begin_attempt(work_id, executor, ctx.pid, &self.daemon_actor())
    }

    /// Liveness heartbeat during a prompt pump.
    pub fn heartbeat(&self, lease: &ExecutionLease) -> Result<(), ForgeError> {
        self.forge.heartbeat(lease)
    }

    /// Report a pump failure; Forge returns the work to Ready.
    pub fn fail_attempt(&self, lease: &ExecutionLease, message: &str) -> Result<WorkItem, ForgeError> {
        self.forge
            .fail_attempt(lease, message, &self.daemon_actor())
    }

    /// Interrupt on cancel, stashing the ACP wire session id so a future
    /// `session/resume` integration can reattach instead of restarting.
    pub fn interrupt_attempt(
        &self,
        lease: &ExecutionLease,
        _reason: &str,
        acp_session_id: Option<&str>,
    ) -> Result<WorkItem, ForgeError> {
        let recovery = match acp_session_id {
            Some(token) if !token.trim().is_empty() => RecoveryDisposition::ResumeSupported {
                provider_token: token.to_string(),
            },
            _ => RecoveryDisposition::RestartAllowed,
        };
        self.forge
            .interrupt_attempt(lease, recovery, &self.daemon_actor())
    }

    /// Stage a prompt-accepted line into the attempt's command log (lease-fenced).
    pub fn record_prompt(&self, lease: &ExecutionLease, prompt_chars: usize) -> Result<(), ForgeError> {
        let line = serde_json::json!({
            "kind": "prompt",
            "chars": prompt_chars,
            "at": chrono::Utc::now().to_rfc3339(),
        });
        self.forge.append_command_log(lease, &line)
    }

    /// Stage a tool-call line into the attempt's command log (lease-fenced).
    pub fn record_tool(
        &self,
        lease: &ExecutionLease,
        name: &str,
        tool_id: &str,
    ) -> Result<(), ForgeError> {
        let line = serde_json::json!({
            "kind": "tool",
            "name": name,
            "id": tool_id,
            "at": chrono::Utc::now().to_rfc3339(),
        });
        self.forge.append_command_log(lease, &line)
    }

    fn daemon_actor(&self) -> ActorRef {
        ActorRef {
            kind: ActorKind::System,
            id: "medousa-daemon".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use medousa_forge::forge::Forge;
    use medousa_forge::model::WorkState;
    use std::process::Command;

    fn git(repo: &std::path::Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .expect("git spawn");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn provisioned_work() -> (tempfile::TempDir, Forge, WorkId) {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-b", "main"]);
        git(&repo, &["config", "user.email", "forge@test"]);
        git(&repo, &["config", "user.name", "Forge Test"]);
        std::fs::write(repo.join("a.txt"), "one").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-m", "init"]);

        let forge = Forge::open(dir.path().join("forge")).unwrap();
        let actor = ActorRef {
            kind: ActorKind::System,
            id: "test".into(),
        };
        let item = forge
            .register(
                "acp adapter test",
                "adapter lease lifecycle",
                &repo,
                "main",
                "test-owner",
                &actor,
            )
            .unwrap();
        let item = forge.provision(&item.id, &actor).unwrap();
        assert!(matches!(item.state, WorkState::Ready));
        (dir, forge, item.id)
    }

    fn ctx<'a>() -> AcpForgeContext<'a> {
        AcpForgeContext {
            agent_session_id: "agent-test",
            acp_session_id: "acp-test",
            chat_session_id: "chat-test",
            runtime: "cursor",
            pid: None,
        }
    }

    #[test]
    fn begin_heartbeat_interrupt_round_trip() {
        let (_dir, forge, work_id) = provisioned_work();
        let adapter = AcpForgeAdapter::new(&forge);

        let (item, lease) = adapter.begin_attempt(&work_id, &ctx()).unwrap();
        assert!(matches!(item.state, WorkState::Executing));
        assert_eq!(lease.work_id, work_id);
        let attempt = item.attempts.last().unwrap();
        assert_eq!(attempt.executor.kind, "acp-cursor");
        assert_eq!(
            attempt.executor.detail["agent_session_id"],
            serde_json::Value::String("agent-test".into())
        );

        adapter.heartbeat(&lease).unwrap();
        let item = forge.load(&work_id).unwrap();
        let attempt = item.attempts.last().unwrap();
        let heartbeat = attempt.lease.as_ref().unwrap().heartbeat_at;
        assert!(heartbeat >= lease.heartbeat_at);

        let item = adapter
            .interrupt_attempt(&lease, "user cancel", Some("acp-test"))
            .unwrap();
        assert!(matches!(item.state, WorkState::Ready));
        let attempt = item.attempts.last().unwrap();
        assert_eq!(
            attempt.recovery,
            Some(RecoveryDisposition::ResumeSupported {
                provider_token: "acp-test".into()
            })
        );
    }

    #[test]
    fn fail_returns_work_to_ready_and_lease_dies() {
        let (_dir, forge, work_id) = provisioned_work();
        let adapter = AcpForgeAdapter::new(&forge);

        let (_, lease) = adapter.begin_attempt(&work_id, &ctx()).unwrap();
        let item = adapter.fail_attempt(&lease, "pump exploded").unwrap();
        assert!(matches!(item.state, WorkState::Ready));
        let attempt = item.attempts.last().unwrap();
        assert_eq!(attempt.recovery, Some(RecoveryDisposition::RestartAllowed));

        // After fail the item returns to Ready with no active attempt — a dead
        // lease must be rejected, whatever the specific fencing error.
        assert!(adapter.heartbeat(&lease).is_err());
    }

    #[test]
    fn second_attempt_on_ready_work_starts_fresh_lease() {
        let (_dir, forge, work_id) = provisioned_work();
        let adapter = AcpForgeAdapter::new(&forge);

        let (_, lease_one) = adapter.begin_attempt(&work_id, &ctx()).unwrap();
        adapter.interrupt_attempt(&lease_one, "cancel", None).unwrap();
        let (item, lease_two) = adapter.begin_attempt(&work_id, &ctx()).unwrap();
        assert_eq!(item.attempts.len(), 2);
        assert_ne!(lease_one.lease_id, lease_two.lease_id);
        assert!(lease_two.generation > lease_one.generation);
    }

    #[test]
    fn interrupt_without_wire_id_marks_restart_allowed() {
        let (_dir, forge, work_id) = provisioned_work();
        let adapter = AcpForgeAdapter::new(&forge);

        let (_, lease) = adapter.begin_attempt(&work_id, &ctx()).unwrap();
        let item = adapter
            .interrupt_attempt(&lease, "cancel", None)
            .unwrap();
        let attempt = item.attempts.last().unwrap();
        assert_eq!(attempt.recovery, Some(RecoveryDisposition::RestartAllowed));
    }

    #[test]
    fn record_prompt_and_tool_stage_command_log() {
        let (_dir, forge, work_id) = provisioned_work();
        let adapter = AcpForgeAdapter::new(&forge);

        let (item, lease) = adapter.begin_attempt(&work_id, &ctx()).unwrap();
        let seq = item.attempts.last().unwrap().seq;
        adapter.record_prompt(&lease, 42).unwrap();
        adapter.record_tool(&lease, "read_file", "tool-1").unwrap();

        let path = forge
            .store()
            .item_dir(&work_id)
            .join("attempts")
            .join(seq.to_string())
            .join("evidence/commands.jsonl");
        let raw = std::fs::read_to_string(path).unwrap();
        assert!(raw.contains("\"prompt\""));
        assert!(raw.contains("\"read_file\""));
        assert!(raw.contains("42"));
    }
}
