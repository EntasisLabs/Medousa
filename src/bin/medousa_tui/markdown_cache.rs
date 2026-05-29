use std::hash::{Hash, Hasher};

use ratatui::text::Line;

use super::{MarkdownCacheKey, TuiState};

pub(crate) fn invalidate_markdown_cache(state: &TuiState) {
    state.markdown_cache.borrow_mut().clear();
    state.markdown_cache_order.borrow_mut().clear();
}

fn content_hash(content: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

pub(crate) fn render_markdown_lines_cached(
    state: &TuiState,
    content: &str,
    width: u16,
) -> Vec<Line<'static>> {
    let key = MarkdownCacheKey {
        width,
        content_hash: content_hash(content),
    };

    if let Some(lines) = state.markdown_cache.borrow().get(&key) {
        return lines.clone();
    }

    let rendered = render_markdown_lines(content, width);
    {
        let mut cache = state.markdown_cache.borrow_mut();
        let mut order = state.markdown_cache_order.borrow_mut();
        if !cache.contains_key(&key) {
            order.push_back(key);
        }
        cache.insert(key, rendered.clone());
        while order.len() > 512 {
            if let Some(old) = order.pop_front() {
                cache.remove(&old);
            }
        }
    }

    rendered
}

pub(crate) fn render_markdown_lines(content: &str, width: u16) -> Vec<Line<'static>> {
    let max_width = width.max(20) as usize;
    let mut lines = Vec::new();
    let mut in_code_block = false;

    for raw_line in content.replace("\r\n", "\n").lines() {
        let trimmed = raw_line.trim_end();

        if trimmed.trim_start().starts_with("```") {
            in_code_block = !in_code_block;
            if !lines.is_empty() {
                lines.push(Line::raw(""));
            }
            continue;
        }

        if in_code_block {
            wrap_text_line(trimmed, max_width, &mut lines);
            continue;
        }

        let normalized = normalize_markdown_line(trimmed);
        if normalized.is_empty() {
            lines.push(Line::raw(""));
        } else {
            wrap_text_line(&normalized, max_width, &mut lines);
        }
    }

    if lines.is_empty() {
        lines.push(Line::raw(""));
    }

    lines
}

fn normalize_markdown_line(line: &str) -> String {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let without_heading = trimmed.trim_start_matches('#').trim();
    let without_quote = without_heading.trim_start_matches('>').trim();

    if let Some(rest) = without_quote
        .strip_prefix("- ")
        .or_else(|| without_quote.strip_prefix("* "))
    {
        return format!("- {}", rest.trim());
    }

    if let Some((idx, rest)) = split_ordered_prefix(without_quote) {
        return format!("{}. {}", idx, rest.trim());
    }

    without_quote.to_string()
}

fn split_ordered_prefix(line: &str) -> Option<(&str, &str)> {
    let dot_idx = line.find('.')?;
    if dot_idx == 0 {
        return None;
    }

    let (prefix, rest_with_dot) = line.split_at(dot_idx);
    if !prefix.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }

    let rest = rest_with_dot.strip_prefix('.')?.trim_start();
    Some((prefix, rest))
}

fn wrap_text_line(input: &str, max_width: usize, out: &mut Vec<Line<'static>>) {
    if input.is_empty() {
        out.push(Line::raw(""));
        return;
    }

    let mut current = String::new();
    for word in input.split_whitespace() {
        let candidate = if current.is_empty() {
            word.to_string()
        } else {
            format!("{} {}", current, word)
        };

        if candidate.chars().count() <= max_width {
            current = candidate;
            continue;
        }

        if !current.is_empty() {
            out.push(Line::from(current.clone()));
            current.clear();
        }

        if word.chars().count() <= max_width {
            current = word.to_string();
        } else {
            wrap_long_word(word, max_width, out);
        }
    }

    if !current.is_empty() {
        out.push(Line::from(current));
    }
}

fn wrap_long_word(word: &str, max_width: usize, out: &mut Vec<Line<'static>>) {
    let mut chunk = String::new();
    for ch in word.chars() {
        if chunk.chars().count() >= max_width {
            out.push(Line::from(chunk.clone()));
            chunk.clear();
        }
        chunk.push(ch);
    }

    if !chunk.is_empty() {
        out.push(Line::from(chunk));
    }
}
