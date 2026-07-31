//! Detamu world-model host — versioned repo snapshots at commit OIDs.
//!
//! Detamu is a consumer SDK in the daemon (not folded into Forge or medousa-code).
//! Durable identity = commit OID. Scores exposed as `code_avec`, never bare `avec`.
//! See `architecture/detamu-medousa-fit.md`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use detamu::code::{AvecCodeScorer, GraphMetricsDeriver};
use detamu::code_query::{CodeEntityFilter, CodeQuery};
use detamu::core::SnapshotId;
use detamu::sdk::{Detamu, IndexReport};
use detamu::store::DetamuStore;
use detamu::surreal::SurrealStore;
use detamu_language::LanguagePack;
use detamu_language_rust::RustLanguagePack;
use detamu_source_git::{GitRepositoryAnalyzer, GitRepositorySource};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::RwLock;

use crate::daemon::state::AppState;
use crate::paths::medousa_data_dir;

/// Pointers from a Forge work item to Detamu snapshots (sidecar — not on
/// EvidenceManifest, which is digest-sensitive).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkDetamuBinding {
    pub work_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline: Option<SnapshotSlot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sealed: Option<SnapshotSlot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_index: Option<IndexSummary>,
    #[serde(default)]
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotSlot {
    /// queued | indexing | ready | failed
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotRef {
    pub world: String,
    pub version: String,
}

impl From<&SnapshotId> for SnapshotRef {
    fn from(id: &SnapshotId) -> Self {
        Self {
            world: id.world.as_str().to_owned(),
            version: id.version.as_str().to_owned(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexSummary {
    pub entities: usize,
    pub relations: usize,
    pub analyzers_run: usize,
    pub analyzers_skipped: usize,
    pub coverage: String,
    pub snapshot: SnapshotRef,
}

impl From<&IndexReport> for IndexSummary {
    fn from(report: &IndexReport) -> Self {
        Self {
            entities: report.entities,
            relations: report.relations,
            analyzers_run: report.analyzers_run,
            analyzers_skipped: report.analyzers_skipped,
            coverage: format!("{:?}", report.coverage).to_ascii_lowercase(),
            snapshot: SnapshotRef::from(&report.snapshot),
        }
    }
}

pub struct DetamuHost {
    detamu: Detamu,
    store: Arc<dyn DetamuStore>,
    root: PathBuf,
    /// Last successful index (any path), for status without a work_id.
    last_global: RwLock<Option<IndexSummary>>,
}

impl DetamuHost {
    pub async fn open(root: PathBuf) -> Result<Arc<Self>, String> {
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let db_path = root.join("store.surrealkv");
        let surreal = SurrealStore::surrealkv(&db_path, "medousa", "detamu")
            .await
            .map_err(|e| e.to_string())?;
        let store: Arc<dyn DetamuStore> = Arc::new(surreal);
        let rust_pack = RustLanguagePack::new(Arc::new(GitRepositorySource));
        let mut builder = Detamu::builder(Arc::clone(&store))
            .analyzer(Arc::new(GitRepositoryAnalyzer))
            .deriver(Arc::new(GraphMetricsDeriver))
            .scoring_model(Arc::new(AvecCodeScorer::default()));
        for analyzer in rust_pack.analyzers() {
            builder = builder.analyzer(analyzer);
        }
        let detamu = builder.build();
        Ok(Arc::new(Self {
            detamu,
            store,
            root,
            last_global: RwLock::new(None),
        }))
    }

    pub fn default_root() -> PathBuf {
        medousa_data_dir().join("detamu")
    }

    pub fn bindings_dir(&self) -> PathBuf {
        self.root.join("bindings")
    }

    pub fn status_json(&self) -> Value {
        json!({
            "ok": true,
            "available": true,
            "root": self.root.display().to_string(),
            "message": "Detamu world-model host ready (inventory + Code AVEC scoring)",
            "capabilities": self.capabilities_json(),
        })
    }

    pub async fn index_path(
        &self,
        locator: &Path,
        revision: Option<&str>,
    ) -> Result<IndexReport, String> {
        let request = detamu::model::SourceRequest {
            locator: locator.to_string_lossy().into_owned(),
            version: revision.map(str::to_owned),
        };
        let report = self
            .detamu
            .index_source(&GitRepositorySource, &request)
            .await
            .map_err(|e| e.to_string())?;
        *self.last_global.write().await = Some(IndexSummary::from(&report));
        Ok(report)
    }

    /// Index a Forge worktree at an explicit OID and stash the snapshot pointer.
    pub async fn index_forge_work(
        &self,
        work_id: &str,
        worktree: &Path,
        oid: &str,
        kind: BindingKind,
    ) -> Result<IndexReport, String> {
        self.mark_slot(work_id, kind, "indexing", Some(oid), None)?;
        let report = match self.index_path(worktree, Some(oid)).await {
            Ok(r) => r,
            Err(err) => {
                let _ = self.mark_slot(work_id, kind, "failed", Some(oid), Some(&err));
                return Err(err);
            }
        };
        let mut binding = self.load_binding(work_id).unwrap_or_else(|| WorkDetamuBinding {
            work_id: work_id.to_owned(),
            ..Default::default()
        });
        let snap = SnapshotRef::from(&report.snapshot);
        let slot = SnapshotSlot {
            state: "ready".into(),
            world: Some(snap.world),
            version: Some(snap.version),
            error: None,
        };
        match kind {
            BindingKind::Baseline => binding.baseline = Some(slot),
            BindingKind::Sealed => binding.sealed = Some(slot),
        }
        binding.last_index = Some(IndexSummary::from(&report));
        binding.diagnostics.clear();
        self.save_binding(&binding)?;
        Ok(report)
    }

    pub fn mark_slot(
        &self,
        work_id: &str,
        kind: BindingKind,
        state: &str,
        version: Option<&str>,
        error: Option<&str>,
    ) -> Result<(), String> {
        let mut binding = self.load_binding(work_id).unwrap_or_else(|| WorkDetamuBinding {
            work_id: work_id.to_owned(),
            ..Default::default()
        });
        let slot = SnapshotSlot {
            state: state.to_owned(),
            world: None,
            version: version.map(str::to_owned),
            error: error.map(str::to_owned),
        };
        match kind {
            BindingKind::Baseline => binding.baseline = Some(slot),
            BindingKind::Sealed => binding.sealed = Some(slot),
        }
        if let Some(err) = error {
            binding.diagnostics.push(err.to_owned());
        }
        self.save_binding(&binding)
    }

    pub async fn binding_status_json(&self, work_id: &str) -> Value {
        let binding = self.load_binding(work_id).unwrap_or_else(|| WorkDetamuBinding {
            work_id: work_id.to_owned(),
            ..Default::default()
        });
        json!({
            "work_id": binding.work_id,
            "baseline": binding.baseline,
            "sealed": binding.sealed,
            "last_index": binding.last_index,
            "diagnostics": binding.diagnostics,
            "capabilities": self.capabilities_json(),
        })
    }

    pub fn capabilities_json(&self) -> Value {
        json!({
            "git_inventory": true,
            "rust_syntax": true,
            "lizard": false,
            "rust_analyzer": false,
            "note": "Optional Lizard / rust-analyzer adapters not wired in this host"
        })
    }

    pub fn load_binding(&self, work_id: &str) -> Option<WorkDetamuBinding> {
        let path = self.bindings_dir().join(format!("{work_id}.json"));
        let raw = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&raw).ok()
    }

    fn save_binding(&self, binding: &WorkDetamuBinding) -> Result<(), String> {
        let dir = self.bindings_dir();
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join(format!("{}.json", binding.work_id));
        let raw = serde_json::to_string_pretty(binding).map_err(|e| e.to_string())?;
        std::fs::write(path, raw).map_err(|e| e.to_string())
    }

    pub async fn find_files(
        &self,
        snapshot: &SnapshotId,
        path_contains: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Value, String> {
        let query = CodeQuery::new(Arc::clone(&self.store));
        let filter = CodeEntityFilter {
            path: path_contains.map(str::to_owned),
            name_contains: None,
            kind: Some("file".into()),
            language: None,
            limit,
        };
        let entities = query
            .find(snapshot, &filter)
            .await
            .map_err(|e| e.to_string())?;
        let files: Vec<Value> = entities
            .iter()
            .map(|obs| {
                json!({
                    "id": obs.entity.id.as_str(),
                    "label": obs.entity.label,
                    "kind": obs.entity.kind,
                    "path": obs.attributes.get("file_path").and_then(|v| v.as_str()),
                    "language": obs.attributes.get("language").and_then(|v| v.as_str()),
                })
            })
            .collect();
        Ok(json!({
            "ok": true,
            "snapshot": {
                "world": snapshot.world.as_str(),
                "version": snapshot.version.as_str(),
            },
            "files": files,
        }))
    }

    pub async fn last_global_summary(&self) -> Option<IndexSummary> {
        self.last_global.read().await.clone()
    }

    pub fn code_query(&self) -> CodeQuery {
        CodeQuery::new(Arc::clone(&self.store))
    }

    pub async fn find_entities(
        &self,
        snapshot: &SnapshotId,
        filter: &CodeEntityFilter,
    ) -> Result<Value, String> {
        let query = self.code_query();
        let entities = query
            .find(snapshot, filter)
            .await
            .map_err(|e| e.to_string())?;
        let items: Vec<Value> = entities.iter().map(entity_summary_json).collect();
        Ok(json!({
            "ok": true,
            "snapshot": snapshot_json(snapshot),
            "entities": items,
        }))
    }

    pub async fn impact(
        &self,
        snapshot: &SnapshotId,
        entity_id: &str,
        max_depth: u32,
        max_nodes: usize,
    ) -> Result<Value, String> {
        let query = self.code_query();
        match query
            .impact(
                snapshot,
                &detamu::core::EntityId::new(entity_id),
                max_depth,
                max_nodes,
            )
            .await
        {
            Ok(impact) => {
                let nodes: Vec<Value> = impact
                    .graph
                    .nodes
                    .iter()
                    .map(|node| {
                        let mut v = entity_summary_json(&node.observation);
                        if let Some(obj) = v.as_object_mut() {
                            obj.insert("depth".into(), json!(node.depth));
                        }
                        v
                    })
                    .collect();
                Ok(json!({
                    "ok": true,
                    "snapshot": snapshot_json(snapshot),
                    "target": entity_summary_json(&impact.target),
                    "direct_dependents": impact.direct_dependents,
                    "transitive_dependents": impact.transitive_dependents,
                    "nodes": nodes,
                }))
            }
            Err(detamu::query::QueryError::EntityNotFound { .. }) => Ok(json!({
                "ok": true,
                "snapshot": snapshot_json(snapshot),
                "entity_id": entity_id,
                "direct_dependents": 0,
                "transitive_dependents": 0,
                "nodes": [],
                "message": "entity not in snapshot graph (inventory-only or kind not indexed)",
            })),
            Err(err) => Err(err.to_string()),
        }
    }

    pub async fn code_avec_gaps(&self, snapshot: &SnapshotId) -> Result<Value, String> {
        let query = self.code_query();
        let report = query.gaps(snapshot).await.map_err(|e| e.to_string())?;
        let gaps: Vec<Value> = report
            .gaps
            .iter()
            .map(|gap| {
                let mut v = entity_summary_value(&gap.entity);
                if let Some(obj) = v.as_object_mut() {
                    obj.insert("missing_measurements".into(), json!(gap.missing_measurements));
                    obj.insert("missing_scores".into(), json!(gap.missing_scores));
                }
                v
            })
            .collect();
        Ok(json!({
            "ok": true,
            "snapshot": snapshot_json(snapshot),
            "code_avec": {
                "scoreable_entities": report.scoreable_entities,
                "fully_scored_entities": report.fully_scored_entities,
                "gaps": gaps,
            },
        }))
    }

    pub async fn at_location(
        &self,
        snapshot: &SnapshotId,
        path: &str,
        line: u32,
    ) -> Result<Value, String> {
        let query = self.code_query();
        let entity = query
            .at_location(snapshot, path, line)
            .await
            .map_err(|e| e.to_string())?;
        Ok(json!({
            "ok": true,
            "snapshot": snapshot_json(snapshot),
            "entity": entity.as_ref().map(entity_summary_json),
        }))
    }
}

fn snapshot_json(snapshot: &SnapshotId) -> Value {
    json!({
        "world": snapshot.world.as_str(),
        "version": snapshot.version.as_str(),
    })
}

fn entity_summary_json(obs: &detamu::core::EntityObservation) -> Value {
    let summary = detamu::code_query::CodeEntitySummary::from(obs);
    entity_summary_value(&summary)
}

fn entity_summary_value(summary: &detamu::code_query::CodeEntitySummary) -> Value {
    json!({
        "id": summary.id.as_str(),
        "label": summary.label,
        "kind": summary.kind,
        "path": summary.path,
        "language": serde_json::Value::Null,
        "line_start": summary.line_start,
        "line_end": summary.line_end,
    })
}

#[derive(Debug, Clone, Copy)]
pub enum BindingKind {
    Baseline,
    Sealed,
}

/// Best-effort index after Forge provision/seal. Never fails the Forge verb.
pub async fn maybe_index_forge_item(
    host: &Option<Arc<DetamuHost>>,
    work_id: &str,
    worktree: &Path,
    oid: &str,
    kind: BindingKind,
) {
    let Some(host) = host else {
        return;
    };
    match host.index_forge_work(work_id, worktree, oid, kind).await {
        Ok(report) => {
            tracing::info!(
                work_id,
                oid,
                entities = report.entities,
                "detamu indexed forge worktree"
            );
        }
        Err(err) => {
            tracing::warn!(work_id, oid, %err, "detamu index failed (non-fatal)");
        }
    }
}

/// Fire-and-forget index so Forge mutations return immediately.
pub fn spawn_index_forge_item(
    host: Option<Arc<DetamuHost>>,
    work_id: String,
    worktree: PathBuf,
    oid: String,
    kind: BindingKind,
) {
    let Some(host) = host else {
        return;
    };
    let _ = host.mark_slot(&work_id, kind, "queued", Some(&oid), None);
    tokio::spawn(async move {
        maybe_index_forge_item(&Some(host), &work_id, &worktree, &oid, kind).await;
    });
}

// ---------------------------------------------------------------------------
// HTTP — /v1/world/* (distinct from medousa-code /v1/detamu/* stubs)
// ---------------------------------------------------------------------------

pub fn world_router(state: AppState) -> Router {
    Router::new()
        .route("/v1/world", get(world_status))
        .route("/v1/world/status", get(world_status))
        .route("/v1/world/index", post(world_index))
        .route("/v1/world/files", get(world_files))
        .route("/v1/world/impact", get(world_impact))
        .route("/v1/world/code_avec", get(world_code_avec))
        .route("/v1/world/find", get(world_find))
        .route("/v1/world/at_location", get(world_at_location))
        .route("/v1/world/bindings/{work_id}", get(world_binding))
        .with_state(state)
}

async fn world_status(State(state): State<AppState>) -> Json<Value> {
    let Some(host) = state.detamu.as_ref() else {
        return Json(json!({
            "ok": false,
            "available": false,
            "message": "Detamu host not opened (check daemon logs / storage initialization)",
        }));
    };
    let mut body = host.status_json();
    if let Some(last) = host.last_global_summary().await {
        body.as_object_mut()
            .expect("object")
            .insert("last_index".into(), serde_json::to_value(last).unwrap_or(Value::Null));
    }
    Json(body)
}

#[derive(Debug, Deserialize)]
struct IndexBody {
    #[serde(default)]
    work_id: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    revision: Option<String>,
    #[serde(default)]
    kind: Option<String>,
}

async fn world_index(
    State(state): State<AppState>,
    Json(body): Json<IndexBody>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let host = state.detamu.as_ref().ok_or((
        axum::http::StatusCode::SERVICE_UNAVAILABLE,
        "Detamu host unavailable".into(),
    ))?;

    // Prefer work-scoped reindex from undertaking UI (no arbitrary paths).
    if let Some(work_id) = body.work_id.as_deref().filter(|s| !s.trim().is_empty()) {
        let item = state
            .forge
            .load(&medousa_forge::model::WorkId::from(work_id.to_string()))
            .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e.to_string()))?;
        let env = item.environment.ok_or((
            axum::http::StatusCode::BAD_REQUEST,
            format!("work {work_id} is not provisioned"),
        ))?;
        let oid = body
            .revision
            .clone()
            .unwrap_or_else(|| env.baseline_oid.as_str().to_owned());
        let kind = match body.kind.as_deref() {
            Some("sealed") => BindingKind::Sealed,
            _ => BindingKind::Baseline,
        };
        spawn_index_forge_item(
            state.detamu.clone(),
            work_id.to_owned(),
            env.worktree.clone(),
            oid,
            kind,
        );
        return Ok(Json(json!({
            "ok": true,
            "work_id": work_id,
            "queued": true,
            "binding": host.binding_status_json(work_id).await,
        })));
    }

    let path = body
        .path
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .ok_or((
            axum::http::StatusCode::BAD_REQUEST,
            "path or work_id required".into(),
        ))?;
    let report = host
        .index_path(Path::new(path), body.revision.as_deref())
        .await
        .map_err(|e| (axum::http::StatusCode::BAD_GATEWAY, e))?;
    Ok(Json(json!({
        "ok": true,
        "report": IndexSummary::from(&report),
    })))
}

#[derive(Debug, Deserialize)]
struct FilesQuery {
    #[serde(default)]
    work_id: Option<String>,
    #[serde(default)]
    world: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct SnapshotQuery {
    #[serde(default)]
    work_id: Option<String>,
    #[serde(default)]
    world: Option<String>,
    #[serde(default)]
    version: Option<String>,
}

impl From<&FilesQuery> for SnapshotQuery {
    fn from(q: &FilesQuery) -> Self {
        Self {
            work_id: q.work_id.clone(),
            world: q.world.clone(),
            version: q.version.clone(),
        }
    }
}

async fn world_files(
    State(state): State<AppState>,
    Query(q): Query<FilesQuery>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let host = state.detamu.as_ref().ok_or((
        axum::http::StatusCode::SERVICE_UNAVAILABLE,
        "Detamu host unavailable".into(),
    ))?;

    let snapshot = resolve_snapshot(host, &SnapshotQuery::from(&q))?;
    let value = host
        .find_files(&snapshot, q.path.as_deref(), q.limit)
        .await
        .map_err(|e| (axum::http::StatusCode::BAD_GATEWAY, e))?;
    Ok(Json(value))
}

#[derive(Debug, Deserialize)]
struct ImpactQuery {
    #[serde(flatten)]
    snapshot: SnapshotQuery,
    entity_id: String,
    #[serde(default = "default_impact_depth")]
    max_depth: u32,
    #[serde(default = "default_impact_nodes")]
    max_nodes: usize,
}

fn default_impact_depth() -> u32 {
    4
}

fn default_impact_nodes() -> usize {
    200
}

async fn world_impact(
    State(state): State<AppState>,
    Query(q): Query<ImpactQuery>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let host = state.detamu.as_ref().ok_or((
        axum::http::StatusCode::SERVICE_UNAVAILABLE,
        "Detamu host unavailable".into(),
    ))?;
    let snapshot = resolve_snapshot(host, &q.snapshot)?;
    let value = host
        .impact(&snapshot, &q.entity_id, q.max_depth, q.max_nodes)
        .await
        .map_err(|e| (axum::http::StatusCode::BAD_GATEWAY, e))?;
    Ok(Json(value))
}

async fn world_code_avec(
    State(state): State<AppState>,
    Query(q): Query<SnapshotQuery>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let host = state.detamu.as_ref().ok_or((
        axum::http::StatusCode::SERVICE_UNAVAILABLE,
        "Detamu host unavailable".into(),
    ))?;
    let snapshot = resolve_snapshot(host, &q)?;
    let value = host
        .code_avec_gaps(&snapshot)
        .await
        .map_err(|e| (axum::http::StatusCode::BAD_GATEWAY, e))?;
    Ok(Json(value))
}

#[derive(Debug, Deserialize)]
struct FindQuery {
    #[serde(flatten)]
    snapshot: SnapshotQuery,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    name_contains: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

async fn world_find(
    State(state): State<AppState>,
    Query(q): Query<FindQuery>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let host = state.detamu.as_ref().ok_or((
        axum::http::StatusCode::SERVICE_UNAVAILABLE,
        "Detamu host unavailable".into(),
    ))?;
    let snapshot = resolve_snapshot(host, &q.snapshot)?;
    let filter = CodeEntityFilter {
        path: q.path.clone(),
        name_contains: q.name_contains.clone(),
        kind: q.kind.clone(),
        language: None,
        limit: q.limit,
    };
    let value = host
        .find_entities(&snapshot, &filter)
        .await
        .map_err(|e| (axum::http::StatusCode::BAD_GATEWAY, e))?;
    Ok(Json(value))
}

#[derive(Debug, Deserialize)]
struct AtLocationQuery {
    #[serde(flatten)]
    snapshot: SnapshotQuery,
    path: String,
    line: u32,
}

async fn world_at_location(
    State(state): State<AppState>,
    Query(q): Query<AtLocationQuery>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let host = state.detamu.as_ref().ok_or((
        axum::http::StatusCode::SERVICE_UNAVAILABLE,
        "Detamu host unavailable".into(),
    ))?;
    let snapshot = resolve_snapshot(host, &q.snapshot)?;
    let value = host
        .at_location(&snapshot, &q.path, q.line)
        .await
        .map_err(|e| (axum::http::StatusCode::BAD_GATEWAY, e))?;
    Ok(Json(value))
}

fn resolve_snapshot(
    host: &DetamuHost,
    q: &SnapshotQuery,
) -> Result<SnapshotId, (axum::http::StatusCode, String)> {
    if let (Some(world), Some(version)) = (q.world.as_deref(), q.version.as_deref()) {
        return Ok(SnapshotId::new(world, version));
    }
    if let Some(work_id) = q.work_id.as_deref() {
        let binding = host.load_binding(work_id).ok_or((
            axum::http::StatusCode::NOT_FOUND,
            format!("no Detamu binding for work {work_id}"),
        ))?;
        let slot = binding
            .sealed
            .filter(|s| s.state == "ready")
            .or_else(|| binding.baseline.filter(|s| s.state == "ready"))
            .ok_or((
                axum::http::StatusCode::NOT_FOUND,
                format!("work {work_id} has no ready indexed snapshot yet"),
            ))?;
        let world = slot.world.as_deref().ok_or((
            axum::http::StatusCode::NOT_FOUND,
            format!("work {work_id} snapshot missing world"),
        ))?;
        let version = slot.version.as_deref().ok_or((
            axum::http::StatusCode::NOT_FOUND,
            format!("work {work_id} snapshot missing version"),
        ))?;
        return Ok(SnapshotId::new(world, version));
    }
    Err((
        axum::http::StatusCode::BAD_REQUEST,
        "provide work_id or world+version".into(),
    ))
}

async fn world_binding(
    State(state): State<AppState>,
    axum::extract::Path(work_id): axum::extract::Path<String>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let host = state.detamu.as_ref().ok_or((
        axum::http::StatusCode::SERVICE_UNAVAILABLE,
        "Detamu host unavailable".into(),
    ))?;
    let binding = host.load_binding(&work_id).ok_or((
        axum::http::StatusCode::NOT_FOUND,
        format!("no Detamu binding for work {work_id}"),
    ))?;
    Ok(Json(serde_json::to_value(binding).unwrap_or(Value::Null)))
}
