//! Thin RNDR host projection over the canonical application core.
use medousa_app_core::{
    BeginTurnRequest, ChatCommand, ChatCore, ChatSnapshot, ChatUpdate, TurnFact,
};
use medousa_types::{InteractiveTurnRequest, TurnStreamEnvelopeV3};

#[derive(Debug, Default)]
pub struct Chat {
    core: ChatCore,
    snapshot: Option<ChatSnapshot>,
    pub rows: Vec<(String, String, String)>,
    // Host connection notices only; execution state lives in the snapshot.
    pub status: String,
    pub uncertain: bool,
}
impl Chat {
    pub fn offline() -> Self {
        Self {
            status: "Offline preview · no daemon connected".into(),
            ..Self::default()
        }
    }
    pub fn session(&self) -> Option<&str> {
        self.snapshot.as_ref().map(|s| s.session_id.as_str())
    }
    pub fn snapshot(&self) -> Option<&ChatSnapshot> {
        self.snapshot.as_ref()
    }
    pub fn busy(&self) -> bool {
        self.snapshot
            .as_ref()
            .is_some_and(|s| s.active_turn_id.is_some())
    }
    pub fn dispatch(&mut self, command: ChatCommand) -> Result<(), String> {
        match self.core.dispatch(command).map_err(|e| e.to_string())? {
            ChatUpdate::Snapshot { snapshot } => self.snapshot = Some(snapshot),
            _ => return Err("Expected canonical chat snapshot".into()),
        }
        self.refresh_rows();
        Ok(())
    }
    fn refresh_rows(&mut self) {
        use medousa_app_core::{AgentPlacementEntry, ChatItemRole, present_chat_items};
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        let visible: Vec<_> = present_chat_items(&snapshot.items, &[])
            .into_iter()
            .cloned()
            .collect();
        let groups = match self.core.dispatch(ChatCommand::ProjectAgentPlacement {
            session_id: snapshot.session_id.clone(),
            authority_id: snapshot.authority_id.clone(),
            visible_item_ids: visible.iter().map(|i| i.id.clone()).collect(),
            items: Some(visible.clone()),
            proposals: vec![],
            workers: snapshot
                .workers
                .iter()
                .map(|w| medousa_app_core::WorkerPlacementFact {
                    work_id: w.work_id.clone(),
                    parent_turn_id: w.parent_turn_id.clone(),
                    parent_item_id: w.parent_item_id.clone(),
                })
                .collect(),
        }) {
            Ok(ChatUpdate::AgentPlacement { groups }) => groups,
            _ => Vec::new(),
        };
        self.rows.clear();
        // The core supplies visibility, ordering and anchor IDs. RNDR supplies
        // only literal labels/body presentation, never a second timeline sort.
        for item in std::iter::once(None).chain(visible.iter().map(Some)) {
            let anchor = item.map(|item| item.id.as_str());
            if let Some(item) = item {
                let role = match item.role {
                    ChatItemRole::User => "user",
                    ChatItemRole::Assistant => "assistant",
                    ChatItemRole::System => "system",
                };
                let mut body = item.content.clone();
                if body.is_empty() {
                    body = item
                        .error_line
                        .clone()
                        .or_else(|| item.status_line.clone())
                        .unwrap_or_default();
                }
                self.rows.push((item.id.clone(), role.into(), body));
            }
            for group in groups
                .iter()
                .filter(|g| g.anchor_item_id.as_deref() == anchor)
            {
                for entry in &group.entries {
                    match entry {
                        AgentPlacementEntry::Worker { work_id, .. } => self.rows.push((
                            format!("placement:worker:{work_id}"),
                            "system".into(),
                            format!("Worker · {work_id}"),
                        )),
                        AgentPlacementEntry::Proposal { proposal_id, .. } => self.rows.push((
                            format!("placement:proposal:{proposal_id}"),
                            "system".into(),
                            format!("Proposal · {proposal_id}"),
                        )),
                    }
                }
            }
        }
    }
    pub fn hydrate(
        &mut self,
        session: String,
        authority: String,
        turns: Vec<serde_json::Value>,
        active: Option<TurnFact>,
    ) -> Result<(), String> {
        self.dispatch(ChatCommand::OpenSession {
            session_id: session.clone(),
            authority_id: authority.clone(),
        })?;
        self.dispatch(ChatCommand::SwitchFocus {
            session_id: session.clone(),
        })?;
        if active.is_none() {
            for turn in self.snapshot().map(|s| s.turns.clone()).unwrap_or_default() {
                self.dispatch(ChatCommand::SettleTurn {
                    session_id: session.clone(),
                    turn_id: turn.turn_id,
                    expected_revision: None,
                    reusable_stream: true,
                })?;
            }
        }
        self.dispatch(ChatCommand::ApplyHistory {
            session_id: session.clone(),
            authority_id: authority,
            turns,
            lane: None,
            ask_job_id: None,
            replace: false,
            local_items: None,
            expected_revision: None,
        })?;
        // A successful daemon read owns active-ticket presence. Never infer it
        // from prose or clear a connection failure by pretending a turn finished.
        let open = self.snapshot.as_ref().unwrap().turns.clone();
        for turn in open {
            if active.as_ref().is_none_or(|a| a.turn_id != turn.turn_id) {
                self.dispatch(ChatCommand::SettleTurn {
                    session_id: session.clone(),
                    turn_id: turn.turn_id,
                    expected_revision: None,
                    reusable_stream: true,
                })?;
            }
        }
        if let Some(mut turn) = active {
            let snapshot = self.snapshot.as_ref().unwrap();
            // Keep the persisted ID, including transcript coordinate. A cold
            // attach replays from zero into this shell; a warm attach retains
            // content and lets the core's sequence cursor fence the full replay.
            turn.message_id = snapshot
                .items
                .iter()
                .find(|i| {
                    i.role == medousa_app_core::ChatItemRole::Assistant
                        && i.turn_id.as_deref() == Some(&turn.turn_id)
                })
                .map(|i| i.id.clone())
                .or_else(|| Some(format!("assistant:{}", turn.turn_id)));
            let reset = !snapshot.last_seq_by_turn.contains_key(&turn.turn_id);
            self.dispatch(ChatCommand::RegisterTurn {
                session_id: session,
                turn,
                reset_assistant_content: reset,
                expected_revision: None,
            })?;
        }
        self.uncertain = false;
        self.status = if self.busy() {
            "Reattached · receiving daemon events"
        } else {
            "History reconciled · non-atomic daemon reads"
        }
        .into();
        Ok(())
    }
    pub fn admitted(&mut self, turn: String, prompt: String) -> Result<(), String> {
        let session = self.session().ok_or("Open a session first")?.to_string();
        self.dispatch(ChatCommand::BeginTurn {
            session_id: session,
            request: BeginTurnRequest {
                turn_id: turn.clone(),
                user_item_id: format!("user:{turn}"),
                assistant_item_id: format!("assistant:{turn}"),
                user_text: prompt,
                created_at: None,
                lane: None,
                ask_job_id: None,
                work_id: None,
                status_line: None,
                speaker_profile_id: None,
                media_attachments: None,
                mode: None,
                workspace_card_id: None,
                reusable_stream: true,
            },
            expected_revision: None,
        })
    }
    pub fn apply(&mut self, envelope: TurnStreamEnvelopeV3) -> Result<(), String> {
        let session = self.session().ok_or("Open a session first")?.to_string();
        let terminal = envelope.event.is_terminal();
        let turn_id = envelope.turn_id.clone();
        let permission = matches!(
            envelope.event,
            medousa_types::TurnStreamEventV3::PermissionRequest { .. }
        );
        self.dispatch(ChatCommand::ApplyStreamEvent {
            session_id: session.clone(),
            envelope,
            expected_revision: None,
            promote_item_id: None,
        })?;
        if terminal {
            self.dispatch(ChatCommand::SettleTurn {
                session_id: session,
                turn_id,
                expected_revision: None,
                reusable_stream: true,
            })?;
        }
        if permission {
            self.status = "Permission requested · use the existing Medousa app to review; RNDR cannot approve".into();
        }
        Ok(())
    }
    pub fn fail(&mut self, message: String) {
        self.status = message;
        self.uncertain = true;
    }
    /// Synthetic/offline sample setup uses the same history contract as transport.
    pub fn set_fixture(&mut self, rows: Vec<(String, String)>) {
        let turns = rows.into_iter().enumerate().map(|(i, (role, content))| serde_json::json!({"entry_id":format!("fixture-{i}"), "entry_seq":i+1, "role":role,"content":content})).collect();
        let session = self.session().unwrap_or("synthetic-offline").to_string();
        self.dispatch(ChatCommand::OpenSession {
            session_id: session.clone(),
            authority_id: "synthetic".into(),
        })
        .unwrap();
        self.dispatch(ChatCommand::ApplyHistory {
            session_id: session,
            authority_id: "synthetic".into(),
            turns,
            lane: None,
            ask_job_id: None,
            replace: true,
            local_items: None,
            expected_revision: None,
        })
        .unwrap();
    }
}

