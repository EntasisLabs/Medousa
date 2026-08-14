//! Versioned, collision-free filesystem layout for session-owned data.
//!
//! Request identifiers must be validated at ingress. Store code derives an
//! opaque key even after validation so logical identifiers never become path
//! components. The legacy helpers exist only to migrate strictly safe names;
//! malformed legacy entries are left untouched for the H02 quarantine flow.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use once_cell::sync::OnceCell;
use sha2::{Digest as _, Sha256};

use crate::store_root::{StoreEntryKind, StorePath, StoreRoot, StoreRootError};

pub use medousa_types::session::{
    InvalidSessionId, MAX_SESSION_ID_BYTES, SessionId, validate_session_id,
};

const STORAGE_KEY_DOMAIN: &[u8] = b"medousa/session-storage/v1\0";

pub fn new_session_id() -> SessionId {
    SessionId::parse(format!("ses_{}", uuid::Uuid::new_v4().simple()))
        .expect("daemon-generated session id must be valid")
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StorageKey(String);

impl StorageKey {
    pub fn for_session(session_id: &SessionId) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(STORAGE_KEY_DOMAIN);
        hasher.update(session_id.as_str().as_bytes());
        Self(format!("s1-{:x}", hasher.finalize()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_encoded(value: &str) -> bool {
        value.len() == 67
            && value.starts_with("s1-")
            && value.as_bytes()[3..].iter().all(u8::is_ascii_hexdigit)
    }
}

pub(crate) struct SessionFileEntry {
    path: StorePath,
    pub modified: Option<SystemTime>,
}

impl SessionFileEntry {
    pub fn file_stem(&self) -> &str {
        self.path
            .file_name()
            .rsplit_once('.')
            .map(|(stem, _)| stem)
            .expect("session file entry extension was validated")
    }
}

/// Capability-owned flat file store for one session data surface.
pub(crate) struct SessionFileStore {
    root_path: PathBuf,
    extension: &'static str,
    root: OnceCell<StoreRoot>,
}

impl SessionFileStore {
    pub fn new(root_path: PathBuf, extension: &'static str) -> Self {
        debug_assert!(
            !extension.is_empty() && extension.bytes().all(|byte| byte.is_ascii_alphanumeric())
        );
        Self {
            root_path,
            extension,
            root: OnceCell::new(),
        }
    }

    fn root(&self) -> Result<&StoreRoot, StoreRootError> {
        self.root
            .get_or_try_init(|| StoreRoot::open_or_create(&self.root_path))
    }

    fn current_path(&self, session_id: &SessionId) -> StorePath {
        self.path_for_stem(StorageKey::for_session(session_id).as_str())
    }

    fn legacy_path(&self, session_id: &SessionId) -> StorePath {
        self.path_for_stem(session_id.as_str())
    }

    fn path_for_stem(&self, stem: &str) -> StorePath {
        StorePath::parse(&format!("{stem}.{}", self.extension))
            .expect("validated session file name must be a valid store path")
    }

    fn migrate_legacy(&self, session_id: &SessionId) -> Result<StorePath, StoreRootError> {
        let root = self.root()?;
        let current = self.current_path(session_id);
        if root.is_file(&current)? {
            return Ok(current);
        }
        let legacy = self.legacy_path(session_id);
        if root.is_file(&legacy)? {
            root.rename(&legacy, &current)?;
        }
        Ok(current)
    }

    pub fn read(&self, session_id: &SessionId) -> Result<Vec<u8>, StoreRootError> {
        let root = self.root()?;
        let current = self.current_path(session_id);
        match root.read(&current) {
            Ok(bytes) => Ok(bytes),
            Err(error) if error.is_not_found() => root.read(&self.legacy_path(session_id)),
            Err(error) => Err(error),
        }
    }

    pub fn append(&self, session_id: &SessionId, bytes: &[u8]) -> Result<(), StoreRootError> {
        let path = self.migrate_legacy(session_id)?;
        self.root()?.append(&path, bytes)
    }

    pub fn atomic_write(&self, session_id: &SessionId, bytes: &[u8]) -> Result<(), StoreRootError> {
        let path = self.migrate_legacy(session_id)?;
        self.root()?.atomic_write(&path, bytes)
    }

    pub fn remove(&self, session_id: &SessionId) -> Result<(), StoreRootError> {
        let root = self.root()?;
        root.remove_file(&self.current_path(session_id))?;
        root.remove_file(&self.legacy_path(session_id))
    }

    pub fn list(&self) -> Result<Vec<SessionFileEntry>, StoreRootError> {
        let suffix = format!(".{}", self.extension);
        Ok(self
            .root()?
            .list_root()?
            .into_iter()
            .filter(|entry| entry.kind == StoreEntryKind::File)
            .filter(|entry| entry.path.file_name().ends_with(&suffix))
            .map(|entry| SessionFileEntry {
                path: entry.path,
                modified: entry.modified,
            })
            .collect())
    }

    pub fn read_entry(&self, entry: &SessionFileEntry) -> Result<Vec<u8>, StoreRootError> {
        self.root()?.read(&entry.path)
    }
}

pub fn session_file(root: &Path, session_id: &SessionId, extension: &str) -> PathBuf {
    root.join(format!(
        "{}.{}",
        StorageKey::for_session(session_id).as_str(),
        extension
    ))
}

pub fn session_dir(root: &Path, session_id: &SessionId) -> PathBuf {
    root.join(StorageKey::for_session(session_id).as_str())
}

fn legacy_session_dir(root: &Path, session_id: &SessionId) -> PathBuf {
    root.join(session_id.as_str())
}

/// Resolve an opaque directory, with a read-only fallback for a strictly safe legacy directory.
pub fn session_dir_for_read(root: &Path, session_id: &SessionId) -> PathBuf {
    let current = session_dir(root, session_id);
    if current.exists() {
        return current;
    }
    let legacy = legacy_session_dir(root, session_id);
    if legacy.is_dir() { legacy } else { current }
}

/// Return the opaque directory, migrating a strictly safe legacy directory first.
pub fn session_dir_for_write(root: &Path, session_id: &SessionId) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(root)?;
    let current = session_dir(root, session_id);
    let legacy = legacy_session_dir(root, session_id);
    if !current.exists() && legacy.is_dir() {
        std::fs::rename(legacy, &current)?;
    }
    std::fs::create_dir_all(&current)?;
    Ok(current)
}

/// Idempotently remove both the opaque directory and a strictly safe legacy directory.
pub fn remove_session_dir(root: &Path, session_id: &SessionId) -> std::io::Result<()> {
    remove_dir_if_present(&session_dir(root, session_id))?;
    remove_dir_if_present(&legacy_session_dir(root, session_id))?;
    Ok(())
}

fn remove_dir_if_present(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn legacy_session_file(root: &Path, session_id: &SessionId, extension: &str) -> PathBuf {
    root.join(format!("{}.{extension}", session_id.as_str()))
}

/// Resolve an opaque file, with a read-only fallback for a strictly safe legacy file.
pub fn session_file_for_read(root: &Path, session_id: &SessionId, extension: &str) -> PathBuf {
    let current = session_file(root, session_id, extension);
    if current.exists() {
        return current;
    }
    let legacy = legacy_session_file(root, session_id, extension);
    if legacy.is_file() { legacy } else { current }
}

/// Return the opaque write path, migrating a strictly safe legacy file first.
pub fn session_file_for_write(
    root: &Path,
    session_id: &SessionId,
    extension: &str,
) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(root)?;
    let current = session_file(root, session_id, extension);
    let legacy = legacy_session_file(root, session_id, extension);
    if !current.exists() && legacy.is_file() {
        std::fs::rename(legacy, &current)?;
    }
    Ok(current)
}

/// Idempotently unlink both the opaque file and a strictly safe legacy file.
pub fn remove_session_file(
    root: &Path,
    session_id: &SessionId,
    extension: &str,
) -> std::io::Result<()> {
    remove_file_if_present(&session_file(root, session_id, extension))?;
    remove_file_if_present(&legacy_session_file(root, session_id, extension))?;
    Ok(())
}

fn remove_file_if_present(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(value: &str) -> SessionId {
        SessionId::parse(value).unwrap()
    }

    #[test]
    fn compatibility_parser_accepts_existing_safe_forms_without_normalizing() {
        for value in ["session-abc", "medousa_home_123", "A", "0"] {
            assert_eq!(validate_session_id(value).unwrap(), value);
        }
    }

    #[test]
    fn compatibility_parser_rejects_hostile_and_ambiguous_forms() {
        for value in [
            "",
            " session",
            "session ",
            ".",
            "..",
            "../outside",
            "a/b",
            "a\\b",
            "/absolute",
            "C:\\absolute",
            "session.json",
            "session:stream",
            "nul",
            "COM1",
            "line\nfeed",
            "café",
        ] {
            assert!(validate_session_id(value).is_err(), "accepted {value:?}");
        }
        assert!(validate_session_id(&"a".repeat(MAX_SESSION_ID_BYTES + 1)).is_err());
    }

    #[test]
    fn storage_keys_are_versioned_stable_and_collision_free_for_distinct_spellings() {
        let lower_id = id("session-a");
        let upper_id = id("session-A");
        let underscore_id = id("session_a");
        let lower = StorageKey::for_session(&lower_id);
        assert!(lower.as_str().starts_with("s1-"));
        assert_eq!(lower, StorageKey::for_session(&lower_id));
        assert_ne!(lower, StorageKey::for_session(&upper_id));
        assert_ne!(lower, StorageKey::for_session(&underscore_id));
    }

    #[test]
    fn daemon_generated_ids_use_the_canonical_128_bit_format() {
        let first = new_session_id();
        let second = new_session_id();
        assert!(first.as_str().starts_with("ses_"));
        assert_eq!(first.as_str().len(), 36);
        assert_ne!(first, second);
    }

    #[test]
    fn session_files_are_flat_opaque_paths() {
        let root = Path::new("/trusted/history");
        let path = session_file(root, &id("session-a"), "jsonl");
        assert_eq!(path.parent(), Some(root));
        assert!(
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("s1-")
        );
    }

    #[test]
    fn session_directories_are_flat_opaque_paths() {
        let root = Path::new("/trusted/artifacts");
        let path = session_dir(root, &id("session-a"));
        assert_eq!(path.parent(), Some(root));
        assert!(
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("s1-")
        );
    }

    #[test]
    fn first_write_migrates_a_safe_legacy_file_without_copying() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("history");
        std::fs::create_dir_all(&root).unwrap();
        let legacy = root.join("session-a.jsonl");
        std::fs::write(&legacy, b"turn\n").unwrap();

        let session_id = id("session-a");
        let current = session_file_for_write(&root, &session_id, "jsonl").unwrap();

        assert_eq!(std::fs::read(&current).unwrap(), b"turn\n");
        assert!(!legacy.exists());
        assert_eq!(current, session_file(&root, &session_id, "jsonl"));
    }

    #[test]
    fn capability_file_store_migrates_appends_replaces_lists_and_removes() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("history");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("session-a.jsonl"), b"legacy\n").unwrap();
        let files = SessionFileStore::new(root.clone(), "jsonl");
        let session_id = id("session-a");

        assert_eq!(files.read(&session_id).unwrap(), b"legacy\n");
        files.append(&session_id, b"next\n").unwrap();
        assert_eq!(files.read(&session_id).unwrap(), b"legacy\nnext\n");
        assert!(!root.join("session-a.jsonl").exists());
        assert_eq!(files.list().unwrap().len(), 1);

        files.atomic_write(&session_id, b"replacement\n").unwrap();
        assert_eq!(files.read(&session_id).unwrap(), b"replacement\n");
        files.remove(&session_id).unwrap();
        assert!(files.list().unwrap().is_empty());
    }

    #[test]
    fn capability_file_store_retains_the_original_root_handle() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("history");
        let held = temp.path().join("held-history");
        let files = SessionFileStore::new(root.clone(), "jsonl");
        assert!(files.list().unwrap().is_empty());

        std::fs::rename(&root, &held).unwrap();
        std::fs::create_dir(&root).unwrap();
        files.append(&id("session-a"), b"held\n").unwrap();

        let file_name = format!(
            "{}.jsonl",
            StorageKey::for_session(&id("session-a")).as_str()
        );
        assert_eq!(std::fs::read(held.join(&file_name)).unwrap(), b"held\n");
        assert!(!root.join(file_name).exists());
    }

    #[test]
    fn first_write_migrates_a_safe_legacy_directory_without_copying() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("artifacts");
        let legacy = root.join("session-a");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("payload.json"), b"{}").unwrap();

        let session_id = id("session-a");
        let current = session_dir_for_write(&root, &session_id).unwrap();

        assert_eq!(std::fs::read(current.join("payload.json")).unwrap(), b"{}");
        assert!(!legacy.exists());
        assert_eq!(current, session_dir(&root, &session_id));
    }

    #[test]
    fn malformed_identifiers_cannot_acquire_storage_authority() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("history");
        std::fs::create_dir_all(&root).unwrap();
        let outside = temp.path().join("outside.jsonl");
        std::fs::write(&outside, b"canary").unwrap();

        assert!(SessionId::parse("../outside").is_err());
        assert_eq!(std::fs::read(outside).unwrap(), b"canary");

        let directory_root = temp.path().join("artifacts");
        std::fs::create_dir_all(&directory_root).unwrap();
        let outside_dir = temp.path().join("outside-dir");
        std::fs::create_dir_all(&outside_dir).unwrap();
        std::fs::write(outside_dir.join("canary"), b"safe").unwrap();

        assert!(SessionId::parse("../outside-dir").is_err());
        assert_eq!(std::fs::read(outside_dir.join("canary")).unwrap(), b"safe");
    }

    #[test]
    fn every_declared_satellite_uses_one_flat_opaque_component() {
        let trusted = Path::new("/trusted");
        let session_id = id("hostile-looking-session");
        for root_name in [
            "history",
            "catalog",
            "shared_catalog",
            "session_surfaces",
            "turn_ledger",
        ] {
            let root = trusted.join(root_name);
            let path = session_file(&root, &session_id, "json");
            assert_eq!(path.parent(), Some(root.as_path()), "{root_name}");
        }
        for root_name in [
            "artifacts",
            "media",
            "extractions",
            "verifications",
            "context_packs",
            "coder_turn_checkpoints",
        ] {
            let root = trusted.join(root_name);
            let path = session_dir(&root, &session_id);
            assert_eq!(path.parent(), Some(root.as_path()), "{root_name}");
        }
    }
}
