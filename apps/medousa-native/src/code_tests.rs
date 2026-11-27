use super::*;
fn fixture() -> App {
    let mut app = App::new();
    app.add_plugin(rndr_ui::UiPlugin);
    let w = app.world_mut();
    let root = w.spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::row()
    },));
    mount(w, root);
    app
}
fn click(w: &mut World, a: Action) {
    let id = next();
    w.resource_mut::<Code>().unwrap().actions.insert(id, a);
    assert!(action(w, &HostIntent::Command(id)));
}
fn key(work: &str, path: &str) -> Key {
    (work.into(), path.into())
}
fn file(work: &str, path: &str, content: &str, digest: &str) -> Value {
    json!({"work_id":work,"path":path,"content":content,"digest":digest,"encoding":"utf-8","byte_size":content.len()})
}
fn load(w: &mut World, work: &str, path: &str, content: &str, digest: &str) {
    w.resource_mut::<Code>().unwrap().selected = Some(work.into());
    click(w, Action::File(path.into()));
    let c = take_commands(w)
        .into_iter()
        .find(|c| matches!(c.operation, Operation::Load(_)))
        .unwrap();
    apply(
        w,
        CodeUpdate {
            command: c,
            result: Ok(file(work, path, content, digest)),
        },
    );
}
fn edit(w: &mut World, k: &Key, content: &str) {
    let id = w.resource::<Code>().unwrap().views[k].document;
    let d = w
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap();
    d.reload(d.revision(), content).unwrap();
    sync(w);
}
fn session(w: &mut World, work: &str) {
    w.resource_mut::<Code>().unwrap().sessions.insert(
        work.into(),
        Session {
            lease: Lease {
                id: "human-lease".into(),
                generation: 7,
            },
            checked: Instant::now(),
        },
    );
}
fn tab_event(app: &mut App, kind: TabStripEventKind) {
    let strip = app.world().resource::<Code>().unwrap().strip.unwrap().root;
    send(app.world_mut(), TabStripEvent { strip, kind });
    app.update();
    sync(app.world_mut());
}
#[test]
fn tab_order_uses_open_order_and_rndr_reorder_without_recreating_documents() {
    let mut app = fixture();
    load(app.world_mut(), "one", "z.rs", "z", "z1");
    load(app.world_mut(), "two", "other.rs", "other", "o1");
    load(app.world_mut(), "one", "a.rs", "a", "a1");
    let c = app.world().resource::<Code>().unwrap();
    let z = c.views[&key("one", "z.rs")].id;
    let a = c.views[&key("one", "a.rs")].id;
    let document = c.views[&key("one", "z.rs")].document;
    assert_eq!(
        c.tab_specs().iter().map(|s| s.id).collect::<Vec<_>>(),
        vec![TabId(z.0), TabId(a.0)]
    );
    tab_event(
        &mut app,
        TabStripEventKind::Reordered(rndr_ui::tabs::TabMove {
            id: TabId(a.0),
            target: TabId(z.0),
            place: rndr_ui::drag::DropPlace::Before,
        }),
    );
    let c = app.world().resource::<Code>().unwrap();
    assert_eq!(
        c.tab_specs().iter().map(|s| s.id).collect::<Vec<_>>(),
        vec![TabId(a.0), TabId(z.0)]
    );
    assert!(c.open_tabs.contains(&key("two", "other.rs")));
    assert_eq!(c.views[&key("one", "z.rs")].document, document);
    assert_eq!(c.key(), Some(key("one", "a.rs")));
    assert!(take_commands(app.world_mut()).is_empty());
}
#[test]
fn closing_last_tab_preserves_uncertain_draft_and_reopens_same_document() {
    let mut app = fixture();
    load(app.world_mut(), "one", "a.rs", "original", "d1");
    edit(app.world_mut(), &key("one", "a.rs"), "draft");
    let c = app.world_mut().resource_mut::<Code>().unwrap();
    c.buffers.get_mut(&key("one", "a.rs")).unwrap().uncertain = true;
    let view = &c.views[&key("one", "a.rs")];
    let (id, document) = (TabId(view.id.0), view.document);
    tab_event(&mut app, TabStripEventKind::CloseRequested(id));
    let c = app.world().resource::<Code>().unwrap();
    assert!(c.key().is_none());
    assert!(c.open_tabs.is_empty());
    assert!(
        app.world()
            .has::<Hidden>(c.views[&key("one", "a.rs")].handle.root)
    );
    click(app.world_mut(), Action::File("a.rs".into()));
    let c = app.world().resource::<Code>().unwrap();
    assert_eq!(c.views[&key("one", "a.rs")].document, document);
    assert_eq!(c.buffers[&key("one", "a.rs")].content, "draft");
    assert!(c.buffers[&key("one", "a.rs")].uncertain);
    assert!(take_commands(app.world_mut()).is_empty());
}
#[test]
fn find_bars_reuse_canonical_per_file_state_and_hide_on_switch() {
    let mut app = fixture();
    load(app.world_mut(), "one", "a.rs", "🦀 a a", "d1");
    click(app.world_mut(), Action::Find);
    let bar = app.world().resource::<Code>().unwrap().views[&key("one", "a.rs")]
        .find
        .unwrap();
    app.world_mut().get_mut::<Label>(bar.query).unwrap().text = "a".into();
    rndr_ui::source_tools::apply_find_bars(app.world_mut());
    assert_eq!(
        app.world().get::<Label>(bar.status).unwrap().text,
        "2 matches"
    );
    load(app.world_mut(), "one", "b.rs", "b", "d2");
    assert!(app.world().has::<Hidden>(bar.root));
    click(app.world_mut(), Action::File("a.rs".into()));
    let current = app.world().resource::<Code>().unwrap().views[&key("one", "a.rs")]
        .find
        .unwrap();
    assert_eq!(current.root, bar.root);
    assert!(!app.world().has::<Hidden>(bar.root));
    assert_eq!(app.world().get::<Label>(bar.query).unwrap().text, "a");
    click(app.world_mut(), Action::Find);
    assert!(app.world().has::<Hidden>(bar.root));
}
#[test]
fn breadcrumb_pickers_scope_threads_by_repository_without_replacing_drafts() {
    let mut app = fixture();
    let w = app.world_mut();
    load(w, "one", "src/lib.rs", "retained", "d1");
    edit(w, &key("one", "src/lib.rs"), "unsaved draft");
    let document = w.resource::<Code>().unwrap().views[&key("one", "src/lib.rs")].document;
    click(w, Action::Refresh);
    let command = take_commands(w)
        .into_iter()
        .find(|c| matches!(c.operation, Operation::Projects))
        .unwrap();
    apply(
        w,
        CodeUpdate {
            command,
            result: Ok(json!([
                {"id":"one", "title":"First thread", "target":{"kind":"git", "repo_path":"/a/repo"}, "human_phase":"working"},
                {"id":"two", "title":"Second thread", "target":{"kind":"git", "repo_path":"/a/repo"}},
                {"id":"three", "title":"Other repo", "target":{"kind":"git", "repo_path":"/b/repo"}}
            ])),
        },
    );
    // Browsing shows files only, never the entire project catalog above them.
    assert!(
        !w.resource::<Code>()
            .unwrap()
            .actions
            .values()
            .any(|a| matches!(a, Action::Repository(_) | Action::Project(_)))
    );
    click(w, Action::ChooseProjects);
    assert_eq!(
        w.resource::<Code>()
            .unwrap()
            .actions
            .values()
            .filter(|a| matches!(a, Action::Repository(_)))
            .count(),
        2
    );
    assert!(
        !w.resource::<Code>()
            .unwrap()
            .actions
            .values()
            .any(|a| matches!(a, Action::Project(_)))
    );
    click(w, Action::Repository("/a/repo".into()));
    assert_eq!(
        w.resource::<Code>()
            .unwrap()
            .actions
            .values()
            .filter(|a| matches!(a, Action::Project(_)))
            .count(),
        2
    );
    assert!(
        !w.resource::<Code>()
            .unwrap()
            .actions
            .values()
            .any(|a| matches!(a, Action::Project(id) if id == "three"))
    );
    click(w, Action::Project("two".into()));
    assert_eq!(
        w.resource::<Code>().unwrap().selected.as_deref(),
        Some("two")
    );
    assert!(w.resource::<Code>().unwrap().explorer_picker.is_none());
    assert!(
        take_commands(w)
            .iter()
            .any(|c| matches!(&c.operation, Operation::Tree(work) if work == "two"))
    );
    click(w, Action::ChooseUndertakings);
    assert_eq!(
        w.resource::<Code>().unwrap().explorer_picker,
        Some(ExplorerPicker::Undertakings("/a/repo".into()))
    );
    click(w, Action::Project("one".into()));
    let c = w.resource::<Code>().unwrap();
    assert_eq!(c.views[&key("one", "src/lib.rs")].document, document);
    assert_eq!(
        c.buffers[&key("one", "src/lib.rs")].content,
        "unsaved draft"
    );
    assert_eq!(c.key(), Some(key("one", "src/lib.rs")));
}
#[test]
fn cancelling_workspace_picker_preserves_active_scope_and_resets_filter() {
    let mut app = fixture();
    let w = app.world_mut();
    load(w, "one", "a.rs", "saved", "d1");
    take_commands(w);
    click(w, Action::ChooseProjects);
    let c = w.resource_mut::<Code>().unwrap();
    c.explorer_query = "unrelated".into();
    let filter = c.explorer_filter.unwrap();
    w.get_mut::<Label>(filter).unwrap().text = "unrelated".into();
    click(w, Action::Repository("/other/repo".into()));
    assert_eq!(
        w.resource::<Code>().unwrap().selected.as_deref(),
        Some("one")
    );
    click(w, Action::ClosePicker);
    let c = w.resource::<Code>().unwrap();
    assert!(c.explorer_query.is_empty());
    assert!(w.get::<Label>(filter).unwrap().text.is_empty());
    assert!(c.explorer_picker.is_none());
    assert_eq!(c.key(), Some(key("one", "a.rs")));
    assert!(take_commands(w).is_empty());
}
#[test]
fn activation_only_reads_and_is_idempotent() {
    let mut a = fixture();
    let w = a.world_mut();
    assert!(take_commands(w).is_empty());
    activate(w);
    activate(w);
    let q = take_commands(w);
    assert_eq!(q.len(), 1);
    assert!(matches!(q[0].operation, Operation::Projects));
}
#[test]
fn retained_files_and_projects_keep_distinct_documents_and_drafts() {
    let mut a = fixture();
    let w = a.world_mut();
    let k = key("one", "src/lib.rs");
    load(w, "one", "src/lib.rs", "one", "d1");
    edit(w, &k, "draft one");
    let first = w.resource::<Code>().unwrap().views[&k].handle;
    load(w, "two", "src/lib.rs", "two", "d2");
    edit(w, &key("two", "src/lib.rs"), "draft two");
    assert!(w.has::<Hidden>(first.root));
    click(w, Action::Project("one".into()));
    assert!(!w.has::<Hidden>(first.root));
    assert_eq!(
        w.resource::<Code>().unwrap().buffers[&k].content,
        "draft one"
    );
    assert_eq!(w.resource::<Code>().unwrap().views[&k].handle, first);
}
#[test]
fn save_requires_local_human_lease_and_digest_fence() {
    let mut a = fixture();
    let w = a.world_mut();
    let k = key("one", "a.rs");
    load(w, "one", "a.rs", "base", "opaque-digest");
    edit(w, &k, "draft");
    click(w, Action::Save);
    assert!(take_commands(w).is_empty());
    session(w, "one");
    click(w, Action::Save);
    let c = take_commands(w).pop().unwrap();
    let Operation::Save {
        key,
        content,
        digest,
        lease,
    } = c.operation
    else {
        panic!("not save")
    };
    assert_eq!(key, k);
    assert_eq!(content, "draft");
    assert_eq!(digest, "opaque-digest");
    assert_eq!(lease.id, "human-lease");
    assert_eq!(lease.generation, 7);
}
#[test]
fn save_preserves_edits_made_while_request_in_flight() {
    let mut a = fixture();
    let w = a.world_mut();
    let k = key("one", "a.rs");
    load(w, "one", "a.rs", "base", "v1");
    session(w, "one");
    edit(w, &k, "sent");
    click(w, Action::Save);
    let c = take_commands(w).pop().unwrap();
    edit(w, &k, "later edit");
    apply(
        w,
        CodeUpdate {
            command: c,
            result: Ok(file("one", "a.rs", "sent", "v2")),
        },
    );
    let b = &w.resource::<Code>().unwrap().buffers[&k];
    assert_eq!(b.content, "later edit");
    assert_eq!(b.baseline.content, "sent");
    assert_eq!(b.baseline.digest, "v2");
    assert!(b.dirty());
}
#[test]
fn unknown_save_blocks_retry_until_latest_read() {
    let mut a = fixture();
    let w = a.world_mut();
    let k = key("one", "a.rs");
    load(w, "one", "a.rs", "base", "v1");
    session(w, "one");
    edit(w, &k, "sent");
    click(w, Action::Save);
    let c = take_commands(w).pop().unwrap();
    apply(
        w,
        CodeUpdate {
            command: c,
            result: Err("connection lost".into()),
        },
    );
    click(w, Action::Save);
    assert!(take_commands(w).is_empty());
    assert!(w.resource::<Code>().unwrap().buffers[&k].uncertain);
    click(w, Action::Check);
    let c = take_commands(w).pop().unwrap();
    apply(
        w,
        CodeUpdate {
            command: c,
            result: Ok(file("one", "a.rs", "sent", "v2")),
        },
    );
    let b = &w.resource::<Code>().unwrap().buffers[&k];
    assert!(!b.uncertain);
    assert!(!b.dirty());
}
#[test]
fn remote_change_requires_explicit_acknowledgement_before_save() {
    let mut a = fixture();
    let w = a.world_mut();
    let k = key("one", "a.rs");
    load(w, "one", "a.rs", "base", "v1");
    session(w, "one");
    edit(w, &k, "local");
    click(w, Action::Check);
    let c = take_commands(w).pop().unwrap();
    apply(
        w,
        CodeUpdate {
            command: c,
            result: Ok(file("one", "a.rs", "remote", "v2")),
        },
    );
    assert_eq!(w.resource::<Code>().unwrap().buffers[&k].content, "local");
    click(w, Action::Save);
    assert!(take_commands(w).is_empty());
    click(w, Action::Acknowledge);
    click(w, Action::Save);
    assert!(matches!(&take_commands(w)[0].operation,Operation::Save{digest,..} if digest=="v2"));
}
#[test]
fn preview_binary_lossy_and_truncated_are_readonly() {
    for extra in [
        json!({"encoding":"binary"}),
        json!({"encoding":"utf-8-lossy"}),
        json!({"preview":true}),
        json!({"truncated":true}),
    ] {
        let mut v = file("one", "a.rs", "preview", "v1");
        v.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let s = source(&v, &key("one", "a.rs")).unwrap();
        assert!(s.readonly);
        let b = Buffer {
            baseline: s,
            content: "edited".into(),
            remote: None,
            uncertain: false,
            error: String::new(),
        };
        assert!(!b.saveable());
    }
    let mut a = fixture();
    let w = a.world_mut();
    w.resource_mut::<Code>().unwrap().selected = Some("one".into());
    click(w, Action::File("a.rs".into()));
    let c = take_commands(w).pop().unwrap();
    let mut v = file("one", "a.rs", "preview", "v1");
    v["preview"] = json!(true);
    apply(
        w,
        CodeUpdate {
            command: c,
            result: Ok(v),
        },
    );
    let id = w.resource::<Code>().unwrap().views[&key("one", "a.rs")].document;
    assert!(
        w.resource::<SourceDocuments>()
            .unwrap()
            .get(id)
            .unwrap()
            .is_readonly()
    );
}
#[test]
fn identity_mismatch_and_missing_digest_fail_closed() {
    let k = key("one", "a.rs");
    assert!(source(&file("other", "a.rs", "x", "v"), &k).is_err());
    assert!(source(&file("one", "other.rs", "x", "v"), &k).is_err());
    assert!(source(&file("one", "a.rs", "x", ""), &k).is_err());
}
#[test]
fn human_begin_requires_affordance_and_unknown_begin_cannot_blind_retry() {
    let mut a = fixture();
    let w = a.world_mut();
    w.resource_mut::<Code>().unwrap().selected = Some("one".into());
    click(w, Action::Begin);
    assert!(take_commands(w).is_empty());
    w.resource_mut::<Code>().unwrap().projects.push(Project {
        id: "one".into(),
        title: "One".into(),
        begin: false,
        resume: true,
    });
    click(w, Action::Begin);
    let c = take_commands(w).pop().unwrap();
    assert!(matches!(
        &c.operation,
        Operation::Begin { resume: true, .. }
    ));
    apply(
        w,
        CodeUpdate {
            command: c,
            result: Err("unknown outcome".into()),
        },
    );
    click(w, Action::Begin);
    assert!(take_commands(w).is_empty());
}
#[test]
fn heartbeat_loss_revokes_local_save_authority_not_drafts() {
    let mut a = fixture();
    let w = a.world_mut();
    let k = key("one", "a.rs");
    load(w, "one", "a.rs", "base", "v1");
    edit(w, &k, "draft");
    session(w, "one");
    w.resource_mut::<Code>()
        .unwrap()
        .sessions
        .get_mut("one")
        .unwrap()
        .checked = Instant::now() - Duration::from_secs(21);
    sync(w);
    let c = take_commands(w).pop().unwrap();
    assert!(matches!(c.operation, Operation::Heartbeat { .. }));
    apply(
        w,
        CodeUpdate {
            command: c,
            result: Err("lease revoked".into()),
        },
    );
    click(w, Action::Save);
    assert!(take_commands(w).is_empty());
    assert_eq!(w.resource::<Code>().unwrap().buffers[&k].content, "draft");
}
#[test]
fn stale_updates_do_not_clear_live_request() {
    let mut a = fixture();
    let w = a.world_mut();
    activate(w);
    let c = take_commands(w).pop().unwrap();
    let mut stale = c.clone();
    stale.serial += 1;
    apply(
        w,
        CodeUpdate {
            command: stale,
            result: Ok(json!([])),
        },
    );
    assert_eq!(w.resource::<Code>().unwrap().pending["projects"], c.serial);
}
#[test]
fn generated_routes_encode_identity_as_one_path_component() {
    assert_eq!(
        route(ops::FORGE_ITEMS_BY_WORK_ID_SOURCE_GET, "a/b"),
        "/v1/forge/items/a%2Fb/source"
    );
}
#[test]
fn source_reads_wait_for_composition_and_switching_is_blocked() {
    let mut a = fixture();
    let w = a.world_mut();
    let k = key("one", "a.rs");
    load(w, "one", "a.rs", "base", "v1");
    click(w, Action::Check);
    let c = take_commands(w).pop().unwrap();
    let editor = w.resource::<Code>().unwrap().views[&k].handle.editor;
    w.insert_resource(Focus {
        entity: Some(editor),
    });
    let mut input = Input::default();
    input.ime_preedit = "漢".into();
    w.insert_resource(input);
    rndr_core::editing::prepare_source_input(w);
    assert!(!can_deactivate(w));
    click(w, Action::Project("two".into()));
    assert_eq!(
        w.resource::<Code>().unwrap().selected.as_deref(),
        Some("one")
    );
    apply(
        w,
        CodeUpdate {
            command: c,
            result: Ok(file("one", "a.rs", "latest", "v2")),
        },
    );
    assert_eq!(w.resource::<Code>().unwrap().deferred.len(), 1);
    assert_eq!(w.resource::<Code>().unwrap().buffers[&k].content, "base");
    w.resource_mut::<Input>().unwrap().ime_preedit.clear();
    rndr_core::editing::prepare_source_input(w);
    sync(w);
    assert!(can_deactivate(w));
    assert!(w.resource::<Code>().unwrap().deferred.is_empty());
    assert_eq!(w.resource::<Code>().unwrap().buffers[&k].content, "latest");
}
/// Exercise the real SDK HTTP transport against an isolated loopback fixture,
/// never a user's daemon, project or execution lease.
#[tokio::test]
async fn sdk_wire_uses_human_executor_fenced_put_and_empty_heartbeat() {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let mut requests = Vec::new();
        for reply in [
            json!({"lease":{"work_id":"fixture","lease_id":"human","generation":8}}).to_string(),
            file("fixture", "src/a b.rs", "saved", "v2").to_string(),
            String::new(),
        ] {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut buf = [0; 4096];
            let end = loop {
                let n = stream.read(&mut buf).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buf[..n]);
                if let Some(i) = bytes.windows(4).position(|s| s == b"\r\n\r\n") {
                    break i + 4;
                }
            };
            let header = String::from_utf8_lossy(&bytes[..end]).into_owned();
            let len: usize = header
                .lines()
                .find_map(|s| {
                    s.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .and_then(|s| s.trim().parse().ok())
                })
                .unwrap_or(0);
            while bytes.len() < end + len {
                let n = stream.read(&mut buf).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buf[..n]);
            }
            let body: Value = serde_json::from_slice(&bytes[end..end + len]).unwrap();
            requests.push((header.lines().next().unwrap().to_owned(), body));
            write!(stream,"HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",if reply.is_empty(){"204 No Content"}else{"200 OK"},reply.len(),reply).unwrap();
        }
        requests
    });
    let client = MedousaClient::new(format!("http://{address}"));
    let begin = execute(
        &client,
        CodeCommand {
            serial: 1,
            operation: Operation::Begin {
                work: "fixture".into(),
                resume: false,
            },
        },
    )
    .await;
    assert!(begin.result.is_ok());
    let lease = Lease {
        id: "human".into(),
        generation: 8,
    };
    let save = execute(
        &client,
        CodeCommand {
            serial: 2,
            operation: Operation::Save {
                key: key("fixture", "src/a b.rs"),
                content: "saved".into(),
                digest: "v1".into(),
                lease: lease.clone(),
            },
        },
    )
    .await;
    assert!(save.result.is_ok());
    let beat = execute(
        &client,
        CodeCommand {
            serial: 3,
            operation: Operation::Heartbeat {
                work: "fixture".into(),
                lease,
            },
        },
    )
    .await;
    assert!(beat.result.is_ok());
    let requests = server.join().unwrap();
    assert_eq!(
        requests[0].0,
        "POST /v1/forge/items/fixture/attempts HTTP/1.1"
    );
    assert_eq!(requests[0].1["executor"]["kind"], "human");
    assert_eq!(requests[1].0, "PUT /v1/forge/items/fixture/source HTTP/1.1");
    assert_eq!(
        requests[1].1,
        json!({"path":"src/a b.rs","content":"saved","expected_digest":"v1","lease_id":"human","generation":8})
    );
    assert_eq!(
        requests[2].0,
        "POST /v1/forge/leases/human/heartbeat HTTP/1.1"
    );
    assert_eq!(requests[2].1, json!({"generation":8}));
}

