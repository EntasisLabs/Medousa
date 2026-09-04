//! Durable Bot profiles and their conversation bindings.

use std::fs;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

use chrono::Utc;
use medousa_types::{
    BOT_PROFILE_SCHEMA_VERSION, BotId, BotOpenResponse, BotProfile, BotSessionBinding,
    BotSessionKind, CreateBotRequest, DuplicateBotRequest, SessionBotResponse,
    SetBotArchivedRequest, SetSessionBotRequest, UpdateBotRequest,
};
use serde::{Deserialize, Serialize};

const MAX_BOTS_PER_PROFILE: usize = 100;
const MAX_DISPLAY_NAME_CHARS: usize = 80;
const MAX_ROLE_DESCRIPTION_CHARS: usize = 500;
const MAX_AVATAR_REF_CHARS: usize = 1_024;
const MAX_MANUSCRIPT_ID_CHARS: usize = 160;
const MAX_ADDITIONAL_MANUSCRIPTS: usize = 8;

static BOT_PROFILE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[derive(Debug, Serialize, Deserialize)]
struct BotProfileFile {
    #[serde(default = "current_schema_version")]
    schema_version: u32,
    #[serde(default)]
    bots: Vec<BotProfile>,
    #[serde(default)]
    bindings: Vec<BotSessionBinding>,
}

impl Default for BotProfileFile {
    fn default() -> Self {
        Self {
            schema_version: BOT_PROFILE_SCHEMA_VERSION,
            bots: Vec::new(),
            bindings: Vec::new(),
        }
    }
}

fn current_schema_version() -> u32 {
    BOT_PROFILE_SCHEMA_VERSION
}

#[derive(Debug, Clone)]
pub struct BotProfileStore {
    path: PathBuf,
}

impl BotProfileStore {
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn daemon_default() -> Self {
        Self::at(crate::paths::medousa_data_dir().join("bot_profiles.json"))
    }

