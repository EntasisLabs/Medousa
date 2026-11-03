//! Session switch, stale rejection, stream replay, and agent relationship tests.

use chrono::Utc;
use medousa_app_core::contract::{
    BeginTurnRequest, ChatCommand, ChatItemRole, ChatLane, ChatRelationship, ChatRevision,
    ChatUpdate,
};
use medousa_app_core::{guard_focused_session, ChatCore};
use medousa_types::{
    AuthorityId, SessionHistoryResponse, TranscriptEntry, TranscriptEntryId, TurnStreamEnvelopeV3,
    TurnStreamEventV3, TURN_STREAM_V3_SCHEMA_VERSION,
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

#[test]
fn replay_does_not_duplicate_stream_content() {
    let mut core = ChatCore::new();
    core.dispatch(ChatCommand::OpenSession {
        session_id: "ses_a".into(),
        authority_id: authority().as_str().into(),
    })
    .unwrap();
    core.dispatch(ChatCommand::BeginTurn {
        session_id: "ses_a".into(),
        request: BeginTurnRequest {
            turn_id: "turn-1".into(),
            user_item_id: "u1".into(),
            assistant_item_id: "a1".into(),
            user_text: "Hi".into(),
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

    let envelope = TurnStreamEnvelopeV3 {
        schema_version: TURN_STREAM_V3_SCHEMA_VERSION,
        turn_id: "turn-1".into(),
        seq: 1,
        emitted_at_utc: Utc::now(),
        event: TurnStreamEventV3::ContentAppend {
            segment_id: "text".into(),
            text: "Hello".into(),
        },
    };
    let ChatUpdate::Snapshot { snapshot } = core
        .dispatch(ChatCommand::ApplyStreamEvent {
            session_id: "ses_a".into(),
            envelope: envelope.clone(),
            expected_revision: None,
            promote_item_id: None,
        })
        .unwrap()
    else {
        panic!("expected snapshot");
    };
    assert_eq!(snapshot.items[1].content, "Hello");
    let revision = snapshot.revision;

    // Replay same seq — must not duplicate.
    let ChatUpdate::Snapshot { snapshot } = core
        .dispatch(ChatCommand::ApplyStreamEvent {
            session_id: "ses_a".into(),
            envelope,
            expected_revision: None,
            promote_item_id: None,
        })
        .unwrap()
    else {
        panic!("expected snapshot");
    };
    assert_eq!(snapshot.items[1].content, "Hello");
    assert_eq!(snapshot.revision, revision);
}

#[test]
fn recovery_preserves_active_turn_when_history_merges() {
    let mut core = ChatCore::new();
    let auth = authority();
    core.dispatch(ChatCommand::OpenSession {
        session_id: "ses_a".into(),
        authority_id: auth.as_str().into(),
    })
    .unwrap();
    core.dispatch(ChatCommand::BeginTurn {
        session_id: "ses_a".into(),
        request: BeginTurnRequest {
            turn_id: "active".into(),
            user_item_id: "u-live".into(),
            assistant_item_id: "a-live".into(),
            user_text: "Next".into(),
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

    let history = SessionHistoryResponse {
        authority_id: auth,
        session_id: "ses_a".into(),
        turns: vec![
            entry(&"b".repeat(32), 1, "user", "Earlier"),
            entry(&"c".repeat(32), 2, "assistant", "Previous answer"),
        ],
        next_cursor: None,
    };
    let ChatUpdate::Snapshot { snapshot } = core
        .dispatch(ChatCommand::ApplyHistory {
            session_id: "ses_a".into(),
            authority_id: history.authority_id.as_str().into(),
            turns: history
                .turns
                .iter()
                .map(|turn| serde_json::to_value(turn).unwrap())
                .collect(),
            lane: Some(ChatLane::Chat),
            ask_job_id: None,
            expected_revision: None,
            replace: false,
            local_items: None,
        })
        .unwrap()
    else {
        panic!("expected snapshot");
    };
    assert_eq!(snapshot.active_turn_id.as_deref(), Some("active"));
    let ids: Vec<_> = snapshot.items.iter().map(|item| item.id.as_str()).collect();
    assert!(ids.contains(&"a-live"));
    assert!(ids.iter().any(|id| id.contains("ent_")));
    let live = snapshot
        .items
        .iter()
        .find(|item| item.id == "a-live")
        .unwrap();
    assert_eq!(live.streaming, Some(true));
    assert_eq!(live.turn_id.as_deref(), Some("active"));
}

#[test]
fn session_switch_rejects_stale_revision() {
    let mut core = ChatCore::new();
    core.dispatch(ChatCommand::OpenSession {
        session_id: "ses_a".into(),
        authority_id: authority().as_str().into(),
    })
    .unwrap();
    let ChatUpdate::Snapshot { snapshot } = core
        .dispatch(ChatCommand::BeginTurn {
            session_id: "ses_a".into(),
            request: BeginTurnRequest {
                turn_id: "t1".into(),
                user_item_id: "u1".into(),
                assistant_item_id: "a1".into(),
                user_text: "Hi".into(),
                created_at: None,
                lane: None,
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
        .unwrap()
    else {
        panic!("expected snapshot");
    };
    let stale = ChatRevision(0);
    let err = core
        .dispatch(ChatCommand::ApplyStreamEvent {
            session_id: "ses_a".into(),
            envelope: TurnStreamEnvelopeV3 {
                schema_version: TURN_STREAM_V3_SCHEMA_VERSION,
                turn_id: "t1".into(),
                seq: 1,
                emitted_at_utc: Utc::now(),
                event: TurnStreamEventV3::ContentAppend {
                    segment_id: "text".into(),
                    text: "nope".into(),
                },
            },
            expected_revision: Some(stale),
            promote_item_id: None,
        })
        .unwrap_err();
    assert!(matches!(err, medousa_app_core::ChatCoreError::StaleRevision { .. }));
    assert_eq!(snapshot.revision, ChatRevision(1));
}

#[test]
fn focused_session_guard_rejects_cross_session_updates() {
    assert!(guard_focused_session(Some("ses_a"), "ses_a").is_ok());
    assert!(guard_focused_session(Some("ses_a"), "ses_b").is_err());
}

#[test]
fn tool_and_agent_relationships_retain_stable_placement() {
    let mut core = ChatCore::new();
    core.dispatch(ChatCommand::OpenSession {
        session_id: "ses_a".into(),
        authority_id: authority().as_str().into(),
    })
    .unwrap();
    core.dispatch(ChatCommand::BeginTurn {
        session_id: "ses_a".into(),
        request: BeginTurnRequest {
            turn_id: "parent-turn".into(),
            user_item_id: "u1".into(),
            assistant_item_id: "a1".into(),
            user_text: "delegate".into(),
            created_at: None,
            lane: None,
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
    core.dispatch(ChatCommand::ApplyStreamEvent {
        session_id: "ses_a".into(),
        envelope: TurnStreamEnvelopeV3 {
            schema_version: TURN_STREAM_V3_SCHEMA_VERSION,
            turn_id: "parent-turn".into(),
            seq: 1,
            emitted_at_utc: Utc::now(),
            event: TurnStreamEventV3::ToolStarted {
                tool_run_id: "run-1".into(),
                tool_name: "workshop_spawn".into(),
                input_summary: "spawn".into(),
                input_params: Vec::new(),
                tool_round: 1,
            },
        },
        expected_revision: None,
        promote_item_id: None,
    })
    .unwrap();

    let ChatUpdate::Snapshot { snapshot } = core
        .dispatch(ChatCommand::LinkWorker {
            session_id: "ses_a".into(),
            work_id: "work-1".into(),
            parent_turn_id: Some("parent-turn".into()),
            parent_item_id: Some("a1".into()),
            reply_to_work_id: None,
            worker_session_id: None,
        expected_revision: None,
        })
        .unwrap()
    else {
        panic!("expected snapshot");
    };

    assert!(snapshot.relationships.iter().any(|rel| matches!(
        rel,
        ChatRelationship::ToolBelongsTo {
            tool_run_id,
            turn_id,
            item_id,
        } if tool_run_id == "run-1" && turn_id == "parent-turn" && item_id == "a1"
    )));
    assert!(snapshot.relationships.iter().any(|rel| matches!(
        rel,
        ChatRelationship::AgentSpawn {
            work_id,
            parent_turn_id: Some(turn),
            parent_item_id: Some(item),
        } if work_id == "work-1" && turn == "parent-turn" && item == "a1"
    )));

    // Follow-up preserves reply relationship rather than appearing as a new spawn.
    let ChatUpdate::Snapshot { snapshot } = core
        .dispatch(ChatCommand::LinkWorker {
            session_id: "ses_a".into(),
            work_id: "work-1".into(),
            parent_turn_id: Some("other-turn".into()),
            parent_item_id: Some("wrong".into()),
            reply_to_work_id: Some("work-0".into()),
            worker_session_id: None,
        expected_revision: None,
        })
        .unwrap()
    else {
        panic!("expected snapshot");
    };
    assert!(snapshot.relationships.iter().any(|rel| matches!(
        rel,
        ChatRelationship::AgentFollowUp {
            work_id,
            reply_to_work_id,
            parent_turn_id: Some(turn),
            ..
        } if work_id == "work-1"
            && reply_to_work_id == "work-0"
            && turn == "parent-turn" // spawn parent immutable
    )));
}

#[test]
fn dispose_cleans_up_session_subscriptions() {
    let mut core = ChatCore::new();
    core.dispatch(ChatCommand::OpenSession {
        session_id: "ses_a".into(),
        authority_id: authority().as_str().into(),
    })
    .unwrap();
    assert!(matches!(
        core.dispatch(ChatCommand::DisposeSession {
            session_id: "ses_a".into(),
        })
        .unwrap(),
        ChatUpdate::Disposed { .. }
    ));
    assert!(core.snapshot("ses_a").is_err());
}

#[test]
fn begin_turn_user_appears_once() {
    let mut core = ChatCore::new();
    core.dispatch(ChatCommand::OpenSession {
        session_id: "ses_a".into(),
        authority_id: authority().as_str().into(),
    })
    .unwrap();
    let ChatUpdate::Snapshot { snapshot } = core
        .dispatch(ChatCommand::BeginTurn {
            session_id: "ses_a".into(),
            request: BeginTurnRequest {
                turn_id: "t1".into(),
                user_item_id: "u1".into(),
                assistant_item_id: "a1".into(),
                user_text: "Hello".into(),
                created_at: None,
                lane: None,
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
        .unwrap()
    else {
        panic!("expected snapshot");
    };
    let users: Vec<_> = snapshot
        .items
        .iter()
        .filter(|item| item.role == ChatItemRole::User)
        .collect();
    assert_eq!(users.len(), 1);
    assert_eq!(users[0].content, "Hello");
}
