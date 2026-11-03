//! Stateful chat session projections and command dispatch.

use std::collections::{BTreeMap, HashMap};

use medousa_types::TurnStreamEnvelopeV3;
use thiserror::Error;

use crate::agents::{
    project_agent_placement, proposal_relationships_from_groups, relationships_from_items,
    resolve_worker_parent_item_id,
};
use crate::contract::{
    AskThreadBatch, BeginTurnRequest, ChatCommand, ChatItem, ChatItemRole, ChatLane, ChatRevision,
    ChatSnapshot, ChatUpdate, ProposalFact, TurnFact, WorkerLinkFact, WorkerPlacementFact,
    WorkerReturnFact,
};
use crate::display::present_chat_item_ids;
use crate::merge::{dedupe_messages_by_id, merge_transcript};
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

struct LinkWorkerArgs<'a> {
    session_id: &'a str,
    work_id: String,
    parent_turn_id: Option<String>,
    parent_item_id: Option<String>,
    reply_to_work_id: Option<String>,
    worker_session_id: Option<String>,
    expected_revision: Option<ChatRevision>,
}

#[derive(Debug, Clone)]
struct WorkerLink {
    work_id: String,
    parent_turn_id: Option<String>,
    parent_item_id: Option<String>,
    reply_to_work_id: Option<String>,
    synthesis_item_id: Option<String>,
    synthesis_delivered: bool,
    session_id: Option<String>,
}

#[derive(Debug, Clone)]
struct TurnRecord {
    turn_id: String,
    mode: String,
    phase: String,
    message_id: Option<String>,
    workspace_card_id: Option<String>,
    reusable_stream: bool,
    budget_request_id: Option<String>,
    requested_rounds: Option<u64>,
}

impl TurnRecord {
    fn to_fact(&self) -> TurnFact {
        TurnFact {
            turn_id: self.turn_id.clone(),
            mode: self.mode.clone(),
            phase: self.phase.clone(),
            message_id: self.message_id.clone(),
            workspace_card_id: self.workspace_card_id.clone(),
            reusable_stream: self.reusable_stream,
            budget_request_id: self.budget_request_id.clone(),
            requested_rounds: self.requested_rounds,
        }
    }
}

fn synthesis_item_id_for(work_id: &str) -> String {
    format!("worker-synth:{work_id}")
}

fn turn_mode_from_request(request: &BeginTurnRequest) -> String {
    if let Some(mode) = request.mode.as_deref().map(str::trim).filter(|m| !m.is_empty()) {
        return mode.to_string();
    }
    match request.lane {
        Some(ChatLane::Ask) | Some(ChatLane::Worker) => "background".into(),
        _ => "interactive".into(),
    }
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
    /// Insertion order for placement when segment coordinates are missing.
    worker_order: Vec<String>,
    turns: BTreeMap<String, TurnRecord>,
    proposals: Vec<ProposalFact>,
    visible_item_ids: Vec<String>,
}

impl SessionState {
    fn worker_facts(&self) -> Vec<WorkerPlacementFact> {
        self.worker_order
            .iter()
            .filter_map(|work_id| self.workers.get(work_id))
            .map(|link| WorkerPlacementFact {
                work_id: link.work_id.clone(),
                parent_turn_id: link.parent_turn_id.clone(),
                parent_item_id: link.parent_item_id.clone(),
            })
            .collect()
    }

    fn placement_groups(&self) -> Vec<crate::contract::AgentPlacementGroup> {
        let visible = if self.visible_item_ids.is_empty() {
            self.items.iter().map(|item| item.id.clone()).collect()
        } else {
            self.visible_item_ids.clone()
        };
        project_agent_placement(
            &self.items,
            &visible,
            &self.proposals,
            &self.worker_facts(),
            Some(self.authority_id.as_str()),
            Some(self.session_id.as_str()),
        )
    }

