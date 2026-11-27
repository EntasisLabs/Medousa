//! Hermetic tests of the real retained-resource coordinator, not stand-in panels.
use super::*;
use crate::{
    surfaces,
    ui::{self, conversations},
};
use rndr_core::editing::{DocumentId, SourceDocuments};

fn fixture() -> App {
    let mut app = ui::app();
    app.world_mut().insert_resource(Viewport::new(1400., 900.));
    app.update();
    app
}

#[derive(Clone)]
struct ChatFixture {
    route: u64,
    root: Entity,
    original: Entity,
    document: DocumentId,
    draft: String,
    resource: Resource,
}
fn chat(app: &mut App, session: &str, draft: &str) -> ChatFixture {
    conversations::workspace_new(app.world_mut());
    let m = app.world_mut().resource_mut::<ui::Model>().unwrap();
    m.owner = "fixture-owner".into();
    m.chat
        .hydrate(session.into(), "assistant".into(), vec![], None)
        .unwrap();
    let document = m.document_id();
    let doc = app
        .world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(document)
        .unwrap();
    doc.reload(doc.revision(), draft).unwrap();
    let item = conversations::workspace_items(app.world())
        .into_iter()
        .find(|i| i.session.as_deref() == Some(session))
        .unwrap();
    ChatFixture {
        route: item.route,
        root: item.entity,
        original: app.world().get::<Parent>(item.entity).unwrap().entity,
        document,
        draft: draft.into(),
        resource: Resource::Chat {
            session_id: session.into(),
            workshop_id: item.owner,
        },
    }
}
fn enter(app: &mut App) {
    surfaces::select(app.world_mut(), SurfaceId::Workspace);
    sync(app.world_mut());
}
fn command(app: &mut App, a: Action) {
    let s = app.world_mut().resource_mut::<State>().unwrap();
    s.next += 1;
    let id = s.next;
    s.actions.insert(id, a);
    assert!(action(app.world_mut(), &HostIntent::Command(id)));
}
fn assert_retained(app: &App, chats: &[&ChatFixture]) {
    let w = app.world();
    for c in chats {
        assert!(
            w.get::<Layout>(c.root).is_some(),
            "canonical root was despawned"
        );
        assert_eq!(
            w.resource::<SourceDocuments>()
                .unwrap()
                .get(c.document)
                .unwrap()
                .text()
                .to_string(),
            c.draft
        );
        assert_eq!(
            conversations::workspace_items(w)
                .iter()
                .find(|i| i.route == c.route)
                .unwrap()
                .entity,
            c.root
        );
    }
}
fn tab(w: &World, resource: &Resource) -> u64 {
    w.resource::<State>()
        .unwrap()
        .model
        .active()
        .groups
        .iter()
        .flat_map(|g| &g.tabs)
        .find(|t| &t.resource == resource)
        .unwrap()
        .id
}

#[test]
fn arrangement_changes_keep_canonical_chat_roots_and_source_documents() {
    let mut app = fixture();
    let a = chat(&mut app, "chat-a", "draft A: keep undo and ownership");
    let b = chat(&mut app, "chat-b", "draft B: independent source");
    enter(&mut app);
    command(&mut app, Action::Open(a.resource.clone()));
    command(&mut app, Action::Open(b.resource.clone()));
    let first = app
        .world()
        .resource::<State>()
        .unwrap()
        .model
        .active()
        .focused_group;
    let b_tab = tab(app.world(), &b.resource);
    command(&mut app, Action::Split(Edge::Right));
    let second = app
        .world()
        .resource::<State>()
        .unwrap()
        .model
        .active()
        .focused_group;
    assert_ne!(first, second);
    app.world_mut()
        .resource_mut::<State>()
        .unwrap()
        .model
        .move_tab(b_tab, second, 0)
        .unwrap();
    sync(app.world_mut());
    for _ in 0..4 {
        app.update();
    }
    assert_retained(&app, &[&a, &b]);
    for c in [&a, &b] {
        assert!(!app.world().has::<Hidden>(c.root));
    }
    assert_ne!(
        app.world().get::<Parent>(a.root).unwrap().entity,
        app.world().get::<Parent>(b.root).unwrap().entity
    );
    command(&mut app, Action::Zoom);
    assert_retained(&app, &[&a, &b]);
    assert!(app.world().has::<Hidden>(a.root));
    assert!(!app.world().has::<Hidden>(b.root));
    command(&mut app, Action::Zoom);
    let original_desktop = app
        .world()
        .resource::<State>()
        .unwrap()
        .model
        .active_desktop;
    command(&mut app, Action::Duplicate);
    assert_ne!(
        app.world()
            .resource::<State>()
            .unwrap()
            .model
            .active_desktop,
        original_desktop
    );
    assert_retained(&app, &[&a, &b]);
    let copy_tab = tab(app.world(), &b.resource);
    app.world_mut()
        .resource_mut::<State>()
        .unwrap()
        .model
        .close_tab(copy_tab)
        .unwrap();
    sync(app.world_mut());
    assert!(app.world().has::<Hidden>(b.root));
    assert_retained(&app, &[&a, &b]);
    command(&mut app, Action::Desktop(original_desktop));
    for _ in 0..3 {
        app.update();
    }
    assert!(!app.world().has::<Hidden>(b.root));
    assert_retained(&app, &[&a, &b]);
    // The dedicated Chat destination must reclaim its own active root immediately.
    surfaces::select(app.world_mut(), SurfaceId::Chat);
    for _ in 0..3 {
        app.update();
    }
    for c in [&a, &b] {
        assert_eq!(
            app.world().get::<Parent>(c.root).unwrap().entity,
            c.original
        );
    }
    assert!(conversations::workspace_focus(app.world_mut(), b.route));
    app.update();
    assert!(!app.world().has::<Hidden>(b.root));
    assert_retained(&app, &[&a, &b]);
}

