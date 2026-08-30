//! Environment-aware adapters for the existing Medousa tool catalog.
//!
//! The model sees the same tool ids and schemas. When turn admission binds a
//! work environment, workspace and process operations cross the runtime-neutral
//! port; daemon-native memory, web, orchestration, and UI tools remain local.

use std::collections::{BTreeMap, HashMap};
use std::path::{Component, Path};

use medousa_runtime::{
    MAX_WORK_ENVIRONMENT_STDIN_BYTES, WORK_ENVIRONMENT_WORKSPACE_ROOT, WorkEnvironmentBinding,
    WorkEnvironmentExecRequest, WorkEnvironmentExecResult, WorkEnvironmentPhase,
};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use stasis::prelude::{Result as StasisResult, StasisError};

const MAX_CODE_READ_BYTES: u64 = 128 * 1024;
const MAX_CODE_RANGE_BYTES: u64 = 64 * 1024;
const MAX_CODE_RANGE_LINES: u64 = 1_000;
const DEFAULT_CODE_RANGE_LINES: u64 = 200;
const MAX_CODE_WRITE_BYTES: usize = MAX_WORK_ENVIRONMENT_STDIN_BYTES;
const MAX_SEARCH_RESULTS: usize = 500;
const MAX_TOOL_OUTPUT_BYTES: u64 = 1024 * 1024;

#[derive(Clone)]
pub(crate) struct EnvironmentToolInvocation {
    binding: WorkEnvironmentBinding,
    operation_id: String,
}

impl EnvironmentToolInvocation {
    pub(crate) fn new(binding: WorkEnvironmentBinding, operation_id: impl Into<String>) -> Self {
        Self {
            binding,
            operation_id: operation_id.into(),
        }
    }

    pub(crate) fn active(tool_name: &str) -> Option<Self> {
        let context = crate::agent_runtime::execution_context::active_turn_execution_context()?;
        let binding = context.work_environment()?.clone();
        Some(Self::new(
            binding,
            context.next_work_environment_operation_id(tool_name),
        ))
    }

    pub(crate) fn binding(&self) -> &WorkEnvironmentBinding {
        &self.binding
    }

    fn idempotency_key(&self, boundary: &str) -> String {
        format!("{}:{boundary}", self.operation_id)
    }
}

#[derive(Debug)]
pub(crate) struct EnvironmentCodeReadRequest {
    pub path: String,
    pub line_start: Option<u64>,
    pub line_end: Option<u64>,
    pub byte_start: Option<u64>,
    pub byte_end: Option<u64>,
}

#[derive(Debug)]
pub(crate) struct EnvironmentCodeWriteRequest {
    pub path: String,
    pub expected_sha256: String,
    pub content: Option<String>,
    pub find: Option<String>,
    pub replace: Option<String>,
}

fn port_error(error: medousa_runtime::WorkEnvironmentError) -> StasisError {
    StasisError::PortFailure(format!("work environment: {error}"))
}

fn failed_exec(operation: &str, result: &WorkEnvironmentExecResult) -> StasisError {
    let detail = result.stderr.trim();
    let detail = if detail.is_empty() {
        format!("exit code {:?}", result.exit_code)
    } else {
        detail.to_string()
    };
    StasisError::PortFailure(format!("work environment {operation} failed: {detail}"))
}

fn ensure_governed_mutation(invocation: &EnvironmentToolInvocation) -> StasisResult<()> {
    let fence = &invocation.binding.fence;
    if fence.forge_environment_generation.is_none() || fence.forge_execution_generation.is_none() {
        return Err(StasisError::PortFailure(
            "work environment mutation requires both Stasis and Forge fences".to_string(),
        ));
    }
    Ok(())
}

