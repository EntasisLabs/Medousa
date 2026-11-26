use super::*;
use medousa_app_core::ChatRevision;

fn item(id: &str, role: &str, content: &str) -> ChatItem {
    serde_json::from_value(serde_json::json!({"id":id,"role":role,"content":content})).unwrap()
}
fn snapshot(items: Vec<ChatItem>, revision: u64) -> ChatSnapshot {
    serde_json::from_value(
        serde_json::json!({"sessionId":"s", "authorityId":"a", "revision":revision, "items":items}),
    )
    .unwrap()
}
fn activate(world: &mut World, entity: Entity) -> bool {
    let name = world.get::<Button>(entity).unwrap().action.clone();
    action(world, &ActionIntent { entity, name })
}
fn fixture() -> (World, Entity) {
    let mut world = World::new();
    let parent = world.spawn((Layout::column(),));
    let root = setup(&mut world, parent);
    world.insert(
        root,
        Rect {
            w: 800.,
            h: 600.,
            ..Default::default()
        },
    );
    (world, root)
}
#[test]
fn literal_user_text_and_original_clipboard() {
    let (mut world, root) = fixture();
    let text = "# Not a heading\n[not a link](https://example.org)";
    sync(
        &mut world,
        root,
        Some(&snapshot(vec![item("u", "user", text)], 1)),
    );
    let state = world.get::<Timeline>(root).unwrap();
    let body = state.rows[0]
        .parts
        .iter()
        .find(|p| p.id == "body")
        .unwrap()
        .entity;
    assert_eq!(
        world.get::<MarkdownView>(body).unwrap().source(),
        literal(text)
    );
    let copy = children_of(&world, state.rows[0].entity)
        .into_iter()
        .find(|&e| world.has::<CopyText>(e))
        .unwrap();
    assert!(activate(&mut world, copy));
    assert_eq!(world.resource::<Clipboard>().unwrap().text, text);
}
#[test]
fn retained_identity_append_and_insertion() {
    let (mut world, root) = fixture();
    sync(
        &mut world,
        root,
        Some(&snapshot(vec![item("a", "assistant", "Hello")], 1)),
    );
    let row = world.get::<Timeline>(root).unwrap().rows[0].entity;
    let body = world.get::<Timeline>(root).unwrap().rows[0]
        .parts
        .iter()
        .find(|p| p.id == "body")
        .unwrap()
        .entity;
    sync(
        &mut world,
        root,
        Some(&snapshot(
            vec![
                item("u", "user", "Hi"),
                item("a", "assistant", "Hello world"),
            ],
            2,
        )),
    );
    let state = world.get::<Timeline>(root).unwrap();
    assert_eq!(state.rows[1].entity, row);
    assert_eq!(
        state.rows[1]
            .parts
            .iter()
            .find(|p| p.id == "body")
            .unwrap()
            .entity,
        body
    );
    assert_eq!(
        world.get::<MarkdownView>(body).unwrap().source(),
        "Hello world"
    );
}
#[test]
fn typed_tools_are_deduplicated_and_lazy_selectable_receipts() {
    let (mut world, root) = fixture();
    let mut response = item("a", "assistant", "aggregate ignored");
    let run = ToolRunState {
        run_id: "r".into(),
        tool_name: "read_file".into(),
        status: "completed".into(),
        round: 1,
        input_summary: Some("src/main.rs".into()),
        input_params: None,
        output_summary: Some("fn main() {}".into()),
        artifact_refs: None,
    };
    response.segments = Some(vec![
        ChatSegment::Text {
            segment_id: "t".into(),
            model_round: None,
            markdown: "Reading".into(),
            committed: true,
        },
        ChatSegment::ToolGroup {
            group_id: "g".into(),
            tool_round: 1,
            runs: vec![run.clone()],
        },
    ]);
    response.tool_runs = Some(vec![run]);
    sync(&mut world, root, Some(&snapshot(vec![response], 1)));
    let row = &world.get::<Timeline>(root).unwrap().rows[0];
    assert!(!row.parts.iter().any(|p| p.id == "body"));
    assert_eq!(row.parts.iter().filter(|p| p.id == "tool:r").count(), 1);
    let tool = row.parts.iter().find(|p| p.id == "tool:r").unwrap().entity;
    assert!(world.get::<DetailBody>(tool).is_none());
    assert!(world.entities_with::<Modal>().is_empty());
    let before = world.entities_with::<MarkdownView>().len();
    let header = children_of(&world, tool)[0];
    assert!(activate(&mut world, header));
    assert!(
        world.entities_with::<Modal>().is_empty(),
        "the thread panel docks, not a modal"
    );
    let call = world
        .query::<Semantics>()
        .iter()
        .find(|(_, s)| {
            s.name
                .as_deref()
                .is_some_and(|n| n.starts_with("Expand src/main.rs"))
        })
        .unwrap()
        .0;
    assert!(activate(&mut world, call));
    assert!(world.entities_with::<MarkdownView>().len() > before);
    assert!(
        world
            .query::<MarkdownView>()
            .iter()
            .any(|(_, v)| v.source().contains("fn main"))
    );
    let close = world
        .query::<Semantics>()
        .into_iter()
        .find(|(_, s)| s.name.as_deref() == Some("Close tool activity"))
        .unwrap()
        .0;
    assert!(activate(&mut world, close));
    assert!(world.entities_with::<Modal>().is_empty());
    assert_eq!(world.entities_with::<MarkdownView>().len(), before);
}
#[test]
fn large_history_mounts_only_near_viewport_and_scroll_does_not_snap() {
    let (mut world, root) = fixture();
    let history = snapshot(
        (0..500)
            .map(|i| item(&i.to_string(), "assistant", "Body"))
            .collect(),
        1,
    );
    sync(&mut world, root, Some(&history));
    assert!(world.entities_with::<MarkdownView>().len() < 40);
    // Simulate completed layout at the tail, then a user's scroll upward.
    world.get_mut::<Scroll>(root).unwrap().content[1] = 100_000.;
    world.get_mut::<Scroll>(root).unwrap().offset[1] = -99_400.;
    record_offset(&mut world, root);
    record_offset(&mut world, root);
    world.get_mut::<Scroll>(root).unwrap().offset[1] = -10_000.;
    sync(&mut world, root, Some(&history));
    assert!(!world.get::<Timeline>(root).unwrap().following);
    assert_eq!(world.get::<Scroll>(root).unwrap().offset[1], -10_000.);
    assert!(world.entities_with::<MarkdownView>().len() < 40);
}
#[test]
fn session_switch_clears_old_rows_even_when_revision_equal() {
    let (mut world, root) = fixture();
    let first = snapshot(vec![item("a", "assistant", "old")], 1);
    sync(&mut world, root, Some(&first));
    let old = world.get::<Timeline>(root).unwrap().rows[0].entity;
    let mut second = snapshot(vec![item("a", "assistant", "new")], 1);
    second.session_id = "other".into();
    sync(&mut world, root, Some(&second));
    assert!(!world.is_alive(old));
    assert_eq!(
        world.get::<Timeline>(root).unwrap().revision,
        Some(ChatRevision(1))
    );
}

