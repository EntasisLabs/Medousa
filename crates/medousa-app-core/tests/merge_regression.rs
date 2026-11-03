//! Regression coverage for live ↔ persisted reconciliation.

use medousa_app_core::contract::{
    ChatItem, ChatItemRole, ChatSegment, TranscriptCoordinate,
};
use medousa_app_core::{apply_v3_envelope_to_item, merge_transcript};
use medousa_types::{TurnStreamEnvelopeV3, TurnStreamEventV3, TURN_STREAM_V3_SCHEMA_VERSION};
use chrono::Utc;

fn saved(id: &str, seq: u64, role: ChatItemRole, content: &str) -> ChatItem {
    ChatItem {
        id: id.into(),
        role,
        content: content.into(),
        transcript: Some(TranscriptCoordinate {
            authority_id: "authority".into(),
            session_id: "chat".into(),
            entry_id: id.into(),
            entry_seq: seq,
            source: None,
        }),
        ..empty()
    }
}

fn empty() -> ChatItem {
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

#[test]
fn inserts_recovered_earlier_answer_before_active_shell() {
    let first = saved("first", 1, ChatItemRole::User, "First");
    let next = saved("next", 3, ChatItemRole::User, "Next");
    let active = ChatItem {
        id: "live".into(),
        role: ChatItemRole::Assistant,
        content: "Working".into(),
        streaming: Some(true),
        turn_id: Some("active".into()),
        ..empty()
    };
    let result = merge_transcript(
        vec![first.clone(), next.clone(), active.clone()],
        vec![
            first,
            saved("answer", 2, ChatItemRole::Assistant, "Done"),
            next,
        ],
    );
    assert_eq!(
        result.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
        ["first", "answer", "next", "live"]
    );
    assert_eq!(result[3].id, active.id);
}

#[test]
fn repairs_appended_durable_entries_without_separating_active_prompt() {
    let first = saved("first", 1, ChatItemRole::User, "First");
    let next = saved("next", 3, ChatItemRole::User, "Next");
    let answer = saved("answer", 2, ChatItemRole::Assistant, "Done");
    let active = ChatItem {
        id: "live".into(),
        role: ChatItemRole::Assistant,
        content: "Working".into(),
        streaming: Some(true),
        ..empty()
    };
    let result = merge_transcript(
        vec![first.clone(), next.clone(), active, answer.clone()],
        vec![first, answer, next],
    );
    assert_eq!(
        result.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
        ["first", "answer", "next", "live"]
    );
}

#[test]
fn places_recovered_history_before_coordinate_less_active_prompt() {
    let prompt = ChatItem {
        id: "prompt".into(),
        role: ChatItemRole::User,
        content: "Next".into(),
        ..empty()
    };
    let active = ChatItem {
        id: "live".into(),
        role: ChatItemRole::Assistant,
        content: String::new(),
        streaming: Some(true),
        ..empty()
    };
    let recovered = saved("older", 2, ChatItemRole::Assistant, "Previous answer");
    let result = merge_transcript(vec![prompt, active], vec![recovered]);
    assert_eq!(
        result.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
        ["older", "prompt", "live"]
    );
}

#[test]
fn user_message_appears_exactly_once_for_optimistic_match() {
    let local = vec![ChatItem {
        id: "optimistic".into(),
        role: ChatItemRole::User,
        content: "continue".into(),
        ..empty()
    }];
    let history = vec![
        saved("first", 1, ChatItemRole::User, "continue"),
        saved("second", 3, ChatItemRole::User, "continue"),
    ];
    let result = merge_transcript(local, history.clone());
    assert_eq!(
        result.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
        ["optimistic", "second"]
    );
    assert_eq!(merge_transcript(result.clone(), history), result);
}

#[test]
fn preserves_distinct_durable_prompts_with_identical_text() {
    let first = saved("first", 1, ChatItemRole::User, "continue");
    let second = saved("second", 3, ChatItemRole::User, "continue");
    let result = merge_transcript(vec![first.clone()], vec![first, second]);
    assert_eq!(
        result.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
        ["first", "second"]
    );
}

#[test]
fn never_replaces_durable_assistant_with_equal_text_from_another_turn() {
    let first = saved("first", 2, ChatItemRole::Assistant, "Done");
    let second = saved("second", 4, ChatItemRole::Assistant, "Done");
    let result = merge_transcript(vec![first.clone()], vec![second.clone()]);
    assert_eq!(result, vec![first, second]);
}

#[test]
fn live_and_persisted_content_converge_without_double_append() {
    let local = ChatItem {
        id: "live".into(),
        role: ChatItemRole::Assistant,
        turn_id: Some("turn".into()),
        streaming: Some(true),
        content: "Hello ".into(),
        reasoning: Some("Think ".into()),
        segments: Some(vec![ChatSegment::Text {
            segment_id: "text".into(),
            model_round: Some(1),
            markdown: "Hello ".into(),
            committed: false,
        }]),
        ..empty()
    };
    let history = ChatItem {
        id: "saved".into(),
        role: ChatItemRole::Assistant,
        turn_id: Some("turn".into()),
        streaming: Some(false),
        content: "Hello world".into(),
        reasoning: Some("Think carefully".into()),
        segments: Some(vec![ChatSegment::Text {
            segment_id: "text".into(),
            model_round: Some(1),
            markdown: "Hello world".into(),
            committed: true,
        }]),
        ..empty()
    };
    let mut merged = merge_transcript(vec![local], vec![history.clone()]);
    assert_eq!(merged.len(), 1);
    let envelope_world = TurnStreamEnvelopeV3 {
        schema_version: TURN_STREAM_V3_SCHEMA_VERSION,
        turn_id: "turn".into(),
        seq: 2,
        emitted_at_utc: Utc::now(),
        event: TurnStreamEventV3::ContentAppend {
            segment_id: "text".into(),
            text: "world".into(),
        },
    };
    let envelope_reason = TurnStreamEnvelopeV3 {
        schema_version: TURN_STREAM_V3_SCHEMA_VERSION,
        turn_id: "turn".into(),
        seq: 3,
        emitted_at_utc: Utc::now(),
        event: TurnStreamEventV3::ReasoningAppend {
            text: "carefully".into(),
        },
    };
    merged[0] = apply_v3_envelope_to_item(merged[0].clone(), &envelope_world);
    merged[0] = apply_v3_envelope_to_item(merged[0].clone(), &envelope_reason);
    assert_eq!(merged[0].content, "Hello world");
    assert_eq!(merged[0].reasoning.as_deref(), Some("Think carefully"));
    let settled = merge_transcript(
        vec![ChatItem {
            streaming: Some(false),
            ..merged[0].clone()
        }],
        vec![history],
    );
    assert!(matches!(
        settled[0].segments.as_ref().unwrap()[0],
        ChatSegment::Text {
            committed: true,
            ..
        }
    ));
}

#[test]
fn hydrates_attachments_by_stable_turn_identity() {
    use medousa_app_core::contract::LiveTranscriptAttachment;
    use medousa_app_core::contract::LiveTranscriptRow;

    let local = vec![ChatItem {
        id: "stream".into(),
        role: ChatItemRole::Assistant,
        turn_id: Some("turn".into()),
        content: "Answer".into(),
        streaming: Some(true),
        ..empty()
    }];
    let live_transcripts = vec![LiveTranscriptAttachment {
        id: "attachment".into(),
        rows: vec![LiveTranscriptRow {
            role: ChatItemRole::User,
            text: "Hi".into(),
        }],
    }];
    let result = merge_transcript(
        local,
        vec![ChatItem {
            id: "saved".into(),
            role: ChatItemRole::Assistant,
            turn_id: Some("turn".into()),
            content: "Answer".into(),
            live_transcripts: Some(live_transcripts.clone()),
            ..empty()
        }],
    );
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].id, "stream");
    assert_eq!(result[0].streaming, Some(true));
    assert_eq!(result[0].live_transcripts, Some(live_transcripts));
}
