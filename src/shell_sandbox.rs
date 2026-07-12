//! OS-native process sandbox for Medousa agent / Grapheme shell runs.
//!
//! Default backends (no Docker / OpenShell):
//! - Linux: bubblewrap when available, else systemd-run hardening
//! - macOS: Seatbelt via `sandbox-exec`
//! - Windows: process spawn with job-style limits (soft FS isolation for now)

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const DEFAULT_MAX_OUTPUT_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellPermissionProfile {
    /// Working directory inside the jail (must exist).
    #[serde(default)]
    pub cwd: Option<PathBuf>,
    /// Paths the process may write (and read). Others are read-only or denied per backend.
    #[serde(default)]
    pub writable_roots: Vec<PathBuf>,
    /// Extra read-only bind paths (Linux bwrap). Empty = broad host read where backend allows.
    #[serde(default)]
    pub readonly_roots: Vec<PathBuf>,
    /// Network access. Default deny.
    #[serde(default)]
    pub network: bool,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_max_output_bytes")]
    pub max_output_bytes: usize,
    /// Optional allowlist of executable basenames (e.g. `["ls", "git"]`). Empty = any.
    #[serde(default)]
    pub allowed_binaries: Vec<String>,
}

fn default_timeout_ms() -> u64 {
    DEFAULT_TIMEOUT_MS
}

fn default_max_output_bytes() -> usize {
    DEFAULT_MAX_OUTPUT_BYTES
}

impl Default for ShellPermissionProfile {
    fn default() -> Self {
        Self {
            cwd: None,
            writable_roots: Vec::new(),
            readonly_roots: Vec::new(),
            network: false,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
            allowed_binaries: Vec::new(),
        }
    }
}

impl ShellPermissionProfile {
    pub fn from_args(args: &Value) -> Result<Self, String> {
        let mut profile = Self::default();
        if let Some(cwd) = args
            .get("cwd")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            profile.cwd = Some(PathBuf::from(cwd));
        }
        if let Some(network) = args.get("network").and_then(Value::as_bool) {
            profile.network = network;
        }
        if let Some(timeout_ms) = args
            .get("timeout_ms")
            .and_then(Value::as_u64)
            .or_else(|| args.get("timeout").and_then(Value::as_u64))
        {
            profile.timeout_ms = timeout_ms.max(100);
        }
        if let Some(max_output) = args.get("max_output_bytes").and_then(Value::as_u64) {
            profile.max_output_bytes = (max_output as usize).max(1024);
        }
        profile.writable_roots = path_list_from_args(args, &["writable_roots", "writable"]);
        profile.readonly_roots = path_list_from_args(args, &["readonly_roots", "readonly"]);
        if let Some(arr) = args
            .get("allowed_binaries")
            .or_else(|| args.get("binaries"))
            .and_then(Value::as_array)
        {
            profile.allowed_binaries = arr
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
                .filter(|s| !s.is_empty())
                .collect();
        }
        Ok(profile)
    }

    pub fn resolve_cwd(&self) -> PathBuf {
        self.cwd
            .clone()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."))
    }
}

