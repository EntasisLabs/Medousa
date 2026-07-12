//! Append-only workspace feed + revision persistence.

use std::collections::HashMap;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::{Mutex, RwLock};

use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

use crate::daemon_api::{WorkBoardColumn, WorkCardAssociations, WorkspaceEvent};
use crate::session;
use crate::workspace::persist::{
    queue_append_feed_line, queue_snapshot_associations, queue_snapshot_card_states,
    queue_write_revision,
};

const FEED_FILE: &str = "workspace/feed.jsonl";
const REVISION_FILE: &str = "workspace/revision";
const CARD_STATE_FILE: &str = "workspace/card_states.json";
const ASSOC_FILE: &str = "workspace/associations.json";

static STORE: Lazy<WorkspaceStore> = Lazy::new(WorkspaceStore::new);

pub fn workspace_store() -> &'static WorkspaceStore {
    &STORE
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct CardStateSnapshot {
    columns: HashMap<String, WorkBoardColumn>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct AssociationRecord {
    pub card_id: String,
    #[serde(default)]
    pub vault_paths: Vec<String>,
    #[serde(default)]
    pub artifact_ids: Vec<String>,
    #[serde(default)]
    pub locus_node_ids: Vec<String>,
}

pub struct WorkspaceStore {
    revision: Mutex<u64>,
    feed: Mutex<Vec<WorkspaceEvent>>,
    card_states: RwLock<CardStateSnapshot>,
    associations: RwLock<HashMap<String, WorkCardAssociations>>,
}

impl WorkspaceStore {
    fn new() -> Self {
        let store = Self {
            revision: Mutex::new(0),
            feed: Mutex::new(Vec::new()),
            card_states: RwLock::new(CardStateSnapshot::default()),
            associations: RwLock::new(HashMap::new()),
        };
        store.reload_from_disk();
        store
    }

    fn workspace_dir() -> PathBuf {
        session::medousa_data_dir().join("workspace")
    }

    fn path(relative: &str) -> PathBuf {
        Self::workspace_dir().join(relative.strip_prefix("workspace/").unwrap_or(relative))
    }

    fn reload_from_disk(&self) {
        let _ = std::fs::create_dir_all(Self::workspace_dir());

        if let Ok(raw) = std::fs::read_to_string(Self::path(REVISION_FILE))
            && let Ok(value) = raw.trim().parse::<u64>() {
                *self.revision.lock().expect("revision") = value;
            }

        let mut events = Vec::new();
        if let Ok(file) = File::open(Self::path(FEED_FILE)) {
            for line in BufReader::new(file).lines().map_while(Result::ok) {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if let Ok(event) = serde_json::from_str::<WorkspaceEvent>(trimmed) {
                    events.push(event);
                }
            }
        }
        *self.feed.lock().expect("feed") = events;

        if let Ok(raw) = std::fs::read_to_string(Self::path(CARD_STATE_FILE))
            && let Ok(snapshot) = serde_json::from_str::<CardStateSnapshot>(&raw) {
                *self.card_states.write().expect("card states") = snapshot;
            }

        if let Ok(raw) = std::fs::read_to_string(Self::path(ASSOC_FILE))
            && let Ok(rows) = serde_json::from_str::<Vec<AssociationRecord>>(&raw) {
                let mut map = HashMap::new();
                for row in rows {
                    map.insert(
                        row.card_id.clone(),
                        WorkCardAssociations {
                            vault_paths: row.vault_paths,
                            artifact_ids: row.artifact_ids,
                            locus_node_ids: row.locus_node_ids,
                        },
                    );
                }
                *self.associations.write().expect("associations") = map;
            }
    }

    pub fn revision(&self) -> u64 {
        *self.revision.lock().expect("revision")
    }

    pub fn bump_revision(&self) -> u64 {
        let mut guard = self.revision.lock().expect("revision");
        *guard = guard.saturating_add(1);
        let value = *guard;
        queue_write_revision(value);
        value
    }

    pub fn append_event(&self, event: WorkspaceEvent) -> u64 {
        {
            let mut feed = self.feed.lock().expect("feed");
            feed.push(event.clone());
            if let Ok(line) = serde_json::to_string(&event) {
                queue_append_feed_line(line);
            }
        }
        self.bump_revision()
    }

    pub fn list_feed(
        &self,
        since_id: Option<&str>,
        since_revision: Option<u64>,
        limit: usize,
    ) -> Vec<WorkspaceEvent> {
        let feed = self.feed.lock().expect("feed");
        let current_revision = self.revision();
        if since_revision.is_some_and(|value| value >= current_revision) {
            return Vec::new();
        }

        let slice = if let Some(marker) = since_id {
            let start = feed
                .iter()
                .position(|event| event.id == marker)
                .map(|index| index + 1)
                .unwrap_or(0);
            feed[start..].to_vec()
        } else {
            feed.clone()
        };

        if slice.len() > limit {
            slice[slice.len() - limit..].to_vec()
        } else {
            slice
        }
    }

    pub fn feed_tail(&self, limit: usize) -> Vec<WorkspaceEvent> {
        let feed = self.feed.lock().expect("feed");
        let start = feed.len().saturating_sub(limit);
        feed[start..].to_vec()
    }

    pub fn feed_len(&self) -> usize {
        self.feed.lock().expect("feed").len()
    }

    pub fn feed_events_from(&self, index: usize) -> Vec<WorkspaceEvent> {
        let feed = self.feed.lock().expect("feed");
        if index >= feed.len() {
            Vec::new()
        } else {
            feed[index..].to_vec()
        }
    }

    pub fn card_states_snapshot(&self) -> HashMap<String, WorkBoardColumn> {
        self.card_states
            .read()
            .expect("card states")
            .columns
            .clone()
    }

    pub fn previous_column(&self, card_id: &str) -> Option<WorkBoardColumn> {
        self.card_states
            .read()
            .expect("card states")
            .columns
            .get(card_id)
            .copied()
    }

    pub fn remember_column(&self, card_id: &str, column: WorkBoardColumn) {
        self.card_states
            .write()
            .expect("card states")
            .columns
            .insert(card_id.to_string(), column);
        self.persist_card_states();
    }

    pub fn prune_card_state(&self, card_id: &str) {
        self.card_states
            .write()
            .expect("card states")
            .columns
            .remove(card_id);
        self.persist_card_states();
    }

    fn persist_card_states(&self) {
        let snapshot = self.card_states.read().expect("card states").clone();
        if let Ok(raw) = serde_json::to_string_pretty(&snapshot) {
            queue_snapshot_card_states(raw);
        }
    }

    pub fn associations(&self, card_id: &str) -> WorkCardAssociations {
        self.associations
            .read()
            .expect("associations")
            .get(card_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn set_vault_association(&self, card_id: &str, vault_path: String) {
        let mut guard = self.associations.write().expect("associations");
        let entry = guard.entry(card_id.to_string()).or_default();
        if !entry.vault_paths.iter().any(|path| path == &vault_path) {
            entry.vault_paths.push(vault_path);
        }
        Self::persist_associations(&guard);
    }

    fn persist_associations(map: &HashMap<String, WorkCardAssociations>) {
        let rows = map
            .iter()
            .map(|(card_id, assoc)| AssociationRecord {
                card_id: card_id.clone(),
                vault_paths: assoc.vault_paths.clone(),
                artifact_ids: assoc.artifact_ids.clone(),
                locus_node_ids: assoc.locus_node_ids.clone(),
            })
            .collect::<Vec<_>>();
        if let Ok(raw) = serde_json::to_string_pretty(&rows) {
            queue_snapshot_associations(raw);
        }
    }

    pub fn prune_feed_older_than(&self, cutoff: DateTime<Utc>) {
        let mut feed = self.feed.lock().expect("feed");
        feed.retain(|event| event.timestamp_utc >= cutoff);
    }
}