    fn load(&self) -> Result<BotProfileFile, String> {
        let file = match fs::read(&self.path) {
            Ok(raw) => serde_json::from_slice::<BotProfileFile>(&raw)
                .map_err(|error| format!("decode Bot profiles: {error}"))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(BotProfileFile::default());
            }
            Err(error) => return Err(format!("read Bot profiles: {error}")),
        };
        if file.schema_version != BOT_PROFILE_SCHEMA_VERSION {
            return Err(format!(
                "unsupported Bot profile schema version {}",
                file.schema_version
            ));
        }
        Ok(file)
    }

    fn save(&self, file: &BotProfileFile) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("create Bot profile directory: {error}"))?;
        }
        let raw = serde_json::to_vec_pretty(file)
            .map_err(|error| format!("encode Bot profiles: {error}"))?;
        crate::session::atomic_write(&self.path, &raw)
            .map_err(|error| format!("write Bot profiles: {error}"))
    }

    pub fn list(&self, owner_profile_id: &str) -> Result<Vec<BotProfile>, String> {
        let owner_profile_id = validate_owner(owner_profile_id)?;
        let _guard = lock_store()?;
        let mut bots = self
            .load()?
            .bots
            .into_iter()
            .filter(|bot| bot.owner_profile_id == owner_profile_id)
            .collect::<Vec<_>>();
        bots.sort_by(|left, right| {
            left.archived
                .cmp(&right.archived)
                .then_with(|| right.updated_at.cmp(&left.updated_at))
                .then_with(|| left.display_name.cmp(&right.display_name))
        });
        Ok(bots)
    }

    pub fn get(&self, owner_profile_id: &str, bot_id: &BotId) -> Result<BotProfile, String> {
        let owner_profile_id = validate_owner(owner_profile_id)?;
        let _guard = lock_store()?;
        let file = self.load()?;
        owned_bot(&file, owner_profile_id, bot_id).cloned()
    }

    pub fn create(
        &self,
        owner_profile_id: &str,
        primary_session_id: &str,
        mut request: CreateBotRequest,
    ) -> Result<BotOpenResponse, String> {
        let owner_profile_id = validate_owner(owner_profile_id)?.to_string();
        validate_create_request(&mut request)?;
        let primary_session_id = validate_session(primary_session_id)?;
        let _guard = lock_store()?;
        let mut file = self.load()?;
        ensure_capacity(&file, &owner_profile_id)?;

        let bot_id = new_bot_id()?;
        let now = Utc::now();
        let bot = BotProfile {
            schema_version: BOT_PROFILE_SCHEMA_VERSION,
            memory_scope_id: bot_id.to_string(),
            bot_id: bot_id.clone(),
            owner_profile_id,
            display_name: request.display_name,
            role_description: request.role_description,
            avatar_ref: request.avatar_ref,
            primary_manuscript_id: request.primary_manuscript_id,
            additional_manuscript_ids: request.additional_manuscript_ids,
            default_mode: request.default_mode,
            primary_session_id: Some(primary_session_id.clone()),
            archived: false,
            revision: 1,
            created_at: now,
            updated_at: now,
        };
        let binding = BotSessionBinding {
            bot_id,
            session_id: primary_session_id,
            kind: BotSessionKind::Primary,
            bot_revision_at_bind: bot.revision,
            created_at: now,
        };
        file.bots.push(bot.clone());
        file.bindings.push(binding.clone());
        self.save(&file)?;
        Ok(BotOpenResponse { bot, binding })
    }

    pub fn update(
        &self,
        owner_profile_id: &str,
        bot_id: &BotId,
        mut request: UpdateBotRequest,
    ) -> Result<BotProfile, String> {
        let owner_profile_id = validate_owner(owner_profile_id)?;
        validate_update_request(&mut request)?;
        let _guard = lock_store()?;
        let mut file = self.load()?;
        let bot = owned_bot_mut(&mut file, owner_profile_id, bot_id)?;
        require_revision(bot, request.expected_revision)?;
        bot.display_name = request.display_name;
        bot.role_description = request.role_description;
        bot.avatar_ref = request.avatar_ref;
        bot.primary_manuscript_id = request.primary_manuscript_id;
        bot.additional_manuscript_ids = request.additional_manuscript_ids;
        bot.default_mode = request.default_mode;
        bot.revision = bot.revision.saturating_add(1);
        bot.updated_at = Utc::now();
        let updated = bot.clone();
        self.save(&file)?;
        Ok(updated)
    }

    pub fn set_archived(
        &self,
        owner_profile_id: &str,
        bot_id: &BotId,
        request: SetBotArchivedRequest,
    ) -> Result<BotProfile, String> {
        let owner_profile_id = validate_owner(owner_profile_id)?;
        let _guard = lock_store()?;
        let mut file = self.load()?;
        let bot = owned_bot_mut(&mut file, owner_profile_id, bot_id)?;
        require_revision(bot, request.expected_revision)?;
        if bot.archived != request.archived {
            bot.archived = request.archived;
            bot.revision = bot.revision.saturating_add(1);
            bot.updated_at = Utc::now();
        }
        let updated = bot.clone();
        self.save(&file)?;
        Ok(updated)
    }

    pub fn duplicate(
        &self,
        owner_profile_id: &str,
        source_bot_id: &BotId,
        primary_session_id: &str,
        mut request: DuplicateBotRequest,
    ) -> Result<BotOpenResponse, String> {
        let owner_profile_id = validate_owner(owner_profile_id)?.to_string();
        request.display_name = normalize_optional(
            request.display_name,
            MAX_DISPLAY_NAME_CHARS,
            "display_name",
        )?;
        let primary_session_id = validate_session(primary_session_id)?;
        let _guard = lock_store()?;
        let mut file = self.load()?;
        ensure_capacity(&file, &owner_profile_id)?;
        let source = owned_bot(&file, &owner_profile_id, source_bot_id)?.clone();
        let bot_id = new_bot_id()?;
        let now = Utc::now();
        let bot = BotProfile {
            schema_version: BOT_PROFILE_SCHEMA_VERSION,
            bot_id: bot_id.clone(),
            owner_profile_id,
            display_name: request
                .display_name
                .unwrap_or_else(|| format!("{} copy", source.display_name)),
            role_description: source.role_description,
            avatar_ref: source.avatar_ref,
            primary_manuscript_id: source.primary_manuscript_id,
            additional_manuscript_ids: source.additional_manuscript_ids,
            memory_scope_id: bot_id.to_string(),
            default_mode: source.default_mode,
            primary_session_id: Some(primary_session_id.clone()),
            archived: false,
            revision: 1,
            created_at: now,
            updated_at: now,
        };
        let binding = BotSessionBinding {
            bot_id,
            session_id: primary_session_id,
            kind: BotSessionKind::Primary,
            bot_revision_at_bind: bot.revision,
            created_at: now,
        };
        file.bots.push(bot.clone());
        file.bindings.push(binding.clone());
        self.save(&file)?;
        Ok(BotOpenResponse { bot, binding })
    }

    /// Return the primary conversation, creating a replacement when a previous
    /// primary conversation was explicitly unbound or deleted.
    pub fn open(
        &self,
        owner_profile_id: &str,
        bot_id: &BotId,
        replacement_session_id: &str,
    ) -> Result<BotOpenResponse, String> {
        let owner_profile_id = validate_owner(owner_profile_id)?;
        let replacement_session_id = validate_session(replacement_session_id)?;
        let _guard = lock_store()?;
        let mut file = self.load()?;
        let bot_index = owned_bot_index(&file, owner_profile_id, bot_id)?;
        if file.bots[bot_index].archived {
            return Err("archived Bot must be restored before opening it".to_string());
        }

        if let Some(binding) = file.bindings.iter().find(|binding| {
            binding.bot_id == *bot_id && binding.kind == BotSessionKind::Primary
        }) {
            return Ok(BotOpenResponse {
                bot: file.bots[bot_index].clone(),
                binding: binding.clone(),
            });
        }

        let now = Utc::now();
        let bot = &mut file.bots[bot_index];
        bot.primary_session_id = Some(replacement_session_id.clone());
        bot.revision = bot.revision.saturating_add(1);
        bot.updated_at = now;
        let binding = BotSessionBinding {
            bot_id: bot_id.clone(),
            session_id: replacement_session_id,
            kind: BotSessionKind::Primary,
            bot_revision_at_bind: bot.revision,
            created_at: now,
        };
        let bot = bot.clone();
        file.bindings.push(binding.clone());
        self.save(&file)?;
        Ok(BotOpenResponse { bot, binding })
    }

    pub fn resolve_session(
        &self,
        owner_profile_id: &str,
        session_id: &str,
    ) -> Result<SessionBotResponse, String> {
        let owner_profile_id = validate_owner(owner_profile_id)?;
        let session_id = validate_session(session_id)?;
        let _guard = lock_store()?;
        let file = self.load()?;
        let Some(binding) = file
            .bindings
            .iter()
            .find(|binding| binding.session_id == session_id)
            .cloned()
        else {
            return Ok(SessionBotResponse {
                session_id,
                binding: None,
                bot: None,
            });
        };
        let bot = owned_bot(&file, owner_profile_id, &binding.bot_id)?.clone();
        Ok(SessionBotResponse {
            session_id,
            binding: Some(binding),
            bot: Some(bot),
        })
    }

    pub fn bind_session(
        &self,
        owner_profile_id: &str,
        session_id: &str,
        request: SetSessionBotRequest,
    ) -> Result<SessionBotResponse, String> {
        let owner_profile_id = validate_owner(owner_profile_id)?;
        let session_id = validate_session(session_id)?;
        let _guard = lock_store()?;
        let mut file = self.load()?;
        let bot_index = owned_bot_index(&file, owner_profile_id, &request.bot_id)?;
        if file.bots[bot_index].archived {
            return Err("cannot bind a conversation to an archived Bot".to_string());
        }
        if let Some(existing) = file
            .bindings
            .iter()
            .find(|binding| binding.session_id == session_id)
        {
            if existing.bot_id == request.bot_id && existing.kind == request.kind {
                return Ok(SessionBotResponse {
                    session_id,
                    binding: Some(existing.clone()),
                    bot: Some(file.bots[bot_index].clone()),
                });
            }
            return Err("conversation is already bound to another Bot".to_string());
        }
        if request.kind == BotSessionKind::Primary
            && file.bindings.iter().any(|binding| {
                binding.bot_id == request.bot_id && binding.kind == BotSessionKind::Primary
            })
        {
            return Err("Bot already has a primary conversation".to_string());
        }

        let now = Utc::now();
        let bot = &mut file.bots[bot_index];
        if request.kind == BotSessionKind::Primary {
            bot.primary_session_id = Some(session_id.clone());
            bot.revision = bot.revision.saturating_add(1);
            bot.updated_at = now;
        }
        let binding = BotSessionBinding {
            bot_id: request.bot_id,
            session_id: session_id.clone(),
            kind: request.kind,
            bot_revision_at_bind: bot.revision,
            created_at: now,
        };
        let bot = bot.clone();
        file.bindings.push(binding.clone());
        self.save(&file)?;
        Ok(SessionBotResponse {
            session_id,
            binding: Some(binding),
            bot: Some(bot),
        })
    }

    pub fn unbind_session(
        &self,
        owner_profile_id: &str,
        session_id: &str,
    ) -> Result<SessionBotResponse, String> {
        let owner_profile_id = validate_owner(owner_profile_id)?;
        let session_id = validate_session(session_id)?;
        let _guard = lock_store()?;
        let mut file = self.load()?;
        let Some(binding_index) = file
            .bindings
            .iter()
            .position(|binding| binding.session_id == session_id)
        else {
            return Ok(SessionBotResponse {
                session_id,
                binding: None,
                bot: None,
            });
        };
        let binding = file.bindings[binding_index].clone();
        let bot_index = owned_bot_index(&file, owner_profile_id, &binding.bot_id)?;
        file.bindings.remove(binding_index);
        let bot = &mut file.bots[bot_index];
        if binding.kind == BotSessionKind::Primary {
            bot.primary_session_id = None;
            bot.revision = bot.revision.saturating_add(1);
            bot.updated_at = Utc::now();
        }
        let bot = bot.clone();
        self.save(&file)?;
        Ok(SessionBotResponse {
            session_id,
            binding: None,
            bot: Some(bot),
        })
    }

    /// Deletion-surface hook. The session is already authorized by the caller.
    pub fn remove_session_binding(&self, session_id: &str) -> Result<(), String> {
        let session_id = validate_session(session_id)?;
        let _guard = lock_store()?;
        let mut file = self.load()?;
        let Some(binding_index) = file
            .bindings
            .iter()
            .position(|binding| binding.session_id == session_id)
        else {
            return Ok(());
        };
        let binding = file.bindings.remove(binding_index);
        if binding.kind == BotSessionKind::Primary
            && let Some(bot) = file.bots.iter_mut().find(|bot| bot.bot_id == binding.bot_id)
        {
            bot.primary_session_id = None;
            bot.revision = bot.revision.saturating_add(1);
            bot.updated_at = Utc::now();
        }
        self.save(&file)
    }
}

