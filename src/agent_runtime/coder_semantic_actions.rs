//! Governed semantic code actions for one exact Coder attempt.
//!
//! Language servers may propose a `WorkspaceEdit`; they never mutate Forge
//! state directly. The runtime normalizes the complete proposal, snapshots
//! every touched path, issues a stable change-set object, and applies that
//! object once through Forge's lease- and digest-fenced transaction endpoint.

use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};
use std::path::{Component, Path, PathBuf};

use genai::chat::Tool;
use medousa_forge::forge::Forge;
use medousa_forge::model::{ChangeStatus, ChangedFile, ExecutionLease, WorkPolicy};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest as _, Sha256};
use stasis::domain::errors::StasisError;
use stasis::prelude::Result;

use super::coder_mode::CoderEntryContext;

pub const COGNITION_CODER_SYMBOL_REFACTOR: &str = "cognition_coder_symbol_refactor";
pub const COGNITION_CODER_CHANGE_SET_APPLY: &str = "cognition_coder_change_set_apply";
pub const COGNITION_CODER_AFFECTED_TESTS: &str = "cognition_coder_affected_tests";

pub const SEMANTIC_ACTION_TOOL_NAMES: &[&str] = &[
    COGNITION_CODER_SYMBOL_REFACTOR,
    COGNITION_CODER_CHANGE_SET_APPLY,
    COGNITION_CODER_AFFECTED_TESTS,
];

const MAX_WORKSPACE_OPERATIONS: usize = 512;
const MAX_WORKSPACE_BYTES: usize = 8 * 1024 * 1024;
const MAX_SOURCE_BYTES: usize = 2 * 1024 * 1024;
const MAX_CHANGE_SETS: usize = 8;
const MAX_CHANGE_SET_CACHE_BYTES: usize = 16 * 1024 * 1024;
const MAX_TEXT_EDITS_PER_DOCUMENT: usize = 4_096;
const MAX_REFERENCE_RESULTS: usize = 200;
const MAX_TEST_CANDIDATES: usize = 2_000;
const MAX_AFFECTED_PATHS: usize = 20;

