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

use crate::store_root::{StoreEntry, StoreEntryKind, StorePath, StoreRoot, StoreRootError};

pub use medousa_types::session::{
    InvalidSessionId, MAX_SESSION_ID_BYTES, SessionId, validate_session_id,
};

const STORAGE_KEY_DOMAIN: &[u8] = b"medousa/session-storage/v1\0";
const OBJECT_KEY_DOMAIN: &[u8] = b"medousa/session-object/v1\0";

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

pub(crate) fn session_object_path(
    namespace: &'static [u8],
    object_id: &str,
    extension: &'static str,
) -> StorePath {
    debug_assert!(!namespace.is_empty());
    debug_assert!(
        !extension.is_empty() && extension.bytes().all(|byte| byte.is_ascii_alphanumeric())
    );
    let mut hasher = Sha256::new();
    hasher.update(OBJECT_KEY_DOMAIN);
    hasher.update(namespace);
    hasher.update([0]);
    hasher.update(object_id.as_bytes());
    StorePath::parse(&format!("o1-{:x}.{extension}", hasher.finalize()))
        .expect("object key must be a valid store path")
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

/// Capability-owned session-directory store with confined root-level indexes.
pub(crate) struct SessionDirectoryStore {
    root_path: PathBuf,
    legacy_dir: fn(&SessionId) -> StorePath,
    root: OnceCell<StoreRoot>,
}

impl std::fmt::Debug for SessionDirectoryStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SessionDirectoryStore")
            .field("root_path", &self.root_path)
            .field("initialized", &self.root.get().is_some())
            .finish_non_exhaustive()
    }
}

impl SessionDirectoryStore {
    pub fn new(root_path: PathBuf) -> Self {
        Self::new_with_legacy_directory(root_path, legacy_session_directory)
    }

    pub fn new_with_legacy_directory(
        root_path: PathBuf,
        legacy_dir: fn(&SessionId) -> StorePath,
    ) -> Self {
        Self {
            root_path,
            legacy_dir,
            root: OnceCell::new(),
        }
    }

    fn root(&self) -> Result<&StoreRoot, StoreRootError> {
        self.root
            .get_or_try_init(|| StoreRoot::open_or_create(&self.root_path))
    }

    fn current_dir(&self, session_id: &SessionId) -> StorePath {
        StorePath::parse(StorageKey::for_session(session_id).as_str())
            .expect("storage key must be a valid store path")
    }

    fn legacy_dir(&self, session_id: &SessionId) -> StorePath {
        (self.legacy_dir)(session_id)
    }

    fn session_dir_for_read(&self, session_id: &SessionId) -> Result<StorePath, StoreRootError> {
        let root = self.root()?;
        let current = self.current_dir(session_id);
        if root.is_dir(&current)? {
            return Ok(current);
        }
        let legacy = self.legacy_dir(session_id);
        if root.is_dir(&legacy)? {
            Ok(legacy)
        } else {
            Ok(current)
        }
    }

    fn session_dir_for_write(&self, session_id: &SessionId) -> Result<StorePath, StoreRootError> {
        let root = self.root()?;
        let current = self.current_dir(session_id);
        if root.is_dir(&current)? {
            return Ok(current);
        }
        let legacy = self.legacy_dir(session_id);
        if root.is_dir(&legacy)? {
            root.rename(&legacy, &current)?;
        } else {
            root.create_dir_all(&current)?;
        }
        Ok(current)
    }

    pub fn read(
        &self,
        session_id: &SessionId,
        relative: &StorePath,
    ) -> Result<Vec<u8>, StoreRootError> {
        let directory = self.session_dir_for_read(session_id)?;
        self.root()?.read(&directory.join(relative)?)
    }

    pub fn read_limited(
        &self,
        session_id: &SessionId,
        relative: &StorePath,
        max_bytes: u64,
    ) -> Result<Vec<u8>, StoreRootError> {
        let directory = self.session_dir_for_read(session_id)?;
        self.root()?
            .read_limited(&directory.join(relative)?, max_bytes)
    }

    pub fn atomic_write(
        &self,
        session_id: &SessionId,
        relative: &StorePath,
        bytes: &[u8],
    ) -> Result<(), StoreRootError> {
        let directory = self.session_dir_for_write(session_id)?;
        self.root()?.atomic_write(&directory.join(relative)?, bytes)
    }

    pub fn list(&self, session_id: &SessionId) -> Result<Vec<StoreEntry>, StoreRootError> {
        let directory = self.session_dir_for_read(session_id)?;
        self.root()?.list_directory(&directory)
    }

