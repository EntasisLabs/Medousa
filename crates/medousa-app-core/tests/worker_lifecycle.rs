//! Worker-lane follow-up bubble lifecycle and BeginTurn seed fields.

use medousa_app_core::contract::{
    BeginTurnRequest, ChatCommand, ChatItemRole, ChatLane, ChatUpdate, WorkerLinkFact,
};
use medousa_app_core::ChatCore;
use medousa_types::{
    TurnStreamEnvelopeV3, TurnStreamEventV3, TURN_STREAM_V3_SCHEMA_VERSION,
};
use chrono::Utc;

fn open(core: &mut ChatCore) {
    core.dispatch(ChatCommand::OpenSession {
        session_id: "ses".into(),
        authority_id: "auth".into(),
    })
    .unwrap();
}

fn begin(core: &mut ChatCore) {
    core.dispatch(ChatCommand::BeginTurn {
        session_id: "ses".into(),
        request: BeginTurnRequest {
            turn_id: "parent".into(),
            user_item_id: "u".into(),
            assistant_item_id: "a".into(),
            user_text: "delegate".into(),
            created_at: Some("2026-11-03T12:00:00Z".into()),
            lane: Some(ChatLane::Chat),
            ask_job_id: None,
            work_id: None,
            status_line: Some("Background turn started".into()),
            speaker_profile_id: Some("profile".into()),
            media_attachments: None,
        },
        expected_revision: None,
    })
    .unwrap();
}

#[test]
fn begin_turn_seeds_status_and_speaker() {
    let mut core = ChatCore::new();
    open(&mut core);
    begin(&mut core);
    let snapshot = core.snapshot("ses").unwrap();
    assert_eq!(snapshot.items[0].speaker_profile_id.as_deref(), Some("profile"));
    assert_eq!(
        snapshot.items[1].status_line.as_deref(),
        Some("Background turn started")
    );
    assert_eq!(snapshot.active_turn_id.as_deref(), Some("parent"));
}

#[test]
fn ensure_follow_up_is_idempotent_and_stable() {
    let mut core = ChatCore::new();
    open(&mut core);
    begin(&mut core);
    core.dispatch(ChatCommand::LinkWorker {
        session_id: "ses".into(),
        work_id: "work".into(),
        parent_turn_id: Some("parent".into()),
        parent_item_id: Some("a".into()),
        reply_to_work_id: None,
        expected_revision: None,
    })
    .unwrap();

    let ChatUpdate::Snapshot { snapshot } = core
        .dispatch(ChatCommand::EnsureWorkerFollowUp {
            session_id: "ses".into(),
            work_id: "work".into(),
            turn_id: Some("parent".into()),
            item_id: None,
            status_line: Some("Working…".into()),
            streaming: Some(true),
            expected_revision: None,
        })
        .unwrap()
    else {
        panic!("expected snapshot");
    };
    let follow = snapshot
        .items
        .iter()
        .find(|item| item.lane == Some(ChatLane::Worker))
        .expect("follow-up");
    assert_eq!(follow.id, "worker-synth:work");
    assert_eq!(follow.status_line.as_deref(), Some("Working…"));
    assert_eq!(
        snapshot.workers,
        vec![WorkerLinkFact {
            work_id: "work".into(),
            parent_turn_id: Some("parent".into()),
            parent_item_id: Some("a".into()),
            reply_to_work_id: None,
            synthesis_item_id: Some("worker-synth:work".into()),
            synthesis_delivered: false,
        }]
    );

    let before = snapshot.items.len();
    let ChatUpdate::Snapshot { snapshot } = core
        .dispatch(ChatCommand::EnsureWorkerFollowUp {
            session_id: "ses".into(),
            work_id: "work".into(),
            turn_id: Some("parent".into()),
            item_id: None,
            status_line: Some("Still working".into()),
            streaming: Some(true),
            expected_revision: None,
        })
        .unwrap()
    else {
        panic!("expected snapshot");
    };
    assert_eq!(snapshot.items.len(), before);
    let follow = snapshot
        .items
        .iter()
        .find(|item| item.id == "worker-synth:work")
        .unwrap();
    assert_eq!(follow.status_line.as_deref(), Some("Still working"));
}

