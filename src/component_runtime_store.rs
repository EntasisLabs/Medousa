//! Engine-owned ring buffer for presentation artifact runtime logs.

use std::path::PathBuf;
use std::sync::Arc;

use chrono::{Duration, Utc};
use medousa_types::component_runtime::{
    ComponentRuntimeEvent, ComponentRuntimeEventInput, ComponentRuntimeProbeResult,
    ComponentRuntimeProbeStatus, DEFAULT_RUNTIME_EVENT_TAIL_LIMIT, MAX_RUNTIME_EVENTS_PER_COMPONENT,
    RUNTIME_EVENT_RETENTION_HOURS,
};
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use stasis::prelude::RuntimeComposition;
use surrealdb::engine::any::Any;
use surrealdb::Surreal;
use surrealdb_types::SurrealValue;
use tokio::sync::{broadcast, RwLock};

const RUNTIME_EVENT_TABLE: &str = "component_runtime_event";
const FILE_STORE_DIR: &str = "component_runtime";

const RUNTIME_EVENT_SCHEMA: &[&str] = &[
    "DEFINE TABLE component_runtime_event SCHEMAFULL",
    "DEFINE FIELD profile_id ON TABLE component_runtime_event TYPE string",
    "DEFINE FIELD component_id ON TABLE component_runtime_event TYPE string",
    "DEFINE FIELD session_id ON TABLE component_runtime_event TYPE option<string>",
    "DEFINE FIELD level ON TABLE component_runtime_event TYPE string",
    "DEFINE FIELD message ON TABLE component_runtime_event TYPE string",
    "DEFINE FIELD stack ON TABLE component_runtime_event TYPE option<string>",
    "DEFINE FIELD source ON TABLE component_runtime_event TYPE option<string>",
    "DEFINE FIELD emitted_at ON TABLE component_runtime_event TYPE datetime",
    "DEFINE INDEX idx_component_runtime_scope ON TABLE component_runtime_event COLUMNS profile_id, component_id, emitted_at",
];

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
struct RuntimeEventRecord {
    profile_id: String,
    component_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
    level: String,
    message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stack: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source: Option<String>,
    emitted_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct FileRuntimeLogDocument {
    #[serde(default)]
    events: Vec<ComponentRuntimeEvent>,
    #[serde(default)]
    next_seq: u64,
}

#[derive(Debug, Clone)]
pub struct PendingProbe {
    pub probe_id: String,
    pub profile_id: String,
    pub component_id: String,
    pub created_at: chrono::DateTime<Utc>,
}

pub struct ComponentRuntimeHub {
    db: Option<Surreal<Any>>,
    pending_probes: RwLock<std::collections::HashMap<String, PendingProbe>>,
    probe_results: RwLock<std::collections::HashMap<String, ComponentRuntimeProbeResult>>,
    probe_tx: broadcast::Sender<medousa_types::component_runtime::ComponentRuntimeProbeRequest>,
}

static RUNTIME_HUB: OnceCell<Arc<ComponentRuntimeHub>> = OnceCell::new();

pub async fn init_component_runtime_with_runtime(runtime: &RuntimeComposition) {
    let hub = Arc::new(ComponentRuntimeHub::new(runtime));
    if let Err(err) = hub.ensure_schema().await {
        eprintln!("Component runtime schema init error: {err}");
    }
    let _ = RUNTIME_HUB.set(hub);
    eprintln!("Component runtime hub (artifact doctor logs) initialized");
}

pub fn component_runtime_hub() -> Arc<ComponentRuntimeHub> {
    RUNTIME_HUB
        .get()
        .cloned()
        .unwrap_or_else(|| Arc::new(ComponentRuntimeHub::new_without_db()))
}

impl ComponentRuntimeHub {
    pub fn new(runtime: &RuntimeComposition) -> Self {
        let db = match runtime {
            RuntimeComposition::Surreal(rt) => Some(rt.job_store.db()),
            _ => None,
        };
        let (probe_tx, _) = broadcast::channel(32);
        Self {
            db,
            pending_probes: RwLock::new(std::collections::HashMap::new()),
            probe_results: RwLock::new(std::collections::HashMap::new()),
            probe_tx,
        }
    }

