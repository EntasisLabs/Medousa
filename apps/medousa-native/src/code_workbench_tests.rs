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
    assert!(
        w.resource::<Code>()
            .unwrap()
            .workbench
            .run_uncertain
            .contains("work")
    );
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
