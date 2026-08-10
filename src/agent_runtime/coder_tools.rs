//! Least-authority tool surface for one Forge-fenced Coder turn.

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path};
use std::sync::{Arc, Mutex as StdMutex, Weak};
use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt;
use genai::chat::Tool;
use medousa_forge::forge::Forge;
use medousa_forge::model::{
    ActorRef, ChangeStatus, ChangedFile, ExecutionLease, RecoveryDisposition, ReviewDecisionId,
    WorkItem, WorkPolicy, WorkState,
};
use once_cell::sync::Lazy;
use schemars::JsonSchema;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use stasis::application::orchestration::tool_registry::ToolRegistry;
use stasis::domain::errors::StasisError;
use stasis::prelude::Result;
use tokio::sync::Mutex;

use super::coder_activity::{CoderActivityStore, CoderAgentIdentity, CoderToolActivityAdmission};
use super::coder_claims::CoderClaimScope;
use super::coder_mode::CoderEntryContext;
use crate::typed_tools::{
    CompatOption, ModeToolAdapter, ToolCatalog, ToolDomainId, ToolExposureRef, ToolId,
    ToolPlacementIndex, ToolRegistrar,
};

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

const COGNITION_ENGINEERING_POINTERS_ID: ToolId = ToolId::new(COGNITION_ENGINEERING_POINTERS);
const ENGINEERING_POINTERS_DESCRIPTION: &str =
    "List ranked engineering pointers for this undertaking without replaying full history.";

#[derive(Debug, Clone, PartialEq, Eq, JsonSchema)]
#[schemars(transparent)]
struct CoderToolIntent(#[schemars(length(max = 320))] String);

impl CoderToolIntent {
    fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for CoderToolIntent {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        super::coder_activity::validate_intent(&raw)
            .map(Self)
            .map_err(D::Error::custom)
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
struct CoderCallMetadata {
    /// One short outcome-oriented sentence explaining why this tool call is being made (not private reasoning).
    #[schemars(length(max = 320))]
    intent: CoderToolIntent,
}

static CODER_MODE_ADAPTER: Lazy<ModeToolAdapter<CoderCallMetadata>> = Lazy::new(|| {
    ModeToolAdapter::new(crate::tool_catalog::CODER_MODE_ID)
        .expect("Coder mode metadata contract must normalize")
});

#[derive(Debug, Deserialize, JsonSchema)]
struct EngineeringPointersInput {
    #[serde(default)]
    #[schemars(
        with = "usize",
        range(min = 1, max = 24),
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    limit: CompatOption<usize>,
}

#[derive(Debug, Serialize, JsonSchema)]
struct EngineeringPointersOutput {
    ok: bool,
    count: usize,
    pointers: Vec<super::coder_pointers::CoderEngineeringPointer>,
}

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

const CODER_ADVANCED_MEMORY_TOOLS: &[&str] = &[
    "cognition_memory_schema",
    "cognition_memory_context",
    "cognition_memory_list",
    "cognition_memory_recall",
    "cognition_memory_store",
    "cognition_memory_tags",
];

const CODER_SCOPED_MEMORY_TOOLS: &[&str] = &[
    "cognition_memory_context",
    "cognition_memory_list",
    "cognition_memory_recall",
    "cognition_memory_store",
    "cognition_memory_tags",
];

const CODER_RESEARCH_TOOLS: &[&str] = &[
    "cognition_web_search",
    "cognition_browser_fetch",
    "cognition_browser_snapshot",
    "cognition_browser_act",
];

const CODER_CAPABILITY_TOOLS: &[&str] = &[
    "cognition_capability_search",
    "cognition_capability_resolve",
    "cognition_capability_invoke",
    "cognition_mcp_discover",
    "cognition_mcp_servers",
    "cognition_mcp_invoke",
    "cognition_grapheme_modules",
    "cognition_grapheme_modules_info",
    "cognition_grapheme_modules_ops",
    "cognition_grapheme_examples",
    "cognition_grapheme_run",
    "cognition_grapheme_cli_run",
    "cognition_grapheme_template_run",
];

const CODER_WORKSPACE_TOOLS: &[&str] = &[
    "cognition_vault_list",
    "cognition_vault_read",
    "cognition_vault_grep",
    "cognition_vault_search",
    "cognition_vault_write",
    "cognition_artifact_list",
    "cognition_artifact_read",
    "cognition_artifact_grep",
    "cognition_artifact_write",
];

const CODER_DISCOVERABLE_DOMAINS: &[&str] = &[
    "intelligence",
    "semantic_actions",
    "causal",
    "world_model",
    "experiments",
    "history",
    "memory",
    "research",
    "capabilities",
    "workspace",
];

const CODER_DISCOVERABLE_DOMAIN_IDS: &[ToolDomainId] = &[
    ToolDomainId::new("intelligence"),
    ToolDomainId::new("semantic_actions"),
    ToolDomainId::new("causal"),
    ToolDomainId::new("world_model"),
    ToolDomainId::new("experiments"),
    ToolDomainId::new("history"),
    ToolDomainId::new("memory"),
    ToolDomainId::new("research"),
    ToolDomainId::new("capabilities"),
    ToolDomainId::new("workspace"),
];

const CODER_RUNTIME_TOOLS: &[&str] = &[
    COGNITION_CODER_TOOLS_DISCOVER,
    COGNITION_ENGINEERING_POINTERS,
    COGNITION_ENGINEERING_POINTER_FOLLOW,
    COGNITION_ENGINEERING_HISTORY,
    COGNITION_CODER_EVIDENCE_READ,
];

const CODER_MEMORY_IO_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_MEMORY_FLUSH_WRITES_PER_PASS: usize = 4;
const MAX_ACCEPTED_PROMOTIONS_PER_KIND: usize = 4;
const MAX_ARCHIVED_MEMORY_ENVIRONMENTS: usize = 64;
const MAX_CONCURRENT_MEMORY_ARCHIVES: usize = 4;
const MAX_LIFECYCLE_RECONCILIATIONS_PER_PASS: usize = 8;
const MAX_CONCURRENT_MEMORY_RECONCILIATIONS: usize = 4;

static CODER_MEMORY_RETRY_QUEUES: Lazy<
    StdMutex<HashMap<String, Weak<Mutex<super::coder_memory::CoderMemoryRetryQueue>>>>,
> = Lazy::new(|| StdMutex::new(HashMap::new()));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CoderMemoryBoundary {
    Change,
    Verification,
    Handoff,
    Budget,
    Terminal,
}

struct CoderMemoryCheckpoint<'a> {
    boundary: CoderMemoryBoundary,
    tool_name: &'a str,
    intent: &'a str,
    call_id: &'a str,
    input: &'a Value,
    result: std::result::Result<&'a Value, &'a StasisError>,
}

impl CoderMemoryBoundary {
    fn memory_kind(self, succeeded: bool) -> &'static str {
        if !succeeded {
            return "open_gap";
        }
        match self {
            Self::Change => "change",
            Self::Verification => "verification",
            Self::Handoff => "handoff",
            Self::Budget | Self::Terminal => "checkpoint",
        }
    }

    fn summary_prefix(self, succeeded: bool) -> &'static str {
        match (self, succeeded) {
            (Self::Change, true) => "Applied change",
            (Self::Change, false) => "Change failed",
            (Self::Verification, true) => "Verification completed",
            (Self::Verification, false) => "Verification failed",
            (Self::Handoff, true) => "Handoff checkpoint",
            (Self::Handoff, false) => "Handoff failed",
            (Self::Budget, true) => "Budget checkpoint",
            (Self::Budget, false) => "Budget checkpoint failed",
            (Self::Terminal, true) => "Terminal checkpoint",
            (Self::Terminal, false) => "Terminal checkpoint failed",
        }
    }
}

fn automatic_memory_boundary(
    tool_name: &str,
    claims: &[CoderClaimScope],
) -> Option<CoderMemoryBoundary> {
    if tool_name == crate::coding_tools::COGNITION_CODE_APPLY_PATCH
        || tool_name == super::coder_semantic_actions::COGNITION_CODER_CHANGE_SET_APPLY
    {
        Some(CoderMemoryBoundary::Change)
    } else if tool_name == crate::code_intelligence_tools::COGNITION_CODE_DIAGNOSTICS
        || claims
            .iter()
            .any(|claim| claim.mode == super::coder_claims::CoderClaimMode::Verify)
    {
        Some(CoderMemoryBoundary::Verification)
    } else if crate::turn_control_tools::is_checkpoint_turn_tool_name(tool_name)
        || crate::turn_control_tools::is_begin_work_tool_name(tool_name)
        || tool_name == crate::agent_runtime::turn_worker_tools::COGNITION_SPAWN_TURN_WORKER
    {
        Some(CoderMemoryBoundary::Handoff)
    } else if crate::turn_control_tools::is_request_more_rounds_tool_name(tool_name) {
        Some(CoderMemoryBoundary::Budget)
    } else if crate::turn_control_tools::is_finish_turn_tool_name(tool_name) {
        Some(CoderMemoryBoundary::Terminal)
    } else {
        None
    }
}

