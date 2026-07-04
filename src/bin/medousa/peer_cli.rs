//! Headless peer/portal client: connect, list, message, inbox.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use base64::Engine;
use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use medousa::daemon_api::{resolve_daemon_url, DEFAULT_DAEMON_URL};
use medousa::paths::medousa_data_dir;
use rand::RngCore;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CliConnection {
    id: String,
    role: String,
    label: String,
    workshop_device_id: String,
    daemon_url: String,
    pairing_id: String,
    phone_id: String,
    session_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    iroh_ticket: Option<String>,
    connected_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct CliConnectionStore {
    #[serde(default)]
    connections: Vec<CliConnection>,
}

pub fn run_peer(args: &[String]) -> Result<()> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_peer_help();
        return Ok(());
    }

    match args.first().map(String::as_str) {
        None | Some("help") => {
            print_peer_help();
            Ok(())
        }
        Some("nearby") => run_nearby(args),
        Some("connect") => run_connect(args),
        Some("list") => run_list(),
        Some("remove") => run_remove(args),
        Some("send") => run_send(args),
        Some("inbox") => run_inbox(args),
        Some("read") => run_read(args),
        Some(other) => bail!(
            "unknown peer subcommand '{other}'. run 'medousa peer --help' for usage"
        ),
    }
}

fn run_nearby(args: &[String]) -> Result<()> {
    let daemon_url = resolve_daemon_url_arg(args);
    let client = http_client()?;
    let response = client
        .get(format!("{daemon_url}/v1/lan/workshops"))
        .send()
        .context("GET /v1/lan/workshops")?;
    if !response.status().is_success() {
        bail!("GET /v1/lan/workshops returned {}", response.status());
    }
    let body: Value = response.json().context("parse workshops json")?;
    let workshops = body
        .get("workshops")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if workshops.is_empty() {
        println!("No workshops discovered on the LAN.");
        return Ok(());
    }
    println!("NAME\tDEVICE\tURL");
    for workshop in workshops {
        let name = workshop
            .get("peerName")
            .and_then(Value::as_str)
            .or_else(|| workshop.get("host").and_then(Value::as_str))
            .unwrap_or("-");
        let device = workshop
            .get("deviceId")
            .and_then(Value::as_str)
            .unwrap_or("-");
        let url = workshop
            .get("daemonUrl")
            .and_then(Value::as_str)
            .unwrap_or("-");
        let short = device.chars().take(8).collect::<String>();
        println!("{name}\t{short}\t{url}");
    }
    Ok(())
}

fn run_connect(args: &[String]) -> Result<()> {
    let daemon_url = args
        .get(1)
        .map(String::as_str)
        .filter(|value| !value.starts_with("--"))
        .context("usage: medousa peer connect <daemon-url> [--portal] [--name <label>]")?;
    let daemon_url = normalize_url(daemon_url);
    let role = if has_flag(args, "--portal") {
        "portal"
    } else {
        "peer"
    };
    let label = find_arg_value(args, "--name");

    let client = http_client()?;
    let qr = client
        .get(format!("{daemon_url}/qr"))
        .send()
        .context("GET /qr")?
        .error_for_status()
        .context("GET /qr failed")?
        .json::<Value>()
        .context("parse /qr")?;
    let qr_url = qr
        .get("url")
        .and_then(Value::as_str)
        .context("missing url in /qr response")?;

    let connection = complete_pair_ceremony(&client, qr_url, &daemon_url, role, label.as_deref())?;
    let mut store = load_store()?;
    store.connections.retain(|entry| entry.id != connection.id);
    println!(
        "Connected as {role} to {} ({})",
        connection.label, connection.id
    );
    println!("Daemon: {}", connection.daemon_url);
    if connection.iroh_ticket.is_some() {
        println!("Iroh ticket: saved (off-LAN capable)");
    }
    store.connections.push(connection);
    save_store(&store)?;
    Ok(())
}

fn run_list() -> Result<()> {
    let store = load_store()?;
    if store.connections.is_empty() {
        println!("No CLI connections. Use: medousa peer connect <url>");
        return Ok(());
    }
    println!("ID\tROLE\tLABEL\tURL");
    for entry in &store.connections {
        println!(
            "{}\t{}\t{}\t{}",
            entry.id, entry.role, entry.label, entry.daemon_url
        );
    }
    Ok(())
}