fn lock_store() -> Result<std::sync::MutexGuard<'static, ()>, String> {
    BOT_PROFILE_LOCK
        .lock()
        .map_err(|_| "Bot profile store lock poisoned".to_string())
}

fn ensure_capacity(file: &BotProfileFile, owner_profile_id: &str) -> Result<(), String> {
    if file
        .bots
        .iter()
        .filter(|bot| bot.owner_profile_id == owner_profile_id)
        .count()
        >= MAX_BOTS_PER_PROFILE
    {
        return Err(format!("Bot profile limit reached ({MAX_BOTS_PER_PROFILE})"));
    }
    Ok(())
}

fn owned_bot<'a>(
    file: &'a BotProfileFile,
    owner_profile_id: &str,
    bot_id: &BotId,
) -> Result<&'a BotProfile, String> {
    file.bots
        .iter()
        .find(|bot| bot.bot_id == *bot_id && bot.owner_profile_id == owner_profile_id)
        .ok_or_else(|| "Bot not found".to_string())
}

fn owned_bot_mut<'a>(
    file: &'a mut BotProfileFile,
    owner_profile_id: &str,
    bot_id: &BotId,
) -> Result<&'a mut BotProfile, String> {
    file.bots
        .iter_mut()
        .find(|bot| bot.bot_id == *bot_id && bot.owner_profile_id == owner_profile_id)
        .ok_or_else(|| "Bot not found".to_string())
}

