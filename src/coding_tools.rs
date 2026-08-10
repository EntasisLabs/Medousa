//! Opt-in coding domain cognition tools (Medousa is not a default coding agent).
//!
//! These tools unlock only when a session surface opts in (manuscript / Forge
//! work bind / Settings) — they are never in the default interactive palette.
//! `code_read` / `code_search` / `code_apply_patch` are rooted at the scripts
//! library or an explicit `root` under the workshop; `shell_session_*` drive
//! the workshop-owned PTY sessions on the daemon.

use std::io::{BufRead, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use stasis::prelude::{Result as StasisResult, StasisError};

use crate::typed_tools::{CompatOption, ExternalJson, ToolId, medousa_tool};

pub const COGNITION_CODE_READ: &str = "cognition_code_read";
pub const COGNITION_CODE_SEARCH: &str = "cognition_code_search";
pub const COGNITION_CODE_APPLY_PATCH: &str = "cognition_code_apply_patch";
pub const COGNITION_SHELL_SESSION_STATUS: &str = "cognition_shell_session_status";
pub const COGNITION_SHELL_SESSION_RUN: &str = "cognition_shell_session_run";
pub const COGNITION_SHELL_SESSION_INTERRUPT: &str = "cognition_shell_session_interrupt";
/// One-shot shell for Coder — Forge-bound PTY facade (not OS `cognition_shell_run`).
pub const COGNITION_CODER_SHELL_RUN: &str = "cognition_coder_shell_run";
pub const COGNITION_CODER_SHELL_STATUS: &str = "cognition_coder_shell_status";

const COGNITION_CODE_READ_ID: ToolId = ToolId::new(COGNITION_CODE_READ);
const COGNITION_CODE_SEARCH_ID: ToolId = ToolId::new(COGNITION_CODE_SEARCH);
const COGNITION_CODE_APPLY_PATCH_ID: ToolId = ToolId::new(COGNITION_CODE_APPLY_PATCH);
const COGNITION_SHELL_SESSION_STATUS_ID: ToolId = ToolId::new(COGNITION_SHELL_SESSION_STATUS);
const COGNITION_SHELL_SESSION_RUN_ID: ToolId = ToolId::new(COGNITION_SHELL_SESSION_RUN);
const COGNITION_SHELL_SESSION_INTERRUPT_ID: ToolId = ToolId::new(COGNITION_SHELL_SESSION_INTERRUPT);
const COGNITION_CODER_SHELL_RUN_ID: ToolId = ToolId::new(COGNITION_CODER_SHELL_RUN);
const COGNITION_CODER_SHELL_STATUS_ID: ToolId = ToolId::new(COGNITION_CODER_SHELL_STATUS);

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
    matches!(
        name,
        COGNITION_CODER_SHELL_RUN | COGNITION_CODER_SHELL_STATUS
    )
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
    crate::daemon_self_url::daemon_self_base_url()
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

fn append_shell_output(output: &mut String, bytes: &[u8]) -> bool {
    let chunk = String::from_utf8_lossy(bytes);
    if output.len().saturating_add(chunk.len()) > MAX_SHELL_OUTPUT_BYTES {
        return false;
    }
    output.push_str(&chunk);
    true
}

fn accept_shell_ready_watermark(next_sequence: &mut u64, sequence: u64) {
    // The host is authoritative here: a replacement may restart sequencing.
    *next_sequence = sequence;
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
    decode_daemon_response(resp).await
}

async fn daemon_get(path: &str) -> StasisResult<Value> {
    let client = reqwest::Client::new();
    let url = format!("{}{path}", daemon_base().trim_end_matches('/'));
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| StasisError::PortFailure(format!("daemon session proxy: {e}")))?;
    decode_daemon_response(resp).await
}

async fn decode_daemon_response(response: reqwest::Response) -> StasisResult<Value> {
    let status = response.status();
    let bytes = response.bytes().await.map_err(|error| {
        StasisError::PortFailure(format!("daemon response read failed: {error}"))
    })?;
    decode_daemon_response_bytes(status, &bytes)
}

fn decode_daemon_response_bytes(status: reqwest::StatusCode, bytes: &[u8]) -> StasisResult<Value> {
    if status.is_success() {
        return serde_json::from_slice(bytes).map_err(|error| {
            StasisError::PortFailure(format!("daemon {status} returned invalid JSON: {error}"))
        });
    }

    let detail = serde_json::from_slice::<Value>(bytes)
        .map(|value| value.to_string())
        .unwrap_or_else(|_| String::from_utf8_lossy(bytes).trim().to_string());
    let detail = if detail.is_empty() {
        "empty response body".to_string()
    } else {
        detail
    };
    Err(StasisError::PortFailure(format!(
        "daemon {status}: {detail}"
    )))
}

async fn create_bound_shell_session(
    work_id: Option<&str>,
    lease_id: Option<&str>,
    lease_generation: Option<u64>,
    attempt_id: Option<&str>,
) -> StasisResult<Value> {
    daemon_post(
        "/v1/sessions/shell",
        json!({
            "work_id": work_id.filter(|value| !value.trim().is_empty()),
            "lease_id": lease_id.filter(|value| !value.trim().is_empty()),
            "lease_generation": lease_generation,
            "attempt_id": attempt_id,
            "cwd": Value::Null,
        }),
    )
    .await
}