fn run_remove(args: &[String]) -> Result<()> {
    let id = args
        .get(1)
        .map(String::as_str)
        .filter(|value| !value.starts_with("--"))
        .context("usage: medousa peer remove <id>")?;
    let mut store = load_store()?;
    let before = store.connections.len();
    store.connections.retain(|entry| entry.id != id && !entry.label.eq_ignore_ascii_case(id));
    if store.connections.len() == before {
        bail!("connection not found: {id}");
    }
    save_store(&store)?;
    println!("Removed connection {id}");
    Ok(())
}

fn run_send(args: &[String]) -> Result<()> {
    let id = args
        .get(1)
        .map(String::as_str)
        .filter(|value| !value.starts_with("--"))
        .context("usage: medousa peer send <id|label> <message>")?;
    let message = args
        .get(2..)
        .map(|parts| parts.join(" "))
        .filter(|value| !value.trim().is_empty())
        .context("usage: medousa peer send <id|label> <message>")?;

    let store = load_store()?;
    let connection = find_connection(&store, id).context("connection not found")?;
    if connection.role != "peer" && connection.role != "portal" {
        bail!("connection role cannot send messages");
    }

    let sender = sender_identity()?;
    let client = http_client()?;
    let response = client
        .post(format!("{}/v1/peer/messages", connection.daemon_url))
        .bearer_auth(&connection.session_token)
        .json(&serde_json::json!({
            "body": message,
            "fromDeviceId": sender.device_id,
            "fromName": sender.peer_name,
        }))
        .send()
        .context("POST /v1/peer/messages")?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        bail!("send failed HTTP {status}: {body}");
    }
    println!("Sent to {}", connection.label);
    Ok(())
}

fn run_inbox(args: &[String]) -> Result<()> {
    let daemon_url = resolve_daemon_url_arg(args);
    let unread_only = has_flag(args, "--unread");
    let client = http_client()?;
    let mut url = format!("{daemon_url}/v1/peer/messages");
    if unread_only {
        url.push_str("?unreadOnly=true");
    }
    let response = client
        .get(&url)
        .send()
        .context("GET /v1/peer/messages")?;
    if !response.status().is_success() {
        bail!("GET /v1/peer/messages returned {}", response.status());
    }
    let body: Value = response.json().context("parse inbox json")?;
    let messages = body
        .get("messages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if messages.is_empty() {
        println!("Inbox empty.");
        return Ok(());
    }
    println!("ID\tFROM\tREAD\tBODY");
    for message in messages {
        let id = message.get("id").and_then(Value::as_str).unwrap_or("-");
        let from = message
            .get("fromName")
            .and_then(Value::as_str)
            .unwrap_or("-");
        let read = if message.get("readAt").and_then(Value::as_str).is_some() {
            "yes"
        } else {
            "no"
        };
        let body = message
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or("")
            .replace('\n', " ");
        let short_id = id.chars().take(12).collect::<String>();
        let preview: String = body.chars().take(60).collect();
        println!("{short_id}\t{from}\t{read}\t{preview}");
    }
    Ok(())
}

fn run_read(args: &[String]) -> Result<()> {
    let id = args
        .get(1)
        .map(String::as_str)
        .filter(|value| !value.starts_with("--"))
        .context("usage: medousa peer read <message-id>")?;
    let daemon_url = resolve_daemon_url_arg(args);
    let client = http_client()?;
    let response = client
        .post(format!("{daemon_url}/v1/peer/messages/{id}/read"))
        .send()
        .context("POST mark read")?;
    if !response.status().is_success() {
        bail!("mark read returned {}", response.status());
    }
    let body: Value = response.json().context("parse message json")?;
    let from = body.get("fromName").and_then(Value::as_str).unwrap_or("-");
    let text = body.get("body").and_then(Value::as_str).unwrap_or("");
    println!("From: {from}");
    println!("{text}");
    if let Some(result) = body.get("attachmentResult") {
        if let Some(summary) = result.get("summary").and_then(Value::as_str) {
            println!("Attachment: {summary}");
        }
    }
    Ok(())
}

