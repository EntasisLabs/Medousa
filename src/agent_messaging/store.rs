//! Transactional local inbox. Retries use caller keys; admission is recorded
//! before model execution, and interrupted admission never silently replays.
use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

pub(super) const TURN_PREFIX: &str = "agent_message_";
const MAX_PENDING: usize = 64;
const MAX_SENDS_PER_TURN: usize = 8;
const MAX_THREAD_MESSAGES: usize = 16;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum MessageStatus {
    Queued,
    Delivering,
    Delivered,
    Failed,
    Uncertain,
}
impl MessageStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Delivering => "delivering",
            Self::Delivered => "delivered",
            Self::Failed => "failed",
            Self::Uncertain => "uncertain",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Message {
    pub id: String,
    pub profile_id: String,
    pub source_session: String,
    pub source_turn: String,
    pub source_name: String,
    pub target_session: String,
    pub target_name: String,
    pub body: String,
    pub root_id: String,
    pub in_reply_to: Option<String>,
    pub created_at: DateTime<Utc>,
    pub status: MessageStatus,
    pub delivery_turn: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug)]
pub(super) struct PendingMessage {
    pub id: String,
    pub profile_id: String,
    pub target_session: String,
    pub status: MessageStatus,
    pub delivery_turn: Option<String>,
}

