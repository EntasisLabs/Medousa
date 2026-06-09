//! Grapheme script index records.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GraphemeScriptEntry {
    pub id: String,
    pub name: String,
    pub modules: Vec<String>,
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<String>,
    pub version: u32,
    pub body_path: String,
    pub body_hash: String,
    pub created_at_utc: DateTime<Utc>,
    pub updated_at_utc: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_session_id: Option<String>,
}

impl GraphemeScriptEntry {
    pub fn summary_line(&self) -> String {
        let modules = if self.modules.is_empty() {
            "-".to_string()
        } else {
            self.modules.join(",")
        };
        let intent = self.intent.as_deref().unwrap_or("-");
        format!(
            "- id={} name={} v={} modules=[{}] intent={}",
            self.id, self.name, self.version, modules, intent
        )
    }
}

pub fn slugify_script_id(raw: &str) -> String {
    raw.to_ascii_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_normalizes_names() {
        assert_eq!(slugify_script_id("Web Research v2"), "web-research-v2");
        assert_eq!(slugify_script_id("  http.poll  "), "http-poll");
    }
}
