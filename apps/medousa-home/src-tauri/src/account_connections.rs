//! Local vendor setup actions for the existing Codex, Cursor, and Hermes integrations.
//! Runtime discovery for all external agents comes from the connected daemon.
//!
//! Medousa never holds vendor tokens — login orchestrates the official CLIs
//! (`codex login`, `cursor agent login` / `agent login`, `hermes acp --setup`), which keep
//! credentials in their own stores. We probe sign-in state via the daemon agents surface,
//! start device auth / terminal login from here, and shell out to `* logout` on sign-out
//! (Hermes has no logout — credentials stay in `~/.hermes`). Missing CLIs are installed via
//! the vendors' official installers (not Packages).

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use crate::daemon::window_request::WorkshopRequest;
use medousa_types::AgentRuntimeInfo;
use serde::Serialize;
use tauri_plugin_opener::open_url;

use crate::daemon::sdk::{client_for_request, sdk_error};

fn codex_command() -> String {
    std::env::var("MEDOUSA_ACP_CODEX_COMMAND").unwrap_or_else(|_| "codex".into())
}

fn cursor_command() -> String {
    std::env::var("MEDOUSA_ACP_CURSOR_COMMAND").unwrap_or_else(|_| "agent".into())
}

fn hermes_command() -> String {
    std::env::var("MEDOUSA_ACP_HERMES_COMMAND").unwrap_or_else(|_| "hermes".into())
}

/// Resolve a bare command name to an absolute path when possible (for Terminal.app
/// windows that inherit a thin GUI PATH and miss `~/.local/bin`).
fn resolve_command_abs(command: &str) -> String {
    ensure_vendor_cli_path();
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return command.to_string();
    }
    let path = PathBuf::from(trimmed);
    if path.is_absolute() || trimmed.contains('/') || trimmed.contains('\\') {
        return trimmed.to_string();
    }
    let mut dirs = Vec::new();
    if let Some(path_var) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path_var));
    }
    for extra in common_vendor_bin_dirs() {
        if !dirs.iter().any(|dir| dir == &extra) {
            dirs.push(extra);
        }
    }
    for dir in dirs {
        let candidate = dir.join(trimmed);
        if candidate.is_file() {
            return candidate.display().to_string();
        }
        #[cfg(windows)]
        {
            let exe = dir.join(format!("{trimmed}.exe"));
            if exe.is_file() {
                return exe.display().to_string();
            }
            let cmd = dir.join(format!("{trimmed}.cmd"));
            if cmd.is_file() {
                return cmd.display().to_string();
            }
        }
    }
    trimmed.to_string()
}

/// Cursor auth entry points.
///
/// Prefer `cursor agent login` when the Cursor app CLI is present — `/usr/local/bin/cursor`
/// is on the default macOS PATH, while the standalone `agent` binary often lives only in
/// `~/.local/bin`, which Terminal windows opened via osascript may not see. Docs also
/// accept bare `agent login`; both share the same auth store.
fn cursor_auth_argv(action: &str) -> Result<(String, Vec<String>, String), String> {
    ensure_vendor_cli_path();
    if command_on_path("cursor") {
        let cursor = resolve_command_abs("cursor");
        return Ok((
            cursor,
            vec!["agent".into(), action.to_string()],
            format!("cursor agent {action}"),
        ));
    }
    let agent = cursor_command();
    if !command_on_path(&agent) && !command_on_path("cursor-agent") {
        return Err(
            "'cursor' / 'agent' not found — tap Install on the Cursor card in Settings → External Agents"
                .into(),
        );
    }
    let program = if command_on_path(&agent) {
        resolve_command_abs(&agent)
    } else {
        resolve_command_abs("cursor-agent")
    };
    Ok((program, vec![action.to_string()], format!("agent {action}")))
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
}

/// Locations preferred by Codex / Cursor official installers (and Homebrew / npm).
fn common_vendor_bin_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = home_dir() {
        dirs.push(home.join(".local").join("bin"));
        dirs.push(home.join(".npm-global").join("bin"));
        dirs.push(home.join("bin"));
        #[cfg(windows)]
        {
            dirs.push(
                home.join("AppData")
                    .join("Local")
                    .join("Programs")
                    .join("codex"),
            );
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
fn ensure_vendor_cli_path() {
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
    if let Ok(joined) = std::env::join_paths(parts) {
        // SAFETY: called only from Tauri commands before spawning vendor CLIs;
        // Medousa does not concurrently read PATH for security decisions here.
        unsafe { std::env::set_var("PATH", joined) };
    }
}

fn command_on_path(command: &str) -> bool {
    ensure_vendor_cli_path();
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return false;
    }
    let path = PathBuf::from(trimmed);
    if path.is_absolute() || trimmed.contains('/') || trimmed.contains('\\') {
        return path.is_file();
    }
    let mut dirs = Vec::new();
    if let Some(path_var) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path_var));
    }
    for extra in common_vendor_bin_dirs() {
        if !dirs.iter().any(|dir| dir == &extra) {
            dirs.push(extra);
        }
    }
    dirs.iter().any(|dir| {
        if dir.join(trimmed).is_file() {
            return true;
        }
        #[cfg(windows)]
        {
            if dir.join(format!("{trimmed}.exe")).is_file()
                || dir.join(format!("{trimmed}.cmd")).is_file()
            {
                return true;
            }
        }
        false
    })
}