    fn new_without_db() -> Self {
        let (probe_tx, _) = broadcast::channel(32);
        Self {
            db: None,
            pending_probes: RwLock::new(std::collections::HashMap::new()),
            probe_results: RwLock::new(std::collections::HashMap::new()),
            probe_tx,
        }
    }

    pub fn subscribe_probes(
        &self,
    ) -> broadcast::Receiver<medousa_types::component_runtime::ComponentRuntimeProbeRequest> {
        self.probe_tx.subscribe()
    }

    pub async fn ensure_schema(&self) -> Result<(), surrealdb::Error> {
        let Some(db) = &self.db else {
            return Ok(());
        };
        for statement in RUNTIME_EVENT_SCHEMA {
            if let Err(err) = db.query(*statement).await {
                let text = err.to_string();
                if !(text.contains("already exists")
                    || text.contains("already defined")
                    || text.contains("Overwrite index"))
                {
                    return Err(err);
                }
            }
        }
        Ok(())
    }

    pub async fn append_events(
        &self,
        profile_id: &str,
        component_id: &str,
        session_id: Option<&str>,
        inputs: &[ComponentRuntimeEventInput],
    ) -> Result<usize, String> {
        if inputs.is_empty() {
            return Ok(0);
        }
        let accepted = if let Some(db) = &self.db {
            self.surreal_append(db, profile_id, component_id, session_id, inputs)
                .await?
        } else {
            self.file_append(profile_id, component_id, session_id, inputs)
                .await?
        };
        let _ = self.prune_old(profile_id, component_id).await;
        Ok(accepted)
    }

    pub async fn tail(
        &self,
        profile_id: &str,
        component_id: &str,
        limit: usize,
    ) -> Result<Vec<ComponentRuntimeEvent>, String> {
        let limit = limit.clamp(1, MAX_RUNTIME_EVENTS_PER_COMPONENT);
        if let Some(db) = &self.db {
            self.surreal_tail(db, profile_id, component_id, limit).await
        } else {
            self.file_tail(profile_id, component_id, limit).await
        }
    }

    pub async fn register_probe(
        &self,
        profile_id: &str,
        component_id: &str,
    ) -> medousa_types::component_runtime::ComponentRuntimeProbeRequest {
        let probe_id = format!(
            "probe-{}-{}",
            component_id,
            Utc::now().timestamp_millis()
        );
        let request = medousa_types::component_runtime::ComponentRuntimeProbeRequest {
            probe_id: probe_id.clone(),
            component_id: component_id.to_string(),
            profile_id: Some(profile_id.to_string()),
        };
        self.pending_probes.write().await.insert(
            probe_id.clone(),
            PendingProbe {
                probe_id,
                profile_id: profile_id.to_string(),
                component_id: component_id.to_string(),
                created_at: Utc::now(),
            },
        );
        let _ = self.probe_tx.send(request.clone());
        crate::environment_store::environment_hub()
            .emit_stream_event(medousa_types::environment::EnvironmentStreamEvent {
                revision: 0,
                event_type: "runtime_probe".to_string(),
                emitted_at_utc: Utc::now(),
                spec: None,
                component_patches: None,
                feed_event: None,
                runtime_probe: Some(request.clone()),
            })
            .await;
        request
    }

    pub async fn complete_probe(&self, result: ComponentRuntimeProbeResult) {
        let probe_id = result.probe_id.clone();
        self.probe_results
            .write()
            .await
            .insert(probe_id.clone(), result);
        self.pending_probes.write().await.remove(&probe_id);
    }

