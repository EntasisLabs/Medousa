//! Locus-aligned semantic tags for vault notes (frontmatter + search).

use std::collections::BTreeSet;

use crate::locus_semantic_tags::normalize_semantic_tags;
use crate::vault::links::parse_inline_tags;
use crate::vault::note::{parse_frontmatter_tags, strip_frontmatter};

/// Default linking tags for vault notes — overlaps with Locus workshop vocabulary plus `vault`.
pub fn default_vault_semantic_tags(chat_session_id: Option<&str>) -> Vec<String> {
    let mut tags = vec!["medousa".to_string(), "vault".to_string()];
    if let Some(session_id) = chat_session_id.filter(|value| !value.trim().is_empty()) {
        tags.extend(crate::locus_semantic_tags::default_workshop_semantic_tags(session_id));
    } else {
        let profile_id = crate::user_profiles::resolve_workshop_identity_user_id();
        if let Some(slug) = crate::user_profiles::profile_slug_from_id(&profile_id) {
            if slug != crate::locus_memory::LOCUS_DEFAULT_TENANT {
                tags.push(format!("profile:{slug}"));
            }
        }
    }
    normalize_semantic_tags(tags.iter().map(String::as_str))
}

pub fn normalize_note_tags<I, S>(tags: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    normalize_semantic_tags(tags)
}

pub fn parse_tags_query(raw: Option<&str>) -> Vec<String> {
    raw.map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            normalize_semantic_tags(value.split(',').map(str::trim))
        })
        .unwrap_or_default()
}

pub fn entry_has_all_tags(entry_tags: &[String], required: &[String]) -> bool {
    let required = normalize_semantic_tags(required.iter().map(String::as_str));
    if required.is_empty() {
        return true;
    }
    let entry_set: BTreeSet<String> =
        normalize_semantic_tags(entry_tags.iter().map(String::as_str))
            .into_iter()
            .collect();
    required.iter().all(|tag| entry_set.contains(tag))
}

pub fn collect_distinct_tags(entries: &[crate::vault::note::VaultIndexEntry], prefix: Option<&str>, limit: usize) -> Vec<String> {
    let prefix = prefix
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase());
    let mut tags = BTreeSet::new();
    for entry in entries {
        for tag in &entry.tags {
            let normalized = normalize_semantic_tags([tag.as_str()]);
            for tag in normalized {
                if let Some(prefix) = prefix.as_deref() {
                    if !tag.starts_with(prefix) {
                        continue;
                    }
                }
                tags.insert(tag);
            }
        }
    }
    let mut out: Vec<String> = tags.into_iter().collect();
    out.sort();
    out.truncate(limit.max(1));
    out
}

fn format_tags_yaml_line(tags: &[String]) -> String {
    format!(
        "tags: [{}]",
        tags
            .iter()
            .map(|tag| format!("\"{}\"", tag.replace('"', "\\\"")))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

/// Merge tags into YAML frontmatter (creates frontmatter when absent).
pub fn upsert_frontmatter_tags(body: &str, tags: &[String]) -> String {
    let tags = normalize_semantic_tags(tags.iter().map(String::as_str));
    if tags.is_empty() {
        return body.to_string();
    }
    let tags_line = format_tags_yaml_line(&tags);
    let (content, frontmatter) = strip_frontmatter(body);
    if let Some(fm) = frontmatter {
        let mut lines: Vec<String> = Vec::new();
        let mut replaced = false;
        for line in fm.lines() {
            if line.trim_start().starts_with("tags:") {
                lines.push(tags_line.clone());
                replaced = true;
            } else {
                lines.push(line.to_string());
            }
        }
        if !replaced {
            lines.push(tags_line);
        }
        format!("---\n{}\n---\n\n{}", lines.join("\n"), content.trim_start())
    } else {
        format!("---\n{tags_line}\n---\n\n{}", body.trim_start())
    }
}

/// Merge existing + explicit + optional workshop defaults, then persist in frontmatter.
pub fn apply_semantic_tags_on_write(
    content: &str,
    session_id: Option<&str>,
    extra_tags: Option<&[String]>,
    auto_workshop_tags: bool,
) -> String {
    let (_, frontmatter) = strip_frontmatter(content);
    let mut tags = frontmatter
        .map(parse_frontmatter_tags)
        .unwrap_or_default();
    tags.extend(parse_inline_tags(content));
    if let Some(extra) = extra_tags {
        tags.extend(extra.iter().cloned());
    }
    if auto_workshop_tags {
        tags.extend(default_vault_semantic_tags(session_id));
    }
    let merged = normalize_semantic_tags(tags.iter().map(String::as_str));
    upsert_frontmatter_tags(content, &merged)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upserts_tags_into_frontmatter() {
        let body = "---\nkind: note\n---\n\n# Hello";
        let out = upsert_frontmatter_tags(body, &["medousa".into(), "vault".into()]);
        assert!(out.contains("tags:"));
        assert!(out.contains("medousa"));
        assert!(out.contains("kind: note"));
    }

    #[test]
    fn match_all_tags_requires_every_tag() {
        assert!(entry_has_all_tags(
            &["medousa".into(), "vault".into(), "session".into()],
            &["medousa".into(), "vault".into()],
        ));
        assert!(!entry_has_all_tags(
            &["medousa".into()],
            &["medousa".into(), "vault".into()],
        ));
    }
}
