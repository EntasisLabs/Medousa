//! Vault service orchestration.

use anyhow::Result;

use crate::daemon_api::{
    VaultBacklinksResponse, VaultDeleteResponse, VaultFileContentResponse, VaultNoteContentResponse,
    VaultNotesListResponse, VaultTagsListResponse, VaultWriteRequest, VaultWriteResponse,
    WorkspaceEventActor,
};
use crate::vault::path::resolve_user_note_path;
use crate::vault::search::search_vault;
use crate::vault::semantic_tags::{apply_semantic_tags_on_write, collect_distinct_tags, entry_has_all_tags, parse_tags_query};
use crate::vault::store::vault_store;
use crate::workspace::store::workspace_store;
use base64::Engine;
use std::fs;

pub struct VaultService;

impl VaultService {
    pub fn list_notes(
        prefix: Option<&str>,
        limit: usize,
        tags: Option<&str>,
        tag_prefix: Option<&str>,
    ) -> VaultNotesListResponse {
        let limit = limit.clamp(1, 500);
        let required = parse_tags_query(tags);
        let prefix_filter = tag_prefix
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_ascii_lowercase());
        let entries = vault_store().list_entries(prefix, limit.saturating_mul(4));
        let notes = entries
            .into_iter()
            .filter(|entry| entry_has_all_tags(&entry.tags, &required))
            .filter(|entry| {
                prefix_filter.as_ref().is_none_or(|prefix| {
                    entry.tags.iter().any(|tag| tag.to_ascii_lowercase().starts_with(prefix))
                })
            })
            .take(limit)
            .map(|entry| entry.to_vault_note(vault_store().backlinks_for(&entry.path)))
            .collect();
        VaultNotesListResponse { notes }
    }

    pub fn list_tags(prefix: Option<&str>, limit: usize) -> VaultTagsListResponse {
        let limit = limit.clamp(1, 500);
        let tags = collect_distinct_tags(&vault_store().all_entries(), prefix, limit);
        VaultTagsListResponse {
            count: tags.len(),
            tags,
        }
    }

    pub fn get_note(path: &str) -> Result<VaultNoteContentResponse> {
        let entry = vault_store()
            .get_entry(path)
            .ok_or_else(|| anyhow::anyhow!("vault note not found: {path}"))?;
        let content = vault_store().read_content(path)?;
        let backlinks = vault_store().backlinks_for(path);
        Ok(VaultNoteContentResponse {
            note: entry.to_vault_note(backlinks),
            content,
        })
    }

    /// Read a vault-relative file (images, attachments) for remote Home preview.
    pub fn read_file(path: &str) -> Result<VaultFileContentResponse> {
        const MAX_PREVIEW_BYTES: u64 = 8 * 1024 * 1024;
        let absolute = resolve_user_note_path(path)?;
        if !absolute.is_file() {
            anyhow::bail!("vault file not found: {path}");
        }
        let meta = fs::metadata(&absolute)?;
        let size = meta.len();
        if size > MAX_PREVIEW_BYTES {
            anyhow::bail!("vault file too large for preview (max 8MB)");
        }
        let bytes = fs::read(&absolute)?;
        let content_type = mime_guess_from_path(&absolute);
        Ok(VaultFileContentResponse {
            path: path.trim().trim_start_matches('/').to_string(),
            content_type,
            base64: base64::engine::general_purpose::STANDARD.encode(bytes),
            size,
        })
    }

    pub fn write_note(
        path: Option<&str>,
        request: &VaultWriteRequest,
        if_match: Option<&str>,
    ) -> Result<VaultWriteResponse> {
        Self::write_note_with_actor(path, request, if_match, WorkspaceEventActor::Operator, None)
    }

    pub fn write_note_with_actor(
        path: Option<&str>,
        request: &VaultWriteRequest,
        if_match: Option<&str>,
        actor: WorkspaceEventActor,
        tool_name: Option<&str>,
    ) -> Result<VaultWriteResponse> {
        let target_path = request
            .path
            .as_deref()
            .or(path)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("path is required"))?;
        let existed = vault_store().get_entry(target_path).is_some();
        let auto_workshop_tags = if crate::vault::roots::active_root_skips_auto_workshop_tags()
        {
            false
        } else {
            request.auto_workshop_tags
        };
        let content = apply_semantic_tags_on_write(
            &request.content,
            request.session_id.as_deref(),
            request.semantic_tags.as_deref(),
            auto_workshop_tags,
        );
        let entry = vault_store().write_content(target_path, &content, if_match)?;
        append_vault_feed_event(&entry.path, &entry.title, !existed, actor, tool_name);
        Ok(VaultWriteResponse {
            note: entry.to_vault_note(vault_store().backlinks_for(&entry.path)),
            created: !existed,
            content: Some(content),
        })
    }

    pub fn delete_note(path: &str) -> Result<VaultDeleteResponse> {
        vault_store().delete_note(path)?;
        Ok(VaultDeleteResponse {
            path: path.to_string(),
            deleted: true,
        })
    }

    pub fn relocate_note(from_path: &str, to_path: &str) -> Result<VaultWriteResponse> {
        let from = from_path.trim();
        let to = to_path.trim();
        if from.is_empty() || to.is_empty() {
            anyhow::bail!("from_path and to_path are required");
        }
        if from == to {
            return Self::get_note(from).map(|read| VaultWriteResponse {
                note: read.note,
                created: false,
                content: Some(read.content),
            });
        }
        if vault_store().get_entry(to).is_some() {
            anyhow::bail!("a note already exists at path: {to}");
        }
        let read = Self::get_note(from)?;
        let request = VaultWriteRequest {
            path: Some(to.to_string()),
            content: read.content,
            ..Default::default()
        };
        let written = Self::write_note_with_actor(
            Some(to),
            &request,
            None,
            WorkspaceEventActor::Agent,
            Some("cognition_vault_move"),
        )?;
        vault_store().delete_note(from)?;
        Ok(written)
    }

    pub fn search(
        query: Option<&str>,
        limit: usize,
        tags: Option<&str>,
    ) -> Result<crate::daemon_api::VaultSearchResponse> {
        let required = parse_tags_query(tags);
        if query.map(str::trim).filter(|value| !value.is_empty()).is_none() {
            if required.is_empty() {
                return Ok(crate::daemon_api::VaultSearchResponse {
                    query: String::new(),
                    hits: Vec::new(),
                });
            }
            let listed = Self::list_notes(None, limit, tags, None);
            let hits = listed
                .notes
                .into_iter()
                .map(|note| crate::daemon_api::VaultSearchHit {
                    note: crate::daemon_api::VaultNoteSummary {
                        path: note.path.clone(),
                        title: note.title.clone(),
                        modified_at_utc: note.modified_at_utc,
                        kind: note.kind,
                    },
                    score: 1.0,
                    matched_terms: required.clone(),
                    snippet: None,
                })
                .collect();
            return Ok(crate::daemon_api::VaultSearchResponse {
                query: required.join(", "),
                hits,
            });
        }
        let mut response = search_vault(query.unwrap_or_default().trim(), limit.clamp(1, 100))?;
        if !required.is_empty() {
            response.hits.retain(|hit| {
                vault_store()
                    .get_entry(&hit.note.path)
                    .is_some_and(|entry| entry_has_all_tags(&entry.tags, &required))
            });
        }
        Ok(response)
    }

    pub fn backlinks(path: &str) -> Result<VaultBacklinksResponse> {
        let _ = vault_store()
            .get_entry(path)
            .ok_or_else(|| anyhow::anyhow!("vault note not found: {path}"))?;
        Ok(VaultBacklinksResponse {
            path: path.to_string(),
            backlinks: vault_store().backlinks_for(path),
        })
    }
}