#[test]
fn native_layout_follows_tail_but_preserves_readers_scroll_during_append() {
    let mut app = App::new();
    app.add_plugin(MarkdownPlugin);
    let parent = app.world_mut().spawn((Layout {
        width: Size::Px(700.),
        height: Size::Px(400.),
        ..Layout::column()
    },));
    let root = setup(app.world_mut(), parent);
    let mut history = snapshot(
        vec![
            item("u", "user", "Hello"),
            item("a", "assistant", &"A paragraph of text.\n\n".repeat(40)),
        ],
        1,
    );
    for _ in 0..8 {
        sync(app.world_mut(), root, Some(&history));
        app.update();
    }
    let scroll = app.world().get::<Scroll>(root).unwrap();
    let bottom = (scroll.content[1] - app.world().get::<Rect>(root).unwrap().h).max(0.);
    assert!(bottom > 400.);
    assert!(
        (scroll.offset[1] + bottom).abs() < 2.,
        "offset {} bottom {bottom}",
        scroll.offset[1]
    );
    app.world_mut().get_mut::<Scroll>(root).unwrap().offset[1] = -30.;
    for _ in 0..4 {
        sync(app.world_mut(), root, Some(&history));
        app.update();
    }
    history.items[1].content.push_str("More text.\n\n");
    history.revision = ChatRevision(2);
    for _ in 0..4 {
        sync(app.world_mut(), root, Some(&history));
        app.update();
    }
    assert!((app.world().get::<Scroll>(root).unwrap().offset[1] + 30.).abs() < 2.);
}

