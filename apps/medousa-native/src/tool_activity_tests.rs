use super::*;
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
/// A timeline frame (transcript + navigation) with one round's summary in
/// it, as the chat tab mounts them, and the panel opened from it.
fn launch(world: &mut World, snapshot: &ChatSnapshot) -> Entity {
    let (_, button) = launch_in(world, snapshot, 0);
    button
}
fn launch_in(world: &mut World, snapshot: &ChatSnapshot, round: usize) -> (Entity, Entity) {
    let tab = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    },));
    let timeline = super::super::setup(world, tab);
    let frame = world.get::<Parent>(timeline).unwrap().entity;
    world.get_mut::<Timeline>(timeline).unwrap().session = snapshot.session_id.clone();
    sync(world, Some(snapshot));
    let part = project(&snapshot.items[0])
        .into_iter()
        .filter(|p| !p.tools.is_empty())
        .nth(round)
        .unwrap();
    let row = mount(world, timeline, None, &snapshot.items[0].id, &part);
    let button = children_of(world, row)[0];
    world.insert_resource(Focus {
        entity: Some(button),
    });
    assert!(click(world, button));
    (frame, button)
}
fn click(world: &mut World, entity: Entity) -> bool {
    let name = world.get::<Button>(entity).unwrap().action.clone();
    action(world, &ActionIntent { entity, name })
}
fn toggle(world: &mut World, run_id: &str) {
    let row = world
        .entities_with::<RunToggle>()
        .into_iter()
        .find(|&e| world.get::<RunToggle>(e).unwrap().0 == run_id)
        .unwrap();
    assert!(click(world, row));
}
fn state(world: &World) -> &Inspector {
    world.resource::<Inspector>().unwrap()
}
fn markdown_has(world: &World, needle: &str) -> bool {
    world
        .query::<MarkdownView>()
        .iter()
        .any(|(_, v)| v.source().contains(needle))
}
fn ui_app(width: f32) -> App {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.add_plugin(MarkdownPlugin);
    app.add_plugin(rndr_ui::UiPlugin);
    app.world_mut().insert_resource(Viewport::new(width, 800.));
    app
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
fn panel_docks_beside_the_transcript_without_a_modal() {
    let mut app = ui_app(1200.);
    let s = history(vec![run("r1", "read", "completed")], 1);
    let (frame, _) = launch_in(app.world_mut(), &s, 0);
    for _ in 0..3 {
        app.update();
        sync(app.world_mut(), Some(&s));
    }
    let w = app.world();
    assert!(w.entities_with::<Modal>().is_empty());
    let panel = state(w).root.unwrap();
    let sash = state(w).sash.unwrap();
    let children = children_of(w, frame);
    assert_eq!(&children[children.len() - 2..], &[sash, panel]);
    let rect = *w.get::<Rect>(panel).unwrap();
    assert!((rect.w - PANEL_DEFAULT).abs() < 2., "panel {rect:?}");
    assert!(rect.x > 700., "panel {rect:?}");
    assert!(!w.has::<Absolute>(panel));
}
#[test]
fn second_click_closes_and_another_round_retargets() {
    let mut world = World::new();
    let mut s = history(
        vec![
            run("r1", "read", "completed"),
            run("r2", "run", "completed"),
        ],
        1,
    );
    split_rounds(&mut s);
    let (frame, first) = launch_in(&mut world, &s, 0);
    let panel = state(&world).root.unwrap();
    let timeline = children_of(&world, frame)[0];
    let other = project(&s.items[0])
        .into_iter()
        .filter(|p| !p.tools.is_empty())
        .nth(1)
        .unwrap();
    let row = mount(&mut world, timeline, None, "a", &other);
    let second = children_of(&world, row)[0];
    assert!(click(&mut world, second));
    assert_eq!(state(&world).root, Some(panel), "retarget keeps the panel");
    assert_eq!(state(&world).selected.as_ref().unwrap().part.id, other.id);
    assert!(click(&mut world, second));
    assert!(!world.is_alive(panel));
    assert!(state(&world).selected.is_none());
    assert!(click(&mut world, first));
    assert!(state(&world).root.is_some());
}
/// Two rounds in one turn, split by narrative.
fn split_rounds(s: &mut ChatSnapshot) {
    let runs = s.items[0].tool_runs.clone().unwrap();
    s.items[0].segments = Some(vec![
        ChatSegment::ToolGroup {
            group_id: "g1".into(),
            tool_round: 1,
            runs: vec![runs[0].clone()],
        },
        ChatSegment::Text {
            segment_id: "t".into(),
            model_round: None,
            markdown: "Narrative".into(),
            committed: true,
        },
        ChatSegment::ToolGroup {
            group_id: "g2".into(),
            tool_round: 2,
            runs: vec![runs[1].clone()],
        },
    ]);
}
#[test]
fn panel_reconciles_live_runs_retains_scroll_and_does_no_idle_remount() {
    let mut world = World::new();
    let mut s = history(vec![run("r1", "cognition_code_read", "running")], 1);
    launch(&mut world, &s);
    sync(&mut world, Some(&s));
    let root = state(&world).root.unwrap();
    let body = state(&world).body.unwrap();
    toggle(&mut world, "r1");
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
    assert_eq!(state(&world).root, Some(root));
    assert_eq!(children_of(&world, body).len(), 2);
    assert_eq!(world.get::<Scroll>(body).unwrap().offset[1], -20.);
    assert!(
        markdown_has(&world, "Source loaded"),
        "expanded call stays open"
    );
    let stats: Vec<String> = children_of(&world, state(&world).stats.unwrap())
        .into_iter()
        .filter_map(|e| world.get::<Label>(e).map(|l| l.text.clone()))
        .collect();
    assert_eq!(stats[0], "Round 1 · 2 calls");
    assert_eq!(stats.len(), 3, "round, running and done: {stats:?}");
    s.session_id = "other".into();
    sync(&mut world, Some(&s));
    assert!(!world.is_alive(root));
    assert!(state(&world).selected.is_none());
}
#[test]
fn calls_expand_in_place_several_at_once_with_readable_parameters() {
    let mut world = World::new();
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
    let mut r2 = r.clone();
    r2.run_id = "r2".into();
    r2.output_summary = Some("Second result".into());
    let s = history(vec![r, r2], 1);
    launch(&mut world, &s);
    assert!(!markdown_has(&world, "finish"), "rows start collapsed");
    let row = world.entities_with::<RunToggle>()[0];
    let labels: Vec<String> = world
        .query::<Label>()
        .into_iter()
        .filter(|(e, _)| within(&world, *e, row))
        .map(|(_, l)| l.text.clone())
        .collect();
    // The intent is the title and the tool kind sits under it.
    assert!(labels.contains(&"Load schemas".to_string()), "{labels:?}");
    let name = world.get::<Semantics>(row).unwrap().name.clone().unwrap();
    assert_eq!(name, "Expand Load schemas");
    toggle(&mut world, "r1");
    toggle(&mut world, "r2");
    assert!(markdown_has(&world, "finish"));
    assert!(markdown_has(&world, "Second result"));
    assert!(
        !markdown_has(&world, "Load schemas"),
        "intent is not repeated"
    );
    assert!(!markdown_has(&world, "truncated"));
    toggle(&mut world, "r2");
    assert!(!markdown_has(&world, "Second result"));
    assert!(markdown_has(&world, "finish"));
}
#[test]
fn a_turns_other_rounds_are_dividers_to_jump_between() {
    let mut world = World::new();
    let mut s = history(
        vec![
            run("r1", "read", "completed"),
            run("r2", "run", "completed"),
        ],
        1,
    );
    split_rounds(&mut s);
    launch(&mut world, &s);
    sync(&mut world, Some(&s));
    let body = state(&world).body.unwrap();
    // Round 1 header, its call, Round 2 header.
    assert_eq!(children_of(&world, body).len(), 3);
    let target = world
        .entities_with::<RoundTarget>()
        .into_iter()
        .find(|&e| world.get::<RoundTarget>(e).unwrap().0 != "tool:r1")
        .unwrap();
    assert!(click(&mut world, target));
    assert_eq!(state(&world).selected.as_ref().unwrap().part.id, "tool:r2");
    assert_eq!(children_of(&world, body).len(), 3);
}
#[test]
fn a_newer_round_shows_a_chip_instead_of_following() {
    let mut world = World::new();
    let mut s = history(vec![run("r1", "read", "completed")], 1);
    launch(&mut world, &s);
    sync(&mut world, Some(&s));
    let chip = state(&world).chip.unwrap();
    assert!(world.has::<Hidden>(chip));
    let mut next: ChatItem =
        serde_json::from_value(serde_json::json!({"id":"b", "role":"assistant", "content":""}))
            .unwrap();
    next.tool_runs = Some(vec![run("r9", "search", "running")]);
    s.items.push(next);
    s.revision = ChatRevision(2);
    sync(&mut world, Some(&s));
    assert_eq!(state(&world).selected.as_ref().unwrap().item, "a");
    assert_eq!(state(&world).newer.as_ref().unwrap().item, "b");
    assert!(!world.has::<Hidden>(chip));
    let jump = children_of(&world, chip)[0];
    assert!(click(&mut world, jump));
    sync(&mut world, Some(&s));
    assert_eq!(state(&world).selected.as_ref().unwrap().item, "b");
    assert!(state(&world).newer.is_none());
    assert!(world.has::<Hidden>(chip));
}
#[test]
fn escape_and_close_return_focus_to_the_summary() {
    let mut app = App::new();
    app.add_plugin(Chrome);
    let s = history(vec![run("r1", "read", "completed")], 1);
    let trigger = launch(app.world_mut(), &s);
    app.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(NamedKey::Escape, Modifiers::default())];
    sync(app.world_mut(), Some(&s));
    assert!(state(app.world()).root.is_none());
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(trigger)
    );
    let trigger = launch(app.world_mut(), &s);
    let close = app
        .world()
        .query::<Semantics>()
        .into_iter()
        .find(|(e, s)| {
            s.name.as_deref() == Some("Close tool activity") && app.world().has::<Button>(*e)
        })
        .unwrap()
        .0;
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(close);
    assert!(click(app.world_mut(), close));
    assert!(state(app.world()).root.is_none());
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(trigger)
    );
}
#[test]
fn a_narrow_tab_floats_the_panel_over_the_transcript() {
    let mut app = ui_app(560.);
    let s = history(vec![run("r1", "read", "completed")], 1);
    launch(app.world_mut(), &s);
    for _ in 0..3 {
        app.update();
        sync(app.world_mut(), Some(&s));
    }
    let w = app.world();
    let panel = state(w).root.unwrap();
    assert!(w.has::<Absolute>(panel));
    assert!(w.has::<Hidden>(state(w).sash.unwrap()));
    assert!(w.get::<Rect>(panel).unwrap().w <= 560.);
}
#[test]
fn expanded_image_call_keeps_its_gallery() {
    let mut r = run("r1", "cognition_image_view", "succeeded");
    r.artifact_refs = Some(
        serde_json::from_value(serde_json::json!([{
            "role":"output", "contentType":"image/png", "byteSize":10,
            "hash64":"h", "artifactId":"art:trial", "label":"Trial"
        }]))
        .unwrap(),
    );
    let mut world = World::new();
    let snapshot = history(vec![r], 1);
    assert!(
        project(&snapshot.items[0])
            .iter()
            .any(|p| p.images == [("art:trial".into(), "Trial".into())])
    );
    launch(&mut world, &snapshot);
    toggle(&mut world, "r1");
    assert!(
        world
            .query::<Semantics>()
            .iter()
            .any(|(_, s)| s.name.as_deref() == Some("Image attachment: Trial"))
    );
}