fn complete_pair_ceremony(
    client: &reqwest::blocking::Client,
    qr_url: &str,
    daemon_url: &str,
    role: &str,
    label: Option<&str>,
) -> Result<CliConnection> {
    let status = client
        .get(format!("{daemon_url}/pair/status"))
        .send()
        .context("GET /pair/status")?
        .error_for_status()
        .context("pair status")?
        .json::<Value>()
        .context("parse pair status")?;
    let device_id = status
        .get("deviceId")
        .and_then(Value::as_str)
        .context("missing deviceId")?
        .to_string();
    let peer_name = status
        .get("peerName")
        .and_then(Value::as_str)
        .unwrap_or("Workshop")
        .to_string();
    let daemon_public_key = status
        .get("daemonPublicKey")
        .and_then(Value::as_str)
        .context("missing daemonPublicKey")?;

    let parsed = parse_pair_qr_url(qr_url)?;
    if parsed.device_id != device_id {
        bail!("pairing link does not match workshop device id — refresh QR and retry");
    }
    verify_qr_signature(&parsed, daemon_public_key)?;

    let identity = load_or_create_identity()?;
    let public_key = base64url_encode(identity.verifying_key.as_bytes());
    let display_name = label
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(if role == "peer" {
            "CLI Peer"
        } else {
            "CLI Portal"
        });

    let init = client
        .post(format!("{daemon_url}/pair/init"))
        .json(&serde_json::json!({
            "qrToken": parsed.qr_token,
            "phoneId": identity.phone_id,
            "phoneName": display_name,
            "publicKey": public_key,
            "role": role,
        }))
        .send()
        .context("POST /pair/init")?
        .error_for_status()
        .context("pair init failed")?
        .json::<Value>()
        .context("parse pair init")?;
    if init.get("status").and_then(Value::as_str) != Some("challenge") {
        let reason = init
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or("rejected");
        bail!("pair init failed: {reason}");
    }
    let server_nonce = init
        .get("serverNonce")
        .and_then(Value::as_str)
        .context("missing serverNonce")?;
    let session_id = init
        .get("sessionId")
        .and_then(Value::as_str)
        .context("missing sessionId")?;

    let signed_nonce = sign_message(&identity.signing_key, server_nonce);
    let mut phone_nonce = [0u8; 32];
    OsRng.fill_bytes(&mut phone_nonce);
    let phone_nonce_b64 = base64url_encode(&phone_nonce);

    let verify = client
        .post(format!("{daemon_url}/pair/verify"))
        .json(&serde_json::json!({
            "sessionId": session_id,
            "signedNonce": signed_nonce,
            "phoneNonce": phone_nonce_b64,
        }))
        .send()
        .context("POST /pair/verify")?
        .error_for_status()
        .context("pair verify failed")?
        .json::<Value>()
        .context("parse pair verify")?;
    if verify.get("status").and_then(Value::as_str) != Some("paired") {
        let reason = verify
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or("rejected");
        bail!("pair verify failed: {reason}");
    }
    let server_signed = verify
        .get("serverSignedNonce")
        .and_then(Value::as_str)
        .context("missing serverSignedNonce")?;
    let daemon_vk = parse_verifying_key(daemon_public_key)?;
    verify_message(&daemon_vk, &phone_nonce_b64, server_signed)
        .context("workshop signature check failed")?;

    let pairing_id = verify
        .get("pairingId")
        .and_then(Value::as_str)
        .context("missing pairingId")?
        .to_string();
    let session_token = verify
        .get("sessionToken")
        .and_then(Value::as_str)
        .context("missing sessionToken")?
        .to_string();

    let iroh_ticket = client
        .get(format!("{daemon_url}/pair/iroh-ticket"))
        .send()
        .ok()
        .and_then(|response| {
            if !response.status().is_success() {
                return None;
            }
            response.json::<Value>().ok()
        })
        .and_then(|body| {
            let ticket = body.get("ticket")?.as_str()?.trim();
            if ticket.is_empty() {
                None
            } else {
                Some(ticket.to_string())
            }
        })
        .or(parsed.iroh_ticket);

    let id = if role == "peer" {
        format!("peer-{device_id}")
    } else {
        format!("portal-{device_id}")
    };

    Ok(CliConnection {
        id,
        role: role.to_string(),
        label: peer_name,
        workshop_device_id: device_id,
        daemon_url: daemon_url.to_string(),
        pairing_id,
        phone_id: identity.phone_id,
        session_token,
        iroh_ticket,
        connected_at: chrono::Utc::now().to_rfc3339(),
    })
}

