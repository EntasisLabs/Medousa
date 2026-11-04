use medousa_app_core::{
    ChatCommand, ChatCore, ChatItemRole, ChatLane, ChatUpdate, TurnFact,
};

fn open(core: &mut ChatCore, session_id: &str) {
    core.dispatch(ChatCommand::OpenSession {
        session_id: session_id.into(),
        authority_id: format!("auth_{}", "0".repeat(64)),
    })
    .unwrap();
}

#[test]
fn register_turn_seeds_assistant_shell_when_missing() {
    let mut core = ChatCore::default();
    open(&mut core, "ses_a");

    let ChatUpdate::Snapshot { snapshot } = core
        .dispatch(ChatCommand::RegisterTurn {
            session_id: "ses_a".into(),
            turn: TurnFact {
                turn_id: "turn_1".into(),
                mode: "interactive".into(),
                phase: "streaming".into(),
                message_id: Some("asst_1".into()),
                workspace_card_id: None,
                reusable_stream: false,
                budget_request_id: None,
                requested_rounds: None,
            },
            reset_assistant_content: false,
            expected_revision: None,
        })
        .unwrap()
    else {
        panic!("expected snapshot");
    };

    assert_eq!(snapshot.items.len(), 1);
    let item = &snapshot.items[0];
    assert_eq!(item.id, "asst_1");
    assert_eq!(item.role, ChatItemRole::Assistant);
    assert_eq!(item.turn_id.as_deref(), Some("turn_1"));
    assert_eq!(item.lane, Some(ChatLane::Chat));
    assert_eq!(item.streaming, Some(true));
    assert!(snapshot.turns.iter().any(|t| t.turn_id == "turn_1"));
}

#[test]
fn register_turn_ask_mode_sets_ask_lane() {
    let mut core = ChatCore::default();
    open(&mut core, "ses_a");

    let ChatUpdate::Snapshot { snapshot } = core
        .dispatch(ChatCommand::RegisterTurn {
            session_id: "ses_a".into(),
            turn: TurnFact {
                turn_id: "ask_1".into(),
                mode: "background".into(),
                phase: "streaming".into(),
                message_id: Some("asst_ask".into()),
                workspace_card_id: Some("card_1".into()),
                reusable_stream: false,
                budget_request_id: None,
                requested_rounds: None,
            },
            reset_assistant_content: false,
            expected_revision: None,
        })
        .unwrap()
    else {
        panic!("expected snapshot");
    };

    let item = &snapshot.items[0];
    assert_eq!(item.lane, Some(ChatLane::Ask));
    assert_eq!(item.ask_job_id.as_deref(), Some("card_1"));
}
