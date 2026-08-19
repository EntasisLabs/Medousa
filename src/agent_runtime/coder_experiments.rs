//! Governed comparison of sealed Coder experiment candidates.
//!
//! Candidate code is read only through immutable Forge evidence and exact Git
//! object ids. Semantic notebook state is read from the candidate's
//! runtime-derived Locus scope and cut off at the evidence seal timestamp.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Duration;

use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use genai::chat::Tool;
use medousa_forge::forge::Forge;
use medousa_forge::model::{
    AttemptId, AttemptState, ChangeStatus, ChangedFile, EvidenceManifest, GitOid, WorkId,
};
use serde_json::{Value, json};
use stasis::application::orchestration::tool_registry::ToolRegistry;
use stasis::prelude::{Result, StasisError};

use super::coder_memory::CoderMemoryScope;
use super::coder_mode::CoderEntryContext;

pub const COGNITION_CODER_EXPERIMENT_COMPARE: &str = "cognition_coder_experiment_compare";

const MAX_CANDIDATES: usize = 4;
const MAX_CHANGED_PATHS: usize = 80;
const MAX_NOTEBOOK_NODES: usize = 10;
const NOTEBOOK_QUERY_LIMIT: usize = 32;
const MAX_PATH_CHARS: usize = 512;
const MEMORY_TIMEOUT: Duration = Duration::from_secs(2);
const NOTEBOOK_KINDS: &[&str] = &[
    "experiment",
    "acceptance_criterion",
    "next_action",
    "decision",
    "verification",
    "open_gap",
];

#[derive(Clone)]
struct SealedCandidate {
    attempt_id: AttemptId,
    attempt_seq: u32,
    executor_kind: String,
    branch: String,
    environment_generation: u32,
    repo_id: String,
    manifest: EvidenceManifest,
}

impl SealedCandidate {
    fn memory_scope(&self, work_id: &str) -> CoderMemoryScope {
        CoderMemoryScope::for_environment(
            &self.repo_id,
            work_id,
            &self.branch,
            self.environment_generation,
        )
    }
}

pub fn tool_definition() -> Tool {
    Tool::new(COGNITION_CODER_EXPERIMENT_COMPARE)
        .with_description(
            "Compare 2–4 sealed experiment candidates using immutable Forge evidence, exact Git trees, and each candidate's temporally pinned engineering notebook. Live sibling worktrees are never read.",
        )
        .with_schema(json!({
            "type": "object",
            "properties": {
                "attempt_ids": {
                    "type": "array",
                    "description": "Optional exact sealed attempt ids. Omit to compare the latest sealed candidates.",
                    "items": { "type": "string", "maxLength": 160 },
                    "minItems": 2,
                    "maxItems": MAX_CANDIDATES,
                    "uniqueItems": true
                }
            }
        }))
}

pub fn sealed_candidate_count(forge: &Forge, work_id: &str) -> Result<usize> {
    let item = forge
        .load(&WorkId::from(work_id.to_string()))
        .map_err(|error| input_error(format!("cannot inspect sealed candidates: {error}")))?;
    Ok(item
        .attempts
        .iter()
        .filter(|attempt| attempt.state == AttemptState::Completed && attempt.evidence_id.is_some())
        .count())
}