pub fn turn_request(
    session: &str,
    prompt: &str,
    provider: &str,
    model: &str,
) -> InteractiveTurnRequest {
    // Deserialize the shared wire contract; no app-owned parallel protocol.
    serde_json::from_value(serde_json::json!({
        "session_id": session, "prompt": prompt, "persist_user_turn": true,
        "response_depth_mode": "standard", "provider": provider, "model": model,
        "stage_routing": medousa_types::StageRoutingMatrix::default_for(provider, model)
    }))
    .expect("static interactive request must match Medousa types")
}

/// Only the existing local credential boundary is supported in this slice.
/// No remote redirects, userinfo, or arbitrary credential forwarding.
pub use medousa_desktop_host::connection::local_url;
pub fn stream_path(path: &str) -> Result<&str, String> {
    if !path.starts_with("/v1/") || path.contains("://") || path.contains('\\') {
        return Err(
            "Daemon returned an unsupported stream URL; refusing credential forwarding".into(),
        );
    }
    Ok(path)
}

#[cfg(test)]
mod migration_tests {
    use super::*;
    use medousa_app_core::ChatItemRole;
    use serde_json::{Value, json};

    fn entry(id: &str, seq: u64, turn: &str, role: &str, text: &str) -> Value {
        json!({"entry_id":id,"entry_seq":seq,"role":role,"content":text,
            "timestamp":"2026-11-14T00:00:00Z","tool_names":[],
            "caused_by":{"authority_id":"auth","session_id":"s","execution_id":turn}})
    }
    fn event(turn: &str, seq: u64, event: Value) -> TurnStreamEnvelopeV3 {
        serde_json::from_value(json!({"schema_version":3,"turn_id":turn,"seq":seq,
            "emitted_at_utc":"2026-11-14T00:00:00Z","event":event}))
        .unwrap()
    }
    fn active(turn: &str) -> TurnFact {
        TurnFact {
            turn_id: turn.into(),
            mode: "interactive".into(),
            phase: "streaming".into(),
            message_id: None,
            workspace_card_id: None,
            reusable_stream: true,
            budget_request_id: None,
            requested_rounds: None,
        }
    }
    fn opened() -> Chat {
        let mut chat = Chat::offline();
        chat.hydrate("s".into(), "auth".into(), vec![], None)
            .unwrap();
        chat
    }