fn coder_tool_allowed(tool_id: ToolId, policy: &WorkPolicy) -> bool {
    let tool_name = tool_id.as_str();
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

fn shared_memory_retry_queue(
    scope: &super::coder_memory::CoderMemoryScope,
) -> Arc<Mutex<super::coder_memory::CoderMemoryRetryQueue>> {
    let mut queues = CODER_MEMORY_RETRY_QUEUES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    queues.retain(|_, queue| queue.strong_count() > 0);
    if let Some(queue) = queues.get(&scope.session_id).and_then(Weak::upgrade) {
        return queue;
    }
    let queue = Arc::new(Mutex::new(
        super::coder_memory::CoderMemoryRetryQueue::for_scope(scope),
    ));
    queues.insert(scope.session_id.clone(), Arc::downgrade(&queue));
    queue
}

async fn invoke_locus_registry_tool(
    registry: &dyn ToolRegistry,
    tool_name: &str,
    input: Value,
) -> Result<Value> {
    tokio::time::timeout(
        CODER_MEMORY_IO_TIMEOUT,
        registry.invoke_tool(tool_name, input),
    )
    .await
    .map_err(|_| {
        StasisError::PortFailure(format!(
            "Coder memory operation '{tool_name}' timed out after {} seconds",
            CODER_MEMORY_IO_TIMEOUT.as_secs()
        ))
    })?
}

async fn try_store_memory_commit_with_registry(
    registry: &dyn ToolRegistry,
    scope: &super::coder_memory::CoderMemoryScope,
    commit: &super::coder_memory::CoderMemoryCommit,
) -> Result<Value> {
    let existing = invoke_locus_registry_tool(
        registry,
        "cognition_memory_list",
        json!({
            "session_id": scope.session_id,
            "semantic_tags": [commit.dedupe_tag],
            "limit": 1,
        }),
    )
    .await?;
    if let Some(node_id) = super::coder_memory::first_node_id(&existing) {
        return Ok(json!({
            "ok": true,
            "stored": false,
            "duplicate": true,
            "node_id": node_id,
            "kind": commit.kind,
            "summary": commit.summary,
            "scope": scope.public_descriptor(),
        }));
    }

    let stored = invoke_locus_registry_tool(
        registry,
        "cognition_memory_store",
        json!({
            "session_id": scope.session_id,
            "node": commit.raw_node,
            "semantic_tags": commit.semantic_tags,
        }),
    )
    .await?;
    let accepted = stored
        .get("stored")
        .and_then(Value::as_bool)
        .or_else(|| stored.get("valid").and_then(Value::as_bool))
        .unwrap_or(false);
    if !accepted {
        let validation_error = stored
            .get("validation_error")
            .and_then(Value::as_str)
            .unwrap_or("Locus did not accept the compiled node");
        return Err(StasisError::PortFailure(format!(
            "Coder memory store rejected a runtime-compiled node: {}",
            bounded_memory_error(validation_error)
        )));
    }
    Ok(json!({
        "ok": true,
        "stored": true,
        "duplicate": false,
        "node_id": stored.get("node_id"),
        "kind": commit.kind,
        "summary": commit.summary,
        "scope": scope.public_descriptor(),
    }))
}

async fn persist_lifecycle_commits(
    registry: &dyn ToolRegistry,
    scope: &super::coder_memory::CoderMemoryScope,
    commits: Vec<super::coder_memory::CoderMemoryCommit>,
) -> Result<Value> {
    let queue = shared_memory_retry_queue(scope);
    let mut queue = queue.lock().await;
    let mut flushed = 0usize;
    let mut memory_available = true;
    while flushed < MAX_MEMORY_FLUSH_WRITES_PER_PASS
        && let Some(pending) = queue.front()
    {
        match try_store_memory_commit_with_registry(registry, scope, &pending).await {
            Ok(_) => {
                queue.pop_front(&pending.dedupe_tag)?;
                flushed += 1;
            }
            Err(_) => {
                memory_available = false;
                break;
            }
        }
    }

    let mut writes_attempted = flushed;
    let mut stored = 0usize;
    let mut queued = 0usize;
    for commit in commits {
        if memory_available && writes_attempted < MAX_MEMORY_FLUSH_WRITES_PER_PASS {
            writes_attempted += 1;
            match try_store_memory_commit_with_registry(registry, scope, &commit).await {
                Ok(_) => {
                    stored += 1;
                    continue;
                }
                Err(error) => {
                    tracing::warn!(
                        error = %error,
                        memory_kind = %commit.kind,
                        "deferring Coder memory lifecycle write"
                    );
                    memory_available = false;
                }
            }
        }
        queue.enqueue(commit)?;
        queued += 1;
    }
    Ok(json!({
        "ok": true,
        "scope": scope.public_descriptor(),
        "flushed": flushed,
        "stored": stored,
        "queued": queued,
        "pending_writes": queue.len(),
    }))
}

async fn promote_memory_task(
    registry: &dyn ToolRegistry,
    task: &super::coder_memory::CoderMemoryPromotionTask,
) -> Result<Value> {
    let source_scope = task.source_scope();
    let source_flush = persist_lifecycle_commits(registry, &source_scope, Vec::new()).await?;
    if source_flush
        .get("pending_writes")
        .and_then(Value::as_u64)
        .unwrap_or_default()
        > 0
    {
        return Err(StasisError::PortFailure(
            "accepted memory promotion is waiting for its source queue to drain".into(),
        ));
    }
    let accepted_head_tag = format!("head:{}", task.accepted_head.trim());
    let (decisions, verifications) = tokio::join!(
        invoke_locus_registry_tool(
            registry,
            "cognition_memory_list",
            json!({
                "session_id": source_scope.session_id,
                "semantic_tags": ["kind:decision", accepted_head_tag.clone()],
                "limit": MAX_ACCEPTED_PROMOTIONS_PER_KIND,
            }),
        ),
        invoke_locus_registry_tool(
            registry,
            "cognition_memory_list",
            json!({
                "session_id": source_scope.session_id,
                "semantic_tags": ["kind:verification", accepted_head_tag],
                "limit": MAX_ACCEPTED_PROMOTIONS_PER_KIND,
            }),
        ),
    );
    let decisions = decisions?;
    let verifications = verifications?;
    let decisions = super::coder_memory::promotion_candidates(
        &decisions,
        &task.accepted_head,
        "decision",
        MAX_ACCEPTED_PROMOTIONS_PER_KIND,
    );
    let verifications = super::coder_memory::promotion_candidates(
        &verifications,
        &task.accepted_head,
        "verification",
        MAX_ACCEPTED_PROMOTIONS_PER_KIND,
    );
    let undertaking_scope = source_scope.accepted_undertaking_scope();
    let repository_scope = source_scope.accepted_repository_scope();
    let identity = CoderAgentIdentity::for_turn(
        "forge-memory-lifecycle",
        &task.decision_id,
        &task.attempt_id,
    );
    let compile = |node: &Value,
                   scope: &super::coder_memory::CoderMemoryScope|
     -> Result<super::coder_memory::CoderMemoryCommit> {
        super::coder_memory::build_promotion_commit(
            node,
            scope,
            &identity,
            &task.accepted_head,
            &task.decision_id,
            &task.evidence_id,
            &task.evidence_digest,
        )
    };
    let mut undertaking_commits = decisions
        .iter()
        .map(|node| compile(node, &undertaking_scope))
        .collect::<Result<Vec<_>>>()?;
    undertaking_commits.extend(
        verifications
            .iter()
            .map(|node| compile(node, &undertaking_scope))
            .collect::<Result<Vec<_>>>()?,
    );
    let repository_commits = verifications
        .iter()
        .map(|node| compile(node, &repository_scope))
        .collect::<Result<Vec<_>>>()?;
    let (undertaking, repository) = tokio::join!(
        persist_lifecycle_commits(registry, &undertaking_scope, undertaking_commits),
        persist_lifecycle_commits(registry, &repository_scope, repository_commits),
    );
    Ok(json!({
        "ok": true,
        "decision_candidates": decisions.len(),
        "verification_candidates": verifications.len(),
        "undertaking": undertaking?,
        "repository": repository?,
    }))
}

pub async fn reconcile_coder_memory_lineage(
    registry: Arc<dyn ToolRegistry>,
    repo_id: &str,
) -> Value {
    let tasks = super::coder_memory::CoderMemoryPromotionTask::pending_for_repo(repo_id);
    let reconciled = futures_util::stream::iter(
        tasks
            .into_iter()
            .take(MAX_LIFECYCLE_RECONCILIATIONS_PER_PASS),
    )
    .map(|task| {
        let registry = registry.clone();
        async move {
            let result = promote_memory_task(registry.as_ref(), &task).await;
            (task, result)
        }
    })
    .buffer_unordered(MAX_CONCURRENT_MEMORY_RECONCILIATIONS)
    .collect::<Vec<_>>()
    .await;
    let mut completed = 0usize;
    let mut deferred = 0usize;
    let mut errors = Vec::new();
    for (task, result) in reconciled {
        match result {
            Ok(_) => match task.remove() {
                Ok(()) => completed += 1,
                Err(error) => {
                    deferred += 1;
                    let _ = task.persist();
                    errors.push(bounded_memory_error(&error.to_string()));
                }
            },
            Err(error) => {
                deferred += 1;
                let _ = task.persist();
                errors.push(bounded_memory_error(&error.to_string()));
            }
        }
    }
    json!({
        "ok": errors.is_empty(),
        "completed": completed,
        "deferred": deferred,
        "errors": errors,
    })
}

pub async fn finalize_coder_memory_lineage(
    registry: Arc<dyn ToolRegistry>,
    forge: Arc<Forge>,
    item: &WorkItem,
    accepted_decision_id: Option<&ReviewDecisionId>,
) -> Value {
    if !item.state.is_terminal() {
        return json!({ "ok": true, "skipped": "undertaking_not_terminal" });
    }

    let mut promotion = Value::Null;
    if item.state == WorkState::Accepted
        && let Some(decision_id) = accepted_decision_id
        && let Some(decision) = item
            .review_decisions
            .iter()
            .find(|decision| &decision.id == decision_id)
        && let Some(environment) = item.environment_for_attempt(&decision.attempt_id)
    {
        let task = super::coder_memory::CoderMemoryPromotionTask::new(
            environment.repo.repo_id.to_string(),
            item.id.to_string(),
            environment.branch.clone(),
            environment.generation,
            decision.reviewed_head_oid.to_string(),
            decision.id.to_string(),
            decision.attempt_id.to_string(),
            decision.evidence_id.to_string(),
            decision.evidence_digest.to_string(),
        );
        promotion = match promote_memory_task(registry.as_ref(), &task).await {
            Ok(report) => {
                if let Err(error) = task.remove() {
                    tracing::warn!(error = %error, "failed to clear completed memory promotion task");
                }
                report
            }
            Err(error) => {
                let durable = task.persist().is_ok();
                tracing::warn!(
                    error = %error,
                    durable,
                    work_id = %item.id,
                    "deferred accepted Coder memory promotion"
                );
                json!({
                    "ok": false,
                    "deferred": true,
                    "durable": durable,
                    "error": bounded_memory_error(&error.to_string()),
                })
            }
        };
    }

    let terminal_state = item.state.to_string();
    let detail = format!(
        "Forge terminal state {terminal_state}; semantic nodes remain audit-only and are excluded from active ambient recall."
    );
    let mut seen = HashSet::new();
    let mut environments = item
        .attempts
        .iter()
        .rev()
        .filter_map(|attempt| attempt.environment.as_ref())
        .chain(item.environment.iter())
        .filter(|environment| seen.insert((environment.branch.clone(), environment.generation)))
        .take(MAX_ARCHIVED_MEMORY_ENVIRONMENTS)
        .cloned()
        .collect::<Vec<_>>();
    environments.reverse();
    let archive_futures = environments.into_iter().map(|environment| {
        let registry = registry.clone();
        let forge = forge.clone();
        let work_id = item.id.to_string();
        let terminal_state = terminal_state.clone();
        let detail = detail.clone();
        async move {
            let scope = super::coder_memory::CoderMemoryScope::for_environment(
                &environment.repo.repo_id.to_string(),
                &work_id,
                &environment.branch,
                environment.generation,
            );
            let current_head = forge
                .git()
                .head_oid(&environment.worktree)
                .map(|head| head.to_string())
                .unwrap_or_else(|_| environment.baseline_oid.to_string());
            let identity = CoderAgentIdentity::for_turn(
                "forge-memory-lifecycle",
                format!("terminal-{terminal_state}"),
                "terminal",
            );
            let commit = super::coder_memory::build_archive_commit(
                &scope,
                &identity,
                &current_head,
                &terminal_state,
                &detail,
            )?;
            persist_lifecycle_commits(registry.as_ref(), &scope, vec![commit]).await
        }
    });
    let archived = futures_util::stream::iter(archive_futures)
        .buffer_unordered(MAX_CONCURRENT_MEMORY_ARCHIVES)
        .collect::<Vec<_>>()
        .await;
    let archive_errors = archived
        .iter()
        .filter_map(|result| result.as_ref().err())
        .map(|error| bounded_memory_error(&error.to_string()))
        .collect::<Vec<_>>();
    json!({
        "ok": promotion.get("ok").and_then(Value::as_bool) != Some(false)
            && archive_errors.is_empty(),
        "terminal_state": terminal_state,
        "promotion": promotion,
        "archived_environments": archived.len().saturating_sub(archive_errors.len()),
        "archive_errors": archive_errors,
    })
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
    catalog: Option<Arc<ToolCatalog>>,
    authority: Weak<CoderTurnLease>,
    entry: Arc<CoderEntryContext>,
    memory_scope: super::coder_memory::CoderMemoryScope,
    memory_retry_queue: Arc<Mutex<super::coder_memory::CoderMemoryRetryQueue>>,
    policy: WorkPolicy,
    visible_tools: Arc<StdMutex<HashSet<ToolId>>>,
    memory_cursor: Arc<StdMutex<Option<String>>>,
    change_sets: Arc<StdMutex<super::coder_semantic_actions::CoderChangeSetStore>>,
    shell_state: Arc<Mutex<CoderShellState>>,
}

#[derive(Default)]
struct CoderShellState {
    owned_sessions: HashSet<String>,
    preferred_session: Option<String>,
    cursors: HashMap<String, u64>,
}

impl CoderBoundToolRegistry {
    pub fn new(
        inner: Arc<dyn ToolRegistry>,
        authority: &Arc<CoderTurnLease>,
        entry: Arc<CoderEntryContext>,
        policy: WorkPolicy,
    ) -> Self {
        Self::new_inner(inner, None, authority, entry, policy)
    }

    pub fn new_with_catalog(
        inner: Arc<dyn ToolRegistry>,
        catalog: Arc<ToolCatalog>,
        authority: &Arc<CoderTurnLease>,
        entry: Arc<CoderEntryContext>,
        policy: WorkPolicy,
    ) -> Self {
        Self::new_inner(inner, Some(catalog), authority, entry, policy)
    }

    fn new_inner(
        inner: Arc<dyn ToolRegistry>,
        catalog: Option<Arc<ToolCatalog>>,
        authority: &Arc<CoderTurnLease>,
        entry: Arc<CoderEntryContext>,
        policy: WorkPolicy,
    ) -> Self {
        let memory_scope = super::coder_memory::CoderMemoryScope::for_entry(&entry);
        let memory_retry_queue = shared_memory_retry_queue(&memory_scope);
        let mut visible_tools = coder_initial_tool_ids()
            .into_iter()
            .filter(|id| coder_tool_allowed(*id, &policy))
            .collect::<HashSet<_>>();
        if entry.editor.active_path.is_some() || entry.editor.containing_symbol.is_some() {
            visible_tools.extend(
                crate::code_intelligence_tools::CODE_COGNITION_TOOLS
                    .iter()
                    .map(|name| ToolId::new(name)),
            );
        }
        Self {
            inner,
            catalog,
            authority: Arc::downgrade(authority),
            entry,
            memory_scope,
            memory_retry_queue,
            policy,
            visible_tools: Arc::new(StdMutex::new(visible_tools)),
            memory_cursor: Arc::new(StdMutex::new(None)),
            change_sets: Arc::new(StdMutex::new(Default::default())),
            shell_state: Arc::new(Mutex::new(CoderShellState::default())),
        }
    }

    fn resolve_wire_tool_id(&self, wire_name: &str) -> Result<ToolId> {
        self.catalog
            .as_ref()
            .and_then(|catalog| catalog.resolve_wire_id(wire_name).ok())
            .or_else(|| resolve_known_coder_tool_id(wire_name))
            .ok_or_else(|| {
                StasisError::PortFailure(format!(
                    "tool is absent from the assembled Coder catalog: {wire_name}"
                ))
            })
    }

    fn authority(&self) -> Result<Arc<CoderTurnLease>> {
        self.authority
            .upgrade()
            .ok_or_else(|| StasisError::PortFailure("Coder turn authority has expired".to_string()))
    }

    pub async fn initial_prompt_appendix(&self) -> Result<String> {
        let authority = self.authority()?;
        let shared = authority.shared_space_prompt_appendix()?;
        let pointers = self.ranked_pointers(super::coder_pointers::MAX_AMBIENT_POINTERS)?;
        self.refresh_visible_from_pointers(&pointers)?;
        let lineage_reconciliation =
            reconcile_coder_memory_lineage(self.inner.clone(), &self.memory_scope.repo_id).await;
        if lineage_reconciliation.get("ok").and_then(Value::as_bool) == Some(false) {
            tracing::warn!(
                report = %lineage_reconciliation,
                "Coder memory lineage reconciliation remains deferred"
            );
        }
        let memory = self.environment_memory_overview(&authority, 10).await?;
        self.refresh_visible_from_memory_overview(&memory)?;
        if super::coder_experiments::sealed_candidate_count(
            authority.forge.as_ref(),
            &self.entry.work_id,
        )
        .is_ok_and(|count| count >= 2)
        {
            let _ = self.unlock_domain(ToolDomainId::new("experiments"))?;
        }
        Ok(format!(
            "{shared}\n\n{}\n\n{}",
            super::coder_pointers::engineering_pointer_prompt_appendix(&pointers),
            super::coder_memory::environment_overview_prompt_appendix(&memory),
        ))
    }

    pub fn undertaking_id(&self) -> &str {
        &self.entry.work_id
    }

    pub(crate) fn checkpoint_visible_tools(&self) -> Result<Vec<String>> {
        let mut visible = self
            .visible_tools
            .lock()
            .map_err(|err| {
                StasisError::PortFailure(format!("Coder visible tool lock poisoned: {err}"))
            })?
            .iter()
            .map(|id| id.as_str().to_string())
            .collect::<Vec<_>>();
        visible.sort();
        Ok(visible)
    }

    /// Restore only model visibility. Forge/runtime policy remains the immutable
    /// authority superset, and `list_tools` still intersects this set with the
    /// tools actually registered for the turn.
    pub(crate) fn restore_checkpoint_surface(
        &self,
        visible_tools: &[String],
        memory_cursor: Option<&str>,
    ) -> Result<()> {
        let mut visible = self.visible_tools.lock().map_err(|err| {
            StasisError::PortFailure(format!("Coder visible tool lock poisoned: {err}"))
        })?;
        *visible = visible_tools
            .iter()
            .filter_map(|name| self.resolve_wire_tool_id(name).ok())
            .filter(|id| coder_tool_allowed(*id, &self.policy))
            .collect();
        drop(visible);
        *self.memory_cursor.lock().map_err(|err| {
            StasisError::PortFailure(format!("Coder memory cursor lock poisoned: {err}"))
        })? = memory_cursor
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        Ok(())
    }

    pub(crate) fn checkpoint_activity_cursor(&self) -> Result<u64> {
        let authority = self.authority()?;
        authority
            .activity
            .snapshot(&self.entry.work_id, &authority.identity.agent_id)
            .map(|snapshot| snapshot.revision)
            .map_err(|err| {
                StasisError::PortFailure(format!("cannot read Coder activity cursor: {err}"))
            })
    }

    pub(crate) fn checkpoint_agent_id(&self) -> Result<String> {
        Ok(self.authority()?.identity.agent_id.clone())
    }

    pub(crate) fn checkpoint_memory_cursor(&self) -> Result<Option<String>> {
        self.memory_cursor
            .lock()
            .map(|cursor| cursor.clone())
            .map_err(|err| {
                StasisError::PortFailure(format!("Coder memory cursor lock poisoned: {err}"))
            })
    }

    fn remember_memory_cursor(&self, result: &Value) {
        let Some(cursor) = result
            .get("node_id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| super::coder_memory::first_node_id(result))
        else {
            return;
        };
        match self.memory_cursor.lock() {
            Ok(mut current) => *current = Some(cursor),
            Err(err) => tracing::warn!(error = %err, "failed to update Coder memory cursor"),
        }
    }

    pub(crate) fn engineering_events(
        &self,
    ) -> Result<Vec<super::coder_activity::CoderActivityEvent>> {
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

    fn invoke_engineering_pointers(
        &self,
        input: EngineeringPointersInput,
    ) -> Result<EngineeringPointersOutput> {
        let limit = input.limit.into_option().unwrap_or(12).clamp(1, 24);
        let pointers = self.ranked_pointers(limit)?;
        self.refresh_visible_from_pointers(&pointers)?;
        Ok(EngineeringPointersOutput {
            ok: true,
            count: pointers.len(),
            pointers,
        })
    }

    fn unlock_domain(&self, domain: ToolDomainId) -> Result<Vec<String>> {
        let ids = coder_domain_tool_ids(domain).ok_or_else(|| {
            StasisError::PortFailure(format!(
                "unknown Coder tool domain '{domain}'; expected one of {}",
                CODER_DISCOVERABLE_DOMAINS.join(", ")
            ))
        })?;
        let mut visible = self.visible_tools.lock().map_err(|err| {
            StasisError::PortFailure(format!("Coder visible tool lock poisoned: {err}"))
        })?;
        let mut unlocked = Vec::new();
        for id in ids {
            if coder_tool_allowed(id, &self.policy) && visible.insert(id) {
                unlocked.push(id.as_str().to_string());
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
            let _ = self.unlock_domain(ToolDomainId::new("intelligence"))?;
        }
        let needs_semantic_actions = pointers.iter().any(|pointer| {
            matches!(
                pointer.kind,
                super::coder_pointers::CoderPointerKind::Symbol
                    | super::coder_pointers::CoderPointerKind::ChangeSet
            )
        });
        if needs_semantic_actions {
            let _ = self.unlock_domain(ToolDomainId::new("semantic_actions"))?;
        }
        let needs_causal = pointers.iter().any(|pointer| {
            matches!(
                pointer.status,
                super::coder_activity::CoderActivityKind::ToolBlocked
                    | super::coder_activity::CoderActivityKind::ToolFailed
            )
        });
        if needs_causal {
            let _ = self.unlock_domain(ToolDomainId::new("causal"))?;
        }
        Ok(())
    }

    fn refresh_visible_from_memory_overview(&self, overview: &Value) -> Result<()> {
        let nodes = overview
            .get("nodes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .chain(
                overview
                    .get("pending_writes")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten(),
            );
        let mut needs_intelligence = false;
        let mut needs_semantic_actions = false;
        let mut needs_causal = false;
        let mut needs_world_model = false;
        let mut needs_experiments = false;
        for node in nodes {
            let kind = node.get("kind").and_then(Value::as_str);
            let has_symbols = node
                .get("symbols")
                .and_then(Value::as_array)
                .is_some_and(|symbols| !symbols.is_empty());
            needs_intelligence |=
                has_symbols || matches!(kind, Some("discovery" | "hypothesis" | "open_gap"));
            needs_world_model |= matches!(
                kind,
                Some("change" | "verification" | "checkpoint" | "handoff")
            );
            needs_semantic_actions |= matches!(kind, Some("change" | "decision" | "verification"));
            needs_causal |= matches!(kind, Some("hypothesis" | "verification" | "open_gap"));
            needs_experiments |= matches!(
                kind,
                Some("experiment" | "acceptance_criterion" | "next_action")
            );
        }
        if needs_intelligence {
            let _ = self.unlock_domain(ToolDomainId::new("intelligence"))?;
        }
        if needs_world_model {
            let _ = self.unlock_domain(ToolDomainId::new("world_model"))?;
        }
        if needs_semantic_actions {
            let _ = self.unlock_domain(ToolDomainId::new("semantic_actions"))?;
        }
        if needs_causal {
            let _ = self.unlock_domain(ToolDomainId::new("causal"))?;
        }
        if needs_experiments {
            let _ = self.unlock_domain(ToolDomainId::new("experiments"))?;
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
                let domain_id = resolve_coder_domain_id(domain).ok_or_else(|| {
                    StasisError::PortFailure(format!(
                        "unknown Coder tool domain '{domain}'; expected one of {}",
                        CODER_DISCOVERABLE_DOMAINS.join(", ")
                    ))
                })?;
                let unlocked = self.unlock_domain(domain_id)?;
                Ok(json!({
                    "ok": true,
                    "domain": domain,
                    "newly_visible": unlocked,
                    "available_domains": CODER_DISCOVERABLE_DOMAINS,
                }))
            }
            COGNITION_ENGINEERING_POINTERS => {
                let input = crate::typed_tools::deserialize_input::<EngineeringPointersInput>(
                    COGNITION_ENGINEERING_POINTERS_ID,
                    input.clone(),
                )?;
                let output = self.invoke_engineering_pointers(input)?;
                crate::typed_tools::serialize_output(COGNITION_ENGINEERING_POINTERS_ID, output)
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

    async fn invoke_locus_tool(&self, tool_name: &str, input: Value) -> Result<Value> {
        invoke_locus_registry_tool(self.inner.as_ref(), tool_name, input).await
    }

    async fn try_store_memory_commit(
        &self,
        commit: &super::coder_memory::CoderMemoryCommit,
    ) -> Result<Value> {
        self.try_store_memory_commit_in_scope(&self.memory_scope, commit)
            .await
    }

    async fn try_store_memory_commit_in_scope(
        &self,
        scope: &super::coder_memory::CoderMemoryScope,
        commit: &super::coder_memory::CoderMemoryCommit,
    ) -> Result<Value> {
        let stored =
            try_store_memory_commit_with_registry(self.inner.as_ref(), scope, commit).await?;
        if scope.session_id == self.memory_scope.session_id {
            self.remember_memory_cursor(&stored);
        }
        Ok(stored)
    }

    async fn flush_memory_queue_locked(
        &self,
        scope: &super::coder_memory::CoderMemoryScope,
        queue: &mut super::coder_memory::CoderMemoryRetryQueue,
    ) -> (usize, Option<String>) {
        let mut flushed = 0usize;
        while flushed < MAX_MEMORY_FLUSH_WRITES_PER_PASS
            && let Some(commit) = queue.front()
        {
            match self.try_store_memory_commit_in_scope(scope, &commit).await {
                Ok(_) => {
                    if let Err(error) = queue.pop_front(&commit.dedupe_tag) {
                        tracing::warn!(error = %error, "failed to persist a drained Coder memory queue");
                    }
                    flushed += 1;
                }
                Err(error) => {
                    return (flushed, Some(bounded_memory_error(&error.to_string())));
                }
            }
        }
        (flushed, None)
    }

    pub async fn flush_memory_queue(&self) -> Value {
        let mut queue = self.memory_retry_queue.lock().await;
        let before = queue.len();
        let (flushed, error) = self
            .flush_memory_queue_locked(&self.memory_scope, &mut queue)
            .await;
        json!({
            "ok": error.is_none(),
            "queued_before": before,
            "flushed": flushed,
            "pending_writes": queue.len(),
            "error": error,
        })
    }

    async fn flush_memory_scope_queue(
        &self,
        scope: &super::coder_memory::CoderMemoryScope,
    ) -> Value {
        if scope.session_id == self.memory_scope.session_id {
            return self.flush_memory_queue().await;
        }
        let queue = shared_memory_retry_queue(scope);
        let mut queue = queue.lock().await;
        let before = queue.len();
        let (flushed, error) = self.flush_memory_queue_locked(scope, &mut queue).await;
        json!({
            "ok": error.is_none(),
            "queued_before": before,
            "flushed": flushed,
            "pending_writes": queue.len(),
            "error": error,
        })
    }

    async fn persist_or_queue_memory_commit(
        &self,
        authority: &CoderTurnLease,
        commit: super::coder_memory::CoderMemoryCommit,
        source: &str,
    ) -> Value {
        let mut queue = self.memory_retry_queue.lock().await;
        let (_, flush_error) = self
            .flush_memory_queue_locked(&self.memory_scope, &mut queue)
            .await;
        let direct = if flush_error.is_none() {
            self.try_store_memory_commit(&commit).await
        } else {
            Err(StasisError::PortFailure(
                flush_error.clone().unwrap_or_default(),
            ))
        };
        let response = match direct {
            Ok(mut stored) => {
                stored["queued"] = Value::Bool(false);
                stored["pending_writes"] = json!(queue.len());
                stored
            }
            Err(error) => {
                let error = bounded_memory_error(&error.to_string());
                let durable_queue = match queue.enqueue(commit.clone()) {
                    Ok(_) => true,
                    Err(queue_error) => {
                        tracing::warn!(error = %queue_error, "failed to persist Coder memory retry queue");
                        false
                    }
                };
                tracing::warn!(
                    source,
                    memory_kind = %commit.kind,
                    pending_writes = queue.len(),
                    error,
                    "deferred Coder memory write without failing the coding turn"
                );
                json!({
                    "ok": true,
                    "stored": false,
                    "duplicate": false,
                    "queued": true,
                    "durable_queue": durable_queue,
                    "pending_writes": queue.len(),
                    "kind": commit.kind,
                    "summary": commit.summary,
                    "scope": self.memory_scope.public_descriptor(),
                    "memory_status": "deferred",
                    "deferred_error": error,
                })
            }
        };
        authority.append_receipt(json!({
            "kind": "medousa_coder_memory_checkpoint",
            "source": source,
            "memory_kind": commit.kind,
            "dedupe_tag": commit.dedupe_tag,
            "stored": response.get("stored"),
            "duplicate": response.get("duplicate"),
            "queued": response.get("queued"),
            "pending_writes": response.get("pending_writes"),
        }));
        response
    }

    async fn pending_memory_summaries(
        &self,
    ) -> (usize, Vec<super::coder_memory::CoderPendingMemorySummary>) {
        let queue = self.memory_retry_queue.lock().await;
        (queue.len(), queue.pending_summaries())
    }

    async fn environment_memory_overview(
        &self,
        authority: &CoderTurnLease,
        limit: usize,
    ) -> Result<Value> {
        let current_head = authority
            .forge
            .git()
            .head_oid(&self.entry.worktree)
            .map_err(|error| {
                StasisError::PortFailure(format!(
                    "cannot observe Coder HEAD before memory overview: {error}"
                ))
            })?
            .to_string();
        let parent_scope = self.memory_scope.parent_environment_scope();
        let undertaking_scope = self.memory_scope.accepted_undertaking_scope();
        let repository_scope = self.memory_scope.accepted_repository_scope();
        let (flush, parent_flush, undertaking_flush, repository_flush) = tokio::join!(
            self.flush_memory_scope_queue(&self.memory_scope),
            async {
                match parent_scope.as_ref() {
                    Some(scope) => Some(self.flush_memory_scope_queue(scope).await),
                    None => None,
                }
            },
            self.flush_memory_scope_queue(&undertaking_scope),
            self.flush_memory_scope_queue(&repository_scope),
        );
        let lineage_limit = limit.saturating_mul(2).clamp(1, 40);
        let (current, parent, undertaking, repository) = tokio::join!(
            self.invoke_locus_tool(
                "cognition_memory_list",
                json!({
                    "session_id": self.memory_scope.session_id,
                    "limit": lineage_limit,
                }),
            ),
            async {
                match parent_scope.as_ref() {
                    Some(scope) => Some(
                        self.invoke_locus_tool(
                            "cognition_memory_list",
                            json!({
                                "session_id": scope.session_id,
                                "limit": lineage_limit,
                            }),
                        )
                        .await,
                    ),
                    None => None,
                }
            },
            self.invoke_locus_tool(
                "cognition_memory_list",
                json!({
                    "session_id": undertaking_scope.session_id,
                    "limit": lineage_limit,
                }),
            ),
            self.invoke_locus_tool(
                "cognition_memory_list",
                json!({
                    "session_id": repository_scope.session_id,
                    "limit": lineage_limit,
                }),
            ),
        );
        if let Ok(result) = current.as_ref() {
            self.remember_memory_cursor(result);
        }
        let (pending_count, pending) = self.pending_memory_summaries().await;
        let mut overview = super::coder_memory::project_lineage_recall(
            &self.memory_scope,
            &current_head,
            super::coder_memory::CoderMemoryLineageSources {
                current: current.as_ref().ok(),
                parent: parent.as_ref().and_then(|result| result.as_ref().ok()),
                undertaking: undertaking.as_ref().ok(),
                repository: repository.as_ref().ok(),
            },
            false,
            limit,
        );
        let mut unavailable_sources = Vec::new();
        if current.is_err() {
            unavailable_sources.push("current_environment");
        }
        if parent.as_ref().is_some_and(Result::is_err) {
            unavailable_sources.push("inherited_parent");
        }
        if undertaking.is_err() {
            unavailable_sources.push("accepted_undertaking");
        }
        if repository.is_err() {
            unavailable_sources.push("accepted_repository");
        }
        let flush_degraded = [&flush, &undertaking_flush, &repository_flush]
            .into_iter()
            .chain(parent_flush.as_ref())
            .any(|result| result.get("ok").and_then(Value::as_bool) == Some(false));
        let all_unavailable = current.is_err()
            && undertaking.is_err()
            && repository.is_err()
            && parent.as_ref().is_none_or(Result::is_err);
        overview["ok"] = Value::Bool(!all_unavailable);
        overview["memory_status"] = Value::String(
            if all_unavailable {
                "unavailable"
            } else if pending_count > 0 || flush_degraded || !unavailable_sources.is_empty() {
                "degraded"
            } else {
                "available"
            }
            .into(),
        );
        overview["unavailable_sources"] = json!(unavailable_sources);
        overview["pending_write_count"] = json!(pending_count);
        overview["pending_writes"] = json!(pending);
        overview["retry_flush"] = json!({
            "current_environment": flush,
            "inherited_parent": parent_flush,
            "accepted_undertaking": undertaking_flush,
            "accepted_repository": repository_flush,
        });
        Ok(overview)
    }

    async fn invoke_coder_memory_tool(
        &self,
        authority: &CoderTurnLease,
        tool_name: &str,
        input: &Value,
    ) -> Result<Value> {
        let current_head = authority
            .forge
            .git()
            .head_oid(&self.entry.worktree)
            .map_err(|error| {
                StasisError::PortFailure(format!(
                    "cannot observe Coder HEAD before memory operation: {error}"
                ))
            })?
            .to_string();

        match tool_name {
            super::coder_memory::COGNITION_CODER_MEMORY_OVERVIEW => {
                self.environment_memory_overview(
                    authority,
                    super::coder_memory::overview_limit(input),
                )
                .await
            }
            super::coder_memory::COGNITION_CODER_MEMORY_RECALL => {
                let query = super::coder_memory::parse_recall_query(input)?;
                let semantic_tags = super::coder_memory::recall_semantic_tags(&query);
                let parent_scope = self.memory_scope.parent_environment_scope();
                let undertaking_scope = self.memory_scope.accepted_undertaking_scope();
                let repository_scope = self.memory_scope.accepted_repository_scope();
                let (flush, parent_flush, undertaking_flush, repository_flush) = tokio::join!(
                    self.flush_memory_scope_queue(&self.memory_scope),
                    async {
                        match parent_scope.as_ref() {
                            Some(scope) => Some(self.flush_memory_scope_queue(scope).await),
                            None => None,
                        }
                    },
                    self.flush_memory_scope_queue(&undertaking_scope),
                    self.flush_memory_scope_queue(&repository_scope),
                );
                let recall_input = |session_id: &str| {
                    let mut input = json!({
                        "session_id": session_id,
                        "query": query.query,
                        "limit": query.limit.saturating_mul(2).clamp(1, 24),
                    });
                    if !semantic_tags.is_empty() {
                        input["semantic_tags"] = json!(semantic_tags);
                    }
                    input
                };
                let (current, parent, undertaking, repository) = tokio::join!(
                    self.invoke_locus_tool(
                        "cognition_memory_recall",
                        recall_input(&self.memory_scope.session_id),
                    ),
                    async {
                        match parent_scope.as_ref() {
                            Some(scope) => Some(
                                self.invoke_locus_tool(
                                    "cognition_memory_recall",
                                    recall_input(&scope.session_id),
                                )
                                .await,
                            ),
                            None => None,
                        }
                    },
                    self.invoke_locus_tool(
                        "cognition_memory_recall",
                        recall_input(&undertaking_scope.session_id),
                    ),
                    self.invoke_locus_tool(
                        "cognition_memory_recall",
                        recall_input(&repository_scope.session_id),
                    ),
                );
                if let Ok(result) = current.as_ref() {
                    self.remember_memory_cursor(result);
                }
                let (pending_count, pending) = self.pending_memory_summaries().await;
                let mut recalled = super::coder_memory::project_lineage_recall(
                    &self.memory_scope,
                    &current_head,
                    super::coder_memory::CoderMemoryLineageSources {
                        current: current.as_ref().ok(),
                        parent: parent.as_ref().and_then(|result| result.as_ref().ok()),
                        undertaking: undertaking.as_ref().ok(),
                        repository: repository.as_ref().ok(),
                    },
                    true,
                    query.limit,
                );
                let mut unavailable_sources = Vec::new();
                if current.is_err() {
                    unavailable_sources.push("current_environment");
                }
                if parent.as_ref().is_some_and(Result::is_err) {
                    unavailable_sources.push("inherited_parent");
                }
                if undertaking.is_err() {
                    unavailable_sources.push("accepted_undertaking");
                }
                if repository.is_err() {
                    unavailable_sources.push("accepted_repository");
                }
                let flush_degraded = [&flush, &undertaking_flush, &repository_flush]
                    .into_iter()
                    .chain(parent_flush.as_ref())
                    .any(|result| result.get("ok").and_then(Value::as_bool) == Some(false));
                let all_unavailable = current.is_err()
                    && undertaking.is_err()
                    && repository.is_err()
                    && parent.as_ref().is_none_or(Result::is_err);
                recalled["ok"] = Value::Bool(!all_unavailable);
                recalled["memory_status"] = Value::String(
                    if all_unavailable {
                        "unavailable"
                    } else if pending_count > 0 || flush_degraded || !unavailable_sources.is_empty()
                    {
                        "degraded"
                    } else {
                        "available"
                    }
                    .into(),
                );
                recalled["unavailable_sources"] = json!(unavailable_sources);
                recalled["pending_write_count"] = json!(pending_count);
                recalled["pending_writes"] = json!(pending);
                recalled["retry_flush"] = json!({
                    "current_environment": flush,
                    "inherited_parent": parent_flush,
                    "accepted_undertaking": undertaking_flush,
                    "accepted_repository": repository_flush,
                });
                Ok(recalled)
            }
            super::coder_memory::COGNITION_CODER_MEMORY_COMMIT => {
                let commit = super::coder_memory::build_commit(
                    input,
                    &self.memory_scope,
                    &authority.identity,
                    &current_head,
                )?;
                if matches!(
                    commit.kind.as_str(),
                    "experiment" | "acceptance_criterion" | "next_action"
                ) {
                    let _ = self.unlock_domain(ToolDomainId::new("experiments"))?;
                }
                Ok(self
                    .persist_or_queue_memory_commit(authority, commit, "model_commit")
                    .await)
            }
            _ => Err(StasisError::PortFailure(format!(
                "unknown Coder memory tool: {tool_name}"
            ))),
        }
    }

    async fn checkpoint_tool_boundary(
        &self,
        authority: &CoderTurnLease,
        checkpoint: CoderMemoryCheckpoint<'_>,
    ) {
        let CoderMemoryCheckpoint {
            boundary,
            tool_name,
            intent,
            call_id,
            input,
            result,
        } = checkpoint;
        let succeeded = result
            .as_ref()
            .ok()
            .and_then(|output| output.get("ok").and_then(Value::as_bool))
            .unwrap_or_else(|| result.is_ok());
        let current_head = match authority.forge.git().head_oid(&self.entry.worktree) {
            Ok(head) => head.to_string(),
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    tool = tool_name,
                    "skipping automatic Coder memory checkpoint because HEAD is unavailable"
                );
                return;
            }
        };
        let detail = match result {
            Ok(output) if !succeeded => output
                .get("error")
                .and_then(Value::as_str)
                .map(|error| {
                    format!(
                        "tool={tool_name}; outcome=failed; error={}",
                        bounded_memory_error(error)
                    )
                })
                .unwrap_or_else(|| format!("tool={tool_name}; outcome=failed")),
            Ok(_) => format!("tool={tool_name}; outcome=completed"),
            Err(error) => format!(
                "tool={tool_name}; outcome=failed; error={}",
                bounded_memory_error(&error.to_string())
            ),
        };
        let commit_input = json!({
            "kind": boundary.memory_kind(succeeded),
            "summary": memory_boundary_summary(boundary, succeeded, intent, input),
            "details": detail,
            "paths": memory_checkpoint_paths(input, &self.entry.worktree),
            "evidence_refs": [format!("engineering:call:{call_id}")],
        });
        let commit = match super::coder_memory::build_commit(
            &commit_input,
            &self.memory_scope,
            &authority.identity,
            &current_head,
        ) {
            Ok(commit) => commit,
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    tool = tool_name,
                    "failed to compile automatic Coder memory checkpoint"
                );
                return;
            }
        };
        let _ = self
            .persist_or_queue_memory_commit(authority, commit, tool_name)
            .await;
    }

    fn bind_input(&self, tool_name: &str, mut input: Value) -> Result<Value> {
        let map = input.as_object_mut().ok_or_else(|| {
            StasisError::PortFailure("Coder tools require an object input".into())
        })?;
        if CODER_SCOPED_MEMORY_TOOLS.contains(&tool_name) {
            if tool_name == "cognition_memory_store"
                && let Some(raw_node) = map
                    .get("node")
                    .or_else(|| map.get("content"))
                    .and_then(Value::as_str)
            {
                super::coder_memory::validate_raw_node_scope(
                    raw_node,
                    &self.memory_scope.session_id,
                )?;
            }
            map.insert(
                "session_id".into(),
                Value::String(self.memory_scope.session_id.clone()),
            );
        }
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
            reject_mismatched_string(map.get("work_id"), &self.entry.work_id, "work_id")?;
            let authority = self.authority()?;
            reject_mismatched_string(
                map.get("attempt_id"),
                authority.lease().attempt_id.as_str(),
                "attempt_id",
            )?;
            map.insert("work_id".into(), Value::String(self.entry.work_id.clone()));
            map.insert(
                "attempt_id".into(),
                Value::String(authority.lease().attempt_id.to_string()),
            );
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

    fn enrich_semantic_input(&self, tool_name: &str, mut input: Value) -> Result<Value> {
        if tool_name != super::coder_semantic_actions::COGNITION_CODER_CHANGE_SET_APPLY {
            return Ok(input);
        }
        let map = input.as_object_mut().ok_or_else(|| {
            StasisError::PortFailure("Coder tools require an object input".into())
        })?;
        let change_set_id = map
            .get("change_set_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StasisError::PortFailure("change_set_id is required".into()))?;
        let paths = self
            .change_sets
            .lock()
            .map_err(|error| {
                StasisError::PortFailure(format!("change-set store is unavailable: {error}"))
            })?
            .paths_for(change_set_id)
            .unwrap_or_default();
        map.insert("paths".into(), json!(paths));
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
        let session_ids = {
            let mut state = self.shell_state.lock().await;
            state.preferred_session = None;
            state.cursors.clear();
            state.owned_sessions.drain().collect::<Vec<_>>()
        };
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
        if !self
            .shell_state
            .lock()
            .await
            .owned_sessions
            .contains(session_id)
        {
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
            let mut state = self.shell_state.lock().await;
            state.owned_sessions.insert(session_id.to_string());
            state.preferred_session = Some(session_id.to_string());
            if let Some(sequence) = output.get("next_sequence").and_then(Value::as_u64) {
                state.cursors.insert(session_id.to_string(), sequence);
            }
        }
    }

    async fn prepare_turn_shell_session(&self, tool_name: &str, input: &mut Value) {
        if !matches!(
            tool_name,
            crate::coding_tools::COGNITION_SHELL_SESSION_RUN
                | crate::coding_tools::COGNITION_CODER_SHELL_RUN
        ) {
            return;
        }
        let mut session_id = input
            .get("session_id")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string);
        let state = self.shell_state.lock().await;
        if session_id.is_none() && tool_name == crate::coding_tools::COGNITION_CODER_SHELL_RUN {
            session_id.clone_from(&state.preferred_session);
        }
        let cursor = session_id
            .as_deref()
            .and_then(|session_id| state.cursors.get(session_id).copied());
        drop(state);

        if let Some(map) = input.as_object_mut() {
            map.remove("after_sequence");
            if let Some(session_id) = session_id {
                map.insert("session_id".into(), Value::String(session_id));
            }
            if let Some(cursor) = cursor {
                map.insert("after_sequence".into(), Value::from(cursor));
            }
        }
    }
}

fn coder_domain_tool_ids(domain: ToolDomainId) -> Option<Vec<ToolId>> {
    let names: &[&'static str] = match domain.as_str() {
        "intelligence" => crate::code_intelligence_tools::CODE_COGNITION_TOOLS,
        "semantic_actions" => super::coder_semantic_actions::SEMANTIC_ACTION_TOOL_NAMES,
        "causal" => &[super::coder_causal::COGNITION_CODER_CAUSAL_QUERY],
        "world_model" => crate::detamu_tools::DETAMU_COGNITION_TOOLS,
        "experiments" => &[super::coder_experiments::COGNITION_CODER_EXPERIMENT_COMPARE],
        "history" => &[
            COGNITION_ENGINEERING_HISTORY,
            crate::chat_history_tools::COGNITION_CHAT_HISTORY_SEARCH,
            crate::chat_history_tools::COGNITION_CHAT_HISTORY_READ,
        ],
        "memory" => CODER_ADVANCED_MEMORY_TOOLS,
        "research" => CODER_RESEARCH_TOOLS,
        "capabilities" => CODER_CAPABILITY_TOOLS,
        "workspace" => CODER_WORKSPACE_TOOLS,
        _ => return None,
    };
    Some(names.iter().map(|name| ToolId::new(name)).collect())
}

fn resolve_coder_domain_id(wire_domain: &str) -> Option<ToolDomainId> {
    CODER_DISCOVERABLE_DOMAIN_IDS
        .iter()
        .copied()
        .find(|domain| domain.as_str() == wire_domain)
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
        let tools = if let Some(catalog) = &self.catalog {
            catalog.definitions_matching(|entry| {
                entry
                    .placement
                    .exposes_mode(crate::tool_catalog::CODER_MODE_ID)
            })
        } else {
            let mut tools = self.inner.list_tools().await?;
            tools.extend(coder_runtime_tool_definitions());
            tools
        };
        tools
            .into_iter()
            .filter_map(|tool| {
                let id = self.resolve_wire_tool_id(tool.name.as_str()).ok()?;
                (coder_tool_allowed(id, &self.policy) && visible.contains(&id)).then_some(tool)
            })
            .map(|tool| {
                with_required_coder_intent(with_coder_tool_advertisement(tool)).map_err(|error| {
                    StasisError::PortFailure(format!("cannot compile Coder tool surface: {error}"))
                })
            })
            .collect()
    }

    async fn invoke_tool(&self, tool_name: &str, input: Value) -> Result<Value> {
        let tool_id = self.resolve_wire_tool_id(tool_name)?;
        if !coder_tool_allowed(tool_id, &self.policy) {
            return Err(StasisError::PortFailure(format!(
                "tool is outside the Coder mode contract: {tool_name}"
            )));
        }
        let visible = self
            .visible_tools
            .lock()
            .map_err(|err| {
                StasisError::PortFailure(format!("Coder visible tool lock poisoned: {err}"))
            })?
            .contains(&tool_id);
        let tool_name = tool_id.as_str();
        let authority = self.authority()?;
        authority.heartbeat()?;
        let (metadata, input) = take_coder_call(input)?;
        let intent = metadata.intent;
        let spawn_intent_hint =
            crate::agent_runtime::turn_worker::TurnWorkerIntent::parse(intent.as_str());
        let input = self.enrich_semantic_input(tool_name, input)?;
        let targets = tool_targets(tool_name, &input, authority.lease());
        let claims = super::coder_claims::infer_tool_claims(
            tool_name,
            &input,
            authority.lease(),
            &self.entry.worktree,
        );
        let memory_boundary = automatic_memory_boundary(tool_name, &claims);
        let admission = match authority.begin_tool_activity(
            tool_name,
            intent.as_str(),
            targets.clone(),
            claims,
        ) {
            Ok(admission) => admission,
            Err(error) => {
                if let Ok(conflict) = serde_json::from_str::<Value>(&error) {
                    authority.append_receipt(json!({
                        "kind": "medousa_coder_tool",
                        "call_id": conflict.get("call_id"),
                        "tool": tool_name,
                        "intent": intent.as_str(),
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
            authority.finish_tool_activity(
                &call_id,
                tool_name,
                intent.as_str(),
                targets,
                Err(&err),
            );
            authority.append_receipt(tool_receipt(
                tool_name,
                intent.as_str(),
                &call_id,
                &input,
                Err(&err),
            ));
            return Err(err);
        }
        let input = match self.bind_input(tool_name, input) {
            Ok(input) => input,
            Err(err) => {
                authority.finish_tool_activity(
                    &call_id,
                    tool_name,
                    intent.as_str(),
                    targets,
                    Err(&err),
                );
                authority.append_receipt(tool_receipt(
                    tool_name,
                    intent.as_str(),
                    &call_id,
                    &Value::Null,
                    Err(&err),
                ));
                return Err(err);
            }
        };
        let mut input = input;
        self.prepare_turn_shell_session(tool_name, &mut input).await;
        if let Err(err) = self.validate_shell_session(tool_name, &input).await {
            authority.finish_tool_activity(
                &call_id,
                tool_name,
                intent.as_str(),
                targets,
                Err(&err),
            );
            authority.append_receipt(tool_receipt(
                tool_name,
                intent.as_str(),
                &call_id,
                &input,
                Err(&err),
            ));
            return Err(err);
        }
        let result = if super::coder_memory::CODER_MEMORY_TOOL_NAMES.contains(&tool_name) {
            self.invoke_coder_memory_tool(&authority, tool_name, &input)
                .await
        } else if tool_name == super::coder_experiments::COGNITION_CODER_EXPERIMENT_COMPARE {
            super::coder_experiments::compare_sealed_candidates(
                authority.forge.as_ref(),
                self.inner.as_ref(),
                &self.entry,
                &input,
            )
            .await
        } else if tool_name == super::coder_semantic_actions::COGNITION_CODER_SYMBOL_REFACTOR {
            super::coder_semantic_actions::invoke_symbol_refactor(
                authority.forge.as_ref(),
                self.change_sets.as_ref(),
                &self.entry,
                authority.lease(),
                &self.policy,
                &input,
            )
            .await
        } else if tool_name == super::coder_semantic_actions::COGNITION_CODER_CHANGE_SET_APPLY {
            super::coder_semantic_actions::apply_change_set(
                self.change_sets.as_ref(),
                authority.lease(),
                &input,
            )
            .await
        } else if tool_name == super::coder_semantic_actions::COGNITION_CODER_AFFECTED_TESTS {
            super::coder_semantic_actions::affected_tests(
                authority.forge.as_ref(),
                &self.entry,
                authority.lease(),
                &input,
            )
            .await
        } else if tool_name == super::coder_causal::COGNITION_CODER_CAUSAL_QUERY {
            super::coder_causal::invoke_causal_query(
                authority.forge.as_ref(),
                self.inner.as_ref(),
                &self.entry,
                &self.engineering_events()?,
                &input,
            )
            .await
        } else if CODER_RUNTIME_TOOLS.contains(&tool_name) {
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
        } else if tool_name == crate::agent_runtime::turn_worker_tools::COGNITION_SPAWN_TURN_WORKER
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
        authority.finish_tool_activity(
            &call_id,
            tool_name,
            intent.as_str(),
            targets,
            result.as_ref(),
        );
        authority.append_receipt(tool_receipt(
            tool_name,
            intent.as_str(),
            &call_id,
            &input,
            result.as_ref(),
        ));
        if let Some(boundary) = memory_boundary {
            self.checkpoint_tool_boundary(
                &authority,
                CoderMemoryCheckpoint {
                    boundary,
                    tool_name,
                    intent: intent.as_str(),
                    call_id: &call_id,
                    input: &input,
                    result: result.as_ref(),
                },
            )
            .await;
        }
        result
    }
}

fn coder_runtime_tool_definitions() -> Vec<Tool> {
    let mut tools = vec![
        Tool::new(COGNITION_CODER_TOOLS_DISCOVER)
            .with_description(
                "Reveal one already-authorized Coder tool pack for this turn. Packs are monotonic and cannot expand Forge authority.",
            )
            .with_schema(json!({
                "type": "object",
                "properties": {
                    "domain": {
                        "type": "string",
                        "enum": CODER_DISCOVERABLE_DOMAINS
                    }
                },
                "required": ["domain"]
            })),
        Tool::new(COGNITION_ENGINEERING_POINTERS)
            .with_description(ENGINEERING_POINTERS_DESCRIPTION)
            .with_schema(
                crate::typed_tools::normalize_input_schema::<EngineeringPointersInput>()
                    .expect("engineering pointers input schema must normalize"),
            ),
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
    ];
    tools.push(super::coder_experiments::tool_definition());
    tools.push(super::coder_causal::tool_definition());
    tools.extend(super::coder_semantic_actions::tool_definitions());
    tools.extend(super::coder_memory::tool_definitions());
    tools
}

fn with_required_coder_intent(
    tool: Tool,
) -> std::result::Result<Tool, crate::typed_tools::ModeToolAdapterError> {
    let replaces_base_intent = tool.name.as_str()
        == crate::agent_runtime::turn_worker_tools::COGNITION_SPAWN_TURN_WORKER
        || crate::turn_control_tools::is_begin_work_tool_name(tool.name.as_str());
    let base_has_intent = tool
        .schema
        .as_ref()
        .and_then(|schema| schema.get("properties"))
        .and_then(Value::as_object)
        .is_some_and(|properties| properties.contains_key("intent"));
    if replaces_base_intent && base_has_intent {
        CODER_MODE_ADAPTER.compose_tool_with_projection(
            tool,
            &crate::typed_tools::ModeInputProjection::replacing(["intent"]),
        )
    } else {
        CODER_MODE_ADAPTER.compose_tool(tool)
    }
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
        "cognition_memory_schema" => tool.with_description(
            "Advanced raw Locus schema diagnostic. Normal Coder memory does not require model-authored STTP; use cognition_coder_memory_commit.",
        ),
        "cognition_memory_store" => tool.with_description(
            "Advanced diagnostic-only raw STTP store. For normal Coder memory writes use cognition_coder_memory_commit; the runtime compiles strict STTP from simple structured fields.",
        ),
        _ => tool,
    }
}

pub(crate) fn register_catalog_placements(index: &mut ToolPlacementIndex) {
    for id in coder_initial_tool_ids() {
        index.add_exposure(
            id,
            ToolExposureRef::new(
                crate::tool_catalog::CODER_MODE_ID,
                crate::tool_catalog::INITIAL_SURFACE_ID,
            ),
        );
    }
    for name in crate::code_intelligence_tools::CODE_COGNITION_TOOLS {
        index.add_exposure(
            ToolId::new(name),
            ToolExposureRef::new(
                crate::tool_catalog::CODER_MODE_ID,
                crate::tool_catalog::EDITOR_CONTEXT_SURFACE_ID,
            ),
        );
    }
    for domain in CODER_DISCOVERABLE_DOMAIN_IDS {
        for id in coder_domain_tool_ids(*domain).expect("known Coder domain") {
            index.add_exposure(
                id,
                ToolExposureRef::domain(
                    crate::tool_catalog::CODER_MODE_ID,
                    crate::tool_catalog::DOMAIN_SURFACE_ID,
                    *domain,
                ),
            );
        }
    }
}

pub(crate) fn register_catalog_runtime_adapters(
    registrar: &mut ToolRegistrar,
) -> std::result::Result<(), crate::typed_tools::ToolCatalogError> {
    for tool in coder_runtime_tool_definitions() {
        let id = resolve_known_coder_tool_id(tool.name.as_str()).ok_or_else(|| {
            crate::typed_tools::ToolCatalogError::UnknownTool(tool.name.as_str().to_string())
        })?;
        let output_schema = (id == COGNITION_ENGINEERING_POINTERS_ID)
            .then(crate::typed_tools::normalize_output_schema::<EngineeringPointersOutput>)
            .transpose()
            .map_err(|_| crate::typed_tools::ToolCatalogError::ContractDrift {
                id,
                field: "output schema normalization",
            })?;
        registrar.register_runtime_adapter(id, tool, output_schema)?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn contract_projected_tools(catalog: &ToolCatalog) -> Vec<Tool> {
    let policy = WorkPolicy::default();
    catalog
        .definitions_matching(|entry| {
            entry
                .placement
                .exposes_mode(crate::tool_catalog::CODER_MODE_ID)
                && coder_tool_allowed(entry.id, &policy)
        })
        .into_iter()
        .map(|tool| with_required_coder_intent(tool).expect("compile Coder metadata"))
        .map(with_coder_tool_advertisement)
        .collect()
}

#[cfg(test)]
pub(crate) fn contract_policy_references() -> HashSet<String> {
    let mut names = coder_visible_tool_ids()
        .into_iter()
        .map(|id| id.as_str().to_string())
        .collect::<HashSet<_>>();
    names.extend(
        GENERAL_MODE_RUNTIME_TOOLS
            .iter()
            .map(|name| (*name).to_string()),
    );
    names.insert(crate::shell_tools::COGNITION_SHELL_RUN.to_string());
    names.insert(crate::shell_tools::COGNITION_SHELL_STATUS.to_string());
    names
}

#[cfg(test)]
pub(crate) fn contract_placement_labels(catalog: &ToolCatalog, tool_name: &str) -> Vec<String> {
    let mut placements = catalog
        .resolve_wire_id(tool_name)
        .ok()
        .and_then(|id| catalog.get(id))
        .into_iter()
        .flat_map(|entry| entry.placement.exposures.iter())
        .filter(|exposure| exposure.mode == crate::tool_catalog::CODER_MODE_ID)
        .map(|exposure| exposure.label())
        .collect::<Vec<_>>();
    placements.sort();
    placements.dedup();
    placements
}

fn coder_initial_tool_ids() -> HashSet<ToolId> {
    crate::coding_tools::CODING_COGNITION_TOOLS
        .iter()
        .chain(TURN_CONTROL_TOOLS.iter())
        .chain(CODER_PEER_SPAWN_TOOLS.iter())
        .chain(super::coder_memory::CODER_MEMORY_TOOL_NAMES.iter())
        .chain(
            [
                COGNITION_CODER_TOOLS_DISCOVER,
                COGNITION_ENGINEERING_POINTERS,
                COGNITION_ENGINEERING_POINTER_FOLLOW,
                COGNITION_CODER_EVIDENCE_READ,
            ]
            .iter(),
        )
        .copied()
        .map(ToolId::new)
        .collect()
}

fn coder_visible_tool_ids() -> HashSet<ToolId> {
    let mut ids = coder_initial_tool_ids();
    ids.extend(
        crate::code_intelligence_tools::CODE_COGNITION_TOOLS
            .iter()
            .map(|name| ToolId::new(name)),
    );
    for domain in CODER_DISCOVERABLE_DOMAIN_IDS {
        ids.extend(coder_domain_tool_ids(*domain).into_iter().flatten());
    }
    ids
}

fn resolve_known_coder_tool_id(wire_name: &str) -> Option<ToolId> {
    crate::tool_names::registered_cognition_tools()
        .map(ToolId::new)
        .chain(coder_visible_tool_ids())
        .chain(GENERAL_MODE_RUNTIME_TOOLS.iter().copied().map(ToolId::new))
        .chain(
            [
                crate::shell_tools::COGNITION_SHELL_RUN,
                crate::shell_tools::COGNITION_SHELL_STATUS,
            ]
            .into_iter()
            .map(ToolId::new),
        )
        .find(|id| id.as_str() == wire_name)
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

fn take_coder_call(input: Value) -> Result<(CoderCallMetadata, Value)> {
    CODER_MODE_ADAPTER.split_call(input).map_err(|error| {
        let message = match &error {
            crate::typed_tools::ModeToolAdapterError::CallInputMustBeObject => {
                "Coder tools require an object input".to_string()
            }
            crate::typed_tools::ModeToolAdapterError::InvalidMetadata(message)
                if message.contains("missing field `intent`")
                    || message.contains("invalid type") =>
            {
                "Coder tool intent is required".to_string()
            }
            _ => error.to_string(),
        };
        StasisError::PortFailure(message)
    })
}

fn tool_targets(tool_name: &str, input: &Value, lease: &ExecutionLease) -> Vec<String> {
    let mut targets = Vec::new();
    if let Some(path) = input.get("path").and_then(Value::as_str) {
        targets.push(format!("file://{}", path.trim()));
    }
    if let Some(uri) = input.get("uri").and_then(Value::as_str) {
        targets.push(uri.trim().to_string());
    }
    if let Some(paths) = input.get("paths").and_then(Value::as_array) {
        targets.extend(
            paths
                .iter()
                .filter_map(Value::as_str)
                .map(|path| format!("file://{}", path.trim())),
        );
    }
    if let Some(change_set_id) = input.get("change_set_id").and_then(Value::as_str) {
        targets.push(change_set_id.trim().to_string());
    }
    if super::coder_memory::CODER_MEMORY_TOOL_NAMES.contains(&tool_name)
        || tool_name.starts_with("cognition_memory_")
    {
        targets.push(format!("work://{}/memory", lease.work_id));
    } else if let Some(session_id) = input.get("session_id").and_then(Value::as_str) {
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
    if tool_name == super::coder_causal::COGNITION_CODER_CAUSAL_QUERY {
        targets.push(format!("work://{}/causal", lease.work_id));
    }
    targets.sort();
    targets.dedup();
    targets.truncate(8);
    targets
}

fn memory_checkpoint_paths(input: &Value, worktree: &Path) -> Vec<String> {
    let mut candidates = Vec::new();
    if let Some(path) = input.get("path").and_then(Value::as_str) {
        candidates.push(path);
    }
    if let Some(uri) = input
        .get("uri")
        .and_then(Value::as_str)
        .and_then(|uri| uri.strip_prefix("file://"))
    {
        candidates.push(uri);
    }
    if let Some(paths) = input.get("paths").and_then(Value::as_array) {
        candidates.extend(paths.iter().filter_map(Value::as_str));
    }
    let mut paths = candidates
        .into_iter()
        .filter_map(|candidate| {
            let path = Path::new(candidate.trim());
            if path.is_absolute() {
                path.strip_prefix(worktree)
                    .ok()
                    .and_then(|relative| normalize_relative_path(&relative.to_string_lossy()).ok())
            } else {
                normalize_relative_path(candidate).ok()
            }
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    paths.truncate(20);
    paths
}

fn memory_boundary_summary(
    boundary: CoderMemoryBoundary,
    succeeded: bool,
    intent: &str,
    input: &Value,
) -> String {
    let preferred_fields: &[&str] = match boundary {
        CoderMemoryBoundary::Handoff => &["progress_summary", "goal", "task", "message"],
        CoderMemoryBoundary::Budget => &["progress_summary", "reason"],
        CoderMemoryBoundary::Terminal => &["message", "reason"],
        CoderMemoryBoundary::Change | CoderMemoryBoundary::Verification => &[],
    };
    let state = preferred_fields
        .iter()
        .find_map(|field| input.get(*field).and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| truncate(value, 1_600))
        .unwrap_or_else(|| intent.to_string());
    format!("{}: {state}", boundary.summary_prefix(succeeded))
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
                "stable_object_id": output.pointer("/change_set/id")
                    .or_else(|| output.pointer("/action/id"))
                    .or_else(|| output.pointer("/selection/id"))
                    .or_else(|| output.get("workflow")),
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

fn bounded_memory_error(value: &str) -> String {
    truncate(&super::coder_evidence::redact_evidence_text(value), 500)
}

#[cfg(test)]
mod tests {
    use super::*;
    use medousa_forge::git::{CheckpointAuthor, GitEngine};
    use medousa_forge::model::{ExecutorDescriptor, WorkState};
    use std::sync::Mutex as StdMutex;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tempfile::TempDir;

    #[derive(Default)]
    struct RecordingRegistry {
        last_input: StdMutex<Option<Value>>,
        invoked_tools: StdMutex<Vec<String>>,
        invocations: StdMutex<Vec<(String, Value)>>,
        memory_nodes: StdMutex<Vec<Value>>,
        memory_unavailable: AtomicBool,
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
                Tool::new("cognition_memory_list"),
                Tool::new("cognition_memory_recall"),
                Tool::new("cognition_memory_store"),
                Tool::new("cognition_web_search"),
                Tool::new("cognition_mcp_discover"),
                Tool::new("cognition_mcp_invoke"),
                Tool::new("cognition_runtime_jobs_cancel"),
                Tool::new("cognition_spawn_turn_worker").with_description("Host spawn worker"),
                Tool::new("cognition_turn_begin_work").with_description("Enter bound Workshop"),
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
            self.invocations
                .lock()
                .expect("invocations lock")
                .push((tool_name.to_string(), input.clone()));
            if tool_name.starts_with("cognition_memory_")
                && self.memory_unavailable.load(Ordering::SeqCst)
            {
                return Err(StasisError::PortFailure(
                    "simulated Locus outage".to_string(),
                ));
            }
            if tool_name == "cognition_memory_list" || tool_name == "cognition_memory_recall" {
                let session_id = input.get("session_id").and_then(Value::as_str);
                let required_tags = input
                    .get("semantic_tags")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>();
                let limit = input.get("limit").and_then(Value::as_u64).unwrap_or(20) as usize;
                let nodes = self
                    .memory_nodes
                    .lock()
                    .expect("memory nodes")
                    .iter()
                    .filter(|node| {
                        session_id.is_none_or(|session_id| {
                            node.get("session_id").and_then(Value::as_str) == Some(session_id)
                        }) && required_tags.iter().all(|required| {
                            node.get("semantic_tags")
                                .and_then(Value::as_array)
                                .is_some_and(|tags| {
                                    tags.iter().any(|tag| tag.as_str() == Some(required))
                                })
                        })
                    })
                    .take(limit)
                    .cloned()
                    .collect::<Vec<_>>();
                Ok(json!({ "retrieved": nodes.len(), "nodes": nodes }))
            } else if tool_name == "cognition_memory_store" {
                let session_id = input
                    .get("session_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let raw = input
                    .get("node")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let parsed = locus_core_rs::SttpNodeParser::new().try_parse(raw, session_id);
                let context_summary = parsed
                    .node
                    .as_ref()
                    .and_then(|node| node.context_summary.clone());
                let mut nodes = self.memory_nodes.lock().expect("memory nodes");
                let node_id = format!("memory-node-{}", nodes.len() + 1);
                nodes.push(json!({
                    "sync_key": node_id,
                    "session_id": session_id,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                    "context_summary": context_summary,
                    "semantic_tags": input.get("semantic_tags").cloned().unwrap_or_else(|| json!([])),
                    "raw": raw,
                }));
                Ok(json!({
                    "node_id": node_id,
                    "valid": parsed.success,
                    "stored": parsed.success,
                    "validation_error": parsed.error,
                }))
            } else if tool_name == crate::coding_tools::COGNITION_SHELL_SESSION_STATUS
                || tool_name == crate::coding_tools::COGNITION_CODER_SHELL_RUN
                || tool_name == crate::coding_tools::COGNITION_CODER_SHELL_STATUS
            {
                Ok(json!({
                    "ok": true,
                    "session_id": "shell-1",
                    "next_sequence": 17,
                    "input": input
                }))
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
        let invoked = inner_a.invoked_tools.lock().expect("tools lock");
        assert_eq!(
            invoked.first().map(String::as_str),
            Some(crate::coding_tools::COGNITION_CODE_APPLY_PATCH)
        );
        assert!(invoked.iter().any(|tool| tool == "cognition_memory_store"));
    }

    #[tokio::test]
    async fn surface_exposes_only_the_coder_bootstrap_until_discovery() {
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
            registry.visible_tools.lock().expect("visible tools").len() <= 24,
            "Coder bootstrap schema must remain intentionally bounded"
        );
        for memory_tool in super::super::coder_memory::CODER_MEMORY_TOOL_NAMES {
            assert!(
                tools.iter().any(|tool| tool.name.as_str() == *memory_tool),
                "Coder memory bootstrap tool missing: {memory_tool}"
            );
        }
        let memory_commit = tools
            .iter()
            .find(|tool| {
                tool.name.as_str() == super::super::coder_memory::COGNITION_CODER_MEMORY_COMMIT
            })
            .expect("typed memory commit visible");
        let memory_schema = memory_commit.schema.as_ref().expect("memory commit schema");
        let properties = memory_schema["properties"]
            .as_object()
            .expect("memory commit properties");
        for runtime_owned in [
            "node",
            "session_id",
            "semantic_tags",
            "origin_session",
            "timestamp",
            "user_avec",
            "model_avec",
        ] {
            assert!(
                !properties.contains_key(runtime_owned),
                "runtime-owned STTP field leaked into the model schema: {runtime_owned}"
            );
        }
        let required = memory_schema["required"]
            .as_array()
            .expect("memory commit required fields");
        for field in ["intent", "kind", "summary"] {
            assert!(
                required.iter().any(|value| value.as_str() == Some(field)),
                "simple memory commit requirement missing: {field}"
            );
        }
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
        for hidden in [
            "cognition_vault_write",
            "cognition_memory_recall",
            "cognition_memory_store",
            "cognition_web_search",
            "cognition_mcp_discover",
            "cognition_mcp_invoke",
        ] {
            assert!(
                tools.iter().all(|tool| tool.name.as_str() != hidden),
                "unselected tool leaked into the Coder bootstrap: {hidden}"
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

        let hidden_memory = registry
            .invoke_tool(
                "cognition_memory_recall",
                json!({
                    "intent": "Recall the user's established implementation preferences",
                    "session_id": "attacker-controlled-session",
                    "query": "implementation preferences"
                }),
            )
            .await
            .expect_err("undiscovered memory tool denied");
        assert!(hidden_memory.to_string().contains("not visible"));

        registry
            .invoke_tool(
                COGNITION_CODER_TOOLS_DISCOVER,
                json!({
                    "intent": "Reveal bounded Locus tools for explicit memory recall",
                    "domain": "memory"
                }),
            )
            .await
            .expect("discover memory");
        let after_memory = registry.list_tools().await.expect("list after memory");
        for memory_tool in ["cognition_memory_recall", "cognition_memory_store"] {
            assert!(
                after_memory
                    .iter()
                    .any(|tool| tool.name.as_str() == memory_tool),
                "discovered memory tool missing: {memory_tool}"
            );
        }
        let raw_store = after_memory
            .iter()
            .find(|tool| tool.name.as_str() == "cognition_memory_store")
            .expect("advanced raw store visible");
        assert!(
            raw_store
                .description
                .as_deref()
                .is_some_and(|description| description.contains("cognition_coder_memory_commit")),
            "advanced raw store must direct Coder back to the typed facade"
        );
        assert!(
            after_memory
                .iter()
                .all(|tool| tool.name.as_str() != "cognition_vault_write"),
            "discovering memory must not reveal the workspace pack"
        );

        registry
            .invoke_tool(
                "cognition_memory_recall",
                json!({
                    "intent": "Recall the user's established implementation preferences",
                    "session_id": "attacker-controlled-session",
                    "query": "implementation preferences"
                }),
            )
            .await
            .expect("discovered memory tool");
        let input = inner
            .last_input
            .lock()
            .expect("input lock")
            .clone()
            .expect("recorded memory input");
        assert_eq!(input["query"], "implementation preferences");
        assert_eq!(
            input["session_id"],
            super::super::coder_memory::CoderMemoryScope::for_entry(&fixture.entry).session_id
        );
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
    async fn exact_restore_replaces_visible_surface_without_expanding_authority() {
        let fixture = fixture();
        let authority = authority(&fixture);
        let inner = Arc::new(RecordingRegistry::default());
        let registry = CoderBoundToolRegistry::new(
            inner,
            &authority,
            fixture.entry.clone(),
            fixture.policy.clone(),
        );

        registry
            .restore_checkpoint_surface(
                &[
                    crate::coding_tools::COGNITION_CODE_READ.to_string(),
                    "cognition_runtime_jobs_cancel".to_string(),
                ],
                Some("node-42"),
            )
            .expect("restore checkpoint surface");

        assert_eq!(
            registry.checkpoint_visible_tools().unwrap(),
            vec![crate::coding_tools::COGNITION_CODE_READ.to_string()]
        );
        assert_eq!(
            registry.checkpoint_memory_cursor().unwrap().as_deref(),
            Some("node-42")
        );
        let tools = registry.list_tools().await.expect("list restored tools");
        assert_eq!(tools.len(), 1);
        assert_eq!(
            tools[0].name.as_str(),
            crate::coding_tools::COGNITION_CODE_READ
        );
    }

    #[tokio::test]
    async fn coder_memory_facade_pins_environment_scope_and_dedupes_commits() {
        let fixture = fixture();
        let authority = authority(&fixture);
        let inner = Arc::new(RecordingRegistry::default());
        let registry = CoderBoundToolRegistry::new(
            inner.clone(),
            &authority,
            fixture.entry.clone(),
            fixture.policy.clone(),
        );
        let commit_input = json!({
            "intent": "Preserve the selected runtime boundary for another Coder agent",
            "session_id": "attacker-controlled-session",
            "kind": "decision",
            "summary": "Keep semantic memory separate from exact turn checkpoints",
            "details": "Locus stores explicit engineering state while the turn checkpoint owns provider protocol recovery.",
            "paths": ["src/agent_runtime/coder_memory.rs"],
            "relations": [{
                "rel": "supports",
                "target": "decision:durable-coder",
                "confidence": 0.98
            }]
        });

        let first = registry
            .invoke_tool(
                super::super::coder_memory::COGNITION_CODER_MEMORY_COMMIT,
                commit_input.clone(),
            )
            .await
            .expect("commit memory");
        assert_eq!(first["ok"], true);
        assert_eq!(first["stored"], true);
        assert_eq!(first["duplicate"], false);
        assert_eq!(first["scope"]["repo_id"], fixture.entry.repo_id);
        assert_eq!(first["scope"]["work_id"], fixture.entry.work_id);
        assert!(!first.to_string().contains("attacker-controlled-session"));

        let stored_input = inner
            .last_input
            .lock()
            .expect("last input")
            .clone()
            .expect("stored input");
        let pinned_session = stored_input["session_id"]
            .as_str()
            .expect("pinned Locus session")
            .to_string();
        assert_ne!(pinned_session, "attacker-controlled-session");
        assert!(pinned_session.contains("coder:"));
        assert_eq!(
            stored_input["node"]
                .as_str()
                .map(|node| node.contains(&pinned_session)),
            Some(true)
        );
        let mismatched_raw = stored_input["node"]
            .as_str()
            .expect("stored STTP")
            .replace(&pinned_session, "another-locus-session");
        let scope_error = registry
            .bind_input("cognition_memory_store", json!({ "node": mismatched_raw }))
            .expect_err("raw STTP cannot escape the environment scope");
        assert!(
            scope_error
                .to_string()
                .contains("governed Coder environment")
        );

        let second = registry
            .invoke_tool(
                super::super::coder_memory::COGNITION_CODER_MEMORY_COMMIT,
                commit_input,
            )
            .await
            .expect("dedupe memory");
        assert_eq!(second["ok"], true);
        assert_eq!(second["stored"], false);
        assert_eq!(second["duplicate"], true);
        assert_eq!(
            inner
                .invoked_tools
                .lock()
                .expect("invoked tools")
                .iter()
                .filter(|tool| tool.as_str() == "cognition_memory_store")
                .count(),
            1
        );

        let recalled = registry
            .invoke_tool(
                super::super::coder_memory::COGNITION_CODER_MEMORY_RECALL,
                json!({
                    "intent": "Recover the prior boundary decision from this worktree",
                    "session_id": "attacker-controlled-session",
                    "query": "semantic memory exact turn checkpoints",
                    "kind": "decision",
                    "path": "src/agent_runtime/coder_memory.rs",
                    "limit": 4
                }),
            )
            .await
            .expect("recall memory");
        assert_eq!(recalled["retrieved"], 1);
        assert_eq!(recalled["nodes"][0]["kind"], "decision");
        assert_eq!(recalled["nodes"][0]["stale"], false);
        assert!(
            recalled["nodes"][0]["content"]
                .as_str()
                .is_some_and(|content| content.contains("semantic memory separate"))
        );
        assert_eq!(
            recalled["nodes"][0]["relations"][0]["target"],
            "decision:durable-coder"
        );
        assert!(!recalled.to_string().contains(&pinned_session));
        assert!(!recalled.to_string().contains("attacker-controlled-session"));

        let recall_input = inner
            .invocations
            .lock()
            .expect("invocations")
            .iter()
            .rev()
            .find(|(tool, input)| {
                tool == "cognition_memory_recall"
                    && input.get("session_id").and_then(Value::as_str)
                        == Some(pinned_session.as_str())
            })
            .map(|(_, input)| input.clone())
            .expect("environment recall input");
        assert_eq!(recall_input["session_id"], pinned_session);
        assert_eq!(
            recall_input["semantic_tags"],
            json!(["kind:decision", "path:src/agent_runtime/coder_memory.rs"])
        );
    }

    #[tokio::test]
    async fn accepted_promotion_excludes_transient_nodes_and_uses_stable_scopes() {
        let fixture = fixture();
        let inner = Arc::new(RecordingRegistry::default());
        let source_scope = super::super::coder_memory::CoderMemoryScope::for_entry(&fixture.entry);
        let identity = CoderAgentIdentity::for_turn("chat-1", 1, "attempt-1");
        for (kind, summary) in [
            ("decision", "Keep the typed memory facade"),
            ("hypothesis", "A transient sibling theory"),
        ] {
            let commit = super::super::coder_memory::build_commit(
                &json!({ "kind": kind, "summary": summary }),
                &source_scope,
                &identity,
                &fixture.entry.head_oid,
            )
            .expect("source memory commit");
            try_store_memory_commit_with_registry(inner.as_ref(), &source_scope, &commit)
                .await
                .expect("store source memory");
        }
        let queued_verification = super::super::coder_memory::build_commit(
            &json!({ "kind": "verification", "summary": "Focused tests pass" }),
            &source_scope,
            &identity,
            &fixture.entry.head_oid,
        )
        .expect("queued source verification");
        let source_queue = shared_memory_retry_queue(&source_scope);
        source_queue
            .lock()
            .await
            .enqueue(queued_verification)
            .expect("queue source verification");
        for (kind, summary) in [
            ("decision", "A stale rejected design"),
            ("verification", "Checks from an older HEAD"),
        ] {
            let commit = super::super::coder_memory::build_commit(
                &json!({ "kind": kind, "summary": summary }),
                &source_scope,
                &identity,
                "stale-head",
            )
            .expect("stale source memory commit");
            try_store_memory_commit_with_registry(inner.as_ref(), &source_scope, &commit)
                .await
                .expect("store stale source memory");
        }

        let task = super::super::coder_memory::CoderMemoryPromotionTask::new(
            source_scope.repo_id.clone(),
            source_scope.work_id.clone(),
            source_scope.branch.clone(),
            source_scope.environment_generation,
            fixture.entry.head_oid.clone(),
            "decision-1",
            "attempt-1",
            "evidence-1",
            "digest-1",
        );
        let report = promote_memory_task(inner.as_ref(), &task)
            .await
            .expect("promote accepted memory");
        assert_eq!(report["decision_candidates"], 1);
        assert_eq!(report["verification_candidates"], 1);
        assert_eq!(source_queue.lock().await.len(), 0);

        let undertaking_session = source_scope.accepted_undertaking_scope().session_id;
        let repository_session = source_scope.accepted_repository_scope().session_id;
        let nodes = inner.memory_nodes.lock().expect("memory nodes");
        let undertaking = nodes
            .iter()
            .filter(|node| node["session_id"] == undertaking_session)
            .collect::<Vec<_>>();
        let repository = nodes
            .iter()
            .filter(|node| node["session_id"] == repository_session)
            .collect::<Vec<_>>();
        assert_eq!(undertaking.len(), 2);
        assert_eq!(repository.len(), 1);
        assert!(undertaking.iter().all(|node| {
            node["semantic_tags"]
                .as_array()
                .is_some_and(|tags| tags.iter().any(|tag| tag == "knowledge:accepted"))
        }));
        assert!(
            repository[0]["semantic_tags"]
                .as_array()
                .is_some_and(|tags| tags.iter().any(|tag| tag == "kind:verification"))
        );
        assert!(nodes.iter().all(|node| {
            node["session_id"] == source_scope.session_id
                || !node["context_summary"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("transient sibling theory")
        }));
        let promoted = undertaking
            .iter()
            .chain(repository.iter())
            .map(|node| node["context_summary"].as_str().unwrap_or_default())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(!promoted.contains("stale rejected design"));
        assert!(!promoted.contains("older HEAD"));
    }

    #[tokio::test]
    async fn discarded_undertaking_appends_an_archive_marker() {
        let fixture = fixture();
        let inner = Arc::new(RecordingRegistry::default());
        let work_id = medousa_forge::model::WorkId::from(fixture.entry.work_id.clone());
        let item = fixture
            .forge
            .discard(&work_id, &Forge::system_actor())
            .expect("discard undertaking");

        let report =
            finalize_coder_memory_lineage(inner.clone(), fixture.forge.clone(), &item, None).await;
        assert_eq!(report["ok"], true);
        assert_eq!(report["terminal_state"], "discarded");
        assert_eq!(report["archived_environments"], 1);

        let nodes = inner.memory_nodes.lock().expect("memory nodes");
        assert_eq!(nodes.len(), 1);
        let tags = nodes[0]["semantic_tags"].as_array().expect("archive tags");
        assert!(tags.iter().any(|tag| tag == "lineage:archived"));
        assert!(tags.iter().any(|tag| tag == "terminal:discarded"));
        assert!(tags.iter().any(|tag| tag == "kind:checkpoint"));
    }

    #[tokio::test]
    async fn automatic_checkpoint_survives_locus_outage_and_flushes_once() {
        let fixture = fixture();
        let authority = authority(&fixture);
        let inner = Arc::new(RecordingRegistry::default());
        inner.memory_unavailable.store(true, Ordering::SeqCst);
        let registry = CoderBoundToolRegistry::new(
            inner.clone(),
            &authority,
            fixture.entry.clone(),
            fixture.policy.clone(),
        );

        let patch_result = registry
            .invoke_tool(
                crate::coding_tools::COGNITION_CODE_APPLY_PATCH,
                json!({
                    "intent": "Update the demo implementation",
                    "path": "src/lib.rs",
                    "expected_sha256": "missing",
                    "content": "pub fn demo() { println!(\"updated\"); }\n"
                }),
            )
            .await
            .expect("Locus outage must not fail the patch tool");
        assert_eq!(patch_result["ok"], true);
        assert_eq!(registry.memory_retry_queue.lock().await.len(), 1);
        assert!(inner.memory_nodes.lock().expect("memory nodes").is_empty());

        inner.memory_unavailable.store(false, Ordering::SeqCst);
        let flushed = registry.flush_memory_queue().await;
        assert_eq!(flushed["ok"], true);
        assert_eq!(flushed["flushed"], 1);
        assert_eq!(flushed["pending_writes"], 0);
        {
            let nodes = inner.memory_nodes.lock().expect("memory nodes");
            assert_eq!(nodes.len(), 1);
            assert!(
                nodes[0]["semantic_tags"]
                    .as_array()
                    .is_some_and(|tags| tags.iter().any(|tag| tag == "kind:change"))
            );
        }

        let second_flush = registry.flush_memory_queue().await;
        assert_eq!(second_flush["flushed"], 0);
        assert_eq!(
            inner
                .invoked_tools
                .lock()
                .expect("invoked tools")
                .iter()
                .filter(|tool| tool.as_str() == "cognition_memory_store")
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn turn_entry_uses_pending_memory_without_requiring_locus() {
        let fixture = fixture();
        let authority = authority(&fixture);
        let inner = Arc::new(RecordingRegistry::default());
        let registry = CoderBoundToolRegistry::new(
            inner.clone(),
            &authority,
            fixture.entry.clone(),
            fixture.policy.clone(),
        );
        let head = fixture.entry.head_oid.clone();
        for input in [
            json!({
                "kind": "open_gap",
                "summary": "Parser symbol still needs focused inspection",
                "symbols": ["demo::run"]
            }),
            json!({
                "kind": "change",
                "summary": "A prior agent changed the parser boundary",
                "paths": ["src/lib.rs"]
            }),
        ] {
            let commit = super::super::coder_memory::build_commit(
                &input,
                &registry.memory_scope,
                &authority.identity,
                &head,
            )
            .expect("pending memory commit");
            registry
                .memory_retry_queue
                .lock()
                .await
                .enqueue(commit)
                .expect("queue pending memory");
        }
        inner.memory_unavailable.store(true, Ordering::SeqCst);

        let appendix = registry
            .initial_prompt_appendix()
            .await
            .expect("Coder entry remains available without Locus");
        assert!(appendix.contains("memory_status"));
        assert!(appendix.contains("unavailable"));
        assert!(appendix.contains("Parser symbol still needs focused inspection"));
        assert!(appendix.contains("A prior agent changed the parser boundary"));
        assert!(!appendix.contains(&registry.memory_scope.session_id));

        let tools = registry.list_tools().await.expect("memory-informed tools");
        assert!(tools.iter().any(|tool| {
            tool.name.as_str() == crate::code_intelligence_tools::COGNITION_CODE_DIAGNOSTICS
        }));
        assert!(
            tools
                .iter()
                .any(|tool| { tool.name.as_str() == crate::detamu_tools::COGNITION_DETAMU_STATUS })
        );
    }

    #[test]
    fn automatic_memory_boundaries_are_explicit_and_bounded() {
        let verify = CoderClaimScope {
            target: "tree://work".into(),
            mode: super::super::coder_claims::CoderClaimMode::Verify,
            hazardous: false,
            reason: "focused verification".into(),
        };
        assert_eq!(
            automatic_memory_boundary(crate::coding_tools::COGNITION_SHELL_SESSION_RUN, &[verify]),
            Some(CoderMemoryBoundary::Verification)
        );
        assert_eq!(
            automatic_memory_boundary(crate::turn_control_tools::COGNITION_TURN_CHECKPOINT, &[]),
            Some(CoderMemoryBoundary::Handoff)
        );
        assert_eq!(
            automatic_memory_boundary(
                crate::turn_control_tools::COGNITION_TURN_REQUEST_MORE_ROUNDS,
                &[]
            ),
            Some(CoderMemoryBoundary::Budget)
        );
        assert_eq!(
            automatic_memory_boundary(crate::turn_control_tools::COGNITION_TURN_FINISH, &[]),
            Some(CoderMemoryBoundary::Terminal)
        );
        assert_eq!(
            automatic_memory_boundary(crate::coding_tools::COGNITION_CODE_READ, &[]),
            None
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

    #[test]
    fn typed_coder_envelope_separates_metadata_from_pointer_input() {
        let (metadata, input) = take_coder_call(json!({
            "intent": "  Inspect   the ranked engineering context  ",
            "limit": 4
        }))
        .expect("typed Coder envelope");
        let intent = metadata.intent;
        assert_eq!(intent.as_str(), "Inspect the ranked engineering context");
        assert_eq!(input, json!({ "limit": 4 }));
        assert!(input.get("intent").is_none());

        let base_schema = crate::typed_tools::normalize_input_schema::<EngineeringPointersInput>()
            .expect("pointer schema");
        assert_eq!(
            base_schema["properties"]
                .as_object()
                .expect("pointer properties")
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            vec!["limit"]
        );
        for runtime_owned in [
            "intent",
            "work_id",
            "attempt_id",
            "lease_id",
            "lease_generation",
            "root",
        ] {
            assert!(base_schema["properties"].get(runtime_owned).is_none());
        }

        let projected = with_required_coder_intent(
            Tool::new(COGNITION_ENGINEERING_POINTERS).with_schema(base_schema),
        )
        .expect("compose Coder metadata");
        let projected_schema = projected.schema.expect("projected pointer schema");
        assert_eq!(projected_schema["properties"]["intent"]["maxLength"], 320);
        assert!(
            projected_schema["required"]
                .as_array()
                .expect("required metadata")
                .iter()
                .any(|field| field == "intent")
        );
        crate::typed_tools::normalize_output_schema::<EngineeringPointersOutput>()
            .expect("typed pointer output schema");

        for invalid in [json!({ "limit": 4 }), json!({ "intent": 42, "limit": 4 })] {
            let error = take_coder_call(invalid).expect_err("intent must be a string");
            assert!(error.to_string().contains("Coder tool intent is required"));
        }
    }

    #[tokio::test]
    async fn typed_pointer_call_records_one_intent_across_its_lifecycle() {
        let fixture = fixture();
        let authority = authority(&fixture);
        let inner = Arc::new(RecordingRegistry::default());
        let registry = CoderBoundToolRegistry::new(
            inner.clone(),
            &authority,
            fixture.entry.clone(),
            fixture.policy.clone(),
        );
        let intent = "Inspect ranked engineering context before the next change";

        let output = registry
            .invoke_tool(
                COGNITION_ENGINEERING_POINTERS,
                json!({ "intent": intent, "limit": 4 }),
            )
            .await
            .expect("typed pointer call");
        assert_eq!(output["ok"], true);
        assert!(output["pointers"].is_array());
        assert!(
            inner.invoked_tools.lock().expect("tools lock").is_empty(),
            "mode metadata and runtime handling must not leak into the base registry"
        );

        let events = fixture
            .activity
            .events_for_work(&fixture.entry.work_id)
            .expect("pointer lifecycle");
        let pointer_events = events
            .iter()
            .filter(|event| event.tool.as_deref() == Some(COGNITION_ENGINEERING_POINTERS))
            .collect::<Vec<_>>();
        assert!(pointer_events.iter().any(|event| {
            event.kind == super::super::coder_activity::CoderActivityKind::ToolPlanned
        }));
        assert!(pointer_events.iter().any(|event| {
            event.kind == super::super::coder_activity::CoderActivityKind::ToolCompleted
        }));
        assert!(
            pointer_events
                .iter()
                .all(|event| event.intent.as_deref() == Some(intent))
        );
    }

    #[test]
    fn engineering_pointer_wire_optionals_remain_lenient_for_legacy_values() {
        let input: EngineeringPointersInput = serde_json::from_value(json!({
            "limit": "24",
        }))
        .expect("engineering pointer input");
        assert!(input.limit.into_option().is_none());
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
                .all(|tool| tool.name.as_str() != "cognition_memory_recall")
        );
        assert!(
            after
                .iter()
                .all(|tool| tool.name.as_str() != "cognition_runtime_jobs_cancel")
        );

        let experiments = registry
            .invoke_tool(
                COGNITION_CODER_TOOLS_DISCOVER,
                json!({
                    "intent": "Reveal sealed candidate comparison for the experiment review",
                    "domain": "experiments"
                }),
            )
            .await
            .expect("discover experiments");
        assert!(
            experiments["newly_visible"]
                .as_array()
                .is_some_and(|tools| {
                    tools.iter().any(|tool| {
                        tool.as_str()
                            == Some(
                                super::super::coder_experiments::COGNITION_CODER_EXPERIMENT_COMPARE,
                            )
                    })
                })
        );
        let after = registry.list_tools().await.expect("experiment tools");
        let compare = after
            .iter()
            .find(|tool| {
                tool.name.as_str()
                    == super::super::coder_experiments::COGNITION_CODER_EXPERIMENT_COMPARE
            })
            .expect("comparison tool visible");
        let required = compare
            .schema
            .as_ref()
            .and_then(|schema| schema.get("required"))
            .and_then(Value::as_array)
            .expect("comparison required fields");
        assert_eq!(required, &vec![Value::String("intent".into())]);

        for (domain, expected_tool) in [
            (
                "semantic_actions",
                super::super::coder_semantic_actions::COGNITION_CODER_SYMBOL_REFACTOR,
            ),
            (
                "causal",
                super::super::coder_causal::COGNITION_CODER_CAUSAL_QUERY,
            ),
        ] {
            let discovered = registry
                .invoke_tool(
                    COGNITION_CODER_TOOLS_DISCOVER,
                    json!({
                        "intent": format!("Reveal the governed {domain} workflow"),
                        "domain": domain,
                    }),
                )
                .await
                .expect("discover final-slice domain");
            assert!(discovered["newly_visible"].as_array().is_some_and(|tools| {
                tools
                    .iter()
                    .any(|tool| tool.as_str() == Some(expected_tool))
            }));
        }
        let after = registry
            .list_tools()
            .await
            .expect("semantic and causal tools");
        let apply = after
            .iter()
            .find(|tool| {
                tool.name.as_str()
                    == super::super::coder_semantic_actions::COGNITION_CODER_CHANGE_SET_APPLY
            })
            .expect("change-set apply visible");
        let properties = apply
            .schema
            .as_ref()
            .and_then(|schema| schema.get("properties"))
            .and_then(Value::as_object)
            .expect("change-set schema properties");
        for runtime_owned in [
            "paths",
            "preconditions",
            "operations",
            "lease_id",
            "generation",
            "work_id",
            "attempt_id",
        ] {
            assert!(
                !properties.contains_key(runtime_owned),
                "runtime-owned semantic field leaked into model schema: {runtime_owned}"
            );
        }
    }

    #[tokio::test]
    async fn experiment_notebook_state_auto_reveals_only_the_comparison_tool() {
        let fixture = fixture();
        let authority = authority(&fixture);
        let registry = CoderBoundToolRegistry::new(
            Arc::new(RecordingRegistry::default()),
            &authority,
            fixture.entry.clone(),
            fixture.policy.clone(),
        );
        let before = registry.list_tools().await.expect("initial tools");
        assert!(before.iter().all(|tool| {
            tool.name.as_str()
                != super::super::coder_experiments::COGNITION_CODER_EXPERIMENT_COMPARE
        }));

        registry
            .invoke_tool(
                super::super::coder_memory::COGNITION_CODER_MEMORY_COMMIT,
                json!({
                    "intent": "Record the criterion before comparing candidates",
                    "kind": "acceptance_criterion",
                    "summary": "Tests stay green"
                }),
            )
            .await
            .expect("commit experiment notebook state");
        let after = registry.list_tools().await.expect("experiment tools");
        assert!(after.iter().any(|tool| {
            tool.name.as_str()
                == super::super::coder_experiments::COGNITION_CODER_EXPERIMENT_COMPARE
        }));
        assert!(
            after
                .iter()
                .all(|tool| tool.name.as_str() != COGNITION_ENGINEERING_HISTORY)
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
            ToolId::new(crate::shell_tools::COGNITION_SHELL_RUN),
            &policy
        ));
        assert!(!coder_tool_allowed(
            ToolId::new(crate::shell_tools::COGNITION_SHELL_STATUS),
            &policy
        ));
        assert!(coder_tool_allowed(
            ToolId::new(crate::coding_tools::COGNITION_CODER_SHELL_RUN),
            &policy
        ));
        assert!(coder_tool_allowed(
            ToolId::new(crate::coding_tools::COGNITION_CODER_SHELL_STATUS),
            &policy
        ));
        assert!(coder_tool_allowed(
            ToolId::new("cognition_spawn_turn_worker"),
            &policy
        ));
        assert!(!coder_tool_allowed(
            ToolId::new("cognition_workshop_steer"),
            &policy
        ));
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

        let goal_only =
            remap_begin_work_to_spawn_input(&json!({ "goal": "Write a focused unit test" }), None)
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
        let invoked = inner.invoked_tools.lock().expect("tools");
        assert_eq!(
            invoked.first().map(String::as_str),
            Some("cognition_spawn_turn_worker")
        );
        assert!(invoked.iter().any(|tool| tool == "cognition_memory_store"));
        drop(invoked);
        let input = &out["input"];
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
        let input = inner
            .last_input
            .lock()
            .expect("input")
            .clone()
            .expect("input");
        assert_eq!(input["work_id"], fixture.entry.work_id);
        assert_eq!(input["lease_id"], authority.lease().lease_id.to_string());
        assert_eq!(
            input["attempt_id"],
            authority.lease().attempt_id.to_string()
        );

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

    #[test]
    fn code_intelligence_binding_pins_the_exact_attempt() {
        let fixture = fixture();
        let authority = authority(&fixture);
        let registry = CoderBoundToolRegistry::new(
            Arc::new(RecordingRegistry::default()),
            &authority,
            fixture.entry.clone(),
            fixture.policy.clone(),
        );
        let uri = reqwest::Url::from_file_path(fixture.entry.worktree.join("src/lib.rs"))
            .expect("file URI")
            .to_string();
        let bound = registry
            .bind_input(
                crate::code_intelligence_tools::COGNITION_CODE_DIAGNOSTICS,
                json!({ "uri": uri }),
            )
            .expect("bound intelligence input");
        assert_eq!(bound["work_id"], fixture.entry.work_id);
        assert_eq!(
            bound["attempt_id"],
            authority.lease().attempt_id.to_string()
        );
        let mismatch = registry
            .bind_input(
                crate::code_intelligence_tools::COGNITION_CODE_DIAGNOSTICS,
                json!({
                    "uri": bound["uri"],
                    "attempt_id": "attempt-from-a-sibling"
                }),
            )
            .expect_err("sibling attempt rejected");
        assert!(mismatch.to_string().contains("attempt_id"));
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
        let input = inner
            .last_input
            .lock()
            .expect("input")
            .clone()
            .expect("input");
        assert_eq!(input["session_id"], "shell-1");
        assert_eq!(input["command"], "echo two");
        assert_eq!(input["after_sequence"], 17);
    }

    #[tokio::test]
    async fn coder_shell_prefers_the_most_recently_used_owned_session() {
        let fixture = fixture();
        let authority = authority(&fixture);
        let registry = CoderBoundToolRegistry::new(
            Arc::new(RecordingRegistry::default()),
            &authority,
            fixture.entry.clone(),
            fixture.policy.clone(),
        );
        registry
            .record_shell_session(
                crate::coding_tools::COGNITION_SHELL_SESSION_STATUS,
                &json!({ "session_id": "shell-stuck", "next_sequence": 4 }),
            )
            .await;
        registry
            .record_shell_session(
                crate::coding_tools::COGNITION_SHELL_SESSION_STATUS,
                &json!({ "session_id": "shell-fresh", "next_sequence": 9 }),
            )
            .await;

        let mut input = json!({ "command": "pwd" });
        registry
            .prepare_turn_shell_session(crate::coding_tools::COGNITION_CODER_SHELL_RUN, &mut input)
            .await;

        assert_eq!(input["session_id"], "shell-fresh");
        assert_eq!(input["after_sequence"], 9);
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
