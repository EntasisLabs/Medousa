//! Opt-in coding domain cognition tools (Medousa is not a default coding agent).
//!
//! These tools unlock only when a session surface opts in (manuscript / Forge
//! work bind / Settings) — they are never in the default interactive palette.
//! `code_read` / `code_search` / `code_apply_patch` are rooted at the scripts
//! library or an explicit `root` under the workshop; `shell_session_*` drive
//! the workshop-owned PTY sessions on the daemon.

use std::io::{BufRead, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use stasis::application::orchestration::tool_registry::StasisTool;
use stasis::prelude::{Result as StasisResult, StasisError};

pub const COGNITION_CODE_READ: &str = "cognition_code_read";
pub const COGNITION_CODE_SEARCH: &str = "cognition_code_search";
pub const COGNITION_CODE_APPLY_PATCH: &str = "cognition_code_apply_patch";
pub const COGNITION_SHELL_SESSION_STATUS: &str = "cognition_shell_session_status";
pub const COGNITION_SHELL_SESSION_RUN: &str = "cognition_shell_session_run";
pub const COGNITION_SHELL_SESSION_INTERRUPT: &str = "cognition_shell_session_interrupt";
/// One-shot shell for Coder — Forge-bound PTY facade (not OS `cognition_shell_run`).
pub const COGNITION_CODER_SHELL_RUN: &str = "cognition_coder_shell_run";
pub const COGNITION_CODER_SHELL_STATUS: &str = "cognition_coder_shell_status";

pub const CODING_COGNITION_TOOLS: &[&str] = &[
    COGNITION_CODE_READ,
    COGNITION_CODE_SEARCH,
    COGNITION_CODE_APPLY_PATCH,
    COGNITION_SHELL_SESSION_STATUS,
    COGNITION_SHELL_SESSION_RUN,
    COGNITION_SHELL_SESSION_INTERRUPT,
    COGNITION_CODER_SHELL_RUN,
    COGNITION_CODER_SHELL_STATUS,
];

const MAX_CODE_READ_BYTES: u64 = 128 * 1024;
const MAX_CODE_RANGE_BYTES: usize = 64 * 1024;
const MAX_CODE_RANGE_LINES: usize = 1_000;
const DEFAULT_CODE_RANGE_LINES: usize = 200;
const MAX_CODE_ORIENTATION_SCAN_BYTES: u64 = 64 * 1024 * 1024;
const MAX_CODE_WRITE_BYTES: usize = 4 * 1024 * 1024;
const MAX_SHELL_OUTPUT_BYTES: usize = 1024 * 1024;

pub fn is_coding_cognition_tool(name: &str) -> bool {
    CODING_COGNITION_TOOLS.contains(&name)
}

pub fn is_coder_shell_tool(name: &str) -> bool {
    matches!(name, COGNITION_CODER_SHELL_RUN | COGNITION_CODER_SHELL_STATUS)
}

pub fn is_shell_session_tool(name: &str) -> bool {
    matches!(
        name,
        COGNITION_SHELL_SESSION_STATUS
            | COGNITION_SHELL_SESSION_RUN
            | COGNITION_SHELL_SESSION_INTERRUPT
            | COGNITION_CODER_SHELL_RUN
            | COGNITION_CODER_SHELL_STATUS
    )
}

fn daemon_base() -> String {
    std::env::var("MEDOUSA_DAEMON_URL").unwrap_or_else(|_| "http://127.0.0.1:8741".into())
}

fn allowed_roots() -> Vec<PathBuf> {
    let mut roots = vec![crate::grapheme_script::store::GraphemeScriptStore::root_dir()];
    roots.extend(crate::daemon::shell_session_host::forge_worktree_roots_for_tools());
    roots
}

fn resolve_root(root: Option<&str>) -> StasisResult<PathBuf> {
    let base = match root.map(str::trim).filter(|s| !s.is_empty()) {
        Some(raw) => PathBuf::from(raw),
        None => crate::grapheme_script::store::GraphemeScriptStore::root_dir(),
    };
    let canon = base.canonicalize().map_err(|err| {
        StasisError::PortFailure(format!(
            "cannot resolve coding root {}: {err}",
            base.display()
        ))
    })?;
    let allowed: Vec<PathBuf> = allowed_roots()
        .into_iter()
        .filter_map(|root| root.canonicalize().ok())
        .collect();
    if !allowed.iter().any(|root| canon.starts_with(root)) {
        return Err(StasisError::PortFailure(format!(
            "root not under allowed workshop roots: {}",
            canon.display()
        )));
    }
    Ok(canon)
}

fn resolve_path(root: &Path, rel: &str) -> StasisResult<PathBuf> {
    if rel.trim().is_empty() {
        return Err(StasisError::PortFailure("path is required".into()));
    }
    let path = if Path::new(rel).is_absolute() {
        PathBuf::from(rel)
    } else {
        root.join(rel)
    };
    let authority_path = if path.exists() {
        path.canonicalize().map_err(|err| {
            StasisError::PortFailure(format!("cannot resolve {}: {err}", path.display()))
        })?
    } else {
        let mut ancestor = path.parent();
        let mut resolved_parent = None;
        while let Some(candidate) = ancestor {
            if candidate.exists() {
                resolved_parent = Some(candidate.canonicalize().map_err(|err| {
                    StasisError::PortFailure(format!(
                        "cannot resolve parent {}: {err}",
                        candidate.display()
                    ))
                })?);
                break;
            }
            ancestor = candidate.parent();
        }
        resolved_parent.ok_or_else(|| {
            StasisError::PortFailure(format!("path has no existing parent: {}", path.display()))
        })?
    };
    if !authority_path.starts_with(root) {
        return Err(StasisError::PortFailure(format!(
            "path escapes root: {}",
            path.display()
        )));
    }
    Ok(path)
}

fn content_digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn truncate_utf8_bytes(value: &mut String, max_bytes: usize) {
    if value.len() <= max_bytes {
        return;
    }
    let mut boundary = max_bytes;
    while boundary > 0 && !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    value.truncate(boundary);
}

fn verify_expected_digest(path: &Path, expected: &str) -> StasisResult<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(bytes) => {
            let actual = content_digest(&bytes);
            if expected != actual {
                return Err(StasisError::PortFailure(format!(
                    "stale file digest for {}: expected {expected}, found {actual}",
                    path.display()
                )));
            }
            Ok(Some(bytes))
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound && expected == "missing" => Ok(None),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            Err(StasisError::PortFailure(format!(
                "stale file digest for {}: expected {expected}, found missing",
                path.display()
            )))
        }
        Err(err) => Err(StasisError::PortFailure(format!(
            "read {}: {err}",
            path.display()
        ))),
    }
}

