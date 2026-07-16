//! Vault filesystem store + on-disk index.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;

use crate::vault::links::{VaultLinkIndex, load_link_index_from_disk, persist_link_index};
use crate::vault::note::{VaultIndexEntry, VaultNoteSource, build_index_entry, content_hash};
use crate::vault::path::{
    normalize_vault_path, project_vault_overlay_root, resolve_overlay_note_path,
    resolve_user_note_path, trash_path_for, user_vault_root,
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

    fn index_path() -> PathBuf {
        user_vault_root().join(INDEX_FILE)
    }

    fn reload_from_disk(&self) {
        let _ = fs::create_dir_all(user_vault_root());
        let mut map = HashMap::new();
        if let Ok(file) = File::open(Self::index_path()) {
            for line in BufReader::new(file).lines().map_while(Result::ok) {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if let Ok(entry) = serde_json::from_str::<VaultIndexEntry>(trimmed) {
                    map.insert(entry.path.clone(), entry);
                }
            }
        }
        *self.index.write().expect("vault index") = map;
    }

    fn persist_index(&self) {
        let entries = self.index.read().expect("vault index").clone();
        let _ = fs::create_dir_all(user_vault_root());
        let path = Self::index_path();
        let mut lines = entries
            .values()
            .cloned()
            .collect::<Vec<_>>();
        lines.sort_by(|left, right| left.path.cmp(&right.path));
        let mut file = File::create(&path).expect("vault index write");
        for entry in lines {
            if let Ok(line) = serde_json::to_string(&entry) {
                let _ = writeln!(file, "{line}");
            }
        }
    }

    pub fn refresh_from_disk(&self) -> Result<()> {
        self.reload_from_disk();
        let mut drafts = Vec::new();

        self.scan_root(&user_vault_root(), VaultNoteSource::User, &mut drafts)?;
        if let Some(overlay) = project_vault_overlay_root()
            && overlay.is_dir() {
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
        self.merge_discovered(discovered);
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
        root: &Path,
        source: VaultNoteSource,
        drafts: &mut Vec<ScanDraft>,
    ) -> Result<()> {
        if !root.is_dir() {
            return Ok(());
        }
        self.scan_dir(root, root, source, drafts)
    }

    fn scan_dir(
        &self,
        root: &Path,
        dir: &Path,
        source: VaultNoteSource,
        drafts: &mut Vec<ScanDraft>,
    ) -> Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                let dir_name = path.file_name().and_then(|name| name.to_str()).unwrap_or("");
                if dir_name == ".trash" || dir_name == ".obsidian" || dir_name == ".git" {
                    continue;
                }
                if dir_name.starts_with('.') {
                    continue;
                }
                self.scan_dir(root, &path, source.clone(), drafts)?;
                continue;
            }
            if !path.is_file() {
                continue;
            }
            let file_name = path.file_name().and_then(|name| name.to_str()).unwrap_or("");
            if file_name == INDEX_FILE || file_name == LINKS_FILE {
                continue;
            }
            if !Self::is_indexable_vault_file(&path) {
                continue;
            }
            let relative = path
                .strip_prefix(root)
                .ok()
                .and_then(|value| value.to_str())
                .map(|value| value.replace('\\', "/"))
                .unwrap_or_default();
            let normalized = match normalize_vault_path(&relative) {
                Ok(value) => value,
                Err(_) => continue,
            };
            // Skip binary / non-UTF8 instead of failing the whole vault scan.
            let body = match fs::read_to_string(&path) {
                Ok(body) => body,
                Err(_) => continue,
            };
            let metadata = match fs::metadata(&path) {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };
            let modified = Self::file_timestamp(metadata.modified());
            let created = Self::file_timestamp(metadata.created());

            drafts.push(ScanDraft {
                path: normalized,
                body,
                created_at: created,
                modified_at: modified,
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

    fn merge_discovered(&self, discovered: HashMap<String, VaultIndexEntry>) {
        let mut index = self.index.write().expect("vault index");
        for (path, entry) in discovered {
            match index.get(&path) {
                Some(existing) if existing.source == VaultNoteSource::ProjectOverlay => {
                    if entry.source == VaultNoteSource::User {
                        index.insert(path, entry);
                    }
                }
                Some(existing) if existing.content_hash != entry.content_hash => {
                    let created = existing.created_at_utc;
                    let mut merged = entry;
                    merged.created_at_utc = created;
                    index.insert(path, merged);
                }
                None => {
                    index.insert(path, entry);
                }
                _ => {}
            }
        }
    }

    pub fn list_entries(&self, prefix: Option<&str>, limit: usize) -> Vec<VaultIndexEntry> {
        let _ = self.refresh_from_disk();
        let index = self.index.read().expect("vault index");
        let mut entries = index.values().cloned().collect::<Vec<_>>();
        if let Some(prefix) = prefix.map(str::trim).filter(|value| !value.is_empty()) {
            entries.retain(|entry| entry.path.starts_with(prefix));
        }
        entries.sort_by_key(|right| std::cmp::Reverse(right.modified_at_utc));
        entries.truncate(limit);
        entries
    }

    pub fn get_entry(&self, path: &str) -> Option<VaultIndexEntry> {
        let _ = self.refresh_from_disk();
        self.index.read().expect("vault index").get(path).cloned()
    }

    pub fn read_content(&self, path: &str) -> Result<String> {
        if let Ok(user_path) = resolve_user_note_path(path)
            && user_path.is_file() {
                return fs::read_to_string(&user_path)
                    .with_context(|| format!("read {}", user_path.display()));
            }
        if let Ok(Some(overlay_path)) = resolve_overlay_note_path(path) {
            return fs::read_to_string(&overlay_path)
                .with_context(|| format!("read {}", overlay_path.display()));
        }
        bail!("vault note not found: {path}")
    }

    pub fn write_content(
        &self,
        path: &str,
        content: &str,
        if_match: Option<&str>,
    ) -> Result<VaultIndexEntry> {
        let normalized = normalize_vault_path(path)?;
        let absolute = resolve_user_note_path(&normalized)?;
        if let Some(expected) = if_match.map(str::trim).filter(|value| !value.is_empty()) {
            if absolute.is_file() {
                let existing = fs::read_to_string(&absolute)?;
                let actual = content_hash(&existing);
                if actual != expected {
                    bail!("content_hash mismatch (If-Match failed)");
                }
            } else {
                bail!("content_hash mismatch (note does not exist)");
            }
        }

        if let Some(parent) = absolute.parent() {
            fs::create_dir_all(parent)?;
        }

        let created_at = if absolute.is_file() {
            self.get_entry(&normalized)
                .map(|entry| entry.created_at_utc)
                .unwrap_or_else(Utc::now)
        } else {
            Utc::now()
        };
        fs::write(&absolute, content)?;
        let modified_at = fs::metadata(&absolute)
            .ok()
            .and_then(|meta| meta.modified().ok())
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
        let normalized = normalize_vault_path(path)?;
        let absolute = resolve_user_note_path(&normalized)?;
        if !absolute.is_file() {
            bail!("vault note not found: {normalized}");
        }

        let trash = trash_path_for(&normalized)?;
        if let Some(parent) = trash.parent() {
            fs::create_dir_all(parent)?;
        }
        if trash.exists() {
            fs::remove_file(&trash)?;
        }
        fs::rename(&absolute, &trash)?;
        self.index
            .write()
            .expect("vault index")
            .remove(&normalized);
        self.rebuild_link_index();
        self.persist_index();
        Ok(())
    }

    pub fn note_exists(&self, path: &str) -> bool {
        let _ = self.refresh_from_disk();
        normalize_vault_path(path)
            .ok()
            .and_then(|normalized| self.index.read().expect("vault index").get(&normalized).cloned())
            .is_some()
    }

    pub fn backlinks_for(&self, path: &str) -> Vec<String> {
        let _ = self.refresh_from_disk();
        let normalized = match normalize_vault_path(path) {
            Ok(value) => value,
            Err(_) => return Vec::new(),
        };
        self.link_index
            .read()
            .expect("vault links")
            .backlinks_for(&normalized)
    }

    fn file_timestamp(
        value: Result<std::time::SystemTime, std::io::Error>,
    ) -> DateTime<Utc> {
        value
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| {
                DateTime::<Utc>::from_timestamp(duration.as_secs() as i64, 0).unwrap_or_else(Utc::now)
            })
            .unwrap_or_else(Utc::now)
    }

    pub fn all_entries(&self) -> Vec<VaultIndexEntry> {
        let _ = self.refresh_from_disk();
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
