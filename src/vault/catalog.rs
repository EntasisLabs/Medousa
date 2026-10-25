//! Read-only root-qualified title/path search. Never changes the editing root or
//! publishes into the active vault's projection, buffers, or search index.
use super::{
    note::{extract_title, parse_frontmatter_tags, resolve_kind, strip_frontmatter},
    path::VaultPath,
};
use crate::{
    daemon_api::{VaultNoteSummary, VaultSearchHit, VaultSearchResponse},
    store_root::{StoreEntryKind, StoreRoot},
};
use anyhow::{Context, Result, bail};
use once_cell::sync::Lazy;
use std::{
    collections::HashMap,
    io::Read,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const MAX_ENTRIES: usize = 50_000;
const PREFIX_BYTES: u64 = 8 * 1024;
const MAX_PREFIX_BYTES: u64 = 256 * 1024 * 1024;
struct Catalog {
    at: Instant,
    notes: Arc<Vec<(VaultNoteSummary, String)>>,
}
type CatalogKey = (PathBuf, Option<PathBuf>);
static CATALOGS: Lazy<Mutex<HashMap<CatalogKey, Catalog>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub fn search_root(
    root_id: &str,
    query: &str,
    limit: usize,
    tags: Option<&str>,
) -> Result<VaultSearchResponse> {
    let root = super::roots::list_vault_root_views()
        .roots
        .into_iter()
        .find(|root| root.id == root_id)
        .ok_or_else(|| anyhow::anyhow!("Unknown vault root: {root_id}"))?;
    let path = PathBuf::from(root.path);
    let overlay_path = root
        .is_default
        .then(super::path::project_vault_overlay_root)
        .flatten();
    let key = (path.clone(), overlay_path.clone());
    let cached = CATALOGS
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(&key)
        .filter(|catalog| catalog.at.elapsed() < Duration::from_secs(3))
        .map(|catalog| catalog.notes.clone());
    let notes = match cached {
        Some(notes) => notes,
        None => {
            let files =
                StoreRoot::open_nofollow(&path).context("Open configured vault for search")?;
            let mut notes = scan(&files)?;
            if let Some(overlay_path) = overlay_path {
                let overlay = match StoreRoot::open_nofollow(&overlay_path) {
                    Ok(files) => Some(files),
                    Err(error) if error.is_not_found() => None,
                    Err(error) => return Err(error.into()),
                };
                if let Some(overlay) = overlay {
                    let existing = notes
                        .iter()
                        .map(|(note, _)| note.path.clone())
                        .collect::<std::collections::HashSet<_>>();
                    notes.extend(
                        scan(&overlay)?
                            .into_iter()
                            .filter(|(note, _)| !existing.contains(&note.path)),
                    );
                }
            }
            let notes = Arc::new(notes);
            let mut cache = CATALOGS.lock().unwrap_or_else(|p| p.into_inner());
            if cache.len() >= 16
                && let Some(oldest) = cache
                    .iter()
                    .min_by_key(|(_, value)| value.at)
                    .map(|(key, _)| key.clone())
            {
                cache.remove(&oldest);
            }
            cache.insert(
                key,
                Catalog {
                    at: Instant::now(),
                    notes: notes.clone(),
                },
            );
            notes
        }
    };
    let query = query.trim().to_lowercase();
    let filters = tags
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .collect::<Vec<_>>();
    let mut ranked = notes
        .iter()
        .filter(|(note, _)| {
            filters.iter().all(|tag| {
                note.tags
                    .iter()
                    .any(|value| value.eq_ignore_ascii_case(tag))
            })
        })
        .filter_map(|(note, snippet)| {
            let score = fuzzy_score(
                &query,
                &format!("{} {}", note.title, note.path).to_lowercase(),
            );
            (score > 0).then_some((score, note, snippet))
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| b.1.modified_at_utc.cmp(&a.1.modified_at_utc))
            .then_with(|| a.1.path.cmp(&b.1.path))
    });
    Ok(VaultSearchResponse {
        root_id: Some(root_id.into()),
        query,
        indexing: None,
        hits: ranked
            .into_iter()
            .take(limit.clamp(1, 100))
            .map(|(score, note, snippet)| VaultSearchHit {
                note: note.clone(),
                score: (score as f32 / 300.0).min(1.0),
                matched_terms: vec![],
                snippet: Some(snippet.clone()),
            })
            .collect(),
    })
}

fn fuzzy_score(query: &str, text: &str) -> usize {
    if query.is_empty() {
        return 1;
    }
    if text.starts_with(query) {
        return 300;
    }
    if text.contains(query) {
        return 200;
    }
    let mut chars = query.chars();
    let mut next = chars.next();
    for character in text.chars() {
        if next == Some(character) {
            next = chars.next();
            if next.is_none() {
                return 100;
            }
        }
    }
    0
}