pub async fn compare_sealed_candidates(
    forge: &Forge,
    registry: &dyn ToolRegistry,
    entry: &CoderEntryContext,
    input: &Value,
) -> Result<Value> {
    let requested = parse_attempt_ids(input)?;
    let candidates = select_candidates(forge, entry, requested.as_deref())?;

    let notebook_reads = futures_util::stream::iter(candidates.iter().cloned().map(|candidate| {
        let work_id = entry.work_id.clone();
        async move {
            let attempt_id = candidate.attempt_id.to_string();
            let notebook = read_candidate_notebook(registry, &work_id, &candidate).await;
            (attempt_id, notebook)
        }
    }))
    .buffer_unordered(MAX_CANDIDATES)
    .collect::<Vec<_>>()
    .await
    .into_iter()
    .collect::<HashMap<_, _>>();

    let candidate_projections = candidates
        .iter()
        .map(|candidate| {
            let notebook = notebook_reads
                .get(candidate.attempt_id.as_str())
                .cloned()
                .unwrap_or_else(unavailable_notebook);
            project_candidate(candidate, notebook)
        })
        .collect::<Vec<_>>();
    let pairwise = compare_candidate_pairs(forge, &entry.worktree, &candidates);

    Ok(json!({
        "ok": true,
        "work_id": entry.work_id,
        "comparison_basis": "sealed_forge_evidence_exact_git_trees_and_temporally_pinned_environment_memory",
        "candidate_count": candidate_projections.len(),
        "candidates": candidate_projections,
        "pair_count": pairwise.len(),
        "pairwise": pairwise,
        "safety": {
            "sealed_candidates_only": true,
            "live_sibling_worktrees_read": false,
            "raw_patches_included": false,
            "raw_source_included": false,
            "candidate_limit": MAX_CANDIDATES,
        }
    }))
}

fn parse_attempt_ids(input: &Value) -> Result<Option<Vec<AttemptId>>> {
    let Some(value) = input.get("attempt_ids") else {
        return Ok(None);
    };
    let values = value
        .as_array()
        .ok_or_else(|| input_error("attempt_ids must be an array"))?;
    if !(2..=MAX_CANDIDATES).contains(&values.len()) {
        return Err(input_error(format!(
            "attempt_ids must contain 2–{MAX_CANDIDATES} sealed attempts"
        )));
    }
    let mut seen = HashSet::new();
    let mut ids = Vec::with_capacity(values.len());
    for value in values {
        let id = value
            .as_str()
            .map(str::trim)
            .filter(|id| !id.is_empty() && id.chars().count() <= 160)
            .ok_or_else(|| input_error("attempt_ids entries must be non-empty strings"))?;
        if !seen.insert(id.to_string()) {
            return Err(input_error("attempt_ids must be unique"));
        }
        ids.push(AttemptId::from(id.to_string()));
    }
    Ok(Some(ids))
}

fn select_candidates(
    forge: &Forge,
    entry: &CoderEntryContext,
    requested: Option<&[AttemptId]>,
) -> Result<Vec<SealedCandidate>> {
    let work_id = WorkId::from(entry.work_id.clone());
    let item = forge
        .load(&work_id)
        .map_err(|error| input_error(format!("cannot load Coder undertaking: {error}")))?;
    let attempts = if let Some(requested) = requested {
        requested
            .iter()
            .map(|attempt_id| {
                item.attempt(attempt_id).ok_or_else(|| {
                    input_error(format!(
                        "attempt {attempt_id} does not belong to this undertaking"
                    ))
                })
            })
            .collect::<Result<Vec<_>>>()?
    } else {
        let mut attempts = item
            .attempts
            .iter()
            .filter(|attempt| {
                attempt.state == AttemptState::Completed && attempt.evidence_id.is_some()
            })
            .collect::<Vec<_>>();
        attempts.sort_by_key(|attempt| std::cmp::Reverse(attempt.seq));
        attempts.truncate(MAX_CANDIDATES);
        attempts
    };
    if attempts.len() < 2 {
        return Err(input_error(
            "at least two sealed candidates are required for comparison",
        ));
    }

    attempts
        .into_iter()
        .map(|attempt| {
            if attempt.state != AttemptState::Completed || attempt.evidence_id.is_none() {
                return Err(input_error(format!(
                    "attempt {} is not a sealed candidate",
                    attempt.id
                )));
            }
            let environment = item.environment_for_attempt(&attempt.id).ok_or_else(|| {
                input_error(format!(
                    "sealed attempt {} has no governed environment",
                    attempt.id
                ))
            })?;
            if environment.repo.repo_id.as_str() != entry.repo_id {
                return Err(input_error(format!(
                    "sealed attempt {} belongs to a different repository",
                    attempt.id
                )));
            }
            let manifest = forge
                .evidence_manifest_for_attempt(&work_id, &attempt.id)
                .map_err(|error| {
                    input_error(format!(
                        "cannot load sealed evidence for attempt {}: {error}",
                        attempt.id
                    ))
                })?;
            Ok(SealedCandidate {
                attempt_id: attempt.id.clone(),
                attempt_seq: attempt.seq,
                executor_kind: truncate(&attempt.executor.kind, 120),
                branch: environment.branch.clone(),
                environment_generation: environment.generation,
                repo_id: environment.repo.repo_id.to_string(),
                manifest,
            })
        })
        .collect()
}