pub fn tool_definitions() -> Vec<Tool> {
    vec![
        Tool::new(COGNITION_CODER_SYMBOL_REFACTOR)
            .with_description(
                "Resolve symbol references or preview a language-server rename. Rename previews return a change_set_id for cognition_coder_change_set_apply.",
            )
            .with_schema(json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["references", "rename_preview"]
                    },
                    "path": {
                        "type": "string",
                        "description": "Repository-relative source path"
                    },
                    "line": { "type": "integer", "minimum": 0 },
                    "character": {
                        "type": "integer",
                        "minimum": 0,
                        "description": "0-based UTF-16 character offset"
                    },
                    "new_name": {
                        "type": "string",
                        "description": "Required only for rename_preview"
                    },
                    "language": {
                        "type": "string",
                        "description": "Optional language override when extension detection is insufficient"
                    }
                },
                "required": ["action", "path", "line", "character"]
            })),
        Tool::new(COGNITION_CODER_CHANGE_SET_APPLY)
            .with_description(
                "Apply a change set returned by a Coder preview tool.",
            )
            .with_schema(json!({
                "type": "object",
                "properties": {
                    "change_set_id": {
                        "type": "string",
                        "description": "Stable id returned by cognition_coder_symbol_refactor"
                    }
                },
                "required": ["change_set_id"]
            })),
        Tool::new(COGNITION_CODER_AFFECTED_TESTS)
            .with_description(
                "Rank repository-discovered tests affected by current or supplied paths and an optional symbol. This selects verification targets; it does not run them.",
            )
            .with_schema(json!({
                "type": "object",
                "properties": {
                    "paths": {
                        "type": "array",
                        "items": { "type": "string" },
                        "maxItems": MAX_AFFECTED_PATHS,
                        "description": "Optional repository-relative paths; current dirty paths are used when omitted"
                    },
                    "symbol": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 50 }
                }
            })),
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ChangeSetState {
    Previewed,
    Applying,
    Applied,
    Uncertain,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum WorkspacePrecondition {
    Existing {
        path: String,
        expected_digest: String,
    },
    Missing {
        path: String,
    },
}

impl WorkspacePrecondition {
    fn path(&self) -> &str {
        match self {
            Self::Existing { path, .. } | Self::Missing { path } => path,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum WorkspaceOperation {
    Write { path: String, content: String },
    Create { path: String, content: String },
    Rename { path: String, destination: String },
    Delete { path: String },
}

impl WorkspaceOperation {
    fn paths(&self) -> Vec<&str> {
        match self {
            Self::Write { path, .. } | Self::Create { path, .. } | Self::Delete { path } => {
                vec![path]
            }
            Self::Rename { path, destination } => vec![path, destination],
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct ChangeSetFile {
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    old_path: Option<String>,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    before_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    after_digest: Option<String>,
    before_bytes: usize,
    after_bytes: usize,
}

#[derive(Debug, Clone)]
struct SemanticChangeSet {
    id: String,
    work_id: String,
    attempt_id: String,
    base_head_oid: String,
    source_path: String,
    source_line: u32,
    source_character: u32,
    new_name: String,
    preconditions: Vec<WorkspacePrecondition>,
    operations: Vec<WorkspaceOperation>,
    files: Vec<ChangeSetFile>,
    annotation_labels: Vec<String>,
}

impl SemanticChangeSet {
    fn payload_bytes(&self) -> usize {
        self.operations
            .iter()
            .map(|operation| match operation {
                WorkspaceOperation::Write { content, .. }
                | WorkspaceOperation::Create { content, .. } => content.len(),
                WorkspaceOperation::Rename { .. } | WorkspaceOperation::Delete { .. } => 0,
            })
            .sum()
    }

    fn touched_paths(&self) -> Vec<String> {
        let mut paths = self
            .operations
            .iter()
            .flat_map(WorkspaceOperation::paths)
            .map(str::to_string)
            .collect::<Vec<_>>();
        paths.sort();
        paths.dedup();
        paths
    }

    fn projection(&self, state: ChangeSetState) -> Value {
        json!({
            "id": self.id,
            "kind": "symbol_rename",
            "state": state,
            "work_id": self.work_id,
            "attempt_id": self.attempt_id,
            "base_head_oid": self.base_head_oid,
            "source": {
                "path": self.source_path,
                "line": self.source_line,
                "character": self.source_character,
                "new_name": self.new_name,
            },
            "precondition_count": self.preconditions.len(),
            "operation_count": self.operations.len(),
            "file_count": self.files.len(),
            "files": self.files,
            "annotation_labels": self.annotation_labels,
            "safety": {
                "language_server_mutated_files": false,
                "forge_transaction_required": true,
                "every_touched_path_preconditioned": true,
                "raw_source_included": false,
            }
        })
    }
}

#[derive(Debug, Clone)]
struct CachedChangeSet {
    plan: SemanticChangeSet,
    state: ChangeSetState,
}

#[derive(Debug, Default)]
pub struct CoderChangeSetStore {
    entries: VecDeque<CachedChangeSet>,
}

impl CoderChangeSetStore {
    fn insert(&mut self, plan: SemanticChangeSet) -> Result<ChangeSetState> {
        if let Some(existing) = self.entries.iter().find(|entry| entry.plan.id == plan.id) {
            return Ok(existing.state);
        }
        let plan_bytes = plan.payload_bytes();
        while self.entries.len() >= MAX_CHANGE_SETS
            || self
                .entries
                .iter()
                .map(|entry| entry.plan.payload_bytes())
                .sum::<usize>()
                .saturating_add(plan_bytes)
                > MAX_CHANGE_SET_CACHE_BYTES
        {
            let Some(removable) = self
                .entries
                .iter()
                .position(|entry| entry.state != ChangeSetState::Applying)
            else {
                return Err(input_error(
                    "change-set cache is occupied by in-flight work; finish reconciliation before previewing another refactor",
                ));
            };
            self.entries.remove(removable);
        }
        self.entries.push_back(CachedChangeSet {
            plan,
            state: ChangeSetState::Previewed,
        });
        Ok(ChangeSetState::Previewed)
    }

    pub fn paths_for(&self, change_set_id: &str) -> Option<Vec<String>> {
        self.entries
            .iter()
            .find(|entry| entry.plan.id == change_set_id.trim())
            .map(|entry| entry.plan.touched_paths())
    }

    fn begin_apply(
        &mut self,
        change_set_id: &str,
        lease: &ExecutionLease,
    ) -> Result<SemanticChangeSet> {
        let entry = self
            .entries
            .iter_mut()
            .find(|entry| entry.plan.id == change_set_id.trim())
            .ok_or_else(|| {
                input_error(
                    "change set is unavailable in this active turn; preview the refactor again",
                )
            })?;
        if entry.plan.work_id != lease.work_id.as_str()
            || entry.plan.attempt_id != lease.attempt_id.as_str()
        {
            return Err(input_error(
                "change set belongs to a different governed attempt",
            ));
        }
        match entry.state {
            ChangeSetState::Previewed => {
                entry.state = ChangeSetState::Applying;
                Ok(entry.plan.clone())
            }
            ChangeSetState::Applied => Err(input_error(
                "change set is already applied; no side effect was replayed",
            )),
            ChangeSetState::Applying | ChangeSetState::Uncertain => Err(input_error(
                "change-set outcome is uncertain; automatic replay is forbidden—reconcile the worktree and preview again",
            )),
        }
    }

    fn finish_apply(&mut self, change_set_id: &str, succeeded: bool) {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.plan.id == change_set_id)
        {
            entry.state = if succeeded {
                ChangeSetState::Applied
            } else {
                ChangeSetState::Uncertain
            };
        }
    }
}

pub async fn invoke_symbol_refactor(
    forge: &Forge,
    store: &std::sync::Mutex<CoderChangeSetStore>,
    entry: &CoderEntryContext,
    lease: &ExecutionLease,
    policy: &WorkPolicy,
    input: &Value,
) -> Result<Value> {
    let action = required_string(input, "action")?;
    if !matches!(action, "references" | "rename_preview") {
        return Err(input_error("action must be references or rename_preview"));
    }
    let path = normalize_relative_path(required_string(input, "path")?)?;
    let document = resolve_existing_path(&entry.worktree, &path)?;
    let uri = reqwest::Url::from_file_path(&document)
        .map_err(|_| input_error("could not construct the governed document URI"))?
        .to_string();
    let line = required_u32(input, "line")?;
    let character = required_u32(input, "character")?;
    let language = optional_bounded_string(input, "language", 80)?;
    let new_name = if action == "rename_preview" {
        Some(validate_new_name(required_string(input, "new_name")?)?)
    } else {
        None
    };
    let mut request = json!({
        "action": if action == "references" { "references" } else { "rename" },
        "uri": uri,
        "line": line,
        "character": character,
        "work_id": entry.work_id,
        "attempt_id": lease.attempt_id,
    });
    if let Some(language) = language {
        request["language"] = Value::String(language);
    }
    if let Some(new_name) = new_name.as_deref() {
        request["new_name"] = Value::String(new_name.to_string());
    }
    let response = crate::code_intelligence_tools::request_code_action(request).await?;
    let result = response
        .get("result")
        .ok_or_else(|| input_error("coding engine returned no language-server result"))?;
    let head_oid = forge
        .git()
        .head_oid(&entry.worktree)
        .map_err(|error| input_error(format!("cannot observe Coder HEAD: {error}")))?
        .to_string();
    if action == "references" {
        return project_references(entry, &path, line, character, &head_oid, result);
    }

    let plan = build_workspace_edit_plan(
        entry,
        lease,
        policy,
        &head_oid,
        &path,
        line,
        character,
        new_name.as_deref().expect("rename name"),
        result,
    )?;
    let mut store = store
        .lock()
        .map_err(|error| input_error(format!("change-set store is unavailable: {error}")))?;
    let state = store.insert(plan.clone())?;
    Ok(json!({
        "ok": true,
        "action": "rename_preview",
        "change_set": plan.projection(state),
        "next_decision": if state == ChangeSetState::Previewed {
            "Review the bounded file list, then pass change_set.id to cognition_coder_change_set_apply only if the complete rename is intended."
        } else {
            "This exact change set already has lifecycle state; inspect change_set.state and do not replay an uncertain application."
        }
    }))
}

pub async fn apply_change_set(
    store: &std::sync::Mutex<CoderChangeSetStore>,
    lease: &ExecutionLease,
    input: &Value,
) -> Result<Value> {
    let change_set_id = required_string(input, "change_set_id")?.trim().to_string();
    let plan = store
        .lock()
        .map_err(|error| input_error(format!("change-set store is unavailable: {error}")))?
        .begin_apply(&change_set_id, lease)?;
    let body = json!({
        "preconditions": plan.preconditions,
        "operations": plan.operations,
        "lease_id": lease.lease_id,
        "generation": lease.generation,
    });
    let url = format!(
        "{}/v1/forge/items/{}/source/workspace-edit",
        crate::daemon_self_url::daemon_self_base_url().trim_end_matches('/'),
        urlencoding::encode(lease.work_id.as_str()),
    );
    let client = crate::daemon_self_url::authenticated_http_client()
        .map_err(|error| input_error(format!("cannot authorize Forge change set: {error}")))?;
    let response = client.put(url).json(&body).send().await;
    let result = match response {
        Ok(response) if response.status().is_success() => response
            .json::<Value>()
            .await
            .map_err(|error| input_error(format!("invalid Forge change-set response: {error}"))),
        Ok(response) => {
            let status = response.status();
            let detail = response.text().await.unwrap_or_default();
            Err(input_error(format!(
                "Forge rejected change set {change_set_id} ({status}); outcome is not replayable: {}",
                bounded(&detail, 500)
            )))
        }
        Err(error) => Err(input_error(format!(
            "Forge change-set outcome is uncertain after transport failure; automatic replay is forbidden: {error}"
        ))),
    };
    let succeeded = result.is_ok();
    store
        .lock()
        .map_err(|error| input_error(format!("change-set store is unavailable: {error}")))?
        .finish_apply(&change_set_id, succeeded);
    let response = result?;
    let updated_files = response
        .as_array()
        .into_iter()
        .flatten()
        .take(MAX_WORKSPACE_OPERATIONS)
        .map(|file| {
            json!({
                "path": file.get("path"),
                "digest": file.get("digest"),
                "byte_size": file.get("byte_size"),
                "encoding": file.get("encoding"),
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "ok": true,
        "change_set": plan.projection(ChangeSetState::Applied),
        "updated_files": updated_files,
        "raw_source_included": false,
        "replayed": false,
    }))
}

pub async fn affected_tests(
    forge: &Forge,
    entry: &CoderEntryContext,
    lease: &ExecutionLease,
    input: &Value,
) -> Result<Value> {
    let mut paths = input
        .get("paths")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .take(MAX_AFFECTED_PATHS)
                .map(|value| {
                    value
                        .as_str()
                        .ok_or_else(|| input_error("paths entries must be strings"))
                        .and_then(normalize_relative_path)
                })
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?
        .unwrap_or_default();
    if paths.is_empty() {
        paths = forge
            .git()
            .status_porcelain(&entry.worktree)
            .map_err(|error| input_error(format!("cannot inspect changed paths: {error}")))?
            .into_iter()
            .map(|change| change.path)
            .take(MAX_AFFECTED_PATHS)
            .collect();
    }
    if paths.is_empty()
        && let Some(active_path) = entry.editor.active_path.as_deref()
        && let Ok(path) = entry_relative_path(entry, active_path)
    {
        paths.push(path);
    }
    paths.sort();
    paths.dedup();
    if paths.is_empty() {
        return Err(input_error(
            "affected-test selection needs at least one path and the worktree has no changed paths",
        ));
    }
    let symbol = optional_bounded_string(input, "symbol", 240)?;
    let limit = input
        .get("limit")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(12)
        .clamp(1, 50);
    let mut url = reqwest::Url::parse(&format!(
        "{}/v1/forge/items/{}/tests",
        crate::daemon_self_url::daemon_self_base_url().trim_end_matches('/'),
        urlencoding::encode(lease.work_id.as_str()),
    ))
    .map_err(|error| input_error(format!("invalid Forge tests URL: {error}")))?;
    url.query_pairs_mut()
        .append_pair("attempt_id", lease.attempt_id.as_str());
    let client = crate::daemon_self_url::authenticated_http_client()
        .map_err(|error| input_error(format!("cannot authorize Forge test discovery: {error}")))?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| input_error(format!("cannot query repository tests: {error}")))?;
    if !response.status().is_success() {
        let status = response.status();
        let detail = response.text().await.unwrap_or_default();
        return Err(input_error(format!(
            "Forge test discovery failed ({status}): {}",
            bounded(&detail, 500)
        )));
    }
    let tests = response
        .json::<Vec<DiscoveredTest>>()
        .await
        .map_err(|error| input_error(format!("invalid Forge test catalog: {error}")))?;
    let head_oid = forge
        .git()
        .head_oid(&entry.worktree)
        .map_err(|error| input_error(format!("cannot observe Coder HEAD: {error}")))?
        .to_string();
    let mut broad_task_ids = tests
        .iter()
        .map(|test| test.task_id.clone())
        .collect::<Vec<_>>();
    broad_task_ids.sort();
    broad_task_ids.dedup();
    broad_task_ids.truncate(16);
    let ranked = rank_tests(&tests, &paths, symbol.as_deref(), limit);
    let selection_id = stable_id(
        "tests",
        &json!({
            "work_id": entry.work_id,
            "attempt_id": lease.attempt_id,
            "head_oid": head_oid,
            "paths": paths,
            "symbol": symbol,
            "tests": ranked.iter().map(|candidate| &candidate.test.id).collect::<Vec<_>>(),
        }),
    );
    Ok(json!({
        "ok": true,
        "selection": {
            "id": selection_id,
            "kind": "affected_test_selection",
            "head_oid": head_oid,
            "paths": paths,
            "symbol": symbol,
            "catalog_count": tests.len(),
            "candidate_count": ranked.len(),
            "candidates": ranked,
            "broad_task_ids": broad_task_ids,
        },
        "executed": false,
        "next_decision": if ranked.is_empty() {
            "No individually discoverable test had a defensible path or symbol relationship. Use a listed broad_task_id or the repository-native verification command."
        } else {
            "Run the narrow candidates first, then the broader project test task when the change warrants it."
        }
    }))
}

#[derive(Debug, Clone)]
struct SourceSnapshot {
    content: String,
    digest: String,
}

#[derive(Debug, Clone)]
struct VirtualFile {
    content: String,
    lineage: Option<String>,
}

struct WorkspaceEditBuilder<'a> {
    root: &'a Path,
    initial: BTreeMap<String, Option<SourceSnapshot>>,
    state: BTreeMap<String, VirtualFile>,
    operations: Vec<WorkspaceOperation>,
    content_bytes: usize,
}

impl<'a> WorkspaceEditBuilder<'a> {
    fn new(root: &'a Path) -> Self {
        Self {
            root,
            initial: BTreeMap::new(),
            state: BTreeMap::new(),
            operations: Vec::new(),
            content_bytes: 0,
        }
    }

    fn load_initial(&mut self, path: &str) -> Result<Option<SourceSnapshot>> {
        if let Some(snapshot) = self.initial.get(path) {
            return Ok(snapshot.clone());
        }
        let resolved = resolve_maybe_new_path(self.root, path)?;
        let snapshot = if resolved.exists() {
            if !resolved.is_file() {
                return Err(input_error(format!(
                    "semantic edit path is not a file: {path}"
                )));
            }
            let bytes = std::fs::read(&resolved)
                .map_err(|error| input_error(format!("cannot read {path}: {error}")))?;
            if bytes.len() > MAX_SOURCE_BYTES {
                return Err(input_error(format!(
                    "{path} exceeds the semantic edit file limit"
                )));
            }
            let content = String::from_utf8(bytes)
                .map_err(|_| input_error(format!("{path} is not UTF-8 source text")))?;
            Some(SourceSnapshot {
                digest: source_digest(content.as_bytes()),
                content,
            })
        } else {
            None
        };
        if let Some(snapshot) = snapshot.as_ref() {
            self.state.insert(
                path.to_string(),
                VirtualFile {
                    content: snapshot.content.clone(),
                    lineage: Some(path.to_string()),
                },
            );
        }
        self.initial.insert(path.to_string(), snapshot.clone());
        Ok(snapshot)
    }

    fn require_file(&mut self, path: &str, label: &str) -> Result<VirtualFile> {
        self.load_initial(path)?;
        self.state
            .get(path)
            .cloned()
            .ok_or_else(|| input_error(format!("{label} requires {path}, but it does not exist")))
    }

    fn push_operation(&mut self, operation: WorkspaceOperation) -> Result<()> {
        if self.operations.len() >= MAX_WORKSPACE_OPERATIONS {
            return Err(input_error("workspace edit contains too many operations"));
        }
        let bytes = match &operation {
            WorkspaceOperation::Write { content, .. }
            | WorkspaceOperation::Create { content, .. } => content.len(),
            WorkspaceOperation::Rename { .. } | WorkspaceOperation::Delete { .. } => 0,
        };
        self.content_bytes = self.content_bytes.saturating_add(bytes);
        if self.content_bytes > MAX_WORKSPACE_BYTES {
            return Err(input_error(
                "workspace edit content exceeds the combined semantic edit limit",
            ));
        }
        self.operations.push(operation);
        Ok(())
    }

    fn apply_text_document_edit(&mut self, uri: &Value, edits: &Value, label: &str) -> Result<()> {
        let path = workspace_path_from_uri(
            uri.as_str()
                .ok_or_else(|| input_error(format!("{label} is missing a file URI")))?,
            self.root,
        )?;
        let file = self.require_file(&path, label)?;
        let content = apply_text_edits(&file.content, edits, &format!("{label} for {path}"))?;
        if content == file.content {
            return Ok(());
        }
        if content.len() > MAX_SOURCE_BYTES {
            return Err(input_error(format!(
                "{path} exceeds the semantic edit file limit"
            )));
        }
        self.state.insert(
            path.clone(),
            VirtualFile {
                content: content.clone(),
                ..file
            },
        );
        self.push_operation(WorkspaceOperation::Write { path, content })
    }

    fn apply_resource_operation(&mut self, change: &Map<String, Value>, label: &str) -> Result<()> {
        match change.get("kind").and_then(Value::as_str) {
            Some("create") => {
                let path = workspace_path_from_uri(
                    required_object_string(change, "uri", label)?,
                    self.root,
                )?;
                self.load_initial(&path)?;
                if let Some(existing) = self.state.get(&path).cloned() {
                    if boolean_option(change.get("options"), "overwrite") {
                        if !existing.content.is_empty() {
                            self.state.insert(
                                path.clone(),
                                VirtualFile {
                                    content: String::new(),
                                    ..existing
                                },
                            );
                            self.push_operation(WorkspaceOperation::Write {
                                path,
                                content: String::new(),
                            })?;
                        }
                    } else if !boolean_option(change.get("options"), "ignoreIfExists") {
                        return Err(input_error(format!(
                            "cannot create {path} because it already exists"
                        )));
                    }
                } else {
                    self.state.insert(
                        path.clone(),
                        VirtualFile {
                            content: String::new(),
                            lineage: None,
                        },
                    );
                    self.push_operation(WorkspaceOperation::Create {
                        path,
                        content: String::new(),
                    })?;
                }
            }
            Some("rename") => {
                let path = workspace_path_from_uri(
                    required_object_string(change, "oldUri", label)?,
                    self.root,
                )?;
                let destination = workspace_path_from_uri(
                    required_object_string(change, "newUri", label)?,
                    self.root,
                )?;
                if path == destination {
                    return Ok(());
                }
                let source = self.require_file(&path, label)?;
                self.load_initial(&destination)?;
                if self.state.contains_key(&destination) {
                    if boolean_option(change.get("options"), "overwrite") {
                        self.state.remove(&destination);
                        self.push_operation(WorkspaceOperation::Delete {
                            path: destination.clone(),
                        })?;
                    } else if boolean_option(change.get("options"), "ignoreIfExists") {
                        return Ok(());
                    } else {
                        return Err(input_error(format!(
                            "cannot rename {path} to {destination} because the destination exists"
                        )));
                    }
                }
                self.state.remove(&path);
                self.state.insert(destination.clone(), source);
                self.push_operation(WorkspaceOperation::Rename { path, destination })?;
            }
            Some("delete") => {
                let path = workspace_path_from_uri(
                    required_object_string(change, "uri", label)?,
                    self.root,
                )?;
                if boolean_option(change.get("options"), "recursive") {
                    return Err(input_error(format!(
                        "recursive semantic deletion is unsupported: {path}"
                    )));
                }
                self.load_initial(&path)?;
                if self.state.remove(&path).is_some() {
                    self.push_operation(WorkspaceOperation::Delete { path })?;
                } else if !boolean_option(change.get("options"), "ignoreIfNotExists") {
                    return Err(input_error(format!(
                        "cannot delete {path} because it does not exist"
                    )));
                }
            }
            kind => {
                return Err(input_error(format!(
                    "unsupported workspace resource operation{}",
                    kind.map(|kind| format!(" ({kind})")).unwrap_or_default()
                )));
            }
        }
        Ok(())
    }

    fn finish(
        self,
    ) -> Result<(
        Vec<WorkspacePrecondition>,
        Vec<WorkspaceOperation>,
        Vec<ChangeSetFile>,
    )> {
        if self.operations.is_empty() {
            return Err(input_error("language server proposed no source changes"));
        }
        let mut touched = BTreeSet::new();
        for operation in &self.operations {
            touched.extend(operation.paths().into_iter().map(str::to_string));
        }
        let preconditions = self
            .initial
            .iter()
            .filter(|(path, _)| touched.contains(*path))
            .map(|(path, snapshot)| match snapshot {
                Some(snapshot) => WorkspacePrecondition::Existing {
                    path: path.clone(),
                    expected_digest: snapshot.digest.clone(),
                },
                None => WorkspacePrecondition::Missing { path: path.clone() },
            })
            .collect::<Vec<_>>();
        if preconditions.len() != touched.len()
            || preconditions
                .iter()
                .any(|precondition| !touched.contains(precondition.path()))
        {
            return Err(input_error(
                "semantic change set could not bind every touched path to a precondition",
            ));
        }
        let files = preview_files(&self.initial, &self.state);
        Ok((preconditions, self.operations, files))
    }
}

#[allow(clippy::too_many_arguments)]
fn build_workspace_edit_plan(
    entry: &CoderEntryContext,
    lease: &ExecutionLease,
    policy: &WorkPolicy,
    head_oid: &str,
    source_path: &str,
    source_line: u32,
    source_character: u32,
    new_name: &str,
    raw_result: &Value,
) -> Result<SemanticChangeSet> {
    let edit = raw_result
        .get("edit")
        .filter(|value| value.is_object())
        .unwrap_or(raw_result);
    let edit = edit
        .as_object()
        .ok_or_else(|| input_error("language server returned no workspace edit"))?;
    let mut builder = WorkspaceEditBuilder::new(&entry.worktree);
    if let Some(changes) = edit.get("changes") {
        let changes = changes
            .as_object()
            .ok_or_else(|| input_error("workspace edit changes must be a URI-to-edits map"))?;
        let mut uris = changes.keys().collect::<Vec<_>>();
        uris.sort();
        for uri in uris {
            builder.apply_text_document_edit(
                &Value::String(uri.clone()),
                &changes[uri],
                "Workspace edit",
            )?;
        }
    }
    if let Some(document_changes) = edit.get("documentChanges") {
        let document_changes = document_changes
            .as_array()
            .ok_or_else(|| input_error("workspace edit documentChanges must be an array"))?;
        for (index, raw_change) in document_changes.iter().enumerate() {
            let change = raw_change.as_object().ok_or_else(|| {
                input_error(format!("workspace operation {} is invalid", index + 1))
            })?;
            let label = format!("Workspace operation {}", index + 1);
            if let Some(text_document) = change.get("textDocument").and_then(Value::as_object) {
                let uri = text_document
                    .get("uri")
                    .ok_or_else(|| input_error(format!("{label} is missing a document URI")))?;
                let edits = change
                    .get("edits")
                    .ok_or_else(|| input_error(format!("{label} is missing edits")))?;
                builder.apply_text_document_edit(uri, edits, &label)?;
            } else {
                builder.apply_resource_operation(change, &label)?;
            }
        }
    }
    let (preconditions, operations, files) = builder.finish()?;
    validate_change_set_policy(policy, &files, &operations)?;
    let annotations = annotation_labels(edit);
    let id = stable_id(
        "changeset",
        &json!({
            "work_id": entry.work_id,
            "attempt_id": lease.attempt_id,
            "base_head_oid": head_oid,
            "source_path": source_path,
            "source_line": source_line,
            "source_character": source_character,
            "new_name": new_name,
            "preconditions": preconditions,
            "operations": operations,
        }),
    );
    Ok(SemanticChangeSet {
        id,
        work_id: entry.work_id.clone(),
        attempt_id: lease.attempt_id.to_string(),
        base_head_oid: head_oid.to_string(),
        source_path: source_path.to_string(),
        source_line,
        source_character,
        new_name: new_name.to_string(),
        preconditions,
        operations,
        files,
        annotation_labels: annotations,
    })
}

fn validate_change_set_policy(
    policy: &WorkPolicy,
    files: &[ChangeSetFile],
    operations: &[WorkspaceOperation],
) -> Result<()> {
    let mut changed = files
        .iter()
        .map(|file| ChangedFile {
            path: file.path.clone(),
            status: match file.status.as_str() {
                "created" => ChangeStatus::Added,
                "deleted" => ChangeStatus::Deleted,
                "renamed" => ChangeStatus::Renamed,
                _ => ChangeStatus::Modified,
            },
            old_path: file.old_path.clone(),
            is_binary: false,
            byte_size: Some(file.after_bytes as u64),
        })
        .collect::<Vec<_>>();
    let projected_paths = changed
        .iter()
        .flat_map(|file| std::iter::once(file.path.as_str()).chain(file.old_path.as_deref()))
        .map(str::to_string)
        .collect::<HashSet<_>>();
    for operation in operations {
        for path in operation.paths() {
            if !projected_paths.contains(path) {
                changed.push(ChangedFile {
                    path: path.to_string(),
                    status: ChangeStatus::Modified,
                    old_path: None,
                    is_binary: false,
                    byte_size: None,
                });
            }
        }
    }
    let violations = medousa_forge::policy::evaluate_paths(policy, &changed)
        .map_err(|error| input_error(format!("invalid Forge path policy: {error}")))?;
    if let Some(violation) = violations.first() {
        return Err(input_error(format!(
            "semantic change set denied by Forge policy: {} ({})",
            violation.path, violation.rule
        )));
    }
    Ok(())
}

fn preview_files(
    initial: &BTreeMap<String, Option<SourceSnapshot>>,
    state: &BTreeMap<String, VirtualFile>,
) -> Vec<ChangeSetFile> {
    let mut files = Vec::new();
    let final_by_lineage = state
        .iter()
        .filter_map(|(path, file)| {
            file.lineage
                .as_ref()
                .map(|lineage| (lineage.clone(), (path, file)))
        })
        .collect::<BTreeMap<_, _>>();
    for (initial_path, source) in initial {
        let Some(source) = source else { continue };
        match final_by_lineage.get(initial_path) {
            None => files.push(ChangeSetFile {
                path: initial_path.clone(),
                old_path: None,
                status: "deleted".into(),
                before_digest: Some(source.digest.clone()),
                after_digest: None,
                before_bytes: source.content.len(),
                after_bytes: 0,
            }),
            Some((path, file)) if path.as_str() != initial_path => {
                files.push(ChangeSetFile {
                    path: (*path).clone(),
                    old_path: Some(initial_path.clone()),
                    status: "renamed".into(),
                    before_digest: Some(source.digest.clone()),
                    after_digest: Some(source_digest(file.content.as_bytes())),
                    before_bytes: source.content.len(),
                    after_bytes: file.content.len(),
                });
            }
            Some((_, file)) if file.content != source.content => {
                files.push(ChangeSetFile {
                    path: initial_path.clone(),
                    old_path: None,
                    status: "modified".into(),
                    before_digest: Some(source.digest.clone()),
                    after_digest: Some(source_digest(file.content.as_bytes())),
                    before_bytes: source.content.len(),
                    after_bytes: file.content.len(),
                });
            }
            Some(_) => {}
        }
    }
    for (path, file) in state {
        if file.lineage.is_none() {
            files.push(ChangeSetFile {
                path: path.clone(),
                old_path: None,
                status: "created".into(),
                before_digest: None,
                after_digest: Some(source_digest(file.content.as_bytes())),
                before_bytes: 0,
                after_bytes: file.content.len(),
            });
        }
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    files
}

#[derive(Debug, Clone)]
struct TextEdit {
    from: usize,
    to: usize,
    text: String,
    index: usize,
}

fn apply_text_edits(content: &str, raw_edits: &Value, label: &str) -> Result<String> {
    let raw_edits = raw_edits
        .as_array()
        .ok_or_else(|| input_error(format!("{label} must contain an edits array")))?;
    if raw_edits.len() > MAX_TEXT_EDITS_PER_DOCUMENT {
        return Err(input_error(format!("{label} contains too many text edits")));
    }
    let mut edits = Vec::with_capacity(raw_edits.len());
    for (index, raw_edit) in raw_edits.iter().enumerate() {
        let edit = raw_edit
            .as_object()
            .ok_or_else(|| input_error(format!("{label} edit {} is invalid", index + 1)))?;
        let range = edit
            .get("range")
            .and_then(Value::as_object)
            .ok_or_else(|| input_error(format!("{label} edit {} has no range", index + 1)))?;
        let text = edit
            .get("newText")
            .and_then(Value::as_str)
            .ok_or_else(|| input_error(format!("{label} edit {} has no replacement", index + 1)))?
            .to_string();
        let from = text_position_offset(
            content,
            range.get("start"),
            &format!("{label} edit {} start", index + 1),
        )?;
        let to = text_position_offset(
            content,
            range.get("end"),
            &format!("{label} edit {} end", index + 1),
        )?;
        if to < from {
            return Err(input_error(format!(
                "{label} edit {} has a reversed range",
                index + 1
            )));
        }
        edits.push(TextEdit {
            from,
            to,
            text,
            index,
        });
    }
    let mut ascending = edits.clone();
    ascending.sort_by_key(|edit| (edit.from, edit.to, edit.index));
    for pair in ascending.windows(2) {
        let previous = &pair[0];
        let current = &pair[1];
        let same_insertion = previous.from == previous.to
            && current.from == current.to
            && previous.from == current.from;
        if !same_insertion && (current.from < previous.to || current.from == previous.from) {
            return Err(input_error(format!("{label} contains overlapping edits")));
        }
    }
    edits.sort_by(|left, right| {
        right
            .from
            .cmp(&left.from)
            .then_with(|| right.index.cmp(&left.index))
    });
    let mut next = content.to_string();
    for edit in edits {
        next.replace_range(edit.from..edit.to, &edit.text);
    }
    Ok(next)
}

fn text_position_offset(content: &str, raw: Option<&Value>, label: &str) -> Result<usize> {
    let position = raw
        .and_then(Value::as_object)
        .ok_or_else(|| input_error(format!("{label} is missing")))?;
    let line = position
        .get("line")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| input_error(format!("{label}.line must be a non-negative integer")))?;
    let character = position
        .get("character")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| input_error(format!("{label}.character must be a non-negative integer")))?;
    let starts = std::iter::once(0)
        .chain(
            content
                .bytes()
                .enumerate()
                .filter_map(|(index, byte)| (byte == b'\n').then_some(index + 1)),
        )
        .collect::<Vec<_>>();
    let start = *starts
        .get(line)
        .ok_or_else(|| input_error(format!("{label}.line is outside the document")))?;
    let mut end = starts
        .get(line + 1)
        .copied()
        .map(|offset| offset.saturating_sub(1))
        .unwrap_or(content.len());
    if end > start && content.as_bytes()[end - 1] == b'\r' {
        end -= 1;
    }
    let line_text = &content[start..end];
    let mut utf16 = 0usize;
    for (byte, value) in line_text.char_indices() {
        if utf16 == character {
            return Ok(start + byte);
        }
        let width = value.len_utf16();
        if character < utf16 + width {
            return Err(input_error(format!(
                "{label}.character splits a UTF-16 surrogate pair"
            )));
        }
        utf16 += width;
    }
    if utf16 == character {
        Ok(end)
    } else {
        Err(input_error(format!(
            "{label}.character is outside line {line}"
        )))
    }
}

fn project_references(
    entry: &CoderEntryContext,
    source_path: &str,
    line: u32,
    character: u32,
    head_oid: &str,
    result: &Value,
) -> Result<Value> {
    let raw = result
        .as_array()
        .ok_or_else(|| input_error("language server returned an invalid reference list"))?;
    let mut references = Vec::new();
    for location in raw.iter().take(MAX_REFERENCE_RESULTS) {
        let Some(location) = location.as_object() else {
            continue;
        };
        let uri = location
            .get("uri")
            .or_else(|| location.get("targetUri"))
            .and_then(Value::as_str);
        let range = location
            .get("range")
            .or_else(|| location.get("targetSelectionRange"))
            .or_else(|| location.get("targetRange"));
        let (Some(uri), Some(range)) = (uri, range) else {
            continue;
        };
        let path = workspace_path_from_uri(uri, &entry.worktree)?;
        let range = bounded_range(range)?;
        let id = stable_id(
            "reference",
            &json!({
                "work_id": entry.work_id,
                "head_oid": head_oid,
                "path": path,
                "range": range,
            }),
        );
        references.push(json!({ "id": id, "path": path, "range": range }));
    }
    let symbol_action_id = stable_id(
        "symbol-action",
        &json!({
            "work_id": entry.work_id,
            "head_oid": head_oid,
            "path": source_path,
            "line": line,
            "character": character,
            "action": "references",
        }),
    );
    Ok(json!({
        "ok": true,
        "action": {
            "id": symbol_action_id,
            "kind": "symbol_references",
            "head_oid": head_oid,
            "source": { "path": source_path, "line": line, "character": character },
            "reference_count": references.len(),
            "references_truncated": raw.len() > MAX_REFERENCE_RESULTS,
            "references": references,
        }
    }))
}

fn bounded_range(value: &Value) -> Result<Value> {
    let range = value
        .as_object()
        .ok_or_else(|| input_error("language-server location has no range"))?;
    let position = |name: &str| -> Result<Value> {
        let value = range
            .get(name)
            .and_then(Value::as_object)
            .ok_or_else(|| input_error("language-server location has an invalid range"))?;
        let line = value
            .get("line")
            .and_then(Value::as_u64)
            .ok_or_else(|| input_error("language-server range line is invalid"))?;
        let character = value
            .get("character")
            .and_then(Value::as_u64)
            .ok_or_else(|| input_error("language-server range character is invalid"))?;
        Ok(json!({ "line": line, "character": character }))
    };
    Ok(json!({ "start": position("start")?, "end": position("end")? }))
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct DiscoveredTest {
    id: String,
    label: String,
    path: String,
    line: u32,
    task_id: String,
}

#[derive(Debug, Clone, Serialize)]
struct RankedTest {
    id: String,
    score: u32,
    reasons: Vec<String>,
    test: DiscoveredTest,
}

fn rank_tests(
    tests: &[DiscoveredTest],
    paths: &[String],
    symbol: Option<&str>,
    limit: usize,
) -> Vec<RankedTest> {
    let symbol = symbol.map(str::to_ascii_lowercase);
    let mut ranked = tests
        .iter()
        .take(MAX_TEST_CANDIDATES)
        .map(|test| {
            let test_path = test.path.to_ascii_lowercase();
            let test_label = test.label.to_ascii_lowercase();
            let test_stem = file_stem(&test_path);
            let mut score = 1u32;
            let mut reasons = Vec::new();
            for path in paths {
                let path = path.to_ascii_lowercase();
                let stem = file_stem(&path);
                if path == test_path {
                    score = score.max(120);
                    reasons.push(format!("test is declared in changed path {path}"));
                } else {
                    let common = common_directory_prefix(&path, &test_path);
                    if common > 0 {
                        score = score.max(20 + (common as u32 * 8).min(48));
                        reasons.push(format!("shares {common} path segment(s) with {path}"));
                    }
                    if !stem.is_empty()
                        && (test_stem.contains(stem.as_str())
                            || test_path.contains(&format!("/{stem}_test"))
                            || test_path.contains(&format!("/{stem}.test"))
                            || test_path.contains(&format!("/{stem}.spec")))
                    {
                        score = score.max(90);
                        reasons.push(format!("test path matches changed module {stem}"));
                    }
                }
            }
            if let Some(symbol) = symbol.as_deref()
                && !symbol.is_empty()
                && (test_label.contains(symbol) || test_path.contains(symbol))
            {
                score = score.saturating_add(55);
                reasons.push(format!("test name or path matches symbol {symbol}"));
            }
            reasons.sort();
            reasons.dedup();
            reasons.truncate(4);
            RankedTest {
                id: stable_id(
                    "test-target",
                    &json!({ "task_id": test.task_id, "test_id": test.id }),
                ),
                score,
                reasons,
                test: test.clone(),
            }
        })
        .filter(|candidate| candidate.score > 1)
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.test.path.cmp(&right.test.path))
            .then_with(|| left.test.id.cmp(&right.test.id))
    });
    ranked.truncate(limit);
    ranked
}

fn common_directory_prefix(left: &str, right: &str) -> usize {
    let left = left.split('/').collect::<Vec<_>>();
    let right = right.split('/').collect::<Vec<_>>();
    left.iter()
        .take(left.len().saturating_sub(1))
        .zip(right.iter().take(right.len().saturating_sub(1)))
        .take_while(|(left, right)| left == right)
        .count()
}

fn file_stem(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
        .trim_end_matches("_test")
        .trim_end_matches(".test")
        .trim_end_matches(".spec")
        .to_string()
}

fn annotation_labels(edit: &Map<String, Value>) -> Vec<String> {
    let annotations = edit.get("changeAnnotations").and_then(Value::as_object);
    let mut ids = Vec::new();
    collect_annotation_ids(&Value::Object(edit.clone()), 0, &mut ids);
    let mut seen = HashSet::new();
    ids.into_iter()
        .filter(|id| seen.insert(id.clone()))
        .take(32)
        .map(|id| {
            let annotation = annotations.and_then(|annotations| annotations.get(&id));
            annotation
                .and_then(Value::as_object)
                .and_then(|annotation| {
                    annotation
                        .get("label")
                        .or_else(|| annotation.get("description"))
                        .and_then(Value::as_str)
                })
                .map(|label| bounded(label.trim(), 200))
                .filter(|label| !label.is_empty())
                .unwrap_or(id)
        })
        .collect()
}

fn collect_annotation_ids(value: &Value, depth: usize, ids: &mut Vec<String>) {
    if depth > 24 || ids.len() >= 64 {
        return;
    }
    match value {
        Value::Array(values) => {
            for value in values {
                collect_annotation_ids(value, depth + 1, ids);
            }
        }
        Value::Object(object) => {
            if let Some(id) = object.get("annotationId").and_then(Value::as_str) {
                ids.push(bounded(id, 200));
            }
            for (key, value) in object {
                if key != "changeAnnotations" {
                    collect_annotation_ids(value, depth + 1, ids);
                }
            }
        }
        _ => {}
    }
}

fn workspace_path_from_uri(uri: &str, root: &Path) -> Result<String> {
    let lower = uri.to_ascii_lowercase();
    if lower.contains("%2f") || lower.contains("%5c") {
        return Err(input_error(
            "workspace edit URI contains an encoded path separator",
        ));
    }
    let url = reqwest::Url::parse(uri)
        .map_err(|error| input_error(format!("invalid workspace edit URI: {error}")))?;
    if url.scheme() != "file" {
        return Err(input_error("workspace edit URI must use the file scheme"));
    }
    let path = url
        .to_file_path()
        .map_err(|_| input_error("workspace edit URI is not a valid workshop path"))?;
    path_relative_to_root(root, &path)
}

fn entry_relative_path(entry: &CoderEntryContext, raw: &str) -> Result<String> {
    let path = Path::new(raw.trim());
    if path.is_absolute() {
        path_relative_to_root(&entry.worktree, path)
    } else {
        normalize_relative_path(raw)
    }
}

fn path_relative_to_root(root: &Path, path: &Path) -> Result<String> {
    let root = root
        .canonicalize()
        .map_err(|error| input_error(format!("governed worktree is unavailable: {error}")))?;
    let resolved = if path.exists() {
        path.canonicalize()
            .map_err(|error| input_error(format!("cannot resolve workspace path: {error}")))?
    } else {
        let parent = path
            .parent()
            .ok_or_else(|| input_error("workspace path has no parent"))?
            .canonicalize()
            .map_err(|error| input_error(format!("cannot resolve workspace parent: {error}")))?;
        parent.join(
            path.file_name()
                .ok_or_else(|| input_error("workspace path has no file name"))?,
        )
    };
    if resolved == root || !resolved.starts_with(&root) {
        return Err(input_error(
            "semantic action path escapes the governed Coder worktree",
        ));
    }
    normalize_relative_path(
        &resolved
            .strip_prefix(root)
            .map_err(|_| input_error("cannot relativize semantic action path"))?
            .to_string_lossy(),
    )
}

fn resolve_existing_path(root: &Path, relative: &str) -> Result<PathBuf> {
    let root = root
        .canonicalize()
        .map_err(|error| input_error(format!("governed worktree is unavailable: {error}")))?;
    let path = root
        .join(relative)
        .canonicalize()
        .map_err(|error| input_error(format!("semantic action source does not exist: {error}")))?;
    if !path.starts_with(&root) || !path.is_file() {
        return Err(input_error(
            "semantic action source must be a file inside the governed worktree",
        ));
    }
    Ok(path)
}

fn resolve_maybe_new_path(root: &Path, relative: &str) -> Result<PathBuf> {
    let relative = normalize_relative_path(relative)?;
    let root = root
        .canonicalize()
        .map_err(|error| input_error(format!("governed worktree is unavailable: {error}")))?;
    let candidate = root.join(&relative);
    let resolved = if candidate.exists() {
        candidate
            .canonicalize()
            .map_err(|error| input_error(format!("cannot resolve {relative}: {error}")))?
    } else {
        let parent = candidate
            .parent()
            .ok_or_else(|| input_error("semantic edit path has no parent"))?
            .canonicalize()
            .map_err(|error| {
                input_error(format!("cannot resolve parent for {relative}: {error}"))
            })?;
        parent.join(
            candidate
                .file_name()
                .ok_or_else(|| input_error("semantic edit path has no file name"))?,
        )
    };
    if !resolved.starts_with(&root) {
        return Err(input_error(
            "semantic edit path escapes the governed Coder worktree",
        ));
    }
    Ok(resolved)
}

pub(crate) fn normalize_relative_path(raw: &str) -> Result<String> {
    let normalized = raw.trim().replace('\\', "/");
    let path = Path::new(&normalized);
    if normalized.is_empty() || path.is_absolute() {
        return Err(input_error(
            "semantic action path must be relative to the governed worktree",
        ));
    }
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
        || path
            .components()
            .next()
            .is_some_and(|component| component.as_os_str() == ".git")
    {
        return Err(input_error(
            "semantic action path cannot traverse or edit repository metadata",
        ));
    }
    Ok(path.to_string_lossy().replace('\\', "/"))
}

fn required_string<'a>(input: &'a Value, field: &str) -> Result<&'a str> {
    input
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| input_error(format!("{field} is required")))
}

