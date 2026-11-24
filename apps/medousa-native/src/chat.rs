//! Thin RNDR host projection over the canonical application core.
use medousa_app_core::{
    BeginTurnRequest, ChatCommand, ChatCore, ChatSnapshot, ChatUpdate, TurnFact,
};
use medousa_types::{InteractiveTurnRequest, TurnStreamEnvelopeV3};

#[derive(Debug, Default)]
pub struct Chat {
    core: ChatCore,
    snapshot: Option<ChatSnapshot>,
    // Host connection notices only; execution state lives in the snapshot.
    pub status: String,
    pub uncertain: bool,
}
impl Chat {
    pub fn connecting() -> Self {
        Self {
            status: "Connecting…".into(),
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
        let background = matches!(&command,ChatCommand::ApplyStreamEvent{session_id,..} if self.session().is_some_and(|id|id!=session_id));
        match self.core.dispatch(command).map_err(|e| e.to_string())? {
            ChatUpdate::Snapshot { snapshot } => {
                if !background {
                    self.snapshot = Some(snapshot);
                }
            }
            _ => return Err("Expected canonical chat snapshot".into()),
        }
        Ok(())
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
            "Connected"
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
            self.status = "Permission requested".into();
        }
        Ok(())
    }
    pub fn fail(&mut self, message: String) {
        self.status = message;
        self.uncertain = true;
    }
}

pub fn turn_request(
    session: &str,
    prompt: &str,
    provider: &str,
    model: &str,
) -> InteractiveTurnRequest {
    InteractiveTurnRequest {
        session_id: session.into(),
        prompt: prompt.into(),
        agent_mode: None,
        code_context: None,
        code_project_setup_authorized: false,
        worker_execution_target: None,
        persist_user_turn: true,
        response_depth_mode: "standard".into(),
        reasoning_effort: String::new(),
        provider: provider.into(),
        model: model.into(),
        stage_routing: medousa_types::StageRoutingMatrix::default_for(provider, model),
        surface: None,
        host_context: None,
        max_tool_rounds: None,
        retry_runtime_max_rounds: None,
        manuscript_id: None,
        additional_manuscript_ids: None,
        suggested_capability_ids: None,
        voice_preset_id: None,
        voice_appendix: None,
        scheduled_tool_allowlist: None,
        media_refs: vec![],
        liquid_interactions: vec![],
        identity_user_id: None,
        wake: None,
    }
}