async fn read_candidate_notebook(
    registry: &dyn ToolRegistry,
    work_id: &str,
    candidate: &SealedCandidate,
) -> Value {
    let scope = candidate.memory_scope(work_id);
    let result = tokio::time::timeout(
        MEMORY_TIMEOUT,
        registry.invoke_tool(
            crate::public_api::COGNITION_MEMORY_QUERY,
            json!({
                "action": "memory.list",
                "session_id": scope.session_id,
                "limit": NOTEBOOK_QUERY_LIMIT,
            }),
        ),
    )
    .await;
    let Ok(Ok(result)) = result else {
        return unavailable_notebook();
    };
    let projected = super::coder_memory::project_recall(
        &scope,
        candidate.manifest.sealed_head_oid.as_str(),
        &result,
        false,
        NOTEBOOK_QUERY_LIMIT,
    );
    let nodes = projected
        .get("nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|node| {
            node.get("kind")
                .and_then(Value::as_str)
                .is_some_and(|kind| NOTEBOOK_KINDS.contains(&kind))
                && node_at_or_before(node, candidate.manifest.sealed_at)
        })
        .take(MAX_NOTEBOOK_NODES)
        .cloned()
        .collect::<Vec<_>>();
    json!({
        "memory_status": "available",
        "cutoff": candidate.manifest.sealed_at,
        "node_count": nodes.len(),
        "nodes": nodes,
    })
}

fn node_at_or_before(node: &Value, cutoff: DateTime<Utc>) -> bool {
    node.get("timestamp")
        .and_then(Value::as_str)
        .and_then(|timestamp| DateTime::parse_from_rfc3339(timestamp).ok())
        .is_some_and(|timestamp| timestamp.with_timezone(&Utc) <= cutoff)
}

fn unavailable_notebook() -> Value {
    json!({
        "memory_status": "unavailable",
        "node_count": 0,
        "nodes": [],
    })
}

fn project_candidate(candidate: &SealedCandidate, notebook: Value) -> Value {
    let mut status_counts = BTreeMap::<String, usize>::new();
    let mut known_bytes = 0u64;
    let mut binary_file_count = 0usize;
    for file in &candidate.manifest.changed_files {
        *status_counts
            .entry(change_status_name(file.status).to_string())
            .or_default() += 1;
        known_bytes = known_bytes.saturating_add(file.byte_size.unwrap_or_default());
        binary_file_count += usize::from(file.is_binary);
    }
    let changed_files = candidate
        .manifest
        .changed_files
        .iter()
        .take(MAX_CHANGED_PATHS)
        .map(project_changed_file)
        .collect::<Vec<_>>();
    json!({
        "attempt_id": candidate.attempt_id,
        "attempt_seq": candidate.attempt_seq,
        "executor_kind": candidate.executor_kind,
        "branch": candidate.branch,
        "environment_generation": candidate.environment_generation,
        "baseline_oid": candidate.manifest.baseline_oid,
        "sealed_head_oid": candidate.manifest.sealed_head_oid,
        "current_base_oid_at_seal": candidate.manifest.current_base_oid,
        "base_advanced_at_seal": candidate.manifest.base_advanced,
        "evidence_id": candidate.manifest.evidence_id,
        "evidence_digest": candidate.manifest.bundle_digest,
        "sealed_at": candidate.manifest.sealed_at,
        "evidence_truncated": candidate.manifest.truncated,
        "changed_file_count": candidate.manifest.changed_files.len(),
        "changed_files_truncated": candidate.manifest.changed_files.len() > MAX_CHANGED_PATHS,
        "change_status_counts": status_counts,
        "known_changed_bytes": known_bytes,
        "binary_file_count": binary_file_count,
        "changed_files": changed_files,
        "notebook": notebook,
    })
}

