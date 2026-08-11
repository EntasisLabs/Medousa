//! Lightweight, dependency-free syntax coloring for TUI code panes.
//!
//! Extension-dispatched keyword / comment / string spans — not a full highlighter.

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

pub fn highlight_source(path: &str, source: &str) -> Vec<Line<'static>> {
    let ext = path
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "rs" => highlight_c_like(source, RUST_KEYWORDS, "//"),
        "ts" | "tsx" | "js" | "jsx" => highlight_c_like(source, TS_KEYWORDS, "//"),
        "json" => highlight_json(source),
        "md" | "markdown" => highlight_markdown(source),
        "toml" | "yaml" | "yml" => highlight_hash_comments(source),
        "py" => highlight_c_like(source, PYTHON_KEYWORDS, "#"),
        "sh" | "bash" | "zsh" => highlight_hash_comments(source),
        _ => source
            .lines()
            .map(|line| Line::from(Span::raw(line.to_string())))
            .collect(),
    }
}

const RUST_KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
    "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move",
    "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true",
    "type", "unsafe", "use", "where", "while",
];

const TS_KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "case", "catch", "class", "const", "continue", "debugger",
    "default", "delete", "do", "else", "enum", "export", "extends", "false", "finally", "for",
    "from", "function", "if", "import", "in", "instanceof", "interface", "let", "new", "null",
    "return", "static", "super", "switch", "this", "throw", "true", "try", "typeof", "var",
    "void", "while", "with", "yield", "type", "of",
];

const PYTHON_KEYWORDS: &[&str] = &[
    "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del", "elif",
    "else", "except", "False", "finally", "for", "from", "global", "if", "import", "in", "is",
    "lambda", "None", "nonlocal", "not", "or", "pass", "raise", "return", "True", "try", "while",
    "with", "yield",
];

fn style_comment() -> Style {
    Style::default().fg(Color::DarkGray)
}
fn style_string() -> Style {
    Style::default().fg(Color::Green)
}
fn style_keyword() -> Style {
    Style::default()
        .fg(Color::Magenta)
        .add_modifier(Modifier::BOLD)
}
fn style_plain() -> Style {
    Style::default().fg(Color::White)
}
fn style_heading() -> Style {
    Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}

fn highlight_c_like(source: &str, keywords: &[&str], line_comment: &str) -> Vec<Line<'static>> {
    source
        .lines()
        .map(|line| highlight_c_like_line(line, keywords, line_comment))
        .collect()
}

fn highlight_c_like_line(line: &str, keywords: &[&str], line_comment: &str) -> Line<'static> {
    if let Some(idx) = line.find(line_comment) {
        let (code, comment) = line.split_at(idx);
        let mut spans = tokenize_code(code, keywords);
        spans.push(Span::styled(comment.to_string(), style_comment()));
        return Line::from(spans);
    }
    Line::from(tokenize_code(line, keywords))
}

fn tokenize_code(code: &str, keywords: &[&str]) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut rest = code;
    while !rest.is_empty() {
        if rest.starts_with('"') || rest.starts_with('\'') {
            let quote = rest.chars().next().unwrap();
            let mut end = 1;
            let bytes = rest.as_bytes();
            while end < bytes.len() {
                if bytes[end] == b'\\' && end + 1 < bytes.len() {
                    end += 2;
                    continue;
                }
                if bytes[end] == quote as u8 {
                    end += 1;
                    break;
                }
                end += 1;
            }
            let (s, next) = rest.split_at(end.min(rest.len()));
            spans.push(Span::styled(s.to_string(), style_string()));
            rest = next;
            continue;
        }
        let ch = rest.chars().next().unwrap();
        if ch.is_ascii_alphabetic() || ch == '_' {
            let len = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .map(|c| c.len_utf8())
                .sum::<usize>();
            let (word, next) = rest.split_at(len);
            let style = if keywords.contains(&word) {
                style_keyword()
            } else {
                style_plain()
            };
            spans.push(Span::styled(word.to_string(), style));
            rest = next;
        } else {
            let len = ch.len_utf8();
            let (s, next) = rest.split_at(len);
            spans.push(Span::styled(s.to_string(), style_plain()));
            rest = next;
        }
    }
    if spans.is_empty() {
        spans.push(Span::styled(String::new(), style_plain()));
    }
    spans
}

fn highlight_json(source: &str) -> Vec<Line<'static>> {
    source
        .lines()
        .map(|line| {
            // Cheap: color quoted segments green; rest plain.
            let mut spans = Vec::new();
            let mut rest = line;
            while !rest.is_empty() {
                if let Some(start) = rest.find('"') {
                    if start > 0 {
                        spans.push(Span::styled(
                            rest[..start].to_string(),
                            style_plain(),
                        ));
                    }
                    let after = &rest[start + 1..];
                    if let Some(end) = after.find('"') {
                        let end_abs = start + 1 + end + 1;
                        spans.push(Span::styled(
                            rest[start..end_abs].to_string(),
                            style_string(),
                        ));
                        rest = &rest[end_abs..];
                    } else {
                        spans.push(Span::styled(rest[start..].to_string(), style_string()));
                        break;
                    }
                } else {
                    spans.push(Span::styled(rest.to_string(), style_plain()));
                    break;
                }
            }
            if spans.is_empty() {
                spans.push(Span::raw(String::new()));
            }
            Line::from(spans)
        })
        .collect()
}

fn highlight_markdown(source: &str) -> Vec<Line<'static>> {
    source
        .lines()
        .map(|line| {
            if line.starts_with('#') {
                Line::from(Span::styled(line.to_string(), style_heading()))
            } else if line.trim_start().starts_with("```") {
                Line::from(Span::styled(line.to_string(), style_comment()))
            } else if let Some(idx) = line.find('`') {
                let mut spans = Vec::new();
                let mut rest = line;
                let mut in_code = false;
                while !rest.is_empty() {
                    if let Some(tick) = rest.find('`') {
                        if tick > 0 {
                            let style = if in_code {
                                style_string()
                            } else {
                                style_plain()
                            };
                            spans.push(Span::styled(rest[..tick].to_string(), style));
                        }
                        spans.push(Span::styled("`".to_string(), style_string()));
                        rest = &rest[tick + 1..];
                        in_code = !in_code;
                    } else {
                        let style = if in_code {
                            style_string()
                        } else {
                            style_plain()
                        };
                        spans.push(Span::styled(rest.to_string(), style));
                        break;
                    }
                }
                if idx == 0 && spans.is_empty() {
                    Line::from(Span::styled(line.to_string(), style_plain()))
                } else {
                    Line::from(spans)
                }
            } else {
                Line::from(Span::styled(line.to_string(), style_plain()))
            }
        })
        .collect()
}

fn highlight_hash_comments(source: &str) -> Vec<Line<'static>> {
    source
        .lines()
        .map(|line| {
            if let Some(idx) = line.find('#') {
                let (code, comment) = line.split_at(idx);
                Line::from(vec![
                    Span::styled(code.to_string(), style_plain()),
                    Span::styled(comment.to_string(), style_comment()),
                ])
            } else {
                Line::from(Span::styled(line.to_string(), style_plain()))
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_keywords_and_comments() {
        let lines = highlight_source("foo.rs", "fn main() {} // hi\nlet x = \"a\";");
        assert_eq!(lines.len(), 2);
        assert!(lines[0].spans.iter().any(|s| s.content == "fn"));
    }

    #[test]
    fn markdown_heading() {
        let lines = highlight_source("note.md", "# Title\nbody");
        assert_eq!(lines.len(), 2);
    }
}
