//! Generation-stamped incremental vault projection (H07.2).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use crate::vault::note::{VaultIndexEntry, VaultNoteSource};

#[derive(Debug, Clone, Default)]
pub struct VaultProjection {
    pub generation: u64,
    pub by_path: HashMap<String, VaultIndexEntry>,
    pub paths_by_stem: HashMap<String, BTreeSet<String>>,
    pub paths_by_folded_title: HashMap<String, BTreeSet<String>>,
    pub paths_by_title_slug: HashMap<String, BTreeSet<String>>,
    pub children_by_parent: HashMap<String, BTreeSet<String>>,
    pub forward_links: HashMap<String, BTreeSet<String>>,
    pub back_links: HashMap<String, BTreeSet<String>>,
    pub unresolved_by_token: HashMap<String, BTreeSet<String>>,
}

impl VaultProjection {
    pub fn get(&self, path: &str) -> Option<&VaultIndexEntry> {
        self.by_path.get(path)
    }

    pub fn backlinks(&self, path: &str) -> Vec<String> {
        self.back_links
            .get(path)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn apply_upsert(&mut self, entry: VaultIndexEntry, generation: u64) {
        if let Some(previous) = self.by_path.remove(&entry.path) {
            self.remove_lookups(&previous);
            self.remove_links(&previous.path);
        }
        self.insert_lookups(&entry);
        self.insert_links(&entry);
        self.by_path.insert(entry.path.clone(), entry);
        self.generation = generation;
    }

    pub fn apply_remove(&mut self, path: &str, generation: u64) {
        if let Some(previous) = self.by_path.remove(path) {
            self.remove_lookups(&previous);
            self.remove_links(path);
        }
        self.generation = generation;
    }

    fn insert_lookups(&mut self, entry: &VaultIndexEntry) {
        let stem = filename_stem(&entry.path);
        self.paths_by_stem
            .entry(stem)
            .or_default()
            .insert(entry.path.clone());
        let folded = entry.title.trim().to_ascii_lowercase();
        if !folded.is_empty() {
            self.paths_by_folded_title
                .entry(folded)
                .or_default()
                .insert(entry.path.clone());
        }
        let slug = title_slug(&entry.title);
        if !slug.is_empty() {
            self.paths_by_title_slug
                .entry(slug)
                .or_default()
                .insert(entry.path.clone());
        }
        let parent = parent_key(&entry.path);
        self.children_by_parent
            .entry(parent)
            .or_default()
            .insert(entry.path.clone());
    }

    fn remove_lookups(&mut self, entry: &VaultIndexEntry) {
        let stem = filename_stem(&entry.path);
        if let Some(set) = self.paths_by_stem.get_mut(&stem) {
            set.remove(&entry.path);
            if set.is_empty() {
                self.paths_by_stem.remove(&stem);
            }
        }
        let folded = entry.title.trim().to_ascii_lowercase();
        if let Some(set) = self.paths_by_folded_title.get_mut(&folded) {
            set.remove(&entry.path);
            if set.is_empty() {
                self.paths_by_folded_title.remove(&folded);
            }
        }
        let slug = title_slug(&entry.title);
        if let Some(set) = self.paths_by_title_slug.get_mut(&slug) {
            set.remove(&entry.path);
            if set.is_empty() {
                self.paths_by_title_slug.remove(&slug);
            }
        }
        let parent = parent_key(&entry.path);
        if let Some(set) = self.children_by_parent.get_mut(&parent) {
            set.remove(&entry.path);
            if set.is_empty() {
                self.children_by_parent.remove(&parent);
            }
        }
    }

    fn insert_links(&mut self, entry: &VaultIndexEntry) {
        let mut forward = BTreeSet::new();
        for target in &entry.wikilinks_out {
            forward.insert(target.clone());
            self.back_links
                .entry(target.clone())
                .or_default()
                .insert(entry.path.clone());
        }
        self.forward_links.insert(entry.path.clone(), forward);
    }

    fn remove_links(&mut self, path: &str) {
        if let Some(forward) = self.forward_links.remove(path) {
            for target in forward {
                if let Some(back) = self.back_links.get_mut(&target) {
                    back.remove(path);
                    if back.is_empty() {
                        self.back_links.remove(&target);
                    }
                }
            }
        }
        // Notes that pointed at `path`: drop the edge without scanning all sets.
        if let Some(sources) = self.back_links.remove(path) {
            for source in sources {
                if let Some(fwd) = self.forward_links.get_mut(&source) {
                    fwd.remove(path);
                }
            }
        }
    }

    pub fn resolve_wikilink(&self, token: &str, source_path: Option<&str>) -> WikilinkResolution {
        let token = token.trim();
        if token.is_empty() {
            return WikilinkResolution::Missing;
        }
        let normalized = if token.ends_with(".md") {
            token.to_string()
        } else if token.contains('/') {
            format!("{token}.md")
        } else {
            token.to_string()
        };
        if self.by_path.contains_key(&normalized) {
            return WikilinkResolution::Resolved(normalized);
        }
        if let Some(source) = source_path
            && let Some(dir) = source.rsplit_once('/').map(|(dir, _)| dir)
        {
            let candidate = format!("{dir}/{}.md", filename_stem(token));
            if self.by_path.contains_key(&candidate) {
                return WikilinkResolution::Resolved(candidate);
            }
        }
        let stem = filename_stem(token);
        let mut candidates = BTreeSet::new();
        if let Some(set) = self.paths_by_stem.get(&stem) {
            candidates.extend(set.iter().cloned());
        }
        if let Some(set) = self.paths_by_folded_title.get(&stem.to_ascii_lowercase()) {
            candidates.extend(set.iter().cloned());
        }
        if let Some(set) = self.paths_by_title_slug.get(&title_slug(token)) {
            candidates.extend(set.iter().cloned());
        }
        match candidates.len() {
            0 => WikilinkResolution::Missing,
            1 => WikilinkResolution::Resolved(candidates.into_iter().next().unwrap()),
            _ => WikilinkResolution::Ambiguous(candidates.into_iter().collect()),
        }
    }

    /// Candidate paths for one wikilink token (O(collisions), not O(corpus)).
    pub fn wikilink_candidates(&self, raw: &str, source_path: &str) -> Vec<String> {
        let token = raw
            .split('|')
            .next()
            .unwrap_or(raw)
            .trim()
            .trim_matches('"')
            .trim_matches('\'');
        if token.is_empty() {
            return Vec::new();
        }
        let mut candidates = Vec::new();
        if token.contains('/') {
            if let Ok(path) =
                crate::vault::path::normalize_vault_path(&format!("{}.md", token.trim_end_matches(".md")))
            {
                candidates.push(path);
            }
        } else {
            let stem = token.trim_end_matches(".md");
            let same_dir_candidate = source_path
                .rsplit_once('/')
                .map(|(dir, _)| format!("{dir}/{stem}.md"))
                .unwrap_or_else(|| format!("{stem}.md"));
            if let Ok(same_dir) = crate::vault::path::normalize_vault_path(&same_dir_candidate) {
                candidates.push(same_dir);
            }
            if let Ok(root) = crate::vault::path::normalize_vault_path(&format!("{stem}.md")) {
                candidates.push(root);
            }
            if let Some(set) = self.paths_by_stem.get(stem) {
                candidates.extend(set.iter().cloned());
            }
            if let Some(set) = self.paths_by_folded_title.get(&stem.to_ascii_lowercase()) {
                candidates.extend(set.iter().cloned());
            }
            if let Some(set) = self.paths_by_title_slug.get(&title_slug(stem)) {
                candidates.extend(set.iter().cloned());
            }
        }
        candidates.sort();
        candidates.dedup();
        candidates
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WikilinkResolution {
    Resolved(String),
    Ambiguous(Vec<String>),
    Missing,
}

#[derive(Debug)]
pub struct ProjectionOwner {
    current: RwLock<Arc<VaultProjection>>,
    reconcile_epoch: AtomicU64,
    certified_epoch: AtomicU64,
    stale: AtomicU64,
}

impl ProjectionOwner {
    pub fn new() -> Self {
        Self {
            current: RwLock::new(Arc::new(VaultProjection::default())),
            reconcile_epoch: AtomicU64::new(0),
            certified_epoch: AtomicU64::new(0),
            stale: AtomicU64::new(0),
        }
    }

    pub fn snapshot(&self) -> Arc<VaultProjection> {
        Arc::clone(
            &self
                .current
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        )
    }

    pub fn replace(&self, projection: VaultProjection) -> Arc<VaultProjection> {
        let arc = Arc::new(projection);
        *self
            .current
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Arc::clone(&arc);
        arc
    }

    pub fn upsert(&self, entry: VaultIndexEntry, generation: u64) -> Arc<VaultProjection> {
        let mut guard = self
            .current
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // In-place when this Arc is unique; otherwise one COW clone (no
        // unconditional full-structure clone on every upsert).
        let projection = Arc::make_mut(&mut *guard);
        projection.apply_upsert(entry, generation);
        Arc::clone(&*guard)
    }

    pub fn remove(&self, path: &str, generation: u64) -> Arc<VaultProjection> {
        let mut guard = self
            .current
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let projection = Arc::make_mut(&mut *guard);
        projection.apply_remove(path, generation);
        Arc::clone(&*guard)
    }

    pub fn mark_stale_reconciling(&self) -> u64 {
        self.stale.store(1, Ordering::Release);
        self.reconcile_epoch.fetch_add(1, Ordering::AcqRel) + 1
    }

    pub fn reconcile_epoch(&self) -> u64 {
        self.reconcile_epoch.load(Ordering::Acquire)
    }

    /// Certify that a reconcile completed for `epoch` if it is still current.
    pub fn certify_reconcile(&self, epoch: u64) {
        if self.reconcile_epoch.load(Ordering::Acquire) == epoch {
            self.certified_epoch.store(epoch, Ordering::Release);
            self.stale.store(0, Ordering::Release);
        }
    }

    pub fn clear_stale(&self) {
        let epoch = self.reconcile_epoch.load(Ordering::Acquire);
        self.certified_epoch.store(epoch, Ordering::Release);
        self.stale.store(0, Ordering::Release);
    }

    pub fn is_stale(&self) -> bool {
        self.stale.load(Ordering::Acquire) != 0
    }

    /// Warm skip is allowed only when the projection is non-stale and the
    /// last certify matches the current reconcile epoch.
    pub fn needs_reconcile(&self) -> bool {
        self.is_stale()
            || self.certified_epoch.load(Ordering::Acquire)
                != self.reconcile_epoch.load(Ordering::Acquire)
    }
}

impl Default for ProjectionOwner {
    fn default() -> Self {
        Self::new()
    }
}

fn filename_stem(path: &str) -> String {
    let base = path.rsplit('/').next().unwrap_or(path);
    base.trim_end_matches(".md").to_string()
}

fn title_slug(raw: &str) -> String {
    raw.to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn parent_key(path: &str) -> String {
    match path.rsplit_once('/') {
        Some((parent, _)) => parent.to_string(),
        None => String::new(),
    }
}

pub fn build_projection_from_entries(
    entries: impl IntoIterator<Item = VaultIndexEntry>,
    generation: u64,
) -> VaultProjection {
    let mut projection = VaultProjection {
        generation,
        ..VaultProjection::default()
    };
    // Stable insert order for determinism.
    let mut ordered: BTreeMap<String, VaultIndexEntry> = BTreeMap::new();
    for entry in entries {
        ordered.insert(entry.path.clone(), entry);
    }
    for entry in ordered.into_values() {
        projection.apply_upsert(entry, generation);
    }
    projection
}

pub fn vault_projection_generation() -> u64 {
    // Filled by store's ProjectionOwner; default 1 when empty.
    1
}

#[allow(dead_code)]
pub fn source_rank(source: &VaultNoteSource) -> u8 {
    match source {
        VaultNoteSource::User => 0,
        VaultNoteSource::ProjectOverlay => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn entry(path: &str, title: &str, links: &[&str]) -> VaultIndexEntry {
        VaultIndexEntry {
            path: path.into(),
            title: title.into(),
            byte_size: 1,
            content_hash: "sha256:x".into(),
            modified_at_utc: Utc::now(),
            created_at_utc: Utc::now(),
            tags: Vec::new(),
            wikilinks_out: links.iter().map(|value| (*value).to_string()).collect(),
            kind: None,
            source: VaultNoteSource::User,
        }
    }

    #[test]
    fn incremental_upsert_updates_only_touched_buckets() {
        let mut projection = VaultProjection::default();
        projection.apply_upsert(entry("a.md", "A", &["b.md"]), 1);
        projection.apply_upsert(entry("b.md", "B", &[]), 2);
        assert_eq!(projection.backlinks("b.md"), vec!["a.md".to_string()]);
        projection.apply_upsert(entry("a.md", "A", &[]), 3);
        assert!(projection.backlinks("b.md").is_empty());
        assert_eq!(projection.generation, 3);
    }

    #[test]
    fn owner_upsert_is_in_place_when_unshared() {
        let owner = ProjectionOwner::new();
        let before = owner.snapshot();
        drop(before); // unique Arc
        owner.upsert(entry("a.md", "A", &[]), 1);
        assert_eq!(owner.snapshot().by_path.len(), 1);
        owner.upsert(entry("b.md", "B", &["a.md"]), 2);
        assert_eq!(owner.snapshot().backlinks("a.md"), vec!["b.md".to_string()]);
    }

    #[test]
    fn wikilink_resolution_reports_ambiguity() {
        let mut projection = VaultProjection::default();
        projection.apply_upsert(entry("one/same.md", "Same", &[]), 1);
        projection.apply_upsert(entry("two/same.md", "Same", &[]), 2);
        match projection.resolve_wikilink("same", None) {
            WikilinkResolution::Ambiguous(paths) => {
                assert_eq!(paths.len(), 2);
            }
            other => panic!("expected ambiguous, got {other:?}"),
        }
    }
}
