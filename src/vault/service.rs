//! Vault service orchestration.

use anyhow::Result;

use crate::daemon_api::{
    VaultBacklinksResponse, VaultDeleteResponse, VaultNoteContentResponse,
    VaultNotesListResponse, VaultWriteRequest, VaultWriteResponse,
};
use crate::vault::search::search_vault;
use crate::vault::store::vault_store;
use crate::workspace::store::workspace_store;

pub struct VaultService;

impl VaultService {
    pub fn list_notes(prefix: Option<&str>, limit: usize) -> VaultNotesListResponse {
        let limit = limit.clamp(1, 500);
        let entries = vault_store().list_entries(prefix, limit);
        let notes = entries
            .into_iter()
            .map(|entry| entry.to_vault_note(vault_store().backlinks_for(&entry.path)))
            .collect();
        VaultNotesListResponse { notes }
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

    pub fn write_note(
        path: Option<&str>,
        request: &VaultWriteRequest,
        if_match: Option<&str>,
    ) -> Result<VaultWriteResponse> {
        let target_path = request
            .path
            .as_deref()
            .or(path)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("path is required"))?;
        let existed = vault_store().get_entry(target_path).is_some();
        let entry = vault_store().write_content(target_path, &request.content, if_match)?;
        append_vault_feed_event(&entry.path, &entry.title);
        Ok(VaultWriteResponse {
            note: entry.to_vault_note(vault_store().backlinks_for(&entry.path)),
            created: !existed,
        })
    }

    pub fn delete_note(path: &str) -> Result<VaultDeleteResponse> {
        vault_store().delete_note(path)?;
        Ok(VaultDeleteResponse {
            path: path.to_string(),
            deleted: true,
        })
    }

    pub fn search(query: &str, limit: usize) -> Result<crate::daemon_api::VaultSearchResponse> {
        search_vault(query, limit.clamp(1, 100))
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
mod tests {
    use std::sync::{Mutex, OnceLock};

    use super::*;

    fn vault_test_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .expect("vault test lock")
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
            },
            None,
        )
        .expect("weekly");
        VaultService::write_note(
            Some(&daily),
            &VaultWriteRequest {
                path: Some(daily.clone()),
                content: format!("# Daily\n\nSee [[weekly-review-{suffix}]]\n"),
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
        };
        let written = VaultService::write_note(Some(&path), &request, None).expect("write");
        assert!(written.created);
        let read = VaultService::get_note(&path).expect("read");
        assert_eq!(read.content, content);
        let search = VaultService::search(&format!("token {token}"), 5).expect("search");
        assert!(search.hits.iter().any(|hit| hit.note.path == path));
        let deleted = VaultService::delete_note(&path).expect("delete");
        assert!(deleted.deleted);
    }
}

fn append_vault_feed_event(path: &str, title: &str) {
    let mut refs = Vec::new();
    refs.push(crate::daemon_api::WorkspaceEventRef {
        ref_type: "vault_path".to_string(),
        ref_id: path.to_string(),
    });
    let event = crate::daemon_api::WorkspaceEvent {
        id: crate::workspace::event::new_event_id(),
        timestamp_utc: chrono::Utc::now(),
        kind: crate::daemon_api::WorkspaceEventKind::VaultNoteUpdated,
        actor: crate::daemon_api::WorkspaceEventActor::Operator,
        summary: format!("Vault updated — {title}"),
        refs,
    };
    workspace_store().append_event(event);
}