fn required_object_string<'a>(
    object: &'a Map<String, Value>,
    field: &str,
    label: &str,
) -> Result<&'a str> {
    object
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| input_error(format!("{label} is missing {field}")))
}

fn optional_bounded_string(input: &Value, field: &str, max: usize) -> Result<Option<String>> {
    let Some(value) = input.get(field) else {
        return Ok(None);
    };
    let value = value
        .as_str()
        .ok_or_else(|| input_error(format!("{field} must be a string")))?
        .trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.chars().count() > max || value.chars().any(char::is_control) {
        return Err(input_error(format!("{field} is invalid or too long")));
    }
    Ok(Some(value.to_string()))
}

fn required_u32(input: &Value, field: &str) -> Result<u32> {
    input
        .get(field)
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| input_error(format!("{field} must be a non-negative integer")))
}

fn validate_new_name(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty()
        || value.chars().count() > 256
        || value.chars().any(char::is_control)
        || value.contains(['/', '\\'])
    {
        return Err(input_error("new_name is invalid or too long"));
    }
    Ok(value.to_string())
}

fn boolean_option(value: Option<&Value>, key: &str) -> bool {
    value
        .and_then(Value::as_object)
        .and_then(|options| options.get(key))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn source_digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn stable_id(kind: &str, value: &Value) -> String {
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    format!("{kind}:sha256:{:x}", Sha256::digest(bytes))
}

fn bounded(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        value.to_string()
    } else {
        value.chars().take(max_chars).collect::<String>() + "…"
    }
}

