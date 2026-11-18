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
fn search_fields_survive_panel_teardown_and_are_project_scoped() {
    let mut app = fixture();
    let w = app.world_mut();
    act(w, Action::Search);
    let fields = w
        .resource::<Code>()
        .unwrap()
        .workbench
        .search_fields
        .unwrap();
    for (e, text) in fields
        .into_iter()
        .zip(["needle", "src/**/*.rs", "target/**"])
    {
        w.get_mut::<Label>(e).unwrap().text = text.into();
    }
    act(w, Action::SearchToggle(SearchToggle::Regex));
    act(w, Action::SearchToggle(SearchToggle::Case));
    act(w, Action::SearchToggle(SearchToggle::Word));
    act(w, Action::Close);
    act(w, Action::Search);
    let fields = w
        .resource::<Code>()
        .unwrap()
        .workbench
        .search_fields
        .unwrap();
    assert_eq!(w.get::<Label>(fields[0]).unwrap().text, "needle");
    act(w, Action::SubmitSearch);
    let commands = super::super::take_commands(w);
    let Operation::Workbench(Command {
        kind: Kind::Search(options),
        ..
    }) = &commands[0].operation
    else {
        panic!()
    };
    assert_eq!(options.query, "needle");
    assert_eq!(options.include, "src/**/*.rs");
    assert_eq!(options.exclude, "target/**");
    assert!(options.regex && options.case_sensitive && options.whole_word);
    w.resource_mut::<Code>().unwrap().selected = Some("other".into());
    act(w, Action::Search);
    let c = w.resource::<Code>().unwrap();
    assert_eq!(
        w.get::<Label>(c.workbench.search_fields.unwrap()[0])
            .unwrap()
            .text,
        ""
    );
}
#[test]
fn paging_retains_exact_search_options_and_daemon_cursor() {
    let mut app = fixture();
    let w = app.world_mut();
    act(w, Action::Search);
    let field = w
        .resource::<Code>()
        .unwrap()
        .workbench
        .search_fields
        .unwrap()[0];
    w.get_mut::<Label>(field).unwrap().text = "needle".into();
    act(w, Action::SubmitSearch);
    let request = super::super::take_commands(w).remove(0);
    let Operation::Workbench(command) = &request.operation else { panic!() };
    let command = command.clone();
    let mut c = std::mem::take(w.resource_mut::<Code>().unwrap());
    c.pending.remove(&tag(&request.operation));
    let Kind::Search(first) = command.kind.clone() else {
        panic!()
    };
    apply(
        w,
        &mut c,
        command,
        Ok(json!({"work_id":"work","matches":[],"next_cursor":"opaque+/cursor","truncated":true})),
    );
    *w.resource_mut::<Code>().unwrap() = c;
    act(w, Action::MoreSearch);
    let Operation::Workbench(command) = super::super::take_commands(w).remove(0).operation else {
        panic!()
    };
    let Kind::Search(next) = command.kind else {
        panic!()
    };
    assert_eq!(next.query, first.query);
    assert_eq!(next.cursor.as_deref(), Some("opaque+/cursor"));
}
#[test]
fn lagging_run_list_does_not_clear_unknown_admission() {
    let mut app = fixture();
    let w = app.world_mut();
    let mut c = std::mem::take(w.resource_mut::<Code>().unwrap());
    c.workbench.run_uncertain.insert("work".into());
    apply(
        w,
        &mut c,
        Command {
            work: "work".into(),
            kind: Kind::Runs,
        },
        Ok(json!([])),
    );
    assert!(c.workbench.run_uncertain.contains("work"));
    *w.resource_mut::<Code>().unwrap() = c;
    act(
        w,
        Action::RunTest {
            task: "task".into(),
            test: "test".into(),
        },
    );
    assert!(super::super::take_commands(w).is_empty());
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
    assert!(
        w.resource::<Code>()
            .unwrap()
            .workbench
            .run_uncertain
            .contains("work")
    );
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
fn task_and_named_test_starts_respect_pending_and_unknown_admission() {
    for start in [
        Action::Run("test".into()),
        Action::RunTest {
            task: "test".into(),
            test: "named".into(),
        },
    ] {
        let mut app = fixture();
        let w = app.world_mut();
        let c = w.resource_mut::<Code>().unwrap();
        c.workbench.scope = Some("work".into());
        c.sessions.insert(
            "work".into(),
            Session {
                lease: Lease {
                    id: "lease".into(),
                    generation: 3,
                },
                checked: Instant::now(),
            },
        );
        act(w, Action::Tasks);
        let pending = super::super::take_commands(w).pop().unwrap();
        act(w, start.clone());
        assert!(super::super::take_commands(w).is_empty());
        assert!(
            !w.resource::<Code>()
                .unwrap()
                .workbench
                .run_uncertain
                .contains("work")
        );
        super::super::apply(
            w,
            CodeUpdate {
                command: pending,
                result: Ok(json!([])),
            },
        );
        act(w, start.clone());
        let admission = super::super::take_commands(w).pop().unwrap();
        assert!(matches!(
            admission.operation,
            Operation::Workbench(Command {
                kind: Kind::Run { .. },
                ..
            })
        ));
        act(w, start.clone());
        assert!(super::super::take_commands(w).is_empty());
        super::super::apply(
            w,
            CodeUpdate {
                command: admission,
                result: Err("connection lost".into()),
            },
        );
        // A terminal transport error is not proof of an idle daemon task.
        assert!(!busy(w.resource::<Code>().unwrap(), "work"));
        act(w, Action::Runs);
        let history = super::super::take_commands(w).pop().unwrap();
        super::super::apply(
            w,
            CodeUpdate {
                command: history,
                result: Ok(json!({"runs":[]})),
            },
        );
        act(w, start);
        assert!(super::super::take_commands(w).is_empty());
        assert!(
            w.resource::<Code>()
                .unwrap()
                .workbench
                .run_uncertain
                .contains("work")
        );
    }
}

#[test]
fn diff_projection_requires_exact_work_and_unicode_path_identity() {
    for (work, path, accepted) in [
        ("work", "src/é.rs", true),
        ("other", "src/é.rs", false),
        ("work", "src/e.rs", false),
    ] {
        let mut app = fixture();
        let w = app.world_mut();
        let mut c = std::mem::take(w.resource_mut::<Code>().unwrap());
        apply(
            w,
            &mut c,
            Command {
                work: "work".into(),
                kind: Kind::Diff("src/é.rs".into()),
            },
            Ok(json!({"work_id":work,"path":path,"binary":true})),
        );
        let labels: Vec<_> = w
            .query::<Label>()
            .iter()
            .map(|(_, l)| l.text.clone())
            .collect();
        assert_eq!(
            labels
                .iter()
                .any(|s| s == "Binary file · no text diff available."),
            accepted
        );
        assert_eq!(
            labels
                .iter()
                .any(|s| s == "Diff identity mismatch · refresh Changes."),
            !accepted
        );
    }
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
    let (rows, annotations) = crate::diff_presentation::rows(&World::new(), &value);
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[0].marker, DiffMarker::Header);
    assert_eq!(rows[1].marker, DiffMarker::Removed);
    assert_eq!(rows[2].marker, DiffMarker::Added);
    assert!(rows[1].text.contains("old"));
    assert!(rows[2].text.contains("new"));
    assert_eq!(rows[3].text, "");
    assert_eq!(annotations[2].new_line, Some(5));
    assert_eq!(annotations[0].old_line, Some(4));
    assert_eq!(rows[1].text, "old");
    let mut app = fixture();
    let parent = app
        .world()
        .resource::<Code>()
        .unwrap()
        .workbench
        .panel
        .unwrap();
    rndr_ui::diff::DiffView::mount_annotated(
        app.world_mut(),
        parent,
        UiId(next()),
        rndr_ui::diff::DiffProps::new("Native diff"),
        &rows,
        &annotations,
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
    c.open_tab(&key);
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

#[test]
fn actual_code_diff_result_owns_a_bounded_two_axis_viewport() {
    let mut app = crate::ui::app();
    app.update();
    app.world_mut().insert_resource(Viewport::new(1100., 800.));
    crate::surfaces::select(app.world_mut(), crate::surfaces::SurfaceId::Code);
    app.world_mut().resource_mut::<Code>().unwrap().selected = Some("work".into());
    for _ in 0..4 { rndr_paint::frame(&mut app); }
    let w = app.world_mut();
    let mut c = std::mem::take(w.resource_mut::<Code>().unwrap());
    apply(w, &mut c, Command { work: "work".into(), kind: Kind::Diff("src/main.rs".into()) },
        Ok(json!({"work_id":"work","path":"src/main.rs","hunks":[{"old_start":1,"old_count":1,"new_start":1,"new_count":1,
            "lines":[{"kind":"addition","new_line":1,"content":format!("let value = 42; // {} END", "long_".repeat(200))}]}]})));
    *w.resource_mut::<Code>().unwrap() = c;
    for _ in 0..4 { rndr_paint::frame(&mut app); }
    let w = app.world();
    let root = w.query::<Semantics>().into_iter().find(|(_, sem)| sem.name.as_deref() == Some("Changes to src/main.rs")).unwrap().0;
    let rect = w.get::<Rect>(root).unwrap();
    assert!(rect.h > 50. && rect.h < 240., "Diff must fit its results pane: {rect:?}");
    assert_eq!(w.get::<Layout>(root).unwrap().height, Size::Fill);
    assert!(w.get::<Scroll>(root).unwrap().content[0] > rect.w + 500.);
}
