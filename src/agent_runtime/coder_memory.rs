//! Forge-scoped semantic working memory for Coder.
//!
//! Locus stores the temporal nodes, while this module owns the Coder-facing
//! scope, compact schemas, canonical STTP construction, and bounded recall
//! projection. The model never chooses the underlying Locus session.

use std::collections::{HashSet, VecDeque};
use std::fmt::Write as _;
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

use chrono::{SecondsFormat, Utc};
use genai::chat::Tool;
use locus_core_rs::SttpNodeParser;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use stasis::prelude::{Result, StasisError};

use super::coder_activity::CoderAgentIdentity;
use super::coder_mode::CoderEntryContext;

pub const COGNITION_CODER_MEMORY_OVERVIEW: &str = "cognition_coder_memory_overview";
pub const COGNITION_CODER_MEMORY_RECALL: &str = "cognition_coder_memory_recall";
pub const COGNITION_CODER_MEMORY_COMMIT: &str = "cognition_coder_memory_commit";

pub const CODER_MEMORY_TOOL_NAMES: &[&str] = &[
    COGNITION_CODER_MEMORY_OVERVIEW,
    COGNITION_CODER_MEMORY_RECALL,
    COGNITION_CODER_MEMORY_COMMIT,
];

const MEMORY_KINDS: &[&str] = &[
    "goal",
    "discovery",
    "hypothesis",
    "experiment",
    "acceptance_criterion",
    "next_action",
    "decision",
    "change",
    "verification",
    "open_gap",
    "checkpoint",
    "handoff",
];

const MEMORY_RELATIONS: &[&str] = &[
    "supports",
    "contradicts",
    "supersedes",
    "depends_on",
    "applies_to",
    "verified_by",
    "derived_from",
    "blocks",
    "resolves",
];