#[test]
fn missing_reference_uses_workspace_context_not_previous_chat() {
    let mut app = fixture();
    let a = chat(&mut app, "present", "do not dispatch this draft");
    enter(&mut app);
    command(&mut app, Action::Open(a.resource.clone()));
    assert_eq!(surfaces::effective(app.world()), SurfaceId::Chat);
    command(
        &mut app,
        Action::Open(Resource::Chat {
            session_id: "not-loaded".into(),
            workshop_id: "other-owner".into(),
        }),
    );
    assert_eq!(surfaces::effective(app.world()), SurfaceId::Workspace);
    assert_eq!(focused_surface(app.world()), None);
    assert!(app.world().has::<Hidden>(a.root));
    assert_retained(&app, &[&a]);
}

#[test]
fn pending_promotion_is_fenced_by_kind_owner_and_terminal_work() {
    let chat = |s: &str, w: &str| Resource::Chat {
        session_id: s.into(),
        workshop_id: w.into(),
    };
    let shell = |s: &str, w: &str, work: Option<&str>| Resource::Terminal {
        session_id: s.into(),
        workshop_id: w.into(),
        work_id: work.map(str::to_owned),
    };
    assert!(pending_promotes(
        &chat("pending:1", "a"),
        &chat("session", "a")
    ));
    assert!(!pending_promotes(
        &chat("pending:1", "a"),
        &chat("session", "b")
    ));
    assert!(!pending_promotes(&chat("session", "a"), &chat("next", "a")));
    assert!(!pending_promotes(
        &chat("pending:1", "a"),
        &chat("pending:2", "a")
    ));
    assert!(pending_promotes(
        &shell("pending:1", "a", None),
        &shell("s", "a", None)
    ));
    assert!(!pending_promotes(
        &shell("pending:1", "a", None),
        &shell("s", "a", Some("work"))
    ));
    assert!(!pending_promotes(
        &shell("pending:1", "a", Some("w")),
        &shell("s", "b", Some("w"))
    ));
    assert!(!pending_promotes(
        &chat("pending:1", "a"),
        &shell("s", "a", None)
    ));
}

