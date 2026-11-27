use super::*;

fn reference(id: &str) -> MediaRef {
    MediaRef {
        media_id: id.into(),
        kind: "document".into(),
        mime: "text/plain".into(),
        label: Some(id.into()),
        source_media_id: None,
        generation_id: None,
        parent_generation_id: None,
    }
}
fn world() -> World {
    let mut world = World::new();
    world.insert_resource(Attachments::default());
    world
}
fn pending(world: &mut World, session: &str, id: u64) {
    let state = world.resource_mut::<Attachments>().unwrap();
    state.next = state.next.max(id);
    state.drafts.entry(session.into()).or_default().push(Item {
        id,
        label: "Files".into(),
        state: ItemState::Pending,
    });
}
fn completed(session: &str, id: u64, media_id: &str) -> AttachmentUpdate {
    AttachmentUpdate {
        session: session.into(),
        id,
        items: vec![(media_id.into(), Ok(reference(media_id)))],
    }
}

#[test]
fn pending_and_failed_files_prevent_send_but_not_other_sessions() {
    let mut w = world();
    pending(&mut w, "a", 1);
    assert!(media_refs(&w, "a").unwrap_err().contains("Wait"));
    assert!(media_refs(&w, "b").unwrap().is_empty());
    apply(
        &mut w,
        AttachmentUpdate {
            session: "a".into(),
            id: 1,
            items: vec![("bad".into(), Err("denied".into()))],
        },
    );
    assert!(media_refs(&w, "a").unwrap_err().contains("denied"));
}

#[test]
fn late_completion_is_scoped_to_its_original_session() {
    let mut w = world();
    pending(&mut w, "a", 1);
    pending(&mut w, "b", 2);
    w.resource_mut::<Attachments>().unwrap().session = Some("b".into());
    apply(&mut w, completed("a", 1, "a-file"));
    assert_eq!(media_refs(&w, "a").unwrap(), vec![reference("a-file")]);
    assert!(media_refs(&w, "b").is_err());
}

#[test]
fn removed_pending_upload_cannot_resurrect_or_replace_newer_selection() {
    let mut w = world();
    pending(&mut w, "a", 1);
    w.resource_mut::<Attachments>()
        .unwrap()
        .drafts
        .get_mut("a")
        .unwrap()
        .clear();
    pending(&mut w, "a", 2);
    apply(&mut w, completed("a", 1, "old"));
    assert!(media_refs(&w, "a").is_err());
    apply(&mut w, completed("a", 2, "new"));
    assert_eq!(media_refs(&w, "a").unwrap(), vec![reference("new")]);
    apply(&mut w, completed("a", 2, "duplicate"));
    assert_eq!(media_refs(&w, "a").unwrap(), vec![reference("new")]);
}

#[test]
fn admitted_clear_preserves_new_attachments_and_other_sessions() {
    let mut w = world();
    pending(&mut w, "a", 1);
    apply(&mut w, completed("a", 1, "sent"));
    pending(&mut w, "a", 3);
    apply(&mut w, completed("a", 3, "new"));
    pending(&mut w, "b", 5);
    apply(&mut w, completed("b", 5, "other"));
    clear_admitted(&mut w, "a", &[reference("sent")]);
    assert_eq!(media_refs(&w, "a").unwrap(), vec![reference("new")]);
    assert_eq!(media_refs(&w, "b").unwrap(), vec![reference("other")]);
}

#[test]
fn picker_cancel_clears_pending_and_unblocks_send() {
    let mut w = world();
    pending(&mut w, "a", 1);
    apply(
        &mut w,
        AttachmentUpdate {
            session: "a".into(),
            id: 1,
            items: vec![],
        },
    );
    assert!(media_refs(&w, "a").unwrap().is_empty());
}

#[test]
fn disk_reads_reject_empty_directory_and_oversized_files() {
    let temp = tempfile::tempdir().unwrap();
    assert!(read_file(temp.path().to_owned()).is_err());
    let path = temp.path().join("sample.txt");
    std::fs::write(&path, []).unwrap();
    assert!(read_file(path.clone()).unwrap_err().contains("Empty"));
    std::fs::write(&path, b"hello").unwrap();
    let (bytes, mime) = read_file(path.clone()).unwrap();
    assert_eq!(bytes, b"hello");
    assert_eq!(mime, "text/plain");
    std::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(MAX_BYTES + 1)
        .unwrap();
    assert!(read_file(path).unwrap_err().contains("25 MiB"));
}

