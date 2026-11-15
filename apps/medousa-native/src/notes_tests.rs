use super::*;
use serde_json::json;
fn fixture() -> App {
    let mut app = App::new();
    app.add_plugin(rndr_ui::UiPlugin);
    let w = app.world_mut();
    let parent = w.spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::row()
    },));
    mount(w, parent);
    app
}
fn response(path: &str, content: &str, token: Option<&str>) -> VaultNoteContentResponse {
    serde_json::from_value(json!({"note":{"path":path,"title":path,"byte_size":content.len(),"content_hash":"hash","modified_at_utc":"2026-01-01T00:00:00Z","created_at_utc":"2026-01-01T00:00:00Z","tags":[],"wikilinks_out":[],"backlinks":[],"kind":"note"},"content":content,"note_version":token})).unwrap()
}
fn click(w: &mut World, a: Action) {
    let id = next();
    w.resource_mut::<Notes>().unwrap().actions.insert(id, a);
    assert!(action(w, &HostIntent::Command(id)));
}
fn load(w: &mut World, path: &str, content: &str, token: Option<&str>) {
    click(w, Action::Open(path.into()));
    let c = drain_commands(w).pop().unwrap();
    apply(
        w,
        NoteUpdate {
            command: c,
            result: Ok(Response::Loaded(response(path, content, token))),
        },
    );
}
fn edit(w: &mut World, path: &str, content: &str) {
    let id = w.resource::<Notes>().unwrap().views[path].document;
    let d = w
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap();
    d.reload(d.revision(), content).unwrap();
    tick(w);
}
fn write(path: &str, content: Option<&str>, token: Option<&str>) -> VaultWriteResponse {
    let loaded = response(path, "", token);
    VaultWriteResponse {
        note: loaded.note,
        content: content.map(str::to_owned),
        note_version: loaded.note_version,
        created: false,
        vault_generation: None,
    }
}
#[test]
fn retained_editors_keep_distinct_drafts_history_and_entities() {
    let mut app = fixture();
    let w = app.world_mut();
    load(w, "a.md", "alpha", Some("a-v1"));
    edit(w, "a.md", "draft alpha");
    let a = w.resource::<Notes>().unwrap().views["a.md"].handle;
    load(w, "b.md", "beta", Some("b-v1"));
    edit(w, "b.md", "draft beta");
    assert!(w.has::<Hidden>(a.root));
    click(w, Action::Open("a.md".into()));
    assert!(drain_commands(w).is_empty());
    assert_eq!(w.resource::<Notes>().unwrap().views["a.md"].handle, a);
    click(w, Action::ToggleSource);
    assert!(!w.has::<Hidden>(a.root));
    assert_eq!(
        w.resource::<Notes>()
            .unwrap()
            .core
            .get("a.md")
            .unwrap()
            .content,
        "draft alpha"
    );
    assert_eq!(
        w.resource::<Notes>()
            .unwrap()
            .core
            .get("b.md")
            .unwrap()
            .content,
        "draft beta"
    );
}
#[test]
fn save_fences_duplicate_actions_and_preserves_inflight_edits() {
    let mut app = fixture();
    let w = app.world_mut();
    load(w, "a.md", "alpha", Some("opaque-v1"));
    edit(w, "a.md", "first edit");
    click(w, Action::Save);
    click(w, Action::Save);
    let commands = drain_commands(w);
    assert_eq!(commands.len(), 1);
    let c = commands[0].clone();
    assert!(
        matches!(&c.operation,Operation::Save{token,content,..} if token=="opaque-v1" && content=="first edit")
    );
    edit(w, "a.md", "newer edit");
    apply(
        w,
        NoteUpdate {
            command: c.clone(),
            result: Ok(Response::Saved(write(
                "a.md",
                Some("canonical first edit"),
                Some("opaque-v2"),
            ))),
        },
    );
    let b = w.resource::<Notes>().unwrap().core.get("a.md").unwrap();
    assert_eq!(b.content, "newer edit");
    assert_eq!(b.baseline_content, "canonical first edit");
    assert!(b.dirty);
    assert_eq!(b.content_hash.as_deref(), Some("opaque-v2"));
    // Duplicate terminal delivery cannot roll back a later local revision.
    apply(
        w,
        NoteUpdate {
            command: c,
            result: Ok(Response::Saved(write("a.md", Some("stale"), Some("stale")))),
        },
    );
    assert_eq!(
        w.resource::<Notes>()
            .unwrap()
            .core
            .get("a.md")
            .unwrap()
            .content,
        "newer edit"
    );
}
#[test]
fn canonical_save_response_updates_editor_when_no_newer_edits() {
    let mut app = fixture();
    let w = app.world_mut();
    load(w, "a.md", "alpha", None);
    edit(w, "a.md", "draft");
    click(w, Action::Save);
    let c = drain_commands(w).pop().unwrap();
    assert!(matches!(&c.operation,Operation::Save{token,..} if token=="hash"));
    apply(
        w,
        NoteUpdate {
            command: c,
            result: Ok(Response::Saved(write(
                "a.md",
                Some("canonical"),
                Some("v2"),
            ))),
        },
    );
    let n = w.resource::<Notes>().unwrap();
    assert!(!n.core.get("a.md").unwrap().dirty);
    assert_eq!(
        w.resource::<SourceDocuments>()
            .unwrap()
            .get(n.views["a.md"].document)
            .unwrap()
            .text(),
        "canonical"
    );
}
#[test]
fn conflict_and_unknown_outcome_never_discard_or_blindly_retry() {
    let mut app = fixture();
    let w = app.world_mut();
    load(w, "a.md", "alpha", Some("v1"));
    edit(w, "a.md", "my edits");
    click(w, Action::Save);
    let c = drain_commands(w).pop().unwrap();
    apply(
        w,
        NoteUpdate {
            command: c,
            result: Err("precondition failed or connection lost".into()),
        },
    );
    click(w, Action::Save);
    assert!(drain_commands(w).is_empty());
    click(w, Action::Check);
    let c = drain_commands(w).pop().unwrap();
    apply(
        w,
        NoteUpdate {
            command: c,
            result: Ok(Response::Loaded(response(
                "a.md",
                "agent edits",
                Some("v2"),
            ))),
        },
    );
    let b = w.resource::<Notes>().unwrap().core.get("a.md").unwrap();
    assert!(b.conflict);
    assert_eq!(b.content, "my edits");
    click(w, Action::Save);
    assert!(drain_commands(w).is_empty());
    click(w, Action::Acknowledge);
    click(w, Action::Save);
    assert!(
        matches!(&drain_commands(w)[0].operation,Operation::Save{content,token,..} if content=="my edits" && token=="v2")
    );
}
#[test]
fn checking_unchanged_disk_keeps_draft_without_false_conflict() {
    let mut app = fixture();
    let w = app.world_mut();
    load(w, "a.md", "alpha", Some("v1"));
    edit(w, "a.md", "draft");
    click(w, Action::Check);
    let c = drain_commands(w).pop().unwrap();
    apply(
        w,
        NoteUpdate {
            command: c,
            result: Ok(Response::Loaded(response("a.md", "alpha", Some("v2")))),
        },
    );
    let b = w.resource::<Notes>().unwrap().core.get("a.md").unwrap();
    assert!(!b.conflict);
    assert!(b.dirty);
    assert_eq!(b.content, "draft");
    assert_eq!(b.content_hash.as_deref(), Some("v2"));
}
#[test]
fn load_results_keep_original_note_identity_when_selection_moves() {
    let mut app = fixture();
    let w = app.world_mut();
    click(w, Action::Open("a.md".into()));
    let a = drain_commands(w).pop().unwrap();
    load(w, "b.md", "beta", Some("b-v1"));
    apply(
        w,
        NoteUpdate {
            command: a.clone(),
            result: Ok(Response::Loaded(response("a.md", "alpha", Some("a-v1")))),
        },
    );
    assert_eq!(
        w.resource::<Notes>().unwrap().selected.as_deref(),
        Some("b.md")
    );
    assert!(w.has::<Hidden>(w.resource::<Notes>().unwrap().views["a.md"].handle.root));
    apply(
        w,
        NoteUpdate {
            command: a,
            result: Ok(Response::Loaded(response("a.md", "stale", Some("stale")))),
        },
    );
    assert_eq!(
        w.resource::<Notes>()
            .unwrap()
            .core
            .get("a.md")
            .unwrap()
            .content,
        "alpha"
    );
}
#[test]
fn activation_and_pagination_are_explicit_and_fenced() {
    let mut app = fixture();
    let w = app.world_mut();
    assert!(drain_commands(w).is_empty());
    activate(w);
    activate(w);
    let commands = drain_commands(w);
    assert_eq!(commands.len(), 1);
    let c = commands[0].clone();
    let page = VaultNotesListResponse {
        notes: vec![],
        vault_generation: Some(7),
        next_cursor: Some("next".into()),
        truncated: false,
        reset_required: false,
    };
    apply(
        w,
        NoteUpdate {
            command: c.clone(),
            result: Ok(Response::List(page)),
        },
    );
    assert!(drain_commands(w).is_empty());
    click(w, Action::More);
    let more = drain_commands(w).pop().unwrap();
    assert!(
        matches!(more.operation,Operation::List{cursor:Some(ref cursor),generation:Some(7)} if cursor=="next")
    );
    apply(
        w,
        NoteUpdate {
            command: c,
            result: Err("stale duplicate".into()),
        },
    );
    assert!(w.resource::<Notes>().unwrap().list_error.is_empty());
}
#[test]
fn absent_save_token_blocks_write_at_model_boundary() {
    let mut n = Notes::default();
    n.selected = Some("a.md".into());
    n.core
        .dispatch(VaultBufferCommand::SeedBuffer {
            buffer: NoteBufferFact {
                path: "a.md".into(),
                content: "draft".into(),
                baseline_content: "base".into(),
                content_hash: None,
                title: "a".into(),
                dirty: true,
                content_revision: 1,
                conflict: false,
            },
        })
        .unwrap();
    n.save();
    assert!(n.queue.is_empty());
    assert!(n.errors.contains_key("a.md"));
}
#[test]
fn retained_source_undo_and_focus_do_not_leak_between_notes() {
    use rndr_core::editing::{EditOrigin, SourceSelection};
    let mut app = fixture();
    let w = app.world_mut();
    load(w, "a.md", "alpha", Some("v1"));
    let (id, handle) = {
        let v = &w.resource::<Notes>().unwrap().views["a.md"];
        (v.document, v.handle)
    };
    let d = w
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap();
    d.set_selection(SourceSelection { anchor: 0, head: 5 })
        .unwrap();
    d.replace_selections(d.revision(), "draft", EditOrigin::Host(17))
        .unwrap();
    tick(w);
    w.resource_mut::<Focus>().unwrap().entity = Some(handle.editor);
    load(w, "b.md", "beta", Some("b1"));
    assert!(w.resource::<Focus>().unwrap().entity.is_none());
    click(w, Action::Open("a.md".into()));
    let d = w
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap();
    d.undo().unwrap();
    assert_eq!(d.text(), "alpha");
    d.redo().unwrap();
    assert_eq!(d.text(), "draft");
}
#[test]
fn reverting_conflicted_buffer_does_not_bypass_explicit_acknowledgement() {
    let mut app = fixture();
    let w = app.world_mut();
    load(w, "a.md", "alpha", Some("v1"));
    edit(w, "a.md", "draft");
    click(w, Action::Check);
    let c = drain_commands(w).pop().unwrap();
    apply(
        w,
        NoteUpdate {
            command: c,
            result: Ok(Response::Loaded(response(
                "a.md",
                "agent edits",
                Some("v2"),
            ))),
        },
    );
    edit(w, "a.md", "alpha");
    edit(w, "a.md", "draft again");
    // Core clears its conflict flag when the draft returns to baseline, but
    // the adapter still requires acknowledging the externally changed bytes.
    assert!(
        !w.resource::<Notes>()
            .unwrap()
            .core
            .get("a.md")
            .unwrap()
            .conflict
    );
    click(w, Action::Save);
    assert!(drain_commands(w).is_empty());
    click(w, Action::Acknowledge);
    click(w, Action::Save);
    assert_eq!(drain_commands(w).len(), 1);
}

