//! Typed vault paths and root capabilities.

use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use anyhow::{Context, Result, bail};
use once_cell::sync::Lazy;
use unicode_normalization::UnicodeNormalization;

use crate::store_root::{StoreRoot, StoreRootPath};
use crate::vault::roots::active_vault_root;

const MAX_VAULT_PATH_BYTES: usize = 1024;
const MAX_VAULT_PATH_SEGMENTS: usize = 64;

static VAULT_ROOT_CAPABILITIES: Lazy<RwLock<HashMap<PathBuf, Arc<StoreRoot>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VaultPath {
    segments: Vec<String>,
    serialized: String,
}

impl VaultPath {
    pub fn parse(raw: &str) -> Result<Self> {
        Self::parse_with_hidden(raw, false)
    }

    fn parse_with_hidden(raw: &str, allow_hidden: bool) -> Result<Self> {
        if raw.is_empty() {
            bail!("vault path is required");
        }
        if raw.len() > MAX_VAULT_PATH_BYTES {
            bail!("vault path is too long");
        }
        if raw.starts_with(['/', '\\']) {
            bail!("vault path must be relative");
        }
        if raw.contains('\\') {
            bail!("vault path must use forward slashes");
        }
        if raw.nfc().ne(raw.chars()) {
            bail!("vault path must use canonical Unicode normalization");
        }

        let raw_segments = raw.split('/').collect::<Vec<_>>();
        if raw_segments.len() > MAX_VAULT_PATH_SEGMENTS {
            bail!("vault path has too many segments");
        }
        for segment in &raw_segments {
            validate_vault_segment(segment, allow_hidden)?;
        }

        Ok(Self {
            segments: raw_segments
                .iter()
                .map(|segment| (*segment).to_string())
                .collect(),
            serialized: raw.to_string(),
        })
    }

    pub fn as_str(&self) -> &str {
        &self.serialized
    }

    pub(crate) fn internal(raw: &str) -> Result<Self> {
        Self::parse_with_hidden(raw, true)
    }

    pub(crate) fn join_segment(&self, segment: &str) -> Result<Self> {
        Self::parse(&format!("{}/{segment}", self.as_str()))
    }

    pub(crate) fn join_internal_segment(&self, segment: &str) -> Result<Self> {
        Self::parse_with_hidden(&format!("{}/{segment}", self.as_str()), true)
    }

    pub(crate) fn trash_root() -> Self {
        Self {
            segments: vec![".trash".to_string()],
            serialized: ".trash".to_string(),
        }
    }

    pub(crate) fn trash_path(&self) -> Self {
        let mut segments = Self::trash_root().segments;
        segments.reserve(self.segments.len());
        segments.extend(self.segments.iter().cloned());
        Self {
            serialized: segments.join("/"),
            segments,
        }
    }
}

impl fmt::Display for VaultPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl StoreRootPath for VaultPath {
    fn segments(&self) -> &[String] {
        &self.segments
    }
}

fn validate_vault_segment(segment: &str, allow_hidden: bool) -> Result<()> {
    if segment.is_empty() {
        bail!("vault path contains an empty segment");
    }
    if matches!(segment, "." | "..") {
        bail!("vault path contains a dot segment");
    }
    if segment.chars().any(char::is_control) {
        bail!("vault path contains a control character");
    }
    if !allow_hidden && segment.starts_with('.') {
        bail!("vault path contains a reserved hidden segment");
    }
    if segment.ends_with(['.', ' ']) {
        bail!("vault path contains a platform alias");
    }
    if segment.contains(['<', '>', ':', '"', '|', '?', '*']) {
        bail!("vault path contains a platform-reserved character");
    }

    let basename = segment.split_once('.').map_or(segment, |(base, _)| base);
    let upper = basename.to_ascii_uppercase();
    if matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || upper
            .strip_prefix("COM")
            .or_else(|| upper.strip_prefix("LPT"))
            .is_some_and(|suffix| suffix.len() == 1 && matches!(suffix.as_bytes()[0], b'1'..=b'9'))
    {
        bail!("vault path contains a reserved device name");
    }
    Ok(())
}

/// Active user vault root (multi-vault aware).
pub fn user_vault_root() -> PathBuf {
    active_vault_root()
}

