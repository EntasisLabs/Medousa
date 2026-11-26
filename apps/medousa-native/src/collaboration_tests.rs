//! Synthetic collaboration acceptance; no daemon, provider or credential access.
use super::*;
use serde_json::json;

fn row(id: &str, turn: Option<&str>) -> WorkCardDetail {
    serde_json::from_value(json!({
        "card":{"id":id,"column":"in_flight","title":"Native UI review","status_label":"running","created_at_utc":"2026-11-15T00:00:00Z","updated_at_utc":"2026-11-15T00:01:00Z"},
        "kind":"turn_worker","session_id":"parent","correlation_id":turn,"work_id":id,"terminal":false,
        "user_ack":"Native UI review","live_output":"I am checking clipping and pane sizing.",
        "live_tool_activity":[{"run_id":"read-1","name":"cognition_code_read","round":1,"status":"succeeded","started_at":"2026-11-15T00:00:30Z","output_summary":"Pane state is retained."}],
        "worker_transcript":{"work_id":id,"session_id":"parent","parent_runtime_id":"native","execution_runtime_id":"native","task_prompt":"Inspect native pane clipping. Preserve unrelated work.","result_text":null,"steer_messages":[{"text":"Also check the narrow layout.","at":"2026-11-15T00:00:40Z"}],"retention_note":"Tools are retained rolling evidence, not an exhaustive transcript."}
    })).unwrap()
}
fn fixture() -> App {
    let mut app = app();
    app.world_mut().insert_resource(Viewport::new(1200., 850.));
    app.update();
    let model = app.world_mut().resource_mut::<Model>().unwrap();
    model.chat.hydrate("parent".into(), "a".into(), vec![
        json!({"entry_id":"u","entry_seq":1,"role":"user","content":"Please check the native layout."}),
        json!({"entry_id":"a","entry_seq":2,"role":"assistant","turn_id":"turn-1","content":"I’m working with a native UI reviewer to check **clipping and pane sizing**. You can inspect our exchange while I continue here."})
    ], None).unwrap();
    for _ in 0..8 {
        app.update();
    }
    receive(
        app.world_mut(),
        "parent",
        Ok(vec![row("worker-1", Some("turn-1"))]),
    );
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
fn target(app: &App, id: &str) -> Entity {
    app.world()
        .query::<Target>()
        .iter()
        .find(|(_, t)| t.0 == id)
        .expect("worker marker mounted")
        .0
}
fn text(app: &mut App, text: &str) {
    let document = app.world().resource::<Model>().unwrap().document;
    let doc = app
        .world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(document)
        .unwrap();
    doc.commit(rndr_core::editing::SourceBatch {
        document,
        base: doc.revision(),
        origin: rndr_core::editing::EditOrigin::Host(99),
        changes: vec![rndr_core::editing::SourceChange {
            range: 0..doc.text().len(),
            replacement: text.into(),
        }],
    })
    .unwrap();
    app.update();
}
#[test]
fn exchange_back_retains_parent_draft_document_scroll_and_focus() {
    let mut app = fixture();
    // A genuinely scrollable parent, not merely the zero-offset empty case.
    app.world_mut().resource_mut::<Model>().unwrap().chat.hydrate("parent".into(),"a".into(),vec![json!({"entry_id":"a","entry_seq":2,"role":"assistant","turn_id":"turn-1","content":"Checking pane sizing and retaining the parent conversation.\n\n".repeat(45)})],None).unwrap();
    for _ in 0..12 {
        app.update();
    }
    text(&mut app, "Keep this parent draft");
    let ui = *app.world().resource::<Ui>().unwrap();
    let document = app.world().resource::<Model>().unwrap().document;
    let revision = app
        .world()
        .resource::<SourceDocuments>()
        .unwrap()
        .get(document)
        .unwrap()
        .revision();
    let marker = target(&app, "worker-1");
    let scroll = app.world().get::<Scroll>(ui.timeline).unwrap().offset;
    assert!(
        scroll[1] < -10.,
        "fixture must exercise a nonzero parent viewport: {scroll:?}"
    );
    activate(&mut app, marker);
    assert!(
        app.world()
            .resource::<Model>()
            .unwrap()
            .collaboration
            .is_open()
    );
    assert!(!app.world().has::<Hidden>(ui.exchange.root));
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(ui.exchange.back)
    );
    assert_eq!(
        app.world().resource::<Model>().unwrap().chat.session(),
        Some("parent")
    );
    assert!(!app.world().has::<Hidden>(ui.timeline));

    activate(&mut app, ui.exchange.back);
    assert!(
        !app.world()
            .resource::<Model>()
            .unwrap()
            .collaboration
            .is_open()
    );
    assert!(app.world().has::<Hidden>(ui.exchange.root));
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(marker)
    );
    assert_eq!(app.world().resource::<Model>().unwrap().document, document);
    let doc = app
        .world()
        .resource::<SourceDocuments>()
        .unwrap()
        .get(document)
        .unwrap();
    assert_eq!(doc.text(), "Keep this parent draft");
    assert_eq!(doc.revision(), revision);
    let retained = app.world().get::<Scroll>(ui.timeline).unwrap().offset;
    assert!(
        (retained[1] - scroll[1]).abs() <= 4.,
        "same reading location after split reflow: {retained:?} vs {scroll:?}"
    );
}
#[test]
fn replay_live_result_and_rehydrate_keep_single_worker_and_stable_placement() {
    let mut app = fixture();
    let marker = target(&app, "worker-1");
    activate(&mut app, marker);
    let mut result = row("worker-1", Some("turn-1"));
    result.card.column = WorkBoardColumn::WrappingUp;
    result.worker_transcript.as_mut().unwrap().result_text =
        Some("Full persisted result; no clipped tail.".into());
    for _ in 0..3 {
        receive(app.world_mut(), "parent", Ok(vec![result.clone()]));
        app.update();
    }
    let m = app.world().resource::<Model>().unwrap();
    assert_eq!(m.collaboration.rows.len(), 1);
    assert_eq!(
        m.collaboration
            .projection
            .as_ref()
            .unwrap()
            .items
            .iter()
            .filter(|i| i.id.ends_with(":output"))
            .count(),
        1
    );
    assert_eq!(
        m.collaboration
            .projection
            .as_ref()
            .unwrap()
            .items
            .iter()
            .find(|i| i.id.ends_with(":output"))
            .unwrap()
            .content,
        "Full persisted result; no clipped tail."
    );
    assert_eq!(
        lifecycle(&result),
        "Result ready · Awaiting Medousa summary"
    );
    let ui = *app.world().resource::<Ui>().unwrap();
    activate(&mut app, ui.exchange.back);
    assert_eq!(
        app.world()
            .query::<Target>()
            .iter()
            .filter(|(_, t)| t.0 == "worker-1")
            .count(),
        1
    );
    let snapshot = app
        .world()
        .resource::<Model>()
        .unwrap()
        .chat
        .snapshot()
        .unwrap();
    assert_eq!(anchor(snapshot, &result), Some("parent:a"));
    let mut fresh = fixture();
    receive(fresh.world_mut(), "parent", Ok(vec![result]));
    fresh.update();
    assert_eq!(
        fresh
            .world()
            .query::<Target>()
            .iter()
            .filter(|(_, t)| t.0 == "worker-1")
            .count(),
        1
    );
}
#[test]
fn background_reply_is_routed_to_retained_parent_and_not_another_chat() {
    let mut app = fixture();
    let marker = target(&app, "worker-1");
    activate(&mut app, marker);
    let generation = app.world().resource::<Model>().unwrap().generation;
    let original_ui = *app.world().resource::<Ui>().unwrap();
    conversations::navigate(app.world_mut(), NEW);
    for _ in 0..4 {
        app.update();
    }
    assert_ne!(
        app.world().resource::<Model>().unwrap().generation,
        generation
    );
    let mut result = row("worker-1", Some("turn-1"));
    result.worker_transcript.as_mut().unwrap().result_text = Some("Finished in background".into());
    super::super::apply_reply(
        app.world_mut(),
        Reply {
            generation,
            update: Update::Collaboration {
                session: "parent".into(),
                result: Ok(vec![result]),
            },
        },
    );
    assert!(
        app.world()
            .resource::<Model>()
            .unwrap()
            .collaboration
            .rows
            .is_empty()
    );
    conversations::with_session(app.world_mut(), "parent", |world| {
        let m = world.resource::<Model>().unwrap();
        assert!(m.collaboration.is_open());
        assert_eq!(
            m.collaboration.rows[0]
                .worker_transcript
                .as_ref()
                .unwrap()
                .result_text
                .as_deref(),
            Some("Finished in background")
        );
        assert_eq!(
            world.resource::<Ui>().unwrap().timeline,
            original_ui.timeline
        );
    });
}
#[test]
fn wrong_session_and_mismatched_full_history_cannot_leak_into_exchange() {
    let mut app = fixture();
    let mut wrong = row("wrong", None);
    receive(app.world_mut(), "elsewhere", Ok(vec![wrong.clone()]));
    wrong.worker_transcript.as_mut().unwrap().session_id = "elsewhere".into();
    receive(app.world_mut(), "parent", Ok(vec![wrong]));
    let state = &app.world().resource::<Model>().unwrap().collaboration;
    assert_eq!(state.rows.len(), 1);
    assert_eq!(work_id(&state.rows[0]), "worker-1");
}
#[test]
fn unanchored_history_is_collapsed_and_reconciles_to_correct_turn() {
    let mut app = fixture();
    receive(app.world_mut(), "parent", Ok(vec![row("older", None)]));
    for _ in 0..4 {
        app.update();
    }
    let ui = *app.world().resource::<Ui>().unwrap();
    assert!(!app.world().has::<Hidden>(ui.exchange.earlier));
    assert!(app.world().has::<Hidden>(ui.exchange.earlier_rows));
    assert!(
        !app.world()
            .query::<Target>()
            .iter()
            .any(|(_, t)| t.0 == "older")
    );
    app.world_mut()
        .resource_mut::<Model>()
        .unwrap()
        .collaboration
        .earlier_open = true;
    app.update();
    let earlier = target(&app, "older");
    receive(
        app.world_mut(),
        "parent",
        Ok(vec![row("older", Some("turn-1"))]),
    );
    app.update();
    assert!(app.world().has::<Hidden>(earlier));
    assert!(app.world().has::<Hidden>(ui.exchange.earlier));
}
fn history_action(app: &App, command: u64, work: &str) -> Entity {
    app.world()
        .query::<HistoryTarget>()
        .iter()
        .find(|(e, t)| {
            t.work == work
                && !app.world().has::<Hidden>(*e)
                && app.world().get::<Button>(*e).is_some_and(|b| {
                    rndr_ui::controls::host_intent(
                        app.world(),
                        &ActionIntent {
                            entity: *e,
                            name: b.action.clone(),
                        },
                    ) == Some(HostIntent::Command(command))
                })
        })
        .expect("visible scoped history control")
        .0
}
#[test]
fn finished_history_dismiss_restore_is_local_and_live_work_reappears() {
    let mut app = fixture();
    let mut finished = row("older", None);
    finished.card.column = WorkBoardColumn::Done;
    finished.terminal = true;
    receive(app.world_mut(), "parent", Ok(vec![finished.clone()]));
    app.world_mut()
        .resource_mut::<Model>()
        .unwrap()
        .collaboration
        .earlier_open = true;
    app.update();
    let marker = target(&app, "older");
    let dismiss = history_action(&app, HISTORY_DISMISS, "older");
    activate(&mut app, dismiss);
    assert!(app.world().has::<Hidden>(marker));
    // Both canonical worker rows remain intact; receive merges the older row
    // with the current turn's worker rather than replacing the existing row.
    let rows = &app.world().resource::<Model>().unwrap().collaboration.rows;
    assert_eq!(rows.len(), 2);
    let expected_older = row("older", None).card.id;
    let expected_current = row("worker-1", None).card.id;
    assert!(rows.iter().any(|r| r.card.id == expected_older));
    assert!(rows.iter().any(|r| r.card.id == expected_current));
    let show = history_action(&app, HISTORY_VISIBILITY, "");
    activate(&mut app, show);
    assert!(!app.world().has::<Hidden>(marker));
    let restore = history_action(&app, HISTORY_RESTORE, "older");
    activate(&mut app, restore);
    let dismiss = history_action(&app, HISTORY_DISMISS, "older");
    activate(&mut app, dismiss);
    receive(app.world_mut(), "parent", Ok(vec![row("older", None)]));
    app.update();
    assert!(!app.world().has::<Hidden>(marker));
    assert!(
        app.world()
            .resource::<Model>()
            .unwrap()
            .collaboration
            .history
            .dismissed
            .values()
            .all(|ids| !ids.contains("older"))
    );
}
#[test]
fn history_preferences_are_authority_session_scoped_and_attention_is_never_hidden() {
    let mut app = fixture();
    let snapshot = app
        .world()
        .resource::<Model>()
        .unwrap()
        .chat
        .snapshot()
        .unwrap()
        .clone();
    let mut prefs = HistoryPreferences::default();
    prefs.ids(&snapshot).insert("older".into());
    let mut other = snapshot.clone();
    other.authority_id = "another-authority".into();
    assert!(prefs.ids(&other).is_empty());
    other = snapshot.clone();
    other.session_id = "another-session".into();
    assert!(prefs.ids(&other).is_empty());
    let mut blocked = row("older", None);
    blocked.terminal = true;
    blocked.card.column = WorkBoardColumn::Blocked;
    prefs.reconcile(&snapshot, &[blocked]);
    assert!(!prefs.ids(&snapshot).contains("older"));
    other = snapshot.clone();
    other.authority_id = "wrong".into();
    // Keep the fixture visible so RNDR admits the request; the history
    // authority guard, not hidden-control rejection, must reject its effect.
    let parent = app.world().resource::<Ui>().unwrap().content;
    let stale = history_control(
        app.world_mut(),
        parent,
        HISTORY_DISMISS,
        "older",
        "Dismiss",
        &other,
    );
    let name = app.world().get::<Button>(stale).unwrap().action.clone();
    assert!(action(
        app.world_mut(),
        &ActionIntent {
            entity: stale,
            name
        }
    ));
    assert!(
        app.world()
            .resource::<Model>()
            .unwrap()
            .collaboration
            .history
            .dismissed
            .values()
            .all(HashSet::is_empty)
    );
}

