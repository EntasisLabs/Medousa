use super::*;
fn app() -> App {
    let mut a = crate::ui::app();
    a.world_mut().insert_resource(Viewport::new(1200., 800.));
    a.update();
    a
}
fn row(s: &str, id: &str, label: &str) -> ArtifactSummary {
    ArtifactSummary {
        artifact_id: id.into(),
        session_id: s.into(),
        label: label.into(),
        presentation: None,
        byte_size: 4200,
        stored_at_utc: chrono::DateTime::parse_from_rfc3339("2026-11-26T20:00:00Z")
            .unwrap()
            .into(),
        root_artifact_id: None,
        supersedes_artifact_id: None,
    }
}
fn load(w: &mut World, rows: Vec<ArtifactSummary>) {
    activate(w);
    let command = take_commands(w).pop().unwrap();
    apply(
        w,
        Update {
            command,
            result: Ok(Response::Listed(rows)),
        },
    );
    sync(w);
}
fn trigger(w: &mut World, predicate: impl Fn(&Action) -> bool) {
    let id = w
        .resource::<State>()
        .unwrap()
        .actions
        .iter()
        .find(|(_, a)| predicate(a))
        .map(|(id, _)| *id)
        .unwrap();
    assert!(action(w, &HostIntent::Command(id)));
}
#[test]
fn artifacts_are_lazy_retained_and_registered_in_navigation() {
    let mut a = app();
    assert!(take_commands(a.world_mut()).is_empty());
    crate::surfaces::select(a.world_mut(), SurfaceId::Artifacts);
    assert_eq!(take_commands(a.world_mut()).len(), 1);
    crate::surfaces::select(a.world_mut(), SurfaceId::Chat);
    crate::surfaces::select(a.world_mut(), SurfaceId::Artifacts);
    assert!(take_commands(a.world_mut()).is_empty());
}
#[test]
fn stale_list_cannot_replace_latest_query_or_remounted_library() {
    let mut a = app();
    activate(a.world_mut());
    let old = take_commands(a.world_mut()).pop().unwrap();
    request(a.world_mut().resource_mut::<State>().unwrap());
    let new = take_commands(a.world_mut()).pop().unwrap();
    apply(
        a.world_mut(),
        Update {
            command: old.clone(),
            result: Ok(Response::Listed(vec![row("s", "old", "Old")])),
        },
    );
    assert!(a.world().resource::<State>().unwrap().rows.is_empty());
    apply(
        a.world_mut(),
        Update {
            command: new,
            result: Ok(Response::Listed(vec![row("s", "new", "New")])),
        },
    );
    assert_eq!(
        a.world().resource::<State>().unwrap().rows[0].artifact_id,
        "new"
    );
    let root = crate::surfaces::root(a.world(), SurfaceId::Artifacts).unwrap();
    for c in children_of(a.world(), root) {
        despawn(a.world_mut(), c);
    }
    mount(a.world_mut(), root);
    activate(a.world_mut());
    apply(
        a.world_mut(),
        Update {
            command: old,
            result: Err("stale failure".into()),
        },
    );
    assert!(a.world().resource::<State>().unwrap().error.is_none());
}
#[test]
fn search_retains_input_focus_and_scopes_server_query_across_chats() {
    let mut a = app();
    load(
        a.world_mut(),
        vec![
            row("one", "a", "Design notes"),
            row("two", "b", "Demo dashboard"),
        ],
    );
    let input = a.world().resource::<State>().unwrap().search;
    a.world_mut().resource_mut::<Focus>().unwrap().entity = Some(input);
    replace_field_text(a.world_mut(), input, "dashboard");
    sync(a.world_mut());
    assert_eq!(a.world().resource::<Focus>().unwrap().entity, Some(input));
    let s = a.world().resource::<State>().unwrap();
    assert_eq!(
        s.actions
            .values()
            .filter(|v| matches!(v, Action::Select(..)))
            .count(),
        1
    );
    trigger(a.world_mut(), |v| matches!(v, Action::Search));
    assert!(matches!(&take_commands(a.world_mut())[0].kind,Kind::List(q) if q=="dashboard"));
    trigger(a.world_mut(), |v| matches!(v, Action::Clear));
    sync(a.world_mut());
    assert_eq!(a.world().get::<Label>(input).unwrap().text, "");
}
#[test]
fn selection_uses_session_and_artifact_identity_and_refresh_preserves_it() {
    let mut a = app();
    load(
        a.world_mut(),
        vec![row("one", "same", "One"), row("two", "same", "Two")],
    );
    trigger(
        a.world_mut(),
        |v| matches!(v,Action::Select(s,_) if s=="two"),
    );
    sync(a.world_mut());
    assert_eq!(
        selected(a.world().resource::<State>().unwrap())
            .unwrap()
            .label,
        "Two"
    );
    trigger(a.world_mut(), |v| matches!(v, Action::Refresh));
    let command = take_commands(a.world_mut()).pop().unwrap();
    apply(
        a.world_mut(),
        Update {
            command,
            result: Ok(Response::Listed(vec![row("two", "same", "Updated")])),
        },
    );
    assert_eq!(
        selected(a.world().resource::<State>().unwrap())
            .unwrap()
            .label,
        "Updated"
    );
}
#[test]
fn errors_preserve_loaded_rows_and_retry_is_explicit() {
    let mut a = app();
    load(a.world_mut(), vec![row("s", "a", "Saved")]);
    trigger(a.world_mut(), |v| matches!(v, Action::Refresh));
    let command = take_commands(a.world_mut()).pop().unwrap();
    apply(
        a.world_mut(),
        Update {
            command,
            result: Err("Offline".into()),
        },
    );
    sync(a.world_mut());
    let s = a.world().resource::<State>().unwrap();
    assert_eq!(s.rows.len(), 1);
    assert!(s.error.is_some());
    assert!(take_commands(a.world_mut()).is_empty());
}
#[test]
fn export_is_single_flight_blocks_rebind_and_reports_cancel_or_failure() {
    let mut a = app();
    load(a.world_mut(), vec![row("s", "a", "Saved")]);
    trigger(a.world_mut(), |v| matches!(v, Action::Select(..)));
    sync(a.world_mut());
    trigger(a.world_mut(), |v| matches!(v, Action::Export));
    trigger(a.world_mut(), |v| matches!(v, Action::Export));
    let mut commands = take_commands(a.world_mut());
    assert_eq!(commands.len(), 1);
    assert!(!can_rebind(a.world()));
    apply(
        a.world_mut(),
        Update {
            command: commands.pop().unwrap(),
            result: Ok(Response::Exported(false)),
        },
    );
    assert!(can_rebind(a.world()));
    assert!(
        a.world()
            .resource::<State>()
            .unwrap()
            .notice
            .contains("cancelled")
    );
    trigger(a.world_mut(), |v| matches!(v, Action::Export));
    let command = take_commands(a.world_mut()).pop().unwrap();
    apply(
        a.world_mut(),
        Update {
            command,
            result: Err("Disk full".into()),
        },
    );
    assert!(
        a.world()
            .resource::<State>()
            .unwrap()
            .notice
            .contains("Disk full")
    );
}
#[test]
fn preview_remains_open_in_library_and_retires_on_surface_exit() {
    let mut a = app();
    crate::surfaces::select(a.world_mut(), SurfaceId::Artifacts);
    let command = take_commands(a.world_mut()).pop().unwrap();
    apply(
        a.world_mut(),
        Update {
            command,
            result: Ok(Response::Listed(vec![row("s", "a", "Saved")])),
        },
    );
    sync(a.world_mut());
    trigger(a.world_mut(), |v| matches!(v, Action::Select(..)));
    sync(a.world_mut());
    crate::artifact_preview::tick(a.world_mut());
    assert!(crate::artifact_preview::inline_snapshot(a.world()).is_some());
    assert!(a.world().entities_with::<Modal>().is_empty());
    crate::surfaces::select(a.world_mut(), SurfaceId::Chat);
    crate::artifact_preview::tick(a.world_mut());
    assert!(a.world().entities_with::<Modal>().is_empty());
    assert!(crate::artifact_preview::inline_snapshot(a.world()).is_none());
}
#[test]
fn safe_export_filename_never_contains_path_components() {
    assert_eq!(filename(""), "artifact.html");
    let name = filename("../../a\\b:<script>\n");
    assert!(!name.contains('/'));
    assert!(!name.contains('\\'));
    assert!(!name.contains('\n'));
    assert!(name.ends_with(".html"));
}
#[test]
fn source_chat_opens_selected_conversation_not_current_chat() {
    let mut a = app();
    crate::surfaces::select(a.world_mut(), SurfaceId::Artifacts);
    let command = take_commands(a.world_mut()).pop().unwrap();
    apply(
        a.world_mut(),
        Update {
            command,
            result: Ok(Response::Listed(vec![row("source-chat", "a", "Saved")])),
        },
    );
    sync(a.world_mut());
    trigger(a.world_mut(), |v| matches!(v, Action::Select(..)));
    sync(a.world_mut());
    trigger(a.world_mut(), |v| matches!(v, Action::Source));
    assert_eq!(crate::surfaces::active(a.world()), SurfaceId::Chat);
}
#[test]
#[ignore = "manual GPU visual evidence"]
fn export_library_visuals() {
    let mut a = app();
    crate::surfaces::select(a.world_mut(), SurfaceId::Artifacts);
    let command = take_commands(a.world_mut()).pop().unwrap();
    apply(
        a.world_mut(),
        Update {
            command,
            result: Ok(Response::Listed(vec![
                row("ses_one", "dashboard", "Workshop pulse"),
                row("ses_two", "diagram", "Native renderer architecture"),
                row(
                    "ses_three",
                    "long",
                    "A longer artifact title with lots of useful context and more words",
                ),
            ])),
        },
    );
    sync(a.world_mut());
    trigger(
        a.world_mut(),
        |v| matches!(v,Action::Select(_,id) if id=="dashboard"),
    );
    let engine: std::sync::Arc<dyn TextEngine> =
        std::sync::Arc::new(rndr_render::ParleyText::new());
    a.world_mut().insert_resource(engine);
    let mut exporter = rndr_render::Exporter::new().unwrap();
    std::fs::create_dir_all("verification").unwrap();
    for (name, width, dark, collapsed) in [
        ("wide-dark", 1200, true, false),
        ("narrow-dark", 760, true, false),
        ("wide-light", 1200, false, false),
        ("focused-dark", 1200, true, true),
        ("focused-narrow", 640, true, true),
    ] {
        if crate::sidebars::open(a.world(), SurfaceId::Artifacts) == collapsed {
            crate::sidebars::toggle(a.world_mut());
        }
        a.world_mut()
            .insert_resource(Viewport::new(width as f32, 800.));
        publish_system_appearance(
            a.world_mut(),
            SystemAppearance {
                dark: Some(dark),
                ..Default::default()
            },
        );
        for _ in 0..12 {
            a.update();
        }
        let (_, frame) = rndr_paint::frame_with_snapshot_frame(&mut a, |world, frame| {
            exporter
                .render_frame(Some(world), frame, width, 800, 1.)
                .unwrap()
        });
        image::save_buffer(
            format!("verification/artifacts-{name}.png"),
            &frame.rgba,
            frame.width,
            frame.height,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
    for (name, error) in [
        ("empty", None),
        (
            "error",
            Some("Could not reach the workshop. Check its connection and try again.".to_owned()),
        ),
    ] {
        let state = a.world_mut().resource_mut::<State>().unwrap();
        state.rows.clear();
        state.selected = None;
        state.error = error;
        state.loading = None;
        state.dirty = true;
        a.world_mut().insert_resource(Viewport::new(1000., 800.));
        for _ in 0..8 {
            a.update();
        }
        let (_, frame) = rndr_paint::frame_with_snapshot_frame(&mut a, |world, frame| {
            exporter
                .render_frame(Some(world), frame, 1000, 800, 1.)
                .unwrap()
        });
        image::save_buffer(
            format!("verification/artifacts-{name}.png"),
            &frame.rgba,
            frame.width,
            frame.height,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}
#[test]
fn keyboard_selection_keeps_focus_on_equivalent_artifact_row() {
    let mut a = app();
    crate::surfaces::select(a.world_mut(), SurfaceId::Artifacts);
    load(a.world_mut(), vec![row("s", "a", "Saved")]);
    let root = a
        .world()
        .query::<ActionTarget>()
        .iter()
        .find(|(_, a)| matches!(&a.0, Action::Select(..)))
        .map(|(e, _)| *e)
        .unwrap();
    assert!(request_focus(a.world_mut(), root));
    trigger(a.world_mut(), |v| matches!(v, Action::Select(..)));
    sync(a.world_mut());
    let focused = a.world().resource::<Focus>().unwrap().entity.unwrap();
    assert!(a.world().is_alive(focused));
    assert!(
        matches!(&a.world().get::<ActionTarget>(focused).unwrap().0,Action::Select(s,id) if s=="s"&&id=="a")
    );
}
#[test]
fn search_command_uses_current_input_not_previous_render_frame() {
    let mut a = app();
    load(a.world_mut(), vec![]);
    let input = a.world().resource::<State>().unwrap().search;
    replace_field_text(a.world_mut(), input, "new query");
    trigger(a.world_mut(), |v| matches!(v, Action::Search));
    assert!(matches!(&take_commands(a.world_mut())[0].kind,Kind::List(q) if q=="new query"));
}
#[test]
fn ime_preedit_prevents_rebinding_and_clear() {
    let mut a = app();
    let input = a.world().resource::<State>().unwrap().search;
    a.world_mut().insert(
        input,
        ImePreview {
            text: "あ".into(),
            cursor: None,
        },
    );
    assert!(!can_rebind(a.world()));
    trigger(a.world_mut(), |v| matches!(v, Action::Clear));
    assert!(a.world().has::<ImePreview>(input));
}
#[test]
fn preview_survives_resize_and_does_not_lose_its_anchor() {
    let mut a = app();
    crate::surfaces::select(a.world_mut(), SurfaceId::Artifacts);
    let command = take_commands(a.world_mut()).pop().unwrap();
    apply(
        a.world_mut(),
        Update {
            command,
            result: Ok(Response::Listed(vec![row("s", "a", "Saved")])),
        },
    );
    sync(a.world_mut());
    trigger(a.world_mut(), |v| matches!(v, Action::Select(..)));
    sync(a.world_mut());
    a.world_mut().insert_resource(Viewport::new(760., 800.));
    for _ in 0..6 {
        a.update();
    }
    assert!(crate::artifact_preview::inline_snapshot(a.world()).is_some());
    assert!(a.world().entities_with::<Modal>().is_empty());
}
fn http_fixture(response: String) -> (MedousaClient, std::thread::JoinHandle<String>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut bytes = Vec::new();
        loop {
            let mut buf = [0; 1024];
            let n = stream.read(&mut buf).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&buf[..n]);
            if let Some(end) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                let size = String::from_utf8_lossy(&bytes[..end])
                    .lines()
                    .find_map(|l| {
                        l.to_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|v| v.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                if bytes.len() >= end + 4 + size {
                    break;
                }
            }
        }
        let _ = stream.write_all(response.as_bytes());
        String::from_utf8(bytes).unwrap()
    });
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::AUTHORIZATION,
        "Bearer fixture-artifacts".parse().unwrap(),
    );
    let http = reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .unwrap();
    (
        MedousaClient::with_transport(
            std::sync::Arc::new(medousa_sdk::HttpTransport::with_client(http)),
            origin,
        ),
        task,
    )
}
fn json_response(v: serde_json::Value) -> String {
    let body = v.to_string();
    format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}
#[tokio::test]
async fn library_requests_authenticated_bounded_workshop_wide_metadata() {
    let (client, server) = http_fixture(json_response(
        serde_json::json!({"artifacts":[row("other-chat","id","Diagram")]}),
    ));
    let update = execute(
        &client,
        Command {
            serial: 1,
            kind: Kind::List("Diagram".into()),
        },
    )
    .await;
    assert!(
        matches!(update.result,Ok(Response::Listed(rows)) if rows.len()==1 && rows[0].session_id=="other-chat")
    );
    let request = server.join().unwrap();
    assert!(request.starts_with("POST /v1/runtime/artifact/list-ui HTTP/1.1"));
    assert!(
        request
            .to_lowercase()
            .contains("authorization: bearer fixture-artifacts")
    );
    let body: serde_json::Value =
        serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
    assert_eq!(body["limit"], 500);
    assert_eq!(body["query"], "Diagram");
    assert!(body.get("session_id").is_none());
}
#[tokio::test]
async fn library_rejects_oversized_and_invalid_transport_responses() {
    for response in [
        "HTTP/1.1 200 OK\r\nContent-Length: 99999999\r\nConnection: close\r\n\r\n".into(),
        json_response(serde_json::json!({"artifacts":"wrong type"})),
    ] {
        let (client, server) = http_fixture(response);
        assert!(
            execute(
                &client,
                Command {
                    serial: 1,
                    kind: Kind::List("".into())
                }
            )
            .await
            .result
            .is_err()
        );
        server.join().unwrap();
    }
}
#[tokio::test]
async fn export_fetch_uses_original_chat_and_validates_exact_html_identity() {
    for id in ["correct", "different"] {
        let html = "<!doctype html><p>Saved café 🌱</p>";
        let (client, server) = http_fixture(json_response(
            serde_json::json!({"artifact_id":id,"label":"Demo","body":html,"byte_size":html.len(),"mime":"text/html"}),
        ));
        let sid = crate::routing::qualify("authority-b", "original-chat");
        let result = crate::artifact_preview::fetch_html(&client, &sid, "correct").await;
        if id == "correct" {
            assert_eq!(result.unwrap(), html);
        } else {
            assert!(result.is_err());
        }
        let request = server.join().unwrap();
        assert!(request.starts_with("POST /v1/runtime/artifact/fetch "));
        let body: serde_json::Value =
            serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(body["session_id"], "original-chat");
        assert_eq!(body["artifact_id"], "correct");
    }
}
#[tokio::test]
async fn device_html_save_preserves_exact_bytes_and_reports_unwritable_destination() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("artifact.html");
    let html = "<!doctype html><p>café 🌱</p>\n";
    write_device_html(path.clone(), html.into()).await.unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), html.as_bytes());
    assert!(
        write_device_html(dir.path().join("missing/artifact.html"), html.into())
            .await
            .is_err()
    );
}
#[test]
fn source_chat_keeps_library_workshop_when_other_chat_has_same_local_id() {
    let mut a = app();
    let dir = tempfile::tempdir().unwrap();
    let mut channels = crate::workshops::install_fixture(
        a.world_mut(),
        dir.path().join("sources.json"),
        &[("owner-a", "Studio"), ("owner-b", "Laptop")],
    );
    a.update();
    crate::surfaces::select(a.world_mut(), SurfaceId::Settings);
    crate::settings::show_connection_fixture(a.world_mut());
    a.update();
    assert!(crate::workshops::action(
        a.world_mut(),
        &HostIntent::Command(0x5753_4008)
    ));
    assert!(crate::workshops::action(
        a.world_mut(),
        &HostIntent::Command(0x5753_3001)
    ));
    crate::ui::open_conversation(a.world_mut(), crate::routing::qualify("owner-a", "same"));
    crate::surfaces::select(a.world_mut(), SurfaceId::Artifacts);
    let command = take_commands(a.world_mut()).pop().unwrap();
    apply(
        a.world_mut(),
        Update {
            command,
            result: Ok(Response::Listed(vec![row("same", "a", "Laptop artifact")])),
        },
    );
    sync(a.world_mut());
    trigger(a.world_mut(), |v| matches!(v, Action::Select(..)));
    sync(a.world_mut());
    assert_eq!(
        session(a.world(), "same"),
        crate::routing::qualify("owner-b", "same")
    );
    trigger(a.world_mut(), |v| matches!(v, Action::Source));
    assert_eq!(
        a.world().resource::<crate::ui::Model>().unwrap().owner,
        "owner-b"
    );
    let mut requested = vec![];
    while let Ok(request) = channels[1].try_recv() {
        if let crate::backend::Command::Open(s) = request.command {
            requested.push(s);
        }
    }
    assert_eq!(requested, vec![crate::routing::qualify("owner-b", "same")]);
}
#[test]
fn hidden_chat_history_update_does_not_close_library_preview() {
    let mut a = app();
    crate::surfaces::select(a.world_mut(), SurfaceId::Artifacts);
    let command = take_commands(a.world_mut()).pop().unwrap();
    apply(
        a.world_mut(),
        Update {
            command,
            result: Ok(Response::Listed(vec![row("artifact-chat", "a", "Saved")])),
        },
    );
    sync(a.world_mut());
    trigger(a.world_mut(), |v| matches!(v, Action::Select(..)));
    sync(a.world_mut());
    a.world_mut()
        .resource_mut::<crate::ui::Model>()
        .unwrap()
        .chat
        .hydrate("background-chat".into(), "owner".into(), vec![], None)
        .unwrap();
    crate::artifact_preview::tick(a.world_mut());
    assert!(crate::artifact_preview::inline_snapshot(a.world()).is_some());
    assert!(a.world().entities_with::<Modal>().is_empty());
    let dir = tempfile::tempdir().unwrap();
    let _channels =
        crate::workshops::install_fixture(a.world_mut(), dir.path().join("sources.json"), &[]);
    crate::surfaces::select(a.world_mut(), SurfaceId::Chat);
    crate::auxiliary::dispatch(a.world_mut());
    assert!(a.world().entities_with::<Modal>().is_empty());
}