const MAX_SUMMARY_CHARS: usize = 2_000;
const MAX_DETAILS_CHARS: usize = 6_000;
const MAX_LIST_ITEMS: usize = 20;
const MAX_ITEM_CHARS: usize = 512;
const MAX_RELATIONS: usize = 16;
const MAX_RECALL_LIMIT: usize = 12;
const MAX_OVERVIEW_LIMIT: usize = 20;
const MAX_RECALLED_RAW_CHARS: usize = 6_000;
const MAX_PENDING_MEMORY_WRITES: usize = 64;
const MAX_MEMORY_QUEUE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_PENDING_OVERVIEW_ITEMS: usize = 8;
const MEMORY_QUEUE_SCHEMA_VERSION: u32 = 1;
const MEMORY_LIFECYCLE_SCHEMA_VERSION: u32 = 1;
const MAX_PENDING_LIFECYCLE_TASKS: usize = 64;
const MAX_LIFECYCLE_TASK_BYTES: u64 = 64 * 1024;
const INHERITED_MEMORY_KINDS: &[&str] = &[
    "goal",
    "experiment",
    "acceptance_criterion",
    "next_action",
    "decision",
    "verification",
    "open_gap",
    "checkpoint",
    "handoff",
];
const ACCEPTED_MEMORY_KINDS: &[&str] = &["decision", "verification"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoderMemoryScopeKind {
    Environment,
    AcceptedUndertaking,
    AcceptedRepository,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoderMemoryParentScope {
    pub session_id: String,
    pub branch: String,
    pub branch_digest: String,
    pub environment_generation: u32,
    pub inherited_before_utc: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoderMemoryScope {
    pub session_id: String,
    pub kind: CoderMemoryScopeKind,
    pub repo_id: String,
    pub work_id: String,
    pub branch: String,
    pub branch_digest: String,
    pub environment_generation: u32,
    pub parent: Option<CoderMemoryParentScope>,
}

impl CoderMemoryScope {
    pub fn for_entry(entry: &CoderEntryContext) -> Self {
        let mut scope = Self::for_environment(
            &entry.repo_id,
            &entry.work_id,
            &entry.branch,
            entry.environment_generation,
        );
        scope.parent = entry.memory_parent.as_ref().map(|parent| {
            let branch_digest = short_digest(&parent.branch);
            CoderMemoryParentScope {
                session_id: crate::locus_memory::resolve_workshop_locus_session(
                    &environment_memory_key(
                        &entry.repo_id,
                        &entry.work_id,
                        &branch_digest,
                        parent.environment_generation,
                    ),
                ),
                branch: parent.branch.clone(),
                branch_digest,
                environment_generation: parent.environment_generation,
                inherited_before_utc: parent.inherited_before_utc.clone(),
            }
        });
        scope
    }

    pub fn for_environment(
        repo_id: &str,
        work_id: &str,
        branch: &str,
        environment_generation: u32,
    ) -> Self {
        let branch_digest = short_digest(branch);
        let environment_key =
            environment_memory_key(repo_id, work_id, &branch_digest, environment_generation);
        Self {
            session_id: crate::locus_memory::resolve_workshop_locus_session(&environment_key),
            kind: CoderMemoryScopeKind::Environment,
            repo_id: repo_id.to_string(),
            work_id: work_id.to_string(),
            branch: branch.to_string(),
            branch_digest,
            environment_generation,
            parent: None,
        }
    }

    pub fn parent_environment_scope(&self) -> Option<Self> {
        let parent = self.parent.as_ref()?;
        Some(Self {
            session_id: parent.session_id.clone(),
            kind: CoderMemoryScopeKind::Environment,
            repo_id: self.repo_id.clone(),
            work_id: self.work_id.clone(),
            branch: parent.branch.clone(),
            branch_digest: parent.branch_digest.clone(),
            environment_generation: parent.environment_generation,
            parent: None,
        })
    }

    pub fn accepted_undertaking_scope(&self) -> Self {
        let mut scope = self.clone();
        scope.session_id = crate::locus_memory::resolve_workshop_locus_session(&format!(
            "coder:{}:{}:accepted",
            self.repo_id, self.work_id
        ));
        scope.kind = CoderMemoryScopeKind::AcceptedUndertaking;
        scope.parent = None;
        scope
    }

    pub fn accepted_repository_scope(&self) -> Self {
        let mut scope = self.clone();
        scope.session_id = crate::locus_memory::resolve_workshop_locus_session(&format!(
            "coder:{}:accepted",
            self.repo_id
        ));
        scope.kind = CoderMemoryScopeKind::AcceptedRepository;
        scope.parent = None;
        scope
    }

    pub fn public_descriptor(&self) -> Value {
        let mut descriptor = json!({
            "scope_kind": match self.kind {
                CoderMemoryScopeKind::Environment => "environment",
                CoderMemoryScopeKind::AcceptedUndertaking => "accepted_undertaking",
                CoderMemoryScopeKind::AcceptedRepository => "accepted_repository",
            },
            "repo_id": self.repo_id,
            "work_id": self.work_id,
            "source_branch": self.branch,
            "source_environment_generation": self.environment_generation,
            "source_environment_id": format!("{}:g{}", self.branch_digest, self.environment_generation),
        });
        if self.kind == CoderMemoryScopeKind::Environment {
            descriptor["branch"] = Value::String(self.branch.clone());
            descriptor["environment_generation"] = json!(self.environment_generation);
            descriptor["environment_id"] = Value::String(format!(
                "{}:g{}",
                self.branch_digest, self.environment_generation
            ));
            if let Some(parent) = self.parent.as_ref() {
                descriptor["derived_from"] = json!({
                    "environment_id": format!("{}:g{}", parent.branch_digest, parent.environment_generation),
                    "branch": parent.branch,
                    "environment_generation": parent.environment_generation,
                    "inherited_before_utc": parent.inherited_before_utc,
                });
            }
        }
        descriptor
    }

    pub fn base_tags(&self) -> Vec<String> {
        let mut tags = vec!["coder-memory".to_string(), format!("repo:{}", self.repo_id)];
        match self.kind {
            CoderMemoryScopeKind::Environment => {
                tags.push(format!("work:{}", self.work_id));
                tags.push(format!(
                    "environment:{}:g{}",
                    self.branch_digest, self.environment_generation
                ));
                tags.push("memory-scope:environment".to_string());
            }
            CoderMemoryScopeKind::AcceptedUndertaking => {
                tags.push(format!("work:{}", self.work_id));
                tags.push("memory-scope:undertaking".to_string());
                tags.push("knowledge:accepted".to_string());
            }
            CoderMemoryScopeKind::AcceptedRepository => {
                tags.push("memory-scope:repository".to_string());
                tags.push("knowledge:accepted".to_string());
            }
        }
        tags
    }
}

fn environment_memory_key(
    repo_id: &str,
    work_id: &str,
    branch_digest: &str,
    generation: u32,
) -> String {
    format!("coder:{repo_id}:{work_id}:{branch_digest}:g{generation}")
}

fn short_digest(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    format!("{digest:x}")[..16].to_string()
}

#[derive(Debug, Clone, PartialEq)]
pub struct CoderMemoryRelation {
    pub relation: String,
    pub target: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoderMemoryCommit {
    pub kind: String,
    pub summary: String,
    pub raw_node: String,
    pub semantic_tags: Vec<String>,
    pub dedupe_tag: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CoderPendingMemorySummary {
    pub kind: String,
    pub summary: String,
}

/// Durable routing intent for accepted knowledge whose source Locus session
/// was temporarily unavailable. It stores identifiers only, never node bodies
/// or repository content; retry rereads the canonical source session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoderMemoryPromotionTask {
    schema_version: u32,
    pub repo_id: String,
    pub work_id: String,
    pub source_branch: String,
    pub source_environment_generation: u32,
    pub accepted_head: String,
    pub decision_id: String,
    pub attempt_id: String,
    pub evidence_id: String,
    pub evidence_digest: String,
}

impl CoderMemoryPromotionTask {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        repo_id: impl Into<String>,
        work_id: impl Into<String>,
        source_branch: impl Into<String>,
        source_environment_generation: u32,
        accepted_head: impl Into<String>,
        decision_id: impl Into<String>,
        attempt_id: impl Into<String>,
        evidence_id: impl Into<String>,
        evidence_digest: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: MEMORY_LIFECYCLE_SCHEMA_VERSION,
            repo_id: repo_id.into(),
            work_id: work_id.into(),
            source_branch: source_branch.into(),
            source_environment_generation,
            accepted_head: accepted_head.into(),
            decision_id: decision_id.into(),
            attempt_id: attempt_id.into(),
            evidence_id: evidence_id.into(),
            evidence_digest: evidence_digest.into(),
        }
    }

    pub fn source_scope(&self) -> CoderMemoryScope {
        CoderMemoryScope::for_environment(
            &self.repo_id,
            &self.work_id,
            &self.source_branch,
            self.source_environment_generation,
        )
    }

    pub fn persist(&self) -> Result<()> {
        if cfg!(test) {
            return Ok(());
        }
        let raw = serde_json::to_vec_pretty(self).map_err(|error| {
            input_error(format!("cannot encode pending memory promotion: {error}"))
        })?;
        if raw.len() as u64 > MAX_LIFECYCLE_TASK_BYTES {
            return Err(input_error(
                "pending memory promotion exceeds its byte bound",
            ));
        }
        let path = memory_lifecycle_task_path(self);
        if !path.exists() {
            let pending = std::fs::read_dir(memory_lifecycle_directory(&self.repo_id))
                .map(|entries| entries.flatten().take(MAX_PENDING_LIFECYCLE_TASKS).count())
                .unwrap_or_default();
            if pending >= MAX_PENDING_LIFECYCLE_TASKS {
                return Err(input_error(
                    "pending memory promotion queue reached its repository bound",
                ));
            }
        }
        crate::session::atomic_write(&path, &raw).map_err(|error| {
            input_error(format!("cannot persist pending memory promotion: {error}"))
        })
    }

    pub fn remove(&self) -> Result<()> {
        if cfg!(test) {
            return Ok(());
        }
        let path = memory_lifecycle_task_path(self);
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(input_error(format!(
                "cannot remove completed memory promotion: {error}"
            ))),
        }
    }

    pub fn pending_for_repo(repo_id: &str) -> Vec<Self> {
        if cfg!(test) {
            return Vec::new();
        }
        let directory = memory_lifecycle_directory(repo_id);
        let Ok(entries) = std::fs::read_dir(directory) else {
            return Vec::new();
        };
        let mut tasks = entries
            .flatten()
            .take(MAX_PENDING_LIFECYCLE_TASKS.saturating_mul(2))
            .filter_map(|entry| {
                let path = entry.path();
                let metadata = std::fs::symlink_metadata(&path).ok()?;
                if metadata.file_type().is_symlink()
                    || !metadata.is_file()
                    || metadata.len() > MAX_LIFECYCLE_TASK_BYTES
                {
                    return None;
                }
                let queued_at = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                let raw = std::fs::read(path).ok()?;
                let task = serde_json::from_slice::<Self>(&raw).ok()?;
                (task.schema_version == MEMORY_LIFECYCLE_SCHEMA_VERSION && task.repo_id == repo_id)
                    .then_some((queued_at, task))
            })
            .collect::<Vec<_>>();
        tasks.sort_by(|(left_time, left), (right_time, right)| {
            left_time
                .cmp(right_time)
                .then_with(|| left.decision_id.cmp(&right.decision_id))
        });
        tasks.truncate(MAX_PENDING_LIFECYCLE_TASKS);
        tasks.into_iter().map(|(_, task)| task).collect()
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct CoderMemoryQueueFile {
    schema_version: u32,
    session_id: String,
    entries: Vec<CoderMemoryCommit>,
}

/// Bounded, redacted semantic writes that could not reach Locus yet.
///
/// The queue is persisted per governed environment. It contains only commits
/// already compiled and redacted by [`build_commit`], never raw tool output.
#[derive(Debug)]
pub struct CoderMemoryRetryQueue {
    session_id: String,
    path: Option<PathBuf>,
    entries: VecDeque<CoderMemoryCommit>,
}

impl CoderMemoryRetryQueue {
    pub fn for_scope(scope: &CoderMemoryScope) -> Self {
        let path = if cfg!(test) {
            None
        } else {
            Some(memory_queue_path(scope))
        };
        Self::load(scope, path)
    }

    #[cfg(test)]
    fn at_path(scope: &CoderMemoryScope, path: PathBuf) -> Self {
        Self::load(scope, Some(path))
    }

    fn load(scope: &CoderMemoryScope, path: Option<PathBuf>) -> Self {
        let mut queue = Self {
            session_id: scope.session_id.clone(),
            path,
            entries: VecDeque::new(),
        };
        let Some(path) = queue.path.as_ref() else {
            return queue;
        };
        let raw = match std::fs::metadata(path) {
            Ok(metadata) if metadata.len() > MAX_MEMORY_QUEUE_BYTES => {
                tracing::warn!(path = %path.display(), "ignoring oversized Coder memory retry queue");
                return queue;
            }
            Ok(_) => match std::fs::read(path) {
                Ok(raw) => raw,
                Err(error) => {
                    tracing::warn!(error = %error, path = %path.display(), "failed to read Coder memory retry queue");
                    return queue;
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return queue,
            Err(error) => {
                tracing::warn!(error = %error, path = %path.display(), "failed to inspect Coder memory retry queue");
                return queue;
            }
        };
        let file = match serde_json::from_slice::<CoderMemoryQueueFile>(&raw) {
            Ok(file)
                if file.schema_version == MEMORY_QUEUE_SCHEMA_VERSION
                    && file.session_id == scope.session_id =>
            {
                file
            }
            Ok(_) => {
                tracing::warn!(path = %path.display(), "ignoring Coder memory queue with mismatched scope or version");
                return queue;
            }
            Err(error) => {
                tracing::warn!(error = %error, path = %path.display(), "ignoring malformed Coder memory retry queue");
                return queue;
            }
        };
        let mut seen = HashSet::new();
        for commit in file
            .entries
            .into_iter()
            .rev()
            .take(MAX_PENDING_MEMORY_WRITES)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            let valid_scope = validate_raw_node_scope(&commit.raw_node, &scope.session_id).is_ok();
            let valid_dedupe = commit.semantic_tags.contains(&commit.dedupe_tag);
            if valid_scope && valid_dedupe && seen.insert(commit.dedupe_tag.clone()) {
                queue.entries.push_back(commit);
            }
        }
        queue
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn front(&self) -> Option<CoderMemoryCommit> {
        self.entries.front().cloned()
    }

    pub fn pending_summaries(&self) -> Vec<CoderPendingMemorySummary> {
        self.entries
            .iter()
            .rev()
            .take(MAX_PENDING_OVERVIEW_ITEMS)
            .map(|commit| CoderPendingMemorySummary {
                kind: commit.kind.clone(),
                summary: truncate_chars(&commit.summary, 500),
            })
            .collect()
    }

    /// Adds one semantic write. Returns whether a new entry was inserted.
    pub fn enqueue(&mut self, commit: CoderMemoryCommit) -> Result<bool> {
        if self
            .entries
            .iter()
            .any(|pending| pending.dedupe_tag == commit.dedupe_tag)
        {
            return Ok(false);
        }
        if self.entries.len() == MAX_PENDING_MEMORY_WRITES {
            tracing::warn!(
                pending_writes = self.entries.len(),
                "Coder memory retry queue reached its bound; replacing the oldest deferred summary"
            );
            self.entries.pop_front();
        }
        self.entries.push_back(commit);
        self.persist()?;
        Ok(true)
    }

    pub fn pop_front(&mut self, expected_dedupe_tag: &str) -> Result<bool> {
        if self
            .entries
            .front()
            .is_some_and(|commit| commit.dedupe_tag == expected_dedupe_tag)
        {
            self.entries.pop_front();
            self.persist()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn persist(&mut self) -> Result<()> {
        let Some(path) = self.path.clone() else {
            return Ok(());
        };
        let raw = loop {
            let file = CoderMemoryQueueFile {
                schema_version: MEMORY_QUEUE_SCHEMA_VERSION,
                session_id: self.session_id.clone(),
                entries: self.entries.iter().cloned().collect(),
            };
            let raw = serde_json::to_vec_pretty(&file).map_err(|error| {
                input_error(format!("cannot encode memory retry queue: {error}"))
            })?;
            if raw.len() as u64 <= MAX_MEMORY_QUEUE_BYTES {
                break raw;
            }
            if self.entries.len() <= 1 {
                return Err(input_error(format!(
                    "one memory retry entry exceeds the {}-byte queue bound",
                    MAX_MEMORY_QUEUE_BYTES
                )));
            }
            self.entries.pop_front();
            tracing::warn!(
                pending_writes = self.entries.len(),
                "Coder memory retry queue reached its byte bound; replacing the oldest deferred summary"
            );
        };
        crate::session::atomic_write(&path, &raw)
            .map_err(|error| input_error(format!("cannot persist memory retry queue: {error}")))
    }
}

fn memory_queue_path(scope: &CoderMemoryScope) -> PathBuf {
    let digest = Sha256::digest(scope.session_id.as_bytes());
    crate::paths::medousa_data_dir()
        .join("coder_memory_queue")
        .join(format!("{:x}.json", digest))
}

fn memory_lifecycle_directory(repo_id: &str) -> PathBuf {
    let repo_digest = Sha256::digest(repo_id.as_bytes());
    crate::paths::medousa_data_dir()
        .join("coder_memory_lifecycle")
        .join(format!("{repo_digest:x}"))
}

fn memory_lifecycle_task_path(task: &CoderMemoryPromotionTask) -> PathBuf {
    let digest = Sha256::digest(
        format!("{}:{}:{}", task.repo_id, task.work_id, task.decision_id).as_bytes(),
    );
    memory_lifecycle_directory(&task.repo_id).join(format!("{:x}.json", digest))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoderMemoryRecallQuery {
    pub query: String,
    pub kind: Option<String>,
    pub path: Option<String>,
    pub limit: usize,
}

pub fn tool_definitions() -> Vec<Tool> {
    vec![
        Tool::new(COGNITION_CODER_MEMORY_OVERVIEW)
            .with_description(
                "Load compact semantic working state for this governed Coder environment plus a bounded fork snapshot and accepted undertaking/repository knowledge. Live sibling state is excluded.",
            )
            .with_schema(json!({
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "minimum": 1, "maximum": MAX_OVERVIEW_LIMIT }
                }
            })),
        Tool::new(COGNITION_CODER_MEMORY_RECALL)
            .with_description(
                "Recall bounded STTP working-memory nodes from this governed environment, its immutable parent snapshot, and accepted knowledge scopes. The runtime pins every scope, excludes live sibling state, and labels changed-HEAD observations stale.",
            )
            .with_schema(json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "maxLength": MAX_SUMMARY_CHARS },
                    "kind": { "type": "string", "enum": MEMORY_KINDS },
                    "path": { "type": "string", "description": "Optional repository-relative path" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": MAX_RECALL_LIMIT }
                },
                "required": ["query"]
            })),
        Tool::new(COGNITION_CODER_MEMORY_COMMIT)
            .with_description(
                "Commit an explicit engineering working-state summary as canonical STTP in this governed environment. Store decisions and evidence, not private reasoning or raw source/log payloads.",
            )
            .with_schema(json!({
                "type": "object",
                "properties": {
                    "kind": { "type": "string", "enum": MEMORY_KINDS },
                    "summary": { "type": "string", "maxLength": MAX_SUMMARY_CHARS },
                    "details": { "type": "string", "maxLength": MAX_DETAILS_CHARS },
                    "paths": {
                        "type": "array",
                        "maxItems": MAX_LIST_ITEMS,
                        "items": { "type": "string", "maxLength": MAX_ITEM_CHARS }
                    },
                    "symbols": {
                        "type": "array",
                        "maxItems": MAX_LIST_ITEMS,
                        "items": { "type": "string", "maxLength": MAX_ITEM_CHARS }
                    },
                    "evidence_refs": {
                        "type": "array",
                        "maxItems": MAX_LIST_ITEMS,
                        "items": { "type": "string", "maxLength": MAX_ITEM_CHARS }
                    },
                    "relations": {
                        "type": "array",
                        "maxItems": MAX_RELATIONS,
                        "items": {
                            "type": "object",
                            "properties": {
                                "rel": { "type": "string", "enum": MEMORY_RELATIONS },
                                "target": { "type": "string", "maxLength": MAX_ITEM_CHARS },
                                "confidence": { "type": "number", "minimum": 0.0, "maximum": 1.0 }
                            },
                            "required": ["rel", "target"]
                        }
                    }
                },
                "required": ["kind", "summary"]
            })),
    ]
}

pub fn overview_limit(input: &Value) -> usize {
    input
        .get("limit")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(10)
        .clamp(1, MAX_OVERVIEW_LIMIT)
}

pub fn parse_recall_query(input: &Value) -> Result<CoderMemoryRecallQuery> {
    let query = required_text(input, "query", MAX_SUMMARY_CHARS)?;
    let kind = optional_text(input, "kind", 64)?;
    if let Some(kind) = kind.as_deref()
        && !MEMORY_KINDS.contains(&kind)
    {
        return Err(input_error(format!(
            "unknown Coder memory kind '{kind}'; expected one of {}",
            MEMORY_KINDS.join(", ")
        )));
    }
    let path = input
        .get("path")
        .and_then(Value::as_str)
        .map(normalize_relative_path)
        .transpose()?;
    let limit = input
        .get("limit")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(6)
        .clamp(1, MAX_RECALL_LIMIT);
    Ok(CoderMemoryRecallQuery {
        query,
        kind,
        path,
        limit,
    })
}

pub fn validate_raw_node_scope(raw_node: &str, expected_session_id: &str) -> Result<()> {
    let parsed = SttpNodeParser::with_profile(crate::locus_memory::resolve_locus_ingest_profile())
        .try_parse(raw_node, expected_session_id);
    if !parsed.success {
        return Err(input_error(format!(
            "advanced raw STTP must parse before it can be stored: {}",
            parsed
                .error
                .unwrap_or_else(|| "unknown STTP validation error".to_string())
        )));
    }
    let envelope_start = unquoted_marker_positions(raw_node, "⦿⟨")
        .first()
        .copied()
        .ok_or_else(|| input_error("advanced raw STTP is missing its envelope block"))?;
    let content_start = unquoted_marker_positions(raw_node, "◈⟨")
        .first()
        .copied()
        .filter(|content_start| *content_start > envelope_start)
        .ok_or_else(|| input_error("advanced raw STTP is missing its content block"))?;
    let envelope = &raw_node[envelope_start..content_start];
    let session_ids = unquoted_string_fields(envelope, "session_id");
    if session_ids.as_slice() != [expected_session_id] {
        return Err(input_error(
            "advanced raw STTP session_id must match the governed Coder environment",
        ));
    }
    Ok(())
}

pub fn build_commit(
    input: &Value,
    scope: &CoderMemoryScope,
    identity: &CoderAgentIdentity,
    current_head: &str,
) -> Result<CoderMemoryCommit> {
    build_commit_with_tags(input, scope, identity, current_head, &[])
}

pub fn build_commit_with_tags(
    input: &Value,
    scope: &CoderMemoryScope,
    identity: &CoderAgentIdentity,
    current_head: &str,
    additional_tags: &[String],
) -> Result<CoderMemoryCommit> {
    let kind = required_text(input, "kind", 64)?;
    if !MEMORY_KINDS.contains(&kind.as_str()) {
        return Err(input_error(format!(
            "unknown Coder memory kind '{kind}'; expected one of {}",
            MEMORY_KINDS.join(", ")
        )));
    }
    let summary = super::coder_evidence::redact_evidence_text(&required_text(
        input,
        "summary",
        MAX_SUMMARY_CHARS,
    )?);
    let details = optional_text(input, "details", MAX_DETAILS_CHARS)?
        .map(|details| super::coder_evidence::redact_evidence_text(&details));
    let paths = string_list(input, "paths", MAX_LIST_ITEMS, MAX_ITEM_CHARS)?
        .into_iter()
        .map(|path| normalize_relative_path(&path))
        .collect::<Result<Vec<_>>>()?;
    let mut symbols = string_list(input, "symbols", MAX_LIST_ITEMS, MAX_ITEM_CHARS)?
        .into_iter()
        .map(|symbol| super::coder_evidence::redact_evidence_text(&symbol))
        .collect::<Vec<_>>();
    dedupe_preserving_order(&mut symbols);
    let mut evidence_refs = string_list(input, "evidence_refs", MAX_LIST_ITEMS, MAX_ITEM_CHARS)?
        .into_iter()
        .map(|reference| super::coder_evidence::redact_evidence_text(&reference))
        .collect::<Vec<_>>();
    dedupe_preserving_order(&mut evidence_refs);
    let relations = parse_relations(input)?;

    let dedupe_value = json!({
        "session": scope.session_id,
        "kind": kind,
        "summary": summary,
        "details": details,
        "paths": paths,
        "symbols": symbols,
        "evidence_refs": evidence_refs,
        "relations": relations.iter().map(|relation| json!({
            "rel": relation.relation,
            "target": relation.target,
            "confidence": relation.confidence,
        })).collect::<Vec<_>>(),
        "observed_head": current_head,
    });
    let dedupe_hash = Sha256::digest(
        serde_json::to_vec(&dedupe_value)
            .map_err(|error| input_error(format!("cannot fingerprint Coder memory: {error}")))?,
    );
    let dedupe_key = format!("{dedupe_hash:x}");
    let dedupe_tag = format!("coder-dedupe:{}", &dedupe_key[..40]);

    let mut semantic_tags = scope.base_tags();
    semantic_tags.push(format!("kind:{kind}"));
    semantic_tags.push(format!("head:{}", current_head.trim()));
    semantic_tags.push(dedupe_tag.clone());
    semantic_tags.extend(paths.iter().map(|path| indexed_tag("path", path)));
    semantic_tags.extend(symbols.iter().map(|symbol| indexed_tag("symbol", symbol)));
    semantic_tags.extend(additional_tags.iter().cloned());
    dedupe_preserving_order(&mut semantic_tags);

    let links = if relations.is_empty() {
        String::new()
    } else {
        let entries = relations
            .iter()
            .map(|relation| {
                format!(
                    "{{ rel: {}, target: {}, confidence: {:.3} }}",
                    json_string(&relation.relation),
                    json_string(&relation.target),
                    relation.confidence
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!(", semantic_links: [{entries}]")
    };

    let context_summary = truncate_chars(&format!("{kind}: {summary}"), 1_000);
    let tags_json = escape_protocol_glyphs(
        &serde_json::to_string(&semantic_tags)
            .map_err(|error| input_error(format!("cannot encode Coder memory tags: {error}")))?,
    );
    let timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    let mut content_fields = vec![
        format!("memory_kind(.99): {}", json_string(&kind)),
        format!("summary(.98): {}", json_string(&summary)),
        format!("observed_head(.99): {}", json_string(current_head.trim())),
        format!("dedupe_key(.99): {}", json_string(&dedupe_key)),
        format!("repo_id(.99): {}", json_string(&scope.repo_id)),
        format!("work_id(.99): {}", json_string(&scope.work_id)),
        format!("branch(.99): {}", json_string(&scope.branch)),
        format!(
            "environment_id(.99): {}",
            json_string(&format!(
                "{}:g{}",
                scope.branch_digest, scope.environment_generation
            ))
        ),
        format!(
            "environment_generation(.99): {}",
            scope.environment_generation
        ),
        format!("author_agent(.99): {}", json_string(&identity.agent_id)),
        format!("author_session(.99): {}", json_string(&identity.session_id)),
        format!("author_turn(.99): {}", json_string(&identity.turn_id)),
        format!("author_attempt(.99): {}", json_string(&identity.attempt_id)),
    ];
    if let Some(details) = details.as_deref() {
        content_fields.push(format!("details(.95): {}", json_string(details)));
    }
    if !paths.is_empty() {
        content_fields.push(format!("paths(.98): {}", json_string_array(&paths)));
    }
    if !symbols.is_empty() {
        content_fields.push(format!("symbols(.96): {}", json_string_array(&symbols)));
    }
    if !evidence_refs.is_empty() {
        content_fields.push(format!(
            "evidence_refs(.99): {}",
            json_string_array(&evidence_refs)
        ));
    }

    let raw_node = format!(
        "⊕⟨ ⏣0{{ trigger: manual, response_format: temporal_node, origin_session: {session}, compression_depth: 1, parent_node: null{links}, prime: {{ attractor_config: {{ stability: 0.94, friction: 0.16, logic: 0.99, autonomy: 0.84 }}, context_summary: {context_summary}, relevant_tier: raw, retrieval_budget: 12, semantic_tags: {tags_json} }} }} ⟩\n\
⦿⟨ ⏣0{{ timestamp: {timestamp}, tier: raw, session_id: {session}, schema_version: \"sttp-1.0\", user_avec: {{ stability: 0.90, friction: 0.20, logic: 0.96, autonomy: 0.84, psi: 2.90 }}, model_avec: {{ stability: 0.94, friction: 0.16, logic: 0.99, autonomy: 0.84, psi: 2.93 }} }} ⟩\n\
◈⟨ ⏣0{{\n    {content}\n}} ⟩\n\
⍉⟨ ⏣0{{ rho: 0.98, kappa: 0.99, psi: 2.93, compression_avec: {{ stability: 0.94, friction: 0.16, logic: 0.99, autonomy: 0.84, psi: 2.93 }} }} ⟩",
        session = json_string(&scope.session_id),
        context_summary = json_string(&context_summary),
        timestamp = json_string(&timestamp),
        content = content_fields.join(",\n    "),
    );

    super::sttp::validate_canonical_sttp_node(&raw_node).map_err(|error| {
        input_error(format!(
            "Coder memory compiler emitted invalid STTP: {error}"
        ))
    })?;

    Ok(CoderMemoryCommit {
        kind,
        summary,
        raw_node,
        semantic_tags,
        dedupe_tag,
    })
}

pub fn recall_semantic_tags(query: &CoderMemoryRecallQuery) -> Vec<String> {
    let mut tags = Vec::new();
    if let Some(kind) = query.kind.as_deref() {
        tags.push(parser_encoded_string(&format!("kind:{kind}")));
    }
    if let Some(path) = query.path.as_deref() {
        tags.push(parser_encoded_string(&indexed_tag("path", path)));
    }
    tags
}

pub fn first_node_id(result: &Value) -> Option<String> {
    result
        .get("nodes")
        .and_then(Value::as_array)
        .and_then(|nodes| nodes.first())
        .and_then(|node| {
            node.get("sync_key")
                .or_else(|| node.get("node_id"))
                .and_then(Value::as_str)
        })
        .map(str::to_string)
}

pub fn project_recall(
    scope: &CoderMemoryScope,
    current_head: &str,
    result: &Value,
    include_raw: bool,
    limit: usize,
) -> Value {
    let nodes = result
        .get("nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(limit)
        .map(|node| project_node(node, current_head, include_raw))
        .collect::<Vec<_>>();
    json!({
        "ok": true,
        "scope": scope.public_descriptor(),
        "current_head": current_head,
        "retrieved": nodes.len(),
        "nodes": nodes,
    })
}

/// Merge bounded current, parent-snapshot, and accepted knowledge without ever
/// querying a live sibling environment. The caller pins each result to its
/// daemon-derived Locus session before this projection runs.
#[derive(Debug, Clone, Copy, Default)]
pub struct CoderMemoryLineageSources<'a> {
    pub current: Option<&'a Value>,
    pub parent: Option<&'a Value>,
    pub undertaking: Option<&'a Value>,
    pub repository: Option<&'a Value>,
}

pub fn project_lineage_recall(
    scope: &CoderMemoryScope,
    current_head: &str,
    sources: CoderMemoryLineageSources<'_>,
    include_raw: bool,
    limit: usize,
) -> Value {
    let limit = limit.clamp(1, MAX_RECALL_LIMIT.max(MAX_OVERVIEW_LIMIT));
    let local_nodes = projected_source_nodes(
        sources.current,
        current_head,
        include_raw,
        "current_environment",
        |_| true,
    );
    let inherited_nodes = projected_source_nodes(
        sources.parent,
        current_head,
        include_raw,
        "inherited_parent",
        |node| inherited_node_allowed(scope, node),
    );
    let undertaking_nodes = projected_source_nodes(
        sources.undertaking,
        current_head,
        include_raw,
        "accepted_undertaking",
        |node| accepted_node_allowed(node, "memory-scope:undertaking"),
    );
    let repository_nodes = projected_source_nodes(
        sources.repository,
        current_head,
        include_raw,
        "accepted_repository",
        |node| accepted_node_allowed(node, "memory-scope:repository"),
    );
    let available = json!({
        "current_environment": local_nodes.len(),
        "inherited_parent": inherited_nodes.len(),
        "accepted_undertaking": undertaking_nodes.len(),
        "accepted_repository": repository_nodes.len(),
    });
    let nodes = merge_lineage_nodes(
        local_nodes,
        inherited_nodes,
        undertaking_nodes,
        repository_nodes,
        limit,
    );
    json!({
        "ok": true,
        "scope": scope.public_descriptor(),
        "current_head": current_head,
        "retrieved": nodes.len(),
        "available_by_origin": available,
        "nodes": nodes,
    })
}

fn projected_source_nodes<F>(
    result: Option<&Value>,
    current_head: &str,
    include_raw: bool,
    origin: &str,
    mut predicate: F,
) -> Vec<Value>
where
    F: FnMut(&Value) -> bool,
{
    result
        .and_then(|result| result.get("nodes"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|node| predicate(node))
        .map(|node| {
            let mut projected = project_node(node, current_head, include_raw);
            projected["memory_origin"] = Value::String(origin.to_string());
            projected["inherited"] = Value::Bool(origin == "inherited_parent");
            projected["accepted_knowledge"] = Value::Bool(origin.starts_with("accepted_"));
            projected
        })
        .collect()
}

fn inherited_node_allowed(scope: &CoderMemoryScope, node: &Value) -> bool {
    let Some(parent) = scope.parent.as_ref() else {
        return false;
    };
    let Some(kind) = node_memory_kind(node) else {
        return false;
    };
    if !INHERITED_MEMORY_KINDS.contains(&kind.as_str()) {
        return false;
    }
    let Some(timestamp) = node.get("timestamp").and_then(Value::as_str) else {
        return false;
    };
    let Ok(timestamp) = chrono::DateTime::parse_from_rfc3339(timestamp) else {
        return false;
    };
    let Ok(cutoff) = chrono::DateTime::parse_from_rfc3339(&parent.inherited_before_utc) else {
        return false;
    };
    timestamp <= cutoff
}

fn accepted_node_allowed(node: &Value, expected_scope_tag: &str) -> bool {
    let Some(kind) = node_memory_kind(node) else {
        return false;
    };
    ACCEPTED_MEMORY_KINDS.contains(&kind.as_str())
        && node_has_tag(node, "knowledge:accepted")
        && node_has_tag(node, expected_scope_tag)
}

fn node_memory_kind(node: &Value) -> Option<String> {
    let tags = node
        .get("semantic_tags")
        .and_then(Value::as_array)?
        .iter()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    tag_value(&tags, "kind:").map(|kind| decode_parser_string(&kind))
}

fn node_has_tag(node: &Value, expected: &str) -> bool {
    node.get("semantic_tags")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .any(|tag| decode_parser_string(tag).eq_ignore_ascii_case(expected))
}

pub fn promotion_candidates(
    result: &Value,
    current_head: &str,
    expected_kind: &str,
    limit: usize,
) -> Vec<Value> {
    result
        .get("nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|node| {
            node_memory_kind(node).as_deref() == Some(expected_kind)
                && node_has_tag(node, &format!("head:{}", current_head.trim()))
        })
        .take(limit)
        .map(|node| project_node(node, current_head, false))
        .collect()
}

pub fn build_promotion_commit(
    source_node: &Value,
    target_scope: &CoderMemoryScope,
    identity: &CoderAgentIdentity,
    accepted_head: &str,
    decision_id: &str,
    evidence_id: &str,
    evidence_digest: &str,
) -> Result<CoderMemoryCommit> {
    let kind = source_node
        .get("kind")
        .and_then(Value::as_str)
        .filter(|kind| ACCEPTED_MEMORY_KINDS.contains(kind))
        .ok_or_else(|| input_error("only decisions and verification may be promoted"))?;
    let context_summary = source_node
        .get("summary")
        .and_then(Value::as_str)
        .unwrap_or("accepted engineering knowledge");
    let summary = context_summary
        .strip_prefix(&format!("{kind}: "))
        .unwrap_or(context_summary);
    let source_node_id = source_node
        .get("node_id")
        .and_then(Value::as_str)
        .unwrap_or("unknown-source-node");
    let input = json!({
        "kind": kind,
        "summary": summary,
        "details": format!(
            "Accepted Forge outcome; promoted from environment memory node {source_node_id}."
        ),
        "paths": source_node.get("paths").cloned().unwrap_or_else(|| json!([])),
        "symbols": source_node.get("symbols").cloned().unwrap_or_else(|| json!([])),
        "evidence_refs": [
            format!("forge:decision:{decision_id}"),
            format!("forge:evidence:{evidence_id}"),
            format!("forge:evidence-digest:{evidence_digest}"),
        ],
        "relations": [{
            "rel": "derived_from",
            "target": source_node_id,
            "confidence": 1.0,
        }],
    });
    build_commit_with_tags(
        &input,
        target_scope,
        identity,
        accepted_head,
        &[
            "knowledge:accepted".to_string(),
            format!("accepted-head:{}", accepted_head.trim()),
            format!("forge-decision:{}", short_digest(decision_id)),
        ],
    )
}

pub fn build_archive_commit(
    scope: &CoderMemoryScope,
    identity: &CoderAgentIdentity,
    current_head: &str,
    terminal_state: &str,
    detail: &str,
) -> Result<CoderMemoryCommit> {
    let input = json!({
        "kind": "checkpoint",
        "summary": format!(
            "Archived governed environment memory after undertaking became {terminal_state}."
        ),
        "details": detail,
    });
    build_commit_with_tags(
        &input,
        scope,
        identity,
        current_head,
        &[
            "lineage:archived".to_string(),
            format!("terminal:{terminal_state}"),
        ],
    )
}

fn merge_lineage_nodes(
    current: Vec<Value>,
    inherited: Vec<Value>,
    undertaking: Vec<Value>,
    repository: Vec<Value>,
    limit: usize,
) -> Vec<Value> {
    let sources = [current, undertaking, repository, inherited];
    let non_local_sources = sources[1..]
        .iter()
        .filter(|nodes| !nodes.is_empty())
        .count();
    let reserved = non_local_sources.min(limit / 2);
    let local_take = sources[0].len().min(limit.saturating_sub(reserved));
    let mut positions = [0usize; 4];
    let mut merged = Vec::with_capacity(limit);
    for node in sources[0].iter().take(local_take) {
        merged.push(node.clone());
    }
    positions[0] = local_take;

    for source_index in 1..sources.len() {
        if merged.len() >= limit {
            break;
        }
        if let Some(node) = sources[source_index].first() {
            merged.push(node.clone());
            positions[source_index] = 1;
        }
    }

    while merged.len() < limit {
        let mut advanced = false;
        for source_index in 0..sources.len() {
            if let Some(node) = sources[source_index].get(positions[source_index]) {
                merged.push(node.clone());
                positions[source_index] += 1;
                advanced = true;
                if merged.len() == limit {
                    break;
                }
            }
        }
        if !advanced {
            break;
        }
    }
    merged
}

fn project_node(node: &Value, current_head: &str, include_raw: bool) -> Value {
    let tags = node
        .get("semantic_tags")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    let observed_head = tag_value(&tags, "head:").map(|value| decode_parser_string(&value));
    let kind = tag_value(&tags, "kind:").map(|value| decode_parser_string(&value));
    let paths = tags
        .iter()
        .filter_map(|tag| tag.strip_prefix("path:"))
        .map(decode_parser_string)
        .collect::<Vec<_>>();
    let symbols = tags
        .iter()
        .filter_map(|tag| tag.strip_prefix("symbol:"))
        .map(decode_parser_string)
        .collect::<Vec<_>>();
    let stale = observed_head
        .as_deref()
        .is_some_and(|head| head != current_head.trim());
    let raw = node.get("raw").and_then(Value::as_str).unwrap_or_default();
    let relations = SttpNodeParser::new()
        .try_parse(raw, "")
        .node
        .and_then(|node| node.semantic_links)
        .unwrap_or_default()
        .into_iter()
        .map(|link| {
            json!({
                "rel": decode_parser_string(&link.rel),
                "target": decode_parser_string(&link.target),
                "confidence": link.confidence,
            })
        })
        .collect::<Vec<_>>();
    let summary = node
        .get("context_summary")
        .and_then(Value::as_str)
        .map(decode_parser_string);
    let mut projected = json!({
        "node_id": node.get("sync_key").or_else(|| node.get("node_id")),
        "kind": kind,
        "summary": summary,
        "timestamp": node.get("timestamp"),
        "observed_head": observed_head,
        "stale": stale,
        "paths": paths,
        "symbols": symbols,
        "relations": relations,
    });
    if include_raw {
        let content = recalled_content(raw);
        projected["content"] = Value::String(truncate_chars(&content, MAX_RECALLED_RAW_CHARS));
        projected["content_truncated"] =
            Value::Bool(content.chars().count() > MAX_RECALLED_RAW_CHARS);
    }
    projected
}

/// Compile the bounded, public Coder memory state into model context.
///
/// `overview` must already be projected through [`project_recall`]. The raw
/// Locus session id and raw STTP nodes are therefore never exposed here.
pub fn environment_overview_prompt_appendix(overview: &Value) -> String {
    let timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let overview = parser_safe_json(overview);
    let mut out = String::new();
    let _ = writeln!(
        out,
        "⊕⟨ ⏣0{{ trigger: manual, response_format: temporal_node, origin_session: \"medousa-coder-memory-overview\", compression_depth: 1, parent_node: ref:⏣0, prime: {{ attractor_config: {{ stability: 0.96, friction: 0.12, logic: 0.99, autonomy: 0.84 }}, context_summary: \"Bounded semantic working state for the governed Coder environment.\", relevant_tier: raw, retrieval_budget: 12 }} }} ⟩"
    );
    let _ = writeln!(
        out,
        "⦿⟨ ⏣0{{ timestamp: \"{timestamp}\", tier: raw, session_id: \"medousa-coder-memory-overview\", schema_version: \"sttp-1.0\", user_avec: {{ stability: 0.90, friction: 0.20, logic: 0.96, autonomy: 0.84, psi: 2.90 }}, model_avec: {{ stability: 0.96, friction: 0.12, logic: 0.99, autonomy: 0.84, psi: 2.95 }} }} ⟩"
    );
    let _ = writeln!(out, "◈⟨ ⏣0{{");
    let _ = writeln!(out, "    environment_memory(.99): {overview},");
    let _ = writeln!(
        out,
        "    memory_contract(.99): \"Treat current non-stale nodes as compact working state. Inherited nodes are frozen at the fork cutoff; accepted knowledge is review-promoted; live sibling state is excluded. Queued writes are usable summaries awaiting durable Locus storage. Verify repository facts against Forge and Git before mutation.\""
    );
    let _ = writeln!(out, "}} ⟩");
    let _ = write!(
        out,
        "⍉⟨ ⏣0{{ rho: 0.99, kappa: 0.99, psi: 2.95, compression_avec: {{ stability: 0.96, friction: 0.12, logic: 0.99, autonomy: 0.84, psi: 2.95 }} }} ⟩"
    );
    debug_assert!(
        super::sttp::validate_canonical_sttp_node(&out).is_ok(),
        "Coder environment-memory compiler emitted invalid STTP"
    );
    out
}

fn recalled_content(raw: &str) -> String {
    let Some(start) = unquoted_marker_positions(raw, "◈⟨").first().copied() else {
        return raw.to_string();
    };
    let end = unquoted_marker_positions(raw, "⍉⟨")
        .into_iter()
        .find(|end| *end > start)
        .unwrap_or(raw.len());
    decode_sttp_display_strings(raw[start..end].trim())
}

fn tag_value(tags: &[&str], prefix: &str) -> Option<String> {
    tags.iter()
        .find_map(|tag| tag.strip_prefix(prefix))
        .map(str::to_string)
}

fn indexed_tag(prefix: &str, value: &str) -> String {
    let tag = format!("{prefix}:{value}");
    if tag.chars().count() <= 64 {
        tag
    } else {
        format!("{prefix}-sha:{}", short_digest(value))
    }
}

fn required_text(input: &Value, field: &str, max_chars: usize) -> Result<String> {
    input
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| input_error(format!("{field} is required")))
        .and_then(|value| bounded_text(value, field, max_chars))
}

fn optional_text(input: &Value, field: &str, max_chars: usize) -> Result<Option<String>> {
    input
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| bounded_text(value, field, max_chars))
        .transpose()
}

fn bounded_text(value: &str, field: &str, max_chars: usize) -> Result<String> {
    if value.chars().count() > max_chars {
        Err(input_error(format!(
            "{field} exceeds the {max_chars}-character Coder memory limit"
        )))
    } else {
        Ok(value.to_string())
    }
}

fn string_list(
    input: &Value,
    field: &str,
    max_items: usize,
    max_chars: usize,
) -> Result<Vec<String>> {
    let Some(values) = input.get(field) else {
        return Ok(Vec::new());
    };
    let values = values
        .as_array()
        .ok_or_else(|| input_error(format!("{field} must be an array")))?;
    if values.len() > max_items {
        return Err(input_error(format!(
            "{field} exceeds the {max_items}-item Coder memory limit"
        )));
    }
    let mut out = Vec::new();
    for value in values {
        let value = value
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| input_error(format!("{field} entries must be non-empty strings")))?;
        out.push(bounded_text(value, field, max_chars)?);
    }
    dedupe_preserving_order(&mut out);
    Ok(out)
}

fn parse_relations(input: &Value) -> Result<Vec<CoderMemoryRelation>> {
    let Some(relations) = input.get("relations") else {
        return Ok(Vec::new());
    };
    let relations = relations
        .as_array()
        .ok_or_else(|| input_error("relations must be an array"))?;
    if relations.len() > MAX_RELATIONS {
        return Err(input_error(format!(
            "relations exceeds the {MAX_RELATIONS}-item Coder memory limit"
        )));
    }
    relations
        .iter()
        .map(|relation| {
            let rel = required_text(relation, "rel", 64)?;
            if !MEMORY_RELATIONS.contains(&rel.as_str()) {
                return Err(input_error(format!(
                    "unknown Coder memory relation '{rel}'; expected one of {}",
                    MEMORY_RELATIONS.join(", ")
                )));
            }
            let target = super::coder_evidence::redact_evidence_text(&required_text(
                relation,
                "target",
                MAX_ITEM_CHARS,
            )?);
            let confidence = relation
                .get("confidence")
                .and_then(Value::as_f64)
                .unwrap_or(0.9);
            if !(0.0..=1.0).contains(&confidence) {
                return Err(input_error("relation confidence must be between 0 and 1"));
            }
            Ok(CoderMemoryRelation {
                relation: rel,
                target,
                confidence,
            })
        })
        .collect()
}

fn normalize_relative_path(raw: &str) -> Result<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(input_error("memory path cannot be empty"));
    }
    let path = Path::new(raw);
    if path.is_absolute() {
        return Err(input_error("memory paths must be repository-relative"));
    }
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().to_string()),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(input_error(
                    "memory paths cannot escape the governed repository",
                ));
            }
        }
    }
    if parts.is_empty() {
        return Err(input_error("memory path cannot be empty"));
    }
    Ok(parts.join("/"))
}