#[test]
fn core_filters_empty_handoff_shells_and_renders_failures() {
    let (mut world, root) = fixture();
    let mut shell = item("shell", "assistant", "");
    shell.stage_whisper = Some("Worker started".into());
    let mut failure = item("failed", "assistant", "");
    failure.failed = Some(true);
    failure.error_line = Some("Provider unavailable".into());
    failure.error_detail = Some("Retry later".into());
    sync(&mut world, root, Some(&snapshot(vec![shell, failure], 1)));
    let state = world.get::<Timeline>(root).unwrap();
    assert_eq!(state.rows.len(), 1);
    assert_eq!(state.rows[0].id, "failed");
    assert!(
        state.rows[0]
            .parts
            .iter()
            .any(|p| p.id == "error" && p.part.text.contains("unavailable"))
    );
}

#[test]
fn assistant_generated_images_are_inline_chronological_deduplicated_and_retained() {
    let run = serde_json::json!({"runId":"r", "toolName":"cognition_image_generate", "status":"succeeded", "round":1,
    "artifactRefs":[
        {"role":"input","contentType":"image/png","byteSize":10,"hash64":"h","artifactId":"art:input"},
        {"role":"output","contentType":"application/json","byteSize":10,"hash64":"h","artifactId":"art:json"},
        {"role":"output","contentType":"image/png","byteSize":10,"hash64":"h","artifactId":"art:pixels","label":"Concept.png"}
    ]});
    let mut assistant = item("a", "assistant", "Here are the concepts.");
    assistant.tool_runs = Some(vec![serde_json::from_value(run.clone()).unwrap()]);
    assistant.segments = Some(
        serde_json::from_value(serde_json::json!([
            {"kind":"text","segmentId":"intro","markdown":"Two concepts","committed":true},
            {"kind":"tool_group","groupId":"g","toolRound":1,"runs":[run]},
            {"kind":"text","segmentId":"end","markdown":"Which works?","committed":true}
        ]))
        .unwrap(),
    );
    let projected = project(&assistant);
    let image = projected.iter().position(|p| !p.images.is_empty()).unwrap();
    assert_eq!(projected[image].images[0].0, "art:pixels");
    assert_eq!(projected[image + 1].id, "text:end");
    assert_eq!(projected.iter().filter(|p| !p.images.is_empty()).count(), 1);
    let (mut world, root) = fixture();
    sync(
        &mut world,
        root,
        Some(&snapshot(vec![assistant.clone()], 1)),
    );
    let entity = world.get::<Timeline>(root).unwrap().rows[0]
        .parts
        .iter()
        .find(|p| p.id == "artifact:art:pixels")
        .unwrap()
        .entity;
    sync(
        &mut world,
        root,
        Some(&snapshot(vec![assistant.clone()], 2)),
    );
    assert_eq!(
        entity,
        world.get::<Timeline>(root).unwrap().rows[0]
            .parts
            .iter()
            .find(|p| p.id == "artifact:art:pixels")
            .unwrap()
            .entity
    );
    assistant.segments = None;
    assert_eq!(
        project(&assistant)
            .iter()
            .filter(|p| !p.images.is_empty())
            .count(),
        1
    );
    assistant.tool_runs.as_mut().unwrap()[0].status = "failed".into();
    assert!(!project(&assistant).iter().any(|p| !p.images.is_empty()));
}