#[test]
fn automatic_inline_preview_retains_document_across_search_resize_refresh_and_export_notice() {
    let mut a = app();
    crate::surfaces::select(a.world_mut(), SurfaceId::Artifacts);
    load(
        a.world_mut(),
        vec![row("s", "a", "Saved"), row("s", "b", "Other")],
    );
    trigger(
        a.world_mut(),
        |v| matches!(v, Action::Select(_, id) if id == "a"),
    );
    sync(a.world_mut());
    let first = crate::artifact_preview::inline_snapshot(a.world()).unwrap();
    assert!(a.world().entities_with::<Modal>().is_empty());
    crate::artifact_preview::drain_commands(a.world_mut());
    let search = a.world().resource::<State>().unwrap().search;
    replace_field_text(a.world_mut(), search, "Other");
    sync(a.world_mut());
    a.world_mut().insert_resource(Viewport::new(760., 800.));
    for _ in 0..8 {
        a.update();
    }
    request(a.world_mut().resource_mut::<State>().unwrap());
    let command = take_commands(a.world_mut()).pop().unwrap();
    apply(
        a.world_mut(),
        Update {
            command,
            result: Ok(Response::Listed(vec![
                row("s", "a", "Saved"),
                row("s", "b", "Other"),
            ])),
        },
    );
    sync(a.world_mut());
    let state = a.world_mut().resource_mut::<State>().unwrap();
    state.notice = "HTML saved to this device.".into();
    state.dirty = true;
    sync(a.world_mut());
    assert_eq!(
        crate::artifact_preview::inline_snapshot(a.world()),
        Some(first)
    );
    assert!(crate::artifact_preview::drain_commands(a.world_mut()).is_empty());
    for _ in 0..4 {
        a.update();
    }
    let viewport = a.world().resource::<State>().unwrap().viewport.unwrap();
    let rect = a.world().get::<Rect>(viewport).unwrap();
    assert!(rect.x > 300. && rect.y < 160. && rect.h > 600., "{rect:?}");
    assert!(
        rect.x + rect.w <= 760. && rect.y + rect.h <= 800.,
        "{rect:?}"
    );
    assert!(
        !a.world()
            .has::<Scroll>(a.world().resource::<State>().unwrap().detail)
    );
    trigger(a.world_mut(), |v| matches!(v, Action::Preview));
    sync(a.world_mut());
    let reloaded = crate::artifact_preview::inline_snapshot(a.world()).unwrap();
    assert_ne!(reloaded, first);
    crate::surfaces::select(a.world_mut(), SurfaceId::Chat);
    crate::artifact_preview::tick(a.world_mut());
    assert!(crate::artifact_preview::inline_snapshot(a.world()).is_none());
    crate::surfaces::select(a.world_mut(), SurfaceId::Artifacts);
    sync(a.world_mut());
    assert!(crate::artifact_preview::inline_snapshot(a.world()).is_some());
}