#[test]
fn deliver_synthesis_finalizes_handoff_and_unlocks_settle() {
    let mut core = ChatCore::new();
    open(&mut core);
    begin(&mut core);
    core.dispatch(ChatCommand::LinkWorker {
        session_id: "ses".into(),
        work_id: "work".into(),
        parent_turn_id: Some("parent".into()),
        parent_item_id: Some("a".into()),
        reply_to_work_id: None,
        expected_revision: None,
    })
    .unwrap();
    core.dispatch(ChatCommand::EnsureWorkerFollowUp {
        session_id: "ses".into(),
        work_id: "work".into(),
        turn_id: Some("parent".into()),
        item_id: None,
        status_line: Some("Working…".into()),
        streaming: Some(true),
        expected_revision: None,
    })
    .unwrap();

    let ChatUpdate::Snapshot { snapshot } = core
        .dispatch(ChatCommand::DeliverWorkerSynthesis {
            session_id: "ses".into(),
            work_id: "work".into(),
            content: "Done".into(),
            tools: Some(vec!["read".into()]),
            expected_revision: None,
        })
        .unwrap()
    else {
        panic!("expected snapshot");
    };
    let follow = snapshot
        .items
        .iter()
        .find(|item| item.id == "worker-synth:work")
        .unwrap();
    assert_eq!(follow.content, "Done");
    assert_eq!(follow.streaming, Some(false));
    assert_eq!(follow.tools.as_deref(), Some(["read".to_string()].as_slice()));
    let handoff = snapshot.items.iter().find(|item| item.id == "a").unwrap();
    assert_eq!(handoff.streaming, Some(false));
    assert!(snapshot.workers[0].synthesis_delivered);

    // Idempotent — second deliver does not rewrite content after delivered.
    let ChatUpdate::Snapshot { snapshot } = core
        .dispatch(ChatCommand::DeliverWorkerSynthesis {
            session_id: "ses".into(),
            work_id: "work".into(),
            content: "Other".into(),
            tools: None,
            expected_revision: None,
        })
        .unwrap()
    else {
        panic!("expected snapshot");
    };
    assert_eq!(
        snapshot
            .items
            .iter()
            .find(|item| item.id == "worker-synth:work")
            .unwrap()
            .content,
        "Done"
    );
}

#[test]
fn worker_synthesis_stream_targets_follow_up_not_handoff() {
    let mut core = ChatCore::new();
    open(&mut core);
    begin(&mut core);
    core.dispatch(ChatCommand::LinkWorker {
        session_id: "ses".into(),
        work_id: "work".into(),
        parent_turn_id: Some("parent".into()),
        parent_item_id: Some("a".into()),
        reply_to_work_id: None,
        expected_revision: None,
    })
    .unwrap();
    core.dispatch(ChatCommand::EnsureWorkerFollowUp {
        session_id: "ses".into(),
        work_id: "work".into(),
        turn_id: Some("parent".into()),
        item_id: None,
        status_line: None,
        streaming: Some(true),
        expected_revision: None,
    })
    .unwrap();

    let ChatUpdate::Snapshot { snapshot } = core
        .dispatch(ChatCommand::ApplyStreamEvent {
            session_id: "ses".into(),
            envelope: TurnStreamEnvelopeV3 {
                schema_version: TURN_STREAM_V3_SCHEMA_VERSION,
                turn_id: "parent".into(),
                seq: 1,
                emitted_at_utc: Utc::now(),
                event: TurnStreamEventV3::WorkerSynthesis {
                    text: "Result body".into(),
                    tool_names: vec![],
                    work_id: Some("work".into()),
                },
            },
            expected_revision: None,
            promote_item_id: None,
        })
        .unwrap()
    else {
        panic!("expected snapshot");
    };
    assert_eq!(
        snapshot
            .items
            .iter()
            .find(|item| item.id == "worker-synth:work")
            .unwrap()
            .content,
        "Result body"
    );
    assert_eq!(
        snapshot.items.iter().find(|item| item.id == "a").unwrap().content,
        ""
    );
}

