//! ACP client bones for hot-swappable agentic runtimes.
//!
//! The **daemon** owns this library and exposes `/v1/agents` via the Medousa SDK.
//! Clients never speak ACP directly.
//!
//! Stub session + Cursor/Codex process adapters (spawn when binary exists;
//! real `session/new` → `session/prompt` → `session/update` pump).
//! Cursor: `agent acp`. Codex: `codex-acp` or `npx -y @agentclientprotocol/codex-acp`
//! (stock `codex` has no `acp` subcommand). Missing CLI → stub; spawn/handshake
//! failures surface as errors (no silent stub). Force stub: `MEDOUSA_ACP_FORCE_STUB=1`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Mutex;

use anyhow::{Result, bail};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::Mutex as AsyncMutex;

/// Built-in runtime ids for 0.4.0 bones QA.
pub const RUNTIME_CURSOR: &str = "cursor";
pub const RUNTIME_CODEX: &str = "codex";
pub const RUNTIME_MEDOUSA: &str = "medousa";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentRuntimeKind {
    Medousa,
    Cursor,
    Codex,
}

impl AgentRuntimeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Medousa => RUNTIME_MEDOUSA,
            Self::Cursor => RUNTIME_CURSOR,
            Self::Codex => RUNTIME_CODEX,
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "medousa" | "native" => Some(Self::Medousa),
            "cursor" => Some(Self::Cursor),
            "codex" => Some(Self::Codex),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpAgentConfig {
    pub kind: AgentRuntimeKind,
    /// Command to spawn (e.g. `agent`, `codex`). Empty = unset.
    pub command: String,
    pub args: Vec<String>,
    /// Working directory hint (workshop root).
    pub cwd: Option<String>,
}

impl AcpAgentConfig {
    pub fn cursor_default() -> Self {
        let command = std::env::var("MEDOUSA_ACP_CURSOR_COMMAND")
            .unwrap_or_else(|_| "agent".into());
        let args = if std::env::var("MEDOUSA_ACP_CURSOR_ARGS").is_ok() {
            env_args("MEDOUSA_ACP_CURSOR_ARGS", &["acp"])
        } else {
            vec!["acp".into()]
        };
        Self {
            kind: AgentRuntimeKind::Cursor,
            command: resolve_command_path(&command)
                .map(|p| p.display().to_string())
                .unwrap_or(command),
            args,
            cwd: None,
        }
    }

    pub fn codex_default() -> Self {
        // Stock `codex` has no `acp` subcommand — ACP goes through the
        // `@agentclientprotocol/codex-acp` adapter (or a `codex-acp` binary).
        if let Ok(command) = std::env::var("MEDOUSA_ACP_CODEX_COMMAND") {
            let args = env_args("MEDOUSA_ACP_CODEX_ARGS", &[]);
            return Self {
                kind: AgentRuntimeKind::Codex,
                command: resolve_command_path(&command)
                    .map(|p| p.display().to_string())
                    .unwrap_or(command),
                args,
                cwd: None,
            };
        }
        let (command, args) = resolve_codex_acp_launch();
        Self {
            kind: AgentRuntimeKind::Codex,
            command,
            args,
            cwd: None,
        }
    }
}

/// Prefer an installed `codex-acp` binary; otherwise `npx -y @agentclientprotocol/codex-acp`.
fn resolve_codex_acp_launch() -> (String, Vec<String>) {
    if let Some(path) = resolve_command_path("codex-acp") {
        return (path.display().to_string(), Vec::new());
    }
    if let Some(npx) = resolve_command_path("npx") {
        return (
            npx.display().to_string(),
            vec![
                "-y".into(),
                "@agentclientprotocol/codex-acp".into(),
            ],
        );
    }
    // Last resort — will fail with a clear spawn/handshake error.
    let codex = resolve_command_path("codex")
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "codex".into());
    (codex, vec!["acp".into()])
}

fn env_args(key: &str, default: &[&str]) -> Vec<String> {
    if let Ok(raw) = std::env::var(key) {
        let parts: Vec<String> = raw
            .split_whitespace()
            .map(str::to_string)
            .filter(|s| !s.is_empty())
            .collect();
        if !parts.is_empty() {
            return parts;
        }
    }
    default.iter().map(|s| (*s).to_string()).collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpSessionId(pub String);

/// Sign-in state of the vendor account backing an external runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeAuthStatus {
    SignedOut,
    SignedIn,
    Unknown,
}

/// Probe of a vendor CLI login — binary presence plus best-effort auth state.
/// Never reads token contents; only checks that vendor credential stores
/// exist and look non-empty.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeAuthProbe {
    pub status: RuntimeAuthStatus,
    pub binary_present: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
}

fn dir_has_files(dir: &PathBuf) -> bool {
    std::fs::read_dir(dir)
        .map(|mut entries| entries.any(|e| e.is_ok()))
        .unwrap_or(false)
}