#[cfg(target_os = "windows")]
fn detach_new_session(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(target_os = "windows"))]
fn detach_new_session(_command: &mut Command) {}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AccountConnectionInfo {
    /// `chatgpt` (Codex CLI), `cursor`, or `hermes`.
    pub id: String,
    pub label: String,
    pub runtime: String,
    pub binary_present: bool,
    pub command: Option<String>,
    /// `signed_in` | `signed_out` | `unknown`.
    pub auth_status: String,
    pub detail: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AccountConnections {
    pub chatgpt: AccountConnectionInfo,
    pub cursor: AccountConnectionInfo,
    pub hermes: AccountConnectionInfo,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DeviceAuthStart {
    pub url: String,
    pub code: Option<String>,
    pub detail: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AccountCliInstallResult {
    pub account: String,
    pub command: String,
    pub detail: String,
}

fn connection_from_runtime(
    id: &str,
    label: &str,
    info: Option<&AgentRuntimeInfo>,
) -> AccountConnectionInfo {
    let binary_present = info.map(|i| i.binary_present).unwrap_or(false);
    let auth_status = info
        .and_then(|i| i.auth_status.clone())
        .unwrap_or_else(|| "unknown".into());
    let detail = info.and_then(|i| i.auth_detail.clone().or(i.detail.clone()));

    AccountConnectionInfo {
        id: id.to_string(),
        label: label.to_string(),
        runtime: match id {
            "chatgpt" => "codex".into(),
            "hermes" => "hermes".into(),
            _ => "cursor".into(),
        },
        binary_present,
        command: info.and_then(|i| i.command.clone()),
        auth_status,
        detail,
    }
}

/// Legacy three-account projection of the invoking window's daemon runtime status.
#[tauri::command]
pub async fn account_connections_probe(
    state: WorkshopRequest,
) -> Result<AccountConnections, String> {
    ensure_vendor_cli_path();
    let runtimes = client_for_request(&state)
        .await?
        .agents()
        .list_runtimes()
        .await
        .map_err(sdk_error)?
        .runtimes;
    let codex = runtimes.iter().find(|r| r.runtime == "codex");
    let cursor = runtimes.iter().find(|r| r.runtime == "cursor");
    let hermes = runtimes.iter().find(|r| r.runtime == "hermes");
    Ok(AccountConnections {
        chatgpt: connection_from_runtime("chatgpt", "ChatGPT", codex),
        cursor: connection_from_runtime("cursor", "Cursor", cursor),
        hermes: connection_from_runtime("hermes", "Hermes", hermes),
    })
}

fn install_timeout() -> Duration {
    Duration::from_secs(5 * 60)
}

fn run_install_shell(script: &str) -> Result<String, String> {
    #[cfg(windows)]
    {
        let mut command = Command::new("powershell");
        command
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                script,
            ])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        detach_new_session(&mut command);
        let output = command
            .output()
            .map_err(|err| format!("couldn't start installer: {err}"))?;
        let combined = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if !output.status.success() {
            return Err(format!(
                "installer failed (exit {}): {}",
                output.status.code().unwrap_or(-1),
                combined.trim()
            ));
        }
        return Ok(combined);
    }
    #[cfg(not(windows))]
    {
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg(script)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .env("CI", "1");
        detach_new_session(&mut command);
        let child = command
            .spawn()
            .map_err(|err| format!("couldn't start installer: {err}"))?;
        let output = wait_with_timeout(child, install_timeout())?;
        let combined = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if !output.status.success() {
            return Err(format!(
                "installer failed (exit {}): {}",
                output.status.code().unwrap_or(-1),
                combined.trim()
            ));
        }
        Ok(combined)
    }
}

#[cfg(not(windows))]
fn wait_with_timeout(
    mut child: std::process::Child,
    timeout: Duration,
) -> Result<std::process::Output, String> {
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                return child
                    .wait_with_output()
                    .map_err(|err| format!("installer exit failed: {err}"));
            }
            Ok(None) => {
                if started.elapsed() > timeout {
                    let _ = child.kill();
                    return Err("installer timed out after 5 minutes".into());
                }
                std::thread::sleep(Duration::from_millis(200));
            }
            Err(err) => return Err(format!("installer wait failed: {err}")),
        }
    }
}