async fn daemon_post(path: &str, body: Value) -> StasisResult<Value> {
    let client = reqwest::Client::new();
    let url = format!("{}{path}", daemon_base().trim_end_matches('/'));
    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| StasisError::PortFailure(format!("daemon session proxy: {e}")))?;
    let status = resp.status();
    let value = resp
        .json::<Value>()
        .await
        .map_err(|e| StasisError::PortFailure(e.to_string()))?;
    if !status.is_success() {
        return Err(StasisError::PortFailure(format!(
            "daemon {status}: {value}"
        )));
    }
    Ok(value)
}

async fn daemon_get(path: &str) -> StasisResult<Value> {
    let client = reqwest::Client::new();
    let url = format!("{}{path}", daemon_base().trim_end_matches('/'));
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| StasisError::PortFailure(format!("daemon session proxy: {e}")))?;
    let status = resp.status();
    let value = resp
        .json::<Value>()
        .await
        .map_err(|e| StasisError::PortFailure(e.to_string()))?;
    if !status.is_success() {
        return Err(StasisError::PortFailure(format!(
            "daemon {status}: {value}"
        )));
    }
    Ok(value)
}

fn root_and_path(input: &Value) -> StasisResult<(PathBuf, PathBuf)> {
    let root = resolve_root(input.get("root").and_then(|v| v.as_str()))?;
    let path = input
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| StasisError::PortFailure("path is required".into()))?;
    let resolved = resolve_path(&root, path)?;
    Ok((root, resolved))
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

pub struct CognitionCodeReadTool;
pub struct CognitionCodeSearchTool;
pub struct CognitionCodeApplyPatchTool;
pub struct CognitionShellSessionStatusTool;
pub struct CognitionShellSessionRunTool;
pub struct CognitionShellSessionInterruptTool;
pub struct CognitionCoderShellRunTool;
pub struct CognitionCoderShellStatusTool;

#[async_trait]
impl StasisTool for CognitionCodeReadTool {
    fn name(&self) -> &'static str {
        COGNITION_CODE_READ
    }
    fn description(&self) -> Option<&'static str> {
        Some(
            "Read a whole text file when it fits, or a bounded line/byte range. Oversized whole-file requests return actionable range orientation instead of an opaque failure. Coding domain only.",
        )
    }
    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Absolute or root-relative file path" },
                "root": { "type": "string", "description": "Optional explicit root (default: scripts library)" },
                "line_start": { "type": "integer", "minimum": 1, "description": "Optional 1-based inclusive line start" },
                "line_end": { "type": "integer", "minimum": 1, "description": "Optional 1-based inclusive line end" },
                "byte_start": { "type": "integer", "minimum": 0, "description": "Optional 0-based byte start; cannot be combined with line ranges" },
                "byte_end": { "type": "integer", "minimum": 1, "description": "Optional exclusive byte end; cannot be combined with line ranges" }
            },
            "required": ["path"]
        }))
    }
    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let (root, path) = root_and_path(&input)?;
        tokio::task::spawn_blocking(move || code_read_observation(&root, &path, &input))
            .await
            .map_err(|err| StasisError::PortFailure(format!("code_read task failed: {err}")))?
    }
}

fn code_read_observation(root: &Path, path: &Path, input: &Value) -> StasisResult<Value> {
    let line_start = input.get("line_start").and_then(Value::as_u64);
    let line_end = input.get("line_end").and_then(Value::as_u64);
    let byte_start = input.get("byte_start").and_then(Value::as_u64);
    let byte_end = input.get("byte_end").and_then(Value::as_u64);
    let has_line_range = line_start.is_some() || line_end.is_some();
    let has_byte_range = byte_start.is_some() || byte_end.is_some();
    if has_line_range && has_byte_range {
        return Err(StasisError::PortFailure(
            "code_read accepts either a line range or a byte range, not both".into(),
        ));
    }

    let metadata = std::fs::metadata(path)
        .map_err(|err| StasisError::PortFailure(format!("read {}: {err}", path.display())))?;
    if !metadata.is_file() {
        return Err(StasisError::PortFailure(format!(
            "code_read target is not a file: {}",
            path.display()
        )));
    }
    let file_bytes = metadata.len();
    if !has_line_range && !has_byte_range && file_bytes <= MAX_CODE_READ_BYTES {
        let bytes = std::fs::read(path)
            .map_err(|err| StasisError::PortFailure(format!("read {}: {err}", path.display())))?;
        let digest = content_digest(&bytes);
        let content = match String::from_utf8(bytes) {
            Ok(content) => content,
            Err(error) => {
                return Ok(json!({
                    "ok": true,
                    "read_status": "orientation_required",
                    "path": path.display().to_string(),
                    "root": root.display().to_string(),
                    "bytes": file_bytes,
                    "digest": digest,
                    "content": Value::Null,
                    "coverage": { "complete": false, "observed_bytes": 0 },
                    "orientation": {
                        "reason": "file_is_not_utf8_text",
                        "message": "Whole-file text decoding is unavailable. Use a bounded byte range for lossy orientation or a binary-aware domain tool.",
                        "suggested_reads": [{
                            "path": path.display().to_string(),
                            "root": root.display().to_string(),
                            "byte_start": 0,
                            "byte_end": (MAX_CODE_RANGE_BYTES as u64).min(file_bytes),
                        }],
                        "invalid_utf8_at": error.utf8_error().valid_up_to(),
                    }
                }));
            }
        };
        return Ok(json!({
            "ok": true,
            "read_status": "complete",
            "path": path.display().to_string(),
            "root": root.display().to_string(),
            "bytes": content.len(),
            "total_lines": text_line_count(&content),
            "digest": digest,
            "coverage": {
                "complete": true,
                "byte_start": 0,
                "byte_end": content.len(),
            },
            "content": content,
        }));
    }

    if has_line_range {
        return read_line_range(root, path, file_bytes, line_start, line_end);
    }
    if has_byte_range {
        return read_byte_range(root, path, file_bytes, byte_start, byte_end);
    }

    oversized_file_orientation(root, path, file_bytes)
}

