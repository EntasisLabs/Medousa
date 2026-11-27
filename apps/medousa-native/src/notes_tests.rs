use super::*;
#[path = "notes_management_tests.rs"]
mod management_tests;
use serde_json::json;
fn fixture() -> App {
    let mut app = App::new();
    app.add_plugin(rndr_ui::UiPlugin);
    app.add_plugin(rndr_markdown_ui::MarkdownPlugin);
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
fn source_rebind_rejects_dirty_buffers_and_pending_saves_without_replacing_documents() {
    let mut app = fixture();
    let w = app.world_mut();
    load(w, "a.md", "saved", Some("v1"));
    let document = w.resource::<Notes>().unwrap().views["a.md"].document;
    assert!(can_rebind(w));
    edit(w, "a.md", "keep this unsaved draft");
    assert!(!can_rebind(w));
    assert_eq!(
        w.resource::<Notes>().unwrap().views["a.md"].document,
        document
    );
    click(w, Action::Save);
    assert!(
        !can_rebind(w),
        "uncertain or pending saves must not be rebound"
    );
    assert_eq!(
        w.resource::<Notes>().unwrap().views["a.md"].document,
        document
    );
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
    let identity = commands[0].clone();
    assert!(matches!(
        identity.operation,
        Operation::Manage(management::Command::Identity)
    ));
    apply(
        w,
        NoteUpdate {
            command: identity,
            result: Ok(Response::Managed(
                serde_json::to_value(medousa_types::VaultRootsResponse {
                    active_root_id: "vault-a".into(),
                    roots: vec![],
                })
                .unwrap(),
            )),
        },
    );
    assert_eq!(
        w.resource::<Notes>().unwrap().core.workshop_scope_id(),
        "vault-a"
    );
    let c = drain_commands(w).pop().unwrap();
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
fn rich_markdown_uses_shared_session_and_preserves_frontmatter_bytes() {
    let mut app = fixture();
    let w = app.world_mut();
    let prefix = "\u{feff}---\r\ntitle: Keep me\r\ntags: [one, two]\r\n---\r\n";
    load(w, "a.md", &format!("{prefix}alpha"), Some("v1"));
    click(w, Action::Write);
    let (rich_id, rich_root, source_id) = {
        let v = &w.resource::<Notes>().unwrap().views["a.md"];
        (v.rich_document, v.rich.root, v.document)
    };
    assert!(!w.has::<Hidden>(rich_root));
    w.resource_mut::<MarkdownDocuments>()
        .unwrap()
        .get_mut(rich_id)
        .unwrap()
        .move_caret(0, 5)
        .unwrap();
    click(w, Action::Format(MarkdownCommand::Strong));
    let expected = format!("{prefix}**alpha**");
    assert_eq!(
        w.resource::<Notes>()
            .unwrap()
            .core
            .get("a.md")
            .unwrap()
            .content,
        expected
    );
    assert_eq!(
        w.resource::<SourceDocuments>()
            .unwrap()
            .get(source_id)
            .unwrap()
            .text(),
        expected
    );
    click(w, Action::ToggleSource);
    assert!(w.has::<Hidden>(rich_root));
    click(w, Action::Write);
    assert_eq!(
        w.resource::<Notes>().unwrap().views["a.md"].rich_document,
        rich_id
    );
    assert_eq!(
        w.resource::<MarkdownDocuments>()
            .unwrap()
            .get(rich_id)
            .unwrap()
            .bytes(),
        "**alpha**"
    );
    click(w, Action::Save);
    assert!(matches!(drain_commands(w).pop().unwrap().operation,
        Operation::Save { content, token, .. } if content == expected && token == "v1"));
}
#[test]
fn source_and_rich_handoffs_preserve_drafts_and_independent_undo() {
    let mut app = fixture();
    let w = app.world_mut();
    load(w, "a.md", "alpha", Some("v1"));
    click(w, Action::Write);
    let id = w.resource::<Notes>().unwrap().views["a.md"].rich_document;
    w.resource_mut::<MarkdownDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap()
        .move_caret(0, 5)
        .unwrap();
    click(w, Action::Format(MarkdownCommand::Strong));
    click(w, Action::Format(MarkdownCommand::Undo));
    assert_eq!(
        w.resource::<Notes>()
            .unwrap()
            .core
            .get("a.md")
            .unwrap()
            .content,
        "alpha"
    );
    click(w, Action::Format(MarkdownCommand::Redo));
    click(w, Action::ToggleSource);
    let source = w.resource::<Notes>().unwrap().views["a.md"].document;
    w.resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(source)
        .unwrap()
        .undo()
        .unwrap();
    tick(w);
    assert_eq!(
        w.resource::<Notes>()
            .unwrap()
            .core
            .get("a.md")
            .unwrap()
            .content,
        "alpha"
    );
    click(w, Action::Write);
    assert_eq!(
        w.resource::<MarkdownDocuments>()
            .unwrap()
            .get(id)
            .unwrap()
            .bytes(),
        "alpha"
    );
    load(w, "b.md", "beta", Some("b-v1"));
    click(w, Action::Open("a.md".into()));
    assert_eq!(
        w.resource::<Notes>().unwrap().views["a.md"].rich_document,
        id
    );
}
#[test]
fn rich_composition_blocks_mode_note_and_management_navigation() {
    let mut app = fixture();
    let w = app.world_mut();
    load(w, "a.md", "alpha", Some("v1"));
    click(w, Action::Write);
    let (id, root) = {
        let v = &w.resource::<Notes>().unwrap().views["a.md"];
        (v.rich_document, v.rich.root)
    };
    w.resource_mut::<MarkdownDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap()
        .composition_start(0, 0)
        .unwrap();
    assert!(!can_deactivate(w));
    click(w, Action::ToggleSource);
    click(w, Action::Read);
    click(w, Action::Open("b.md".into()));
    click(w, Action::Manage(management::Action::CloseNote));
    assert!(w.resource::<Notes>().unwrap().live);
    assert!(!w.has::<Hidden>(root));
    assert_eq!(
        w.resource::<Notes>().unwrap().selected.as_deref(),
        Some("a.md")
    );
    assert!(drain_commands(w).is_empty());
    w.resource_mut::<MarkdownDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap()
        .composition_cancel();
    assert!(can_deactivate(w));
    w.resource_mut::<Focus>().unwrap().entity = Some(root);
    click(w, Action::Open("b.md".into()));
    assert!(w.resource::<Focus>().unwrap().entity.is_none());
}
#[test]
fn rich_edits_during_save_remain_dirty_against_acknowledged_baseline() {
    let mut app = fixture();
    let w = app.world_mut();
    load(w, "a.md", "alpha", Some("v1"));
    click(w, Action::Write);
    let id = w.resource::<Notes>().unwrap().views["a.md"].rich_document;
    w.resource_mut::<MarkdownDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap()
        .move_caret(0, 5)
        .unwrap();
    click(w, Action::Format(MarkdownCommand::Strong));
    click(w, Action::Save);
    let command = drain_commands(w).pop().unwrap();
    w.resource_mut::<MarkdownDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap()
        .edit(
            rndr_markdown::Command::Replace {
                start: 0,
                end: 9,
                text: "late draft".into(),
            },
            2000,
        )
        .unwrap();
    apply(
        w,
        NoteUpdate {
            command,
            result: Ok(Response::Saved(write(
                "a.md",
                Some("**alpha**"),
                Some("v2"),
            ))),
        },
    );
    let n = w.resource::<Notes>().unwrap();
    let b = n.core.get("a.md").unwrap();
    assert_eq!(b.content, "late draft");
    assert_eq!(b.baseline_content, "**alpha**");
    assert_eq!(b.content_hash.as_deref(), Some("v2"));
    assert!(b.dirty);
}
#[test]
fn shared_library_tree_preserves_nested_disclosure_through_search() {
    let mut app = fixture();
    let w = app.world_mut();
    let summary = |path: &str, title: &str| {
        let mut note = response(path, "", Some("v1")).note;
        note.title = title.into();
        serde_json::from_value::<VaultNoteSummary>(serde_json::to_value(note).unwrap()).unwrap()
    };
    let mut n = std::mem::take(w.resource_mut::<Notes>().unwrap());
    n.list = vec![
        summary("work/deep/a.md", "Alpha"),
        summary("work/b.md", "Beta"),
        summary("b.md", "Root"),
    ];
    n.browse = Browse::Folders;
    n.expanded.insert("work".into());
    let (nodes, expanded) = library_model(&mut n);
    assert_eq!(nodes.len(), 5);
    let work = n.tree_ids["folder:work"];
    let deep = n.tree_ids["folder:work/deep"];
    let alpha = n.tree_ids["note:work/deep/a.md"];
    assert_eq!(
        nodes.iter().find(|node| node.id == deep).unwrap().parent,
        Some(work)
    );
    assert_eq!(
        nodes.iter().find(|node| node.id == alpha).unwrap().parent,
        Some(deep)
    );
    assert_eq!(expanded, vec![work]);
    n.filter = "ALPHA".into();
    let (filtered, expanded) = library_model(&mut n);
    assert_eq!(filtered.len(), 3);
    assert_eq!(expanded, vec![work, deep]);
    assert_eq!(n.tree_paths.len(), 1);
    assert_eq!(n.expanded, std::collections::HashSet::from(["work".into()]));
    n.filter.clear();
    let (_, expanded) = library_model(&mut n);
    assert_eq!(expanded, vec![work]);
    assert_eq!(n.tree_ids["note:work/deep/a.md"], alpha);
    n.redraw = true;
    w.insert_resource(n);
    tick(w);
    let tree = w.resource::<Notes>().unwrap().tree.unwrap();
    assert_eq!(tree.node_count(w), 5);
    assert_eq!(tree.visible_count(w), 4);
}

#[test]
fn shared_library_keyboard_selection_snapshots_active_draft() {
    let mut app = fixture();
    let w = app.world_mut();
    load(w, "a.md", "first", Some("a-v1"));
    edit(w, "a.md", "retained first draft");
    let mut n = std::mem::take(w.resource_mut::<Notes>().unwrap());
    n.list = ["a.md", "b.md"]
        .iter()
        .map(|path| {
            serde_json::from_value::<VaultNoteSummary>(
                serde_json::to_value(response(path, "", Some("v1")).note).unwrap(),
            )
            .unwrap()
        })
        .collect();
    n.redraw = true;
    w.insert_resource(n);
    tick(w);
    let n = w.resource::<Notes>().unwrap();
    let tree = n.tree.unwrap().root;
    let target = n.tree_ids["note:b.md"];
    send(
        w,
        TreeEvent {
            tree,
            kind: TreeEventKind::Selected(Some(target)),
        },
    );
    app.update(); // Promote queued shared tree events before host Update.
    let w = app.world_mut();
    tick(w);
    assert_eq!(
        w.resource::<Notes>().unwrap().selected.as_deref(),
        Some("b.md")
    );
    assert_eq!(
        w.resource::<Notes>()
            .unwrap()
            .core
            .get("a.md")
            .unwrap()
            .content,
        "retained first draft"
    );
    assert!(
        drain_commands(w)
            .iter()
            .any(|c| matches!(&c.operation, Operation::Load(path) if path == "b.md"))
    );
}

#[test]
fn note_statistics_match_body_only_unicode_and_read_time_contract() {
    assert_eq!(note_statistics(""), "Empty note");
    assert_eq!(
        note_statistics("---\ntitle: ignored\n---\nhello 🌊"),
        "2 words · ~1 min read · 8 chars"
    );
    assert_eq!(
        note_statistics(&vec!["word"; 201].join(" ")),
        "201 words · ~2 min read · 1004 chars"
    );
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
    let mut app = crate::ui::app();
    app.update();
    crate::surfaces::select(app.world_mut(), crate::surfaces::SurfaceId::Notes);
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
    let text_color = visual::color(app.world(), visual::text());
    app.world_mut().insert_resource(MarkdownTheme {
        surface: Color::rgba(0., 0., 0., 0.),
        text: text_color,
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
    for (width, height, suffix) in [
        (1200, 850, "wide"),
        (760, 540, "narrow"),
        (1200, 850, "light"),
        (760, 650, "light-zoom"),
    ] {
        publish_system_appearance(
            app.world_mut(),
            SystemAppearance {
                dark: Some(!suffix.starts_with("light")),
                ..Default::default()
            },
        );
        app.world_mut()
            .resource_mut::<rndr_ui::UiTheme>()
            .unwrap()
            .reading_scale = if suffix.ends_with("zoom") { 1.5 } else { 1. };
        app.world_mut()
            .insert_resource(Viewport::new(width as f32, height as f32));
        for _ in 0..4 {
            rndr_paint::frame(&mut app);
        }
        if suffix == "narrow" {
            // Exercise the breadcrumb focus chrome, not just its idle paint.
            let group = app
                .world()
                .resource::<Notes>()
                .unwrap()
                .actions
                .values()
                .find_map(|a| match a {
                    Action::Picker(Picker::Group, anchor) => Some(*anchor),
                    _ => None,
                })
                .unwrap();
            assert!(request_focus(app.world_mut(), group));
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
fn integrated_note_band_preserves_documents_across_surface_switches() {
    let mut app = crate::ui::app();
    app.update();
    crate::surfaces::select(app.world_mut(), crate::surfaces::SurfaceId::Notes);
    load(app.world_mut(), "a.md", "# Alpha", Some("v1"));
    load(app.world_mut(), "b.md", "# Beta", Some("v2"));
    let document = app.world().resource::<Notes>().unwrap().views["b.md"].document;
    for width in [1200., 760., 1600.] {
        app.world_mut().insert_resource(Viewport::new(width, 850.));
        for _ in 0..8 {
            app.update();
        }
        let w = app.world();
        let n = w.resource::<Notes>().unwrap();
        let tabs = n.tabstrip.unwrap();
        assert!(w.has::<Hidden>(tabs));
        let tab = w
            .query::<Tab>()
            .iter()
            .find(|(_, t)| t.id == n.views["b.md"].tab)
            .unwrap()
            .0;
        let r = w.get::<Rect>(tab).unwrap();
        assert!(r.y >= 0. && r.y + r.h <= crate::ui::chrome::HEIGHT);
        let toolbar = w.get::<Rect>(n.toolbar.unwrap()).unwrap();
        assert!(toolbar.y >= crate::ui::chrome::HEIGHT);
        for child in children_of(w, n.toolbar.unwrap()) {
            let r = w.get::<Rect>(child).unwrap();
            assert!(r.x + r.w <= width + 1., "note action clipped: {r:?}");
        }
    }
    crate::surfaces::select(app.world_mut(), crate::surfaces::SurfaceId::Chat);
    for _ in 0..3 {
        app.update();
    }
    crate::surfaces::select(app.world_mut(), crate::surfaces::SurfaceId::Notes);
    for _ in 0..3 {
        app.update();
    }
    assert_eq!(
        app.world().resource::<Notes>().unwrap().views["b.md"].document,
        document
    );
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

#[test]
fn find_retains_per_note_query_and_source_identity_across_modes() {
    let mut app = fixture();
    let w = app.world_mut();
    load(
        w,
        "a.md",
        "---\ntitle: Exact\n---\n🦀 alpha alpha",
        Some("a1"),
    );
    click(w, Action::Write);
    let document = w.resource::<Notes>().unwrap().views["a.md"].document;
    click(w, Action::Find);
    let n = w.resource::<Notes>().unwrap();
    assert!(n.source && !n.live);
    let bar = n.views["a.md"].find.unwrap();
    w.get_mut::<Label>(bar.query).unwrap().text = "alpha".into();
    rndr_ui::source_tools::apply_find_bars(w);
    assert_eq!(w.get::<Label>(bar.status).unwrap().text, "2 matches");
    assert_eq!(w.resource::<Focus>().unwrap().entity, Some(bar.query));
    load(w, "b.md", "beta", Some("b1"));
    assert!(w.has::<Hidden>(bar.root));
    click(w, Action::Open("a.md".into()));
    assert_eq!(
        w.resource::<Notes>().unwrap().views["a.md"].document,
        document
    );
    assert_eq!(
        w.resource::<Notes>().unwrap().views["a.md"]
            .find
            .unwrap()
            .root,
        bar.root
    );
    assert!(!w.has::<Hidden>(bar.root));
    click(w, Action::Read);
    assert!(w.has::<Hidden>(bar.root));
    click(w, Action::Find);
    assert!(!w.has::<Hidden>(bar.root));
    assert_eq!(w.get::<Label>(bar.query).unwrap().text, "alpha");
    click(w, Action::Find);
    assert!(w.has::<Hidden>(bar.root));
    assert!(drain_commands(w).is_empty());
}

#[test]
fn find_replace_uses_shared_commands_preserving_metadata_and_undo() {
    let mut app = fixture();
    let w = app.world_mut();
    let original = "\u{feff}---\r\ntitle: Exact\r\n---\r\n🦀 alpha alpha\n";
    load(w, "a.md", original, Some("a1"));
    click(w, Action::Find);
    let n = w.resource::<Notes>().unwrap();
    let bar = n.views["a.md"].find.unwrap();
    let document = n.views["a.md"].document;
    w.get_mut::<Label>(bar.query).unwrap().text = "alpha".into();
    w.get_mut::<Label>(bar.replacement).unwrap().text = "β".into();
    rndr_ui::source_tools::apply_find_bars(w);
    // Activate the real shared button rather than bypassing its dispatcher.
    let replace = w
        .entities_with::<Semantics>()
        .into_iter()
        .find(|e| w.get::<Semantics>(*e).and_then(|s| s.name.as_deref()) == Some("Replace all"))
        .unwrap();
    assert_eq!(try_activate(w, replace), ActivationResult::Accepted);
    rndr_ui::source_tools::apply_find_bars(w);
    tick(w);
    let n = w.resource::<Notes>().unwrap();
    assert_eq!(
        n.core.get("a.md").unwrap().content,
        original.replace("alpha", "β")
    );
    assert!(n.core.get("a.md").unwrap().dirty);
    assert_eq!(
        n.core.get("a.md").unwrap().content_hash.as_deref(),
        Some("a1")
    );
    w.resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(document)
        .unwrap()
        .undo()
        .unwrap();
    tick(w);
    assert_eq!(
        w.resource::<Notes>()
            .unwrap()
            .core
            .get("a.md")
            .unwrap()
            .content,
        original
    );
    assert!(drain_commands(w).is_empty());
}

#[test]
fn find_cannot_hide_active_rich_composition() {
    let mut app = fixture();
    let w = app.world_mut();
    load(w, "a.md", "alpha", Some("a1"));
    click(w, Action::Write);
    let id = w.resource::<Notes>().unwrap().views["a.md"].rich_document;
    w.resource_mut::<MarkdownDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap()
        .composition_start(0, 0)
        .unwrap();
    click(w, Action::Find);
    let n = w.resource::<Notes>().unwrap();
    assert!(n.live && !n.source);
    assert!(n.views["a.md"].find.is_none());
    assert!(drain_commands(w).is_empty());
}

fn summary(path: &str, title: &str, date: &str) -> VaultNoteSummary {
    serde_json::from_value(json!({"path":path,"title":title,"modified_at_utc":date})).unwrap()
}

#[test]
fn default_sidebar_lists_loaded_notes_by_date_without_opening_folders() {
    let mut app = fixture();
    let w = app.world_mut();
    let mut n = std::mem::take(w.resource_mut::<Notes>().unwrap());
    assert_eq!(n.browse, Browse::Recent);
    assert!(w.get::<Hidden>(n.search.unwrap()).is_some());
    n.list = vec![
        summary("a/archive.md", "Archived", "2021-03-04T00:00:00Z"),
        summary("b/design.md", "Design", "2022-04-02T00:00:00Z"),
    ];
    n.list[1].kind = "Project".into();
    let (nodes, _) = library_model(&mut n);
    assert!(nodes.iter().all(|node| node.parent.is_none()));
    assert_eq!(nodes[0].label, "Design");
    assert_eq!(nodes[0].secondary, "Apr 2 · Project");
    assert_eq!(nodes[1].secondary, "Mar 4 · Note");
}

#[test]
fn recent_browse_matches_visit_then_modified_contract_without_duplicate_paths() {
    let mut app = fixture();
    let w = app.world_mut();
    load(w, "folder/old.md", "draft", Some("v1"));
    edit(w, "folder/old.md", "retained draft");
    let source = w.resource::<Notes>().unwrap().views["folder/old.md"].document;
    let mut n = std::mem::take(w.resource_mut::<Notes>().unwrap());
    n.list = vec![
        summary("folder/old.md", "Same title", "2020-01-01T00:00:00Z"),
        summary("new.md", "Same title", "2026-01-01T00:00:00Z"),
        summary("middle.md", "Middle", "2025-01-01T00:00:00Z"),
        summary("new.md", "Duplicate page", "2026-01-01T00:00:00Z"),
    ];
    n.redraw = true;
    w.insert_resource(n);
    tick(w);
    click(w, Action::Browse(Browse::Recent));
    let mut n = std::mem::take(w.resource_mut::<Notes>().unwrap());
    let (nodes, expanded) = library_model(&mut n);
    assert!(expanded.is_empty());
    assert_eq!(
        nodes
            .iter()
            .map(|v| n.tree_paths[&v.id].as_str())
            .collect::<Vec<_>>(),
        vec!["folder/old.md", "new.md", "middle.md"]
    );
    assert!(nodes.iter().all(|v| v.parent.is_none()));
    assert_eq!(
        n.core.get("folder/old.md").unwrap().content,
        "retained draft"
    );
    assert_eq!(n.views["folder/old.md"].document, source);
    w.get_mut::<Label>(n.search.unwrap()).unwrap().text = "same TITLE".into();
    n.filter = "same TITLE".into();
    assert_eq!(library_model(&mut n).0.len(), 2);
    w.insert_resource(n);
    click(w, Action::Browse(Browse::Folders));
    let mut n = std::mem::take(w.resource_mut::<Notes>().unwrap());
    assert_eq!(n.filter, "same TITLE");
    assert_eq!(library_model(&mut n).0.len(), 3);
    w.insert_resource(n);
    assert!(drain_commands(w).is_empty());
}

#[test]
fn selected_then_activated_tree_event_queues_one_load_and_ignores_foreign_tree() {
    let mut app = fixture();
    let w = app.world_mut();
    let mut n = std::mem::take(w.resource_mut::<Notes>().unwrap());
    n.list = vec![
        summary("a.md", "Same", "2026-01-01T00:00:00Z"),
        summary("b.md", "Same", "2026-01-01T00:00:00Z"),
    ];
    n.redraw = true;
    w.insert_resource(n);
    tick(w);
    let n = w.resource::<Notes>().unwrap();
    let tree = n.tree.unwrap().root;
    let a = n.tree_ids["note:a.md"];
    let b = n.tree_ids["note:b.md"];
    assert_ne!(a, b);
    send(
        w,
        TreeEvent {
            tree,
            kind: TreeEventKind::Selected(Some(a)),
        },
    );
    send(
        w,
        TreeEvent {
            tree,
            kind: TreeEventKind::Activated(a),
        },
    );
    let foreign = w.spawn((Layout::row(),));
    send(
        w,
        TreeEvent {
            tree: foreign,
            kind: TreeEventKind::Activated(b),
        },
    );
    app.update();
    let w = app.world_mut();
    tick(w);
    let commands = drain_commands(w);
    assert_eq!(commands.len(), 1);
    assert!(matches!(&commands[0].operation, Operation::Load(p) if p == "a.md"));
    assert_eq!(
        w.resource::<Notes>().unwrap().selected.as_deref(),
        Some("a.md")
    );
}

#[test]
fn tabs_reorder_without_recreating_history_or_changing_selection() {
    let mut app = fixture();
    let w = app.world_mut();
    load(w, "z.md", "z", Some("z1"));
    load(w, "a.md", "a", Some("a1"));
    edit(w, "z.md", "draft z");
    let n = w.resource::<Notes>().unwrap();
    assert_eq!(n.tab_order, vec!["z.md", "a.md"]);
    let a = TabId(n.views["a.md"].tab);
    let z = TabId(n.views["z.md"].tab);
    let source = n.views["z.md"].document;
    let strip = n.tabstrip.unwrap();
    send(
        w,
        TabStripEvent {
            strip,
            kind: TabStripEventKind::Reordered(rndr_ui::tabs::TabMove {
                id: a,
                target: z,
                place: rndr_ui::drag::DropPlace::Before,
            }),
        },
    );
    app.update();
    let w = app.world_mut();
    tick(w);
    let n = w.resource::<Notes>().unwrap();
    assert_eq!(n.tab_order, vec!["a.md", "z.md"]);
    assert_eq!(n.selected.as_deref(), Some("a.md"));
    assert_eq!(n.views["z.md"].document, source);
    assert_eq!(n.core.get("z.md").unwrap().content, "draft z");
    assert!(drain_commands(w).is_empty());
}

#[test]
#[ignore = "Synthetic 12-case native render fixture; coordinator build lane and pixel comparison required"]
fn notes_modes_appearances_widths_render_fixture() {
    use std::sync::Arc;
    let mut app = crate::ui::app();
    app.update();
    crate::surfaces::select(app.world_mut(), crate::surfaces::SurfaceId::Notes);
    // Activation requires a root-identity reply before it admits note loads.
    // Exercise that real handshake rather than misrouting Loaded to Identity.
    let identity = drain_commands(app.world_mut()).pop().unwrap();
    assert!(matches!(
        identity.operation,
        Operation::Manage(management::Command::Identity)
    ));
    apply(
        app.world_mut(),
        NoteUpdate {
            command: identity,
            result: Ok(Response::Managed(
                json!({"activeRootId":"fixture-root","roots":[]}),
            )),
        },
    );
    let listing = drain_commands(app.world_mut());
    assert_eq!(listing.len(), 1);
    assert!(matches!(listing[0].operation, Operation::List { .. }));
    apply(
        app.world_mut(),
        NoteUpdate {
            command: listing.into_iter().next().unwrap(),
            result: Ok(Response::List(VaultNotesListResponse {
                notes: vec![summary(
                    "fixtures/synthetic.md",
                    "Synthetic parity fixture",
                    "2026-01-01T00:00:00Z",
                )],
                vault_generation: Some(1),
                next_cursor: None,
                truncated: false,
                reset_required: false,
            })),
        },
    );
    let engine: Arc<dyn TextEngine> = Arc::new(rndr_render::ParleyText::new());
    app.world_mut().insert_resource(engine);
    // Intentionally synthetic: no daemon IO, live note writes or real secrets.
    let source = "\u{feff}---\r\ntitle: Synthetic parity fixture\r\ntags: [fixture, review]\r\n---\r\n# Synthetic Notes\n\nUnicode: café 🦀 中文. **Bold**, *italic*, `code` and [link](https://example.com).\n\n## Checklist\n\n- [ ] Preserve draft\n- [x] Preserve source\n\n> A quote with a long line to exercise wrapping and narrow layouts.\n\n```rust\nfn fixture() { println!(\"synthetic\"); }\n```\n\n| Kind | State |\n| --- | --- |\n| Mixed blocks | Retained |\n\n### Ending\n\nFinal paragraph.\n";
    load(
        app.world_mut(),
        "fixtures/synthetic.md",
        source,
        Some("fixture-v1"),
    );
    app.world_mut().resource_mut::<Notes>().unwrap().list = vec![summary(
        "fixtures/synthetic.md",
        "Synthetic parity fixture",
        "2026-01-01T00:00:00Z",
    )];
    app.world_mut().resource_mut::<Notes>().unwrap().redraw = true;
    tick(app.world_mut());
    let mut exporter = rndr_render::Exporter::new().unwrap();
    std::fs::create_dir_all("verification").unwrap();
    for dark in [true, false] {
        for (width, height, size) in [(1440, 900, "wide"), (900, 850, "narrow")] {
            for mode in ["read", "source", "rich"] {
                publish_system_appearance(
                    app.world_mut(),
                    SystemAppearance {
                        dark: Some(dark),
                        ..Default::default()
                    },
                );
                click(app.world_mut(), Action::Read);
                match mode {
                    "source" => click(app.world_mut(), Action::ToggleSource),
                    "rich" => click(app.world_mut(), Action::Write),
                    _ => {}
                }
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
                    format!(
                        "verification/native-notes-fixture-{mode}-{}-{size}.png",
                        if dark { "dark" } else { "light" }
                    ),
                    &image.rgba,
                    image.width,
                    image.height,
                    image::ColorType::Rgba8,
                )
                .unwrap();
                assert_eq!(
                    app.world()
                        .resource::<Notes>()
                        .unwrap()
                        .core
                        .get("fixtures/synthetic.md")
                        .unwrap()
                        .content,
                    source
                );
            }
        }
    }
}

#[test]
fn find_shortcut_routes_only_owned_loaded_documents_and_rejects_composition() {
    let mut app = fixture();
    let w = app.world_mut();
    assert!(!shortcut(w, "find"));
    load(w, "a.md", "alpha", Some("a1"));
    assert!(!shortcut(w, "unknown"));
    assert!(shortcut(w, "find"));
    assert!(w.resource::<Notes>().unwrap().views["a.md"].find_open);
    click(w, Action::Write);
    let id = w.resource::<Notes>().unwrap().views["a.md"].rich_document;
    w.resource_mut::<MarkdownDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap()
        .composition_start(0, 0)
        .unwrap();
    assert!(!shortcut(w, "find"));
    assert!(w.resource::<Notes>().unwrap().live);
    assert!(drain_commands(w).is_empty());
}

#[test]
fn group_popup_keeps_library_in_place_and_search_preserves_scope() {
    let mut app = fixture();
    app.update();
    let w = app.world_mut();
    let n = w.resource::<Notes>().unwrap();
    let tree = n.tree.unwrap().root;
    let trigger = n
        .actions
        .values()
        .find(|a| matches!(a, Action::Picker(Picker::Group, _)))
        .unwrap()
        .clone();
    click(w, trigger);
    let n = w.resource::<Notes>().unwrap();
    assert_eq!(n.tree.unwrap().root, tree);
    let (menu, items) = n.menu.as_ref().unwrap();
    let id = *items
        .iter()
        .find(|(_, a)| matches!(a, Action::Group(Some("projects/"))))
        .unwrap()
        .0;
    let picked = rndr_ui::menu::MenuPicked {
        menu: menu.root,
        item: id,
    };
    send(w, picked);
    app.update();
    let w = app.world_mut();
    tick(w);
    assert_eq!(w.resource::<Notes>().unwrap().group, Some("projects/"));
    click(w, Action::SearchList);
    let n = w.resource::<Notes>().unwrap();
    assert_eq!(w.resource::<Focus>().unwrap().entity, n.search);
    assert!(w.has::<Hidden>(n.header.unwrap()));
    click(w, Action::SearchList);
    assert_eq!(w.resource::<Notes>().unwrap().group, Some("projects/"));
    assert!(drain_commands(w).is_empty());
}

fn names(w: &World, parent: Entity) -> Vec<String> {
    children_of(w, parent)
        .into_iter()
        .filter_map(|e| w.get::<Semantics>(e).and_then(|s| s.name.clone()))
        .collect()
}

#[test]
fn note_header_is_one_quiet_row_and_the_rest_lives_behind_more() {
    let mut app = fixture();
    let w = app.world_mut();
    load(w, "a.md", "# A\n\nbody", Some("v1"));
    tick(w);
    let toolbar = w.resource::<Notes>().unwrap().toolbar.unwrap();
    // A clean note: the path, find and "…", no Save and no mode buttons.
    assert_eq!(names(w, toolbar), ["Find / Replace", "More note actions"]);
    edit(w, "a.md", "# A\n\nbody, edited");
    assert!(names(w, toolbar).contains(&"Save".to_string()));
    // Writing shows four format glyphs, not seven text buttons.
    click(w, Action::Write);
    tick(w);
    let formatting = w.resource::<Notes>().unwrap().formatting.unwrap();
    assert_eq!(names(w, formatting), ["Bold", "Italic", "Heading", "List"]);
    // "…" opens a menu whose picks route through the same actions.
    let more = children_of(w, toolbar)
        .into_iter()
        .find(|e| {
            w.get::<Semantics>(*e).and_then(|s| s.name.as_deref()) == Some("More note actions")
        })
        .unwrap();
    let id = w
        .resource::<Notes>()
        .unwrap()
        .actions
        .iter()
        .find(|(_, a)| matches!(a, Action::Overflow(anchor) if *anchor == more))
        .map(|(id, _)| *id)
        .expect("the … button routes to the overflow menu");
    assert!(action(w, &HostIntent::Command(id)));
    let (menu, actions) = w.resource::<Notes>().unwrap().menu.clone().unwrap();
    let source = actions
        .iter()
        .find(|(_, a)| matches!(a, Action::ToggleSource))
        .map(|(id, _)| *id)
        .unwrap();
    assert!(
        actions
            .values()
            .any(|a| matches!(a, Action::Format(MarkdownCommand::Quote)))
    );
    let before = w.resource::<Notes>().unwrap().source;
    send(
        w,
        rndr_ui::menu::MenuPicked {
            menu: menu.root,
            item: source,
        },
    );
    app.update();
    tick(app.world_mut());
    assert_ne!(app.world().resource::<Notes>().unwrap().source, before);
}
#[test]
fn closing_a_clean_note_releases_its_source_and_rich_documents() {
    let mut app = fixture();
    let w = app.world_mut();
    load(
        w,
        "a.md",
        &"# Long note\n\nBody text.\n".repeat(500),
        Some("v1"),
    );
    let (document, rich) = {
        let v = &w.resource::<Notes>().unwrap().views["a.md"];
        (v.document, v.rich_document)
    };
    assert!(
        w.resource::<SourceDocuments>()
            .unwrap()
            .get(document)
            .is_some()
    );
    click(w, Action::Manage(management::Action::CloseNote));
    assert!(!w.resource::<Notes>().unwrap().views.contains_key("a.md"));
    assert!(
        w.resource::<SourceDocuments>()
            .unwrap()
            .get(document)
            .is_none()
    );
    assert!(
        w.resource::<MarkdownDocuments>()
            .unwrap()
            .get(rich)
            .is_none()
    );
}

#[test]
fn navigation_context_tracks_emitting_preview_and_preserves_parked_dirty_note() {
    let mut app = fixture();
    let w = app.world_mut();
    load(w, "folder/a.md", "one", Some("v1"));
    let preview = w.resource::<Notes>().unwrap().views["folder/a.md"].preview;
    edit(w, "folder/a.md", "unsaved draft");
    load(w, "other.md", "two", Some("v2"));
    assert_eq!(link_context(w, preview).as_deref(), Some("folder/a.md"));
    assert!(
        w.resource::<Notes>()
            .unwrap()
            .core
            .get("folder/a.md")
            .unwrap()
            .dirty
    );
    assert_eq!(
        w.resource::<Notes>()
            .unwrap()
            .core
            .get("folder/a.md")
            .unwrap()
            .content,
        "unsaved draft"
    );
    assert!(!can_rebind(w));
}

#[test]
fn navigation_waits_for_confirmed_identity_and_failure_drops_deferred_open() {
    let mut app = fixture();
    let w = app.world_mut();
    activate(w);
    w.resource_mut::<Notes>().unwrap().pending_navigation = Some("target.md".into());
    let command = drain_commands(w)
        .into_iter()
        .find(|c| {
            matches!(
                c.operation,
                Operation::Manage(management::Command::Identity)
            )
        })
        .unwrap();
    apply(
        w,
        NoteUpdate {
            command,
            result: Err("offline".into()),
        },
    );
    let n = w.resource::<Notes>().unwrap();
    assert!(n.selected.is_none());
    assert!(n.pending_navigation.is_none());
    assert!(!n.initializing);
    assert!(drain_commands(w).is_empty());
}

#[test]
fn navigation_rich_editor_link_event_opens_relative_note_without_losing_draft() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = crate::ui::app();
    let _channels = crate::workshops::install_fixture(
        app.world_mut(),
        dir.path().join("sources.json"),
        &[("authority-a", "A")],
    );
    app.world_mut().insert_resource(Viewport::new(1000., 800.));
    app.update();
    assert!(crate::workshops::navigate_binding(
        app.world_mut(),
        crate::surfaces::SurfaceId::Notes,
        "authority-a"
    ));
    crate::surfaces::select(app.world_mut(), crate::surfaces::SurfaceId::Notes);
    app.update(); // Publish the newly mounted Notes source binding before input.
    complete_identity_fixture(app.world_mut());
    let w = app.world_mut();
    assert_eq!(
        crate::workshops::binding(w, crate::surfaces::SurfaceId::Notes)
            .unwrap()
            .authority,
        "authority-a"
    );
    load(w, "folder/current.md", "[Next](Next.md)", Some("v1"));
    edit(w, "folder/current.md", "[Next](Next.md) unsaved");
    let rich = w.resource::<Notes>().unwrap().views["folder/current.md"]
        .rich
        .root;
    assert_eq!(link_context(w, rich).as_deref(), Some("folder/current.md"));
    send(
        w,
        LinkActivated {
            view: rich,
            href: "Next.md".into(),
        },
    );
    app.update();
    let n = app.world().resource::<Notes>().unwrap();
    assert_eq!(n.selected.as_deref(), Some("folder/Next.md"));
    let draft = n.core.get("folder/current.md").unwrap();
    assert!(draft.dirty);
    assert_eq!(draft.content, "[Next](Next.md) unsaved");
}

#[test]
fn navigation_heading_scrolls_to_offscreen_duplicate_in_preview() {
    let mut app = fixture();
    let body = format!(
        "# Topic\n\n{}\n\n# Topic\n\nEnd",
        (0..100)
            .map(|i| format!("Paragraph {i}.\n\n"))
            .collect::<String>()
    );
    load(app.world_mut(), "long.md", &body, Some("v1"));
    let preview = {
        let n = app.world_mut().resource_mut::<Notes>().unwrap();
        n.live = false;
        n.source = false;
        n.pending_heading = Some(("long.md".into(), "topic-1".into()));
        n.redraw = true;
        n.views["long.md"].preview
    };
    for _ in 0..4 {
        tick(app.world_mut());
        app.update();
    }
    assert!(
        app.world()
            .resource::<Notes>()
            .unwrap()
            .pending_heading
            .is_none()
    );
    assert!(app.world().get::<Scroll>(preview).unwrap().offset[1] < -100.);
    assert_eq!(
        app.world()
            .resource::<Notes>()
            .unwrap()
            .core
            .get("long.md")
            .unwrap()
            .content,
        body
    );
}

#[test]
fn workspace_reparents_complete_note_without_losing_draft_or_document() {
    let mut app = fixture();
    let w = app.world_mut();
    load(w, "a.md", "saved", Some("v1"));
    edit(w, "a.md", "unsaved draft");
    load(w, "b.md", "other", Some("v2"));
    let items = workspace_items(w);
    let item = items.iter().find(|i| i.path == "a.md").unwrap();
    assert!(item.dirty);
    let entity = item.entity;
    let view = &w.resource::<Notes>().unwrap().views["a.md"];
    let document = view.document;
    let source = view.handle.root;
    let rich = view.rich.root;
    let preview = view.preview;
    assert!(children_of(w, entity).contains(&source));
    assert!(children_of(w, entity).contains(&rich));
    assert!(children_of(w, entity).contains(&preview));
    let home = w.resource::<Notes>().unwrap().editors.unwrap();
    let workspace = w.spawn((Layout::column(),));
    set_parent(w, entity, workspace);
    workspace_show(w, &["a.md".into(), "b.md".into()]);
    tick(w);
    assert!(!w.has::<Hidden>(entity));
    assert!(workspace_focus(w, "a.md"));
    assert!(!workspace_focus(w, "missing.md"));
    assert_eq!(
        w.resource::<Notes>().unwrap().selected.as_deref(),
        Some("a.md")
    );
    workspace_show(w, &[]);
    set_parent(w, entity, home);
    tick(w);
    let n = w.resource::<Notes>().unwrap();
    assert_eq!(n.views["a.md"].root, entity);
    assert_eq!(n.views["a.md"].document, document);
    assert_eq!(n.core.get("a.md").unwrap().content, "unsaved draft");
    assert!(n.core.get("a.md").unwrap().dirty);
}
