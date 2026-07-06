//! Peer conversation store (inbound + outbound copies on this workshop).

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::share::bundle::ShareBundle;

const INBOX_CAP: usize = 500;
const INBOX_FILE: &str = "peer_inbox.json";

static INBOX_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerMessageAttachmentSummary {
    pub imported: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifacts_imported: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault_notes_imported: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerMessage {
    pub id: String,
    pub from_device_id: String,
    pub from_name: String,
    pub body: String,
    pub sent_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_at: Option<DateTime<Utc>>,
    /// `in` = delivered to this workshop, `out` = sent from this workshop.
    #[serde(default = "default_direction_in")]
    pub direction: String,
    /// Recipient device id (set for outbound copies and optional on inbound).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_device_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_name: Option<String>,
    /// Raw attachment bundle (cleared after successful auto-import when summary is set).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachment: Option<ShareBundle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachment_result: Option<PeerMessageAttachmentSummary>,
}

fn default_direction_in() -> String {
    "in".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct PeerInboxFile {
    #[serde(default)]
    messages: Vec<PeerMessage>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerMessagePostRequest {
    pub body: String,
    #[serde(default)]
    pub from_device_id: Option<String>,
    #[serde(default)]
    pub from_name: Option<String>,
    #[serde(default)]
    pub to_device_id: Option<String>,
    #[serde(default)]
    pub to_name: Option<String>,
    /// Only honored for local (loopback) posts. Remote posts are always inbound.
    #[serde(default)]
    pub direction: Option<String>,
    #[serde(default)]
    pub attachment: Option<ShareBundle>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerMessagesListResponse {
    pub messages: Vec<PeerMessage>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerUnreadCountResponse {
    pub unread: usize,
}

fn inbox_path() -> PathBuf {
    crate::paths::medousa_data_dir().join(INBOX_FILE)
}

fn load_inbox() -> Result<PeerInboxFile> {
    let path = inbox_path();
    if !path.is_file() {
        return Ok(PeerInboxFile::default());
    }
    let raw = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&raw).context("parse peer inbox")
}

fn save_inbox(inbox: &PeerInboxFile) -> Result<()> {
    let path = inbox_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("create peer inbox parent")?;
    }
    let raw = serde_json::to_string_pretty(inbox).context("serialize peer inbox")?;
    fs::write(&path, raw).with_context(|| format!("write {}", path.display()))
}

pub fn append_message(message: PeerMessage) -> Result<PeerMessage> {
    let _guard = INBOX_LOCK.lock().expect("peer inbox lock");
    let mut inbox = load_inbox()?;
    inbox.messages.insert(0, message.clone());
    if inbox.messages.len() > INBOX_CAP {
        inbox.messages.truncate(INBOX_CAP);
    }
    save_inbox(&inbox)?;
    Ok(message)
}

pub fn list_messages(unread_only: bool) -> Result<Vec<PeerMessage>> {
    let _guard = INBOX_LOCK.lock().expect("peer inbox lock");
    let inbox = load_inbox()?;
    let mut messages = inbox.messages;
    if unread_only {
        messages.retain(|message| message.direction != "out" && message.read_at.is_none());
    }
    Ok(messages)
}

/// Messages in a conversation with `device_id` (as sender or recipient).
pub fn messages_for_device(device_id: &str) -> Result<Vec<PeerMessage>> {
    let messages = list_messages(false)?;
    Ok(messages
        .into_iter()
        .filter(|message| involves_device(message, device_id))
        .collect())
}

/// Full inbox with optional thread filter (portal / host).
pub fn list_messages_filtered(unread_only: bool, device_id: Option<&str>) -> Result<Vec<PeerMessage>> {
    let mut messages = list_messages(unread_only)?;
    if let Some(device_id) = device_id.filter(|value| !value.is_empty()) {
        messages.retain(|message| involves_device(message, device_id));
    }
    Ok(messages)
}

/// Peer-scoped remote view: one thread; unread = outbound copies not yet read by peer.
pub fn list_messages_for_peer_device(device_id: &str, unread_only: bool) -> Result<Vec<PeerMessage>> {
    let mut messages = messages_for_device(device_id)?;
    if unread_only {
        messages.retain(|message| message.direction == "out" && message.read_at.is_none());
    }
    Ok(messages)
}

pub fn unread_count_for_device(device_id: &str) -> Result<usize> {
    Ok(list_messages_for_peer_device(device_id, true)?.len())
}

pub fn involves_device(message: &PeerMessage, device_id: &str) -> bool {
    device_ids_match(&message.from_device_id, device_id)
        || message
            .to_device_id
            .as_deref()
            .is_some_and(|to| device_ids_match(to, device_id))
}

fn device_ids_match(left: &str, right: &str) -> bool {
    if left.is_empty() || right.is_empty() {
        return left == right;
    }
    left == right
        || left.starts_with(&right[..right.len().min(8)])
        || right.starts_with(&left[..left.len().min(8)])
}

pub fn unread_count() -> Result<usize> {
    let messages = list_messages(true)?;
    Ok(messages.len())
}

pub fn mark_read(message_id: &str) -> Result<PeerMessage> {
    let _guard = INBOX_LOCK.lock().expect("peer inbox lock");
    let mut inbox = load_inbox()?;
    let message = inbox
        .messages
        .iter_mut()
        .find(|entry| entry.id == message_id)
        .ok_or_else(|| anyhow::anyhow!("message not found"))?;
    if message.read_at.is_none() {
        message.read_at = Some(Utc::now());
    }
    let cloned = message.clone();
    save_inbox(&inbox)?;
    Ok(cloned)
}

pub fn get_message(message_id: &str) -> Result<Option<PeerMessage>> {
    let messages = list_messages(false)?;
    Ok(messages.into_iter().find(|entry| entry.id == message_id))
}

pub fn build_message(
    request: PeerMessagePostRequest,
    fallback_from_id: &str,
    fallback_from_name: &str,
    default_direction: &str,
    default_to_id: Option<&str>,
    default_to_name: Option<&str>,
) -> Result<PeerMessage> {
    let body = request.body.trim().to_string();
    if body.is_empty() && request.attachment.is_none() {
        bail!("message body or attachment is required");
    }
    if let Some(bundle) = &request.attachment {
        let errors = bundle.validate();
        if !errors.is_empty() {
            bail!(errors.join("; "));
        }
    }
    let direction = request
        .direction
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(default_direction)
        .to_ascii_lowercase();
    let direction = if direction == "out" { "out" } else { "in" }.to_string();

    let to_device_id = request
        .to_device_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| default_to_id.map(str::to_string));
    let to_name = request
        .to_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| default_to_name.map(str::to_string));

    Ok(PeerMessage {
        id: format!("msg_{}", Uuid::new_v4()),
        from_device_id: request
            .from_device_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(fallback_from_id)
            .to_string(),
        from_name: request
            .from_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(fallback_from_name)
            .to_string(),
        body,
        sent_at: Utc::now(),
        // Recipients mark read when they open the message (including outbound copies they poll).
        read_at: None,
        direction,
        to_device_id,
        to_name,
        attachment: request.attachment,
        attachment_result: None,
    })
}

/// Backward-compatible helper used by older call sites/tests.
pub fn build_inbound_message(
    request: PeerMessagePostRequest,
    fallback_device_id: &str,
    fallback_name: &str,
) -> Result<PeerMessage> {
    build_message(request, fallback_device_id, fallback_name, "in", None, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_inbound_rejects_empty() {
        let err = build_inbound_message(
            PeerMessagePostRequest {
                body: "  ".to_string(),
                from_device_id: None,
                from_name: None,
                to_device_id: None,
                to_name: None,
                direction: None,
                attachment: None,
            },
            "dev",
            "Peer",
        )
        .unwrap_err();
        assert!(err.to_string().contains("required"));
    }

    #[test]
    fn build_outbound_marks_read() {
        let message = build_message(
            PeerMessagePostRequest {
                body: "hello".to_string(),
                from_device_id: Some("host".to_string()),
                from_name: Some("Host".to_string()),
                to_device_id: Some("peer".to_string()),
                to_name: Some("Peer".to_string()),
                direction: Some("out".to_string()),
                attachment: None,
            },
            "host",
            "Host",
            "out",
            None,
            None,
        )
        .unwrap();
        assert_eq!(message.direction, "out");
        assert_eq!(message.to_device_id.as_deref(), Some("peer"));
        assert!(message.read_at.is_none());
    }

    #[test]
    fn involves_device_matches_to_and_from() {
        let message = PeerMessage {
            id: "1".into(),
            from_device_id: "aaa".into(),
            from_name: "A".into(),
            body: "hi".into(),
            sent_at: Utc::now(),
            read_at: None,
            direction: "out".into(),
            to_device_id: Some("bbb".into()),
            to_name: Some("B".into()),
            attachment: None,
            attachment_result: None,
        };
        assert!(involves_device(&message, "bbb"));
        assert!(involves_device(&message, "aaa"));
        assert!(!involves_device(&message, "ccc"));
    }
}