    fn worker_link_facts(&self) -> Vec<WorkerLinkFact> {
        self.worker_order
            .iter()
            .filter_map(|work_id| self.workers.get(work_id))
            .map(|link| WorkerLinkFact {
                work_id: link.work_id.clone(),
                parent_turn_id: link.parent_turn_id.clone(),
                parent_item_id: link.parent_item_id.clone(),
                reply_to_work_id: link.reply_to_work_id.clone(),
                synthesis_item_id: link.synthesis_item_id.clone(),
                synthesis_delivered: link.synthesis_delivered,
                session_id: link.session_id.clone(),
            })
            .collect()
    }

    fn turn_facts(&self) -> Vec<TurnFact> {
        self.turns.values().map(TurnRecord::to_fact).collect()
    }

    fn snapshot(&self) -> ChatSnapshot {
        let worker_tuples: Vec<_> = self
            .worker_order
            .iter()
            .filter_map(|work_id| self.workers.get(work_id))
            .map(|link| {
                (
                    link.work_id.clone(),
                    link.parent_turn_id.clone(),
                    link.parent_item_id.clone(),
                    link.reply_to_work_id.clone(),
                )
            })
            .collect();
        let agent_groups = self.placement_groups();
        let mut relationships = relationships_from_items(&self.items, &worker_tuples);
        relationships.extend(proposal_relationships_from_groups(&agent_groups));
        ChatSnapshot {
            session_id: self.session_id.clone(),
            authority_id: self.authority_id.clone(),
            revision: self.revision,
            items: self.items.clone(),
            active_turn_id: self.active_turn_id.clone(),
            relationships,
            agent_groups,
            workers: self.worker_link_facts(),
            turns: self.turn_facts(),
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
                worker_session_id,
                expected_revision,
            } => self.link_worker(LinkWorkerArgs {
                session_id: &session_id,
                work_id,
                parent_turn_id,
                parent_item_id,
                reply_to_work_id,
                worker_session_id,
                expected_revision,
            }),
            ChatCommand::RegisterTurn {
                session_id,
                turn,
                expected_revision,
            } => self.register_turn(&session_id, turn, expected_revision),
            ChatCommand::EnsureWorkerFollowUp {
                session_id,
                work_id,
                turn_id,
                item_id,
                status_line,
                streaming,
                expected_revision,
            } => self.ensure_worker_follow_up(
                &session_id,
                work_id,
                turn_id,
                item_id,
                status_line,
                streaming,
                expected_revision,
            ),
            ChatCommand::FinalizeWorkerHandoff {
                session_id,
                item_id,
                expected_revision,
            } => self.finalize_worker_handoff(&session_id, &item_id, expected_revision),
            ChatCommand::MarkWorkerSynthesisDelivered {
                session_id,
                work_id,
                expected_revision,
            } => self.mark_worker_synthesis_delivered(&session_id, &work_id, expected_revision),
            ChatCommand::DeliverWorkerSynthesis {
                session_id,
                work_id,
                content,
                tools,
                expected_revision,
            } => self.deliver_worker_synthesis(
                &session_id,
                work_id,
                content,
                tools,
                expected_revision,
            ),
            ChatCommand::ApplyProposals {
                session_id,
                proposals,
                visible_item_ids,
                expected_revision,
            } => self.apply_proposals(&session_id, proposals, visible_item_ids, expected_revision),
            ChatCommand::ApplyVisibleItems {
                session_id,
                visible_item_ids,
                expected_revision,
            } => self.apply_visible_items(&session_id, visible_item_ids, expected_revision),
            ChatCommand::ProjectAgentPlacement {
                session_id,
                authority_id,
                items,
                proposals,
                workers,
                visible_item_ids,
            } => self.project_placement(
                &session_id,
                &authority_id,
                items,
                proposals,
                workers,
                visible_item_ids,
            ),
            ChatCommand::HydrateAskThreads {
                session_id,
                batches,
                promoted_ask_ids,
                expected_revision,
            } => self.hydrate_ask_threads(
                &session_id,
                batches,
                promoted_ask_ids,
                expected_revision,
            ),
            ChatCommand::PromoteAskToChat {
                session_id,
                ask_job_id,
                expected_revision,
            } => self.promote_ask_to_chat(&session_id, &ask_job_id, expected_revision),
            ChatCommand::ProjectVisibleItems {
                session_id,
                items,
                worker_returns,
            } => self.project_visible_items(&session_id, items, worker_returns),
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
                worker_order: Vec::new(),
                turns: BTreeMap::new(),
                proposals: Vec::new(),
                visible_item_ids: Vec::new(),
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
        let mode = turn_mode_from_request(&request);
        let workspace_card_id = request
            .workspace_card_id
            .clone()
            .or_else(|| request.ask_job_id.clone())
            .or_else(|| request.work_id.clone());
        let assistant_item_id = request.assistant_item_id.clone();
        let turn_id = request.turn_id.clone();
        let reusable_stream = request.reusable_stream;
        session.items.push(ChatItem {
            id: request.user_item_id,
            role: ChatItemRole::User,
            content: request.user_text,
            created_at: request.created_at.clone(),
            streaming: None,
            turn_id: Some(turn_id.clone()),
            lane,
            ask_job_id: request.ask_job_id.clone(),
            work_id: request.work_id.clone(),
            speaker_profile_id: request.speaker_profile_id.clone(),
            media_attachments: request.media_attachments.clone(),
            ..empty_item()
        });
        session.items.push(ChatItem {
            id: assistant_item_id.clone(),
            role: ChatItemRole::Assistant,
            content: String::new(),
            created_at: request.created_at,
            streaming: Some(true),
            turn_id: Some(turn_id.clone()),
            lane,
            ask_job_id: request.ask_job_id,
            work_id: request.work_id,
            status_line: request.status_line,
            segments: Some(Vec::new()),
            ..empty_item()
        });
        session.turns.insert(
            turn_id.clone(),
            TurnRecord {
                turn_id: turn_id.clone(),
                mode,
                phase: "streaming".into(),
                message_id: Some(assistant_item_id),
                workspace_card_id,
                reusable_stream,
                budget_request_id: None,
                requested_rounds: None,
            },
        );
        session.active_turn_id = Some(turn_id);
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

        let index = resolve_stream_target_index(session, &turn_id, &envelope.event);

        let target_item_id = if let Some(index) = index {
            session.items[index] =
                apply_v3_envelope_to_item(session.items[index].clone(), &envelope);
            Some(session.items[index].id.clone())
        } else if v3_event_promotes_chat_message(&envelope.event) {
            let id = promote_item_id.unwrap_or_else(|| format!("orphan:{turn_id}"));
            let mut shell = ChatItem {
                id: id.clone(),
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
            Some(id)
        } else {
            None
        };

        if target_item_id.is_some() || session.turns.contains_key(&turn_id) {
            update_turn_from_stream(session, &turn_id, &envelope.event, target_item_id.as_deref());
        } else if target_item_id.is_none() {
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

        for item in session.items.iter_mut() {
            if item.role == ChatItemRole::Assistant && item.turn_id.as_deref() == Some(turn_id) {
                item.streaming = Some(false);
            }
        }
        if session.active_turn_id.as_deref() == Some(turn_id) {
            session.active_turn_id = None;
        }
        let keep_seq = reusable_stream
            || session
                .turns
                .get(turn_id)
                .is_some_and(|turn| turn.reusable_stream);
        session.turns.remove(turn_id);
        if !keep_seq {
            session.last_seq_by_turn.remove(turn_id);
        }
        session.bump();
        Ok(ChatUpdate::Snapshot {
            snapshot: session.snapshot(),
        })
    }

    fn link_worker(&mut self, args: LinkWorkerArgs<'_>) -> Result<ChatUpdate, ChatCoreError> {
        let session = self
            .sessions
            .get_mut(args.session_id)
            .ok_or_else(|| ChatCoreError::SessionNotOpen(args.session_id.to_string()))?;
        Self::ensure_revision(session, args.expected_revision)?;

        let resolved_parent = resolve_worker_parent_item_id(
            &session.items,
            args.parent_item_id.as_deref(),
            args.parent_turn_id.as_deref(),
            Some(args.work_id.as_str()),
        )
        .map(str::to_string)
        .or(args.parent_item_id);

        match session.workers.get_mut(&args.work_id) {
            Some(existing) => {
                // Spawn parent is immutable once set.
                if existing.parent_turn_id.is_none() {
                    existing.parent_turn_id = args.parent_turn_id;
                }
                if existing.parent_item_id.is_none() {
                    existing.parent_item_id = resolved_parent;
                }
                if let Some(reply) = args.reply_to_work_id {
                    existing.reply_to_work_id = Some(reply);
                }
                if existing.session_id.is_none() {
                    existing.session_id = args.worker_session_id;
                }
            }
            None => {
                session.worker_order.push(args.work_id.clone());
                session.workers.insert(
                    args.work_id.clone(),
                    WorkerLink {
                        work_id: args.work_id.clone(),
                        parent_turn_id: args.parent_turn_id,
                        parent_item_id: resolved_parent,
                        reply_to_work_id: args.reply_to_work_id,
                        synthesis_item_id: None,
                        synthesis_delivered: false,
                        session_id: args.worker_session_id,
                    },
                );
            }
        }
        if let Some(parent_turn) = session
            .workers
            .get(&args.work_id)
            .and_then(|link| link.parent_turn_id.clone())
            && let Some(turn) = session.turns.get_mut(&parent_turn)
            && turn.workspace_card_id.is_none()
        {
            turn.workspace_card_id = Some(args.work_id);
        }
        session.bump();
        Ok(ChatUpdate::Snapshot {
            snapshot: session.snapshot(),
        })
    }

    fn register_turn(
        &mut self,
        session_id: &str,
        turn: TurnFact,
        expected_revision: Option<ChatRevision>,
    ) -> Result<ChatUpdate, ChatCoreError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| ChatCoreError::SessionNotOpen(session_id.to_string()))?;
        Self::ensure_revision(session, expected_revision)?;
        let turn_id = turn.turn_id.trim().to_string();
        if turn_id.is_empty() {
            return Err(ChatCoreError::Rejected("turn_id must not be empty".into()));
        }
        session.turns.insert(
            turn_id.clone(),
            TurnRecord {
                turn_id: turn_id.clone(),
                mode: if turn.mode.trim().is_empty() {
                    "interactive".into()
                } else {
                    turn.mode
                },
                phase: if turn.phase.trim().is_empty() {
                    "streaming".into()
                } else {
                    turn.phase
                },
                message_id: turn.message_id,
                workspace_card_id: turn.workspace_card_id,
                reusable_stream: turn.reusable_stream,
                budget_request_id: turn.budget_request_id,
                requested_rounds: turn.requested_rounds,
            },
        );
        if session.active_turn_id.is_none() {
            session.active_turn_id = Some(turn_id);
        }
        session.bump();
        Ok(ChatUpdate::Snapshot {
            snapshot: session.snapshot(),
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn ensure_worker_follow_up(
        &mut self,
        session_id: &str,
        work_id: String,
        turn_id: Option<String>,
        item_id: Option<String>,
        status_line: Option<String>,
        streaming: Option<bool>,
        expected_revision: Option<ChatRevision>,
    ) -> Result<ChatUpdate, ChatCoreError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| ChatCoreError::SessionNotOpen(session_id.to_string()))?;
        Self::ensure_revision(session, expected_revision)?;

        if !session.workers.contains_key(&work_id) {
            session.worker_order.push(work_id.clone());
            session.workers.insert(
                work_id.clone(),
                WorkerLink {
                    work_id: work_id.clone(),
                    parent_turn_id: turn_id.clone(),
                    parent_item_id: None,
                    reply_to_work_id: None,
                    synthesis_item_id: None,
                    synthesis_delivered: false,
                    session_id: None,
                },
            );
        }

        let existing_id = session
            .workers
            .get(&work_id)
            .and_then(|link| link.synthesis_item_id.clone());
        let synthesis_id = existing_id
            .or(item_id)
            .unwrap_or_else(|| synthesis_item_id_for(&work_id));
        let turn = turn_id
            .or_else(|| {
                session
                    .workers
                    .get(&work_id)
                    .and_then(|link| link.parent_turn_id.clone())
            })
            .unwrap_or_default();
        let streaming = streaming.unwrap_or(true);

        if let Some(index) = session
            .items
            .iter()
            .position(|item| item.id == synthesis_id)
        {
            let item = &mut session.items[index];
            item.streaming = Some(streaming);
            if status_line.is_some() {
                item.status_line = status_line;
            }
            item.lane = Some(ChatLane::Worker);
            item.work_id = Some(work_id.clone());
            if !turn.is_empty() {
                item.turn_id = Some(turn);
            }
        } else {
            session.items.push(ChatItem {
                id: synthesis_id.clone(),
                role: ChatItemRole::Assistant,
                content: String::new(),
                streaming: Some(streaming),
                turn_id: if turn.is_empty() { None } else { Some(turn) },
                lane: Some(ChatLane::Worker),
                work_id: Some(work_id.clone()),
                status_line,
                segments: Some(Vec::new()),
                ..empty_item()
            });
        }

        if let Some(link) = session.workers.get_mut(&work_id)
            && link.synthesis_item_id.is_none()
        {
            link.synthesis_item_id = Some(synthesis_id);
        }

        session.bump();
        Ok(ChatUpdate::Snapshot {
            snapshot: session.snapshot(),
        })
    }

    fn finalize_worker_handoff(
        &mut self,
        session_id: &str,
        item_id: &str,
        expected_revision: Option<ChatRevision>,
    ) -> Result<ChatUpdate, ChatCoreError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| ChatCoreError::SessionNotOpen(session_id.to_string()))?;
        Self::ensure_revision(session, expected_revision)?;
        if let Some(item) = session.items.iter_mut().find(|item| item.id == item_id) {
            item.streaming = Some(false);
            item.phase = None;
            item.status_line = None;
        }
        session.bump();
        Ok(ChatUpdate::Snapshot {
            snapshot: session.snapshot(),
        })
    }

    fn mark_worker_synthesis_delivered(
        &mut self,
        session_id: &str,
        work_id: &str,
        expected_revision: Option<ChatRevision>,
    ) -> Result<ChatUpdate, ChatCoreError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| ChatCoreError::SessionNotOpen(session_id.to_string()))?;
        Self::ensure_revision(session, expected_revision)?;
        if let Some(link) = session.workers.get_mut(work_id) {
            link.synthesis_delivered = true;
        }
        session.bump();
        Ok(ChatUpdate::Snapshot {
            snapshot: session.snapshot(),
        })
    }

    fn deliver_worker_synthesis(
        &mut self,
        session_id: &str,
        work_id: String,
        content: String,
        tools: Option<Vec<String>>,
        expected_revision: Option<ChatRevision>,
    ) -> Result<ChatUpdate, ChatCoreError> {
        {
            let session = self
                .sessions
                .get(session_id)
                .ok_or_else(|| ChatCoreError::SessionNotOpen(session_id.to_string()))?;
            Self::ensure_revision(session, expected_revision)?;
            if session
                .workers
                .get(&work_id)
                .is_some_and(|link| link.synthesis_delivered)
            {
                return Ok(ChatUpdate::Snapshot {
                    snapshot: session.snapshot(),
                });
            }
        }

        let turn_id = self
            .sessions
            .get(session_id)
            .and_then(|session| session.workers.get(&work_id))
            .and_then(|link| link.parent_turn_id.clone());
        let handoff_id = self
            .sessions
            .get(session_id)
            .and_then(|session| session.workers.get(&work_id))
            .and_then(|link| link.parent_item_id.clone());

        self.ensure_worker_follow_up(
            session_id,
            work_id.clone(),
            turn_id,
            None,
            None,
            Some(false),
            None,
        )?;

        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| ChatCoreError::SessionNotOpen(session_id.to_string()))?;
        let synthesis_id = session
            .workers
            .get(&work_id)
            .and_then(|link| link.synthesis_item_id.clone())
            .unwrap_or_else(|| synthesis_item_id_for(&work_id));
        if let Some(item) = session
            .items
            .iter_mut()
            .find(|item| item.id == synthesis_id)
        {
            item.content = content;
            item.streaming = Some(false);
            item.failed = Some(false);
            item.error_line = None;
            item.answer_state = None;
            item.phase = None;
            item.status_line = None;
            item.lane = Some(ChatLane::Worker);
            item.work_id = Some(work_id.clone());
            if let Some(tools) = tools {
                item.tools = Some(tools);
            }
        }
        if let Some(handoff_id) = handoff_id.as_deref()
            && let Some(item) = session.items.iter_mut().find(|item| item.id == handoff_id)
        {
            item.streaming = Some(false);
            item.phase = None;
            item.status_line = None;
        }
        if let Some(link) = session.workers.get_mut(&work_id) {
            link.synthesis_delivered = true;
            if link.synthesis_item_id.is_none() {
                link.synthesis_item_id = Some(synthesis_id);
            }
        }
        session.bump();
        Ok(ChatUpdate::Snapshot {
            snapshot: session.snapshot(),
        })
    }

    fn apply_proposals(
        &mut self,
        session_id: &str,
        proposals: Vec<ProposalFact>,
        visible_item_ids: Option<Vec<String>>,
        expected_revision: Option<ChatRevision>,
    ) -> Result<ChatUpdate, ChatCoreError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| ChatCoreError::SessionNotOpen(session_id.to_string()))?;
        Self::ensure_revision(session, expected_revision)?;
        session.proposals = proposals;
        if let Some(visible) = visible_item_ids {
            session.visible_item_ids = visible;
        }
        session.bump();
        Ok(ChatUpdate::Snapshot {
            snapshot: session.snapshot(),
        })
    }

    fn apply_visible_items(
        &mut self,
        session_id: &str,
        visible_item_ids: Vec<String>,
        expected_revision: Option<ChatRevision>,
    ) -> Result<ChatUpdate, ChatCoreError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| ChatCoreError::SessionNotOpen(session_id.to_string()))?;
        Self::ensure_revision(session, expected_revision)?;
        session.visible_item_ids = visible_item_ids;
        session.bump();
        Ok(ChatUpdate::Snapshot {
            snapshot: session.snapshot(),
        })
    }

    fn project_placement(
        &self,
        session_id: &str,
        authority_id: &str,
        items: Option<Vec<ChatItem>>,
        proposals: Vec<ProposalFact>,
        workers: Vec<WorkerPlacementFact>,
        visible_item_ids: Vec<String>,
    ) -> Result<ChatUpdate, ChatCoreError> {
        let session_items = self.sessions.get(session_id).map(|session| &session.items);
        let items = match items {
            Some(items) => items,
            None => session_items
                .cloned()
                .ok_or_else(|| ChatCoreError::SessionNotOpen(session_id.to_string()))?,
        };
        let scope_authority = if authority_id.trim().is_empty() {
            self.sessions
                .get(session_id)
                .map(|session| session.authority_id.as_str())
        } else {
            Some(authority_id)
        };
        let scope_session = Some(session_id);
        let groups = project_agent_placement(
            &items,
            &visible_item_ids,
            &proposals,
            &workers,
            scope_authority,
            scope_session,
        );
        Ok(ChatUpdate::AgentPlacement { groups })
    }

    fn hydrate_ask_threads(
        &mut self,
        session_id: &str,
        batches: Vec<AskThreadBatch>,
        promoted_ask_ids: Vec<String>,
        expected_revision: Option<ChatRevision>,
    ) -> Result<ChatUpdate, ChatCoreError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| ChatCoreError::SessionNotOpen(session_id.to_string()))?;
        Self::ensure_revision(session, expected_revision)?;

        let promoted: std::collections::HashSet<&str> =
            promoted_ask_ids.iter().map(String::as_str).collect();
        let mut already: std::collections::HashSet<String> = session
            .items
            .iter()
            .filter_map(|item| {
                item.ask_job_id
                    .as_ref()
                    .map(|id| id.trim().to_string())
                    .filter(|id| !id.is_empty())
            })
            .collect();

        let mut appended = Vec::new();
        for batch in batches {
            let job_id = batch.ask_job_id.trim().to_string();
            if job_id.is_empty() {
                continue;
            }
            if promoted.contains(job_id.as_str()) || already.contains(&job_id) {
                continue;
            }
            if batch.turns.is_empty() {
                continue;
            }
            let projected = project_wire_history(
                &batch.ask_session_id,
                &batch.authority_id,
                &decode_wire_turns(&batch.turns),
                Some(ChatLane::Ask),
                Some(job_id.clone()),
            );
            if projected.is_empty() {
                continue;
            }
            appended.extend(projected);
            already.insert(job_id);
        }

        if !appended.is_empty() {
            let mut next = session.items.clone();
            next.extend(appended);
            session.items = dedupe_messages_by_id(next);
            session.bump();
        }

        Ok(ChatUpdate::Snapshot {
            snapshot: session.snapshot(),
        })
    }

    fn promote_ask_to_chat(
        &mut self,
        session_id: &str,
        ask_job_id: &str,
        expected_revision: Option<ChatRevision>,
    ) -> Result<ChatUpdate, ChatCoreError> {
        let trimmed = ask_job_id.trim();
        if trimmed.is_empty() {
            return Err(ChatCoreError::Rejected(
                "ask_job_id must not be empty".into(),
            ));
        }
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| ChatCoreError::SessionNotOpen(session_id.to_string()))?;
        Self::ensure_revision(session, expected_revision)?;

        let mut changed = false;
        for item in &mut session.items {
            if item.ask_job_id.as_deref() == Some(trimmed) {
                item.lane = Some(ChatLane::Chat);
                item.ask_job_id = None;
                changed = true;
            }
        }
        if changed {
            session.bump();
        }
        Ok(ChatUpdate::Snapshot {
            snapshot: session.snapshot(),
        })
    }

    fn project_visible_items(
        &self,
        session_id: &str,
        items: Option<Vec<ChatItem>>,
        worker_returns: Vec<WorkerReturnFact>,
    ) -> Result<ChatUpdate, ChatCoreError> {
        let items = match items {
            Some(items) => items,
            None => self
                .sessions
                .get(session_id)
                .map(|session| session.items.clone())
                .ok_or_else(|| ChatCoreError::SessionNotOpen(session_id.to_string()))?,
        };
        Ok(ChatUpdate::VisibleItems {
            item_ids: present_chat_item_ids(&items, &worker_returns),
        })
    }
}