#[test]
fn leases_arm_idle_deadline_and_cancel_it_while_heartbeat_is_pending() {
    let mut app = fixture();
    session(app.world_mut(), "work");
    sync(app.world_mut());
    assert!(app.next_deadline().is_some());
    assert!(app.world().resource::<Code>().unwrap().lease_wake.is_some());
    app.world_mut()
        .resource_mut::<Code>()
        .unwrap()
        .sessions
        .get_mut("work")
        .unwrap()
        .checked = Instant::now() - Duration::from_secs(21);
    sync(app.world_mut());
    let command = take_commands(app.world_mut())
        .into_iter()
        .find(|c| matches!(c.operation, Operation::Heartbeat { .. }))
        .unwrap();
    assert!(app.world().resource::<Code>().unwrap().lease_wake.is_none());
    apply(
        app.world_mut(),
        CodeUpdate {
            command,
            result: Ok(Value::Null),
        },
    );
    assert!(app.world().resource::<Code>().unwrap().lease_wake.is_some());
    app.world_mut()
        .resource_mut::<Code>()
        .unwrap()
        .sessions
        .clear();
    sync(app.world_mut());
    assert!(app.world().resource::<Code>().unwrap().lease_wake.is_none());
}

#[test]
fn dense_source_geometry_and_live_token_colors_preserve_editor_identity() {
    let mut a = fixture();
    let w = a.world_mut();
    load(w, "one", "src/lib.rs", "fn main() { let x = 42; }", "d1");
    let k = key("one", "src/lib.rs");
    let c = w.resource::<Code>().unwrap();
    let view = c.views[&k].handle;
    let editors = c.editors.unwrap();
    let rows = c.rows.unwrap();
    assert_eq!(w.get::<Layout>(editors).unwrap().padding, 0.);
    assert_eq!(w.get::<Layout>(editors).unwrap().gap, 0.);
    assert_eq!(w.get::<Layout>(view.root).unwrap().height, Size::Fill);
    assert_eq!(w.get::<Layout>(rows).unwrap().gap, 0.);
    assert_eq!(
        w.get::<Layout>(w.get::<Parent>(rows).unwrap().entity)
            .unwrap()
            .width,
        Size::Px(300.)
    );
    assert!(!w.get::<Spans>(view.editor).unwrap().runs.is_empty());
    edit(w, &k, "// updated\nlet value = 9;");
    assert_eq!(w.resource::<Code>().unwrap().views[&k].handle, view);
    let runs = &w.get::<Spans>(view.editor).unwrap().runs;
    let original = code_token_spans("// updated\nlet value = 9;");
    assert_eq!(
        runs.iter().map(|r| (r.start, r.end)).collect::<Vec<_>>(),
        original
            .iter()
            .map(|r| (r.start, r.end))
            .collect::<Vec<_>>()
    );
    let palette = rndr_ui::UiTheme::default().markdown_theme();
    assert_eq!(runs[0].color, palette.comment);
    assert_eq!(runs[1].color, palette.keyword);
    assert_eq!(runs[2].color, palette.number);
    let document = w.resource::<Code>().unwrap().views[&k].document;
    let mut theme = rndr_ui::UiTheme::default();
    theme.appearance = rndr_ui::Appearance::Dark;
    rndr_ui::apply_theme_bridge(w, theme.clone());
    refresh_appearance(w);
    assert_eq!(
        w.get::<Spans>(view.editor).unwrap().runs[0].color,
        theme.markdown_theme().comment
    );
    assert_eq!(w.resource::<Code>().unwrap().views[&k].document, document);
    let style = &w.get::<UiRecipe>(view.editor).unwrap().0;
    assert_eq!(style.corner, Some(Corner::Square));
    assert_eq!(style.padding, Some(rndr_ui::Space::Zero));
}