fn oversized_file_orientation(root: &Path, path: &Path, file_bytes: u64) -> StasisResult<Value> {
    let metadata = scan_text_metadata(path, file_bytes <= MAX_CODE_ORIENTATION_SCAN_BYTES)?;
    let suggested_reads = if let Some(total_lines) = metadata.total_lines {
        let tail_start = total_lines
            .saturating_sub(DEFAULT_CODE_RANGE_LINES)
            .saturating_add(1)
            .max(1);
        let middle_start = (total_lines / 2)
            .saturating_sub(DEFAULT_CODE_RANGE_LINES / 2)
            .max(1);
        json!([
            {
                "purpose": "orient_from_file_start",
                "path": path.display().to_string(),
                "root": root.display().to_string(),
                "line_start": 1,
                "line_end": DEFAULT_CODE_RANGE_LINES.min(total_lines),
            },
            {
                "purpose": "inspect_file_middle",
                "path": path.display().to_string(),
                "root": root.display().to_string(),
                "line_start": middle_start,
                "line_end": middle_start.saturating_add(DEFAULT_CODE_RANGE_LINES - 1).min(total_lines),
            },
            {
                "purpose": "orient_from_file_end",
                "path": path.display().to_string(),
                "root": root.display().to_string(),
                "line_start": tail_start,
                "line_end": total_lines,
            }
        ])
    } else {
        let tail_start = file_bytes.saturating_sub(MAX_CODE_RANGE_BYTES as u64);
        json!([
            {
                "purpose": "inspect_file_start",
                "path": path.display().to_string(),
                "root": root.display().to_string(),
                "byte_start": 0,
                "byte_end": (MAX_CODE_RANGE_BYTES as u64).min(file_bytes),
            },
            {
                "purpose": "inspect_file_end",
                "path": path.display().to_string(),
                "root": root.display().to_string(),
                "byte_start": tail_start,
                "byte_end": file_bytes,
            }
        ])
    };
    Ok(json!({
        "ok": true,
        "read_status": "orientation_required",
        "path": path.display().to_string(),
        "root": root.display().to_string(),
        "bytes": file_bytes,
        "total_lines": metadata.total_lines,
        "digest": metadata.digest,
        "content": Value::Null,
        "coverage": {
            "complete": false,
            "observed_bytes": 0,
            "file_bytes": file_bytes,
        },
        "orientation": {
            "reason": "file_exceeds_whole_read_budget",
            "message": "The file is available, but a whole-file response would exceed the model-safe read budget. Continue with one of the suggested line or byte ranges.",
            "whole_read_limit_bytes": MAX_CODE_READ_BYTES,
            "range_read_limit_bytes": MAX_CODE_RANGE_BYTES,
            "max_range_lines": MAX_CODE_RANGE_LINES,
            "metadata_complete": metadata.complete,
            "suggested_reads": suggested_reads,
        }
    }))
}

