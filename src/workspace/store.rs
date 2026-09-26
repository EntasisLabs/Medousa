//! Append-only workspace feed + revision persistence.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, RwLock};

use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;

use crate::daemon_api::{WorkBoardColumn, WorkCardAssociations, WorkspaceEvent};
use crate::persistence::PersistenceError;
#[cfg(test)]
use crate::persistence::PersistenceErrorKind;
use crate::workspace::persist::{WorkspaceMutation, queue_mutations, startup_projection};
const MAX_FEED_EVENTS: usize = 4_096;
const MAX_FEED_BYTES: usize = 8 * 1024 * 1024;

static STORE: Lazy<WorkspaceStore> = Lazy::new(WorkspaceStore::new);

pub fn workspace_store() -> &'static WorkspaceStore {
    &STORE
}

pub struct WorkspaceStore {
    submit_mutations:
        Arc<dyn Fn(Vec<WorkspaceMutation>) -> Result<(), PersistenceError> + Send + Sync>,
    revision: Mutex<u64>,
    feed: Mutex<VecDeque<WorkspaceEvent>>,
    feed_bytes: Mutex<usize>,
    card_states: RwLock<HashMap<String, WorkBoardColumn>>,
    associations: RwLock<HashMap<String, WorkCardAssociations>>,
}

impl WorkspaceStore {
    fn new() -> Self {
        let store = Self {
            submit_mutations: Arc::new(queue_mutations),
            revision: Mutex::new(0),
            feed: Mutex::new(VecDeque::new()),
            feed_bytes: Mutex::new(0),
            card_states: RwLock::new(HashMap::new()),
            associations: RwLock::new(HashMap::new()),
        };
        store.reload_from_disk();
        store
    }

    fn reload_from_disk(&self) {
        if let Some(projection) = startup_projection() {
            let feed_bytes = projection
                .feed
                .iter()
                .map(|event| serde_json::to_vec(event).map_or(0, |bytes| bytes.len()))
                .sum();
            *self.revision.lock().expect("revision") = projection.revision;
            *self.feed.lock().expect("feed") = projection.feed;
            *self.feed_bytes.lock().expect("feed bytes") = feed_bytes;
            *self.card_states.write().expect("card states") = projection.card_columns;
            *self.associations.write().expect("associations") = projection.associations;
        }
    }

    pub fn revision(&self) -> u64 {
        *self.revision.lock().expect("revision")
    }

    pub fn bump_revision(&self) -> Result<u64, PersistenceError> {
        let mut guard = self.revision.lock().expect("revision");
        let value = guard.saturating_add(1);
        (self.submit_mutations)(vec![WorkspaceMutation::SetRevision { revision: value }])?;
        *guard = value;
        Ok(value)
    }

    pub fn append_event(&self, event: WorkspaceEvent) -> Result<u64, PersistenceError> {
        let event_bytes = serde_json::to_vec(&event).map_or(0, |bytes| bytes.len());
        let mut feed = self.feed.lock().expect("feed");
        let mut feed_bytes = self.feed_bytes.lock().expect("feed bytes");
        let mut revision = self.revision.lock().expect("revision");
        let value = revision.saturating_add(1);
        (self.submit_mutations)(vec![WorkspaceMutation::AppendEventAndRevision {
            event: event.clone(),
            revision: value,
        }])?;
        retain_feed_event(&mut feed, &mut feed_bytes, event, event_bytes);
        *revision = value;
        Ok(value)
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

        let mut slice = if let Some(marker) = since_id {
            let start = feed
                .iter()
                .position(|event| event.id == marker)
                .map(|index| index + 1)
                .unwrap_or(0);
            feed.iter().skip(start).cloned().collect::<Vec<_>>()
        } else {
            feed.iter().cloned().collect::<Vec<_>>()
        };

        if slice.len() > limit {
            slice.drain(..slice.len() - limit);
        }
        slice
    }

    pub fn feed_tail(&self, limit: usize) -> Vec<WorkspaceEvent> {
        let feed = self.feed.lock().expect("feed");
        let start = feed.len().saturating_sub(limit);
        feed.iter().skip(start).cloned().collect()
    }

    pub fn feed_len(&self) -> usize {
        self.feed.lock().expect("feed").len()
    }

    pub fn feed_events_from(&self, index: usize) -> Vec<WorkspaceEvent> {
        let feed = self.feed.lock().expect("feed");
        if index >= feed.len() {
            Vec::new()
        } else {
            feed.iter().skip(index).cloned().collect()
        }
    }

    pub fn card_states_snapshot(&self) -> HashMap<String, WorkBoardColumn> {
        self.card_states.read().expect("card states").clone()
    }

    pub fn previous_column(&self, card_id: &str) -> Option<WorkBoardColumn> {
        self.card_states
            .read()
            .expect("card states")
            .get(card_id)
            .copied()
    }

