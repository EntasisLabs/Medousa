//! Durable slug-reservation journal and rebuildable listing catalog (H06.4).

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use medousa_store::{
    CommitReceipt, DurabilityLevel, FileTransaction, PersistenceError, PersistenceErrorKind,
    StoreKind, StorePath, StoreRoot,
};
use serde::{Deserialize, Serialize};

use crate::error::{ForgeError, Result};
use crate::model::{WorkId, WorkItem, WorkState};
use crate::owner::ForgeCommitReceipt;

pub const UNPARAMETERIZED_LIST_CAP: usize = 256;
pub const MAX_PAGE_LIMIT: usize = 1_024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SlugReservationState {
    Reserved,
    Committed,
    Released,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlugReservation {
    pub slug: String,
    pub operation_id: String,
    pub work_id: Option<WorkId>,
    pub item_generation: Option<u64>,
    pub state: SlugReservationState,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub work_id: WorkId,
    pub slug: String,
    pub title: String,
    pub state: WorkState,
    pub owner: String,
    pub updated_at: DateTime<Utc>,
    pub active_attempts: usize,
    pub item_generation: u64,
    pub snapshot_seq: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogPage {
    pub items: Vec<CatalogEntry>,
    pub next_cursor: Option<String>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct CatalogSnapshot {
    entries: BTreeMap<String, CatalogEntry>,
}

pub struct SlugReservationJournal {
    tx: FileTransaction,
    path: StorePath,
    reservations: Mutex<BTreeMap<String, SlugReservation>>,
}

impl SlugReservationJournal {
    pub fn open(root: Arc<StoreRoot>) -> Result<Self> {
        let path = StorePath::parse("slug_reservations.jsonl")
            .map_err(|err| ForgeError::Store(err.to_string()))?;
        let tx = FileTransaction::new(root);
        let reservations = load_reservations(tx.root(), &path)?;
        Ok(Self {
            tx,
            path,
            reservations: Mutex::new(reservations),
        })
    }

    pub fn reserve(&self, slug: &str, operation_id: impl Into<String>) -> Result<SlugReservation> {
        let operation_id = operation_id.into();
        let mut reservations = self
            .reservations
            .lock()
            .map_err(|_| ForgeError::Store("slug journal poisoned".into()))?;
        if let Some(existing) = reservations.get(slug) {
            match existing.state {
                SlugReservationState::Released => {}
                SlugReservationState::Reserved
                    if existing.operation_id == operation_id && existing.work_id.is_none() =>
                {
                    return Ok(existing.clone());
                }
                SlugReservationState::Reserved | SlugReservationState::Committed => {
                    return Err(ForgeError::SlugConflict(slug.to_owned()));
                }
            }
        }
        let record = SlugReservation {
            slug: slug.to_owned(),
            operation_id,
            work_id: None,
            item_generation: None,
            state: SlugReservationState::Reserved,
            at: Utc::now(),
        };
        self.append_synced(&record)?;
        reservations.insert(slug.to_owned(), record.clone());
        Ok(record)
    }

    pub fn commit(
        &self,
        slug: &str,
        work_id: WorkId,
        item_generation: u64,
    ) -> Result<SlugReservation> {
        let mut reservations = self
            .reservations
            .lock()
            .map_err(|_| ForgeError::Store("slug journal poisoned".into()))?;
        let Some(current) = reservations.get(slug).cloned() else {
            return Err(ForgeError::Store(format!(
                "slug reservation missing for {slug}"
            )));
        };
        if current.state == SlugReservationState::Committed
            && current.work_id.as_ref() == Some(&work_id)
            && current.item_generation == Some(item_generation)
        {
            return Ok(current);
        }
        if current.state == SlugReservationState::Released {
            return Err(ForgeError::Store(format!(
                "cannot commit released slug {slug}"
            )));
        }
        let record = SlugReservation {
            work_id: Some(work_id),
            item_generation: Some(item_generation),
            state: SlugReservationState::Committed,
            at: Utc::now(),
            ..current
        };
        self.append_synced(&record)?;
        reservations.insert(slug.to_owned(), record.clone());
        Ok(record)
    }

    /// Release a reservation that never became a durable work item.
    /// Committed slugs cannot be released — the item owns the slug.
    pub fn release(&self, slug: &str) -> Result<()> {
        let mut reservations = self
            .reservations
            .lock()
            .map_err(|_| ForgeError::Store("slug journal poisoned".into()))?;
        let Some(current) = reservations.get(slug).cloned() else {
            return Ok(());
        };
        match current.state {
            SlugReservationState::Released => Ok(()),
            SlugReservationState::Committed => Err(ForgeError::Store(format!(
                "refusing to release slug {slug}: work item was durably created"
            ))),
            SlugReservationState::Reserved => {
                let record = SlugReservation {
                    state: SlugReservationState::Released,
                    at: Utc::now(),
                    ..current
                };
                self.append_synced(&record)?;
                reservations.insert(slug.to_owned(), record);
                Ok(())
            }
        }
    }

    pub fn recover_orphans(&self) -> Result<Vec<SlugReservation>> {
        let reservations = self
            .reservations
            .lock()
            .map_err(|_| ForgeError::Store("slug journal poisoned".into()))?;
        Ok(reservations
            .values()
            .filter(|record| {
                record.state == SlugReservationState::Reserved && record.work_id.is_none()
            })
            .cloned()
            .collect())
    }

    pub fn taken_slugs(&self) -> Result<Vec<String>> {
        let reservations = self
            .reservations
            .lock()
            .map_err(|_| ForgeError::Store("slug journal poisoned".into()))?;
        Ok(reservations
            .values()
            .filter(|record| record.state != SlugReservationState::Released)
            .map(|record| record.slug.clone())
            .collect())
    }

    fn append_synced(&self, record: &SlugReservation) -> Result<()> {
        let bytes = serde_json::to_vec(record)?;
        self.tx
            .append_record(&self.path, &bytes, DurabilityLevel::Synced)
            .map_err(persistence_error)?;
        Ok(())
    }
}

pub struct ForgeCatalog {
    tx: FileTransaction,
    path: StorePath,
    entries: Mutex<BTreeMap<String, CatalogEntry>>,
}

impl ForgeCatalog {
    pub fn open(root: Arc<StoreRoot>) -> Result<Self> {
        let path = StorePath::parse("catalog/snapshot.json")
            .map_err(|err| ForgeError::Store(err.to_string()))?;
        let tx = FileTransaction::new(root);
        let snapshot = load_catalog_snapshot(tx.root(), &path)?;
        Ok(Self {
            tx,
            path,
            entries: Mutex::new(snapshot.entries),
        })
    }

    /// Publish a catalog entry only after a durable snapshot commit.
    pub fn publish(&self, item: &WorkItem, receipt: &ForgeCommitReceipt) -> Result<CommitReceipt> {
        let entry = CatalogEntry {
            work_id: item.id.clone(),
            slug: item.slug.clone(),
            title: item.title.clone(),
            state: item.state,
            owner: item.owner.clone(),
            updated_at: item.updated_at,
            active_attempts: item.active_attempt_ids().len(),
            item_generation: receipt.item_generation,
            snapshot_seq: receipt.last_seq,
        };
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| ForgeError::Store("catalog poisoned".into()))?;
        let mut published_entries = entries.clone();
        published_entries.insert(item.id.as_str().to_owned(), entry);
        let bytes = serde_json::to_vec(&CatalogSnapshot {
            entries: published_entries.clone(),
        })?;
        let written = self
            .tx
            .replace_snapshot(&self.path, &bytes, DurabilityLevel::Synced)
            .map_err(persistence_error)?;
        *entries = published_entries;
        Ok(CommitReceipt::new(
            StoreKind::ForgeCatalog,
            item.id.as_str(),
            receipt.item_generation,
            DurabilityLevel::Synced,
            written,
        ))
    }

    /// Drop the disposable in-memory projection after a failed publication.
    /// The next query rebuilds it from authoritative item snapshots/logs.
    pub fn invalidate(&self) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.clear();
        }
    }

    pub fn page(&self, limit: Option<usize>, cursor: Option<&str>) -> Result<CatalogPage> {
        let entries = self
            .entries
            .lock()
            .map_err(|_| ForgeError::Store("catalog poisoned".into()))?;
        let start = cursor
            .and_then(|cursor| {
                entries
                    .keys()
                    .position(|key| key.as_str() > cursor)
                    .or_else(|| entries.keys().position(|key| key == cursor).map(|i| i + 1))
            })
            .unwrap_or(0);
        let requested = limit.unwrap_or(UNPARAMETERIZED_LIST_CAP);
        let cap = requested.min(MAX_PAGE_LIMIT);
        let slice: Vec<CatalogEntry> = entries
            .values()
            .skip(start)
            .take(cap + 1)
            .cloned()
            .collect();
        let truncated = slice.len() > cap;
        let items: Vec<CatalogEntry> = slice.into_iter().take(cap).collect();
        let next_cursor = truncated
            .then(|| items.last().map(|entry| entry.work_id.as_str().to_owned()))
            .flatten();
        Ok(CatalogPage {
            items,
            next_cursor,
            truncated,
        })
    }

    pub fn all_entries(&self) -> Result<Vec<CatalogEntry>> {
        let entries = self
            .entries
            .lock()
            .map_err(|_| ForgeError::Store("catalog poisoned".into()))?;
        Ok(entries.values().cloned().collect())
    }

    pub fn all_capped(&self) -> Result<(Vec<CatalogEntry>, bool)> {
        let page = self.page(Some(UNPARAMETERIZED_LIST_CAP), None)?;
        Ok((page.items, page.truncated))
    }

    pub fn rebuild_from(&self, items: impl IntoIterator<Item = (WorkItem, u64)>) -> Result<()> {
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| ForgeError::Store("catalog poisoned".into()))?;
        entries.clear();
        for (item, generation) in items {
            entries.insert(
                item.id.as_str().to_owned(),
                CatalogEntry {
                    work_id: item.id.clone(),
                    slug: item.slug.clone(),
                    title: item.title.clone(),
                    state: item.state,
                    owner: item.owner.clone(),
                    updated_at: item.updated_at,
                    active_attempts: item.active_attempt_ids().len(),
                    item_generation: generation,
                    snapshot_seq: generation,
                },
            );
        }
        let bytes = serde_json::to_vec(&CatalogSnapshot {
            entries: entries.clone(),
        })?;
        self.tx
            .replace_snapshot(&self.path, &bytes, DurabilityLevel::Synced)
            .map_err(persistence_error)?;
        Ok(())
    }
}