#[test]
fn explorer_rows_are_compact_clipped_and_use_supported_icons() {
    let mut a = fixture();
    let w = a.world_mut();
    load(w, "one", "src/lib.rs", "one", "d1");
    {
        let c = w.resource_mut::<Code>().unwrap();
        c.files.insert("one".into(), vec!["src/lib.rs".into()]);
        c.expanded.insert(("one".into(), "src".into()));
        c.redraw = true;
    }
    sync(w);
    let c = w.resource::<Code>().unwrap();
    let mut labels = Vec::new();
    fn collect(w: &World, e: Entity, labels: &mut Vec<Entity>) {
        if w.has::<Label>(e) {
            labels.push(e);
        }
        for child in children_of(w, e) {
            collect(w, child, labels);
        }
    }
    collect(w, c.rows.unwrap(), &mut labels);
    let file = labels
        .iter()
        .find(|e| w.get::<Label>(**e).unwrap().text.contains("lib.rs"))
        .unwrap();
    assert_eq!(
        w.get::<Label>(*file).unwrap().overflow,
        TextOverflow::Ellipsis
    );
    assert!(
        w.get::<Label>(*file)
            .unwrap()
            .text
            .contains(Icon::FileCode.scalar())
    );
    // An expanded folder turns its one right-pointing chevron a quarter.
    assert!(labels.iter().any(|e| {
        w.get::<Label>(*e).unwrap().text == Icon::ChevronRight.scalar().to_string()
            && w.get::<Transform>(*e)
                .is_some_and(|t| (t.rotate - std::f32::consts::FRAC_PI_2).abs() < 1e-4)
    }));
    assert!(!labels.iter().any(|e| {
        w.get::<Label>(*e)
            .unwrap()
            .text
            .contains(Icon::ChevronDown.scalar())
    }));
}

