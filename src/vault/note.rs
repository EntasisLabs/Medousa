//! Vault note metadata extraction.

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::daemon_api::VaultNote;
use crate::vault::links::{merge_tags, parse_inline_tags, parse_raw_wikilinks, resolve_wikilink_target};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VaultNoteSource {
    User,
    ProjectOverlay,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VaultIndexEntry {
    pub path: String,
    pub title: String,
    pub byte_size: usize,
    pub content_hash: String,
    pub modified_at_utc: DateTime<Utc>,
    pub created_at_utc: DateTime<Utc>,
    pub tags: Vec<String>,
    pub wikilinks_out: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    pub source: VaultNoteSource,
}

impl VaultIndexEntry {
    pub fn to_vault_note(&self, backlinks: Vec<String>) -> VaultNote {
        VaultNote {
            path: self.path.clone(),
            title: self.title.clone(),
            byte_size: self.byte_size,
            content_hash: self.content_hash.clone(),
            modified_at_utc: self.modified_at_utc,
            created_at_utc: self.created_at_utc,
            tags: self.tags.clone(),
            wikilinks_out: self.wikilinks_out.clone(),
            kind: self
                .kind
                .clone()
                .unwrap_or_else(|| resolve_kind_from_path(&self.path)),
            backlinks,
        }
    }
}

pub fn content_hash(body: &str) -> String {
    let digest = Sha256::digest(body.as_bytes());
    format!("sha256:{digest:x}")
}

pub fn extract_title(body: &str, path: &str) -> String {
    for line in body.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix('#') {
            let title = rest.trim_start_matches('#').trim();
            if !title.is_empty() {
                return title.to_string();
            }
        }
    }

    path.rsplit('/')
        .next()
        .unwrap_or(path)
        .trim_end_matches(".md")
        .replace('-', " ")
}

pub fn strip_frontmatter(body: &str) -> (&str, Option<&str>) {
    let trimmed = body.trim_start();
    if !trimmed.starts_with("---") {
        return (body, None);
    }
    let Some(rest) = trimmed.strip_prefix("---") else {
        return (body, None);
    };
    let Some(end) = rest.find("\n---") else {
        return (body, None);
    };
    // Drop newlines that follow the opening `---` so rewrite does not grow blanks.
    let frontmatter = rest[..end].trim_matches('\n');
    let content = &rest[end + 4..];
    (content.trim_start(), Some(frontmatter))
}

pub fn parse_frontmatter_tags(frontmatter: &str) -> Vec<String> {
    let mut tags = Vec::new();
    for line in frontmatter.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("tags:") {
            let raw = rest.trim();
            if raw.starts_with('[') && raw.ends_with(']') {
                let inner = raw.trim_start_matches('[').trim_end_matches(']');
                for item in inner.split(',') {
                    let tag = item.trim().trim_matches('"').trim_matches('\'');
                    if !tag.is_empty() {
                        tags.push(tag.to_string());
                    }
                }
            } else {
                for item in raw.split(',') {
                    let tag = item.trim().trim_matches('"').trim_matches('\'');
                    if !tag.is_empty() {
                        tags.push(tag.to_string());
                    }
                }
            }
        }
    }
    tags
}

pub fn normalize_kind(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "daily" | "journal" => "daily".to_string(),
        "project" | "projects" => "project".to_string(),
        "ledger" | "finance" => "ledger".to_string(),
        "inbox" | "capture" => "inbox".to_string(),
        "bug" | "bugs" => "bug".to_string(),
        "note" | "notes" => "note".to_string(),
        other if !other.is_empty() => other.to_string(),
        _ => "note".to_string(),
    }
}

pub fn parse_frontmatter_kind(frontmatter: &str) -> Option<String> {
    for line in frontmatter.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("kind:") {
            let raw = rest.trim().trim_matches('"').trim_matches('\'');
            if !raw.is_empty() {
                return Some(normalize_kind(raw));
            }
        }
    }
    None
}