pub(super) struct MessageStore {
    path: PathBuf,
}
impl MessageStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
    fn connection(&self) -> Result<Connection> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(&self.path)?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch("CREATE TABLE IF NOT EXISTS agent_messages (
            id TEXT PRIMARY KEY, profile_id TEXT NOT NULL, source_session TEXT NOT NULL,
            source_turn TEXT NOT NULL, target_session TEXT NOT NULL, root_id TEXT NOT NULL,
            status TEXT NOT NULL, delivery_turn TEXT, payload TEXT NOT NULL);
            CREATE INDEX IF NOT EXISTS agent_messages_pending ON agent_messages(status);
            CREATE INDEX IF NOT EXISTS agent_messages_source ON agent_messages(profile_id, source_turn);
            CREATE INDEX IF NOT EXISTS agent_messages_root ON agent_messages(root_id);
            CREATE INDEX IF NOT EXISTS agent_messages_delivery ON agent_messages(delivery_turn);")?;
        Ok(connection)
    }
    fn find(connection: &Connection, id: &str) -> Result<Option<Message>> {
        let raw: Option<String> = connection
            .query_row(
                "SELECT payload FROM agent_messages WHERE id=?1",
                [id],
                |row| row.get(0),
            )
            .optional()?;
        raw.map(|raw| decode(&raw)).transpose()
    }
    fn write(transaction: &Transaction<'_>, message: &Message) -> Result<()> {
        let payload = serde_json::to_string(message)?;
        if payload.len() > 64 * 1024 {
            bail!("agent message exceeds its record budget");
        }
        transaction.execute("INSERT INTO agent_messages (id,profile_id,source_session,source_turn,target_session,root_id,status,delivery_turn,payload)
            VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9) ON CONFLICT(id) DO UPDATE SET status=excluded.status,delivery_turn=excluded.delivery_turn,payload=excluded.payload",
            params![message.id,message.profile_id,message.source_session,message.source_turn,message.target_session,message.root_id,message.status.as_str(),message.delivery_turn,payload])?;
        Ok(())
    }
    pub fn enqueue(&self, mut message: Message, request_key: &str) -> Result<Message> {
        let mut connection = self.connection()?;
        let transaction =
            connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let mut digest = Sha256::new();
        for value in [
            &message.profile_id,
            &message.source_session,
            &message.source_turn,
            request_key,
        ] {
            digest.update((value.len() as u64).to_be_bytes());
            digest.update(value.as_bytes());
        }
        message.id = format!("msg_{:x}", digest.finalize());
        if let Some(existing) = Self::find(&transaction, &message.id)? {
            if existing.target_session != message.target_session
                || existing.body != message.body
                || existing.in_reply_to != message.in_reply_to
            {
                bail!("request_key was already used for a different message");
            }
            return Ok(existing);
        }
        if message.source_session == message.target_session {
            bail!("an agent cannot message its own chat");
        }
        if message.body.trim().is_empty() || message.body.len() > 8000 {
            bail!("message must contain 1–8000 UTF-8 bytes");
        }
        if request_key.is_empty() || request_key.len() > 160 {
            bail!("request_key must contain 1–160 bytes");
        }
        let previous = if let Some(id) = &message.in_reply_to {
            let previous = Self::find(&transaction, id)?
                .ok_or_else(|| anyhow::anyhow!("reply message is unavailable"))?;
            if previous.profile_id != message.profile_id
                || previous.target_session != message.source_session
                || previous.source_session != message.target_session
            {
                bail!("only the recipient can reply through this message's return route");
            }
            Some(previous)
        } else {
            // Native message wakes cannot escape their exchange's budget by
            // omitting in_reply_to and pretending to start a new root.
            let raw: Option<String> = transaction
                .query_row(
                    "SELECT payload FROM agent_messages WHERE delivery_turn=?1 LIMIT 1",
                    [&message.source_turn],
                    |row| row.get(0),
                )
                .optional()?;
            raw.map(|raw| decode(&raw)).transpose()?
        };
        message.root_id = previous
            .map(|previous| previous.root_id)
            .unwrap_or_else(|| message.id.clone());
        let pending: usize = transaction.query_row(
            "SELECT count(*) FROM agent_messages WHERE status IN ('queued','delivering')",
            [],
            |row| row.get(0),
        )?;
        if pending >= MAX_PENDING {
            bail!("agent message inbox capacity reached");
        }
        let sends: usize = transaction.query_row(
            "SELECT count(*) FROM agent_messages WHERE profile_id=?1 AND source_turn=?2",
            params![message.profile_id, message.source_turn],
            |row| row.get(0),
        )?;
        if sends >= MAX_SENDS_PER_TURN {
            bail!("this turn's eight-message budget is exhausted");
        }
        let exchange: usize = transaction.query_row(
            "SELECT count(*) FROM agent_messages WHERE root_id=?1",
            [&message.root_id],
            |row| row.get(0),
        )?;
        if exchange >= MAX_THREAD_MESSAGES {
            bail!("this exchange's sixteen-message budget is exhausted; return to the user");
        }
        message.status = MessageStatus::Queued;
        Self::write(&transaction, &message)?;
        transaction.commit()?;
        Ok(message)
    }
    pub fn get(&self, id: &str) -> Result<Option<Message>> {
        Self::find(&self.connection()?, id)
    }
    pub fn pending(&self) -> Result<Vec<PendingMessage>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare("SELECT id,profile_id,target_session,status,delivery_turn FROM agent_messages WHERE status IN ('queued','delivering') ORDER BY rowid LIMIT 64")?;
        Ok(statement
            .query_map([], |row| {
                let status: String = row.get(3)?;
                Ok(PendingMessage {
                    id: row.get(0)?,
                    profile_id: row.get(1)?,
                    target_session: row.get(2)?,
                    status: if status == "queued" {
                        MessageStatus::Queued
                    } else {
                        MessageStatus::Delivering
                    },
                    delivery_turn: row.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?)
    }
    pub fn claim(&self, id: &str) -> Result<Option<Message>> {
        let mut connection = self.connection()?;
        let transaction =
            connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let Some(mut message) =
            Self::find(&transaction, id)?.filter(|m| m.status == MessageStatus::Queued)
        else {
            return Ok(None);
        };
        message.status = MessageStatus::Delivering;
        message.error = None;
        message.delivery_turn = Some(format!("{TURN_PREFIX}{}", message.id));
        Self::write(&transaction, &message)?;
        transaction.commit()?;
        Ok(Some(message))
    }
    pub fn outcome(&self, id: &str, status: MessageStatus, error: Option<String>) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction =
            connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let mut message = Self::find(&transaction, id)?
            .ok_or_else(|| anyhow::anyhow!("message is unavailable"))?;
        message.status = status;
        if message.status == MessageStatus::Queued {
            message.delivery_turn = None;
        }
        message.error = error.map(|error| error.chars().take(500).collect());
        Self::write(&transaction, &message)?;
        transaction.commit()?;
        Ok(())
    }
    pub fn inbox(&self, profile: &str, session: &str, turn: &str) -> Result<Vec<Message>> {
        let mut connection = self.connection()?;
        let transaction =
            connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let mut messages: Vec<Message> = {
            let mut statement = transaction.prepare("SELECT payload FROM agent_messages WHERE profile_id=?1 AND target_session=?2 AND status='queued' ORDER BY rowid LIMIT 8")?;
            statement
                .query_map(params![profile, session], |row| row.get::<_, String>(0))?
                .map(|raw| decode(&raw?))
                .collect::<Result<_>>()?
        };
        for message in &mut messages {
            message.status = MessageStatus::Delivered;
            message.delivery_turn = Some(turn.to_string());
            Self::write(&transaction, message)?;
        }
        transaction.commit()?;
        Ok(messages)
    }
}
fn decode(raw: &str) -> Result<Message> {
    if raw.len() > 64 * 1024 {
        bail!("agent message exceeds its record budget");
    }
    Ok(serde_json::from_str(raw)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn message(from: &str, to: &str, turn: &str) -> Message {
        Message {
            id: String::new(),
            profile_id: "user:alice".into(),
            source_session: from.into(),
            source_turn: turn.into(),
            source_name: from.into(),
            target_session: to.into(),
            target_name: to.into(),
            body: "hello".into(),
            root_id: String::new(),
            in_reply_to: None,
            created_at: Utc::now(),
            status: MessageStatus::Queued,
            delivery_turn: None,
            error: None,
        }
    }
    #[test]
    fn retries_are_idempotent_and_cannot_change_payload() {
        let dir = tempfile::tempdir().unwrap();
        let store = MessageStore::new(dir.path().join("messages.sqlite3"));
        let first = store.enqueue(message("a", "b", "t"), "one").unwrap();
        assert_eq!(
            store.enqueue(message("a", "b", "t"), "one").unwrap().id,
            first.id
        );
        assert!(store.enqueue(message("a", "c", "t"), "one").is_err());
        assert_eq!(
            MessageStore::new(dir.path().join("messages.sqlite3"))
                .pending()
                .unwrap()
                .len(),
            1
        );
    }
    #[test]
    fn busy_recipient_can_receive_without_a_second_wake() {
        let dir = tempfile::tempdir().unwrap();
        let store = MessageStore::new(dir.path().join("messages.sqlite3"));
        let m = store.enqueue(message("a", "b", "t"), "one").unwrap();
        assert!(store.inbox("user:bob", "b", "active").unwrap().is_empty());
        assert_eq!(store.inbox("user:alice", "b", "active").unwrap().len(), 1);
        assert!(store.claim(&m.id).unwrap().is_none());
        assert!(store.inbox("user:alice", "b", "active").unwrap().is_empty());
    }
    #[test]
    fn replies_use_immutable_return_route_and_inherit_budget() {
        let dir = tempfile::tempdir().unwrap();
        let store = MessageStore::new(dir.path().join("messages.sqlite3"));
        let first = store.enqueue(message("a", "b", "t"), "one").unwrap();
        let mut forged = message("c", "a", "x");
        forged.in_reply_to = Some(first.id.clone());
        assert!(store.enqueue(forged, "two").is_err());
        let mut reply = message("b", "a", "reply");
        reply.in_reply_to = Some(first.id.clone());
        let second = store.enqueue(reply, "two").unwrap();
        assert_eq!(second.root_id, first.root_id);
        let claimed = store.claim(&second.id).unwrap().unwrap();
        let forward = store
            .enqueue(
                message("a", "c", claimed.delivery_turn.as_ref().unwrap()),
                "three",
            )
            .unwrap();
        assert_eq!(forward.root_id, first.root_id);
    }
    #[test]
    fn restart_retains_admission_and_never_requeues_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("messages.sqlite3");
        let store = MessageStore::new(path.clone());
        let m = store.enqueue(message("a", "b", "t"), "one").unwrap();
        store.claim(&m.id).unwrap();
        let restarted = MessageStore::new(path);
        assert_eq!(
            restarted.pending().unwrap()[0].status,
            MessageStatus::Delivering
        );
        assert!(restarted.claim(&m.id).unwrap().is_none());
    }
    #[test]
    fn simultaneous_retries_create_one_message() {
        let dir = tempfile::tempdir().unwrap();
        let store = std::sync::Arc::new(MessageStore::new(dir.path().join("messages.sqlite3")));
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let store = store.clone();
                std::thread::spawn(move || store.enqueue(message("a", "b", "t"), "one").unwrap().id)
            })
            .collect();
        let ids: Vec<_> = threads
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect();
        assert!(ids.iter().all(|id| id == &ids[0]));
        assert_eq!(store.pending().unwrap().len(), 1);
    }
    #[test]
    fn reply_loops_cannot_escape_the_exchange_budget() {
        let dir = tempfile::tempdir().unwrap();
        let store = MessageStore::new(dir.path().join("messages.sqlite3"));
        let mut previous = store.enqueue(message("a", "b", "t"), "one").unwrap();
        for index in 1..16 {
            let mut reply = message(
                &previous.target_session,
                &previous.source_session,
                &format!("turn-{index}"),
            );
            reply.in_reply_to = Some(previous.id.clone());
            previous = store.enqueue(reply, "reply").unwrap();
        }
        let mut reply = message(
            &previous.target_session,
            &previous.source_session,
            "turn-17",
        );
        reply.in_reply_to = Some(previous.id.clone());
        assert!(
            store
                .enqueue(reply, "reply")
                .unwrap_err()
                .to_string()
                .contains("sixteen-message")
        );
    }
    #[test]
    fn escaped_unicode_and_control_data_survives_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let store = MessageStore::new(dir.path().join("messages.sqlite3"));
        let mut input = message("a", "b", "t");
        input.body = "\0".repeat(8000);
        let sent = store.enqueue(input, "one").unwrap();
        assert_eq!(store.get(&sent.id).unwrap().unwrap().body, sent.body);
    }
}
