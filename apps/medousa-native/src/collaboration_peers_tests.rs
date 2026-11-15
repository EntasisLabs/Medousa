use super::*;
use serde_json::json;
const AUTH: &str = "auth_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
pub(crate) fn row() -> PeerProposalReviewRecord {
    let request = json!({"assignment_id":"assignment","idempotency_key":"key","owner_principal_id":"owner","owner_session":{"authority_id":"a","session_id":"parent"},"channel":{"authority_id":"a","channel_id":"channel"},"target":{"authority_id":"a","execution_runtime_id":"workshop","runtime":"codex"},"context":{"manifest_id":"ctx_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","sources":[],"created_by":"owner","created_at":"2026-11-15T00:00:00Z"},"execution_session":{"authority_id":"a","session_id":"requested-not-created"},"instructions":"Native layout review\nCheck pane sizing and preserve unrelated work.","execution_grant_id":"grant","forge_work_id":"forge"});
    let binding = json!({"assignment_id":"assignment","owner_principal_id":"owner","channel":{"authority_id":"a","channel_id":"channel"},"target":{"authority_id":"a","execution_runtime_id":"workshop","runtime":"codex"},"execution_session":{"authority_id":"a","session_id":"execution"},"agent_session_id":"agent"});
    serde_json::from_str(&json!({"proposal":{"proposal_id":"proposal","request":request,"expires_at":"2026-11-16T00:00:00Z","continue_owner":false},"decision":null,"binding":binding,"receipt":null,"progress":{"state":"running","observed_at":"2026-11-15T00:00:00Z","current_activity":"Reviewing native layout"}}).to_string().replace("\"a\"", &format!("\"{AUTH}\""))).unwrap()
}
fn app_fixture() -> App {
    let mut app = app();
    app.world_mut().insert_resource(Viewport::new(1200., 850.));
    app.update();
    app.world_mut().resource_mut::<Model>().unwrap().chat.hydrate("parent".into(),AUTH.into(),vec![json!({"entry_id":"a","entry_seq":1,"role":"assistant","content":"I’m working with a layout reviewer. Inspect our exchange while I continue here."})],None).unwrap();
    for _ in 0..8 {
        app.update();
    }
    receive(
        app.world_mut(),
        "parent",
        Ok(Reading {
            rows: vec![row()],
            selected: None,
            history: None,
        }),
    );
    app.world_mut()
        .resource_mut::<Model>()
        .unwrap()
        .collaboration
        .earlier_open = true;
    for _ in 0..8 {
        app.update();
    }
    app
}
fn activate(app: &mut App, entity: Entity) {
    assert_eq!(
        try_activate(app.world_mut(), entity),
        ActivationResult::Accepted
    );
    for _ in 0..8 {
        app.update();
    }
}
fn open(app: &mut App) -> String {
    let key = key(&row());
    let entity = app
        .world()
        .query::<Target>()
        .iter()
        .find(|(_, target)| target.0 == key)
        .unwrap()
        .0;
    activate(app, entity);
    key
}
fn history() -> History {
    History {
        session: "execution".into(),
        authority: AUTH.into(),
        turns: vec![
            json!({"entry_id":"input","entry_seq":1,"role":"user","content":"Check native layout."}),
        ],
        active: Some(medousa_types::ActiveSessionTurn {
            turn_id: "live".into(),
            session_id: "execution".into(),
            stream_url: "/v1/interactive/turn/live/stream".into(),
            phase: "running".into(),
            composer_handoff: false,
            started_at: "2026-11-15T00:00:00Z".parse().unwrap(),
        }),
    }
}
fn ready(app: &mut App, id: &str) {
    receive(
        app.world_mut(),
        "parent",
        Ok(Reading {
            rows: vec![row()],
            selected: Some(id.into()),
            history: Some(Ok(history())),
        }),
    );
    for _ in 0..4 {
        app.update();
    }
}
fn event(seq: u64, event: serde_json::Value) -> medousa_types::TurnStreamEnvelopeV3 {
    serde_json::from_value(json!({"schema_version":3,"turn_id":"live","seq":seq,"emitted_at_utc":"2026-11-15T00:00:00Z","event":event})).unwrap()
}
#[test]
fn only_matching_durable_bindings_establish_local_execution_chat() {
    let good = row();

    assert_eq!(bound(&good, "parent", AUTH), Some("execution"));
    assert_eq!(bound(&good, "other", AUTH), None);
    assert_eq!(bound(&good, "parent", "remote"), None);
    let mut wrong = good.clone();
    wrong.binding = None;
    assert_eq!(
        bound(&wrong, "parent", AUTH),
        None,
        "requested execution session is not a binding"
    );
    wrong = good.clone();
    wrong.binding.as_mut().unwrap().assignment_id = "other".into();
    assert_eq!(bound(&wrong, "parent", AUTH), None);
    wrong = good.clone();
    wrong.binding.as_mut().unwrap().execution_session.session_id =
        medousa_types::SessionId::parse("parent").unwrap();
    assert_eq!(bound(&wrong, "parent", AUTH), None);
    wrong = good.clone();
    wrong.receipt = Some(ExternalPeerAssignmentReceipt {
        receipt_id: "receipt".into(),
        binding: good.binding.clone().unwrap(),
        outcome: PeerAssignmentOutcome::Failed,
        result: "Failure evidence".into(),
    });
    assert_eq!(status(&wrong), "Execution failed");
    wrong.binding = None;
    assert_eq!(
        bound(&wrong, "parent", AUTH),
        Some("execution"),
        "terminal receipt alone retains binding"
    );
}
#[test]
fn peer_live_replay_is_isolated_deduplicated_and_failure_retains_text() {
    let mut app = app_fixture();
    let id = open(&mut app);
    ready(&mut app, &id);
    let before = app
        .world()
        .resource::<Model>()
        .unwrap()
        .chat
        .snapshot()
        .unwrap()
        .clone();
    let start = event(
        1,
        json!({"type":"assistant_text_started","segment_id":"s","model_round":1}),
    );
    let delta = event(
        2,
        json!({"type":"content_append","segment_id":"s","text":"Checking clipping live."}),
    );
    stream_event(app.world_mut(), "parent", &id, Ok(start));
    stream_event(app.world_mut(), "parent", &id, Ok(delta.clone()));
    stream_event(app.world_mut(), "parent", &id, Ok(delta));
    ready(&mut app, &id); // reconnect/history overlap retains the live canonical cursor
    stream_event(app.world_mut(), "other", &id, Err("Wrong parent".into()));
    stream_event(
        app.world_mut(),
        "parent",
        "other",
        Err("Wrong selection".into()),
    );
    let state = &app.world().resource::<Model>().unwrap().collaboration.peers;
    let snapshot = state.history.as_ref().unwrap().1.snapshot().unwrap();
    assert_eq!(
        snapshot
            .items
            .iter()
            .filter(|i| i.content.contains("Checking clipping live."))
            .count(),
        1
    );
    assert_eq!(
        app.world()
            .resource::<Model>()
            .unwrap()
            .chat
            .snapshot()
            .unwrap(),
        &before
    );
    stream_event(
        app.world_mut(),
        "parent",
        &id,
        Err("Offline · Refresh to retry".into()),
    );
    let state = &app.world().resource::<Model>().unwrap().collaboration.peers;
    assert!(
        state
            .history
            .as_ref()
            .unwrap()
            .1
            .snapshot()
            .unwrap()
            .items
            .iter()
            .any(|i| i.content.contains("Checking clipping live."))
    );
    assert!(state.history_error.as_ref().unwrap().contains("Offline"));
}
#[test]
fn open_execution_uses_retained_tab_and_returns_to_same_parent_exchange_and_draft() {
    let mut app = app_fixture();
    let document = app.world().resource::<Model>().unwrap().document;
    let doc = app
        .world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(document)
        .unwrap();
    doc.reload(doc.revision(), "Keep my parent draft").unwrap();
    let parent_route = app.world().resource::<Model>().unwrap().generation;
    let id = open(&mut app);
    ready(&mut app, &id);
    let ui = *app.world().resource::<Ui>().unwrap();
    assert!(!app.world().has::<Hidden>(ui.exchange.execution));
    activate(&mut app, ui.exchange.execution);
    assert_ne!(
        app.world().resource::<Model>().unwrap().generation,
        parent_route
    );
    let route = app.world().resource::<Model>().unwrap().generation;
    conversations::reply(
        app.world_mut(),
        Reply {
            generation: route,
            update: Update::History("execution".into(), "a".into(), vec![], None),
        },
    );
    for _ in 0..4 {
        app.update();
    }
    conversations::open(app.world_mut(), Some("parent".into()));
    for _ in 0..4 {
        app.update();
    }
    assert_eq!(
        app.world().resource::<Model>().unwrap().generation,
        parent_route
    );
    assert_eq!(app.world().resource::<Model>().unwrap().document, document);
    assert_eq!(
        app.world()
            .resource::<SourceDocuments>()
            .unwrap()
            .get(document)
            .unwrap()
            .text(),
        "Keep my parent draft"
    );
    assert_eq!(
        app.world()
            .resource::<Model>()
            .unwrap()
            .collaboration
            .selected
            .as_deref(),
        Some(id.as_str())
    );
    activate(&mut app, ui.exchange.back);
    assert!(
        !app.world()
            .resource::<Model>()
            .unwrap()
            .collaboration
            .is_open()
    );
}
#[test]
fn earlier_links_are_not_attached_to_unrelated_turn_and_rebuild_without_duplicates() {
    let mut app = app_fixture();
    let snapshot = app
        .world()
        .resource::<Model>()
        .unwrap()
        .chat
        .snapshot()
        .unwrap();
    assert_eq!(anchor(snapshot, &row()), None);
    for _ in 0..3 {
        receive(
            app.world_mut(),
            "parent",
            Ok(Reading {
                rows: vec![row()],
                selected: None,
                history: None,
            }),
        );
        app.update();
    }
    assert_eq!(
        app.world()
            .query::<Target>()
            .iter()
            .filter(|(_, t)| t.0 == key(&row()))
            .count(),
        1
    );
    let id = open(&mut app);
    ready(&mut app, &id);
    receive(app.world_mut(), "parent", Err("Offline".into()));
    assert_eq!(
        app.world()
            .resource::<Model>()
            .unwrap()
            .collaboration
            .peers
            .rows
            .len(),
        1
    );
}
#[test]
#[ignore = "Synthetic linked execution GPU visual acceptance"]
fn linked_exchange_visual_acceptance() {
    let engine: std::sync::Arc<dyn TextEngine> =
        std::sync::Arc::new(rndr_render::ParleyText::new());
    let mut exporter = rndr_render::Exporter::new().unwrap();
    std::fs::create_dir_all("verification").unwrap();
    for (width, height, name, dark) in [
        (1200, 850, "wide", true),
        (760, 650, "narrow", true),
        (760, 650, "light", false),
    ] {
        let mut app = app_fixture();
        app.world_mut().insert_resource(engine.clone());
        app.world_mut()
            .insert_resource(Viewport::new(width as f32, height as f32));
        publish_system_appearance(
            app.world_mut(),
            SystemAppearance {
                dark: Some(dark),
                ..Default::default()
            },
        );
        let id = open(&mut app);
        ready(&mut app, &id);
        stream_event(
            app.world_mut(),
            "parent",
            &id,
            Ok(event(
                1,
                json!({"type":"assistant_text_started","segment_id":"s","model_round":1}),
            )),
        );
        stream_event(
            app.world_mut(),
            "parent",
            &id,
            Ok(event(
                2,
                json!({"type":"content_append","segment_id":"s","text":"I’m checking the native pane sizing and clipping.\n\n- The parent draft stays intact.\n- Opening this execution chat keeps the host conversation in its own tab.\n- Returning a result does not approve or integrate the work."}),
            )),
        );
        for _ in 0..12 {
            rndr_paint::frame(&mut app);
        }
        let (_, image) = rndr_paint::frame_with_snapshot_frame(&mut app, |world, frame| {
            exporter
                .render_frame(Some(world), frame, width, height, 1.)
                .unwrap()
        });
        image::save_buffer(
            format!("verification/native-linked-exchange-{name}.png"),
            &image.rgba,
            image.width,
            image.height,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}