struct ParsedQr {
    address: String,
    device_id: String,
    qr_token: String,
    signature: String,
    iroh_ticket: Option<String>,
}

fn parse_pair_qr_url(raw: &str) -> Result<ParsedQr> {
    let trimmed = raw.trim();
    if !trimmed.starts_with("medousa://pair/") {
        bail!("pairing url must use medousa://pair/ scheme");
    }
    let query = trimmed
        .split_once('?')
        .map(|(_, query)| query)
        .unwrap_or("");
    let mut params: HashMap<String, String> = HashMap::new();
    for part in query.split('&') {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        let decoded = urlencoding::decode(value)
            .map(|owned| owned.into_owned())
            .unwrap_or_else(|_| value.to_string());
        if !decoded.trim().is_empty() {
            params.insert(key.to_string(), decoded);
        }
    }
    Ok(ParsedQr {
        address: params
            .get("a")
            .cloned()
            .context("pairing url missing a=")?,
        device_id: params
            .get("d")
            .cloned()
            .context("pairing url missing d=")?,
        qr_token: params
            .get("t")
            .cloned()
            .context("pairing url missing t=")?,
        signature: params
            .get("s")
            .cloned()
            .context("pairing url missing s=")?,
        iroh_ticket: params.get("k").cloned().filter(|value| !value.is_empty()),
    })
}

fn verify_qr_signature(parsed: &ParsedQr, daemon_public_key: &str) -> Result<()> {
    let message = if let Some(ticket) = &parsed.iroh_ticket {
        format!(
            "{}|{}|{}|{}",
            parsed.address, parsed.device_id, parsed.qr_token, ticket
        )
    } else {
        format!("{}|{}|{}", parsed.address, parsed.device_id, parsed.qr_token)
    };
    let key = parse_verifying_key(daemon_public_key)?;
    verify_message(&key, &message, &parsed.signature)
}

struct CliIdentity {
    phone_id: String,
    signing_key: SigningKey,
    verifying_key: VerifyingKey,
}

fn load_or_create_identity() -> Result<CliIdentity> {
    let path = cli_dir().join("identity.secret");
    if path.is_file() {
        let raw = fs::read_to_string(&path).context("read cli identity")?;
        let bytes = decode_secret_bytes(raw.trim())?;
        if bytes.len() != 32 {
            bail!("cli identity secret must be 32 bytes");
        }
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&bytes);
        let signing_key = SigningKey::from_bytes(&seed);
        let verifying_key = signing_key.verifying_key();
        return Ok(CliIdentity {
            phone_id: device_id_from_key(&verifying_key),
            signing_key,
            verifying_key,
        });
    }
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let signing_key = SigningKey::from_bytes(&bytes);
    let verifying_key = signing_key.verifying_key();
    fs::create_dir_all(cli_dir()).context("create cli dir")?;
    let encoded = bytes.iter().map(|byte| format!("{byte:02x}")).collect::<String>();
    fs::write(&path, format!("{encoded}\n")).context("write cli identity")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(CliIdentity {
        phone_id: device_id_from_key(&verifying_key),
        signing_key,
        verifying_key,
    })
}

fn decode_secret_bytes(raw: &str) -> Result<Vec<u8>> {
    if raw.len() == 64 && raw.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return (0..raw.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(&raw[index..index + 2], 16))
            .collect::<Result<Vec<_>, _>>()
            .context("decode hex identity");
    }
    base64url_decode(raw)
}

