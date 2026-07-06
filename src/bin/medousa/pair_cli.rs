use std::net::{TcpStream, ToSocketAddrs};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use medousa::daemon_api::{
    detect_lan_ipv4, resolve_daemon_url, DEFAULT_DAEMON_PORT, DEFAULT_DAEMON_URL,
};
use serde_json::Value;

pub fn run_pair(args: &[String]) -> Result<()> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_pair_help();
        return Ok(());
    }

    let daemon_url = resolve_pair_daemon_url(args);
    match args.first().map(String::as_str) {
        None | Some("status") | Some("list") => run_pair_status(&daemon_url),
        Some("qr") => run_pair_qr(&daemon_url, args),
        Some("remove") => run_pair_remove(&resolve_pair_remove_daemon_url(args), args),
        Some("lan") => run_pair_lan(args),
        Some(other) => bail!(
            "unknown pair subcommand '{other}'. run 'medousa pair --help' for usage"
        ),
    }
}

fn run_pair_status(daemon_url: &str) -> Result<()> {
    let client = http_client()?;
    let response = client
        .get(format!("{daemon_url}/pair/status"))
        .send()
        .context("GET /pair/status")?;
    if !response.status().is_success() {
        bail!("GET /pair/status returned {}", response.status());
    }
    let body: Value = response.json().context("parse /pair/status json")?;
    let peer_name = body
        .get("peerName")
        .and_then(Value::as_str)
        .unwrap_or("-");
    let device_id = body
        .get("deviceId")
        .and_then(Value::as_str)
        .unwrap_or("-");
    println!("Workshop: {peer_name} ({device_id})");
    println!("LAN pairing window: {}", lan_window_label());

    if let Some(devices) = body.get("pairedDevices").and_then(Value::as_array) {
        if devices.is_empty() {
            println!("No paired surfaces.");
        } else {
            println!("ID\tROLE\tNAME\tLAST SEEN");
            for device in devices {
                let pairing_id = device
                    .get("pairingId")
                    .and_then(Value::as_str)
                    .unwrap_or("-");
                let role = device
                    .get("role")
                    .and_then(Value::as_str)
                    .unwrap_or("portal");
                let name = device
                    .get("phoneName")
                    .and_then(Value::as_str)
                    .unwrap_or("-");
                let last_seen = device
                    .get("lastSeen")
                    .and_then(Value::as_str)
                    .unwrap_or("-");
                let short_id = pairing_id.chars().take(8).collect::<String>();
                println!("{short_id}\t{role}\t{name}\t{last_seen}");
            }
        }
    } else {
        println!("{}", serde_json::to_string_pretty(&body)?);
    }
    Ok(())
}

fn run_pair_qr(daemon_url: &str, args: &[String]) -> Result<()> {
    let client = http_client()?;
    let path = if has_flag(args, "--full") {
        format!("{daemon_url}/qr?full=true")
    } else {
        format!("{daemon_url}/qr")
    };
    let response = client.get(&path).send().context("GET /qr")?;
    if !response.status().is_success() {
        bail!("GET /qr returned {}", response.status());
    }
    let body: Value = response.json().context("parse /qr json")?;
    let url = body
        .get("url")
        .and_then(Value::as_str)
        .context("missing url in /qr response")?;
    let short_code = body
        .get("shortCode")
        .and_then(Value::as_str)
        .unwrap_or("-");
    let expires_at = body
        .get("expiresAt")
        .and_then(Value::as_str)
        .unwrap_or("-");

    println!("Pairing URL:\n{url}");
    if url.contains("medousa://pair/2.0") {
        println!("Protocol: QR v2 (LAN + Iroh ticket — large; for paste only)");
    } else {
        println!("Protocol: QR v1 compact (camera-friendly; Iroh ticket fetched after pair)");
    }
    println!("Short code: {short_code}");
    println!("Expires: {expires_at}");

    if has_flag(args, "--term") {
        print_terminal_qr(url)?;
    }
    if has_flag(args, "--open") {
        open_url_in_browser(url)?;
    }
    Ok(())
}

