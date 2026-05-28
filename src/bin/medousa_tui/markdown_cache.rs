use std::hash::{Hash, Hasher};

use ratatui::text::Line;
use ratatui_markdown::{DefaultTheme, markdown::MarkdownRenderer};

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
    let renderer = MarkdownRenderer::new(max_width);
    let blocks = renderer.parse(content);
    renderer.render(&blocks, &DefaultTheme)
}