fn device_id_from_key(key: &VerifyingKey) -> String {
    let digest = Sha256::digest(key.as_bytes());
    digest[..4]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn sign_message(signing_key: &SigningKey, message: &str) -> String {
    let signature = signing_key.sign(message.as_bytes());
    base64url_encode(&signature.to_bytes())
}

fn verify_message(key: &VerifyingKey, message: &str, signature_b64: &str) -> Result<()> {
    let bytes = base64url_decode(signature_b64)?;
    let signature = ed25519_dalek::Signature::from_slice(&bytes)
        .map_err(|err| anyhow::anyhow!("invalid signature: {err}"))?;
    key.verify(message.as_bytes(), &signature)
        .map_err(|err| anyhow::anyhow!("signature verify failed: {err}"))
}

fn parse_verifying_key(raw: &str) -> Result<VerifyingKey> {
    let bytes = base64url_decode(raw)?;
    VerifyingKey::from_bytes(
        bytes
            .as_slice()
            .try_into()
            .map_err(|_| anyhow::anyhow!("public key must be 32 bytes"))?,
    )
    .map_err(|err| anyhow::anyhow!("invalid public key: {err}"))
}

fn base64url_encode(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn base64url_decode(raw: &str) -> Result<Vec<u8>> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(raw.trim())
        .or_else(|_| base64::engine::general_purpose::STANDARD.decode(raw.trim()))
        .context("base64url decode")
}

struct SenderIdentity {
    device_id: String,
    peer_name: String,
}

fn sender_identity() -> Result<SenderIdentity> {
    let daemon_url = resolve_daemon_url(None);
    if let Ok(client) = http_client()
        && let Ok(response) = client.get(format!("{daemon_url}/pair/status")).send()
        && response.status().is_success()
        && let Ok(body) = response.json::<Value>()
    {
        if let Some(device_id) = body.get("deviceId").and_then(Value::as_str) {
            let peer_name = body
                .get("peerName")
                .and_then(Value::as_str)
                .unwrap_or("CLI")
                .to_string();
            return Ok(SenderIdentity {
                device_id: device_id.to_string(),
                peer_name,
            });
        }
    }
    let identity = load_or_create_identity()?;
    Ok(SenderIdentity {
        device_id: identity.phone_id,
        peer_name: "CLI".to_string(),
    })
}

fn find_connection<'a>(store: &'a CliConnectionStore, id: &str) -> Option<&'a CliConnection> {
    store.connections.iter().find(|entry| {
        entry.id == id
            || entry.label.eq_ignore_ascii_case(id)
            || entry.workshop_device_id.starts_with(id)
    })
}

fn load_store() -> Result<CliConnectionStore> {
    let path = connections_path();
    if !path.is_file() {
        return Ok(CliConnectionStore::default());
    }
    let raw = fs::read_to_string(&path).context("read cli connections")?;
    serde_json::from_str(&raw).context("parse cli connections")
}

fn save_store(store: &CliConnectionStore) -> Result<()> {
    fs::create_dir_all(cli_dir()).context("create cli dir")?;
    let path = connections_path();
    let raw = serde_json::to_string_pretty(store).context("serialize connections")?;
    fs::write(&path, raw).context("write cli connections")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn cli_dir() -> PathBuf {
    medousa_data_dir().join("cli")
}

fn connections_path() -> PathBuf {
    cli_dir().join("connections.json")
}

fn http_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .context("build HTTP client")
}

fn resolve_daemon_url_arg(args: &[String]) -> String {
    find_arg_value(args, "--daemon-url")
        .map(|value| normalize_url(&value))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| resolve_daemon_url(None))
}

fn normalize_url(raw: &str) -> String {
    raw.trim().trim_end_matches('/').to_string()
}

fn print_peer_help() {
    println!("Medousa peer/portal client (headless)");
    println!();
    println!("Connect this machine as a surface to another Medousa:");
    println!("  peer  — inbox + share only");
    println!("  portal — full client (use --portal)");
    println!();
    println!("USAGE:");
    println!("  medousa peer nearby [--daemon-url <url>]");
    println!("  medousa peer connect <daemon-url> [--portal] [--name <label>]");
    println!("  medousa peer list");
    println!("  medousa peer remove <id|label>");
    println!("  medousa peer send <id|label> <message>");
    println!("  medousa peer inbox [--unread] [--daemon-url <url>]");
    println!("  medousa peer read <message-id> [--daemon-url <url>]");
    println!();
    println!("Credentials: {}/connections.json", cli_dir().display());
    println!("Host inbox defaults to {DEFAULT_DAEMON_URL} (this engine's inbox).");
    println!();
    println!("EXAMPLES:");
    println!("  medousa peer nearby");
    println!("  medousa peer connect http://192.168.1.20:7419");
    println!("  medousa peer connect http://192.168.1.20:7419 --portal --name mini");
    println!("  medousa peer send mini \"hello from headless\"");
    println!("  medousa peer inbox --unread");
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
