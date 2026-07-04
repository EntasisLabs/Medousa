//! Async peer inbox for trusted workshop messaging.

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
    /// Raw attachment bundle (cleared after successful auto-import when summary is set).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachment: Option<ShareBundle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachment_result: Option<PeerMessageAttachmentSummary>,
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
        messages.retain(|message| message.read_at.is_none());
    }
    Ok(messages)
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

pub fn build_inbound_message(
    request: PeerMessagePostRequest,
    fallback_device_id: &str,
    fallback_name: &str,
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
    Ok(PeerMessage {
        id: format!("msg_{}", Uuid::new_v4()),
        from_device_id: request
            .from_device_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(fallback_device_id)
            .to_string(),
        from_name: request
            .from_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(fallback_name)
            .to_string(),
        body,
        sent_at: Utc::now(),
        read_at: None,
        attachment: request.attachment,
        attachment_result: None,
    })
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
                attachment: None,
            },
            "dev",
            "Peer",
        )
        .unwrap_err();
        assert!(err.to_string().contains("required"));
    }

    #[test]
    fn build_inbound_accepts_body() {
        let message = build_inbound_message(
            PeerMessagePostRequest {
                body: "hello".to_string(),
                from_device_id: Some("abc".to_string()),
                from_name: Some("Studio".to_string()),
                attachment: None,
            },
            "dev",
            "Peer",
        )
        .unwrap();
        assert_eq!(message.body, "hello");
        assert_eq!(message.from_device_id, "abc");
        assert!(message.id.starts_with("msg_"));
    }
}
