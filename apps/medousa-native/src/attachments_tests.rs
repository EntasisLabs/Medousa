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
