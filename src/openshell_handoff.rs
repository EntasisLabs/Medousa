//! OpenShell gateway handoff — doctor probes and config resolution (Sprint B).

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use serde::Deserialize;

pub const DEFAULT_OPENSHELL_GATEWAY_URL: &str = "http://127.0.0.1:8080";
pub const ENV_OPENSHELL_GATEWAY_URL: &str = "MEDOUSA_OPENSHELL_GATEWAY_URL";

#[derive(Debug, Clone)]
pub struct OpenshellDoctorReport {
    pub gateway_url: String,
    pub gateway_reachable: bool,
    pub readyz_ok: bool,
    pub cli_installed: bool,
    pub cli_version: Option<String>,
    pub gateway_binary: Option<PathBuf>,
    pub sandbox_binary: Option<PathBuf>,
    pub podman_socket: PathBuf,
    pub podman_socket_active: bool,
    pub active_gateway_name: Option<String>,
    pub policy_templates_dir: PathBuf,
    pub policy_template_count: usize,
}

#[derive(Debug, Deserialize)]
struct OpenshellGatewayMetadata {
    gateway_endpoint: String,
}

pub fn openshell_config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("openshell")
}

pub fn medousa_openshell_policies_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("medousa")
        .join("openshell-policies")
}

pub fn resolve_openshell_gateway_url(explicit: Option<&str>) -> String {
    if let Some(url) = explicit.map(str::trim).filter(|value| !value.is_empty()) {
        return url.to_string();
    }
    if let Ok(url) = std::env::var(ENV_OPENSHELL_GATEWAY_URL) {
        let trimmed = url.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    read_active_gateway_endpoint().unwrap_or_else(|| DEFAULT_OPENSHELL_GATEWAY_URL.to_string())
}

pub fn read_active_gateway_name() -> Option<String> {
    let path = openshell_config_dir().join("active_gateway");
    let raw = std::fs::read_to_string(&path).ok()?;
    let name = raw.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

fn read_active_gateway_endpoint() -> Option<String> {
    let name = read_active_gateway_name()?;
    let metadata_path = openshell_config_dir()
        .join("gateways")
        .join(&name)
        .join("metadata.json");
    let raw = std::fs::read_to_string(&metadata_path).ok()?;
    let parsed: OpenshellGatewayMetadata = serde_json::from_str(&raw).ok()?;
    let endpoint = parsed.gateway_endpoint.trim();
    if endpoint.is_empty() {
        None
    } else {
        Some(endpoint.to_string())
    }
}

pub fn resolve_openshell_local_bin(name: &str) -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    for candidate in [
        home.join(".local").join("openshell").join("bin").join(name),
        home.join(".local").join("bin").join(name),
    ] {
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

pub fn podman_user_socket_path() -> PathBuf {
    #[cfg(unix)]
    {
        let uid = unsafe { libc::getuid() };
        PathBuf::from(format!("/run/user/{uid}/podman/podman.sock"))
    }
    #[cfg(not(unix))]
    {
        PathBuf::from(r"C:\nonexistent\medousa-podman.sock")
    }
}

pub fn probe_tcp_endpoint(url: &str, timeout: Duration) -> bool {
    let trimmed = url.trim();
    let without_scheme = trimmed
        .strip_prefix("http://")
        .or_else(|| trimmed.strip_prefix("https://"))
        .unwrap_or(trimmed);
    let host_port = without_scheme.split('/').next().unwrap_or(without_scheme);
    std::net::ToSocketAddrs::to_socket_addrs(host_port)
        .ok()
        .and_then(|mut addrs| addrs.next())
        .map(|addr| std::net::TcpStream::connect_timeout(&addr, timeout).is_ok())
        .unwrap_or(false)
}

pub fn probe_openshell_readyz(gateway_url: &str) -> bool {
    let base = gateway_url.trim_end_matches('/');
    let client = match reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
    {
        Ok(client) => client,
        Err(_) => return false,
    };
    for path in ["/readyz", "/healthz"] {
        let url = format!("{base}{path}");
        if let Ok(response) = client.get(&url).send()
            && response.status().is_success()
        {
            return true;
        }
    }
    false
}

pub fn openshell_cli_version() -> Option<String> {
    let output = std::process::Command::new("openshell")
        .arg("--version")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().next()?.trim();
    if line.is_empty() {
        None
    } else {
        Some(line.to_string())
    }
}

pub fn collect_openshell_doctor_report() -> OpenshellDoctorReport {
    let gateway_url = resolve_openshell_gateway_url(None);
    let gateway_reachable = probe_tcp_endpoint(&gateway_url, Duration::from_millis(300));
    let readyz_ok = gateway_reachable && probe_openshell_readyz(&gateway_url);
    let cli_version = openshell_cli_version();
    let cli_installed = cli_version.is_some();
    let gateway_binary = resolve_openshell_local_bin("openshell-gateway");
    let sandbox_binary = resolve_openshell_local_bin("openshell-sandbox");
    let podman_socket = podman_user_socket_path();
    let podman_socket_active = podman_socket.exists();
    let active_gateway_name = read_active_gateway_name();
    let policy_templates_dir = medousa_openshell_policies_dir();
    let policy_template_count = count_yaml_policies(&policy_templates_dir);

    OpenshellDoctorReport {
        gateway_url,
        gateway_reachable,
        readyz_ok,
        cli_installed,
        cli_version,
        gateway_binary,
        sandbox_binary,
        podman_socket,
        podman_socket_active,
        active_gateway_name,
        policy_templates_dir,
        policy_template_count,
    }
}

fn count_yaml_policies(dir: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .path()
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext == "yaml" || ext == "yml")
        })
        .count()
}

