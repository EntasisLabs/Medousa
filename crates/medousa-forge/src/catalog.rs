//! Durable slug-reservation journal and rebuildable listing catalog (H06.3).

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use medousa_store::{CommitReceipt, DurabilityLevel, StoreKind};
use serde::{Deserialize, Serialize};

use crate::error::{ForgeError, Result};
use crate::model::{WorkId, WorkItem, WorkState};
use crate::owner::ForgeCommitReceipt;

pub const UNPARAMETERIZED_LIST_CAP: usize = 256;

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

pub struct SlugReservationJournal {
    path: PathBuf,
    reservations: Mutex<BTreeMap<String, SlugReservation>>,
}

impl SlugReservationJournal {
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let path = root.as_ref().join("slug_reservations.jsonl");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let reservations = load_reservations(&path)?;
        Ok(Self {
            path,
            reservations: Mutex::new(reservations),
        })
    }

    pub fn reserve(&self, slug: &str, operation_id: impl Into<String>) -> Result<SlugReservation> {
        let mut reservations = self
            .reservations
            .lock()
            .map_err(|_| ForgeError::Store("slug journal poisoned".into()))?;
        if let Some(existing) = reservations.get(slug)
            && existing.state != SlugReservationState::Released
        {
            return Err(ForgeError::SlugConflict(slug.to_owned()));
        }
        let record = SlugReservation {
            slug: slug.to_owned(),
            operation_id: operation_id.into(),
            work_id: None,
            item_generation: None,
            state: SlugReservationState::Reserved,
            at: Utc::now(),
        };
        append_record(&self.path, &record)?;
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
        let record = SlugReservation {
            work_id: Some(work_id),
            item_generation: Some(item_generation),
            state: SlugReservationState::Committed,
            at: Utc::now(),
            ..current
        };
        append_record(&self.path, &record)?;
        reservations.insert(slug.to_owned(), record.clone());
        Ok(record)
    }

    pub fn release(&self, slug: &str) -> Result<()> {
        let mut reservations = self
            .reservations
            .lock()
            .map_err(|_| ForgeError::Store("slug journal poisoned".into()))?;
        if let Some(current) = reservations.get(slug).cloned() {
            let record = SlugReservation {
                state: SlugReservationState::Released,
                at: Utc::now(),
                ..current
            };
            append_record(&self.path, &record)?;
            reservations.insert(slug.to_owned(), record);
        }
        Ok(())
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
}

#[derive(Default)]
pub struct ForgeCatalog {
    entries: Mutex<BTreeMap<String, CatalogEntry>>,
}

impl ForgeCatalog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn publish(&self, item: &WorkItem, receipt: &ForgeCommitReceipt) -> CommitReceipt {
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
        if let Ok(mut entries) = self.entries.lock() {
            entries.insert(item.id.as_str().to_owned(), entry);
        }
        CommitReceipt::new(
            StoreKind::ForgeCatalog,
            item.id.as_str(),
            receipt.item_generation,
            DurabilityLevel::Written,
            0,
        )
    }

    pub fn page(&self, limit: Option<usize>, cursor: Option<&str>) -> CatalogPage {
        let Ok(entries) = self.entries.lock() else {
            return CatalogPage {
                items: Vec::new(),
                next_cursor: None,
                truncated: true,
            };
        };
        let start = cursor
            .and_then(|cursor| {
                entries
                    .keys()
                    .position(|key| key.as_str() > cursor)
                    .or_else(|| entries.keys().position(|key| key == cursor).map(|i| i + 1))
            })
            .unwrap_or(0);
        let cap = limit.unwrap_or(UNPARAMETERIZED_LIST_CAP);
        let slice: Vec<CatalogEntry> = entries.values().skip(start).take(cap + 1).cloned().collect();
        let truncated = slice.len() > cap;
        let items: Vec<CatalogEntry> = slice.into_iter().take(cap).collect();
        let next_cursor = truncated
            .then(|| items.last().map(|entry| entry.work_id.as_str().to_owned()))
            .flatten();
        CatalogPage {
            items,
            next_cursor,
            truncated,
        }
    }

    pub fn all_capped(&self) -> (Vec<CatalogEntry>, bool) {
        let page = self.page(Some(UNPARAMETERIZED_LIST_CAP), None);
        (page.items, page.truncated)
    }

    pub fn rebuild_from(&self, items: impl IntoIterator<Item = (WorkItem, u64)>) {
        if let Ok(mut entries) = self.entries.lock() {
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
        }
    }
}

fn load_reservations(path: &Path) -> Result<BTreeMap<String, SlugReservation>> {
    let mut reservations = BTreeMap::new();
    if !path.exists() {
        return Ok(reservations);
    }
    let file = std::fs::File::open(path)?;
    for line in BufReader::new(file).lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(record) = serde_json::from_str::<SlugReservation>(&line) {
            reservations.insert(record.slug.clone(), record);
        }
    }
    Ok(reservations)
}

fn append_record(path: &Path, record: &SlugReservation) -> Result<()> {
    let mut line = serde_json::to_vec(record)?;
    line.push(b'\n');
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(&line)?;
    file.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{GitOid, GitWorkTarget, WorkTarget};
    use crate::owner::ForgeCommitReceipt;
    use medousa_store::CommitReceipt;
    use tempfile::TempDir;

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

    #[test]
    fn reserve_create_commit_and_orphan_recovery() {
        let tmp = TempDir::new().unwrap();
        let journal = SlugReservationJournal::open(tmp.path()).unwrap();
        journal.reserve("alpha", "op-1").unwrap();
        assert!(journal.reserve("alpha", "op-2").is_err());
        let orphans = journal.recover_orphans().unwrap();
        assert_eq!(orphans.len(), 1);
        journal.release("alpha").unwrap();
        assert!(journal.recover_orphans().unwrap().is_empty());
        journal.reserve("alpha", "op-3").unwrap();
        let work = item("alpha");
        journal.commit("alpha", work.id.clone(), 1).unwrap();
        assert!(journal.recover_orphans().unwrap().is_empty());
    }

    #[test]
    fn catalog_pages_without_loading_histories() {
        let catalog = ForgeCatalog::new();
        let first = item("one");
        let second = item("two");
        let receipt = |item: &WorkItem| ForgeCommitReceipt {
            work_id: item.id.clone(),
            item_generation: 1,
            first_seq: 1,
            last_seq: 1,
            log_offset: 0,
            durability: DurabilityLevel::Written,
            operation_generation: None,
            persistence: CommitReceipt::new(StoreKind::Forge, item.id.as_str(), 1, DurabilityLevel::Written, 0),
        };
        catalog.publish(&first, &receipt(&first));
        catalog.publish(&second, &receipt(&second));
        let page = catalog.page(Some(1), None);
        assert_eq!(page.items.len(), 1);
        assert!(page.truncated);
        assert!(page.next_cursor.is_some());
    }
}
