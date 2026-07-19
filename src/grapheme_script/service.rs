//! Grapheme script library service layer.

use anyhow::Result;
use chrono::Utc;
use serde::Serialize;

use super::entry::GraphemeScriptEntry;
use super::store::grapheme_script_store;

const TITLE_WEIGHT: f32 = 0.35;
const MODULE_WEIGHT: f32 = 0.25;
const TAG_WEIGHT: f32 = 0.15;
const BODY_WEIGHT: f32 = 0.15;
const RECENCY_WEIGHT: f32 = 0.10;

#[derive(Debug, Clone, Serialize)]
pub struct GraphemeScriptHit {
    pub id: String,
    pub name: String,
    pub modules: Vec<String>,
    pub tags: Vec<String>,
    pub intent: Option<String>,
    pub version: u32,
    pub score: f32,
    pub line: String,
}

pub struct GraphemeScriptService;

impl GraphemeScriptService {
    pub fn save(
        id: Option<&str>,
        name: &str,
        body: &str,
        modules: Vec<String>,
        tags: Vec<String>,
        intent: Option<String>,
        source_session_id: Option<String>,
    ) -> Result<GraphemeScriptEntry> {
        grapheme_script_store().save_script(
            id,
            name,
            body,
            modules,
            tags,
            intent,
            source_session_id,
        )
    }

    pub fn delete(id: &str) -> Result<GraphemeScriptEntry> {
        grapheme_script_store().delete_script(id)
    }

    pub fn rename(id: &str, name: &str) -> Result<GraphemeScriptEntry> {
        grapheme_script_store().rename_script(id, name)
    }

    pub fn load(id: &str) -> Result<(GraphemeScriptEntry, String)> {
        let entry = grapheme_script_store()
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("grapheme script not found: {id}"))?;
        let body = grapheme_script_store().read_body(&entry)?;
        Ok((entry, body))
    }

    pub fn list(
        module: Option<&str>,
        tag: Option<&str>,
        limit: usize,
    ) -> Vec<GraphemeScriptEntry> {
        let module = module.map(str::trim).filter(|value| !value.is_empty());
        let tag = tag.map(str::trim).filter(|value| !value.is_empty());
        grapheme_script_store()
            .all_entries()
            .into_iter()
            .filter(|entry| module_filter(entry, module))
            .filter(|entry| tag_filter(entry, tag))
            .take(limit.clamp(1, 200))
            .collect()
    }

    pub fn search_ranked(
        query: &str,
        module: Option<&str>,
        tag: Option<&str>,
        limit: usize,
    ) -> Vec<GraphemeScriptHit> {
        let query = query.trim();
        let module = module.map(str::trim).filter(|value| !value.is_empty());
        let tag = tag.map(str::trim).filter(|value| !value.is_empty());
        let terms = tokenize(query);
        let store = grapheme_script_store();
        let entries = store.all_entries();
        let newest = entries
            .iter()
            .map(|entry| entry.updated_at_utc)
            .max()
            .unwrap_or_else(Utc::now);

        let mut hits = Vec::new();
        for entry in entries {
            if !module_filter(&entry, module) || !tag_filter(&entry, tag) {
                continue;
            }

            let body = store.read_body(&entry).unwrap_or_default();
            let mut score = 0.0f32;
            if terms.is_empty() {
                score = 0.2;
            } else {
                let name = entry.name.to_ascii_lowercase();
                let intent = entry.intent.as_deref().unwrap_or("").to_ascii_lowercase();
                for term in &terms {
                    if name.contains(term) {
                        score += TITLE_WEIGHT / terms.len() as f32;
                    }
                    if intent.contains(term) {
                        score += TAG_WEIGHT / terms.len() as f32;
                    }
                    if entry
                        .modules
                        .iter()
                        .any(|module| module.contains(term))
                    {
                        score += MODULE_WEIGHT / terms.len() as f32;
                    }
                    if entry.tags.iter().any(|tag| tag.contains(term)) {
                        score += TAG_WEIGHT / terms.len() as f32;
                    }
                    if body.to_ascii_lowercase().contains(term) {
                        score += BODY_WEIGHT / terms.len() as f32;
                    }
                }
                if query.len() >= 3 && body.to_ascii_lowercase().contains(&query.to_ascii_lowercase()) {
                    score += 0.15;
                }
            }

            if score <= 0.0 {
                continue;
            }

            let age_hours = newest
                .signed_duration_since(entry.updated_at_utc)
                .num_hours()
                .max(0) as f32;
            score += RECENCY_WEIGHT * (1.0 / (1.0 + age_hours / 24.0));
            score = score.clamp(0.0, 1.0);

            hits.push(GraphemeScriptHit {
                line: entry.summary_line(),
                id: entry.id.clone(),
                name: entry.name.clone(),
                modules: entry.modules.clone(),
                tags: entry.tags.clone(),
                intent: entry.intent.clone(),
                version: entry.version,
                score,
            });
        }

        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        hits.truncate(limit.clamp(1, 50));
        hits
    }
}

fn module_filter(entry: &GraphemeScriptEntry, module: Option<&str>) -> bool {
    match module {
        Some(module) => entry
            .modules
            .iter()
            .any(|value| value.contains(module)),
        None => true,
    }
}

fn tag_filter(entry: &GraphemeScriptEntry, tag: Option<&str>) -> bool {
    match tag {
        Some(tag) => entry.tags.iter().any(|value| value.contains(tag)),
        None => true,
    }
}

fn tokenize(query: &str) -> Vec<String> {
    query
        .split(|c: char| !c.is_ascii_alphanumeric())
        .map(str::trim)
        .filter(|token| token.len() >= 2)
        .map(|token| token.to_ascii_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};

    use super::*;
    use crate::grapheme_script::entry::slugify_script_id;

    fn script_test_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .expect("grapheme script test lock")
    }

    #[test]
    fn save_list_load_search_round_trip() {
        let _guard = script_test_lock();
        let token = uuid::Uuid::new_v4().simple().to_string();
        let name = format!("Probe {token}");
        let id = slugify_script_id(&name);
        let body = format!(
            "import core from \"grapheme/core\"\nquery Probe {{\n  set {{ message: \"{token}\" }}\n  |> core.echo(message: $current.message)\n}}"
        );

        let saved = GraphemeScriptService::save(
            Some(&id),
            &name,
            &body,
            vec!["core".to_string(), "web".to_string()],
            vec!["research".to_string()],
            Some("web lookup".to_string()),
            None,
        )
        .expect("save");

        assert_eq!(saved.id, id);
        assert!(saved.version >= 1);

        let listed = GraphemeScriptService::list(Some("web"), None, 20);
        assert!(listed.iter().any(|entry| entry.id == id));

        let (loaded, loaded_body) = GraphemeScriptService::load(&id).expect("load");
        assert_eq!(loaded.id, id);
        assert_eq!(loaded_body, body);

        let hits = GraphemeScriptService::search_ranked(&token, None, None, 5);
        assert!(hits.iter().any(|hit| hit.id == id));
    }
}
