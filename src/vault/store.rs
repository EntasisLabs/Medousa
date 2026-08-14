//! Vault filesystem store + on-disk index.

use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, RwLock};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;

use crate::store_root::{StoreEntryKind, StoreRoot};
use crate::vault::links::{VaultLinkIndex, load_link_index_from_disk, persist_link_index};
use crate::vault::note::{VaultIndexEntry, VaultNoteSource, build_index_entry, content_hash};
use crate::vault::path::{
    VaultPath, normalize_vault_path, project_vault_overlay_capability, user_vault_capability,
};

const INDEX_FILE: &str = "index.jsonl";
const LINKS_FILE: &str = "links.jsonl";

struct ScanDraft {
    path: String,
    body: String,
    created_at: DateTime<Utc>,
    modified_at: DateTime<Utc>,
    source: VaultNoteSource,
}

/// Metadata collected during an incremental vault walk (no body read).
#[derive(Clone)]
struct FileMeta {
    path: String,
    relative: VaultPath,
    files: Arc<StoreRoot>,
    created_at: DateTime<Utc>,
    modified_at: DateTime<Utc>,
    source: VaultNoteSource,
}

static STORE: Lazy<VaultStore> = Lazy::new(VaultStore::new);

pub fn vault_store() -> &'static VaultStore {
    &STORE
}

pub struct VaultStore {
    index: RwLock<HashMap<String, VaultIndexEntry>>,
    link_index: RwLock<VaultLinkIndex>,
}

impl VaultStore {
    fn new() -> Self {
        let store = Self {
            index: RwLock::new(HashMap::new()),
            link_index: RwLock::new(load_link_index_from_disk()),
        };
        store.reload_from_disk();
        store
    }

    fn index_path() -> VaultPath {
        VaultPath::parse(INDEX_FILE).expect("static vault index path must be valid")
    }

    fn reload_from_disk(&self) {
        let mut map = HashMap::new();
        if let Ok(files) = user_vault_capability()
            && let Ok(bytes) = files.read(&Self::index_path())
        {
            for line in bytes.split(|byte| *byte == b'\n') {
                if line.iter().all(u8::is_ascii_whitespace) {
                    continue;
                }
                if let Ok(entry) = serde_json::from_slice::<VaultIndexEntry>(line) {
                    map.insert(entry.path.clone(), entry);
                }
            }
        }
        *self.index.write().expect("vault index") = map;
    }

    fn persist_index(&self) {
        let entries = self.index.read().expect("vault index").clone();
        let mut lines = entries.values().cloned().collect::<Vec<_>>();
        lines.sort_by(|left, right| left.path.cmp(&right.path));
        let mut bytes = Vec::new();
        for entry in lines {
            if serde_json::to_writer(&mut bytes, &entry).is_ok() {
                bytes.push(b'\n');
            }
        }
        if let Ok(files) = user_vault_capability() {
            let _ = files.atomic_write(&Self::index_path(), &bytes);
        }
    }

    pub fn refresh_from_disk(&self) -> Result<()> {
        self.reload_from_disk();
        let mut drafts = Vec::new();

        self.scan_root(
            &user_vault_capability()?,
            VaultNoteSource::User,
            &mut drafts,
        )?;
        if let Some(overlay) = project_vault_overlay_capability()? {
            self.scan_root(&overlay, VaultNoteSource::ProjectOverlay, &mut drafts)?;
        }

        let mut by_path: HashMap<String, ScanDraft> = HashMap::new();
        for draft in drafts {
            match by_path.get(&draft.path) {
                Some(existing) if existing.source == VaultNoteSource::User => continue,
                _ => {
                    by_path.insert(draft.path.clone(), draft);
                }
            }
        }
        let discovered = Self::finalize_entries(by_path.into_values().collect());
        *self.index.write().expect("vault index") = discovered;
        self.rebuild_link_index();
        self.persist_index();
        Ok(())
    }