#[test]
fn user_turn_navigation_stops_following_and_latest_restores_it() {
    let (mut world, root) = fixture();
    let history = snapshot(
        (0..12)
            .map(|n| {
                item(
                    &format!("turn-{n}"),
                    if n % 2 == 0 { "user" } else { "assistant" },
                    "A substantial turn\n\nwith a response.",
                )
            })
            .collect(),
        1,
    );
    sync(&mut world, root, Some(&history));
    world.get_mut::<Scroll>(root).unwrap().content[1] = 2200.;
    world.get_mut::<Scroll>(root).unwrap().offset[1] = -350.;
    world.get_mut::<Timeline>(root).unwrap().samples = 0;
    navigate(&mut world, root, PREVIOUS_TURN);
    assert!(!world.get::<Timeline>(root).unwrap().following);
    let previous = world.get::<Scroll>(root).unwrap().offset[1];
    assert!(previous > -350.);
    navigate(&mut world, root, NEXT_TURN);
    assert!(world.get::<Scroll>(root).unwrap().offset[1] < previous);
    sync(&mut world, root, Some(&history));
    let latest = world.get::<Timeline>(root).unwrap().latest.unwrap();
    assert!(!world.has::<Hidden>(latest));
    assert!(activate(&mut world, latest));
    assert!(world.get::<Timeline>(root).unwrap().following);
    sync(&mut world, root, Some(&history));
    assert!(world.has::<Hidden>(latest));
}

#[test]
fn turn_controls_have_real_rndr_disabled_state_at_reachable_boundaries() {
    let (mut world, root) = fixture();
    // Find controls by their actual RNDR opaque intent, not guessed action text.
    let controls = children_of(
        &world,
        world.get::<Timeline>(root).unwrap().navigation.unwrap(),
    );
    let previous = controls[0];
    let next = controls[1];
    assert!(!world.get::<Button>(previous).unwrap().enabled);
    assert!(!world.get::<Button>(next).unwrap().enabled);
    assert!(world.has::<ControlDisabled>(previous));
    let history = snapshot(
        (0..12)
            .map(|n| item(&n.to_string(), "user", "Turn"))
            .collect(),
        1,
    );
    sync(&mut world, root, Some(&history));
    world.get_mut::<Scroll>(root).unwrap().content[1] = 2200.;
    world.get_mut::<Scroll>(root).unwrap().offset[1] = 0.;
    sync_navigation(&mut world, root);
    assert!(!world.get::<Button>(previous).unwrap().enabled);
    assert!(world.get::<Button>(next).unwrap().enabled);
    world.get_mut::<Scroll>(root).unwrap().offset[1] = -1600.;
    sync_navigation(&mut world, root);
    assert!(world.get::<Button>(previous).unwrap().enabled);
    assert!(!world.get::<Button>(next).unwrap().enabled);
    assert!(world.has::<ControlDisabled>(next));
    let name = world.get::<Button>(next).unwrap().action.clone();
    assert_eq!(
        rndr_ui::controls::host_intent(&world, &ActionIntent { entity: next, name }),
        None
    );
}