fn owned_bot_index(
    file: &BotProfileFile,
    owner_profile_id: &str,
    bot_id: &BotId,
) -> Result<usize, String> {
    file.bots
        .iter()
        .position(|bot| bot.bot_id == *bot_id && bot.owner_profile_id == owner_profile_id)
        .ok_or_else(|| "Bot not found".to_string())
}

fn require_revision(bot: &BotProfile, expected_revision: u64) -> Result<(), String> {
    if bot.revision != expected_revision {
        return Err(format!(
            "Bot revision conflict: expected {expected_revision}, current {}",
            bot.revision
        ));
    }
    Ok(())
}

fn validate_owner(owner_profile_id: &str) -> Result<&str, String> {
    let owner_profile_id = owner_profile_id.trim();
    if owner_profile_id.is_empty() {
        return Err("owner profile is required".to_string());
    }
    Ok(owner_profile_id)
}

fn validate_session(session_id: &str) -> Result<String, String> {
    medousa_types::SessionId::parse(session_id)
        .map(|value| value.to_string())
        .map_err(|error| error.to_string())
}

fn validate_create_request(request: &mut CreateBotRequest) -> Result<(), String> {
    request.display_name = normalize_required(
        std::mem::take(&mut request.display_name),
        MAX_DISPLAY_NAME_CHARS,
        "display_name",
    )?;
    request.role_description = normalize_optional(
        request.role_description.take(),
        MAX_ROLE_DESCRIPTION_CHARS,
        "role_description",
    )?;
    request.avatar_ref = normalize_optional(
        request.avatar_ref.take(),
        MAX_AVATAR_REF_CHARS,
        "avatar_ref",
    )?;
    normalize_manuscripts(
        &mut request.primary_manuscript_id,
        &mut request.additional_manuscript_ids,
    )
}

