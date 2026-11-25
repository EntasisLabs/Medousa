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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum MessageStatus {
    Queued,
    Delivering,
    Delivered,
    Failed,
    Uncertain,
}
impl Message {
    pub(super) fn session_event(
        &self,
        execution: &str,
    ) -> stasis::domain::agent::envelope::AgentEnvelope {
        use stasis::domain::agent::envelope::{
            AGENT_ENVELOPE_SCHEMA_VERSION_V1, AgentEnvelope, AgentEnvelopeKind,
        };
        AgentEnvelope {
            schema_version: AGENT_ENVELOPE_SCHEMA_VERSION_V1,
            kind: AgentEnvelopeKind::MessageAppended,
            envelope_id: self.id.clone(),
            session_id: self.target_session.clone(),
            thread_id: None,
            turn_id: Some(execution.into()),
            job_id: None,
            correlation_id: self.root_id.clone(),
            causation_id: self.source_turn.clone(),
            participant_id: Some(self.source_session.clone()),
            occurred_at: self.created_at,
            payload: serde_json::json!({"message":self.body,"source_workshop":self.source_workshop}),
        }
    }

    pub(super) fn wake_grant(&self) -> medousa_types::wake::WakeGrant {
        let source = match self.wake_source.as_deref() {
            Some("process_completed") => medousa_types::wake::WakeSource::ProcessCompleted,
            Some("timed_out") => medousa_types::wake::WakeSource::ProcessTimedOut,
            Some("worker_result") => medousa_types::wake::WakeSource::WorkerResult,
            Some("peer_terminal") => medousa_types::wake::WakeSource::PeerTerminal,
            Some("terminal_callback") => medousa_types::wake::WakeSource::TerminalCallback,
            Some("native_callback") => medousa_types::wake::WakeSource::NativeCallback,
            _ => medousa_types::wake::WakeSource::AgentInbox,
        };
        medousa_types::wake::WakeGrant::for_source(source)
    }
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
    #[serde(default)]
    pub source_workshop: Option<String>,
    #[serde(default)]
    pub target_workshop: Option<String>,
    #[serde(default)]
    pub hop: usize,
    #[serde(default)]
    pub remote_inbound: bool,
    /// A terminal receipt confirmed by the remote mailbox. Not model-facing policy.
    #[serde(default)]
    pub receipt_acknowledged: bool,
    #[serde(default)]
    pub transport_route: Option<TransportRoute>,
    /// How many times delivery has been queued again after a missing turn.
    #[serde(default)]
    pub attempts: u32,
    /// Visible retry schedule. Absent means the message can be claimed now.
    #[serde(default)]
    pub next_attempt_at: Option<DateTime<Utc>>,
    /// Last turn-ticket phase observed before the in-memory ticket was cleared.
    #[serde(default)]
    pub observed_phase: Option<String>,
    /// Wake to install on the recipient turn. Absent means normal agent communication.
    #[serde(default)]
    pub wake_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_execution: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "connection_id", rename_all = "snake_case")]
pub(super) enum TransportRoute {
    Connection(String),
    Mailbox,
}

