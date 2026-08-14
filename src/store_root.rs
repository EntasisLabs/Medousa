//! Handle-relative filesystem authority for daemon-owned storage roots.
//!
//! Ambient paths are accepted only while opening a trusted root. Every later
//! operation walks validated relative segments from that open directory and
//! refuses symbolic-link/reparse traversal.

use std::fmt;
use std::io::{Read, Write};
use std::path::Path;

use cap_fs_ext::{DirExt as _, FollowSymlinks, OpenOptionsFollowExt as _};
use cap_std::ambient_authority;
use cap_std::fs::{Dir, File, OpenOptions};

const MAX_STORE_PATH_BYTES: usize = 1024;
const MAX_STORE_PATH_SEGMENTS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorePath {
    segments: Vec<String>,
}

impl StorePath {
    pub fn parse(value: &str) -> Result<Self, StoreRootError> {
        if value.is_empty() {
            return Err(StoreRootError::InvalidPath("empty"));
        }
        if value.len() > MAX_STORE_PATH_BYTES {
            return Err(StoreRootError::InvalidPath("too_long"));
        }
        if !value.is_ascii() {
            return Err(StoreRootError::InvalidPath("non_ascii"));
        }
        if value.starts_with('/') || value.starts_with('\\') {
            return Err(StoreRootError::InvalidPath("absolute"));
        }
        if value.contains('\\') {
            return Err(StoreRootError::InvalidPath("backslash"));
        }

        let segments = value.split('/').collect::<Vec<_>>();
        if segments.len() > MAX_STORE_PATH_SEGMENTS {
            return Err(StoreRootError::InvalidPath("too_deep"));
        }
        for segment in &segments {
            validate_segment(segment)?;
        }

        Ok(Self {
            segments: segments.into_iter().map(str::to_string).collect(),
        })
    }

    fn split_parent(&self) -> (&[String], &str) {
        let (leaf, parents) = self
            .segments
            .split_last()
            .expect("validated non-empty path");
        (parents, leaf)
    }
}

fn validate_segment(segment: &str) -> Result<(), StoreRootError> {
    if segment.is_empty() {
        return Err(StoreRootError::InvalidPath("empty_segment"));
    }
    if matches!(segment, "." | "..") {
        return Err(StoreRootError::InvalidPath("dot_segment"));
    }
    if segment.chars().any(char::is_control) {
        return Err(StoreRootError::InvalidPath("control_character"));
    }
    if segment.ends_with(['.', ' ']) {
        return Err(StoreRootError::InvalidPath("platform_alias"));
    }
    if segment.contains(['<', '>', ':', '"', '|', '?', '*']) {
        return Err(StoreRootError::InvalidPath("platform_character"));
    }

    let basename = segment.split_once('.').map_or(segment, |(base, _)| base);
    let upper = basename.to_ascii_uppercase();
    if matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || upper
            .strip_prefix("COM")
            .or_else(|| upper.strip_prefix("LPT"))
            .is_some_and(|suffix| suffix.len() == 1 && matches!(suffix.as_bytes()[0], b'1'..=b'9'))
    {
        return Err(StoreRootError::InvalidPath("platform_alias"));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfinementReason {
    SymbolicLink,
}

#[derive(Debug)]
pub enum StoreRootError {
    InvalidPath(&'static str),
    Confinement {
        operation: &'static str,
        reason: ConfinementReason,
    },
    Io {
        operation: &'static str,
        source: std::io::Error,
    },
}

impl StoreRootError {
    fn io(operation: &'static str, source: std::io::Error) -> Self {
        Self::Io { operation, source }
    }
}

impl fmt::Display for StoreRootError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPath(reason) => write!(formatter, "invalid store path: {reason}"),
            Self::Confinement { operation, reason } => {
                write!(
                    formatter,
                    "store confinement rejected {operation}: {reason:?}"
                )
            }
            Self::Io { operation, source } => {
                write!(formatter, "store {operation} failed: {source}")
            }
        }
    }
}

impl std::error::Error for StoreRootError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::InvalidPath(_) | Self::Confinement { .. } => None,
        }
    }
}

pub struct StoreRoot {
    dir: Dir,
}

impl StoreRoot {
    /// Open an existing trusted root using ambient authority exactly once.
    pub fn open(path: &Path) -> Result<Self, StoreRootError> {
        let dir = Dir::open_ambient_dir(path, ambient_authority())
            .map_err(|error| StoreRootError::io("open_root", error))?;
        Ok(Self { dir })
    }

