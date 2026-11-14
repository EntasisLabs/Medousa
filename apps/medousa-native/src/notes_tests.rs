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