#[test]
fn source_tabs_are_retained_and_hide_when_project_has_no_file() {
    let mut a = fixture();
    let w = a.world_mut();
    load(w, "one", "a.rs", "a", "d1");
    load(w, "one", "b.rs", "b", "d2");
    let c = w.resource::<Code>().unwrap();
    let strip = c.strip.unwrap();
    let first = c.views[&key("one", "a.rs")].id.0;
    let second = c.views[&key("one", "b.rs")].id.0;
    assert!(strip.tab(w, TabId(first)).is_some());
    assert_eq!(strip.selected(w), Some(TabId(second)));
    assert!(w.has::<Hidden>(strip.body));
    click(w, Action::File("a.rs".into()));
    assert_eq!(
        w.resource::<Code>().unwrap().strip.unwrap().root,
        strip.root
    );
    assert_eq!(strip.selected(w), Some(TabId(first)));
    assert!(take_commands(w).is_empty());
    click(w, Action::Project("empty".into()));
    assert!(w.has::<Hidden>(w.resource::<Code>().unwrap().tabs.unwrap()));
}

#[test]
fn tab_activation_routes_only_owned_strip_and_preserves_drafts_without_reads() {
    let mut a = fixture();
    load(a.world_mut(), "one", "a.rs", "a", "d1");
    edit(a.world_mut(), &key("one", "a.rs"), "draft a");
    load(a.world_mut(), "one", "b.rs", "b", "d2");
    // Publish the initial RNDR selection before injecting a later user request.
    for _ in 0..2 {
        a.update();
        sync(a.world_mut());
    }
    let c = a.world().resource::<Code>().unwrap();
    let strip = c.strip.unwrap();
    let id = TabId(c.views[&key("one", "a.rs")].id.0);
    let other = a.world_mut().spawn((Layout::row(),));
    send(
        a.world_mut(),
        TabStripEvent {
            strip: other,
            kind: TabStripEventKind::Activated(id),
        },
    );
    a.update();
    sync(a.world_mut());
    assert_eq!(
        a.world().resource::<Code>().unwrap().key(),
        Some(key("one", "b.rs"))
    );
    send(
        a.world_mut(),
        TabStripEvent {
            strip: strip.root,
            kind: TabStripEventKind::Activated(id),
        },
    );
    a.update();
    sync(a.world_mut());
    let c = a.world().resource::<Code>().unwrap();
    assert_eq!(c.key(), Some(key("one", "a.rs")));
    assert_eq!(c.buffers[&key("one", "a.rs")].content, "draft a");
    assert_eq!(strip.selected(a.world()), Some(id));
    assert!(take_commands(a.world_mut()).is_empty());
    send(
        a.world_mut(),
        TabStripEvent {
            strip: strip.root,
            kind: TabStripEventKind::CloseRequested(id),
        },
    );
    a.update();
    sync(a.world_mut());
    let c = a.world().resource::<Code>().unwrap();
    assert_eq!(c.views.len(), 2); // Closing is never a document/draft discard.
    assert!(!c.open_tabs.contains(&key("one", "a.rs")));
    assert_eq!(c.key(), Some(key("one", "b.rs")));
    assert_eq!(c.buffers[&key("one", "a.rs")].content, "draft a");
    // A subsequent tree click occurs in a later frame, after the close request.
    for _ in 0..2 {
        a.update();
        sync(a.world_mut());
    }
    click(a.world_mut(), Action::File("a.rs".into()));
    assert!(take_commands(a.world_mut()).is_empty());
    assert_eq!(
        a.world().resource::<Code>().unwrap().key(),
        Some(key("one", "a.rs"))
    );
}

