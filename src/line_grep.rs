//! Line-oriented literal grep shared by vault and artifact edit tools.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LineGrepMatch {
    pub line: usize,
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context_before: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context_after: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LineGrepResult {
    pub pattern: String,
    pub total_lines: usize,
    pub match_count: usize,
    pub matches: Vec<LineGrepMatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LineExcerpt {
    pub content: String,
    pub truncated: bool,
    pub total_lines: usize,
    pub total_chars: usize,
    pub line_start: usize,
    pub line_end: usize,
}

pub fn grep_lines(
    body: &str,
    pattern: &str,
    context_lines: usize,
    limit: usize,
) -> Result<LineGrepResult, String> {
    let pattern = pattern.trim();
    if pattern.is_empty() {
        return Err("pattern is required".to_string());
    }
    let context_lines = context_lines.min(10);
    let limit = limit.clamp(1, 200);
    let pattern_lower = pattern.to_ascii_lowercase();
    let lines: Vec<&str> = body.lines().collect();
    let total_lines = lines.len();
    let mut matches = Vec::new();

    for (index, line) in lines.iter().enumerate() {
        if !line.to_ascii_lowercase().contains(&pattern_lower) {
            continue;
        }
        let line_no = index + 1;
        let context_before = if context_lines == 0 || index == 0 {
            Vec::new()
        } else {
            let start = index.saturating_sub(context_lines);
            lines[start..index]
                .iter()
                .map(|value| (*value).to_string())
                .collect()
        };
        let context_after = if context_lines == 0 {
            Vec::new()
        } else {
            lines
                .iter()
                .skip(index + 1)
                .take(context_lines)
                .map(|value| (*value).to_string())
                .collect()
        };
        matches.push(LineGrepMatch {
            line: line_no,
            text: (*line).to_string(),
            context_before,
            context_after,
        });
        if matches.len() >= limit {
            break;
        }
    }

    Ok(LineGrepResult {
        pattern: pattern.to_string(),
        total_lines,
        match_count: matches.len(),
        matches,
    })
}

pub fn excerpt_lines(
    body: &str,
    line_start: Option<usize>,
    line_end: Option<usize>,
    max_chars: usize,
) -> LineExcerpt {
    let max_chars = max_chars.clamp(256, 20_000);
    let lines: Vec<&str> = body.lines().collect();
    let total_lines = lines.len();
    let total_chars = body.chars().count();

    if total_lines == 0 {
        return LineExcerpt {
            content: String::new(),
            truncated: false,
            total_lines: 0,
            total_chars,
            line_start: 0,
            line_end: 0,
        };
    }

    let start = line_start.unwrap_or(1).max(1).min(total_lines);
    let end = line_end.unwrap_or(total_lines).max(start).min(total_lines);
    let slice = lines[(start - 1)..end].join("\n");
    let truncated_by_chars = slice.chars().count() > max_chars;
    let content = if truncated_by_chars {
        format!("{}…", slice.chars().take(max_chars).collect::<String>())
    } else {
        slice
    };

    LineExcerpt {
        content,
        truncated: truncated_by_chars || end < total_lines || start > 1,
        total_lines,
        total_chars,
        line_start: start,
        line_end: end,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grep_finds_case_insensitive_matches_with_context() {
        let body = "alpha\nBeta line\nGamma\nbeta again";
        let result = grep_lines(body, "beta", 1, 10).expect("grep");
        assert_eq!(result.match_count, 2);
        assert_eq!(result.matches[0].line, 2);
        assert_eq!(result.matches[0].context_before, vec!["alpha"]);
        assert_eq!(result.matches[0].context_after, vec!["Gamma"]);
    }

    #[test]
    fn excerpt_respects_line_range_and_char_cap() {
        let body = (1..=20).map(|n| format!("line {n}")).collect::<Vec<_>>().join("\n");
        let excerpt = excerpt_lines(&body, Some(3), Some(5), 50);
        assert_eq!(excerpt.line_start, 3);
        assert_eq!(excerpt.line_end, 5);
        assert!(excerpt.content.contains("line 3"));
    }
}