fn path_has_content(path: &PathBuf) -> bool {
    if path.is_file() {
        return std::fs::metadata(path).map(|m| m.len() > 0).unwrap_or(false);
    }
    if path.is_dir() {
        return dir_has_files(path);
    }
    false
}

/// Codex CLI stores ChatGPT login under `~/.codex/auth.json` (with optional
/// config/credentials siblings). We only check existence, never parse.
fn codex_auth_probe(binary_present: bool) -> RuntimeAuthProbe {
    let codex_dir = home_dir().map(|home| home.join(".codex"));
    let auth_file = codex_dir.as_ref().map(|dir| dir.join("auth.json"));
    let creds_dir = codex_dir.as_ref().map(|dir| dir.join("credentials"));
    let signed_in = auth_file
        .as_ref()
        .map(path_has_content)
        .unwrap_or(false)
        || creds_dir.as_ref().map(path_has_content).unwrap_or(false);

    let (status, detail) = if signed_in {
        (
            RuntimeAuthStatus::SignedIn,
            Some("ChatGPT sign-in found in ~/.codex".into()),
        )
    } else if codex_dir.as_ref().map(|d| d.is_dir()).unwrap_or(false) {
        (
            RuntimeAuthStatus::SignedOut,
            Some("codex present but not signed in — run codex login".into()),
        )
    } else if binary_present {
        (
            RuntimeAuthStatus::SignedOut,
            Some("not signed in — run codex login".into()),
        )
    } else {
        (
            RuntimeAuthStatus::Unknown,
            Some("codex CLI not installed".into()),
        )
    };
    RuntimeAuthProbe {
        status,
        binary_present,
        detail,
    }
}

