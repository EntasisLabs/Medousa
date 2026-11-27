use super::*;
use rndr_canvas::{Operation, ShapeKind};
use rndr_canvas_ui::editor::Tool;
use std::sync::Arc;

fn app() -> App {
    let mut app = crate::ui::app();
    app.world_mut().insert_resource(Viewport::new(1200., 850.));
    app.update();
    crate::surfaces::select(app.world_mut(), crate::surfaces::SurfaceId::Create);
    app
}

fn act(w: &mut World, a: Action) {
    let id = next();
    state_mut(w).unwrap().actions.insert(id, a);
    assert!(action(w, &HostIntent::Command(id)));
    sync(w);
}

fn summary(id: &str, title: &str, pinned: bool, updated: &str) -> CreativeProjectSummary {
    CreativeProjectSummary {
        id: id.into(),
        revision: 0,
        title: title.into(),
        pinned,
        updated_at: updated.into(),
    }
}

/// A document as the daemon would return it, built with RNDR's own store.
fn document(id: &str, title: &str) -> Mutation {
    use rndr_canvas::{CanvasStore, CreateRequest, MemoryStore};
    let store = MemoryStore::new();
    store
        .create(CreateRequest {
            id: id.into(),
            title: title.into(),
            scene: Starter::Diagram.scene(),
        })
        .unwrap();
    store.get(id).unwrap()
}

fn reply(w: &mut World, command: &Command, result: Result<Response, CanvasError>) {
    apply(
        w,
        Update {
            command: command.clone(),
            result,
        },
    );
    sync(w);
}

fn rectangle(id: &str) -> Operation {
    Operation::put(Node::new(
        id,
        Content::Shape {
            shape: ShapeKind::Rectangle,
            fill: "#ffd166".into(),
            stroke: "transparent".into(),
            stroke_width: 0.,
            line: None,
        },
        Transform {
            x: 40.,
            y: 40.,
            width: 120.,
            height: 80.,
            rotation: 0.,
        },
    ))
}

#[test]
fn starters_match_medousa_home_and_round_trip_through_the_daemon_types() {
    for starter in Starter::ALL {
        let scene = starter.scene();
        let daemon: medousa_types::CreativeScene = convert(&scene).unwrap();
        let back: Scene = convert(&daemon).unwrap();
        assert_eq!(back, scene, "{starter:?}");
        assert_eq!(daemon.schema, "medousa-create");
        assert_eq!(daemon.version, 3);
        match starter {
            Starter::Blank => assert!(scene.nodes.is_empty()),
            Starter::Composition => {
                assert_eq!(scene.nodes[0].transform.width, 1024.);
                assert_eq!(scene.nodes[0].transform.height, 1024.);
            }
            _ => assert_eq!(scene.nodes[0].transform.width, 1200.),
        }
    }
    assert_eq!(Starter::Blank.title(), "Untitled canvas");
    let id = uuid();
    assert_eq!(id.len(), 36);
    assert_eq!(&id[14..15], "4", "{id}");
    assert_ne!(uuid(), uuid());
}

#[test]
fn daemon_errors_map_onto_what_the_canvas_session_does_next() {
    use medousa_sdk::SdkError;
    let http = |text: &str| canvas_error(SdkError::Http(text.into()));
    assert!(matches!(
        http("409 Conflict: creative revision conflict (current revision 7)"),
        CanvasError::Conflict(7)
    ));
    assert!(matches!(
        http("409 Conflict: unsupported creative document version"),
        CanvasError::Unsupported
    ));
    assert!(matches!(
        http("404 Not Found: missing"),
        CanvasError::NotFound
    ));
    assert!(matches!(
        http("400 Bad Request: invalid creative document: bad"),
        CanvasError::Invalid(_)
    ));
    // Unreachable or failing: kept for a retry with the same id.
    assert!(matches!(
        canvas_error(SdkError::Transport("connection refused".into())),
        CanvasError::Storage(_)
    ));
    assert!(matches!(
        http("500 Internal: disk"),
        CanvasError::Storage(_)
    ));
}

#[test]
fn opening_lists_projects_and_a_pick_loads_it_into_the_canvas() {
    let mut app = app();
    let w = app.world_mut();
    let commands = take_commands(w);
    assert!(matches!(
        commands.as_slice(),
        [Command {
            kind: Kind::List,
            ..
        }]
    ));
    reply(
        w,
        &commands[0],
        Ok(Response::Listed(CreativeListResponse {
            projects: vec![
                summary("a", "Older", false, "2026-01-01T00:00:00Z"),
                summary("b", "Pinned board", true, "2025-01-01T00:00:00Z"),
                summary("c", "Newest", false, "2026-06-01T00:00:00Z"),
            ],
            next_cursor: None,
        })),
    );
    let order: Vec<_> = state(w)
        .unwrap()
        .projects
        .iter()
        .map(|p| p.id.as_str())
        .collect();
    assert_eq!(order, ["b", "c", "a"], "pinned first, then most recent");
    let editor = state(w).unwrap().editor;
    assert!(w.has::<Hidden>(editor), "nothing open yet");

    act(w, Action::Open("c".into()));
    let commands = take_commands(w);
    let get = commands
        .iter()
        .find(|c| matches!(&c.kind, Kind::Get(id) if id == "c"))
        .expect("read the picked project");
    reply(
        w,
        get,
        Ok(Response::Document(Box::new(document("c", "Newest")))),
    );
    assert!(!w.has::<Hidden>(editor));
    let view = w.resource::<CanvasView>().unwrap();
    assert_eq!(view.editor.document_id(), "c");
    assert_eq!(view.editor.nodes().len(), 1, "the starter frame");
}