fn run_pair_remove(daemon_url: &str, args: &[String]) -> Result<()> {
    let pairing_id = args
        .get(1)
        .map(String::as_str)
        .filter(|value| !value.starts_with("--"))
        .context("usage: medousa pair remove <pairing_id>")?;
    let client = http_client()?;
    let response = client
        .delete(format!("{daemon_url}/pair/{pairing_id}"))
        .send()
        .context("DELETE /pair/{pairing_id}")?;
    match response.status().as_u16() {
        204 => {
            println!("Removed pairing {pairing_id}");
            Ok(())
        }
        401 | 403 => bail!(
            "DELETE /pair/{pairing_id} unauthorized — use http://127.0.0.1:7419 from this host, or pass the paired device's session token as Authorization: Bearer"
        ),
        404 => bail!("pairing not found: {pairing_id}"),
        status => bail!("DELETE /pair/{pairing_id} returned {status}"),
    }
}

fn run_pair_lan(args: &[String]) -> Result<()> {
    match args.get(1).map(String::as_str) {
        None | Some("status") => {
            println!("LAN pairing window: {}", lan_window_label());
            if let Some(ip) = detect_lan_ipv4() {
                println!("LAN address: {ip}:{DEFAULT_DAEMON_PORT}");
            }
            println!("Open:  medousa pair lan on");
            println!("Close: medousa pair lan off");
            Ok(())
        }
        Some("on") => restart_daemon_public(true),
        Some("off") => restart_daemon_public(false),
        Some(other) => bail!(
            "unknown lan subcommand '{other}'. try: medousa pair lan status|on|off"
        ),
    }
}

fn restart_daemon_public(public: bool) -> Result<()> {
    let exe = std::env::current_exe().context("resolve medousa binary")?;
    let mut command = Command::new(exe);
    command.arg("start").arg("daemon-restart");
    if public {
        command.arg("--public");
        println!("Opening LAN pairing window (bind 0.0.0.0)…");
    } else {
        println!("Closing LAN pairing window (bind loopback)…");
    }
    let status = command
        .status()
        .context("restart daemon for LAN pairing window")?;
    if !status.success() {
        bail!("daemon restart failed with status {status}");
    }
    println!("LAN pairing window: {}", lan_window_label());
    Ok(())
}

fn lan_window_label() -> &'static str {
    if lan_pairing_window_open() {
        "open"
    } else {
        "closed"
    }
}

fn lan_pairing_window_open() -> bool {
    let Some(ip) = detect_lan_ipv4() else {
        return false;
    };
    let bind = format!("{ip}:{DEFAULT_DAEMON_PORT}");
    if let Ok(mut addrs) = bind.to_socket_addrs()
        && let Some(addr) = addrs.next()
    {
        return TcpStream::connect_timeout(&addr, Duration::from_millis(250)).is_ok();
    }
    false
}

fn resolve_pair_remove_daemon_url(args: &[String]) -> String {
    find_arg_value(args, "--daemon-url")
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_DAEMON_URL.to_string())
}

fn resolve_pair_daemon_url(args: &[String]) -> String {
    find_arg_value(args, "--daemon-url")
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| resolve_daemon_url(None))
}

fn http_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .context("build HTTP client")
}

fn print_terminal_qr(url: &str) -> Result<()> {
    use qrcode::QrCode;
    let code = QrCode::new(url.as_bytes()).context("build terminal qr")?;
    let string = code
        .render::<char>()
        .quiet_zone(true)
        .module_dimensions(2, 1)
        .build();
    println!("\n{string}");
    Ok(())
}

fn open_url_in_browser(url: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .status()
            .context("open pairing url")?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .status()
            .context("open pairing url")?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .status()
            .context("open pairing url")?;
    }
    Ok(())
}

fn print_pair_help() {
    println!("Medousa host pairing (headless)");
    println!();
    println!("USAGE:");
    println!("  medousa pair status [--daemon-url <url>]");
    println!("  medousa pair list [--daemon-url <url>]");
    println!("  medousa pair qr [--term] [--open] [--full] [--daemon-url <url>]");
    println!("  medousa pair remove <pairing_id> [--daemon-url <url>]");
    println!("  medousa pair lan status|on|off");
    println!();
    println!("LAN pairing window binds the engine to 0.0.0.0 so peers can GET /qr.");
    println!("Turn it off after pairing — clients keep working over Iroh.");
    println!();
    println!("Remove defaults to {DEFAULT_DAEMON_URL} (loopback admin).");
    println!("Connect as a client: medousa peer --help");
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

fn find_arg_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == flag)
        .and_then(|index| args.get(index + 1))
        .cloned()
}
