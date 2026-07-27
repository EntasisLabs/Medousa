//! On-disk grapheme script index + bodies.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::RwLock;

use anyhow::{Context, Result, bail};
use chrono::Utc;
use once_cell::sync::Lazy;
use sha2::{Digest, Sha256};

use crate::session;

use super::entry::{GraphemeScriptEntry, slugify_script_id};

const INDEX_FILE: &str = "index.jsonl";
const SCRIPTS_DIR: &str = "scripts";

static STORE: Lazy<GraphemeScriptStore> = Lazy::new(GraphemeScriptStore::new);

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
        session::medousa_data_dir().join("grapheme-scripts")
    }

    fn index_path() -> PathBuf {
        Self::root_dir().join(INDEX_FILE)
    }

    fn scripts_dir() -> PathBuf {
        Self::root_dir().join(SCRIPTS_DIR)
    }

    fn body_path_for(id: &str) -> PathBuf {
        Self::scripts_dir().join(format!("{id}.grapheme"))
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
                if let Ok(entry) = serde_json::from_str::<GraphemeScriptEntry>(trimmed) {
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
        self.index.read().expect("grapheme script index").get(id).cloned()
    }

    pub fn read_body(&self, entry: &GraphemeScriptEntry) -> Result<String> {
        let path = Self::root_dir().join(&entry.body_path);
        ensure_within_root(&Self::root_dir(), &path)?;
        fs::read_to_string(&path).with_context(|| format!("read script body {}", path.display()))
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

        fs::create_dir_all(Self::scripts_dir())?;
        let absolute = Self::body_path_for(&id);
        let body_path = format!("{SCRIPTS_DIR}/{id}.grapheme");
        ensure_within_root(&Self::root_dir(), &absolute)?;
        fs::write(&absolute, body)?;

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
            intent: intent.map(|value| value.trim().to_string()).filter(|v| !v.is_empty()),
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

        let absolute = Self::root_dir().join(&entry.body_path);
        if absolute.exists() {
            ensure_within_root(&Self::root_dir(), &absolute)?;
            fs::remove_file(&absolute)
                .with_context(|| format!("delete script body {}", absolute.display()))?;
        }

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

/// Containment check that works when the leaf file does not exist yet.
/// Matches vault path handling so Windows `\\?\` / case / separator mismatches
/// do not block first save.
fn ensure_within_root(root: &Path, candidate: &Path) -> Result<()> {
    for component in candidate.components() {
        if matches!(component, Component::ParentDir) {
            bail!("script path escapes library root");
        }
    }

    let root_abs = absolutize_path_for_check(root);
    let enclosed = candidate
        .parent()
        .map(absolutize_path_for_check)
        .unwrap_or_else(|| absolutize_path_for_check(candidate));

    if !path_starts_with(&enclosed, &root_abs) {
        bail!("script path escapes library root");
    }
    Ok(())
}

fn absolutize_path_for_check(path: &Path) -> PathBuf {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    strip_verbatim_prefix(absolute)
}

#[cfg(windows)]
fn strip_verbatim_prefix(path: PathBuf) -> PathBuf {
    let display = path.display().to_string();
    if let Some(rest) = display.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path
    }
}

#[cfg(not(windows))]
fn strip_verbatim_prefix(path: PathBuf) -> PathBuf {
    path
}

fn path_starts_with(path: &Path, prefix: &Path) -> bool {
    let mut path_components = path.components();
    let mut prefix_components = prefix.components();

    loop {
        match (prefix_components.next(), path_components.next()) {
            (None, _) => return true,
            (Some(prefix_component), Some(path_component)) => {
                if !path_component_eq(path_component, prefix_component) {
                    return false;
                }
            }
            (Some(_), None) => return false,
        }
    }
}

fn path_component_eq(left: Component<'_>, right: Component<'_>) -> bool {
    match (left, right) {
        (Component::Prefix(left), Component::Prefix(right)) => {
            left.as_os_str().eq_ignore_ascii_case(right.as_os_str())
        }
        (Component::RootDir, Component::RootDir) => true,
        (Component::CurDir, Component::CurDir) => true,
        (Component::Normal(left), Component::Normal(right)) => left.eq_ignore_ascii_case(right),
        (left, right) => left.as_os_str() == right.as_os_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_within_root_allows_new_script_body_before_file_exists() {
        let base = std::env::temp_dir().join(format!(
            "medousa-grapheme-path-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let root = base.join("grapheme-scripts");
        let scripts = root.join("scripts");
        fs::create_dir_all(&scripts).expect("scripts dir");
        let body = scripts.join("hello-world.grapheme");

        ensure_within_root(&root, &body).expect("new script body should stay inside root");

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn ensure_within_root_rejects_paths_outside_root() {
        let base = std::env::temp_dir().join(format!(
            "medousa-grapheme-path-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let root = base.join("grapheme-scripts");
        fs::create_dir_all(&root).expect("root");
        let outside = base.join("outside.grapheme");

        assert!(ensure_within_root(&root, &outside).is_err());

        let _ = fs::remove_dir_all(base);
    }
}
