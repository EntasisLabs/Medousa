use super::*;
fn fixture() -> App {
    let mut app = App::new();
    app.add_plugin(rndr_ui::UiPlugin);
    let w = app.world_mut();
    let root = w.spawn((Layout::column(),));
    super::super::mount(w, root);
    w.resource_mut::<Code>().unwrap().selected = Some("work".into());
    app
}
fn act(w: &mut World, a: Action) {
    let mut c = std::mem::take(w.resource_mut::<Code>().unwrap());
    action(w, &mut c, a);
    *w.resource_mut::<Code>().unwrap() = c;
}
#[test]
fn unicode_positions_are_utf16_not_bytes() {
    assert_eq!(position("a\n🦀éx", 8), (1, 3));
    assert_eq!(position("a\n🦀éx", 4), (1, 0));
}
#[test]
fn uri_navigation_stays_inside_exact_daemon_root() {
    assert_eq!(
        file_uri("/work tree/a", "src/é.rs"),
        "file:///work%20tree/a/src/%C3%A9.rs"
    );
    assert_eq!(
        location_path("file:///work%20tree/a/src/%C3%A9.rs", "/work tree/a"),
        Some("src/é.rs".into())
    );
    for uri in [
        "file:///work/a-other/x.rs",
        "file:///work/a/../secret",
        "file:///work/a/x%2fy",
        "https://host/work/a/x",
    ] {
        assert!(location_path(uri, "/work/a").is_none(), "{uri}");
    }
}
#[test]
fn stale_project_response_does_not_rebuild_current_panel() {
    let mut app = fixture();
    let w = app.world_mut();
    let mut c = std::mem::take(w.resource_mut::<Code>().unwrap());
    apply(
        w,
        &mut c,
        Command {
            work: "other".into(),
            kind: Kind::Tasks,
        },
        Ok(json!([])),
    );
    assert!(c.workbench.scope.is_none());
}
#[test]
fn task_start_requires_current_lease_and_blocks_unknown_replay() {
    let mut app = fixture();
    let w = app.world_mut();
    w.resource_mut::<Code>().unwrap().workbench.scope = Some("work".into());
    act(w, Action::Run("test".into()));
    assert!(super::super::take_commands(w).is_empty());
    w.resource_mut::<Code>().unwrap().sessions.insert(
        "work".into(),
        Session {
            lease: Lease {
                id: "lease".into(),
                generation: 3,
            },
            checked: Instant::now(),
        },
    );
    act(w, Action::Run("test".into()));
    let commands = super::super::take_commands(w);
    assert_eq!(commands.len(), 1);
    let command = commands.into_iter().next().unwrap();
    super::super::apply(
        w,
        CodeUpdate {
            command,
            result: Err("connection lost".into()),
        },
    );
    act(w, Action::Run("test".into()));
    assert!(super::super::take_commands(w).is_empty());
    assert!(w
        .resource::<Code>()
        .unwrap()
        .workbench
        .run_uncertain
        .contains("work"));
}
#[test]
fn quick_open_filters_only_current_daemon_tree_and_rejects_unsafe_paths() {
    let mut app = fixture();
    let w = app.world_mut();
    w.resource_mut::<Code>().unwrap().files.insert(
        "work".into(),
        vec!["src/é.rs".into(), "README.md".into(), "../secret.rs".into()],
    );
    w.resource_mut::<Code>()
        .unwrap()
        .files
        .insert("other".into(), vec!["private.rs".into()]);
    act(w, Action::QuickOpen);
    let input = w.resource::<Code>().unwrap().workbench.input.unwrap();
    w.get_mut::<Label>(input).unwrap().text = ".RS".into();
    act(w, Action::FilterFiles);
    let c = w.resource::<Code>().unwrap();
    let paths: Vec<_> = c
        .workbench
        .actions
        .values()
        .filter_map(|a| match a {
            super::super::Action::Workbench(Action::Open { path, .. }) => Some(path.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(paths, vec!["src/é.rs"]);
    assert!(super::super::take_commands(w).is_empty());
}
#[test]
fn commands_are_single_flight_per_project() {
    let mut app = fixture();
    let w = app.world_mut();
    act(w, Action::Tasks);
    act(w, Action::Changes);
    assert_eq!(super::super::take_commands(w).len(), 1);
}
#[test]
fn retained_panel_action_does_not_follow_project_switch() {
    let mut app = fixture();
    let w = app.world_mut();
    w.resource_mut::<Code>().unwrap().workbench.scope = Some("other".into());
    act(w, Action::Output("run".into()));
    assert!(super::super::take_commands(w).is_empty());
}

#[test]
fn daemon_diff_hunks_map_to_canonical_rndr_markers_and_line_numbers() {
    use rndr_ui::diff::DiffMarker;
    let value = json!({"hunks":[{"old_start":4,"old_count":1,"new_start":4,"new_count":2,
        "lines":[{"kind":"deletion","old_line":4,"content":"old\n"},
                 {"kind":"addition","new_line":4,"content":"new\n"},
                 {"kind":"addition","new_line":5,"content":"\n"}]}]});
    let rows = diff_rows(&value);
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[0].marker, DiffMarker::Header);
    assert_eq!(rows[1].marker, DiffMarker::Removed);
    assert_eq!(rows[2].marker, DiffMarker::Added);
    assert!(rows[1].text.contains("old"));
    assert!(rows[2].text.contains("new"));
    assert!(rows[3].text.contains('5'));
    assert!(rows.iter().all(|r| !r.text.trim().is_empty()));
    let mut app = fixture();
    let parent = app
        .world()
        .resource::<Code>()
        .unwrap()
        .workbench
        .panel
        .unwrap();
    rndr_ui::diff::DiffView::mount(
        app.world_mut(),
        parent,
        UiId(next()),
        rndr_ui::diff::DiffProps::new("Native diff"),
        &rows,
    )
    .unwrap();
}

#[test]
fn references_use_saved_file_preconditions_like_other_language_observations() {
    let mut app = fixture();
    let w = app.world_mut();
    let key = ("work".to_owned(), "lib.rs".to_owned());
    let c = w.resource_mut::<Code>().unwrap();
    c.current.insert(key.0.clone(), key.1.clone());
    c.enqueue(Operation::Load(key.clone()));
    let command = super::super::take_commands(w).pop().unwrap();
    super::super::apply(
        w,
        CodeUpdate {
            command,
            result: Ok(
                json!({"work_id":"work", "path":"lib.rs", "content":"fn f() {}",
            "digest":"saved", "encoding":"utf-8", "byte_size":9}),
            ),
        },
    );
    act(w, Action::Language(Feature::References));
    let commands = super::super::take_commands(w);
    assert_eq!(commands.len(), 1);
    assert!(matches!(&commands[0].operation,
        Operation::Workbench(Command { kind: Kind::Language { feature: Feature::References, digest, .. }, .. })
            if digest == "saved"));
    // A dirty buffer must not send a disk-position reference query.
    w.resource_mut::<Code>()
        .unwrap()
        .buffers
        .get_mut(&key)
        .unwrap()
        .content
        .push('x');
    act(w, Action::Language(Feature::References));
    assert!(super::super::take_commands(w).is_empty());
}