#[derive(Debug)]
pub(super) struct PendingMessage {
    pub terminal_event: bool,
    pub target_execution: Option<String>,
    pub id: String,
    pub profile_id: String,
    pub target_session: String,
    pub status: MessageStatus,
    pub delivery_turn: Option<String>,
    pub remote_outbound: bool,
    pub attempts: u32,
    pub next_attempt_at: Option<DateTime<Utc>>,
    pub observed_phase: Option<String>,
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
        let mut connection = Connection::open(&self.path)?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch("CREATE TABLE IF NOT EXISTS agent_messages (
            id TEXT PRIMARY KEY, profile_id TEXT NOT NULL, source_session TEXT NOT NULL,
            source_turn TEXT NOT NULL, target_session TEXT NOT NULL, root_id TEXT NOT NULL,
            status TEXT NOT NULL, delivery_turn TEXT, payload TEXT NOT NULL);
            CREATE INDEX IF NOT EXISTS agent_messages_pending ON agent_messages(status);
            CREATE INDEX IF NOT EXISTS agent_messages_source ON agent_messages(profile_id, source_turn);
            CREATE INDEX IF NOT EXISTS agent_messages_root ON agent_messages(root_id);
            CREATE INDEX IF NOT EXISTS agent_messages_delivery ON agent_messages(delivery_turn);")?;
        super::channels::initialize(&mut connection)?;
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
    #[cfg(test)]
    pub fn enqueue(&self, message: Message, request_key: &str) -> Result<Message> {
        self.enqueue_agent(message, request_key, true)
    }
    pub fn enqueue_agent(
        &self,
        message: Message,
        request_key: &str,
        may_open_channel: bool,
    ) -> Result<Message> {
        self.enqueue_inner(message, request_key, Some(may_open_channel))
    }
    pub fn enqueue_event(&self, message: Message, request_key: &str) -> Result<Message> {
        if message.wake_source.is_none() {
            bail!("runtime event requires its wake source");
        }
        self.enqueue_inner(message, request_key, None)
    }
    fn enqueue_inner(
        &self,
        mut message: Message,
        request_key: &str,
        channel_admission: Option<bool>,
    ) -> Result<Message> {
        let mut connection = self.connection()?;
        let transaction =
            connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let mut digest = Sha256::new();
        if let Some(workshop) = &message.source_workshop {
            digest.update(b"medousa/agent-message/workshop/v1\0");
            digest.update((workshop.len() as u64).to_be_bytes());
            digest.update(workshop.as_bytes());
        }
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
            if existing.target_execution != message.target_execution
                || existing.target_session != message.target_session
                || existing.body != message.body
                || existing.in_reply_to != message.in_reply_to
                || existing.source_workshop != message.source_workshop
                || existing.target_workshop != message.target_workshop
            {
                bail!("request_key was already used for a different message");
            }
            return Ok(existing);
        }
        if channel_admission.is_some()
            && message.source_session == message.target_session
            && message
                .target_execution
                .as_deref()
                .is_none_or(|target| target == message.source_turn)
            && message.source_workshop == message.target_workshop
        {
            bail!(
                "an agent cannot message its own chat. Allowed path: reply to the other chat, or return the result to the user in this chat."
            );
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
                || previous.source_workshop != message.target_workshop
                || previous.target_workshop != message.source_workshop
            {
                bail!("only the recipient can reply through this message's return route");
            }
            Some(previous)
        } else {
            // Retain causal correlation for forwards from an inbox turn.
            let raw: Option<String> = transaction
                .query_row(
                    "SELECT payload FROM agent_messages WHERE delivery_turn=?1 LIMIT 1",
                    [&message.source_turn],
                    |row| row.get(0),
                )
                .optional()?;
            raw.map(|raw| decode(&raw)).transpose()?
        };
        message.hop = previous.as_ref().map_or(Ok(0), |previous| {
            previous
                .hop
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!("invalid message hop value"))
        })?;
        message.root_id = previous
            .map(|previous| previous.root_id)
            .unwrap_or_else(|| message.id.clone());
        Self::check_capacity(&transaction)?;
        if let Some(may_open) = channel_admission {
            if may_open {
                super::channels::remember(&transaction, &message)?;
            } else {
                super::channels::require(&transaction, &message)?;
            }
        }
        message.status = MessageStatus::Queued;
        Self::write(&transaction, &message)?;
        transaction.commit()?;
        Ok(message)
    }
    fn check_capacity(transaction: &Transaction<'_>) -> Result<()> {
        let pending: usize = transaction.query_row(
            "SELECT count(*) FROM agent_messages WHERE status IN ('queued','delivering')",
            [],
            |row| row.get(0),
        )?;
        if pending >= MAX_PENDING {
            bail!("agent message inbox capacity reached");
        }
        Ok(())
    }
    /// The signed transport has already bound both workshop identities. A
    /// reply is additionally tied to the exact durable outbound return route.
    pub fn accept_remote(&self, mut message: Message) -> Result<Message> {
        let mut connection = self.connection()?;
        let transaction =
            connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        if let Some(id) = &message.in_reply_to {
            let previous = Self::find(&transaction, id)?.ok_or_else(|| {
                anyhow::anyhow!("remote reply has no saved outbound return route")
            })?;
            Self::validate_remote_reply(&message, &previous)?;
            message.transport_route = previous.transport_route;
        }
        if let Some(existing) = Self::find(&transaction, &message.id)? {
            if !existing.remote_inbound
                || existing.profile_id != message.profile_id
                || existing.source_workshop != message.source_workshop
                || existing.target_workshop != message.target_workshop
                || existing.source_session != message.source_session
                || existing.source_turn != message.source_turn
                || existing.source_name != message.source_name
                || existing.target_session != message.target_session
                || existing.target_name != message.target_name
                || existing.body != message.body
                || existing.root_id != message.root_id
                || existing.in_reply_to != message.in_reply_to
                || existing.hop != message.hop
                || existing.created_at != message.created_at
                || existing.transport_route != message.transport_route
            {
                bail!("remote message ID was already used for different content or route");
            }
            return Ok(existing);
        }
        Self::check_capacity(&transaction)?;
        message.status = MessageStatus::Queued;
        message.delivery_turn = None;
        message.error = None;
        message.remote_inbound = true;
        super::channels::remember(&transaction, &message)?;
        Self::write(&transaction, &message)?;
        transaction.commit()?;
        Ok(message)
    }
    pub fn validate_remote_reply(message: &Message, previous: &Message) -> Result<()> {
        if previous.remote_inbound
            || previous.profile_id != message.profile_id
            || previous.target_workshop != message.source_workshop
            || previous.source_workshop != message.target_workshop
            || previous.target_session != message.source_session
            || previous.source_session != message.target_session
            || previous.root_id != message.root_id
            || previous.hop.checked_add(1) != Some(message.hop)
        {
            bail!("remote reply does not match the saved outbound return route");
        }
        Ok(())
    }
    /// Round-robin pagination keeps old saved return routes reachable after
    /// delivery, without loading an unbounded conversation history each tick.
    pub fn outbound_routes(&self, after: u64) -> Result<Vec<(u64, String)>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare("SELECT rowid,id FROM agent_messages WHERE rowid>?1 AND json_extract(payload,'$.target_workshop') IS NOT NULL AND NOT coalesce(json_extract(payload,'$.remote_inbound'),0) AND status IN ('queued','delivering','delivered') ORDER BY rowid LIMIT 8")?;
        Ok(statement
            .query_map([after], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?)
    }
    pub fn replies_for_route(
        &self,
        workshop: &str,
        session: &str,
        root: &str,
    ) -> Result<Vec<Message>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare("SELECT payload FROM agent_messages WHERE target_session=?1 AND root_id=?2 AND json_extract(payload,'$.target_workshop')=?3 AND NOT coalesce(json_extract(payload,'$.remote_inbound'),0) AND json_extract(payload,'$.in_reply_to') IS NOT NULL AND status IN ('queued','delivering') ORDER BY rowid LIMIT 2")?;
        statement
            .query_map(params![session, root, workshop], |row| {
                row.get::<_, String>(0)
            })?
            .map(|raw| decode(&raw?))
            .collect()
    }
    pub fn received_reply_outcomes(&self, previous: &Message) -> Result<Vec<Message>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare("SELECT payload FROM agent_messages WHERE target_session=?1 AND root_id=?2 AND json_extract(payload,'$.source_workshop')=?3 AND coalesce(json_extract(payload,'$.remote_inbound'),0) AND status IN ('delivered','failed','uncertain') AND NOT coalesce(json_extract(payload,'$.receipt_acknowledged'),0) ORDER BY rowid LIMIT 16")?;
        statement
            .query_map(
                params![
                    previous.source_session,
                    previous.root_id,
                    previous.target_workshop
                ],
                |row| row.get::<_, String>(0),
            )?
            .map(|raw| decode(&raw?))
            .collect()
    }
    pub fn acknowledge_reply_outcomes(&self, sent: &[Message]) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction =
            connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        for receipt in sent {
            if let Some(mut current) = Self::find(&transaction, &receipt.id)?
                && current.remote_inbound
                && current.status == receipt.status
            {
                current.receipt_acknowledged = true;
                Self::write(&transaction, &current)?;
            }
        }
        transaction.commit()?;
        Ok(())
    }
    pub fn get(&self, id: &str) -> Result<Option<Message>> {
        Self::find(&self.connection()?, id)
    }
    pub fn pending(&self) -> Result<Vec<PendingMessage>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare("SELECT payload,(json_extract(payload,'$.target_workshop') IS NOT NULL AND NOT coalesce(json_extract(payload,'$.remote_inbound'),0)) FROM agent_messages WHERE status IN ('queued','delivering','uncertain') AND NOT (status='uncertain' AND coalesce(json_extract(payload,'$.remote_inbound'),0)=0 AND coalesce(json_extract(payload,'$.wake_source'),'') IN ('worker_result','peer_terminal')) ORDER BY rowid LIMIT 64")?;
        statement
            .query_map([], |row| {
                let payload: String = row.get(0)?;
                let remote_outbound: bool = row.get(1)?;
                Ok((payload, remote_outbound))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .map(|(payload, remote_outbound)| {
                let message = decode(&payload)?;
                Ok(PendingMessage {
                    terminal_event: super::is_terminal_event(&message),
                    target_execution: message.target_execution,
                    id: message.id,
                    profile_id: message.profile_id,
                    target_session: message.target_session,
                    status: message.status,
                    delivery_turn: message.delivery_turn,
                    remote_outbound,
                    attempts: message.attempts,
                    next_attempt_at: message.next_attempt_at,
                    observed_phase: message.observed_phase,
                })
            })
            .collect()
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
        if message.status != status {
            message.receipt_acknowledged = false;
        }
        message.status = status;
        if message.status == MessageStatus::Queued {
            message.delivery_turn = None;
        }
        message.error = error.map(|error| error.chars().take(500).collect());
        Self::write(&transaction, &message)?;
        transaction.commit()?;
        Ok(())
    }
    /// Remember the ticket phase before the in-memory ticket disappears.
    /// A completed turn is delivered here so a later missing-ticket poll cannot
    /// mark it uncertain.
    pub fn observe_phase(&self, id: &str, phase: &str) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction =
            connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let Some(mut message) = Self::find(&transaction, id)? else {
            return Ok(());
        };
        if message.status == MessageStatus::Delivered {
            return Ok(());
        }
        message.observed_phase = Some(phase.to_string());
        let prior_status = message.status.clone();
        match phase {
            "done" => {
                message.status = MessageStatus::Delivered;
                message.error = None;
            }
            "error" => {
                message.status = MessageStatus::Failed;
                message.error = Some(
                    "recipient turn ended with an error. Allowed path: resend with a new request_key."
                        .into(),
                );
            }
            "cancelled" => {
                message.status = MessageStatus::Failed;
                message.error = Some(
                    "recipient turn was cancelled. Allowed path: resend with a new request_key."
                        .into(),
                );
            }
            _ => {}
        }
        if message.status != prior_status {
            message.receipt_acknowledged = false;
        }
        Self::write(&transaction, &message)?;
        transaction.commit()?;
        Ok(())
    }
    /// Queue the message again. A missing ticket is not a terminal failure.
    pub fn requeue_for_retry(&self, id: &str) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction =
            connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let mut message = Self::find(&transaction, id)?
            .ok_or_else(|| anyhow::anyhow!("message is unavailable"))?;
        message.attempts = message.attempts.saturating_add(1);
        if message.attempts > 8 {
            message.status = MessageStatus::Failed;
            message.error = Some(
                "delivery exhausted 8 retries because the recipient turn never became visible. Allowed path: resend with a new request_key."
                    .into(),
            );
        } else {
            let delay = 2_u32.saturating_pow(message.attempts).min(60);
            message.status = MessageStatus::Queued;
            message.delivery_turn = None;
            message.next_attempt_at = Some(Utc::now() + chrono::Duration::seconds(delay as i64));
            message.error = Some(format!(
                "queued until the recipient can take it (attempt {}); the previous admission's turn is unavailable",
                message.attempts
            ));
        }
        Self::write(&transaction, &message)?;
        transaction.commit()?;
        Ok(())
    }
    pub fn note_waiting(&self, id: &str) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction =
            connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let mut message = Self::find(&transaction, id)?
            .ok_or_else(|| anyhow::anyhow!("message is unavailable"))?;
        if message.status == MessageStatus::Queued {
            message.error = Some("waiting: recipient has an active turn".into());
            Self::write(&transaction, &message)?;
        }
        transaction.commit()?;
        Ok(())
    }
    #[cfg(test)]
    pub fn inbox(&self, profile: &str, session: &str, turn: &str) -> Result<Vec<Message>> {
        self.inbox_matching(profile, session, turn, false, |_| true)
    }

    pub fn inbox_matching(
        &self,
        profile: &str,
        session: &str,
        turn: &str,
        pinned_only: bool,
        allowed: impl Fn(&Message) -> bool,
    ) -> Result<Vec<Message>> {
        let mut connection = self.connection()?;
        let transaction =
            connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let mut messages: Vec<Message> = {
            let mut statement = transaction.prepare("SELECT payload FROM agent_messages WHERE profile_id=?1 AND target_session=?2 AND status='queued' AND ((NOT ?4 AND json_extract(payload,'$.target_execution') IS NULL) OR json_extract(payload,'$.target_execution')=?3) AND (coalesce(json_extract(payload,'$.remote_inbound'),0)=1 OR json_extract(payload,'$.target_workshop') IS NULL) ORDER BY rowid LIMIT 64")?;
            statement
                .query_map(params![profile, session, turn, pinned_only], |row| {
                    row.get::<_, String>(0)
                })?
                .map(|raw| decode(&raw?))
                .collect::<Result<_>>()?
        };
        messages.retain(allowed);
        messages.truncate(8);
        for message in &mut messages {
            message.status = MessageStatus::Delivered;
            message.receipt_acknowledged = false;
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
            source_workshop: None,
            target_workshop: None,
            hop: 0,
            remote_inbound: false,
            receipt_acknowledged: false,
            transport_route: None,
            attempts: 0,
            next_attempt_at: None,
            observed_phase: None,
            target_execution: None,
            wake_source: None,
        }
    }
    #[test]
    fn a_missing_recipient_turn_is_queued_again_instead_of_uncertain() {
        let dir = tempfile::tempdir().unwrap();
        let store = MessageStore::new(dir.path().join("messages.sqlite3"));
        let admitted = store.enqueue(message("a", "b", "t"), "handback").unwrap();
        let claimed = store.claim(&admitted.id).unwrap().unwrap();
        assert_eq!(claimed.status, MessageStatus::Delivering);
        store.observe_phase(&admitted.id, "done").unwrap();
        let delivered = store.get(&admitted.id).unwrap().unwrap();
        assert_eq!(delivered.status, MessageStatus::Delivered);
        assert!(store.pending().unwrap().is_empty());

        let again = store.enqueue(message("a", "c", "t2"), "lost").unwrap();
        store.claim(&again.id).unwrap();
        store.requeue_for_retry(&again.id).unwrap();
        let queued = store.get(&again.id).unwrap().unwrap();
        assert_eq!(queued.status, MessageStatus::Queued);
        assert!(queued.attempts >= 1);
        assert!(
            queued
                .error
                .unwrap()
                .contains("queued until the recipient can take it")
        );
        assert_ne!(queued.status, MessageStatus::Uncertain);
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
    fn coder_clarification_is_read_only_by_the_addressed_assignment_after_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("messages.sqlite3");
        let store = MessageStore::new(path.clone());
        let mut clarification = message("ses_a", "ses_peer_b", "turn_a");
        clarification.target_execution = Some("medousa_coder_assignment_b".into());
        let sent = store
            .enqueue_agent(clarification.clone(), "clarify", true)
            .unwrap();
        drop(store);
        let store = MessageStore::new(path);
        assert!(
            store
                .inbox("user:alice", "ses_peer_b", "other_assignment")
                .unwrap()
                .is_empty()
        );
        let got = store
            .inbox("user:alice", "ses_peer_b", "medousa_coder_assignment_b")
            .unwrap();
        assert_eq!(got.len(), 1);
        let event = got[0].session_event("medousa_coder_assignment_b");
        assert_eq!(event.participant_id.as_deref(), Some("ses_a"));
        assert_eq!(event.envelope_id, sent.id);
        assert_eq!(event.payload["message"], clarification.body);
        assert!(store.claim(&sent.id).unwrap().is_none());
        assert!(
            store
                .inbox("user:alice", "ses_peer_b", "medousa_coder_assignment_b")
                .unwrap()
                .is_empty()
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
    fn addressed_worker_and_parent_each_consume_their_own_mail() {
        let dir = tempfile::tempdir().unwrap();
        let store = MessageStore::new(dir.path().join("messages.sqlite3"));
        let mut pinned = message("a", "a", "parent");
        pinned.target_execution = Some("worker".into());
        let pinned = store.enqueue(pinned, "clarification").unwrap();
        let normal = store.enqueue(message("c", "a", "sender"), "hello").unwrap();
        assert!(
            store
                .inbox_matching("user:alice", "a", "sibling", true, |_| true)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            store
                .inbox_matching("user:alice", "a", "worker", true, |_| true)
                .unwrap()[0]
                .id,
            pinned.id
        );
        assert_eq!(
            store.inbox("user:alice", "a", "parent").unwrap()[0].id,
            normal.id
        );
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
    fn long_exchanges_keep_their_channel_and_return_route_across_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("messages.sqlite3");
        let mut store = MessageStore::new(path.clone());
        let mut previous = store.enqueue(message("a", "b", "t"), "one").unwrap();
        for index in 1..=100 {
            let turn = format!("turn-{index}");
            store
                .inbox("user:alice", &previous.target_session, &turn)
                .unwrap();
            let mut reply = message(&previous.target_session, &previous.source_session, &turn);
            reply.in_reply_to = Some(previous.id.clone());
            let next = store.enqueue_agent(reply, "reply", false).unwrap();
            assert_eq!(next.root_id, previous.root_id);
            assert_eq!(next.hop, index);
            previous = next;
            if index == 50 {
                store = MessageStore::new(path.clone());
            }
        }
    }
    #[test]
    fn one_turn_can_send_more_than_eight_messages() {
        let dir = tempfile::tempdir().unwrap();
        let store = MessageStore::new(dir.path().join("messages.sqlite3"));
        for index in 0..12 {
            store
                .enqueue(message("a", "b", "same-turn"), &format!("key-{index}"))
                .unwrap();
        }
        assert_eq!(store.pending().unwrap().len(), 12);
    }
    #[test]
    fn retained_permissions_are_bidirectional_and_do_not_expand_to_other_chats() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("messages.sqlite3");
        let store = MessageStore::new(path.clone());
        store
            .enqueue_agent(message("a", "b", "human"), "one", true)
            .unwrap();
        let store = MessageStore::new(path);
        store
            .enqueue_agent(message("b", "a", "peer"), "two", false)
            .unwrap();
        assert!(
            store
                .enqueue_agent(message("b", "c", "peer"), "three", false)
                .is_err()
        );
        let mut other_owner = message("a", "b", "peer");
        other_owner.profile_id = "user:bob".into();
        assert!(store.enqueue_agent(other_owner, "other", false).is_err());
        let mut other_workshop = message("a", "b", "peer");
        other_workshop.target_workshop = Some("unapproved-workshop".into());
        assert!(store.enqueue_agent(other_workshop, "other", false).is_err());
    }
    #[test]
    fn terminal_receipts_are_idempotent_across_restart_and_not_drained_inline() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("messages.sqlite3");
        let event = super::super::terminal_message(
            "user:alice",
            "ses_owner",
            "ses_owner",
            "receipt_cancelled",
            "cancelled".into(),
            medousa_types::wake::WakeSource::WorkerResult,
        );
        let store = MessageStore::new(path.clone());
        let saved = store
            .enqueue_event(event.clone(), "terminal_worker_result")
            .unwrap();
        store.note_waiting(&saved.id).unwrap();
        assert!(
            store
                .inbox_matching("user:alice", "ses_owner", "old-turn", false, |m| m
                    .wake_source
                    .is_none())
                .unwrap()
                .is_empty()
        );
        let restarted = MessageStore::new(path);
        let same = restarted
            .enqueue_event(event.clone(), "terminal_worker_result")
            .unwrap();
        assert_eq!(same.id, saved.id);
        assert_eq!(restarted.pending().unwrap().len(), 1);
        let claimed = restarted.claim(&saved.id).unwrap().unwrap();
        assert_ne!(claimed.delivery_turn.as_deref(), Some("old-turn"));
        assert!(restarted.claim(&saved.id).unwrap().is_none());
        restarted.observe_phase(&saved.id, "done").unwrap();
        let retried = restarted
            .enqueue_event(event, "terminal_worker_result")
            .unwrap();
        assert_eq!(retried.status, MessageStatus::Delivered);
        assert!(restarted.pending().unwrap().is_empty());
        assert!(
            restarted
                .enqueue_agent(
                    message("ses_owner", "ses_other", "receipt-turn"),
                    "no-new-channel",
                    false
                )
                .is_err()
        );
    }

    #[test]
    fn terminal_unknown_admission_is_retained_not_replayed_after_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("messages.sqlite3");
        let store = MessageStore::new(path.clone());
        let event = super::super::terminal_message(
            "user:alice",
            "ses_worker",
            "ses_owner",
            "receipt",
            "result".into(),
            medousa_types::wake::WakeSource::PeerTerminal,
        );
        let saved = store.enqueue_event(event, "terminal_peer").unwrap();
        store.claim(&saved.id).unwrap().unwrap();
        store
            .outcome(
                &saved.id,
                MessageStatus::Uncertain,
                Some("admission outcome unknown".into()),
            )
            .unwrap();
        let store = MessageStore::new(path);
        assert!(store.pending().unwrap().is_empty());
        let retained = store.get(&saved.id).unwrap().unwrap();
        assert_eq!(retained.status, MessageStatus::Uncertain);
        assert_eq!(retained.error.as_deref(), Some("admission outcome unknown"));
        assert!(store.claim(&saved.id).unwrap().is_none());
    }

    #[test]
    fn process_notifications_do_not_authorize_chat_communication() {
        let dir = tempfile::tempdir().unwrap();
        let store = MessageStore::new(dir.path().join("messages.sqlite3"));
        let mut event = message("a", "b", "process_supervisor");
        event.wake_source = Some("process_completed".into());
        store.enqueue_event(event, "event").unwrap();
        assert!(
            store
                .enqueue_agent(message("b", "a", "peer"), "reply", false)
                .is_err()
        );
    }
    #[test]
    fn legacy_admitted_routes_are_migrated_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("messages.sqlite3");
        let store = MessageStore::new(path.clone());
        store.enqueue(message("a", "b", "human"), "one").unwrap();
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch("DROP TABLE agent_message_channels; PRAGMA user_version=0;")
            .unwrap();
        let store = MessageStore::new(path);
        store
            .enqueue_agent(message("b", "a", "peer"), "two", false)
            .unwrap();
        // Removing a permission is not undone by opening another connection.
        connection
            .execute_batch("DELETE FROM agent_message_channels;")
            .unwrap();
        assert!(
            store
                .enqueue_agent(message("a", "b", "peer"), "three", false)
                .is_err()
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
    #[test]
    fn existing_local_records_keep_their_inbox_and_retry_identity() {
        let dir = tempfile::tempdir().unwrap();
        let store = MessageStore::new(dir.path().join("messages.sqlite3"));
        let sent = store.enqueue(message("a", "b", "t"), "one").unwrap();
        let mut legacy = serde_json::to_value(&sent).unwrap();
        for field in [
            "source_workshop",
            "target_workshop",
            "hop",
            "remote_inbound",
            "transport_route",
        ] {
            legacy.as_object_mut().unwrap().remove(field);
        }
        store
            .connection()
            .unwrap()
            .execute(
                "UPDATE agent_messages SET payload=?1 WHERE id=?2",
                params![legacy.to_string(), sent.id],
            )
            .unwrap();
        let retried = store.enqueue(message("a", "b", "t"), "one").unwrap();
        assert_eq!(retried.id, sent.id);
        assert!(!store.pending().unwrap()[0].remote_outbound);
        assert!(store.outbound_routes(0).unwrap().is_empty());
        assert_eq!(store.inbox("user:alice", "b", "active").unwrap().len(), 1);
    }
}
