//! Size-capped log rotation for append-only daemon sinks.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Rotation policy for a single log file.
#[derive(Debug, Clone, Copy)]
pub struct RotateConfig {
    /// Rotate when the file exceeds this many bytes (default 50 MiB).
    pub max_bytes: u64,
    /// Number of rotated backups to retain (`.1`, `.2`, …).
    pub max_backups: u8,
}

impl Default for RotateConfig {
    fn default() -> Self {
        Self {
            max_bytes: 50 * 1024 * 1024,
            max_backups: 3,
        }
    }
}

impl RotateConfig {
    pub fn from_env() -> Self {
        let max_bytes = std::env::var("MEDOUSA_LOG_ROTATE_MAX_BYTES")
            .ok()
            .and_then(|raw| raw.trim().parse().ok())
            .unwrap_or_else(|| Self::default().max_bytes);
        let max_backups = std::env::var("MEDOUSA_LOG_ROTATE_MAX_BACKUPS")
            .ok()
            .and_then(|raw| raw.trim().parse().ok())
            .unwrap_or_else(|| Self::default().max_backups);
        Self {
            max_bytes,
            max_backups: max_backups.clamp(1, 10),
        }
    }
}

/// Rotate `path` when it exceeds `config.max_bytes`. Returns `true` when a rotation occurred.
pub fn rotate_if_oversized(path: &Path, config: RotateConfig) -> io::Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    let len = fs::metadata(path)?.len();
    if len <= config.max_bytes {
        return Ok(false);
    }

    // Drop the oldest backup, then shift `.N` → `.N+1`, then move active → `.1`.
    let oldest = backup_path(path, config.max_backups);
    if oldest.exists() {
        let _ = fs::remove_file(&oldest);
    }
    for idx in (1..config.max_backups).rev() {
        let from = backup_path(path, idx);
        let to = backup_path(path, idx + 1);
        if from.exists() {
            fs::rename(&from, &to)?;
        }
    }
    let first = backup_path(path, 1);
    fs::rename(path, &first)?;
    Ok(true)
}

fn backup_path(path: &Path, index: u8) -> PathBuf {
    PathBuf::from(format!("{}.{}", path.display(), index))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn rotates_when_over_limit() {
        let dir = std::env::temp_dir().join(format!("medousa-log-rotate-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("daemon.log");
        {
            let mut file = fs::File::create(&path).unwrap();
            file.write_all(b"0123456789").unwrap();
        }
        let rotated = rotate_if_oversized(
            &path,
            RotateConfig {
                max_bytes: 5,
                max_backups: 2,
            },
        )
        .unwrap();
        assert!(rotated);
        assert!(!path.exists());
        assert!(backup_path(&path, 1).exists());
        let _ = fs::remove_dir_all(dir);
    }
}