#[test]
fn navigation_remeasures_stable_turn_and_does_not_move_other_panes() {
    let (mut world, root) = fixture();
    let parent = world.spawn((Layout::column(),));
    let other = setup(&mut world, parent);
    world.insert(
        other,
        Rect {
            w: 800.,
            h: 600.,
            ..Default::default()
        },
    );
    let history = snapshot(
        (0..12)
            .map(|n| item(&n.to_string(), "user", "Turn"))
            .collect(),
        1,
    );
    sync(&mut world, root, Some(&history));
    sync(&mut world, other, Some(&history));
    world.get_mut::<Scroll>(root).unwrap().content[1] = 2200.;
    world.get_mut::<Scroll>(root).unwrap().offset[1] = -400.;
    // Reconcile the visible window before simulating measured rows above it.
    sync(&mut world, root, Some(&history));
    navigate(&mut world, root, PREVIOUS_TURN);
    let anchor = world
        .get::<Timeline>(root)
        .unwrap()
        .navigation_anchor
        .clone()
        .unwrap();
    let before = world.get::<Scroll>(root).unwrap().offset[1];
    let other_offset = world.get::<Scroll>(other).unwrap().offset;
    let rows: Vec<_> = world
        .get::<Timeline>(root)
        .unwrap()
        .rows
        .iter()
        .map(|r| r.entity)
        .collect();
    for entity in &rows {
        world.insert(
            *entity,
            Rect {
                w: 600.,
                h: 160.,
                ..Default::default()
            },
        );
    }
    // The first row grows after layout, as with streamed/reflowed content.
    world.get_mut::<Rect>(rows[0]).unwrap().h = 260.;
    assert!(record_offset(&mut world, root));
    assert_eq!(world.get::<Scroll>(root).unwrap().offset[1], before - 100.);
    assert_eq!(world.get::<Scroll>(other).unwrap().offset, other_offset);
    let mut streaming = history.clone();
    streaming.revision = medousa_app_core::ChatRevision(2);
    streaming
        .items
        .last_mut()
        .unwrap()
        .content
        .push_str(" streaming tail");
    sync(&mut world, root, Some(&streaming));
    record_offset(&mut world, root);
    assert_eq!(
        world
            .get::<Timeline>(root)
            .unwrap()
            .navigation_anchor
            .as_deref(),
        Some(anchor.as_str())
    );
    assert!(!world.get::<Timeline>(root).unwrap().following);
    assert_eq!(world.get::<Scroll>(root).unwrap().offset[1], before - 100.);
    // Direct wheel/scrollbar input relinquishes the programmatic anchor.
    world.get_mut::<Scroll>(root).unwrap().offset[1] += 20.;
    sync(&mut world, root, Some(&streaming));
    assert!(
        world
            .get::<Timeline>(root)
            .unwrap()
            .navigation_anchor
            .is_none()
    );
}