fn daemon_session_id(response: &Value) -> StasisResult<String> {
    response
        .get("session_id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| StasisError::PortFailure("daemon did not return session_id".into()))
}

fn root_and_path(path: &str, requested_root: Option<&str>) -> StasisResult<(PathBuf, PathBuf)> {
    let root = resolve_root(requested_root)?;
    let resolved = resolve_path(&root, path)?;
    Ok((root, resolved))
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

struct CognitionCodeReadTool;
struct CognitionCodeSearchTool;
struct CognitionCodeApplyPatchTool;
struct CognitionShellSessionStatusTool;
struct CognitionShellSessionRunTool;
struct CognitionShellSessionInterruptTool;
struct CognitionCoderShellRunTool;
struct CognitionCoderShellStatusTool;

#[derive(Debug, Deserialize, JsonSchema)]
struct CodeReadInput {
    /// Absolute or root-relative file path
    path: String,
    /// Optional explicit root (default: scripts library)
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    root: CompatOption<String>,
    /// Optional 1-based inclusive line start
    #[serde(default)]
    #[schemars(
        with = "u64",
        range(min = 1),
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    line_start: CompatOption<u64>,
    /// Optional 1-based inclusive line end
    #[serde(default)]
    #[schemars(
        with = "u64",
        range(min = 1),
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    line_end: CompatOption<u64>,
    /// Optional 0-based byte start; cannot be combined with line ranges
    #[serde(default)]
    #[schemars(
        with = "u64",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    byte_start: CompatOption<u64>,
    /// Optional exclusive byte end; cannot be combined with line ranges
    #[serde(default)]
    #[schemars(
        with = "u64",
        range(min = 1),
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    byte_end: CompatOption<u64>,
}

#[derive(Debug, Clone, Copy, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum CodeReadStatus {
    Complete,
    OrientationRequired,
    Range,
    Partial,
}

#[derive(Debug, Clone, Copy, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum CodeReadEncoding {
    Utf8,
    Utf8Lossy,
}

#[derive(Debug, Serialize, JsonSchema)]
struct CodeReadSuggestion {
    #[serde(skip_serializing_if = "Option::is_none")]
    purpose: Option<String>,
    path: String,
    root: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    line_start: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    line_end: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    byte_start: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    byte_end: Option<u64>,
}

impl CodeReadSuggestion {
    fn byte(path: &Path, root: &Path, byte_start: u64, byte_end: u64) -> Self {
        Self {
            purpose: None,
            path: path.display().to_string(),
            root: root.display().to_string(),
            line: None,
            line_start: None,
            line_end: None,
            byte_start: Some(byte_start),
            byte_end: Some(byte_end),
        }
    }

