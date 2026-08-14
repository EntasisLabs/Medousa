//! Versioned, collision-free filesystem layout for session-owned data.
//!
//! Request identifiers must be validated at ingress. Store code derives an
//! opaque key even after validation so logical identifiers never become path
//! components. The legacy helpers exist only to migrate strictly safe names;
//! malformed legacy entries are left untouched for the H02 quarantine flow.

use std::fmt;
use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};

const STORAGE_KEY_DOMAIN: &[u8] = b"medousa/session-storage/v1\0";
pub const MAX_SESSION_ID_BYTES: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidSessionId {
    reason: &'static str,
}

impl InvalidSessionId {
    pub fn reason(&self) -> &'static str {
        self.reason
    }
}

impl fmt::Display for InvalidSessionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid session id: {}", self.reason)
    }
}

impl std::error::Error for InvalidSessionId {}

/// Validate the compatibility session-ID grammar without normalization.
pub fn validate_session_id(session_id: &str) -> Result<&str, InvalidSessionId> {
    if session_id.is_empty() {
        return Err(InvalidSessionId { reason: "empty" });
    }
    if session_id.len() > MAX_SESSION_ID_BYTES {
        return Err(InvalidSessionId { reason: "too_long" });
    }
    if !session_id.is_ascii() {
        return Err(InvalidSessionId {
            reason: "non_ascii",
        });
    }
    if !session_id
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(InvalidSessionId {
            reason: "invalid_character",
        });
    }
    if is_windows_device_name(session_id) {
        return Err(InvalidSessionId {
            reason: "platform_alias",
        });
    }
    Ok(session_id)
}

