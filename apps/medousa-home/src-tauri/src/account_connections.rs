//! Settings → Connections: ChatGPT (via Codex CLI) and Cursor account sign-in.
//!
//! Medousa never holds vendor tokens — login orchestrates the official CLIs
//! (`codex login`, `cursor agent login` / `agent login`), which keep credentials in their own
//! stores. We probe sign-in state via the daemon agents surface, start device
//! auth / terminal login from here, and shell out to `* logout` on sign-out.
//! Missing CLIs are installed via the vendors' official installers (not Packages).

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use medousa_types::AgentRuntimeInfo;
use serde::Serialize;
use tauri::State;
use tauri_plugin_opener::open_url;

use crate::daemon::{
    DaemonState,
    sdk::{client, sdk_error},
};

fn codex_command() -> String {
    std::env::var("MEDOUSA_ACP_CODEX_COMMAND").unwrap_or_else(|_| "codex".into())
}

fn cursor_command() -> String {
    std::env::var("MEDOUSA_ACP_CURSOR_COMMAND").unwrap_or_else(|_| "agent".into())
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
            "'cursor' / 'agent' not found — tap Install on the Cursor card in Settings → Connections"
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
    /// `chatgpt` (Codex CLI) or `cursor`.
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
    let command = match id {
        "chatgpt" => Some(codex_command()),
        "cursor" => Some(cursor_command()),
        _ => None,
    };
    let binary_present = info
        .map(|i| i.binary_present)
        .unwrap_or_else(|| command.as_deref().map(command_on_path).unwrap_or(false));
    // Prefer local discovery when the daemon hasn't picked up a just-installed CLI yet.
    let binary_present = binary_present
        || command.as_deref().map(command_on_path).unwrap_or(false)
        || (id == "cursor" && (command_on_path("agent") || command_on_path("cursor-agent")));

    let mut auth_status = info
        .and_then(|i| i.auth_status.clone())
        .unwrap_or_else(|| "unknown".into());
    let mut detail = info.and_then(|i| i.auth_detail.clone().or(i.detail.clone()));

    // Cursor tokens live in the OS keychain — file probes on the daemon often
    // lag. Ask the local CLI so Connections flips to signed-in right after login.
    if id == "cursor" && auth_status != "signed_in" {
        if let Some((local_status, local_detail)) = local_cursor_auth_status() {
            auth_status = local_status;
            if let Some(local_detail) = local_detail {
                detail = Some(local_detail);
            }
        }
    }

    AccountConnectionInfo {
        id: id.to_string(),
        label: label.to_string(),
        runtime: match id {
            "chatgpt" => "codex".into(),
            _ => "cursor".into(),
        },
        binary_present,
        command: info.and_then(|i| i.command.clone()).or(command),
        auth_status,
        detail,
    }
}

/// Run `agent status --format json` locally (tokens are in keychain, not files).
fn local_cursor_auth_status() -> Option<(String, Option<String>)> {
    ensure_vendor_cli_path();
    let program = if command_on_path("agent") {
        resolve_command_abs("agent")
    } else if command_on_path("cursor-agent") {
        resolve_command_abs("cursor-agent")
    } else {
        return None;
    };
    let output = Command::new(&program)
        .args(["status", "--format", "json"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let value: serde_json::Value = serde_json::from_str(text.trim()).ok()?;
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
            "signed_in".into(),
            message.or_else(|| Some("Cursor CLI reports signed in".into())),
        ))
    } else {
        Some((
            "signed_out".into(),
            message.or_else(|| Some("Cursor CLI reports signed out".into())),
        ))
    }
}