    fn line(
        purpose: &'static str,
        path: &Path,
        root: &Path,
        line_start: usize,
        line_end: usize,
    ) -> Self {
        Self {
            purpose: Some(purpose.to_string()),
            path: path.display().to_string(),
            root: root.display().to_string(),
            line: None,
            line_start: Some(line_start),
            line_end: Some(line_end),
            byte_start: None,
            byte_end: None,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
struct CodeReadObservedCoverage {
    complete: bool,
    observed_bytes: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_bytes: Option<u64>,
}

#[derive(Debug, Serialize, JsonSchema)]
struct CodeReadCompleteCoverage {
    complete: bool,
    byte_start: usize,
    byte_end: usize,
}

#[derive(Debug, Serialize, JsonSchema)]
struct CodeReadLineRequest {
    line_start: usize,
    line_end: usize,
}

#[derive(Debug, Serialize, JsonSchema)]
struct CodeReadByteRequest {
    byte_start: u64,
    byte_end: u64,
}

#[derive(Debug, Serialize, JsonSchema)]
struct CodeReadLineCoverage {
    complete: bool,
    line_start: Option<usize>,
    line_end: Option<usize>,
    byte_start: Option<u64>,
    byte_end: Option<u64>,
    returned_bytes: usize,
    metadata_complete: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
struct CodeReadByteCoverage {
    complete: bool,
    byte_start: u64,
    byte_end: u64,
    returned_bytes: usize,
}

#[derive(Debug, Serialize, JsonSchema)]
struct CodeReadNonUtf8Orientation {
    reason: String,
    message: String,
    suggested_reads: Vec<CodeReadSuggestion>,
    invalid_utf8_at: usize,
}

#[derive(Debug, Serialize, JsonSchema)]
struct CodeReadOversizedOrientation {
    reason: String,
    message: String,
    whole_read_limit_bytes: u64,
    range_read_limit_bytes: usize,
    max_range_lines: usize,
    metadata_complete: bool,
    suggested_reads: Vec<CodeReadSuggestion>,
}

#[derive(Debug, Serialize, JsonSchema)]
struct CodeReadLineScanOrientation {
    reason: String,
    message: String,
    suggested_reads: Vec<CodeReadSuggestion>,
    metadata_scan_limit_bytes: u64,
}

#[derive(Debug, Serialize, JsonSchema)]
struct CodeReadLineOrientation {
    reason: String,
    next_read: Option<CodeReadSuggestion>,
    max_range_bytes: usize,
    max_range_lines: usize,
    file_ended_with_newline: Option<bool>,
}

#[derive(Debug, Serialize, JsonSchema)]
struct CodeReadByteOrientation {
    reason: String,
    next_read: Option<CodeReadSuggestion>,
    max_range_bytes: usize,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
enum CodeReadOutput {
    NonUtf8 {
        ok: bool,
        read_status: CodeReadStatus,
        path: String,
        root: String,
        bytes: u64,
        digest: String,
        content: Option<String>,
        coverage: CodeReadObservedCoverage,
        orientation: CodeReadNonUtf8Orientation,
    },
    Complete {
        ok: bool,
        read_status: CodeReadStatus,
        path: String,
        root: String,
        bytes: usize,
        total_lines: usize,
        digest: String,
        coverage: CodeReadCompleteCoverage,
        content: String,
    },
    Oversized {
        ok: bool,
        read_status: CodeReadStatus,
        path: String,
        root: String,
        bytes: u64,
        total_lines: Option<usize>,
        digest: Option<String>,
        content: Option<String>,
        coverage: CodeReadObservedCoverage,
        orientation: CodeReadOversizedOrientation,
    },
    LineScanBudget {
        ok: bool,
        read_status: CodeReadStatus,
        path: String,
        root: String,
        bytes: u64,
        total_lines: Option<usize>,
        digest: Option<String>,
        content: Option<String>,
        coverage: CodeReadObservedCoverage,
        orientation: CodeReadLineScanOrientation,
    },
    LineRange {
        ok: bool,
        read_status: CodeReadStatus,
        path: String,
        root: String,
        bytes: u64,
        total_lines: Option<usize>,
        digest: Option<String>,
        requested: CodeReadLineRequest,
        coverage: CodeReadLineCoverage,
        encoding: CodeReadEncoding,
        content: String,
        orientation: CodeReadLineOrientation,
    },
    ByteRange {
        ok: bool,
        read_status: CodeReadStatus,
        path: String,
        root: String,
        bytes: u64,
        total_lines: Option<usize>,
        digest: Option<String>,
        requested: CodeReadByteRequest,
        coverage: CodeReadByteCoverage,
        encoding: CodeReadEncoding,
        content: String,
        orientation: CodeReadByteOrientation,
    },
}

#[medousa_tool(id = COGNITION_CODE_READ_ID)]
impl CognitionCodeReadTool {
    /// Read a whole text file when it fits, or a bounded line/byte range. Oversized whole-file requests return actionable range orientation instead of an opaque failure. Coding domain only.
    async fn invoke_typed(&self, input: CodeReadInput) -> stasis::prelude::Result<CodeReadOutput> {
        let requested_root = input.root.as_ref().cloned();
        let (root, path) = root_and_path(&input.path, requested_root.as_deref())?;
        tokio::task::spawn_blocking(move || code_read_observation(&root, &path, &input))
            .await
            .map_err(|err| StasisError::PortFailure(format!("code_read task failed: {err}")))?
    }
}

fn code_read_observation(
    root: &Path,
    path: &Path,
    input: &CodeReadInput,
) -> StasisResult<CodeReadOutput> {
    let line_start = input.line_start.as_ref().copied();
    let line_end = input.line_end.as_ref().copied();
    let byte_start = input.byte_start.as_ref().copied();
    let byte_end = input.byte_end.as_ref().copied();
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
                return Ok(CodeReadOutput::NonUtf8 {
                    ok: true,
                    read_status: CodeReadStatus::OrientationRequired,
                    path: path.display().to_string(),
                    root: root.display().to_string(),
                    bytes: file_bytes,
                    digest,
                    content: None,
                    coverage: CodeReadObservedCoverage {
                        complete: false,
                        observed_bytes: 0,
                        file_bytes: None,
                    },
                    orientation: CodeReadNonUtf8Orientation {
                        reason: "file_is_not_utf8_text".to_string(),
                        message: "Whole-file text decoding is unavailable. Use a bounded byte range for lossy orientation or a binary-aware domain tool.".to_string(),
                        suggested_reads: vec![CodeReadSuggestion::byte(
                            path,
                            root,
                            0,
                            (MAX_CODE_RANGE_BYTES as u64).min(file_bytes),
                        )],
                        invalid_utf8_at: error.utf8_error().valid_up_to(),
                    },
                });
            }
        };
        return Ok(CodeReadOutput::Complete {
            ok: true,
            read_status: CodeReadStatus::Complete,
            path: path.display().to_string(),
            root: root.display().to_string(),
            bytes: content.len(),
            total_lines: text_line_count(&content),
            digest,
            coverage: CodeReadCompleteCoverage {
                complete: true,
                byte_start: 0,
                byte_end: content.len(),
            },
            content,
        });
    }

    if has_line_range {
        return read_line_range(root, path, file_bytes, line_start, line_end);
    }
    if has_byte_range {
        return read_byte_range(root, path, file_bytes, byte_start, byte_end);
    }

    oversized_file_orientation(root, path, file_bytes)
}

fn oversized_file_orientation(
    root: &Path,
    path: &Path,
    file_bytes: u64,
) -> StasisResult<CodeReadOutput> {
    let metadata = scan_text_metadata(path, file_bytes <= MAX_CODE_ORIENTATION_SCAN_BYTES)?;
    let suggested_reads = if let Some(total_lines) = metadata.total_lines {
        let tail_start = total_lines
            .saturating_sub(DEFAULT_CODE_RANGE_LINES)
            .saturating_add(1)
            .max(1);
        let middle_start = (total_lines / 2)
            .saturating_sub(DEFAULT_CODE_RANGE_LINES / 2)
            .max(1);
        vec![
            CodeReadSuggestion::line(
                "orient_from_file_start",
                path,
                root,
                1,
                DEFAULT_CODE_RANGE_LINES.min(total_lines),
            ),
            CodeReadSuggestion::line(
                "inspect_file_middle",
                path,
                root,
                middle_start,
                middle_start
                    .saturating_add(DEFAULT_CODE_RANGE_LINES - 1)
                    .min(total_lines),
            ),
            CodeReadSuggestion::line("orient_from_file_end", path, root, tail_start, total_lines),
        ]
    } else {
        let tail_start = file_bytes.saturating_sub(MAX_CODE_RANGE_BYTES as u64);
        let mut start =
            CodeReadSuggestion::byte(path, root, 0, (MAX_CODE_RANGE_BYTES as u64).min(file_bytes));
        start.purpose = Some("inspect_file_start".to_string());
        let mut end = CodeReadSuggestion::byte(path, root, tail_start, file_bytes);
        end.purpose = Some("inspect_file_end".to_string());
        vec![start, end]
    };
    Ok(CodeReadOutput::Oversized {
        ok: true,
        read_status: CodeReadStatus::OrientationRequired,
        path: path.display().to_string(),
        root: root.display().to_string(),
        bytes: file_bytes,
        total_lines: metadata.total_lines,
        digest: metadata.digest,
        content: None,
        coverage: CodeReadObservedCoverage {
            complete: false,
            observed_bytes: 0,
            file_bytes: Some(file_bytes),
        },
        orientation: CodeReadOversizedOrientation {
            reason: "file_exceeds_whole_read_budget".to_string(),
            message: "The file is available, but a whole-file response would exceed the model-safe read budget. Continue with one of the suggested line or byte ranges.".to_string(),
            whole_read_limit_bytes: MAX_CODE_READ_BYTES,
            range_read_limit_bytes: MAX_CODE_RANGE_BYTES,
            max_range_lines: MAX_CODE_RANGE_LINES,
            metadata_complete: metadata.complete,
            suggested_reads,
        },
    })
}

fn read_line_range(
    root: &Path,
    path: &Path,
    file_bytes: u64,
    requested_start: Option<u64>,
    requested_end: Option<u64>,
) -> StasisResult<CodeReadOutput> {
    let start = requested_start.unwrap_or(1).max(1) as usize;
    let requested_end = requested_end
        .map(|value| value.max(start as u64) as usize)
        .unwrap_or_else(|| start.saturating_add(DEFAULT_CODE_RANGE_LINES - 1));
    if file_bytes > MAX_CODE_ORIENTATION_SCAN_BYTES {
        return Ok(CodeReadOutput::LineScanBudget {
            ok: true,
            read_status: CodeReadStatus::OrientationRequired,
            path: path.display().to_string(),
            root: root.display().to_string(),
            bytes: file_bytes,
            total_lines: None,
            digest: None,
            content: None,
            coverage: CodeReadObservedCoverage {
                complete: false,
                observed_bytes: 0,
                file_bytes: None,
            },
            orientation: CodeReadLineScanOrientation {
                reason: "line_index_scan_budget_exceeded".to_string(),
                message: "This file is too large for a bounded line-index scan. Continue with byte ranges or search for a narrower anchor.".to_string(),
                suggested_reads: vec![CodeReadSuggestion::byte(
                    path,
                    root,
                    0,
                    (MAX_CODE_RANGE_BYTES as u64).min(file_bytes),
                )],
                metadata_scan_limit_bytes: MAX_CODE_ORIENTATION_SCAN_BYTES,
            },
        });
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
        CodeReadEncoding::Utf8
    } else {
        CodeReadEncoding::Utf8Lossy
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
            let mut suggestion = CodeReadSuggestion::byte(
                path,
                root,
                byte_start,
                byte_start
                    .saturating_add(MAX_CODE_RANGE_BYTES as u64)
                    .min(byte_end),
            );
            suggestion.purpose = Some("continue_within_oversized_line".to_string());
            suggestion.line = Some(line);
            suggestion
        })
        .or_else(|| {
            next_line.map(|line_start| {
                CodeReadSuggestion::line(
                    "continue_by_line",
                    path,
                    root,
                    line_start,
                    line_start.saturating_add(DEFAULT_CODE_RANGE_LINES - 1),
                )
            })
        });
    Ok(CodeReadOutput::LineRange {
        ok: true,
        read_status: if request_clamped || output_limited {
            CodeReadStatus::Partial
        } else {
            CodeReadStatus::Range
        },
        path: path.display().to_string(),
        root: root.display().to_string(),
        bytes: file_bytes,
        total_lines,
        digest,
        requested: CodeReadLineRequest {
            line_start: start,
            line_end: requested_end,
        },
        coverage: CodeReadLineCoverage {
            complete: file_complete,
            line_start: returned_start,
            line_end: returned_end,
            byte_start: returned_byte_start,
            byte_end: returned_byte_end,
            returned_bytes,
            metadata_complete: scan_complete,
        },
        encoding,
        content,
        orientation: CodeReadLineOrientation {
            reason: if request_clamped || output_limited {
                "range_bounded_by_response_budget"
            } else {
                "requested_range_returned"
            }
            .to_string(),
            next_read: continuation,
            max_range_bytes: MAX_CODE_RANGE_BYTES,
            max_range_lines: MAX_CODE_RANGE_LINES,
            file_ended_with_newline: scan_complete.then_some(last_byte == Some(b'\n')),
        },
    })
}

fn read_byte_range(
    root: &Path,
    path: &Path,
    file_bytes: u64,
    requested_start: Option<u64>,
    requested_end: Option<u64>,
) -> StasisResult<CodeReadOutput> {
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
        CodeReadEncoding::Utf8
    } else {
        CodeReadEncoding::Utf8Lossy
    };
    let content = String::from_utf8_lossy(&bytes).into_owned();
    let next_read = (effective_end < file_bytes).then(|| {
        CodeReadSuggestion::byte(
            path,
            root,
            effective_end,
            effective_end
                .saturating_add(MAX_CODE_RANGE_BYTES as u64)
                .min(file_bytes),
        )
    });
    Ok(CodeReadOutput::ByteRange {
        ok: true,
        read_status: if effective_end < requested_end {
            CodeReadStatus::Partial
        } else {
            CodeReadStatus::Range
        },
        path: path.display().to_string(),
        root: root.display().to_string(),
        bytes: file_bytes,
        total_lines: None,
        digest: None,
        requested: CodeReadByteRequest {
            byte_start: start,
            byte_end: requested_end,
        },
        coverage: CodeReadByteCoverage {
            complete: start == 0 && effective_end == file_bytes,
            byte_start: start,
            byte_end: effective_end,
            returned_bytes: bytes.len(),
        },
        encoding,
        content,
        orientation: CodeReadByteOrientation {
            reason: if effective_end < requested_end {
                "range_bounded_by_response_budget"
            } else {
                "requested_range_returned"
            }
            .to_string(),
            next_read,
            max_range_bytes: MAX_CODE_RANGE_BYTES,
        },
    })
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

#[derive(Debug, Deserialize, JsonSchema)]
struct CodeSearchInput {
    query: String,
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    root: CompatOption<String>,
    #[serde(default)]
    #[schemars(
        with = "i64",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    max_results: CompatOption<u64>,
}

#[derive(Debug, Serialize, JsonSchema)]
struct CodeSearchMatch {
    path: String,
    lines: Vec<usize>,
}

#[derive(Debug, Serialize, JsonSchema)]
struct CodeSearchOutput {
    ok: bool,
    root: String,
    query: String,
    results: Vec<CodeSearchMatch>,
}

#[medousa_tool(id = COGNITION_CODE_SEARCH_ID)]
impl CognitionCodeSearchTool {
    /// Search for a substring under the scripts root or a Forge worktree. Coding domain only.
    async fn invoke_typed(
        &self,
        input: CodeSearchInput,
    ) -> stasis::prelude::Result<CodeSearchOutput> {
        if input.query.chars().count() > 512 {
            return Err(StasisError::PortFailure(
                "query exceeds 512 characters".into(),
            ));
        }
        let requested_root = input.root.into_option();
        let root = resolve_root(requested_root.as_deref())?;
        let max = input.max_results.into_option().unwrap_or(50).clamp(1, 500) as usize;

        let mut results = Vec::new();
        let mut scanned = 0usize;
        search_dir(&root, &root, &input.query, max, &mut scanned, &mut results)
            .map_err(|e| StasisError::PortFailure(e.to_string()))?;
        Ok(CodeSearchOutput {
            ok: true,
            root: root.display().to_string(),
            query: input.query,
            results,
        })
    }
}

fn search_dir(
    dir: &Path,
    root: &Path,
    query: &str,
    max: usize,
    scanned: &mut usize,
    out: &mut Vec<CodeSearchMatch>,
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
                out.push(CodeSearchMatch {
                    path: rel.display().to_string(),
                    lines,
                });
                if out.len() >= max || *scanned >= MAX_SCANNED_FILES {
                    return Ok(());
                }
            }
        }
    }
    Ok(())
}