    /// Cheap freshness pass for list/get: metadata walk, re-read only changed files.
    pub fn ensure_index_fresh(&self) -> Result<()> {
        let mut metas = Vec::new();
        self.collect_metas(user_vault_capability()?, VaultNoteSource::User, &mut metas)?;
        if let Some(overlay) = project_vault_overlay_capability()? {
            self.collect_metas(overlay, VaultNoteSource::ProjectOverlay, &mut metas)?;
        }

        let mut by_path: HashMap<String, FileMeta> = HashMap::new();
        for meta in metas {
            match by_path.get(&meta.path) {
                Some(existing) if existing.source == VaultNoteSource::User => continue,
                _ => {
                    by_path.insert(meta.path.clone(), meta);
                }
            }
        }

        let existing = self.index.read().expect("vault index").clone();
        let mut dirty = false;
        let mut to_read: Vec<FileMeta> = Vec::new();

        for (path, meta) in &by_path {
            match existing.get(path) {
                Some(entry)
                    if entry.modified_at_utc.timestamp() == meta.modified_at.timestamp()
                        && entry.source == meta.source => {}
                _ => {
                    dirty = true;
                    to_read.push(meta.clone());
                }
            }
        }

        let discovered_paths: HashSet<String> = by_path.keys().cloned().collect();
        let removed: Vec<String> = existing
            .keys()
            .filter(|path| !discovered_paths.contains(*path))
            .cloned()
            .collect();
        if !removed.is_empty() {
            dirty = true;
        }

        if !dirty {
            return Ok(());
        }

        let mut drafts = Vec::with_capacity(to_read.len());
        for meta in to_read {
            let body = match meta.files.read(&meta.relative) {
                Ok(bytes) => match String::from_utf8(bytes) {
                    Ok(body) => body,
                    Err(_) => continue,
                },
                Err(_) => continue,
            };
            drafts.push(ScanDraft {
                path: meta.path,
                body,
                created_at: meta.created_at,
                modified_at: meta.modified_at,
                source: meta.source,
            });
        }
        let updated = Self::finalize_entries(drafts);

        {
            let mut index = self.index.write().expect("vault index");
            for path in removed {
                index.remove(&path);
            }
            for (path, entry) in updated {
                match index.get(&path) {
                    Some(existing) if existing.source == VaultNoteSource::ProjectOverlay => {
                        if entry.source == VaultNoteSource::User {
                            index.insert(path, entry);
                        }
                    }
                    Some(existing) => {
                        let created = existing.created_at_utc;
                        let mut merged = entry;
                        merged.created_at_utc = created;
                        index.insert(path, merged);
                    }
                    None => {
                        index.insert(path, entry);
                    }
                }
            }
        }

        self.rebuild_link_index();
        self.persist_index();
        Ok(())
    }

    fn rebuild_link_index(&self) {
        let entries: Vec<_> = self
            .index
            .read()
            .expect("vault index")
            .values()
            .cloned()
            .collect();
        let links = VaultLinkIndex::rebuild(&entries);
        let _ = persist_link_index(&links);
        *self.link_index.write().expect("vault links") = links;
    }

    fn finalize_entries(drafts: Vec<ScanDraft>) -> HashMap<String, VaultIndexEntry> {
        let known: HashSet<String> = drafts.iter().map(|draft| draft.path.clone()).collect();
        let seed_entries: Vec<VaultIndexEntry> = drafts
            .iter()
            .map(|draft| VaultIndexEntry {
                path: draft.path.clone(),
                title: crate::vault::note::extract_title(&draft.body, &draft.path),
                byte_size: draft.body.len(),
                content_hash: content_hash(&draft.body),
                modified_at_utc: draft.modified_at,
                created_at_utc: draft.created_at,
                tags: Vec::new(),
                wikilinks_out: Vec::new(),
                kind: None,
                source: draft.source.clone(),
            })
            .collect();

        let mut out = HashMap::new();
        for draft in drafts {
            let index_entry = build_index_entry(
                &draft.path,
                &draft.body,
                draft.created_at,
                draft.modified_at,
                draft.source,
                &known,
                &seed_entries,
            );
            out.insert(draft.path, index_entry);
        }
        out
    }

    fn scan_root(
        &self,
        files: &Arc<StoreRoot>,
        source: VaultNoteSource,
        drafts: &mut Vec<ScanDraft>,
    ) -> Result<()> {
        self.scan_dir(files, None, source, drafts)
    }

    fn collect_metas(
        &self,
        files: Arc<StoreRoot>,
        source: VaultNoteSource,
        out: &mut Vec<FileMeta>,
    ) -> Result<()> {
        self.collect_metas_dir(&files, None, source, out)
    }

    fn collect_metas_dir(
        &self,
        files: &Arc<StoreRoot>,
        dir: Option<&VaultPath>,
        source: VaultNoteSource,
        out: &mut Vec<FileMeta>,
    ) -> Result<()> {
        let entries = match dir {
            Some(dir) => files.list_directory_utf8(dir)?,
            None => files.list_root_utf8()?,
        };
        for entry in entries {
            if entry.name.starts_with('.') {
                continue;
            }
            let relative = match dir {
                Some(dir) => dir.join_segment(&entry.name),
                None => VaultPath::parse(&entry.name),
            };
            let Ok(relative) = relative else {
                continue;
            };
            if entry.kind == StoreEntryKind::Directory {
                self.collect_metas_dir(files, Some(&relative), source.clone(), out)?;
                continue;
            }
            if entry.kind != StoreEntryKind::File
                || entry.name == INDEX_FILE
                || entry.name == LINKS_FILE
                || !Self::is_indexable_vault_file(Path::new(relative.as_str()))
            {
                continue;
            }
            out.push(FileMeta {
                path: relative.to_string(),
                relative,
                files: Arc::clone(files),
                created_at: Self::optional_timestamp(entry.created),
                modified_at: Self::optional_timestamp(entry.modified),
                source: source.clone(),
            });
        }
        Ok(())
    }