#[test]
fn canvas_edits_show_at_once_and_save_through_the_worker() {
    let mut app = app();
    let w = app.world_mut();
    take_commands(w);
    act(w, Action::Open("doc".into()));
    let get = take_commands(w)
        .into_iter()
        .find(|c| matches!(c.kind, Kind::Get(_)))
        .unwrap();
    reply(
        w,
        &get,
        Ok(Response::Document(Box::new(document("doc", "Board")))),
    );

    let v = w.resource_mut::<CanvasView>().unwrap();
    assert!(v.editor.commit("Add rectangle", vec![rectangle("r")]));
    assert_eq!(v.editor.nodes().len(), 2, "shown before any save");
    sync(w);
    assert_eq!(state(w).unwrap().status, (true, false));
    let commands = take_commands(w);
    let save = commands
        .iter()
        .find(|c| matches!(c.kind, Kind::Transact(..)))
        .expect("one transaction leaves");
    let Kind::Transact(_, transaction) = &save.kind else {
        unreachable!()
    };
    assert_eq!(transaction.expected_revision, 0);
    // The transaction the daemon receives is the same JSON.
    let wire: CreativeTransaction = convert(transaction).unwrap();
    assert_eq!(wire.operations.len(), 1);

    let mut saved = document("doc", "Board");
    saved.document =
        rndr_canvas::apply_operations(&saved.document, &transaction.operations).unwrap();
    saved.document.revision = 1;
    saved.applied_revision = 1;
    saved.can_undo = true;
    reply(w, save, Ok(Response::Document(Box::new(saved))));
    let v = w.resource::<CanvasView>().unwrap();
    assert!(!v.editor.is_saving());
    assert_eq!(v.editor.mutation().document.revision, 1);
    assert_eq!(state(w).unwrap().status, (false, false));
}

#[test]
fn an_unreachable_workshop_keeps_edits_and_retries_the_same_request() {
    let mut app = app();
    let w = app.world_mut();
    take_commands(w);
    act(w, Action::Open("doc".into()));
    let get = take_commands(w)
        .into_iter()
        .find(|c| matches!(c.kind, Kind::Get(_)))
        .unwrap();
    reply(
        w,
        &get,
        Ok(Response::Document(Box::new(document("doc", "Board")))),
    );
    w.resource_mut::<CanvasView>()
        .unwrap()
        .editor
        .commit("Add rectangle", vec![rectangle("r")]);
    sync(w);
    let save = take_commands(w)
        .into_iter()
        .find(|c| matches!(c.kind, Kind::Transact(..)))
        .unwrap();
    reply(
        w,
        &save,
        Err(CanvasError::Storage("connection refused".into())),
    );
    assert_eq!(state(w).unwrap().status, (true, true));
    assert_eq!(w.resource::<CanvasView>().unwrap().editor.nodes().len(), 2);
    assert!(take_commands(w).is_empty(), "no resend until asked");

    act(w, Action::Retry);
    let again = take_commands(w)
        .into_iter()
        .find(|c| matches!(c.kind, Kind::Transact(..)))
        .expect("resent");
    let (Kind::Transact(_, a), Kind::Transact(_, b)) = (&save.kind, &again.kind) else {
        unreachable!()
    };
    assert_eq!(
        a.id, b.id,
        "same id, so a save that did land is not doubled"
    );
}

#[test]
fn a_new_project_is_created_once_then_opened() {
    let mut app = app();
    let w = app.world_mut();
    take_commands(w);
    act(w, Action::New(Starter::Diagram));
    act(w, Action::New(Starter::Blank));
    let commands = take_commands(w);
    let creates: Vec<_> = commands
        .iter()
        .filter(|c| matches!(c.kind, Kind::Create(_)))
        .collect();
    assert_eq!(creates.len(), 1, "one create at a time");
    let Kind::Create(request) = &creates[0].kind else {
        unreachable!()
    };
    assert_eq!(request.title, "Diagram");
    reply(w, creates[0], Ok(Response::Created(request.id.clone())));
    let next = take_commands(w);
    assert!(
        next.iter()
            .any(|c| matches!(&c.kind, Kind::Get(id) if *id == request.id))
    );
    assert!(next.iter().any(|c| matches!(c.kind, Kind::List)));
}

