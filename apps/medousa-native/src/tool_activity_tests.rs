use super::*;
#[test]
fn backdrop_hit_target_is_an_activatable_native_button() {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.add_plugin(MarkdownPlugin);
    app.add_plugin(rndr_ui::UiPlugin);
    app.world_mut().insert_resource(Viewport::new(1000., 800.));
    let s = history(vec![run("r1", "read", "completed")], 1);
    launch(app.world_mut(), &s);
    for _ in 0..3 {
        app.update();
    }
    let hit_inside = hit_test(app.world(), 500., 300.).unwrap();
    assert_ne!(
        ancestor_with::<Button>(app.world(), hit_inside)
            .and_then(|b| app.world().get::<Button>(b))
            .map(|b| b.action.as_str()),
        Some("tool-activity-backdrop"),
        "interior hit={hit_inside:?}, rect={:?}",
        app.world().get::<Rect>(hit_inside)
    );
    let hit = hit_test(app.world(), 10., 10.).unwrap();
    let button = ancestor_with::<Button>(app.world(), hit);
    assert!(
        button.is_some(),
        "hit={hit:?} rect={:?} semantics={:?}",
        app.world().get::<Rect>(hit),
        app.world().get::<Semantics>(hit)
    );
    assert_eq!(
        app.world().get::<Button>(button.unwrap()).unwrap().action,
        "tool-activity-backdrop"
    );
    assert_eq!(
        try_activate(app.world_mut(), button.unwrap()),
        ActivationResult::Accepted
    );
    assert!(app.world().entities_with::<Modal>().is_empty());
}
use medousa_app_core::{ChatItem, contract::ChatSegment};
use medousa_types::ToolInputParam;
fn run(id: &str, tool: &str, status: &str) -> ToolRunState {
    ToolRunState {
        run_id: id.into(),
        tool_name: tool.into(),
        status: status.into(),
        round: 1,
        input_summary: Some("Read source".into()),
        input_params: None,
        output_summary: None,
        artifact_refs: None,
    }
}
fn history(runs: Vec<ToolRunState>, revision: u64) -> ChatSnapshot {
    let mut item: ChatItem =
        serde_json::from_value(serde_json::json!({"id":"a", "role":"assistant", "content":""}))
            .unwrap();
    item.tool_runs = Some(runs);
    serde_json::from_value(serde_json::json!({"sessionId":"s", "authorityId":"a", "revision":revision, "items":[item]})).unwrap()
}
fn launch(world: &mut World, snapshot: &ChatSnapshot) -> Entity {
    sync(world, Some(snapshot));
    let parent = world.spawn((Layout::column(),));
    let part = project(&snapshot.items[0])
        .into_iter()
        .find(|p| !p.tools.is_empty())
        .unwrap();
    let row = mount(world, parent, None, "a", &part);
    let button = children_of(world, row)[0];
    world.insert_resource(Focus {
        entity: Some(button),
    });
    assert!(action(
        world,
        &ActionIntent {
            entity: button,
            name: world.get::<Button>(button).unwrap().action.clone()
        }
    ));
    button
}
#[test]
fn summary_matches_tauri_bounded_context_and_status_priority() {
    let runs = vec![
        run("1", "cognition_coder_shell_run", "succeeded"),
        run("2", "cognition_image_view", "completed"),
    ];
    assert_eq!(summary(&runs), "2 tools  Run · Image view");
    assert_eq!(
        summary(&[run("1", "cognition_turn_finish", "completed")]),
        "1 tool"
    );
    assert_eq!(
        summary(&[run("1", "cognition_code_read", "running")]),
        "1 tool  Reading"
    );
    assert_eq!(
        summary(&[run("1", "cognition_code_read", "failed")]),
        "1 tool  Code read"
    );
    assert_eq!(
        subtitle(&[run("1", "a", "running"), run("2", "b", "failed")]),
        "1 running · 2 total"
    );
    assert_eq!(subtitle(&[run("1", "a", "failed")]), "1 failed · 1 total");
    assert_eq!(
        result(&run("1", "a", "failed")),
        "The tool did not return a result summary."
    );
    assert_eq!(
        result(&run("1", "a", "running")),
        "Waiting for the tool to finish…"
    );
}
#[test]
fn adjacent_rounds_group_without_crossing_narrative_or_losing_ids() {
    let mut s = history(
        vec![
            run("r1", "read", "completed"),
            run("r2", "read", "completed"),
            run("r3", "run", "completed"),
        ],
        1,
    );
    s.items[0].segments = Some(vec![
        ChatSegment::ToolGroup {
            group_id: "g1".into(),
            tool_round: 1,
            runs: vec![s.items[0].tool_runs.as_ref().unwrap()[0].clone()],
        },
        ChatSegment::ToolGroup {
            group_id: "g2".into(),
            tool_round: 2,
            runs: vec![s.items[0].tool_runs.as_ref().unwrap()[1].clone()],
        },
        ChatSegment::Text {
            segment_id: "t".into(),
            model_round: None,
            markdown: "Narrative".into(),
            committed: true,
        },
        ChatSegment::ToolGroup {
            group_id: "g3".into(),
            tool_round: 3,
            runs: vec![s.items[0].tool_runs.as_ref().unwrap()[2].clone()],
        },
    ]);
    let parts = project(&s.items[0]);
    let groups: Vec<_> = parts.iter().filter(|p| !p.tools.is_empty()).collect();
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].id, "tool:r1");
    assert_eq!(groups[0].tools.len(), 2);
    assert_eq!(groups[1].id, "tool:r3");
    assert!(
        parts.iter().position(|p| p.id == "text:t").unwrap()
            > parts.iter().position(|p| p.id == "tool:r1").unwrap()
    );
}
#[test]
fn open_inspector_reconciles_live_runs_retains_scroll_and_does_no_idle_remount() {
    let mut world = World::new();
    let mut s = history(vec![run("r1", "cognition_code_read", "running")], 1);
    launch(&mut world, &s);
    let root = world.resource::<Inspector>().unwrap().root.unwrap();
    let body = world.resource::<Inspector>().unwrap().body.unwrap();
    world.get_mut::<Scroll>(body).unwrap().offset[1] = -20.;
    let first = children_of(&world, body);
    for _ in 0..20 {
        sync(&mut world, Some(&s));
    }
    assert_eq!(children_of(&world, body), first);
    s.items[0].tool_runs.as_mut().unwrap()[0].status = "succeeded".into();
    s.items[0].tool_runs.as_mut().unwrap()[0].output_summary = Some("Source loaded".into());
    s.items[0]
        .tool_runs
        .as_mut()
        .unwrap()
        .push(run("r2", "cognition_image_view", "running"));
    s.revision = ChatRevision(2);
    sync(&mut world, Some(&s));
    assert_eq!(world.resource::<Inspector>().unwrap().root, Some(root));
    assert_eq!(children_of(&world, body).len(), 2);
    assert_eq!(world.get::<Scroll>(body).unwrap().offset[1], -20.);
    assert!(
        world
            .query::<MarkdownView>()
            .iter()
            .any(|(_, v)| v.source().contains("Source loaded"))
    );
    let state = world.resource::<Inspector>().unwrap();
    assert_eq!(
        world.get::<Label>(state.subtitle.unwrap()).unwrap().text,
        "1 running · 2 total"
    );
    s.session_id = "other".into();
    sync(&mut world, Some(&s));
    assert!(!world.is_alive(root));
    assert!(world.resource::<Inspector>().unwrap().selected.is_none());
}
#[test]
fn escape_and_backdrop_restore_focus_without_leaking_modal() {
    let mut app = App::new();
    app.add_plugin(Chrome);
    let s = history(vec![run("r1", "read", "completed")], 1);
    let trigger = launch(app.world_mut(), &s);
    app.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(NamedKey::Escape, Modifiers::default())];
    app.update();
    sync(app.world_mut(), Some(&s));
    assert!(app.world().entities_with::<Modal>().is_empty());
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(trigger)
    );
    launch(app.world_mut(), &s);
    let backdrop = app
        .world()
        .query::<Button>()
        .into_iter()
        .find(|(_, b)| b.action == "tool-activity-backdrop")
        .unwrap()
        .0;
    let handler = app.world().get::<ActivationHandler>(backdrop).unwrap().0;
    handler(app.world_mut(), backdrop);
    assert!(app.world().entities_with::<Modal>().is_empty());
}
#[test]
fn native_modal_geometry_scroll_and_parameter_values_are_readable_not_json_wrappers() {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.add_plugin(MarkdownPlugin);
    app.add_plugin(rndr_ui::UiPlugin);
    app.world_mut().insert_resource(Viewport::new(1000., 800.));
    let mut r = run("r1", "cognition_schema", "succeeded");
    r.input_params = Some(vec![
        ToolInputParam {
            key: "intent".into(),
            value: "Load schemas".into(),
            truncated: false,
        },
        ToolInputParam {
            key: "types".into(),
            value: "[\"turn.finish\"]".into(),
            truncated: true,
        },
    ]);
    let s = history(
        (0..25)
            .map(|i| {
                let mut r = r.clone();
                r.run_id = i.to_string();
                r
            })
            .collect(),
        1,
    );
    launch(app.world_mut(), &s);
    for _ in 0..5 {
        app.update();
    }
    let state = app.world().resource::<Inspector>().unwrap();
    let sheet = app.world().get::<Rect>(state.sheet.unwrap()).unwrap();
    assert!((sheet.w - 608.).abs() < 2., "sheet {sheet:?}");
    assert!((sheet.x - 196.).abs() < 2., "sheet {sheet:?}");
    assert!((sheet.h - 640.).abs() < 2., "sheet {sheet:?}");
    let scroll = app.world().get::<Scroll>(state.body.unwrap()).unwrap();
    assert!(scroll.content[1] > 640.);
    assert!(
        app.world()
            .query::<MarkdownView>()
            .iter()
            .any(|(_, v)| v.source().contains("Load schemas"))
    );
    assert!(
        !app.world()
            .query::<MarkdownView>()
            .iter()
            .any(|(_, v)| v.source().contains("truncated"))
    );
    app.world_mut().insert_resource(Viewport::new(400., 500.));
    sync(app.world_mut(), Some(&s));
    for _ in 0..3 {
        app.update();
    }
    let state = app.world().resource::<Inspector>().unwrap();
    assert!(app.world().get::<Rect>(state.sheet.unwrap()).unwrap().w <= 368.);
}
