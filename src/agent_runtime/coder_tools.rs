//! Least-authority tool surface for one Forge-fenced Coder turn.

use std::collections::HashSet;
use std::path::{Component, Path};
use std::sync::{Arc, Weak};

use async_trait::async_trait;
use genai::chat::Tool;
use medousa_forge::forge::Forge;
use medousa_forge::model::{
    ActorRef, ChangeStatus, ChangedFile, ExecutionLease, RecoveryDisposition, WorkPolicy,
};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use stasis::application::orchestration::tool_registry::ToolRegistry;
use stasis::domain::errors::StasisError;
use stasis::prelude::Result;
use tokio::sync::Mutex;

use super::coder_mode::CoderEntryContext;

const TURN_CONTROL_TOOLS: &[&str] = &[
    "cognition_turn_begin_work",
    "cognition_turn_update_user",
    "cognition_turn_checkpoint",
    "cognition_turn_finish",
    "cognition_turn_request_more_rounds",
];

pub struct CoderTurnLease {
    forge: Arc<Forge>,
    lease: ExecutionLease,
    actor: ActorRef,
}

impl CoderTurnLease {
    pub fn new(forge: Arc<Forge>, lease: ExecutionLease) -> Self {
        Self {
            forge,
            lease,
            actor: Forge::system_actor(),
        }
    }

    pub fn lease(&self) -> &ExecutionLease {
        &self.lease
    }

    fn heartbeat(&self) -> Result<()> {
        self.forge
            .heartbeat(&self.lease)
            .map_err(|err| StasisError::PortFailure(format!("Coder Forge lease rejected: {err}")))
    }

    fn append_receipt(&self, receipt: Value) {
        if let Err(err) = self.forge.append_command_log(&self.lease, &receipt) {
            tracing::warn!(error = %err, "failed to append Coder tool receipt");
        }
    }
}

impl Drop for CoderTurnLease {
    fn drop(&mut self) {
        if let Err(err) = self.forge.interrupt_attempt(
            &self.lease,
            RecoveryDisposition::RestartAllowed,
            &self.actor,
        ) {
            tracing::warn!(error = %err, work_id = %self.lease.work_id, "failed to release Coder turn lease");
        }
    }
}

#[derive(Clone)]
pub struct CoderBoundToolRegistry {
    inner: Arc<dyn ToolRegistry>,
    authority: Weak<CoderTurnLease>,
    entry: Arc<CoderEntryContext>,
    policy: WorkPolicy,
    allowed_tools: HashSet<String>,
    shell_sessions: Arc<Mutex<HashSet<String>>>,
}

