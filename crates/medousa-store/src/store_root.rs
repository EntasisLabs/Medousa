//! Handle-relative filesystem authority for daemon-owned storage roots.
//!
//! Ambient paths are accepted only while opening a trusted root. Every later
//! operation walks validated relative segments from that open directory and
//! refuses symbolic-link/reparse traversal.

use std::fmt;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

use cap_fs_ext::{DirExt as _, FollowSymlinks, MetadataExt as _, OpenOptionsFollowExt as _};
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

    pub fn file_name(&self) -> &str {
        self.segments
            .last()
            .expect("validated non-empty store path")
    }

    pub fn join(&self, child: &Self) -> Result<Self, StoreRootError> {
        let segment_count = self.segments.len() + child.segments.len();
        if segment_count > MAX_STORE_PATH_SEGMENTS {
            return Err(StoreRootError::InvalidPath("too_deep"));
        }
        let byte_count = self
            .segments
            .iter()
            .chain(&child.segments)
            .map(String::len)
            .sum::<usize>()
            + segment_count.saturating_sub(1);
        if byte_count > MAX_STORE_PATH_BYTES {
            return Err(StoreRootError::InvalidPath("too_long"));
        }
        let mut segments = Vec::with_capacity(self.segments.len() + child.segments.len());
        segments.extend(self.segments.iter().cloned());
        segments.extend(child.segments.iter().cloned());
        Ok(Self { segments })
    }
}

impl fmt::Display for StorePath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, segment) in self.segments.iter().enumerate() {
            if index > 0 {
                formatter.write_str("/")?;
            }
            formatter.write_str(segment)?;
        }
        Ok(())
    }
}

/// A validated relative path accepted by [`StoreRoot`].
///
/// Daemon-owned stores use [`StorePath`]. User-facing stores can provide a
/// domain type with a broader character grammar while retaining the same
/// handle-relative, no-follow filesystem operations.
pub trait StoreRootPath {
    fn segments(&self) -> &[String];

    fn split_parent(&self) -> (&[String], &str) {
        let (leaf, parents) = self
            .segments()
            .split_last()
            .expect("validated non-empty path");
        (parents, leaf)
    }
}

impl StoreRootPath for StorePath {
    fn segments(&self) -> &[String] {
        &self.segments
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
    HardLink,
    RootIdentity,
}

#[derive(Debug)]
pub enum StoreRootError {
    InvalidPath(&'static str),
    Limit {
        operation: &'static str,
        max_bytes: u64,
    },
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

    pub fn is_not_found(&self) -> bool {
        matches!(
            self,
            Self::Io { source, .. } if source.kind() == std::io::ErrorKind::NotFound
        )
    }
}

impl fmt::Display for StoreRootError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPath(reason) => write!(formatter, "invalid store path: {reason}"),
            Self::Limit {
                operation,
                max_bytes,
            } => write!(
                formatter,
                "store {operation} exceeded the {max_bytes}-byte limit"
            ),
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
            Self::InvalidPath(_) | Self::Limit { .. } | Self::Confinement { .. } => None,
        }
    }
}

pub struct StoreRoot {
    dir: Dir,
    /// Windows capability operations are implemented with path-relative APIs.
    /// Keeping every opened ancestor alive without `FILE_SHARE_DELETE` pins
    /// the complete root spelling against rename/delete replacement.
    #[cfg(windows)]
    _ancestor_guards: Vec<Dir>,
    #[cfg(windows)]
    process_path_pinned: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreEntryKind {
    File,
    Directory,
    Link,
    Other,
}

#[derive(Debug)]
pub struct StoreEntry {
    pub path: StorePath,
    pub kind: StoreEntryKind,
    pub size: u64,
    pub modified: Option<SystemTime>,
}

#[derive(Debug)]
pub struct StoreDirectoryEntry {
    pub name: String,
    pub kind: StoreEntryKind,
    pub size: u64,
    pub created: Option<SystemTime>,
    pub modified: Option<SystemTime>,
}

#[derive(Debug, Clone, Copy)]
pub struct StoreMetadata {
    pub kind: StoreEntryKind,
    pub size: u64,
    pub created: Option<SystemTime>,
    pub modified: Option<SystemTime>,
}

impl StoreRoot {
    /// Open an existing trusted root using ambient authority exactly once.
    pub fn open(path: &Path) -> Result<Self, StoreRootError> {
        let dir = Dir::open_ambient_dir(path, ambient_authority())
            .map_err(|error| StoreRootError::io("open_root", error))?;
        Ok(Self {
            dir,
            #[cfg(windows)]
            _ancestor_guards: Vec::new(),
            #[cfg(windows)]
            process_path_pinned: false,
        })
    }

