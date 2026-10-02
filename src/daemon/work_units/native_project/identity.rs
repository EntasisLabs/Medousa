//! Workshop-owned repository identity. Paths locate objects; UUIDs name them.
use super::*;
use crate::vault::contracts::VaultFileIdentity;
use fs2::FileExt;
use medousa_store::{DurabilityLevel, FileTransaction, StorePath, StoreRoot};
use serde::{Deserialize, Serialize};

const MAX_PROJECTS: usize = 256;
const MAX_LOCATOR_HISTORY: usize = 8;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProjectIdentity {
    pub id: String,
    pub common_dir: String,
    file: VaultFileIdentity,
    pub available: bool,
    pub revision: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    schema_version: u16,
    namespace: String,
    root_file: VaultFileIdentity,
    revision: u64,
    projects: Vec<ProjectIdentity>,
}
pub(super) struct ProjectRegistry {
    root: Arc<StoreRoot>,
    _lock: std::fs::File,
    snapshot: Snapshot,
}
fn path(extension: &str) -> Result<StorePath> {
    Ok(StorePath::parse(&format!("resource-projects.{extension}"))?)
}
fn nonce() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}
fn valid_nonce(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl ProjectRegistry {
    pub(super) fn open(forge: &Forge) -> Result<Self> {
        let root = forge.store().store_root_arc().clone();
        let lock = root.open_lock_file(&path("lock")?)?;
        lock.try_lock_exclusive()
            .map_err(|e| anyhow::anyhow!("project identity custody unavailable: {e}"))?;
        let root_file = VaultFileIdentity::from(root.root_metadata()?);
        let snapshot = match root.read_limited(&path("json")?, MAX_SNAPSHOT_BYTES as u64) {
            Ok(bytes) => {
                let snapshot: Snapshot = serde_json::from_slice(&bytes)?;
                if snapshot.schema_version != 1
                    || !valid_nonce(&snapshot.namespace)
                    || snapshot.root_file != root_file
                    || snapshot.projects.len() > MAX_PROJECTS
                {
                    bail!("invalid or copied project identity registry");
                }
                let mut ids = std::collections::BTreeSet::new();
                for record in &snapshot.projects {
                    if !valid_nonce(&record.id)
                        || !ids.insert(&record.id)
                        || record.common_dir.is_empty()
                        || record.common_dir.len() > 4096
                        || record.revision == 0
                        || record.revision > snapshot.revision
                    {
                        bail!("invalid project identity record");
                    }
                }
                for record in &snapshot.projects {
                    if snapshot
                        .projects
                        .iter()
                        .filter(|r| r.file == record.file)
                        .count()
                        != 1
                        || snapshot
                            .projects
                            .iter()
                            .filter(|r| r.common_dir == record.common_dir)
                            .count()
                            > MAX_LOCATOR_HISTORY
                        || (record.available
                            && snapshot.projects.iter().any(|r| {
                                r.id != record.id
                                    && r.available
                                    && r.common_dir == record.common_dir
                            }))
                    {
                        bail!("conflicting project identity custody");
                    }
                }
                snapshot
            }
            Err(e) if e.is_not_found() => {
                let snapshot = Snapshot {
                    schema_version: 1,
                    namespace: nonce(),
                    root_file,
                    revision: 0,
                    projects: vec![],
                };
                FileTransaction::new(root.clone()).create_only(
                    &path("json")?,
                    &serde_json::to_vec(&snapshot)?,
                    DurabilityLevel::Synced,
                )?;
                snapshot
            }
            Err(e) => return Err(e.into()),
        };
        // Complete a previous publication's interrupted parent fence.
        root.sync_parent_of(&path("json")?)?;
        Ok(Self {
            root,
            _lock: lock,
            snapshot,
        })
    }
    pub(super) fn namespace(&self) -> &str {
        &self.snapshot.namespace
    }
    pub(super) fn get(&self, id: &str) -> Result<ProjectIdentity> {
        self.snapshot
            .projects
            .iter()
            .find(|r| r.id == id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown project identity"))
    }
    fn persist(&self) -> Result<()> {
        if serde_json::to_vec(&self.snapshot)?.len() > MAX_SNAPSHOT_BYTES {
            bail!("project registry exceeds byte limit");
        }
        FileTransaction::new(self.root.clone())
            .replace_snapshot_json(&path("json")?, &self.snapshot)?;
        Ok(())
    }
    pub(super) fn observe_missing(
        &mut self,
        common_dir: &std::path::Path,
    ) -> Result<Vec<ProjectIdentity>> {
        let locator = common_dir
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("repository locator is not UTF-8"))?;
        let mut changed = false;
        for record in &mut self.snapshot.projects {
            if record.common_dir == locator && record.available {
                self.snapshot.revision = self
                    .snapshot
                    .revision
                    .checked_add(1)
                    .ok_or_else(|| anyhow::anyhow!("project revision exhausted"))?;
                record.available = false;
                record.revision = self.snapshot.revision;
                changed = true;
            }
        }
        if changed {
            self.persist()?;
        }
        let mut records: Vec<_> = self
            .snapshot
            .projects
            .iter()
            .filter(|r| r.common_dir == locator)
            .cloned()
            .collect();
        records.sort_by_key(|r| std::cmp::Reverse(r.revision));
        if records.is_empty() {
            bail!("repository is missing and has no retained identity");
        }
        Ok(records)
    }
    pub(super) fn observe(
        &mut self,
        common_dir: &std::path::Path,
        files: &StoreRoot,
    ) -> Result<Vec<ProjectIdentity>> {
        let locator = common_dir
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("repository locator is not UTF-8"))?
            .to_string();
        if locator.len() > 4096 {
            bail!("repository locator exceeds metadata limit");
        }
        let file = VaultFileIdentity::from(files.root_metadata()?);
        let found = self.snapshot.projects.iter().position(|r| r.file == file);
        if (found.is_none() && self.snapshot.projects.len() >= MAX_PROJECTS)
            || self
                .snapshot
                .projects
                .iter()
                .filter(|r| r.common_dir == locator && r.file != file)
                .count()
                >= MAX_LOCATOR_HISTORY
        {
            bail!("project identity history is full");
        }
        let mut changed = false;
        for record in &mut self.snapshot.projects {
            if record.common_dir == locator && record.file != file && record.available {
                self.snapshot.revision = self
                    .snapshot
                    .revision
                    .checked_add(1)
                    .ok_or_else(|| anyhow::anyhow!("project revision exhausted"))?;
                record.available = false;
                record.revision = self.snapshot.revision;
                changed = true;
            }
        }
        let index = match found {
            Some(i) => i,
            None => {
                self.snapshot.projects.push(ProjectIdentity {
                    id: nonce(),
                    common_dir: locator.clone(),
                    file,
                    available: false,
                    revision: 0,
                });
                self.snapshot.projects.len() - 1
            }
        };
        let current = &mut self.snapshot.projects[index];
        if current.common_dir != locator || !current.available {
            self.snapshot.revision = self
                .snapshot
                .revision
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!("project revision exhausted"))?;
            current.common_dir = locator.clone();
            current.available = true;
            current.revision = self.snapshot.revision;
            changed = true;
        }
        if changed {
            self.persist()?;
        }
        let mut records = vec![self.snapshot.projects[index].clone()];
        let current_id = records[0].id.clone();
        records.extend(
            self.snapshot
                .projects
                .iter()
                .filter(|r| r.common_dir == locator && r.id != current_id)
                .cloned(),
        );
        Ok(records)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn physical_relocation_retains_id_and_locator_history_never_evicts_it() {
        let dir = tempfile::tempdir().unwrap();
        let forge = Forge::open(dir.path().canonicalize().unwrap().join("forge")).unwrap();
        let common = dir.path().canonicalize().unwrap().join("common");
        std::fs::create_dir(&common).unwrap();
        let files = StoreRoot::open_nofollow(&common).unwrap();
        let mut registry = ProjectRegistry::open(&forge).unwrap();
        let first = registry.observe(&common, &files).unwrap()[0].clone();
        drop(registry);
        let moved = dir.path().canonicalize().unwrap().join("moved");
        std::fs::rename(&common, &moved).unwrap();
        let mut registry = ProjectRegistry::open(&forge).unwrap();
        let adopted = registry.observe(&moved, &files).unwrap()[0].clone();
        assert_eq!(first.id, adopted.id);
        assert_ne!(first.revision, adopted.revision);
        for n in 1..MAX_LOCATOR_HISTORY {
            std::fs::rename(
                &moved,
                dir.path().canonicalize().unwrap().join(format!("old-{n}")),
            )
            .unwrap();
            std::fs::create_dir(&moved).unwrap();
            registry
                .observe(&moved, &StoreRoot::open_nofollow(&moved).unwrap())
                .unwrap();
        }
        std::fs::rename(
            &moved,
            dir.path().canonicalize().unwrap().join("last-retained"),
        )
        .unwrap();
        std::fs::create_dir(&moved).unwrap();
        assert!(
            registry
                .observe(&moved, &StoreRoot::open_nofollow(&moved).unwrap())
                .is_err()
        );
        drop(registry);
        let registry = ProjectRegistry::open(&forge).unwrap();
        assert_eq!(registry.get(&first.id).unwrap().id, first.id);
        assert!(!registry.get(&first.id).unwrap().available);
    }
    #[test]
    fn corrupt_and_copied_registries_fail_without_issuing_a_new_namespace() {
        let dir = tempfile::tempdir().unwrap();
        let forge = Forge::open(dir.path().canonicalize().unwrap().join("forge")).unwrap();
        let registry = ProjectRegistry::open(&forge).unwrap();
        let namespace = registry.namespace().to_string();
        drop(registry);
        let bytes = forge
            .store()
            .store_root()
            .read(&path("json").unwrap())
            .unwrap();
        let other = Forge::open(dir.path().canonicalize().unwrap().join("other-forge")).unwrap();
        other
            .store()
            .store_root()
            .atomic_write(&path("json").unwrap(), &bytes)
            .unwrap();
        assert!(ProjectRegistry::open(&other).is_err());
        assert_eq!(
            other
                .store()
                .store_root()
                .read(&path("json").unwrap())
                .unwrap(),
            bytes
        );
        forge
            .store()
            .store_root()
            .atomic_write(&path("json").unwrap(), b"{broken")
            .unwrap();
        assert!(ProjectRegistry::open(&forge).is_err());
        assert_eq!(
            forge
                .store()
                .store_root()
                .read(&path("json").unwrap())
                .unwrap(),
            b"{broken"
        );
        forge
            .store()
            .store_root()
            .atomic_write(&path("json").unwrap(), &bytes)
            .unwrap();
        assert_eq!(
            ProjectRegistry::open(&forge).unwrap().namespace(),
            namespace
        );
    }
}