#[test]
fn intent_leads_and_kind_drops_the_namespace() {
    let mut r = run("1", "cognition_coder_shell_run", "succeeded");
    assert_eq!(intent(&r), "Read source");
    r.input_params = Some(vec![ToolInputParam {
        key: "Description".into(),
        value: "Inspect icon   and packaging\nconfiguration".into(),
        truncated: false,
    }]);
    assert_eq!(intent(&r), "Inspect icon and packaging configuration");
    assert_eq!(kind("cognition_coder_shell_run"), "Shell");
    assert_eq!(kind("cognition_coder_read_batch"), "Read batch");
    r.input_params = None;
    r.input_summary = None;
    assert_eq!(intent(&r), "Coder shell run");
}
#[test]
fn only_a_failure_offers_a_filter() {
    assert!(filters(&[run("1", "shell_run", "succeeded")]).is_empty());
    let runs = vec![
        run("1", "coder_shell_run", "succeeded"),
        run("2", "coder_shell_run", "failed"),
        run("3", "coder_read_batch", "succeeded"),
    ];
    let labels: Vec<_> = filters(&runs).into_iter().map(|(_, l)| l).collect();
    assert_eq!(labels, ["All", "Failed"]);
    let mut world = World::new();
    let s = history(runs, 1);
    launch(&mut world, &s);
    let failed = world
        .entities_with::<FilterTarget>()
        .into_iter()
        .find(|&e| world.get::<FilterTarget>(e).unwrap().0 == Filter::Failed)
        .unwrap();
    assert!(click(&mut world, failed));
    let rows = world.entities_with::<RunToggle>();
    assert_eq!(rows.len(), 1);
    assert_eq!(world.get::<RunToggle>(rows[0]).unwrap().0, "2");
}
#[test]
fn kind_trails_a_call_only_when_the_round_mixes_kinds() {
    let mut world = World::new();
    let s = history(
        vec![
            run("1", "coder_shell_run", "succeeded"),
            run("2", "coder_shell_run", "succeeded"),
        ],
        1,
    );
    launch(&mut world, &s);
    for row in world.entities_with::<RunToggle>() {
        let name = world.get::<Semantics>(row).unwrap().name.clone().unwrap();
        assert!(!name.contains("Shell"), "{name}");
    }
    let mut world = World::new();
    let s = history(
        vec![
            run("1", "coder_shell_run", "succeeded"),
            run("2", "coder_read_batch", "succeeded"),
        ],
        1,
    );
    launch(&mut world, &s);
    let names: Vec<_> = world
        .entities_with::<RunToggle>()
        .into_iter()
        .map(|e| world.get::<Semantics>(e).unwrap().name.clone().unwrap())
        .collect();
    assert!(names.iter().any(|n| n.ends_with(", Shell")), "{names:?}");
    assert!(
        names.iter().any(|n| n.ends_with(", Read batch")),
        "{names:?}"
    );
}
#[test]
fn long_results_preview_then_show_all_and_copy_the_whole_result() {
    let mut r = run("r1", "read", "succeeded");
    r.output_summary = Some("x".repeat(RESULT_PREVIEW + 50));
    let mut world = World::new();
    let s = history(vec![r], 1);
    launch(&mut world, &s);
    toggle(&mut world, "r1");
    assert_eq!(world_max(&world), RESULT_PREVIEW + 1);
    let copy = world.entities_with::<CopyText>()[0];
    assert_eq!(
        world.get::<CopyText>(copy).unwrap().0.len(),
        RESULT_PREVIEW + 50
    );
    let more = world.entities_with::<FullToggle>()[0];
    assert!(click(&mut world, more));
    assert_eq!(world_max(&world), RESULT_PREVIEW + 50);
}
fn world_max(world: &World) -> usize {
    world
        .query::<MarkdownView>()
        .iter()
        .map(|(_, v)| {
            v.source().chars().filter(|c| *c == 'x').count() + v.source().matches('…').count()
        })
        .max()
        .unwrap_or(0)
}
