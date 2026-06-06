//! Vault filesystem store + on-disk index.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use anyhow::{Context, Result, bail};
use chrono::Utc;
use once_cell::sync::Lazy;
use crate::vault::note::{VaultIndexEntry, VaultNoteSource, build_index_entry, content_hash};
use crate::vault::path::{
    normalize_vault_path, project_vault_overlay_root, resolve_overlay_note_path,
    resolve_user_note_path, trash_path_for, user_vault_root,
};

const INDEX_FILE: &str = "index.jsonl";

static STORE: Lazy<VaultStore> = Lazy::new(VaultStore::new);

pub fn vault_store() -> &'static VaultStore {
    &STORE
}

pub struct VaultStore {
    index: RwLock<HashMap<String, VaultIndexEntry>>,
}

impl VaultStore {
    fn new() -> Self {
        let store = Self {
            index: RwLock::new(HashMap::new()),
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
        let mut discovered = HashMap::new();

        self.scan_root(&user_vault_root(), VaultNoteSource::User, &mut discovered)?;
        if let Some(overlay) = project_vault_overlay_root() {
            if overlay.is_dir() {
                self.scan_root(&overlay, VaultNoteSource::ProjectOverlay, &mut discovered)?;
            }
        }

        {
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
        self.persist_index();
        Ok(())
    }

    fn scan_root(
        &self,
        root: &Path,
        source: VaultNoteSource,
        out: &mut HashMap<String, VaultIndexEntry>,
    ) -> Result<()> {
        if !root.is_dir() {
            return Ok(());
        }
        self.scan_dir(root, root, source, out)
    }

    fn scan_dir(
        &self,
        root: &Path,
        dir: &Path,
        source: VaultNoteSource,
        out: &mut HashMap<String, VaultIndexEntry>,
    ) -> Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().and_then(|name| name.to_str()) == Some(".trash") {
                    continue;
                }
                self.scan_dir(root, &path, source.clone(), out)?;
                continue;
            }
            if !path.is_file() {
                continue;
            }
            if path.file_name().and_then(|name| name.to_str()) == Some(INDEX_FILE) {
                continue;
            }
            let relative = path
                .strip_prefix(root)
                .ok()
                .and_then(|value| value.to_str())
                .unwrap_or_default();
            let normalized = match normalize_vault_path(relative) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let body = fs::read_to_string(&path)
                .with_context(|| format!("read vault note {}", path.display()))?;
            let metadata = fs::metadata(&path)?;
            let modified = metadata
                .modified()
                .ok()
                .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| {
                    chrono::DateTime::<Utc>::from_timestamp(duration.as_secs() as i64, 0)
                        .unwrap_or_else(Utc::now)
                })
                .unwrap_or_else(Utc::now);
            let created = metadata
                .created()
                .ok()
                .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| {
                    chrono::DateTime::<Utc>::from_timestamp(duration.as_secs() as i64, 0)
                        .unwrap_or_else(Utc::now)
                })
                .unwrap_or_else(Utc::now);

            let index_entry = build_index_entry(
                &normalized,
                &body,
                created,
                modified,
                source.clone(),
            );

            match out.get(&normalized) {
                Some(existing) if existing.source == VaultNoteSource::User => continue,
                _ => {
                    out.insert(normalized, index_entry);
                }
            }
        }
        Ok(())
    }

    pub fn list_entries(&self, prefix: Option<&str>, limit: usize) -> Vec<VaultIndexEntry> {
        let _ = self.refresh_from_disk();
        let index = self.index.read().expect("vault index");
        let mut entries = index.values().cloned().collect::<Vec<_>>();
        if let Some(prefix) = prefix.map(str::trim).filter(|value| !value.is_empty()) {
            entries.retain(|entry| entry.path.starts_with(prefix));
        }
        entries.sort_by(|left, right| right.modified_at_utc.cmp(&left.modified_at_utc));
        entries.truncate(limit);
        entries
    }

    pub fn get_entry(&self, path: &str) -> Option<VaultIndexEntry> {
        let _ = self.refresh_from_disk();
        self.index.read().expect("vault index").get(path).cloned()
    }

    pub fn read_content(&self, path: &str) -> Result<String> {
        if let Ok(user_path) = resolve_user_note_path(path) {
            if user_path.is_file() {
                return fs::read_to_string(&user_path)
                    .with_context(|| format!("read {}", user_path.display()));
            }
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

        let entry = build_index_entry(
            &normalized,
            content,
            created_at,
            modified_at,
            VaultNoteSource::User,
        );
        self.index
            .write()
            .expect("vault index")
            .insert(normalized.clone(), entry.clone());
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
        self.persist_index();
        Ok(())
    }

    pub fn backlinks_for(&self, path: &str) -> Vec<String> {
        let _ = self.refresh_from_disk();
        let normalized = normalize_vault_path(path).ok();
        let Some(target) = normalized else {
            return Vec::new();
        };
        let index = self.index.read().expect("vault index");
        let mut backlinks = Vec::new();
        for entry in index.values() {
            if entry.wikilinks_out.iter().any(|link| link == &target) {
                backlinks.push(entry.path.clone());
            }
        }
        backlinks.sort();
        backlinks
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