fn dedupe_preserving_order(values: &mut Vec<String>) {
    let mut seen = HashSet::new();
    values.retain(|value| seen.insert(value.clone()));
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        value.to_string()
    } else {
        value.chars().take(max_chars).collect::<String>() + "…"
    }
}

fn json_string(value: &str) -> String {
    escape_protocol_glyphs(&serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string()))
}

fn json_string_array(values: &[String]) -> String {
    escape_protocol_glyphs(&serde_json::to_string(values).unwrap_or_else(|_| "[]".to_string()))
}

fn parser_safe_json(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => json_string(value),
        Value::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(parser_safe_json)
                .collect::<Vec<_>>()
                .join(",")
        ),
        Value::Object(values) => format!(
            "{{{}}}",
            values
                .iter()
                .map(|(key, value)| format!("{}:{}", json_string(key), parser_safe_json(value)))
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

fn parser_encoded_string(value: &str) -> String {
    let encoded = json_string(value);
    encoded
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(&encoded)
        .to_string()
}

fn decode_parser_string(value: &str) -> String {
    serde_json::from_str::<String>(&format!("\"{value}\"")).unwrap_or_else(|_| value.to_string())
}

fn decode_sttp_display_strings(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut cursor = 0usize;
    while let Some(relative_start) = value[cursor..].find('"') {
        let start = cursor + relative_start;
        output.push_str(&value[cursor..start]);
        let mut escaped = false;
        let mut end = None;
        for (relative_end, character) in value[start + 1..].char_indices() {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                end = Some(start + 1 + relative_end);
                break;
            }
        }
        let Some(end) = end else {
            output.push_str(&value[start..]);
            return output;
        };
        let token = &value[start..=end];
        if let Ok(decoded) = serde_json::from_str::<String>(token) {
            output.push_str(&serde_json::to_string(&decoded).unwrap_or_else(|_| token.to_string()));
        } else {
            output.push_str(token);
        }
        cursor = end + 1;
    }
    output.push_str(&value[cursor..]);
    output
}