fn update_turn_from_stream(
    session: &mut SessionState,
    turn_id: &str,
    event: &medousa_types::TurnStreamEventV3,
    target_item_id: Option<&str>,
) {
    use medousa_types::{TurnStreamEventV3, WorkerAckKind};

    let Some(turn) = session.turns.get_mut(turn_id) else {
        return;
    };
    match event {
        TurnStreamEventV3::WorkerAck { ack_kind, work_id, .. } => {
            turn.phase = match ack_kind {
                WorkerAckKind::Workshop => "workshop_handoff",
                WorkerAckKind::Worker => "worker_handoff",
            }
            .into();
            // Handoff releases the live assistant binding.
            turn.message_id = None;
            if let Some(work_id) = work_id.clone() {
                turn.workspace_card_id = Some(work_id);
            }
        }
        TurnStreamEventV3::BudgetApprovalRequired {
            request_id,
            requested_rounds,
            ..
        } => {
            turn.phase = "budget_blocked".into();
            turn.message_id = target_item_id.map(str::to_string);
            turn.budget_request_id = Some(request_id.clone());
            turn.requested_rounds = Some(*requested_rounds as u64);
            turn.workspace_card_id = Some(request_id.clone());
        }
        TurnStreamEventV3::Status { phase, .. } => {
            if turn.phase != "worker_handoff"
                && turn.phase != "workshop_handoff"
                && turn.phase != "budget_blocked"
            {
                turn.phase = phase.clone();
            }
            if let Some(item_id) = target_item_id
                && turn.message_id.is_none()
            {
                turn.message_id = Some(item_id.to_string());
            }
        }
        TurnStreamEventV3::WorkerSynthesis { work_id, .. } => {
            if let Some(work_id) = work_id.clone() {
                turn.workspace_card_id = Some(work_id);
            }
            if let Some(item_id) = target_item_id {
                turn.message_id = Some(item_id.to_string());
            }
            turn.phase = "streaming".into();
        }
        TurnStreamEventV3::TurnCompleted { .. } => {
            if turn.mode == "background"
                && (turn.phase == "worker_handoff" || turn.phase == "workshop_handoff")
            {
                // Keep handoff phase until synthesis settles.
            } else {
                turn.phase = "done".into();
            }
        }
        _ => {
            if let Some(item_id) = target_item_id
                && turn.message_id.is_none()
                && turn.phase != "worker_handoff"
                && turn.phase != "workshop_handoff"
            {
                turn.message_id = Some(item_id.to_string());
            }
        }
    }
}