    #[test]
    fn identical_prompts_remain_distinct_after_durable_reconciliation() {
        let mut chat = opened();
        let mut history = vec![];
        for (index, turn) in ["one", "two"].into_iter().enumerate() {
            chat.admitted(turn.into(), "same prompt".into()).unwrap();
            chat.apply(event(turn, 1, json!({"type":"turn_completed","outcome":"completed","aggregate_text":"same answer"}))).unwrap();
            history.push(entry(
                &format!("u{index}"),
                index as u64 * 2 + 1,
                turn,
                "user",
                "same prompt",
            ));
            history.push(entry(
                &format!("a{index}"),
                index as u64 * 2 + 2,
                turn,
                "assistant",
                "same answer",
            ));
            chat.hydrate("s".into(), "auth".into(), history.clone(), None)
                .unwrap();
        }
        let snapshot = chat.snapshot().unwrap();
        assert_eq!(
            snapshot
                .items
                .iter()
                .filter(|i| i.role == ChatItemRole::User)
                .count(),
            2
        );
        assert_eq!(
            snapshot
                .items
                .iter()
                .filter(|i| i.role == ChatItemRole::Assistant)
                .count(),
            2
        );
        assert_eq!(chat.rows.len(), 4);
        assert!(!chat.busy());
    }