#[test]
fn source_tab_activation_during_composition_keeps_current_document() {
    let mut a = fixture();
    load(a.world_mut(), "one", "a.rs", "a", "d1");
    load(a.world_mut(), "one", "b.rs", "b", "d2");
    let w = a.world_mut();
    let c = w.resource::<Code>().unwrap();
    let strip = c.strip.unwrap();
    let first = TabId(c.views[&key("one", "a.rs")].id.0);
    let current = c.views[&key("one", "b.rs")].handle.editor;
    w.insert_resource(Focus {
        entity: Some(current),
    });
    let mut input = Input::default();
    input.ime_preedit = "漢".into();
    w.insert_resource(input);
    rndr_core::editing::prepare_source_input(w);
    assert!(!can_deactivate(w));
    send(
        w,
        TabStripEvent {
            strip: strip.root,
            kind: TabStripEventKind::Activated(first),
        },
    );
    // Flip the queued tab event into this frame, then establish preedit
    // again before the host adapter reads the activation request.
    a.update();
    let w = a.world_mut();
    w.insert_resource(Focus {
        entity: Some(current),
    });
    let mut input = Input::default();
    input.ime_preedit = "漢".into();
    w.insert_resource(input);
    rndr_core::editing::prepare_source_input(w);
    assert_eq!(events::<TabStripEvent>(w).len(), 1);
    sync(w);
    assert_eq!(
        w.resource::<Code>().unwrap().key(),
        Some(key("one", "b.rs"))
    );
    assert_eq!(w.resource::<Focus>().unwrap().entity, Some(current));
}