fn read_line_range(
    root: &Path,
    path: &Path,
    file_bytes: u64,
    requested_start: Option<u64>,
    requested_end: Option<u64>,
) -> StasisResult<Value> {
    let start = requested_start.unwrap_or(1).max(1) as usize;
    let requested_end = requested_end
        .map(|value| value.max(start as u64) as usize)
        .unwrap_or_else(|| start.saturating_add(DEFAULT_CODE_RANGE_LINES - 1));
    if file_bytes > MAX_CODE_ORIENTATION_SCAN_BYTES {
        return Ok(json!({
            "ok": true,
            "read_status": "orientation_required",
            "path": path.display().to_string(),
            "root": root.display().to_string(),
            "bytes": file_bytes,
            "total_lines": Value::Null,
            "digest": Value::Null,
            "content": Value::Null,
            "coverage": { "complete": false, "observed_bytes": 0 },
            "orientation": {
                "reason": "line_index_scan_budget_exceeded",
                "message": "This file is too large for a bounded line-index scan. Continue with byte ranges or search for a narrower anchor.",
                "suggested_reads": [{
                    "path": path.display().to_string(),
                    "root": root.display().to_string(),
                    "byte_start": 0,
                    "byte_end": (MAX_CODE_RANGE_BYTES as u64).min(file_bytes),
                }],
                "metadata_scan_limit_bytes": MAX_CODE_ORIENTATION_SCAN_BYTES,
            }
        }));
    }
    let effective_end = requested_end.min(start.saturating_add(MAX_CODE_RANGE_LINES - 1));
    let file = std::fs::File::open(path)
        .map_err(|err| StasisError::PortFailure(format!("read {}: {err}", path.display())))?;
    let mut reader = std::io::BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = Vec::new();
    let mut content = Vec::new();
    let mut line_number = 0usize;
    let mut returned_start = None;
    let mut returned_end = None;
    let mut byte_offset = 0u64;
    let mut returned_byte_start = None;
    let mut returned_byte_end = None;
    let request_clamped = requested_end > effective_end;
    let mut output_limited = false;
    let mut oversized_line = None;
    let mut last_byte = None;

    let scan_complete = loop {
        buffer.clear();
        let read = reader
            .read_until(b'\n', &mut buffer)
            .map_err(|err| StasisError::PortFailure(format!("read {}: {err}", path.display())))?;
        if read == 0 {
            break true;
        }
        line_number = line_number.saturating_add(1);
        last_byte = buffer.last().copied();
        hasher.update(&buffer);
        let next_offset = byte_offset.saturating_add(read as u64);
        if line_number >= start && line_number <= effective_end {
            if content.len().saturating_add(buffer.len()) > MAX_CODE_RANGE_BYTES {
                output_limited = true;
                oversized_line.get_or_insert((line_number, byte_offset, next_offset));
            } else if !output_limited {
                returned_start.get_or_insert(line_number);
                returned_end = Some(line_number);
                returned_byte_start.get_or_insert(byte_offset);
                returned_byte_end = Some(next_offset);
                content.extend_from_slice(&buffer);
            }
        }
        byte_offset = next_offset;
        if line_number >= effective_end {
            break byte_offset >= file_bytes;
        }
    };

    let encoding = if std::str::from_utf8(&content).is_ok() {
        "utf8"
    } else {
        "utf8_lossy"
    };
    let returned_bytes = content.len();
    let content = String::from_utf8_lossy(&content).into_owned();
    let total_lines = scan_complete.then_some(line_number);
    let digest = scan_complete.then(|| format!("sha256:{:x}", hasher.finalize()));
    let returned_end_value = returned_end.unwrap_or(start.saturating_sub(1));
    let has_more = total_lines
        .map(|total| returned_end_value < total)
        .unwrap_or(returned_end_value <= effective_end);
    let next_line = if has_more {
        Some(returned_end_value.saturating_add(1).max(start))
    } else {
        None
    };
    let file_complete =
        returned_start == Some(1) && total_lines.is_some_and(|total| returned_end == Some(total));
    let continuation = oversized_line
        .map(|(line, byte_start, byte_end)| {
            json!({
                "purpose": "continue_within_oversized_line",
                "path": path.display().to_string(),
                "root": root.display().to_string(),
                "line": line,
                "byte_start": byte_start,
                "byte_end": byte_start.saturating_add(MAX_CODE_RANGE_BYTES as u64).min(byte_end),
            })
        })
        .or_else(|| {
            next_line.map(|line_start| {
                json!({
                    "purpose": "continue_by_line",
                    "path": path.display().to_string(),
                    "root": root.display().to_string(),
                    "line_start": line_start,
                    "line_end": line_start.saturating_add(DEFAULT_CODE_RANGE_LINES - 1),
                })
            })
        });
    Ok(json!({
        "ok": true,
        "read_status": if request_clamped || output_limited { "partial" } else { "range" },
        "path": path.display().to_string(),
        "root": root.display().to_string(),
        "bytes": file_bytes,
        "total_lines": total_lines,
        "digest": digest,
        "requested": { "line_start": start, "line_end": requested_end },
        "coverage": {
            "complete": file_complete,
            "line_start": returned_start,
            "line_end": returned_end,
            "byte_start": returned_byte_start,
            "byte_end": returned_byte_end,
            "returned_bytes": returned_bytes,
            "metadata_complete": scan_complete,
        },
        "encoding": encoding,
        "content": content,
        "orientation": {
            "reason": if request_clamped || output_limited { "range_bounded_by_response_budget" } else { "requested_range_returned" },
            "next_read": continuation,
            "max_range_bytes": MAX_CODE_RANGE_BYTES,
            "max_range_lines": MAX_CODE_RANGE_LINES,
            "file_ended_with_newline": scan_complete.then_some(last_byte == Some(b'\n')),
        }
    }))
}

fn read_byte_range(
    root: &Path,
    path: &Path,
    file_bytes: u64,
    requested_start: Option<u64>,
    requested_end: Option<u64>,
) -> StasisResult<Value> {
    let start = requested_start.unwrap_or(0).min(file_bytes);
    let requested_end = requested_end
        .unwrap_or_else(|| start.saturating_add(MAX_CODE_RANGE_BYTES as u64))
        .max(start)
        .min(file_bytes);
    let effective_end = requested_end.min(start.saturating_add(MAX_CODE_RANGE_BYTES as u64));
    let mut file = std::fs::File::open(path)
        .map_err(|err| StasisError::PortFailure(format!("read {}: {err}", path.display())))?;
    file.seek(SeekFrom::Start(start))
        .map_err(|err| StasisError::PortFailure(format!("seek {}: {err}", path.display())))?;
    let mut bytes = vec![0u8; effective_end.saturating_sub(start) as usize];
    file.read_exact(&mut bytes)
        .map_err(|err| StasisError::PortFailure(format!("read {}: {err}", path.display())))?;
    let encoding = if std::str::from_utf8(&bytes).is_ok() {
        "utf8"
    } else {
        "utf8_lossy"
    };
    let content = String::from_utf8_lossy(&bytes).into_owned();
    let next_read = (effective_end < file_bytes).then(|| {
        json!({
            "path": path.display().to_string(),
            "root": root.display().to_string(),
            "byte_start": effective_end,
            "byte_end": effective_end.saturating_add(MAX_CODE_RANGE_BYTES as u64).min(file_bytes),
        })
    });
    Ok(json!({
        "ok": true,
        "read_status": if effective_end < requested_end { "partial" } else { "range" },
        "path": path.display().to_string(),
        "root": root.display().to_string(),
        "bytes": file_bytes,
        "total_lines": Value::Null,
        "digest": Value::Null,
        "requested": { "byte_start": start, "byte_end": requested_end },
        "coverage": {
            "complete": start == 0 && effective_end == file_bytes,
            "byte_start": start,
            "byte_end": effective_end,
            "returned_bytes": bytes.len(),
        },
        "encoding": encoding,
        "content": content,
        "orientation": {
            "reason": if effective_end < requested_end { "range_bounded_by_response_budget" } else { "requested_range_returned" },
            "next_read": next_read,
            "max_range_bytes": MAX_CODE_RANGE_BYTES,
        }
    }))
}

struct CodeFileMetadata {
    digest: Option<String>,
    total_lines: Option<usize>,
    complete: bool,
}