fn normalized_relative_path(raw: &str) -> StasisResult<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(StasisError::PortFailure("path is required".to_string()));
    }
    let path = Path::new(raw);
    if path.is_absolute() {
        return Err(StasisError::PortFailure(
            "work environment paths must be relative to /workspace".to_string(),
        ));
    }
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(StasisError::PortFailure(
                    "work environment path escapes /workspace".to_string(),
                ));
            }
        }
    }
    if parts.is_empty() {
        return Err(StasisError::PortFailure("path is required".to_string()));
    }
    Ok(parts.join("/"))
}

pub(crate) fn workspace_directory(raw: Option<&str>) -> StasisResult<String> {
    let Some(raw) = raw.map(str::trim).filter(|raw| !raw.is_empty()) else {
        return Ok(WORK_ENVIRONMENT_WORKSPACE_ROOT.to_string());
    };
    if raw == WORK_ENVIRONMENT_WORKSPACE_ROOT {
        return Ok(raw.to_string());
    }
    if let Some(relative) = raw.strip_prefix("/workspace/") {
        return normalized_relative_path(relative)
            .map(|relative| format!("{WORK_ENVIRONMENT_WORKSPACE_ROOT}/{relative}"));
    }
    if Path::new(raw).is_absolute() {
        return Err(StasisError::PortFailure(
            "host working directories cannot be used by a bound work environment".to_string(),
        ));
    }
    normalized_relative_path(raw)
        .map(|relative| format!("{WORK_ENVIRONMENT_WORKSPACE_ROOT}/{relative}"))
}

async fn exec(
    invocation: &EnvironmentToolInvocation,
    boundary: &str,
    program: impl Into<String>,
    args: Vec<String>,
    working_directory: String,
    environment: BTreeMap<String, String>,
    stdin: Option<String>,
    timeout_seconds: u64,
    max_output_bytes: u64,
) -> StasisResult<WorkEnvironmentExecResult> {
    invocation
        .binding
        .port
        .exec(
            &invocation.binding.handle,
            WorkEnvironmentExecRequest {
                idempotency_key: invocation.idempotency_key(boundary),
                program: program.into(),
                args,
                working_directory: Some(working_directory),
                environment,
                stdin,
                timeout_seconds: timeout_seconds.clamp(1, 60 * 60),
                max_output_bytes: max_output_bytes.clamp(1, MAX_TOOL_OUTPUT_BYTES),
            },
            &invocation.binding.fence,
        )
        .await
        .map_err(port_error)
}

pub(crate) async fn status(
    invocation: &EnvironmentToolInvocation,
) -> StasisResult<(bool, WorkEnvironmentPhase)> {
    let state = invocation
        .binding
        .port
        .inspect(&invocation.binding.handle)
        .await
        .map_err(port_error)?;
    Ok((state.phase == WorkEnvironmentPhase::Running, state.phase))
}

pub(crate) async fn shell_exec(
    invocation: &EnvironmentToolInvocation,
    program: String,
    args: Vec<String>,
    cwd: Option<&str>,
    stdin: Option<String>,
    timeout_ms: u64,
    max_output_bytes: u64,
) -> StasisResult<WorkEnvironmentExecResult> {
    ensure_governed_mutation(invocation)?;
    let cwd = workspace_directory(cwd)?;
    let environment = BTreeMap::from([
        ("PAGER".to_string(), "cat".to_string()),
        ("GIT_PAGER".to_string(), "cat".to_string()),
        ("SYSTEMD_PAGER".to_string(), "cat".to_string()),
        ("PAGERSECURE".to_string(), "0".to_string()),
        ("LESS".to_string(), "FRX".to_string()),
    ]);
    exec(
        invocation,
        "shell",
        program,
        args,
        cwd,
        environment,
        stdin,
        timeout_ms.saturating_add(999) / 1_000,
        max_output_bytes,
    )
    .await
}