#[test]
fn tool_letters_reach_the_canvas_only_while_nothing_else_has_focus() {
    let mut app = app();
    take_commands(app.world_mut());
    act(app.world_mut(), Action::Open("doc".into()));
    let get = take_commands(app.world_mut())
        .into_iter()
        .find(|c| matches!(c.kind, Kind::Get(_)))
        .unwrap();
    reply(
        app.world_mut(),
        &get,
        Ok(Response::Document(Box::new(document("doc", "Board")))),
    );
    // Lay the canvas slot out.
    app.update();
    let tool = |w: &World| w.resource::<CanvasView>().unwrap().editor.tool();
    let press = |w: &mut World| {
        w.resource_mut::<Input>().unwrap().keys =
            vec![KeyEvent::character('r', Modifiers::default())];
    };
    let w = app.world_mut();
    let sidebar = state(w).unwrap().sidebar;
    w.resource_mut::<Focus>().unwrap().entity = Some(sidebar);
    press(w);
    sync(w);
    assert_eq!(tool(w), rndr_canvas_ui::editor::Tool::Select);
    w.resource_mut::<Focus>().unwrap().entity = None;
    press(w);
    sync(w);
    assert_eq!(tool(w), rndr_canvas_ui::editor::Tool::Rectangle);
}

#[test]
#[ignore = "writes verification/native-create-{dark,light}.png"]
fn create_visual_acceptance() {
    use rndr_canvas::{CanvasStore, CreateRequest, MemoryStore};
    let engine: Arc<dyn TextEngine> = Arc::new(rndr_render::ParleyText::new());
    let mut exporter = rndr_render::Exporter::new().unwrap();
    std::fs::create_dir_all("verification").unwrap();
    let store = MemoryStore::new();
    store
        .create(CreateRequest {
            id: "board".into(),
            title: "Launch board".into(),
            scene: Starter::Diagram.scene(),
        })
        .unwrap();
    let shape = |id: &str, shape: ShapeKind, fill: &str, x: f64, y: f64| {
        Operation::put(Node::new(
            id,
            Content::Shape {
                shape,
                fill: fill.into(),
                stroke: "#1f2937".into(),
                stroke_width: 2.,
                line: None,
            },
            Transform {
                x,
                y,
                width: 220.,
                height: 140.,
                rotation: 0.,
            },
        ))
    };
    let document = store
        .transact(
            "board",
            Transaction {
                id: "seed".into(),
                expected_revision: 0,
                label: "Seed".into(),
                operations: vec![
                    shape("idea", ShapeKind::Rectangle, "#ffd166", 120., 160.),
                    shape("plan", ShapeKind::Ellipse, "#8ecae6", 480., 160.),
                    shape("ship", ShapeKind::Rectangle, "#b7e4c7", 840., 160.),
                ],
            },
        )
        .unwrap();
    for (dark, suffix) in [(true, "dark"), (false, "light")] {
        let mut app = crate::ui::app();
        app.world_mut()
            .insert_resource(rndr_ui::InteractionMotion::snap());
        app.world_mut().insert_resource(engine.clone());
        app.world_mut().insert_resource(Viewport::new(1400., 900.));
        publish_system_appearance(
            app.world_mut(),
            SystemAppearance {
                dark: Some(dark),
                ..Default::default()
            },
        );
        rndr_paint::frame(&mut app);
        crate::surfaces::select(app.world_mut(), crate::surfaces::SurfaceId::Create);
        let w = app.world_mut();
        take_commands(w);
        let list = Command {
            serial: state(w).unwrap().list.unwrap(),
            kind: Kind::List,
        };
        reply(
            w,
            &list,
            Ok(Response::Listed(CreativeListResponse {
                projects: vec![
                    summary("board", "Launch board", true, "2026-11-20T00:00:00Z"),
                    summary("shots", "Screenshot markup", false, "2026-11-19T00:00:00Z"),
                    summary("ui", "UI concept", false, "2026-11-18T00:00:00Z"),
                ],
                next_cursor: None,
            })),
        );
        act(w, Action::Open("board".into()));
        let get = take_commands(w)
            .into_iter()
            .find(|c| matches!(c.kind, Kind::Get(_)))
            .unwrap();
        reply(w, &get, Ok(Response::Document(Box::new(document.clone()))));
        for _ in 0..4 {
            rndr_paint::frame(&mut app);
        }
        {
            let v = app.world_mut().resource_mut::<CanvasView>().unwrap();
            v.editor.camera = rndr_canvas::Camera {
                pan_x: 40.,
                pan_y: 40.,
                zoom: 0.62,
            };
            v.editor.select(&["plan".into()]);
        }
        for _ in 0..4 {
            rndr_paint::frame(&mut app);
        }
        let mut shoot = |app: &mut App, name: &str| {
            let (_, image) = rndr_paint::frame_with_snapshot_frame(app, |world, frame| {
                exporter
                    .render_frame(Some(world), frame, 1400, 900, 1.)
                    .unwrap()
            });
            image::save_buffer(
                format!("verification/native-create{name}-{suffix}.png"),
                &image.rgba,
                image.width,
                image.height,
                image::ColorType::Rgba8,
            )
            .unwrap();
        };
        shoot(&mut app, "");
        // Style palette, shape picker and details open together.
        with_state(app.world_mut(), |w, s| {
            canvas::run(w, s, canvas::Do::Palette);
            canvas::run(w, s, canvas::Do::Details);
            canvas::run(w, s, canvas::Do::Shapes);
        });
        for _ in 0..4 {
            rndr_paint::frame(&mut app);
        }
        shoot(&mut app, "-palette");
        // Ask Medousa with a proposal previewing over the canvas.
        with_state(app.world_mut(), |w, s| {
            canvas::run(w, s, canvas::Do::Palette);
            canvas::run(w, s, canvas::Do::Details);
            canvas::run(w, s, canvas::Do::Shapes);
            canvas::run(w, s, canvas::Do::Tool(Tool::Select));
            canvas::run(w, s, canvas::Do::Ask);
            s.ask.list = None;
            let recolor = {
                let mut n = w
                    .resource::<CanvasView>()
                    .unwrap()
                    .editor
                    .nodes()
                    .into_iter()
                    .find(|n| n.id == "plan")
                    .unwrap();
                if let Content::Shape { fill, .. } = &mut n.content {
                    *fill = "#9474dc".into();
                }
                Operation::put(n)
            };
            s.ask.view = Some(ask::View {
                id: "job-1".into(),
                source: ask::Source::Job,
                status: medousa_types::CreativeJobStatus::Ready,
                session: "session-1".into(),
                label: Some("Make the plan step stand out in violet".into()),
                operations: vec![recolor],
                base: Some(document.document.revision),
                error: None,
            });
            s.ask.preview = true;
        });
        app.world_mut()
            .resource_mut::<CanvasView>()
            .unwrap()
            .editor
            .select(&["plan".into()]);
        for _ in 0..4 {
            rndr_paint::frame(&mut app);
        }
        shoot(&mut app, "-ask");
    }
}