#[test]
fn sidebar_import_moves_existing_resource_into_focused_empty_group() {
    let mut app = fixture();
    let a = chat(&mut app, "a", "draft A");
    let b = chat(&mut app, "b", "draft B");
    enter(&mut app);
    command(&mut app, Action::Open(a.resource.clone()));
    command(&mut app, Action::Open(b.resource.clone()));
    command(&mut app, Action::Split(Edge::Right));
    let group = app
        .world()
        .resource::<State>()
        .unwrap()
        .model
        .active()
        .focused_group;
    command(&mut app, Action::Open(a.resource.clone()));
    assert_eq!(
        app.world()
            .resource::<State>()
            .unwrap()
            .model
            .active()
            .tab_group(tab(app.world(), &a.resource)),
        Some(group)
    );
    assert_eq!(
        app.world()
            .resource::<State>()
            .unwrap()
            .model
            .active()
            .groups
            .len(),
        2
    );
    command(&mut app, Action::Open(a.resource.clone()));
    assert_retained(&app, &[&a, &b]);
    assert_eq!(
        app.world()
            .resource::<State>()
            .unwrap()
            .model
            .active()
            .groups
            .iter()
            .map(|g| g.tabs.len())
            .sum::<usize>(),
        2
    );
}
#[test]
fn empty_group_accepts_only_tab_payloads_owned_by_this_workspace() {
    let mut app = fixture();
    let a = chat(&mut app, "a", "keep A");
    let b = chat(&mut app, "b", "keep B");
    enter(&mut app);
    command(&mut app, Action::Open(a.resource.clone()));
    command(&mut app, Action::Open(b.resource.clone()));
    command(&mut app, Action::Split(Edge::Right));
    let s = app.world().resource::<State>().unwrap();
    let group = s.model.active().focused_group;
    let c = s.chrome.as_ref().unwrap();
    let target = c.groups.iter().find(|g| g.id == group).unwrap().root;
    let owner = c
        .groups
        .iter()
        .find_map(|g| g.strip.map(|h| h.root))
        .unwrap();
    assert!(
        app.world()
            .get::<rndr_ui::drag::DropTarget>(target)
            .unwrap()
            .kinds
            .contains(&rndr_ui::tabs::TAB)
    );
    let mut drop = rndr_ui::drag::DragDrop {
        payload: rndr_ui::drag::DragPayload {
            kind: rndr_ui::tabs::TAB,
            value: tab(app.world(), &a.resource),
            owner: None,
        },
        target,
        place: rndr_ui::drag::DropPlace::Inside,
    };
    assert!(!empty_drop(app.world_mut(), drop));
    drop.payload.owner = Some(owner);
    assert!(empty_drop(app.world_mut(), drop));
    sync(app.world_mut());
    assert_eq!(
        app.world()
            .resource::<State>()
            .unwrap()
            .model
            .active()
            .tab_group(tab(app.world(), &a.resource)),
        Some(group)
    );
    assert_retained(&app, &[&a, &b]);
}
#[test]
fn empty_group_close_keeps_backing_resources() {
    let mut app = fixture();
    let a = chat(&mut app, "a", "keep draft");
    enter(&mut app);
    command(&mut app, Action::Open(a.resource.clone()));
    command(&mut app, Action::Split(Edge::Bottom));
    let group = app
        .world()
        .resource::<State>()
        .unwrap()
        .model
        .active()
        .focused_group;
    command(&mut app, Action::CloseGroup(group));
    assert_eq!(
        app.world()
            .resource::<State>()
            .unwrap()
            .model
            .active()
            .groups
            .len(),
        1
    );
    assert_retained(&app, &[&a]);
}
#[test]
fn same_group_move_uses_pre_removal_drop_boundaries() {
    let mut m = Multiplexer::default();
    let mut ids = vec![];
    for s in ["a", "b", "c", "d"] {
        ids.push(
            m.open(
                Resource::Chat {
                    session_id: s.into(),
                    workshop_id: "owner".into(),
                },
                s,
            )
            .unwrap(),
        );
    }
    let g = m.active().focused_group;
    let order = |m: &Multiplexer| {
        m.active()
            .group(g)
            .unwrap()
            .tabs
            .iter()
            .map(|t| t.id)
            .collect::<Vec<_>>()
    };
    move_at(&mut m, ids[0], g, 4).unwrap();
    assert_eq!(order(&m), vec![ids[1], ids[2], ids[3], ids[0]]);
    move_at(&mut m, ids[0], g, 0).unwrap();
    assert_eq!(order(&m), ids);
    move_at(&mut m, ids[1], g, 2).unwrap();
    assert_eq!(
        order(&m),
        ids,
        "dropping immediately after oneself is a no-op"
    );
    move_at(&mut m, ids[3], g, 1).unwrap();
    assert_eq!(order(&m), vec![ids[0], ids[3], ids[1], ids[2]]);
}