#[test]
fn full_text_beats_preview_and_reading_ids_do_not_depend_on_content() {
    let mut row = row("worker-1", Some("turn-1"));
    let before = project(&row);
    row.result_excerpt = Some("short preview".into());
    row.worker_transcript.as_mut().unwrap().result_text = Some("complete persisted result".into());
    let after = project(&row);
    assert_eq!(before.items[1].id, after.items[1].id);
    assert_ne!(before.revision, after.revision);
    assert!(
        after
            .items
            .iter()
            .any(|i| i.content == "complete persisted result")
    );
    assert!(!after.items.iter().any(|i| i.content == "short preview"));
    assert_eq!(
        after
            .items
            .last()
            .unwrap()
            .tool_runs
            .as_ref()
            .unwrap()
            .len(),
        1
    );
    row.worker_transcript = None;
    assert!(
        project(&row).items[0]
            .content
            .contains("Only the card preview")
    );
}
#[test]
fn tool_disclosure_uses_worker_snapshot_and_closes_on_return() {
    let mut app = fixture();
    let marker = target(&app, "worker-1");
    activate(&mut app, marker);
    let timeline = app.world().resource::<Ui>().unwrap().exchange.timeline;
    app.world_mut().get_mut::<Scroll>(timeline).unwrap().offset[1] = -10000.;
    for _ in 0..8 {
        app.update();
    }
    let trigger = app
        .world()
        .query::<Semantics>()
        .iter()
        .find(|(_, s)| {
            s.name
                .as_deref()
                .is_some_and(|n| n.starts_with("Open tool activity:"))
        })
        .expect("recorded tool evidence is disclosed")
        .0;
    activate(&mut app, trigger);
    let inspector = app
        .world()
        .query::<Semantics>()
        .iter()
        .find(|(_, s)| s.name.as_deref() == Some("Tool activity"))
        .expect("inspector remains open through settled exchange frames")
        .0;
    assert!(app.world().is_alive(inspector));
    // Calls start collapsed in the thread panel; expand one to read its result.
    let call = app
        .world()
        .query::<Semantics>()
        .iter()
        .find(|(_, s)| {
            s.name
                .as_deref()
                .is_some_and(|n| n.starts_with("Expand Code read"))
        })
        .expect("the call is listed in the thread panel")
        .0;
    activate(&mut app, call);
    assert!(
        app.world()
            .query::<MarkdownView>()
            .iter()
            .any(|(_, v)| v.source().contains("Pane state is retained"))
    );
    let ui = *app.world().resource::<Ui>().unwrap();
    // Dismiss the panel before the relationship header.
    let close = app
        .world()
        .query::<Semantics>()
        .iter()
        .find(|(_, s)| s.name.as_deref() == Some("Close tool activity"))
        .expect("inspector close")
        .0;
    activate(&mut app, close);
    assert!(!app.world().is_alive(inspector));
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
fn exchange_refresh_is_single_flight_and_failures_preserve_recorded_text() {
    let mut app = fixture();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    app.world_mut().resource_mut::<Model>().unwrap().commands = Some(tx);
    app.world_mut()
        .resource_mut::<Model>()
        .unwrap()
        .collaboration
        .refreshed = None;
    app.update();
    let commands = [
        rx.try_recv().unwrap().command,
        rx.try_recv().unwrap().command,
    ];
    assert!(
        commands
            .iter()
            .any(|c| matches!(c, Command::Collaboration { .. }))
    );
    assert!(
        commands
            .iter()
            .any(|c| matches!(c, Command::PeerCollaboration { .. }))
    );
    for _ in 0..8 {
        app.update();
    }
    assert!(rx.try_recv().is_err());
    receive(
        app.world_mut(),
        "parent",
        Err("Offline · Refresh to retry".into()),
    );
    assert_eq!(
        app.world()
            .resource::<Model>()
            .unwrap()
            .collaboration
            .rows
            .len(),
        1
    );
    assert!(
        !app.world()
            .resource::<Model>()
            .unwrap()
            .collaboration
            .inflight
    );
}

#[test]
#[ignore = "Synthetic Parley/GPU collaboration visual acceptance"]
fn collaboration_visual_acceptance() {
    let engine: std::sync::Arc<dyn TextEngine> =
        std::sync::Arc::new(rndr_render::ParleyText::new());
    let mut exporter = rndr_render::Exporter::new().unwrap();
    std::fs::create_dir_all("verification").unwrap();
    for (width, height, name, open, dark) in [
        (1200, 850, "main", false, true),
        (1200, 850, "exchange", true, true),
        (760, 650, "exchange-narrow", true, true),
        (760, 650, "exchange-light", true, false),
    ] {
        let mut app = fixture();
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
        for _ in 0..8 {
            rndr_paint::frame(&mut app);
        }
        if open {
            let marker = target(&app, "worker-1");
            activate(&mut app, marker);
        }
        for _ in 0..12 {
            rndr_paint::frame(&mut app);
        }
        let ui = *app.world().resource::<Ui>().unwrap();
        assert_eq!(!app.world().has::<Hidden>(ui.exchange.root), open);
        let (_, image) = rndr_paint::frame_with_snapshot_frame(&mut app, |world, frame| {
            exporter
                .render_frame(Some(world), frame, width, height, 1.)
                .unwrap()
        });
        image::save_buffer(
            format!("verification/native-collaboration-{name}.png"),
            &image.rgba,
            image.width,
            image.height,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}

fn reply_text(app: &mut App, value: &str) -> DocumentId {
    let document = app
        .world()
        .resource::<Model>()
        .unwrap()
        .collaboration
        .reply_document
        .unwrap();
    let mut doc = app
        .world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(document)
        .unwrap();
    doc.commit(rndr_core::editing::SourceBatch {
        document,
        base: doc.revision(),
        origin: rndr_core::editing::EditOrigin::Host(99),
        changes: vec![rndr_core::editing::SourceChange {
            range: 0..doc.text().len(),
            replacement: value.into(),
        }],
    })
    .unwrap();
    app.update();
    document
}

#[test]
fn work_thread_retains_two_lanes_wide_and_uses_one_lane_narrow() {
    let mut app = fixture();
    app.world_mut().insert_resource(Viewport::new(1600., 850.));
    for _ in 0..8 {
        app.update();
    }
    let marker = target(&app, "worker-1");
    activate(&mut app, marker);
    let ui = *app.world().resource::<Ui>().unwrap();
    assert!(!app.world().has::<Hidden>(ui.exchange.main));
    let main = app.world().get::<Rect>(ui.exchange.main).unwrap();
    let thread = app.world().get::<Rect>(ui.exchange.root).unwrap();
    assert!(thread.x >= main.x + main.w - 1., "{main:?} {thread:?}");
    assert!(main.w > 600. && thread.w >= 340.);
    app.world_mut().insert_resource(Viewport::new(760., 850.));
    for _ in 0..8 {
        app.update();
    }
    assert!(app.world().has::<Hidden>(ui.exchange.main));
    super::super::action(app.world_mut(), SEND);
    assert!(!app.world().resource::<Model>().unwrap().chat.busy());
    activate(&mut app, ui.exchange.back);
    assert!(!app.world().has::<Hidden>(ui.exchange.main));
}

#[test]
fn task_reply_is_scoped_and_retains_each_draft_without_touching_parent() {
    let mut app = fixture();
    receive(
        app.world_mut(),
        "parent",
        Ok(vec![
            row("worker-1", Some("turn-1")),
            row("worker-2", Some("turn-1")),
        ]),
    );
    for _ in 0..8 {
        app.update();
    }
    text(&mut app, "parent draft");
    let main = app.world().resource::<Model>().unwrap().document;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    app.world_mut().resource_mut::<Model>().unwrap().commands = Some(tx);
    let marker = target(&app, "worker-1");
    activate(&mut app, marker);
    let first = reply_text(&mut app, "reply for first worker");
    let ui = *app.world().resource::<Ui>().unwrap();
    activate(&mut app, ui.exchange.back);
    let marker = target(&app, "worker-2");
    activate(&mut app, marker);
    let second = reply_text(&mut app, "second worker draft");
    assert_ne!(first, second);
    activate(&mut app, ui.exchange.back);
    let marker = target(&app, "worker-1");
    activate(&mut app, marker);
    assert_eq!(
        app.world()
            .resource::<Model>()
            .unwrap()
            .collaboration
            .reply_document,
        Some(first)
    );
    let send = app.world().resource::<Ui>().unwrap().exchange.reply_send;
    activate(&mut app, send);
    let mut commands = vec![];
    while let Ok(request) = rx.try_recv() {
        commands.push(request.command);
    }
    let nonce = commands
        .into_iter()
        .find_map(|c| match c {
            Command::ThreadReply {
                session,
                work_id,
                message,
                nonce,
            } => {
                assert_eq!(
                    (session.as_str(), work_id.as_str(), message.as_str()),
                    ("parent", "worker-1", "reply for first worker")
                );
                Some(nonce)
            }
            _ => None,
        })
        .expect("one task-scoped reply");
    reply_received(app.world_mut(), "parent", "worker-1", nonce, Ok(()));
    assert_eq!(
        app.world()
            .resource::<SourceDocuments>()
            .unwrap()
            .get(first)
            .unwrap()
            .text(),
        ""
    );
    assert_eq!(
        app.world()
            .resource::<SourceDocuments>()
            .unwrap()
            .get(second)
            .unwrap()
            .text(),
        "second worker draft"
    );
    assert_eq!(
        app.world()
            .resource::<SourceDocuments>()
            .unwrap()
            .get(main)
            .unwrap()
            .text(),
        "parent draft"
    );
}

#[test]
fn late_reply_ack_preserves_new_edits_and_unknown_delivery_blocks_replay() {
    let mut app = fixture();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    app.world_mut().resource_mut::<Model>().unwrap().commands = Some(tx);
    let marker = target(&app, "worker-1");
    activate(&mut app, marker);
    let document = reply_text(&mut app, "original reply");
    send_reply(app.world_mut());
    let nonce = app.world().resource::<Model>().unwrap().collaboration.nonce;
    reply_text(&mut app, "new edits while sending");
    reply_received(app.world_mut(), "parent", "worker-1", nonce, Ok(()));
    assert_eq!(
        app.world()
            .resource::<SourceDocuments>()
            .unwrap()
            .get(document)
            .unwrap()
            .text(),
        "new edits while sending"
    );
    send_reply(app.world_mut());
    let nonce = app.world().resource::<Model>().unwrap().collaboration.nonce;
    reply_received(
        app.world_mut(),
        "parent",
        "worker-1",
        nonce,
        Err("Delivery unresolved. Inspect thread before retrying.".into()),
    );
    send_reply(app.world_mut());
    let mut count = 0;
    while let Ok(request) = rx.try_recv() {
        count += usize::from(matches!(request.command, Command::ThreadReply { .. }));
    }
    assert_eq!(count, 2);
    assert!(
        app.world()
            .resource::<Model>()
            .unwrap()
            .collaboration
            .reply_pending
            .is_some()
    );
}

#[test]
fn thread_reply_wraps_incremental_appends_and_scrolls_caret_after_resize() {
    let mut app = fixture();
    let marker = target(&app, "worker-1");
    activate(&mut app, marker);
    for width in [1600., 760.] {
        app.world_mut().insert_resource(Viewport::new(width, 850.));
        for _ in 0..8 {
            app.update();
        }
        let editor = app
            .world()
            .resource::<Ui>()
            .unwrap()
            .exchange
            .reply_editor
            .editor;
        request_focus(app.world_mut(), editor);
        for _ in 0..4 {
            app.world_mut().resource_mut::<Input>().unwrap().chars =
                "A longer worker reply should wrap without losing the caret outside the view. "
                    .into();
            app.update();
            app.world_mut()
                .resource_mut::<Input>()
                .unwrap()
                .chars
                .clear();
            for _ in 0..4 {
                app.update();
            }
            let w = app.world();
            let rect = w.get::<Rect>(editor).unwrap();
            let caret = w.get::<Caret>(editor).unwrap();
            let label = w.get::<Label>(editor).unwrap();
            let layout = w.get::<TextLayout>(editor).unwrap();
            assert_eq!(caret.index, label.text.len());
            assert!(layout.lines.iter().all(|line| line.width <= rect.w + 0.5));
            let (_, y) = w.get::<CaretRow>(editor).map_or_else(
                || layout.caret_xy(caret.index),
                |hint| layout.caret_xy_on_line(caret.index, hint.row),
            );
            let line = layout.lines.iter().find(|line| line.y == y).unwrap();
            assert!(
                y >= 0. && y + line.h + 2. * TEXT_ORIGIN_Y <= rect.h + 0.5,
                "reply caret clipped at {y} in {rect:?}"
            );
        }
    }
}
