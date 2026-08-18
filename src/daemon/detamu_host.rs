//! Detamu world-model host — versioned repo snapshots at commit OIDs.
//!
//! Detamu is a consumer SDK in the daemon (not folded into Forge or medousa-code).
//! Durable identity = commit OID. Scores exposed as `code_avec`, never bare `avec`.
//! See `architecture/detamu-medousa-fit.md`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};

use axum::Json;
use axum::extract::{Query, State};
use axum::routing::{get, post};
use chrono::Utc;
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
use serde_json::{Value, json};
use surrealdb::Surreal;
use surrealdb::engine::local::{Db, SurrealKv};
use tokio::sync::{Mutex as TokioMutex, RwLock};

use crate::daemon::route_policy::{
    BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RoutePolicy,
};
use crate::daemon::state::AppState;
use crate::paths::medousa_data_dir;
use crate::store_root::{StorePath, StoreRoot};
use crate::surreal_config;

const MAX_DETAMU_BINDING_BYTES: u64 = 4 * 1024 * 1024;

/// Medousa-owned repair after `ensure_schema`. Nested SCHEMAFULL field defs from
/// older writes survive `DEFINE FIELD OVERWRITE` of the parent unless we drop
/// them first. Named so unit tests can lock the SQL without opening KV.
pub(crate) const DETAMU_FLEXIBLE_FIELD_REPAIR: &str = r"
REMOVE FIELD IF EXISTS provenance ON TABLE detamu_snapshot;
DEFINE FIELD OVERWRITE provenance ON TABLE detamu_snapshot TYPE array<object> FLEXIBLE;
REMOVE FIELD IF EXISTS diagnostics ON TABLE detamu_snapshot;
DEFINE FIELD OVERWRITE diagnostics ON TABLE detamu_snapshot TYPE array<object> FLEXIBLE;
REMOVE FIELD IF EXISTS payload ON TABLE detamu_entity_observation;
DEFINE FIELD OVERWRITE payload ON TABLE detamu_entity_observation TYPE object FLEXIBLE;
REMOVE FIELD IF EXISTS payload ON TABLE detamu_relation_observation;
DEFINE FIELD OVERWRITE payload ON TABLE detamu_relation_observation TYPE object FLEXIBLE;
";

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

/// Shareable Detamu owner: directories exist at boot, SurrealKV stays closed
/// until a world query or Forge index needs it.
pub struct DetamuHandle {
    root: PathBuf,
    host: StdMutex<Option<Arc<DetamuHost>>>,
    open_lock: TokioMutex<()>,
}

impl DetamuHandle {
    /// Create `{root}` and `bindings/` only. Never calls `Surreal::new`.
    pub fn dormant(root: PathBuf) -> Arc<Self> {
        if let Err(err) = std::fs::create_dir_all(root.join("bindings")) {
            tracing::warn!(
                path = %root.display(),
                %err,
                "failed to create detamu directories"
            );
        }
        Arc::new(Self {
            root,
            host: StdMutex::new(None),
            open_lock: TokioMutex::new(()),
        })
    }