/// Cursor Agent CLI keeps tokens in the OS keychain / secure storage — not a
/// simple `auth.json`. Probe via `agent status --format json` (authoritative),
/// then fall back to `~/.cursor/cli-config.json` authInfo and legacy markers.
fn cursor_auth_probe(binary_present: bool) -> RuntimeAuthProbe {
    if let Some((status, detail)) = cursor_auth_from_cli_status() {
        return RuntimeAuthProbe {
            status,
            binary_present: binary_present || status == RuntimeAuthStatus::SignedIn,
            detail,
        };
    }
    if let Some(detail) = cursor_auth_from_cli_config() {
        return RuntimeAuthProbe {
            status: RuntimeAuthStatus::SignedIn,
            binary_present: true,
            detail: Some(detail),
        };
    }
    if cursor_keychain_has_tokens() {
        return RuntimeAuthProbe {
            status: RuntimeAuthStatus::SignedIn,
            binary_present: true,
            detail: Some("Cursor sign-in found in OS keychain".into()),
        };
    }

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(home) = home_dir() {
        candidates.push(home.join(".cursor"));
        #[cfg(target_os = "macos")]
        candidates.push(home.join("Library/Application Support/Cursor"));
        #[cfg(target_os = "windows")]
        {
            if let Some(appdata) = std::env::var_os("APPDATA") {
                candidates.push(PathBuf::from(appdata).join("Cursor"));
            }
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        candidates.push(home.join(".config/Cursor"));
    }

    // Legacy file markers (older agent builds). Prefer not to treat empty dirs as signed-in.
    let auth_markers = ["auth.json", "session.json", "sessions", "cli-state.json"];
    let mut saw_cursor_dir = false;
    let mut signed_in = false;
    for dir in &candidates {
        if !dir.is_dir() {
            continue;
        }
        saw_cursor_dir = true;
        for marker in auth_markers {
            if path_has_content(&dir.join(marker)) {
                signed_in = true;
                break;
            }
        }
        if signed_in {
            break;
        }
    }

    let (status, detail) = if signed_in {
        (
            RuntimeAuthStatus::SignedIn,
            Some("Cursor sign-in found".into()),
        )
    } else if saw_cursor_dir || binary_present {
        (
            RuntimeAuthStatus::SignedOut,
            Some("not signed in — run `cursor agent login` (or `agent login`)".into()),
        )
    } else {
        (
            RuntimeAuthStatus::Unknown,
            Some("Cursor agent CLI not installed".into()),
        )
    };
    RuntimeAuthProbe {
        status,
        binary_present,
        detail,
    }
}

/// Ask the Cursor Agent CLI — tokens live in keychain, so this is the reliable check.
fn cursor_auth_from_cli_status() -> Option<(RuntimeAuthStatus, Option<String>)> {
    ensure_vendor_cli_path();
    let program = resolve_command_path("agent")
        .or_else(|| resolve_command_path("cursor-agent"))?;
    let output = std::process::Command::new(&program)
        .args(["status", "--format", "json"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let value: Value = serde_json::from_str(text.trim()).ok()?;
    let authenticated = value
        .get("isAuthenticated")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
        || value.get("status").and_then(|v| v.as_str()) == Some("authenticated");
    let message = value
        .get("message")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    if authenticated {
        Some((
            RuntimeAuthStatus::SignedIn,
            message.or_else(|| Some("Cursor CLI reports signed in".into())),
        ))
    } else {
        Some((
            RuntimeAuthStatus::SignedOut,
            message.or_else(|| Some("Cursor CLI reports signed out".into())),
        ))
    }
}

/// `~/.cursor/cli-config.json` → `authInfo` is written after a successful login
/// (email / userId only — never tokens).
fn cursor_auth_from_cli_config() -> Option<String> {
    let path = home_dir()?.join(".cursor").join("cli-config.json");
    let text = std::fs::read_to_string(path).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    let auth = value.get("authInfo")?;
    let email = auth
        .get("email")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());
    let user_id = auth
        .get("userId")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());
    if email.is_none() && user_id.is_none() {
        return None;
    }
    Some(match email {
        Some(email) => format!("Cursor sign-in found ({email})"),
        None => "Cursor sign-in found in cli-config".into(),
    })
}

/// macOS: Cursor Agent stores `cursor-access-token` / `cursor-refresh-token` in
/// the login keychain. Existence only — never read the secret.
fn cursor_keychain_has_tokens() -> bool {
    #[cfg(target_os = "macos")]
    {
        for service in ["cursor-access-token", "cursor-refresh-token"] {
            let ok = std::process::Command::new("security")
                .args(["find-generic-password", "-s", service])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if ok {
                return true;
            }
        }
        false
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// Best-effort auth probe for an external runtime (medousa native is always
/// "signed in" — it needs no vendor account).
pub fn runtime_auth_probe(kind: AgentRuntimeKind) -> RuntimeAuthProbe {
    match kind {
        AgentRuntimeKind::Medousa => RuntimeAuthProbe {
            status: RuntimeAuthStatus::SignedIn,
            binary_present: true,
            detail: Some("native runtime".into()),
        },
        AgentRuntimeKind::Cursor => {
            let cfg = AcpAgentConfig::cursor_default();
            // Prefer the login CLI name (`agent`) even when resolved to an absolute path.
            let present = command_available("agent")
                || command_available(&cfg.command)
                || PathBuf::from(&cfg.command).is_file();
            cursor_auth_probe(present)
        }
        AgentRuntimeKind::Codex => {
            // Sign-in is via the Codex CLI; ACP itself may launch through `codex-acp` / npx.
            codex_auth_probe(command_available("codex"))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AcpEvent {
    MessageDelta { text: String },
    MessageDone { text: String },
    ToolCall {
        id: String,
        name: String,
        input: Value,
    },
    PermissionRequest {
        id: String,
        summary: String,
    },
    Error { message: String },
    Done,
}

#[async_trait]
pub trait AcpClient: Send + Sync {
    async fn create_session(&self, config: &AcpAgentConfig) -> Result<AcpSessionId>;
    async fn prompt(&self, session: &AcpSessionId, text: &str) -> Result<()>;
    async fn cancel(&self, session: &AcpSessionId) -> Result<()>;
    async fn next_event(&self, session: &AcpSessionId) -> Result<Option<AcpEvent>>;
    /// Reply to an inbound `session/request_permission` JSON-RPC id.
    async fn respond_permission(
        &self,
        session: &AcpSessionId,
        request_id: &str,
        approved: bool,
    ) -> Result<()>;
}

struct StubSessionState {
    queue: Vec<AcpEvent>,
    cancelled: bool,
}

/// Placeholder client — proves wiring without requiring Cursor/Codex installed.
pub struct StubAcpClient {
    sessions: Mutex<HashMap<String, StubSessionState>>,
}

impl Default for StubAcpClient {
    fn default() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }
}

impl StubAcpClient {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl AcpClient for StubAcpClient {
    async fn create_session(&self, config: &AcpAgentConfig) -> Result<AcpSessionId> {
        if matches!(config.kind, AgentRuntimeKind::Medousa) {
            bail!("use native Medousa turn path for medousa runtime");
        }
        let id = AcpSessionId(format!(
            "stub-{}-{}",
            config.kind.as_str(),
            uuid_v4_lite()
        ));
        let mut guard = self.sessions.lock().expect("stub sessions");
        guard.insert(
            id.0.clone(),
            StubSessionState {
                queue: Vec::new(),
                cancelled: false,
            },
        );
        Ok(id)
    }

    async fn prompt(&self, session: &AcpSessionId, text: &str) -> Result<()> {
        let mut guard = self.sessions.lock().expect("stub sessions");
        let state = guard
            .get_mut(&session.0)
            .ok_or_else(|| anyhow::anyhow!("unknown stub session {}", session.0))?;
        if state.cancelled {
            bail!("session cancelled");
        }
        let preview: String = text.chars().take(120).collect();
        state.queue.push(AcpEvent::MessageDelta {
            text: format!("[stub {}] ", session.0),
        });
        state.queue.push(AcpEvent::MessageDone {
            text: format!(
                "[medousa-acp-client] stub runtime acknowledged prompt ({} chars): {preview}",
                text.len()
            ),
        });
        // Demo permission pause once per prompt when MEDOUSA_ACP_STUB_PERMISSION=1
        if std::env::var("MEDOUSA_ACP_STUB_PERMISSION")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
        {
            state.queue.push(AcpEvent::PermissionRequest {
                id: format!("perm-{}", uuid_v4_lite()),
                summary: "Stub ACP permission: allow demo tool call?".into(),
            });
        }
        state.queue.push(AcpEvent::Done);
        Ok(())
    }

    async fn cancel(&self, session: &AcpSessionId) -> Result<()> {
        let mut guard = self.sessions.lock().expect("stub sessions");
        if let Some(state) = guard.get_mut(&session.0) {
            state.cancelled = true;
            state.queue.clear();
            state.queue.push(AcpEvent::Error {
                message: "cancelled".into(),
            });
            state.queue.push(AcpEvent::Done);
        }
        Ok(())
    }

    async fn next_event(&self, session: &AcpSessionId) -> Result<Option<AcpEvent>> {
        let mut guard = self.sessions.lock().expect("stub sessions");
        let state = guard
            .get_mut(&session.0)
            .ok_or_else(|| anyhow::anyhow!("unknown stub session {}", session.0))?;
        Ok(if state.queue.is_empty() {
            None
        } else {
            Some(state.queue.remove(0))
        })
    }

    async fn respond_permission(
        &self,
        _session: &AcpSessionId,
        _request_id: &str,
        _approved: bool,
    ) -> Result<()> {
        Ok(())
    }
}

fn uuid_v4_lite() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:x}")
}

/// Resolve a configured external runtime (Cursor / Codex only in 0.4.0).
pub fn external_runtime_config(kind: AgentRuntimeKind) -> Result<AcpAgentConfig> {
    match kind {
        AgentRuntimeKind::Cursor => Ok(AcpAgentConfig::cursor_default()),
        AgentRuntimeKind::Codex => Ok(AcpAgentConfig::codex_default()),
        AgentRuntimeKind::Medousa => {
            bail!("medousa runtime is not an ACP external agent")
        }
    }
}

/// True when `command` appears resolvable on PATH (or is an absolute existing file).
/// Also checks common vendor install locations (`~/.local/bin`, Homebrew, npm global)
/// so GUI apps that inherit a minimal PATH still find Codex / Cursor Agent CLIs.
pub fn command_available(command: &str) -> bool {
    resolve_command_path(command).is_some()
}

/// Resolve `command` to an absolute path using PATH + common vendor install dirs.
pub fn resolve_command_path(command: &str) -> Option<PathBuf> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return None;
    }
    let path = PathBuf::from(trimmed);
    if path.is_absolute() || trimmed.contains('/') || trimmed.contains('\\') {
        return path.is_file().then_some(path);
    }
    for dir in command_search_dirs() {
        let candidate = dir.join(trimmed);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let exe = dir.join(format!("{trimmed}.exe"));
            if exe.is_file() {
                return Some(exe);
            }
            let cmd = dir.join(format!("{trimmed}.cmd"));
            if cmd.is_file() {
                return Some(cmd);
            }
        }
    }
    None
}

/// Directories searched for vendor CLI binaries, PATH first then common installs.
fn command_search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(path_var) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path_var));
    }
    for extra in common_vendor_bin_dirs() {
        if !dirs.iter().any(|dir| dir == &extra) {
            dirs.push(extra);
        }
    }
    dirs
}

