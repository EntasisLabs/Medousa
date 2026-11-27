use super::*;
use medousa_sdk::HttpTransport;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
fn review() -> Value {
    json!({"work_id":"work","title":"Review terminal lifecycle","state":"sealed","human_phase":"review","attempt_id":"attempt","attempt_seq":2,"evidence_id":"evidence","evidence_digest":"digest","baseline_oid":"base","sealed_head_oid":"head","changed_files":[{"path":"src/main.rs","lines_added":1,"lines_removed":1}],"allowed_actions":{"review":{"allowed":true},"apply":{"allowed":true}},"synthesis":{"outcome":"Persistent workshop terminal sessions, with explicit reconnect and new shell controls.","status":"ready_for_review","status_summary":"The requested terminal integration is ready for review.","risk":"medium","risk_summary":"Changes touch session ownership and terminal input handling.","verification":{"label":"Native regression suite","success":true,"exit_code":0,"command":["cargo","test"]},"issues":[],"recommended_next_action":"Inspect the sealed changes, then approve or request changes."},"candidates":[{"attempt_id":"attempt","attempt_seq":2},{"attempt_id":"older","attempt_seq":1}],"decision":{"id":"decision"},"comments":[]})
}
fn diff() -> Value {
    json!({"work_id":"work","attempt_id":"attempt","path":"src/main.rs","baseline_oid":"base","reviewed_oid":"head","hunks":[{"old_start":1,"old_count":1,"new_start":1,"new_count":1,"lines":[{"kind":"deletion","old_line":1,"new_line":null,"content":"old text\n"},{"kind":"addition","old_line":null,"new_line":1,"content":"new text\n"}]}]})
}
fn world() -> World {
    let mut app = App::new();
    app.add_plugin(rndr_ui::UiPlugin);
    let mut w = std::mem::take(app.world_mut());
    let p = w.spawn((Layout {
        width: Size::Px(1100.),
        height: Size::Px(800.),
        ..Layout::column()
    },));
    mount(&mut w, p);
    w
}
fn seeded() -> World {
    let mut w = world();
    let s = w.resource_mut::<State>().unwrap();
    s.work = Some("work".into());
    s.review = review();
    s.redraw = true;
    sync(&mut w);
    w
}
fn act(w: &mut World, a: Action) {
    w.resource_mut::<State>().unwrap().actions.insert(7, a);
    assert!(action(w, &HostIntent::Command(7)));
}
fn body(w: &mut World, text: &str) {
    let e = w.resource::<State>().unwrap().body.unwrap();
    w.get_mut::<Label>(e).unwrap().text = text.into();
}
fn read_command(serial: u64, work: &str) -> Command {
    Command {
        serial,
        kind: Kind::Review {
            work: work.into(),
            attempt: None,
        },
    }
}
#[test]
fn approval_fails_closed_for_missing_or_blocked_evidence() {
    let good = review();
    assert!(approvable(&good));
    for path in [
        "evidence_id",
        "evidence_digest",
        "attempt_id",
        "sealed_head_oid",
        "baseline_oid",
    ] {
        let mut v = good.clone();
        v[path] = Value::Null;
        assert!(!approvable(&v), "{path}");
    }
    for key in ["truncated", "base_advanced"] {
        let mut v = good.clone();
        v[key] = true.into();
        assert!(!approvable(&v));
    }
    let mut v = good.clone();
    v["synthesis"]["issues"] = json!([{"blocks_approval":true}]);
    assert!(!approvable(&v));
    v = good.clone();
    v["synthesis"]["verification"]["success"] = false.into();
    assert!(!approvable(&v));
    v = good.clone();
    v["policy"] = json!({"violations":["unsafe"]});
    assert!(!approvable(&v));
    v = good.clone();
    v["allowed_actions"]["review"]["allowed"] = false.into();
    assert!(!approvable(&v));
    v = good;
    v["changed_files"] = json!([]);
    assert!(!approvable(&v));
}
#[test]
fn delayed_review_and_file_cannot_replace_new_selection() {
    let mut w = seeded();
    let s = w.resource_mut::<State>().unwrap();
    s.read_pending = Some(11);
    s.work = Some("other".into());
    apply(
        &mut w,
        Update {
            command: read_command(10, "work"),
            result: Ok(review()),
        },
    );
    assert_eq!(
        w.resource::<State>().unwrap().work.as_deref(),
        Some("other")
    );
    assert_eq!(w.resource::<State>().unwrap().read_pending, Some(11));
    let r = revision(&review()).unwrap();
    let s = w.resource_mut::<State>().unwrap();
    s.file = Some(("another.rs".into(), Value::Null));
    s.file_pending = Some(12);
    apply(
        &mut w,
        Update {
            command: Command {
                serial: 12,
                kind: Kind::File {
                    revision: r,
                    path: "src/main.rs".into(),
                },
            },
            result: Ok(diff()),
        },
    );
    assert_eq!(
        w.resource::<State>().unwrap().file.as_ref().unwrap().0,
        "another.rs"
    );
    assert!(
        w.resource::<State>()
            .unwrap()
            .file
            .as_ref()
            .unwrap()
            .1
            .is_null()
    );
}
#[test]
fn diff_identity_and_markers_are_exact() {
    let r = revision(&review()).unwrap();
    assert!(file_matches(&diff(), &r, "src/main.rs"));
    for key in [
        "work_id",
        "attempt_id",
        "path",
        "baseline_oid",
        "reviewed_oid",
    ] {
        let mut v = diff();
        v[key] = "different".into();
        assert!(!file_matches(&v, &r, "src/main.rs"));
    }
    let (rows, _) = crate::diff_presentation::rows(&World::new(), &diff());
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[1].marker, rndr_ui::diff::DiffMarker::Removed);
    assert_eq!(rows[2].marker, rndr_ui::diff::DiffMarker::Added);
}
#[test]
fn explicit_confirmation_and_unknown_write_never_replay() {
    let mut w = seeded();
    body(&mut w, "Looks good");
    act(&mut w, Action::Approve);
    assert!(take_commands(&mut w).is_empty());
    act(&mut w, Action::Confirm);
    let mut commands = take_commands(&mut w);
    assert_eq!(commands.len(), 1);
    let c = commands.remove(0);
    let Kind::Mutate {
        revision: r,
        mutation: Mutation::Approve { rationale, .. },
    } = &c.kind
    else {
        panic!()
    };
    assert_eq!(r.digest, "digest");
    assert_eq!(rationale, "Looks good");
    act(&mut w, Action::Confirm);
    assert!(take_commands(&mut w).is_empty());
    apply(
        &mut w,
        Update {
            command: c,
            result: Err("Delivery unknown".into()),
        },
    );
    assert!(w.resource::<State>().unwrap().uncertain);
    act(&mut w, Action::Approve);
    act(&mut w, Action::Confirm);
    assert!(take_commands(&mut w).is_empty());
    act(&mut w, Action::Refresh);
    assert!(
        take_commands(&mut w)
            .iter()
            .all(|c| !matches!(c.kind, Kind::Mutate { .. }))
    );
}
#[test]
fn request_changes_requires_rationale_and_clears_only_on_success() {
    let mut w = seeded();
    act(&mut w, Action::Request);
    assert!(w.resource::<State>().unwrap().confirmation.is_none());
    body(&mut w, "Handle disconnects");
    act(&mut w, Action::Request);
    act(&mut w, Action::Confirm);
    let c = take_commands(&mut w).remove(0);
    apply(
        &mut w,
        Update {
            command: c,
            result: Ok(json!({})),
        },
    );
    assert!(w.resource::<State>().unwrap().draft.body.is_empty());
    sync(&mut w);
    assert!(w.resource::<State>().unwrap().draft.body.is_empty());
}
#[test]
fn comment_requires_real_side_line_and_sealed_anchor() {
    let mut w = seeded();
    let s = w.resource_mut::<State>().unwrap();
    s.file = Some(("src/main.rs".into(), diff()));
    s.redraw = true;
    sync(&mut w);
    body(&mut w, "Please explain this");
    let e = w.resource::<State>().unwrap().line.unwrap();
    w.get_mut::<Label>(e).unwrap().text = "999".into();
    act(&mut w, Action::Comment);
    assert!(w.resource::<State>().unwrap().confirmation.is_none());
    let e = w.resource::<State>().unwrap().line.unwrap();
    w.get_mut::<Label>(e).unwrap().text = "1".into();
    act(&mut w, Action::Comment);
    act(&mut w, Action::Confirm);
    let c = take_commands(&mut w).remove(0);
    let Kind::Mutate {
        revision: r,
        mutation: m,
    } = c.kind
    else {
        panic!()
    };
    let (_, v) = mutation_body(&r, &m);
    assert_eq!(v["side"], "new");
    assert_eq!(v["anchor_text"], "new text\n");
    assert_eq!(v["attempt_id"], "attempt");
    assert_eq!(v["evidence_id"], "evidence");
}
#[test]
fn drafts_survive_file_navigation_and_are_isolated_by_evidence() {
    let mut w = seeded();
    body(&mut w, "Summary draft");
    act(&mut w, Action::File("src/main.rs".into()));
    assert!(w.resource::<State>().unwrap().draft.body.is_empty());
    body(&mut w, "File draft");
    act(&mut w, Action::Summary);
    assert_eq!(w.resource::<State>().unwrap().draft.body, "Summary draft");
    sync(&mut w);
    assert_eq!(w.resource::<State>().unwrap().draft.body, "Summary draft");
    act(&mut w, Action::File("src/main.rs".into()));
    assert_eq!(w.resource::<State>().unwrap().draft.body, "File draft");
    let s = w.resource_mut::<State>().unwrap();
    s.read_pending = Some(99);
    let mut v = review();
    v["evidence_id"] = "new-evidence".into();
    apply(
        &mut w,
        Update {
            command: read_command(99, "work"),
            result: Ok(v),
        },
    );
    assert!(w.resource::<State>().unwrap().draft.body.is_empty());
}
#[test]
fn refresh_invalidates_confirmation_and_duplicate_activation_does_not_reload() {
    let mut w = seeded();
    act(&mut w, Action::Approve);
    act(&mut w, Action::Refresh);
    assert!(w.resource::<State>().unwrap().confirmation.is_none());
    assert!(!ready(w.resource::<State>().unwrap()));
    take_commands(&mut w);
    w.resource_mut::<State>().unwrap().loaded = true;
    activate(&mut w);
    assert!(take_commands(&mut w).is_empty());
}
#[tokio::test]
async fn paginated_queue_uses_canonical_queries() {
    let (client, h) = fixture(vec![
        json_wire(
            "GET",
            "/v1/forge/items?limit=256",
            json!({"items":[{"id":"one"}],"truncated":true,"next_cursor":"next"}),
        ),
        json_wire(
            "GET",
            "/v1/forge/items?limit=256&cursor=next",
            json!({"items":[{"id":"two"}],"truncated":false}),
        ),
    ]);
    let u = execute(
        &client,
        Command {
            serial: 1,
            kind: Kind::Queue,
        },
    )
    .await;
    assert_eq!(u.result.unwrap().as_array().unwrap().len(), 2);
    h.join().unwrap();
}
#[tokio::test]
async fn incomplete_pagination_fails_instead_of_showing_partial_queue() {
    let (client, h) = fixture(vec![json_wire(
        "GET",
        "/v1/forge/items?limit=256",
        json!({"items":[],"truncated":true}),
    )]);
    assert!(
        execute(
            &client,
            Command {
                serial: 1,
                kind: Kind::Queue
            }
        )
        .await
        .result
        .is_err()
    );
    h.join().unwrap();
}
#[tokio::test]
async fn approval_preflight_stops_if_evidence_changes() {
    let mut fresh = review();
    fresh["evidence_digest"] = "new".into();
    let (client, h) = fixture(vec![json_wire(
        "GET",
        "/v1/forge/items/work/review?attempt_id=attempt",
        fresh,
    )]);
    let u = execute(
        &client,
        Command {
            serial: 1,
            kind: Kind::Mutate {
                revision: revision(&review()).unwrap(),
                mutation: Mutation::Approve {
                    strategy: "preserve_branch".into(),
                    rationale: "ok".into(),
                },
            },
        },
    )
    .await;
    assert!(u.result.is_err());
    h.join().unwrap();
}
#[tokio::test]
async fn approve_then_separate_apply_have_exact_authority_payloads() {
    let mut wire = json_wire(
        "POST",
        "/v1/forge/items/work/decisions",
        json!({"id":"decision"}),
    );
    wire.check = Some(|raw| {
        let v: Value = serde_json::from_str(raw.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(v["evidence_id"], "evidence");
        assert_eq!(v["evidence_digest"], "digest");
        assert_eq!(v["strategy"], "preserve_branch");
    });
    let (client, h) = fixture(vec![
        json_wire(
            "GET",
            "/v1/forge/items/work/review?attempt_id=attempt",
            review(),
        ),
        wire,
    ]);
    assert!(
        execute(
            &client,
            Command {
                serial: 1,
                kind: Kind::Mutate {
                    revision: revision(&review()).unwrap(),
                    mutation: Mutation::Approve {
                        strategy: "preserve_branch".into(),
                        rationale: "Reviewed".into()
                    }
                }
            }
        )
        .await
        .result
        .is_ok()
    );
    h.join().unwrap();
    let mut wire = json_wire("POST", "/v1/forge/items/work/apply", json!({"ok":true}));
    wire.check = Some(|raw| {
        let v: Value = serde_json::from_str(raw.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(v, json!({"decision_id":"decision"}));
    });
    let (client, h) = fixture(vec![
        json_wire(
            "GET",
            "/v1/forge/items/work/review?attempt_id=attempt",
            review(),
        ),
        wire,
    ]);
    assert!(
        execute(
            &client,
            Command {
                serial: 2,
                kind: Kind::Mutate {
                    revision: revision(&review()).unwrap(),
                    mutation: Mutation::Apply("decision".into())
                }
            }
        )
        .await
        .result
        .is_ok()
    );
    h.join().unwrap();
}
#[tokio::test]
async fn failed_mutation_is_not_retried_or_leaked() {
    let mut wire = json_wire(
        "POST",
        "/v1/forge/items/work/review/request-changes",
        json!({"secret":"DO_NOT_RENDER"}),
    );
    wire.status = 409;
    let (client, h) = fixture(vec![
        json_wire(
            "GET",
            "/v1/forge/items/work/review?attempt_id=attempt",
            review(),
        ),
        wire,
    ]);
    let error = execute(
        &client,
        Command {
            serial: 1,
            kind: Kind::Mutate {
                revision: revision(&review()).unwrap(),
                mutation: Mutation::Request("Fix it".into()),
            },
        },
    )
    .await
    .result
    .unwrap_err();
    assert!(!error.contains("DO_NOT_RENDER"));
    h.join().unwrap();
}

struct Wire {
    method: &'static str,
    path: String,
    status: u16,
    content_type: &'static str,
    body: String,
    check: Option<fn(&str)>,
}
fn json_wire(method: &'static str, path: &str, body: Value) -> Wire {
    Wire {
        method,
        path: path.into(),
        status: 200,
        content_type: "application/json",
        body: body.to_string(),
        check: None,
    }
}
fn fixture(wires: Vec<Wire>) -> (Arc<MedousaClient>, thread::JoinHandle<()>) {
    fixture_gated(wires, None)
}

fn fixture_gated(
    wires: Vec<Wire>,
    gate: Option<(
        &'static str,
        std::sync::mpsc::Receiver<()>,
        std::sync::mpsc::Sender<()>,
    )>,
) -> (Arc<MedousaClient>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let handle = thread::spawn(move || {
        let mut pending = wires;
        let mut gate = gate;
        let mut blocked_response = None;
        while !pending.is_empty() {
            let deadline = Instant::now() + Duration::from_secs(8);
            let mut socket = loop {
                match listener.accept() {
                    Ok((s, _)) => break s,
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(e) => panic!(
                        "missing fixture request {} {}: {e}",
                        pending[0].method, pending[0].path
                    ),
                }
            };
            socket.set_nonblocking(false).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut raw = Vec::new();
            let mut buffer = [0; 4096];
            let header_end = loop {
                let n = socket.read(&mut buffer).unwrap();
                assert!(n > 0);
                raw.extend_from_slice(&buffer[..n]);
                if let Some(i) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                    break i + 4;
                }
            };
            let header = String::from_utf8(raw[..header_end].to_vec()).unwrap();
            let length = header
                .lines()
                .find_map(|s| {
                    s.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .and_then(|s| s.trim().parse::<usize>().ok())
                })
                .unwrap_or(0);
            while raw.len() < header_end + length {
                let n = socket.read(&mut buffer).unwrap();
                assert!(n > 0);
                raw.extend_from_slice(&buffer[..n]);
            }
            // Independent catalogs may arrive in any order. Match wire routes,
            // preserving same-route order (e.g. active/history reconciliation).
            let index = pending
                .iter()
                .position(|expected| {
                    header.starts_with(&format!("{} {} HTTP/1.1", expected.method, expected.path))
                })
                .unwrap_or_else(|| panic!("unexpected request: {header}"));
            let expected = pending.remove(index);
            if let Some(check) = expected.check {
                check(std::str::from_utf8(&raw).unwrap());
            }
            let release = if gate
                .as_ref()
                .is_some_and(|(path, _, _)| *path == expected.path)
            {
                gate.take().map(|(_, release, started)| {
                    started.send(()).unwrap();
                    release
                })
            } else {
                None
            };
            let gated = release.is_some();
            let respond = move || {
                if let Some(release) = release {
                    release
                        .recv_timeout(Duration::from_secs(5))
                        .expect("test must release gated response");
                }
                let result = write!(
                    socket,
                    "HTTP/1.1 {} OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    expected.status,
                    expected.content_type,
                    expected.body.len(),
                    expected.body
                );
                // Gated read-only work may be intentionally cancelled by newer
                // navigation or a mode mutation before its response is released.
                if !gated {
                    result.unwrap();
                }
            };
            // Keep accepting requests while a deliberately blocked endpoint waits.
            if gated {
                blocked_response = Some(thread::spawn(respond));
            } else {
                respond();
            }
        }
        if let Some(response) = blocked_response {
            response.join().unwrap();
        }
    });
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    (
        Arc::new(MedousaClient::with_transport(
            Arc::new(HttpTransport::with_client(http)),
            origin,
        )),
        handle,
    )
}
pub(crate) fn seed_visual(w: &mut World, changes: bool, confirm: bool) {
    let s = w.resource_mut::<State>().unwrap();
    s.loaded = true;
    s.queue_pending = None;
    s.work = Some("work".into());
    s.review = review();
    s.review["title"] = json!("Rust CI Engine — MVP");
    s.review["synthesis"]["outcome"] = json!(
        "Implement the Rust CI Engine with deterministic execution, durable results and bounded native workers. Includes failure and cancellation cleanup, artifact provenance, and isolated work for review. No credentials, permission expansion, publishing or deployment."
    );
    s.review["candidates"][0]["executor"] = json!("medousa-assistant");
    s.review["candidates"][0]["branch"] = json!("worktree/rust-ci-engine");
    s.review["synthesis"]["unresolved_issues"] = json!([
        "No project check was recorded for this revision.",
        "Reconnect after daemon restart is untested."
    ]);
    s.review["changed_files"] = json!([
        {"path":"src/main.rs","lines_added":14,"lines_removed":2,"scopes":[{"id":"a","label":"engine::main","kind":"function","line_start":1,"line_end":20,"lines_added":9,"lines_removed":2},{"id":"b","label":"engine::Config","kind":"struct","line_start":22,"line_end":40,"lines_added":5,"lines_removed":0}]},
        {"path":"Cargo.toml","lines_added":14,"lines_removed":0},
        {"path":"Cargo.lock","lines_added":107,"lines_removed":0},
        {"path":"README.md","lines_added":25,"lines_removed":0},
        {"path":"docs/CONTRACT-v1.md","lines_added":223,"lines_removed":0},
        {"path":"docs/DEVELOPMENT.md","lines_added":71,"lines_removed":0},
        {"path":"docs/SDK-MAPPING.md","lines_added":155,"lines_removed":0},
        {"path":"docs/VERIFICATION.md","lines_added":138,"lines_removed":0},
        {"path":"examples/request.json","lines_added":5,"lines_removed":0},
        {"path":"examples/skipped-result.json","lines_added":62,"lines_removed":0},
        {"path":"src/admission.rs","lines_added":55,"lines_removed":0},
        {"path":"src/contract.rs","lines_added":82,"lines_removed":0},
        {"path":"src/executor.rs","lines_added":28,"lines_removed":0},
        {"path":"src/grapheme_host.rs","lines_added":16,"lines_removed":0},
        {"path":"src/lib.rs","lines_added":8,"lines_removed":0},
        {"path":"src/stasis_adapter.rs","lines_added":23,"lines_removed":0},
        {"path":"tests/cli.rs","lines_added":48,"lines_removed":0},
        {"path":"tests/contracts.rs","lines_added":405,"lines_removed":0}
    ]);
    s.items = vec![
        json!({"id":"work","title":"Terminal integration","human_phase":"review","state":"sealed"}),
        json!({"id":"next","title":"Composer attachment handling","human_phase":"work","state":"running"}),
        json!({"id":"done","title":"Chat navigation polish","human_phase":"complete","state":"applied"}),
    ];
    s.changes = changes;
    if changes {
        let mut value = diff();
        value["hunks"][0]["new_count"] = json!(80);
        value["hunks"][0]["lines"] = json!((1..=80).map(|line| json!({
            "kind":"addition", "old_line":null, "new_line":line,
            "content": if line == 3 {
                "Long sealed source line with punctuation, descenders (gypq), and a trailing marker beyond the pane edge — END_OF_LONG_LINE".to_owned()
            } else { format!("let result_{line} = inspect_sealed_changes(); // gypq") }
        })).collect::<Vec<_>>());
        s.expanded.insert("src/main.rs".into());
        s.symbols.insert("src/main.rs".into());
        s.file_cache.insert("src/main.rs".into(), value.clone());
        s.file = Some(("src/main.rs".into(), value));
    }
    if confirm {
        s.confirmation = Some(Mutation::Approve {
            strategy: "preserve_branch".into(),
            rationale: "Regression suite and sealed diff reviewed.".into(),
        });
    }
    s.redraw = true;
    sync(w);
}

#[test]
fn presentation_tabs_keep_file_navigation_out_of_summary_and_details_collapsed() {
    let mut w = seeded();
    let s = w.resource::<State>().unwrap();
    assert!(!s.changes);
    assert!(
        !s.actions
            .values()
            .any(|a| matches!(a, Action::File(_) | Action::ToggleFile(_)))
    );
    assert!(
        !w.query::<Label>()
            .iter()
            .any(|(_, l)| l.text.starts_with("Evidence "))
    );
    act(&mut w, Action::About);
    assert!(
        w.query::<Label>()
            .iter()
            .any(|(_, l)| l.text.starts_with("Evidence "))
    );
    act(&mut w, Action::Changes);
    assert!(w.resource::<State>().unwrap().changes);
    assert!(
        w.resource::<State>()
            .unwrap()
            .actions
            .values()
            .any(|a| matches!(a, Action::File(_) | Action::ToggleFile(_)))
    );
    assert!(take_commands(&mut w).is_empty()); // Reading tab selection is local, never a write.
    act(&mut w, Action::Summary);
    assert!(!w.resource::<State>().unwrap().changes);
}

#[test]
fn inbox_filters_are_local_and_resize_preserves_review_drafts() {
    let mut w = seeded();
    seed_visual(&mut w, false, false);
    act(&mut w, Action::Filter(2));
    let s = w.resource::<State>().unwrap();
    assert!(
        s.actions
            .values()
            .any(|a| matches!(a, Action::Open(id) if id == "done"))
    );
    assert!(
        !s.actions
            .values()
            .any(|a| matches!(a, Action::Open(id) if id == "work"))
    );
    assert!(take_commands(&mut w).is_empty());
    body(&mut w, "Keep this rationale while resizing.");
    w.insert_resource(Viewport::new(760., 760.));
    sync(&mut w);
    assert!(!w.resource::<State>().unwrap().viewport.0);
    assert_eq!(
        w.resource::<State>().unwrap().draft.body,
        "Keep this rationale while resizing."
    );
    w.insert_resource(Viewport::new(1200., 850.));
    sync(&mut w);
    assert!(w.resource::<State>().unwrap().viewport.0);
    assert_eq!(
        w.resource::<State>().unwrap().draft.body,
        "Keep this rationale while resizing."
    );
}

pub(crate) fn assert_visual_geometry(w: &World, changes: bool) {
    let detail = w.resource::<State>().unwrap().detail.unwrap();
    let viewport = w.resource::<Viewport>().unwrap();
    let detail_rect = w.get::<Rect>(detail).unwrap();
    assert!(detail_rect.w > 240. && detail_rect.h > 100.);
    assert!(detail_rect.x + detail_rect.w <= viewport.width + 1.);
    let label_rect = |text: &str| {
        let entity = w
            .query::<Label>()
            .into_iter()
            .find(|(e, l)| l.text == text && crate::surfaces::descendant(w, *e, detail))
            .unwrap_or_else(|| panic!("Missing review label: {text}"))
            .0;
        *w.get::<Rect>(entity).unwrap()
    };
    if !changes {
        let title = label_rect("Rust CI Engine — MVP");
        let checks = label_rect("Checks");
        assert!(title.w > 150. && title.h > 10.);
        assert!(title.x + title.w <= detail_rect.x + detail_rect.w + 1.);
        if viewport.width >= 1100. {
            assert!(
                checks.x >= title.x + title.w,
                "Metadata must sit beside the reading column"
            );
        } else {
            assert!(
                checks.y > title.y + title.h,
                "Metadata must stack in narrow windows"
            );
        }
    } else {
        let files = label_rect(&format!(
            "{} changed files",
            w.resource::<State>().unwrap().review["changed_files"]
                .as_array()
                .unwrap()
                .len()
        ));
        let diff = w
            .query::<Semantics>()
            .into_iter()
            .find(|(e, sem)| {
                sem.name.as_deref() == Some("Sealed changes to src/main.rs")
                    && crate::surfaces::descendant(w, *e, detail)
            })
            .unwrap()
            .0;
        let rect = w.get::<Rect>(diff).unwrap();
        assert!(
            rect.h > 150.,
            "Diff must retain a usable viewport: {rect:?}"
        );
        assert!(
            rect.w > detail_rect.w - 12.,
            "Accordion diffs use the full reading width: {rect:?}; detail {detail_rect:?}"
        );
        assert_eq!(w.get::<Layout>(diff).unwrap().height, Size::Fill);
        for (e, label) in w.query::<Label>() {
            if label.text.contains("inspect_sealed_changes") {
                let row = w.get::<Rect>(e).unwrap();
                assert!(
                    row.h >= crate::diff_presentation::row_height(w) - 0.1,
                    "Diff code line must not clip: {row:?}"
                );
            }
        }
        assert!(
            files.w > 80. && files.w <= detail_rect.w + 1.,
            "File header {files:?}; detail {detail_rect:?}"
        );
        assert!(
            w.query::<Scroll>()
                .iter()
                .any(|(e, s)| crate::surfaces::descendant(w, *e, detail)
                    && s.content[1] > w.get::<Rect>(*e).unwrap().h + 10.),
            "Large reviews need a bounded scrolling file list"
        );
    }
}

#[test]
fn diff_rows_use_code_metrics_without_altering_source() {
    let mut value = diff();
    let long = "gypq ".repeat(100);
    value["hunks"][0]["lines"][1]["content"] = json!(format!("{long}\n"));
    let w = World::new();
    let (rows, _) = crate::diff_presentation::rows(&w, &value);
    assert!(
        rows.iter()
            .all(|r| r.height == crate::diff_presentation::row_height(&w))
    );
    assert!(rows[2].text.ends_with(&long));
}

pub(crate) fn scroll_visual_diff_to_end(w: &mut World) {
    let root = w
        .query::<Semantics>()
        .into_iter()
        .find(|(_, sem)| sem.name.as_deref() == Some("Sealed changes to src/main.rs"))
        .unwrap()
        .0;
    rndr_ui::collections::VirtualViewHandle { root }
        .jump(w, rndr_ui::collections::ItemId(81), 0)
        .unwrap();
}
pub(crate) fn assert_visual_diff_end(w: &World) {
    assert!(
        w.query::<Label>().into_iter().any(|(_, label)| label
            .text
            .contains("let result_80 = inspect_sealed_changes()")),
        "Last sealed diff row must be reachable"
    );
}

fn multi_file(w: &mut World) {
    w.resource_mut::<State>().unwrap().review["changed_files"] = json!([
        {"path":"src/main.rs","lines_added":1,"lines_removed":1},
        {"path":"src/other.rs","lines_added":2,"lines_removed":0}
    ]);
}
fn complete_file(w: &mut World, command: Command) {
    let Kind::File { path, .. } = &command.kind else {
        panic!("expected file read")
    };
    let mut value = diff();
    value["path"] = json!(path);
    apply(
        w,
        Update {
            command,
            result: Ok(value),
        },
    );
}
#[test]
fn accordions_serialize_expand_all_cache_exact_files_and_collapse_locally() {
    let mut w = seeded();
    multi_file(&mut w);
    act(&mut w, Action::ExpandAll);
    let mut reads = take_commands(&mut w);
    assert_eq!(reads.len(), 1);
    assert_eq!(w.resource::<State>().unwrap().expanded.len(), 2);
    complete_file(&mut w, reads.remove(0));
    let mut reads = take_commands(&mut w);
    assert_eq!(reads.len(), 1);
    complete_file(&mut w, reads.remove(0));
    assert_eq!(w.resource::<State>().unwrap().file_cache.len(), 2);
    assert!(take_commands(&mut w).is_empty());
    act(&mut w, Action::CollapseAll);
    assert!(w.resource::<State>().unwrap().expanded.is_empty());
    act(&mut w, Action::ToggleFile("src/other.rs".into()));
    assert!(
        take_commands(&mut w).is_empty(),
        "Cached reopening is local"
    );
    assert_eq!(
        w.resource::<State>().unwrap().file.as_ref().unwrap().1["path"],
        "src/other.rs"
    );
}
#[test]
fn collapse_all_stops_queued_reads_and_revision_change_rejects_inflight_cache() {
    let mut w = seeded();
    multi_file(&mut w);
    act(&mut w, Action::ExpandAll);
    let read = take_commands(&mut w).remove(0);
    act(&mut w, Action::CollapseAll);
    complete_file(&mut w, read);
    assert!(take_commands(&mut w).is_empty());
    assert_eq!(w.resource::<State>().unwrap().file_cache.len(), 1);
    act(&mut w, Action::ToggleFile("src/other.rs".into()));
    let old = take_commands(&mut w).remove(0);
    act(&mut w, Action::Candidate("older".into()));
    take_commands(&mut w);
    complete_file(&mut w, old);
    assert!(w.resource::<State>().unwrap().file_cache.is_empty());
    assert!(w.resource::<State>().unwrap().expanded.is_empty());
}
#[test]
fn accordion_failure_is_not_retried_until_explicit_action() {
    let mut w = seeded();
    act(&mut w, Action::ToggleFile("src/main.rs".into()));
    let read = take_commands(&mut w).remove(0);
    apply(
        &mut w,
        Update {
            command: read,
            result: Err("Unavailable".into()),
        },
    );
    assert!(take_commands(&mut w).is_empty());
    assert!(
        w.resource::<State>()
            .unwrap()
            .file_errors
            .contains("src/main.rs")
    );
    act(&mut w, Action::File("src/main.rs".into()));
    assert_eq!(take_commands(&mut w).len(), 1);
}
#[test]
fn loading_other_accordion_never_changes_comment_file_or_draft() {
    let mut w = seeded();
    multi_file(&mut w);
    act(&mut w, Action::File("src/main.rs".into()));
    let read = take_commands(&mut w).remove(0);
    complete_file(&mut w, read);
    body(&mut w, "Keep this exact file draft");
    act(&mut w, Action::ExpandAll);
    let read = take_commands(&mut w).remove(0);
    complete_file(&mut w, read);
    let s = w.resource::<State>().unwrap();
    assert_eq!(s.file.as_ref().unwrap().0, "src/main.rs");
    assert_eq!(s.draft.body, "Keep this exact file draft");
    act(&mut w, Action::File("outside-review.rs".into()));
    assert!(take_commands(&mut w).is_empty());
    assert_eq!(
        w.resource::<State>().unwrap().file.as_ref().unwrap().0,
        "src/main.rs"
    );
}

#[test]
fn accordion_rerender_preserves_diff_anchor_without_crossing_revision() {
    let mut app = crate::ui::app();
    app.update();
    app.world_mut().insert_resource(Viewport::new(1200., 850.));
    crate::surfaces::select(app.world_mut(), crate::surfaces::SurfaceId::Reviews);
    take_commands(app.world_mut());
    seed_visual(app.world_mut(), true, false);
    for _ in 0..4 {
        rndr_paint::frame(&mut app);
    }
    scroll_visual_diff_to_end(app.world_mut());
    for _ in 0..4 {
        rndr_paint::frame(&mut app);
    }
    let root = |w: &World| {
        w.query::<Semantics>()
            .into_iter()
            .find(|(_, s)| s.name.as_deref() == Some("Sealed changes to src/main.rs"))
            .unwrap()
            .0
    };
    let before = rndr_ui::collections::VirtualViewHandle {
        root: root(app.world()),
    }
    .anchor(app.world())
    .unwrap();
    act(app.world_mut(), Action::CollapseAll);
    act(app.world_mut(), Action::ToggleFile("src/main.rs".into()));
    for _ in 0..4 {
        rndr_paint::frame(&mut app);
    }
    let after = rndr_ui::collections::VirtualViewHandle {
        root: root(app.world()),
    }
    .anchor(app.world())
    .unwrap();
    assert_eq!(before, after);
    act(app.world_mut(), Action::Candidate("older".into()));
    assert!(
        app.world()
            .resource::<State>()
            .unwrap()
            .diff_positions
            .is_empty()
    );
}

pub(crate) fn visual_accordions(w: &mut World, expanded: bool) {
    let s = w.resource_mut::<State>().unwrap();
    s.review["changed_files"] = json!([
        {"path":"src/main.rs","lines_added":18,"lines_removed":7},
        {"path":"src/other.rs","lines_added":2,"lines_removed":1}
    ]);
    let mut other = diff();
    other["path"] = json!("src/other.rs");
    s.file_cache.insert("src/other.rs".into(), other);
    act(
        w,
        if expanded {
            Action::ExpandAll
        } else {
            Action::CollapseAll
        },
    );
}

pub(crate) fn visual_context(w: &mut World, expanded: bool) {
    let mut value = diff();
    value["baseline"] = json!({"content":"// Sealed before and after versions\nlet café = old(10);\n\n// Unchanged context from Git, not the checkout\nfinish();\n"});
    value["reviewed"] = json!({"content":"// Sealed before and after versions\nlet café = new(20);\n\n// Unchanged context from Git, not the checkout\nfinish();\n"});
    value["hunks"] = json!([{"old_start":2,"old_count":1,"new_start":2,"new_count":1,"lines":[
        {"kind":"deletion","old_line":2,"content":"let café = old(10);\n"},
        {"kind":"addition","new_line":2,"content":"let café = new(20);\n"}]}]);
    let s = w.resource_mut::<State>().unwrap();
    s.expanded.clear();
    s.expanded.insert("src/main.rs".into());
    s.context.clear();
    s.context_cache.clear();
    if expanded {
        s.context.insert("src/main.rs".into());
    }
    s.file_cache.insert("src/main.rs".into(), value.clone());
    s.file = Some(("src/main.rs".into(), value));
    s.diff_positions.clear();
    s.changes = true;
    s.redraw = true;
    sync(w);
}

#[test]
fn context_toggle_is_local_preserves_comment_authority_and_resets_on_revision_change() {
    let mut w = seeded();
    visual_context(&mut w, false);
    take_commands(&mut w);
    body(&mut w, "Keep my anchored draft");
    let original = w.resource::<State>().unwrap().file.clone();
    act(&mut w, Action::Context("src/main.rs".into()));
    sync(&mut w);
    let s = w.resource::<State>().unwrap();
    assert!(s.context.contains("src/main.rs"));
    assert_eq!(s.file, original);
    assert_eq!(s.draft.body, "Keep my anchored draft");
    assert!(
        w.query::<Label>()
            .iter()
            .any(|(_, l)| l.text == "Collapse unchanged context")
    );
    assert!(take_commands(&mut w).is_empty());
    act(&mut w, Action::Context("src/main.rs".into()));
    sync(&mut w);
    assert!(w.resource::<State>().unwrap().context.is_empty());
    assert!(
        w.query::<Label>()
            .iter()
            .any(|(_, l)| l.text == "Show unchanged context")
    );
    act(&mut w, Action::Context("src/main.rs".into()));
    act(&mut w, Action::Candidate("older".into()));
    assert!(w.resource::<State>().unwrap().context.is_empty());
}

#[test]
fn incomplete_sealed_file_has_no_context_control() {
    let mut w = seeded();
    visual_context(&mut w, false);
    let s = w.resource_mut::<State>().unwrap();
    s.file_cache.get_mut("src/main.rs").unwrap()["truncated"] = json!(true);
    s.context_cache.clear();
    s.redraw = true;
    sync(&mut w);
    assert!(
        !w.resource::<State>()
            .unwrap()
            .actions
            .values()
            .any(|a| matches!(a, Action::Context(_)))
    );
}

#[test]
fn change_kinds_match_medousa_home_path_hints() {
    use super::view::change_kind;
    for (path, kind) in [
        ("src/main.rs", 0),
        ("apps/web/App.svelte", 0),
        ("tests/cli.rs", 1),
        ("src/reviews_tests.rs", 1),
        ("web/button.test.ts", 1),
        ("test_parser.py", 1),
        (".github/workflows/ci.yml", 2),
        ("Jenkinsfile", 2),
        ("docs/CONTRACT-v1.md", 3),
        ("README", 3),
        ("CHANGELOG.md", 3),
        ("Cargo.toml", 4),
        ("examples/request.json", 4),
    ] {
        assert_eq!(change_kind(path), kind, "{path}");
    }
}

#[test]
fn restore_needs_the_loaded_reviewed_blob_and_explicit_confirmation() {
    let mut w = seeded();
    act(&mut w, Action::Restore("src/main.rs".into()));
    assert!(w.resource::<State>().unwrap().confirmation.is_none());
    act(&mut w, Action::Restore("not/in/review.rs".into()));
    assert!(w.resource::<State>().unwrap().confirmation.is_none());
    w.resource_mut::<State>()
        .unwrap()
        .file_cache
        .insert("src/main.rs".into(), diff());
    act(&mut w, Action::Restore("src/main.rs".into()));
    assert!(take_commands(&mut w).is_empty());
    act(&mut w, Action::Confirm);
    let c = take_commands(&mut w).remove(0);
    let Kind::Mutate {
        revision: r,
        mutation: m @ Mutation::Restore { .. },
    } = &c.kind
    else {
        panic!()
    };
    let (op, body) = mutation_body(r, m);
    assert_eq!(op.path, ops::FORGE_ITEMS_BY_WORK_ID_REVIEW_FILE_POST.path);
    assert_eq!(
        body,
        json!({"path":"src/main.rs","expected_reviewed_oid":"head","attempt_id":"attempt"})
    );
    apply(
        &mut w,
        Update {
            command: c,
            result: Ok(json!({"path":"src/main.rs"})),
        },
    );
    let s = w.resource::<State>().unwrap();
    assert!(s.message.starts_with("Restored src/main.rs"));
    assert!(s.open_code.is_none(), "Handed to Code, never left pending");
}

#[test]
fn symbols_toggle_per_file() {
    let mut w = seeded();
    act(&mut w, Action::Symbols("src/main.rs".into()));
    assert!(
        w.resource::<State>()
            .unwrap()
            .symbols
            .contains("src/main.rs")
    );
    act(&mut w, Action::Symbols("src/main.rs".into()));
    assert!(w.resource::<State>().unwrap().symbols.is_empty());
}
