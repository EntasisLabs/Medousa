//! App input/update/layout regressions, not physical keyboard or screen-reader proof.
use super::*;
use admin_model::{Desktop, ReadError, ReadState, Record, Resource, Route};
use desktop::Handles;
fn settle(a: &mut App) {
    for _ in 0..8 {
        a.update();
    }
}
fn button(a: &App, action: &str) -> Entity {
    a.world()
        .entities_with::<Button>()
        .into_iter()
        .find(|e| a.world().get::<Button>(*e).unwrap().action == action)
        .expect(action)
}
fn key(a: &mut App, named: NamedKey, shift: bool) {
    a.world_mut().resource_mut::<Input>().unwrap().keys = vec![KeyEvent::named(
        named,
        Modifiers {
            shift,
            ..Modifiers::new()
        },
    )];
    a.update();
    settle(a);
}
fn focus_action(a: &App) -> String {
    let e = a.world().resource::<Focus>().unwrap().entity.unwrap();
    a.world()
        .get::<Button>(e)
        .map(|b| b.action.clone())
        .unwrap_or_default()
}
fn activate(a: &mut App, action: &str) {
    let e = button(a, action);
    assert_eq!(try_activate(a.world_mut(), e), ActivationResult::Accepted);
    settle(a);
}
fn rows() -> Vec<Record> {
    (0..80)
        .map(|i| {
            Record::new(
                format!("Work item {i}"),
                vec![("Board state", "In flight".into())],
            )
        })
        .collect()
}
#[test]
fn chooser_keyboard_current_name_escape_and_selection_preserve_chat() {
    for width in [480., 1120.] {
        let (mut a, mut commands) = ui_tests::ready_unverified();
        a.world_mut().insert_resource(Viewport::new(width, 650.));
        let ui = *a.world().resource::<Ui>().unwrap();
        reset_prompt(a.world_mut(), ui.composer, "draft 👋");
        desktop::navigate(a.world_mut(), Route::Settings(3));
        settle(&mut a);
        let chooser = a.world().resource::<Handles>().unwrap().chooser;
        assert!(request_focus(a.world_mut(), chooser));
        key(&mut a, NamedKey::Enter, false);
        let root = a.world().resource::<menus::Menus>().unwrap().root.unwrap();
        assert!(!a.world().has::<Hidden>(root));
        assert_eq!(focus_action(&a), "settings:3");
        let current = a.world().resource::<Focus>().unwrap().entity.unwrap();
        assert!(
            a.world()
                .get::<Semantics>(current)
                .unwrap()
                .name
                .as_ref()
                .unwrap()
                .contains("current settings view")
        );
        key(&mut a, NamedKey::Tab, false);
        assert_eq!(focus_action(&a), "settings:4");
        key(&mut a, NamedKey::Tab, true);
        assert_eq!(focus_action(&a), "settings:3");
        // A responsive page rebuild must not delete the popup anchor or its focus.
        a.world_mut()
            .insert_resource(Viewport::new(if width < 800. { 1120. } else { 480. }, 760.));
        settle(&mut a);
        assert_eq!(a.world().resource::<Handles>().unwrap().chooser, chooser);
        assert_eq!(focus_action(&a), "settings:3");
        key(&mut a, NamedKey::Escape, false);
        assert!(a.world().has::<Hidden>(root));
        assert_eq!(a.world().resource::<Focus>().unwrap().entity, Some(chooser));
        assert_eq!(
            a.world().resource::<Desktop>().unwrap().route,
            Route::Settings(3)
        );
        key(&mut a, NamedKey::Enter, false);
        key(&mut a, NamedKey::End, false);
        assert_eq!(focus_action(&a), "settings:8");
        key(&mut a, NamedKey::Home, false);
        assert_eq!(focus_action(&a), "settings:0");
        key(&mut a, NamedKey::End, false);
        key(&mut a, NamedKey::Enter, false);
        assert_eq!(
            a.world().resource::<Desktop>().unwrap().route,
            Route::Settings(8)
        );
        assert_eq!(focus_action(&a), "settings:choose");
        assert!(
            a.world()
                .get::<Semantics>(chooser)
                .unwrap()
                .name
                .as_ref()
                .unwrap()
                .contains("Connection")
        );
        activate(&mut a, "desk:chat");
        assert_eq!(
            focus_action(&a),
            "desk:chat",
            "return focus must not land on Settings or hidden composer"
        );
        activate(&mut a, "desk:settings");
        assert_eq!(
            a.world().resource::<Desktop>().unwrap().route,
            Route::Settings(8)
        );
        assert_eq!(
            a.world().get::<Label>(ui.composer).unwrap().text,
            "draft 👋"
        );
        assert_eq!(
            a.world()
                .resource::<Model>()
                .unwrap()
                .chat
                .session
                .as_deref(),
            Some("s")
        );
        assert!(commands.try_recv().is_err());
        assert!(!composer::native_input_ready(a.world()));
    }
}
#[test]
fn activity_first_detail_metadata_is_visible_at_minimum_size() {
    let mut a = app();
    a.update();
    a.world_mut().insert_resource(
        std::sync::Arc::new(rndr_render::ParleyText::new()) as std::sync::Arc<dyn TextEngine>
    );
    a.world_mut().insert_resource(Viewport::new(480., 650.));
    a.world_mut()
        .insert_resource(desktop_fixtures::SyntheticDesktop);
    a.world_mut()
        .resource_mut::<Desktop>()
        .unwrap()
        .reads
        .insert(Resource::Activity, ReadState::Ready(rows()));
    desktop::navigate(a.world_mut(), Route::Data(Resource::Activity));
    let d = a.world_mut().resource_mut::<Desktop>().unwrap();
    d.selected = Some(0);
    d.dirty = true;
    settle(&mut a);
    let h = a.world().resource::<Handles>().unwrap();
    let state = a
        .world()
        .entities_with::<Label>()
        .into_iter()
        .find(|e| a.world().get::<Label>(*e).unwrap().text == "In flight")
        .unwrap();
    let r = a.world().get::<Rect>(state).unwrap();
    let port = a.world().get::<Rect>(h.body).unwrap();
    let offset = a.world().get::<Scroll>(h.body).unwrap().offset[1];
    assert!(
        r.y + offset >= port.y && r.y + offset + r.h <= port.y + port.h,
        "metadata below fold: {r:?} {port:?}"
    );
}
#[test]
fn narrow_settings_header_contains_wrapped_fixture_title() {
    let mut a = app();
    a.update();
    a.world_mut().insert_resource(
        std::sync::Arc::new(rndr_render::ParleyText::new()) as std::sync::Arc<dyn TextEngine>
    );
    a.world_mut().insert_resource(Viewport::new(480., 650.));
    desktop::navigate(a.world_mut(), Route::Settings(3));
    settle(&mut a);
    let title = a.world().resource::<Handles>().unwrap().title;
    a.world_mut().get_mut::<Label>(title).unwrap().text = "FIXTURE · Runtime Controls".into();
    settle(&mut a);
    let header = a.world().get::<Parent>(title).unwrap().entity;
    let r = a.world().get::<Rect>(title).unwrap();
    let port = a.world().get::<Rect>(header).unwrap();
    // Layout Rects can be clamped even when ink crowds the boundary. Lock the
    // explicit two-line-space policy; actual pixels are separately inspected.
    assert!(port.h >= 60., "reserve two-line space beside the chooser");
    assert!(
        r.y + r.h <= port.y + port.h + 0.5,
        "title extends beyond header: {r:?} {port:?}"
    );
    let h = a.world().resource::<Handles>().unwrap();
    assert_eq!(
        a.world().get::<Label>(h.chooser).unwrap().text,
        "Choose settings view"
    );
    assert_eq!(
        a.world().get::<Label>(h.automation_heading).unwrap().text,
        "Auto"
    );
}
#[test]
fn fixed_dock_is_visible_while_primary_doors_scroll_at_minimum_window() {
    let mut a = app();
    a.update();
    a.world_mut().insert_resource(Viewport::new(480., 650.));
    settle(&mut a);
    let h = a.world().resource::<Handles>().unwrap().clone();
    assert!(!a.world().has::<Scroll>(h.rail));
    assert!(a.world().has::<Scroll>(h.primary_rail));
    for action in ["desk:history", "desk:settings", "desk:profiles"] {
        let e = button(&a, action);
        assert_eq!(a.world().get::<Parent>(e).unwrap().entity, h.dock);
        let r = a.world().get::<Rect>(e).unwrap();
        assert!(r.y >= 0. && r.y + r.h <= 650., "{action}: {r:?}");
    }
    let before = *a.world().get::<Rect>(h.dock).unwrap();
    a.world_mut()
        .get_mut::<Scroll>(h.primary_rail)
        .unwrap()
        .offset[1] = -160.;
    settle(&mut a);
    assert_eq!(*a.world().get::<Rect>(h.dock).unwrap(), before);
    activate(&mut a, "desk:history");
    assert_eq!(
        a.world().resource::<Desktop>().unwrap().route,
        Route::Data(Resource::Activity)
    );
    let e = button(&a, "desk:history");
    assert!(
        a.world()
            .get::<Semantics>(e)
            .unwrap()
            .name
            .as_ref()
            .unwrap()
            .contains("current destination")
    );
}
#[test]
fn activity_detail_back_retains_list_scroll_filter_and_keyboard_origin() {
    let mut a = app();
    a.update();
    a.world_mut()
        .resource_mut::<Desktop>()
        .unwrap()
        .reads
        .insert(Resource::Activity, ReadState::Ready(rows()));
    desktop::navigate(a.world_mut(), Route::Data(Resource::Activity));
    settle(&mut a);
    let h = a.world().resource::<Handles>().unwrap().clone();
    a.world_mut().get_mut::<Scroll>(h.body).unwrap().offset[1] = -200.;
    settle(&mut a);
    assert!(a.world().get::<Scroll>(h.body).unwrap().offset[1] < 0.);
    let row = button(&a, "record:7");
    request_focus(a.world_mut(), row);
    settle(&mut a); // canonical focus visibility may reveal an offscreen row
    let offset = a.world().get::<Scroll>(h.body).unwrap().offset;
    key(&mut a, NamedKey::Enter, false);
    assert_eq!(a.world().resource::<Desktop>().unwrap().selected, Some(7));
    assert_eq!(focus_action(&a), "admin:back");
    key(&mut a, NamedKey::Enter, false);
    assert_eq!(a.world().resource::<Desktop>().unwrap().selected, None);
    assert_eq!(focus_action(&a), "record:7");
    assert_eq!(a.world().get::<Scroll>(h.body).unwrap().offset, offset);
    reset_prompt(a.world_mut(), h.search, "Work item 7");
    settle(&mut a);
    activate(&mut a, "record:7");
    activate(&mut a, "admin:back");
    assert_eq!(
        a.world().resource::<Desktop>().unwrap().query,
        "Work item 7"
    );
    assert_eq!(focus_action(&a), "record:7");
}
#[test]
fn background_activity_reply_does_not_replace_settings_control_or_chat_projection() {
    let (mut a, mut commands) = ui_tests::ready_unverified();
    let ui = *a.world().resource::<Ui>().unwrap();
    reset_prompt(a.world_mut(), ui.composer, "private unsent draft");
    desktop::navigate(a.world_mut(), Route::Settings(0));
    settle(&mut a);
    let light = button(&a, "appearance:light");
    request_focus(a.world_mut(), light);
    let serial = a
        .world_mut()
        .resource_mut::<Desktop>()
        .unwrap()
        .begin(Resource::Activity)
        .unwrap();
    send(
        a.world_mut(),
        admin_backend::ReadReply {
            resource: Resource::Activity,
            serial,
            result: Ok(rows()),
        },
    );
    settle(&mut a);
    assert_eq!(a.world().resource::<Focus>().unwrap().entity, Some(light));
    assert_eq!(
        a.world().get::<Label>(ui.composer).unwrap().text,
        "private unsent draft"
    );
    assert_eq!(a.world().resource::<Model>().unwrap().generation, 0);
    assert!(commands.try_recv().is_err());
}
#[test]
fn unrelated_and_stale_replies_preserve_activity_selection_and_scroll() {
    let mut a = app();
    a.update();
    a.world_mut()
        .resource_mut::<Desktop>()
        .unwrap()
        .reads
        .insert(Resource::Activity, ReadState::Ready(rows()));
    desktop::navigate(a.world_mut(), Route::Data(Resource::Activity));
    settle(&mut a);
    activate(&mut a, "record:7");
    let h = a.world().resource::<Handles>().unwrap().clone();
    let focus = a.world().resource::<Focus>().unwrap().entity;
    let offset = a.world().get::<Scroll>(h.body).unwrap().offset;
    let serial = a
        .world_mut()
        .resource_mut::<Desktop>()
        .unwrap()
        .begin(Resource::Models)
        .unwrap();
    send(
        a.world_mut(),
        admin_backend::ReadReply {
            resource: Resource::Models,
            serial,
            result: Err(ReadError::Unavailable),
        },
    );
    send(
        a.world_mut(),
        admin_backend::ReadReply {
            resource: Resource::Activity,
            serial: 999,
            result: Ok(vec![]),
        },
    );
    settle(&mut a);
    assert_eq!(a.world().resource::<Desktop>().unwrap().selected, Some(7));
    assert_eq!(a.world().resource::<Focus>().unwrap().entity, focus);
    assert_eq!(a.world().get::<Scroll>(h.body).unwrap().offset, offset);
    assert!(
        matches!(a.world().resource::<Desktop>().unwrap().reads.get(&Resource::Activity), Some(ReadState::Ready(r)) if r.len() == 80)
    );
}