pub(crate) async fn code_read(
    invocation: &EnvironmentToolInvocation,
    request: EnvironmentCodeReadRequest,
) -> StasisResult<Value> {
    let path = normalized_relative_path(&request.path)?;
    let line_range = request.line_start.is_some() || request.line_end.is_some();
    let byte_range = request.byte_start.is_some() || request.byte_end.is_some();
    if line_range && byte_range {
        return Err(StasisError::PortFailure(
            "code.read accepts either a line range or a byte range, not both".to_string(),
        ));
    }

    let (boundary, script, mut args, max_output) = if line_range {
        let start = request.line_start.unwrap_or(1).max(1);
        let requested_end = request
            .line_end
            .unwrap_or_else(|| start.saturating_add(DEFAULT_CODE_RANGE_LINES - 1))
            .max(start);
        let end = requested_end.min(start.saturating_add(MAX_CODE_RANGE_LINES - 1));
        (
            "code-read-lines",
            "sed -n \"${2},${3}p\" \"$1\"",
            vec![
                "sh".to_string(),
                format!("./{path}"),
                start.to_string(),
                end.to_string(),
            ],
            MAX_CODE_RANGE_BYTES,
        )
    } else if byte_range {
        let start = request.byte_start.unwrap_or(0);
        let requested_end = request
            .byte_end
            .unwrap_or_else(|| start.saturating_add(MAX_CODE_RANGE_BYTES))
            .max(start);
        let count = requested_end
            .saturating_sub(start)
            .min(MAX_CODE_RANGE_BYTES);
        (
            "code-read-bytes",
            "dd if=\"$1\" bs=1 skip=\"$2\" count=\"$3\" 2>/dev/null",
            vec![
                "sh".to_string(),
                format!("./{path}"),
                start.to_string(),
                count.to_string(),
            ],
            MAX_CODE_RANGE_BYTES,
        )
    } else {
        (
            "code-read",
            "cat \"$1\"",
            vec!["sh".to_string(), format!("./{path}")],
            MAX_CODE_READ_BYTES,
        )
    };
    args.insert(0, script.to_string());
    args.insert(0, "-c".to_string());
    let result = exec(
        invocation,
        boundary,
        "/bin/sh",
        args,
        WORK_ENVIRONMENT_WORKSPACE_ROOT.to_string(),
        BTreeMap::new(),
        None,
        30,
        max_output,
    )
    .await?;
    if result.exit_code != Some(0) {
        return Err(failed_exec("code read", &result));
    }
    let complete = !result.output_truncated;
    let digest = complete.then(|| format!("sha256:{:x}", Sha256::digest(result.stdout.as_bytes())));
    Ok(json!({
        "ok": true,
        "read_status": if complete { if line_range || byte_range { "range" } else { "complete" } } else { "orientation_required" },
        "path": path,
        "root": WORK_ENVIRONMENT_WORKSPACE_ROOT,
        "bytes": result.stdout.len(),
        "digest": digest,
        "content": complete.then_some(result.stdout),
        "coverage": {
            "complete": complete && !line_range && !byte_range,
            "output_truncated": result.output_truncated,
        },
        "execution_id": result.execution_id,
    }))
}

