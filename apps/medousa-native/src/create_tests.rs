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