fn opened() -> App {
    let mut app = app();
    take_commands(app.world_mut());
    act(app.world_mut(), Action::Open("doc".into()));
    let get = take_commands(app.world_mut())
        .into_iter()
        .find(|c| matches!(c.kind, Kind::Get(_)))
        .unwrap();
    reply(
        app.world_mut(),
        &get,
        Ok(Response::Document(Box::new(document("doc", "Board")))),
    );
    app.update();
    app
}

fn run(w: &mut World, a: canvas::Do) {
    with_state(w, |w, s| canvas::run(w, s, a));
    sync(w);
}

fn editor(w: &World) -> &CanvasEditor {
    &w.resource::<CanvasView>().unwrap().editor
}

/// Names of the controls under the floating chrome, for asserting what shows.
fn shown(w: &World) -> Vec<String> {
    let mut out = vec![];
    let mut stack: Vec<Entity> = w
        .entities_with::<Semantics>()
        .into_iter()
        .filter(|e| {
            let body = state(w).unwrap().slot;
            let body = w.get::<Parent>(body).unwrap().entity;
            crate::surfaces::descendant(w, *e, body) && !hidden(w, *e)
        })
        .collect();
    for e in stack.drain(..) {
        if let Some(name) = w.get::<Semantics>(e).and_then(|s| s.name.clone()) {
            out.push(name);
        }
    }
    out
}
fn hidden(w: &World, mut e: Entity) -> bool {
    loop {
        if w.has::<Hidden>(e) {
            return true;
        }
        match w.get::<Parent>(e) {
            Some(p) => e = p.entity,
            None => return false,
        }
    }
}

#[test]
fn the_tool_strip_picks_tools_and_the_shape_picker_follows_it() {
    let mut app = opened();
    let w = app.world_mut();
    let names = shown(w);
    for name in [
        "Select (V)",
        "Draw (P)",
        "Shapes (R)",
        "Text (T)",
        "Undo",
        "Style",
        "Zoom in",
    ] {
        assert!(names.iter().any(|n| n == name), "{name} in {names:?}");
    }
    assert!(
        !names.iter().any(|n| n == "Ellipse"),
        "picker starts closed"
    );
    run(w, canvas::Do::Shapes);
    assert_eq!(editor(w).tool(), Tool::Rectangle);
    assert!(shown(w).iter().any(|n| n == "Ellipse"));
    run(w, canvas::Do::Tool(Tool::Ellipse));
    assert_eq!(editor(w).tool(), Tool::Ellipse);
    assert!(
        !shown(w).iter().any(|n| n == "Ellipse"),
        "picking closes it"
    );
    run(w, canvas::Do::Tool(Tool::Pen));
    assert_eq!(editor(w).tool(), Tool::Pen);
}