#[test]
fn quiet_bubbles_align_by_role_and_media_is_a_real_preview() {
    let (mut world, root) = fixture();
    let mut user = item("u", "user", "Attached screenshot");
    user.media_attachments = Some(vec![serde_json::from_value(
        serde_json::json!({"mediaId":"m","kind":"image","mime":"image/png","label":"screen.png"}),
    )
    .unwrap()]);
    let history = snapshot(vec![user, item("a", "assistant", "A quieter reply")], 1);
    sync(&mut world, root, Some(&history));
    let rows = &world.get::<Timeline>(root).unwrap().rows;
    // User turns pack to the trailing edge; assistant turns read from the start.
    assert_eq!(
        world.get::<Align>(rows[0].entity).unwrap().justify,
        Justify::End
    );
    assert_eq!(
        world.get::<Align>(rows[1].entity).unwrap().justify,
        Justify::Start
    );
    let bubble = rows[0].body.unwrap();
    let reading = rows[1].body.unwrap();
    assert_eq!(world.get::<Radius>(bubble).unwrap().0, BUBBLE_RADIUS);
    assert_eq!(
        world.get::<rndr_ui::UiStyle>(bubble).unwrap().background,
        Some(visual::user().into())
    );
    assert_eq!(world.get::<Padding>(reading).unwrap().left, 0.);
    assert_eq!(world.get::<Radius>(reading).unwrap().0, 0.);
    assert_eq!(
        world.get::<rndr_ui::UiStyle>(reading).unwrap().background,
        Some(rndr_ui::UiColor::Explicit(Color::rgba(0., 0., 0., 0.)))
    );
    // The speaker changes between these turns.
    assert_eq!(world.get::<Padding>(rows[0].entity).unwrap().top, 0.);
    assert_eq!(
        world.get::<Padding>(rows[1].entity).unwrap().top,
        SPEAKER_BREAK
    );
    // Quiet until wanted, but still a real focusable, named action.
    let copy = children_of(&world, rows[1].entity)
        .into_iter()
        .find(|e| world.has::<CopyText>(*e))
        .unwrap();
    assert!(world.has::<Focusable>(copy));
    assert_eq!(
        world.get::<Semantics>(copy).unwrap().name.as_deref(),
        Some("Copy message")
    );
    assert_eq!(world.get::<Opacity>(copy).unwrap().0, 0.);
    let padding = world.get::<Padding>(root).unwrap();
    let lane = visual::column_width(828.);
    assert_eq!(padding.left, (828. - lane) / 2.);
    assert_eq!(
        world.get::<Layout>(rows[1].entity).unwrap().width,
        Size::Px(lane)
    );
    let body = rows[1]
        .parts
        .iter()
        .find(|p| p.id == "body")
        .unwrap()
        .entity;
    assert_eq!(
        world.get::<rndr_ui::UiStyle>(body).unwrap().typography,
        Some(rndr_ui::Typography::Reading)
    );
    // Two parties need no speaker labels.
    assert!(
        rows.iter()
            .all(|row| row.parts.iter().all(|p| p.id != "header"))
    );
    let media = rows[0]
        .parts
        .iter()
        .find(|p| p.id == "media:m")
        .unwrap()
        .entity;
    assert!(!world.has::<MarkdownView>(media));
    crate::media_preview::tick(&mut world);
    assert_eq!(crate::media_preview::drain_commands(&mut world).len(), 1);
    sync(&mut world, root, Some(&history));
    crate::media_preview::tick(&mut world);
    assert!(crate::media_preview::drain_commands(&mut world).is_empty());
}

#[test]
fn timeline_scrollbar_rests_hidden_and_wakes_for_pointer_scroll_or_focus() {
    let (mut world, root) = fixture();
    let navigation = world.get::<Timeline>(root).unwrap().navigation.unwrap();
    let rail = children_of(&world, navigation)
        .into_iter()
        .find(|e| world.has::<Scrollbar>(*e))
        .unwrap();
    assert_eq!(world.get::<Layout>(rail).unwrap().width, Size::Px(8.));
    assert_eq!(world.get::<Scrollbar>(rail).unwrap().target, root);
    sync_navigation(&mut world, root);
    assert_eq!(world.get::<Opacity>(rail).unwrap().0, 0.);
    // Overflow alone does not paint a line down the conversation.
    world.get_mut::<Scroll>(root).unwrap().content[1] = 1200.;
    world.insert(
        root,
        Rect {
            x: 0.,
            y: 0.,
            w: 600.,
            h: 400.,
        },
    );
    let mut input = world.resource::<Input>().cloned().unwrap_or_default();
    input.pointer.position = [-10., -10.];
    world.insert_resource(input.clone());
    sync_navigation(&mut world, root);
    assert_eq!(world.get::<Opacity>(rail).unwrap().0, 0.);
    // The pointer over the conversation wakes it.
    input.pointer.position = [300., 200.];
    world.insert_resource(input.clone());
    sync_navigation(&mut world, root);
    assert_eq!(world.get::<Opacity>(rail).unwrap().0, 1.);
    // So does scrolling, without the pointer there.
    input.pointer.position = [-10., -10.];
    world.insert_resource(input);
    sync_navigation(&mut world, root);
    assert_eq!(world.get::<Opacity>(rail).unwrap().0, 0.);
    world.get_mut::<Scroll>(root).unwrap().offset[1] = -120.;
    sync_navigation(&mut world, root);
    assert_eq!(world.get::<Opacity>(rail).unwrap().0, 1.);
    world.get_mut::<Scroll>(root).unwrap().content[1] = 0.;
    world.insert_resource(Focus {
        entity: Some(rail),
        ..Default::default()
    });
    sync_navigation(&mut world, root);
    assert_eq!(world.get::<Opacity>(rail).unwrap().0, 1.);
}

