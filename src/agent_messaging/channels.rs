//! Retained permission for an already admitted pair of chats. Transport and
//! session visibility are still checked on every send and delivery.
use super::store::Message;
use anyhow::{Result, bail};
use rusqlite::{Connection, Transaction, params};

pub(super) fn initialize(connection: &mut Connection) -> Result<()> {
    if connection.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))? >= 1 {
        return Ok(());
    }
    let transaction =
        connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    transaction.execute_batch("CREATE TABLE IF NOT EXISTS agent_message_channels (
        profile_id TEXT NOT NULL, source_session TEXT NOT NULL, target_session TEXT NOT NULL,
        source_workshop TEXT NOT NULL, target_workshop TEXT NOT NULL, admitted_turn TEXT NOT NULL,
        PRIMARY KEY(profile_id,source_session,target_session,source_workshop,target_workshop));
        INSERT OR IGNORE INTO agent_message_channels
        SELECT profile_id, source_session, target_session,
            coalesce(json_extract(payload,'$.source_workshop'),''),
            coalesce(json_extract(payload,'$.target_workshop'),''), source_turn
        FROM agent_messages WHERE json_extract(payload,'$.wake_source') IS NULL AND source_turn!='process_supervisor';
        PRAGMA user_version=1;")?;
    transaction.commit()?;
    Ok(())
}

pub(super) fn remember(transaction: &Transaction<'_>, message: &Message) -> Result<()> {
    transaction.execute(
        "INSERT OR IGNORE INTO agent_message_channels VALUES (?1,?2,?3,?4,?5,?6)",
        params![
            message.profile_id,
            message.source_session,
            message.target_session,
            message.source_workshop.as_deref().unwrap_or_default(),
            message.target_workshop.as_deref().unwrap_or_default(),
            message.source_turn
        ],
    )?;
    Ok(())
}

pub(super) fn require(transaction: &Transaction<'_>, message: &Message) -> Result<()> {
    let allowed: bool = transaction.query_row("SELECT EXISTS(SELECT 1 FROM agent_message_channels
        WHERE profile_id=?1 AND ((source_session=?2 AND target_session=?3 AND source_workshop=?4 AND target_workshop=?5)
        OR (source_session=?3 AND target_session=?2 AND source_workshop=?5 AND target_workshop=?4)))",
        params![message.profile_id, message.source_session, message.target_session,
            message.source_workshop.as_deref().unwrap_or_default(),
            message.target_workshop.as_deref().unwrap_or_default()], |row| row.get(0))?;
    if !allowed {
        bail!("these chats have not been authorized to communicate; ask the user to connect them");
    }
    Ok(())
}