/// Probe both accounts (via the daemon agents surface when reachable; falls
/// back to local binary presence when the engine is down).
#[tauri::command]
pub async fn account_connections_probe(
    state: State<'_, DaemonState>,
) -> Result<AccountConnections, String> {
    ensure_vendor_cli_path();
    let runtimes = client(&state)
        .agents()
        .list_runtimes()
        .await
        .map_err(sdk_error)?
        .runtimes;
    let codex = runtimes.iter().find(|r| r.runtime == "codex");
    let cursor = runtimes.iter().find(|r| r.runtime == "cursor");
    Ok(AccountConnections {
        chatgpt: connection_from_runtime("chatgpt", "ChatGPT", codex),
        cursor: connection_from_runtime("cursor", "Cursor", cursor),
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

/// Install the vendor CLI for an account using the official installer.
/// ChatGPT → Codex (`chatgpt.com/codex/install`); Cursor → Agent CLI (`cursor.com/install`).
#[tauri::command]
pub async fn account_cli_install(account: String) -> Result<AccountCliInstallResult, String> {
    let (command, label) = match account.as_str() {
        "chatgpt" => (codex_command(), "Codex CLI"),
        "cursor" => (cursor_command(), "Cursor Agent CLI"),
        other => return Err(format!("unknown account '{other}'")),
    };
    ensure_vendor_cli_path();
    if command_on_path(&command) {
        let mut detail = format!("{label} is already installed.");
        if account == "chatgpt" {
            match ensure_codex_acp_adapter() {
                Ok(note) => detail = format!("{detail} {note}"),
                Err(err) => {
                    detail = format!(
                        "{detail} (Codex ACP adapter not installed yet: {err}. \
                         Medousa will try `npx -y @agentclientprotocol/codex-acp@1.1.14` at runtime.)"
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
            _ => unreachable!(),
        };
        run_install_shell(script)?;
        ensure_vendor_cli_path();
        if !command_on_path(&command_owned) {
            return Err(format!(
                "{label_owned} installed, but '{command_owned}' is still not on PATH. \
                 Quit and reopen Medousa, or add ~/.local/bin to your PATH."
            ));
        }
        // Codex ACP needs the separate adapter (stock `codex` has no `acp` subcommand).
        let mut detail = format!("{label_owned} installed — you can sign in now.");
        if account_owned == "chatgpt" {
            match ensure_codex_acp_adapter() {
                Ok(note) => detail = format!("{detail} {note}"),
                Err(err) => {
                    detail = format!(
                        "{detail} (Codex ACP adapter not installed yet: {err}. \
                         Medousa will try `npx -y @agentclientprotocol/codex-acp@1.1.14` at runtime.)"
                    );
                }
            }
        }
        Ok(AccountCliInstallResult {
            account: account_owned,
            command: command_owned,
            detail,
        })
    })
    .await
    .map_err(|err| format!("install task failed: {err}"))?
}

/// Install the protocol-tested Codex ACP adapter so ChatGPT runtime can speak ACP.
fn ensure_codex_acp_adapter() -> Result<String, String> {
    ensure_vendor_cli_path();
    if command_on_path("codex-acp") {
        return Ok("Codex ACP adapter already present.".into());
    }
    let npm = ["npm", "npm.cmd"]
        .into_iter()
        .find(|name| command_on_path(name))
        .ok_or_else(|| "npm not found".to_string())?;
    let mut command = Command::new(npm);
    command
        .args(["install", "-g", "@agentclientprotocol/codex-acp@1.1.14"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    detach_new_session(&mut command);
    let output = command
        .output()
        .map_err(|err| format!("couldn't start npm: {err}"))?;
    if !output.status.success() {
        let combined = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return Err(format!(
            "npm install failed (exit {}): {}",
            output.status.code().unwrap_or(-1),
            combined.trim()
        ));
    }
    ensure_vendor_cli_path();
    if command_on_path("codex-acp") {
        Ok("Codex ACP adapter installed.".into())
    } else {
        Ok(
            "npm installed the adapter; restart Medousa if runtime still can't find `codex-acp`."
                .into(),
        )
    }
}

/// ChatGPT login → `codex login --device-auth`, parse the verification URL +
/// user code from stdout, open the system browser, and let the process finish
/// in the background (it polls until the user approves).
#[tauri::command]
pub async fn account_chatgpt_begin_device_login() -> Result<DeviceAuthStart, String> {
    let command = codex_command();
    if !command_on_path(&command) {
        return Err(format!(
            "'{command}' not found — tap Install on the ChatGPT card in Settings → Connections"
        ));
    }

    let output = Command::new(&command).args(["login", "status"]).output();
    if let Ok(status) = output {
        let text = String::from_utf8_lossy(&status.stdout);
        let normalized = text.to_lowercase();
        if normalized.contains("logged in using chatgpt") {
            return Err("Already signed in to ChatGPT — refresh status instead".into());
        }
        if normalized.contains("api key") || normalized.contains("apikey") {
            let logout = Command::new(&command)
                .arg("logout")
                .output()
                .map_err(|err| format!("couldn't replace Codex API-key login: {err}"))?;
            if !logout.status.success() {
                return Err(
                    "Codex is using an API key and could not sign out — run `codex logout`, then connect ChatGPT again"
                        .into(),
                );
            }
        }
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

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "codex login produced no stdout".to_string())?;
    let mut reader = std::io::BufReader::new(stdout);
    let mut url: Option<String> = None;
    let mut code: Option<String> = None;
    let mut transcript = String::new();

    use std::io::BufRead;
    let started = std::time::Instant::now();
    while started.elapsed() < std::time::Duration::from_secs(30) {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                transcript.push_str(&line);
                let trimmed = line.trim();
                if url.is_none() && trimmed.starts_with("http") {
                    url = Some(trimmed.to_string());
                }
                if let Some(idx) = trimmed.find("http") {
                    if url.is_none() {
                        url = Some(
                            trimmed[idx..]
                                .split_whitespace()
                                .next()
                                .unwrap_or("")
                                .to_string(),
                        );
                    }
                }
                // Device codes look like XXXX-XXXX or 8+ alnum groups.
                if code.is_none() {
                    for token in trimmed.split_whitespace() {
                        let clean = token.trim_matches(|c: char| !c.is_alphanumeric() && c != '-');
                        if clean.len() >= 6
                            && clean.chars().filter(|c| c.is_alphanumeric()).count() >= 6
                            && !clean.starts_with("http")
                        {
                            code = Some(clean.to_string());
                            break;
                        }
                    }
                }
                if url.is_some() {
                    break;
                }
            }
            Err(_) => break,
        }
    }

    let url = url.ok_or_else(|| {
        let _ = child.kill();
        format!(
            "couldn't read a sign-in link from codex login (output so far: {})",
            transcript.trim().chars().take(300).collect::<String>()
        )
    })?;

    // Let the child keep polling for approval in the background.
    std::thread::spawn(move || {
        let _ = child.wait();
    });

    open_url(&url, None::<&str>).map_err(|err| err.to_string())?;
    Ok(DeviceAuthStart {
        url,
        code,
        detail: Some(
            "Approve in the browser that just opened — codex finishes sign-in on its own.".into(),
        ),
    })
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
        return Ok(());
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
        return Err("no terminal emulator found to run the login".into());
    }
}

/// Fallback / Cursor login: run the vendor CLI's interactive login in a
/// terminal window (browser callback flows need a TTY).
#[tauri::command]
pub async fn account_begin_terminal_login(account: String) -> Result<String, String> {
    let (command, args, hint) = match account.as_str() {
        "chatgpt" => {
            let cmd = codex_command();
            if !command_on_path(&cmd) {
                return Err(
                    "'codex' not found — tap Install on the ChatGPT card in Settings → Connections"
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
        other => return Err(format!("unknown account '{other}'")),
    };
    open_terminal_with_login(&command, &args)?;
    Ok(format!(
        "Sign in with `{hint}` in the terminal that opened, then return here — status updates automatically."
    ))
}

#[tauri::command]
pub async fn account_sign_out(account: String) -> Result<String, String> {
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