    fn scan_dir(
        &self,
        files: &Arc<StoreRoot>,
        dir: Option<&VaultPath>,
        source: VaultNoteSource,
        drafts: &mut Vec<ScanDraft>,
    ) -> Result<()> {
        let entries = match dir {
            Some(dir) => files.list_directory_utf8(dir)?,
            None => files.list_root_utf8()?,
        };
        for entry in entries {
            if entry.name.starts_with('.') {
                continue;
            }
            let relative = match dir {
                Some(dir) => dir.join_segment(&entry.name),
                None => VaultPath::parse(&entry.name),
            };
            let Ok(relative) = relative else {
                continue;
            };
            if entry.kind == StoreEntryKind::Directory {
                self.scan_dir(files, Some(&relative), source.clone(), drafts)?;
                continue;
            }
            if entry.kind != StoreEntryKind::File
                || entry.name == INDEX_FILE
                || entry.name == LINKS_FILE
                || !Self::is_indexable_vault_file(Path::new(relative.as_str()))
            {
                continue;
            }
            // Skip binary / non-UTF8 instead of failing the whole vault scan.
            let body = match files
                .read(&relative)
                .ok()
                .and_then(|bytes| String::from_utf8(bytes).ok())
            {
                Some(body) => body,
                None => continue,
            };

            drafts.push(ScanDraft {
                path: relative.to_string(),
                body,
                created_at: Self::optional_timestamp(entry.created),
                modified_at: Self::optional_timestamp(entry.modified),
                source: source.clone(),
            });
        }
        Ok(())
    }

    /// Prefer markdown and known text; skip binaries so Obsidian assets don't break scans.
    fn is_indexable_vault_file(path: &Path) -> bool {
        let ext = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        matches!(
            ext.as_str(),
            "md" | "markdown"
                | "txt"
                | "csv"
                | "tsv"
                | "json"
                | "yaml"
                | "yml"
                | "toml"
                | "html"
                | "htm"
                | "svg"
                | "xml"
                | "css"
                | "js"
                | "ts"
                | "mjs"
                | "cjs"
        )
    }

    pub fn list_entries(&self, prefix: Option<&str>, limit: usize) -> Vec<VaultIndexEntry> {
        let _ = self.ensure_index_fresh();
        let index = self.index.read().expect("vault index");
        let mut entries = index.values().cloned().collect::<Vec<_>>();
        if let Some(prefix) = prefix.map(str::trim).filter(|value| !value.is_empty()) {
            entries.retain(|entry| entry.path.starts_with(prefix));
        }
        entries.sort_by_key(|right| Reverse(right.modified_at_utc));
        entries.truncate(limit);
        entries
    }

    pub fn get_entry(&self, path: &str) -> Option<VaultIndexEntry> {
        let _ = self.ensure_index_fresh();
        self.peek_entry(path)
    }

    fn peek_entry(&self, path: &str) -> Option<VaultIndexEntry> {
        let normalized = normalize_vault_path(path).ok()?;
        self.index
            .read()
            .expect("vault index")
            .get(&normalized)
            .cloned()
    }

    pub fn read_content(&self, path: &str) -> Result<String> {
        let path = VaultPath::parse(path)?;
        let user = user_vault_capability()?;
        if user.is_file(&path)? {
            return String::from_utf8(user.read(&path)?).context("vault note is not UTF-8");
        }
        if let Some(overlay) = project_vault_overlay_capability()?
            && overlay.is_file(&path)?
        {
            return String::from_utf8(overlay.read(&path)?).context("vault note is not UTF-8");
        }
        bail!("vault note not found: {path}")
    }

    pub fn write_content(
        &self,
        path: &str,
        content: &str,
        if_match: Option<&str>,
    ) -> Result<VaultIndexEntry> {
        let path = VaultPath::parse(path)?;
        let normalized = path.to_string();
        let files = user_vault_capability()?;
        if let Some(expected) = if_match.map(str::trim).filter(|value| !value.is_empty()) {
            if files.is_file(&path)? {
                let existing = String::from_utf8(files.read(&path)?)?;
                let actual = content_hash(&existing);
                if actual != expected {
                    bail!("content_hash mismatch (If-Match failed)");
                }
            } else {
                bail!("content_hash mismatch (note does not exist)");
            }
        }

        let existed = files.is_file(&path)?;
        let created_at = if existed {
            self.peek_entry(&normalized)
                .map(|entry| entry.created_at_utc)
                .unwrap_or_else(Utc::now)
        } else {
            Utc::now()
        };
        files.atomic_write(&path, content.as_bytes())?;
        let modified_at = files
            .metadata(&path)
            .ok()
            .and_then(|meta| meta.modified)
            .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| {
                chrono::DateTime::<Utc>::from_timestamp(duration.as_secs() as i64, 0)
                    .unwrap_or_else(Utc::now)
            })
            .unwrap_or_else(Utc::now);

