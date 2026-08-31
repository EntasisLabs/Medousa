use std::fs;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

use chrono::Utc;
use medousa_types::{
    CreatePromptStashRequest, DeletePromptStashResponse, PromptStash, PromptStashId,
};
use serde::{Deserialize, Serialize};

const MAX_STASHES_PER_PROFILE: usize = 200;
const MAX_DRAFT_BYTES: usize = 64 * 1024;
const MAX_LABEL_CHARS: usize = 120;
const MAX_MEDIA_REFS: usize = 16;

static PROMPT_STASH_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[derive(Debug, Default, Serialize, Deserialize)]
struct PromptStashFile {
    #[serde(default)]
    stashes: Vec<PromptStash>,
}

#[derive(Debug, Clone)]
pub struct PromptStashStore {
    path: PathBuf,
}

impl PromptStashStore {
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn daemon_default() -> Self {
        Self::at(crate::paths::medousa_data_dir().join("prompt_stashes.json"))
    }

    fn load(&self) -> Result<PromptStashFile, String> {
        match fs::read(&self.path) {
            Ok(raw) => serde_json::from_slice(&raw)
                .map_err(|error| format!("decode prompt stashes: {error}")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(PromptStashFile::default())
            }
            Err(error) => Err(format!("read prompt stashes: {error}")),
        }
    }

    fn save(&self, file: &PromptStashFile) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("create prompt stash directory: {error}"))?;
        }
        let raw = serde_json::to_vec_pretty(file)
            .map_err(|error| format!("encode prompt stashes: {error}"))?;
        crate::session::atomic_write(&self.path, &raw)
            .map_err(|error| format!("write prompt stashes: {error}"))
    }

    pub fn create(
        &self,
        created_by: &str,
        mut request: CreatePromptStashRequest,
    ) -> Result<PromptStash, String> {
        validate_request(&mut request)?;
        let _guard = PROMPT_STASH_LOCK
            .lock()
            .map_err(|_| "prompt stash store lock poisoned".to_string())?;
        let mut file = self.load()?;
        if file
            .stashes
            .iter()
            .filter(|stash| stash.created_by == created_by)
            .count()
            >= MAX_STASHES_PER_PROFILE
        {
            return Err(format!(
                "prompt stash limit reached ({MAX_STASHES_PER_PROFILE})"
            ));
        }
        let now = Utc::now();
        let stash = PromptStash {
            stash_id: PromptStashId::parse(format!("pst_{}", uuid::Uuid::new_v4().simple()))
                .map_err(|error| error.to_string())?,
            label: request.label,
            draft: request.draft,
            context_manifest_id: request.context_manifest_id,
            source_session: request.source_session,
            created_by: created_by.to_string(),
            created_at: now,
            updated_at: now,
        };
        file.stashes.push(stash.clone());
        self.save(&file)?;
        Ok(stash)
    }

    pub fn list(&self, created_by: &str) -> Result<Vec<PromptStash>, String> {
        let _guard = PROMPT_STASH_LOCK
            .lock()
            .map_err(|_| "prompt stash store lock poisoned".to_string())?;
        let mut stashes: Vec<_> = self
            .load()?
            .stashes
            .into_iter()
            .filter(|stash| stash.created_by == created_by)
            .collect();
        stashes.sort_by_key(|stash| std::cmp::Reverse(stash.updated_at));
        Ok(stashes)
    }

    pub fn delete(
        &self,
        created_by: &str,
        stash_id: &PromptStashId,
    ) -> Result<DeletePromptStashResponse, String> {
        let _guard = PROMPT_STASH_LOCK
            .lock()
            .map_err(|_| "prompt stash store lock poisoned".to_string())?;
        let mut file = self.load()?;
        let before = file.stashes.len();
        file.stashes
            .retain(|stash| stash.stash_id != *stash_id || stash.created_by != created_by);
        let deleted = file.stashes.len() != before;
        if deleted {
            self.save(&file)?;
        }
        Ok(DeletePromptStashResponse {
            stash_id: stash_id.clone(),
            deleted,
        })
    }
}

fn validate_request(request: &mut CreatePromptStashRequest) -> Result<(), String> {
    if request.draft.text.len() > MAX_DRAFT_BYTES {
        return Err(format!("draft exceeds {MAX_DRAFT_BYTES} bytes"));
    }
    if request.draft.text.trim().is_empty() && request.draft.media_refs.is_empty() {
        return Err("prompt stash must contain text or attachments".to_string());
    }
    if request.draft.media_refs.len() > MAX_MEDIA_REFS {
        return Err(format!("prompt stash exceeds {MAX_MEDIA_REFS} attachments"));
    }
    request.label = request
        .label
        .take()
        .map(|label| label.trim().to_string())
        .filter(|label| !label.is_empty());
    if request
        .label
        .as_ref()
        .is_some_and(|label| label.chars().count() > MAX_LABEL_CHARS)
    {
        return Err(format!("label exceeds {MAX_LABEL_CHARS} characters"));
    }
    request.draft.mode = normalize_hint(request.draft.mode.take(), "mode")?;
    request.draft.model = normalize_hint(request.draft.model.take(), "model")?;
    Ok(())
}

fn normalize_hint(value: Option<String>, field: &str) -> Result<Option<String>, String> {
    let normalized = value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if normalized
        .as_ref()
        .is_some_and(|value| value.chars().count() > 160)
    {
        return Err(format!("{field} exceeds 160 characters"));
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use medousa_types::PromptStashDraft;

    fn store(root: &std::path::Path) -> PromptStashStore {
        PromptStashStore::at(root.join("prompt_stashes.json"))
    }

    fn request(text: &str) -> CreatePromptStashRequest {
        CreatePromptStashRequest {
            label: Some("  Follow up  ".to_string()),
            draft: PromptStashDraft {
                text: text.to_string(),
                media_refs: vec![],
                mode: Some(" general ".to_string()),
                model: None,
            },
            context_manifest_id: None,
            source_session: None,
        }
    }

    #[test]
    fn explicit_stashes_are_profile_scoped_and_durable() {
        let temp = tempfile::tempdir().unwrap();
        let prompt_store = store(temp.path());
        let alice = prompt_store
            .create("user:alice", request("draft one"))
            .unwrap();
        prompt_store
            .create("user:bob", request("draft two"))
            .unwrap();

        let reloaded = store(temp.path()).list("user:alice").unwrap();
        assert_eq!(reloaded.len(), 1);
        assert_eq!(reloaded[0].stash_id, alice.stash_id);
        assert_eq!(reloaded[0].label.as_deref(), Some("Follow up"));
        assert_eq!(reloaded[0].draft.mode.as_deref(), Some("general"));
    }

    #[test]
    fn deletion_does_not_reveal_or_remove_another_profiles_stash() {
        let temp = tempfile::tempdir().unwrap();
        let store = store(temp.path());
        let stash = store.create("user:alice", request("private")).unwrap();

        assert!(!store.delete("user:bob", &stash.stash_id).unwrap().deleted);
        assert_eq!(store.list("user:alice").unwrap().len(), 1);
        assert!(store.delete("user:alice", &stash.stash_id).unwrap().deleted);
        assert!(store.list("user:alice").unwrap().is_empty());
    }

    #[test]
    fn empty_stashes_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let error = store(temp.path())
            .create("user:alice", request("  "))
            .unwrap_err();
        assert!(error.contains("text or attachments"));
    }
}