pub(crate) async fn code_search(
    invocation: &EnvironmentToolInvocation,
    query: &str,
    max_results: Option<u64>,
) -> StasisResult<Value> {
    if query.chars().count() > 512 {
        return Err(StasisError::PortFailure(
            "query exceeds 512 characters".to_string(),
        ));
    }
    let max = max_results
        .unwrap_or(50)
        .clamp(1, MAX_SEARCH_RESULTS as u64) as usize;
    let script = "if command -v rg >/dev/null 2>&1; then rg -n -F --hidden -g '!.git/**' -- \"$1\" .; else find . -type f ! -path './.git/*' -exec grep -H -n -F \"$1\" {} \\;; fi";
    let result = exec(
        invocation,
        "code-search",
        "/bin/sh",
        vec![
            "-c".to_string(),
            script.to_string(),
            "sh".to_string(),
            query.to_string(),
        ],
        WORK_ENVIRONMENT_WORKSPACE_ROOT.to_string(),
        BTreeMap::new(),
        None,
        30,
        MAX_TOOL_OUTPUT_BYTES,
    )
    .await?;
    if !matches!(result.exit_code, Some(0 | 1)) {
        return Err(failed_exec("code search", &result));
    }
    let mut matches: HashMap<String, Vec<usize>> = HashMap::new();
    for line in result.stdout.lines().take(max.saturating_mul(5)) {
        let mut parts = line.splitn(3, ':');
        let Some(path) = parts.next() else { continue };
        let Some(line_number) = parts.next().and_then(|line| line.parse::<usize>().ok()) else {
            continue;
        };
        let path = path.strip_prefix("./").unwrap_or(path).to_string();
        let lines = matches.entry(path).or_default();
        if lines.len() < 5 {
            lines.push(line_number);
        }
        if matches.len() >= max {
            break;
        }
    }
    let mut results = matches
        .into_iter()
        .map(|(path, lines)| json!({ "path": path, "lines": lines }))
        .collect::<Vec<_>>();
    results.sort_by(|left, right| left["path"].as_str().cmp(&right["path"].as_str()));
    Ok(json!({
        "ok": true,
        "root": WORK_ENVIRONMENT_WORKSPACE_ROOT,
        "query": query,
        "results": results,
        "output_truncated": result.output_truncated,
        "execution_id": result.execution_id,
    }))
}

pub(crate) async fn code_write(
    invocation: &EnvironmentToolInvocation,
    request: EnvironmentCodeWriteRequest,
) -> StasisResult<Value> {
    ensure_governed_mutation(invocation)?;
    let path = normalized_relative_path(&request.path)?;
    let expected = request.expected_sha256.trim();
    if expected.is_empty() {
        return Err(StasisError::PortFailure(
            "expected_sha256 is required".to_string(),
        ));
    }
    let read = exec(
        invocation,
        "code-write-read-current",
        "/bin/sh",
        vec![
            "-c".to_string(),
            "if [ -e \"$1\" ]; then cat \"$1\"; else exit 44; fi".to_string(),
            "sh".to_string(),
            format!("./{path}"),
        ],
        WORK_ENVIRONMENT_WORKSPACE_ROOT.to_string(),
        BTreeMap::new(),
        None,
        30,
        MAX_CODE_WRITE_BYTES as u64,
    )
    .await?;
    let existing = match read.exit_code {
        Some(0) if !read.output_truncated => Some(read.stdout),
        Some(44) => None,
        Some(0) => {
            return Err(StasisError::PortFailure(format!(
                "code.write target exceeds {MAX_CODE_WRITE_BYTES} bytes"
            )));
        }
        _ => return Err(failed_exec("code write precondition read", &read)),
    };
    let actual = existing
        .as_ref()
        .map(|content| format!("sha256:{:x}", Sha256::digest(content.as_bytes())))
        .unwrap_or_else(|| "missing".to_string());
    if actual != expected {
        return Err(StasisError::PortFailure(format!(
            "stale file digest for {path}: expected {expected}, found {actual}"
        )));
    }

    let (mode, next) = if let Some(content) = request.content {
        ("write", content)
    } else {
        let find = request.find.ok_or_else(|| {
            StasisError::PortFailure("provide content or find + replace for code.write".to_string())
        })?;
        let replace = request.replace.ok_or_else(|| {
            StasisError::PortFailure("provide content or find + replace for code.write".to_string())
        })?;
        let existing = existing
            .ok_or_else(|| StasisError::PortFailure("cannot patch a missing file".to_string()))?;
        if !existing.contains(&find) {
            return Err(StasisError::PortFailure(
                "find snippet not present in file".to_string(),
            ));
        }
        ("patch", existing.replacen(&find, &replace, 1))
    };
    if next.len() > MAX_CODE_WRITE_BYTES {
        return Err(StasisError::PortFailure(format!(
            "code.write content exceeds {MAX_CODE_WRITE_BYTES} bytes"
        )));
    }
    let result = exec(
        invocation,
        "code-write-commit",
        "/bin/sh",
        vec![
            "-c".to_string(),
            "set -eu; path=$1; dir=${path%/*}; [ \"$dir\" = \"$path\" ] || mkdir -p \"$dir\"; tmp=\"${path}.medousa.$$\"; trap 'rm -f \"$tmp\"' EXIT HUP INT TERM; cat > \"$tmp\"; mv \"$tmp\" \"$path\"; trap - EXIT HUP INT TERM".to_string(),
            "sh".to_string(),
            format!("./{path}"),
        ],
        WORK_ENVIRONMENT_WORKSPACE_ROOT.to_string(),
        BTreeMap::new(),
        Some(next.clone()),
        30,
        64 * 1024,
    )
    .await?;
    if result.exit_code != Some(0) {
        return Err(failed_exec("code write", &result));
    }
    Ok(json!({
        "ok": true,
        "mode": mode,
        "path": path,
        "root": WORK_ENVIRONMENT_WORKSPACE_ROOT,
        "bytes": next.len(),
        "digest": format!("sha256:{:x}", Sha256::digest(next.as_bytes())),
        "execution_id": result.execution_id,
    }))
}

