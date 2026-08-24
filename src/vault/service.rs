//! Vault service orchestration.

use anyhow::Result;

use crate::daemon_api::{
    VaultBacklinksResponse, VaultDeleteResponse, VaultFileContentResponse,
    VaultNoteContentResponse, VaultNotesListResponse, VaultTagsListResponse, VaultWriteRequest,
    VaultWriteResponse, WorkspaceEventActor,
};
use crate::vault::path::{VaultPath, user_vault_capability};
use crate::vault::search::search_vault;
use crate::vault::semantic_tags::{
    apply_semantic_tags_on_write, collect_distinct_tags, entry_has_all_tags, parse_tags_query,
};
use crate::vault::store::vault_store;
#[cfg(feature = "full-daemon")]
use crate::workspace::store::workspace_store;
use base64::Engine;

pub struct VaultService;

impl VaultService {
    pub fn list_notes(
        prefix: Option<&str>,
        limit: usize,
        tags: Option<&str>,
        tag_prefix: Option<&str>,
    ) -> VaultNotesListResponse {
        Self::list_notes_paged(prefix, limit, tags, tag_prefix, None, None)
    }

    pub fn list_notes_paged(
        prefix: Option<&str>,
        limit: usize,
        tags: Option<&str>,
        tag_prefix: Option<&str>,
        cursor: Option<&str>,
        expected_generation: Option<u64>,
    ) -> VaultNotesListResponse {
        let limit = limit.clamp(1, 500);
        let required = parse_tags_query(tags);
        let prefix_filter = tag_prefix
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_ascii_lowercase());
        let _ = vault_store().ensure_index_fresh();
        let projection = crate::vault::store::vault_projection();
        let generation = projection.generation;
        let root = crate::vault::owner::ensure_owner_for_active_root()
            .map(|owner| owner.root_id.as_str().to_string())
            .unwrap_or_else(|_| "local".into());
        let filter = list_filter_hash(prefix, tags, tag_prefix);
        if let Some(expected) = expected_generation
            && expected > 0
            && generation > 0
            && expected != generation
        {
            return list_reset(generation);
        }
        let after_path = if let Some(raw) = cursor.map(str::trim).filter(|value| !value.is_empty())
        {
            match decode_list_cursor(raw) {
                Some((cursor_root, cursor_filter, cursor_gen, path))
                    if cursor_root == root
                        && cursor_filter == filter
                        && cursor_gen == generation =>
                {
                    Some(path)
                }
                _ => return list_reset(generation),
            }
        } else {
            None
        };