#[test]
fn kinds_match_supported_media_references() {
    assert_eq!(kind("image/png"), "image");
    assert_eq!(kind("audio/wav"), "audio");
    assert_eq!(kind("text/csv"), "spreadsheet");
    assert_eq!(kind("application/pdf"), "document");
}

#[test]
fn composer_attachment_tray_is_separate_bounded_and_session_owned() {
    let mut app = App::new();
    app.add_plugin(rndr_ui::UiPlugin);
    app.world_mut().insert_resource(Viewport::new(700., 300.));
    let w = app.world_mut();
    let host = w.spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    },));
    let tray = w.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            ..Layout::column()
        },
        Scroll::default(),
    ));
    set_parent(w, tray, host);
    let picker = setup(w, host, tray);
    w.get_mut::<Layout>(picker).unwrap().width = Size::Px(40.);
    sync(w, picker, Some("a"));
    assert!(w.has::<Hidden>(tray));
    for id in 1..=5 {
        pending(w, "a", id);
    }
    w.resource_mut::<Attachments>().unwrap().revision += 1;
    sync(w, picker, Some("a"));
    app.update();
    app.update();
    let w = app.world_mut();
    assert!(!w.has::<Hidden>(tray));
    assert_eq!(
        children_of(w, picker).len(),
        1,
        "only picker belongs in narrow toolbar slot"
    );
    assert_eq!(children_of(w, tray).len(), 5);
    assert_eq!(w.get::<Rect>(tray).unwrap().h, 96.);
    assert!(w.get::<Rect>(tray).unwrap().w > 600.);
    sync(w, picker, Some("b"));
    assert!(w.has::<Hidden>(tray));
    assert!(children_of(w, tray).is_empty());
}

#[test]
fn image_upload_has_preview_and_removal_does_not_send_or_cross_sessions() {
    let mut world = World::new();
    let parent = world.spawn((Layout::column(),));
    let tray = world.spawn((Layout::column(),));
    let picker = setup(&mut world, parent, tray);
    pending(&mut world, "a", 1);
    let mut media = reference("m");
    media.kind = "image".into();
    media.mime = "image/png".into();
    media.label = Some("test.png".into());
    apply(
        &mut world,
        AttachmentUpdate {
            session: "a".into(),
            id: 1,
            items: vec![("test.png".into(), Ok(media))],
        },
    );
    sync(&mut world, picker, Some("a"));
    crate::media_preview::tick(&mut world);
    assert_eq!(crate::media_preview::drain_commands(&mut world).len(), 1);
    assert_eq!(media_refs(&world, "a").unwrap().len(), 1);
    assert!(matches!(world.get::<Layout>(tray).unwrap().height, Size::Px(h) if h >= 96.));
    sync(&mut world, picker, Some("b"));
    assert!(world.has::<Hidden>(tray));
    assert!(children_of(&world, tray).is_empty());
}

#[test]
fn direct_drops_reserve_slots_and_keep_exact_paths_and_session() {
    let mut w = world();
    queue_files(
        &mut w,
        "a",
        vec!["/device/one.txt".into(), "/device/Files".into()],
    )
    .unwrap();
    assert!(media_refs(&w, "a").unwrap_err().contains("Wait"));
    queue_files(&mut w, "a", vec!["/device/three.txt".into()]).unwrap();
    let commands = drain_commands(&mut w);
    assert_eq!(commands.len(), 3);
    assert!(commands.iter().all(|c| c.session == "a" && c.limit == 1));
    assert_eq!(
        commands[0].files,
        Some(vec![PathBuf::from("/device/one.txt")])
    );
    for (i, c) in commands.into_iter().enumerate() {
        apply(&mut w, completed(&c.session, c.id, &format!("file-{i}")));
    }
    assert_eq!(media_refs(&w, "a").unwrap().len(), 3);
    assert!(media_refs(&w, "b").unwrap().is_empty());
}

#[test]
fn direct_drop_capacity_rejects_whole_batch_without_partial_admission() {
    let mut w = world();
    queue_files(
        &mut w,
        "a",
        (0..9).map(|i| format!("{i}.txt").into()).collect(),
    )
    .unwrap();
    assert!(queue_files(&mut w, "a", vec!["extra.txt".into(), "overflow.txt".into()]).is_err());
    assert_eq!(drain_commands(&mut w).len(), 9);
    queue_files(&mut w, "a", vec!["last.txt".into()]).unwrap();
    assert!(queue_files(&mut w, "a", vec!["too-many.txt".into()]).is_err());
    assert_eq!(w.resource::<Attachments>().unwrap().drafts["a"].len(), 10);
}