/// Optional project overlay: `{root}/.medousa/vault/`.
pub fn project_vault_overlay_root() -> Option<PathBuf> {
    project_root().map(|root| root.join(".medousa").join("vault"))
}

pub fn user_vault_capability() -> Result<Arc<StoreRoot>> {
    vault_capability_for_root(user_vault_root())
}

pub fn project_vault_overlay_capability() -> Result<Option<Arc<StoreRoot>>> {
    let Some(root) = project_vault_overlay_root() else {
        return Ok(None);
    };
    if let Some(existing) = VAULT_ROOT_CAPABILITIES
        .read()
        .expect("vault root capabilities")
        .get(&root)
        .cloned()
    {
        return Ok(Some(existing));
    }
    let opened = match StoreRoot::open_nofollow(&root) {
        Ok(opened) => Arc::new(opened),
        Err(error) if error.is_not_found() => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| format!("open vault root {}", root.display()));
        }
    };
    let mut capabilities = VAULT_ROOT_CAPABILITIES
        .write()
        .expect("vault root capabilities");
    Ok(Some(capabilities.entry(root).or_insert(opened).clone()))
}

pub(crate) fn vault_capability_for_root(root: PathBuf) -> Result<Arc<StoreRoot>> {
    if let Some(existing) = VAULT_ROOT_CAPABILITIES
        .read()
        .expect("vault root capabilities")
        .get(&root)
        .cloned()
    {
        return Ok(existing);
    }

    let opened = Arc::new(
        StoreRoot::open_or_create_nofollow(&root)
            .with_context(|| format!("open vault root {}", root.display()))?,
    );
    let mut capabilities = VAULT_ROOT_CAPABILITIES
        .write()
        .expect("vault root capabilities");
    Ok(capabilities.entry(root).or_insert(opened).clone())
}

fn project_root() -> Option<PathBuf> {
    if let Ok(raw) = std::env::var("MEDOUSA_PROJECT_ROOT") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }

    std::env::current_dir().ok()
}

pub fn normalize_vault_path(raw: &str) -> Result<String> {
    Ok(VaultPath::parse(raw)?.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn normalize_rejects_traversal() {
        assert!(normalize_vault_path("../secret").is_err());
        assert!(normalize_vault_path("journal/2026-05-30.md").is_ok());
    }

    #[test]
    fn vault_path_rejects_aliases_and_preserves_unicode() {
        for hostile in [
            "/absolute.md",
            "../outside.md",
            "notes//empty.md",
            "notes/./dot.md",
            "notes/.hidden.md",
            "notes/NUL.txt",
            "notes/com1.log",
            "notes/trailing. ",
            "notes/stream:name.md",
            "notes\\windows.md",
        ] {
            assert!(VaultPath::parse(hostile).is_err(), "accepted {hostile:?}");
        }
        let unicode = VaultPath::parse("研究/Δοκιμή.md").expect("Unicode path");
        assert_eq!(unicode.as_str(), "研究/Δοκιμή.md");
    }

    #[cfg(unix)]
    #[test]
    fn vault_root_capability_rejects_link_ancestors() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().canonicalize().expect("canonical tempdir");
        let outside = root.join("outside");
        fs::create_dir(&outside).expect("outside");
        symlink(&outside, root.join("linked")).expect("symlink");

        let error = match StoreRoot::open_or_create_nofollow(&root.join("linked/vault")) {
            Ok(_) => panic!("link ancestor must fail"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("confinement"));
    }

    #[test]
    fn cached_vault_capability_survives_ambient_root_replacement() {
        let temp = tempfile::tempdir().expect("tempdir");
        let parent = temp.path().canonicalize().expect("canonical tempdir");
        let root = parent.join("vault");
        let held = parent.join("held-vault");
        let files = vault_capability_for_root(root.clone()).expect("open vault");
        std::fs::rename(&root, &held).expect("rename root");
        std::fs::create_dir(&root).expect("replacement root");

        let note = VaultPath::parse("journal/proof.md").expect("note path");
        files.atomic_write(&note, b"held").expect("write held root");

        assert_eq!(
            std::fs::read(held.join("journal/proof.md")).expect("read held"),
            b"held"
        );
        assert!(!root.join("journal/proof.md").exists());
    }
}
