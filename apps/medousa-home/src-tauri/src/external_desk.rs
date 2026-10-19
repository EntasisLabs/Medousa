use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use medousa_forge::execution::{ExecutionClass, ForgeExecutionService};
use serde::Serialize;

pub(crate) fn file_execution() -> &'static ForgeExecutionService {
    static EXECUTION: std::sync::OnceLock<ForgeExecutionService> = std::sync::OnceLock::new();
    EXECUTION.get_or_init(ForgeExecutionService::new)
}

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

const DIRECTORY_PAGE_SIZE: usize = 200;
type DirectoryKey = (bool, String, String);

#[derive(Debug, Serialize)]
pub struct ExternalDirectoryPage {
    pub entries: Vec<ExternalFileEntry>,
    pub next_cursor: Option<String>,
}

#[tauri::command]
pub async fn external_desk_list_directory(
    path: String,
    cursor: Option<String>,
    show_hidden: bool,
) -> Result<ExternalDirectoryPage, String> {
    file_execution()
        .run(ExecutionClass::Observation, 256 * 1024, move || {
            Ok(list_directory(path, cursor, show_hidden))
        })
        .await
        .map_err(|error| error.to_string())?
}

fn list_directory(
    path: String,
    cursor: Option<String>,
    show_hidden: bool,
) -> Result<ExternalDirectoryPage, String> {
    if path.trim().is_empty() {
        return Err("path is required".into());
    }
    let after: Option<DirectoryKey> = cursor
        .map(|value| serde_json::from_str(&value))
        .transpose()
        .map_err(|_| "Invalid folder page cursor".to_string())?;
    // Keep only one page plus a lookahead. Metadata is read only for that page;
    // enumerating a folder never walks into its children.
    let mut candidates = std::collections::BTreeMap::new();
    for entry in std::fs::read_dir(&path).map_err(|error| format!("Cannot open folder: {error}"))? {
        let entry = entry.map_err(|error| format!("Cannot read folder entry: {error}"))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !show_hidden && name.starts_with('.') {
            continue;
        }
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        let is_dir = if file_type.is_symlink() {
            std::fs::metadata(entry.path()).is_ok_and(|metadata| metadata.is_dir())
        } else {
            file_type.is_dir()
        };
        let key = (!is_dir, name.to_lowercase(), name);
        if after.as_ref().is_some_and(|after| key <= *after) {
            continue;
        }
        candidates.insert(key, entry.path());
        if candidates.len() > DIRECTORY_PAGE_SIZE + 1 {
            candidates.pop_last();
        }
    }
    let more = candidates.len() > DIRECTORY_PAGE_SIZE;
    if more {
        candidates.pop_last();
    }
    let next_cursor = if more {
        candidates
            .last_key_value()
            .map(|(key, _)| serde_json::to_string(key).unwrap())
    } else {
        None
    };
    let entries = candidates
        .into_iter()
        .map(|((is_file, _, name), path)| {
            let metadata = std::fs::metadata(&path).ok();
            ExternalFileEntry {
                ext: if is_file {
                    path.extension()
                        .map(|ext| ext.to_string_lossy().to_lowercase())
                } else {
                    None
                },
                path: path.to_string_lossy().into_owned(),
                name,
                is_dir: !is_file,
                modified_at_utc: metadata
                    .as_ref()
                    .map(|metadata| modified_at(metadata).to_rfc3339())
                    .unwrap_or_default(),
                size_bytes: metadata.as_ref().map_or(0, |metadata| metadata.len()),
            }
        })
        .collect();
    Ok(ExternalDirectoryPage {
        entries,
        next_cursor,
    })
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
            stamp
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .and_then(|duration| {
                    DateTime::from_timestamp(duration.as_secs() as i64, duration.subsec_nanos())
                })
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
pub async fn external_desk_scan_root(path: String) -> Result<Vec<ExternalFileEntry>, String> {
    file_execution()
        .run(ExecutionClass::Observation, 256 * 1024, move || {
            Ok(scan_root(path))
        })
        .await
        .map_err(|error| error.to_string())?
}

fn scan_root(path: String) -> Result<Vec<ExternalFileEntry>, String> {
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

const MAX_TEXT_READ_BYTES: u64 = 2_000_000;
const MAX_BINARY_READ_BYTES: u64 = 12_000_000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ExternalFilePayload {
    pub kind: String,
    pub content: String,
    pub size_bytes: u64,
}

#[tauri::command]
pub async fn external_desk_read_file(path: String) -> Result<ExternalFilePayload, String> {
    file_execution()
        .run(ExecutionClass::StoreIo, path.len(), move || {
            Ok(read_file(path))
        })
        .await
        .map_err(|error| error.to_string())?
}

fn read_file(path: String) -> Result<ExternalFilePayload, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("path is required".to_string());
    }
    let file_path = PathBuf::from(trimmed);
    if !file_path.exists() {
        return Err(format!("file not found: {trimmed}"));
    }
    if !file_path.is_file() {
        return Err(format!("not a file: {trimmed}"));
    }

    let metadata = std::fs::metadata(&file_path).map_err(|err| err.to_string())?;
    let size_bytes = metadata.len();
    let ext = file_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_lowercase();
    let is_binary = matches!(
        ext.as_str(),
        "xlsx"
            | "xls"
            | "xlsm"
            | "pdf"
            | "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "webp"
            | "avif"
            | "bmp"
            | "svg"
            | "ico"
            | "heic"
            | "heif"
            | "tif"
            | "tiff"
    );

    if is_binary {
        if size_bytes > MAX_BINARY_READ_BYTES {
            return Err(format!(
                "file too large to preview ({size_bytes} bytes; max {MAX_BINARY_READ_BYTES})"
            ));
        }
        let bytes = std::fs::read(&file_path).map_err(|err| err.to_string())?;
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        Ok(ExternalFilePayload {
            kind: "base64".to_string(),
            content: STANDARD.encode(bytes),
            size_bytes,
        })
    } else {
        if size_bytes > MAX_TEXT_READ_BYTES {
            return Err(format!(
                "file too large to preview ({size_bytes} bytes; max {MAX_TEXT_READ_BYTES})"
            ));
        }
        let content = std::fs::read_to_string(&file_path).map_err(|err| err.to_string())?;
        Ok(ExternalFilePayload {
            kind: "text".to_string(),
            content,
            size_bytes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_pages_are_complete_and_shallow_with_optional_hidden_items() {
        let root = std::env::temp_dir().join(format!(
            "medousa-folders-{}",
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        std::fs::create_dir_all(root.join("build/deep/child")).unwrap();
        std::fs::write(root.join("build/deep/child/not-on-root-page.txt"), "nested").unwrap();
        std::fs::write(root.join(".hidden"), "hidden").unwrap();
        for index in 0..220 {
            std::fs::write(root.join(format!("file-{index:03}.txt")), "text").unwrap();
        }
        let path = root.to_string_lossy().into_owned();
        let first = list_directory(path.clone(), None, false).unwrap();
        assert_eq!(first.entries.len(), DIRECTORY_PAGE_SIZE);
        assert_eq!(first.entries[0].name, "build");
        assert!(first.entries[0].is_dir);
        let second = list_directory(path.clone(), first.next_cursor.clone(), false).unwrap();
        assert_eq!(second.entries.len(), 21);
        assert!(second.next_cursor.is_none());
        let names: std::collections::BTreeSet<_> = first
            .entries
            .iter()
            .chain(&second.entries)
            .map(|entry| &entry.name)
            .collect();
        assert_eq!(names.len(), 221);
        assert!(!names.contains(&"not-on-root-page.txt".to_string()));
        assert!(!names.contains(&".hidden".to_string()));
        assert!(
            list_directory(path.clone(), None, true)
                .unwrap()
                .entries
                .iter()
                .any(|entry| entry.name == ".hidden")
        );
        assert!(list_directory(path, Some("broken cursor".into()), false).is_err());
        assert!(
            list_directory(
                root.join("missing").to_string_lossy().into_owned(),
                None,
                false
            )
            .is_err()
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn pinned_documents_deliver_binary_bytes_and_text_without_a_vault_root() {
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        let root = std::env::temp_dir().join(format!(
            "medousa-files-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        std::fs::create_dir_all(&root).unwrap();
        for (name, bytes) in [
            ("report.pdf", b"%PDF-1.4\n\xff".as_slice()),
            ("image.png", b"\x89PNG\r\n\x1a\n".as_slice()),
            ("figure.svg", b"<svg/>".as_slice()),
        ] {
            let path = root.join(name);
            std::fs::write(&path, bytes).unwrap();
            let payload = read_file(path.to_string_lossy().into()).unwrap();
            assert_eq!(payload.kind, "base64");
            assert_eq!(STANDARD.decode(payload.content).unwrap(), bytes);
        }
        let path = root.join("config.json");
        std::fs::write(&path, "{\"name\":\"Medousa\"}").unwrap();
        let payload = read_file(path.to_string_lossy().into()).unwrap();
        assert_eq!(payload.kind, "text");
        assert_eq!(payload.content, "{\"name\":\"Medousa\"}");
        std::fs::remove_dir_all(root).unwrap();
    }
}