pub fn resolve_kind_from_path(path: &str) -> String {
    if path.starts_with("journal/") {
        "daily".to_string()
    } else if path.starts_with("projects/") {
        "project".to_string()
    } else if path.starts_with("finance/") {
        "ledger".to_string()
    } else if path.starts_with("inbox/") {
        "inbox".to_string()
    } else if path.starts_with("bugs/") {
        "bug".to_string()
    } else {
        "note".to_string()
    }
}

pub fn resolve_kind(path: &str, frontmatter: Option<&str>) -> String {
    frontmatter
        .and_then(parse_frontmatter_kind)
        .unwrap_or_else(|| resolve_kind_from_path(path))
}

pub fn resolve_wikilinks_for_body(
    path: &str,
    body: &str,
    known_paths: &HashSet<String>,
    entries: &[VaultIndexEntry],
) -> Vec<String> {
    let mut resolved = Vec::new();
    for raw in parse_raw_wikilinks(body) {
        if let Some(target) = resolve_wikilink_target(&raw, path, known_paths, entries)
            && !resolved.iter().any(|existing| existing == &target) {
                resolved.push(target);
            }
    }
    resolved
}

pub fn build_index_entry(
    path: &str,
    body: &str,
    created_at: DateTime<Utc>,
    modified_at: DateTime<Utc>,
    source: VaultNoteSource,
    known_paths: &HashSet<String>,
    entries: &[VaultIndexEntry],
) -> VaultIndexEntry {
    let (_content, frontmatter) = strip_frontmatter(body);
    let frontmatter_tags = frontmatter
        .map(parse_frontmatter_tags)
        .unwrap_or_default();
    let tags = crate::vault::semantic_tags::normalize_note_tags(merge_tags(
        frontmatter_tags,
        parse_inline_tags(body),
    ));
    let kind = resolve_kind(path, frontmatter);
    VaultIndexEntry {
        path: path.to_string(),
        title: extract_title(body, path),
        byte_size: body.len(),
        content_hash: content_hash(body),
        modified_at_utc: modified_at,
        created_at_utc: created_at,
        tags,
        wikilinks_out: resolve_wikilinks_for_body(path, body, known_paths, entries),
        kind: Some(kind),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_title_from_heading() {
        let body = "# Weekly Review\n\nSome text";
        assert_eq!(extract_title(body, "journal/x.md"), "Weekly Review");
    }

    #[test]
    fn parse_kind_from_frontmatter() {
        let body = "---\nkind: ledger\n---\n# Expenses";
        let (_, fm) = strip_frontmatter(body);
        let fm = fm.expect("frontmatter");
        assert_eq!(parse_frontmatter_kind(fm), Some("ledger".to_string()));
        assert_eq!(resolve_kind("finance/x.md", Some(fm)), "ledger");
    }

    #[test]
    fn infer_kind_from_path_when_frontmatter_missing() {
        assert_eq!(resolve_kind_from_path("journal/2026-06-08.md"), "daily");
        assert_eq!(resolve_kind_from_path("projects/plan.md"), "project");
    }

    #[test]
    fn parse_tags_and_wikilinks() {
        let body = "---\ntags: [medousa, vault]\n---\n# Note\nSee [[projects/plan]]";
        let (_, fm) = strip_frontmatter(body);
        assert_eq!(
            parse_frontmatter_tags(fm.unwrap()),
            vec!["medousa".to_string(), "vault".to_string()]
        );
        let known = HashSet::from(["projects/plan.md".to_string()]);
        let entries = vec![VaultIndexEntry {
            path: "projects/plan.md".to_string(),
            title: "Plan".to_string(),
            byte_size: body.len(),
            content_hash: content_hash(body),
            modified_at_utc: Utc::now(),
            created_at_utc: Utc::now(),
            tags: vec![],
            wikilinks_out: vec![],
            kind: Some("project".to_string()),
            source: VaultNoteSource::User,
        }];
        assert!(resolve_wikilinks_for_body("journal/note.md", body, &known, &entries)
            .iter()
            .any(|link| link.contains("projects")));
    }
}