fn scan_text_metadata(path: &Path, scan: bool) -> StasisResult<CodeFileMetadata> {
    if !scan {
        return Ok(CodeFileMetadata {
            digest: None,
            total_lines: None,
            complete: false,
        });
    }
    let mut file = std::fs::File::open(path)
        .map_err(|err| StasisError::PortFailure(format!("read {}: {err}", path.display())))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut bytes = 0usize;
    let mut newline_count = 0usize;
    let mut last_byte = None;
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|err| StasisError::PortFailure(format!("read {}: {err}", path.display())))?;
        if read == 0 {
            break;
        }
        let chunk = &buffer[..read];
        hasher.update(chunk);
        bytes = bytes.saturating_add(read);
        newline_count =
            newline_count.saturating_add(chunk.iter().filter(|byte| **byte == b'\n').count());
        last_byte = chunk.last().copied();
    }
    let total_lines = if bytes == 0 {
        0
    } else {
        newline_count + usize::from(last_byte != Some(b'\n'))
    };
    Ok(CodeFileMetadata {
        digest: Some(format!("sha256:{:x}", hasher.finalize())),
        total_lines: Some(total_lines),
        complete: true,
    })
}

fn text_line_count(content: &str) -> usize {
    if content.is_empty() {
        0
    } else {
        content
            .as_bytes()
            .iter()
            .filter(|byte| **byte == b'\n')
            .count()
            + usize::from(!content.ends_with('\n'))
    }
}

#[async_trait]
impl StasisTool for CognitionCodeSearchTool {
    fn name(&self) -> &'static str {
        COGNITION_CODE_SEARCH
    }
    fn description(&self) -> Option<&'static str> {
        Some(
            "Search for a substring under the scripts root or a Forge worktree. Coding domain only.",
        )
    }
    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" },
                "root": { "type": "string" },
                "max_results": { "type": "integer" }
            },
            "required": ["query"]
        }))
    }
    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let query = input
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| StasisError::PortFailure("query is required".into()))?;
        if query.chars().count() > 512 {
            return Err(StasisError::PortFailure(
                "query exceeds 512 characters".into(),
            ));
        }
        let root = resolve_root(input.get("root").and_then(|v| v.as_str()))?;
        let max = input
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(50)
            .clamp(1, 500) as usize;

        let mut results = Vec::new();
        let mut scanned = 0usize;
        search_dir(&root, &root, query, max, &mut scanned, &mut results)
            .map_err(|e| StasisError::PortFailure(e.to_string()))?;
        Ok(
            json!({ "ok": true, "root": root.display().to_string(), "query": query, "results": results }),
        )
    }
}

fn search_dir(
    dir: &Path,
    root: &Path,
    query: &str,
    max: usize,
    scanned: &mut usize,
    out: &mut Vec<Value>,
) -> std::io::Result<()> {
    const MAX_SCANNED_FILES: usize = 20_000;
    const MAX_SEARCH_FILE_BYTES: u64 = 2 * 1024 * 1024;
    if out.len() >= max || *scanned >= MAX_SCANNED_FILES {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            search_dir(&path, root, query, max, scanned, out)?;
        } else if path.is_file() {
            *scanned += 1;
            if entry.metadata()?.len() > MAX_SEARCH_FILE_BYTES {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            if content.contains(query) {
                let rel = path.strip_prefix(root).unwrap_or(&path);
                let lines: Vec<usize> = content
                    .lines()
                    .enumerate()
                    .filter(|(_, l)| l.contains(query))
                    .map(|(i, _)| i + 1)
                    .take(5)
                    .collect();
                out.push(json!({
                    "path": rel.display().to_string(),
                    "lines": lines,
                }));
                if out.len() >= max || *scanned >= MAX_SCANNED_FILES {
                    return Ok(());
                }
            }
        }
    }
    Ok(())
}

#[async_trait]
impl StasisTool for CognitionCodeApplyPatchTool {
    fn name(&self) -> &'static str {
        COGNITION_CODE_APPLY_PATCH
    }
    fn description(&self) -> Option<&'static str> {
        Some(
            "Write full content or replace an exact snippet in a file under the session root / Forge worktree. Coding domain only.",
        )
    }
    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "root": { "type": "string" },
                "content": { "type": "string", "description": "Full file content (write)" },
                "find": { "type": "string", "description": "Exact snippet to replace" },
                "replace": { "type": "string", "description": "Replacement for `find`" }
                ,"expected_sha256": { "type": "string", "description": "Required current digest from code_read, or `missing` for a new file" }
            },
            "required": ["path", "expected_sha256"]
        }))
    }
    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let (root, path) = root_and_path(&input)?;
        let expected_digest = input
            .get("expected_sha256")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| StasisError::PortFailure("expected_sha256 is required".into()))?;
        let existing = verify_expected_digest(&path, expected_digest)?;
        if let Some(content) = input.get("content").and_then(|v| v.as_str()) {
            if content.len() > MAX_CODE_WRITE_BYTES {
                return Err(StasisError::PortFailure(format!(
                    "content exceeds code_apply_patch limit of {MAX_CODE_WRITE_BYTES} bytes"
                )));
            }
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|e| StasisError::PortFailure(e.to_string()))?;
            }
            tokio::fs::write(&path, content)
                .await
                .map_err(|e| StasisError::PortFailure(format!("write {}: {e}", path.display())))?;
            return Ok(json!({
                "ok": true,
                "mode": "write",
                "path": path.display().to_string(),
                "root": root.display().to_string(),
                "bytes": content.len(),
                "digest": content_digest(content.as_bytes()),
            }));
        }
        let find = input.get("find").and_then(|v| v.as_str());
        let replace = input.get("replace").and_then(|v| v.as_str());
        let (find, replace) = match (find, replace) {
            (Some(f), Some(r)) => (f, r),
            _ => {
                return Err(StasisError::PortFailure(
                    "provide `content` (write) or `find` + `replace` (patch)".into(),
                ));
            }
        };
        let existing = existing.ok_or_else(|| {
            StasisError::PortFailure("cannot replace a snippet in a missing file".into())
        })?;
        let existing = String::from_utf8(existing)
            .map_err(|_| StasisError::PortFailure("patch target is not UTF-8 text".into()))?;
        if !existing.contains(find) {
            return Err(StasisError::PortFailure(
                "find snippet not present in file".into(),
            ));
        }
        let next = existing.replacen(find, replace, 1);
        if next.len() > MAX_CODE_WRITE_BYTES {
            return Err(StasisError::PortFailure(format!(
                "patched content exceeds code_apply_patch limit of {MAX_CODE_WRITE_BYTES} bytes"
            )));
        }
        tokio::fs::write(&path, &next)
            .await
            .map_err(|e| StasisError::PortFailure(format!("write {}: {e}", path.display())))?;
        Ok(json!({
            "ok": true,
            "mode": "patch",
            "path": path.display().to_string(),
            "root": root.display().to_string(),
            "bytes": next.len(),
            "digest": content_digest(next.as_bytes()),
        }))
    }
}