fn scan(files: &StoreRoot) -> Result<Vec<(VaultNoteSummary, String)>> {
    let mut folders: Vec<Option<VaultPath>> = vec![None];
    let mut notes = Vec::new();
    let mut visited = 0;
    let mut bytes = 0;
    while let Some(folder) = folders.pop() {
        let entries = match folder.as_ref() {
            Some(path) => files.list_directory_utf8(path)?,
            None => files.list_root_utf8()?,
        };
        for entry in entries {
            visited += 1;
            if visited > MAX_ENTRIES {
                bail!("Vault search exceeds the bounded directory scan limit");
            }
            if entry.name.starts_with('.') {
                continue;
            }
            let path = match &folder {
                Some(folder) => folder.join_segment(&entry.name),
                None => VaultPath::parse(&entry.name),
            };
            let Ok(path) = path else {
                continue;
            };
            if entry.kind == StoreEntryKind::Directory {
                folders.push(Some(path));
                continue;
            }
            if entry.kind != StoreEntryKind::File
                || !super::store::VaultStore::is_indexable_vault_file(std::path::Path::new(
                    path.as_str(),
                ))
            {
                continue;
            }
            let mut prefix = Vec::new();
            files
                .open_read_file(&path)?
                .take(PREFIX_BYTES)
                .read_to_end(&mut prefix)?;
            bytes += prefix.len() as u64;
            if bytes > MAX_PREFIX_BYTES {
                bail!("Vault search exceeds the bounded text scan limit");
            }
            let body = String::from_utf8_lossy(&prefix);
            let (text, frontmatter) = strip_frontmatter(&body);
            let note = VaultNoteSummary {
                path: path.as_str().into(),
                title: extract_title(&body, path.as_str()),
                modified_at_utc: entry.modified.unwrap_or(std::time::UNIX_EPOCH).into(),
                kind: resolve_kind(path.as_str(), frontmatter),
                tags: frontmatter.map(parse_frontmatter_tags).unwrap_or_default(),
            };
            notes.push((note, text.chars().take(600).collect()));
        }
    }
    Ok(notes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn configured_roots_keep_duplicate_relative_names_separate_without_writes() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        std::fs::write(a.path().join("same.md"), "# Medousa note\nbody").unwrap();
        std::fs::write(
            b.path().join("same.md"),
            "---\ntags: [design]\n---\n# Obsidian note\nbody",
        )
        .unwrap();
        let first =
            scan(&StoreRoot::open_nofollow(&a.path().canonicalize().unwrap()).unwrap()).unwrap();
        let second =
            scan(&StoreRoot::open_nofollow(&b.path().canonicalize().unwrap()).unwrap()).unwrap();
        assert_eq!(first[0].0.path, second[0].0.path);
        assert_eq!(first[0].0.title, "Medousa note");
        assert_eq!(second[0].0.title, "Obsidian note");
        assert_eq!(second[0].0.tags, vec!["design"]);
        assert_eq!(std::fs::read_dir(a.path()).unwrap().count(), 1);
        assert_eq!(std::fs::read_dir(b.path()).unwrap().count(), 1);
    }
    #[test]
    fn metadata_and_title_search_keep_unicode_and_ignore_hidden_assets() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".obsidian")).unwrap();
        std::fs::write(dir.path().join(".obsidian/hidden.md"), "# Hidden").unwrap();
        std::fs::write(
            dir.path().join("unicode.md"),
            format!("# 日本語\n{}", "👩🏽‍💻".repeat(1000)),
        )
        .unwrap();
        let notes =
            scan(&StoreRoot::open_nofollow(&dir.path().canonicalize().unwrap()).unwrap()).unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].0.title, "日本語");
        assert!(fuzzy_score("日本", &notes[0].0.title) > 0);
        assert!(fuzzy_score("nmd", "note.md") > 0);
        assert_eq!(fuzzy_score("missing", "note.md"), 0);
    }
    #[cfg(unix)]
    #[test]
    fn search_never_traverses_links_outside_the_configured_root() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("safe.md"), "# Safe").unwrap();
        std::fs::write(outside.path().join("secret.md"), "# Outside").unwrap();
        symlink(outside.path(), root.path().join("linked")).unwrap();
        symlink(
            outside.path().join("secret.md"),
            root.path().join("linked.md"),
        )
        .unwrap();
        let notes =
            scan(&StoreRoot::open_nofollow(&root.path().canonicalize().unwrap()).unwrap()).unwrap();
        assert_eq!(
            notes
                .iter()
                .map(|(note, _)| note.path.as_str())
                .collect::<Vec<_>>(),
            ["safe.md"]
        );
    }
}