impl CoderBoundToolRegistry {
    pub fn new(
        inner: Arc<dyn ToolRegistry>,
        authority: &Arc<CoderTurnLease>,
        entry: Arc<CoderEntryContext>,
        policy: WorkPolicy,
    ) -> Self {
        let mut allowed_tools: HashSet<String> = crate::coding_tools::CODING_COGNITION_TOOLS
            .iter()
            .chain(crate::code_intelligence_tools::CODE_COGNITION_TOOLS.iter())
            .chain(crate::detamu_tools::DETAMU_COGNITION_TOOLS.iter())
            .map(|name| (*name).to_string())
            .collect();
        allowed_tools.extend(TURN_CONTROL_TOOLS.iter().map(|name| (*name).to_string()));
        if !policy.allowed_paths.is_empty() || !policy.denied_paths.is_empty() {
            allowed_tools.remove(crate::coding_tools::COGNITION_SHELL_SESSION_STATUS);
            allowed_tools.remove(crate::coding_tools::COGNITION_SHELL_SESSION_RUN);
            allowed_tools.remove(crate::coding_tools::COGNITION_SHELL_SESSION_INTERRUPT);
        }
        Self {
            inner,
            authority: Arc::downgrade(authority),
            entry,
            policy,
            allowed_tools,
            shell_sessions: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    fn authority(&self) -> Result<Arc<CoderTurnLease>> {
        self.authority
            .upgrade()
            .ok_or_else(|| StasisError::PortFailure("Coder turn authority has expired".to_string()))
    }

    fn bind_input(&self, tool_name: &str, mut input: Value) -> Result<Value> {
        let map = input.as_object_mut().ok_or_else(|| {
            StasisError::PortFailure("Coder tools require an object input".into())
        })?;
        if crate::coding_tools::is_coding_cognition_tool(tool_name) {
            match tool_name {
                crate::coding_tools::COGNITION_CODE_READ
                | crate::coding_tools::COGNITION_CODE_SEARCH
                | crate::coding_tools::COGNITION_CODE_APPLY_PATCH => {
                    reject_mismatched_string(
                        map.get("root"),
                        &self.entry.worktree.to_string_lossy(),
                        "root",
                    )?;
                    map.insert(
                        "root".into(),
                        Value::String(self.entry.worktree.to_string_lossy().into_owned()),
                    );
                    if tool_name == crate::coding_tools::COGNITION_CODE_APPLY_PATCH {
                        self.validate_mutation_path(map.get("path"))?;
                    }
                }
                crate::coding_tools::COGNITION_SHELL_SESSION_STATUS => {
                    if map.get("create").and_then(Value::as_bool) != Some(true) {
                        return Err(StasisError::PortFailure(
                            "Coder may create a bound shell session but cannot list unrelated sessions"
                                .into(),
                        ));
                    }
                    self.bind_work_and_lease(map)?;
                }
                crate::coding_tools::COGNITION_SHELL_SESSION_RUN => {
                    if map.get("session_id").is_none() {
                        self.bind_work_and_lease(map)?;
                    }
                }
                crate::coding_tools::COGNITION_SHELL_SESSION_INTERRUPT => {}
                _ => {}
            }
        } else if crate::code_intelligence_tools::is_code_cognition_tool(tool_name) {
            self.validate_lsp_uri(map.get("uri"))?;
        } else if crate::detamu_tools::is_detamu_cognition_tool(tool_name) {
            if map.contains_key("world") || map.contains_key("version") {
                return Err(StasisError::PortFailure(
                    "Coder Detamu queries are pinned to the active Forge undertaking".into(),
                ));
            }
            reject_mismatched_string(map.get("work_id"), &self.entry.work_id, "work_id")?;
            map.insert("work_id".into(), Value::String(self.entry.work_id.clone()));
        }
        Ok(input)
    }

    fn bind_work_and_lease(&self, map: &mut serde_json::Map<String, Value>) -> Result<()> {
        reject_mismatched_string(map.get("work_id"), &self.entry.work_id, "work_id")?;
        let authority = self.authority()?;
        map.insert("work_id".into(), Value::String(self.entry.work_id.clone()));
        map.insert(
            "lease_id".into(),
            Value::String(authority.lease().lease_id.to_string()),
        );
        map.insert(
            "lease_generation".into(),
            Value::from(authority.lease().generation),
        );
        map.insert(
            "attempt_id".into(),
            Value::String(authority.lease().attempt_id.to_string()),
        );
        Ok(())
    }

    pub async fn interrupt_shell_sessions(&self) {
        let session_ids: Vec<String> = self.shell_sessions.lock().await.drain().collect();
        for session_id in session_ids {
            let input = json!({ "session_id": session_id });
            let result = self
                .inner
                .invoke_tool(
                    crate::coding_tools::COGNITION_SHELL_SESSION_INTERRUPT,
                    input.clone(),
                )
                .await;
            if let Ok(authority) = self.authority() {
                authority.append_receipt(tool_receipt(
                    crate::coding_tools::COGNITION_SHELL_SESSION_INTERRUPT,
                    &input,
                    result.as_ref(),
                ));
            }
            if let Err(err) = result {
                tracing::warn!(error = %err, "failed to interrupt Coder shell session");
            }
        }
    }

    fn validate_mutation_path(&self, value: Option<&Value>) -> Result<()> {
        let path = value.and_then(Value::as_str).ok_or_else(|| {
            StasisError::PortFailure("path is required for Coder mutation".into())
        })?;
        let normalized = normalize_relative_path(path)?;
        let violations = medousa_forge::policy::evaluate_paths(
            &self.policy,
            &[ChangedFile {
                path: normalized,
                status: ChangeStatus::Modified,
                old_path: None,
                is_binary: false,
                byte_size: None,
            }],
        )
        .map_err(|err| StasisError::PortFailure(format!("invalid Forge path policy: {err}")))?;
        if let Some(violation) = violations.first() {
            return Err(StasisError::PortFailure(format!(
                "Coder mutation denied by Forge policy: {} ({})",
                violation.path, violation.rule
            )));
        }
        Ok(())
    }

    fn validate_lsp_uri(&self, value: Option<&Value>) -> Result<()> {
        let uri = value.and_then(Value::as_str).ok_or_else(|| {
            StasisError::PortFailure("uri is required for Coder language intelligence".into())
        })?;
        let url = reqwest::Url::parse(uri)
            .map_err(|err| StasisError::PortFailure(format!("invalid file URI: {err}")))?;
        let path = url.to_file_path().map_err(|_| {
            StasisError::PortFailure("Coder language intelligence requires a file:// URI".into())
        })?;
        let canonical = path
            .canonicalize()
            .map_err(|err| StasisError::PortFailure(format!("cannot resolve LSP path: {err}")))?;
        if !canonical.starts_with(&self.entry.worktree) {
            return Err(StasisError::PortFailure(
                "LSP path escapes the governed Coder worktree".into(),
            ));
        }
        Ok(())
    }

    async fn validate_shell_session(&self, tool_name: &str, input: &Value) -> Result<()> {
        if !matches!(
            tool_name,
            crate::coding_tools::COGNITION_SHELL_SESSION_RUN
                | crate::coding_tools::COGNITION_SHELL_SESSION_INTERRUPT
        ) {
            return Ok(());
        }
        let Some(session_id) = input.get("session_id").and_then(Value::as_str) else {
            return Ok(());
        };
        if !self.shell_sessions.lock().await.contains(session_id) {
            return Err(StasisError::PortFailure(
                "shell session is not owned by this Coder turn".into(),
            ));
        }
        Ok(())
    }

    async fn record_shell_session(&self, tool_name: &str, output: &Value) {
        if matches!(
            tool_name,
            crate::coding_tools::COGNITION_SHELL_SESSION_STATUS
                | crate::coding_tools::COGNITION_SHELL_SESSION_RUN
        ) && let Some(session_id) = output.get("session_id").and_then(Value::as_str)
        {
            self.shell_sessions
                .lock()
                .await
                .insert(session_id.to_string());
        }
    }
}

#[async_trait]
impl ToolRegistry for CoderBoundToolRegistry {
    async fn list_tools(&self) -> Result<Vec<Tool>> {
        let _authority = self.authority()?;
        let tools = self.inner.list_tools().await?;
        Ok(tools
            .into_iter()
            .filter(|tool| self.allowed_tools.contains(tool.name.as_str()))
            .collect())
    }

    async fn invoke_tool(&self, tool_name: &str, input: Value) -> Result<Value> {
        if !self.allowed_tools.contains(tool_name) {
            return Err(StasisError::PortFailure(format!(
                "tool is outside the Coder mode contract: {tool_name}"
            )));
        }
        let authority = self.authority()?;
        authority.heartbeat()?;
        let input = self.bind_input(tool_name, input)?;
        self.validate_shell_session(tool_name, &input).await?;
        let result = self.inner.invoke_tool(tool_name, input.clone()).await;
        if let Ok(output) = &result {
            self.record_shell_session(tool_name, output).await;
        }
        authority.append_receipt(tool_receipt(tool_name, &input, result.as_ref()));
        result
    }
}

fn reject_mismatched_string(value: Option<&Value>, expected: &str, field: &str) -> Result<()> {
    if let Some(actual) = value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        && actual != expected
    {
        return Err(StasisError::PortFailure(format!(
            "Coder {field} is pinned by Forge; expected '{expected}'"
        )));
    }
    Ok(())
}

fn normalize_relative_path(value: &str) -> Result<String> {
    let path = Path::new(value.trim());
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err(StasisError::PortFailure(
            "Coder mutation path must be relative to the governed worktree".into(),
        ));
    }
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(StasisError::PortFailure(
            "Coder mutation path cannot traverse outside the governed worktree".into(),
        ));
    }
    Ok(path.to_string_lossy().replace('\\', "/"))
}