#[test]
fn style_palette_changes_the_selection_and_the_next_object() {
    let mut app = opened();
    let w = app.world_mut();
    w.resource_mut::<CanvasView>()
        .unwrap()
        .editor
        .commit("Add rectangle", vec![rectangle("r")]);
    w.resource_mut::<CanvasView>()
        .unwrap()
        .editor
        .select(&["r".into()]);
    run(w, canvas::Do::Palette);
    assert!(shown(w).iter().any(|n| n == "Blue"));
    run(w, canvas::Do::Color("#4e75d9"));
    run(w, canvas::Do::Fill(canvas::Fill::Soft));
    run(w, canvas::Do::Size(3));
    let node = editor(w).nodes().into_iter().find(|n| n.id == "r").unwrap();
    let Content::Shape {
        stroke,
        fill,
        stroke_width,
        ..
    } = node.content
    else {
        panic!("shape")
    };
    assert_eq!(stroke, "#4e75d9");
    assert_eq!(fill, "#4e75d924", "medousa-home's soft wash");
    assert_eq!(stroke_width, 12.);
    // Text sizes follow medousa-home's scale.
    run(w, canvas::Do::Tool(Tool::Text));
    run(w, canvas::Do::Size(1));
    assert_eq!(editor(w).style.font_size, 32.);
}

#[test]
fn selection_actions_and_layers_act_on_the_canvas() {
    let mut app = opened();
    let w = app.world_mut();
    w.resource_mut::<CanvasView>()
        .unwrap()
        .editor
        .commit("Add rectangle", vec![rectangle("r")]);
    w.resource_mut::<CanvasView>()
        .unwrap()
        .editor
        .select(&["r".into()]);
    sync(w);
    let names = shown(w);
    for name in [
        "Align",
        "Arrange",
        "Duplicate selection",
        "Delete selection",
    ] {
        assert!(names.iter().any(|n| n == name), "{name} in {names:?}");
    }
    run(w, canvas::Do::Duplicate);
    assert_eq!(editor(w).nodes().len(), 3);
    run(w, canvas::Do::Details);
    let names = shown(w);
    assert!(
        names.iter().filter(|n| *n == "Hide layer").count() >= 3,
        "{names:?}"
    );
    run(w, canvas::Do::Visible("r".into()));
    assert!(
        !editor(w)
            .nodes()
            .into_iter()
            .find(|n| n.id == "r")
            .unwrap()
            .visible
    );
    // Hidden layers are not selectable; showing it again makes it pickable.
    run(w, canvas::Do::Layer("r".into()));
    assert!(editor(w).selection().is_empty());
    run(w, canvas::Do::Visible("r".into()));
    run(w, canvas::Do::Layer("r".into()));
    assert_eq!(editor(w).selection(), ["r".to_string()]);
}

#[test]
fn imported_images_land_centered_in_the_selected_or_first_frame() {
    let frame = Starter::Diagram.scene().nodes.remove(0);
    let nodes = vec![frame.clone()];
    // Larger than the frame: scaled to fit, centered, parented.
    let (parent, t) = placement(&nodes, &[], 2400, 920);
    assert_eq!(parent.as_deref(), Some(frame.id.as_str()));
    assert_eq!((t.width, t.height), (1200., 460.));
    assert_eq!((t.x, t.y), (0., 230.));
    // Small images keep their size.
    let (_, t) = placement(&nodes, &[], 100, 50);
    assert_eq!((t.width, t.height, t.x, t.y), (100., 50., 550., 435.));
    // No frame: a 1200 × 920 area at the origin.
    let (parent, _) = placement(&[], &[], 10, 10);
    assert!(parent.is_none());
}

#[test]
fn an_idle_project_is_reread_and_edits_made_elsewhere_appear() {
    let mut app = opened();
    let w = app.world_mut();
    take_commands(w);
    state_mut(w).unwrap().polled = std::time::Instant::now() - POLL;
    sync(w);
    let commands = take_commands(w);
    let poll = commands
        .iter()
        .find(|c| matches!(c.kind, Kind::Get(_)))
        .expect("idle re-read");
    sync(w);
    assert!(
        !take_commands(w)
            .iter()
            .any(|c| matches!(c.kind, Kind::Get(_))),
        "one read at a time"
    );
    let mut newer = document("doc", "Board");
    newer.document =
        rndr_canvas::apply_operations(&newer.document, &[rectangle("theirs")]).unwrap();
    newer.document.revision = 3;
    reply(w, poll, Ok(Response::Document(Box::new(newer))));
    assert!(editor(w).nodes().iter().any(|n| n.id == "theirs"));
    // Not while an edit of ours is unsaved.
    w.resource_mut::<CanvasView>()
        .unwrap()
        .editor
        .commit("Add mine", vec![rectangle("mine")]);
    state_mut(w).unwrap().polled = std::time::Instant::now() - POLL;
    sync(w);
    assert!(
        !take_commands(w)
            .iter()
            .any(|c| matches!(c.kind, Kind::Get(_)))
    );
}