#[test]
fn inspected_images_across_rounds_share_one_row_without_selection() {
    let mut assistant = item("a", "assistant", "");
    assistant.tool_runs = Some(["first", "trial", "last"].into_iter().enumerate().map(|(round,id)|serde_json::from_value(serde_json::json!({"runId":id,"toolName":"cognition_image_view","status":"succeeded","round":round+1,"artifactRefs":[{"role":"output","contentType":"image/png","byteSize":1,"hash64":"h","artifactId":format!("art:{id}")}]})).unwrap()).collect());
    let projected = project(&assistant);
    let rows: Vec<_> = projected.iter().filter(|p| !p.images.is_empty()).collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0]
            .images
            .iter()
            .map(|(id, _)| id.as_str())
            .collect::<Vec<_>>(),
        ["art:first", "art:trial", "art:last"]
    );
    assistant.content = "`![code](art:trial)`\n\n![Before](art:first)\n![After](art:last)".into();
    let projected = project(&assistant);
    let work = projected.last().unwrap();
    assert_eq!(work.text, "View work · 3 images");
    assert_eq!(work.tools.len(), 3);
    assert_eq!(
        projected
            .iter()
            .find(|p| !p.images.is_empty())
            .unwrap()
            .images,
        vec![
            ("art:first".into(), "Before".into()),
            ("art:trial".into(), "Medousa image".into()),
            ("art:last".into(), "After".into())
        ]
    );
}

#[test]
fn detail_disclosure_turns_one_chevron_and_keeps_a_plain_name() {
    let (mut world, root) = fixture();
    let mut failure = item("failed", "assistant", "");
    failure.failed = Some(true);
    failure.error_line = Some("Provider unavailable".into());
    failure.error_detail = Some("Retry later".into());
    sync(&mut world, root, Some(&snapshot(vec![failure], 1)));
    let detail = world.get::<Timeline>(root).unwrap().rows[0]
        .parts
        .iter()
        .find(|p| p.id == "error-detail")
        .unwrap()
        .entity;
    let button = children_of(&world, detail)
        .into_iter()
        .find(|e| world.has::<Button>(*e))
        .unwrap();
    let glyph = Icon::ChevronRight.scalar().to_string();
    let mut labels = vec![];
    let mut pending = vec![button];
    while let Some(e) = pending.pop() {
        if world.has::<Label>(e) {
            labels.push(e);
        }
        pending.extend(children_of(&world, e));
    }
    let chevron = *labels
        .iter()
        .find(|e| world.get::<Label>(**e).unwrap().text == glyph)
        .unwrap();
    let rotate = |w: &World| w.get::<Transform>(chevron).map_or(0., |t| t.rotate);
    assert_eq!(rotate(&world), 0.);
    assert!(labels.iter().all(|e| {
        let text = &world.get::<Label>(*e).unwrap().text;
        !text.contains('▸') && !text.contains('▾')
    }));
    assert!(activate(&mut world, button));
    assert!((rotate(&world) - std::f32::consts::FRAC_PI_2).abs() < 1e-4);
    assert!(activate(&mut world, button));
    assert_eq!(rotate(&world), 0.);
}