    pub fn read(&self, path: &StorePath) -> Result<Vec<u8>, StoreRootError> {
        let mut file = self.open_file(path, false, false)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|error| StoreRootError::io("read", error))?;
        Ok(bytes)
    }

    pub fn append(&self, path: &StorePath, bytes: &[u8]) -> Result<(), StoreRootError> {
        let mut file = self.open_file(path, true, true)?;
        file.write_all(bytes)
            .map_err(|error| StoreRootError::io("append", error))
    }

    pub fn atomic_write(&self, path: &StorePath, bytes: &[u8]) -> Result<(), StoreRootError> {
        let (parent, leaf) = self.open_parent(path, true, "atomic_write")?;
        reject_symlink(&parent, leaf, "atomic_write")?;

        let temporary = format!(".medousa-tmp-{}", uuid::Uuid::new_v4().simple());
        let mut options = OpenOptions::new();
        options
            .write(true)
            .create_new(true)
            .follow(FollowSymlinks::No);
        let mut file = parent
            .open_with(&temporary, &options)
            .map_err(|error| StoreRootError::io("create_temporary", error))?;
        if let Err(error) = file.write_all(bytes).and_then(|()| file.sync_data()) {
            let _ = parent.remove_file(&temporary);
            return Err(StoreRootError::io("write_temporary", error));
        }
        drop(file);
        if let Err(error) = parent.rename(&temporary, &parent, leaf) {
            let _ = parent.remove_file(&temporary);
            return Err(StoreRootError::io("publish_atomic", error));
        }
        Ok(())
    }

    pub fn create_dir_all(&self, path: &StorePath) -> Result<(), StoreRootError> {
        let _ = self.open_directory_chain(&path.segments, true, "create_dir_all")?;
        Ok(())
    }

    pub fn remove_file(&self, path: &StorePath) -> Result<(), StoreRootError> {
        let (parent, leaf) = self.open_parent(path, false, "remove_file")?;
        match parent.symlink_metadata(leaf) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(StoreRootError::Confinement {
                    operation: "remove_file",
                    reason: ConfinementReason::SymbolicLink,
                });
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(StoreRootError::io("remove_file_metadata", error)),
        }
        parent
            .remove_file(leaf)
            .map_err(|error| StoreRootError::io("remove_file", error))
    }

    pub fn remove_dir_all(&self, path: &StorePath) -> Result<(), StoreRootError> {
        let (parent, leaf) = self.open_parent(path, false, "remove_dir_all")?;
        match parent.symlink_metadata(leaf) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(StoreRootError::Confinement {
                    operation: "remove_dir_all",
                    reason: ConfinementReason::SymbolicLink,
                });
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(StoreRootError::io("remove_dir_metadata", error)),
        }
        let child = parent
            .open_dir_nofollow(leaf)
            .map_err(|error| StoreRootError::io("open_remove_dir", error))?;
        child
            .remove_open_dir_all()
            .map_err(|error| StoreRootError::io("remove_dir_all", error))
    }

    pub fn rename(&self, from: &StorePath, to: &StorePath) -> Result<(), StoreRootError> {
        let (from_parent, from_leaf) = self.open_parent(from, false, "rename_source")?;
        reject_symlink(&from_parent, from_leaf, "rename_source")?;
        let (to_parent, to_leaf) = self.open_parent(to, true, "rename_destination")?;
        reject_symlink(&to_parent, to_leaf, "rename_destination")?;
        from_parent
            .rename(from_leaf, &to_parent, to_leaf)
            .map_err(|error| StoreRootError::io("rename", error))
    }

    fn open_file(
        &self,
        path: &StorePath,
        append: bool,
        create: bool,
    ) -> Result<File, StoreRootError> {
        let (parent, leaf) = self.open_parent(path, create, "open_file")?;
        reject_symlink(&parent, leaf, "open_file")?;
        let mut options = OpenOptions::new();
        options
            .read(!append)
            .write(append)
            .append(append)
            .create(create)
            .follow(FollowSymlinks::No);
        parent
            .open_with(leaf, &options)
            .map_err(|error| StoreRootError::io("open_file", error))
    }

    fn open_parent<'path>(
        &self,
        path: &'path StorePath,
        create: bool,
        operation: &'static str,
    ) -> Result<(Dir, &'path str), StoreRootError> {
        let (parents, leaf) = path.split_parent();
        let parent = self.open_directory_chain(parents, create, operation)?;
        Ok((parent, leaf))
    }

    fn open_directory_chain(
        &self,
        segments: &[String],
        create: bool,
        operation: &'static str,
    ) -> Result<Dir, StoreRootError> {
        let mut current = self
            .dir
            .try_clone()
            .map_err(|error| StoreRootError::io("clone_root", error))?;
        for segment in segments {
            if create {
                match current.create_dir(segment) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(StoreRootError::io("create_directory", error)),
                }
            }
            reject_symlink(&current, segment, operation)?;
            current = current
                .open_dir_nofollow(segment)
                .map_err(|error| StoreRootError::io(operation, error))?;
        }
        Ok(current)
    }
}

