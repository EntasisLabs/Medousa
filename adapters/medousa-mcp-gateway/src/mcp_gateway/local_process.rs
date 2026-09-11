//! Local gateway restart admission. Connected clients are never restart targets.

use anyhow::{Context, Result, bail};
use std::net::SocketAddr;
#[cfg(unix)]
use std::time::Duration;

#[cfg(unix)]
#[derive(Debug, PartialEq, Eq)]
struct Listener {
    pid: u32,
    command: String,
}

#[cfg(unix)]
fn parse_listeners(output: &str) -> Result<Vec<Listener>> {
    let mut listeners: Vec<Listener> = Vec::new();
    for line in output.lines() {
        if let Some(pid) = line.strip_prefix('p') {
            listeners.push(Listener {
                pid: pid.parse().context("invalid listener PID")?,
                command: String::new(),
            });
        } else if let Some(command) = line.strip_prefix('c') {
            listeners
                .last_mut()
                .context("listener command without PID")?
                .command = command.into();
        }
    }
    Ok(listeners)
}

#[cfg(unix)]
async fn listeners_on_port(port: u16) -> Result<Vec<Listener>> {
    let mut command = tokio::process::Command::new("lsof");
    command.args([
        "-nP",
        "+c0",
        "-a",
        &format!("-iTCP:{port}"),
        "-sTCP:LISTEN",
        "-Fpc",
    ]);
    command.kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(5), command.output())
        .await
        .context("gateway listener inspection timed out")??;
    if output.status.code() == Some(1) && output.stdout.is_empty() && output.stderr.is_empty() {
        return Ok(Vec::new());
    }
    if !output.status.success() {
        bail!("could not inspect the gateway listener");
    }
    let listeners = parse_listeners(std::str::from_utf8(&output.stdout)?)?;
    // Linux process names can be truncated; use the executable identity rather
    // than accepting a shortened name shared with other processes.
    #[cfg(target_os = "linux")]
    let listeners = {
        let mut listeners = listeners;
        for listener in &mut listeners {
            let executable = tokio::fs::read_link(format!("/proc/{}/exe", listener.pid)).await?;
            listener.command = executable
                .file_name()
                .and_then(|name| name.to_str())
                .context("gateway listener has no executable name")?
                .trim_end_matches(" (deleted)")
                .to_string();
        }
        listeners
    };
    Ok(listeners)
}

#[cfg(unix)]
fn gateway_pid(listeners: &[Listener], own_pid: u32) -> Result<Option<u32>> {
    let [listener] = listeners else {
        if listeners.is_empty() {
            return Ok(None);
        }
        bail!("multiple listeners occupy the MCP gateway port; refusing to stop them");
    };
    if listener.pid <= 1 || listener.pid == own_pid || listener.command != "medousa_mcp_gateway" {
        bail!("the MCP gateway port belongs to another process; refusing to stop it");
    }
    Ok(Some(listener.pid))
}

/// Stops only a verified gateway listener, never arbitrary users of its port.
/// All OS waits run asynchronously and have a deadline.
pub async fn stop_local_gateway(bind: &str) -> Result<()> {
    let address: SocketAddr = bind
        .parse()
        .context("MCP restart requires a numeric local bind address")?;
    if !address.ip().is_loopback() {
        bail!("MCP restart requires a loopback bind address");
    }
    #[cfg(unix)]
    {
        let Some(pid) = gateway_pid(
            &listeners_on_port(address.port()).await?,
            std::process::id(),
        )?
        else {
            return Ok(());
        };
        // Recheck ownership immediately before signalling; do not act on stale
        // discovery results from an earlier status request.
        if gateway_pid(
            &listeners_on_port(address.port()).await?,
            std::process::id(),
        )? != Some(pid)
        {
            bail!("MCP gateway listener changed during restart; retry after checking its status");
        }
        let mut command = tokio::process::Command::new("kill");
        command.args(["-TERM", &pid.to_string()]).kill_on_drop(true);
        let status = tokio::time::timeout(Duration::from_secs(5), command.status())
            .await
            .context("MCP gateway stop timed out")??;
        if !status.success() {
            bail!("could not stop the MCP gateway process");
        }
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            if listeners_on_port(address.port()).await?.is_empty() {
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                bail!("MCP gateway has not released its listener after stopping");
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    #[cfg(not(unix))]
    bail!("Stop the running MCP gateway before restarting it on this platform");
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn restart_refuses_clients_other_processes_and_ambiguous_listeners() {
        let own = 42;
        assert!(gateway_pid(&parse_listeners("p42\ncmedousa-home\n").unwrap(), own).is_err());
        assert!(gateway_pid(&parse_listeners("p43\ncmedousa_daemon\n").unwrap(), own).is_err());
        assert!(
            gateway_pid(
                &parse_listeners("p42\ncmedousa_mcp_gateway\n").unwrap(),
                own
            )
            .is_err()
        );
        assert!(gateway_pid(&parse_listeners("p0\ncmedousa_mcp_gateway\n").unwrap(), own).is_err());
        assert!(
            gateway_pid(
                &parse_listeners("p43\ncmedousa_mcp_gateway\np44\ncmedousa_mcp_gateway\n").unwrap(),
                own
            )
            .is_err()
        );
        assert_eq!(
            gateway_pid(
                &parse_listeners("p43\ncmedousa_mcp_gateway\n").unwrap(),
                own
            )
            .unwrap(),
            Some(43)
        );
    }

    #[test]
    fn connected_client_fixture() {
        let Ok(address) = std::env::var("MEDOUSA_MCP_LISTENER_TEST_ADDR") else {
            return;
        };
        let mut stream = std::net::TcpStream::connect(address).unwrap();
        use std::io::Read;
        stream.read_exact(&mut [0]).unwrap();
    }

    #[tokio::test]
    async fn listener_probe_excludes_an_established_client_process() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let mut child = tokio::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "mcp_gateway::local_process::tests::connected_client_fixture",
            ])
            .env("MEDOUSA_MCP_LISTENER_TEST_ADDR", address.to_string())
            .stdout(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let (mut stream, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
            .await
            .unwrap()
            .unwrap();
        let listeners = listeners_on_port(address.port()).await.unwrap();
        assert!(
            listeners
                .iter()
                .any(|listener| listener.pid == std::process::id())
        );
        assert!(
            listeners
                .iter()
                .all(|listener| Some(listener.pid) != child.id())
        );
        assert!(stop_local_gateway(&address.to_string()).await.is_err());
        use tokio::io::AsyncWriteExt;
        stream.write_all(&[1]).await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_secs(5), child.wait())
                .await
                .unwrap()
                .unwrap()
                .success()
        );
    }
}
