//! OS-backed turn ownership. A locked inode, not a PID or an idle timer.
use fs2::FileExt;
use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
    sync::Mutex,
};

pub struct TurnOwner {
    path: PathBuf,
    file: Mutex<Option<File>>,
}
impl TurnOwner {
    pub fn new() -> std::io::Result<Self> {
        let path = std::env::temp_dir().join(format!("medousa-pty-owner-{}", uuid::Uuid::new_v4()));
        let mut options = OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(&path)?;
        FileExt::try_lock_exclusive(&file)?;
        Ok(Self {
            path,
            file: Mutex::new(Some(file)),
        })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Irreversible: no subsequent shell may be created for this turn.
    pub fn release(&self) {
        self.file.lock().unwrap().take();
    }
}
impl Drop for TurnOwner {
    fn drop(&mut self) {
        self.release();
        let _ = std::fs::remove_file(&self.path);
    }
}

pub(crate) struct OwnerWatch {
    file: File,
    path: PathBuf,
}
impl OwnerWatch {
    pub fn open(path: PathBuf) -> std::io::Result<Self> {
        let temp = std::env::temp_dir().canonicalize()?;
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        let valid_name = name
            .strip_prefix("medousa-pty-owner-")
            .is_some_and(|suffix| uuid::Uuid::parse_str(suffix).is_ok());
        if !valid_name
            || path.parent().and_then(|p| p.canonicalize().ok()).as_ref() != Some(&temp)
            || std::fs::symlink_metadata(&path)?.file_type().is_symlink()
        {
            return Err(std::io::Error::other("invalid turn owner file"));
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
        }
        let file = options.open(&path)?;
        if !file.metadata()?.is_file() || file.metadata()?.len() != 0 {
            return Err(std::io::Error::other("invalid turn owner inode"));
        }
        let watch = Self { file, path };
        if watch.gone()? {
            return Err(std::io::Error::other("turn owner already released"));
        }
        Ok(watch)
    }
    pub fn gone(&self) -> std::io::Result<bool> {
        match FileExt::try_lock_exclusive(&self.file) {
            Ok(()) => {
                FileExt::unlock(&self.file)?;
                Ok(true)
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(false),
            Err(error) => Err(error), // Unknown is NOT evidence of owner loss.
        }
    }
    pub fn remove(&self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