    /// Create a trusted root if needed, then open and retain its authority.
    pub fn open_or_create(path: &Path) -> Result<Self, StoreRootError> {
        std::fs::create_dir_all(path).map_err(|error| StoreRootError::io("create_root", error))?;
        Self::open(path)
    }

    /// Open or create an absolute trusted root without following links in any
    /// existing path component.
    pub fn open_or_create_nofollow(path: &Path) -> Result<Self, StoreRootError> {
        Self::open_absolute_nofollow(path, true)
    }

    /// Open an existing absolute trusted root without following links in any
    /// path component.
    pub fn open_nofollow(path: &Path) -> Result<Self, StoreRootError> {
        Self::open_absolute_nofollow(path, false)
    }

    fn open_absolute_nofollow(path: &Path, create: bool) -> Result<Self, StoreRootError> {
        let (anchor, segments) = absolute_path_parts(path)?;
        let mut current = Dir::open_ambient_dir(anchor, ambient_authority())
            .map_err(|error| StoreRootError::io("open_root_anchor", error))?;
        #[cfg(windows)]
        let mut ancestor_guards = Vec::with_capacity(segments.len());
        for segment in segments {
            if create {
                match current.create_dir(&segment) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(error) => {
                        return Err(StoreRootError::io("create_root_component", error));
                    }
                }
            }
            reject_symlink(&current, &segment, "open_root_component")?;
            let next = current
                .open_dir_nofollow(&segment)
                .map_err(|error| StoreRootError::io("open_root_component", error))?;
            #[cfg(windows)]
            ancestor_guards.push(current);
            current = next;
        }
        Ok(Self {
            dir: current,
            #[cfg(windows)]
            _ancestor_guards: ancestor_guards,
            #[cfg(windows)]
            process_path_pinned: true,
        })
    }