fn escape_protocol_glyphs(value: &str) -> String {
    // Locus 0.4.2's structural lexer also counts braces inside quoted data,
    // so encode both protocol glyphs and object delimiters in runtime-owned
    // string values before assembling the canonical blocks.
    const PROTOCOL_GLYPHS: &str = "{}⊕⟨⟩⦿◈⍉⏣";
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if PROTOCOL_GLYPHS.contains(character) {
            let _ = write!(escaped, "\\u{:04x}", character as u32);
        } else {
            escaped.push(character);
        }
    }
    escaped
}

fn unquoted_marker_positions(value: &str, marker: &str) -> Vec<usize> {
    let mut positions = Vec::new();
    let mut in_string = false;
    let mut escaped = false;
    for (index, character) in value.char_indices() {
        if in_string && escaped {
            escaped = false;
            continue;
        }
        if in_string && character == '\\' {
            escaped = true;
            continue;
        }
        if character == '"' {
            in_string = !in_string;
            continue;
        }
        if !in_string && value[index..].starts_with(marker) {
            positions.push(index);
        }
    }
    positions
}

fn unquoted_string_fields(block: &str, field: &str) -> Vec<String> {
    let marker = format!("{field}:");
    let mut values = Vec::new();
    let mut in_string = false;
    let mut escaped = false;
    for (index, character) in block.char_indices() {
        if in_string && escaped {
            escaped = false;
            continue;
        }
        if in_string && character == '\\' {
            escaped = true;
            continue;
        }
        if character == '"' {
            in_string = !in_string;
            continue;
        }
        let preceded_by_identifier = block[..index]
            .chars()
            .next_back()
            .is_some_and(|character| character.is_ascii_alphanumeric() || character == '_');
        if in_string || preceded_by_identifier || !block[index..].starts_with(&marker) {
            continue;
        }
        let value = block[index + marker.len()..].trim_start();
        if !value.starts_with('"') {
            continue;
        }
        let mut value_escaped = false;
        for (end, value_character) in value.char_indices().skip(1) {
            if value_escaped {
                value_escaped = false;
            } else if value_character == '\\' {
                value_escaped = true;
            } else if value_character == '"' {
                if let Ok(parsed) = serde_json::from_str::<String>(&value[..=end]) {
                    values.push(parsed);
                }
                break;
            }
        }
    }
    values
}