#[test]
#[ignore = "Synthetic Code offscreen acceptance; writes verification PNGs"]
fn code_visual_acceptance() {
    use std::sync::Arc;
    let mut app = crate::ui::app();
    app.update();
    crate::surfaces::select(app.world_mut(), crate::surfaces::SurfaceId::Code);
    rndr_ui::apply_theme_bridge(
        app.world_mut(),
        rndr_ui::UiTheme {
            appearance: rndr_ui::Appearance::Dark,
            density: rndr_ui::Density::Compact,
            ..Default::default()
        },
    );
    let engine: Arc<dyn TextEngine> = Arc::new(rndr_render::ParleyText::new());
    app.world_mut().insert_resource(engine);
    let w = app.world_mut();
    let c = w.resource_mut::<Code>().unwrap();
    c.projects = vec![Project {
        id: "one".into(),
        title: "do proper migration".into(),
        begin: true,
        resume: false,
    }];
    c.repositories.insert(
        "one".into(),
        (
            "/workshop/Medousa".into(),
            "Medousa".into(),
            "working".into(),
        ),
    );
    c.files.insert(
        "one".into(),
        [
            ".cargo/config.toml",
            ".cursor/settings.json",
            ".github/workflows/ci.yml",
            "apps/medousa-native/src/main.rs",
            "apps/medousa-native/src/code.rs",
            "apps/medousa-native/src/notes.rs",
            "apps/medousa-native/src/ui.rs",
            "apps/medousa-native/Cargo.toml",
            "crates/medousa-app-core/src/lib.rs",
            "AGENTS.md",
            "Cargo.toml",
            "README.md",
        ]
        .into_iter()
        .map(String::from)
        .collect(),
    );
    for p in ["apps", "apps/medousa-native", "apps/medousa-native/src"] {
        c.expanded.insert(("one".into(), p.into()));
    }
    load(
        w,
        "one",
        "apps/medousa-native/src/main.rs",
        "// Medousa native desktop\nuse rndr_core::*;\n\nfn main() {\n    let mut app = ui::app();\n    rndr_render::run(app);\n}\n",
        "v1",
    );
    load(
        w,
        "one",
        "apps/medousa-native/src/code.rs",
        include_str!("code.rs"),
        "v2",
    );
    // The fixture supplies its own catalog instead of completing the live request.
    app.world_mut()
        .resource_mut::<Code>()
        .unwrap()
        .pending
        .remove("projects");
    let mut exporter = rndr_render::Exporter::new().unwrap();
    std::fs::create_dir_all("verification").unwrap();
    for (width, height, suffix) in [
        (1200, 850, "wide"),
        (760, 540, "narrow"),
        (1200, 850, "light"),
        (760, 540, "light-narrow"),
        (760, 650, "light-zoom"),
    ] {
        publish_system_appearance(
            app.world_mut(),
            SystemAppearance {
                dark: Some(!suffix.starts_with("light")),
                ..Default::default()
            },
        );
        app.world_mut()
            .resource_mut::<rndr_ui::UiTheme>()
            .unwrap()
            .reading_scale = if suffix.ends_with("zoom") { 1.5 } else { 1. };
        app.world_mut()
            .insert_resource(Viewport::new(width as f32, height as f32));
        for _ in 0..4 {
            rndr_paint::frame(&mut app);
        }
        let (_, image) = rndr_paint::frame_with_snapshot_frame(&mut app, |world, frame| {
            exporter
                .render_frame(Some(world), frame, width, height, 1.)
                .unwrap()
        });
        image::save_buffer(
            format!("verification/native-code-polish-{suffix}.png"),
            &image.rgba,
            image.width,
            image.height,
            image::ColorType::Rgba8,
        )
        .unwrap();
        for (mode, action) in [
            ("projects", Action::ChooseProjects),
            ("undertakings", Action::ChooseUndertakings),
        ] {
            click(app.world_mut(), action);
            for _ in 0..4 {
                rndr_paint::frame(&mut app);
            }
            let (_, image) = rndr_paint::frame_with_snapshot_frame(&mut app, |world, frame| {
                exporter
                    .render_frame(Some(world), frame, width, height, 1.)
                    .unwrap()
            });
            image::save_buffer(
                format!("verification/native-code-{mode}-{suffix}.png"),
                &image.rgba,
                image.width,
                image.height,
                image::ColorType::Rgba8,
            )
            .unwrap();
            click(app.world_mut(), Action::ClosePicker);
        }
    }
}

#[test]
fn integrated_code_band_retains_editor_and_fits_actions() {
    let mut app = crate::ui::app();
    app.update();
    crate::surfaces::select(app.world_mut(), crate::surfaces::SurfaceId::Code);
    load(app.world_mut(), "one", "a.rs", "fn main() {}", "v1");
    load(app.world_mut(), "one", "b.rs", "fn other() {}", "v2");
    session(app.world_mut(), "one");
    edit(app.world_mut(), &key("one", "b.rs"), "fn local_draft() {}");
    let document = app.world().resource::<Code>().unwrap().views[&key("one", "b.rs")].document;
    for width in [1200., 760., 1600.] {
        app.world_mut().insert_resource(Viewport::new(width, 850.));
        for _ in 0..8 {
            app.update();
        }
        let w = app.world();
        let c = w.resource::<Code>().unwrap();
        let strip = c.strip.unwrap();
        assert!(w.has::<Hidden>(strip.root));
        for tab in children_of(w, strip.list) {
            if w.has::<Tab>(tab) {
                let r = w.get::<Rect>(tab).unwrap();
                assert!(r.y >= 0. && r.y + r.h <= crate::ui::chrome::HEIGHT);
            }
        }
        let toolbar = c.toolbar.unwrap();
        assert!(w.get::<Rect>(toolbar).unwrap().y >= crate::ui::chrome::HEIGHT);
        let actions = children_of(w, toolbar)[0];
        for child in children_of(w, actions) {
            let r = w.get::<Rect>(child).unwrap();
            assert!(r.x + r.w <= width + 1., "code action clipped: {r:?}");
        }
        // One header row at every width; the file name survives narrowing.
        assert_eq!(w.get::<Rect>(actions).unwrap().h, 40.);
        let crumb = w
            .query::<crate::native_style::PathCrumb>()
            .iter()
            .map(|(e, _)| *e)
            .next()
            .unwrap();
        assert!(w.get::<Label>(crumb).unwrap().text.ends_with("b.rs"));
    }
    crate::surfaces::select(app.world_mut(), crate::surfaces::SurfaceId::Notes);
    for _ in 0..3 {
        app.update();
    }
    crate::surfaces::select(app.world_mut(), crate::surfaces::SurfaceId::Code);
    for _ in 0..3 {
        app.update();
    }
    assert_eq!(
        app.world().resource::<Code>().unwrap().views[&key("one", "b.rs")].document,
        document
    );
}

#[test]
fn switching_to_unopened_project_hides_previous_band() {
    let mut app = crate::ui::app();
    app.update();
    crate::surfaces::select(app.world_mut(), crate::surfaces::SurfaceId::Code);
    load(app.world_mut(), "one", "a.rs", "fn main() {}", "v1");
    for _ in 0..4 {
        app.update();
    }
    let strip = app.world().resource::<Code>().unwrap().strip.unwrap();
    let bar = app.world().get::<Parent>(strip.scroll).unwrap().entity;
    click(app.world_mut(), Action::Project("empty".into()));
    for _ in 0..4 {
        app.update();
    }
    assert!(app.world().has::<Hidden>(bar));
    click(app.world_mut(), Action::Project("one".into()));
    for _ in 0..4 {
        app.update();
    }
    assert!(!app.world().has::<Hidden>(bar));
}

