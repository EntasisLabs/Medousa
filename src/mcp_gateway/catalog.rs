use chrono::Utc;

use crate::capability_catalog::{McpCatalogSyncEntry, McpCatalogSyncResponse};
use crate::mcp_gateway_api::{McpEffectClass, McpToolCatalogEntry};

pub fn mock_tool_catalog() -> Vec<McpToolCatalogEntry> {
    vec![
        entry(
            "notion",
            "Notion MCP",
            "search_pages",
            "Search pages",
            Some("Full-text search across workspace pages"),
            McpEffectClass::ExternalRead,
            vec!["document_search".to_string()],
        ),
        entry(
            "notion",
            "Notion MCP",
            "create_page",
            "Create page",
            Some("Create a new page in a workspace"),
            McpEffectClass::ExternalWrite,
            vec!["document_write".to_string()],
        ),
        entry(
            "confluence",
            "Confluence MCP",
            "search",
            "Search Confluence",
            Some("Search pages and spaces"),
            McpEffectClass::ExternalRead,
            vec!["document_search".to_string()],
        ),
        entry(
            "google_drive",
            "Google Drive MCP",
            "search_docs",
            "Search documents",
            Some("Search Google Drive files and docs"),
            McpEffectClass::ExternalRead,
            vec!["document_search".to_string()],
        ),
        entry(
            "gmail",
            "Gmail MCP",
            "send_message",
            "Send email",
            Some("Send an email message"),
            McpEffectClass::ExternalSideEffect,
            vec!["send_email".to_string()],
        ),
        entry(
            "github",
            "GitHub MCP",
            "search_issues",
            "Search issues",
            Some("Search repository issues"),
            McpEffectClass::ExternalRead,
            vec!["issue_search".to_string()],
        ),
    ]
}

pub fn mock_catalog_sync_response() -> McpCatalogSyncResponse {
    McpCatalogSyncResponse {
        entries: mock_tool_catalog()
            .into_iter()
            .map(|tool| McpCatalogSyncEntry {
                server_id: tool.server_id.clone(),
                tool_name: tool.tool_name.clone(),
                title: tool.title.clone(),
                capability_ids: tool.capability_ids.clone(),
                available: true,
                unavailable_reason: None,
            })
            .collect(),
        now_utc: Utc::now(),
    }
}

pub fn discover_from_catalog(
    query: &str,
    server_id: Option<&str>,
    limit: usize,
) -> Vec<McpToolCatalogEntry> {
    discover_from_entries(&mock_tool_catalog(), query, server_id, limit)
}

pub fn discover_from_entries(
    catalog: &[McpToolCatalogEntry],
    query: &str,
    server_id: Option<&str>,
    limit: usize,
) -> Vec<McpToolCatalogEntry> {
    let normalized = query.trim().to_ascii_lowercase();
    let tokens = normalized
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();

    let mut scored = catalog
        .iter()
        .cloned()
        .filter(|tool| {
            server_id.is_none_or(|expected| tool.server_id.eq_ignore_ascii_case(expected))
        })
        .filter_map(|tool| {
            let score = score_tool_match(&tool, &normalized, &tokens);
            if score == 0 {
                None
            } else {
                Some((score, tool))
            }
        })
        .collect::<Vec<_>>();

    scored.sort_by(|left, right| right.0.cmp(&left.0));
    scored
        .into_iter()
        .take(limit)
        .map(|(_, tool)| tool)
        .collect()
}

pub fn auto_tag_capabilities(tool_name: &str, description: Option<&str>) -> Vec<String> {
    let corpus = format!(
        "{} {}",
        tool_name.to_ascii_lowercase(),
        description.unwrap_or("").to_ascii_lowercase()
    );
    let mut tags = Vec::new();
    if corpus.contains("search") && (corpus.contains("page") || corpus.contains("doc")) {
        tags.push("document_search".to_string());
    }
    if corpus.contains("send") && (corpus.contains("mail") || corpus.contains("email")) {
        tags.push("send_email".to_string());
    }
    if corpus.contains("search") && corpus.contains("issue") {
        tags.push("issue_search".to_string());
    }
    if corpus.contains("research") || (corpus.contains("search") && corpus.contains("web")) {
        tags.push("web_research".to_string());
    }
    tags
}

fn score_tool_match(tool: &McpToolCatalogEntry, query: &str, tokens: &[&str]) -> u8 {
    if query.is_empty() {
        return 1;
    }

    let haystacks = [
        tool.server_id.as_str(),
        tool.server_title.as_str(),
        tool.tool_name.as_str(),
        tool.title.as_str(),
        tool.description.as_deref().unwrap_or(""),
    ]
    .into_iter()
    .chain(tool.capability_ids.iter().map(String::as_str))
    .map(str::to_ascii_lowercase)
    .collect::<Vec<_>>();

    for haystack in &haystacks {
        if haystack.contains(query) {
            return 100;
        }
    }

    if tokens.is_empty() {
        return 0;
    }

    let overlap = tokens
        .iter()
        .filter(|token| haystacks.iter().any(|haystack| haystack.contains(**token)))
        .count();

    if overlap == 0 {
        return 0;
    }

    ((overlap as f32 / tokens.len() as f32) * 80.0).round().max(1.0) as u8
}

fn entry(
    server_id: &str,
    server_title: &str,
    tool_name: &str,
    title: &str,
    description: Option<&str>,
    effect_class: McpEffectClass,
    capability_ids: Vec<String>,
) -> McpToolCatalogEntry {
    McpToolCatalogEntry {
        server_id: server_id.to_string(),
        server_title: server_title.to_string(),
        tool_name: tool_name.to_string(),
        title: title.to_string(),
        description: description.map(str::to_string),
        input_schema_summary: None,
        effect_class,
        capability_ids,
        stability: "stable".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discover_finds_notion_document_search() {
        let matches = discover_from_catalog("notion pages", None, 10);
        assert!(!matches.is_empty());
        assert!(matches
            .iter()
            .any(|entry| entry.server_id == "notion" && entry.tool_name == "search_pages"));
    }

    #[test]
    fn auto_tags_document_search() {
        let tags = auto_tag_capabilities("search_pages", Some("Search workspace pages"));
        assert!(tags.contains(&"document_search".to_string()));
    }
}