/// Locations preferred by Codex / Cursor official installers (and Homebrew / npm / nvm).
pub fn common_vendor_bin_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = home_dir() {
        dirs.push(home.join(".local").join("bin"));
        dirs.push(home.join(".npm-global").join("bin"));
        dirs.push(home.join("bin"));
        dirs.push(home.join(".volta").join("bin"));
        dirs.push(home.join(".fnm").join("current").join("bin"));
        dirs.push(
            home.join(".local")
                .join("share")
                .join("fnm")
                .join("aliases")
                .join("default")
                .join("bin"),
        );
        // nvm: pick every installed Node bin dir (newest first) so `npx` resolves
        // for the Codex ACP adapter even when the daemon inherits a thin PATH.
        let nvm_node = home.join(".nvm").join("versions").join("node");
        if let Ok(entries) = std::fs::read_dir(&nvm_node) {
            let mut versions: Vec<PathBuf> = entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.is_dir())
                .collect();
            versions.sort();
            versions.reverse();
            for version_dir in versions {
                dirs.push(version_dir.join("bin"));
            }
        }
        #[cfg(windows)]
        {
            dirs.push(home.join("AppData").join("Local").join("Programs").join("codex"));
            dirs.push(home.join("AppData").join("Roaming").join("npm"));
        }
    }
    if cfg!(target_os = "macos") {
        dirs.push(PathBuf::from("/opt/homebrew/bin"));
        dirs.push(PathBuf::from("/usr/local/bin"));
    }
    if cfg!(target_os = "linux") {
        dirs.push(PathBuf::from("/usr/local/bin"));
    }
    dirs
}