        let known: HashSet<String> = self
            .index
            .read()
            .expect("vault index")
            .keys()
            .cloned()
            .chain(std::iter::once(normalized.clone()))
            .collect();
        let seed_entries: Vec<VaultIndexEntry> = self
            .index
            .read()
            .expect("vault index")
            .values()
            .cloned()
            .collect();
        let entry = build_index_entry(
            &normalized,
            content,
            created_at,
            modified_at,
            VaultNoteSource::User,
            &known,
            &seed_entries,
        );
        self.index
            .write()
            .expect("vault index")
            .insert(normalized.clone(), entry.clone());
        self.rebuild_link_index();
        self.persist_index();
        Ok(entry)
    }

    pub fn delete_note(&self, path: &str) -> Result<()> {
        let path = VaultPath::parse(path)?;
        let normalized = path.to_string();
        let files = user_vault_capability()?;
        if !files.is_file(&path)? {
            bail!("vault note not found: {normalized}");
        }

        let trash = path.trash_path();
        if files.is_file(&trash)? {
            files.remove_file(&trash)?;
        }
        files.rename(&path, &trash)?;
        self.index.write().expect("vault index").remove(&normalized);
        self.rebuild_link_index();
        self.persist_index();
        Ok(())
    }

    pub fn list_trash(&self, limit: usize) -> Result<Vec<(String, Option<DateTime<Utc>>)>> {
        let files = user_vault_capability()?;
        let root = VaultPath::trash_root();
        if !files.is_dir(&root)? {
            return Ok(Vec::new());
        }
        let mut entries = Vec::new();
        fn walk(
            files: &StoreRoot,
            dir: &VaultPath,
            prefix: &str,
            out: &mut Vec<(String, Option<DateTime<Utc>>)>,
        ) -> Result<()> {
            for entry in files.list_directory_utf8(dir)? {
                if entry.name.starts_with('.') || entry.kind == StoreEntryKind::Link {
                    continue;
                }
                let rel = if prefix.is_empty() {
                    entry.name.clone()
                } else {
                    format!("{prefix}/{}", entry.name)
                };
                if VaultPath::parse(&rel).is_err() {
                    continue;
                }
                let child = match dir.join_internal_segment(&entry.name) {
                    Ok(child) => child,
                    Err(_) => continue,
                };
                if entry.kind == StoreEntryKind::Directory {
                    walk(files, &child, &rel, out)?;
                } else if entry.kind == StoreEntryKind::File {
                    out.push((rel, entry.modified.map(DateTime::<Utc>::from)));
                }
            }
            Ok(())
        }
        walk(&files, &root, "", &mut entries)?;
        entries.sort_by_key(|entry| Reverse(entry.1));
        entries.truncate(limit.clamp(1, 500));
        Ok(entries)
    }

    pub fn restore_from_trash(&self, path: &str) -> Result<String> {
        let path = VaultPath::parse(path)?;
        let normalized = path.to_string();
        let trash = path.trash_path();
        let files = user_vault_capability()?;
        if !files.is_file(&trash)? {
            bail!("trashed note not found: {normalized}");
        }
        if files.is_file(&path)? {
            bail!("a note already exists at path: {normalized}");
        }
        files.rename(&trash, &path)?;
        let _ = self.ensure_index_fresh();
        Ok(normalized)
    }

    pub fn note_exists(&self, path: &str) -> bool {
        let _ = self.ensure_index_fresh();
        self.peek_entry(path).is_some()
    }

    pub fn backlinks_for(&self, path: &str) -> Vec<String> {
        let _ = self.ensure_index_fresh();
        let normalized = match normalize_vault_path(path) {
            Ok(value) => value,
            Err(_) => return Vec::new(),
        };
        self.link_index
            .read()
            .expect("vault links")
            .backlinks_for(&normalized)
    }

    fn optional_timestamp(value: Option<std::time::SystemTime>) -> DateTime<Utc> {
        value
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| {
                DateTime::<Utc>::from_timestamp(duration.as_secs() as i64, 0)
                    .unwrap_or_else(Utc::now)
            })
            .unwrap_or_else(Utc::now)
    }

    pub fn all_entries(&self) -> Vec<VaultIndexEntry> {
        let _ = self.ensure_index_fresh();
        let mut entries = self
            .index
            .read()
            .expect("vault index")
            .values()
            .cloned()
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| left.path.cmp(&right.path));
        entries
    }
}
