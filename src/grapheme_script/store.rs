//! On-disk grapheme script index + bodies.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

use anyhow::{Context, Result, bail};
use chrono::Utc;
use once_cell::sync::Lazy;
use sha2::{Digest, Sha256};

use medousa_types::authority_id::GraphemeScriptId;

use crate::store_root::{StorePath, StoreRoot};

use super::entry::{GraphemeScriptEntry, slugify_script_id};

const INDEX_FILE: &str = "index.jsonl";
const SCRIPTS_DIR: &str = "scripts";

static STORE: Lazy<GraphemeScriptStore> = Lazy::new(GraphemeScriptStore::new);
static GRAPHEME_SCRIPT_ROOT: OnceLock<PathBuf> = OnceLock::new();

/// Bind the script library to the active daemon deployment root.
pub fn configure_grapheme_script_root(root: PathBuf) -> Result<(), String> {
    if let Some(existing) = GRAPHEME_SCRIPT_ROOT.get() {
        return if existing == &root {
            Ok(())
        } else {
            Err(format!(
                "grapheme script root is already configured as {}",
                existing.display()
            ))
        };
    }
    GRAPHEME_SCRIPT_ROOT
        .set(root)
        .map_err(|_| "grapheme script root was configured concurrently".to_string())
}

#[cfg(test)]
mod test_override {
    use std::cell::RefCell;
    use std::path::PathBuf;

    thread_local! {
        static OVERRIDE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    }

    pub fn set(path: Option<PathBuf>) {
        OVERRIDE.with(|cell| *cell.borrow_mut() = path);
    }

    pub fn get() -> Option<PathBuf> {
        OVERRIDE.with(|cell| cell.borrow().clone())
    }
}

/// Test-only redirect for [`GraphemeScriptStore::root_dir`]. Does not touch product config.
#[cfg(test)]
pub fn set_test_grapheme_script_root_override(path: Option<PathBuf>) {
    test_override::set(path);
}

/// Run grapheme-script work against an isolated temp root (not the live library).
#[cfg(test)]
pub fn with_temp_grapheme_scripts<T>(f: impl FnOnce() -> T) -> T {
    use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
    use std::sync::{Mutex, OnceLock};

    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _lock = LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let base = std::env::temp_dir().join(format!(
        "medousa-grapheme-scripts-test-{}",
        uuid::Uuid::new_v4().simple()
    ));
    fs::create_dir_all(base.join(SCRIPTS_DIR)).expect("temp grapheme scripts root");
    let base = fs::canonicalize(base).expect("canonical temp grapheme scripts root");
    set_test_grapheme_script_root_override(Some(base.clone()));
    grapheme_script_store().reload_from_disk();
    let result = catch_unwind(AssertUnwindSafe(f));
    set_test_grapheme_script_root_override(None);
    grapheme_script_store().reload_from_disk();
    let _ = fs::remove_dir_all(&base);
    match result {
        Ok(value) => value,
        Err(payload) => resume_unwind(payload),
    }
}

pub fn grapheme_script_store() -> &'static GraphemeScriptStore {
    &STORE
}

pub struct GraphemeScriptStore {
    index: RwLock<HashMap<String, GraphemeScriptEntry>>,
}

impl GraphemeScriptStore {
    fn new() -> Self {
        let store = Self {
            index: RwLock::new(HashMap::new()),
        };
        store.reload_from_disk();
        store
    }

    pub fn root_dir() -> PathBuf {
        #[cfg(test)]
        if let Some(path) = test_override::get() {
            return path;
        }
        if let Some(root) = GRAPHEME_SCRIPT_ROOT.get() {
            return root.clone();
        }
        #[cfg(feature = "full-daemon")]
        {
            crate::session::medousa_data_dir().join("grapheme-scripts")
        }
        #[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
        {
            panic!("embedded grapheme script root must be configured before use")
        }
    }

    fn index_path() -> PathBuf {
        Self::root_dir().join(INDEX_FILE)
    }

    fn scripts_dir() -> PathBuf {
        Self::root_dir().join(SCRIPTS_DIR)
    }

    fn body_path_for(id: &GraphemeScriptId) -> Result<StorePath> {
        Ok(StorePath::parse(&format!(
            "{SCRIPTS_DIR}/{}.grapheme",
            id.storage_key().as_str()
        ))?)
    }

    fn legacy_body_path_for(id: &GraphemeScriptId) -> Result<StorePath> {
        Ok(StorePath::parse(&format!(
            "{SCRIPTS_DIR}/{}.grapheme",
            id.as_str()
        ))?)
    }

    pub fn reload_from_disk(&self) {
        let _ = fs::create_dir_all(Self::scripts_dir());
        let mut map = HashMap::new();
        if let Ok(file) = File::open(Self::index_path()) {
            for line in BufReader::new(file).lines().map_while(Result::ok) {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if let Ok(entry) = serde_json::from_str::<GraphemeScriptEntry>(trimmed)
                    && GraphemeScriptId::parse(&entry.id).is_ok()
                {
                    map.insert(entry.id.clone(), entry);
                }
            }
        }
        *self.index.write().expect("grapheme script index") = map;
    }