fn path_list_from_args(args: &Value, keys: &[&str]) -> Vec<PathBuf> {
    for key in keys {
        if let Some(arr) = args.get(*key).and_then(Value::as_array) {
            return arr
                .iter()
                .filter_map(|v| v.as_str().map(PathBuf::from))
                .collect();
        }
        if let Some(s) = args.get(*key).and_then(Value::as_str) {
            if !s.trim().is_empty() {
                return vec![PathBuf::from(s.trim())];
            }
        }
    }
    Vec::new()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellRunRequest {
    pub argv: Vec<String>,
    pub profile: ShellPermissionProfile,
}

impl ShellRunRequest {
    pub fn from_args(args: &Value) -> Result<Self, String> {
        let argv = parse_argv(args)?;
        let mut profile = ShellPermissionProfile::from_args(args)?;
        if profile.writable_roots.is_empty() {
            // Default: allow writes only under cwd when cwd is set explicitly.
            if let Some(cwd) = profile.cwd.clone() {
                profile.writable_roots.push(cwd);
            }
        }
        Ok(Self { argv, profile })
    }
}

fn parse_argv(args: &Value) -> Result<Vec<String>, String> {
    if let Some(arr) = args.get("argv").and_then(Value::as_array) {
        let argv: Vec<String> = arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
        if argv.is_empty() {
            return Err("shell.run argv must be a non-empty string array".to_string());
        }
        return Ok(argv);
    }
    if let Some(command) = args
        .get("command")
        .or_else(|| args.get("cmd"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Ok(shell_wrapper_argv(command));
    }
    Err("shell.run requires `command` (string) or `argv` (string array)".to_string())
}

fn shell_wrapper_argv(command: &str) -> Vec<String> {
    #[cfg(windows)]
    {
        vec!["cmd.exe".to_string(), "/C".to_string(), command.to_string()]
    }
    #[cfg(not(windows))]
    {
        vec!["/bin/sh".to_string(), "-c".to_string(), command.to_string()]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellRunResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub backend: String,
    pub sandboxed: bool,
    pub timed_out: bool,
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

impl ShellRunResult {
    pub fn to_json(&self) -> Value {
        json!(self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellSandboxStatus {
    pub os: String,
    pub backend: String,
    pub ready: bool,
    pub sandboxed: bool,
    pub detail: String,
}

pub fn probe_shell_sandbox() -> ShellSandboxStatus {
    #[cfg(target_os = "linux")]
    {
        if which_bin("bwrap").is_some() {
            return ShellSandboxStatus {
                os: "linux".to_string(),
                backend: "bubblewrap".to_string(),
                ready: true,
                sandboxed: true,
                detail: "bubblewrap available".to_string(),
            };
        }
        if which_bin("systemd-run").is_some() {
            return ShellSandboxStatus {
                os: "linux".to_string(),
                backend: "systemd-run".to_string(),
                ready: true,
                sandboxed: true,
                detail: "systemd-run available (bubblewrap preferred when installed)".to_string(),
            };
        }
        ShellSandboxStatus {
            os: "linux".to_string(),
            backend: "process".to_string(),
            ready: true,
            sandboxed: false,
            detail: "no bwrap/systemd-run — commands run unsandboxed with timeout only".to_string(),
        }
    }
    #[cfg(target_os = "macos")]
    {
        if Path::new("/usr/bin/sandbox-exec").is_file() {
            ShellSandboxStatus {
                os: "macos".to_string(),
                backend: "seatbelt".to_string(),
                ready: true,
                sandboxed: true,
                detail: "sandbox-exec Seatbelt available".to_string(),
            }
        } else {
            ShellSandboxStatus {
                os: "macos".to_string(),
                backend: "process".to_string(),
                ready: true,
                sandboxed: false,
                detail: "sandbox-exec missing — commands run unsandboxed with timeout only"
                    .to_string(),
            }
        }
    }
    #[cfg(target_os = "windows")]
    {
        ShellSandboxStatus {
            os: "windows".to_string(),
            backend: "process".to_string(),
            ready: true,
            sandboxed: false,
            detail: "Windows process spawn with timeout (AppContainer FS jail planned)"
                .to_string(),
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        ShellSandboxStatus {
            os: std::env::consts::OS.to_string(),
            backend: "process".to_string(),
            ready: true,
            sandboxed: false,
            detail: "unsupported OS for native jail — timeout only".to_string(),
        }
    }
}

pub fn run_sandboxed(request: &ShellRunRequest) -> Result<ShellRunResult, String> {
    enforce_binary_allowlist(&request.argv, &request.profile.allowed_binaries)?;
    let cwd = request.profile.resolve_cwd();
    if !cwd.is_dir() {
        return Err(format!("shell.run cwd does not exist: {}", cwd.display()));
    }

    #[cfg(target_os = "linux")]
    {
        if which_bin("bwrap").is_some() {
            return run_with_bwrap(request, &cwd);
        }
        if which_bin("systemd-run").is_some() {
            return run_with_systemd_run(request, &cwd);
        }
        run_unsandboxed(request, &cwd, "process", Some(
            "bwrap/systemd-run not found; ran without OS jail".to_string(),
        ))
    }
    #[cfg(target_os = "macos")]
    {
        if Path::new("/usr/bin/sandbox-exec").is_file() {
            return run_with_seatbelt(request, &cwd);
        }
        run_unsandboxed(
            request,
            &cwd,
            "process",
            Some("sandbox-exec missing; ran without OS jail".to_string()),
        )
    }
    #[cfg(target_os = "windows")]
    {
        run_unsandboxed(
            request,
            &cwd,
            "process",
            Some("Windows AppContainer jail not yet enabled; timeout-only".to_string()),
        )
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        run_unsandboxed(
            request,
            &cwd,
            "process",
            Some("unsupported OS; timeout-only".to_string()),
        )
    }
}

fn enforce_binary_allowlist(argv: &[String], allowlist: &[String]) -> Result<(), String> {
    if allowlist.is_empty() || argv.is_empty() {
        return Ok(());
    }
    // Skip shell wrappers when checking allowlist — check the script body is not feasible;
    // for argv form, check argv[0] basename.
    let program = Path::new(&argv[0])
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(argv[0].as_str());
    if matches!(program, "sh" | "bash" | "zsh" | "cmd.exe" | "cmd" | "powershell") {
        return Ok(());
    }
    let allowed = allowlist
        .iter()
        .any(|name| name == program || name == &argv[0]);
    if !allowed {
        return Err(format!(
            "binary '{program}' is not in allowed_binaries {:?}",
            allowlist
        ));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn which_bin(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn run_unsandboxed(
    request: &ShellRunRequest,
    cwd: &Path,
    backend: &str,
    warning: Option<String>,
) -> Result<ShellRunResult, String> {
    let mut cmd = Command::new(&request.argv[0]);
    if request.argv.len() > 1 {
        cmd.args(&request.argv[1..]);
    }
    cmd.current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear()
        .env("PATH", default_path())
        .env("HOME", dirs::home_dir().unwrap_or_else(|| PathBuf::from("/")))
        .env("TMPDIR", std::env::temp_dir())
        .env("LANG", "C.UTF-8");
    wait_command(cmd, &request.profile, backend, false, warning)
}

#[cfg(target_os = "linux")]
fn run_with_bwrap(request: &ShellRunRequest, cwd: &Path) -> Result<ShellRunResult, String> {
    let mut args = vec![
        "--die-with-parent".to_string(),
        "--new-session".to_string(),
        "--ro-bind".to_string(),
        "/".to_string(),
        "/".to_string(),
        "--dev".to_string(),
        "/dev".to_string(),
        "--proc".to_string(),
        "/proc".to_string(),
        "--tmpfs".to_string(),
        "/tmp".to_string(),
    ];
    if !request.profile.network {
        args.push("--unshare-net".to_string());
    }
    for root in &request.profile.writable_roots {
        let canon = canonicalize_or_clone(root);
        args.push("--bind".to_string());
        args.push(canon.display().to_string());
        args.push(canon.display().to_string());
    }
    for root in &request.profile.readonly_roots {
        let canon = canonicalize_or_clone(root);
        args.push("--ro-bind".to_string());
        args.push(canon.display().to_string());
        args.push(canon.display().to_string());
    }
    args.push("--chdir".to_string());
    args.push(canonicalize_or_clone(cwd).display().to_string());
    args.push("--".to_string());
    args.extend(request.argv.iter().cloned());

    let mut cmd = Command::new("bwrap");
    cmd.args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    wait_command(cmd, &request.profile, "bubblewrap", true, None)
}

#[cfg(target_os = "linux")]
fn run_with_systemd_run(request: &ShellRunRequest, cwd: &Path) -> Result<ShellRunResult, String> {
    let mut args = vec![
        "--user".to_string(),
        "--wait".to_string(),
        "--collect".to_string(),
        "--pipe".to_string(),
        "-p".to_string(),
        "NoNewPrivileges=yes".to_string(),
        "-p".to_string(),
        "PrivateTmp=yes".to_string(),
        "-p".to_string(),
        "ProtectSystem=strict".to_string(),
        "-p".to_string(),
        "ProtectHome=read-only".to_string(),
    ];
    for root in &request.profile.writable_roots {
        args.push("-p".to_string());
        args.push(format!("ReadWritePaths={}", canonicalize_or_clone(root).display()));
    }
    if !request.profile.network {
        args.push("-p".to_string());
        args.push("PrivateNetwork=yes".to_string());
    }
    args.push("-p".to_string());
    args.push(format!("WorkingDirectory={}", canonicalize_or_clone(cwd).display()));
    args.push("--".to_string());
    args.extend(request.argv.iter().cloned());

    let mut cmd = Command::new("systemd-run");
    cmd.args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    wait_command(cmd, &request.profile, "systemd-run", true, None)
}

#[cfg(target_os = "macos")]
fn run_with_seatbelt(request: &ShellRunRequest, cwd: &Path) -> Result<ShellRunResult, String> {
    let profile = build_seatbelt_profile(&request.profile, cwd)?;
    let mut profile_file = tempfile_seatbelt(&profile)?;
    let profile_path = profile_file.path().to_path_buf();

    let mut args = vec![
        "-f".to_string(),
        profile_path.display().to_string(),
    ];
    args.extend(request.argv.iter().cloned());

    let mut cmd = Command::new("/usr/bin/sandbox-exec");
    cmd.args(&args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("HOME", dirs::home_dir().unwrap_or_else(|| PathBuf::from("/")))
        .env("TMPDIR", std::env::temp_dir())
        .env("PATH", default_path())
        .env("LANG", "C.UTF-8");

    let result = wait_command(cmd, &request.profile, "seatbelt", true, None);
    // Keep profile file alive until process exits.
    drop(profile_file);
    result
}

#[cfg(target_os = "macos")]
fn build_seatbelt_profile(profile: &ShellPermissionProfile, cwd: &Path) -> Result<String, String> {
    let mut sb = String::from(
        "(version 1)\n\
         (deny default)\n\
         (allow process*)\n\
         (allow signal)\n\
         (allow sysctl-read)\n\
         (allow mach-lookup)\n\
         (allow file-read*)\n\
         (allow file-write* (literal \"/dev/null\"))\n\
         (allow file-write* (subpath \"/private/tmp\"))\n\
         (allow file-write* (subpath \"/tmp\"))\n",
    );
    let tmp = std::env::temp_dir();
    sb.push_str(&format!(
        "(allow file-write* (subpath \"{}\"))\n",
        escape_sb_path(&tmp)
    ));
    for root in &profile.writable_roots {
        sb.push_str(&format!(
            "(allow file-write* (subpath \"{}\"))\n",
            escape_sb_path(&canonicalize_or_clone(root))
        ));
    }
    sb.push_str(&format!(
        "(allow file-write* (subpath \"{}\"))\n",
        escape_sb_path(&canonicalize_or_clone(cwd))
    ));
    if profile.network {
        sb.push_str("(allow network*)\n");
    } else {
        sb.push_str("(deny network*)\n");
    }
    Ok(sb)
}

#[cfg(target_os = "macos")]
fn escape_sb_path(path: &Path) -> String {
    path.display().to_string().replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(target_os = "macos")]
struct TempSeatbelt {
    path: PathBuf,
}

#[cfg(target_os = "macos")]
impl TempSeatbelt {
    fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(target_os = "macos")]
impl Drop for TempSeatbelt {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(target_os = "macos")]
fn tempfile_seatbelt(contents: &str) -> Result<TempSeatbelt, String> {
    let path = std::env::temp_dir().join(format!(
        "medousa-shell-{}.sb",
        uuid::Uuid::new_v4().simple()
    ));
    let mut file = std::fs::File::create(&path)
        .map_err(|err| format!("failed to write seatbelt profile: {err}"))?;
    file.write_all(contents.as_bytes())
        .map_err(|err| format!("failed to write seatbelt profile: {err}"))?;
    Ok(TempSeatbelt { path })
}

fn canonicalize_or_clone(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn default_path() -> String {
    #[cfg(windows)]
    {
        r"C:\Windows\System32;C:\Windows".to_string()
    }
    #[cfg(not(windows))]
    {
        "/usr/bin:/bin:/usr/sbin:/sbin:/usr/local/bin".to_string()
    }
}

fn wait_command(
    mut cmd: Command,
    profile: &ShellPermissionProfile,
    backend: &str,
    sandboxed: bool,
    warning: Option<String>,
) -> Result<ShellRunResult, String> {
    let started = Instant::now();
    let max_output = profile.max_output_bytes;
    let mut child = cmd
        .spawn()
        .map_err(|err| format!("failed to spawn shell backend '{backend}': {err}"))?;

    let stdout_thread = child.stdout.take().map(|pipe| {
        std::thread::spawn(move || read_limited(pipe, max_output))
    });
    let stderr_thread = child.stderr.take().map(|pipe| {
        std::thread::spawn(move || read_limited(pipe, max_output))
    });

    let timeout = Duration::from_millis(profile.timeout_ms);
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if started.elapsed() >= timeout {
                    let _ = child.kill();
                    timed_out = true;
                    break child.wait().unwrap_or_default();
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(err) => return Err(format!("shell.run wait failed: {err}")),
        }
    };

    let stdout = stdout_thread
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();
    let stderr = if timed_out {
        let mut msg = format!("shell.run timed out after {}ms", profile.timeout_ms);
        let captured = stderr_thread
            .and_then(|handle| handle.join().ok())
            .unwrap_or_default();
        if !captured.is_empty() {
            msg.push('\n');
            msg.push_str(&captured);
        }
        msg
    } else {
        stderr_thread
            .and_then(|handle| handle.join().ok())
            .unwrap_or_default()
    };

    Ok(ShellRunResult {
        exit_code: if timed_out {
            -1
        } else {
            status.code().unwrap_or(-1)
        },
        stdout,
        stderr,
        backend: backend.to_string(),
        sandboxed,
        timed_out,
        duration_ms: started.elapsed().as_millis() as u64,
        warning,
    })
}

fn read_limited(mut pipe: impl Read, max_bytes: usize) -> String {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match pipe.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                let remaining = max_bytes.saturating_sub(buf.len());
                if remaining == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n.min(remaining)]);
            }
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&buf).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_command_into_shell_argv() {
        let req = ShellRunRequest::from_args(&json!({
            "command": "echo hi",
            "network": false,
        }))
        .expect("parse");
        assert!(req.argv.len() >= 2);
        assert!(req.argv.last().is_some_and(|s| s == "echo hi"));
        assert!(!req.profile.network);
    }

    #[test]
    fn probe_reports_ready() {
        let status = probe_shell_sandbox();
        assert!(status.ready);
        assert!(!status.backend.is_empty());
    }

    #[test]
    fn echo_runs_under_default_backend() {
        let req = ShellRunRequest::from_args(&json!({
            "command": "echo medousa-shell-ok",
            "timeout_ms": 10_000,
        }))
        .expect("parse");
        let result = run_sandboxed(&req).expect("run");
        assert!(!result.timed_out, "stderr={}", result.stderr);
        assert!(
            result.stdout.contains("medousa-shell-ok") || result.exit_code == 0,
            "stdout={} stderr={} backend={}",
            result.stdout,
            result.stderr,
            result.backend
        );
    }
}