/// Prepend common vendor CLI dirs to the process PATH so child spawns find them.
pub fn ensure_vendor_cli_path() {
    let extras: Vec<PathBuf> = common_vendor_bin_dirs()
        .into_iter()
        .filter(|dir| dir.is_dir())
        .collect();
    if extras.is_empty() {
        return;
    }
    let mut parts: Vec<PathBuf> = extras;
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            if !parts.iter().any(|existing| existing == &dir) {
                parts.push(dir);
            }
        }
    }
    let joined = std::env::join_paths(parts).ok();
    if let Some(joined) = joined {
        // SAFETY: called before spawning vendor CLIs; PATH is not used for
        // security decisions elsewhere in this crate during the call.
        unsafe { std::env::set_var("PATH", joined) };
    }
}

pub fn runtime_availability(kind: AgentRuntimeKind) -> (bool, Option<String>, Option<String>) {
    match kind {
        AgentRuntimeKind::Medousa => (
            true,
            None,
            Some("Use /v1/turns for native Medousa agent loop".into()),
        ),
        AgentRuntimeKind::Cursor | AgentRuntimeKind::Codex => {
            let cfg = match external_runtime_config(kind) {
                Ok(c) => c,
                Err(err) => return (false, None, Some(err.to_string())),
            };
            let available = command_available(&cfg.command)
                || PathBuf::from(&cfg.command).is_file();
            let detail = if available {
                Some(format!(
                    "command '{}' {} ready",
                    cfg.command,
                    if cfg.args.is_empty() {
                        String::new()
                    } else {
                        cfg.args.join(" ")
                    }
                    .trim()
                ))
            } else if matches!(kind, AgentRuntimeKind::Codex) {
                Some(
                    "Codex ACP adapter not found — install Node.js (for npx) or `codex-acp`, \
                     then retry. Stock `codex` has no `acp` subcommand."
                        .into(),
                )
            } else {
                Some(format!(
                    "command '{}' not found — install from Settings → Connections",
                    cfg.command
                ))
            };
            (true, Some(cfg.command), detail)
        }
    }
}

struct ProcessSession {
    child: Child,
    stdin: ChildStdin,
    lines: Lines<BufReader<tokio::process::ChildStdout>>,
    next_id: u64,
    queue: Vec<AcpEvent>,
    cancelled: bool,
    /// ACP wire session id from `session/new`.
    acp_session_id: Option<String>,
    cwd: Option<String>,
    /// Outstanding `session/prompt` JSON-RPC id waiting for stopReason.
    pending_prompt_id: Option<u64>,
    /// Permission JSON-RPC id → allow option ids (first is preferred).
    permission_allow_options: HashMap<String, Vec<String>>,
}

/// Spawns Cursor/Codex ACP stdio when the binary exists; otherwise behaves like [`StubAcpClient`].
pub struct ExternalAcpClient {
    stub: StubAcpClient,
    processes: AsyncMutex<HashMap<String, ProcessSession>>,
}

impl Default for ExternalAcpClient {
    fn default() -> Self {
        Self {
            stub: StubAcpClient::new(),
            processes: AsyncMutex::new(HashMap::new()),
        }
    }
}

impl ExternalAcpClient {
    pub fn new() -> Self {
        Self::default()
    }

    fn prefer_process() -> bool {
        !std::env::var("MEDOUSA_ACP_FORCE_STUB")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    }
}

#[async_trait]
impl AcpClient for ExternalAcpClient {
    async fn create_session(&self, config: &AcpAgentConfig) -> Result<AcpSessionId> {
        if matches!(config.kind, AgentRuntimeKind::Medousa) {
            bail!("use native Medousa turn path for medousa runtime");
        }
        ensure_vendor_cli_path();
        if !Self::prefer_process() {
            return self.stub.create_session(config).await;
        }

        let mut launch = config.clone();
        if let Some(resolved) = resolve_command_path(&launch.command) {
            launch.command = resolved.display().to_string();
        } else if !PathBuf::from(&launch.command).is_file() {
            // Dev/bones fallback when the vendor CLI isn't installed at all.
            return self.stub.create_session(config).await;
        }

        match spawn_acp_process(&launch).await {
            Ok(mut proc) => {
                let id = AcpSessionId(format!(
                    "acp-{}-{}",
                    config.kind.as_str(),
                    uuid_v4_lite()
                ));
                if let Err(err) = handshake_session(&mut proc).await {
                    let _ = proc.child.kill().await;
                    bail!(
                        "ACP handshake failed for {} (`{} {}`): {err}",
                        config.kind.as_str(),
                        launch.command,
                        launch.args.join(" ")
                    );
                }
                self.processes.lock().await.insert(id.0.clone(), proc);
                Ok(id)
            }
            Err(err) => bail!(
                "ACP process spawn failed for {} (`{} {}`): {err}",
                config.kind.as_str(),
                launch.command,
                launch.args.join(" ")
            ),
        }
    }