async fn require_local_workshop(state: &WorkshopRequest) -> Result<(), String> {
    let local = match state.target().await? {
        crate::active_workshop::ActiveWorkshopTarget::EmbeddedPersonal => true,
        crate::active_workshop::ActiveWorkshopTarget::Transport { workshop, .. } => {
            workshop.kind == "local"
        }
    };
    if local {
        Ok(())
    } else {
        Err(
            "Configure this agent on the workshop host; local credentials belong to this device"
                .into(),
        )
    }
}

/// Install the vendor CLI for an account using the official installer.
/// ChatGPT → Codex; Cursor → Agent CLI; Hermes → Hermes Agent.
#[tauri::command]
pub async fn account_cli_install(
    state: WorkshopRequest,
    account: String,
) -> Result<AccountCliInstallResult, String> {
    require_local_workshop(&state).await?;
    let (command, label) = match account.as_str() {
        "chatgpt" => (codex_command(), "Codex CLI"),
        "cursor" => (cursor_command(), "Cursor Agent CLI"),
        "hermes" => (hermes_command(), "Hermes Agent CLI"),
        other => return Err(format!("unknown account '{other}'")),
    };
    ensure_vendor_cli_path();
    if command_on_path(&command)
        || (account == "hermes" && (command_on_path("hermes") || command_on_path("hermes-acp")))
    {
        let mut detail = format!("{label} is already installed.");
        if account == "hermes" {
            match ensure_hermes_acp_extra() {
                Ok(note) => detail = format!("{detail} {note}"),
                Err(err) => {
                    detail = format!(
                        "{detail} (Hermes ACP extra not confirmed: {err}. \
                         Run `hermes acp --check` after install.)"
                    );
                }
            }
        }
        return Ok(AccountCliInstallResult {
            account: account.clone(),
            command: command.clone(),
            detail,
        });
    }

    let account_owned = account.clone();
    let command_owned = command.clone();
    let label_owned = label.to_string();
    tauri::async_runtime::spawn_blocking(move || {
        let script = match account_owned.as_str() {
            "chatgpt" => {
                #[cfg(windows)]
                {
                    "irm https://chatgpt.com/codex/install.ps1 | iex"
                }
                #[cfg(not(windows))]
                {
                    "curl -fsSL https://chatgpt.com/codex/install.sh | sh"
                }
            }
            "cursor" => {
                #[cfg(windows)]
                {
                    // Cursor's published installer is a bash script; try Git Bash / WSL bash first.
                    r#"
$bash = Get-Command bash -ErrorAction SilentlyContinue
if (-not $bash) { throw "Cursor Agent CLI install needs bash (Git Bash or WSL) on Windows." }
& bash -lc "curl -fsSL https://cursor.com/install | bash"
"#
                }
                #[cfg(not(windows))]
                {
                    "curl -fsSL https://cursor.com/install | bash"
                }
            }
            "hermes" => {
                #[cfg(windows)]
                {
                    "iex (irm https://hermes-agent.nousresearch.com/install.ps1)"
                }
                #[cfg(not(windows))]
                {
                    "curl -fsSL https://hermes-agent.nousresearch.com/install.sh | bash"
                }
            }
            _ => unreachable!(),
        };
        run_install_shell(script)?;
        ensure_vendor_cli_path();
        let hermes_ok = account_owned == "hermes"
            && (command_on_path("hermes") || command_on_path("hermes-acp"));
        if !command_on_path(&command_owned) && !hermes_ok {
            return Err(format!(
                "{label_owned} installed, but '{command_owned}' is still not on PATH. \
                 Quit and reopen Medousa, or add ~/.local/bin to your PATH."
            ));
        }
        let mut detail = format!("{label_owned} installed — you can sign in now.");
        if account_owned == "hermes" {
            match ensure_hermes_acp_extra() {
                Ok(note) => detail = format!("{detail} {note}"),
                Err(err) => {
                    detail = format!(
                        "{detail} (Hermes ACP extra not confirmed: {err}. \
                         Run `hermes acp --check` after install.)"
                    );
                }
            }
        }
        Ok(AccountCliInstallResult {
            account: account_owned,
            command: if hermes_ok && !command_on_path(&command_owned) {
                if command_on_path("hermes-acp") {
                    "hermes-acp".into()
                } else {
                    "hermes".into()
                }
            } else {
                command_owned
            },
            detail,
        })
    })
    .await
    .map_err(|err| format!("install task failed: {err}"))?
}