#[async_trait]
impl StasisTool for CognitionShellSessionStatusTool {
    fn name(&self) -> &'static str {
        COGNITION_SHELL_SESSION_STATUS
    }
    fn description(&self) -> Option<&'static str> {
        Some(
            "List or create a workshop shell session (PTY). Coding domain only — sessions are shared by Home Terminal tabs and agents.",
        )
    }
    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "work_id": { "type": "string", "description": "Optional Forge work id — session cwd binds to the worktree" },
                "lease_id": { "type": "string", "description": "Forge lease fencing token supplied by the runtime" },
                "lease_generation": { "type": "integer", "description": "Forge lease generation supplied by the runtime" },
                "attempt_id": { "type": "string", "description": "Forge attempt id supplied by the runtime" },
                "create": { "type": "boolean", "description": "Create a session (default: list only)" }
            }
        }))
    }
    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let create = input
            .get("create")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let work_id = input
            .get("work_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty());
        let lease_id = input
            .get("lease_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty());
        let lease_generation = input.get("lease_generation").and_then(Value::as_u64);
        let attempt_id = input.get("attempt_id").and_then(Value::as_str);
        if create {
            daemon_post(
                "/v1/sessions/shell",
                json!({ "work_id": work_id, "lease_id": lease_id, "lease_generation": lease_generation, "attempt_id": attempt_id, "cwd": Value::Null }),
            )
            .await
        } else {
            daemon_get("/v1/sessions/shell").await
        }
    }
}

#[async_trait]
impl StasisTool for CognitionShellSessionRunTool {
    fn name(&self) -> &'static str {
        COGNITION_SHELL_SESSION_RUN
    }
    fn description(&self) -> Option<&'static str> {
        Some(
            "Write a command (or raw input) into a workshop shell session. Streams output for `wait_ms`. Coding domain only.",
        )
    }
    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "session_id": { "type": "string", "description": "Existing session id" },
                "work_id": { "type": "string", "description": "Create/bind a session for this Forge work id" },
                "lease_id": { "type": "string", "description": "Forge lease fencing token supplied by the runtime" },
                "lease_generation": { "type": "integer", "description": "Forge lease generation supplied by the runtime" },
                "attempt_id": { "type": "string", "description": "Forge attempt id supplied by the runtime" },
                "command": { "type": "string", "description": "Command line to run (newline appended)" },
                "input": { "type": "string", "description": "Raw bytes to write (base64 not required)" },
                "wait_ms": { "type": "integer", "description": "How long to stream output (default 1500, max 15000)" }
            }
        }))
    }
    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let session_id = match input
            .get("session_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
        {
            Some(id) => id.to_string(),
            None => {
                let work_id = input
                    .get("work_id")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty());
                let lease_id = input
                    .get("lease_id")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty());
                let lease_generation = input.get("lease_generation").and_then(Value::as_u64);
                let attempt_id = input.get("attempt_id").and_then(Value::as_str);
                let created = daemon_post(
                    "/v1/sessions/shell",
                    json!({ "work_id": work_id, "lease_id": lease_id, "lease_generation": lease_generation, "attempt_id": attempt_id, "cwd": Value::Null }),
                )
                .await?;
                created
                    .get("session_id")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
                    .ok_or_else(|| {
                        StasisError::PortFailure("daemon did not return session_id".into())
                    })?
            }
        };
        let payload = if let Some(cmd) = input.get("command").and_then(|v| v.as_str()) {
            format!("{cmd}\n")
        } else if let Some(raw) = input.get("input").and_then(|v| v.as_str()) {
            raw.to_string()
        } else {
            return Err(StasisError::PortFailure(
                "provide `command` or `input`".into(),
            ));
        };
        let wait_ms = input
            .get("wait_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(1500)
            .clamp(100, 15_000);

        let output = stream_session_input(&session_id, payload.as_bytes(), wait_ms).await?;
        Ok(json!({
            "ok": true,
            "session_id": session_id,
            "output": output,
        }))
    }
}

async fn stream_session_input(
    session_id: &str,
    input: &[u8],
    wait_ms: u64,
) -> StasisResult<String> {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    let base = daemon_base().replacen("http", "ws", 1);
    let url = format!(
        "{}/v1/sessions/shell/{}",
        base.trim_end_matches('/'),
        urlencoding::encode(session_id)
    );
    let (mut ws, _) = tokio_tungstenite::connect_async(&url)
        .await
        .map_err(|e| StasisError::PortFailure(format!("session ws connect: {e}")))?;
    let frame = serde_json::json!({
        "type": "stdin",
        "data": base64::engine::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            input
        )
    })
    .to_string();
    ws.send(Message::Text(frame.into()))
        .await
        .map_err(|e| StasisError::PortFailure(format!("session ws send: {e}")))?;

    let mut output = String::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(wait_ms);
    while std::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        match tokio::time::timeout(remaining, ws.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                if let Ok(v) = serde_json::from_str::<Value>(&text)
                    && v.get("type").and_then(|t| t.as_str()) == Some("stdout")
                    && let Some(data) = v.get("data").and_then(|d| d.as_str())
                    && let Ok(bytes) = base64::engine::Engine::decode(
                        &base64::engine::general_purpose::STANDARD,
                        data,
                    )
                {
                    output.push_str(&String::from_utf8_lossy(&bytes));
                    if output.len() >= MAX_SHELL_OUTPUT_BYTES {
                        truncate_utf8_bytes(&mut output, MAX_SHELL_OUTPUT_BYTES);
                        break;
                    }
                }
            }
            Ok(Some(Ok(Message::Binary(bytes)))) => {
                output.push_str(&String::from_utf8_lossy(&bytes));
                if output.len() >= MAX_SHELL_OUTPUT_BYTES {
                    truncate_utf8_bytes(&mut output, MAX_SHELL_OUTPUT_BYTES);
                    break;
                }
            }
            Ok(Some(Ok(Message::Close(_)))) | Ok(None) => break,
            Ok(Some(Err(_))) => break,
            Ok(Some(Ok(_))) => {}
            Err(_) => break,
        }
    }
    let _ = ws.close(None).await;
    Ok(output)
}

