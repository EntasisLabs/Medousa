//! Helpers for Locus 0.4+ `semantic_tags` in STTP prime blocks and recall filters.

use serde_json::Value;
use stasis::ports::outbound::memory::memory_models::MemoryFilter;

/// Lowercase, trim, dedupe tag strings (matches Locus index normalization).
pub fn normalize_semantic_tags<I, S>(tags: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    tags.into_iter()
        .map(|tag| tag.as_ref().trim().to_ascii_lowercase())
        .filter(|tag| !tag.is_empty())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// Parse `semantic_tags` from tool/API JSON (`array<string>` or comma-separated string).
pub fn parse_semantic_tags_from_value(value: Option<&Value>) -> Option<Vec<String>> {
    let value = value?;
    let tags = if let Some(items) = value.as_array() {
        normalize_semantic_tags(items.iter().filter_map(|v| v.as_str()))
    } else if let Some(raw) = value.as_str() {
        normalize_semantic_tags(raw.split(',').map(str::trim))
    } else {
        Vec::new()
    };
    if tags.is_empty() {
        None
    } else {
        Some(tags)
    }
}

/// Parse optional `tag_prefix` for vocabulary / prefix-scoped retrieval.
pub fn parse_tag_prefix_from_value(value: Option<&Value>) -> Option<String> {
    let raw = value?.as_str()?.trim();
    if raw.is_empty() {
        None
    } else {
        Some(raw.to_ascii_lowercase())
    }
}

/// Build a Stasis memory filter from recall/list tool input.
pub fn memory_filter_from_tag_input(input: &Value) -> MemoryFilter {
    let mut filter = MemoryFilter::default();
    if let Some(tags) = parse_semantic_tags_from_value(input.get("semantic_tags")) {
        filter.indexed_tags = Some(tags);
    }
    if let Some(prefix) = parse_tag_prefix_from_value(input.get("tag_prefix")) {
        filter.tag_prefix = Some(prefix);
    }
    filter
}

pub fn input_has_tag_filters(input: &Value) -> bool {
    parse_semantic_tags_from_value(input.get("semantic_tags")).is_some()
        || parse_tag_prefix_from_value(input.get("tag_prefix")).is_some()
}

/// Resolve Locus tenant id for tag vocabulary queries (profile slug or default).
pub fn resolve_workshop_tag_tenant_id(session_id: Option<&str>) -> String {
    if let Some(session) = session_id.filter(|s| !s.trim().is_empty()) {
        return crate::locus_memory::derive_locus_tenant_id(session);
    }
    let profile_id = crate::user_profiles::resolve_workshop_identity_user_id();
    crate::user_profiles::profile_slug_from_id(&profile_id)
        .filter(|slug| *slug != crate::locus_memory::LOCUS_DEFAULT_TENANT)
        .map(|slug| slug.to_string())
        .unwrap_or_else(|| crate::locus_memory::LOCUS_DEFAULT_TENANT.to_string())
}

/// Inject or merge `semantic_tags: [...]` into the prime layer of an STTP node.
/// Tags are lowercased and deduplicated. No-op if `tags` is empty.
pub fn inject_semantic_tags(raw_node: &str, tags: &[String]) -> String {
    let tags = normalize_semantic_tags(tags.iter().map(String::as_str));
    if tags.is_empty() {
        return raw_node.to_string();
    }

    if raw_node.contains("semantic_tags") {
        return raw_node.to_string();
    }

    let tags_json: String = tags
        .iter()
        .map(|tag| format!("\"{}\"", tag.replace('"', "\\\"")))
        .collect::<Vec<_>>()
        .join(", ");

    if let Some(prime_idx) = raw_node.find("prime:") {
        if let Some(open) = raw_node[prime_idx..].find('{') {
            let insert_at = prime_idx + open + 1;
            let injection = format!(" semantic_tags: [{tags_json}],");
            let mut out = String::with_capacity(raw_node.len() + injection.len());
            out.push_str(&raw_node[..insert_at]);
            out.push_str(&injection);
            out.push_str(&raw_node[insert_at..]);
            return out;
        }
    }

    raw_node.to_string()
}

/// Default workshop tags for memory store when the model omits explicit tags.
pub fn default_workshop_semantic_tags(chat_session_id: &str) -> Vec<String> {
    let mut tags = vec!["medousa".to_string(), "session".to_string()];
    let profile_id = crate::user_profiles::resolve_workshop_identity_user_id();
    if let Some(slug) = crate::user_profiles::profile_slug_from_id(&profile_id) {
        if slug != crate::locus_memory::LOCUS_DEFAULT_TENANT {
            tags.push(format!("profile:{slug}"));
        }
    }
    let short = chat_session_id.chars().take(8).collect::<String>();
    if !short.is_empty() {
        tags.push(format!("chat:{short}"));
    }
    tags
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn injects_semantic_tags_into_prime() {
        let raw = r#"⊕⟨ ⏣0{ trigger: manual, prime: { context_summary: "test", relevant_tier: raw } } ⟩"#;
        let out = inject_semantic_tags(raw, &["Medousa".into(), "session".into()]);
        assert!(out.contains("semantic_tags:"));
        assert!(out.contains("medousa"));
    }

    #[test]
    fn normalizes_and_dedupes_tags() {
        let tags = normalize_semantic_tags([" Session ", "session", "Notes"]);
        assert_eq!(tags, vec!["notes", "session"]);
    }

    #[test]
    fn parses_tag_array_and_csv_string() {
        assert_eq!(
            parse_semantic_tags_from_value(Some(&json!(["a", "B"]))),
            Some(vec!["a".to_string(), "b".to_string()])
        );
        assert_eq!(
            parse_semantic_tags_from_value(Some(&json!("profile:work, session"))),
            Some(vec!["profile:work".to_string(), "session".to_string()])
        );
    }
}