    pub async fn wait_for_probe(
        &self,
        probe_id: &str,
        timeout_ms: u64,
    ) -> (ComponentRuntimeProbeStatus, Option<ComponentRuntimeProbeResult>) {
        let deadline =
            tokio::time::Instant::now() + tokio::time::Duration::from_millis(timeout_ms);
        loop {
            if let Some(result) = self.probe_results.read().await.get(probe_id).cloned() {
                return (ComponentRuntimeProbeStatus::Ok, Some(result));
            }
            if !self.pending_probes.read().await.contains_key(probe_id) {
                return (ComponentRuntimeProbeStatus::Unavailable, None);
            }
            if tokio::time::Instant::now() >= deadline {
                self.pending_probes.write().await.remove(probe_id);
                return (ComponentRuntimeProbeStatus::TimedOut, None);
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    }

    async fn surreal_append(
        &self,
        db: &Surreal<Any>,
        profile_id: &str,
        component_id: &str,
        session_id: Option<&str>,
        inputs: &[ComponentRuntimeEventInput],
    ) -> Result<usize, String> {
        let mut accepted = 0usize;
        for input in inputs {
            let emitted_at = input.emitted_at_utc.unwrap_or_else(Utc::now);
            let record = RuntimeEventRecord {
                profile_id: profile_id.to_string(),
                component_id: component_id.to_string(),
                session_id: input
                    .session_id
                    .clone()
                    .or_else(|| session_id.map(str::to_string)),
                level: input.level.clone(),
                message: truncate_message(&input.message),
                stack: input.stack.as_ref().map(|s| truncate_message(s)),
                source: input.source.clone(),
                emitted_at,
            };
            let id = format!(
                "{}__{}__{}",
                sanitize_segment(profile_id),
                sanitize_segment(component_id),
                emitted_at.timestamp_millis()
            );
            db.query("CREATE type::record($table, $id) CONTENT $data")
                .bind(("table", RUNTIME_EVENT_TABLE))
                .bind(("id", id))
                .bind(("data", record))
                .await
                .map_err(|err| err.to_string())?;
            accepted += 1;
        }
        Ok(accepted)
    }

    async fn surreal_tail(
        &self,
        db: &Surreal<Any>,
        profile_id: &str,
        component_id: &str,
        limit: usize,
    ) -> Result<Vec<ComponentRuntimeEvent>, String> {
        let mut response = db
            .query(
                "SELECT profile_id, component_id, session_id, level, message, stack, source, emitted_at \
                 FROM type::table($table) \
                 WHERE profile_id = $profile_id AND component_id = $component_id \
                 ORDER BY emitted_at DESC LIMIT $limit",
            )
            .bind(("table", RUNTIME_EVENT_TABLE))
            .bind(("profile_id", profile_id.to_string()))
            .bind(("component_id", component_id.to_string()))
            .bind(("limit", limit))
            .await
            .map_err(|err| err.to_string())?;
        let rows: Vec<RuntimeEventRecord> = response.take(0).unwrap_or_default();
        let mut events: Vec<ComponentRuntimeEvent> = rows
            .into_iter()
            .enumerate()
            .map(|(index, row)| ComponentRuntimeEvent {
                id: format!("{component_id}-{index}"),
                profile_id: row.profile_id,
                component_id: row.component_id,
                session_id: row.session_id,
                level: row.level,
                message: row.message,
                stack: row.stack,
                source: row.source,
                emitted_at_utc: row.emitted_at,
            })
            .collect();
        events.reverse();
        Ok(events)
    }

    async fn prune_old(&self, profile_id: &str, component_id: &str) -> Result<(), String> {
        let cutoff = Utc::now() - Duration::hours(RUNTIME_EVENT_RETENTION_HOURS);
        if let Some(db) = &self.db {
            db.query(
                "DELETE FROM type::table($table) \
                 WHERE profile_id = $profile_id AND component_id = $component_id AND emitted_at < $cutoff",
            )
            .bind(("table", RUNTIME_EVENT_TABLE))
            .bind(("profile_id", profile_id.to_string()))
            .bind(("component_id", component_id.to_string()))
            .bind(("cutoff", cutoff))
            .await
            .map_err(|err| err.to_string())?;
            let tail = self
                .surreal_tail(db, profile_id, component_id, MAX_RUNTIME_EVENTS_PER_COMPONENT + 1)
                .await?;
            if tail.len() > MAX_RUNTIME_EVENTS_PER_COMPONENT
                && let Some(oldest) = tail.first() {
                    db.query(
                        "DELETE FROM type::table($table) \
                         WHERE profile_id = $profile_id AND component_id = $component_id AND emitted_at <= $cutoff",
                    )
                    .bind(("table", RUNTIME_EVENT_TABLE))
                    .bind(("profile_id", profile_id.to_string()))
                    .bind(("component_id", component_id.to_string()))
                    .bind(("cutoff", oldest.emitted_at_utc))
                    .await
                    .map_err(|err| err.to_string())?;
                }
        } else {
            let mut doc = self.read_file_doc(profile_id, component_id).await?;
            doc.events.retain(|event| event.emitted_at_utc >= cutoff);
            while doc.events.len() > MAX_RUNTIME_EVENTS_PER_COMPONENT {
                doc.events.remove(0);
            }
            self.write_file_doc(profile_id, component_id, &doc).await?;
        }
        Ok(())
    }

    async fn file_append(
        &self,
        profile_id: &str,
        component_id: &str,
        session_id: Option<&str>,
        inputs: &[ComponentRuntimeEventInput],
    ) -> Result<usize, String> {
        let mut doc = self.read_file_doc(profile_id, component_id).await?;
        let mut accepted = 0usize;
        for input in inputs {
            doc.next_seq += 1;
            doc.events.push(ComponentRuntimeEvent {
                id: format!("{component_id}-{}", doc.next_seq),
                profile_id: profile_id.to_string(),
                component_id: component_id.to_string(),
                session_id: input
                    .session_id
                    .clone()
                    .or_else(|| session_id.map(str::to_string)),
                level: input.level.clone(),
                message: truncate_message(&input.message),
                stack: input.stack.as_ref().map(|s| truncate_message(s)),
                source: input.source.clone(),
                emitted_at_utc: input.emitted_at_utc.unwrap_or_else(Utc::now),
            });
            accepted += 1;
        }
        while doc.events.len() > MAX_RUNTIME_EVENTS_PER_COMPONENT {
            doc.events.remove(0);
        }
        self.write_file_doc(profile_id, component_id, &doc).await?;
        Ok(accepted)
    }

    async fn file_tail(
        &self,
        profile_id: &str,
        component_id: &str,
        limit: usize,
    ) -> Result<Vec<ComponentRuntimeEvent>, String> {
        let doc = self.read_file_doc(profile_id, component_id).await?;
        let start = doc.events.len().saturating_sub(limit);
        Ok(doc.events[start..].to_vec())
    }

    async fn read_file_doc(
        &self,
        profile_id: &str,
        component_id: &str,
    ) -> Result<FileRuntimeLogDocument, String> {
        let path = file_doc_path(profile_id, component_id);
        if !path.exists() {
            return Ok(FileRuntimeLogDocument::default());
        }
        let raw = tokio::fs::read_to_string(&path)
            .await
            .map_err(|err| err.to_string())?;
        Ok(serde_json::from_str(&raw).unwrap_or_default())
    }

    async fn write_file_doc(
        &self,
        profile_id: &str,
        component_id: &str,
        doc: &FileRuntimeLogDocument,
    ) -> Result<(), String> {
        let path = file_doc_path(profile_id, component_id);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|err| err.to_string())?;
        }
        let raw = serde_json::to_string_pretty(doc).map_err(|err| err.to_string())?;
        tokio::fs::write(path, raw)
            .await
            .map_err(|err| err.to_string())
    }
}

fn truncate_message(message: &str) -> String {
    const MAX: usize = 4_096;
    if message.chars().count() <= MAX {
        return message.to_string();
    }
    message.chars().take(MAX).collect()
}

fn sanitize_segment(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn file_store_root() -> PathBuf {
    if let Ok(raw) = std::env::var("MEDOUSA_COMPONENT_RUNTIME_ROOT") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    crate::paths::medousa_data_dir().join(FILE_STORE_DIR)
}

fn file_doc_path(profile_id: &str, component_id: &str) -> PathBuf {
    file_store_root()
        .join(sanitize_segment(profile_id))
        .join(format!("{}.json", sanitize_segment(component_id)))
}

pub fn default_tail_limit(limit: Option<usize>) -> usize {
    limit
        .unwrap_or(DEFAULT_RUNTIME_EVENT_TAIL_LIMIT)
        .clamp(1, MAX_RUNTIME_EVENTS_PER_COMPONENT)
}
