use super::model::{Target, TargetConfig};
use anyhow::{Result, bail};
use base64::Engine;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

pub const OUTPUT_LIMIT: usize = 64 * 1024;

pub fn validate(config: &TargetConfig) -> Result<()> {
    if config.name.trim().is_empty()
        || config.name.len() > 80
        || config.name.chars().any(char::is_control)
    {
        bail!("name must be between 1 and 80 characters");
    }
    if config.host.is_empty()
        || config.host.len() > 253
        || config.host.starts_with('-')
        || !config
            .host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-_:".contains(&b))
    {
        bail!("provide a hostname or IP address without SSH options");
    }
    if config.port == 0
        || config.username.is_empty()
        || config.username.len() > 64
        || !config
            .username
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        || config.username.starts_with('-')
    {
        bail!("provide a valid SSH username and port");
    }
    if let Some(path) = &config.identity_file
        && (!Path::new(path).is_absolute()
            || path.chars().any(char::is_control)
            || path.len() > 4096)
    {
        bail!("identity file must be an absolute path on the workshop");
    }
    Ok(())
}

pub fn normalize_keys(keys: &[String]) -> Result<Vec<String>> {
    if keys.is_empty() || keys.len() > 8 || keys.iter().map(String::len).sum::<usize>() > 4096 {
        bail!("verify at least one server host key");
    }
    keys.iter()
        .map(|key| {
            let fields: Vec<_> = key.split_whitespace().collect();
            if fields.len() != 2
                || !matches!(
                    fields[0],
                    "ssh-ed25519"
                        | "ecdsa-sha2-nistp256"
                        | "ecdsa-sha2-nistp384"
                        | "ecdsa-sha2-nistp521"
                        | "ssh-rsa"
                )
            {
                bail!("invalid public server host key");
            }
            let decoded = base64::engine::general_purpose::STANDARD.decode(fields[1])?;
            if decoded.len() < 32 || decoded.len() > 8192 {
                bail!("invalid public server host key size");
            }
            Ok(format!("{} {}", fields[0], fields[1]))
        })
        .collect()
}

/// OpenSSH versions differ in whether banner comments are emitted on stdout.
/// Only known_hosts records contribute keys; comments never become key types.
pub(super) fn parse_keyscan(text: &str) -> Result<Vec<String>> {
    let mut keys = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split_whitespace();
        let _host = fields.next();
        let (Some(kind), Some(blob)) = (fields.next(), fields.next()) else {
            bail!("invalid server host key response");
        };
        keys.push(format!("{kind} {blob}"));
    }
    keys.sort();
    keys.dedup();
    normalize_keys(&keys)
}

/// Ignore user SSH config: a saved target grants exactly one endpoint, not an
/// arbitrary ProxyCommand, port forward, or command from ambient configuration.
pub fn argv(target: &Target, known_hosts: &Path, command: Option<&str>) -> Vec<String> {
    let mut args: Vec<String> = [
        "-F",
        "none",
        "-o",
        "BatchMode=yes",
        "-o",
        "StrictHostKeyChecking=yes",
        "-o",
        "GlobalKnownHostsFile=none",
        "-o",
        "ForwardAgent=no",
        "-o",
        "ClearAllForwardings=yes",
        "-o",
        "ConnectTimeout=8",
        "-o",
        "ServerAliveInterval=15",
        "-o",
        "ServerAliveCountMax=3",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    args.extend([
        "-o".into(),
        format!(
            "UserKnownHostsFile=\"{}\"",
            known_hosts
                .to_string_lossy()
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
        ),
        "-o".into(),
        format!("HostKeyAlias={}", target.id),
        "-p".into(),
        target.config.port.to_string(),
        "-l".into(),
        target.config.username.clone(),
    ]);
    if let Some(path) = &target.config.identity_file {
        args.extend([
            "-o".into(),
            "IdentitiesOnly=yes".into(),
            "-i".into(),
            path.clone(),
        ]);
    }
    args.push(if command.is_some() { "-T" } else { "-tt" }.into());
    args.push(target.config.host.clone());
    if let Some(command) = command {
        args.push(command.into());
    }
    args
}

pub async fn drain(
    mut stream: impl tokio::io::AsyncRead + Unpin,
) -> std::io::Result<(String, bool)> {
    let mut kept = Vec::new();
    let mut truncated = false;
    let mut bytes = [0; 8192];
    loop {
        let count = stream.read(&mut bytes).await?;
        if count == 0 {
            break;
        }
        let retain = count.min(OUTPUT_LIMIT.saturating_sub(kept.len()));
        kept.extend_from_slice(&bytes[..retain]);
        truncated |= retain < count;
    }
    Ok((String::from_utf8_lossy(&kept).into_owned(), truncated))
}

pub fn ssh_command(args: &[String]) -> Command {
    ssh_command_for(Path::new("ssh"), args)
}

pub fn ssh_command_for(program: &Path, args: &[String]) -> Command {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    medousa_host::hide_tokio_subprocess_window(&mut command);
    command
}

pub async fn inspect(config: TargetConfig) -> Result<serde_json::Value> {
    validate(&config)?;
    let mut scan = Command::new("ssh-keyscan");
    scan.args([
        "-T",
        "8",
        "-p",
        &config.port.to_string(),
        "-t",
        "ed25519,ecdsa,rsa",
        &config.host,
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::null())
    .kill_on_drop(true);
    medousa_host::hide_tokio_subprocess_window(&mut scan);
    let mut child = scan.spawn()?;
    let output = drain(child.stdout.take().expect("piped stdout"));
    let (wait, output) = tokio::join!(child.wait(), output);
    if !wait?.success() {
        bail!("server did not return SSH host keys; check address and port");
    }
    let (text, truncated) = output?;
    if truncated {
        bail!("server host key response exceeded the limit");
    }
    let keys = parse_keyscan(&text)?;
    let fingerprints: Vec<_> = keys
        .iter()
        .map(|key| {
            let blob = key.split_whitespace().nth(1).expect("validated key");
            use sha2::{Digest, Sha256};
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(blob)
                .expect("validated base64");
            format!(
                "{} SHA256:{}",
                key.split_whitespace().next().unwrap(),
                base64::engine::general_purpose::STANDARD_NO_PAD.encode(Sha256::digest(bytes))
            )
        })
        .collect();
    Ok(serde_json::json!({"host_keys": keys, "fingerprints": fingerprints}))
}

pub async fn prepare_known_hosts(root: &Path, target: &Target) -> Result<PathBuf> {
    let dir = root.join("known_hosts");
    tokio::fs::create_dir_all(&dir).await?;
    let path = dir.join(&target.id);
    let content: String = target
        .host_keys
        .iter()
        .map(|key| format!("{} {key}\n", target.id))
        .collect();
    // Unique target IDs make these immutable, and no private key is copied here.
    use tokio::io::AsyncWriteExt;
    let temporary = dir.join(format!(".{}", uuid::Uuid::new_v4()));
    let mut file = tokio::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .await?;
    file.write_all(content.as_bytes()).await?;
    file.sync_all().await?;
    drop(file);
    tokio::fs::rename(&temporary, &path).await?;
    Ok(path)
}