#[test]
fn pending_picker_cannot_race_with_direct_drops() {
    let mut w = world();
    pending(&mut w, "a", 1);
    w.resource_mut::<Attachments>()
        .unwrap()
        .drafts
        .get_mut("a")
        .unwrap()[0]
        .state = ItemState::Picking;
    assert!(
        queue_files(&mut w, "a", vec!["one.txt".into()])
            .unwrap_err()
            .contains("picker")
    );
    assert!(drain_commands(&mut w).is_empty());
}

#[tokio::test]
async fn dropped_device_file_upload_uses_owning_session_and_existing_credentials() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::Arc,
        time::Duration,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(4)))
            .unwrap();
        let mut raw = Vec::new();
        let mut buf = [0; 4096];
        let header_end = loop {
            let n = socket.read(&mut buf).unwrap();
            assert!(n > 0);
            raw.extend_from_slice(&buf[..n]);
            if let Some(i) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                break i + 4;
            }
        };
        let header = String::from_utf8_lossy(&raw[..header_end]).to_ascii_lowercase();
        let length: usize = header
            .lines()
            .find_map(|l| {
                l.strip_prefix("content-length:")
                    .and_then(|v| v.trim().parse().ok())
            })
            .unwrap();
        while raw.len() < header_end + length {
            let n = socket.read(&mut buf).unwrap();
            assert!(n > 0);
            raw.extend_from_slice(&buf[..n]);
        }
        let raw = String::from_utf8_lossy(&raw);
        assert!(
            raw.starts_with("POST /v1/media/upload?session_id=wire-session HTTP/1.1"),
            "{raw}"
        );
        assert!(header.contains("authorization: bearer fixture-credential"));
        assert!(raw.contains("filename=\"device-note.txt\""));
        assert!(raw.contains("device file contents"));
        assert!(!raw.contains("device-local-private-dir"));
        let body = r#"{"media_id":"uploaded","mime":"text/plain","byte_size":20,"label":"device-note.txt"}"#;
        write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
    });
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::AUTHORIZATION,
        "Bearer fixture-credential".parse().unwrap(),
    );
    let http = reqwest::Client::builder()
        .default_headers(headers)
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let client = MedousaClient::with_transport(
        Arc::new(medousa_sdk::HttpTransport::with_client(http)),
        origin,
    );
    let dir = tempfile::Builder::new()
        .prefix("device-local-private-dir")
        .tempdir()
        .unwrap();
    let path = dir.path().join("device-note.txt");
    std::fs::write(&path, b"device file contents").unwrap();
    let mut w = world();
    let session = crate::routing::qualify("remote-workshop", "wire-session");
    queue_files(&mut w, &session, vec![path]).unwrap();
    let update = execute(&client, drain_commands(&mut w).remove(0))
        .await
        .unwrap();
    apply(&mut w, update);
    let refs = media_refs(&w, &session).unwrap();
    assert_eq!(refs[0].media_id, "uploaded");
    assert!(media_refs(&w, "wire-session").unwrap().is_empty());
    server.join().unwrap();
}

#[tokio::test]
async fn invalid_dropped_file_stays_removable_and_blocks_send_without_network_request() {
    use std::sync::Arc;
    let client = MedousaClient::with_transport(
        Arc::new(medousa_sdk::HttpTransport::new()),
        "http://127.0.0.1:1",
    );
    let dir = tempfile::tempdir().unwrap();
    let mut w = world();
    queue_files(&mut w, "a", vec![dir.path().to_owned()]).unwrap();
    let update = execute(&client, drain_commands(&mut w).remove(0))
        .await
        .unwrap();
    apply(&mut w, update);
    assert!(
        media_refs(&w, "a")
            .unwrap_err()
            .contains("Only regular files")
    );
    assert!(matches!(
        w.resource::<Attachments>().unwrap().drafts["a"][0].state,
        ItemState::Failed(_)
    ));
    assert!(
        drain_commands(&mut w).is_empty(),
        "Failed drops are never auto-retried"
    );
}