    fn persist_index(&self) -> Result<()> {
        let entries = self.index.read().expect("grapheme script index").clone();
        let mut lines = entries.values().cloned().collect::<Vec<_>>();
        lines.sort_by(|a, b| a.id.cmp(&b.id));
        fs::create_dir_all(Self::root_dir())?;
        let path = Self::index_path();
        let mut file = File::create(&path)?;
        for entry in lines {
            let line = serde_json::to_string(&entry)?;
            writeln!(file, "{line}")?;
        }
        Ok(())
    }

    pub fn all_entries(&self) -> Vec<GraphemeScriptEntry> {
        let mut entries = self
            .index
            .read()
            .expect("grapheme script index")
            .values()
            .cloned()
            .collect::<Vec<_>>();
        entries.sort_by_key(|b| std::cmp::Reverse(b.updated_at_utc));
        entries
    }

    pub fn get(&self, id: &str) -> Option<GraphemeScriptEntry> {
        self.index
            .read()
            .expect("grapheme script index")
            .get(id)
            .cloned()
    }

    pub fn read_body(&self, entry: &GraphemeScriptEntry) -> Result<String> {
        let id = GraphemeScriptId::parse(&entry.id)?;
        let root = StoreRoot::open_nofollow(&Self::root_dir())?;
        let path = Self::body_path_for(&id)?;
        let raw = match root.read_limited(&path, 4 * 1024 * 1024) {
            Ok(raw) => raw,
            Err(error) if error.is_not_found() => {
                root.read_limited(&Self::legacy_body_path_for(&id)?, 4 * 1024 * 1024)?
            }
            Err(error) => return Err(error.into()),
        };
        String::from_utf8(raw).context("script body is not UTF-8")
    }

    #[allow(clippy::too_many_arguments)]
    pub fn save_script(
        &self,
        id: Option<&str>,
        name: &str,
        body: &str,
        modules: Vec<String>,
        tags: Vec<String>,
        intent: Option<String>,
        source_session_id: Option<String>,
    ) -> Result<GraphemeScriptEntry> {
        let name = name.trim();
        if name.is_empty() {
            bail!("name is required");
        }
        if body.trim().is_empty() {
            bail!("body is required");
        }

        let id = id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(slugify_script_id)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| slugify_script_id(name));
        if id.is_empty() {
            bail!("could not derive script id");
        }

        let typed_id = GraphemeScriptId::parse(&id)?;
        let root = StoreRoot::open_or_create_nofollow(&Self::root_dir())?;
        let relative = Self::body_path_for(&typed_id)?;
        root.atomic_write(&relative, body.as_bytes())?;
        let body_path = format!("{SCRIPTS_DIR}/{}.grapheme", typed_id.storage_key().as_str());

        let body_hash = content_hash(body);
        let now = Utc::now();
        let version = self
            .get(&id)
            .map(|existing| existing.version.saturating_add(1))
            .unwrap_or(1);
        let created_at_utc = self
            .get(&id)
            .map(|existing| existing.created_at_utc)
            .unwrap_or(now);

        let entry = GraphemeScriptEntry {
            id: id.clone(),
            name: name.to_string(),
            modules: normalize_tokens(modules),
            tags: normalize_tokens(tags),
            intent: intent
                .map(|value| value.trim().to_string())
                .filter(|v| !v.is_empty()),
            version,
            body_path,
            body_hash,
            created_at_utc,
            updated_at_utc: now,
            source_session_id: source_session_id
                .map(|value| value.trim().to_string())
                .filter(|v| !v.is_empty()),
        };

        self.index
            .write()
            .expect("grapheme script index")
            .insert(id, entry.clone());
        self.persist_index()?;
        Ok(entry)
    }

    pub fn delete_script(&self, id: &str) -> Result<GraphemeScriptEntry> {
        let id = id.trim();
        if id.is_empty() {
            bail!("script_id is required");
        }
        let entry = self
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("grapheme script not found: {id}"))?;

        let typed_id = GraphemeScriptId::parse(&entry.id)?;
        let root = StoreRoot::open_or_create_nofollow(&Self::root_dir())?;
        root.remove_file(&Self::body_path_for(&typed_id)?)?;
        root.remove_file(&Self::legacy_body_path_for(&typed_id)?)?;

        self.index
            .write()
            .expect("grapheme script index")
            .remove(id);
        self.persist_index()?;
        Ok(entry)
    }

    pub fn rename_script(&self, id: &str, name: &str) -> Result<GraphemeScriptEntry> {
        let entry = self
            .get(id.trim())
            .ok_or_else(|| anyhow::anyhow!("grapheme script not found: {id}"))?;
        let body = self.read_body(&entry)?;
        self.save_script(
            Some(&entry.id),
            name,
            &body,
            entry.modules,
            entry.tags,
            entry.intent,
            entry.source_session_id,
        )
    }
}

fn normalize_tokens(values: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for value in values {
        let trimmed = value.trim().to_ascii_lowercase();
        if trimmed.is_empty() {
            continue;
        }
        if !out.iter().any(|existing| existing == &trimmed) {
            out.push(trimmed);
        }
    }
    out
}

pub fn content_hash(body: &str) -> String {
    let digest = Sha256::digest(body.as_bytes());
    format!("sha256:{digest:x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_body_path_is_opaque() {
        let id = GraphemeScriptId::parse("hello-world").unwrap();
        let path = GraphemeScriptStore::body_path_for(&id).unwrap();
        assert!(path.file_name().starts_with("gs1-"));
        assert!(!path.file_name().contains("hello-world"));
    }
}