#[test]
fn compact_toolbar_reserves_document_space_and_keeps_actions_reachable() {
    let mut a = app();
    crate::surfaces::select(a.world_mut(), SurfaceId::Artifacts);
    load(
        a.world_mut(),
        vec![row(
            "s",
            "a",
            "A deliberately long artifact title that must truncate instead of crowding the toolbar actions",
        )],
    );
    trigger(a.world_mut(), |v| matches!(v, Action::Select(..)));
    for width in [1200., 760., 640.] {
        a.world_mut().insert_resource(Viewport::new(width, 800.));
        for _ in 0..12 {
            a.update();
        }
        let w = a.world();
        let s = w.resource::<State>().unwrap();
        let rect = w.get::<Rect>(s.viewport.unwrap()).unwrap();
        assert!(rect.y <= 56. && rect.h >= 740., "{width}: {rect:?}");
        let host = crate::ui::chrome::artifact_toolbar_host(w).unwrap();
        assert_eq!(children_of(w, host).len(), 1);
        let toolbar = children_of(w, host)[0];
        assert!(w.has::<WindowDrag>(toolbar));
        let mut last_right = 0.;
        for action in [Action::Preview, Action::Source, Action::Export] {
            let (entity, _) = w
                .query::<ActionTarget>()
                .iter()
                .find(|(_, target)| same_action(&target.0, &action))
                .map(|(e, t)| (*e, t))
                .unwrap();
            let r = w.get::<Rect>(entity).unwrap();
            assert!(r.w >= 32. && r.h >= 32., "{r:?}");
            assert!(
                r.x >= last_right && r.x + r.w <= width && r.y + r.h <= 48.,
                "{width}: {r:?}"
            );
            last_right = r.x + r.w;
        }
    }
}

