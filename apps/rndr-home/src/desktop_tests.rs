//! Synthetic app-local evidence only: not physical IME or native-window proof.
use super::*;
use admin_model::{Desktop, ReadError, ReadState, Record, Resource, Route, SETTINGS};
use desktop::{Connection, Handles};

fn settle(app: &mut App) {
    for _ in 0..8 {
        app.update();
    }
}
fn text(world: &World) -> String {
    world
        .entities_with::<Label>()
        .iter()
        .map(|e| world.get::<Label>(*e).unwrap().text.clone())
        .collect::<Vec<_>>()
        .join("\n")
}
fn activate(app: &mut App, action: &str) {
    let e = app
        .world()
        .entities_with::<Button>()
        .into_iter()
        .find(|e| app.world().get::<Button>(*e).unwrap().action == action)
        .expect(action);
    assert_eq!(try_activate(app.world_mut(), e), ActivationResult::Accepted);
    settle(app);
}
#[test]
fn routes_are_available_without_native_input_and_never_send_chat_commands() {
    for width in [480., 1120.] {
        let (mut app, mut chat_rx) = ui_tests::ready_unverified();
        app.world_mut().insert_resource(Viewport::new(width, 820.));
        let ui = *app.world().resource::<Ui>().unwrap();
        reset_prompt(app.world_mut(), ui.composer, "unsent draft 👋");
        app.world_mut().resource_mut::<Model>().unwrap().chat.busy = true;
        settle(&mut app);
        activate(&mut app, "desk:settings");
        for (index, (_, title, _)) in SETTINGS.iter().enumerate() {
            assert!(desktop::action(
                app.world_mut(),
                &format!("settings:{index}")
            ));
            settle(&mut app);
            assert_eq!(
                app.world().resource::<Desktop>().unwrap().route,
                Route::Settings(index)
            );
            assert_eq!(
                app.world()
                    .get::<Label>(app.world().resource::<Handles>().unwrap().title)
                    .unwrap()
                    .text,
                *title
            );
            assert!(
                app.world()
                    .has::<Hidden>(app.world().resource::<Handles>().unwrap().chat)
            );
        }
        for (index, _) in Resource::ALL.iter().enumerate() {
            desktop::action(app.world_mut(), &format!("data:{index}"));
            settle(&mut app);
            assert!(text(app.world()).contains("No live data or fixture results loaded"));
        }
        activate(&mut app, "desk:chat");
        assert_eq!(
            app.world().get::<Label>(ui.composer).unwrap().text,
            "unsent draft 👋"
        );
        assert_eq!(app.world().resource::<Model>().unwrap().generation, 0);
        assert!(chat_rx.try_recv().is_err());
        assert!(!composer::native_input_ready(app.world()));
        assert!(!app.world().get::<Button>(ui.send).unwrap().enabled);
    }
}
#[test]
fn navigation_preserves_away_history_scroll_session_and_draft() {
    let (mut app, mut chat_rx) = ui_tests::ready_unverified();
    let ui = *app.world().resource::<Ui>().unwrap();
    app.world_mut()
        .resource_mut::<Model>()
        .unwrap()
        .history_messages = (0..1000)
        .map(|i| {
            (
                "assistant".into(),
                format!("Synthetic {i}\n\n{}", "Paragraph. ".repeat(40)),
            )
        })
        .collect();
    reset_prompt(app.world_mut(), ui.composer, "retained draft");
    settle(&mut app);
    app.world_mut()
        .get_mut::<Scroll>(ui.conversation)
        .unwrap()
        .offset[1] = -200.;
    settle(&mut app);
    let before = app.world().get::<Scroll>(ui.conversation).unwrap().offset;
    activate(&mut app, "desk:settings");
    activate(&mut app, "desk:chat");
    assert_eq!(
        app.world().get::<Scroll>(ui.conversation).unwrap().offset,
        before
    );
    assert_eq!(
        app.world().get::<Label>(ui.composer).unwrap().text,
        "retained draft"
    );
    assert_eq!(
        app.world()
            .resource::<Model>()
            .unwrap()
            .chat
            .session
            .as_deref(),
        Some("s")
    );
    assert!(chat_rx.try_recv().is_err());
}
#[test]
fn loading_error_empty_filter_and_detail_are_distinct_and_read_only() {
    let (mut app, mut chat_rx) = ui_tests::ready_unverified();
    let (tx, mut reads) = mpsc::unbounded_channel();
    app.world_mut().resource_mut::<Connection>().unwrap().sender = Some(tx);
    desktop::navigate(app.world_mut(), Route::Data(Resource::Runtimes));
    settle(&mut app);
    let request = reads.try_recv().unwrap();
    assert_eq!(request.resource, Resource::Runtimes);
    assert!(text(app.world()).contains("Loading…"));
    // Refresh during loading is deduplicated even for forged intents.
    desktop::action(app.world_mut(), "admin:refresh");
    assert!(reads.try_recv().is_err());
    for (result, expected) in [
        (Err(ReadError::Permission), "Permission denied"),
        (Ok(Vec::new()), "No entries returned"),
        (Err(ReadError::Unavailable), "Read unavailable"),
        (Err(ReadError::Incompatible), "incompatible response"),
    ] {
        let serial = match app.world().resource::<Desktop>().unwrap().reads[&Resource::Runtimes] {
            ReadState::Loading(n) => n,
            _ => panic!(),
        };
        send(
            app.world_mut(),
            admin_backend::ReadReply {
                resource: Resource::Runtimes,
                serial,
                result,
            },
        );
        settle(&mut app);
        assert!(text(app.world()).contains(expected));
        activate(&mut app, "admin:refresh");
        assert!(reads.try_recv().is_ok());
    }
    let serial = app.world().resource::<Desktop>().unwrap().serial;
    send(
        app.world_mut(),
        admin_backend::ReadReply {
            resource: Resource::Runtimes,
            serial,
            result: Ok(vec![
                Record::new(
                    "Synthetic runtime",
                    vec![
                        ("Available", "No".into()),
                        ("Account status", "unknown".into()),
                    ],
                ),
                Record::new("Other runtime", vec![("Available", "Yes".into())]),
            ]),
        },
    );
    settle(&mut app);
    activate(&mut app, "record:0");
    assert!(text(app.world()).contains("Account status"));
    activate(&mut app, "admin:back");
    let search = app.world().resource::<Handles>().unwrap().search;
    reset_prompt(app.world_mut(), search, "no match");
    settle(&mut app);
    assert!(text(app.world()).contains("No matching entries"));
    reset_prompt(app.world_mut(), search, "SYNTHETIC");
    settle(&mut app);
    assert!(text(app.world()).contains("1 of 2 entries"));
    assert!(reads.try_recv().is_err());
    assert!(chat_rx.try_recv().is_err());
}
#[test]
fn stale_results_are_resource_scoped_and_not_relabelled_as_current_route() {
    let mut d = Desktop::default();
    let serial = d.begin(Resource::Health).unwrap();
    assert!(d.begin(Resource::Health).is_none());
    d.navigate(Route::Data(Resource::Models));
    d.settle(Resource::Health, serial + 1, Ok(vec![]));
    assert_eq!(d.reads[&Resource::Health], ReadState::Loading(serial));
    d.settle(Resource::Health, serial, Err(ReadError::Permission));
    assert_eq!(d.route, Route::Data(Resource::Models));
    assert!(!d.reads.contains_key(&Resource::Models));
    let next = d.begin(Resource::Health).unwrap();
    d.settle(Resource::Health, serial, Ok(vec![]));
    assert_eq!(d.reads[&Resource::Health], ReadState::Loading(next));
}
#[test]
fn local_appearance_and_filter_never_persist_or_emit_runtime_commands() {
    let (mut app, mut chat_rx) = ui_tests::ready_unverified();
    activate(&mut app, "desk:settings");
    activate(&mut app, "appearance:light");
    assert_eq!(
        presentation::theme(app.world()).appearance,
        Appearance::Light
    );
    activate(&mut app, "appearance:contrast");
    assert_eq!(presentation::theme(app.world()).contrast, Contrast::High);
    assert!(chat_rx.try_recv().is_err());
    let mut fresh = super::app();
    fresh.update();
    assert_eq!(
        presentation::theme(fresh.world()).appearance,
        Appearance::Dark
    );
}
#[test]
fn invalid_routes_and_disconnected_worker_never_panic_or_fake_ready() {
    let mut app = app();
    app.update();
    for action in ["settings:999", "data:999", "desk:unknown", "record:999"] {
        desktop::action(app.world_mut(), action);
    }
    assert_eq!(
        app.world().resource::<Desktop>().unwrap().route,
        Route::Chat
    );
    let (tx, rx) = mpsc::unbounded_channel();
    drop(rx);
    app.world_mut().resource_mut::<Connection>().unwrap().sender = Some(tx);
    desktop::navigate(app.world_mut(), Route::Data(Resource::Health));
    settle(&mut app);
    assert_eq!(
        app.world().resource::<Desktop>().unwrap().reads[&Resource::Health],
        ReadState::Error(ReadError::Disconnected)
    );
    assert!(text(app.world()).contains("Connection worker stopped"));
}
#[cfg(feature = "observe")]
#[test]
fn admin_worker_snapshots_or_input_cannot_be_public_native_observation() {
    use rndr_render::observe::ObservationContent;
    let (mut app, _) = ui_tests::ready();
    app.world_mut().resource_mut::<Model>().unwrap().commands = None;
    app.world_mut()
        .insert_resource(fixtures::FixtureObservation);
    let search = app.world().resource::<Handles>().unwrap().search;
    reset_prompt(app.world_mut(), search, "private filter");
    settle(&mut app);
    assert!(matches!(
        app.world().resource::<ObservationContent>(),
        Some(ObservationContent::Deny)
    ));
    reset_prompt(app.world_mut(), search, "");
    app.world_mut()
        .resource_mut::<Desktop>()
        .unwrap()
        .reads
        .insert(Resource::Health, ReadState::Ready(vec![]));
    settle(&mut app);
    assert!(matches!(
        app.world().resource::<ObservationContent>(),
        Some(ObservationContent::Deny)
    ));
}
