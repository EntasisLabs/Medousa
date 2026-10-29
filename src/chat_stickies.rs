//! Temporary, chat-local working memory on the Locus persistence backend.
//!
//! A dedicated table deliberately keeps this state out of temporal_node and all
//! permanent-memory/semantic recall paths. No implicit eviction or promotion.
use anyhow::{Result, bail, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use surrealdb::{Surreal, engine::any::Any};
use surrealdb_types::SurrealValue;

#[derive(Debug, Clone, SurrealValue)]
struct DbBoard {
    revision: u64,
    payload: String,
    chat_id: String,
}

pub const MAX_STICKY_NOTES: usize = 32;
pub const MAX_STICKY_TEXT_BYTES: usize = 4096;
pub const MAX_STICKY_BOARD_BYTES: usize = 32_768;
const MAX_RECEIPTS: usize = 128;
const MAX_RETAINED_NOTES: usize = 128;
const MAX_RETAINED_BYTES: usize = 131_072;
const TABLE: &str = "chat_sticky_board";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StickyStatus {
    Active,
    Resolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StickyNote {
    pub id: String,
    pub text: String,
    pub kind: String,
    pub pinned: bool,
    pub exact: bool,
    pub source_refs: Vec<String>,
    pub status: StickyStatus,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StickyBoard {
    pub revision: u64,
    pub notes: Vec<StickyNote>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum StickyOperation {
    /// Explicit replacement/update of this note, including protected notes.
    Put {
        note: StickyNote,
    },
    Pin {
        id: String,
        pinned: bool,
    },
    Resolve {
        id: String,
    },
    Reopen {
        id: String,
    },
    Remove {
        id: String,
    },
    /// Atomically replace unprotected source notes with one new note.
    Rollup {
        source_ids: Vec<String>,
        note: StickyNote,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StickyMutationRequest {
    pub expected_revision: u64,
    pub request_id: String,
    pub operation: StickyOperation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Receipt {
    request_id: String,
    fingerprint: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct StoredBoard {
    revision: u64,
    notes: Vec<StickyNote>,
    #[serde(default)]
    receipts: Vec<Receipt>,
}
impl StoredBoard {
    fn board(&self) -> StickyBoard {
        StickyBoard {
            revision: self.revision,
            notes: self.notes.clone(),
        }
    }
    fn apply(&self, request: &StickyMutationRequest) -> Result<Option<Self>> {
        bounded_string(&request.request_id, "request_id", 256)?;
        use sha2::{Digest, Sha256};
        let fingerprint = format!("{:x}", Sha256::digest(serde_json::to_vec(request)?));
        if let Some(receipt) = self
            .receipts
            .iter()
            .find(|r| r.request_id == request.request_id)
        {
            ensure!(
                receipt.fingerprint == fingerprint,
                "sticky request_id reused with different payload"
            );
            return Ok(None);
        }
        ensure!(
            self.revision == request.expected_revision,
            "sticky revision conflict: expected {}, actual {}",
            request.expected_revision,
            self.revision
        );
        let mut next = self.clone();
        match &request.operation {
            StickyOperation::Put { note } => {
                validate_note(note)?;
                if let Some(existing) = next.notes.iter_mut().find(|n| n.id == note.id) {
                    *existing = note.clone();
                } else {
                    next.notes.push(note.clone());
                }
            }
            StickyOperation::Pin { id, pinned } => next.note_mut(id)?.pinned = *pinned,
            StickyOperation::Resolve { id } => next.note_mut(id)?.status = StickyStatus::Resolved,
            StickyOperation::Reopen { id } => next.note_mut(id)?.status = StickyStatus::Active,
            StickyOperation::Remove { id } => {
                next.note_mut(id)?;
                next.notes.retain(|n| &n.id != id);
            }
            StickyOperation::Rollup { source_ids, note } => {
                validate_note(note)?;
                ensure!(!source_ids.is_empty(), "rollup requires source_ids");
                let unique: std::collections::HashSet<_> = source_ids.iter().collect();
                ensure!(unique.len() == source_ids.len(), "duplicate rollup source");
                for id in source_ids {
                    let source = next.note_mut(id)?;
                    ensure!(
                        !source.pinned && !source.exact,
                        "rollup cannot replace pinned/exact note {id}; explicitly update/remove it instead"
                    );
                }
                ensure!(
                    !next
                        .notes
                        .iter()
                        .any(|n| n.id == note.id && !source_ids.contains(&n.id)),
                    "rollup target already exists outside sources"
                );
                ensure!(
                    !source_ids.contains(&note.id),
                    "rollup requires a new target id so source notes remain recoverable"
                );
                let mut replacement = note.clone();
                for id in source_ids {
                    next.note_mut(id)?.status = StickyStatus::Resolved;
                    let reference = format!("sticky:{id}");
                    if !replacement.source_refs.contains(&reference) {
                        replacement.source_refs.push(reference);
                    }
                }
                validate_note(&replacement)?;
                next.notes.push(replacement);
            }
        }
        let active: Vec<_> = next
            .notes
            .iter()
            .filter(|n| n.status == StickyStatus::Active)
            .collect();
        ensure!(
            active.len() <= MAX_STICKY_NOTES,
            "sticky active note capacity exceeded; explicitly resolve or roll up notes"
        );
        ensure!(
            serde_json::to_vec(&active)?.len() <= MAX_STICKY_BOARD_BYTES,
            "sticky active board capacity exceeded; explicitly resolve or roll up notes"
        );
        ensure!(
            next.notes.len() <= MAX_RETAINED_NOTES
                && serde_json::to_vec(&next.notes)?.len() <= MAX_RETAINED_BYTES,
            "sticky retained history capacity exceeded; explicitly remove resolved notes before adding more"
        );
        next.revision = next
            .revision
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("sticky revision exhausted"))?;
        // Replay identity must not retain the text of explicitly removed notes.
        next.receipts.push(Receipt {
            request_id: request.request_id.clone(),
            fingerprint,
        });
        // Only replay metadata expires; notes are never silently dropped.
        if next.receipts.len() > MAX_RECEIPTS {
            next.receipts.remove(0);
        }
        Ok(Some(next))
    }
    fn note_mut(&mut self, id: &str) -> Result<&mut StickyNote> {
        self.notes
            .iter_mut()
            .find(|n| n.id == id)
            .ok_or_else(|| anyhow::anyhow!("sticky note not found: {id}"))
    }
}
fn bounded_string(value: &str, name: &str, max: usize) -> Result<()> {
    ensure!(
        !value.trim().is_empty() && value.len() <= max,
        "invalid sticky {name} (1..={max} UTF-8 bytes)"
    );
    Ok(())
}
fn validate_note(note: &StickyNote) -> Result<()> {
    bounded_string(&note.id, "id", 128)?;
    bounded_string(&note.text, "text", MAX_STICKY_TEXT_BYTES)?;
    bounded_string(&note.kind, "kind", 64)?;
    ensure!(
        note.source_refs.len() <= 16,
        "too many sticky source references"
    );
    for source in &note.source_refs {
        bounded_string(source, "source_ref", 512)?;
    }
    Ok(())
}
fn scope_key(profile: &str, chat: &str) -> Result<String> {
    bounded_string(profile, "profile", 256)?;
    bounded_string(chat, "chat", 512)?;
    // Length-prefixing avoids ambiguous separators and default-profile aliases.
    Ok(format!("{}:{profile}{}:{chat}", profile.len(), chat.len()))
}

#[derive(Clone)]
pub struct ChatStickyStore {
    db: Surreal<Any>,
    initialized: std::sync::Arc<tokio::sync::OnceCell<()>>,
}
impl ChatStickyStore {
    /// Use the same durable handle as Locus. Do not pass an in-memory handle in production.
    pub fn new(db: Surreal<Any>) -> Self {
        Self {
            db,
            initialized: std::sync::Arc::new(tokio::sync::OnceCell::new()),
        }
    }
    async fn ensure_schema(&self) -> Result<()> {
        self.initialized
            .get_or_try_init(|| async {
                self.db
                    .query("DEFINE TABLE IF NOT EXISTS chat_sticky_board SCHEMALESS;")
                    .await?
                    .check()?;
                Ok::<(), anyhow::Error>(())
            })
            .await?;
        Ok(())
    }
    async fn load(&self, key: &str) -> Result<Option<StoredBoard>> {
        self.ensure_schema().await?;
        let row: Option<DbBoard> = self.db.select((TABLE, key)).await?;
        row.map(|row| {
            let board: StoredBoard = serde_json::from_str(&row.payload)?;
            ensure!(
                board.revision == row.revision,
                "sticky storage revision mismatch"
            );
            Ok(board)
        })
        .transpose()
    }
    pub async fn read(&self, profile: &str, chat: &str) -> Result<StickyBoard> {
        let key = scope_key(profile, chat)?;
        Ok(self.load(&key).await?.unwrap_or_default().board())
    }
    /// Purge all profile boards when their owning chat is deleted. Mutation
    /// callers must hold the session lifecycle lease, so deletion cannot race
    /// a late write into resurrecting this state.
    pub async fn delete_session(&self, chat: &str) -> Result<()> {
        bounded_string(chat, "chat", 512)?;
        self.ensure_schema().await?;
        self.db
            .query("DELETE chat_sticky_board WHERE chat_id = $chat;")
            .bind(("chat", chat.to_string()))
            .await?
            .check()?;
        Ok(())
    }
    /// CAS protects concurrent writers, including independent store instances.
    /// A retained identical request is a no-op returning the current board. Once
    /// a receipt expires, its old revision still prevents accidental reapplication.
    pub async fn mutate(
        &self,
        profile: &str,
        chat: &str,
        request: StickyMutationRequest,
    ) -> Result<StickyBoard> {
        let key = scope_key(profile, chat)?;
        let stored = self.load(&key).await?;
        let current = stored.clone().unwrap_or_default();
        let Some(next) = current.apply(&request)? else {
            return Ok(current.board());
        };
        let row = DbBoard {
            revision: next.revision,
            payload: serde_json::to_string(&next)?,
            chat_id: chat.to_string(),
        };
        if stored.is_none() {
            let result: Result<Option<DbBoard>, surrealdb::Error> = self
                .db
                .create((TABLE, key.as_str()))
                .content(row.clone())
                .await;
            match result {
                Ok(Some(_)) => return Ok(next.board()),
                Ok(None) => bail!("sticky create returned no board"),
                Err(error) => {
                    // Do not hide transport/storage failures behind a conflict.
                    let latest = self.load(&key).await?;
                    let Some(latest) = latest else {
                        return Err(error.into());
                    };
                    if latest.apply(&request)?.is_none() {
                        return Ok(latest.board());
                    }
                    return Err(error.into());
                }
            }
        } else {
            let mut response = self.db.query(
                "UPDATE type::record($table, $key) CONTENT $next WHERE revision = $expected RETURN AFTER;"
            ).bind(("table", TABLE)).bind(("key", key.clone()))
                .bind(("next", row)).bind(("expected", request.expected_revision))
                .await?.check()?;
            let rows: Vec<DbBoard> = response.take(0)?;
            if !rows.is_empty() {
                return Ok(next.board());
            }
        }
        // A racing retry may have committed our exact request already.
        let latest = self.load(&key).await?.unwrap_or_default();
        if latest.apply(&request)?.is_none() {
            return Ok(latest.board());
        }
        bail!("sticky revision conflict; reload board before retrying")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn note(id: &str) -> StickyNote {
        StickyNote {
            id: id.into(),
            text: format!("Detail {id}"),
            kind: "detail".into(),
            pinned: false,
            exact: false,
            source_refs: vec!["turn:1".into()],
            status: StickyStatus::Active,
        }
    }
    fn req(rev: u64, id: &str, operation: StickyOperation) -> StickyMutationRequest {
        StickyMutationRequest {
            expected_revision: rev,
            request_id: id.into(),
            operation,
        }
    }
    fn put(rev: u64, id: &str) -> StickyMutationRequest {
        req(rev, id, StickyOperation::Put { note: note(id) })
    }
    #[test]
    fn cas_idempotency_and_payload_collision() {
        let request = put(0, "a");
        let board = StoredBoard::default().apply(&request).unwrap().unwrap();
        assert!(board.apply(&request).unwrap().is_none());
        assert!(board.apply(&put(0, "b")).is_err());
        let mut collision = request;
        collision.operation = StickyOperation::Remove { id: "a".into() };
        assert!(board.apply(&collision).is_err());
    }
    #[test]
    fn rollup_is_atomic_and_preserves_protected_notes() {
        let mut protected = note("a");
        protected.exact = true;
        let board = StoredBoard::default()
            .apply(&req(0, "one", StickyOperation::Put { note: protected }))
            .unwrap()
            .unwrap();
        assert!(
            board
                .apply(&req(
                    1,
                    "two",
                    StickyOperation::Rollup {
                        source_ids: vec!["a".into()],
                        note: note("rolled")
                    }
                ))
                .is_err()
        );
        assert_eq!(board.notes.len(), 1);
        let board = board
            .apply(&req(
                1,
                "explicit",
                StickyOperation::Put { note: note("a") },
            ))
            .unwrap()
            .unwrap();
        let board = board
            .apply(&req(
                2,
                "roll",
                StickyOperation::Rollup {
                    source_ids: vec!["a".into()],
                    note: note("rolled"),
                },
            ))
            .unwrap()
            .unwrap();
        assert_eq!(board.notes[0].id, "a");
        assert_eq!(board.notes[0].status, StickyStatus::Resolved);
        assert_eq!(board.notes[0].text, "Detail a");
        assert_eq!(board.notes[1].id, "rolled");
        assert!(board.notes[1].source_refs.contains(&"sticky:a".to_string()));
        let reopened = board
            .apply(&req(
                3,
                "reopen-source",
                StickyOperation::Reopen { id: "a".into() },
            ))
            .unwrap()
            .unwrap();
        assert_eq!(reopened.notes[0].status, StickyStatus::Active);
    }
    #[test]
    fn limits_never_clip_and_resolved_notes_reopen() {
        let mut board = StoredBoard::default();
        for n in 0..MAX_STICKY_NOTES {
            board = board
                .apply(&put(n as u64, &format!("n{n}")))
                .unwrap()
                .unwrap();
        }
        assert!(board.apply(&put(board.revision, "overflow")).is_err());
        let mut oversized = note("large");
        oversized.text = "é".repeat(MAX_STICKY_TEXT_BYTES);
        assert!(
            board
                .apply(&req(
                    board.revision,
                    "large",
                    StickyOperation::Put { note: oversized }
                ))
                .is_err()
        );
        let board = board
            .apply(&req(
                board.revision,
                "resolve",
                StickyOperation::Resolve { id: "n0".into() },
            ))
            .unwrap()
            .unwrap();
        assert_eq!(board.notes[0].status, StickyStatus::Resolved);
        let board = board
            .apply(&req(
                board.revision,
                "reopen",
                StickyOperation::Reopen { id: "n0".into() },
            ))
            .unwrap()
            .unwrap();
        assert_eq!(board.notes[0].status, StickyStatus::Active);
    }
    #[tokio::test]
    async fn backend_isolation_cas_and_reopen() -> Result<()> {
        let db = surrealdb::engine::any::connect("mem://").await?;
        db.use_ns("sticky_tests").use_db("locus").await?;
        db.query("DEFINE TABLE temporal_node SCHEMALESS;")
            .await?
            .check()?;
        let store = ChatStickyStore::new(db.clone());
        store.mutate("p", "chat", put(0, "a")).await?;
        let reopened = ChatStickyStore::new(db.clone());
        assert_eq!(reopened.read("p", "chat").await?.revision, 1);
        assert!(reopened.read("p2", "chat").await?.notes.is_empty());
        assert!(reopened.read("p", "other").await?.notes.is_empty());
        let (a, b) = tokio::join!(
            store.mutate("p", "chat", put(1, "b")),
            reopened.mutate("p", "chat", put(1, "c"))
        );
        assert_ne!(a.is_ok(), b.is_ok());
        let rows: Vec<surrealdb_types::Value> = db.select("temporal_node").await?;
        assert!(rows.is_empty());
        Ok(())
    }
    #[test]
    fn total_byte_quota_rejects_atomically() {
        let mut board = StoredBoard::default();
        for n in 0..7 {
            let mut item = note(&format!("n{n}"));
            item.text = "x".repeat(MAX_STICKY_TEXT_BYTES);
            board = board
                .apply(&req(
                    n,
                    &format!("r{n}"),
                    StickyOperation::Put { note: item },
                ))
                .unwrap()
                .unwrap();
        }
        let mut item = note("overflow");
        item.text = "x".repeat(MAX_STICKY_TEXT_BYTES);
        assert!(
            board
                .apply(&req(7, "overflow", StickyOperation::Put { note: item }))
                .is_err()
        );
        assert_eq!(board.revision, 7);
        assert_eq!(board.notes.len(), 7);
    }
    #[tokio::test]
    async fn concurrent_first_write_and_replay() -> Result<()> {
        let db = surrealdb::engine::any::connect("mem://").await?;
        db.use_ns("sticky_first").use_db("locus").await?;
        let a = ChatStickyStore::new(db.clone());
        let b = ChatStickyStore::new(db);
        let (one, two) = tokio::join!(
            a.mutate("p", "c", put(0, "a")),
            b.mutate("p", "c", put(0, "b"))
        );
        assert_ne!(one.is_ok(), two.is_ok());
        let current = a.read("p", "c").await?;
        assert_eq!(current.revision, 1);
        assert_eq!(current.notes.len(), 1);
        let original_id = current.notes[0].id.clone();
        a.mutate("p", "c", put(1, "later")).await?;
        let replay = b.mutate("p", "c", put(0, &original_id)).await?;
        assert_eq!(replay.revision, 2);
        assert_eq!(replay.notes.len(), 2);
        Ok(())
    }
    #[cfg(feature = "full-daemon")]
    #[tokio::test]
    async fn durable_database_reopen_preserves_board_and_receipts() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let endpoint = format!("surrealkv://{}", dir.path().join("stickies").display());
        {
            let db = surrealdb::engine::any::connect(endpoint.clone()).await?;
            db.use_ns("durability").use_db("locus").await?;
            let store = ChatStickyStore::new(db);
            store.mutate("p", "c", put(0, "a")).await?;
        }
        // Dropping the final client closes the datastore asynchronously.
        let db = {
            let mut attempt = 0;
            loop {
                match surrealdb::engine::any::connect(endpoint.clone()).await {
                    Ok(db) => break db,
                    Err(error) if attempt >= 40 => return Err(error.into()),
                    Err(_) => {
                        attempt += 1;
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    }
                }
            }
        };
        db.use_ns("durability").use_db("locus").await?;
        let store = ChatStickyStore::new(db);
        assert_eq!(store.read("p", "c").await?.notes[0].text, "Detail a");
        assert_eq!(store.mutate("p", "c", put(0, "a")).await?.revision, 1);
        Ok(())
    }
    #[test]
    fn explicit_removal_leaves_no_note_text_in_replay_metadata() {
        let mut sensitive = note("private");
        sensitive.text = "private ephemeral value".into();
        let board = StoredBoard::default()
            .apply(&req(0, "create", StickyOperation::Put { note: sensitive }))
            .unwrap()
            .unwrap();
        let board = board
            .apply(&req(
                1,
                "delete",
                StickyOperation::Remove {
                    id: "private".into(),
                },
            ))
            .unwrap()
            .unwrap();
        let stored = serde_json::to_string(&board).unwrap();
        assert!(!stored.contains("private ephemeral value"));
        assert!(
            board
                .apply(&req(
                    1,
                    "delete",
                    StickyOperation::Remove {
                        id: "private".into()
                    }
                ))
                .unwrap()
                .is_none()
        );
    }
    #[tokio::test]
    async fn deletion_purges_all_profiles_for_only_the_deleted_chat() -> Result<()> {
        let db = surrealdb::engine::any::connect("mem://").await?;
        db.use_ns("sticky-delete").use_db("test").await?;
        let store = ChatStickyStore::new(db);
        for (profile, chat) in [("a", "chat"), ("b", "chat"), ("a", "other")] {
            store
                .mutate(
                    profile,
                    chat,
                    req(
                        0,
                        "add",
                        StickyOperation::Put {
                            note: note("detail"),
                        },
                    ),
                )
                .await?;
        }
        store.delete_session("chat").await?;
        assert!(store.read("a", "chat").await?.notes.is_empty());
        assert!(store.read("b", "chat").await?.notes.is_empty());
        assert_eq!(store.read("a", "other").await?.notes.len(), 1);
        store.delete_session("chat").await?;
        Ok(())
    }
    #[test]
    fn scopes_are_unambiguous() {
        assert_ne!(scope_key("ab", "c").unwrap(), scope_key("a", "bc").unwrap());
    }
}