#[cfg(test)]
pub(crate) fn vault_integration_test_lock() -> std::sync::MutexGuard<'static, ()> {
    use std::sync::{Mutex, OnceLock};
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .expect("vault test lock")
}

fn mime_guess_from_path(path: &std::path::Path) -> String {
    let ext = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "pdf" => "application/pdf",
        "csv" => "text/csv",
        "tsv" => "text/tab-separated-values",
        "json" => "application/json",
        "md" | "markdown" => "text/markdown",
        "txt" => "text/plain",
        "ics" => "text/calendar",
        _ => "application/octet-stream",
    }
    .to_string()
}

fn append_vault_feed_event(
    path: &str,
    title: &str,
    created: bool,
    actor: WorkspaceEventActor,
    tool_name: Option<&str>,
) {
    let refs = vec![crate::daemon_api::WorkspaceEventRef {
        ref_type: "vault_path".to_string(),
        ref_id: path.to_string(),
    }];
    let detail_line = title.trim().to_string();
    let kind = if created {
        crate::daemon_api::WorkspaceEventKind::VaultNoteCreated
    } else {
        crate::daemon_api::WorkspaceEventKind::VaultNoteUpdated
    };
    let summary = match actor {
        WorkspaceEventActor::Agent => format!("Agent updated vault — {detail_line}"),
        _ => format!("Vault updated — {detail_line}"),
    };
    let tool_names = tool_name.map(|name| vec![name.to_string()]).unwrap_or_default();
    let event = crate::daemon_api::WorkspaceEvent {
        id: crate::workspace::event::new_event_id(),
        timestamp_utc: chrono::Utc::now(),
        kind,
        actor,
        summary,
        refs,
        detail_line: Some(detail_line),
        context_line: Some(path.to_string()),
        intent: None,
        tool_names,
    };
    workspace_store().append_event(event);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault_test_lock() -> std::sync::MutexGuard<'static, ()> {
        vault_integration_test_lock()
    }

    #[test]
    fn wikilink_resolves_and_backlinks() {
        let _guard = vault_test_lock();
        let suffix = uuid::Uuid::new_v4().simple();
        let weekly = format!("journal/weekly-review-{suffix}.md");
        let daily = format!("journal/daily-{suffix}.md");
        VaultService::write_note(
            Some(&weekly),
            &VaultWriteRequest {
                path: Some(weekly.clone()),
                content: "# Weekly Review\n".to_string(),
                ..Default::default()
            },
            None,
        )
        .expect("weekly");
        VaultService::write_note(
            Some(&daily),
            &VaultWriteRequest {
                path: Some(daily.clone()),
                content: format!("# Daily\n\nSee [[weekly-review-{suffix}]]\n"),
                ..Default::default()
            },
            None,
        )
        .expect("daily");

        let read = VaultService::get_note(&daily).expect("read daily");
        assert!(read.note.wikilinks_out.iter().any(|path| path == &weekly));

        let backlinks = VaultService::backlinks(&weekly).expect("backlinks");
        assert!(backlinks.backlinks.iter().any(|path| path == &daily));
    }

    #[test]
    fn round_trip_write_read_search_delete() {
        let _guard = vault_test_lock();
        let path = format!(
            "journal/test-{}.md",
            uuid::Uuid::new_v4().simple()
        );
        let token = uuid::Uuid::new_v4().simple().to_string();
        let content = format!("# Vault Smoke\n\nmedousa vault token {token}\n");
        let request = VaultWriteRequest {
            path: Some(path.clone()),
            content: content.clone(),
            session_id: Some(format!("medousa-home-{token}")),
            semantic_tags: Some(vec!["smoke-test".to_string()]),
            auto_workshop_tags: true,
        };
        let written = VaultService::write_note(Some(&path), &request, None).expect("write");
        assert!(written.created);
        assert!(written.note.tags.iter().any(|tag| tag == "vault"));
        assert!(written.note.tags.iter().any(|tag| tag == "smoke-test"));
        let read = VaultService::get_note(&path).expect("read");
        assert!(read.content.contains("tags:"));
        let search = VaultService::search(Some(&format!("token {token}")), 5, None).expect("search");
        assert!(search.hits.iter().any(|hit| hit.note.path == path));
        let by_tag = VaultService::list_notes(None, 10, Some("smoke-test"), None);
        assert!(by_tag.notes.iter().any(|note| note.path == path));
        let deleted = VaultService::delete_note(&path).expect("delete");
        assert!(deleted.deleted);
    }
}