fn validate_update_request(request: &mut UpdateBotRequest) -> Result<(), String> {
    request.display_name = normalize_required(
        std::mem::take(&mut request.display_name),
        MAX_DISPLAY_NAME_CHARS,
        "display_name",
    )?;
    request.role_description = normalize_optional(
        request.role_description.take(),
        MAX_ROLE_DESCRIPTION_CHARS,
        "role_description",
    )?;
    request.avatar_ref = normalize_optional(
        request.avatar_ref.take(),
        MAX_AVATAR_REF_CHARS,
        "avatar_ref",
    )?;
    normalize_manuscripts(
        &mut request.primary_manuscript_id,
        &mut request.additional_manuscript_ids,
    )
}

fn normalize_manuscripts(primary: &mut String, additional: &mut Vec<String>) -> Result<(), String> {
    *primary = normalize_required(
        std::mem::take(primary),
        MAX_MANUSCRIPT_ID_CHARS,
        "primary_manuscript_id",
    )?;
    if additional.len() > MAX_ADDITIONAL_MANUSCRIPTS {
        return Err(format!(
            "additional_manuscript_ids exceeds {MAX_ADDITIONAL_MANUSCRIPTS} entries"
        ));
    }
    let mut normalized = Vec::with_capacity(additional.len());
    for value in std::mem::take(additional) {
        let value = normalize_required(value, MAX_MANUSCRIPT_ID_CHARS, "manuscript_id")?;
        if value == *primary {
            return Err("primary manuscript cannot also be additional".to_string());
        }
        if normalized.contains(&value) {
            return Err("additional manuscripts must be unique".to_string());
        }
        normalized.push(value);
    }
    *additional = normalized;
    Ok(())
}