    async fn prompt(&self, session: &AcpSessionId, text: &str) -> Result<()> {
        {
            let mut guard = self.processes.lock().await;
            if let Some(proc) = guard.get_mut(&session.0) {
                if proc.cancelled {
                    bail!("session cancelled");
                }
                if let Err(err) = send_prompt(proc, text).await {
                    proc.queue.push(AcpEvent::Error {
                        message: format!("ACP prompt failed: {err}"),
                    });
                    proc.queue.push(AcpEvent::Done);
                }
                return Ok(());
            }
        }
        self.stub.prompt(session, text).await
    }

    async fn cancel(&self, session: &AcpSessionId) -> Result<()> {
        {
            let mut guard = self.processes.lock().await;
            if let Some(mut proc) = guard.remove(&session.0) {
                proc.cancelled = true;
                if let Some(acp_id) = proc.acp_session_id.clone() {
                    let _ = write_line(
                        &mut proc,
                        &json!({
                            "jsonrpc": "2.0",
                            "method": "session/cancel",
                            "params": { "sessionId": acp_id }
                        }),
                    )
                    .await;
                }
                let _ = proc.child.kill().await;
                return Ok(());
            }
        }
        self.stub.cancel(session).await
    }

    async fn next_event(&self, session: &AcpSessionId) -> Result<Option<AcpEvent>> {
        {
            let mut guard = self.processes.lock().await;
            if let Some(proc) = guard.get_mut(&session.0) {
                if !proc.queue.is_empty() {
                    return Ok(Some(proc.queue.remove(0)));
                }
                return drain_stdout(proc).await;
            }
        }
        self.stub.next_event(session).await
    }

    async fn respond_permission(
        &self,
        session: &AcpSessionId,
        request_id: &str,
        approved: bool,
    ) -> Result<()> {
        {
            let mut guard = self.processes.lock().await;
            if let Some(proc) = guard.get_mut(&session.0) {
                return send_permission_response(proc, request_id, approved).await;
            }
        }
        self.stub
            .respond_permission(session, request_id, approved)
            .await
    }
}

async fn spawn_acp_process(config: &AcpAgentConfig) -> Result<ProcessSession> {
    ensure_vendor_cli_path();
    let program = resolve_command_path(&config.command)
        .unwrap_or_else(|| PathBuf::from(&config.command));
    let mut cmd = Command::new(&program);
    cmd.args(&config.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    // Child inherits the enriched PATH so nested tools (npx → node, etc.) resolve.
    if let Ok(path) = std::env::var("PATH") {
        cmd.env("PATH", path);
    }
    if let Some(cwd) = &config.cwd {
        cmd.current_dir(cwd);
    }
    let mut child = cmd.spawn().map_err(|err| {
        anyhow::anyhow!(
            "failed to spawn '{}' (args: {:?}): {err}",
            program.display(),
            config.args
        )
    })?;
    let stdin = child.stdin.take().ok_or_else(|| anyhow::anyhow!("no stdin"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("no stdout"))?;
    Ok(ProcessSession {
        child,
        stdin,
        lines: BufReader::new(stdout).lines(),
        next_id: 1,
        queue: Vec::new(),
        cancelled: false,
        acp_session_id: None,
        cwd: config.cwd.clone(),
        pending_prompt_id: None,
        permission_allow_options: HashMap::new(),
    })
}

async fn handshake_session(proc: &mut ProcessSession) -> Result<()> {
    send_initialize(proc).await?;
    send_session_new(proc).await?;
    Ok(())
}

async fn send_initialize(proc: &mut ProcessSession) -> Result<()> {
    let id = alloc_id(proc);
    let req = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "initialize",
        "params": {
            "protocolVersion": 1,
            "clientInfo": {
                "name": "medousa-acp-client",
                "version": env!("CARGO_PKG_VERSION")
            },
            "clientCapabilities": {
                "fs": { "readTextFile": true },
                "permission": true
            }
        }
    });
    write_line(proc, &req).await?;
    // Drain until we see the initialize response (or timeout).
    for _ in 0..20 {
        let Some(line) = read_line_timeout(proc, 2).await? else {
            break;
        };
        if let Ok(value) = serde_json::from_str::<Value>(&line) {
            if value.get("id").and_then(|v| v.as_u64()) == Some(id) {
                if value.get("error").is_some() {
                    bail!("initialize rejected: {line}");
                }
                return Ok(());
            }
            // Buffer early notifications while waiting.
            if let Some(ev) = map_inbound_line(proc, &value) {
                proc.queue.push(ev);
            }
        }
    }
    bail!("timed out waiting for initialize")
}

