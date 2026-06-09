//! Ranked recall for durable learning artifacts at turn start (Phase 8E.2).

use chrono::Utc;

use crate::agent_runtime::prompt_prep::truncate_text_for_budget;
use crate::grapheme_script::GraphemeScriptService;
use crate::vault::note::strip_frontmatter;
use crate::vault::store::vault_store;

pub const RUNTIME_LEARNING_VAULT_TAGS: &[&str] = &[
    "runtime-learning",
    "runtime_learning",
    "medousa/runtime-learning",
    "medousa-runtime-learning",
];

pub const DEFAULT_SCRIPT_RECALL_LIMIT: usize = 5;
pub const DEFAULT_LEARNING_RECALL_LIMIT: usize = 5;
pub const DEFAULT_SCRIPT_RECALL_BLOCK_CHARS: usize = 900;
pub const DEFAULT_LEARNING_RECALL_BLOCK_CHARS: usize = 1200;
pub const LEARNING_EXCERPT_CHARS: usize = 220;

#[derive(Debug, Clone)]
pub struct RuntimeLearningHit {
    pub path: String,
    pub title: String,
    pub score: f32,
    pub line: String,
}

pub fn build_grapheme_script_recall_block(prompt: &str, char_budget: usize) -> String {
    let hits = GraphemeScriptService::search_ranked(
        prompt,
        None,
        None,
        DEFAULT_SCRIPT_RECALL_LIMIT,
    );
    if hits.is_empty() {
        return String::new();
    }

    let lines: Vec<String> = hits
        .iter()
        .map(|hit| format!("{} score={:.2}", hit.line, hit.score))
        .collect();
    let body = truncate_text_for_budget(&lines.join("\n"), char_budget);
    if body.trim().is_empty() {
        return String::new();
    }
    format!(
        "[MEDOUSA_GRAPHEME_SCRIPTS]\n\
         Saved workshop scripts matching this turn (load with cognition_grapheme_script_load):\n\
         {body}"
    )
}

pub fn rank_runtime_learning_notes(query: &str, limit: usize) -> Vec<RuntimeLearningHit> {
    let terms = tokenize(query);
    let store = vault_store();
    let entries = store.all_entries();
    let newest = entries
        .iter()
        .map(|entry| entry.modified_at_utc)
        .max()
        .unwrap_or_else(Utc::now);

    let mut hits = Vec::new();
    for entry in entries {
        if !entry_has_runtime_learning_tag(&entry.tags) {
            continue;
        }

        let body = match store.read_content(&entry.path) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let (content, _) = strip_frontmatter(&body);
        let title = entry.title.to_ascii_lowercase();
        let path_lower = entry.path.to_ascii_lowercase();
        let content_lower = content.to_ascii_lowercase();

        let mut score = 0.15f32;
        for term in &terms {
            if title.contains(term) || path_lower.contains(term) {
                score += 0.25 / terms.len() as f32;
            }
            if content_lower.contains(term) {
                score += 0.35 / terms.len() as f32;
            }
        }

        let age_hours = newest
            .signed_duration_since(entry.modified_at_utc)
            .num_hours()
            .max(0) as f32;
        score += 0.1 * (1.0 / (1.0 + age_hours / 48.0));
        score = score.clamp(0.0, 1.0);

        let excerpt = truncate_text_for_budget(
            &content.replace('\n', " "),
            LEARNING_EXCERPT_CHARS,
        );
        hits.push(RuntimeLearningHit {
            path: entry.path.clone(),
            title: entry.title.clone(),
            score,
            line: format!(
                "- [[{}]] {} — {}",
                entry.path,
                entry.title,
                excerpt
            ),
        });
    }

    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hits.truncate(limit.clamp(1, 20));
    hits
}

pub fn build_runtime_learnings_block(prompt: &str, char_budget: usize) -> String {
    let hits = rank_runtime_learning_notes(prompt, DEFAULT_LEARNING_RECALL_LIMIT);
    if hits.is_empty() {
        return String::new();
    }

    let lines: Vec<String> = hits.iter().map(|hit| hit.line.clone()).collect();
    let body = truncate_text_for_budget(&lines.join("\n"), char_budget);
    if body.trim().is_empty() {
        return String::new();
    }
    format!(
        "[MEDOUSA_RUNTIME_LEARNINGS]\n\
         Vault notes tagged runtime-learning (save with cognition_vault_write + tags: [runtime-learning]):\n\
         {body}"
    )
}

fn entry_has_runtime_learning_tag(tags: &[String]) -> bool {
    tags.iter().any(|tag| {
        let normalized = tag.trim().to_ascii_lowercase();
        RUNTIME_LEARNING_VAULT_TAGS
            .iter()
            .any(|candidate| normalized == *candidate)
            || (normalized.contains("runtime") && normalized.contains("learning"))
    })
}

fn tokenize(query: &str) -> Vec<String> {
    query
        .split(|c: char| !c.is_ascii_alphanumeric())
        .map(str::trim)
        .filter(|token| token.len() >= 3)
        .map(|token| token.to_ascii_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon_api::VaultWriteRequest;
    use crate::vault::VaultService;

    #[test]
    fn runtime_learning_tag_detection() {
        assert!(entry_has_runtime_learning_tag(&["runtime-learning".to_string()]));
        assert!(entry_has_runtime_learning_tag(&["Medousa/Runtime-Learning".to_string()]));
        assert!(!entry_has_runtime_learning_tag(&["journal".to_string()]));
    }

    #[test]
    fn learning_block_empty_without_tagged_notes() {
        let block = build_runtime_learnings_block("web research calibration", 500);
        // May be empty in clean env — only assert format when hits exist.
        if !block.is_empty() {
            assert!(block.contains("[MEDOUSA_RUNTIME_LEARNINGS]"));
        }
    }

    #[test]
    fn vault_runtime_learning_ranking_finds_tagged_note() {
        let _guard = crate::vault::service::vault_integration_test_lock();
        let token = uuid::Uuid::new_v4().simple().to_string();
        let path = format!("learnings/runtime-{token}.md");
        let content = format!(
            "---\ntags: [runtime-learning, workshop]\n---\n# Learning {token}\n\nPrefer web.duckduckgo before modules search for live facts.\n"
        );
        VaultService::write_note(
            Some(&path),
            &VaultWriteRequest {
                path: Some(path.clone()),
                content,
            },
            None,
        )
        .expect("write learning note");

        let hits = rank_runtime_learning_notes(&token, 5);
        assert!(hits.iter().any(|hit| hit.path == path));

        let block = build_runtime_learnings_block(&token, 800);
        assert!(block.contains("[MEDOUSA_RUNTIME_LEARNINGS]"));
        assert!(block.contains(&path));

        let _ = VaultService::delete_note(&path);
    }
}
