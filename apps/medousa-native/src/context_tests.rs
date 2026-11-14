use super::*;
fn world() -> World {
    let mut w = World::new();
    w.insert_resource(Context::default());
    w
}
fn board(revision: u64) -> Value {
    json!({"revision": revision,"notes":[]})
}
fn note() -> Value {
    json!({"id":"real-note-id","text":"Original","kind":"detail","pinned":true,"exact":true,"source_refs":["turn:2"],"status":"active"})
}
fn loaded(w: &mut World, session: &str, revision: u64) {
    apply(
        w,
        ContextUpdate {
            command: refresh(session),
            result: Ok(board(revision)),
        },
    );
}
fn click(w: &mut World, a: Action) {
    let c = w.resource_mut::<Context>().unwrap();
    c.visible = Some("s".into());
    let id = next();
    c.actions.insert(id, ("s".into(), a));
    assert!(action(w, &HostIntent::Command(id)));
}
#[test]
fn exact_contract_revision_and_identity_preserved() {
    let mut w = world();
    loaded(&mut w, "s", 3);
    click(&mut w, Action::Edit(note()));
    w.resource_mut::<Context>()
        .unwrap()
        .sessions
        .get_mut("s")
        .unwrap()
        .draft = "Corrected exact detail".into();
    click(&mut w, Action::Save);
    let q = drain_commands(&mut w);
    assert_eq!(q.len(), 1);
    let body = q[0].body.as_ref().unwrap();
    assert_eq!(body["action"], "sticky.put");
    assert_eq!(body["expected_revision"], 3);
    assert_eq!(body["note"]["id"], "real-note-id");
    assert_eq!(body["note"]["text"], "Corrected exact detail");
    assert_eq!(body["note"]["source_refs"], json!(["turn:2"]));
    assert_eq!(body["note"]["exact"], true);
    assert!(body["request_key"].as_str().is_some());
}
#[test]
fn errors_are_scoped_and_retry_uses_identical_body() {
    let mut w = world();
    loaded(&mut w, "s", 2);
    click(&mut w, Action::Resolve("real-id".into(), false));
    let command = drain_commands(&mut w).remove(0);
    let body = command.body.clone();
    apply(
        &mut w,
        ContextUpdate {
            command,
            result: Err("lost transport".into()),
        },
    );
    let c = w.resource::<Context>().unwrap();
    assert!(!c.sessions["s"].busy);
    assert!(c.sessions["s"].error.contains("not confirmed"));
    assert!(!c.sessions.contains_key("other"));
    click(&mut w, Action::Retry);
    assert_eq!(drain_commands(&mut w)[0].body, body);
}
#[test]
fn unknown_outcome_blocks_new_mutation() {
    let mut w = world();
    loaded(&mut w, "s", 2);
    click(&mut w, Action::Pin("id".into(), false));
    let command = drain_commands(&mut w).remove(0);
    apply(
        &mut w,
        ContextUpdate {
            command,
            result: Err("403".into()),
        },
    );
    click(&mut w, Action::Resolve("id".into(), false));
    assert!(drain_commands(&mut w).is_empty());
    assert_eq!(
        w.resource::<Context>().unwrap().sessions["s"].board,
        Some(board(2))
    );
}
#[test]
fn stale_refresh_never_resurrects_pre_mutation_board() {
    let mut w = world();
    loaded(&mut w, "s", 2);
    let old = refresh("s");
    click(&mut w, Action::Resolve("id".into(), false));
    let command = drain_commands(&mut w).remove(0);
    apply(
        &mut w,
        ContextUpdate {
            command,
            result: Ok(board(3)),
        },
    );
    apply(
        &mut w,
        ContextUpdate {
            command: old,
            result: Ok(board(2)),
        },
    );
    assert_eq!(
        board_revision(&w.resource::<Context>().unwrap().sessions["s"]),
        Some(3)
    );
}
#[test]
fn edit_keeps_original_revision_across_reload() {
    let mut w = world();
    loaded(&mut w, "s", 2);
    click(&mut w, Action::Edit(note()));
    loaded(&mut w, "s", 3);
    click(&mut w, Action::Save);
    let q = drain_commands(&mut w);
    assert_eq!(q[0].body.as_ref().unwrap()["expected_revision"], 2);
}
#[test]
fn stale_error_does_not_settle_newer_pending_request() {
    let mut w = world();
    loaded(&mut w, "s", 2);
    let old = refresh("s");
    click(&mut w, Action::Pin("id".into(), false));
    drain_commands(&mut w);
    apply(
        &mut w,
        ContextUpdate {
            command: old,
            result: Err("old".into()),
        },
    );
    assert!(w.resource::<Context>().unwrap().sessions["s"].busy);
}
#[test]
fn source_session_update_does_not_touch_visible_chat() {
    let mut w = world();
    loaded(&mut w, "s", 2);
    w.resource_mut::<Context>().unwrap().visible = Some("s".into());
    loaded(&mut w, "old-chat", 9);
    assert_eq!(
        board_revision(&w.resource::<Context>().unwrap().sessions["s"]),
        Some(2)
    );
}
#[test]
fn removal_requires_explicit_confirmation() {
    let mut w = world();
    loaded(&mut w, "s", 2);
    click(&mut w, Action::ConfirmRemove("id".into()));
    assert!(drain_commands(&mut w).is_empty());
    click(&mut w, Action::Remove("id".into()));
    assert!(drain_commands(&mut w).is_empty());
    click(&mut w, Action::ConfirmRemove("id".into()));
    assert_eq!(
        drain_commands(&mut w)[0].body.as_ref().unwrap()["action"],
        "sticky.remove"
    );
}
#[test]
fn bad_wire_data_is_not_success() {
    for value in [
        json!({}),
        json!({"revision":-1,"notes":[]}),
        json!({"revision":1,"notes":[{"id":"n"}]}),
    ] {
        assert!(validate_board(&value).is_err());
    }
    assert!(validate_board(&board(0)).is_ok());
}
#[test]
fn request_keys_are_not_reused_for_new_mutations() {
    assert_ne!(request_key(), request_key());
}
#[test]
fn no_selected_session_actions_and_no_synthetic_answer() {
    let mut w = world();
    assert!(!action(&mut w, &HostIntent::Command(1)));
    assert!(drain_commands(&mut w).is_empty());
}
fn view() -> (World, Entity) {
    let mut w = world();
    let parent = w.spawn((Layout::column(),));
    let root = setup(&mut w, parent);
    loaded(&mut w, "s", 1);
    w.resource_mut::<Context>().unwrap().expanded = true;
    sync(&mut w, root, Some("s"));
    (w, root)
}
#[test]
fn editor_survives_unrelated_frames_usage_updates_and_retains_exact_draft() {
    let (mut w, root) = view();
    click(&mut w, Action::Edit(note()));
    sync(&mut w, root, Some("s"));
    let input = w.resource::<Context>().unwrap().input.unwrap();
    w.get_mut::<Label>(input).unwrap().text = "Unsaved\nexact draft".into();
    let report = ContextUsageReport {
        hud: None,
        layers: vec![],
        total_tokens_estimate: 12,
        total_chars: 48,
        context_limit_tokens: None,
        tool_count: 1,
        estimator: "chars / 4".into(),
    };
    usage(&mut w, "s", report);
    for _ in 0..5 {
        sync(&mut w, root, Some("s"));
    }
    assert_eq!(w.resource::<Context>().unwrap().input, Some(input));
    assert_eq!(w.get::<Label>(input).unwrap().text, "Unsaved\nexact draft");
    click(&mut w, Action::TogglePinned);
    sync(&mut w, root, Some("s"));
    let next_input = w.resource::<Context>().unwrap().input.unwrap();
    assert_eq!(
        w.get::<Label>(next_input).unwrap().text,
        "Unsaved\nexact draft"
    );
}
#[test]
fn cancel_then_add_does_not_resurrect_cancelled_text() {
    let (mut w, root) = view();
    click(&mut w, Action::Edit(note()));
    sync(&mut w, root, Some("s"));
    let input = w.resource::<Context>().unwrap().input.unwrap();
    w.get_mut::<Label>(input).unwrap().text = "Discard this".into();
    click(&mut w, Action::Cancel);
    sync(&mut w, root, Some("s"));
    click(&mut w, Action::Add);
    sync(&mut w, root, Some("s"));
    let input = w.resource::<Context>().unwrap().input.unwrap();
    assert_eq!(w.get::<Label>(input).unwrap().text, "");
}
#[test]
fn unknown_mutation_editor_is_not_editable() {
    let (mut w, root) = view();
    click(&mut w, Action::Edit(note()));
    sync(&mut w, root, Some("s"));
    click(&mut w, Action::Save);
    let command = drain_commands(&mut w).remove(0);
    apply(
        &mut w,
        ContextUpdate {
            command,
            result: Err("offline".into()),
        },
    );
    sync(&mut w, root, Some("s"));
    let input = w.resource::<Context>().unwrap().input.unwrap();
    assert!(!w.has::<Prompt>(input));
}
#[tokio::test]
async fn sdk_transport_uses_real_encoded_route_auth_and_flat_mutation_contract() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::Arc,
        time::Duration,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        for (verb, status, body) in [
            ("GET", 200, board(4).to_string()),
            ("POST", 403, "forbidden".into()),
        ] {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut raw = Vec::new();
            let mut buffer = [0; 2048];
            let end = loop {
                let n = socket.read(&mut buffer).unwrap();
                assert!(n > 0);
                raw.extend_from_slice(&buffer[..n]);
                if let Some(i) = raw.windows(4).position(|v| v == b"\r\n\r\n") {
                    break i + 4;
                }
            };
            let header = String::from_utf8(raw[..end].to_vec()).unwrap();
            let len = header
                .lines()
                .find_map(|l| {
                    l.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .and_then(|v| v.trim().parse::<usize>().ok())
                })
                .unwrap_or(0);
            while raw.len() < end + len {
                let n = socket.read(&mut buffer).unwrap();
                assert!(n > 0);
                raw.extend_from_slice(&buffer[..n]);
            }
            assert!(
                header.starts_with(&format!(
                    "{verb} /v1/sessions/chat%2Fopaque/stickies HTTP/1.1"
                )),
                "{header}"
            );
            assert!(
                header
                    .to_ascii_lowercase()
                    .contains("authorization: bearer test-only")
            );
            if verb == "POST" {
                let v: Value = serde_json::from_slice(&raw[end..]).unwrap();
                assert_eq!(v["action"], "sticky.resolve");
                assert_eq!(v["id"], "real/id");
                assert_eq!(v["expected_revision"], 4);
                assert!(v["request_key"].as_str().is_some());
                assert!(v.get("operation").is_none());
            }
            write!(socket,"HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        }
    });
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::AUTHORIZATION,
        reqwest::header::HeaderValue::from_static("Bearer test-only"),
    );
    let http = reqwest::Client::builder()
        .default_headers(headers)
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    let client = MedousaClient::with_transport(
        Arc::new(medousa_sdk::HttpTransport::with_client(http)),
        origin,
    );
    let mut w = world();
    let read = execute(&client, refresh("chat/opaque")).await.unwrap();
    apply(&mut w, read);
    let s = &w.resource::<Context>().unwrap().sessions["chat/opaque"];
    assert_eq!(board_revision(s), Some(4));
    let command = mutation(
        "chat/opaque",
        s,
        json!({"action":"sticky.resolve","id":"real/id"}),
    )
    .unwrap();
    let update = execute(&client, command).await.unwrap();
    assert!(update.result.is_err());
    apply(&mut w, update);
    assert_eq!(
        board_revision(&w.resource::<Context>().unwrap().sessions["chat/opaque"]),
        Some(4)
    );
    handle.join().unwrap();
}
