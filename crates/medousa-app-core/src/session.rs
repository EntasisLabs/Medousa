//! Stateful chat session projections and command dispatch.

use std::collections::{BTreeMap, HashMap};

use medousa_types::TurnStreamEnvelopeV3;
use thiserror::Error;

use crate::agents::{relationships_from_items, resolve_worker_parent_item_id};
use crate::contract::{
    BeginTurnRequest, ChatCommand, ChatItem, ChatItemRole, ChatLane, ChatRevision, ChatSnapshot,
    ChatUpdate,
};
use crate::merge::merge_transcript;
use crate::stream::{apply_v3_envelope_to_item, v3_event_promotes_chat_message};
use crate::wire_history::{decode_wire_turns, project_wire_history};

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ChatCoreError {
    #[error("session `{0}` is not open")]
    SessionNotOpen(String),
    #[error("stale revision for session `{session_id}`: expected {expected:?}, have {actual:?}")]
    StaleRevision {
        session_id: String,
        expected: ChatRevision,
        actual: ChatRevision,
    },
    #[error("command rejected: {0}")]
    Rejected(String),
}

struct ApplyHistoryArgs<'a> {
    session_id: &'a str,
    authority_id: &'a str,
    turns: Vec<serde_json::Value>,
    lane: Option<ChatLane>,
    ask_job_id: Option<String>,
    expected_revision: Option<ChatRevision>,
    replace: bool,
    local_items: Option<Vec<ChatItem>>,
}

#[derive(Debug, Clone)]
struct WorkerLink {
    work_id: String,
    parent_turn_id: Option<String>,
    parent_item_id: Option<String>,
    reply_to_work_id: Option<String>,
}

#[derive(Debug, Clone)]
struct SessionState {
    session_id: String,
    authority_id: String,
    revision: ChatRevision,
    items: Vec<ChatItem>,
    active_turn_id: Option<String>,
    last_seq_by_turn: BTreeMap<String, u64>,
    workers: HashMap<String, WorkerLink>,
}

impl SessionState {
    fn snapshot(&self) -> ChatSnapshot {
        let worker_tuples: Vec<_> = self
            .workers
            .values()
            .map(|link| {
                (
                    link.work_id.clone(),
                    link.parent_turn_id.clone(),
                    link.parent_item_id.clone(),
                    link.reply_to_work_id.clone(),
                )
            })
            .collect();
        ChatSnapshot {
            session_id: self.session_id.clone(),
            authority_id: self.authority_id.clone(),
            revision: self.revision,
            items: self.items.clone(),
            active_turn_id: self.active_turn_id.clone(),
            relationships: relationships_from_items(&self.items, &worker_tuples),
            last_seq_by_turn: self.last_seq_by_turn.clone(),
        }
    }

    fn bump(&mut self) {
        self.revision = self.revision.next();
    }
}

/// Renderer-agnostic application core for chat timeline ownership.
#[derive(Debug, Default)]
pub struct ChatCore {
    sessions: HashMap<String, SessionState>,
    focused_session_id: Option<String>,
}