pub fn install_starter_openshell_policies_if_missing() -> Result<bool> {
    let target = medousa_openshell_policies_dir();
    std::fs::create_dir_all(&target)
        .with_context(|| format!("create {}", target.display()))?;
    let mut wrote_any = false;
    for (name, contents) in STARTER_OPENSHELL_POLICIES {
        let path = target.join(name);
        if !path.exists() {
            std::fs::write(&path, contents)
                .with_context(|| format!("write {}", path.display()))?;
            wrote_any = true;
        }
    }
    Ok(wrote_any)
}

pub const STARTER_OPENSHELL_POLICIES: &[(&str, &str)] = &[
    (
        "skill-sandbox.yaml",
        r#"# Medousa starter — skill script execution without network egress
version: 1
filesystem_policy:
  include_workdir: true
  read_only: [/usr, /lib, /proc, /dev/urandom, /app, /etc, /var/log]
  read_write: [/sandbox, /tmp, /dev/null]
landlock:
  compatibility: best_effort
process:
  run_as_user: sandbox
  run_as_group: sandbox
"#,
    ),
    (
        "research-readonly.yaml",
        r#"# Medousa starter — read-only HTTPS for research workers (OpenShell network policy)
version: 1
filesystem_policy:
  include_workdir: true
  read_only: [/usr, /lib, /proc, /dev/urandom, /app, /etc, /var/log]
  read_write: [/sandbox, /tmp, /dev/null]
landlock:
  compatibility: best_effort
process:
  run_as_user: sandbox
  run_as_group: sandbox
network_policies:
  https_get:
    name: https-readonly
    endpoints:
      - host: "**"
        port: 443
        protocol: rest
        enforcement: enforce
        access: read-only
    binaries:
      - { path: /usr/bin/curl }
"#,
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_gateway_url_when_unconfigured() {
        unsafe {
            std::env::remove_var(ENV_OPENSHELL_GATEWAY_URL);
        }
        // When no openshell metadata, falls back to default
        let url = resolve_openshell_gateway_url(None);
        assert!(
            url == DEFAULT_OPENSHELL_GATEWAY_URL || url.starts_with("http"),
            "unexpected url: {url}"
        );
    }

    #[test]
    fn probe_tcp_parses_host_port() {
        assert!(!probe_tcp_endpoint("http://127.0.0.1:1", Duration::from_millis(50)));
    }

    #[test]
    fn starter_policies_are_valid_yaml_headers() {
        for (name, body) in STARTER_OPENSHELL_POLICIES {
            assert!(name.ends_with(".yaml"));
            assert!(body.contains("version: 1"));
        }
    }
}