fn project_changed_file(file: &ChangedFile) -> Value {
    json!({
        "path": truncate(&file.path, MAX_PATH_CHARS),
        "status": change_status_name(file.status),
        "old_path": file.old_path.as_deref().map(|path| truncate(path, MAX_PATH_CHARS)),
        "is_binary": file.is_binary,
        "byte_size": file.byte_size,
    })
}

fn compare_candidate_pairs(
    forge: &Forge,
    git_cwd: &std::path::Path,
    candidates: &[SealedCandidate],
) -> Vec<Value> {
    let mut pairs = Vec::new();
    for left_index in 0..candidates.len() {
        for right_index in (left_index + 1)..candidates.len() {
            let left = &candidates[left_index];
            let right = &candidates[right_index];
            pairs.push(project_pair(
                forge,
                git_cwd,
                left,
                right,
                &left.manifest.sealed_head_oid,
                &right.manifest.sealed_head_oid,
            ));
        }
    }
    pairs
}

fn project_pair(
    forge: &Forge,
    git_cwd: &std::path::Path,
    left: &SealedCandidate,
    right: &SealedCandidate,
    left_head: &GitOid,
    right_head: &GitOid,
) -> Value {
    let base = json!({
        "left_attempt_id": left.attempt_id,
        "right_attempt_id": right.attempt_id,
        "left_head_oid": left_head,
        "right_head_oid": right_head,
        "same_head": left_head == right_head,
        "same_baseline": left.manifest.baseline_oid == right.manifest.baseline_oid,
    });
    match forge.git().diff_name_status(git_cwd, left_head, right_head) {
        Ok(delta) => {
            let changed_paths = delta
                .iter()
                .take(MAX_CHANGED_PATHS)
                .map(|entry| {
                    json!({
                        "status": git_status_name(entry.status),
                        "path": truncate(&entry.path, MAX_PATH_CHARS),
                        "old_path": entry.orig_path.as_deref().map(|path| truncate(path, MAX_PATH_CHARS)),
                    })
                })
                .collect::<Vec<_>>();
            let mut projection = base;
            projection["comparison_status"] = Value::String("available".into());
            projection["changed_path_count"] = json!(delta.len());
            projection["changed_paths_truncated"] = Value::Bool(delta.len() > MAX_CHANGED_PATHS);
            projection["changed_paths"] = json!(changed_paths);
            projection
        }
        Err(_) => {
            let mut projection = base;
            projection["comparison_status"] = Value::String("unavailable".into());
            projection["reason"] =
                Value::String("one or more sealed Git objects could not be compared".into());
            projection
        }
    }
}

fn change_status_name(status: ChangeStatus) -> &'static str {
    match status {
        ChangeStatus::Added => "added",
        ChangeStatus::Modified => "modified",
        ChangeStatus::Deleted => "deleted",
        ChangeStatus::Renamed => "renamed",
        ChangeStatus::Copied => "copied",
        ChangeStatus::TypeChanged => "type_changed",
        ChangeStatus::Untracked => "untracked",
        ChangeStatus::Unmerged => "unmerged",
    }
}

fn git_status_name(status: char) -> &'static str {
    match status {
        'A' => "added",
        'D' => "deleted",
        'T' => "type_changed",
        'R' => "renamed",
        'C' => "copied",
        'U' => "unmerged",
        _ => "modified",
    }
}