fn input_error(message: impl Into<String>) -> StasisError {
    StasisError::PortFailure(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use medousa_forge::model::{AttemptId, LeaseId, WorkId};
    use tempfile::TempDir;

    fn lease() -> ExecutionLease {
        ExecutionLease {
            lease_id: LeaseId::new(),
            generation: 3,
            work_id: WorkId::from("work-1".to_string()),
            attempt_id: AttemptId::from("attempt-1".to_string()),
            owner_instance_id: "instance".into(),
            acquired_at: chrono::Utc::now(),
            heartbeat_at: chrono::Utc::now(),
            pid: None,
            process_start_marker: None,
        }
    }

    fn entry(root: &Path) -> CoderEntryContext {
        CoderEntryContext {
            repo_id: "repo-1".into(),
            work_id: "work-1".into(),
            title: "Rename".into(),
            brief: "Rename symbol".into(),
            worktree: root.to_path_buf(),
            branch: "coder/test".into(),
            environment_generation: 1,
            memory_parent: None,
            baseline_oid: "base".into(),
            head_oid: "head".into(),
            changed_paths: Vec::new(),
            allowed_paths: Vec::new(),
            denied_paths: Vec::new(),
            project_markers: Vec::new(),
            repository_instructions: Vec::new(),
            editor: Default::default(),
        }
    }

    #[test]
    fn text_edits_use_utf16_offsets_and_reject_split_surrogates() {
        let content = "let face = \"😀\";\n";
        let edited = apply_text_edits(
            content,
            &json!([{
                "range": {
                    "start": { "line": 0, "character": 12 },
                    "end": { "line": 0, "character": 14 }
                },
                "newText": "🙂"
            }]),
            "rename",
        )
        .expect("UTF-16 edit");
        assert_eq!(edited, "let face = \"🙂\";\n");
        assert!(
            apply_text_edits(
                content,
                &json!([{
                    "range": {
                        "start": { "line": 0, "character": 13 },
                        "end": { "line": 0, "character": 14 }
                    },
                    "newText": "x"
                }]),
                "rename",
            )
            .is_err()
        );
    }

    #[test]
    fn rename_preview_is_stable_complete_and_contains_no_source() {
        let root = TempDir::new().expect("root");
        std::fs::create_dir(root.path().join("src")).expect("src");
        std::fs::write(root.path().join("src/a.rs"), "fn old() {}\n").expect("a");
        std::fs::write(root.path().join("src/b.rs"), "fn call() { old(); }\n").expect("b");
        let entry = entry(root.path());
        let uri_a = reqwest::Url::from_file_path(root.path().join("src/a.rs"))
            .unwrap()
            .to_string();
        let uri_b = reqwest::Url::from_file_path(root.path().join("src/b.rs"))
            .unwrap()
            .to_string();
        let edit = json!({
            "changes": {
                uri_a: [{
                    "range": { "start": {"line": 0, "character": 3}, "end": {"line": 0, "character": 6} },
                    "newText": "fresh"
                }],
                uri_b: [{
                    "range": { "start": {"line": 0, "character": 12}, "end": {"line": 0, "character": 15} },
                    "newText": "fresh"
                }]
            }
        });
        let first = build_workspace_edit_plan(
            &entry,
            &lease(),
            &WorkPolicy::default(),
            "head-1",
            "src/a.rs",
            0,
            4,
            "fresh",
            &edit,
        )
        .expect("plan");
        let second = build_workspace_edit_plan(
            &entry,
            &lease(),
            &WorkPolicy::default(),
            "head-1",
            "src/a.rs",
            0,
            4,
            "fresh",
            &edit,
        )
        .expect("second plan");
        assert_eq!(first.id, second.id);
        assert_eq!(first.preconditions.len(), 2);
        assert_eq!(first.operations.len(), 2);
        let projection = first.projection(ChangeSetState::Previewed).to_string();
        assert!(!projection.contains("fn old"));
        assert!(!projection.contains("fn call"));
        assert!(projection.contains("src/a.rs"));
    }

    #[test]
    fn change_set_store_forbids_duplicate_or_uncertain_replay() {
        let root = TempDir::new().expect("root");
        std::fs::write(root.path().join("a.rs"), "fn old() {}\n").expect("a");
        let entry = entry(root.path());
        let uri = reqwest::Url::from_file_path(root.path().join("a.rs"))
            .unwrap()
            .to_string();
        let plan = build_workspace_edit_plan(
            &entry,
            &lease(),
            &WorkPolicy::default(),
            "head-1",
            "a.rs",
            0,
            4,
            "fresh",
            &json!({
                "changes": {
                    uri: [{
                        "range": { "start": {"line": 0, "character": 3}, "end": {"line": 0, "character": 6} },
                        "newText": "fresh"
                    }]
                }
            }),
        )
        .expect("plan");
        let id = plan.id.clone();
        let mut store = CoderChangeSetStore::default();
        store.insert(plan).expect("store plan");
        store.begin_apply(&id, &lease()).expect("first apply");
        store.finish_apply(&id, false);
        assert!(store.begin_apply(&id, &lease()).is_err());
    }

    #[test]
    fn structured_change_set_preserves_resource_operation_order() {
        let root = TempDir::new().expect("root");
        std::fs::create_dir(root.path().join("src")).expect("src");
        let entry = entry(root.path());
        let temporary = reqwest::Url::from_file_path(root.path().join("src/temporary.rs"))
            .unwrap()
            .to_string();
        let destination = reqwest::Url::from_file_path(root.path().join("src/final.rs"))
            .unwrap()
            .to_string();
        let plan = build_workspace_edit_plan(
            &entry,
            &lease(),
            &WorkPolicy::default(),
            "head-1",
            "src/lib.rs",
            0,
            0,
            "fresh",
            &json!({
                "documentChanges": [
                    { "kind": "create", "uri": temporary },
                    {
                        "textDocument": { "uri": temporary, "version": null },
                        "edits": [{
                            "range": {
                                "start": { "line": 0, "character": 0 },
                                "end": { "line": 0, "character": 0 }
                            },
                            "newText": "pub fn fresh() {}\n"
                        }]
                    },
                    { "kind": "rename", "oldUri": temporary, "newUri": destination }
                ]
            }),
        )
        .expect("structured plan");
        assert!(matches!(
            plan.operations[0],
            WorkspaceOperation::Create { .. }
        ));
        assert!(matches!(
            plan.operations[1],
            WorkspaceOperation::Write { .. }
        ));
        assert!(matches!(
            plan.operations[2],
            WorkspaceOperation::Rename { .. }
        ));
        assert_eq!(plan.preconditions.len(), 2);
        assert_eq!(plan.files.len(), 1);
        assert_eq!(plan.files[0].status, "created");
        assert_eq!(plan.files[0].path, "src/final.rs");
    }

    #[test]
    fn affected_tests_rank_module_and_symbol_matches() {
        let tests = vec![
            DiscoveredTest {
                id: "tests/parser_test.rs::parses_value".into(),
                label: "parses_value".into(),
                path: "tests/parser_test.rs".into(),
                line: 8,
                task_id: "cargo-test".into(),
            },
            DiscoveredTest {
                id: "tests/network_test.rs::connects".into(),
                label: "connects".into(),
                path: "tests/network_test.rs".into(),
                line: 4,
                task_id: "cargo-test".into(),
            },
        ];
        let ranked = rank_tests(&tests, &["src/parser.rs".into()], Some("parses"), 2);
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].test.path, "tests/parser_test.rs");
        assert!(ranked[0].score > 1);
    }
}
