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

use super::coder_activity::{CoderActivityStore, CoderAgentIdentity};
use super::coder_mode::CoderEntryContext;

const TURN_CONTROL_TOOLS: &[&str] = &[
    "cognition_turn_begin_work",
    "cognition_turn_update_user",
    "cognition_turn_checkpoint",
    "cognition_turn_finish",
    "cognition_turn_request_more_rounds",
    "cognition_turn_propose_mode",
];

pub struct CoderTurnLease {
    forge: Arc<Forge>,
    lease: ExecutionLease,
    actor: ActorRef,
    activity: Arc<CoderActivityStore>,
    identity: CoderAgentIdentity,
}

impl CoderTurnLease {
    pub fn new(
        forge: Arc<Forge>,
        lease: ExecutionLease,
        activity: Arc<CoderActivityStore>,
        identity: CoderAgentIdentity,
    ) -> Result<Self> {
        if let Err(err) = activity.register_agent(&lease.work_id.to_string(), &identity) {
            let actor = Forge::system_actor();
            if let Err(release_err) =
                forge.interrupt_attempt(&lease, RecoveryDisposition::RestartAllowed, &actor)
            {
                tracing::warn!(error = %release_err, "failed to release Coder lease after activity registration failure");
            }
            return Err(StasisError::PortFailure(format!(
                "cannot register Coder activity: {err}"
            )));
        }
        Ok(Self {
            forge,
            lease,
            actor: Forge::system_actor(),
            activity,
            identity,
        })
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

    pub fn shared_space_prompt_appendix(&self) -> Result<String> {
        let snapshot = self
            .activity
            .observe_initial(&self.lease.work_id.to_string(), &self.identity.agent_id)
            .map_err(|err| {
                StasisError::PortFailure(format!("cannot compile Coder shared space: {err}"))
            })?;
        Ok(super::coder_activity::shared_space_prompt_appendix(
            &snapshot,
        ))
    }

    fn engineering_delta(&self) -> Result<Option<super::coder_activity::CoderEngineeringDelta>> {
        self.activity
            .observe_delta(&self.lease.work_id.to_string(), &self.identity.agent_id)
            .map_err(|err| {
                StasisError::PortFailure(format!("cannot observe Coder engineering delta: {err}"))
            })
    }

    fn begin_tool_activity(
        &self,
        tool_name: &str,
        intent: &str,
        targets: Vec<String>,
    ) -> Result<String> {
        self.activity
            .begin_tool(
                &self.lease.work_id.to_string(),
                &self.identity,
                tool_name,
                intent,
                targets,
            )
            .map_err(|err| {
                StasisError::PortFailure(format!("cannot record Coder tool intent: {err}"))
            })
    }

    fn finish_tool_activity(
        &self,
        call_id: &str,
        tool_name: &str,
        intent: &str,
        targets: Vec<String>,
        result: std::result::Result<&Value, &StasisError>,
    ) {
        let mapped = result.map_err(|err| err.to_string());
        let activity_result = mapped
            .as_ref()
            .map(|output| *output)
            .map_err(String::as_str);
        if let Err(err) = self.activity.finish_tool(
            &self.lease.work_id.to_string(),
            &self.identity,
            call_id,
            tool_name,
            intent,
            targets,
            activity_result,
        ) {
            tracing::warn!(error = %err, tool = tool_name, "failed to finish Coder activity");
        }
    }
}

impl super::turn_context::ToolRoundContextProvider for CoderBoundToolRegistry {
    fn context_for_next_round(&self) -> Result<Option<String>> {
        let authority = self.authority()?;
        let Some(delta) = authority.engineering_delta()? else {
            return Ok(None);
        };
        let changed_paths = authority
            .forge
            .git()
            .status_porcelain(&self.entry.worktree)
            .map_err(|err| {
                StasisError::PortFailure(format!(
                    "cannot refresh Coder repository observation: {err}"
                ))
            })?
            .into_iter()
            .map(|entry| entry.path)
            .take(80)
            .collect::<Vec<_>>();
        let head_oid = authority
            .forge
            .git()
            .head_oid(&self.entry.worktree)
            .map_err(|err| StasisError::PortFailure(format!("cannot refresh Coder HEAD: {err}")))?;
        let repository_observation = json!({
            "head_oid": head_oid.to_string(),
            "baseline_oid": self.entry.baseline_oid,
            "branch": self.entry.branch,
            "dirty": !changed_paths.is_empty(),
            "changed_path_count": changed_paths.len(),
            "changed_paths": changed_paths,
            "editor_focus": {
                "active_path": self.entry.editor.active_path,
                "containing_symbol": self.entry.editor.containing_symbol,
            },
            "trust": "forge_and_worktree_observation",
        });
        Ok(Some(
            super::coder_activity::engineering_delta_prompt_appendix(
                &delta,
                repository_observation,
            ),
        ))
    }
}

impl Drop for CoderTurnLease {
    fn drop(&mut self) {
        if let Err(err) = self
            .activity
            .leave_agent(&self.lease.work_id.to_string(), &self.identity)
        {
            tracing::warn!(error = %err, work_id = %self.lease.work_id, "failed to release Coder activity presence");
        }
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
                    "Release the governed shell session before ending the Coder turn",
                    "runtime-cleanup",
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
            .map(with_required_coder_intent)
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
        let (intent, input) = take_coder_intent(input)?;
        let targets = tool_targets(tool_name, &input, authority.lease());
        let call_id = authority.begin_tool_activity(tool_name, &intent, targets.clone())?;
        let input = match self.bind_input(tool_name, input) {
            Ok(input) => input,
            Err(err) => {
                authority.finish_tool_activity(&call_id, tool_name, &intent, targets, Err(&err));
                authority.append_receipt(tool_receipt(
                    tool_name,
                    &intent,
                    &call_id,
                    &Value::Null,
                    Err(&err),
                ));
                return Err(err);
            }
        };
        if let Err(err) = self.validate_shell_session(tool_name, &input).await {
            authority.finish_tool_activity(&call_id, tool_name, &intent, targets, Err(&err));
            authority.append_receipt(tool_receipt(
                tool_name,
                &intent,
                &call_id,
                &input,
                Err(&err),
            ));
            return Err(err);
        }
        let result = self.inner.invoke_tool(tool_name, input.clone()).await;
        if let Ok(output) = &result {
            self.record_shell_session(tool_name, output).await;
        }
        authority.finish_tool_activity(&call_id, tool_name, &intent, targets, result.as_ref());
        authority.append_receipt(tool_receipt(
            tool_name,
            &intent,
            &call_id,
            &input,
            result.as_ref(),
        ));
        result
    }
}

fn with_required_coder_intent(mut tool: Tool) -> Tool {
    let schema = tool.schema.get_or_insert_with(|| {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    });
    if !schema.is_object() {
        *schema = json!({ "type": "object", "properties": {}, "required": [] });
    }
    let object = schema.as_object_mut().expect("Coder tool schema object");
    object
        .entry("type")
        .or_insert_with(|| Value::String("object".into()));
    let properties = object
        .entry("properties")
        .or_insert_with(|| Value::Object(Default::default()));
    if !properties.is_object() {
        *properties = Value::Object(Default::default());
    }
    properties
        .as_object_mut()
        .expect("Coder tool properties")
        .insert(
            "intent".into(),
            json!({
                "type": "string",
                "description": "One short outcome-oriented sentence explaining why this tool call is being made (not private reasoning).",
                "maxLength": 320
            }),
        );
    let required = object
        .entry("required")
        .or_insert_with(|| Value::Array(Vec::new()));
    if !required.is_array() {
        *required = Value::Array(Vec::new());
    }
    let required = required.as_array_mut().expect("Coder required fields");
    if !required
        .iter()
        .any(|field| field.as_str() == Some("intent"))
    {
        required.push(Value::String("intent".into()));
    }
    tool
}

fn take_coder_intent(mut input: Value) -> Result<(String, Value)> {
    let map = input
        .as_object_mut()
        .ok_or_else(|| StasisError::PortFailure("Coder tools require an object input".into()))?;
    let raw = map
        .remove("intent")
        .and_then(|value| value.as_str().map(str::to_string))
        .ok_or_else(|| StasisError::PortFailure("Coder tool intent is required".into()))?;
    let intent = super::coder_activity::validate_intent(&raw).map_err(StasisError::PortFailure)?;
    Ok((intent, input))
}

fn tool_targets(tool_name: &str, input: &Value, lease: &ExecutionLease) -> Vec<String> {
    let mut targets = Vec::new();
    if let Some(path) = input.get("path").and_then(Value::as_str) {
        targets.push(format!("file://{}", path.trim()));
    }
    if let Some(uri) = input.get("uri").and_then(Value::as_str) {
        targets.push(uri.trim().to_string());
    }
    if let Some(session_id) = input.get("session_id").and_then(Value::as_str) {
        targets.push(format!("shell://{session_id}"));
    } else if tool_name.starts_with("cognition_shell_") {
        targets.push(format!("attempt://{}", lease.attempt_id));
    }
    if tool_name.starts_with("cognition_detamu_") {
        targets.push(format!("work://{}", lease.work_id));
    }
    targets.sort();
    targets.dedup();
    targets.truncate(8);
    targets
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
    intent: &str,
    call_id: &str,
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
        "call_id": call_id,
        "tool": tool_name,
        "intent": intent,
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
        activity: Arc<CoderActivityStore>,
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
        let activity = Arc::new(CoderActivityStore::open(
            forge_root.path().join("coder-activity.json"),
        ));
        Fixture {
            _repo: repo,
            _forge_root: forge_root,
            forge,
            entry,
            policy,
            activity,
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
        let identity =
            CoderAgentIdentity::for_turn("test-session", 1, &lease.attempt_id.to_string());
        Arc::new(
            CoderTurnLease::new(
                fixture.forge.clone(),
                lease,
                fixture.activity.clone(),
                identity,
            )
            .expect("Coder authority"),
        )
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
        for tool in &tools {
            let schema = tool.schema.as_ref().expect("Coder schema");
            assert!(
                schema["required"]
                    .as_array()
                    .expect("required fields")
                    .iter()
                    .any(|field| field.as_str() == Some("intent"))
            );
        }

        registry
            .invoke_tool(
                crate::coding_tools::COGNITION_CODE_READ,
                json!({
                    "intent": "Inspect the source before making a change",
                    "path": "src/lib.rs"
                }),
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
        assert!(input.get("intent").is_none());

        registry
            .invoke_tool(
                crate::coding_tools::COGNITION_SHELL_SESSION_STATUS,
                json!({
                    "intent": "Open a governed shell for focused verification",
                    "create": true
                }),
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
                    json!({
                        "intent": "Inspect source outside the claimed root",
                        "path": "src/lib.rs",
                        "root": "/tmp/other"
                    }),
                )
                .await
                .is_err()
        );
        assert!(
            registry
                .invoke_tool(
                    crate::coding_tools::COGNITION_CODE_APPLY_PATCH,
                    json!({
                        "intent": "Change a path outside the allowed policy",
                        "path": "README.md",
                        "expected_sha256": "missing",
                        "content": "x"
                    }),
                )
                .await
                .is_err()
        );
        let activity = fixture
            .activity
            .snapshot(&fixture.entry.work_id, "observer")
            .expect("activity snapshot");
        assert!(activity.recent_events.iter().any(|event| {
            event.kind == super::super::coder_activity::CoderActivityKind::ToolFailed
                && event.intent.as_deref() == Some("Change a path outside the allowed policy")
        }));

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

    #[tokio::test]
    async fn surface_requires_intent_before_invoking_domain_tool() {
        let fixture = fixture();
        let authority = authority(&fixture);
        let inner = Arc::new(RecordingRegistry::default());
        let registry = CoderBoundToolRegistry::new(
            inner.clone(),
            &authority,
            fixture.entry.clone(),
            fixture.policy.clone(),
        );

        let error = registry
            .invoke_tool(
                crate::coding_tools::COGNITION_CODE_READ,
                json!({ "path": "src/lib.rs" }),
            )
            .await
            .expect_err("missing intent");
        assert!(error.to_string().contains("intent is required"));
        assert!(inner.invoked_tools.lock().expect("tools lock").is_empty());
    }

    #[tokio::test]
    async fn round_context_reports_unseen_activity_and_fresh_repository_state_once() {
        let fixture = fixture();
        let authority = authority(&fixture);
        authority
            .shared_space_prompt_appendix()
            .expect("initial observation");
        let registry = CoderBoundToolRegistry::new(
            Arc::new(RecordingRegistry::default()),
            &authority,
            fixture.entry.clone(),
            fixture.policy.clone(),
        );

        registry
            .invoke_tool(
                crate::coding_tools::COGNITION_CODE_READ,
                json!({
                    "intent": "Inspect the implementation before changing it",
                    "path": "src/lib.rs"
                }),
            )
            .await
            .expect("read");
        std::fs::write(
            fixture.entry.worktree.join("src/lib.rs"),
            "pub fn demo() { println!(\"changed\"); }\n",
        )
        .expect("external worktree change");

        let context =
            super::super::turn_context::ToolRoundContextProvider::context_for_next_round(&registry)
                .expect("round context")
                .expect("new delta");
        assert!(context.contains("engineering_delta(.99)"));
        assert!(context.contains("Inspect the implementation before changing it"));
        assert!(context.contains("\"dirty\":true"));
        assert!(context.contains("src/lib.rs"));
        super::super::sttp::validate_canonical_sttp_node(&context).expect("canonical delta STTP");

        assert!(
            super::super::turn_context::ToolRoundContextProvider::context_for_next_round(&registry)
                .expect("second context")
                .is_none()
        );
    }
}