    #[test]
    fn warm_reconnect_keeps_live_cursor_and_mount_identity_with_history_ahead() {
        let mut chat = opened();
        chat.admitted("t".into(), "go".into()).unwrap();
        chat.apply(event(
            "t",
            1,
            json!({"type":"content_append","segment_id":"a","text":"first"}),
        ))
        .unwrap();
        let id = chat
            .snapshot()
            .unwrap()
            .items
            .iter()
            .find(|i| i.role == ChatItemRole::Assistant)
            .unwrap()
            .id
            .clone();
        let history = vec![
            entry("u", 1, "t", "user", "go"),
            entry("a", 2, "t", "assistant", "first second"),
        ];
        chat.fail("disconnected".into());
        chat.hydrate(
            "s".into(),
            "auth".into(),
            history.clone(),
            Some(active("t")),
        )
        .unwrap();
        chat.apply(event(
            "t",
            1,
            json!({"type":"content_append","segment_id":"a","text":"first"}),
        ))
        .unwrap();
        chat.apply(event(
            "t",
            2,
            json!({"type":"content_append","segment_id":"a","text":" second"}),
        ))
        .unwrap();
        chat.apply(event(
            "t",
            3,
            json!({"type":"turn_completed","outcome":"completed","aggregate_text":"first second"}),
        ))
        .unwrap();
        chat.hydrate("s".into(), "auth".into(), history, None)
            .unwrap();
        let assistants: Vec<_> = chat
            .snapshot()
            .unwrap()
            .items
            .iter()
            .filter(|i| i.role == ChatItemRole::Assistant)
            .collect();
        assert_eq!(assistants.len(), 1);
        assert_eq!(assistants[0].id, id);
        assert_eq!(assistants[0].content, "first second");
        assert!(assistants[0].transcript.is_some());
        assert!(!chat.uncertain && !chat.busy());
    }

    #[test]
    fn switching_sessions_retains_isolated_core_history() {
        let mut chat = opened();
        chat.hydrate(
            "s".into(),
            "auth".into(),
            vec![entry("u", 1, "t", "user", "first session")],
            None,
        )
        .unwrap();
        let first_id = chat.rows[0].0.clone();
        chat.hydrate("other".into(), "auth".into(), vec![], None)
            .unwrap();
        assert!(chat.rows.is_empty());
        chat.hydrate("s".into(), "auth".into(), vec![], None)
            .unwrap();
        assert_eq!(
            chat.rows,
            vec![(first_id, "user".into(), "first session".into())]
        );
    }

    #[test]
    fn worker_handoff_return_uses_core_lane_and_sequence_fence() {
        let mut chat = opened();
        chat.admitted("t".into(), "delegate".into()).unwrap();
        chat.apply(event(
            "t",
            1,
            json!({"type":"worker_ack","ack_kind":"worker","text":"Worker started","work_id":"w"}),
        ))
        .unwrap();
        let synthesis = event(
            "t",
            2,
            json!({"type":"worker_synthesis","text":"Worker result","work_id":"w"}),
        );
        chat.apply(synthesis.clone()).unwrap();
        chat.apply(synthesis).unwrap();
        chat.apply(event(
            "t",
            3,
            json!({"type":"turn_completed","outcome":"completed","aggregate_text":"Worker result"}),
        ))
        .unwrap();
        let snapshot = chat.snapshot().unwrap();
        let returned: Vec<_> = snapshot
            .items
            .iter()
            .filter(|i| i.work_id.as_deref() == Some("w"))
            .collect();
        assert_eq!(returned.len(), 1);
        assert_eq!(returned[0].content, "Worker result");
        assert_eq!(returned[0].lane, Some(medousa_app_core::ChatLane::Worker));
        assert_eq!(
            chat.rows
                .iter()
                .filter(|(_, _, text)| text == "Worker result")
                .count(),
            1
        );
        assert!(!chat.busy());
    }
}