/// Production text engine + integrated shell; CPU update/layout/paint only.
#[test]
#[ignore = "explicit performance measurement"]
fn loaded_code_performance_probe() {
    use std::sync::Arc;
    let mut app = crate::ui::app();
    let engine: Arc<dyn TextEngine> = Arc::new(rndr_render::ParleyText::new());
    app.world_mut().insert_resource(engine);
    app.world_mut().insert_resource(Viewport::new(1200., 850.));
    app.update();
    crate::surfaces::select(app.world_mut(), crate::surfaces::SurfaceId::Code);
    load(
        app.world_mut(),
        "one",
        "code.rs",
        include_str!("code.rs"),
        "v1",
    );
    let k = key("one", "code.rs");
    let view = &app.world().resource::<Code>().unwrap().views[&k];
    let (document, editor) = (view.document, view.handle.editor);
    for _ in 0..10 {
        rndr_paint::frame(&mut app);
    }
    for mode in ["settled", "scroll", "typing"] {
        let mut samples = Vec::new();
        for i in 0..40 {
            let start = Instant::now();
            if mode == "scroll" {
                app.world_mut().get_mut::<Editor>(editor).unwrap().offset = i as f32 * 18.;
            } else if mode == "typing" {
                let d = app
                    .world_mut()
                    .resource_mut::<SourceDocuments>()
                    .unwrap()
                    .get_mut(document)
                    .unwrap();
                let end = d.text().len();
                d.commit(editing::SourceBatch {
                    document,
                    base: d.revision(),
                    origin: editing::EditOrigin::Host(99),
                    changes: vec![editing::SourceChange {
                        range: end..end,
                        replacement: "x".into(),
                    }],
                })
                .unwrap();
            }
            rndr_paint::frame(&mut app);
            samples.push(start.elapsed().as_secs_f64() * 1000.);
        }
        samples.sort_by(f64::total_cmp);
        eprintln!(
            "loaded Code / Parley {mode}: median={:.3}ms p95={:.3}ms",
            samples[20], samples[38]
        );
    }
}

#[test]
fn code_keyboard_is_surface_scoped_and_consumes_only_handled_primary_chords() {
    use crate::surfaces::{self, SurfaceId};
    for platform in [Platform::Mac, Platform::Windows] {
        let mut a = crate::ui::app();
        a.update(); // Startup mounts the surface controls before file actions.
        let w = a.world_mut();
        w.insert_resource(platform);
        load(w, "one", "a.rs", "saved", "v1");
        let modifiers = Modifiers {
            super_key: platform == Platform::Mac,
            ctrl: platform != Platform::Mac,
            ..Default::default()
        };
        w.resource_mut::<Input>().unwrap().keys = vec![KeyEvent::character('f', modifiers)];
        keyboard(w);
        assert_eq!(
            w.resource::<Input>().unwrap().keys.len(),
            1,
            "Chat keeps its keyboard event"
        );
        assert!(
            w.resource::<Code>().unwrap().views[&key("one", "a.rs")]
                .find
                .is_none()
        );
        surfaces::select(w, SurfaceId::Code);
        w.resource_mut::<Input>().unwrap().ime_preedit = "漢".into();
        keyboard(w);
        assert_eq!(
            w.resource::<Input>().unwrap().keys.len(),
            1,
            "Composition must not open Find"
        );
        w.resource_mut::<Input>().unwrap().ime_preedit.clear();
        w.resource_mut::<Input>()
            .unwrap()
            .keys
            .push(KeyEvent::character('x', Modifiers::default()));
        keyboard(w);
        assert_eq!(w.resource::<Input>().unwrap().keys.len(), 1);
        assert!(
            w.resource::<Code>().unwrap().views[&key("one", "a.rs")]
                .find
                .is_some()
        );
        w.resource_mut::<Input>().unwrap().keys = vec![KeyEvent::character('p', modifiers)];
        keyboard(w);
        assert!(w.resource::<Input>().unwrap().keys.is_empty());
        edit(w, &key("one", "a.rs"), "dirty");
        session(w, "one");
        take_commands(w);
        w.resource_mut::<Input>().unwrap().keys = vec![KeyEvent::character('s', modifiers)];
        keyboard(w);
        assert!(w.resource::<Input>().unwrap().keys.is_empty());
        assert!(
            take_commands(w)
                .iter()
                .any(|c| matches!(c.operation, Operation::Save { .. }))
        );
    }
}
#[test]
fn explorer_query_keeps_dirty_buffers_and_active_project() {
    let mut a = fixture();
    let w = a.world_mut();
    load(w, "one", "a.rs", "saved", "v1");
    edit(w, &key("one", "a.rs"), "dirty");
    w.resource_mut::<Code>().unwrap().explorer_query = "no matching project".into();
    sync(w);
    let c = w.resource::<Code>().unwrap();
    assert_eq!(c.buffers[&key("one", "a.rs")].content, "dirty");
    assert_eq!(c.selected.as_deref(), Some("one"));
}
#[test]
fn scoped_shortcuts_require_a_selected_document_and_reuse_canonical_find() {
    let mut a = fixture();
    assert!(!shortcut(a.world_mut(), "find"));
    assert!(!shortcut(a.world_mut(), "quick-open"));
    load(a.world_mut(), "one", "a.rs", "a", "d1");
    assert!(shortcut(a.world_mut(), "find"));
    let c = a.world().resource::<Code>().unwrap();
    assert!(c.views[&key("one", "a.rs")].find.is_some());
    assert!(!shortcut(a.world_mut(), "unknown"));
    assert!(shortcut(a.world_mut(), "quick-open"));
    assert!(take_commands(a.world_mut()).is_empty());
}

