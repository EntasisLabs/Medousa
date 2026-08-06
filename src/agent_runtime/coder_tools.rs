//! Least-authority tool surface for one Forge-fenced Coder turn.

use std::collections::HashSet;
use std::path::{Component, Path};
use std::sync::{Arc, Mutex as StdMutex, Weak};

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

use super::coder_activity::{CoderActivityStore, CoderAgentIdentity, CoderToolActivityAdmission};
use super::coder_claims::CoderClaimScope;
use super::coder_mode::CoderEntryContext;

const TURN_CONTROL_TOOLS: &[&str] = &[
    "cognition_turn_begin_work",
    "cognition_turn_update_user",
    "cognition_turn_checkpoint",
    "cognition_turn_finish",
    "cognition_turn_request_more_rounds",
    "cognition_turn_propose_mode",
];

pub const COGNITION_CODER_TOOLS_DISCOVER: &str = "cognition_coder_tools_discover";
pub const COGNITION_ENGINEERING_POINTERS: &str = "cognition_engineering_pointers";
pub const COGNITION_ENGINEERING_POINTER_FOLLOW: &str = "cognition_engineering_pointer_follow";
pub const COGNITION_ENGINEERING_HISTORY: &str = "cognition_engineering_history";
pub const COGNITION_CODER_EVIDENCE_READ: &str = "cognition_coder_evidence_read";

const GENERAL_MODE_RUNTIME_TOOLS: &[&str] = &[
    "cognition_job_enqueue",
    "cognition_grapheme_promote_to_job",
    "cognition_grapheme_promote_to_recurring",
    "cognition_grapheme_promote_last_run_to_recurring",
    "cognition_mcp_promote_to_job",
    "cognition_workshop_steer",
];

const CODER_PEER_SPAWN_TOOLS: &[&str] = &[
    "cognition_spawn_turn_worker",
    "cognition_turn_worker_status",
    "cognition_turn_worker_cancel",
];

const CODER_RUNTIME_TOOLS: &[&str] = &[
    COGNITION_CODER_TOOLS_DISCOVER,
    COGNITION_ENGINEERING_POINTERS,
    COGNITION_ENGINEERING_POINTER_FOLLOW,
    COGNITION_ENGINEERING_HISTORY,
    COGNITION_CODER_EVIDENCE_READ,
];

fn coder_tool_allowed(tool_name: &str, policy: &WorkPolicy) -> bool {
    let os_shell = matches!(
        tool_name,
        crate::shell_tools::COGNITION_SHELL_RUN | crate::shell_tools::COGNITION_SHELL_STATUS
    );
    let restricted_shell = (!policy.allowed_paths.is_empty() || !policy.denied_paths.is_empty())
        && matches!(
            tool_name,
            crate::coding_tools::COGNITION_SHELL_SESSION_STATUS
                | crate::coding_tools::COGNITION_SHELL_SESSION_RUN
                | crate::coding_tools::COGNITION_SHELL_SESSION_INTERRUPT
                | crate::coding_tools::COGNITION_CODER_SHELL_RUN
                | crate::coding_tools::COGNITION_CODER_SHELL_STATUS
        );
    !os_shell
        && !restricted_shell
        && !tool_name.starts_with("cognition_runtime_")
        && !GENERAL_MODE_RUNTIME_TOOLS.contains(&tool_name)
}