    pub fn peek(&self) -> Option<Arc<DetamuHost>> {
        self.host
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub async fn get_or_open(&self) -> Result<Arc<DetamuHost>, String> {
        if let Some(host) = self.peek() {
            return Ok(host);
        }
        let _guard = self.open_lock.lock().await;
        if let Some(host) = self.peek() {
            return Ok(host);
        }
        let host = DetamuHost::open(self.root.clone()).await?;
        *self
            .host
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Arc::clone(&host));
        Ok(host)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn store_path(&self) -> PathBuf {
        detamu_store_path(&self.root)
    }

    pub fn bindings_dir(&self) -> PathBuf {
        detamu_bindings_dir(&self.root)
    }

    pub fn load_binding(&self, work_id: &str) -> Option<WorkDetamuBinding> {
        load_binding_at(&self.root, work_id)
    }

    pub fn mark_slot(
        &self,
        work_id: &str,
        kind: BindingKind,
        state: &str,
        version: Option<&str>,
        error: Option<&str>,
    ) -> Result<(), String> {
        mark_slot_at(&self.root, work_id, kind, state, version, error)
    }

    pub fn binding_status_json(&self, work_id: &str) -> Value {
        binding_status_json_at(&self.root, work_id)
    }

    pub async fn status_json(&self) -> Value {
        let loaded = self.peek();
        let (wal_bytes, sst_bytes) = store_disk_stats(&self.root);
        let mut body = json!({
            "ok": true,
            "available": true,
            "loaded": loaded.is_some(),
            "root": self.root.display().to_string(),
            "wal_bytes": wal_bytes,
            "sst_bytes": sst_bytes,
            "message": if loaded.is_some() {
                "Detamu world-model host ready (inventory + Code AVEC scoring)"
            } else {
                "Detamu store is dormant until a world query or Forge index opens it"
            },
            "capabilities": capabilities_json(),
        });
        if let Some(hint) = wal_growth_hint(wal_bytes, sst_bytes) {
            body.as_object_mut()
                .expect("object")
                .insert("hint".into(), json!(hint));
        }
        if let Some(host) = loaded
            && let Some(last) = host.last_global_summary().await
        {
            body.as_object_mut().expect("object").insert(
                "last_index".into(),
                serde_json::to_value(last).unwrap_or(Value::Null),
            );
        }
        body
    }

    /// Rename `store.surrealkv` aside and drop the cached host. Bindings JSON stay.
    pub async fn reset_store(&self) -> Result<Value, String> {
        let _guard = self.open_lock.lock().await;
        *self
            .host
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        let store = self.store_path();
        if !store.exists() {
            return Ok(json!({
                "ok": true,
                "loaded": false,
                "message": "no Detamu store directory to reset",
            }));
        }
        let stamp = Utc::now().format("%Y%m%dT%H%M%SZ");
        let bak = self.root.join(format!("store.surrealkv.bak-{stamp}"));
        std::fs::rename(&store, &bak).map_err(|e| e.to_string())?;
        Ok(json!({
            "ok": true,
            "loaded": false,
            "renamed_to": bak.display().to_string(),
        }))
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
        std::fs::create_dir_all(root.join("bindings")).map_err(|e| e.to_string())?;
        let db_path = detamu_store_path(&root);
        let (wal_bytes, sst_bytes) = store_disk_stats(&root);
        tracing::info!(
            path = %db_path.display(),
            wal_bytes,
            sst_bytes,
            memtable_bytes = surreal_config::desktop_surrealkv_memtable_bytes(),
            block_cache_bytes = surreal_config::desktop_surrealkv_block_cache_bytes(),
            "opening Detamu SurrealKV"
        );
        let endpoint = detamu_surrealkv_endpoint(&db_path);
        let db = Surreal::new::<SurrealKv>(endpoint)
            .await
            .map_err(|e| e.to_string())?;
        db.use_ns("medousa")
            .use_db("detamu")
            .await
            .map_err(|e| e.to_string())?;
        let surreal = SurrealStore::new(db);
        surreal.ensure_schema().await.map_err(|e| e.to_string())?;
        repair_flexible_fields(surreal.database()).await;
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
        detamu_bindings_dir(&self.root)
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
        let mut binding = self
            .load_binding(work_id)
            .unwrap_or_else(|| WorkDetamuBinding {
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
        mark_slot_at(&self.root, work_id, kind, state, version, error)
    }

    pub fn binding_status_json(&self, work_id: &str) -> Value {
        binding_status_json_at(&self.root, work_id)
    }

    pub fn load_binding(&self, work_id: &str) -> Option<WorkDetamuBinding> {
        load_binding_at(&self.root, work_id)
    }

    fn save_binding(&self, binding: &WorkDetamuBinding) -> Result<(), String> {
        save_binding_at(&self.root, binding)
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
                    obj.insert(
                        "missing_measurements".into(),
                        json!(gap.missing_measurements),
                    );
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

pub(crate) fn detamu_surrealkv_endpoint(db_path: &Path) -> String {
    surreal_config::with_desktop_surrealkv_caps(&db_path.to_string_lossy())
}

fn detamu_store_path(root: &Path) -> PathBuf {
    root.join("store.surrealkv")
}

fn detamu_bindings_dir(root: &Path) -> PathBuf {
    root.join("bindings")
}

fn capabilities_json() -> Value {
    json!({
        "git_inventory": true,
        "rust_syntax": true,
        "lizard": false,
        "rust_analyzer": false,
        "note": "Optional Lizard / rust-analyzer adapters not wired in this host"
    })
}

fn load_binding_at(root: &Path, work_id: &str) -> Option<WorkDetamuBinding> {
    let work_id = medousa_forge::model::WorkId::parse_storage(work_id).ok()?;
    let store = StoreRoot::open_or_create_nofollow(&detamu_bindings_dir(root)).ok()?;
    let path = detamu_binding_path(&work_id);
    let raw = match store.read_limited(&path, MAX_DETAMU_BINDING_BYTES) {
        Ok(raw) => raw,
        Err(error) if error.is_not_found() => {
            let legacy = StorePath::parse(&format!("{}.json", work_id.as_str())).ok()?;
            store.read_limited(&legacy, MAX_DETAMU_BINDING_BYTES).ok()?
        }
        Err(_) => return None,
    };
    let binding: WorkDetamuBinding = serde_json::from_slice(&raw).ok()?;
    (binding.work_id == work_id.as_str()).then_some(binding)
}

fn save_binding_at(root: &Path, binding: &WorkDetamuBinding) -> Result<(), String> {
    let work_id = medousa_forge::model::WorkId::parse_storage(&binding.work_id)
        .map_err(|_| "invalid work_id".to_string())?;
    let store = StoreRoot::open_or_create_nofollow(&detamu_bindings_dir(root))
        .map_err(|error| error.to_string())?;
    let raw = serde_json::to_vec_pretty(binding).map_err(|e| e.to_string())?;
    store
        .atomic_write(&detamu_binding_path(&work_id), &raw)
        .map_err(|error| error.to_string())
}

fn mark_slot_at(
    root: &Path,
    work_id: &str,
    kind: BindingKind,
    state: &str,
    version: Option<&str>,
    error: Option<&str>,
) -> Result<(), String> {
    let mut binding = load_binding_at(root, work_id).unwrap_or_else(|| WorkDetamuBinding {
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
    save_binding_at(root, &binding)
}

fn binding_status_json_at(root: &Path, work_id: &str) -> Value {
    let binding = load_binding_at(root, work_id).unwrap_or_else(|| WorkDetamuBinding {
        work_id: work_id.to_owned(),
        ..Default::default()
    });
    json!({
        "work_id": binding.work_id,
        "baseline": binding.baseline,
        "sealed": binding.sealed,
        "last_index": binding.last_index,
        "diagnostics": binding.diagnostics,
        "capabilities": capabilities_json(),
    })
}

pub(crate) fn store_disk_stats(root: &Path) -> (u64, u64) {
    let store = detamu_store_path(root);
    (dir_size(&store.join("wal")), dir_size(&store.join("sst")))
}

fn dir_size(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    let mut total = 0;
    for entry in entries.flatten() {
        let child = entry.path();
        if child.is_dir() {
            total += dir_size(&child);
        } else if let Ok(meta) = entry.metadata() {
            total += meta.len();
        }
    }
    total
}

pub(crate) fn wal_growth_hint(wal_bytes: u64, sst_bytes: u64) -> Option<String> {
    if wal_bytes > surreal_config::DEFAULT_DESKTOP_SURREALKV_MEMTABLE_BYTES
        && wal_bytes > sst_bytes.saturating_mul(8)
    {
        Some(
            "WAL is much larger than SST; POST /v1/world/reset-store to start empty (bindings JSON are kept)"
                .into(),
        )
    } else {
        None
    }
}

async fn repair_flexible_fields(db: &Surreal<Db>) {
    match db.query(DETAMU_FLEXIBLE_FIELD_REPAIR).await {
        Ok(response) => {
            if let Err(err) = response.check() {
                tracing::warn!(%err, "detamu flexible field repair failed");
            } else {
                tracing::info!("detamu flexible field repair applied");
            }
        }
        Err(err) => {
            tracing::warn!(%err, "detamu flexible field repair query failed");
        }
    }
}

fn detamu_binding_path(work_id: &medousa_forge::model::WorkId) -> StorePath {
    StorePath::parse(&format!("{}.json", work_id.storage_key()))
        .expect("opaque Forge work key is a valid Detamu binding path")
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
    host: &DetamuHost,
    work_id: &str,
    worktree: &Path,
    oid: &str,
    kind: BindingKind,
) {
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
    handle: Arc<DetamuHandle>,
    work_id: String,
    worktree: PathBuf,
    oid: String,
    kind: BindingKind,
) {
    let _ = handle.mark_slot(&work_id, kind, "queued", Some(&oid), None);
    tokio::spawn(async move {
        match handle.get_or_open().await {
            Ok(host) => {
                maybe_index_forge_item(&host, &work_id, &worktree, &oid, kind).await;
            }
            Err(err) => {
                tracing::warn!(work_id, oid, %err, "detamu index failed (non-fatal)");
                let _ = handle.mark_slot(&work_id, kind, "failed", Some(&oid), Some(&err));
            }
        }
    });
}

// ---------------------------------------------------------------------------
// HTTP — /v1/world/* (distinct from medousa-code /v1/detamu/* stubs)
// ---------------------------------------------------------------------------

pub fn world_surface() -> DeclaredRouter<AppState> {
    DeclaredRouter::default()
        .route(world_read_policy("/v1/world"), get(world_status))
        .route(world_read_policy("/v1/world/status"), get(world_status))
        .route(
            RoutePolicy {
                method: axum::http::Method::POST,
                path: "/v1/world/index",
                group: RouteGroup::Administration,
                required_capability: Some(crate::request_principal::Capability::AdminExecute),
                bootstrap_public: false,
                browser_policy: BrowserPolicy::NativeOnly,
                body_limit: 256 * 1024,
                rate_limit_class: RateLimitClass::Administration,
            },
            post(world_index),
        )
        .route(
            RoutePolicy {
                method: axum::http::Method::POST,
                path: "/v1/world/reset-store",
                group: RouteGroup::Administration,
                required_capability: Some(crate::request_principal::Capability::AdminExecute),
                bootstrap_public: false,
                browser_policy: BrowserPolicy::NativeOnly,
                body_limit: 1024,
                rate_limit_class: RateLimitClass::Administration,
            },
            post(world_reset_store),
        )
        .route(world_read_policy("/v1/world/files"), get(world_files))
        .route(world_read_policy("/v1/world/impact"), get(world_impact))
        .route(
            world_read_policy("/v1/world/code_avec"),
            get(world_code_avec),
        )
        .route(world_read_policy("/v1/world/find"), get(world_find))
        .route(
            world_read_policy("/v1/world/at_location"),
            get(world_at_location),
        )
        .route(
            world_read_policy("/v1/world/bindings/{work_id}"),
            get(world_binding),
        )
}

fn world_read_policy(path: &'static str) -> RoutePolicy {
    RoutePolicy {
        method: axum::http::Method::GET,
        path,
        group: RouteGroup::Portal,
        required_capability: Some(crate::request_principal::Capability::WorkshopRead),
        bootstrap_public: false,
        browser_policy: BrowserPolicy::NativeOnly,
        body_limit: 1024,
        rate_limit_class: RateLimitClass::Read,
    }
}

async fn world_status(State(state): State<AppState>) -> Json<Value> {
    Json(state.detamu.status_json().await)
}

async fn world_reset_store(
    State(state): State<AppState>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let body = state
        .detamu
        .reset_store()
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(body))
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
            "binding": state.detamu.binding_status_json(work_id),
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
    let handle = state.detamu.clone();
    let path_owned = path.to_owned();
    let revision = body.revision.clone();
    tokio::spawn(async move {
        match handle.get_or_open().await {
            Ok(host) => {
                if let Err(err) = host
                    .index_path(Path::new(&path_owned), revision.as_deref())
                    .await
                {
                    tracing::warn!(path = %path_owned, %err, "detamu path index failed");
                }
            }
            Err(err) => {
                tracing::warn!(path = %path_owned, %err, "detamu path index failed");
            }
        }
    });
    Ok(Json(json!({
        "ok": true,
        "queued": true,
        "path": path,
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

async fn require_open_host(
    state: &AppState,
) -> Result<Arc<DetamuHost>, (axum::http::StatusCode, String)> {
    state.detamu.get_or_open().await.map_err(|e| {
        (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            format!("Detamu host unavailable: {e}"),
        )
    })
}

async fn world_files(
    State(state): State<AppState>,
    Query(q): Query<FilesQuery>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let snapshot = resolve_snapshot(state.detamu.root(), &SnapshotQuery::from(&q))?;
    let host = require_open_host(&state).await?;
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
    let snapshot = resolve_snapshot(state.detamu.root(), &q.snapshot)?;
    let host = require_open_host(&state).await?;
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
    let snapshot = resolve_snapshot(state.detamu.root(), &q)?;
    let host = require_open_host(&state).await?;
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
    let snapshot = resolve_snapshot(state.detamu.root(), &q.snapshot)?;
    let host = require_open_host(&state).await?;
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
    let snapshot = resolve_snapshot(state.detamu.root(), &q.snapshot)?;
    let host = require_open_host(&state).await?;
    let value = host
        .at_location(&snapshot, &q.path, q.line)
        .await
        .map_err(|e| (axum::http::StatusCode::BAD_GATEWAY, e))?;
    Ok(Json(value))
}

fn resolve_snapshot(
    root: &Path,
    q: &SnapshotQuery,
) -> Result<SnapshotId, (axum::http::StatusCode, String)> {
    if let (Some(world), Some(version)) = (q.world.as_deref(), q.version.as_deref()) {
        return Ok(SnapshotId::new(world, version));
    }
    if let Some(work_id) = q.work_id.as_deref() {
        let binding = load_binding_at(root, work_id).ok_or((
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
    let binding = state.detamu.load_binding(&work_id).ok_or((
        axum::http::StatusCode::NOT_FOUND,
        format!("no Detamu binding for work {work_id}"),
    ))?;
    Ok(Json(serde_json::to_value(binding).unwrap_or(Value::Null)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn dormant_handle_does_not_open_kv() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("detamu");
        let handle = DetamuHandle::dormant(root.clone());
        assert!(handle.peek().is_none());
        assert!(handle.bindings_dir().is_dir());
        assert!(!handle.store_path().exists());
        let status = handle.status_json().await;
        assert_eq!(status["available"], true);
        assert_eq!(status["loaded"], false);
        assert_eq!(status["wal_bytes"], 0);
        assert_eq!(status["sst_bytes"], 0);
    }

    #[tokio::test]
    async fn get_or_open_is_single_flight() {
        let dir = tempfile::tempdir().expect("tempdir");
        let handle = DetamuHandle::dormant(dir.path().join("detamu"));
        assert!(handle.peek().is_none());
        let a = handle.clone();
        let b = handle.clone();
        let (left, right) = tokio::join!(a.get_or_open(), b.get_or_open());
        let left = left.expect("open left");
        let right = right.expect("open right");
        assert!(Arc::ptr_eq(&left, &right));
        assert!(handle.peek().is_some());
    }

    #[tokio::test]
    async fn reset_store_renames_kv_and_drops_cache() {
        let dir = tempfile::tempdir().expect("tempdir");
        let handle = DetamuHandle::dormant(dir.path().join("detamu"));
        let store = handle.store_path();
        std::fs::create_dir_all(store.join("wal")).expect("wal");
        std::fs::write(store.join("wal").join("000.wal"), vec![0u8; 16]).expect("wal file");
        let body = handle.reset_store().await.expect("reset");
        assert_eq!(body["ok"], true);
        assert!(!store.exists());
        assert!(handle.peek().is_none());
        let renamed = body["renamed_to"].as_str().expect("renamed_to");
        assert!(Path::new(renamed).exists());
    }

    #[test]
    fn flexible_field_repair_overwrites_payload_fields() {
        assert!(DETAMU_FLEXIBLE_FIELD_REPAIR.contains(
            "DEFINE FIELD OVERWRITE provenance ON TABLE detamu_snapshot TYPE array<object> FLEXIBLE"
        ));
        assert!(DETAMU_FLEXIBLE_FIELD_REPAIR.contains(
            "DEFINE FIELD OVERWRITE diagnostics ON TABLE detamu_snapshot TYPE array<object> FLEXIBLE"
        ));
        assert!(DETAMU_FLEXIBLE_FIELD_REPAIR.contains(
            "DEFINE FIELD OVERWRITE payload ON TABLE detamu_entity_observation TYPE object FLEXIBLE"
        ));
        assert!(DETAMU_FLEXIBLE_FIELD_REPAIR.contains(
            "DEFINE FIELD OVERWRITE payload ON TABLE detamu_relation_observation TYPE object FLEXIBLE"
        ));
    }

    #[test]
    fn wal_hint_when_wal_dwarfs_sst() {
        assert!(wal_growth_hint(65 * 1024 * 1024, 5 * 1024 * 1024).is_some());
        assert!(wal_growth_hint(1024, 0).is_none());
    }

    #[test]
    fn store_disk_stats_sum_wal_and_sst() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("detamu");
        let store = detamu_store_path(&root);
        std::fs::create_dir_all(store.join("wal")).unwrap();
        std::fs::create_dir_all(store.join("sst")).unwrap();
        std::fs::write(store.join("wal").join("a.wal"), vec![0u8; 40]).unwrap();
        std::fs::write(store.join("sst").join("a.sst"), vec![0u8; 7]).unwrap();
        assert_eq!(store_disk_stats(&root), (40, 7));
    }
}