#[test]
fn reading_view_is_default_and_source_toggle_preserves_exact_document() {
    let mut app = fixture();
    let w = app.world_mut();
    let content = "---\ntags: [native, notes]\n---\n# Hello\n\n**Readable** document.";
    load(w, "hello.md", content, Some("version"));
    let v = &w.resource::<Notes>().unwrap().views["hello.md"];
    let (handle, preview, document) = (v.handle, v.preview, v.document);
    assert!(w.has::<Hidden>(handle.root));
    assert!(!w.has::<Hidden>(preview));
    assert_eq!(
        w.get::<MarkdownView>(preview).unwrap().source(),
        "# Hello\n\n**Readable** document."
    );
    click(w, Action::ToggleSource);
    assert!(!w.has::<Hidden>(handle.root));
    assert!(w.has::<Hidden>(preview));
    edit(w, "hello.md", "# New title\n\nLocal draft");
    click(w, Action::ToggleSource);
    assert_eq!(
        w.get::<MarkdownView>(preview).unwrap().source(),
        "# New title\n\nLocal draft"
    );
    click(w, Action::ToggleSource);
    assert_eq!(
        w.resource::<Notes>().unwrap().views["hello.md"].document,
        document
    );
    assert_eq!(
        w.resource::<SourceDocuments>()
            .unwrap()
            .get(document)
            .unwrap()
            .text(),
        "# New title\n\nLocal draft"
    );
    assert!(drain_commands(w).is_empty());
}