fn truncate(value: &str, max_chars: usize) -> String {
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
    use async_trait::async_trait;
    use medousa_forge::forge::SealOptions;
    use medousa_forge::git::{CheckpointAuthor, GitEngine};
    use medousa_forge::model::ExecutorDescriptor;
    use std::sync::Mutex;
    use tempfile::TempDir;

    #[derive(Default)]
    struct NotebookRegistry {
        nodes_by_session: Mutex<HashMap<String, Vec<Value>>>,
        unavailable: bool,
    }

    #[async_trait]
    impl ToolRegistry for NotebookRegistry {
        async fn list_tools(&self) -> Result<Vec<Tool>> {
            Ok(Vec::new())
        }

        async fn invoke_tool(&self, tool_name: &str, input: Value) -> Result<Value> {
            assert_eq!(tool_name, crate::public_api::COGNITION_MEMORY_QUERY);
            if self.unavailable {
                return Err(input_error("simulated Locus outage"));
            }
            let session_id = input
                .get("session_id")
                .and_then(Value::as_str)
                .expect("runtime-pinned session");
            let nodes = self
                .nodes_by_session
                .lock()
                .expect("notebook lock")
                .get(session_id)
                .cloned()
                .unwrap_or_default();
            Ok(json!({ "retrieved": nodes.len(), "nodes": nodes }))
        }
    }

    struct ComparisonFixture {
        _repo: TempDir,
        _forge_root: TempDir,
        forge: Forge,
        entry: CoderEntryContext,
        candidate_ids: Vec<AttemptId>,
    }

    fn comparison_fixture() -> ComparisonFixture {
        let repo = TempDir::new().expect("repo");
        let forge_root = TempDir::new().expect("forge root");
        let git = GitEngine::detect().expect("git");
        assert!(
            std::process::Command::new("git")
                .args(["init", "-b", "main", "--template="])
                .current_dir(repo.path())
                .status()
                .expect("git init")
                .success()
        );
        std::fs::write(repo.path().join("seed.txt"), "seed\n").expect("seed");
        assert!(
            std::process::Command::new("git")
                .args(["add", "-A"])
                .current_dir(repo.path())
                .status()
                .expect("git add")
                .success()
        );
        git.commit_checkpoint(repo.path(), "initial", &CheckpointAuthor::default())
            .expect("initial commit");

        let forge = Forge::open(forge_root.path()).expect("forge");
        let item = forge
            .register(
                "Compare candidates",
                "Try two implementations",
                repo.path(),
                "main",
                "user-1",
                &Forge::system_actor(),
            )
            .expect("register");
        forge
            .provision(&item.id, &Forge::system_actor())
            .expect("provision");
        let executor = || ExecutorDescriptor {
            kind: "test-coder".into(),
            detail: Value::Null,
        };
        let (item, first) = forge
            .begin_isolated_attempt(&item.id, executor(), None, &Forge::system_actor())
            .expect("first attempt");
        let (item, second) = forge
            .begin_isolated_attempt(&item.id, executor(), None, &Forge::system_actor())
            .expect("second attempt");
        let (item, active) = forge
            .begin_isolated_attempt(&item.id, executor(), None, &Forge::system_actor())
            .expect("active comparison attempt");

        let first_env = item
            .environment_for_attempt(&first.attempt_id)
            .expect("first environment");
        let second_env = item
            .environment_for_attempt(&second.attempt_id)
            .expect("second environment");
        std::fs::write(
            first_env.worktree.join("candidate-a.txt"),
            "implementation A\n",
        )
        .expect("candidate A");
        std::fs::write(
            second_env.worktree.join("candidate-b.txt"),
            "implementation B\n",
        )
        .expect("candidate B");
        forge
            .complete_attempt(&first, &SealOptions::default(), &Forge::system_actor())
            .expect("seal first");
        forge
            .complete_attempt(&second, &SealOptions::default(), &Forge::system_actor())
            .expect("seal second");

        let entry = crate::agent_runtime::coder_mode::compile_coder_entry_for_attempt(
            &forge,
            &crate::daemon_api::CodeIntentContext {
                work_id: Some(item.id.to_string()),
                ..Default::default()
            },
            &active.attempt_id,
        )
        .expect("active Coder entry");
        ComparisonFixture {
            _repo: repo,
            _forge_root: forge_root,
            forge,
            entry,
            candidate_ids: vec![first.attempt_id, second.attempt_id],
        }
    }

    #[test]
    fn attempt_selection_requires_unique_bounded_ids() {
        assert!(parse_attempt_ids(&json!({})).unwrap().is_none());
        assert!(parse_attempt_ids(&json!({ "attempt_ids": ["a"] })).is_err());
        assert!(parse_attempt_ids(&json!({ "attempt_ids": ["a", "a"] })).is_err());
        let ids = parse_attempt_ids(&json!({ "attempt_ids": ["a", "b"] }))
            .unwrap()
            .unwrap();
        assert_eq!(ids.len(), 2);
    }

    #[test]
    fn notebook_projection_uses_a_closed_temporal_cutoff() {
        let cutoff = DateTime::parse_from_rfc3339("2026-08-08T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert!(node_at_or_before(
            &json!({ "timestamp": "2026-08-08T12:00:00Z" }),
            cutoff,
        ));
        assert!(!node_at_or_before(
            &json!({ "timestamp": "2026-08-08T12:00:01Z" }),
            cutoff,
        ));
        assert!(!node_at_or_before(&json!({}), cutoff));
    }

    #[tokio::test]
    async fn comparison_reads_only_sealed_evidence_and_pre_seal_notebooks() {
        let fixture = comparison_fixture();
        assert_eq!(
            sealed_candidate_count(&fixture.forge, &fixture.entry.work_id).unwrap(),
            2
        );
        let mut nodes_by_session = HashMap::new();
        for attempt_id in &fixture.candidate_ids {
            let manifest = fixture
                .forge
                .evidence_manifest_for_attempt(
                    &WorkId::from(fixture.entry.work_id.clone()),
                    attempt_id,
                )
                .expect("sealed manifest");
            let item = fixture
                .forge
                .load(&WorkId::from(fixture.entry.work_id.clone()))
                .expect("work item");
            let environment = item
                .environment_for_attempt(attempt_id)
                .expect("candidate environment");
            let scope = CoderMemoryScope::for_environment(
                &fixture.entry.repo_id,
                &fixture.entry.work_id,
                &environment.branch,
                environment.generation,
            );
            nodes_by_session.insert(
                scope.session_id,
                vec![
                    json!({
                        "sync_key": format!("early-{attempt_id}"),
                        "timestamp": (manifest.sealed_at - chrono::Duration::seconds(1)).to_rfc3339(),
                        "context_summary": format!("experiment: early notebook for {attempt_id}"),
                        "semantic_tags": ["kind:experiment", format!("head:{}", manifest.sealed_head_oid)],
                        "raw": "",
                    }),
                    json!({
                        "sync_key": format!("late-{attempt_id}"),
                        "timestamp": (manifest.sealed_at + chrono::Duration::seconds(1)).to_rfc3339(),
                        "context_summary": format!("experiment: late notebook for {attempt_id}"),
                        "semantic_tags": ["kind:experiment", format!("head:{}", manifest.sealed_head_oid)],
                        "raw": "",
                    }),
                ],
            );
        }
        let registry = NotebookRegistry {
            nodes_by_session: Mutex::new(nodes_by_session),
            unavailable: false,
        };

        let compared =
            compare_sealed_candidates(&fixture.forge, &registry, &fixture.entry, &json!({}))
                .await
                .expect("compare candidates");
        assert_eq!(compared["candidate_count"], 2);
        assert_eq!(compared["pair_count"], 1);
        assert_eq!(compared["pairwise"][0]["comparison_status"], "available");
        assert_eq!(compared["pairwise"][0]["changed_path_count"], 2);
        let rendered = compared.to_string();
        assert!(rendered.contains("early notebook"));
        assert!(!rendered.contains("late notebook"));
        assert!(!rendered.contains("implementation A"));
        assert!(!rendered.contains("implementation B"));
        assert!(!rendered.contains(fixture._forge_root.path().to_string_lossy().as_ref()));
    }

    #[tokio::test]
    async fn locus_outage_degrades_notebooks_without_losing_git_comparison() {
        let fixture = comparison_fixture();
        let registry = NotebookRegistry {
            nodes_by_session: Mutex::new(HashMap::new()),
            unavailable: true,
        };
        let compared =
            compare_sealed_candidates(&fixture.forge, &registry, &fixture.entry, &json!({}))
                .await
                .expect("Forge comparison survives Locus outage");
        assert_eq!(compared["pairwise"][0]["comparison_status"], "available");
        assert!(compared["candidates"].as_array().is_some_and(|candidates| {
            candidates.iter().all(|candidate| {
                candidate["notebook"]["memory_status"] == "unavailable"
                    && candidate["notebook"]["node_count"] == 0
            })
        }));
    }
}