#[test]
fn overflow_menu_offers_secondary_commands_and_routes_picks() {
    let mut app = crate::ui::app();
    app.update();
    crate::surfaces::select(app.world_mut(), crate::surfaces::SurfaceId::Code);
    load(app.world_mut(), "one", "a.rs", "fn main() {}", "v1");
    for _ in 0..4 {
        app.update();
    }
    let w = app.world_mut();
    take_commands(w);
    let c = w.resource::<Code>().unwrap();
    let labels: Vec<_> = c.more.as_ref().unwrap().1.iter().map(|x| x.0).collect();
    // Direct icon actions are not repeated in the overflow.
    for direct in ["Search", "Changes", "Problems"] {
        assert!(!labels.contains(&direct), "{direct} duplicated");
    }
    for rare in ["Open file", "Tasks", "Outline", "Find uses", "Find in file"] {
        assert!(labels.contains(&rare), "{rare} missing");
    }
    let more = *c
        .actions
        .iter()
        .find(|(_, a)| matches!(a, Action::More))
        .unwrap()
        .0;
    assert!(action(w, &HostIntent::Command(more)));
    let (menu, target, items) = w.resource::<Code>().unwrap().context_menu.clone().unwrap();
    assert!(menu.is_open(w));
    assert_eq!(target, key("one", "a.rs"));
    let tasks = *items
        .iter()
        .find(|(_, a)| matches!(a, Action::Workbench(workbench::Action::Tasks)))
        .unwrap()
        .0;
    send(
        w,
        rndr_ui::menu::MenuPicked {
            menu: menu.root,
            item: tasks,
        },
    );
    app.update();
    let c = app.world().resource::<Code>().unwrap();
    assert!(c.context_menu.is_none(), "pick not consumed");
    assert!(
        c.pending.contains_key("workbench:one"),
        "{:?}",
        c.pending.keys()
    );
}

#[test]
#[ignore = "Synthetic terminal dock GPU acceptance; no daemon or live shells"]
fn terminal_visual_acceptance() {
    let mut app = crate::ui::app();
    app.update();
    crate::surfaces::select(app.world_mut(), crate::surfaces::SurfaceId::Code);
    let engine: std::sync::Arc<dyn TextEngine> =
        std::sync::Arc::new(rndr_render::ParleyText::new());
    app.world_mut().insert_resource(engine);
    let w = app.world_mut();
    let c = w.resource_mut::<Code>().unwrap();
    c.projects = vec![Project {
        id: "one".into(),
        title: "Native terminal integration".into(),
        begin: true,
        resume: false,
    }];
    c.repositories.insert(
        "one".into(),
        (
            "/workshop/Medousa".into(),
            "Medousa".into(),
            "working".into(),
        ),
    );
    c.files.insert(
        "one".into(),
        vec![
            "src/main.rs".into(),
            "src/terminal.rs".into(),
            "Cargo.toml".into(),
        ],
    );
    c.expanded.insert(("one".into(), "src".into()));
    c.pending.remove("projects");
    load(
        w,
        "one",
        "src/terminal.rs",
        "// Workshop-owned interactive terminal\nuse rndr_core::*;\n\nfn main() {\n    // The workshop owns the shell and working directory.\n    // This device renders the authenticated PTY stream.\n}\n",
        "d1",
    );
    let terminal = crate::terminal::tests::preview(w, "one");
    let mut exporter = rndr_render::Exporter::new().unwrap();
    std::fs::create_dir_all("verification").unwrap();
    for (width, height, suffix) in [
        (1200, 850, "dark"),
        (760, 650, "narrow"),
        (1200, 850, "light"),
    ] {
        publish_system_appearance(
            app.world_mut(),
            SystemAppearance {
                dark: Some(suffix != "light"),
                ..Default::default()
            },
        );
        app.world_mut()
            .insert_resource(Viewport::new(width as f32, height as f32));
        for _ in 0..8 {
            rndr_paint::frame(&mut app);
        }
        let rect = app.world().get::<Rect>(terminal).unwrap();
        assert!(rect.w > 150. && rect.h > 90., "terminal rect {rect:?}");
        assert!(rect.y + rect.h <= height as f32 + 1.);
        let (_, image) = rndr_paint::frame_with_snapshot_frame(&mut app, |world, frame| {
            exporter
                .render_frame(Some(world), frame, width, height, 1.)
                .unwrap()
        });
        image::save_buffer(
            format!("verification/native-terminal-{suffix}.png"),
            &image.rgba,
            image.width,
            image.height,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}
#[test]
fn status_line_follows_the_caret_and_names_the_language_only_with_a_file_open() {
    let mut app = fixture();
    let line = app.world().resource::<Code>().unwrap().status_line.unwrap();
    sync(app.world_mut());
    assert!(app.world().has::<Hidden>(line));
    load(
        app.world_mut(),
        "one",
        "src/a.rs",
        "fn main() {\n    run();\n}\n",
        "d1",
    );
    sync(app.world_mut());
    assert!(!app.world().has::<Hidden>(line));
    // A freshly loaded document leaves the caret at its end.
    assert_eq!(
        app.world().get::<Label>(line).unwrap().text,
        "Ln 4, Col 1 · Rust"
    );
    let id = app.world().resource::<Code>().unwrap().views[&key("one", "src/a.rs")].document;
    // Byte 16 is after "fn main() {\n    " on the second line.
    app.world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap()
        .set_selection(rndr_core::editing::SourceSelection {
            anchor: 16,
            head: 16,
        })
        .unwrap();
    sync(app.world_mut());
    assert_eq!(
        app.world().get::<Label>(line).unwrap().text,
        "Ln 2, Col 5 · Rust"
    );
    assert_eq!(language("notes/README"), None);
    assert_eq!(language("Dockerfile"), Some("Dockerfile"));
}
#[test]
fn closing_a_clean_file_releases_its_editor_document_and_buffer() {
    let mut app = fixture();
    load(app.world_mut(), "one", "a.rs", "fn main() {}", "d1");
    let c = app.world().resource::<Code>().unwrap();
    let k = key("one", "a.rs");
    let document = c.views[&k].document;
    let root = c.views[&k].handle.root;
    let id = TabId(c.views[&k].id.0);
    tab_event(&mut app, TabStripEventKind::CloseRequested(id));
    let c = app.world().resource::<Code>().unwrap();
    assert!(!c.views.contains_key(&k));
    assert!(!c.buffers.contains_key(&k));
    assert!(!app.world().is_alive(root));
    assert!(
        app.world()
            .resource::<SourceDocuments>()
            .unwrap()
            .get(document)
            .is_none()
    );
    // Reopening loads it again rather than finding a stale copy.
    click(app.world_mut(), Action::File("a.rs".into()));
    assert!(
        take_commands(app.world_mut())
            .iter()
            .any(|c| format!("{c:?}").contains("a.rs"))
    );
}

#[test]
fn navigation_code_line_jump_applies_after_load_and_preserves_source() {
    let mut app = fixture();
    let w = app.world_mut();
    let key = key("w", "src/lib.rs");
    let mut c = std::mem::take(w.resource_mut::<Code>().unwrap());
    workbench::navigate_to(w, &mut c, key.clone(), 3);
    *w.resource_mut::<Code>().unwrap() = c;
    load(w, "w", "src/lib.rs", "α\nsecond\nthird", "d1");
    let c = w.resource::<Code>().unwrap();
    let view = &c.views[&key];
    let doc = w
        .resource::<SourceDocuments>()
        .unwrap()
        .get(view.document)
        .unwrap();
    assert_eq!(
        doc.view_selection(view.handle.editor).unwrap().head,
        "α\nsecond\n".len()
    );
    assert_eq!(doc.text(), "α\nsecond\nthird");
    assert!(!c.buffers[&key].dirty());
}