pub(crate) async fn code_intelligence(
    invocation: &EnvironmentToolInvocation,
    operation: &str,
    input: &Value,
) -> StasisResult<Value> {
    let uri = input
        .get("uri")
        .and_then(Value::as_str)
        .ok_or_else(|| StasisError::PortFailure("uri is required".to_string()))?;
    let url = reqwest::Url::parse(uri)
        .map_err(|error| StasisError::PortFailure(format!("invalid code URI: {error}")))?;
    if url.scheme() != "file" || !url.path().starts_with("/workspace/") {
        return Err(StasisError::PortFailure(
            "bound work-environment code intelligence requires a file:///workspace URI".to_string(),
        ));
    }
    let result = exec(
        invocation,
        "code-intelligence",
        "medousa-code",
        vec!["query".to_string(), operation.to_string()],
        WORK_ENVIRONMENT_WORKSPACE_ROOT.to_string(),
        BTreeMap::new(),
        Some(input.to_string()),
        30,
        MAX_TOOL_OUTPUT_BYTES,
    )
    .await?;
    if result.exit_code != Some(0) {
        return Err(failed_exec("code intelligence", &result));
    }
    serde_json::from_str(&result.stdout).map_err(|error| {
        StasisError::PortFailure(format!(
            "work environment medousa-code returned invalid JSON: {error}"
        ))
    })
}