#[test]
fn an_imported_image_joins_the_canvas_as_one_undoable_edit() {
    // Asset ids are `a_` and the content's SHA-256, as the daemon names them.
    const ASSET: &str = "a_0000000000000000000000000000000000000000000000000000000000000000";
    let mut app = opened();
    let w = app.world_mut();
    take_commands(w);
    run(w, canvas::Do::Image);
    let commands = take_commands(w);
    let import = commands
        .iter()
        .find(|c| matches!(c.kind, Kind::Import(_)))
        .expect("picker and upload run on the worker");
    run(w, canvas::Do::Image);
    assert!(
        !take_commands(w)
            .iter()
            .any(|c| matches!(c.kind, Kind::Import(_))),
        "one import at a time"
    );
    let mut after = document("doc", "Board");
    after.document.assets.insert(
        ASSET.into(),
        rndr_canvas::Asset {
            id: ASSET.into(),
            mime: "image/png".into(),
            byte_size: 4,
            sha256: "0".repeat(64),
        },
    );
    after.document.revision = 1;
    reply(
        w,
        import,
        Ok(Response::Imported(Box::new(Imported {
            asset: ASSET.into(),
            width: 2,
            height: 1,
            rgba: vec![255; 8],
            document: after,
        }))),
    );
    let e = editor(w);
    let image = e
        .nodes()
        .into_iter()
        .find(|n| matches!(&n.content, Content::Image { asset_id, .. } if asset_id == ASSET))
        .expect("image node");
    assert_eq!(e.selection(), std::slice::from_ref(&image.id));
    assert!(e.is_saving(), "saved through the worker like any edit");
    assert_eq!(
        w.resource::<CanvasView>()
            .unwrap()
            .images
            .get(ASSET)
            .copied(),
        Some(texture_id("doc", ASSET))
    );
}

fn ask_view(id: &str, status: medousa_types::CreativeJobStatus, ops: Vec<Operation>) -> ask::View {
    ask::View {
        id: id.into(),
        source: ask::Source::Job,
        status,
        session: "session-1".into(),
        label: (!ops.is_empty()).then(|| "Softer palette".into()),
        operations: ops,
        base: Some(0),
        error: None,
    }
}

fn ask_command(commands: &[Command], f: impl Fn(&ask::AskKind) -> bool) -> Command {
    commands
        .iter()
        .find(|c| matches!(&c.kind, Kind::Ask(_, k) if f(k)))
        .cloned()
        .expect("ask command")
}

#[test]
fn asking_medousa_previews_the_proposal_and_using_it_reloads_the_canvas() {
    use medousa_types::CreativeJobStatus as S;
    let mut app = opened();
    let w = app.world_mut();
    // Opening the project read its latest request.
    let latest = latest_read(w);
    reply(w, &latest, Ok(Response::Ask(ask::AskReply::Latest(None))));

    run(w, canvas::Do::Ask);
    take_commands(w);
    // Nothing selected: refused with medousa-home's words, nothing sent.
    state_mut(w).unwrap().ask.draft = "Make it playful".into();
    run(w, canvas::Do::AskSend);
    assert!(
        take_commands(w)
            .iter()
            .all(|c| !matches!(c.kind, Kind::Ask(..)))
    );
    assert!(
        state(w)
            .unwrap()
            .ask
            .error
            .as_deref()
            .unwrap()
            .starts_with("Select something")
    );

    let frame = editor(w).nodes()[0].id.clone();
    w.resource_mut::<CanvasView>()
        .unwrap()
        .editor
        .select(std::slice::from_ref(&frame));
    state_mut(w).unwrap().ask.draft = "Make it playful".into();
    run(w, canvas::Do::AskSend);
    let start = ask_command(&take_commands(w), |k| matches!(k, ask::AskKind::Start(_)));
    let Kind::Ask(_, ask::AskKind::Start(request)) = &start.kind else {
        unreachable!()
    };
    assert_eq!(request.selected_ids, std::slice::from_ref(&frame));
    assert_eq!(request.prompt, "Make it playful");
    assert_eq!(request.expected_revision, 0);
    assert!(names_contains(w, "Sending request…") || state(w).unwrap().ask.sending());

    reply(
        w,
        &start,
        Ok(Response::Ask(ask::AskReply::Job(ask_view(
            &request.id,
            S::Running,
            vec![],
        )))),
    );
    assert!(state(w).unwrap().ask.draft.is_empty());
    assert!(shown(w).iter().any(|n| n == "Cancel creative request"));
    // A running request is checked again after a moment.
    state_mut(w).unwrap().ask.polled =
        Some(std::time::Instant::now() - std::time::Duration::from_secs(3));
    sync(w);
    let check = ask_command(&take_commands(w), |k| matches!(k, ask::AskKind::Job(_)));
    reply(
        w,
        &check,
        Ok(Response::Ask(ask::AskReply::Job(ask_view(
            &request.id,
            S::Ready,
            vec![rectangle("proposed")],
        )))),
    );
    // A fresh proposal opens on its preview without saving anything.
    let e = editor(w);
    assert!(e.previewing());
    assert!(e.nodes().iter().any(|n| n.id == "proposed"));
    assert!(
        !e.mutation()
            .document
            .scene
            .nodes
            .iter()
            .any(|n| n.id == "proposed")
    );
    run(w, canvas::Do::AskPreview(false));
    assert!(
        !editor(w).previewing(),
        "Original shows the canvas as saved"
    );

    run(w, canvas::Do::AskUse);
    let decide = ask_command(&take_commands(w), |k| matches!(k, ask::AskKind::Decide(..)));
    reply(
        w,
        &decide,
        Ok(Response::Ask(ask::AskReply::Decided(
            Some(ask_view(
                &request.id,
                S::Accepted,
                vec![rectangle("proposed")],
            )),
            true,
        ))),
    );
    // The daemon applied it; the canvas reads the document it produced.
    assert!(
        take_commands(w)
            .iter()
            .any(|c| matches!(c.kind, Kind::Get(_)))
    );
    assert!(shown(w).iter().any(|n| n == "Open conversation"));
}