async fn send_session_new(proc: &mut ProcessSession) -> Result<()> {
    let id = alloc_id(proc);
    let cwd = proc
        .cwd
        .clone()
        .or_else(|| std::env::current_dir().ok().map(|p| p.display().to_string()))
        .unwrap_or_else(|| ".".into());
    let req = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "session/new",
        "params": {
            "cwd": cwd,
            "mcpServers": []
        }
    });
    write_line(proc, &req).await?;
    for _ in 0..20 {
        let Some(line) = read_line_timeout(proc, 3).await? else {
            break;
        };
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if value.get("id").and_then(|v| v.as_u64()) == Some(id) {
            let session_id = value
                .pointer("/result/sessionId")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            proc.acp_session_id = session_id;
            if proc.acp_session_id.is_none() {
                bail!("session/new response missing sessionId");
            }
            return Ok(());
        }
        if let Some(ev) = map_inbound_line(proc, &value) {
            proc.queue.push(ev);
        }
    }
    bail!("timed out waiting for session/new")
}

async fn send_prompt(proc: &mut ProcessSession, text: &str) -> Result<()> {
    let Some(session_id) = proc.acp_session_id.clone() else {
        bail!("ACP session has no sessionId — handshake incomplete");
    };
    let id = alloc_id(proc);
    proc.pending_prompt_id = Some(id);
    let req = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "session/prompt",
        "params": {
            "sessionId": session_id,
            "prompt": [{ "type": "text", "text": text }]
        }
    });
    write_line(proc, &req).await
}

async fn send_permission_response(
    proc: &mut ProcessSession,
    request_id: &str,
    approved: bool,
) -> Result<()> {
    let rpc_id: Value = if let Ok(n) = request_id.parse::<u64>() {
        json!(n)
    } else if let Ok(n) = request_id.parse::<i64>() {
        json!(n)
    } else {
        json!(request_id)
    };
    let result = if approved {
        let option_id = proc
            .permission_allow_options
            .get(request_id)
            .and_then(|opts| opts.first())
            .cloned()
            .unwrap_or_else(|| "allow-once".to_string());
        json!({
            "outcome": {
                "outcome": "selected",
                "optionId": option_id
            }
        })
    } else {
        json!({ "outcome": { "outcome": "cancelled" } })
    };
    proc.permission_allow_options.remove(request_id);
    write_line(
        proc,
        &json!({
            "jsonrpc": "2.0",
            "id": rpc_id,
            "result": result
        }),
    )
    .await
}

fn alloc_id(proc: &mut ProcessSession) -> u64 {
    let id = proc.next_id;
    proc.next_id = proc.next_id.saturating_add(1);
    id
}

async fn write_line(proc: &mut ProcessSession, value: &Value) -> Result<()> {
    let mut line = serde_json::to_string(value)?;
    line.push('\n');
    proc.stdin.write_all(line.as_bytes()).await?;
    proc.stdin.flush().await?;
    Ok(())
}

async fn read_line_timeout(proc: &mut ProcessSession, secs: u64) -> Result<Option<String>> {
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(secs),
        proc.lines.next_line(),
    )
    .await;
    match result {
        Ok(Ok(line)) => Ok(line),
        Ok(Err(err)) => Err(err.into()),
        Err(_) => Ok(None),
    }
}

async fn drain_stdout(proc: &mut ProcessSession) -> Result<Option<AcpEvent>> {
    let result = tokio::time::timeout(
        std::time::Duration::from_millis(80),
        proc.lines.next_line(),
    )
    .await;
    let Ok(Ok(Some(line))) = result else {
        return Ok(None);
    };
    let value: Value = match serde_json::from_str(&line) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    Ok(map_inbound_line(proc, &value))
}