fn load_reservations(
    root: &StoreRoot,
    path: &StorePath,
) -> Result<BTreeMap<String, SlugReservation>> {
    let mut reservations = BTreeMap::new();
    if !root.is_file(path).map_err(store_root_error)? {
        return Ok(reservations);
    }
    let raw = root
        .read_limited(path, 64 * 1024 * 1024)
        .map_err(store_root_error)?;
    let mut offset = 0usize;
    for (idx, line) in BufReader::new(raw.as_slice()).lines().enumerate() {
        let line = line?;
        let line_len = line.len() + 1;
        if line.trim().is_empty() {
            offset += line_len;
            continue;
        }
        match serde_json::from_str::<SlugReservation>(&line) {
            Ok(record) => {
                reservations.insert(record.slug.clone(), record);
                offset += line_len;
            }
            Err(err) => {
                let at_eof = offset + line_len >= raw.len() || offset + line.len() >= raw.len();
                if at_eof && !line.trim_end().ends_with('}') {
                    // Incomplete final journal record only.
                    break;
                }
                return Err(ForgeError::Store(format!(
                    "corrupt slug reservation at line {}: {err}",
                    idx + 1
                )));
            }
        }
    }
    Ok(reservations)
}

fn load_catalog_snapshot(root: &StoreRoot, path: &StorePath) -> Result<CatalogSnapshot> {
    if !root.is_file(path).map_err(store_root_error)? {
        return Ok(CatalogSnapshot::default());
    }
    let raw = root
        .read_limited(path, 64 * 1024 * 1024)
        .map_err(store_root_error)?;
    serde_json::from_slice(&raw)
        .map_err(|err| ForgeError::Store(format!("corrupt catalog snapshot: {err}")))
}