#[test]
fn split_ratio_updates_keep_live_sash_entities_across_syncs() {
    let mut app = fixture();
    let a = chat(&mut app, "ratio", "draft");
    enter(&mut app);
    command(&mut app, Action::Open(a.resource.clone()));
    command(&mut app, Action::Split(Edge::Right));
    for _ in 0..4 {
        app.update();
    }
    let handle = app
        .world()
        .resource::<State>()
        .unwrap()
        .chrome
        .as_ref()
        .unwrap()
        .splits[0]
        .1;
    for ratio in [0.35, 0.65, 0.45] {
        app.world_mut()
            .resource_mut::<State>()
            .unwrap()
            .model
            .set_split_ratio(&[], ratio)
            .unwrap();
        sync(app.world_mut());
        sync(app.world_mut());
        assert_eq!(
            app.world()
                .resource::<State>()
                .unwrap()
                .chrome
                .as_ref()
                .unwrap()
                .splits[0]
                .1,
            handle
        );
    }
    // Native event path must also update persistence without rebuilding the drag owner.
    send(
        app.world_mut(),
        SplitSashChange {
            sash: handle.sash,
            ratio: 0.6,
        },
    );
    app.update();
    sync(app.world_mut());
    match &app.world().resource::<State>().unwrap().model.active().root {
        SplitNode::Split { ratio, .. } => assert!((*ratio - 0.6).abs() < 0.001),
        _ => panic!("split lost"),
    }
    assert_eq!(
        app.world()
            .resource::<State>()
            .unwrap()
            .chrome
            .as_ref()
            .unwrap()
            .splits[0]
            .1,
        handle
    );
    for width in [1400., 760., 1600.] {
        app.world_mut().insert_resource(Viewport::new(width, 900.));
        for _ in 0..4 {
            app.update();
        }
        let rect = app.world().get::<Rect>(handle.root).unwrap();
        assert!(rect.w.is_finite() && rect.w > 0. && rect.h > 0.);
        assert_retained(&app, &[&a]);
    }
}

#[test]
fn composing_chat_refuses_workspace_topology_transition() {
    use rndr_core::editing::{attached_source, prepare_source_input};
    let mut app = fixture();
    let a = chat(&mut app, "ime", "native composition");
    enter(&mut app);
    command(&mut app, Action::Open(a.resource.clone()));
    let editor = app
        .world()
        .entities_with::<Buffer>()
        .into_iter()
        .find(|e| attached_source(app.world(), *e) == Some(a.document))
        .unwrap();
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(editor);
    app.world_mut().resource_mut::<Input>().unwrap().ime_preedit = "かな".into();
    prepare_source_input(app.world_mut());
    assert!(
        app.world()
            .resource::<SourceDocuments>()
            .unwrap()
            .get(a.document)
            .unwrap()
            .is_composing()
    );
    let before = app.world().resource::<State>().unwrap().model.to_json();
    command(&mut app, Action::Split(Edge::Right));
    assert_eq!(
        app.world().resource::<State>().unwrap().model.to_json(),
        before
    );
    assert_retained(&app, &[&a]);
}

/// Synthetic GPU acceptance: no daemon, PTY process, app control, or network.
#[test]
#[ignore = "Mixed Workspace Parley/GPU offscreen acceptance"]
fn mixed_chat_and_terminal_dark_light_narrow_visual_acceptance() {
    use std::sync::Arc;
    let mut app = fixture();
    let dir = tempfile::tempdir().unwrap();
    let _channels = crate::workshops::install_fixture(
        app.world_mut(),
        dir.path().join("sources.json"),
        &[("fixture-owner", "Synthetic workshop")],
    );
    let a = chat(
        &mut app,
        "mixed-chat",
        "Keep this draft while moving between desktops.",
    );
    assert!(crate::workshops::navigate_binding(
        app.world_mut(),
        SurfaceId::Terminal,
        "fixture-owner"
    ));
    let terminal = crate::terminal::tests::preview(app.world_mut(), "workspace-visual");
    enter(&mut app);
    command(&mut app, Action::Open(a.resource.clone()));
    command(&mut app, Action::Split(Edge::Right));
    let shell = items(app.world())
        .into_iter()
        .find(|i| i.entity == terminal)
        .unwrap();
    command(&mut app, Action::Open(shell.resource));
    app.world_mut()
        .insert_resource(Arc::new(rndr_render::ParleyText::new()) as Arc<dyn TextEngine>);
    app.world_mut()
        .insert_resource(rndr_ui::InteractionMotion::snap());
    let mut exporter = rndr_render::Exporter::new().unwrap();
    std::fs::create_dir_all("verification").unwrap();
    for (suffix, width, height, dark) in [
        ("dark", 1600u32, 1000u32, true),
        ("light", 1600, 1000, false),
        ("narrow", 900, 900, true),
    ] {
        app.world_mut()
            .insert_resource(Viewport::new(width as f32, height as f32));
        publish_system_appearance(
            app.world_mut(),
            SystemAppearance {
                dark: Some(dark),
                ..Default::default()
            },
        );
        for _ in 0..10 {
            app.update();
        }
        assert_retained(&app, &[&a]);
        assert!(!app.world().has::<Hidden>(a.root));
        assert!(!app.world().has::<Hidden>(terminal));
        let (_, image) = rndr_paint::frame_with_snapshot_frame(&mut app, |world, frame| {
            exporter
                .render_frame(Some(world), frame, width, height, 1.)
                .unwrap()
        });
        image::save_buffer(
            format!("verification/native-workspace-mixed-{suffix}.png"),
            &image.rgba,
            image.width,
            image.height,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}
