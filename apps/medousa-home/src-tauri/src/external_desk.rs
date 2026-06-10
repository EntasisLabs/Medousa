use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Serialize;

const MAX_SCAN_FILES: usize = 800;
const MAX_SCAN_DEPTH: usize = 5;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ExternalFileEntry {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub ext: Option<String>,
    pub modified_at_utc: String,
    pub size_bytes: u64,
}

fn should_skip(name: &str) -> bool {
    if name.starts_with('.') {
        return true;
    }
    matches!(
        name,
        "node_modules" | ".git" | "target" | "dist" | "build" | "__pycache__"
    )
}

fn modified_at(metadata: &std::fs::Metadata) -> DateTime<Utc> {
    metadata
        .modified()
        .ok()
        .and_then(|stamp| {
            stamp.duration_since(std::time::UNIX_EPOCH).ok().and_then(
                |duration| DateTime::from_timestamp(duration.as_secs() as i64, duration.subsec_nanos()),
            )
        })
        .unwrap_or_else(Utc::now)
}

fn scan_dir(path: &Path, depth: usize, out: &mut Vec<ExternalFileEntry>) {
    if out.len() >= MAX_SCAN_FILES || depth > MAX_SCAN_DEPTH {
        return;
    }

    let entries = match std::fs::read_dir(path) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        if out.len() >= MAX_SCAN_FILES {
            break;
        }
        let file_path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if should_skip(&name) {
            continue;
        }

        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        let is_dir = metadata.is_dir();
        let ext = if is_dir {
            None
        } else {
            file_path
                .extension()
                .map(|value| value.to_string_lossy().to_lowercase())
                .filter(|value| !value.is_empty())
        };

        out.push(ExternalFileEntry {
            path: file_path.to_string_lossy().to_string(),
            name,
            is_dir,
            ext,
            modified_at_utc: modified_at(&metadata).to_rfc3339(),
            size_bytes: metadata.len(),
        });

        if is_dir {
            scan_dir(&file_path, depth + 1, out);
        }
    }
}

#[tauri::command]
pub fn external_desk_scan_root(path: String) -> Result<Vec<ExternalFileEntry>, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("path is required".to_string());
    }
    let root = PathBuf::from(trimmed);
    if !root.exists() {
        return Err(format!("path not found: {trimmed}"));
    }
    if !root.is_dir() {
        return Err(format!("not a directory: {trimmed}"));
    }

    let mut entries = Vec::new();
    scan_dir(&root, 0, &mut entries);
    entries.sort_by(|left, right| right.modified_at_utc.cmp(&left.modified_at_utc));
    entries.truncate(MAX_SCAN_FILES);
    Ok(entries)
}