fn requires_coder_discovery(tool_name: &str) -> bool {
    crate::code_intelligence_tools::is_code_cognition_tool(tool_name)
        || crate::detamu_tools::is_detamu_cognition_tool(tool_name)
        || tool_name == COGNITION_ENGINEERING_HISTORY
}

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
        claims: Vec<CoderClaimScope>,
    ) -> std::result::Result<CoderToolActivityAdmission, String> {
        self.activity
            .begin_tool(
                &self.lease.work_id.to_string(),
                &self.identity,
                tool_name,
                intent,
                targets,
                claims,
            )
            .map_err(|err| {
                if serde_json::from_str::<Value>(&err).is_ok() {
                    err
                } else {
                    format!("cannot record Coder tool intent: {err}")
                }
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

struct ClaimHeartbeatGuard {
    task: tokio::task::JoinHandle<()>,
}

impl ClaimHeartbeatGuard {
    fn start(authority: &CoderTurnLease, call_id: &str) -> Self {
        let activity = authority.activity.clone();
        let work_id = authority.lease.work_id.to_string();
        let agent_id = authority.identity.agent_id.clone();
        let call_id = call_id.to_string();
        let task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
            loop {
                interval.tick().await;
                if activity
                    .heartbeat_claims(&work_id, &agent_id, &call_id)
                    .is_err()
                {
                    break;
                }
            }
        });
        Self { task }
    }
}

impl Drop for ClaimHeartbeatGuard {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl super::turn_context::ToolRoundContextProvider for CoderBoundToolRegistry {
    fn context_for_next_round(&self) -> Result<Option<String>> {
        let authority = self.authority()?;
        let Some(delta) = authority.engineering_delta()? else {
            return Ok(None);
        };
        let pointers = self.ranked_pointers(super::coder_pointers::MAX_AMBIENT_POINTERS)?;
        self.refresh_visible_from_pointers(&pointers)?;
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
            "ranked_engineering_pointers": pointers,
            "pointer_tools": {
                "follow": COGNITION_ENGINEERING_POINTER_FOLLOW,
                "history": COGNITION_ENGINEERING_HISTORY,
                "discover": COGNITION_CODER_TOOLS_DISCOVER,
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
    visible_tools: Arc<StdMutex<HashSet<String>>>,
    shell_sessions: Arc<Mutex<HashSet<String>>>,
}

impl CoderBoundToolRegistry {
    pub fn new(
        inner: Arc<dyn ToolRegistry>,
        authority: &Arc<CoderTurnLease>,
        entry: Arc<CoderEntryContext>,
        policy: WorkPolicy,
    ) -> Self {
        let mut visible_tools = crate::coding_tools::CODING_COGNITION_TOOLS
            .iter()
            .chain(TURN_CONTROL_TOOLS.iter())
            .chain(CODER_PEER_SPAWN_TOOLS.iter())
            .chain(
                [
                    COGNITION_CODER_TOOLS_DISCOVER,
                    COGNITION_ENGINEERING_POINTERS,
                    COGNITION_ENGINEERING_POINTER_FOLLOW,
                    COGNITION_CODER_EVIDENCE_READ,
                ]
                .iter(),
            )
            .map(|name| (*name).to_string())
            .filter(|name| coder_tool_allowed(name, &policy))
            .collect::<HashSet<_>>();
        if entry.editor.active_path.is_some() || entry.editor.containing_symbol.is_some() {
            visible_tools.extend(
                crate::code_intelligence_tools::CODE_COGNITION_TOOLS
                    .iter()
                    .map(|name| (*name).to_string()),
            );
        }
        Self {
            inner,
            authority: Arc::downgrade(authority),
            entry,
            policy,
            visible_tools: Arc::new(StdMutex::new(visible_tools)),
            shell_sessions: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    fn authority(&self) -> Result<Arc<CoderTurnLease>> {
        self.authority
            .upgrade()
            .ok_or_else(|| StasisError::PortFailure("Coder turn authority has expired".to_string()))
    }

    pub fn initial_prompt_appendix(&self) -> Result<String> {
        let shared = self.authority()?.shared_space_prompt_appendix()?;
        let pointers = self.ranked_pointers(super::coder_pointers::MAX_AMBIENT_POINTERS)?;
        self.refresh_visible_from_pointers(&pointers)?;
        Ok(format!(
            "{shared}\n\n{}",
            super::coder_pointers::engineering_pointer_prompt_appendix(&pointers)
        ))
    }

    pub fn undertaking_id(&self) -> &str {
        &self.entry.work_id
    }

    fn engineering_events(&self) -> Result<Vec<super::coder_activity::CoderActivityEvent>> {
        let authority = self.authority()?;
        authority
            .activity
            .events_for_work(&self.entry.work_id)
            .map_err(|err| StasisError::PortFailure(format!("cannot read Coder activity: {err}")))
    }

    fn focus_targets(&self) -> Vec<String> {
        let mut targets = Vec::new();
        if let Some(path) = self.entry.editor.active_path.as_deref() {
            targets.push(format!("file://{path}"));
        }
        if let Some(symbol) = self.entry.editor.containing_symbol.as_deref() {
            targets.push(symbol.to_string());
        }
        targets
    }

    fn ranked_pointers(
        &self,
        limit: usize,
    ) -> Result<Vec<super::coder_pointers::CoderEngineeringPointer>> {
        let authority = self.authority()?;
        Ok(super::coder_pointers::rank_engineering_pointers(
            &self.engineering_events()?,
            &authority.identity.agent_id,
            &self.focus_targets(),
            limit,
        ))
    }

    fn unlock_domain(&self, domain: &str) -> Result<Vec<String>> {
        let names: Vec<&str> = match domain {
            "intelligence" => crate::code_intelligence_tools::CODE_COGNITION_TOOLS.to_vec(),
            "world_model" => crate::detamu_tools::DETAMU_COGNITION_TOOLS.to_vec(),
            "history" => vec![COGNITION_ENGINEERING_HISTORY],
            "all" => crate::code_intelligence_tools::CODE_COGNITION_TOOLS
                .iter()
                .chain(crate::detamu_tools::DETAMU_COGNITION_TOOLS.iter())
                .copied()
                .chain(std::iter::once(COGNITION_ENGINEERING_HISTORY))
                .collect(),
            _ => {
                return Err(StasisError::PortFailure(format!(
                    "unknown Coder tool domain '{domain}'; expected intelligence, world_model, history, or all"
                )));
            }
        };
        let mut visible = self.visible_tools.lock().map_err(|err| {
            StasisError::PortFailure(format!("Coder visible tool lock poisoned: {err}"))
        })?;
        let mut unlocked = Vec::new();
        for name in names {
            if coder_tool_allowed(name, &self.policy) && visible.insert(name.to_string()) {
                unlocked.push(name.to_string());
            }
        }
        Ok(unlocked)
    }

    fn refresh_visible_from_pointers(
        &self,
        pointers: &[super::coder_pointers::CoderEngineeringPointer],
    ) -> Result<()> {
        let needs_intelligence = pointers.iter().any(|pointer| {
            matches!(
                pointer.kind,
                super::coder_pointers::CoderPointerKind::Symbol
                    | super::coder_pointers::CoderPointerKind::DiagnosticSet
            )
        });
        if needs_intelligence {
            let _ = self.unlock_domain("intelligence")?;
        }
        Ok(())
    }

    fn invoke_runtime_tool(&self, tool_name: &str, input: &Value) -> Result<Value> {
        match tool_name {
            COGNITION_CODER_TOOLS_DISCOVER => {
                let domain = input
                    .get("domain")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| StasisError::PortFailure("domain is required".into()))?;
                let unlocked = self.unlock_domain(domain)?;
                Ok(json!({
                    "ok": true,
                    "domain": domain,
                    "newly_visible": unlocked,
                    "available_domains": ["intelligence", "world_model", "history"],
                }))
            }
            COGNITION_ENGINEERING_POINTERS => {
                let limit = input
                    .get("limit")
                    .and_then(Value::as_u64)
                    .map(|value| value as usize)
                    .unwrap_or(12)
                    .clamp(1, 24);
                let pointers = self.ranked_pointers(limit)?;
                self.refresh_visible_from_pointers(&pointers)?;
                Ok(json!({ "ok": true, "count": pointers.len(), "pointers": pointers }))
            }
            COGNITION_ENGINEERING_POINTER_FOLLOW => {
                let pointer_id = input
                    .get("pointer_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| StasisError::PortFailure("pointer_id is required".into()))?;
                let detail = super::coder_pointers::follow_engineering_pointer(
                    &self.engineering_events()?,
                    pointer_id,
                )
                .map_err(StasisError::PortFailure)?;
                Ok(json!({ "ok": true, "pointer": detail }))
            }
            COGNITION_ENGINEERING_HISTORY => {
                let query = super::coder_pointers::CoderHistoryQuery {
                    before_revision: input.get("before_revision").and_then(Value::as_u64),
                    tool: input.get("tool").and_then(Value::as_str),
                    agent_id: input.get("agent_id").and_then(Value::as_str),
                    target: input.get("target").and_then(Value::as_str),
                    failed_only: input
                        .get("failed_only")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    limit: input
                        .get("limit")
                        .and_then(Value::as_u64)
                        .map(|value| value as usize)
                        .unwrap_or(20),
                };
                let events =
                    super::coder_pointers::engineering_history(&self.engineering_events()?, &query);
                let next_before_revision = events.last().map(|event| event.revision);
                Ok(json!({
                    "ok": true,
                    "count": events.len(),
                    "events": events,
                    "next_before_revision": next_before_revision,
                }))
            }
            COGNITION_CODER_EVIDENCE_READ => {
                let reference = input
                    .get("reference")
                    .and_then(Value::as_str)
                    .ok_or_else(|| StasisError::PortFailure("reference is required".into()))?;
                let offset = input
                    .get("offset")
                    .and_then(Value::as_u64)
                    .and_then(|value| usize::try_from(value).ok())
                    .unwrap_or(0);
                let max_bytes = input
                    .get("max_bytes")
                    .and_then(Value::as_u64)
                    .and_then(|value| usize::try_from(value).ok())
                    .unwrap_or(32 * 1024);
                let read = super::coder_evidence::CoderEvidenceStore::for_data_root(
                    &crate::paths::medousa_data_dir(),
                )
                .read_range(&self.entry.work_id, reference, offset, max_bytes)
                .map_err(StasisError::PortFailure)?;
                Ok(json!({
                    "ok": true,
                    "evidence": read,
                    "next_decision": if read.next_offset.is_some() {
                        "Use evidence.next_offset only if the remaining payload is necessary."
                    } else {
                        "This evidence object has been read through its end."
                    },
                }))
            }
            _ => Err(StasisError::PortFailure(format!(
                "unknown Coder runtime tool: {tool_name}"
            ))),
        }
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
                crate::coding_tools::COGNITION_CODER_SHELL_STATUS
                | crate::coding_tools::COGNITION_CODER_SHELL_RUN => {
                    self.bind_work_and_lease(map)?;
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
                | crate::coding_tools::COGNITION_CODER_SHELL_RUN
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
                | crate::coding_tools::COGNITION_CODER_SHELL_RUN
                | crate::coding_tools::COGNITION_CODER_SHELL_STATUS
        ) && let Some(session_id) = output.get("session_id").and_then(Value::as_str)
        {
            self.shell_sessions
                .lock()
                .await
                .insert(session_id.to_string());
        }
    }

    async fn prefer_turn_shell_session(&self, tool_name: &str, input: &mut Value) {
        if tool_name != crate::coding_tools::COGNITION_CODER_SHELL_RUN {
            return;
        }
        if input
            .get("session_id")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.trim().is_empty())
        {
            return;
        }
        let Some(session_id) = self.shell_sessions.lock().await.iter().next().cloned() else {
            return;
        };
        if let Some(map) = input.as_object_mut() {
            map.insert("session_id".into(), Value::String(session_id));
        }
    }
}

impl super::coder_evidence::CompactEvidenceReceiptSink for CoderBoundToolRegistry {
    fn stage_compact_receipt(
        &self,
        source_tool: &str,
        source_call_id: Option<&str>,
        receipt: &super::coder_evidence::CoderEvidenceReceipt,
    ) -> std::result::Result<(), String> {
        let authority = self.authority().map_err(|err| err.to_string())?;
        let line = json!({
            "kind": "medousa_coder_ephemeral_evidence_receipt",
            "schema_version": 1,
            "work_id": self.entry.work_id,
            "source_tool": source_tool,
            "source_call_id": source_call_id,
            "digest": receipt.digest,
            "ephemeral_reference": receipt.reference,
            "content_type": receipt.content_type,
            "logical_bytes": receipt.logical_bytes,
            "physical_bytes": receipt.physical_bytes,
            "retention": receipt.retention,
            "expires_at_unix_seconds": receipt.expires_at_unix_seconds,
            "redacted": receipt.redacted,
            "raw_promoted": false,
            "recorded_at": chrono::Utc::now(),
        });
        authority
            .forge
            .append_command_log(authority.lease(), &line)
            .map_err(|err| format!("failed to stage compact evidence receipt: {err}"))
    }
}

#[async_trait]
impl ToolRegistry for CoderBoundToolRegistry {
    async fn list_tools(&self) -> Result<Vec<Tool>> {
        let _authority = self.authority()?;
        let visible = self
            .visible_tools
            .lock()
            .map_err(|err| {
                StasisError::PortFailure(format!("Coder visible tool lock poisoned: {err}"))
            })?
            .clone();
        let mut tools = self.inner.list_tools().await?;
        tools.extend(coder_runtime_tool_definitions());
        Ok(tools
            .into_iter()
            .filter(|tool| {
                coder_tool_allowed(tool.name.as_str(), &self.policy)
                    && (!requires_coder_discovery(tool.name.as_str())
                        || visible.contains(tool.name.as_str()))
            })
            .map(|tool| with_coder_tool_advertisement(with_required_coder_intent(tool)))
            .collect())
    }

    async fn invoke_tool(&self, tool_name: &str, input: Value) -> Result<Value> {
        if !coder_tool_allowed(tool_name, &self.policy) {
            return Err(StasisError::PortFailure(format!(
                "tool is outside the Coder mode contract: {tool_name}"
            )));
        }
        let visible = !requires_coder_discovery(tool_name)
            || self
                .visible_tools
                .lock()
                .map_err(|err| {
                    StasisError::PortFailure(format!("Coder visible tool lock poisoned: {err}"))
                })?
                .contains(tool_name);
        let authority = self.authority()?;
        authority.heartbeat()?;
        let spawn_intent_hint = input
            .get("intent")
            .and_then(Value::as_str)
            .and_then(crate::agent_runtime::turn_worker::TurnWorkerIntent::parse);
        let (intent, input) = take_coder_intent(input)?;
        let targets = tool_targets(tool_name, &input, authority.lease());
        let claims = super::coder_claims::infer_tool_claims(
            tool_name,
            &input,
            authority.lease(),
            &self.entry.worktree,
        );
        let admission =
            match authority.begin_tool_activity(tool_name, &intent, targets.clone(), claims) {
                Ok(admission) => admission,
                Err(error) => {
                    if let Ok(conflict) = serde_json::from_str::<Value>(&error) {
                        authority.append_receipt(json!({
                            "kind": "medousa_coder_tool",
                            "call_id": conflict.get("call_id"),
                            "tool": tool_name,
                            "intent": intent,
                            "ok": false,
                            "detail": conflict,
                        }));
                        return Ok(conflict);
                    }
                    return Err(StasisError::PortFailure(error));
                }
            };
        let call_id = admission.call_id;
        let _claim_heartbeat = ClaimHeartbeatGuard::start(&authority, &call_id);
        if !visible {
            let err = StasisError::PortFailure(format!(
                "Coder tool is authorized but not visible; unlock its domain with {COGNITION_CODER_TOOLS_DISCOVER}: {tool_name}"
            ));
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
        let mut input = input;
        self.prefer_turn_shell_session(tool_name, &mut input).await;
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
        let result = if CODER_RUNTIME_TOOLS.contains(&tool_name) {
            self.invoke_runtime_tool(tool_name, &input)
        } else if crate::turn_control_tools::is_begin_work_tool_name(tool_name) {
            match remap_begin_work_to_spawn_input(&input, spawn_intent_hint) {
                Ok(spawn_input) => {
                    self.inner
                        .invoke_tool(
                            crate::agent_runtime::turn_worker_tools::COGNITION_SPAWN_TURN_WORKER,
                            spawn_input,
                        )
                        .await
                }
                Err(err) => Err(err),
            }
        } else if tool_name
            == crate::agent_runtime::turn_worker_tools::COGNITION_SPAWN_TURN_WORKER
        {
            let mut spawn_input = input.clone();
            ensure_spawn_worker_intent(&mut spawn_input, spawn_intent_hint);
            self.inner.invoke_tool(tool_name, spawn_input).await
        } else {
            self.inner.invoke_tool(tool_name, input.clone()).await
        };
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

fn coder_runtime_tool_definitions() -> Vec<Tool> {
    vec![
        Tool::new(COGNITION_CODER_TOOLS_DISCOVER)
            .with_description(
                "Reveal an already-authorized Coder tool domain for this turn. Domains: intelligence, world_model, history, all.",
            )
            .with_schema(json!({
                "type": "object",
                "properties": {
                    "domain": {
                        "type": "string",
                        "enum": ["intelligence", "world_model", "history", "all"]
                    }
                },
                "required": ["domain"]
            })),
        Tool::new(COGNITION_ENGINEERING_POINTERS)
            .with_description(
                "List ranked engineering pointers for this undertaking without replaying full history.",
            )
            .with_schema(json!({
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "minimum": 1, "maximum": 24 }
                }
            })),
        Tool::new(COGNITION_ENGINEERING_POINTER_FOLLOW)
            .with_description(
                "Resolve one engineering pointer into its bounded causal lifecycle and evidence receipt.",
            )
            .with_schema(json!({
                "type": "object",
                "properties": {
                    "pointer_id": { "type": "string" }
                },
                "required": ["pointer_id"]
            })),
        Tool::new(COGNITION_ENGINEERING_HISTORY)
            .with_description(
                "Query bounded undertaking activity history by revision, tool, agent, target, or failed status.",
            )
            .with_schema(json!({
                "type": "object",
                "properties": {
                    "before_revision": { "type": "integer", "minimum": 1 },
                    "tool": { "type": "string" },
                    "agent_id": { "type": "string" },
                    "target": { "type": "string" },
                    "failed_only": { "type": "boolean" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 50 }
                }
            })),
        Tool::new(COGNITION_CODER_EVIDENCE_READ)
            .with_description(
                "Read one bounded byte range from a redacted ephemeral evidence receipt scoped to this undertaking.",
            )
            .with_schema(json!({
                "type": "object",
                "properties": {
                    "reference": {
                        "type": "string",
                        "description": "coder-evidence reference returned by a bounded tool observation"
                    },
                    "offset": { "type": "integer", "minimum": 0 },
                    "max_bytes": { "type": "integer", "minimum": 1, "maximum": 32768 }
                },
                "required": ["reference"]
            })),
    ]
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

fn with_coder_tool_advertisement(tool: Tool) -> Tool {
    match tool.name.as_str() {
        name if crate::turn_control_tools::is_begin_work_tool_name(name) => tool.with_description(
            "Spawn a peer sub-agent for parallel work on this undertaking; does not leave Coder / does not enter Chat workshop.",
        ),
        "cognition_spawn_turn_worker" => tool.with_description(
            "Spawn a peer sub-agent for parallel research or side tasks while Coder stays on the Forge lease.",
        ),
        "cognition_turn_worker_status" => {
            tool.with_description("Check status of peer sub-agents spawned from this Coder turn.")
        }
        "cognition_turn_worker_cancel" => {
            tool.with_description("Cancel a peer sub-agent spawned from this Coder turn.")
        }
        _ => tool,
    }
}

/// Map Coder `cognition_turn_begin_work` args onto `cognition_spawn_turn_worker`.
pub(crate) fn remap_begin_work_to_spawn_input(
    input: &Value,
    worker_intent_hint: Option<crate::agent_runtime::turn_worker::TurnWorkerIntent>,
) -> Result<Value> {
    let goal = input
        .get("goal")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let message = input
        .get("message")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let (task, user_ack) = match (goal, message) {
        (Some(goal), Some(message)) => (goal.to_string(), message.to_string()),
        (Some(goal), None) => (goal.to_string(), goal.to_string()),
        (None, Some(message)) => (message.to_string(), message.to_string()),
        (None, None) => {
            return Err(StasisError::PortFailure(
                "cognition_turn_begin_work: goal or message is required to spawn a peer sub-agent"
                    .into(),
            ));
        }
    };
    let intent = worker_intent_hint
        .map(|value| value.as_str().to_string())
        .unwrap_or_else(|| default_peer_spawn_intent(&task, &user_ack));
    let mut out = json!({
        "task": task,
        "user_ack": user_ack,
        "intent": intent,
    });
    let map = out.as_object_mut().expect("spawn remap object");
    for key in ["manuscript_id", "stage_role", "model_hint"] {
        if let Some(value) = input.get(key).cloned()
            && !value.is_null()
        {
            map.insert(key.to_string(), value);
        }
    }
    Ok(out)
}

fn default_peer_spawn_intent(task: &str, user_ack: &str) -> String {
    let hay = format!("{task}\n{user_ack}").to_ascii_lowercase();
    if hay.contains("research") || hay.contains("investigate") || hay.contains("survey") {
        "research".into()
    } else {
        "general".into()
    }
}

fn ensure_spawn_worker_intent(
    input: &mut Value,
    hint: Option<crate::agent_runtime::turn_worker::TurnWorkerIntent>,
) {
    let task = input
        .get("task")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let user_ack = input
        .get("user_ack")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let intent = hint
        .map(|value| value.as_str().to_string())
        .unwrap_or_else(|| default_peer_spawn_intent(task, user_ack));
    if let Some(map) = input.as_object_mut() {
        map.insert("intent".into(), Value::String(intent));
    }
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
    if let Some(pointer_id) = input.get("pointer_id").and_then(Value::as_str) {
        targets.push(pointer_id.trim().to_string());
    }
    if let Some(reference) = input.get("reference").and_then(Value::as_str) {
        targets.push(reference.trim().to_string());
    }
    if tool_name == COGNITION_ENGINEERING_HISTORY {
        targets.push(format!("work://{}/history", lease.work_id));
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
                Tool::new(crate::coding_tools::COGNITION_CODER_SHELL_RUN),
                Tool::new(crate::coding_tools::COGNITION_CODER_SHELL_STATUS),
                Tool::new(crate::code_intelligence_tools::COGNITION_CODE_DIAGNOSTICS),
                Tool::new(crate::detamu_tools::COGNITION_DETAMU_STATUS),
                Tool::new("cognition_vault_write"),
                Tool::new("cognition_memory_recall"),
                Tool::new("cognition_memory_store"),
                Tool::new("cognition_web_search"),
                Tool::new("cognition_mcp_discover"),
                Tool::new("cognition_mcp_invoke"),
                Tool::new("cognition_runtime_jobs_cancel"),
                Tool::new("cognition_spawn_turn_worker")
                    .with_description("Host spawn worker"),
                Tool::new("cognition_turn_begin_work")
                    .with_description("Enter bound Workshop"),
                Tool::new("cognition_turn_worker_status"),
                Tool::new("cognition_shell_run"),
                Tool::new("cognition_shell_status"),
            ])
        }

        async fn invoke_tool(&self, tool_name: &str, input: Value) -> Result<Value> {
            *self.last_input.lock().expect("input lock") = Some(input.clone());
            self.invoked_tools
                .lock()
                .expect("tools lock")
                .push(tool_name.to_string());
            if tool_name == crate::coding_tools::COGNITION_SHELL_SESSION_STATUS
                || tool_name == crate::coding_tools::COGNITION_CODER_SHELL_RUN
                || tool_name == crate::coding_tools::COGNITION_CODER_SHELL_STATUS
            {
                Ok(json!({ "ok": true, "session_id": "shell-1", "input": input }))
            } else if tool_name == "cognition_spawn_turn_worker" {
                Ok(json!({
                    "ok": true,
                    "worker_spawned": true,
                    "input": input
                }))
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
        authority_named(fixture, "test-session", 1)
    }

    fn authority_named(fixture: &Fixture, session_id: &str, turn_id: u64) -> Arc<CoderTurnLease> {
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
            CoderAgentIdentity::for_turn(session_id, turn_id, &lease.attempt_id.to_string());
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
    async fn hazardous_inferred_claim_blocks_peer_before_domain_tool_invocation() {
        let fixture = fixture();
        let authority_a = authority_named(&fixture, "session-a", 1);
        let authority_b = authority_named(&fixture, "session-b", 2);
        let inner_a = Arc::new(RecordingRegistry::default());
        let inner_b = Arc::new(RecordingRegistry::default());
        let registry_a = CoderBoundToolRegistry::new(
            inner_a.clone(),
            &authority_a,
            fixture.entry.clone(),
            fixture.policy.clone(),
        );
        let registry_b = CoderBoundToolRegistry::new(
            inner_b.clone(),
            &authority_b,
            fixture.entry.clone(),
            fixture.policy.clone(),
        );

        registry_a
            .invoke_tool(
                crate::coding_tools::COGNITION_CODE_APPLY_PATCH,
                json!({
                    "intent": "Regenerate the Rust dependency lockfile",
                    "path": "Cargo.lock",
                    "expected_sha256": "missing",
                    "content": "version = 4\n"
                }),
            )
            .await
            .expect("first lockfile claim");
        let conflict = registry_b
            .invoke_tool(
                crate::coding_tools::COGNITION_CODE_APPLY_PATCH,
                json!({
                    "intent": "Update the same dependency lockfile",
                    "path": "Cargo.lock",
                    "expected_sha256": "missing",
                    "content": "version = 4\n"
                }),
            )
            .await
            .expect("structured hazardous conflict receipt");
        assert_eq!(conflict["ok"], false);
        assert_eq!(conflict["code"], "coder_claim_conflict");
        assert!(conflict.to_string().contains("session-a"));
        assert!(inner_b.invoked_tools.lock().expect("tools lock").is_empty());
        assert_eq!(
            inner_a.invoked_tools.lock().expect("tools lock").as_slice(),
            [crate::coding_tools::COGNITION_CODE_APPLY_PATCH]
        );
    }

    #[tokio::test]
    async fn surface_inherits_existing_tools_except_runtime_controls() {
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
        assert!(
            tools
                .iter()
                .any(|tool| tool.name.as_str() == crate::coding_tools::COGNITION_CODE_READ)
        );
        assert!(tools.iter().any(|tool| {
            tool.name.as_str() == crate::coding_tools::COGNITION_SHELL_SESSION_STATUS
        }));
        assert!(
            tools
                .iter()
                .any(|tool| tool.name.as_str() == COGNITION_ENGINEERING_POINTER_FOLLOW)
        );
        assert!(
            tools
                .iter()
                .any(|tool| tool.name.as_str() == COGNITION_CODER_EVIDENCE_READ)
        );
        for inherited in [
            "cognition_vault_write",
            "cognition_memory_recall",
            "cognition_memory_store",
            "cognition_web_search",
            "cognition_mcp_discover",
            "cognition_mcp_invoke",
        ] {
            assert!(
                tools.iter().any(|tool| tool.name.as_str() == inherited),
                "missing inherited Coder tool {inherited}"
            );
        }
        for runtime_control in ["cognition_runtime_jobs_cancel", "cognition_shell_run"] {
            assert!(
                tools
                    .iter()
                    .all(|tool| tool.name.as_str() != runtime_control),
                "runtime control leaked into Coder: {runtime_control}"
            );
        }
        assert!(
            tools
                .iter()
                .any(|tool| tool.name.as_str() == "cognition_spawn_turn_worker"),
            "peer spawn should be visible in Coder"
        );
        let begin_work = tools
            .iter()
            .find(|tool| tool.name.as_str() == "cognition_turn_begin_work")
            .expect("begin_work visible");
        assert!(
            begin_work
                .description
                .as_deref()
                .unwrap_or_default()
                .contains("peer sub-agent"),
            "Coder begin_work should advertise peer spawn"
        );
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
                "cognition_memory_recall",
                json!({
                    "intent": "Recall the user's established implementation preferences",
                    "query": "implementation preferences"
                }),
            )
            .await
            .expect("inherited memory tool");
        let input = inner
            .last_input
            .lock()
            .expect("input lock")
            .clone()
            .expect("recorded memory input");
        assert_eq!(input["query"], "implementation preferences");
        assert!(input.get("intent").is_none());

        let denied = registry
            .invoke_tool(
                "cognition_runtime_jobs_cancel",
                json!({ "intent": "Cancel a durable runtime job", "job_id": "job-1" }),
            )
            .await
            .expect_err("runtime control denied");
        assert!(
            denied
                .to_string()
                .contains("outside the Coder mode contract")
        );

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
        assert!(context.contains("engineering:call:"));
        assert!(context.contains(COGNITION_ENGINEERING_POINTER_FOLLOW));
        super::super::sttp::validate_canonical_sttp_node(&context).expect("canonical delta STTP");

        assert!(
            super::super::turn_context::ToolRoundContextProvider::context_for_next_round(&registry)
                .expect("second context")
                .is_none()
        );
    }

    #[tokio::test]
    async fn discovery_reveals_only_authorized_coder_domains_between_rounds() {
        let fixture = fixture();
        let authority = authority(&fixture);
        let registry = CoderBoundToolRegistry::new(
            Arc::new(RecordingRegistry::default()),
            &authority,
            fixture.entry.clone(),
            fixture.policy.clone(),
        );
        let initial = registry.list_tools().await.expect("initial tools");
        assert!(
            initial
                .iter()
                .any(|tool| tool.name.as_str() == COGNITION_CODER_TOOLS_DISCOVER)
        );
        assert!(
            initial
                .iter()
                .any(|tool| tool.name.as_str() == COGNITION_ENGINEERING_POINTERS)
        );
        assert!(
            !initial
                .iter()
                .any(|tool| tool.name.as_str() == COGNITION_ENGINEERING_HISTORY)
        );
        assert!(!initial.iter().any(|tool| {
            tool.name.as_str() == crate::code_intelligence_tools::COGNITION_CODE_DIAGNOSTICS
        }));

        let hidden = registry
            .invoke_tool(
                crate::code_intelligence_tools::COGNITION_CODE_DIAGNOSTICS,
                json!({
                    "intent": "Inspect current compiler diagnostics",
                    "uri": format!("file://{}", fixture.entry.worktree.join("src/lib.rs").display())
                }),
            )
            .await
            .expect_err("hidden tool denied");
        assert!(hidden.to_string().contains("not visible"));
        let hidden_events = fixture
            .activity
            .events_for_work(&fixture.entry.work_id)
            .expect("activity after hidden call");
        assert!(hidden_events.iter().any(|event| {
            event.kind == super::super::coder_activity::CoderActivityKind::ToolFailed
                && event.tool.as_deref()
                    == Some(crate::code_intelligence_tools::COGNITION_CODE_DIAGNOSTICS)
                && event.intent.as_deref() == Some("Inspect current compiler diagnostics")
        }));

        let discovered = registry
            .invoke_tool(
                COGNITION_CODER_TOOLS_DISCOVER,
                json!({
                    "intent": "Reveal code intelligence needed to inspect diagnostics",
                    "domain": "intelligence"
                }),
            )
            .await
            .expect("discover intelligence");
        assert!(
            discovered["newly_visible"]
                .as_array()
                .is_some_and(|tools| !tools.is_empty())
        );
        let after = registry.list_tools().await.expect("tools after discover");
        assert!(after.iter().any(|tool| {
            tool.name.as_str() == crate::code_intelligence_tools::COGNITION_CODE_DIAGNOSTICS
        }));
        assert!(
            after
                .iter()
                .any(|tool| tool.name.as_str() == "cognition_memory_recall")
        );
        assert!(
            after
                .iter()
                .all(|tool| tool.name.as_str() != "cognition_runtime_jobs_cancel")
        );
    }

    #[tokio::test]
    async fn engineering_history_is_bounded_and_unlocked_on_demand() {
        let fixture = fixture();
        let authority = authority(&fixture);
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
                    "intent": "Inspect the current implementation",
                    "path": "src/lib.rs"
                }),
            )
            .await
            .expect("read");
        registry
            .invoke_tool(
                COGNITION_CODER_TOOLS_DISCOVER,
                json!({
                    "intent": "Open bounded engineering history for causal review",
                    "domain": "history"
                }),
            )
            .await
            .expect("discover history");
        let history = registry
            .invoke_tool(
                COGNITION_ENGINEERING_HISTORY,
                json!({
                    "intent": "Review the latest read lifecycle without replaying the transcript",
                    "tool": "code_read",
                    "limit": 2
                }),
            )
            .await
            .expect("history");
        assert_eq!(history["count"], 2);
        assert!(history["events"].as_array().is_some_and(|events| {
            events
                .iter()
                .all(|event| event["tool"] == crate::coding_tools::COGNITION_CODE_READ)
        }));
    }

    #[test]
    fn coder_tool_allowed_denies_os_shell_and_allows_coder_shell() {
        let policy = WorkPolicy::default();
        assert!(!coder_tool_allowed(
            crate::shell_tools::COGNITION_SHELL_RUN,
            &policy
        ));
        assert!(!coder_tool_allowed(
            crate::shell_tools::COGNITION_SHELL_STATUS,
            &policy
        ));
        assert!(coder_tool_allowed(
            crate::coding_tools::COGNITION_CODER_SHELL_RUN,
            &policy
        ));
        assert!(coder_tool_allowed(
            crate::coding_tools::COGNITION_CODER_SHELL_STATUS,
            &policy
        ));
        assert!(coder_tool_allowed("cognition_spawn_turn_worker", &policy));
        assert!(!coder_tool_allowed("cognition_workshop_steer", &policy));
    }

    #[test]
    fn begin_work_remap_builds_spawn_args() {
        let mapped = remap_begin_work_to_spawn_input(
            &json!({
                "goal": "Survey related crates for the bug",
                "message": "Researching dependency graph"
            }),
            None,
        )
        .expect("remap");
        assert_eq!(mapped["task"], "Survey related crates for the bug");
        assert_eq!(mapped["user_ack"], "Researching dependency graph");
        assert_eq!(mapped["intent"], "research");

        let goal_only = remap_begin_work_to_spawn_input(
            &json!({ "goal": "Write a focused unit test" }),
            None,
        )
        .expect("goal only");
        assert_eq!(goal_only["task"], "Write a focused unit test");
        assert_eq!(goal_only["user_ack"], "Write a focused unit test");
        assert_eq!(goal_only["intent"], "general");

        let hinted = remap_begin_work_to_spawn_input(
            &json!({ "message": "Dig into memory nodes" }),
            Some(crate::agent_runtime::turn_worker::TurnWorkerIntent::MemoryContext),
        )
        .expect("hinted");
        assert_eq!(hinted["intent"], "memory.context");
        assert_eq!(hinted["task"], "Dig into memory nodes");
    }

    #[tokio::test]
    async fn begin_work_invokes_spawn_worker() {
        let fixture = fixture();
        let authority = authority(&fixture);
        let inner = Arc::new(RecordingRegistry::default());
        let registry = CoderBoundToolRegistry::new(
            inner.clone(),
            &authority,
            fixture.entry.clone(),
            fixture.policy.clone(),
        );
        let out = registry
            .invoke_tool(
                "cognition_turn_begin_work",
                json!({
                    "intent": "Delegate parallel research",
                    "goal": "Investigate failing CI flakes",
                    "message": "Spinning a research peer"
                }),
            )
            .await
            .expect("begin_work remapped");
        assert_eq!(out["worker_spawned"], true);
        assert_eq!(
            inner.invoked_tools.lock().expect("tools").as_slice(),
            ["cognition_spawn_turn_worker"]
        );
        let input = inner.last_input.lock().expect("input").clone().expect("input");
        assert_eq!(input["task"], "Investigate failing CI flakes");
        assert_eq!(input["user_ack"], "Spinning a research peer");
        assert_eq!(input["intent"], "research");
    }

    #[tokio::test]
    async fn coder_shell_bind_forces_work_and_lease() {
        let fixture = fixture();
        let authority = authority(&fixture);
        let inner = Arc::new(RecordingRegistry::default());
        let registry = CoderBoundToolRegistry::new(
            inner.clone(),
            &authority,
            fixture.entry.clone(),
            fixture.policy.clone(),
        );
        registry
            .invoke_tool(
                crate::coding_tools::COGNITION_CODER_SHELL_RUN,
                json!({
                    "intent": "Run a quick check in the undertaking Terminal",
                    "command": "pwd"
                }),
            )
            .await
            .expect("coder shell");
        let input = inner.last_input.lock().expect("input").clone().expect("input");
        assert_eq!(input["work_id"], fixture.entry.work_id);
        assert_eq!(
            input["lease_id"],
            authority.lease().lease_id.to_string()
        );
        assert_eq!(input["attempt_id"], authority.lease().attempt_id.to_string());

        let err = registry
            .bind_input(
                crate::coding_tools::COGNITION_CODER_SHELL_RUN,
                json!({
                    "command": "pwd",
                    "work_id": "work-not-this-one"
                }),
            )
            .expect_err("mismatched work_id");
        assert!(err.to_string().contains("work_id"));
    }

    #[tokio::test]
    async fn coder_shell_reuses_turn_owned_session() {
        let fixture = fixture();
        let authority = authority(&fixture);
        let inner = Arc::new(RecordingRegistry::default());
        let registry = CoderBoundToolRegistry::new(
            inner.clone(),
            &authority,
            fixture.entry.clone(),
            fixture.policy.clone(),
        );
        registry
            .invoke_tool(
                crate::coding_tools::COGNITION_CODER_SHELL_RUN,
                json!({
                    "intent": "First one-shot creates a session",
                    "command": "echo one"
                }),
            )
            .await
            .expect("first shell");
        registry
            .invoke_tool(
                crate::coding_tools::COGNITION_CODER_SHELL_RUN,
                json!({
                    "intent": "Second one-shot reuses the session",
                    "command": "echo two"
                }),
            )
            .await
            .expect("second shell");
        let input = inner.last_input.lock().expect("input").clone().expect("input");
        assert_eq!(input["session_id"], "shell-1");
        assert_eq!(input["command"], "echo two");
    }

    #[tokio::test]
    async fn os_shell_is_rejected_in_coder() {
        let fixture = fixture();
        let authority = authority(&fixture);
        let registry = CoderBoundToolRegistry::new(
            Arc::new(RecordingRegistry::default()),
            &authority,
            fixture.entry.clone(),
            fixture.policy.clone(),
        );
        let err = registry
            .invoke_tool(
                crate::shell_tools::COGNITION_SHELL_RUN,
                json!({
                    "intent": "Try unbound OS shell",
                    "command": "pwd"
                }),
            )
            .await
            .expect_err("os shell denied");
        assert!(err.to_string().contains("outside the Coder mode contract"));
    }
}