#[test]
fn document_toolbar_is_retained_without_duplicate_rows_and_follows_surface_visibility() {
    let mut a = app();
    crate::surfaces::select(a.world_mut(), SurfaceId::Artifacts);
    load(a.world_mut(), vec![row("s", "a", "Document")]);
    trigger(a.world_mut(), |v| matches!(v, Action::Select(..)));
    for _ in 0..3 {
        a.world_mut().resource_mut::<State>().unwrap().dirty = true;
        a.update();
        let w = a.world();
        let host = crate::ui::chrome::artifact_toolbar_host(w).unwrap();
        assert_eq!(children_of(w, host).len(), 1);
        assert!(!w.has::<Hidden>(host));
    }
    crate::surfaces::select(a.world_mut(), SurfaceId::Chat);
    a.update();
    let host = crate::ui::chrome::artifact_toolbar_host(a.world()).unwrap();
    assert!(a.world().has::<Hidden>(host));
    crate::surfaces::select(a.world_mut(), SurfaceId::Artifacts);
    crate::sidebars::toggle(a.world_mut());
    a.world_mut().insert_resource(Viewport::new(640., 800.));
    for _ in 0..12 {
        a.update();
    }
    let w = a.world();
    assert!(!w.has::<Hidden>(host));
    assert_eq!(children_of(w, host).len(), 1);
    let s = w.resource::<State>().unwrap();
    let viewport = w.get::<Rect>(s.viewport.unwrap()).unwrap();
    assert!(viewport.y <= 56. && viewport.h >= 740., "{viewport:?}");
}
