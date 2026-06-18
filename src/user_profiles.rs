//! Workshop operator profiles — switchable identity principals (`user:work`, `user:home`, …).

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::identity_memory::DEFAULT_USER_ID;
use crate::session;

const PROFILES_FILENAME: &str = "user_profiles.json";
const DEFAULT_PROFILE_DISPLAY_NAME: &str = "Personal";
const MAX_PROFILE_SLUG_LEN: usize = 32;

use std::sync::{Arc, OnceLock, RwLock as StdRwLock};

static WORKSHOP_PROFILE_REGISTRY: OnceLock<StdRwLock<Option<Arc<StdRwLock<UserProfileRegistry>>>>> =
    OnceLock::new();

fn workshop_registry_slot() -> &'static StdRwLock<Option<Arc<StdRwLock<UserProfileRegistry>>>> {
    WORKSHOP_PROFILE_REGISTRY.get_or_init(|| StdRwLock::new(None))
}

/// Wire daemon-owned registry so runtime paths read the same active profile as the API.
pub fn init_workshop_profile_registry(registry: Arc<StdRwLock<UserProfileRegistry>>) {
    *workshop_registry_slot()
        .write()
        .expect("workshop profile registry lock") = Some(registry);
}

fn identity_env_override_user_id() -> Option<String> {
    std::env::var("MEDOUSA_IDENTITY_USER_ID")
        .ok()
        .or_else(|| std::env::var("STASIS_DEFAULT_USER_ID").ok())
        .and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
}

fn registry_active_identity_user_id() -> String {
    if let Some(registry) = workshop_registry_slot()
        .read()
        .expect("workshop profile registry lock")
        .as_ref()
    {
        return registry
            .read()
            .expect("profile registry lock")
            .active_identity_user_id();
    }
    UserProfileRegistry::load_or_bootstrap().active_identity_user_id()
}

/// Canonical workshop operator identity: env override, then active profile from settings.
pub fn resolve_workshop_identity_user_id() -> String {
    identity_env_override_user_id().unwrap_or_else(registry_active_identity_user_id)
}

/// Active profile id from registry (`user:{slug}`), ignoring env user override.
pub fn resolve_workshop_active_profile_id() -> String {
    if let Some(registry) = workshop_registry_slot()
        .read()
        .expect("workshop profile registry lock")
        .as_ref()
    {
        return registry
            .read()
            .expect("profile registry lock")
            .active_profile_id()
            .to_string();
    }
    UserProfileRegistry::load_or_bootstrap()
        .active_profile_id()
        .to_string()
}

/// Profile id for a specific registry record (export/import), not the active profile.
pub fn profile_id_for_slug(slug: &str) -> String {
    format_profile_id(slug)
}

