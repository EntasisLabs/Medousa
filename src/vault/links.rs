//! Wikilink resolution and persisted backlink index (Phase V1).

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::vault::note::VaultIndexEntry;
use crate::vault::path::{normalize_vault_path, user_vault_root};

const LINKS_FILE: &str = "links.jsonl";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultNoteLink {
    pub from_path: String,
    pub to_path: String,
}

#[derive(Debug, Clone, Default)]
pub struct VaultLinkIndex {
    pub forward: HashMap<String, Vec<String>>,
    pub backlinks: HashMap<String, Vec<String>>,
}

impl VaultLinkIndex {
    pub fn rebuild(entries: &[VaultIndexEntry]) -> Self {
        let mut forward: HashMap<String, Vec<String>> = HashMap::new();
        let mut backlinks: HashMap<String, Vec<String>> = HashMap::new();

        for entry in entries {
            if entry.wikilinks_out.is_empty() {
                continue;
            }
            forward.insert(entry.path.clone(), entry.wikilinks_out.clone());
            for target in &entry.wikilinks_out {
                backlinks
                    .entry(target.clone())
                    .or_default()
                    .push(entry.path.clone());
            }
        }

        for list in backlinks.values_mut() {
            list.sort();
            list.dedup();
        }

        Self { forward, backlinks }
    }

    pub fn backlinks_for(&self, path: &str) -> Vec<String> {
        self.backlinks.get(path).cloned().unwrap_or_default()
    }

    pub fn to_link_rows(&self) -> Vec<VaultNoteLink> {
        let mut rows = Vec::new();
        for (from_path, targets) in &self.forward {
            for to_path in targets {
                rows.push(VaultNoteLink {
                    from_path: from_path.clone(),
                    to_path: to_path.clone(),
                });
            }
        }
        rows.sort_by(|left, right| {
            left.from_path
                .cmp(&right.from_path)
                .then_with(|| left.to_path.cmp(&right.to_path))
        });
        rows
    }
}

pub fn resolve_wikilink_target(
    raw: &str,
    source_path: &str,
    known_paths: &HashSet<String>,
    entries: &[VaultIndexEntry],
) -> Option<String> {
    let token = raw
        .split('|')
        .next()
        .unwrap_or(raw)
        .trim()
        .trim_matches('"')
        .trim_matches('\'');
    if token.is_empty() {
        return None;
    }

    let mut candidates = Vec::new();
    if token.contains('/') {
        if let Ok(path) = normalize_vault_path(&format!("{}.md", token.trim_end_matches(".md"))) {
            candidates.push(path);
        }
    } else {
        let stem = token.trim_end_matches(".md");
        if let Ok(same_dir) = normalize_vault_path(&format!(
            "{}/{}.md",
            source_path.rsplit_once('/').map(|(dir, _)| dir).unwrap_or(""),
            stem
        )) {
            candidates.push(same_dir);
        }
        if let Ok(root) = normalize_vault_path(&format!("{stem}.md")) {
            candidates.push(root);
        }
        for path in known_paths {
            if path
                .rsplit('/')
                .next()
                .is_some_and(|filename| filename.trim_end_matches(".md") == stem)
            {
                candidates.push(path.clone());
            }
        }
        let stem_slug = slugify(stem);
        for entry in entries {
            if slugify(&entry.title) == stem_slug {
                candidates.push(entry.path.clone());
            }
        }
    }

    candidates.sort();
    candidates.dedup();
    candidates.into_iter().find(|path| known_paths.contains(path))
}

pub fn parse_raw_wikilinks(body: &str) -> Vec<String> {
    let mut links = Vec::new();
    let bytes = body.as_bytes();
    let mut index = 0usize;
    while index + 4 < bytes.len() {
        if bytes[index] == b'[' && bytes[index + 1] == b'[' {
            let start = index + 2;
            let mut end = start;
            while end + 1 < bytes.len() {
                if bytes[end] == b']' && bytes[end + 1] == b']' {
                    break;
                }
                end += 1;
            }
            if end + 1 < bytes.len() && bytes[end] == b']' && bytes[end + 1] == b']' {
                let raw = body[start..end].trim();
                if !raw.is_empty() {
                    links.push(raw.to_string());
                }
                index = end + 2;
                continue;
            }
        }
        index += 1;
    }
    links
}

pub fn parse_inline_tags(body: &str) -> Vec<String> {
    let (content, _) = crate::vault::note::strip_frontmatter(body);
    let mut tags = Vec::new();
    for token in content.split_whitespace() {
        if let Some(tag) = token.strip_prefix('#') {
            let cleaned = tag
                .trim_matches(|ch: char| !ch.is_alphanumeric() && ch != '-' && ch != '_');
            if !cleaned.is_empty() && !tags.iter().any(|existing| existing == cleaned) {
                tags.push(cleaned.to_string());
            }
        }
    }
    tags
}