pub(crate) fn pty_unavailable() -> StasisError {
    StasisError::PortFailure(
        "PTY attachment is not available for bound work environments yet; use the one-shot Coder shell"
            .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use chrono::Utc;
    use medousa_runtime::{
        WorkEnvironmentCheckpoint, WorkEnvironmentCheckpointPolicy, WorkEnvironmentError,
        WorkEnvironmentFence, WorkEnvironmentHandle, WorkEnvironmentId, WorkEnvironmentPort,
        WorkEnvironmentPtyHandle, WorkEnvironmentPtyRequest, WorkEnvironmentPublicationResult,
        WorkEnvironmentRetention, WorkEnvironmentSpec, WorkEnvironmentState,
        WorkEnvironmentStopReason,
    };
    use stasis::domain::runtime::resource_lease::FencingToken;

    use super::*;

    #[derive(Default)]
    struct RecordingPort {
        requests: Mutex<Vec<(WorkEnvironmentExecRequest, WorkEnvironmentFence)>>,
        responses: Mutex<VecDeque<WorkEnvironmentExecResult>>,
    }

    impl RecordingPort {
        fn with_responses(responses: Vec<WorkEnvironmentExecResult>) -> Arc<Self> {
            Arc::new(Self {
                requests: Mutex::new(Vec::new()),
                responses: Mutex::new(responses.into()),
            })
        }
    }

    #[async_trait]
    impl WorkEnvironmentPort for RecordingPort {
        async fn materialize(
            &self,
            _spec: WorkEnvironmentSpec,
        ) -> Result<WorkEnvironmentHandle, WorkEnvironmentError> {
            Err(WorkEnvironmentError::Unsupported("test".into()))
        }

        async fn inspect(
            &self,
            handle: &WorkEnvironmentHandle,
        ) -> Result<WorkEnvironmentState, WorkEnvironmentError> {
            Ok(WorkEnvironmentState {
                environment_id: handle.environment_id().clone(),
                phase: WorkEnvironmentPhase::Running,
                checkpoint_ref: None,
                message: None,
                updated_at: Utc::now(),
            })
        }

        async fn start(
            &self,
            handle: &WorkEnvironmentHandle,
            _fence: &WorkEnvironmentFence,
        ) -> Result<WorkEnvironmentState, WorkEnvironmentError> {
            self.inspect(handle).await
        }

        async fn exec(
            &self,
            _handle: &WorkEnvironmentHandle,
            request: WorkEnvironmentExecRequest,
            fence: &WorkEnvironmentFence,
        ) -> Result<WorkEnvironmentExecResult, WorkEnvironmentError> {
            self.requests
                .lock()
                .expect("requests")
                .push((request, fence.clone()));
            self.responses
                .lock()
                .expect("responses")
                .pop_front()
                .ok_or_else(|| WorkEnvironmentError::Adapter("missing test response".into()))
        }

        async fn attach_pty(
            &self,
            _handle: &WorkEnvironmentHandle,
            _request: WorkEnvironmentPtyRequest,
            _fence: &WorkEnvironmentFence,
        ) -> Result<WorkEnvironmentPtyHandle, WorkEnvironmentError> {
            Err(WorkEnvironmentError::Unsupported("test".into()))
        }

        async fn checkpoint(
            &self,
            _handle: &WorkEnvironmentHandle,
            _policy: WorkEnvironmentCheckpointPolicy,
            _fence: &WorkEnvironmentFence,
        ) -> Result<WorkEnvironmentCheckpoint, WorkEnvironmentError> {
            Err(WorkEnvironmentError::Unsupported("test".into()))
        }

        async fn publish(
            &self,
            _handle: &WorkEnvironmentHandle,
            _checkpoint: &WorkEnvironmentCheckpoint,
            _fence: &WorkEnvironmentFence,
        ) -> Result<WorkEnvironmentPublicationResult, WorkEnvironmentError> {
            Err(WorkEnvironmentError::Unsupported("test".into()))
        }

        async fn stop(
            &self,
            handle: &WorkEnvironmentHandle,
            _reason: WorkEnvironmentStopReason,
            _fence: &WorkEnvironmentFence,
        ) -> Result<WorkEnvironmentState, WorkEnvironmentError> {
            self.inspect(handle).await
        }

        async fn release(
            &self,
            handle: &WorkEnvironmentHandle,
            _retention: WorkEnvironmentRetention,
            _fence: &WorkEnvironmentFence,
        ) -> Result<WorkEnvironmentState, WorkEnvironmentError> {
            self.inspect(handle).await
        }

        async fn cleanup(
            &self,
            environment_id: &WorkEnvironmentId,
            _retention: WorkEnvironmentRetention,
            _fence: &WorkEnvironmentFence,
        ) -> Result<WorkEnvironmentState, WorkEnvironmentError> {
            Ok(WorkEnvironmentState {
                environment_id: environment_id.clone(),
                phase: WorkEnvironmentPhase::Released,
                checkpoint_ref: None,
                message: None,
                updated_at: Utc::now(),
            })
        }
    }

    fn result(stdout: &str, exit_code: i32) -> WorkEnvironmentExecResult {
        WorkEnvironmentExecResult {
            execution_id: format!("exec-{exit_code}"),
            exit_code: Some(exit_code),
            stdout: stdout.to_string(),
            stderr: String::new(),
            output_truncated: false,
            started_at: Utc::now(),
            finished_at: Utc::now(),
        }
    }

    fn invocation(port: Arc<RecordingPort>, governed: bool) -> EnvironmentToolInvocation {
        let environment_id = WorkEnvironmentId::parse("phase-3-test").expect("id");
        EnvironmentToolInvocation {
            binding: WorkEnvironmentBinding {
                port,
                handle: WorkEnvironmentHandle::new_local(environment_id, "test-adapter"),
                fence: WorkEnvironmentFence {
                    stasis_attempt: FencingToken(7),
                    forge_environment_generation: governed.then_some(3),
                    forge_execution_generation: governed.then_some(11),
                },
            },
            operation_id: "turn:code-write:1".to_string(),
        }
    }

    #[test]
    fn workspace_paths_never_accept_host_or_parent_fallback() {
        assert_eq!(
            normalized_relative_path("src/lib.rs").unwrap(),
            "src/lib.rs"
        );
        assert!(normalized_relative_path("../host-secret").is_err());
        assert!(normalized_relative_path("/Users/example/repo").is_err());
        assert_eq!(
            workspace_directory(Some("crates/core")).unwrap(),
            "/workspace/crates/core"
        );
        assert!(workspace_directory(Some("/tmp/host-worktree")).is_err());
    }

    #[tokio::test]
    async fn code_write_uses_fenced_environment_stdin_without_host_paths() {
        let port = RecordingPort::with_responses(vec![result("", 44), result("", 0)]);
        let invocation = invocation(port.clone(), true);
        let output = code_write(
            &invocation,
            EnvironmentCodeWriteRequest {
                path: "src/lib.rs".to_string(),
                expected_sha256: "missing".to_string(),
                content: Some("pub fn cooked() {}\n".to_string()),
                find: None,
                replace: None,
            },
        )
        .await
        .expect("environment write");
        assert_eq!(output["ok"], true);
        let requests = port.requests.lock().expect("requests");
        assert_eq!(requests.len(), 2);
        assert_eq!(
            requests[1].0.working_directory.as_deref(),
            Some(WORK_ENVIRONMENT_WORKSPACE_ROOT)
        );
        assert_eq!(requests[1].0.stdin.as_deref(), Some("pub fn cooked() {}\n"));
        assert!(!requests[1].0.args.join(" ").contains("/Users/"));
        assert_eq!(requests[1].1, invocation.binding.fence);
    }

    #[tokio::test]
    async fn environment_mutations_fail_closed_without_forge_fences() {
        let port = RecordingPort::with_responses(Vec::new());
        let invocation = invocation(port.clone(), false);
        let error = shell_exec(
            &invocation,
            "/bin/sh".to_string(),
            vec!["-lc".to_string(), "touch nope".to_string()],
            None,
            None,
            1_000,
            1024,
        )
        .await
        .expect_err("missing Forge fences must fail");
        assert!(error.to_string().contains("Stasis and Forge fences"));
        assert!(port.requests.lock().expect("requests").is_empty());
    }

    #[tokio::test]
    async fn code_intelligence_executes_the_image_adapter_not_the_host_proxy() {
        let port = RecordingPort::with_responses(vec![result(r#"{"symbols":[]}"#, 0)]);
        let invocation = invocation(port.clone(), true);
        let output = code_intelligence(
            &invocation,
            "symbols",
            &json!({ "uri": "file:///workspace/src/lib.rs" }),
        )
        .await
        .expect("environment code intelligence");
        assert_eq!(output["symbols"], json!([]));
        let requests = port.requests.lock().expect("requests");
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].0.program, "medousa-code");
        assert_eq!(requests[0].0.args, ["query", "symbols"]);
        assert!(
            requests[0]
                .0
                .stdin
                .as_deref()
                .is_some_and(|stdin| stdin.contains("file:///workspace/src/lib.rs"))
        );
    }
}