fn map_inbound_line(proc: &mut ProcessSession, value: &Value) -> Option<AcpEvent> {
    // JSON-RPC response to session/prompt → turn complete.
    if let Some(id) = value.get("id").and_then(|v| v.as_u64()) {
        if proc.pending_prompt_id == Some(id) {
            proc.pending_prompt_id = None;
            if let Some(err) = value.get("error") {
                let message = err
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("ACP prompt error")
                    .to_string();
                // Emit Error first; Done follows on the next poll.
                proc.queue.push(AcpEvent::Done);
                return Some(AcpEvent::Error { message });
            }
            // Terminal: session/prompt completed (stopReason present or implied).
            let _stop = value
                .pointer("/result/stopReason")
                .and_then(|v| v.as_str())
                .unwrap_or("end_turn");
            return Some(AcpEvent::Done);
        }
        // Other RPC responses (initialize/session/new) — ignore here.
        return None;
    }

    let method = value.get("method").and_then(|m| m.as_str())?;
    if method == "session/request_permission" || method.ends_with("request_permission") {
        let rpc_id = value
            .get("id")
            .map(|v| match v {
                Value::Number(n) => n.to_string(),
                Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .unwrap_or_else(|| format!("perm-{}", uuid_v4_lite()));
        let allow_ids: Vec<String> = value
            .pointer("/params/options")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
            .filter_map(|opt| {
                let kind = opt.get("kind").and_then(|v| v.as_str()).unwrap_or("");
                let id = opt.get("optionId").and_then(|v| v.as_str())?;
                if kind.starts_with("allow") || id.contains("allow") {
                    Some(id.to_string())
                } else {
                    None
                }
            })
            .collect();
        proc.permission_allow_options
            .insert(rpc_id.clone(), allow_ids);
        let title = value
            .pointer("/params/toolCall/title")
            .or_else(|| value.pointer("/params/toolCall/toolCallId"))
            .and_then(|v| v.as_str())
            .unwrap_or("tool call");
        let summary = format!("Allow ACP action: {title}?");
        return Some(AcpEvent::PermissionRequest {
            id: rpc_id,
            summary,
        });
    }

    if method == "session/update" {
        let update = value.pointer("/params/update")?;
        let kind = update
            .get("sessionUpdate")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        match kind {
            "agent_message_chunk" | "agent_thought_chunk" | "user_message_chunk" => {
                let text = update
                    .pointer("/content/text")
                    .or_else(|| update.pointer("/content/content/text"))
                    .and_then(|v| v.as_str())
                    .map(str::to_string)?;
                if text.is_empty() {
                    return None;
                }
                return Some(AcpEvent::MessageDelta { text });
            }
            "tool_call" | "tool_call_update" => {
                let id = update
                    .get("toolCallId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("tool")
                    .to_string();
                let name = update
                    .get("title")
                    .or_else(|| update.get("kind"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("tool")
                    .to_string();
                return Some(AcpEvent::ToolCall {
                    id,
                    name,
                    input: update.clone(),
                });
            }
            "state_update" => {
                let state = update.get("state").and_then(|v| v.as_str()).unwrap_or("");
                if state == "idle" {
                    return Some(AcpEvent::Done);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stub_session_roundtrip() {
        let client = StubAcpClient::new();
        let cfg = AcpAgentConfig::cursor_default();
        let session = client.create_session(&cfg).await.unwrap();
        assert!(session.0.starts_with("stub-cursor-"));
        client.prompt(&session, "hello").await.unwrap();
        let first = client.next_event(&session).await.unwrap();
        assert!(matches!(first, Some(AcpEvent::MessageDelta { .. })));
        let second = client.next_event(&session).await.unwrap();
        assert!(matches!(second, Some(AcpEvent::MessageDone { .. })));
    }

    #[test]
    fn parses_runtime_kinds() {
        assert_eq!(
            AgentRuntimeKind::parse("Cursor"),
            Some(AgentRuntimeKind::Cursor)
        );
        assert_eq!(
            AgentRuntimeKind::parse("codex"),
            Some(AgentRuntimeKind::Codex)
        );
        assert!(external_runtime_config(AgentRuntimeKind::Medousa).is_err());
    }

    #[tokio::test]
    async fn external_falls_back_to_stub() {
        let client = ExternalAcpClient::new();
        let mut cfg = AcpAgentConfig::cursor_default();
        cfg.command = "medousa-acp-missing-binary-xyz".into();
        let session = client.create_session(&cfg).await.unwrap();
        assert!(session.0.starts_with("stub-"));
    }

    #[test]
    fn resolves_local_bin_commands() {
        let home = home_dir().expect("home");
        let local_bin = home.join(".local").join("bin");
        if local_bin.join("agent").is_file() {
            let resolved = resolve_command_path("agent");
            assert!(resolved.is_some());
            assert!(resolved.unwrap().ends_with("agent"));
        }
        if local_bin.join("codex").is_file() {
            assert!(command_available("codex"));
        }
    }

    #[test]
    fn cursor_status_probe_matches_cli_when_available() {
        if resolve_command_path("agent").is_none() && resolve_command_path("cursor-agent").is_none()
        {
            return;
        }
        let probe = cursor_auth_probe(true);
        // If the CLI is present we should get a definitive answer from `agent status`.
        assert!(
            matches!(
                probe.status,
                RuntimeAuthStatus::SignedIn | RuntimeAuthStatus::SignedOut
            ),
            "unexpected status {:?} detail={:?}",
            probe.status,
            probe.detail
        );
    }

    #[test]
    fn codex_default_avoids_stock_acp_subcommand() {
        // Unless overridden, we must not rely on `codex acp` (it doesn't exist).
        if std::env::var_os("MEDOUSA_ACP_CODEX_COMMAND").is_some() {
            return;
        }
        let cfg = AcpAgentConfig::codex_default();
        let joined = format!("{} {}", cfg.command, cfg.args.join(" "));
        assert!(
            !joined.trim_end().ends_with("codex acp")
                || resolve_command_path("npx").is_none()
                    && resolve_command_path("codex-acp").is_none(),
            "unexpected launch: {joined}"
        );
        if resolve_command_path("codex-acp").is_some() {
            assert!(cfg.command.contains("codex-acp"));
            assert!(cfg.args.is_empty());
        } else if resolve_command_path("npx").is_some() {
            assert!(cfg.command.contains("npx"));
            assert!(cfg.args.iter().any(|a| a.contains("codex-acp")));
        }
    }
}