/// The latest-request read a project issues on open. The app's own update
/// dispatches it, so it is rebuilt from the serial the surface waits on.
fn latest_read(w: &mut World) -> Command {
    take_commands(w);
    Command {
        serial: state(w).unwrap().ask.list.expect("read on open"),
        kind: Kind::Ask("doc".into(), ask::AskKind::Latest),
    }
}

fn names_contains(w: &World, text: &str) -> bool {
    w.entities_with::<Label>()
        .into_iter()
        .any(|e| w.get::<Label>(e).is_some_and(|l| l.text == text))
}

#[test]
fn a_lost_request_is_retried_with_the_same_id_and_a_refusal_is_not() {
    let mut app = opened();
    let w = app.world_mut();
    take_commands(w);
    run(w, canvas::Do::Ask);
    let frame = editor(w).nodes()[0].id.clone();
    w.resource_mut::<CanvasView>()
        .unwrap()
        .editor
        .select(&[frame]);
    state_mut(w).unwrap().ask.draft = "Tighten the spacing".into();
    run(w, canvas::Do::AskSend);
    let start = ask_command(&take_commands(w), |k| matches!(k, ask::AskKind::Start(_)));
    reply(
        w,
        &start,
        Err(CanvasError::Storage("connection reset".into())),
    );
    assert!(
        state(w).unwrap().ask.pending.is_some(),
        "kept for an exact retry"
    );
    assert!(shown(w).iter().any(|n| n == "Retry request"));
    run(w, canvas::Do::AskRetry);
    let again = ask_command(&take_commands(w), |k| matches!(k, ask::AskKind::Start(_)));
    let (Kind::Ask(_, ask::AskKind::Start(a)), Kind::Ask(_, ask::AskKind::Start(b))) =
        (&start.kind, &again.kind)
    else {
        unreachable!()
    };
    assert_eq!(a.id, b.id);
    reply(w, &again, Err(CanvasError::Invalid("too large".into())));
    assert!(
        state(w).unwrap().ask.pending.is_none(),
        "a refusal never admitted work"
    );
}

#[test]
fn a_proposal_from_a_conversation_waits_on_the_canvas() {
    let mut app = opened();
    let w = app.world_mut();
    let latest = latest_read(w);
    let mut view = ask_view(
        "p1",
        medousa_types::CreativeJobStatus::Ready,
        vec![rectangle("chat")],
    );
    view.source = ask::Source::Proposal;
    view.session.clear();
    reply(
        w,
        &latest,
        Ok(Response::Ask(ask::AskReply::Latest(Some(view)))),
    );
    run(w, canvas::Do::Ask);
    take_commands(w);
    assert!(shown(w).iter().any(|n| n == "Dismiss proposal"));
    run(w, canvas::Do::AskDismiss);
    let decide = ask_command(&take_commands(w), |k| matches!(k, ask::AskKind::Decide(..)));
    let Kind::Ask(_, ask::AskKind::Decide(source, id, decision)) = &decide.kind else {
        unreachable!()
    };
    assert_eq!(
        (*source, id.as_str(), decision.accept),
        (ask::Source::Proposal, "p1", false)
    );
}

/// Presses and releases the pointer at the center of the control named `name`.
fn click_named(app: &mut App, name: &str) {
    for _ in 0..3 {
        app.update();
    }
    let w = app.world();
    let target = w
        .entities_with::<Semantics>()
        .into_iter()
        .filter(|e| !hidden(w, *e) && w.has::<Focusable>(*e))
        .find(|e| w.get::<Semantics>(*e).and_then(|s| s.name.as_deref()) == Some(name))
        .unwrap_or_else(|| panic!("{name} shown"));
    let r = *w.get::<Rect>(target).unwrap();
    let at = [r.x + r.w / 2., r.y + r.h / 2.];
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.position = at;
    input.pointer.press_at = Some(at);
    input.pointer.pressed = true;
    input.pointer.down = true;
    input.pointer.button = PointerButton::Left;
    app.update();
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.pressed = false;
    input.pointer.down = false;
    input.pointer.released = true;
    app.update();
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.released = false;
    app.update();
}