fn normalize_required(value: String, max_chars: usize, field: &str) -> Result<String, String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(format!("{field} is required"));
    }
    if value.chars().count() > max_chars {
        return Err(format!("{field} exceeds {max_chars} characters"));
    }
    Ok(value)
}

fn normalize_optional(
    value: Option<String>,
    max_chars: usize,
    field: &str,
) -> Result<Option<String>, String> {
    value
        .map(|value| normalize_required(value, max_chars, field))
        .transpose()
}

fn new_bot_id() -> Result<BotId, String> {
    BotId::parse(format!("bot_{}", uuid::Uuid::new_v4().simple()))
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(root: &std::path::Path) -> BotProfileStore {
        BotProfileStore::at(root.join("bot_profiles.json"))
    }

    fn create_request(name: &str) -> CreateBotRequest {
        CreateBotRequest {
            display_name: name.to_string(),
            role_description: Some("Explains systems clearly".to_string()),
            avatar_ref: None,
            primary_manuscript_id: "specialist-mentor".to_string(),
            additional_manuscript_ids: vec!["specialist-rust".to_string()],
            default_mode: Some(medousa_types::AgentModeId::Teacher),
        }
    }

    #[test]
    fn profiles_and_bindings_are_profile_scoped_and_durable() {
        let temp = tempfile::tempdir().unwrap();
        let bots = store(temp.path());
        let created = bots
            .create("user:alice", "session-alice-bot", create_request("Ada"))
            .unwrap();

        let reloaded = store(temp.path());
        assert_eq!(reloaded.list("user:alice").unwrap().len(), 1);
        assert!(reloaded.list("user:bob").unwrap().is_empty());
        assert!(reloaded.get("user:bob", &created.bot.bot_id).is_err());
        let resolved = reloaded
            .resolve_session("user:alice", "session-alice-bot")
            .unwrap();
        assert_eq!(resolved.bot.unwrap().bot_id, created.bot.bot_id);
    }

    #[test]
    fn duplicate_has_fresh_identity_memory_and_conversation() {
        let temp = tempfile::tempdir().unwrap();
        let bots = store(temp.path());
        let original = bots
            .create("user:alice", "session-original", create_request("Ada"))
            .unwrap();
        let duplicate = bots
            .duplicate(
                "user:alice",
                &original.bot.bot_id,
                "session-duplicate",
                DuplicateBotRequest::default(),
            )
            .unwrap();

        assert_ne!(duplicate.bot.bot_id, original.bot.bot_id);
        assert_ne!(duplicate.bot.memory_scope_id, original.bot.memory_scope_id);
        assert_ne!(
            duplicate.bot.primary_session_id,
            original.bot.primary_session_id
        );
        assert_eq!(
            duplicate.bot.primary_manuscript_id,
            original.bot.primary_manuscript_id
        );
    }

    #[test]
    fn updates_require_the_current_revision() {
        let temp = tempfile::tempdir().unwrap();
        let bots = store(temp.path());
        let created = bots
            .create("user:alice", "session-original", create_request("Ada"))
            .unwrap();
        let request = UpdateBotRequest {
            expected_revision: 0,
            display_name: "Grace".to_string(),
            role_description: None,
            avatar_ref: None,
            primary_manuscript_id: "specialist-mentor".to_string(),
            additional_manuscript_ids: vec![],
            default_mode: None,
        };
        assert!(
            bots.update("user:alice", &created.bot.bot_id, request)
                .unwrap_err()
                .contains("revision conflict")
        );
    }

    #[test]
    fn archiving_preserves_the_primary_binding() {
        let temp = tempfile::tempdir().unwrap();
        let bots = store(temp.path());
        let created = bots
            .create("user:alice", "session-original", create_request("Ada"))
            .unwrap();
        bots.set_archived(
            "user:alice",
            &created.bot.bot_id,
            SetBotArchivedRequest {
                archived: true,
                expected_revision: 1,
            },
        )
        .unwrap();

        let resolved = bots
            .resolve_session("user:alice", "session-original")
            .unwrap();
        assert!(resolved.bot.unwrap().archived);
        assert_eq!(resolved.binding.unwrap().kind, BotSessionKind::Primary);
    }
}