/// Ensure Hermes ACP extras are available (`hermes acp` / `hermes-acp`).
fn ensure_hermes_acp_extra() -> Result<String, String> {
    ensure_vendor_cli_path();
    if hermes_acp_ready() {
        return Ok("Hermes ACP launcher ready.".into());
    }
    let checkout = std::env::var_os("HERMES_HOME")
        .map(PathBuf::from)
        .or_else(home_dir)
        .map(|home| {
            if home.ends_with(".hermes") || home.ends_with("hermes") {
                home.join("hermes-agent")
            } else {
                home.join(".hermes").join("hermes-agent")
            }
        });
    let Some(checkout) = checkout.filter(|p| p.is_dir()) else {
        return Err(
            "Hermes checkout not found at ~/.hermes/hermes-agent — re-run the installer or `uv pip install -e '.[acp]'` there"
                .into(),
        );
    };
    let uv = ["uv", "uv.exe"]
        .into_iter()
        .find(|name| command_on_path(name))
        .ok_or_else(|| "uv not found — needed to install Hermes ACP extras".to_string())?;
    let mut command = Command::new(uv);
    command
        .current_dir(&checkout)
        .args(["pip", "install", "-e", ".[acp]"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    detach_new_session(&mut command);
    let output = command
        .output()
        .map_err(|err| format!("couldn't start uv: {err}"))?;
    if !output.status.success() {
        let combined = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return Err(format!(
            "uv pip install failed (exit {}): {}",
            output.status.code().unwrap_or(-1),
            combined.trim()
        ));
    }
    ensure_vendor_cli_path();
    if hermes_acp_ready() {
        Ok("Hermes ACP extras installed.".into())
    } else {
        Ok(
            "ACP extras installed; restart Medousa if runtime still can't find `hermes acp`."
                .into(),
        )
    }
}

fn hermes_acp_ready() -> bool {
    if command_on_path("hermes-acp") {
        return true;
    }
    if !command_on_path("hermes") {
        return false;
    }
    let program = resolve_command_abs("hermes");
    let output = Command::new(&program)
        .args(["acp", "--check"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output();
    matches!(output, Ok(out) if out.status.success())
}

#[derive(Debug, PartialEq)]
enum CodexLoginStatus {
    ChatGpt,
    ApiKey,
    Unknown,
}

fn codex_login_status(success: bool, stdout: &[u8], stderr: &[u8]) -> CodexLoginStatus {
    let status = format!(
        "{}\n{}",
        String::from_utf8_lossy(stdout),
        String::from_utf8_lossy(stderr),
    )
    .to_lowercase();
    if success && status.contains("logged in using chatgpt") {
        CodexLoginStatus::ChatGpt
    } else if status.contains("api key") || status.contains("apikey") {
        CodexLoginStatus::ApiKey
    } else {
        CodexLoginStatus::Unknown
    }
}

#[derive(Default)]
struct CodexDeviceInstructions {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    stdout_closed: bool,
    stderr_closed: bool,
}

impl CodexDeviceInstructions {
    fn push(&mut self, stderr: bool, chunk: &[u8]) -> Result<(), String> {
        if chunk.is_empty() {
            if stderr {
                self.stderr_closed = true;
            } else {
                self.stdout_closed = true;
            }
        }
        let buffer = if stderr {
            &mut self.stderr
        } else {
            &mut self.stdout
        };
        if buffer.len() + chunk.len() > 32 * 1024 {
            return Err("Codex login produced too much output before sign-in instructions.".into());
        }
        buffer.extend_from_slice(chunk);
        Ok(())
    }

    fn instructions(&self) -> Option<(String, String)> {
        let output = format!(
            "{}\n{}",
            complete_codex_output(&self.stdout, self.stdout_closed),
            complete_codex_output(&self.stderr, self.stderr_closed),
        );
        let url = output
            .split_whitespace()
            .find_map(verified_codex_device_url)?;
        let mut code_context = false;
        for line in output.lines() {
            let lower = line.to_ascii_lowercase();
            let mentions_code = lower.contains("one-time code")
                || lower.contains("one time code")
                || lower.contains("your code")
                || lower.trim_start().starts_with("code:")
                || (lower.contains("enter")
                    && lower.contains("code")
                    && !lower.contains("https://"));
            let standalone = line.split_whitespace().count() == 1;
            if mentions_code || code_context || standalone {
                if let Some(code) = line.split_whitespace().find_map(codex_device_code) {
                    return Some((url, code));
                }
            }
            if !line.trim().is_empty() {
                code_context = mentions_code;
            }
        }
        None
    }
}

fn complete_codex_output(bytes: &[u8], closed: bool) -> String {
    let end = if closed {
        bytes.len()
    } else {
        bytes
            .iter()
            .rposition(|byte| *byte == b'\n' || *byte == b'\r')
            .map_or(0, |index| index + 1)
    };
    strip_terminal_controls(&String::from_utf8_lossy(&bytes[..end]))
}

fn strip_terminal_controls(text: &str) -> String {
    let mut chars = text.chars().peekable();
    let mut clean = String::new();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            match chars.next() {
                Some('[') => {
                    for ch in chars.by_ref() {
                        if ('@'..='~').contains(&ch) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    while let Some(ch) = chars.next() {
                        if ch == '\u{7}' || (ch == '\u{1b}' && chars.peek() == Some(&'\\')) {
                            if ch == '\u{1b}' {
                                chars.next();
                            }
                            break;
                        }
                    }
                }
                _ => {}
            }
        } else if !ch.is_control() || ch == '\n' || ch == '\r' || ch == '\t' {
            clean.push(ch);
        }
    }
    clean
}

fn verified_codex_device_url(token: &str) -> Option<String> {
    let token = token.trim_matches(|ch: char| {
        matches!(
            ch,
            '<' | '>' | '(' | ')' | '[' | ']' | '\'' | '"' | ',' | '.'
        )
    });
    let url = url::Url::parse(token).ok()?;
    (url.scheme() == "https"
        && url.host_str() == Some("auth.openai.com")
        && url.path().trim_end_matches('/') == "/codex/device"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.query().is_none()
        && url.fragment().is_none())
    .then(|| url.to_string())
}

fn codex_device_code(token: &str) -> Option<String> {
    // Codex receives user_code as a server-provided string, not a typed format.
    // Recognize familiar device codes conservatively; unknown formats fall back
    // to the vendor terminal flow instead of opening a guessed handoff.
    let code =
        token.trim_matches(|ch: char| matches!(ch, '`' | '\'' | '"' | ',' | '.' | ':' | '(' | ')'));
    let (left, right) = code.split_once('-')?;
    let valid_group = |group: &str| {
        (4..=6).contains(&group.len())
            && group
                .chars()
                .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit())
    };
    (valid_group(left) && valid_group(right)).then(|| code.to_string())
}

fn drain_codex_login_output(
    mut pipe: impl std::io::Read + Send + 'static,
    stderr: bool,
    sender: std::sync::mpsc::Sender<(bool, Vec<u8>)>,
) {
    std::thread::spawn(move || {
        let mut chunk = [0u8; 4096];
        loop {
            match pipe.read(&mut chunk) {
                Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
                Ok(0) | Err(_) => {
                    let _ = sender.send((stderr, Vec::new()));
                    break;
                }
                Ok(count) => {
                    // Keep draining after the handoff receiver closes so the CLI can
                    // finish polling and save its credentials without a full pipe.
                    let _ = sender.send((stderr, chunk[..count].to_vec()));
                }
            }
        }
    });
}

fn check_codex_login_status(command: &str) -> Result<CodexLoginStatus, String> {
    let mut status_command = Command::new(command);
    status_command
        .args(["login", "status"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    detach_new_session(&mut status_command);
    let mut child = status_command
        .spawn()
        .map_err(|_| "Couldn't start the Codex sign-in status check.".to_string())?;
    let result: Result<CodexLoginStatus, String> = (|| {
        let (sender, receiver) = std::sync::mpsc::channel();
        let stdout = child
            .stdout
            .take()
            .ok_or("Codex status produced no stdout pipe")?;
        let stderr = child
            .stderr
            .take()
            .ok_or("Codex status produced no stderr pipe")?;
        drain_codex_login_output(stdout, false, sender.clone());
        drain_codex_login_output(stderr, true, sender);
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut output = CodexDeviceInstructions::default();
        let mut exit = None;
        loop {
            if exit.is_none() {
                exit = child
                    .try_wait()
                    .map_err(|_| "Couldn't check the Codex status process.".to_string())?;
            }
            if let Some(exit) = exit {
                if output.stdout_closed && output.stderr_closed {
                    return Ok(codex_login_status(
                        exit.success(),
                        &output.stdout,
                        &output.stderr,
                    ));
                }
            }
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err(
                    "Timed out checking Codex sign-in status. Try terminal sign-in.".into(),
                );
            }
            let poll = remaining.min(Duration::from_millis(100));
            match receiver.recv_timeout(poll) {
                Ok((stderr, chunk)) => output.push(stderr, &chunk)?,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => std::thread::sleep(poll),
            }
        }
    })();
    if result.is_err() {
        let _ = child.kill();
    }
    let _ = child.wait();
    result
}

fn start_codex_device_login() -> Result<DeviceAuthStart, String> {
    let command = resolve_command_abs(&codex_command());
    if !command_on_path(&command) {
        return Err(format!(
            "'{command}' not found — tap Install on the Codex card in Settings → External Agents"
        ));
    }

    match check_codex_login_status(&command)? {
        CodexLoginStatus::ChatGpt => {
            return Err("Already signed in to ChatGPT — refresh status instead".into());
        }
        CodexLoginStatus::ApiKey => {
            return Err(
                "Codex is using an API key — run `codex logout` before connecting ChatGPT.".into(),
            );
        }
        CodexLoginStatus::Unknown => {}
    }

    let mut child_cmd = Command::new(&command);
    child_cmd
        .args(["login", "--device-auth"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    detach_new_session(&mut child_cmd);
    let mut child = child_cmd
        .spawn()
        .map_err(|err| format!("failed to start codex login: {err}"))?;

    let result: Result<DeviceAuthStart, String> = (|| {
        let (sender, receiver) = std::sync::mpsc::channel();
        let stdout = child
            .stdout
            .take()
            .ok_or("Codex login produced no stdout pipe")?;
        let stderr = child
            .stderr
            .take()
            .ok_or("Codex login produced no stderr pipe")?;
        drain_codex_login_output(stdout, false, sender.clone());
        drain_codex_login_output(stderr, true, sender);
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        let mut instructions = CodexDeviceInstructions::default();
        let (url, code) = loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err("Timed out waiting for Codex device sign-in instructions. Try terminal sign-in.".into());
            }
            let (stderr, chunk) = receiver.recv_timeout(remaining).map_err(|_| {
                "Couldn't read verified device sign-in instructions from Codex. Try terminal sign-in."
                    .to_string()
            })?;
            instructions.push(stderr, &chunk)?;
            if let Some(instructions) = instructions.instructions() {
                break instructions;
            }
        };
        open_url(&url, None::<&str>).map_err(|_| {
            "Couldn't open the Codex sign-in page. Try terminal sign-in.".to_string()
        })?;
        Ok(DeviceAuthStart {
            url,
            code: Some(code),
            detail: Some(
                "Approve in the browser that just opened — codex finishes sign-in on its own."
                    .into(),
            ),
        })
    })();

    if result.is_ok() {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    } else {
        let _ = child.kill();
        let _ = child.wait();
    }
    result
}

/// Start the vendor's device-code login without blocking the async command thread.
/// Instructions may arrive on either pipe; only a verified device URL opens.
#[tauri::command]
pub async fn account_chatgpt_begin_device_login(
    state: WorkshopRequest,
) -> Result<DeviceAuthStart, String> {
    require_local_workshop(&state).await?;
    tauri::async_runtime::spawn_blocking(start_codex_device_login)
        .await
        .map_err(|_| "Codex sign-in task failed.".to_string())?
}

fn open_terminal_with_login(command: &str, args: &[String]) -> Result<(), String> {
    // Quote paths that may contain spaces (e.g. Homebrew Cellar).
    let quoted = |s: &str| {
        if s.chars().any(|c| c.is_whitespace()) {
            format!("\"{}\"", s.replace('"', "\\\""))
        } else {
            s.to_string()
        }
    };
    let shell_line = std::iter::once(quoted(command))
        .chain(args.iter().map(|a| quoted(a)))
        .collect::<Vec<_>>()
        .join(" ");

    #[cfg(target_os = "macos")]
    {
        let script = format!(
            "tell application \"Terminal\" to do script \"{}\"",
            shell_line.replace('\\', "\\\\").replace('"', "\\\"")
        );
        Command::new("osascript")
            .args(["-e", &script])
            .spawn()
            .map_err(|err| format!("couldn't open Terminal: {err}"))?;
        Ok(())
    }
    #[cfg(target_os = "windows")]
    {
        let mut cmd = Command::new("cmd");
        cmd.args(["/c", "start", "cmd", "/k", &shell_line]);
        detach_new_session(&mut cmd);
        cmd.spawn()
            .map_err(|err| format!("couldn't open a terminal: {err}"))?;
        return Ok(());
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        for term in ["x-terminal-emulator", "gnome-terminal", "konsole", "xterm"] {
            let result = Command::new(term).arg("-e").arg(&shell_line).spawn();
            if result.is_ok() {
                return Ok(());
            }
        }
        Err("no terminal emulator found to run the login".into())
    }
}

/// Fallback / Cursor / Hermes login: run the vendor CLI's interactive login in a
/// terminal window (browser callback flows need a TTY).
#[tauri::command]
pub async fn account_begin_terminal_login(
    state: WorkshopRequest,
    account: String,
) -> Result<String, String> {
    require_local_workshop(&state).await?;
    let (command, args, hint) = match account.as_str() {
        "chatgpt" => {
            let cmd = codex_command();
            if !command_on_path(&cmd) {
                return Err(
                    "'codex' not found — tap Install on the Codex card in Settings → External Agents"
                        .into(),
                );
            }
            (
                resolve_command_abs(&cmd),
                vec!["login".into()],
                "codex login".to_string(),
            )
        }
        "cursor" => cursor_auth_argv("login")?,
        "hermes" => {
            ensure_vendor_cli_path();
            if !command_on_path("hermes") && !command_on_path("hermes-acp") {
                return Err(
                    "'hermes' not found — tap Install on the Hermes card in Settings → External Agents"
                        .into(),
                );
            }
            if command_on_path("hermes") {
                (
                    resolve_command_abs("hermes"),
                    vec!["acp".into(), "--setup".into()],
                    "hermes acp --setup".to_string(),
                )
            } else {
                (
                    resolve_command_abs("hermes-acp"),
                    vec!["--setup".into()],
                    "hermes-acp --setup".to_string(),
                )
            }
        }
        other => return Err(format!("unknown account '{other}'")),
    };
    open_terminal_with_login(&command, &args)?;
    Ok(format!(
        "Sign in with `{hint}` in the terminal that opened, then return here — status updates automatically."
    ))
}

#[tauri::command]
pub async fn account_sign_out(state: WorkshopRequest, account: String) -> Result<String, String> {
    require_local_workshop(&state).await?;
    let (command, args) = match account.as_str() {
        "chatgpt" => {
            let cmd = codex_command();
            if !command_on_path(&cmd) {
                return Err("'codex' not found on PATH".into());
            }
            (resolve_command_abs(&cmd), vec!["logout".into()])
        }
        "cursor" => {
            let (cmd, argv, _) = cursor_auth_argv("logout")?;
            (cmd, argv)
        }
        "hermes" => {
            // Hermes has no vendor logout — provider keys live in ~/.hermes.
            // Open interactive model/provider setup so the user can change them.
            ensure_vendor_cli_path();
            if !command_on_path("hermes") {
                return Err(
                    "Hermes credentials live in ~/.hermes — install Hermes or run `hermes model` to reconfigure"
                        .into(),
                );
            }
            open_terminal_with_login(&resolve_command_abs("hermes"), &["model".into()])?;
            return Ok(
                "Opened `hermes model` — Hermes keeps credentials in ~/.hermes; Medousa never deletes them."
                    .into(),
            );
        }
        other => return Err(format!("unknown account '{other}'")),
    };
    let mut cmd = Command::new(&command);
    cmd.args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    detach_new_session(&mut cmd);
    cmd.spawn()
        .map_err(|err| format!("failed to run {command} {}: {err}", args.join(" ")))?
        .wait()
        .map_err(|err| err.to_string())?;
    Ok("Signed out — refresh status to confirm.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_login_status_accepts_either_output_stream() {
        for (stdout, stderr) in [
            (b"Logged in using ChatGPT".as_slice(), b"".as_slice()),
            (b"".as_slice(), b"Logged in using ChatGPT".as_slice()),
        ] {
            assert_eq!(
                codex_login_status(true, stdout, stderr),
                CodexLoginStatus::ChatGpt
            );
        }
        assert_eq!(
            codex_login_status(true, b"", b"Logged in using an API key"),
            CodexLoginStatus::ApiKey,
        );
        assert_eq!(
            codex_login_status(false, b"", b"Not logged in"),
            CodexLoginStatus::Unknown,
        );
        assert_eq!(
            codex_login_status(false, b"", b"Logged in using ChatGPT"),
            CodexLoginStatus::Unknown,
        );
    }

    #[test]
    fn codex_device_instructions_match_the_installed_0145_prompt() {
        // openai/codex rust-v0.145.0, login/src/device_code_auth.rs:136–148.
        let prompt = "Welcome to Codex\nSign in using device code authorization:\n\n1. Open this link in your browser and sign in to your account\n   https://auth.openai.com/codex/device\n\n2. Enter this one-time code (expires in 15 minutes)\n   ABCD-EFGHI\n\nContinue only if you started this login in Codex. If a website or another person gave you this code, cancel.\n";
        for stderr in [false, true] {
            let mut parser = CodexDeviceInstructions::default();
            parser.push(stderr, prompt.as_bytes()).unwrap();
            assert_eq!(
                parser.instructions(),
                Some((
                    "https://auth.openai.com/codex/device".into(),
                    "ABCD-EFGHI".into()
                ))
            );
        }
        let mut unknown = CodexDeviceInstructions::default();
        unknown
            .push(
                false,
                prompt
                    .replace("ABCD-EFGHI", "server-defined-future-format")
                    .as_bytes(),
            )
            .unwrap();
        assert!(
            unknown.instructions().is_none(),
            "unknown formats must fall back without guessing a code"
        );
    }

    #[test]
    fn codex_device_instructions_wait_for_the_real_code_after_the_url() {
        let mut parser = CodexDeviceInstructions::default();
        parser
            .push(
                false,
                b"Welcome to Codex!\nDocs: https://developers.openai.com/codex/auth\n",
            )
            .unwrap();
        parser
            .push(false, b"1. Open https://auth.openai.com/codex/device\n")
            .unwrap();
        assert!(parser.instructions().is_none());
        parser
            .push(false, b"2. Enter this one-time code:\n\x1b[1mABCD-EFGH")
            .unwrap();
        assert!(
            parser.instructions().is_none(),
            "an unfinished chunk must not truncate the code"
        );
        parser.push(false, b"I\x1b[0m\n").unwrap();
        assert_eq!(
            parser.instructions(),
            Some((
                "https://auth.openai.com/codex/device".into(),
                "ABCD-EFGHI".into()
            ))
        );
    }

    #[test]
    fn codex_device_instructions_accept_stderr_and_split_streams() {
        let mut parser = CodexDeviceInstructions::default();
        parser
            .push(false, b"For help: https://chatgpt.com/\n")
            .unwrap();
        parser
            .push(true, b"https://auth.openai.com/codex/de")
            .unwrap();
        assert!(parser.instructions().is_none());
        parser
            .push(true, b"vice\nEnter this code:\nWXYZ-1234\n")
            .unwrap();
        assert_eq!(
            parser.instructions(),
            Some((
                "https://auth.openai.com/codex/device".into(),
                "WXYZ-1234".into()
            ))
        );
        let mut split = CodexDeviceInstructions::default();
        split
            .push(true, b"https://auth.openai.com/codex/device\n")
            .unwrap();
        split.push(false, b"Your code:\nABCD-1234").unwrap();
        assert!(split.instructions().is_none());
        split.push(false, b"").unwrap();
        assert_eq!(
            split.instructions().map(|(_, code)| code),
            Some("ABCD-1234".into())
        );
    }

    #[test]
    fn codex_device_handoff_rejects_other_links_and_introductory_words() {
        for url in [
            "https://chatgpt.com/",
            "https://developers.openai.com/codex/auth",
            "https://auth.openai.com.evil.test/codex/device",
            "http://auth.openai.com/codex/device",
            "https://auth.openai.com/codex/device/elsewhere",
            "https://auth.openai.com/codex/device?redirect_uri=https://example.com",
            "https://user@auth.openai.com/codex/device",
        ] {
            assert!(verified_codex_device_url(url).is_none());
        }
        for word in [
            "Welcome",
            "Instructions",
            "one-time",
            "CODE-EXAMPLE",
            "https://openai.com",
        ] {
            assert!(codex_device_code(word).is_none());
        }
        let mut parser = CodexDeviceInstructions::default();
        parser.push(true, b"https://auth.openai.com/codex/device\nWelcome to Codex. Please enter your code.\n").unwrap();
        assert!(parser.instructions().is_none());
    }

    #[test]
    fn connection_status_remains_owned_by_the_selected_workshop() {
        let runtime = AgentRuntimeInfo {
            runtime: "cursor".into(),
            available: false,
            command: Some("/remote/bin/agent".into()),
            detail: None,
            uses_native_turns: false,
            auth_status: Some("signed_out".into()),
            binary_present: false,
            auth_detail: Some("Sign in on the workshop host".into()),
        };
        let connection = connection_from_runtime("cursor", "Cursor", Some(&runtime));
        assert!(!connection.binary_present);
        assert_eq!(connection.auth_status, "signed_out");
        assert_eq!(connection.command.as_deref(), Some("/remote/bin/agent"));
        assert_eq!(
            connection.detail.as_deref(),
            Some("Sign in on the workshop host")
        );
    }

    #[test]
    fn absent_workshop_status_never_claims_local_installation_or_authentication() {
        let connection = connection_from_runtime("chatgpt", "ChatGPT", None);
        assert!(!connection.binary_present);
        assert_eq!(connection.auth_status, "unknown");
        assert!(connection.command.is_none());
    }
}