fn persistence_error(err: PersistenceError) -> ForgeError {
    match err.kind {
        PersistenceErrorKind::Overloaded => ForgeError::Overloaded(err.to_string()),
        PersistenceErrorKind::Conflict => ForgeError::Conflict(err.to_string()),
        _ => ForgeError::Store(err.to_string()),
    }
}

fn store_root_error(err: medousa_store::StoreRootError) -> ForgeError {
    ForgeError::Store(err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{GitOid, GitWorkTarget, WorkTarget};
    use crate::owner::ForgeCommitReceipt;
    use medousa_store::CommitReceipt;
    use tempfile::TempDir;

    fn root(tmp: &TempDir) -> Arc<StoreRoot> {
        Arc::new(
            StoreRoot::open_or_create_nofollow(
                &tmp.path()
                    .canonicalize()
                    .unwrap_or_else(|_| tmp.path().to_path_buf()),
            )
            .unwrap(),
        )
    }

    fn item(title: &str) -> WorkItem {
        WorkItem::new(
            title,
            "b",
            WorkTarget::Git(GitWorkTarget {
                repo_path: std::path::PathBuf::from("/tmp/repo"),
                base_ref: "main".into(),
                base_oid: GitOid::new("a".repeat(40)),
            }),
            "user-1",
        )
    }

    fn receipt(item: &WorkItem, generation: u64) -> ForgeCommitReceipt {
        ForgeCommitReceipt {
            work_id: item.id.clone(),
            item_generation: generation,
            first_seq: generation,
            last_seq: generation,
            log_offset: 0,
            durability: DurabilityLevel::Synced,
            operation_generation: None,
            persistence: CommitReceipt::new(
                StoreKind::Forge,
                item.id.as_str(),
                generation,
                DurabilityLevel::Synced,
                0,
            ),
        }
    }

    #[test]
    fn reserve_commit_release_restart_and_orphan_recovery() {
        let tmp = TempDir::new().unwrap();
        {
            let journal = SlugReservationJournal::open(root(&tmp)).unwrap();
            journal.reserve("alpha", "op-1").unwrap();
            assert!(journal.reserve("alpha", "op-2").is_err());
            // Idempotent reserve with the same operation id.
            journal.reserve("alpha", "op-1").unwrap();
            let orphans = journal.recover_orphans().unwrap();
            assert_eq!(orphans.len(), 1);
            journal.release("alpha").unwrap();
            journal.release("alpha").unwrap();
            journal.reserve("alpha", "op-3").unwrap();
            let work = item("alpha");
            journal.commit("alpha", work.id.clone(), 1).unwrap();
            journal.commit("alpha", work.id.clone(), 1).unwrap();
            assert!(journal.release("alpha").is_err());
            assert!(journal.recover_orphans().unwrap().is_empty());
        }
        let journal = reopen_journal(&tmp);
        assert!(journal.taken_slugs().unwrap().contains(&"alpha".into()));
        assert!(journal.recover_orphans().unwrap().is_empty());
    }

    fn reopen_journal(tmp: &TempDir) -> SlugReservationJournal {
        SlugReservationJournal::open(root(tmp)).unwrap()
    }

    #[test]
    fn duplicate_slug_conflict_survives_reopen() {
        let tmp = TempDir::new().unwrap();
        let work = item("held");
        {
            let journal = reopen_journal(&tmp);
            journal.reserve("held", "op-1").unwrap();
            journal.commit("held", work.id.clone(), 1).unwrap();
        }
        let journal = reopen_journal(&tmp);
        assert!(journal.reserve("held", "op-2").is_err());
    }

    #[test]
    fn slug_journal_rejects_middle_corruption() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("slug_reservations.jsonl");
        std::fs::write(&path, b"{\"slug\":\"ok\",\"operation_id\":\"1\",\"work_id\":null,\"item_generation\":null,\"state\":\"reserved\",\"at\":\"2026-01-01T00:00:00Z\"}\nnot-json\n{\"slug\":\"later\",\"operation_id\":\"2\",\"work_id\":null,\"item_generation\":null,\"state\":\"reserved\",\"at\":\"2026-01-01T00:00:00Z\"}\n").unwrap();
        let err = SlugReservationJournal::open(root(&tmp));
        assert!(matches!(err, Err(ForgeError::Store(_))));
    }

    #[test]
    fn catalog_publish_is_durable_and_paginates() {
        let tmp = TempDir::new().unwrap();
        let first = item("one");
        let second = item("two");
        {
            let catalog = ForgeCatalog::open(root(&tmp)).unwrap();
            let published = catalog.publish(&first, &receipt(&first, 1)).unwrap();
            assert_eq!(published.durability, DurabilityLevel::Synced);
            catalog.publish(&second, &receipt(&second, 1)).unwrap();
            let page = catalog.page(Some(1), None).unwrap();
            assert_eq!(page.items.len(), 1);
            assert!(page.truncated);
            assert!(page.next_cursor.is_some());
        }
        let catalog = ForgeCatalog::open(root(&tmp)).unwrap();
        assert_eq!(catalog.all_entries().unwrap().len(), 2);
    }

    #[test]
    fn catalog_corruption_fails_closed_on_open() {
        let tmp = TempDir::new().unwrap();
        let catalog_dir = tmp.path().join("catalog");
        std::fs::create_dir_all(&catalog_dir).unwrap();
        std::fs::write(catalog_dir.join("snapshot.json"), b"{not-json").unwrap();
        let err = ForgeCatalog::open(root(&tmp));
        assert!(matches!(err, Err(ForgeError::Store(_))));
    }

    #[test]
    fn catalog_lists_more_than_256_entries() {
        let tmp = TempDir::new().unwrap();
        let catalog = ForgeCatalog::open(root(&tmp)).unwrap();
        for i in 0..300 {
            let work = item(&format!("item-{i:03}"));
            catalog.publish(&work, &receipt(&work, 1)).unwrap();
        }
        let all = catalog.all_entries().unwrap();
        assert_eq!(all.len(), 300);
        let (capped, truncated) = catalog.all_capped().unwrap();
        assert_eq!(capped.len(), UNPARAMETERIZED_LIST_CAP);
        assert!(truncated);
        let page = catalog.page(Some(10_000), None).unwrap();
        assert_eq!(page.items.len(), 300);
        assert!(!page.truncated);
        let limited = catalog.page(Some(MAX_PAGE_LIMIT), None).unwrap();
        assert_eq!(limited.items.len(), MAX_PAGE_LIMIT.min(300));
    }
}