        let path_prefix = prefix
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let start: std::ops::Bound<&str> = if let Some(after) = after_path.as_deref() {
            std::ops::Bound::Excluded(after)
        } else if let Some(prefix) = path_prefix.as_deref() {
            std::ops::Bound::Included(prefix)
        } else {
            std::ops::Bound::Unbounded
        };
        let mut notes = Vec::new();
        let mut truncated = false;
        for path in projection
            .sorted_paths
            .range::<str, _>((start, std::ops::Bound::Unbounded))
        {
            if let Some(prefix) = path_prefix.as_deref()
                && !path.starts_with(prefix)
            {
                break;
            }
            let Some(entry) = projection.by_path.get(path) else {
                continue;
            };
            if !entry_has_all_tags(&entry.tags, &required) {
                continue;
            }
            if prefix_filter.as_ref().is_some_and(|prefix| {
                !entry
                    .tags
                    .iter()
                    .any(|tag| tag.to_ascii_lowercase().starts_with(prefix))
            }) {
                continue;
            }
            if notes.len() >= limit {
                truncated = true;
                break;
            }
            notes.push(entry.to_vault_note_summary());
        }
        let next_cursor = if truncated {
            notes
                .last()
                .map(|note| encode_list_cursor(&root, &filter, generation, &note.path))
        } else {
            None
        };
        VaultNotesListResponse {
            notes,
            vault_generation: Some(generation),
            next_cursor,
            truncated,
            reset_required: false,
        }
    }

    pub fn changes_since(
        since_generation: Option<u64>,
        cursor: Option<&str>,
        limit: usize,
    ) -> crate::daemon_api::VaultChangesResponse {
        let limit = limit.clamp(1, 500);
        let _ = vault_store().ensure_index_fresh();
        let Ok(owner) = crate::vault::owner::ensure_owner_for_active_root() else {
            return changes_reset(0);
        };
        let generation = owner.current_generation();
        let Some(since) = since_generation.filter(|value| *value > 0) else {
            return changes_reset(generation);
        };
        if since > generation {
            return changes_reset(generation);
        }
        let (after_generation, after_path) =
            match cursor.map(str::trim).filter(|value| !value.is_empty()) {
                Some(raw) => match decode_change_cursor(raw) {
                    Some((cursor_root, cursor_generation, path))
                        if cursor_root == owner.root_id.as_str() =>
                    {
                        (Some(cursor_generation), Some(path))
                    }
                    _ => return changes_reset(generation),
                },
                None => (None, None),
            };
        let (records, truncated, reset) =
            owner.changes_since(since, after_generation, after_path.as_deref(), limit);
        if reset {
            return changes_reset(generation);
        }
        let changes = records
            .iter()
            .map(|record| crate::daemon_api::VaultChangeEntry {
                path: record.path.clone(),
                kind: record.kind.clone(),
                note_version: record.note_version.clone(),
            })
            .collect::<Vec<_>>();
        let next_cursor = if truncated {
            records.last().map(|record| {
                encode_change_cursor(owner.root_id.as_str(), record.generation, &record.path)
            })
        } else {
            None
        };
        crate::daemon_api::VaultChangesResponse {
            vault_generation: generation,
            changes,
            next_cursor,
            reset_required: false,
        }
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
        const MAX_ATTEMPTS: usize = 4;
        let mut last_err = None;
        for _ in 0..MAX_ATTEMPTS {
            let _ = vault_store().ensure_index_fresh();
            let generation = crate::vault::store::vault_projection().generation;
            let Some(entry) = vault_store().peek_entry_public(path) else {
                last_err = Some(anyhow::anyhow!("vault note not found: {path}"));
                break;
            };
            let content = match vault_store().read_content(path) {
                Ok(body) => body,
                Err(error) => {
                    last_err = Some(error);
                    continue;
                }
            };
            let projection = crate::vault::store::vault_projection();
            if projection.generation != generation {
                // Generation moved; retry as a single-generation assembly.
                continue;
            }
            let backlinks = if generation > 0 {
                projection.backlinks(&entry.path)
            } else {
                vault_store().backlinks_for(path)
            };
            let generation_after = crate::vault::store::vault_projection().generation;
            if generation_after != generation {
                continue;
            }
            return Ok(VaultNoteContentResponse {
                note: entry.to_vault_note(backlinks),
                content,
                vault_generation: Some(generation),
                note_version: Some(
                    crate::vault::contracts::NoteVersion::encode(
                        crate::vault::owner::ensure_owner_for_active_root()
                            .map(|owner| owner.root_id.as_str().to_string())
                            .unwrap_or_else(|_| "local".into())
                            .as_str(),
                        &entry.source,
                        generation.max(1),
                        &entry.content_hash,
                    )
                    .as_str()
                    .to_string(),
                ),
            });
        }
        Err(last_err.unwrap_or_else(|| {
            anyhow::anyhow!("vault note changed during read; retry the request")
        }))
    }

    /// Read a vault-relative file (images, attachments) for remote Home preview.
    pub fn read_file(path: &str) -> Result<VaultFileContentResponse> {
        const MAX_PREVIEW_BYTES: u64 = 8 * 1024 * 1024;
        let path = VaultPath::parse(path)?;
        let files = user_vault_capability()?;
        if !files.is_file(&path)? {
            anyhow::bail!("vault file not found: {path}");
        }
        let size = files.metadata(&path)?.size;
        if size > MAX_PREVIEW_BYTES {
            anyhow::bail!("vault file too large for preview (max 8MB)");
        }
        let bytes = files.read_limited(&path, MAX_PREVIEW_BYTES)?;
        let content_type = mime_guess_from_path(std::path::Path::new(path.as_str()));
        Ok(VaultFileContentResponse {
            path: path.to_string(),
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

    /// Create-only write used by `POST /v1/vault/notes`. Refuses to clobber an existing path.
    pub fn create_note(request: &VaultWriteRequest) -> Result<VaultWriteResponse> {
        let target_path = request
            .path
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("path is required"))?;
        if vault_store().note_exists(target_path) {
            anyhow::bail!("a note already exists at path: {target_path}");
        }
        Self::write_note(None, request, None)
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
        let auto_workshop_tags = if crate::vault::roots::active_root_skips_auto_workshop_tags() {
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
        let (entry, note_version) =
            vault_store().write_content_versioned(target_path, &content, if_match)?;
        append_vault_feed_event(&entry.path, &entry.title, !existed, actor, tool_name);
        let generation = crate::vault::store::vault_projection().generation;
        Ok(VaultWriteResponse {
            note: entry.to_vault_note(vault_store().backlinks_for(&entry.path)),
            created: !existed,
            content: Some(content),
            vault_generation: Some(generation),
            note_version: Some(note_version.as_str().to_string()),
        })
    }

    pub fn delete_note(path: &str) -> Result<VaultDeleteResponse> {
        vault_store().delete_note(path)?;
        Ok(VaultDeleteResponse {
            path: path.to_string(),
            deleted: true,
        })
    }

    pub fn list_trash(limit: usize) -> Result<crate::daemon_api::VaultTrashListResponse> {
        let entries = vault_store()
            .list_trash(limit)?
            .into_iter()
            .map(|(path, trashed_at)| crate::daemon_api::VaultTrashEntry {
                path,
                trashed_at: trashed_at.map(|ts| ts.to_rfc3339()),
            })
            .collect();
        Ok(crate::daemon_api::VaultTrashListResponse { entries })
    }

    pub fn restore_from_trash(path: &str) -> Result<crate::daemon_api::VaultTrashRestoreResponse> {
        let restored_path = vault_store().restore_from_trash(path)?;
        Ok(crate::daemon_api::VaultTrashRestoreResponse {
            path: restored_path,
            restored: true,
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
                vault_generation: read.vault_generation,
                note_version: read.note_version,
            });
        }
        if vault_store().get_entry(to).is_some() {
            anyhow::bail!("a note already exists at path: {to}");
        }
        if crate::vault::owner::owner_mutations_active() {
            let owner = crate::vault::owner::ensure_owner_for_active_root()
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            crate::vault::relocate::relocate_move(&owner, from, to)
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            // Refresh derived index for destination; source removed by relocate.
            let _ = vault_store().refresh_from_disk();
            return Self::get_note(to).map(|read| VaultWriteResponse {
                note: read.note,
                created: false,
                content: Some(read.content),
                vault_generation: read.vault_generation,
                note_version: read.note_version,
            });
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
            Some("cognition_store_write"),
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
        if query
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
        {
            if required.is_empty() {
                return Ok(crate::daemon_api::VaultSearchResponse {
                    query: String::new(),
                    hits: Vec::new(),
                    indexing: None,
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
                        tags: note.tags,
                    },
                    score: 1.0,
                    matched_terms: required.clone(),
                    snippet: None,
                })
                .collect();
            return Ok(crate::daemon_api::VaultSearchResponse {
                query: required.join(", "),
                hits,
                indexing: None,
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
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Run vault integration work against an isolated temp root (not the live user vault).
#[cfg(test)]
pub(crate) fn with_temp_vault<T>(f: impl FnOnce() -> T) -> T {
    use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

    let _lock = vault_integration_test_lock();
    let base = std::env::temp_dir().join(format!(
        "medousa-vault-test-{}",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(&base).expect("temp vault root");
    let base = base.canonicalize().expect("canonical temp vault root");
    crate::vault::owner::reset_vault_owners();
    crate::vault::path::clear_vault_root_capabilities();
    crate::vault::roots::set_test_vault_root_override(Some(base.clone()));
    let _ = vault_store().refresh_from_disk();
    let result = catch_unwind(AssertUnwindSafe(f));
    crate::vault::roots::set_test_vault_root_override(None);
    crate::vault::owner::reset_vault_owners();
    crate::vault::path::clear_vault_root_capabilities();
    let _ = vault_store().refresh_from_disk();
    let _ = std::fs::remove_dir_all(&base);
    match result {
        Ok(value) => value,
        Err(payload) => resume_unwind(payload),
    }
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

fn list_filter_hash(prefix: Option<&str>, tags: Option<&str>, tag_prefix: Option<&str>) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    prefix.unwrap_or_default().hash(&mut hasher);
    tags.unwrap_or_default().hash(&mut hasher);
    tag_prefix.unwrap_or_default().hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

fn encode_list_cursor(root: &str, filter: &str, generation: u64, path: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(format!("{root}\x1f{filter}\x1f{generation}\x1f{path}"))
}

fn decode_list_cursor(raw: &str) -> Option<(String, String, u64, String)> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(raw.trim())
        .ok()?;
    let text = String::from_utf8(bytes).ok()?;
    let mut parts = text.split('\x1f');
    let root = parts.next()?.to_string();
    let filter = parts.next()?.to_string();
    let generation = parts.next()?.parse().ok()?;
    let path = parts.next()?.to_string();
    if parts.next().is_some() {
        return None;
    }
    Some((root, filter, generation, path))
}

fn encode_change_cursor(root: &str, generation: u64, path: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(format!("{root}\x1f{generation}\x1f{path}"))
}

fn decode_change_cursor(raw: &str) -> Option<(String, u64, String)> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(raw.trim())
        .ok()?;
    let text = String::from_utf8(bytes).ok()?;
    let mut parts = text.split('\x1f');
    let root = parts.next()?.to_string();
    let generation = parts.next()?.parse().ok()?;
    let path = parts.next()?.to_string();
    if parts.next().is_some() {
        return None;
    }
    Some((root, generation, path))
}

fn list_reset(generation: u64) -> VaultNotesListResponse {
    VaultNotesListResponse {
        notes: Vec::new(),
        vault_generation: Some(generation),
        next_cursor: None,
        truncated: false,
        reset_required: true,
    }
}

fn changes_reset(generation: u64) -> crate::daemon_api::VaultChangesResponse {
    crate::daemon_api::VaultChangesResponse {
        vault_generation: generation,
        changes: Vec::new(),
        next_cursor: None,
        reset_required: true,
    }
}

fn append_vault_feed_event(
    path: &str,
    title: &str,
    created: bool,
    actor: WorkspaceEventActor,
    tool_name: Option<&str>,
) {
    #[cfg(not(feature = "full-daemon"))]
    {
        let _ = (path, title, created, actor, tool_name);
    }
    #[cfg(feature = "full-daemon")]
    {
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
        let tool_names = tool_name
            .map(|name| vec![name.to_string()])
            .unwrap_or_default();
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wikilink_resolves_and_backlinks() {
        with_temp_vault(|| {
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
        });
    }

    #[test]
    fn create_note_refuses_existing_path() {
        with_temp_vault(|| {
            let path = format!("journal/create-once-{}.md", uuid::Uuid::new_v4().simple());
            let request = VaultWriteRequest {
                path: Some(path.clone()),
                content: "# Keep me\n\nimportant body\n".to_string(),
                ..Default::default()
            };
            let first = VaultService::create_note(&request).expect("first create");
            assert!(first.created);
            let clash = VaultWriteRequest {
                path: Some(path.clone()),
                content: "# Wiped\n\ntemplate\n".to_string(),
                ..Default::default()
            };
            let err = VaultService::create_note(&clash).expect_err("second create must fail");
            assert!(
                err.to_string().contains("already exists"),
                "unexpected error: {err}"
            );
            let read = VaultService::get_note(&path).expect("read after refused create");
            assert!(read.content.contains("important body"));
            assert!(!read.content.contains("template"));
        });
    }

    #[test]
    fn round_trip_write_read_search_delete() {
        with_temp_vault(|| {
            let path = format!("journal/test-{}.md", uuid::Uuid::new_v4().simple());
            let token = uuid::Uuid::new_v4().simple().to_string();
            let content = format!("# Vault Smoke\n\nmedousa vault token {token}\n");
            let request = VaultWriteRequest {
                path: Some(path.clone()),
                content: content.clone(),
                session_id: Some(format!("medousa-home-{token}")),
                semantic_tags: Some(vec!["smoke-test".to_string()]),
                auto_workshop_tags: true,
            };
            let skips_auto_tags = crate::vault::roots::active_root_skips_auto_workshop_tags();
            let written = VaultService::write_note(Some(&path), &request, None).expect("write");
            assert!(written.created);
            assert_eq!(
                written.note.tags.iter().any(|tag| tag == "vault"),
                !skips_auto_tags
            );
            assert!(written.note.tags.iter().any(|tag| tag == "smoke-test"));
            let read = VaultService::get_note(&path).expect("read");
            assert_eq!(
                written.note_version.as_deref(),
                read.note_version.as_deref(),
                "write and get must return the same opaque NoteVersion"
            );
            assert!(
                written.note_version.as_ref().is_some_and(|value| {
                    crate::vault::contracts::NoteVersion::parse(value.clone()).is_encoded()
                }),
                "write must return an encoded NoteVersion, not a raw digest"
            );
            assert!(read.content.contains("tags:"));
            let search =
                VaultService::search(Some(&format!("token {token}")), 5, None).expect("search");
            assert!(search.hits.iter().any(|hit| hit.note.path == path));
            let by_tag = VaultService::list_notes(None, 10, Some("smoke-test"), None);
            assert!(by_tag.notes.iter().any(|note| note.path == path));
            let deleted = VaultService::delete_note(&path).expect("delete");
            assert!(deleted.deleted);
        });
    }

    #[test]
    fn warm_write_does_not_rewrite_full_index_or_links() {
        with_temp_vault(|| {
            for index in 0..24 {
                let path = format!("journal/warm-{index}.md");
                VaultService::write_note(
                    Some(&path),
                    &VaultWriteRequest {
                        path: Some(path.clone()),
                        content: format!("# Warm {index}\n"),
                        ..Default::default()
                    },
                    None,
                )
                .expect("warm");
            }
            crate::vault::baseline::vault_baseline_counters().reset();
            let path = "journal/hot-write.md".to_string();
            VaultService::write_note(
                Some(&path),
                &VaultWriteRequest {
                    path: Some(path.clone()),
                    content: "# Hot\n\nSee [[warm-0]]\n".to_string(),
                    ..Default::default()
                },
                None,
            )
            .expect("hot write");
            let snap = crate::vault::baseline::vault_baseline_counters().snapshot();
            assert_eq!(
                snap.index_rewrites, 0,
                "hot write must journal a delta, not rewrite index.jsonl"
            );
            assert_eq!(
                snap.link_rebuilds, 0,
                "hot write must not rebuild the full link index"
            );
            let read = VaultService::get_note("journal/hot-write.md").expect("read");
            assert!(
                read.note
                    .wikilinks_out
                    .iter()
                    .any(|path| path == "journal/warm-0.md")
            );
        });
    }

    #[cfg(unix)]
    #[test]
    fn link_backed_vault_entries_cannot_reach_outside_root() {
        use std::os::unix::fs::symlink;

        with_temp_vault(|| {
            let outside = tempfile::tempdir().expect("outside tempdir");
            let canary = outside.path().join("canary.md");
            std::fs::write(&canary, b"outside-safe").expect("canary");
            let vault = crate::vault::path::user_vault_root();
            symlink(&canary, vault.join("linked.md")).expect("link leaf");
            symlink(outside.path(), vault.join("linked-dir")).expect("link directory");

            assert!(VaultService::read_file("linked.md").is_err());
            assert!(VaultService::get_note("linked-dir/canary.md").is_err());
            let request = VaultWriteRequest {
                path: Some("linked-dir/new.md".to_string()),
                content: "must not escape".to_string(),
                ..Default::default()
            };
            assert!(VaultService::write_note(None, &request, None).is_err());
            assert_eq!(
                std::fs::read(&canary).expect("read canary"),
                b"outside-safe"
            );
            assert!(!outside.path().join("new.md").exists());
        });
    }

    #[test]
    fn incremental_reconcile_resolves_wikilinks_against_resident() {
        with_temp_vault(|| {
            let suffix = uuid::Uuid::new_v4().simple();
            let target = format!("journal/resident-b-{suffix}.md");
            let source = format!("journal/resident-a-{suffix}.md");
            VaultService::write_note(
                Some(&target),
                &VaultWriteRequest {
                    path: Some(target.clone()),
                    content: "# Target\n".to_string(),
                    ..Default::default()
                },
                None,
            )
            .expect("target");
            VaultService::write_note(
                Some(&source),
                &VaultWriteRequest {
                    path: Some(source.clone()),
                    content: format!("# Source\n\nSee [[resident-b-{suffix}]]\n"),
                    ..Default::default()
                },
                None,
            )
            .expect("source");

            let files = crate::vault::path::user_vault_capability().expect("capability");
            let source_path = crate::vault::path::VaultPath::parse(&source).expect("path");
            files
                .atomic_write(
                    &source_path,
                    format!("# Source\n\nSee [[resident-b-{suffix}]]\n\nextra\n").as_bytes(),
                )
                .expect("dirty source on disk");
            crate::vault::store::PROJECTION.mark_stale_reconciling();
            vault_store()
                .ensure_index_fresh()
                .expect("incremental reconcile");

            let read = VaultService::get_note(&source).expect("read source");
            assert!(
                read.note.wikilinks_out.iter().any(|path| path == &target),
                "forward link to unmodified resident note must survive dirty reconcile"
            );
            let backlinks = VaultService::backlinks(&target).expect("backlinks");
            assert!(
                backlinks.backlinks.iter().any(|path| path == &source),
                "backlink from dirty note must be retained without rewriting the target"
            );
        });
    }

    #[test]
    fn skipped_dirty_read_does_not_certify_reconcile() {
        with_temp_vault(|| {
            let path = format!("journal/skip-{}.md", uuid::Uuid::new_v4().simple());
            VaultService::write_note(
                Some(&path),
                &VaultWriteRequest {
                    path: Some(path.clone()),
                    content: "# Ok\n".to_string(),
                    ..Default::default()
                },
                None,
            )
            .expect("write");
            let files = crate::vault::path::user_vault_capability().expect("capability");
            files
                .atomic_write(
                    &crate::vault::path::VaultPath::parse(&path).expect("path"),
                    &[0xff, 0xfe, 0x00],
                )
                .expect("invalid utf-8");
            crate::vault::store::PROJECTION.mark_stale_reconciling();
            vault_store()
                .ensure_index_fresh()
                .expect("reconcile with skipped dirty");
            assert!(
                crate::vault::store::PROJECTION.needs_reconcile(),
                "failed dirty reads must not certify the reconcile epoch"
            );
        });
    }

    #[test]
    fn list_cursor_is_opaque_and_stale_cursor_resets() {
        with_temp_vault(|| {
            for index in 0..3 {
                let path = format!("journal/page-{index}.md");
                VaultService::write_note(
                    Some(&path),
                    &VaultWriteRequest {
                        path: Some(path.clone()),
                        content: format!("# Page {index}\n"),
                        ..Default::default()
                    },
                    None,
                )
                .expect("write");
            }
            let first = VaultService::list_notes_paged(None, 1, None, None, None, None);
            assert!(first.truncated);
            let cursor = first.next_cursor.expect("cursor");
            assert!(
                !cursor.contains('\x1f') && super::decode_list_cursor(&cursor).is_some(),
                "cursor must be opaque, got {cursor}"
            );
            let second = VaultService::list_notes_paged(None, 1, None, None, Some(&cursor), None);
            assert!(!second.reset_required);
            assert_ne!(
                first.notes.first().map(|note| note.path.as_str()),
                second.notes.first().map(|note| note.path.as_str())
            );
            let stale =
                VaultService::list_notes_paged(None, 1, None, None, Some("not-a-cursor"), None);
            assert!(stale.reset_required);
            assert!(stale.notes.is_empty());
        });
    }

    #[test]
    fn delete_only_generation_emits_delete_or_reset() {
        with_temp_vault(|| {
            let path = format!("journal/gone-{}.md", uuid::Uuid::new_v4().simple());
            VaultService::write_note(
                Some(&path),
                &VaultWriteRequest {
                    path: Some(path.clone()),
                    content: "# Gone\n".to_string(),
                    ..Default::default()
                },
                None,
            )
            .expect("write");
            let listed = VaultService::list_notes_paged(None, 10, None, None, None, None);
            let since = listed.vault_generation.expect("generation");
            VaultService::delete_note(&path).expect("delete");
            let changes = VaultService::changes_since(Some(since), None, 50);
            if changes.reset_required {
                return;
            }
            assert!(
                changes
                    .changes
                    .iter()
                    .any(|change| change.path == path && change.kind == "delete"),
                "delete-only generation must emit a delete, got {:?}",
                changes.changes
            );
        });
    }
}