#[test]
fn short_user_turns_hug_their_text_and_long_ones_wrap_at_a_measure() {
    let (mut world, root) = fixture();
    let long = "a considerably longer question that keeps going ".repeat(8);
    let history = snapshot(
        vec![
            item("short", "user", "ok"),
            item("long", "user", &long),
            item("a", "assistant", "reply"),
        ],
        1,
    );
    sync(&mut world, root, Some(&history));
    let rows = &world.get::<Timeline>(root).unwrap().rows;
    let width = |row: &Row| match world.get::<Layout>(row.body.unwrap()).unwrap().width {
        Size::Px(w) => w,
        other => panic!("bubble width {other:?}"),
    };
    let lane = visual::column_width(828.);
    assert!(width(&rows[0]) < 120., "{}", width(&rows[0]));
    assert_eq!(width(&rows[1]), (lane * 0.75).min(560.));
    // Same speaker twice: no extra break between them.
    assert_eq!(world.get::<Padding>(rows[1].entity).unwrap().top, 0.);
}

#[test]
fn copy_appears_for_the_hovered_or_focused_message_only() {
    let (mut world, root) = fixture();
    let history = snapshot(
        vec![
            item("u", "user", "question"),
            item("a", "assistant", "answer"),
        ],
        1,
    );
    sync(&mut world, root, Some(&history));
    let rows = world
        .get::<Timeline>(root)
        .unwrap()
        .rows
        .iter()
        .map(|r| r.entity)
        .collect::<Vec<_>>();
    let copy = |world: &World, row: Entity| {
        children_of(world, row)
            .into_iter()
            .find(|e| world.has::<CopyText>(*e))
            .unwrap()
    };
    world.insert(
        rows[0],
        Rect {
            x: 0.,
            y: 0.,
            w: 400.,
            h: 60.,
        },
    );
    world.insert(
        rows[1],
        Rect {
            x: 0.,
            y: 80.,
            w: 400.,
            h: 60.,
        },
    );
    let mut input = world.resource::<Input>().cloned().unwrap_or_default();
    input.pointer.position = [20., 100.];
    world.insert_resource(input);
    chrome(&mut world);
    let opacity = |world: &World, row| world.get::<Opacity>(copy(world, row)).unwrap().0;
    assert_eq!(
        (opacity(&world, rows[0]), opacity(&world, rows[1])),
        (0., 1.)
    );
    // Keyboard focus reveals it without a pointer.
    let mut input = world.resource::<Input>().cloned().unwrap();
    input.pointer.position = [-10., -10.];
    world.insert_resource(input);
    let target = copy(&world, rows[0]);
    world.insert_resource(Focus {
        entity: Some(target),
        ..Default::default()
    });
    chrome(&mut world);
    assert_eq!(
        (opacity(&world, rows[0]), opacity(&world, rows[1])),
        (1., 0.)
    );
}

#[test]
fn html_artifact_cards_are_retained_controls_not_markdown_or_browsers() {
    let (mut world, root) = fixture();
    let mut response = item("a", "assistant", "");
    let artifact: medousa_app_core::contract::UiArtifact = serde_json::from_value(serde_json::json!({
        "artifactId": "saved-html", "mime": "text/html", "label": "Interactive demo", "presentation": "inline"
    })).unwrap();
    response.segments = Some(vec![ChatSegment::Artifact {
        artifact: artifact.clone(),
    }]);
    response.ui_artifacts = Some(vec![artifact]);
    let history = snapshot(vec![response], 1);
    sync(&mut world, root, Some(&history));
    let rows = &world.get::<Timeline>(root).unwrap().rows;
    let cards = rows[0]
        .parts
        .iter()
        .filter(|p| p.part.html.is_some())
        .collect::<Vec<_>>();
    assert_eq!(cards.len(), 1);
    let card = cards[0].entity;
    assert!(world.has::<ActivationHandler>(card));
    assert!(!world.has::<MarkdownView>(card));
    sync(&mut world, root, Some(&history));
    assert_eq!(
        world.get::<Timeline>(root).unwrap().rows[0]
            .parts
            .iter()
            .find(|p| p.part.html.is_some())
            .unwrap()
            .entity,
        card
    );
}
