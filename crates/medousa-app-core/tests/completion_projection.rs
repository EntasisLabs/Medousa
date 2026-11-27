//! Completion envelopes are durable machine facts, not user-facing JSON prose.
use medousa_app_core::contract::{ChatItem, ChatLane, ChatSegment};
use medousa_app_core::history::{ProjectHistoryOptions, project_history};
use medousa_app_core::wire_history::{WireTranscriptEntry, project_wire_history};
use medousa_types::SessionHistoryResponse;
use serde_json::{Value, json};

fn entry(content: &str, role: &str, state: Option<&str>, segmented: bool) -> Value {
    json!({
        "entry_id": "ent_0123456789abcdef0123456789abcdef", "entry_seq": 7,
        "content_digest": "unaltered-digest", "role": role, "content": content,
        "timestamp": "2026-11-27T13:00:00Z", "answer_state": state, "tool_names": [],
        "parts": if segmented { json!([{"kind":"text", "markdown":content, "segment_id":"reply", "model_round":0}]) } else { Value::Null }
    })
}
fn wire(value: Value) -> ChatItem {
    let turn: WireTranscriptEntry = serde_json::from_value(value).unwrap();
    project_wire_history(
        "ses_0123456789abcdef",
        "auth",
        &[turn],
        Some(ChatLane::Chat),
        None,
    )
    .remove(0)
}
fn peer(reply: Value, outcome: &str) -> String {
    json!({"request_id":"peer_assignment_123", "outcome":outcome, "result":reply,
        "worker":{"authority_id":"auth", "session_id":"ses_worker"}})
    .to_string()
}
#[test]
fn both_envelopes_and_terminal_outcomes_project_reply_only() {
    for state in ["completed", "failed", "cancelled", "interrupted"] {
        let reply = "**Done** 🙌\n\n- Kept `code` and [links](https://example.org).";
        for payload in [
            peer(json!(reply), state),
            json!({"request_id":"work-123", "status":state, "text":reply}).to_string(),
        ] {
            let item = wire(entry(
                &payload,
                "assistant",
                Some("worker_synthesis"),
                false,
            ));
            assert_eq!(item.content, reply);
            assert_eq!(item.transcript.unwrap().entry_seq, 7);
        }
    }
}
#[test]
fn ordinary_json_user_input_and_noncompletion_assistant_output_are_untouched() {
    let payload = peer(json!("reply"), "completed");
    for (role, state) in [
        ("user", Some("worker_synthesis")),
        ("assistant", None),
        ("assistant", Some("final")),
    ] {
        assert_eq!(wire(entry(&payload, role, state, false)).content, payload);
    }
}
#[test]
fn invalid_unknown_and_nontext_envelopes_are_losslessly_preserved() {
    for payload in [
        "plain **reply**".to_owned(),
        "{invalid".to_owned(),
        "{}".to_owned(),
        peer(json!({"code":42}), "completed"),
        peer(json!("pending"), "running"),
        json!({"text":"not an envelope"}).to_string(),
    ] {
        assert_eq!(
            wire(entry(
                &payload,
                "assistant",
                Some("worker_synthesis"),
                false
            ))
            .content,
            payload
        );
    }
}
#[test]
fn empty_reply_is_not_replaced_by_json_and_nested_json_reply_is_not_recursively_decoded() {
    for reply in [
        String::new(),
        peer(json!("keep this nested JSON"), "completed"),
    ] {
        assert_eq!(
            wire(entry(
                &peer(json!(reply), "completed"),
                "assistant",
                Some("worker_synthesis"),
                false
            ))
            .content,
            reply
        );
    }
}
#[test]
fn segmented_completion_matches_compatibility_text() {
    let payload = peer(json!("# Result\n\nReal text."), "completed");
    let item = wire(entry(&payload, "assistant", Some("worker_synthesis"), true));
    assert_eq!(item.content, "# Result\n\nReal text.");
    let ChatSegment::Text { markdown, .. } = &item.segments.unwrap()[0] else {
        panic!("missing text")
    };
    assert_eq!(markdown, "# Result\n\nReal text.");
}
#[test]
fn typed_history_and_wire_history_agree_without_mutating_source() {
    let payload = peer(json!("Verified reply."), "completed");
    let value = entry(&payload, "assistant", Some("worker_synthesis"), false);
    let history: SessionHistoryResponse = serde_json::from_value(json!({
        "session_id":"ses_0123456789abcdef", "authority_id":format!("auth_{}", "a".repeat(64)),
        "turns":[value.clone()]
    }))
    .unwrap();
    let typed = project_history(&history, ProjectHistoryOptions::default());
    assert_eq!(typed[0].content, wire(value).content);
    assert_eq!(history.turns[0].turn.content, payload);
}