fn is_windows_device_name(session_id: &str) -> bool {
    let upper = session_id.to_ascii_uppercase();
    matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || upper
            .strip_prefix("COM")
            .or_else(|| upper.strip_prefix("LPT"))
            .is_some_and(|suffix| suffix.len() == 1 && matches!(suffix.as_bytes()[0], b'1'..=b'9'))
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StorageKey(String);

impl StorageKey {
    pub fn for_session(session_id: &str) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(STORAGE_KEY_DOMAIN);
        hasher.update(session_id.as_bytes());
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

pub fn session_file(root: &Path, session_id: &str, extension: &str) -> PathBuf {
    root.join(format!(
        "{}.{}",
        StorageKey::for_session(session_id).as_str(),
        extension
    ))
}

pub fn session_dir(root: &Path, session_id: &str) -> PathBuf {
    root.join(StorageKey::for_session(session_id).as_str())
}

pub fn legacy_session_dir(root: &Path, session_id: &str) -> Option<PathBuf> {
    validate_session_id(session_id)
        .ok()
        .map(|safe| root.join(safe))
}

/// Resolve an opaque directory, with a read-only fallback for a strictly safe legacy directory.
pub fn session_dir_for_read(root: &Path, session_id: &str) -> PathBuf {
    let current = session_dir(root, session_id);
    if current.exists() {
        return current;
    }
    legacy_session_dir(root, session_id)
        .filter(|legacy| legacy.is_dir())
        .unwrap_or(current)
}

/// Return the opaque directory, migrating a strictly safe legacy directory first.
pub fn session_dir_for_write(root: &Path, session_id: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(root)?;
    let current = session_dir(root, session_id);
    if !current.exists()
        && let Some(legacy) = legacy_session_dir(root, session_id)
        && legacy.is_dir()
    {
        std::fs::rename(legacy, &current)?;
    }
    std::fs::create_dir_all(&current)?;
    Ok(current)
}

/// Idempotently remove both the opaque directory and a strictly safe legacy directory.
pub fn remove_session_dir(root: &Path, session_id: &str) -> std::io::Result<()> {
    remove_dir_if_present(&session_dir(root, session_id))?;
    if let Some(legacy) = legacy_session_dir(root, session_id) {
        remove_dir_if_present(&legacy)?;
    }
    Ok(())
}

fn remove_dir_if_present(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

pub fn legacy_session_file(root: &Path, session_id: &str, extension: &str) -> Option<PathBuf> {
    validate_session_id(session_id)
        .ok()
        .map(|safe| root.join(format!("{safe}.{extension}")))
}

/// Resolve an opaque file, with a read-only fallback for a strictly safe legacy file.
pub fn session_file_for_read(root: &Path, session_id: &str, extension: &str) -> PathBuf {
    let current = session_file(root, session_id, extension);
    if current.exists() {
        return current;
    }
    legacy_session_file(root, session_id, extension)
        .filter(|legacy| legacy.is_file())
        .unwrap_or(current)
}

/// Return the opaque write path, migrating a strictly safe legacy file first.
pub fn session_file_for_write(
    root: &Path,
    session_id: &str,
    extension: &str,
) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(root)?;
    let current = session_file(root, session_id, extension);
    if !current.exists()
        && let Some(legacy) = legacy_session_file(root, session_id, extension)
        && legacy.is_file()
    {
        std::fs::rename(legacy, &current)?;
    }
    Ok(current)
}

/// Idempotently unlink both the opaque file and a strictly safe legacy file.
pub fn remove_session_file(root: &Path, session_id: &str, extension: &str) -> std::io::Result<()> {
    remove_file_if_present(&session_file(root, session_id, extension))?;
    if let Some(legacy) = legacy_session_file(root, session_id, extension) {
        remove_file_if_present(&legacy)?;
    }
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
        let lower = StorageKey::for_session("session-a");
        assert!(lower.as_str().starts_with("s1-"));
        assert_eq!(lower, StorageKey::for_session("session-a"));
        assert_ne!(lower, StorageKey::for_session("session-A"));
        assert_ne!(lower, StorageKey::for_session("session_a"));
    }

    #[test]
    fn hostile_identifiers_cannot_select_a_parent_path() {
        let root = Path::new("/trusted/history");
        for value in ["../outside", "/outside", "a/b", "a\\b"] {
            let path = session_file(root, value, "jsonl");
            assert_eq!(path.parent(), Some(root));
            assert!(
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("s1-")
            );
        }
    }

    #[test]
    fn hostile_identifiers_cannot_select_a_parent_directory() {
        let root = Path::new("/trusted/artifacts");
        for value in ["../outside", "/outside", "a/b", "a\\b"] {
            let path = session_dir(root, value);
            assert_eq!(path.parent(), Some(root));
            assert!(
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("s1-")
            );
        }
    }

    #[test]
    fn first_write_migrates_a_safe_legacy_file_without_copying() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("history");
        std::fs::create_dir_all(&root).unwrap();
        let legacy = root.join("session-a.jsonl");
        std::fs::write(&legacy, b"turn\n").unwrap();

        let current = session_file_for_write(&root, "session-a", "jsonl").unwrap();

        assert_eq!(std::fs::read(&current).unwrap(), b"turn\n");
        assert!(!legacy.exists());
        assert_eq!(current, session_file(&root, "session-a", "jsonl"));
    }

    #[test]
    fn first_write_migrates_a_safe_legacy_directory_without_copying() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("artifacts");
        let legacy = root.join("session-a");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("payload.json"), b"{}").unwrap();

        let current = session_dir_for_write(&root, "session-a").unwrap();

        assert_eq!(std::fs::read(current.join("payload.json")).unwrap(), b"{}");
        assert!(!legacy.exists());
        assert_eq!(current, session_dir(&root, "session-a"));
    }

    #[test]
    fn malformed_legacy_names_are_never_used_or_removed() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("history");
        std::fs::create_dir_all(&root).unwrap();
        let outside = temp.path().join("outside.jsonl");
        std::fs::write(&outside, b"canary").unwrap();

        let resolved = session_file_for_read(&root, "../outside", "jsonl");
        remove_session_file(&root, "../outside", "jsonl").unwrap();

        assert_eq!(resolved, session_file(&root, "../outside", "jsonl"));
        assert_eq!(std::fs::read(outside).unwrap(), b"canary");
    }
}