fn tool_receipt(
    tool_name: &str,
    input: &Value,
    result: std::result::Result<&Value, &StasisError>,
) -> Value {
    let command_digest = input
        .get("command")
        .and_then(Value::as_str)
        .map(|command| format!("sha256:{:x}", Sha256::digest(command.as_bytes())));
    let (ok, detail) = match result {
        Ok(output) => (
            true,
            json!({
                "path": output.get("path"),
                "digest": output.get("digest"),
                "session_id": output.get("session_id"),
            }),
        ),
        Err(err) => (false, json!({ "error": truncate(&err.to_string(), 500) })),
    };
    json!({
        "kind": "medousa_coder_tool",
        "tool": tool_name,
        "ok": ok,
        "path": input.get("path"),
        "expected_sha256": input.get("expected_sha256"),
        "command_sha256": command_digest,
        "detail": detail,
    })
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        value.to_string()
    } else {
        value.chars().take(max_chars).collect::<String>() + "…"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use medousa_forge::git::{CheckpointAuthor, GitEngine};
    use medousa_forge::model::{ExecutorDescriptor, WorkState};
    use std::sync::Mutex as StdMutex;
    use tempfile::TempDir;

    #[derive(Default)]
    struct RecordingRegistry {
        last_input: StdMutex<Option<Value>>,
        invoked_tools: StdMutex<Vec<String>>,
    }

    #[async_trait]
    impl ToolRegistry for RecordingRegistry {
        async fn list_tools(&self) -> Result<Vec<Tool>> {
            Ok(vec![
                Tool::new(crate::coding_tools::COGNITION_CODE_READ),
                Tool::new(crate::coding_tools::COGNITION_SHELL_SESSION_STATUS),
                Tool::new("cognition_vault_write"),
            ])
        }

        async fn invoke_tool(&self, tool_name: &str, input: Value) -> Result<Value> {
            *self.last_input.lock().expect("input lock") = Some(input.clone());
            self.invoked_tools
                .lock()
                .expect("tools lock")
                .push(tool_name.to_string());
            if tool_name == crate::coding_tools::COGNITION_SHELL_SESSION_STATUS {
                Ok(json!({ "ok": true, "session_id": "shell-1", "input": input }))
            } else {
                Ok(json!({ "ok": true, "input": input }))
            }
        }
    }

    struct Fixture {
        _repo: TempDir,
        _forge_root: TempDir,
        forge: Arc<Forge>,
        entry: Arc<CoderEntryContext>,
        policy: WorkPolicy,
    }

    fn fixture() -> Fixture {
        let repo = TempDir::new().expect("repo");
        let forge_root = TempDir::new().expect("forge root");
        let git = GitEngine::detect().expect("git");
        let status = std::process::Command::new("git")
            .args(["init", "-b", "main", "--template="])
            .current_dir(repo.path())
            .status()
            .expect("git init");
        assert!(status.success());
        std::fs::create_dir_all(repo.path().join("src")).expect("src");
        std::fs::write(repo.path().join("src/lib.rs"), "pub fn demo() {}\n").expect("source");
        let status = std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(repo.path())
            .status()
            .expect("git add");
        assert!(status.success());
        git.commit_checkpoint(repo.path(), "initial", &CheckpointAuthor::default())
            .expect("commit");
        let forge = Arc::new(Forge::open(forge_root.path()).expect("forge"));
        let policy = WorkPolicy::default();
        let item = forge
            .register_with_policy(
                "Demo",
                "Repair demo",
                repo.path(),
                "main",
                "user-1",
                policy.clone(),
                &Forge::system_actor(),
            )
            .expect("register");
        let item = forge
            .provision(&item.id, &Forge::system_actor())
            .expect("provision");
        let entry = Arc::new(
            super::super::coder_mode::compile_coder_entry(
                &forge,
                &crate::daemon_api::CodeIntentContext {
                    work_id: Some(item.id.to_string()),
                    ..Default::default()
                },
            )
            .expect("entry"),
        );
        Fixture {
            _repo: repo,
            _forge_root: forge_root,
            forge,
            entry,
            policy,
        }
    }

    fn authority(fixture: &Fixture) -> Arc<CoderTurnLease> {
        let (_, lease) = fixture
            .forge
            .begin_attempt(
                &medousa_forge::model::WorkId::from(fixture.entry.work_id.clone()),
                ExecutorDescriptor {
                    kind: "test-coder".into(),
                    detail: Value::Null,
                },
                None,
                &Forge::system_actor(),
            )
            .expect("begin attempt");
        Arc::new(CoderTurnLease::new(fixture.forge.clone(), lease))
    }

    #[tokio::test]
    async fn surface_hides_non_coder_tools_and_injects_forge_root() {
        let fixture = fixture();
        let authority = authority(&fixture);
        let inner = Arc::new(RecordingRegistry::default());
        let registry = CoderBoundToolRegistry::new(
            inner.clone(),
            &authority,
            fixture.entry.clone(),
            fixture.policy.clone(),
        );
        let tools = registry.list_tools().await.expect("list");
        assert_eq!(tools.len(), 2);
        assert!(
            tools
                .iter()
                .any(|tool| tool.name.as_str() == crate::coding_tools::COGNITION_CODE_READ)
        );
        assert!(tools.iter().any(|tool| {
            tool.name.as_str() == crate::coding_tools::COGNITION_SHELL_SESSION_STATUS
        }));

        registry
            .invoke_tool(
                crate::coding_tools::COGNITION_CODE_READ,
                json!({ "path": "src/lib.rs" }),
            )
            .await
            .expect("bound read");
        let input = inner
            .last_input
            .lock()
            .expect("input lock")
            .clone()
            .expect("recorded input");
        assert_eq!(
            input["root"],
            fixture.entry.worktree.to_string_lossy().as_ref()
        );

        registry
            .invoke_tool(
                crate::coding_tools::COGNITION_SHELL_SESSION_STATUS,
                json!({ "create": true }),
            )
            .await
            .expect("bound shell");
        let input = inner
            .last_input
            .lock()
            .expect("input lock")
            .clone()
            .expect("recorded input");
        assert_eq!(input["work_id"], fixture.entry.work_id);
        assert_eq!(input["lease_id"], authority.lease().lease_id.to_string());
        assert_eq!(input["lease_generation"], authority.lease().generation);
        assert_eq!(
            input["attempt_id"],
            authority.lease().attempt_id.to_string()
        );

        registry.interrupt_shell_sessions().await;
        assert_eq!(
            inner.invoked_tools.lock().expect("tools lock").last(),
            Some(&crate::coding_tools::COGNITION_SHELL_SESSION_INTERRUPT.to_string())
        );
    }

    #[tokio::test]
    async fn surface_rejects_root_escape_policy_escape_and_expired_authority() {
        let fixture = fixture();
        let authority = authority(&fixture);
        let restricted_policy = WorkPolicy {
            allowed_paths: vec!["src/**".into()],
            ..WorkPolicy::default()
        };
        let registry = CoderBoundToolRegistry::new(
            Arc::new(RecordingRegistry::default()),
            &authority,
            fixture.entry.clone(),
            restricted_policy,
        );
        assert!(
            registry
                .list_tools()
                .await
                .expect("restricted tools")
                .iter()
                .all(|tool| {
                    tool.name.as_str() != crate::coding_tools::COGNITION_SHELL_SESSION_STATUS
                })
        );
        assert!(
            registry
                .invoke_tool(
                    crate::coding_tools::COGNITION_CODE_READ,
                    json!({ "path": "src/lib.rs", "root": "/tmp/other" }),
                )
                .await
                .is_err()
        );
        assert!(
            registry
                .invoke_tool(
                    crate::coding_tools::COGNITION_CODE_APPLY_PATCH,
                    json!({ "path": "README.md", "expected_sha256": "missing", "content": "x" }),
                )
                .await
                .is_err()
        );

        drop(authority);
        let item = fixture
            .forge
            .load(&medousa_forge::model::WorkId::from(
                fixture.entry.work_id.clone(),
            ))
            .expect("load released work");
        assert_eq!(item.state, WorkState::Ready);
        assert!(registry.list_tools().await.is_err());
        assert!(
            registry
                .invoke_tool(
                    crate::coding_tools::COGNITION_CODE_READ,
                    json!({ "path": "src/lib.rs" }),
                )
                .await
                .is_err()
        );
    }
}