#[test]
fn canvas_controls_answer_real_clicks() {
    let mut app = opened();
    click_named(&mut app, "Draw (P)");
    assert_eq!(editor(app.world()).tool(), Tool::Pen);
    click_named(&mut app, "Style");
    assert!(
        shown(app.world()).iter().any(|n| n == "Blue"),
        "palette opened"
    );
    click_named(&mut app, "Zoom in");
    assert!(editor(app.world()).camera.zoom > 1.0);
    click_named(&mut app, "Shapes (R)");
    click_named(&mut app, "Ellipse");
    assert_eq!(editor(app.world()).tool(), Tool::Ellipse);
    click_named(&mut app, "Details");
    assert!(shown(app.world()).iter().any(|n| n == "Hide layer"));
    click_named(&mut app, "Ask Medousa");
    assert!(state(app.world()).unwrap().ask.open);
    click_named(&mut app, "Close Ask Medousa");
    assert!(!state(app.world()).unwrap().ask.open);
    // A selection-bar action edits the document.
    click_named(&mut app, "Select (V)");
    let frame = editor(app.world()).nodes()[0].id.clone();
    app.world_mut()
        .resource_mut::<CanvasView>()
        .unwrap()
        .editor
        .select(std::slice::from_ref(&frame));
    click_named(&mut app, "Duplicate selection");
    assert_eq!(editor(app.world()).nodes().len(), 2);
}

#[test]
fn a_click_on_bare_canvas_still_takes_focus_from_the_sidebar() {
    let mut app = opened();
    let sidebar = state(app.world()).unwrap().sidebar;
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(sidebar);
    let slot = state(app.world()).unwrap().slot;
    let r = *app.world().get::<Rect>(slot).unwrap();
    // Middle of the canvas, clear of every floating control.
    let at = [r.x + r.w / 2., r.y + r.h / 2.];
    assert_eq!(hit_test(app.world(), at[0], at[1]), Some(slot));
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.position = at;
    input.pointer.press_at = Some(at);
    input.pointer.pressed = true;
    input.pointer.down = true;
    app.update();
    assert_eq!(app.world().resource::<Focus>().unwrap().entity, None);
}

fn pointer(app: &mut App, at: [f32; 2], pressed: bool, down: bool, released: bool) {
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.position = at;
    if pressed {
        input.pointer.press_at = Some(at);
    }
    input.pointer.pressed = pressed;
    input.pointer.down = down;
    input.pointer.released = released;
    input.pointer.button = PointerButton::Left;
    app.update();
}

#[test]
fn drawing_and_selecting_on_the_canvas_with_the_pointer() {
    let mut app = opened();
    click_named(&mut app, "Shapes (R)");
    click_named(&mut app, "Rectangle");
    assert_eq!(editor(app.world()).tool(), Tool::Rectangle);
    let slot = state(app.world()).unwrap().slot;
    let r = *app.world().get::<Rect>(slot).unwrap();
    let a = [r.x + r.w * 0.3, r.y + r.h * 0.3];
    let b = [r.x + r.w * 0.5, r.y + r.h * 0.5];
    let before = editor(app.world()).nodes().len();
    pointer(&mut app, a, true, true, false);
    pointer(&mut app, b, false, true, false);
    pointer(&mut app, b, false, false, true);
    pointer(&mut app, b, false, false, false);
    let e = editor(app.world());
    eprintln!(
        "tool {:?} nodes {} sel {:?} err {:?}",
        e.tool(),
        e.nodes().len(),
        e.selection(),
        e.last_error()
    );
    assert_eq!(e.nodes().len(), before + 1, "a rectangle was drawn");
    // Clicking the new shape with Select picks it, even as one fast click.
    click_named(&mut app, "Select (V)");
    app.world_mut()
        .resource_mut::<CanvasView>()
        .unwrap()
        .editor
        .select(&[]);
    // On the left edge: an unfilled shape is picked by its outline.
    let mid = [a[0], (a[1] + b[1]) / 2.];
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.position = mid;
    input.pointer.press_at = Some(mid);
    input.pointer.pressed = true;
    input.pointer.down = false;
    input.pointer.released = true;
    app.update();
    let e = editor(app.world());
    eprintln!("after click sel {:?}", e.selection());
    assert_eq!(e.selection().len(), 1, "fast click selects");
}

#[test]
fn canvas_controls_survive_a_sidebar_redraw() {
    let mut app = opened();
    for _ in 0..3 {
        app.update();
    }
    // The sidebar redraws on its own, as after each background re-read,
    // without the canvas controls rebuilding.
    state_mut(app.world_mut()).unwrap().redraw = true;
    app.update();
    click_named(&mut app, "Draw (P)");
    assert_eq!(editor(app.world()).tool(), Tool::Pen);
}

#[test]
#[ignore = "Create landing with recent canvases, Parley/GPU offscreen"]
fn create_landing_visual_acceptance() {
    let mut app = app();
    let w = app.world_mut();
    let commands = take_commands(w);
    let now = chrono::Utc::now();
    let ago = |days: i64| (now - chrono::Duration::days(days)).to_rfc3339();
    reply(
        w,
        &commands[0],
        Ok(Response::Listed(CreativeListResponse {
            projects: vec![
                summary("a", "Launch board", true, &ago(0)),
                summary("b", "Onboarding flow concept", false, &ago(1)),
                summary("c", "Settings screenshot markup", false, &ago(3)),
                summary("d", "", false, &ago(12)),
            ],
            next_cursor: None,
        })),
    );
    sync(w);
    crate::ui::visual_tests::render_dark(&mut app, "native-landing-create-recent");
}
