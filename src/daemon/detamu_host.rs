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
    pub baseline: Option<SnapshotRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sealed: Option<SnapshotRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_index: Option<IndexSummary>,
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
        let detamu = Detamu::builder(Arc::clone(&store))
            .analyzer(Arc::new(GitRepositoryAnalyzer))
            .deriver(Arc::new(GraphMetricsDeriver))
            .scoring_model(Arc::new(AvecCodeScorer::default()))
            .build();
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
        let report = self.index_path(worktree, Some(oid)).await?;
        let mut binding = self.load_binding(work_id).unwrap_or_else(|| WorkDetamuBinding {
            work_id: work_id.to_owned(),
            ..Default::default()
        });
        let snap = SnapshotRef::from(&report.snapshot);
        match kind {
            BindingKind::Baseline => binding.baseline = Some(snap),
            BindingKind::Sealed => binding.sealed = Some(snap),
        }
        binding.last_index = Some(IndexSummary::from(&report));
        self.save_binding(&binding)?;
        Ok(report)
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

// ---------------------------------------------------------------------------
// HTTP — /v1/world/* (distinct from medousa-code /v1/detamu/* stubs)
// ---------------------------------------------------------------------------

pub fn world_router(state: AppState) -> Router {
    Router::new()
        .route("/v1/world", get(world_status))
        .route("/v1/world/status", get(world_status))
        .route("/v1/world/index", post(world_index))
        .route("/v1/world/files", get(world_files))
        .route("/v1/world/bindings/{work_id}", get(world_binding))
        .with_state(state)
}

async fn world_status(State(state): State<AppState>) -> Json<Value> {
    let Some(host) = state.detamu.as_ref() else {
        return Json(json!({
            "ok": false,
            "available": false,
            "message": "Detamu host not opened (check daemon logs / path dep)",
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
        let report = host
            .index_forge_work(work_id, &env.worktree, &oid, kind)
            .await
            .map_err(|e| (axum::http::StatusCode::BAD_GATEWAY, e))?;
        return Ok(Json(json!({
            "ok": true,
            "work_id": work_id,
            "report": IndexSummary::from(&report),
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

async fn world_files(
    State(state): State<AppState>,
    Query(q): Query<FilesQuery>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let host = state.detamu.as_ref().ok_or((
        axum::http::StatusCode::SERVICE_UNAVAILABLE,
        "Detamu host unavailable".into(),
    ))?;

    let snapshot = resolve_snapshot(host, &q)?;
    let value = host
        .find_files(&snapshot, q.path.as_deref(), q.limit)
        .await
        .map_err(|e| (axum::http::StatusCode::BAD_GATEWAY, e))?;
    Ok(Json(value))
}

fn resolve_snapshot(
    host: &DetamuHost,
    q: &FilesQuery,
) -> Result<SnapshotId, (axum::http::StatusCode, String)> {
    if let (Some(world), Some(version)) = (q.world.as_deref(), q.version.as_deref()) {
        return Ok(SnapshotId::new(world, version));
    }
    if let Some(work_id) = q.work_id.as_deref() {
        let binding = host.load_binding(work_id).ok_or((
            axum::http::StatusCode::NOT_FOUND,
            format!("no Detamu binding for work {work_id}"),
        ))?;
        let snap = binding
            .sealed
            .or(binding.baseline)
            .ok_or((
                axum::http::StatusCode::NOT_FOUND,
                format!("work {work_id} has no indexed snapshot yet"),
            ))?;
        return Ok(SnapshotId::new(snap.world, snap.version));
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
