//! Synthetic input/layout checks, not native keyboard/IME/a11y qualification.
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
fn activate(a: &mut App, action: &str) {
    let e = button(a, action);
    assert_eq!(try_activate(a.world_mut(), e), ActivationResult::Accepted);
    settle(a);
}
fn key(a: &mut App, named: NamedKey) {
    a.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(named, Modifiers::new())];
    a.update();
    settle(a);
}
fn text(a: &App) -> String {
    a.world()
        .entities_with::<Label>()
        .into_iter()
        .map(|e| a.world().get::<Label>(e).unwrap().text.clone())
        .collect::<Vec<_>>()
        .join("\n")
}
fn rows() -> Vec<Record> {
    (0..80)
        .map(|i| {
            Record::new(
                format!("Journal {i}"),
                vec![
                    ("Vault-relative path", format!("journal/{i}.md")),
                    ("Modified UTC", "2026-11-13T00:00:00Z".into()),
                ],
            )
        })
        .collect()
}
fn ready(a: &mut App) {
    a.world_mut()
        .resource_mut::<Desktop>()
        .unwrap()
        .reads
        .insert(Resource::Notes, ReadState::Ready(rows()));
    desktop::action(a.world_mut(), "desk:notes");
    settle(a);
}
#[test]
fn notes_keyboard_detail_back_filter_resize_and_chat_isolation() {
    for width in [480., 1120.] {
        let (mut a, mut commands) = ui_tests::ready_unverified();
        a.world_mut().insert_resource(Viewport::new(width, 650.));
        let ui = *a.world().resource::<Ui>().unwrap();
        reset_prompt(a.world_mut(), ui.composer, "private unsent draft 👋");
        ready(&mut a);
        assert_eq!(
            a.world().resource::<Desktop>().unwrap().route,
            Route::Data(Resource::Notes)
        );
        assert!(
            a.world()
                .get::<Semantics>(button(&a, "desk:notes"))
                .unwrap()
                .name
                .as_ref()
                .unwrap()
                .contains("current destination")
        );
        let h = a.world().resource::<Handles>().unwrap().clone();
        a.world_mut().get_mut::<Scroll>(h.body).unwrap().offset[1] = -200.;
        let row = button(&a, "record:7");
        request_focus(a.world_mut(), row);
        settle(&mut a);
        let offset = a.world().get::<Scroll>(h.body).unwrap().offset;
        key(&mut a, NamedKey::Enter);
        assert_eq!(a.world().resource::<Desktop>().unwrap().selected, Some(7));
        assert_eq!(
            a.world().resource::<Focus>().unwrap().entity,
            Some(button(&a, "admin:back"))
        );
        assert!(text(&a).contains("journal/7.md"));
        a.world_mut()
            .insert_resource(Viewport::new(if width < 800. { 1120. } else { 480. }, 650.));
        settle(&mut a);
        key(&mut a, NamedKey::Enter);
        assert_eq!(a.world().resource::<Desktop>().unwrap().selected, None);
        assert_eq!(
            a.world().resource::<Focus>().unwrap().entity,
            Some(button(&a, "record:7"))
        );
        // Responsive layout may reveal focus; same-size Back preserves exact offset.
        a.world_mut().get_mut::<Scroll>(h.body).unwrap().offset = offset;
        reset_prompt(a.world_mut(), h.search, "journal/7.md");
        settle(&mut a);
        activate(&mut a, "record:7");
        activate(&mut a, "admin:back");
        assert_eq!(
            a.world().resource::<Desktop>().unwrap().query,
            "journal/7.md"
        );
        assert!(text(&a).contains("1 of 80 entries"));
        reset_prompt(a.world_mut(), h.search, "no-match");
        settle(&mut a);
        assert!(text(&a).contains("No matching entries"));
        desktop::action(a.world_mut(), "record:7");
        settle(&mut a);
        assert_eq!(a.world().resource::<Desktop>().unwrap().selected, None);
        activate(&mut a, "desk:history");
        assert!(a.world().resource::<Desktop>().unwrap().query.is_empty());
        activate(&mut a, "desk:chat");
        assert_eq!(
            a.world().get::<Label>(ui.composer).unwrap().text,
            "private unsent draft 👋"
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
        assert!(!composer::native_input_ready(a.world()));
        assert!(commands.try_recv().is_err());
    }
}
#[test]
fn notes_plain_text_labels_never_enter_markdown_or_link_rendering() {
    let mut a = app();
    a.update();
    let title = "<script>alert(1)</script> [link](https://example.invalid)";
    a.world_mut()
        .resource_mut::<Desktop>()
        .unwrap()
        .reads
        .insert(
            Resource::Notes,
            ReadState::Ready(vec![Record::new(
                title,
                vec![("Vault-relative path", "journal/literal.md".into())],
            )]),
        );
    desktop::navigate(a.world_mut(), Route::Data(Resource::Notes));
    settle(&mut a);
    assert!(a.world().entities_with::<MarkdownView>().is_empty());
    let e = button(&a, "record:0");
    assert_eq!(a.world().get::<Label>(e).unwrap().text, title);
    activate(&mut a, "record:0");
    assert!(text(&a).contains(title));
    assert!(a.world().entities_with::<MarkdownView>().is_empty());
    assert!(!a.world().entities_with::<Button>().into_iter().any(|e| {
        a.world()
            .get::<Button>(e)
            .unwrap()
            .action
            .contains("example.invalid")
    }));
}
#[test]
fn notes_first_detail_path_is_visible_at_minimum_size_with_real_text() {
    let mut a = app();
    a.update();
    a.world_mut().insert_resource(
        std::sync::Arc::new(rndr_render::ParleyText::new()) as std::sync::Arc<dyn TextEngine>
    );
    a.world_mut().insert_resource(Viewport::new(480., 650.));
    ready(&mut a);
    activate(&mut a, "record:0");
    let h = a.world().resource::<Handles>().unwrap();
    let e = a
        .world()
        .entities_with::<Label>()
        .into_iter()
        .find(|e| a.world().get::<Label>(*e).unwrap().text == "journal/0.md")
        .unwrap();
    let r = a.world().get::<Rect>(e).unwrap();
    let port = a.world().get::<Rect>(h.body).unwrap();
    let offset = a.world().get::<Scroll>(h.body).unwrap().offset[1];
    assert!(
        r.y + offset >= port.y && r.y + offset + r.h <= port.y + port.h,
        "first path below fold: {r:?} {port:?}"
    );
}
#[test]
fn notes_back_restores_same_size_scroll_and_focus() {
    let mut a = app();
    a.update();
    ready(&mut a);
    let h = a.world().resource::<Handles>().unwrap().clone();
    a.world_mut().get_mut::<Scroll>(h.body).unwrap().offset[1] = -200.;
    let row = button(&a, "record:7");
    request_focus(a.world_mut(), row);
    settle(&mut a);
    let offset = a.world().get::<Scroll>(h.body).unwrap().offset;
    key(&mut a, NamedKey::Enter);
    key(&mut a, NamedKey::Enter);
    assert_eq!(a.world().get::<Scroll>(h.body).unwrap().offset, offset);
    assert_eq!(
        a.world().resource::<Focus>().unwrap().entity,
        Some(button(&a, "record:7"))
    );
}
#[test]
fn notes_failure_empty_offline_and_loading_are_distinct_and_inert() {
    for (state, expected) in [
        (ReadState::NotLoaded, "Offline / not connected"),
        (ReadState::Loading(1), "Loading…"),
        (
            ReadState::Ready(vec![]),
            "No visible notes in this first page",
        ),
        (ReadState::Error(ReadError::Permission), "Permission denied"),
        (ReadState::Error(ReadError::Unavailable), "Read unavailable"),
        (
            ReadState::Error(ReadError::Incompatible),
            "Unsupported or incompatible",
        ),
        (
            ReadState::Error(ReadError::Disconnected),
            "Connection worker stopped",
        ),
    ] {
        let mut a = app();
        a.update();
        a.world_mut()
            .resource_mut::<Desktop>()
            .unwrap()
            .reads
            .insert(Resource::Notes, state);
        desktop::navigate(a.world_mut(), Route::Data(Resource::Notes));
        settle(&mut a);
        let labels = text(&a);
        assert!(labels.contains(expected), "missing {expected}: {labels}");
        assert!(labels.contains("not the whole vault"));
        assert!(
            !a.world()
                .get::<Button>(button(&a, "admin:refresh"))
                .unwrap()
                .enabled
        );
        assert!(!a.world().entities_with::<Button>().into_iter().any(|e| {
            a.world()
                .get::<Button>(e)
                .unwrap()
                .action
                .starts_with("record:")
        }));
    }
}
#[test]
fn notes_refresh_serial_fence_clears_detail_and_preserves_query_without_cross_resource_updates() {
    let mut a = app();
    a.update();
    ready(&mut a);
    let h = a.world().resource::<Handles>().unwrap().clone();
    reset_prompt(a.world_mut(), h.search, "Journal 7");
    settle(&mut a);
    activate(&mut a, "record:7");
    let serial = a
        .world_mut()
        .resource_mut::<Desktop>()
        .unwrap()
        .begin(Resource::Notes)
        .unwrap();
    settle(&mut a);
    assert_eq!(a.world().resource::<Desktop>().unwrap().selected, None);
    assert!(
        a.world_mut()
            .resource_mut::<Desktop>()
            .unwrap()
            .begin(Resource::Notes)
            .is_none()
    );
    send(
        a.world_mut(),
        admin_backend::ReadReply {
            resource: Resource::Notes,
            serial: serial + 99,
            result: Ok(vec![]),
        },
    );
    settle(&mut a);
    assert!(matches!(
        a.world().resource::<Desktop>().unwrap().reads[&Resource::Notes],
        ReadState::Loading(_)
    ));
    send(
        a.world_mut(),
        admin_backend::ReadReply {
            resource: Resource::Notes,
            serial,
            result: Err(ReadError::Permission),
        },
    );
    settle(&mut a);
    assert!(text(&a).contains("Permission denied"));
    assert_eq!(a.world().resource::<Desktop>().unwrap().query, "Journal 7");
    activate(&mut a, "desk:history");
    let serial = a
        .world_mut()
        .resource_mut::<Desktop>()
        .unwrap()
        .begin(Resource::Notes)
        .unwrap();
    send(
        a.world_mut(),
        admin_backend::ReadReply {
            resource: Resource::Notes,
            serial,
            result: Ok(rows()),
        },
    );
    settle(&mut a);
    assert_eq!(
        a.world().resource::<Desktop>().unwrap().route,
        Route::Data(Resource::Activity)
    );
    assert!(!text(&a).contains("Journal 7"));
    // Window-local snapshots and filters cannot migrate to a new app/session.
    let mut other = app();
    other.update();
    assert!(
        !other
            .world()
            .resource::<Desktop>()
            .unwrap()
            .reads
            .contains_key(&Resource::Notes)
    );
}