#[async_trait]
impl StasisTool for CognitionShellSessionInterruptTool {
    fn name(&self) -> &'static str {
        COGNITION_SHELL_SESSION_INTERRUPT
    }
    fn description(&self) -> Option<&'static str> {
        Some("Send SIGINT to a workshop shell session. Coding domain only.")
    }
    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "session_id": { "type": "string" }
            },
            "required": ["session_id"]
        }))
    }
    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let session_id = input
            .get("session_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| StasisError::PortFailure("session_id is required".into()))?;
        daemon_post(
            &format!("/v1/sessions/shell/{session_id}/signal"),
            json!({ "signal": "interrupt" }),
        )
        .await
    }
}

#[async_trait]
impl StasisTool for CognitionCoderShellStatusTool {
    fn name(&self) -> &'static str {
        COGNITION_CODER_SHELL_STATUS
    }
    fn description(&self) -> Option<&'static str> {
        Some(
            "Coder-only: report Forge-bound Terminal shell readiness for this undertaking. \
             Prefer cognition_coder_shell_run for one-shot commands.",
        )
    }
    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "work_id": { "type": "string" },
                "lease_id": { "type": "string" },
                "lease_generation": { "type": "integer" },
                "attempt_id": { "type": "string" }
            }
        }))
    }
    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        // Ensure a bound session exists so status reflects the undertaking Terminal.
        let work_id = input
            .get("work_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty());
        let lease_id = input
            .get("lease_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty());
        let lease_generation = input.get("lease_generation").and_then(Value::as_u64);
        let attempt_id = input.get("attempt_id").and_then(Value::as_str);
        let created = daemon_post(
            "/v1/sessions/shell",
            json!({
                "work_id": work_id,
                "lease_id": lease_id,
                "lease_generation": lease_generation,
                "attempt_id": attempt_id,
                "cwd": Value::Null
            }),
        )
        .await?;
        Ok(json!({
            "ok": true,
            "surface": "coder_pty",
            "session_id": created.get("session_id"),
            "session": created,
        }))
    }
}

#[async_trait]
impl StasisTool for CognitionCoderShellRunTool {
    fn name(&self) -> &'static str {
        COGNITION_CODER_SHELL_RUN
    }
    fn description(&self) -> Option<&'static str> {
        Some(
            "Coder one-shot shell: run a command in the Forge-bound undertaking Terminal (PTY). \
             Same ergonomics as a simple shell_run, but cwd/authority follow the active lease worktree. \
             Do not use cognition_shell_run in Coder. For multi-step interactive Terminal work use cognition_shell_session_*.",
        )
    }
    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["command"],
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Shell command line (newline appended)"
                },
                "session_id": {
                    "type": "string",
                    "description": "Reuse a turn-owned session when provided by the runtime"
                },
                "work_id": { "type": "string" },
                "lease_id": { "type": "string" },
                "lease_generation": { "type": "integer" },
                "attempt_id": { "type": "string" },
                "wait_ms": {
                    "type": "integer",
                    "description": "How long to stream output (default 3000, max 15000)"
                }
            }
        }))
    }
    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let command = input
            .get("command")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| StasisError::PortFailure("command is required".into()))?;
        let session_id = match input
            .get("session_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
        {
            Some(id) => id.to_string(),
            None => {
                let work_id = input
                    .get("work_id")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty());
                let lease_id = input
                    .get("lease_id")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty());
                let lease_generation = input.get("lease_generation").and_then(Value::as_u64);
                let attempt_id = input.get("attempt_id").and_then(Value::as_str);
                let created = daemon_post(
                    "/v1/sessions/shell",
                    json!({
                        "work_id": work_id,
                        "lease_id": lease_id,
                        "lease_generation": lease_generation,
                        "attempt_id": attempt_id,
                        "cwd": Value::Null
                    }),
                )
                .await?;
                created
                    .get("session_id")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
                    .ok_or_else(|| {
                        StasisError::PortFailure("daemon did not return session_id".into())
                    })?
            }
        };
        let wait_ms = input
            .get("wait_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(3000)
            .clamp(100, 15_000);
        let output =
            stream_session_input(&session_id, format!("{command}\n").as_bytes(), wait_ms).await?;
        Ok(json!({
            "ok": true,
            "surface": "coder_pty",
            "session_id": session_id,
            "command": command,
            "output": output,
        }))
    }
}

