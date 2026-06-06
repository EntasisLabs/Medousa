//! Vault note metadata extraction.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::daemon_api::VaultNote;

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
    let frontmatter = &rest[..end];
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

pub fn parse_wikilinks(body: &str) -> Vec<String> {
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
                if !raw.is_empty() && !raw.contains('|') {
                    if let Ok(path) = crate::vault::path::normalize_vault_path(&format!(
                        "{}.md",
                        raw.trim_end_matches(".md")
                    )) {
                        if !links.iter().any(|existing| existing == &path) {
                            links.push(path);
                        }
                    }
                }
                index = end + 2;
                continue;
            }
        }
        index += 1;
    }
    links
}

pub fn build_index_entry(
    path: &str,
    body: &str,
    created_at: DateTime<Utc>,
    modified_at: DateTime<Utc>,
    source: VaultNoteSource,
) -> VaultIndexEntry {
    let (_content, frontmatter) = strip_frontmatter(body);
    let tags = frontmatter
        .map(parse_frontmatter_tags)
        .unwrap_or_default();
    VaultIndexEntry {
        path: path.to_string(),
        title: extract_title(body, path),
        byte_size: body.len(),
        content_hash: content_hash(body),
        modified_at_utc: modified_at,
        created_at_utc: created_at,
        tags,
        wikilinks_out: parse_wikilinks(body),
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
    fn parse_tags_and_wikilinks() {
        let body = "---\ntags: [medousa, vault]\n---\n# Note\nSee [[projects/plan]]";
        let (_, fm) = strip_frontmatter(body);
        assert_eq!(
            parse_frontmatter_tags(fm.unwrap()),
            vec!["medousa".to_string(), "vault".to_string()]
        );
        assert!(parse_wikilinks(body).iter().any(|link| link.contains("projects")));
    }
}