    pub fn remember_column(
        &self,
        card_id: &str,
        column: WorkBoardColumn,
    ) -> Result<(), PersistenceError> {
        let mut card_states = self.card_states.write().expect("card states");
        (self.submit_mutations)(vec![WorkspaceMutation::SetCardColumn {
            card_id: card_id.to_string(),
            column: Some(column),
        }])?;
        card_states.insert(card_id.to_string(), column);
        Ok(())
    }

    pub fn prune_card_state(&self, card_id: &str) -> Result<(), PersistenceError> {
        let mut card_states = self.card_states.write().expect("card states");
        (self.submit_mutations)(vec![WorkspaceMutation::SetCardColumn {
            card_id: card_id.to_string(),
            column: None,
        }])?;
        card_states.remove(card_id);
        Ok(())
    }

    pub fn associations(&self, card_id: &str) -> WorkCardAssociations {
        self.associations
            .read()
            .expect("associations")
            .get(card_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn set_vault_association(
        &self,
        card_id: &str,
        vault_path: String,
    ) -> Result<(), PersistenceError> {
        let mut guard = self.associations.write().expect("associations");
        let mut association = guard.get(card_id).cloned().unwrap_or_default();
        if !association
            .vault_paths
            .iter()
            .any(|path| path == &vault_path)
        {
            association.vault_paths.push(vault_path);
        }
        (self.submit_mutations)(vec![WorkspaceMutation::SetAssociation {
            card_id: card_id.to_string(),
            association: association.clone(),
        }])?;
        guard.insert(card_id.to_string(), association);
        Ok(())
    }

    /// Admit and publish an association together with its optional feed event
    /// in one journal record. The caller flushes before claiming durability.
    pub fn link_vault_association(
        &self,
        card_id: &str,
        vault_path: String,
        event: Option<WorkspaceEvent>,
    ) -> Result<u64, PersistenceError> {
        let mut associations = self.associations.write().expect("associations");
        let mut association = associations.get(card_id).cloned().unwrap_or_default();
        if !association
            .vault_paths
            .iter()
            .any(|path| path == &vault_path)
        {
            association.vault_paths.push(vault_path);
        }

        let mut feed = self.feed.lock().expect("feed");
        let mut feed_bytes = self.feed_bytes.lock().expect("feed bytes");
        let mut revision = self.revision.lock().expect("revision");
        let next_revision = revision.saturating_add(1);
        let mut mutations = vec![WorkspaceMutation::SetAssociation {
            card_id: card_id.to_string(),
            association: association.clone(),
        }];
        let event_bytes = event
            .as_ref()
            .map(|event| serde_json::to_vec(event).map_or(0, |bytes| bytes.len()));
        mutations.push(if let Some(event) = &event {
            WorkspaceMutation::AppendEventAndRevision {
                event: event.clone(),
                revision: next_revision,
            }
        } else {
            WorkspaceMutation::SetRevision {
                revision: next_revision,
            }
        });
        (self.submit_mutations)(mutations)?;

        associations.insert(card_id.to_string(), association);
        if let (Some(event), Some(event_bytes)) = (event, event_bytes) {
            retain_feed_event(&mut feed, &mut feed_bytes, event, event_bytes);
        }
        *revision = next_revision;
        Ok(next_revision)
    }

    /// Atomically admit a derived column update with its transition event.
    pub fn record_column_transition(
        &self,
        card_id: &str,
        column: WorkBoardColumn,
        event: Option<WorkspaceEvent>,
    ) -> Result<(), PersistenceError> {
        let mut feed = self.feed.lock().expect("feed");
        let mut feed_bytes = self.feed_bytes.lock().expect("feed bytes");
        let mut revision = self.revision.lock().expect("revision");
        let mut card_states = self.card_states.write().expect("card states");
        let mut mutations = vec![WorkspaceMutation::SetCardColumn {
            card_id: card_id.to_string(),
            column: Some(column),
        }];
        let event_bytes = event
            .as_ref()
            .map(|event| serde_json::to_vec(event).map_or(0, |bytes| bytes.len()));
        let next_revision = revision.saturating_add(1);
        if let Some(event) = &event {
            mutations.push(WorkspaceMutation::AppendEventAndRevision {
                event: event.clone(),
                revision: next_revision,
            });
        }
        (self.submit_mutations)(mutations)?;

        card_states.insert(card_id.to_string(), column);
        if let (Some(event), Some(event_bytes)) = (event, event_bytes) {
            retain_feed_event(&mut feed, &mut feed_bytes, event, event_bytes);
            *revision = next_revision;
        }
        Ok(())
    }

    pub fn prune_feed_older_than(&self, cutoff: DateTime<Utc>) -> Result<(), PersistenceError> {
        let mut feed = self.feed.lock().expect("feed");
        if !feed.iter().any(|event| event.timestamp_utc < cutoff) {
            return Ok(());
        }
        (self.submit_mutations)(vec![WorkspaceMutation::PruneFeedBefore { cutoff }])?;
        feed.retain(|event| event.timestamp_utc >= cutoff);
        *self.feed_bytes.lock().expect("feed bytes") = feed
            .iter()
            .map(|event| serde_json::to_vec(event).map_or(0, |bytes| bytes.len()))
            .sum();
        Ok(())
    }
}

#[cfg(test)]
impl WorkspaceStore {
    fn empty_with_submit(
        submit_mutations: impl Fn(Vec<WorkspaceMutation>) -> Result<(), PersistenceError>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self {
            submit_mutations: Arc::new(submit_mutations),
            revision: Mutex::new(0),
            feed: Mutex::new(VecDeque::new()),
            feed_bytes: Mutex::new(0),
            card_states: RwLock::new(HashMap::new()),
            associations: RwLock::new(HashMap::new()),
        }
    }
}

fn retain_feed_event(
    feed: &mut VecDeque<WorkspaceEvent>,
    feed_bytes: &mut usize,
    event: WorkspaceEvent,
    event_bytes: usize,
) {
    feed.push_back(event);
    *feed_bytes = feed_bytes.saturating_add(event_bytes);
    while feed.len() > MAX_FEED_EVENTS || *feed_bytes > MAX_FEED_BYTES {
        let Some(evicted) = feed.pop_front() else {
            break;
        };
        *feed_bytes =
            feed_bytes.saturating_sub(serde_json::to_vec(&evicted).map_or(0, |bytes| bytes.len()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reject_mutations(_: Vec<WorkspaceMutation>) -> Result<(), PersistenceError> {
        Err(PersistenceError::new(
            PersistenceErrorKind::Overloaded,
            "injected admission failure",
        ))
    }

    #[test]
    fn failed_admission_does_not_publish_workspace_state() {
        let store = WorkspaceStore::empty_with_submit(reject_mutations);
        let event = crate::workspace::event::event_for_vault_link("card-1", "notes/test.md")
            .expect("vault event");

        assert!(store.append_event(event.clone()).is_err());
        assert_eq!(
            store.bump_revision().unwrap_err().to_string(),
            "injected admission failure"
        );
        assert!(
            store
                .set_vault_association("card-1", "notes/test.md".to_string())
                .is_err()
        );
        assert!(
            store
                .link_vault_association("card-1", "notes/test.md".to_string(), Some(event.clone()))
                .is_err()
        );
        assert!(
            store
                .record_column_transition("card-1", WorkBoardColumn::Backlog, Some(event.clone()))
                .is_err()
        );
        assert!(
            store
                .remember_column("card-1", WorkBoardColumn::Backlog)
                .is_err()
        );
        assert!(store.prune_card_state("card-1").is_err());

        let mut old_event = event.clone();
        old_event.timestamp_utc = Utc::now() - chrono::Duration::days(8);
        let old_event_bytes = serde_json::to_vec(&old_event).unwrap().len();
        retain_feed_event(
            &mut store.feed.lock().unwrap(),
            &mut store.feed_bytes.lock().unwrap(),
            old_event,
            old_event_bytes,
        );
        assert!(
            store
                .prune_feed_older_than(Utc::now() - chrono::Duration::days(7))
                .is_err()
        );

        assert_eq!(store.revision(), 0);
        assert_eq!(store.feed_len(), 1);
        assert_eq!(store.previous_column("card-1"), None);
        assert!(store.associations("card-1").vault_paths.is_empty());
    }

    #[test]
    fn link_association_and_event_are_admitted_as_one_batch() {
        let batches = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&batches);
        let store = WorkspaceStore::empty_with_submit(move |mutations| {
            captured.lock().unwrap().push(mutations);
            Ok(())
        });
        let event = crate::workspace::event::event_for_vault_link("card-2", "notes/link.md")
            .expect("vault event");
        let event_id = event.id.clone();

        assert_eq!(
            store
                .link_vault_association("card-2", "notes/link.md".to_string(), Some(event))
                .unwrap(),
            1
        );

        let batches = batches.lock().unwrap();
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].len(), 2);
        assert!(matches!(
            &batches[0][0],
            WorkspaceMutation::SetAssociation { card_id, association }
                if card_id == "card-2"
                    && association.vault_paths.len() == 1
                    && association.vault_paths[0] == "notes/link.md"
        ));
        assert!(matches!(
            &batches[0][1],
            WorkspaceMutation::AppendEventAndRevision { event, revision }
                if event.id == event_id && *revision == 1
        ));
        assert_eq!(
            store.associations("card-2").vault_paths,
            vec!["notes/link.md".to_string()]
        );
        assert_eq!(store.feed_tail(1)[0].id, event_id);
        assert_eq!(store.revision(), 1);
    }
}