pub fn register_coding_tools(
    registry: &mut stasis::application::orchestration::tool_registry::InMemoryToolRegistry,
) -> stasis::prelude::Result<()> {
    registry.register_tool(CognitionCodeReadTool)?;
    registry.register_tool(CognitionCodeSearchTool)?;
    registry.register_tool(CognitionCodeApplyPatchTool)?;
    registry.register_tool(CognitionShellSessionStatusTool)?;
    registry.register_tool(CognitionShellSessionRunTool)?;
    registry.register_tool(CognitionShellSessionInterruptTool)?;
    registry.register_tool(CognitionCoderShellRunTool)?;
    registry.register_tool(CognitionCoderShellStatusTool)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_fence_rejects_stale_content_and_allows_missing_sentinel() {
        let temp = tempfile::TempDir::new().expect("tempdir");
        let path = temp.path().join("file.txt");
        std::fs::write(&path, "current").expect("write fixture");
        let digest = content_digest(b"current");
        assert_eq!(
            verify_expected_digest(&path, &digest).expect("matching digest"),
            Some(b"current".to_vec())
        );
        assert!(verify_expected_digest(&path, "sha256:stale").is_err());
        assert_eq!(
            verify_expected_digest(&temp.path().join("new.txt"), "missing")
                .expect("missing sentinel"),
            None
        );
    }

    #[test]
    fn code_read_keeps_whole_file_as_the_default() {
        let temp = tempfile::TempDir::new().expect("tempdir");
        let path = temp.path().join("small.rs");
        std::fs::write(&path, "fn one() {}\nfn two() {}\n").expect("write fixture");
        let result = code_read_observation(temp.path(), &path, &json!({})).expect("read");
        assert_eq!(result["ok"], true);
        assert_eq!(result["read_status"], "complete");
        assert_eq!(result["total_lines"], 2);
        assert_eq!(result["content"], "fn one() {}\nfn two() {}\n");
        assert_eq!(result["coverage"]["complete"], true);
    }

    #[test]
    fn oversized_whole_file_returns_actionable_range_orientation() {
        let temp = tempfile::TempDir::new().expect("tempdir");
        let path = temp.path().join("large.rs");
        let content = (1..=20_000)
            .map(|line| format!("pub const VALUE_{line}: usize = {line}; // bounded fixture\n"))
            .collect::<String>();
        assert!(content.len() as u64 > MAX_CODE_READ_BYTES);
        std::fs::write(&path, content).expect("write fixture");

        let result = code_read_observation(temp.path(), &path, &json!({})).expect("orient");
        assert_eq!(result["ok"], true);
        assert_eq!(result["read_status"], "orientation_required");
        assert_eq!(
            result["orientation"]["reason"],
            "file_exceeds_whole_read_budget"
        );
        assert_eq!(result["content"], Value::Null);
        assert!(
            result["digest"]
                .as_str()
                .is_some_and(|digest| digest.starts_with("sha256:"))
        );
        assert!(
            result["orientation"]["suggested_reads"]
                .as_array()
                .is_some_and(|reads| reads.len() >= 2)
        );
        assert_eq!(
            result["orientation"]["suggested_reads"][0]["root"],
            temp.path().display().to_string()
        );

        let ranged = code_read_observation(
            temp.path(),
            &path,
            &json!({ "line_start": 101, "line_end": 103 }),
        )
        .expect("range");
        assert_eq!(ranged["read_status"], "range");
        assert_eq!(ranged["coverage"]["line_start"], 101);
        assert_eq!(ranged["coverage"]["line_end"], 103);
        assert!(
            ranged["content"]
                .as_str()
                .is_some_and(|text| { text.contains("VALUE_101") && text.contains("VALUE_103") })
        );
        assert!(ranged["orientation"]["next_read"].is_object());

        let clamped = code_read_observation(
            temp.path(),
            &path,
            &json!({ "line_start": 1, "line_end": 5_000 }),
        )
        .expect("clamped range");
        assert_eq!(clamped["read_status"], "partial");
        assert!(!clamped["content"].as_str().unwrap_or_default().is_empty());
        assert!(
            clamped["coverage"]["returned_bytes"]
                .as_u64()
                .is_some_and(|bytes| bytes <= MAX_CODE_RANGE_BYTES as u64)
        );
        assert!(clamped["orientation"]["next_read"].is_object());
    }

    #[test]
    fn oversized_line_orients_to_a_byte_range_without_looping() {
        let temp = tempfile::TempDir::new().expect("tempdir");
        let path = temp.path().join("minified.js");
        std::fs::write(&path, "x".repeat(MAX_CODE_RANGE_BYTES + 4_096)).expect("write fixture");
        let result = code_read_observation(
            temp.path(),
            &path,
            &json!({ "line_start": 1, "line_end": 1 }),
        )
        .expect("line orientation");
        assert_eq!(result["read_status"], "partial");
        assert_eq!(
            result["orientation"]["next_read"]["purpose"],
            "continue_within_oversized_line"
        );
        assert!(result["orientation"]["next_read"]["byte_end"].is_number());
        let byte_start = result["orientation"]["next_read"]["byte_start"]
            .as_u64()
            .expect("byte start");
        let byte_end = result["orientation"]["next_read"]["byte_end"]
            .as_u64()
            .expect("byte end");
        assert!(byte_end.saturating_sub(byte_start) <= MAX_CODE_RANGE_BYTES as u64);
    }

    #[test]
    fn non_utf8_file_returns_byte_orientation_instead_of_a_dead_end() {
        let temp = tempfile::TempDir::new().expect("tempdir");
        let path = temp.path().join("binary.dat");
        std::fs::write(&path, [0xff, 0xfe, b'a', b'b']).expect("write fixture");
        let result = code_read_observation(temp.path(), &path, &json!({})).expect("orientation");
        assert_eq!(result["ok"], true);
        assert_eq!(result["read_status"], "orientation_required");
        assert_eq!(result["orientation"]["reason"], "file_is_not_utf8_text");
        let ranged = code_read_observation(
            temp.path(),
            &path,
            &json!({ "byte_start": 0, "byte_end": 4 }),
        )
        .expect("byte range");
        assert_eq!(ranged["read_status"], "range");
        assert_eq!(ranged["encoding"], "utf8_lossy");
        assert_eq!(ranged["coverage"]["byte_start"], 0);
        assert_eq!(ranged["coverage"]["byte_end"], 4);
    }

    #[cfg(unix)]
    #[test]
    fn new_file_cannot_escape_through_a_symlinked_parent() {
        use std::os::unix::fs::symlink;

        let root = tempfile::TempDir::new().expect("root");
        let outside = tempfile::TempDir::new().expect("outside");
        symlink(outside.path(), root.path().join("escape")).expect("symlink");
        let canonical_root = root.path().canonicalize().expect("canonical root");
        let error = resolve_path(&canonical_root, "escape/new.txt").expect_err("escape rejected");
        assert!(error.to_string().contains("escapes root"));
    }
}