/// Per-turn override (debug) or active workshop profile.
pub fn resolve_workshop_identity_user_id_for_turn(explicit: Option<&str>) -> String {
    explicit
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(resolve_workshop_identity_user_id)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileRecord {
    pub profile_id: String,
    pub display_name: String,
    pub created_at: DateTime<Utc>,
    pub is_default: bool,
    #[serde(default)]
    pub archived: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserProfilesDocument {
    pub active_profile_id: String,
    pub profiles: Vec<ProfileRecord>,
}

impl Default for UserProfilesDocument {
    fn default() -> Self {
        Self {
            active_profile_id: DEFAULT_USER_ID.to_string(),
            profiles: vec![default_profile_record()],
        }
    }
}

#[derive(Debug, Clone)]
pub struct UserProfileRegistry {
    document: UserProfilesDocument,
}

impl UserProfileRegistry {
    pub fn load_or_bootstrap() -> Self {
        match Self::load_from_disk() {
            Ok(mut registry) => {
                registry.ensure_default_profile();
                if let Err(err) = registry.save_to_disk() {
                    eprintln!("user profiles: failed to persist bootstrap: {err}");
                }
                registry
            }
            Err(err) => {
                eprintln!("user profiles: using defaults ({err})");
                let registry = Self {
                    document: UserProfilesDocument::default(),
                };
                if let Err(save_err) = registry.save_to_disk() {
                    eprintln!("user profiles: failed to write defaults: {save_err}");
                }
                registry
            }
        }
    }

    fn load_from_disk() -> Result<Self> {
        let path = profiles_path();
        if !path.exists() {
            return Ok(Self {
                document: UserProfilesDocument::default(),
            });
        }
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("read {}", path.display()))?;
        let document: UserProfilesDocument =
            serde_json::from_str(&raw).context("parse user_profiles.json")?;
        Ok(Self { document })
    }

    fn save_to_disk(&self) -> Result<()> {
        let path = profiles_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        let encoded = serde_json::to_string_pretty(&self.document).context("encode profiles")?;
        fs::write(&path, encoded).with_context(|| format!("write {}", path.display()))?;
        Ok(())
    }

    fn ensure_default_profile(&mut self) {
        if !self
            .document
            .profiles
            .iter()
            .any(|profile| profile.profile_id == DEFAULT_USER_ID && !profile.archived)
        {
            self.document.profiles.insert(0, default_profile_record());
        }
        if !self.active_profiles().any(|profile| {
            profile.profile_id == self.document.active_profile_id
        }) {
            self.document.active_profile_id = DEFAULT_USER_ID.to_string();
        }
    }

    pub fn profiles_path(&self) -> PathBuf {
        profiles_path()
    }

    pub fn list_profiles(&self) -> Vec<ProfileRecord> {
        self.active_profiles().cloned().collect()
    }

    pub fn active_profile_id(&self) -> &str {
        self.document.active_profile_id.as_str()
    }

    /// Active profile's identity user id (ignores env override — use [`resolve_workshop_identity_user_id`]).
    pub fn active_identity_user_id(&self) -> String {
        if self
            .document
            .profiles
            .iter()
            .any(|profile| {
                profile.profile_id == self.document.active_profile_id && !profile.archived
            })
        {
            self.document.active_profile_id.clone()
        } else {
            DEFAULT_USER_ID.to_string()
        }
    }

    /// Resolved workshop identity principal: env override, then active profile, then default.
    pub fn resolve_active_user_id(&self) -> String {
        identity_env_override_user_id().unwrap_or_else(|| self.active_identity_user_id())
    }

    pub fn create_profile(&mut self, slug: &str, display_name: &str) -> Result<ProfileRecord> {
        let slug = normalize_profile_slug(slug)?;
        let profile_id = format_profile_id(&slug);
        if is_reserved_profile_slug(&slug) {
            bail!("profile slug '{slug}' is reserved");
        }
        if self.document.profiles.iter().any(|profile| {
            profile.profile_id == profile_id && !profile.archived
        }) {
            bail!("profile already exists: {profile_id}");
        }

        let display_name = normalize_display_name(display_name)?;
        let record = ProfileRecord {
            profile_id: profile_id.clone(),
            display_name,
            created_at: Utc::now(),
            is_default: false,
            archived: false,
        };
        self.document.profiles.push(record.clone());
        self.save_to_disk()?;
        Ok(record)
    }

    pub fn set_active_profile(&mut self, profile_id: &str) -> Result<String> {
        let profile_id = profile_id.trim();
        if profile_id.is_empty() {
            bail!("profile_id must not be empty");
        }
        if !self.document.profiles.iter().any(|profile| {
            profile.profile_id == profile_id && !profile.archived
        }) {
            bail!("profile not found: {profile_id}");
        }
        self.document.active_profile_id = profile_id.to_string();
        self.save_to_disk()?;
        Ok(self.resolve_active_user_id())
    }
}

fn profiles_path() -> PathBuf {
    session::medousa_data_dir().join(PROFILES_FILENAME)
}

fn default_profile_record() -> ProfileRecord {
    ProfileRecord {
        profile_id: DEFAULT_USER_ID.to_string(),
        display_name: DEFAULT_PROFILE_DISPLAY_NAME.to_string(),
        created_at: Utc::now(),
        is_default: true,
        archived: false,
    }
}

impl UserProfileRegistry {
    fn active_profiles(&self) -> impl Iterator<Item = &ProfileRecord> {
        self.document.profiles.iter().filter(|profile| !profile.archived)
    }
}

pub fn normalize_profile_slug(raw: &str) -> Result<String> {
    let slug = raw.trim().to_ascii_lowercase();
    if slug.is_empty() {
        bail!("profile slug must not be empty");
    }
    if slug.len() > MAX_PROFILE_SLUG_LEN {
        bail!("profile slug must be at most {MAX_PROFILE_SLUG_LEN} characters");
    }
    let mut chars = slug.chars();
    let Some(first) = chars.next() else {
        bail!("profile slug must not be empty");
    };
    if !first.is_ascii_lowercase() {
        bail!("profile slug must start with a lowercase letter");
    }
    if !chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-') {
        bail!("profile slug may only contain [a-z0-9_-]");
    }
    Ok(slug)
}