fn resolve_stream_target_index(
    session: &SessionState,
    turn_id: &str,
    event: &medousa_types::TurnStreamEventV3,
) -> Option<usize> {
    use medousa_types::TurnStreamEventV3;

    let worker_for_turn = session.workers.values().find(|link| {
        link.parent_turn_id.as_deref() == Some(turn_id) || {
            link.synthesis_item_id.as_ref().is_some_and(|id| {
                session
                    .items
                    .iter()
                    .any(|item| item.id == *id && item.turn_id.as_deref() == Some(turn_id))
            })
        }
    });

    let prefers_synthesis = matches!(event, TurnStreamEventV3::WorkerSynthesis { .. })
        || (matches!(
            event,
            TurnStreamEventV3::ContentAppend { .. }
                | TurnStreamEventV3::AssistantTextStarted { .. }
                | TurnStreamEventV3::AssistantTextCommitted { .. }
                | TurnStreamEventV3::TurnCompleted { .. }
        ) && worker_for_turn.is_some_and(|link| {
            !link.synthesis_delivered && link.synthesis_item_id.is_some()
        }));

    let prefers_handoff = matches!(
        event,
        TurnStreamEventV3::WorkerAck { .. }
            | TurnStreamEventV3::ToolStarted { .. }
            | TurnStreamEventV3::ToolFinished { .. }
    );

    if prefers_synthesis
        && let Some(link) = worker_for_turn
        && let Some(synthesis_id) = link.synthesis_item_id.as_deref()
        && let Some(index) = session.items.iter().position(|item| item.id == synthesis_id)
    {
        return Some(index);
    }

    if prefers_handoff
        && let Some(link) = worker_for_turn
        && let Some(parent_id) = link.parent_item_id.as_deref()
        && let Some(index) = session.items.iter().position(|item| item.id == parent_id)
    {
        return Some(index);
    }

    // Prefer non-worker assistant for the turn; fall back to any assistant.
    session
        .items
        .iter()
        .position(|item| {
            item.role == ChatItemRole::Assistant
                && item.turn_id.as_deref() == Some(turn_id)
                && !matches!(item.lane, Some(ChatLane::Worker))
        })
        .or_else(|| {
            session.items.iter().position(|item| {
                item.role == ChatItemRole::Assistant && item.turn_id.as_deref() == Some(turn_id)
            })
        })
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