fn reject_symlink(parent: &Dir, name: &str, operation: &'static str) -> Result<(), StoreRootError> {
    match parent.symlink_metadata(name) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(StoreRootError::Confinement {
            operation,
            reason: ConfinementReason::SymbolicLink,
        }),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(StoreRootError::io(operation, error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(value: &str) -> StorePath {
        StorePath::parse(value).unwrap()
    }

    #[test]
    fn store_path_rejects_cross_platform_aliases() {
        for value in [
            "",
            ".",
            "..",
            "../outside",
            "/absolute",
            "\\absolute",
            "a\\b",
            "a//b",
            "CON",
            "aux.txt",
            "LPT1.log",
            "name.",
            "name ",
            "stream:name",
            "line\nfeed",
            "café",
        ] {
            assert!(StorePath::parse(value).is_err(), "accepted {value:?}");
        }
    }

    #[test]
    fn handle_remains_authoritative_after_root_path_replacement() {
        let temp = tempfile::tempdir().unwrap();
        let root_path = temp.path().join("root");
        let held_path = temp.path().join("held-root");
        std::fs::create_dir(&root_path).unwrap();
        let root = StoreRoot::open(&root_path).unwrap();

        std::fs::rename(&root_path, &held_path).unwrap();
        std::fs::create_dir(&root_path).unwrap();
        root.atomic_write(&path("proof.txt"), b"held").unwrap();

        assert_eq!(std::fs::read(held_path.join("proof.txt")).unwrap(), b"held");
        assert!(!root_path.join("proof.txt").exists());
    }

    #[test]
    fn nested_create_write_read_rename_and_delete_stay_under_root() {
        let temp = tempfile::tempdir().unwrap();
        let root = StoreRoot::open(temp.path()).unwrap();
        root.create_dir_all(&path("one/two")).unwrap();
        root.atomic_write(&path("one/two/value.txt"), b"stale")
            .unwrap();
        root.atomic_write(&path("one/two/value.txt"), b"first")
            .unwrap();
        root.append(&path("one/two/value.txt"), b" second").unwrap();
        assert_eq!(
            root.read(&path("one/two/value.txt")).unwrap(),
            b"first second"
        );

        root.rename(&path("one/two/value.txt"), &path("one/two/renamed.txt"))
            .unwrap();
        root.remove_file(&path("one/two/renamed.txt")).unwrap();
        root.remove_dir_all(&path("one")).unwrap();
        assert!(!temp.path().join("one").exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_leaf_and_ancestor_cannot_escape_reads_or_writes() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let root_path = temp.path().join("root");
        let outside = temp.path().join("outside");
        std::fs::create_dir(&root_path).unwrap();
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("canary.txt"), b"outside").unwrap();
        symlink(outside.join("canary.txt"), root_path.join("leaf.txt")).unwrap();
        symlink(&outside, root_path.join("ancestor")).unwrap();
        let root = StoreRoot::open(&root_path).unwrap();

        assert!(root.read(&path("leaf.txt")).is_err());
        assert!(root.atomic_write(&path("leaf.txt"), b"changed").is_err());
        assert!(root.read(&path("ancestor/canary.txt")).is_err());
        assert!(root
            .atomic_write(&path("ancestor/new.txt"), b"changed")
            .is_err());
        assert_eq!(
            std::fs::read(outside.join("canary.txt")).unwrap(),
            b"outside"
        );
        assert!(!outside.join("new.txt").exists());
    }

    #[cfg(unix)]
    #[test]
    fn recursive_delete_refuses_a_symlinked_directory() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let root_path = temp.path().join("root");
        let outside = temp.path().join("outside");
        std::fs::create_dir(&root_path).unwrap();
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("canary"), b"safe").unwrap();
        symlink(&outside, root_path.join("victim")).unwrap();
        let root = StoreRoot::open(&root_path).unwrap();

        assert!(root.remove_dir_all(&path("victim")).is_err());
        assert_eq!(std::fs::read(outside.join("canary")).unwrap(), b"safe");
    }
}