#[derive(Debug, Deserialize, JsonSchema)]
struct CodeApplyPatchInput {
    path: String,
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    root: CompatOption<String>,
    /// Full file content (write)
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    content: CompatOption<String>,
    /// Exact snippet to replace
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    find: CompatOption<String>,
    /// Replacement for `find`
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    replace: CompatOption<String>,
    /// Required current digest from code_read, or `missing` for a new file
    expected_sha256: String,
}

#[derive(Debug, Clone, Copy, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum CodeApplyMode {
    Write,
    Patch,
}

#[derive(Debug, Serialize, JsonSchema)]
struct CodeApplyPatchOutput {
    ok: bool,
    mode: CodeApplyMode,
    path: String,
    root: String,
    bytes: usize,
    digest: String,
}

#[medousa_tool(id = COGNITION_CODE_APPLY_PATCH_ID)]
impl CognitionCodeApplyPatchTool {
    /// Write full content or replace an exact snippet in a file under the session root / Forge worktree. Coding domain only.
    async fn invoke_typed(
        &self,
        input: CodeApplyPatchInput,
    ) -> stasis::prelude::Result<CodeApplyPatchOutput> {
        let requested_root = input.root.into_option();
        let content = input.content.into_option();
        let find = input.find.into_option();
        let replace = input.replace.into_option();
        let (root, path) = root_and_path(&input.path, requested_root.as_deref())?;
        let expected_digest = input.expected_sha256.trim();
        if expected_digest.is_empty() {
            return Err(StasisError::PortFailure(
                "expected_sha256 is required".into(),
            ));
        }
        let existing = verify_expected_digest(&path, expected_digest)?;
        if let Some(content) = content {
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
            tokio::fs::write(&path, &content)
                .await
                .map_err(|e| StasisError::PortFailure(format!("write {}: {e}", path.display())))?;
            return Ok(CodeApplyPatchOutput {
                ok: true,
                mode: CodeApplyMode::Write,
                path: path.display().to_string(),
                root: root.display().to_string(),
                bytes: content.len(),
                digest: content_digest(content.as_bytes()),
            });
        }
        let (find, replace) = match (find, replace) {
            (Some(find), Some(replace)) => (find, replace),
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
        if !existing.contains(&find) {
            return Err(StasisError::PortFailure(
                "find snippet not present in file".into(),
            ));
        }
        let next = existing.replacen(&find, &replace, 1);
        if next.len() > MAX_CODE_WRITE_BYTES {
            return Err(StasisError::PortFailure(format!(
                "patched content exceeds code_apply_patch limit of {MAX_CODE_WRITE_BYTES} bytes"
            )));
        }
        tokio::fs::write(&path, &next)
            .await
            .map_err(|e| StasisError::PortFailure(format!("write {}: {e}", path.display())))?;
        Ok(CodeApplyPatchOutput {
            ok: true,
            mode: CodeApplyMode::Patch,
            path: path.display().to_string(),
            root: root.display().to_string(),
            bytes: next.len(),
            digest: content_digest(next.as_bytes()),
        })
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ShellSessionStatusInput {
    /// Optional Forge work id — session cwd binds to the worktree
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    work_id: CompatOption<String>,
    /// Forge lease fencing token supplied by the runtime
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    lease_id: CompatOption<String>,
    /// Forge lease generation supplied by the runtime
    #[serde(default)]
    #[schemars(
        with = "i64",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    lease_generation: CompatOption<u64>,
    /// Forge attempt id supplied by the runtime
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    attempt_id: CompatOption<String>,
    /// Create a session (default: list only)
    #[serde(default)]
    #[schemars(
        with = "bool",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    create: CompatOption<bool>,
}

#[medousa_tool(id = COGNITION_SHELL_SESSION_STATUS_ID)]
impl CognitionShellSessionStatusTool {
    /// List or create a workshop shell session (PTY). Coding domain only — sessions are shared by Home Terminal tabs and agents.
    async fn invoke_typed(
        &self,
        input: ShellSessionStatusInput,
    ) -> stasis::prelude::Result<ExternalJson> {
        let create = input.create.into_option().unwrap_or(false);
        let work_id = input.work_id.into_option();
        let lease_id = input.lease_id.into_option();
        let lease_generation = input.lease_generation.into_option();
        let attempt_id = input.attempt_id.into_option();
        let output = if create {
            create_bound_shell_session(
                work_id.as_deref(),
                lease_id.as_deref(),
                lease_generation,
                attempt_id.as_deref(),
            )
            .await?
        } else {
            daemon_get("/v1/sessions/shell").await?
        };
        Ok(ExternalJson::new(output))
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ShellSessionRunInput {
    /// Existing session id
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    session_id: CompatOption<String>,
    /// Create/bind a session for this Forge work id
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    work_id: CompatOption<String>,
    /// Forge lease fencing token supplied by the runtime
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    lease_id: CompatOption<String>,
    /// Forge lease generation supplied by the runtime
    #[serde(default)]
    #[schemars(
        with = "i64",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    lease_generation: CompatOption<u64>,
    /// Forge attempt id supplied by the runtime
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    attempt_id: CompatOption<String>,
    /// Command line to run (newline appended)
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    command: CompatOption<String>,
    /// Raw bytes to write (base64 not required)
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    input: CompatOption<String>,
    /// Read pending output without writing more input
    #[serde(default)]
    #[schemars(
        with = "bool",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    poll: CompatOption<bool>,
    /// How long to stream output (default 1500, max 15000)
    #[serde(default)]
    #[schemars(
        with = "i64",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    wait_ms: CompatOption<u64>,
    /// Runtime-managed output cursor; intentionally absent from the model schema
    #[serde(default)]
    #[schemars(skip)]
    after_sequence: CompatOption<u64>,
}

#[derive(Debug, Serialize, JsonSchema)]
struct ShellSessionRunOutput {
    ok: bool,
    session_id: String,
    output: String,
    input_written: bool,
    next_sequence: u64,
    replay_truncated: bool,
    output_truncated: bool,
}

#[medousa_tool(id = COGNITION_SHELL_SESSION_RUN_ID)]
impl CognitionShellSessionRunTool {
    /// Write a command or raw input into a workshop shell session, or set `poll` to read pending output without typing into the PTY. Streams output for `wait_ms`. Coding domain only.
    async fn invoke_typed(
        &self,
        input: ShellSessionRunInput,
    ) -> stasis::prelude::Result<ShellSessionRunOutput> {
        let session_id_input = input.session_id.into_option();
        let work_id = input.work_id.into_option();
        let lease_id = input.lease_id.into_option();
        let lease_generation = input.lease_generation.into_option();
        let attempt_id = input.attempt_id.into_option();
        let command = input.command.into_option();
        let raw_input = input.input.into_option();
        let poll = input.poll.into_option().unwrap_or(false);
        let after_sequence = input.after_sequence.into_option();
        let wait_ms = input
            .wait_ms
            .into_option()
            .unwrap_or(1500)
            .clamp(100, 15_000);
        let session_id = match session_id_input
            .as_deref()
            .filter(|session_id| !session_id.trim().is_empty())
        {
            Some(session_id) => session_id.to_string(),
            None => {
                let created = create_bound_shell_session(
                    work_id.as_deref(),
                    lease_id.as_deref(),
                    lease_generation,
                    attempt_id.as_deref(),
                )
                .await?;
                daemon_session_id(&created)?
            }
        };
        let payload = if let Some(command) = command {
            Some(format!("{command}\n"))
        } else if let Some(raw_input) = raw_input {
            Some(raw_input)
        } else if poll {
            None
        } else {
            return Err(StasisError::PortFailure(
                "provide `command` or `input`, or set `poll` to true".into(),
            ));
        };
        let polling = payload.is_none();
        let stream = stream_session_input(
            &session_id,
            payload.as_deref().map(str::as_bytes),
            wait_ms,
            after_sequence,
        )
        .await?;
        Ok(ShellSessionRunOutput {
            ok: polling || stream.input_written,
            session_id,
            output: stream.output,
            input_written: stream.input_written,
            next_sequence: stream.next_sequence,
            replay_truncated: stream.replay_truncated,
            output_truncated: stream.output_truncated,
        })
    }
}

struct SessionStreamOutput {
    output: String,
    input_written: bool,
    next_sequence: u64,
    replay_truncated: bool,
    output_truncated: bool,
}

async fn stream_session_input(
    session_id: &str,
    input: Option<&[u8]>,
    wait_ms: u64,
    after_sequence: Option<u64>,
) -> StasisResult<SessionStreamOutput> {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    let base = daemon_base().replacen("http", "ws", 1);
    let attach_query = after_sequence.map_or_else(
        || "?replay=tail".to_string(),
        |sequence| format!("?after_sequence={sequence}"),
    );
    let url = format!(
        "{}/v1/sessions/shell/{}{attach_query}",
        base.trim_end_matches('/'),
        urlencoding::encode(session_id)
    );
    let (mut ws, _) = tokio_tungstenite::connect_async(&url)
        .await
        .map_err(|e| StasisError::PortFailure(format!("session ws connect: {e}")))?;
    let mut pending_input = input.map(|input| {
        serde_json::json!({
            "type": "stdin",
            "data": base64::engine::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                input
            )
        })
        .to_string()
    });

    let mut output = String::new();
    let mut input_written = false;
    let mut next_sequence = after_sequence.unwrap_or(0);
    let mut replay_truncated = false;
    let mut output_truncated = false;
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(wait_ms);
    while std::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        match tokio::time::timeout(remaining, ws.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                if let Ok(v) = serde_json::from_str::<Value>(&text) {
                    match v.get("type").and_then(Value::as_str) {
                        Some("stdout") => {
                            if let Some(data) = v.get("data").and_then(Value::as_str)
                                && let Ok(bytes) = base64::engine::Engine::decode(
                                    &base64::engine::general_purpose::STANDARD,
                                    data,
                                )
                            {
                                if !append_shell_output(&mut output, &bytes) {
                                    output_truncated = true;
                                    break;
                                }
                                if let Some(sequence) = v.get("sequence").and_then(Value::as_u64) {
                                    next_sequence = next_sequence.max(sequence);
                                }
                                if output.len() >= MAX_SHELL_OUTPUT_BYTES {
                                    output_truncated = true;
                                    break;
                                }
                            }
                        }
                        Some("ready") => {
                            if let Some(sequence) = v.get("sequence").and_then(Value::as_u64) {
                                accept_shell_ready_watermark(&mut next_sequence, sequence);
                            }
                            replay_truncated |= v
                                .get("replay_truncated")
                                .and_then(Value::as_bool)
                                .unwrap_or(false);
                            if let Some(frame) = pending_input.take() {
                                ws.send(Message::Text(frame.into())).await.map_err(|e| {
                                    StasisError::PortFailure(format!("session ws send: {e}"))
                                })?;
                                input_written = true;
                            }
                        }
                        Some("output_gap") => replay_truncated = true,
                        _ => {}
                    }
                }
            }
            Ok(Some(Ok(Message::Binary(bytes)))) => {
                if !append_shell_output(&mut output, &bytes)
                    || output.len() >= MAX_SHELL_OUTPUT_BYTES
                {
                    output_truncated = true;
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
    Ok(SessionStreamOutput {
        output,
        input_written,
        next_sequence,
        replay_truncated,
        output_truncated,
    })
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ShellSessionInterruptInput {
    session_id: String,
}

#[medousa_tool(id = COGNITION_SHELL_SESSION_INTERRUPT_ID)]
impl CognitionShellSessionInterruptTool {
    /// Send SIGINT to a workshop shell session. Coding domain only.
    async fn invoke_typed(
        &self,
        input: ShellSessionInterruptInput,
    ) -> stasis::prelude::Result<ExternalJson> {
        let session_id = input.session_id.trim();
        if session_id.is_empty() {
            return Err(StasisError::PortFailure("session_id is required".into()));
        }
        let response = daemon_post(
            &format!("/v1/sessions/shell/{session_id}/signal"),
            json!({ "signal": "interrupt" }),
        )
        .await?;
        Ok(ExternalJson::new(response))
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
struct CoderShellStatusInput {
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    work_id: CompatOption<String>,
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    lease_id: CompatOption<String>,
    #[serde(default)]
    #[schemars(
        with = "i64",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    lease_generation: CompatOption<u64>,
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    attempt_id: CompatOption<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
struct CoderShellStatusOutput {
    ok: bool,
    surface: String,
    session_id: Option<String>,
    session: ExternalJson,
}

#[medousa_tool(id = COGNITION_CODER_SHELL_STATUS_ID)]
impl CognitionCoderShellStatusTool {
    /// Coder-only: report Forge-bound Terminal shell readiness for this undertaking. Prefer cognition_coder_shell_run for one-shot commands.
    async fn invoke_typed(
        &self,
        input: CoderShellStatusInput,
    ) -> stasis::prelude::Result<CoderShellStatusOutput> {
        // Ensure a bound session exists so status reflects the undertaking Terminal.
        let work_id = input.work_id.into_option();
        let lease_id = input.lease_id.into_option();
        let lease_generation = input.lease_generation.into_option();
        let attempt_id = input.attempt_id.into_option();
        let created = create_bound_shell_session(
            work_id.as_deref(),
            lease_id.as_deref(),
            lease_generation,
            attempt_id.as_deref(),
        )
        .await?;
        let session_id = created
            .get("session_id")
            .and_then(Value::as_str)
            .map(str::to_string);
        Ok(CoderShellStatusOutput {
            ok: true,
            surface: "coder_pty".to_string(),
            session_id,
            session: ExternalJson::new(created),
        })
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
struct CoderShellRunInput {
    /// Shell command line (newline appended)
    command: String,
    /// Reuse a turn-owned session when provided by the runtime
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    session_id: CompatOption<String>,
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    work_id: CompatOption<String>,
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    lease_id: CompatOption<String>,
    #[serde(default)]
    #[schemars(
        with = "i64",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    lease_generation: CompatOption<u64>,
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    attempt_id: CompatOption<String>,
    /// How long to stream output (default 3000, max 15000)
    #[serde(default)]
    #[schemars(
        with = "i64",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    wait_ms: CompatOption<u64>,
    /// Runtime-managed output cursor; intentionally absent from the model schema
    #[serde(default)]
    #[schemars(skip)]
    after_sequence: CompatOption<u64>,
}

#[derive(Debug, Serialize, JsonSchema)]
struct CoderShellRunOutput {
    ok: bool,
    surface: String,
    session_id: String,
    command: String,
    output: String,
    input_written: bool,
    next_sequence: u64,
    replay_truncated: bool,
    output_truncated: bool,
}

#[medousa_tool(id = COGNITION_CODER_SHELL_RUN_ID)]
impl CognitionCoderShellRunTool {
    /// Coder one-shot shell: run a command in the Forge-bound undertaking Terminal (PTY). Same ergonomics as a simple shell_run, but cwd/authority follow the active lease worktree. Do not use cognition_shell_run in Coder. For multi-step interactive Terminal work use cognition_shell_session_*.
    async fn invoke_typed(
        &self,
        input: CoderShellRunInput,
    ) -> stasis::prelude::Result<CoderShellRunOutput> {
        let command = input.command.trim().to_string();
        if command.is_empty() {
            return Err(StasisError::PortFailure("command is required".into()));
        }
        let session_id_input = input.session_id.into_option();
        let work_id = input.work_id.into_option();
        let lease_id = input.lease_id.into_option();
        let lease_generation = input.lease_generation.into_option();
        let attempt_id = input.attempt_id.into_option();
        let after_sequence = input.after_sequence.into_option();
        let wait_ms = input
            .wait_ms
            .into_option()
            .unwrap_or(3000)
            .clamp(100, 15_000);
        let session_id = match session_id_input
            .as_deref()
            .filter(|session_id| !session_id.trim().is_empty())
        {
            Some(session_id) => session_id.to_string(),
            None => {
                let created = create_bound_shell_session(
                    work_id.as_deref(),
                    lease_id.as_deref(),
                    lease_generation,
                    attempt_id.as_deref(),
                )
                .await?;
                daemon_session_id(&created)?
            }
        };
        let payload = format!("{command}\n");
        let stream = stream_session_input(
            &session_id,
            Some(payload.as_bytes()),
            wait_ms,
            after_sequence,
        )
        .await?;
        Ok(CoderShellRunOutput {
            ok: stream.input_written,
            surface: "coder_pty".to_string(),
            session_id,
            command,
            output: stream.output,
            input_written: stream.input_written,
            next_sequence: stream.next_sequence,
            replay_truncated: stream.replay_truncated,
            output_truncated: stream.output_truncated,
        })
    }
}

pub fn register_coding_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
) -> stasis::prelude::Result<()> {
    registry.register_typed_tool(CognitionCodeReadTool)?;
    registry.register_typed_tool(CognitionCodeSearchTool)?;
    registry.register_typed_tool(CognitionCodeApplyPatchTool)?;
    registry.register_typed_tool(CognitionShellSessionStatusTool)?;
    registry.register_typed_tool(CognitionShellSessionRunTool)?;
    registry.register_typed_tool(CognitionShellSessionInterruptTool)?;
    registry.register_typed_tool(CognitionCoderShellRunTool)?;
    registry.register_typed_tool(CognitionCoderShellStatusTool)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_response_preserves_plain_text_error_details() {
        let error = decode_daemon_response_bytes(
            reqwest::StatusCode::SERVICE_UNAVAILABLE,
            b"incompatible medousa-session: API revision 2; expected 3",
        )
        .expect_err("503 should fail");
        assert_eq!(
            error.to_string(),
            "port failure: daemon 503 Service Unavailable: incompatible medousa-session: API revision 2; expected 3"
        );
    }

    #[test]
    fn daemon_response_decodes_successful_json() {
        let value =
            decode_daemon_response_bytes(reqwest::StatusCode::OK, br#"{"ok":true,"sessions":[]}"#)
                .expect("valid daemon JSON");
        assert_eq!(value, json!({ "ok": true, "sessions": [] }));
    }

    #[test]
    fn shell_output_limit_never_partially_consumes_a_sequence_chunk() {
        let mut output = "x".repeat(MAX_SHELL_OUTPUT_BYTES - 2);
        let before = output.clone();
        assert!(!append_shell_output(&mut output, b"tail"));
        assert_eq!(output, before);
    }

    #[test]
    fn shell_ready_watermark_can_reset_a_stale_cursor() {
        let mut next_sequence = 42;
        accept_shell_ready_watermark(&mut next_sequence, 10);
        assert_eq!(next_sequence, 10);
    }

    #[test]
    fn shell_run_schema_exposes_poll_but_hides_runtime_cursor() {
        let schema = crate::typed_tools::normalize_input_schema::<ShellSessionRunInput>()
            .expect("shell session run schema");
        assert!(schema["properties"].get("poll").is_some());
        assert!(schema["properties"].get("after_sequence").is_none());
    }

    fn code_read_observation(root: &Path, path: &Path, input: &Value) -> StasisResult<Value> {
        let mut wire_input = input.clone();
        wire_input
            .as_object_mut()
            .expect("code read test input object")
            .insert(
                "path".to_string(),
                Value::String(path.display().to_string()),
            );
        let input = serde_json::from_value::<CodeReadInput>(wire_input)
            .expect("valid typed code read input");
        super::code_read_observation(root, path, &input).and_then(|output| {
            serde_json::to_value(output)
                .map_err(|error| StasisError::PortFailure(error.to_string()))
        })
    }

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
    fn code_read_optional_controls_absorb_wrong_wire_types() {
        let input: CodeReadInput = serde_json::from_value(json!({
            "path": "src/lib.rs",
            "root": 7,
            "line_start": "first",
            "line_end": null,
            "byte_start": false,
            "byte_end": -1
        }))
        .expect("compatible code read input");

        assert!(input.root.is_none());
        assert!(input.line_start.is_none());
        assert!(input.line_end.is_none());
        assert!(input.byte_start.is_none());
        assert!(input.byte_end.is_none());
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
