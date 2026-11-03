//! Ask-lane hydrate / promote into the focused chat session.

use chrono::Utc;
use medousa_app_core::contract::{
    AskThreadBatch, BeginTurnRequest, ChatCommand, ChatLane, ChatUpdate,
};
use medousa_app_core::ChatCore;
use medousa_types::{
    AuthorityId, TranscriptEntry, TranscriptEntryId,
};

fn authority() -> AuthorityId {
    AuthorityId::parse(format!("auth_{}", "a".repeat(64))).unwrap()
}

fn entry(id_hex: &str, seq: u64, role: &str, content: &str) -> TranscriptEntry {
    use medousa_types::ConversationTurn;
    TranscriptEntry {
        entry_id: TranscriptEntryId::parse(format!("ent_{id_hex}")).unwrap(),
        entry_seq: seq,
        caused_by: None,
        source: None,
        content_digest: "digest".into(),
        turn: ConversationTurn {
            role: role.into(),
            content: content.into(),
            timestamp: Utc::now(),
            tool_names: Vec::new(),
            answer_state: None,
            parts: None,
            slice_summary: None,
            speaker_profile_id: None,
            reactions: Vec::new(),
        },
    }
}

fn open(core: &mut ChatCore) {
    core.dispatch(ChatCommand::OpenSession {
        session_id: "ses".into(),
        authority_id: authority().as_str().into(),
    })
    .unwrap();
}

fn ask_batch(job_id: &str, turns: Vec<TranscriptEntry>) -> AskThreadBatch {
    AskThreadBatch {
        ask_job_id: job_id.into(),
        ask_session_id: format!("medousa-ask-{job_id}"),
        authority_id: authority().as_str().into(),
        turns: turns
            .iter()
            .map(|turn| serde_json::to_value(turn).unwrap())
            .collect(),
    }
}

#[test]
fn hydrate_appends_ask_lane_into_focused_session() {
    let mut core = ChatCore::new();
    open(&mut core);
    core.dispatch(ChatCommand::BeginTurn {
        session_id: "ses".into(),
        request: BeginTurnRequest {
            turn_id: "chat-1".into(),
            user_item_id: "u-chat".into(),
            assistant_item_id: "a-chat".into(),
            user_text: "main".into(),
            created_at: None,
            lane: Some(ChatLane::Chat),
            ask_job_id: None,
            work_id: None,
            status_line: None,
            speaker_profile_id: None,
            media_attachments: None,
                    mode: None,
            workspace_card_id: None,
            reusable_stream: false,
        },
        expected_revision: None,
    })
    .unwrap();

    let job = "medousa-daemon-ask-1";
    let ChatUpdate::Snapshot { snapshot } = core
        .dispatch(ChatCommand::HydrateAskThreads {
            session_id: "ses".into(),
            batches: vec![ask_batch(
                job,
                vec![
                    entry(&"1".repeat(32), 1, "user", "ask me"),
                    entry(&"2".repeat(32), 2, "assistant", "asked"),
                ],
            )],
            promoted_ask_ids: vec![],
            expected_revision: None,
        })
        .unwrap()
    else {
        panic!("expected snapshot");
    };

    assert!(snapshot.items.iter().any(|item| item.id == "u-chat"));
    let ask_items: Vec<_> = snapshot
        .items
        .iter()
        .filter(|item| item.ask_job_id.as_deref() == Some(job))
        .collect();
    assert_eq!(ask_items.len(), 2);
    assert!(ask_items
        .iter()
        .all(|item| item.lane == Some(ChatLane::Ask)));
    assert!(ask_items.iter().any(|item| {
        item.transcript
            .as_ref()
            .is_some_and(|t| t.session_id == format!("medousa-ask-{job}"))
    }));
}

#[test]
fn hydrate_skips_promoted_and_already_hydrated_jobs() {
    let mut core = ChatCore::new();
    open(&mut core);
    let job = "medousa-daemon-ask-2";
    core.dispatch(ChatCommand::HydrateAskThreads {
        session_id: "ses".into(),
        batches: vec![ask_batch(
            job,
            vec![entry(&"3".repeat(32), 1, "user", "once")],
        )],
        promoted_ask_ids: vec![],
        expected_revision: None,
    })
    .unwrap();

    let before = core.snapshot("ses").unwrap().items.len();
    let ChatUpdate::Snapshot { snapshot } = core
        .dispatch(ChatCommand::HydrateAskThreads {
            session_id: "ses".into(),
            batches: vec![
                ask_batch(job, vec![entry(&"4".repeat(32), 2, "user", "again")]),
                ask_batch(
                    "medousa-daemon-ask-promoted",
                    vec![entry(&"5".repeat(32), 1, "user", "promo")],
                ),
            ],
            promoted_ask_ids: vec!["medousa-daemon-ask-promoted".into()],
            expected_revision: None,
        })
        .unwrap()
    else {
        panic!("expected snapshot");
    };
    assert_eq!(snapshot.items.len(), before);
    assert!(!snapshot
        .items
        .iter()
        .any(|item| item.ask_job_id.as_deref() == Some("medousa-daemon-ask-promoted")));
}

#[test]
fn promote_flips_lane_and_clears_ask_job_id() {
    let mut core = ChatCore::new();
    open(&mut core);
    let job = "medousa-daemon-ask-3";
    core.dispatch(ChatCommand::HydrateAskThreads {
        session_id: "ses".into(),
        batches: vec![ask_batch(
            job,
            vec![
                entry(&"6".repeat(32), 1, "user", "q"),
                entry(&"7".repeat(32), 2, "assistant", "a"),
            ],
        )],
        promoted_ask_ids: vec![],
        expected_revision: None,
    })
    .unwrap();

    let ChatUpdate::Snapshot { snapshot } = core
        .dispatch(ChatCommand::PromoteAskToChat {
            session_id: "ses".into(),
            ask_job_id: job.into(),
            expected_revision: None,
        })
        .unwrap()
    else {
        panic!("expected snapshot");
    };

    assert!(snapshot
        .items
        .iter()
        .all(|item| item.ask_job_id.is_none()));
    assert!(snapshot
        .items
        .iter()
        .filter(|item| item.role == medousa_app_core::contract::ChatItemRole::User
            || item.role == medousa_app_core::contract::ChatItemRole::Assistant)
        .all(|item| item.lane == Some(ChatLane::Chat)));
}
