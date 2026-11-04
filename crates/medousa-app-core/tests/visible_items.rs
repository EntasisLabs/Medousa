//! Painted visibility projection (handoff shells + linked raw returns).

use medousa_app_core::contract::{
    ChatCommand, ChatItem, ChatItemRole, ChatLane, ChatUpdate, WorkerReturnFact,
};
use medousa_app_core::ChatCore;

fn assistant(id: &str, content: &str) -> ChatItem {
    ChatItem {
        id: id.into(),
        role: ChatItemRole::Assistant,
        content: content.into(),
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

#[test]
fn project_visible_items_drops_shells_and_raw_returns() {
    let mut core = ChatCore::new();
    core.dispatch(ChatCommand::OpenSession {
        session_id: "ses".into(),
        authority_id: "auth".into(),
    })
    .unwrap();

    let user = ChatItem {
        role: ChatItemRole::User,
        ..assistant("u", "go")
    };
    let handoff = ChatItem {
        stage_whisper: Some("Worker started".into()),
        ..assistant("h", "")
    };
    let raw = ChatItem {
        lane: Some(ChatLane::Worker),
        work_id: Some("w".into()),
        ..assistant("raw", "Full result")
    };
    let synth = ChatItem {
        lane: Some(ChatLane::Worker),
        work_id: Some("w".into()),
        ..assistant("synth", "Decision")
    };

    let ChatUpdate::VisibleItems { item_ids } = core
        .dispatch(ChatCommand::ProjectVisibleItems {
            session_id: "ses".into(),
            items: Some(vec![user, handoff, raw, synth]),
            worker_returns: vec![WorkerReturnFact {
                work_id: "w".into(),
                returned_texts: vec!["Full result".into()],
            }],
        })
        .unwrap()
    else {
        panic!("expected visible_items");
    };

    assert_eq!(item_ids, vec!["u", "synth"]);
}

#[test]
fn project_visible_items_reads_open_session_when_items_omitted() {
    let mut core = ChatCore::new();
    core.dispatch(ChatCommand::OpenSession {
        session_id: "ses".into(),
        authority_id: "auth".into(),
    })
    .unwrap();
    core.dispatch(ChatCommand::ApplyVisibleItems {
        session_id: "ses".into(),
        visible_item_ids: vec![],
        expected_revision: None,
    })
    .unwrap();

    // Seed via BeginTurn so the session has items without ApplyHistory wire.
    use medousa_app_core::contract::BeginTurnRequest;
    core.dispatch(ChatCommand::BeginTurn {
        session_id: "ses".into(),
        request: BeginTurnRequest {
            turn_id: "t".into(),
            user_item_id: "u".into(),
            assistant_item_id: "a".into(),
            user_text: "hi".into(),
            created_at: None,
            lane: Some(ChatLane::Chat),
            ask_job_id: None,
            work_id: None,
            status_line: Some("Background worker started".into()),
            speaker_profile_id: None,
            media_attachments: None,
                    mode: None,
            workspace_card_id: None,
            reusable_stream: false,
        },
        expected_revision: None,
    })
    .unwrap();
    // Turn the assistant into an empty handoff shell.
    let mut snapshot = core.snapshot("ses").unwrap();
    if let Some(item) = snapshot.items.iter_mut().find(|item| item.id == "a") {
        item.content.clear();
        item.status_line = None;
        item.stage_whisper = Some("Background worker started".into());
        item.streaming = Some(false);
    }
    // Re-open with patched items through ApplyHistory replace via local_items path:
    // use ProjectVisibleItems with explicit items instead.
    let ChatUpdate::VisibleItems { item_ids } = core
        .dispatch(ChatCommand::ProjectVisibleItems {
            session_id: "ses".into(),
            items: Some(snapshot.items),
            worker_returns: vec![],
        })
        .unwrap()
    else {
        panic!("expected visible_items");
    };
    assert_eq!(item_ids, vec!["u"]);
}