    pub fn remove_session(&self, session_id: &SessionId) -> Result<(), StoreRootError> {
        let root = self.root()?;
        root.remove_dir_all(&self.current_dir(session_id))?;
        root.remove_dir_all(&self.legacy_dir(session_id))
    }

    pub fn read_root(&self, path: &StorePath) -> Result<Vec<u8>, StoreRootError> {
        self.root()?.read(path)
    }

    pub fn append_root(&self, path: &StorePath, bytes: &[u8]) -> Result<(), StoreRootError> {
        self.root()?.append(path, bytes)
    }

    pub fn atomic_write_root(&self, path: &StorePath, bytes: &[u8]) -> Result<(), StoreRootError> {
        self.root()?.atomic_write(path, bytes)
    }
}

fn legacy_session_directory(session_id: &SessionId) -> StorePath {
    StorePath::parse(session_id.as_str()).expect("session id must be a valid store path")
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
    fn object_paths_are_stable_safe_and_domain_separated() {
        let extraction = session_object_path(b"extraction", "ext:session:1", "json");
        let same = session_object_path(b"extraction", "ext:session:1", "json");
        let verification = session_object_path(b"verification", "ext:session:1", "json");

        assert_eq!(extraction, same);
        assert_ne!(extraction, verification);
        assert!(extraction.file_name().starts_with("o1-"));
        assert!(extraction.file_name().ends_with(".json"));
        assert!(!extraction.file_name().contains(':'));
        assert!(StorePath::parse(extraction.file_name()).is_ok());
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
        let files = SessionFileStore::new(PathBuf::from("/trusted/history"), "jsonl");
        let path = files.current_path(&id("session-a"));
        assert!(path.file_name().starts_with("s1-"));
        assert!(path.file_name().ends_with(".jsonl"));
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
    fn capability_directory_store_migrates_nested_data_and_owns_its_index() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("artifacts");
        let legacy = root.join("session-a");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("old.json"), b"legacy").unwrap();
        let store = SessionDirectoryStore::new(root.clone());
        let session_id = id("session-a");
        let old = StorePath::parse("old.json").unwrap();
        let nested = StorePath::parse("nested/new.json").unwrap();
        let index = StorePath::parse("index.jsonl").unwrap();

        assert_eq!(store.read(&session_id, &old).unwrap(), b"legacy");
        store
            .atomic_write(&session_id, &nested, b"replacement")
            .unwrap();
        store.append_root(&index, b"one\n").unwrap();
        store.atomic_write_root(&index, b"two\n").unwrap();

        assert!(!legacy.exists());
        assert_eq!(store.read(&session_id, &nested).unwrap(), b"replacement");
        assert_eq!(store.read_root(&index).unwrap(), b"two\n");
        assert_eq!(store.list(&session_id).unwrap().len(), 2);
        store.remove_session(&session_id).unwrap();
        assert!(store.list(&session_id).is_err());
    }

    #[test]
    fn capability_directory_store_retains_the_original_root_handle() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("artifacts");
        let held = temp.path().join("held-artifacts");
        let store = SessionDirectoryStore::new(root.clone());
        store.root().unwrap();

        std::fs::rename(&root, &held).unwrap();
        std::fs::create_dir(&root).unwrap();
        store
            .atomic_write(
                &id("session-a"),
                &StorePath::parse("proof.json").unwrap(),
                b"held",
            )
            .unwrap();

        let directory = StorageKey::for_session(&id("session-a"));
        assert_eq!(
            std::fs::read(held.join(directory.as_str()).join("proof.json")).unwrap(),
            b"held"
        );
        assert!(!root.join(directory.as_str()).join("proof.json").exists());
    }

    #[cfg(unix)]
    #[test]
    fn capability_directory_store_rejects_link_backed_session_entries() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("artifacts");
        let outside = temp.path().join("outside");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("canary"), b"safe").unwrap();
        let session_id = id("session-a");
        let directory = root.join(StorageKey::for_session(&session_id).as_str());
        std::fs::create_dir(&directory).unwrap();
        symlink(outside.join("canary"), directory.join("payload.json")).unwrap();
        let store = SessionDirectoryStore::new(root);
        let payload = StorePath::parse("payload.json").unwrap();

        assert!(store.read(&session_id, &payload).is_err());
        assert!(
            store
                .atomic_write(&session_id, &payload, b"changed")
                .is_err()
        );
        assert_eq!(std::fs::read(outside.join("canary")).unwrap(), b"safe");
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
            let files = SessionFileStore::new(root, "json");
            let path = files.current_path(&session_id);
            assert!(!path.file_name().contains('/'), "{root_name}");
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