impl ChatCore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn focused_session_id(&self) -> Option<&str> {
        self.focused_session_id.as_deref()
    }

    pub fn snapshot(&self, session_id: &str) -> Result<ChatSnapshot, ChatCoreError> {
        self.sessions
            .get(session_id)
            .map(SessionState::snapshot)
            .ok_or_else(|| ChatCoreError::SessionNotOpen(session_id.to_string()))
    }

    pub fn dispatch(&mut self, command: ChatCommand) -> Result<ChatUpdate, ChatCoreError> {
        match command {
            ChatCommand::OpenSession {
                session_id,
                authority_id,
            } => {
                self.open_session(session_id, authority_id);
                let session_id = self.focused_session_id.clone().unwrap();
                Ok(ChatUpdate::Snapshot {
                    snapshot: self.snapshot(&session_id)?,
                })
            }
            ChatCommand::ApplyHistory {
                session_id,
                authority_id,
                turns,
                lane,
                ask_job_id,
                expected_revision,
                replace,
                local_items,
            } => self.apply_history(ApplyHistoryArgs {
                session_id: &session_id,
                authority_id: &authority_id,
                turns,
                lane,
                ask_job_id,
                expected_revision,
                replace,
                local_items,
            }),
            ChatCommand::ProjectHistory {
                session_id,
                authority_id,
                turns,
                lane,
                ask_job_id,
            } => {
                let decoded = decode_wire_turns(&turns);
                let items =
                    project_wire_history(&session_id, &authority_id, &decoded, lane, ask_job_id);
                Ok(ChatUpdate::Projected { items })
            }
            ChatCommand::BeginTurn {
                session_id,
                request,
                expected_revision,
            } => self.begin_turn(&session_id, request, expected_revision),
            ChatCommand::ApplyStreamEvent {
                session_id,
                envelope,
                expected_revision,
                promote_item_id,
            } => self.apply_stream_event(&session_id, envelope, expected_revision, promote_item_id),
            ChatCommand::SettleTurn {
                session_id,
                turn_id,
                expected_revision,
                reusable_stream,
            } => self.settle_turn(&session_id, &turn_id, expected_revision, reusable_stream),
            ChatCommand::LinkWorker {
                session_id,
                work_id,
                parent_turn_id,
                parent_item_id,
                reply_to_work_id,
                expected_revision,
            } => self.link_worker(
                &session_id,
                work_id,
                parent_turn_id,
                parent_item_id,
                reply_to_work_id,
                expected_revision,
            ),
            ChatCommand::SwitchFocus { session_id } => {
                if !self.sessions.contains_key(&session_id) {
                    return Err(ChatCoreError::SessionNotOpen(session_id));
                }
                self.focused_session_id = Some(session_id.clone());
                Ok(ChatUpdate::Snapshot {
                    snapshot: self.snapshot(&session_id)?,
                })
            }
            ChatCommand::DisposeSession { session_id } => {
                self.sessions.remove(&session_id);
                if self.focused_session_id.as_deref() == Some(session_id.as_str()) {
                    self.focused_session_id = None;
                }
                Ok(ChatUpdate::Disposed { session_id })
            }
            ChatCommand::Snapshot { session_id } => Ok(ChatUpdate::Snapshot {
                snapshot: self.snapshot(&session_id)?,
            }),
        }
    }

    fn open_session(&mut self, session_id: String, authority_id: String) {
        self.sessions
            .entry(session_id.clone())
            .or_insert_with(|| SessionState {
                session_id: session_id.clone(),
                authority_id: authority_id.clone(),
                revision: ChatRevision(0),
                items: Vec::new(),
                active_turn_id: None,
                last_seq_by_turn: BTreeMap::new(),
                workers: HashMap::new(),
            });
        if let Some(existing) = self.sessions.get_mut(&session_id)
            && existing.authority_id != authority_id
            && !authority_id.is_empty()
        {
            existing.authority_id = authority_id;
        }
        self.focused_session_id = Some(session_id);
    }

    fn ensure_revision(
        session: &SessionState,
        expected: Option<ChatRevision>,
    ) -> Result<(), ChatCoreError> {
        if let Some(expected) = expected
            && expected != session.revision
        {
            return Err(ChatCoreError::StaleRevision {
                session_id: session.session_id.clone(),
                expected,
                actual: session.revision,
            });
        }
        Ok(())
    }

    fn apply_history(&mut self, args: ApplyHistoryArgs<'_>) -> Result<ChatUpdate, ChatCoreError> {
        if !self.sessions.contains_key(args.session_id) {
            self.open_session(args.session_id.to_string(), args.authority_id.to_string());
        }
        let session = self
            .sessions
            .get_mut(args.session_id)
            .ok_or_else(|| ChatCoreError::SessionNotOpen(args.session_id.to_string()))?;
        Self::ensure_revision(session, args.expected_revision)?;

        if let Some(items) = args.local_items {
            session.items = items;
        }

        let projected = project_wire_history(
            args.session_id,
            args.authority_id,
            &decode_wire_turns(&args.turns),
            args.lane,
            args.ask_job_id,
        );
        if !args.authority_id.trim().is_empty() {
            session.authority_id = args.authority_id.to_string();
        }
        session.items = if args.replace || session.items.is_empty() {
            projected
        } else {
            merge_transcript(session.items.clone(), projected)
        };
        session.bump();
        Ok(ChatUpdate::Snapshot {
            snapshot: session.snapshot(),
        })
    }

    fn begin_turn(
        &mut self,
        session_id: &str,
        request: BeginTurnRequest,
        expected_revision: Option<ChatRevision>,
    ) -> Result<ChatUpdate, ChatCoreError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| ChatCoreError::SessionNotOpen(session_id.to_string()))?;
        Self::ensure_revision(session, expected_revision)?;

        let lane = request.lane.or(Some(ChatLane::Chat));
        session.items.push(ChatItem {
            id: request.user_item_id,
            role: ChatItemRole::User,
            content: request.user_text,
            created_at: request.created_at.clone(),
            streaming: None,
            turn_id: Some(request.turn_id.clone()),
            lane,
            ask_job_id: request.ask_job_id.clone(),
            work_id: request.work_id.clone(),
            ..empty_item()
        });
        session.items.push(ChatItem {
            id: request.assistant_item_id,
            role: ChatItemRole::Assistant,
            content: String::new(),
            created_at: request.created_at,
            streaming: Some(true),
            turn_id: Some(request.turn_id.clone()),
            lane,
            ask_job_id: request.ask_job_id,
            work_id: request.work_id,
            segments: Some(Vec::new()),
            ..empty_item()
        });
        session.active_turn_id = Some(request.turn_id);
        session.bump();
        Ok(ChatUpdate::Snapshot {
            snapshot: session.snapshot(),
        })
    }

    fn apply_stream_event(
        &mut self,
        session_id: &str,
        envelope: TurnStreamEnvelopeV3,
        expected_revision: Option<ChatRevision>,
        promote_item_id: Option<String>,
    ) -> Result<ChatUpdate, ChatCoreError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| ChatCoreError::SessionNotOpen(session_id.to_string()))?;
        Self::ensure_revision(session, expected_revision)?;

        let turn_id = envelope.turn_id.clone();
        if let Some(&last) = session.last_seq_by_turn.get(&turn_id)
            && envelope.seq <= last
        {
            // Duplicate / replayed event — ignore without bumping revision.
            return Ok(ChatUpdate::Snapshot {
                snapshot: session.snapshot(),
            });
        }
        session
            .last_seq_by_turn
            .insert(turn_id.clone(), envelope.seq);

        let index = session.items.iter().position(|item| {
            item.role == ChatItemRole::Assistant
                && item.turn_id.as_deref() == Some(turn_id.as_str())
        });

        if let Some(index) = index {
            session.items[index] =
                apply_v3_envelope_to_item(session.items[index].clone(), &envelope);
        } else if v3_event_promotes_chat_message(&envelope.event) {
            let mut shell = ChatItem {
                id: promote_item_id.unwrap_or_else(|| format!("orphan:{turn_id}")),
                role: ChatItemRole::Assistant,
                content: String::new(),
                streaming: Some(true),
                turn_id: Some(turn_id.clone()),
                lane: Some(ChatLane::Chat),
                segments: Some(Vec::new()),
                ..empty_item()
            };
            shell = apply_v3_envelope_to_item(shell, &envelope);
            session.items.push(shell);
        } else {
            return Ok(ChatUpdate::Snapshot {
                snapshot: session.snapshot(),
            });
        }

        if session.active_turn_id.is_none() {
            session.active_turn_id = Some(turn_id);
        }
        session.bump();
        Ok(ChatUpdate::Snapshot {
            snapshot: session.snapshot(),
        })
    }

    fn settle_turn(
        &mut self,
        session_id: &str,
        turn_id: &str,
        expected_revision: Option<ChatRevision>,
        reusable_stream: bool,
    ) -> Result<ChatUpdate, ChatCoreError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| ChatCoreError::SessionNotOpen(session_id.to_string()))?;
        Self::ensure_revision(session, expected_revision)?;

        if let Some(item) = session.items.iter_mut().find(|item| {
            item.role == ChatItemRole::Assistant && item.turn_id.as_deref() == Some(turn_id)
        }) {
            item.streaming = Some(false);
        }
        if session.active_turn_id.as_deref() == Some(turn_id) {
            session.active_turn_id = None;
        }
        if !reusable_stream {
            session.last_seq_by_turn.remove(turn_id);
        }
        session.bump();
        Ok(ChatUpdate::Snapshot {
            snapshot: session.snapshot(),
        })
    }

    fn link_worker(
        &mut self,
        session_id: &str,
        work_id: String,
        parent_turn_id: Option<String>,
        parent_item_id: Option<String>,
        reply_to_work_id: Option<String>,
        expected_revision: Option<ChatRevision>,
    ) -> Result<ChatUpdate, ChatCoreError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| ChatCoreError::SessionNotOpen(session_id.to_string()))?;
        Self::ensure_revision(session, expected_revision)?;

        let resolved_parent = resolve_worker_parent_item_id(
            &session.items,
            parent_item_id.as_deref(),
            parent_turn_id.as_deref(),
            Some(work_id.as_str()),
        )
        .map(str::to_string)
        .or(parent_item_id);

        match session.workers.get_mut(&work_id) {
            Some(existing) => {
                // Spawn parent is immutable once set.
                if existing.parent_turn_id.is_none() {
                    existing.parent_turn_id = parent_turn_id;
                }
                if existing.parent_item_id.is_none() {
                    existing.parent_item_id = resolved_parent;
                }
                if let Some(reply) = reply_to_work_id {
                    existing.reply_to_work_id = Some(reply);
                }
            }
            None => {
                session.workers.insert(
                    work_id.clone(),
                    WorkerLink {
                        work_id,
                        parent_turn_id,
                        parent_item_id: resolved_parent,
                        reply_to_work_id,
                    },
                );
            }
        }
        session.bump();
        Ok(ChatUpdate::Snapshot {
            snapshot: session.snapshot(),
        })
    }
}

fn empty_item() -> ChatItem {
    ChatItem {
        id: String::new(),
        role: ChatItemRole::Assistant,
        content: String::new(),
        created_at: None,
        streaming: None,
        turn_id: None,
        turn_index: None,
        lane: None,
        ask_job_id: None,
        work_id: None,
        phase: None,
        status_line: None,
        tools: None,
        tool_runs: None,
        segments: None,
        ui_artifacts: None,
        reasoning: None,
        answer_state: None,
        response_provider: None,
        response_model: None,
        stage_whisper: None,
        budget_request_id: None,
        requested_rounds: None,
        failed: None,
        error_line: None,
        error_detail: None,
        speaker_profile_id: None,
        media_attachments: None,
        host_context: None,
        reactions: None,
        live_transcripts: None,
        transcript: None,
    }
}

/// Reject updates that belong to a different focused session.
pub fn guard_focused_session(
    focused: Option<&str>,
    session_id: &str,
) -> Result<(), ChatCoreError> {
    match focused {
        Some(current) if current == session_id => Ok(()),
        Some(_) => Err(ChatCoreError::Rejected(format!(
            "stale update for session `{session_id}` while focused elsewhere"
        ))),
        None => Ok(()),
    }
}