pub fn merge_tags(frontmatter_tags: Vec<String>, inline_tags: Vec<String>) -> Vec<String> {
    let mut merged = frontmatter_tags;
    for tag in inline_tags {
        if !merged.iter().any(|existing| existing == &tag) {
            merged.push(tag);
        }
    }
    merged
}

fn slugify(raw: &str) -> String {
    raw.to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

pub fn links_path() -> PathBuf {
    user_vault_root().join(LINKS_FILE)
}

pub fn load_link_index_from_disk() -> VaultLinkIndex {
    let mut forward: HashMap<String, Vec<String>> = HashMap::new();
    let mut backlinks: HashMap<String, Vec<String>> = HashMap::new();
    let Ok(file) = File::open(links_path()) else {
        return VaultLinkIndex::default();
    };
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(row) = serde_json::from_str::<VaultNoteLink>(trimmed) else {
            continue;
        };
        forward
            .entry(row.from_path.clone())
            .or_default()
            .push(row.to_path.clone());
        backlinks
            .entry(row.to_path.clone())
            .or_default()
            .push(row.from_path.clone());
    }
    for list in forward.values_mut() {
        list.sort();
        list.dedup();
    }
    for list in backlinks.values_mut() {
        list.sort();
        list.dedup();
    }
    VaultLinkIndex { forward, backlinks }
}

pub fn persist_link_index(index: &VaultLinkIndex) -> std::io::Result<()> {
    let _ = std::fs::create_dir_all(user_vault_root());
    let rows = index.to_link_rows();
    let mut file = File::create(links_path())?;
    for row in rows {
        let line = serde_json::to_string(&row).unwrap_or_default();
        writeln!(file, "{line}")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    use chrono::Utc;

    fn entry(path: &str, _title: &str, body: &str) -> VaultIndexEntry {
        let known = HashSet::from([path.to_string()]);
        let seed = vec![VaultIndexEntry {
            path: path.to_string(),
            title: _title.to_string(),
            byte_size: body.len(),
            content_hash: String::new(),
            modified_at_utc: Utc::now(),
            created_at_utc: Utc::now(),
            tags: Vec::new(),
            wikilinks_out: Vec::new(),
            source: crate::vault::note::VaultNoteSource::User,
        }];
        crate::vault::note::build_index_entry(
            path,
            body,
            Utc::now(),
            Utc::now(),
            crate::vault::note::VaultNoteSource::User,
            &known,
            &seed,
        )
    }

    #[test]
    fn weekly_review_resolves_by_filename() {
        let entries = vec![
            entry("journal/weekly-review.md", "Weekly Review", "# Weekly Review"),
            entry(
                "journal/daily.md",
                "Daily",
                "# Daily\n\nSee [[weekly-review]]",
            ),
        ];
        let known: HashSet<_> = entries.iter().map(|e| e.path.clone()).collect();
        let target = resolve_wikilink_target(
            "weekly-review",
            "journal/daily.md",
            &known,
            &entries,
        )
        .expect("resolved");
        assert_eq!(target, "journal/weekly-review.md");
    }

    #[test]
    fn backlink_index_tracks_resolved_edges() {
        let known: HashSet<_> = [
            "journal/weekly-review.md".to_string(),
            "journal/daily.md".to_string(),
        ]
        .into_iter()
        .collect();
        let seed: Vec<VaultIndexEntry> = known
            .iter()
            .map(|path| VaultIndexEntry {
                path: path.clone(),
                title: path.clone(),
                byte_size: 1,
                content_hash: String::new(),
                modified_at_utc: Utc::now(),
                created_at_utc: Utc::now(),
                tags: Vec::new(),
                wikilinks_out: Vec::new(),
                source: crate::vault::note::VaultNoteSource::User,
            })
            .collect();
        let weekly = crate::vault::note::build_index_entry(
            "journal/weekly-review.md",
            "# Weekly Review",
            Utc::now(),
            Utc::now(),
            crate::vault::note::VaultNoteSource::User,
            &known,
            &seed,
        );
        let daily = crate::vault::note::build_index_entry(
            "journal/daily.md",
            "# Daily\n\nSee [[weekly-review]]",
            Utc::now(),
            Utc::now(),
            crate::vault::note::VaultNoteSource::User,
            &known,
            &seed,
        );
        let links = VaultLinkIndex::rebuild(&[weekly, daily]);
        assert_eq!(
            links.backlinks_for("journal/weekly-review.md"),
            vec!["journal/daily.md".to_string()]
        );
    }

    #[test]
    fn inline_tags_merge_with_frontmatter() {
        let body = "---\ntags: [alpha]\n---\n# Note\n#beta content";
        let (_, fm) = crate::vault::note::strip_frontmatter(body);
        let merged = merge_tags(
            crate::vault::note::parse_frontmatter_tags(fm.unwrap()),
            parse_inline_tags(body),
        );
        assert!(merged.contains(&"alpha".to_string()));
        assert!(merged.contains(&"beta".to_string()));
    }
}