fn input_error(message: impl Into<String>) -> StasisError {
    StasisError::PortFailure(format!("Coder memory: {}", message.into()))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use locus_core_rs::ParseProfile;
    use tempfile::TempDir;

    use super::*;
    use crate::agent_runtime::coder_mode::{
        CoderEditorContext, CoderMemoryParentContext, RepositoryInstruction,
    };

    fn entry(branch: &str, generation: u32) -> CoderEntryContext {
        CoderEntryContext {
            repo_id: "repo-123".to_string(),
            work_id: "work-456".to_string(),
            title: "Demo".to_string(),
            brief: "Build memory".to_string(),
            worktree: PathBuf::from("/tmp/demo"),
            branch: branch.to_string(),
            environment_generation: generation,
            memory_parent: None,
            baseline_oid: "a".repeat(40),
            head_oid: "b".repeat(40),
            changed_paths: Vec::new(),
            allowed_paths: Vec::new(),
            denied_paths: Vec::new(),
            project_markers: Vec::new(),
            repository_instructions: Vec::<RepositoryInstruction>::new(),
            editor: CoderEditorContext::default(),
        }
    }

    #[test]
    fn memory_scope_is_stable_for_one_environment_and_changes_for_forks() {
        let first = CoderMemoryScope::for_entry(&entry("worktree/demo-a1", 1));
        let same = CoderMemoryScope::for_entry(&entry("worktree/demo-a1", 1));
        let fork = CoderMemoryScope::for_entry(&entry("worktree/demo-a2", 1));
        let restart = CoderMemoryScope::for_entry(&entry("worktree/demo-a1", 2));

        assert_eq!(first, same);
        assert_ne!(first.session_id, fork.session_id);
        assert_ne!(first.session_id, restart.session_id);
        assert!(first.session_id.contains("coder:repo-123:work-456:"));
        assert!(!first.session_id.contains("/tmp/demo"));

        assert_eq!(
            first.accepted_undertaking_scope().session_id,
            fork.accepted_undertaking_scope().session_id
        );
        assert_eq!(
            first.accepted_repository_scope().session_id,
            fork.accepted_repository_scope().session_id
        );
    }

    #[test]
    fn retry_queue_is_persistent_bounded_deduplicated_and_redacted() {
        let temp = TempDir::new().expect("queue tempdir");
        let entry = entry("worktree/demo-a1", 1);
        let scope = CoderMemoryScope::for_entry(&entry);
        let identity = CoderAgentIdentity::for_turn("chat-1", 7, "attempt-1");
        let path = temp.path().join("memory-queue.json");
        let secret = build_commit(
            &json!({
                "kind": "checkpoint",
                "summary": "Persist recovery boundary",
                "details": "token=must-not-survive"
            }),
            &scope,
            &identity,
            &entry.head_oid,
        )
        .expect("compiled checkpoint");
        let mut queue = CoderMemoryRetryQueue::at_path(&scope, path.clone());
        assert!(queue.enqueue(secret.clone()).expect("enqueue"));
        assert!(!queue.enqueue(secret).expect("dedupe enqueue"));
        assert_eq!(queue.len(), 1);
        assert!(
            !std::fs::read_to_string(&path)
                .expect("queue file")
                .contains("must-not-survive")
        );

        drop(queue);
        let mut queue = CoderMemoryRetryQueue::at_path(&scope, path.clone());
        assert_eq!(queue.len(), 1);
        let front = queue.front().expect("restored pending write");
        queue
            .pop_front(&front.dedupe_tag)
            .expect("persist queue drain");
        assert!(CoderMemoryRetryQueue::at_path(&scope, path.clone()).is_empty());

        let mut queue = CoderMemoryRetryQueue::at_path(&scope, path.clone());
        for index in 0..(MAX_PENDING_MEMORY_WRITES + 5) {
            let commit = build_commit(
                &json!({
                    "kind": "discovery",
                    "summary": format!("Bounded queued observation {index}")
                }),
                &scope,
                &identity,
                &entry.head_oid,
            )
            .expect("compiled queued observation");
            queue.enqueue(commit).expect("bounded enqueue");
        }
        assert_eq!(queue.len(), MAX_PENDING_MEMORY_WRITES);
        drop(queue);
        assert_eq!(
            CoderMemoryRetryQueue::at_path(&scope, path).len(),
            MAX_PENDING_MEMORY_WRITES
        );
    }

    #[test]
    fn commit_compiles_strict_typed_sttp_with_scope_and_relations() {
        let entry = entry("worktree/demo-a1", 1);
        let scope = CoderMemoryScope::for_entry(&entry);
        let identity = CoderAgentIdentity::for_turn("chat-1", 7, "attempt-1");
        let commit = build_commit(
            &json!({
                "kind": "decision",
                "summary": "Keep exact turn checkpoints separate from semantic memory",
                "details": "Locus records explicit engineering state; the turn ledger owns protocol replay.",
                "paths": ["src/agent_runtime/coder_memory.rs"],
                "symbols": ["CoderMemoryScope"],
                "evidence_refs": ["engineering:call:7"],
                "relations": [{
                    "rel": "supports",
                    "target": "decision:durable-coder",
                    "confidence": 0.97
                }]
            }),
            &scope,
            &identity,
            &entry.head_oid,
        )
        .expect("commit");

        crate::agent_runtime::sttp::validate_canonical_sttp_node(&commit.raw_node)
            .expect("runtime STTP");
        validate_raw_node_scope(&commit.raw_node, &scope.session_id).expect("matching scope");
        let scope_error = validate_raw_node_scope(&commit.raw_node, "another-locus-session")
            .expect_err("mismatched scope rejected");
        assert!(
            scope_error
                .to_string()
                .contains("governed Coder environment")
        );
        let parsed = SttpNodeParser::with_profile(ParseProfile::StrictTypedIr)
            .try_parse(&commit.raw_node, &scope.session_id);
        assert!(
            parsed.success,
            "strict parse failed: {:?}\ndiagnostics={:#?}\n{}",
            parsed.error, parsed.diagnostics, commit.raw_node
        );
        let parsed = parsed.node.expect("parsed node");
        assert_eq!(parsed.session_id, scope.session_id);
        assert!(parsed.semantic_tags.as_ref().is_some_and(|tags| {
            tags.contains(&"kind:decision".to_string())
                && tags.contains(&format!("head:{}", entry.head_oid))
        }));
        assert!(parsed.semantic_links.as_ref().is_some_and(|links| {
            links
                .iter()
                .any(|link| link.rel == "supports" && link.target == "decision:durable-coder")
        }));
    }

    #[test]
    fn structured_commit_escapes_hostile_text_without_model_authored_sttp() {
        let entry = entry("worktree/demo-a1", 1);
        let scope = CoderMemoryScope::for_entry(&entry);
        let identity = CoderAgentIdentity::for_turn("chat-1", 8, "attempt-1");
        let hostile = "quoted \"value\", slash \\\\, newline\nbrace { }, comma, and protocol markers ⊕⟨ ⦿⟨ ◈⟨ ⍉⟨ ⏣0{";
        let hostile_path = "src/{odd \"quoted\" name}.rs";
        let commit = build_commit(
            &json!({
                "kind": "discovery",
                "summary": hostile,
                "details": hostile,
                "paths": [hostile_path],
                "symbols": [hostile],
                "evidence_refs": ["coder-evidence:sha256:abc\\def"],
                "relations": [{
                    "rel": "derived_from",
                    "target": hostile
                }]
            }),
            &scope,
            &identity,
            &entry.head_oid,
        )
        .expect("runtime compiles hostile structured input");

        crate::agent_runtime::sttp::validate_canonical_sttp_node(&commit.raw_node)
            .expect("runtime STTP shape");
        let parsed = SttpNodeParser::with_profile(ParseProfile::StrictTypedIr)
            .try_parse(&commit.raw_node, &scope.session_id);
        assert!(
            parsed.success,
            "strict parser rejected runtime-escaped input: {:?}\ndiagnostics={:#?}\n{}",
            parsed.error, parsed.diagnostics, commit.raw_node
        );
        let parsed = parsed.node.expect("strict node");
        let tags = parsed.semantic_tags.expect("semantic tags");
        let query = parse_recall_query(&json!({
            "query": "hostile path",
            "kind": "discovery",
            "path": hostile_path
        }))
        .expect("recall query");
        for tag in recall_semantic_tags(&query) {
            assert!(
                tags.contains(&tag),
                "stored tag does not match recall filter: {tag}"
            );
        }
        assert_eq!(
            parsed.context_summary.as_deref().map(decode_parser_string),
            Some(format!("discovery: {hostile}"))
        );
        assert!(recalled_content(&commit.raw_node).contains("brace { }"));
        assert!(recalled_content(&commit.raw_node).contains("protocol markers ⊕⟨"));
    }

    #[test]
    fn experiment_notebook_kinds_keep_the_compact_commit_contract() {
        let commit_tool = tool_definitions()
            .into_iter()
            .find(|tool| tool.name.as_str() == COGNITION_CODER_MEMORY_COMMIT)
            .expect("memory commit tool");
        assert_eq!(
            commit_tool
                .schema
                .as_ref()
                .and_then(|schema| schema.get("required")),
            Some(&json!(["kind", "summary"]))
        );
        let kinds = commit_tool
            .schema
            .as_ref()
            .and_then(|schema| schema.pointer("/properties/kind/enum"))
            .and_then(Value::as_array)
            .expect("memory kinds");
        let scope = CoderMemoryScope::for_entry(&entry("worktree/demo-a1", 1));
        let identity = CoderAgentIdentity::for_turn("chat-1", 8, "attempt-1");
        for kind in ["experiment", "acceptance_criterion", "next_action"] {
            assert!(
                kinds
                    .iter()
                    .any(|candidate| candidate.as_str() == Some(kind))
            );
            build_commit(
                &json!({ "kind": kind, "summary": format!("Record {kind}") }),
                &scope,
                &identity,
                "head-1",
            )
            .expect("compact experiment commit");
            let inherited = json!({
                "timestamp": "2026-08-08T00:00:00Z",
                "semantic_tags": [format!("kind:{kind}")]
            });
            let mut child_scope = scope.clone();
            child_scope.parent = Some(CoderMemoryParentScope {
                session_id: "parent-session".into(),
                branch: "worktree/demo-parent".into(),
                branch_digest: "parent".into(),
                environment_generation: 1,
                inherited_before_utc: "2026-08-08T01:00:00Z".into(),
            });
            assert!(inherited_node_allowed(&child_scope, &inherited));
            let accepted = json!({
                "semantic_tags": [
                    format!("kind:{kind}"),
                    "knowledge:accepted",
                    "memory-scope:undertaking"
                ]
            });
            assert!(!accepted_node_allowed(
                &accepted,
                "memory-scope:undertaking"
            ));
        }
    }

    #[test]
    fn commit_dedupe_key_is_semantic_and_path_escape_is_rejected() {
        let entry = entry("worktree/demo-a1", 1);
        let scope = CoderMemoryScope::for_entry(&entry);
        let identity = CoderAgentIdentity::for_turn("chat-1", 7, "attempt-1");
        let input = json!({
            "kind": "verification",
            "summary": "Focused tests pass",
            "details": "token=must-not-persist verification remained green",
            "paths": ["src/lib.rs"]
        });
        let first = build_commit(&input, &scope, &identity, &entry.head_oid).expect("first");
        let second = build_commit(&input, &scope, &identity, &entry.head_oid).expect("second");
        assert_eq!(first.dedupe_tag, second.dedupe_tag);
        assert!(!first.raw_node.contains("must-not-persist"));
        assert!(first.raw_node.contains("token=[REDACTED]"));

        let escaped = build_commit(
            &json!({
                "kind": "discovery",
                "summary": "Unsafe path",
                "paths": ["../outside"]
            }),
            &scope,
            &identity,
            &entry.head_oid,
        )
        .expect_err("path escape denied");
        assert!(escaped.to_string().contains("cannot escape"));
    }

    #[test]
    fn recall_projection_labels_changed_head_stale_and_hides_locus_session() {
        let scope = CoderMemoryScope::for_entry(&entry("worktree/demo-a1", 1));
        let result = json!({
            "nodes": [{
                "sync_key": "node-1",
                "session_id": scope.session_id,
                "timestamp": "2026-08-08T00:00:00Z",
                "context_summary": "decision: use environment lineage",
                "semantic_tags": [
                    "kind:decision",
                    "head:old-head",
                    "path:src/lib.rs",
                    "symbol:demo::run"
                ],
                "raw": "bounded STTP"
            }]
        });
        let projected = project_recall(&scope, "new-head", &result, true, 5);
        assert_eq!(projected["nodes"][0]["stale"], true);
        assert_eq!(projected["nodes"][0]["kind"], "decision");
        assert_eq!(projected["nodes"][0]["paths"][0], "src/lib.rs");
        assert_eq!(projected["nodes"][0]["symbols"][0], "demo::run");
        assert!(projected.to_string().contains("bounded STTP"));
        assert!(!projected.to_string().contains(&scope.session_id));
    }

    #[test]
    fn lineage_projection_honors_fork_cutoff_and_filters_unaccepted_siblings() {
        let mut child_entry = entry("worktree/demo-a2", 1);
        child_entry.memory_parent = Some(CoderMemoryParentContext {
            branch: "worktree/demo-a1".into(),
            environment_generation: 1,
            inherited_before_utc: "2026-08-08T01:00:00Z".into(),
        });
        let scope = CoderMemoryScope::for_entry(&child_entry);
        let current = json!({
            "nodes": [{
                "sync_key": "current-1",
                "timestamp": "2026-08-08T03:00:00Z",
                "context_summary": "hypothesis: current child idea",
                "semantic_tags": ["kind:hypothesis", "head:child-head"],
                "raw": ""
            }]
        });
        let parent = json!({
            "nodes": [
                {
                    "sync_key": "parent-before",
                    "timestamp": "2026-08-08T00:30:00Z",
                    "context_summary": "decision: inherited decision",
                    "semantic_tags": ["kind:decision", "head:parent-head"],
                    "raw": ""
                },
                {
                    "sync_key": "parent-transient",
                    "timestamp": "2026-08-08T00:20:00Z",
                    "context_summary": "hypothesis: transient parent idea",
                    "semantic_tags": ["kind:hypothesis", "head:parent-head"],
                    "raw": ""
                },
                {
                    "sync_key": "parent-after",
                    "timestamp": "2026-08-08T02:00:00Z",
                    "context_summary": "decision: post-fork decision",
                    "semantic_tags": ["kind:decision", "head:parent-head"],
                    "raw": ""
                }
            ]
        });
        let undertaking = json!({
            "nodes": [
                {
                    "sync_key": "accepted-work",
                    "timestamp": "2026-08-08T00:00:00Z",
                    "context_summary": "decision: accepted undertaking decision",
                    "semantic_tags": [
                        "kind:decision",
                        "head:accepted-head",
                        "knowledge:accepted",
                        "memory-scope:undertaking"
                    ],
                    "raw": ""
                },
                {
                    "sync_key": "sibling-transient",
                    "timestamp": "2026-08-08T00:00:00Z",
                    "context_summary": "decision: live sibling conclusion",
                    "semantic_tags": ["kind:decision", "head:sibling-head"],
                    "raw": ""
                }
            ]
        });
        let repository = json!({
            "nodes": [{
                "sync_key": "accepted-repo",
                "timestamp": "2026-08-08T00:00:00Z",
                "context_summary": "verification: accepted repository check",
                "semantic_tags": [
                    "kind:verification",
                    "head:accepted-head",
                    "knowledge:accepted",
                    "memory-scope:repository"
                ],
                "raw": ""
            }]
        });

        let projected = project_lineage_recall(
            &scope,
            "child-head",
            CoderMemoryLineageSources {
                current: Some(&current),
                parent: Some(&parent),
                undertaking: Some(&undertaking),
                repository: Some(&repository),
            },
            false,
            10,
        );
        let rendered = projected.to_string();
        assert!(rendered.contains("current child idea"));
        assert!(rendered.contains("inherited decision"));
        assert!(rendered.contains("accepted undertaking decision"));
        assert!(rendered.contains("accepted repository check"));
        assert!(!rendered.contains("transient parent idea"));
        assert!(!rendered.contains("post-fork decision"));
        assert!(!rendered.contains("live sibling conclusion"));
        assert_eq!(projected["available_by_origin"]["inherited_parent"], 1);
    }

    #[test]
    fn promotion_compiler_keeps_scope_runtime_owned_and_links_source_node() {
        let source_scope = CoderMemoryScope::for_entry(&entry("worktree/demo-a1", 1));
        let target_scope = source_scope.accepted_undertaking_scope();
        let identity =
            CoderAgentIdentity::for_turn("forge-memory-lifecycle", "decision-1", "attempt-1");
        let commit = build_promotion_commit(
            &json!({
                "node_id": "source-node-1",
                "kind": "decision",
                "summary": "decision: preserve the typed facade",
                "paths": ["src/agent_runtime/coder_memory.rs"],
                "symbols": ["CoderMemoryScope"]
            }),
            &target_scope,
            &identity,
            "accepted-head",
            "decision-1",
            "evidence-1",
            "digest-1",
        )
        .expect("promotion commit");

        validate_raw_node_scope(&commit.raw_node, &target_scope.session_id)
            .expect("accepted target scope");
        assert!(commit.semantic_tags.contains(&"knowledge:accepted".into()));
        assert!(
            commit
                .semantic_tags
                .contains(&"memory-scope:undertaking".into())
        );
        assert!(commit.raw_node.contains("derived_from"));
        assert!(commit.raw_node.contains("source-node-1"));
        assert!(
            !target_scope
                .public_descriptor()
                .to_string()
                .contains("session")
        );
    }

    #[test]
    fn environment_overview_is_canonical_and_protocol_safe() {
        let scope = CoderMemoryScope::for_entry(&entry("worktree/demo-a1", 1));
        let overview = json!({
            "ok": true,
            "scope": scope.public_descriptor(),
            "current_head": "head-1",
            "nodes": [{
                "kind": "open_gap",
                "summary": "Quoted { gap } with ⊕⟨ marker",
                "paths": ["src/{odd}.rs"],
                "symbols": ["demo::run"]
            }],
            "pending_writes": []
        });
        let appendix = environment_overview_prompt_appendix(&overview);
        crate::agent_runtime::sttp::validate_canonical_sttp_node(&appendix)
            .expect("canonical overview STTP");
        assert!(!appendix.contains(&scope.session_id));
    }

    #[test]
    fn recalled_content_ignores_protocol_markers_inside_quoted_data() {
        let raw = "◈⟨ ⏣0{ detail(.95): \"quoted ⍉⟨ marker\", next(.99): \"keep me\" } ⟩\n\
⍉⟨ ⏣0{ rho: 0.9 } ⟩";
        let content = recalled_content(raw);
        assert!(content.contains("quoted ⍉⟨ marker"));
        assert!(content.contains("keep me"));
        assert!(!content.contains("rho: 0.9"));
    }
}