    /// Make a Unix child start in this exact opened directory rather than
    /// reopening its ambient pathname. The descriptor is still live between
    /// `fork` and `exec`, even though it is close-on-exec.
    #[cfg(unix)]
    pub fn configure_command_current_dir(
        &self,
        command: &mut Command,
        _ambient_root: &Path,
    ) -> Result<(), StoreRootError> {
        use std::os::fd::AsRawFd as _;
        use std::os::unix::process::CommandExt as _;

        let root_fd = self.dir.as_raw_fd();
        // SAFETY: the closure performs only the async-signal-safe `fchdir`
        // syscall and constructs the error from errno. `root_fd` remains owned
        // by `self`, which the caller must keep alive through `Command::output`.
        unsafe {
            command.pre_exec(move || {
                if libc::fchdir(root_fd) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        Ok(())
    }

    /// Windows has no `fchdir`. The no-follow root retains non-delete-sharing
    /// handles for every component, preventing the ambient spelling from being
    /// renamed or replaced. Reopen it once more and compare filesystem identity
    /// before giving the string path to `CreateProcessW`.
    #[cfg(windows)]
    pub fn configure_command_current_dir(
        &self,
        command: &mut Command,
        ambient_root: &Path,
    ) -> Result<(), StoreRootError> {
        if !self.process_path_pinned {
            return Err(StoreRootError::Confinement {
                operation: "process_root_identity",
                reason: ConfinementReason::RootIdentity,
            });
        }
        let reopened = Self::open_nofollow(ambient_root)?;
        let held = self
            .dir
            .dir_metadata()
            .map_err(|error| StoreRootError::io("process_root_identity", error))?;
        let current = reopened
            .dir
            .dir_metadata()
            .map_err(|error| StoreRootError::io("process_root_identity", error))?;
        if held.dev() != current.dev() || held.ino() != current.ino() {
            return Err(StoreRootError::Confinement {
                operation: "process_root_identity",
                reason: ConfinementReason::RootIdentity,
            });
        }
        command.current_dir(ambient_root);
        Ok(())
    }

    #[cfg(not(any(unix, windows)))]
    pub fn configure_command_current_dir(
        &self,
        command: &mut Command,
        ambient_root: &Path,
    ) -> Result<(), StoreRootError> {
        command.current_dir(ambient_root);
        Ok(())
    }

    pub fn read(&self, path: &impl StoreRootPath) -> Result<Vec<u8>, StoreRootError> {
        let mut file = self.open_file(path, false, false)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|error| StoreRootError::io("read", error))?;
        Ok(bytes)
    }

    pub fn read_limited(
        &self,
        path: &impl StoreRootPath,
        max_bytes: u64,
    ) -> Result<Vec<u8>, StoreRootError> {
        let file = self.open_file(path, false, false)?;
        let mut bytes = Vec::with_capacity(max_bytes.min(8 * 1024) as usize);
        file.take(max_bytes.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|error| StoreRootError::io("read_limited", error))?;
        if bytes.len() as u64 > max_bytes {
            return Err(StoreRootError::Limit {
                operation: "read_limited",
                max_bytes,
            });
        }
        Ok(bytes)
    }

    pub fn is_file(&self, path: &impl StoreRootPath) -> Result<bool, StoreRootError> {
        let (parent, leaf) = match self.open_parent(path, false, "metadata") {
            Ok(value) => value,
            Err(error) if error.is_not_found() => return Ok(false),
            Err(error) => return Err(error),
        };
        match parent.symlink_metadata(leaf) {
            Ok(metadata) if metadata.file_type().is_symlink() => Err(StoreRootError::Confinement {
                operation: "metadata",
                reason: ConfinementReason::SymbolicLink,
            }),
            Ok(metadata) if file_has_multiple_links(&metadata) => {
                Err(StoreRootError::Confinement {
                    operation: "metadata",
                    reason: ConfinementReason::HardLink,
                })
            }
            Ok(metadata) => Ok(metadata.is_file()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(StoreRootError::io("metadata", error)),
        }
    }

    pub fn is_dir(&self, path: &impl StoreRootPath) -> Result<bool, StoreRootError> {
        let (parent, leaf) = match self.open_parent(path, false, "metadata") {
            Ok(value) => value,
            Err(error) if error.is_not_found() => return Ok(false),
            Err(error) => return Err(error),
        };
        match parent.symlink_metadata(leaf) {
            Ok(metadata) if metadata.file_type().is_symlink() => Err(StoreRootError::Confinement {
                operation: "metadata",
                reason: ConfinementReason::SymbolicLink,
            }),
            Ok(metadata) => Ok(metadata.is_dir()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(StoreRootError::io("metadata", error)),
        }
    }

    pub fn metadata(&self, path: &impl StoreRootPath) -> Result<StoreMetadata, StoreRootError> {
        let (parent, leaf) = self.open_parent(path, false, "metadata")?;
        let metadata = parent
            .symlink_metadata(leaf)
            .map_err(|error| StoreRootError::io("metadata", error))?;
        if metadata.file_type().is_symlink() {
            return Err(StoreRootError::Confinement {
                operation: "metadata",
                reason: ConfinementReason::SymbolicLink,
            });
        }
        if file_has_multiple_links(&metadata) {
            return Err(StoreRootError::Confinement {
                operation: "metadata",
                reason: ConfinementReason::HardLink,
            });
        }
        Ok(store_metadata(&metadata))
    }

    /// Enumerate validated direct children from the held root handle.
    ///
    /// Non-UTF-8 and policy-invalid names are ignored; callers never receive a
    /// path they could feed back into the capability without validation.
    pub fn list_root(&self) -> Result<Vec<StoreEntry>, StoreRootError> {
        list_directory(&self.dir)
    }

    pub fn list_directory(
        &self,
        path: &impl StoreRootPath,
    ) -> Result<Vec<StoreEntry>, StoreRootError> {
        let directory = self.open_directory_chain(path.segments(), false, "list_directory")?;
        list_directory(&directory)
    }

    pub fn list_root_utf8(&self) -> Result<Vec<StoreDirectoryEntry>, StoreRootError> {
        list_directory_utf8(&self.dir)
    }

    pub fn list_directory_utf8(
        &self,
        path: &impl StoreRootPath,
    ) -> Result<Vec<StoreDirectoryEntry>, StoreRootError> {
        let directory = self.open_directory_chain(path.segments(), false, "list_directory")?;
        list_directory_utf8(&directory)
    }

    pub fn append(&self, path: &impl StoreRootPath, bytes: &[u8]) -> Result<(), StoreRootError> {
        self.append_durable(path, bytes, false)
    }

    /// Append bytes and optionally cross a data durability fence before returning.
    pub fn append_durable(
        &self,
        path: &impl StoreRootPath,
        bytes: &[u8],
        sync: bool,
    ) -> Result<(), StoreRootError> {
        let mut file = self.open_file(path, true, true)?;
        file.write_all(bytes)
            .map_err(|error| StoreRootError::io("append", error))?;
        if sync {
            file.sync_data()
                .map_err(|error| StoreRootError::io("sync_append", error))?;
        }
        Ok(())
    }

    pub fn atomic_write(
        &self,
        path: &impl StoreRootPath,
        bytes: &[u8],
    ) -> Result<(), StoreRootError> {
        self.atomic_publish(path, bytes, false, "atomic_write")
    }

    /// Create-only atomic publication. Fails if the destination leaf exists.
    pub fn atomic_create(
        &self,
        path: &impl StoreRootPath,
        bytes: &[u8],
    ) -> Result<(), StoreRootError> {
        self.atomic_publish(path, bytes, true, "atomic_create")
    }

    /// Sync the parent directory of `path` when the platform supports it.
    ///
    /// On Windows this is currently a documented no-op at the directory level;
    /// the published file itself is still data-synced before rename.
    pub fn sync_parent_of(&self, path: &impl StoreRootPath) -> Result<(), StoreRootError> {
        let (parent, _leaf) = self.open_parent(path, false, "sync_parent")?;
        sync_directory(&parent).map_err(|error| StoreRootError::io("sync_parent", error))
    }

    fn atomic_publish(
        &self,
        path: &impl StoreRootPath,
        bytes: &[u8],
        create_only: bool,
        operation: &'static str,
    ) -> Result<(), StoreRootError> {
        let (parent, leaf) = self.open_parent(path, true, operation)?;
        reject_symlink(&parent, leaf, operation)?;
        if create_only && parent.symlink_metadata(leaf).is_ok() {
            return Err(StoreRootError::io(
                operation,
                std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    "destination already exists",
                ),
            ));
        }

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
        if create_only && parent.symlink_metadata(leaf).is_ok() {
            let _ = parent.remove_file(&temporary);
            return Err(StoreRootError::io(
                operation,
                std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    "destination already exists",
                ),
            ));
        }
        if let Err(error) = parent.rename(&temporary, &parent, leaf) {
            let _ = parent.remove_file(&temporary);
            return Err(StoreRootError::io("publish_atomic", error));
        }
        sync_directory(&parent).map_err(|error| StoreRootError::io("sync_parent", error))?;
        Ok(())
    }

    /// Copy one regular file between held roots through fixed-size buffering,
    /// then atomically publish it at the destination.
    pub fn atomic_copy_from(
        &self,
        destination: &impl StoreRootPath,
        source_root: &Self,
        source: &impl StoreRootPath,
        max_bytes: u64,
    ) -> Result<u64, StoreRootError> {
        let input = source_root.open_file(source, false, false)?;
        let source_size = input
            .metadata()
            .map_err(|error| StoreRootError::io("copy_source_metadata", error))?
            .len();
        if source_size > max_bytes {
            return Err(StoreRootError::Limit {
                operation: "atomic_copy_from",
                max_bytes,
            });
        }

        let (parent, leaf) = self.open_parent(destination, true, "atomic_copy_from")?;
        reject_symlink(&parent, leaf, "atomic_copy_from")?;
        let temporary = format!(".medousa-tmp-{}", uuid::Uuid::new_v4().simple());
        let mut options = OpenOptions::new();
        options
            .write(true)
            .create_new(true)
            .follow(FollowSymlinks::No);
        let mut output = parent
            .open_with(&temporary, &options)
            .map_err(|error| StoreRootError::io("create_copy_temporary", error))?;

        let copied = match std::io::copy(&mut input.take(max_bytes.saturating_add(1)), &mut output)
        {
            Ok(copied) if copied <= max_bytes => copied,
            Ok(_) => {
                let _ = parent.remove_file(&temporary);
                return Err(StoreRootError::Limit {
                    operation: "atomic_copy_from",
                    max_bytes,
                });
            }
            Err(error) => {
                let _ = parent.remove_file(&temporary);
                return Err(StoreRootError::io("copy_temporary", error));
            }
        };
        if let Err(error) = output.sync_data() {
            let _ = parent.remove_file(&temporary);
            return Err(StoreRootError::io("sync_copy_temporary", error));
        }
        drop(output);
        if let Err(error) = parent.rename(&temporary, &parent, leaf) {
            let _ = parent.remove_file(&temporary);
            return Err(StoreRootError::io("publish_atomic_copy", error));
        }
        Ok(copied)
    }

    pub fn create_dir_all(&self, path: &impl StoreRootPath) -> Result<(), StoreRootError> {
        let _ = self.open_directory_chain(path.segments(), true, "create_dir_all")?;
        Ok(())
    }

    pub fn remove_file(&self, path: &impl StoreRootPath) -> Result<(), StoreRootError> {
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

    pub fn remove_dir_all(&self, path: &impl StoreRootPath) -> Result<(), StoreRootError> {
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

    pub fn rename(
        &self,
        from: &impl StoreRootPath,
        to: &impl StoreRootPath,
    ) -> Result<(), StoreRootError> {
        let (from_parent, from_leaf) = self.open_parent(from, false, "rename_source")?;
        reject_symlink(&from_parent, from_leaf, "rename_source")?;
        reject_hard_link(&from_parent, from_leaf, "rename_source")?;
        let (to_parent, to_leaf) = self.open_parent(to, true, "rename_destination")?;
        reject_symlink(&to_parent, to_leaf, "rename_destination")?;
        from_parent
            .rename(from_leaf, &to_parent, to_leaf)
            .map_err(|error| StoreRootError::io("rename", error))
    }

    fn open_file(
        &self,
        path: &impl StoreRootPath,
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
        let file = parent
            .open_with(leaf, &options)
            .map_err(|error| StoreRootError::io("open_file", error))?;
        let metadata = file
            .metadata()
            .map_err(|error| StoreRootError::io("open_file_metadata", error))?;
        if file_has_multiple_links(&metadata) {
            return Err(StoreRootError::Confinement {
                operation: "open_file",
                reason: ConfinementReason::HardLink,
            });
        }
        Ok(file)
    }

    fn open_parent<'path, P: StoreRootPath + ?Sized>(
        &self,
        path: &'path P,
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

fn absolute_path_parts(path: &Path) -> Result<(PathBuf, Vec<String>), StoreRootError> {
    if !path.is_absolute() {
        return Err(StoreRootError::InvalidPath("root_not_absolute"));
    }

    let mut anchor = PathBuf::new();
    let mut segments = Vec::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => anchor.push(prefix.as_os_str()),
            Component::RootDir => anchor.push(component.as_os_str()),
            Component::Normal(segment) => {
                let segment = segment
                    .to_str()
                    .ok_or(StoreRootError::InvalidPath("root_non_utf8"))?;
                segments.push(segment.to_string());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(StoreRootError::InvalidPath("root_parent_segment"));
            }
        }
    }
    if anchor.as_os_str().is_empty() {
        return Err(StoreRootError::InvalidPath("root_missing_anchor"));
    }
    Ok((anchor, segments))
}

fn list_directory(dir: &Dir) -> Result<Vec<StoreEntry>, StoreRootError> {
    Ok(list_directory_utf8(dir)?
        .into_iter()
        .filter_map(|entry| {
            let path = StorePath::parse(&entry.name).ok()?;
            Some(StoreEntry {
                path,
                kind: entry.kind,
                size: entry.size,
                modified: entry.modified,
            })
        })
        .collect())
}

fn list_directory_utf8(dir: &Dir) -> Result<Vec<StoreDirectoryEntry>, StoreRootError> {
    let entries = dir
        .entries()
        .map_err(|error| StoreRootError::io("list_directory", error))?;
    let mut listed = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| StoreRootError::io("list_entry", error))?;
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        let file_type = entry
            .file_type()
            .map_err(|error| StoreRootError::io("entry_type", error))?;
        let metadata = dir.symlink_metadata(&name).ok();
        let kind = if file_type.is_symlink() {
            StoreEntryKind::Link
        } else if metadata.as_ref().is_some_and(file_has_multiple_links) {
            StoreEntryKind::Other
        } else if file_type.is_file() {
            StoreEntryKind::File
        } else if file_type.is_dir() {
            StoreEntryKind::Directory
        } else {
            StoreEntryKind::Other
        };
        let metadata = (kind != StoreEntryKind::Other)
            .then_some(metadata)
            .flatten();
        let size = metadata.as_ref().map_or(0, |metadata| metadata.len());
        let created = metadata
            .as_ref()
            .and_then(|metadata| metadata.created().ok())
            .map(|created| created.into_std());
        let modified = metadata
            .and_then(|metadata| metadata.modified().ok())
            .map(|modified| modified.into_std());
        listed.push(StoreDirectoryEntry {
            name,
            kind,
            size,
            created,
            modified,
        });
    }
    Ok(listed)
}

fn store_metadata(metadata: &cap_std::fs::Metadata) -> StoreMetadata {
    let file_type = metadata.file_type();
    let kind = if file_type.is_symlink() {
        StoreEntryKind::Link
    } else if file_type.is_file() {
        StoreEntryKind::File
    } else if file_type.is_dir() {
        StoreEntryKind::Directory
    } else {
        StoreEntryKind::Other
    };
    StoreMetadata {
        kind,
        size: metadata.len(),
        created: metadata.created().ok().map(|created| created.into_std()),
        modified: metadata.modified().ok().map(|modified| modified.into_std()),
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

fn reject_hard_link(
    parent: &Dir,
    name: &str,
    operation: &'static str,
) -> Result<(), StoreRootError> {
    match parent.symlink_metadata(name) {
        Ok(metadata) if file_has_multiple_links(&metadata) => Err(StoreRootError::Confinement {
            operation,
            reason: ConfinementReason::HardLink,
        }),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(StoreRootError::io(operation, error)),
    }
}

#[cfg(unix)]
fn sync_directory(directory: &Dir) -> std::io::Result<()> {
    directory.open(".")?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_directory: &Dir) -> std::io::Result<()> {
    // Rust does not expose the directory handle flags needed by
    // FlushFileBuffers on Windows. The temporary file itself is synced before
    // publication; supported Unix targets additionally fence the directory
    // entry here.
    Ok(())
}

fn file_has_multiple_links(metadata: &cap_std::fs::Metadata) -> bool {
    metadata.is_file() && metadata.nlink() > 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    fn create_junction(target: &Path, junction: &Path) {
        use std::os::windows::ffi::OsStrExt as _;
        use windows::Win32::Foundation::{CloseHandle, GENERIC_WRITE};
        use windows::Win32::Storage::FileSystem::{
            CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_MODE,
            OPEN_EXISTING,
        };
        use windows::Win32::System::IO::DeviceIoControl;
        use windows::Win32::System::Ioctl::FSCTL_SET_REPARSE_POINT;
        use windows::Win32::System::SystemServices::IO_REPARSE_TAG_MOUNT_POINT;
        use windows::core::PCWSTR;

        let target = target.canonicalize().expect("canonical junction target");
        std::fs::create_dir(junction).expect("create junction placeholder");

        let target_wide = target.as_os_str().encode_wide().collect::<Vec<_>>();
        let target_wide = target_wide
            .strip_prefix(&[b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16])
            .unwrap_or(&target_wide);
        let mut substitute = r"\??\".encode_utf16().collect::<Vec<_>>();
        substitute.extend_from_slice(target_wide);
        let print = target_wide.to_vec();
        let substitute_bytes = u16::try_from(substitute.len() * 2).expect("junction target length");
        let print_bytes = u16::try_from(print.len() * 2).expect("junction print length");
        let print_offset = substitute_bytes.checked_add(2).expect("junction offset");
        let reparse_data_length = 8_u16
            .checked_add(print_offset)
            .and_then(|length| length.checked_add(print_bytes))
            .and_then(|length| length.checked_add(2))
            .expect("junction reparse data length");

        let mut reparse = Vec::with_capacity(8 + usize::from(reparse_data_length));
        reparse.extend_from_slice(&IO_REPARSE_TAG_MOUNT_POINT.to_le_bytes());
        reparse.extend_from_slice(&reparse_data_length.to_le_bytes());
        reparse.extend_from_slice(&0_u16.to_le_bytes());
        reparse.extend_from_slice(&0_u16.to_le_bytes());
        reparse.extend_from_slice(&substitute_bytes.to_le_bytes());
        reparse.extend_from_slice(&print_offset.to_le_bytes());
        reparse.extend_from_slice(&print_bytes.to_le_bytes());
        for unit in substitute {
            reparse.extend_from_slice(&unit.to_le_bytes());
        }
        reparse.extend_from_slice(&0_u16.to_le_bytes());
        for unit in print {
            reparse.extend_from_slice(&unit.to_le_bytes());
        }
        reparse.extend_from_slice(&0_u16.to_le_bytes());
        assert_eq!(reparse.len(), 8 + usize::from(reparse_data_length));

        let mut junction_wide = junction.as_os_str().encode_wide().collect::<Vec<_>>();
        junction_wide.push(0);
        // SAFETY: the path is NUL-terminated and remains live for the call.
        let handle = unsafe {
            CreateFileW(
                PCWSTR::from_raw(junction_wide.as_ptr()),
                GENERIC_WRITE.0,
                FILE_SHARE_MODE(0),
                None,
                OPEN_EXISTING,
                FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS,
                None,
            )
        }
        .expect("open junction placeholder");
        // SAFETY: `handle` is an open directory handle and `reparse` contains a
        // complete mount-point reparse buffer for the duration of the call.
        let result = unsafe {
            DeviceIoControl(
                handle,
                FSCTL_SET_REPARSE_POINT,
                Some(reparse.as_ptr().cast()),
                u32::try_from(reparse.len()).expect("junction buffer length"),
                None,
                0,
                None,
                None,
            )
        };
        // SAFETY: this function owns `handle` and closes it exactly once.
        unsafe { CloseHandle(handle) }.expect("close junction handle");
        result.expect("set junction reparse point");
    }

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
    fn joined_paths_retain_store_path_bounds() {
        let left = StorePath::parse(&vec!["a"; 40].join("/")).unwrap();
        let right = StorePath::parse(&vec!["b"; 25].join("/")).unwrap();
        assert!(matches!(
            left.join(&right),
            Err(StoreRootError::InvalidPath("too_deep"))
        ));
    }

    #[test]
    fn bounded_read_never_allocates_the_untrusted_tail() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("large.bin"), vec![b'x'; 32 * 1024]).unwrap();
        let root = StoreRoot::open(temp.path()).unwrap();

        assert!(matches!(
            root.read_limited(&path("large.bin"), 1024),
            Err(StoreRootError::Limit {
                operation: "read_limited",
                max_bytes: 1024
            })
        ));
    }

    #[test]
    fn hard_link_cannot_leak_or_mutate_an_outside_inode() {
        let temp = tempfile::tempdir().unwrap();
        let root_path = temp.path().join("root");
        let copy_path = temp.path().join("copy");
        let outside = temp.path().join("outside.txt");
        std::fs::create_dir(&root_path).unwrap();
        std::fs::create_dir(&copy_path).unwrap();
        std::fs::write(&outside, b"outside").unwrap();
        std::fs::hard_link(&outside, root_path.join("linked.txt")).unwrap();
        let root = StoreRoot::open(&root_path).unwrap();
        let copy = StoreRoot::open(&copy_path).unwrap();

        let linked = path("linked.txt");
        let entry = root
            .list_root()
            .unwrap()
            .into_iter()
            .find(|entry| entry.path == linked)
            .unwrap();
        assert_eq!(entry.kind, StoreEntryKind::Other);
        assert_eq!(entry.size, 0);
        assert!(root.read(&linked).is_err());
        assert!(root.metadata(&linked).is_err());
        assert!(root.is_file(&linked).is_err());
        assert!(root.append(&linked, b"changed").is_err());
        assert!(root.rename(&linked, &path("renamed.txt")).is_err());
        assert!(
            copy.atomic_copy_from(&path("copy.txt"), &root, &linked, 1024)
                .is_err()
        );
        assert_eq!(std::fs::read(&outside).unwrap(), b"outside");

        // Atomic replacement does not mutate the linked inode; it safely
        // publishes a new store-owned file at the same name.
        root.atomic_write(&linked, b"inside").unwrap();
        assert_eq!(root.read(&linked).unwrap(), b"inside");
        assert_eq!(std::fs::read(&outside).unwrap(), b"outside");
    }

    #[cfg(unix)]
    #[test]
    fn atomic_copy_uses_held_source_and_destination_roots() {
        let temp = tempfile::tempdir().unwrap();
        let source_path = temp.path().join("source");
        let target_path = temp.path().join("target");
        let held_source = temp.path().join("held-source");
        let held_target = temp.path().join("held-target");
        std::fs::create_dir(&source_path).unwrap();
        std::fs::create_dir(&target_path).unwrap();
        std::fs::write(source_path.join("asset.bin"), vec![b'x'; 128 * 1024]).unwrap();
        let source = StoreRoot::open(&source_path).unwrap();
        let target = StoreRoot::open(&target_path).unwrap();

        std::fs::rename(&source_path, &held_source).unwrap();
        std::fs::rename(&target_path, &held_target).unwrap();
        std::fs::create_dir(&source_path).unwrap();
        std::fs::create_dir(&target_path).unwrap();
        let copied = target
            .atomic_copy_from(
                &path("nested/asset.bin"),
                &source,
                &path("asset.bin"),
                256 * 1024,
            )
            .unwrap();

        assert_eq!(copied, 128 * 1024);
        assert_eq!(
            std::fs::read(held_target.join("nested/asset.bin")).unwrap(),
            vec![b'x'; 128 * 1024]
        );
        assert_eq!(std::fs::read_dir(&source_path).unwrap().count(), 0);
        assert_eq!(std::fs::read_dir(&target_path).unwrap().count(), 0);
    }

    #[cfg(unix)]
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

    #[cfg(unix)]
    #[test]
    fn child_process_starts_from_the_held_root_after_path_replacement() {
        let temp = tempfile::tempdir().unwrap();
        let root_path = temp.path().join("root");
        let held_path = temp.path().join("held-root");
        std::fs::create_dir(&root_path).unwrap();
        let root = StoreRoot::open(&root_path).unwrap();

        std::fs::rename(&root_path, &held_path).unwrap();
        std::fs::create_dir(&root_path).unwrap();
        let mut command = Command::new("sh");
        command.args(["-c", "printf held > process-proof.txt"]);
        root.configure_command_current_dir(&mut command, &root_path)
            .unwrap();
        assert!(command.status().unwrap().success());

        assert_eq!(
            std::fs::read(held_path.join("process-proof.txt")).unwrap(),
            b"held"
        );
        assert!(!root_path.join("process-proof.txt").exists());
    }

    #[cfg(windows)]
    #[test]
    fn windows_root_guards_block_ambient_rename_replacement() {
        let temp = tempfile::tempdir().unwrap();
        let root_path = temp.path().join("root");
        let moved_path = temp.path().join("moved-root");
        let root = StoreRoot::open_or_create_nofollow(&root_path).unwrap();

        assert!(std::fs::rename(&root_path, &moved_path).is_err());
        root.atomic_write(&path("proof.txt"), b"held").unwrap();
        assert_eq!(std::fs::read(root_path.join("proof.txt")).unwrap(), b"held");
        assert!(!moved_path.exists());
    }

    #[cfg(windows)]
    #[test]
    fn windows_process_cwd_rejects_a_different_root_identity() {
        let temp = tempfile::tempdir().unwrap();
        let held_path = temp.path().join("held");
        let other_path = temp.path().join("other");
        let held = StoreRoot::open_or_create_nofollow(&held_path).unwrap();
        StoreRoot::open_or_create_nofollow(&other_path).unwrap();
        let mut command = Command::new("cmd.exe");

        let error = held
            .configure_command_current_dir(&mut command, &other_path)
            .unwrap_err();
        assert!(matches!(
            error,
            StoreRootError::Confinement {
                operation: "process_root_identity",
                reason: ConfinementReason::RootIdentity,
            }
        ));
    }

    #[cfg(windows)]
    #[test]
    fn windows_reparse_ancestor_cannot_acquire_root_authority() {
        let temp = tempfile::tempdir().unwrap();
        let outside = temp.path().join("outside");
        let linked = temp.path().join("linked");
        std::fs::create_dir(&outside).unwrap();
        create_junction(&outside, &linked);

        let error = match StoreRoot::open_or_create_nofollow(&linked.join("vault")) {
            Ok(_) => panic!("reparse ancestor must fail"),
            Err(error) => error,
        };
        assert!(matches!(
            error,
            StoreRootError::Confinement {
                reason: ConfinementReason::SymbolicLink,
                ..
            }
        ));
    }

    #[cfg(windows)]
    #[test]
    fn windows_junction_cannot_escape_any_root_operation() {
        let temp = tempfile::tempdir().unwrap();
        let root_path = temp.path().join("root");
        let outside = temp.path().join("outside");
        std::fs::create_dir(&root_path).unwrap();
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("canary.txt"), b"outside").unwrap();
        create_junction(&outside, &root_path.join("junction"));
        let root = StoreRoot::open_nofollow(&root_path).unwrap();

        let entries = root.list_root_utf8().unwrap();
        assert!(
            entries
                .iter()
                .any(|entry| { entry.name == "junction" && entry.kind == StoreEntryKind::Link })
        );
        assert!(root.list_directory(&path("junction")).is_err());
        assert!(root.read(&path("junction/canary.txt")).is_err());
        assert!(
            root.append(&path("junction/canary.txt"), b"changed")
                .is_err()
        );
        assert!(
            root.atomic_write(&path("junction/new.txt"), b"changed")
                .is_err()
        );
        assert!(
            root.rename(&path("junction/canary.txt"), &path("moved.txt"))
                .is_err()
        );
        assert!(root.remove_file(&path("junction/canary.txt")).is_err());
        assert!(root.remove_dir_all(&path("junction")).is_err());

        assert_eq!(
            std::fs::read(outside.join("canary.txt")).unwrap(),
            b"outside"
        );
        assert!(!outside.join("new.txt").exists());
        assert!(!root_path.join("moved.txt").exists());
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

        let entries = root.list_root().unwrap();
        assert!(entries.iter().any(|entry| {
            entry.path.file_name() == "leaf.txt" && entry.kind == StoreEntryKind::Link
        }));
        assert!(entries.iter().any(|entry| {
            entry.path.file_name() == "ancestor" && entry.kind == StoreEntryKind::Link
        }));
        assert!(root.read(&path("leaf.txt")).is_err());
        assert!(root.atomic_write(&path("leaf.txt"), b"changed").is_err());
        assert!(root.read(&path("ancestor/canary.txt")).is_err());
        assert!(
            root.atomic_write(&path("ancestor/new.txt"), b"changed")
                .is_err()
        );
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