#[test]
fn frontmatter_is_hidden_only_when_delimited_and_never_mutates_source() {
    assert_eq!(document_body("---\ntags: []\n---\n# Note"), "# Note");
    assert_eq!(document_body("---\r\ntags: []\r\n---\r\n# Note"), "# Note");
    assert_eq!(
        document_body("---\nnot closed\n# Note"),
        "---\nnot closed\n# Note"
    );
    assert_eq!(document_body("Body\n---\nBody"), "Body\n---\nBody");
}

#[test]
#[ignore = "Explicit synthetic offscreen visual acceptance; writes verification PNGs"]
fn notes_visual_acceptance() {
    use std::sync::Arc;
    let mut app = fixture();
    app.add_plugin(MarkdownPlugin);
    rndr_ui::apply_theme_bridge(
        app.world_mut(),
        rndr_ui::UiTheme {
            appearance: rndr_ui::Appearance::Dark,
            density: rndr_ui::Density::Compact,
            ..Default::default()
        },
    );
    let engine: Arc<dyn TextEngine> = Arc::new(rndr_render::ParleyText::new());
    app.world_mut().insert_resource(engine);
    app.world_mut().insert_resource(MarkdownTheme {
        surface: Color::rgba(0., 0., 0., 0.),
        text: Color::rgb(0.76, 0.76, 0.79),
        ..MarkdownTheme::default()
    });
    app.world_mut().insert_resource(Viewport::new(1200., 850.));
    let w = app.world_mut();
    let titles = [
        "Medousa backlog — Encrypted secret sharing via MCP",
        "rndr UI Library — Council Review",
        "What Makes a Great Code Editor",
        "Chinese Elm — Indoor Bonsai Care Guide",
        "Medousa fleet rollout — 2026-10-29",
        "Your Cache Hit Rate Is Killing Your Context",
        "Your Cache Optimization Is Damaging the Context Window",
        "Fleet recovery and retry repair",
        "Offline knowledge library",
        "Science, math and art starter kit",
    ];
    w.resource_mut::<Notes>().unwrap().list = titles
        .iter()
        .enumerate()
        .map(|(i, title)| {
            let mut note = response(&format!("Backlog/note-{i}.md"), "", Some("v1")).note;
            note.title = (*title).into();
            serde_json::from_value::<VaultNoteSummary>(serde_json::to_value(note).unwrap()).unwrap()
        })
        .collect();
    let body = "---\ntags: [backlog, mcp, security]\n---\n# Medousa backlog — Encrypted secret sharing via MCP\n\n- **Status:** Backlog / not implemented\n- **Captured:** 2026-10-29\n- **Source:** Pairing-ticket encryption workflow; user confirmed the local setup worked.\n\n## Outcome\n\nMake sharing a pairing ticket or other authorized secret from another agent to Medousa a native tool workflow, without plaintext secrets in chat or manual script/key/file exchange.\n\n## Proposed flow\n\n1. Medousa host creates a recipient encryption keypair in secure local storage.\n2. The sender invokes the proposed `encrypt_and_share_secret` tool with the authorized secret and recipient.\n3. The tool encrypts the payload and returns an opaque retrieval handle.\n\n**Key ownership:** Medousa host holds the private key. No private key or AES key is shared in chat.\n\n## Requirements / acceptance criteria\n\n- Preserve the existing ownership and approval boundaries.\n- Never expose plaintext to model context by default.\n";
    load(w, "Backlog/note-0.md", body, Some("v1"));
    let mut exporter = rndr_render::Exporter::new().unwrap();
    std::fs::create_dir_all("verification").unwrap();
    for (width, height, suffix) in [(1200, 850, "wide"), (760, 540, "narrow")] {
        app.world_mut()
            .insert_resource(Viewport::new(width as f32, height as f32));
        for _ in 0..4 {
            rndr_paint::frame(&mut app);
        }
        let (_, image) = rndr_paint::frame_with_snapshot_frame(&mut app, |world, frame| {
            exporter
                .render_frame(Some(world), frame, width, height, 1.)
                .unwrap()
        });
        image::save_buffer(
            format!("verification/native-notes-polish-{suffix}.png"),
            &image.rgba,
            image.width,
            image.height,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
    click(app.world_mut(), Action::ToggleSource);
    app.world_mut().insert_resource(Viewport::new(1200., 850.));
    for _ in 0..4 {
        rndr_paint::frame(&mut app);
    }
    let (_, image) = rndr_paint::frame_with_snapshot_frame(&mut app, |world, frame| {
        exporter
            .render_frame(Some(world), frame, 1200, 850, 1.)
            .unwrap()
    });
    image::save_buffer(
        "verification/native-notes-polish-source.png",
        &image.rgba,
        image.width,
        image.height,
        image::ColorType::Rgba8,
    )
    .unwrap();
}

#[test]
fn native_note_tabs_activate_only_their_own_retained_document() {
    let mut app = fixture();
    let w = app.world_mut();
    load(w, "a.md", "# Alpha", Some("v1"));
    load(w, "b.md", "# Beta", Some("v2"));
    let n = w.resource::<Notes>().unwrap();
    let (strip, tab, preview) = (
        n.tabstrip.unwrap(),
        n.views["a.md"].tab,
        n.views["a.md"].preview,
    );
    send(
        w,
        TabStripEvent {
            strip,
            kind: TabStripEventKind::Activated(TabId(tab)),
        },
    );
    app.add_system(Schedule::Update, tick);
    app.update();
    let w = app.world_mut();
    assert_eq!(
        w.resource::<Notes>().unwrap().selected.as_deref(),
        Some("a.md")
    );
    assert!(!w.has::<Hidden>(preview));
    assert!(drain_commands(w).is_empty());
}

#[test]
fn revision_capture_tracks_edits_reverts_and_remote_reload() {
    let mut app = fixture();
    let w = app.world_mut();
    load(w, "a.md", "alpha", Some("v1"));
    tick(w);
    let document = w.resource::<Notes>().unwrap().views["a.md"].document;
    let revision = w
        .resource::<SourceDocuments>()
        .unwrap()
        .get(document)
        .unwrap()
        .revision();
    assert_eq!(
        w.resource::<Notes>().unwrap().views["a.md"].captured_revision,
        Some(revision)
    );
    for _ in 0..4 {
        tick(w);
    }
    edit(w, "a.md", "draft");
    assert!(
        w.resource::<Notes>()
            .unwrap()
            .core
            .get("a.md")
            .unwrap()
            .dirty
    );
    edit(w, "a.md", "alpha");
    assert!(
        !w.resource::<Notes>()
            .unwrap()
            .core
            .get("a.md")
            .unwrap()
            .dirty
    );
    click(w, Action::Check);
    let command = drain_commands(w).pop().unwrap();
    apply(
        w,
        NoteUpdate {
            command,
            result: Ok(Response::Loaded(response("a.md", "remote", Some("v2")))),
        },
    );
    tick(w);
    assert_eq!(
        w.resource::<SourceDocuments>()
            .unwrap()
            .get(document)
            .unwrap()
            .text(),
        "remote"
    );
    assert_eq!(
        w.resource::<Notes>()
            .unwrap()
            .core
            .get("a.md")
            .unwrap()
            .content,
        "remote"
    );
}