#[test]
fn settle_turn_clears_all_assistants_for_turn() {
    let mut core = ChatCore::new();
    open(&mut core);
    begin(&mut core);
    core.dispatch(ChatCommand::LinkWorker {
        session_id: "ses".into(),
        work_id: "work".into(),
        parent_turn_id: Some("parent".into()),
        parent_item_id: Some("a".into()),
        reply_to_work_id: None,
        expected_revision: None,
    })
    .unwrap();
    core.dispatch(ChatCommand::EnsureWorkerFollowUp {
        session_id: "ses".into(),
        work_id: "work".into(),
        turn_id: Some("parent".into()),
        item_id: None,
        status_line: None,
        streaming: Some(true),
        expected_revision: None,
    })
    .unwrap();
    let ChatUpdate::Snapshot { snapshot } = core
        .dispatch(ChatCommand::SettleTurn {
            session_id: "ses".into(),
            turn_id: "parent".into(),
            expected_revision: None,
            reusable_stream: false,
        })
        .unwrap()
    else {
        panic!("expected snapshot");
    };
    assert!(snapshot.active_turn_id.is_none());
    for item in snapshot.items.iter().filter(|item| item.role == ChatItemRole::Assistant) {
        assert_eq!(item.streaming, Some(false));
    }
}

#[test]
fn worker_ack_and_budget_paint_handoff_chrome() {
    let mut core = ChatCore::new();
    open(&mut core);
    begin(&mut core);

    let ChatUpdate::Snapshot { snapshot } = core
        .dispatch(ChatCommand::ApplyStreamEvent {
            session_id: "ses".into(),
            envelope: TurnStreamEnvelopeV3 {
                schema_version: TURN_STREAM_V3_SCHEMA_VERSION,
                turn_id: "parent".into(),
                seq: 1,
                emitted_at_utc: Utc::now(),
                event: TurnStreamEventV3::WorkerAck {
                    ack_kind: medousa_types::WorkerAckKind::Worker,
                    text: "Background worker started".into(),
                    tool_names: vec![],
                    work_id: Some("work".into()),
                },
            },
            expected_revision: None,
            promote_item_id: None,
        })
        .unwrap()
    else {
        panic!("expected snapshot");
    };
    let handoff = snapshot.items.iter().find(|item| item.id == "a").unwrap();
    assert_eq!(handoff.streaming, Some(false));
    assert_eq!(handoff.content, "");
    assert_eq!(
        handoff.stage_whisper.as_deref(),
        Some("Background worker started")
    );
    assert_eq!(handoff.work_id.as_deref(), Some("work"));
    assert!(handoff.segments.as_ref().is_some_and(|segments| {
        segments.iter().any(|segment| {
            matches!(
                segment,
                medousa_app_core::contract::ChatSegment::Handoff { .. }
            )
        })
    }));

    let ChatUpdate::Snapshot { snapshot } = core
        .dispatch(ChatCommand::ApplyStreamEvent {
            session_id: "ses".into(),
            envelope: TurnStreamEnvelopeV3 {
                schema_version: TURN_STREAM_V3_SCHEMA_VERSION,
                turn_id: "parent".into(),
                seq: 2,
                emitted_at_utc: Utc::now(),
                event: TurnStreamEventV3::BudgetApprovalRequired {
                    request_id: "budget-1".into(),
                    rounds_executed: 3,
                    max_tool_rounds: 3,
                    requested_rounds: 5,
                    reason: "Need more rounds".into(),
                    progress_summary: None,
                },
            },
            expected_revision: None,
            promote_item_id: None,
        })
        .unwrap()
    else {
        panic!("expected snapshot");
    };
    let blocked = snapshot.items.iter().find(|item| item.id == "a").unwrap();
    assert_eq!(blocked.phase.as_deref(), Some("budget_blocked"));
    assert_eq!(blocked.budget_request_id.as_deref(), Some("budget-1"));
    assert_eq!(blocked.requested_rounds, Some(5));
    assert_eq!(blocked.status_line.as_deref(), Some("Need more rounds"));
}