pub fn format_profile_id(slug: &str) -> String {
    format!("user:{slug}")
}

pub fn profile_slug_from_id(profile_id: &str) -> Option<&str> {
    profile_id.strip_prefix("user:").filter(|slug| !slug.is_empty())
}

pub fn is_reserved_profile_slug(slug: &str) -> bool {
    slug.starts_with("medousa-prompts") || slug == "daemon-agent-runtime"
}

fn normalize_display_name(raw: &str) -> Result<String> {
    let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed.trim();
    if trimmed.is_empty() {
        bail!("display_name must not be empty");
    }
    Ok(trimmed.chars().take(64).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, RwLock as StdRwLock};

    #[test]
    fn normalizes_slug_and_formats_profile_id() {
        let slug = normalize_profile_slug("Work").expect("slug");
        assert_eq!(slug, "work");
        assert_eq!(format_profile_id(&slug), "user:work");
    }

    #[test]
    fn rejects_reserved_slug() {
        assert!(is_reserved_profile_slug("medousa-prompts-system"));
        assert!(is_reserved_profile_slug("daemon-agent-runtime"));
    }

    #[test]
    fn default_document_has_personal_profile() {
        let doc = UserProfilesDocument::default();
        assert_eq!(doc.active_profile_id, DEFAULT_USER_ID);
        assert_eq!(doc.profiles.len(), 1);
        assert!(doc.profiles[0].is_default);
    }

    #[test]
    fn workshop_interactive_channel_uses_profile_slug() {
        let registry = Arc::new(StdRwLock::new(UserProfileRegistry {
            document: UserProfilesDocument {
                active_profile_id: "user:work".to_string(),
                profiles: vec![
                    default_profile_record(),
                    ProfileRecord {
                        profile_id: "user:work".to_string(),
                        display_name: "Work".to_string(),
                        created_at: Utc::now(),
                        is_default: false,
                        archived: false,
                    },
                ],
            },
        }));
        init_workshop_profile_registry(registry);
        assert_eq!(
            crate::identity_memory::workshop_interactive_channel_id(),
            "channel:work"
        );
    }

    #[test]
    fn resolve_active_uses_active_profile_when_env_default() {
        let registry = UserProfileRegistry {
            document: UserProfilesDocument {
                active_profile_id: "user:work".to_string(),
                profiles: vec![
                    default_profile_record(),
                    ProfileRecord {
                        profile_id: "user:work".to_string(),
                        display_name: "Work".to_string(),
                        created_at: Utc::now(),
                        is_default: false,
                        archived: false,
                    },
                ],
            },
        };
        assert_eq!(registry.active_identity_user_id(), "user:work");
    }
}
