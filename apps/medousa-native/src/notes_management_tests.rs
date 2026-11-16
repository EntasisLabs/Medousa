use super::super::management::{Action as ManageAction, Command as ManageCommand};
use super::*;
fn roots(w: &mut World) {
    w.resource_mut::<Notes>().unwrap().core = VaultBufferCore::new("vault-a");
}
fn form(w: &mut World, value: &str) {
    let e = w.resource::<Notes>().unwrap().management.input.unwrap();
    w.get_mut::<Label>(e).unwrap().text = value.into();
}
fn manage(w: &mut World, a: ManageAction) {
    click(w, Action::Manage(a));
}
fn moved(w: &mut World, c: NoteCommand, to: &str) {
    let r = response(to, "saved", Some("v2"));
    apply(
        w,
        NoteUpdate {
            command: c,
            result: Ok(Response::Managed(
                serde_json::to_value(VaultWriteResponse {
                    note: r.note,
                    content: Some(r.content),
                    created: false,
                    note_version: r.note_version,
                    vault_generation: Some(2),
                })
                .unwrap(),
            )),
        },
    );
}
#[test]
fn invalid_create_path_never_reaches_daemon() {
    let mut app = fixture();
    let w = app.world_mut();
    manage(w, ManageAction::New);
    form(w, "../escape.md");
    manage(w, ManageAction::Create);
    assert!(drain_commands(w).is_empty());
}
#[test]
fn search_form_survives_reconciliation() {
    let mut app = fixture();
    let w = app.world_mut();
    manage(w, ManageAction::Search);
    form(w, "retained query");
    w.resource_mut::<Notes>().unwrap().redraw = true;
    tick(w);
    manage(w, ManageAction::Search);
    assert!(
        matches!(drain_commands(w).pop().unwrap().operation,Operation::Manage(ManageCommand::Search(q)) if q=="retained query")
    );
}
#[test]
fn root_switch_never_rebinds_open_document() {
    let mut app = fixture();
    let w = app.world_mut();
    roots(w);
    load(w, "a.md", "saved", Some("v1"));
    manage(w, ManageAction::Switch("vault-b".into()));
    assert!(drain_commands(w).is_empty());
    assert_eq!(
        w.resource::<Notes>().unwrap().core.workshop_scope_id(),
        "vault-a"
    );
}
#[test]
fn move_preserves_late_draft_and_excludes_concurrent_save() {
    let mut app = fixture();
    let w = app.world_mut();
    roots(w);
    load(w, "a.md", "saved", Some("v1"));
    manage(w, ManageAction::Move);
    form(w, "folder/b.md");
    manage(w, ManageAction::SubmitMove("a.md".into()));
    let c = drain_commands(w).pop().unwrap();
    assert!(
        matches!(&c.operation,Operation::Manage(ManageCommand::Move{root,from,to}) if root=="vault-a" && from=="a.md" && to=="folder/b.md")
    );
    w.resource_mut::<Notes>()
        .unwrap()
        .core
        .dispatch(VaultBufferCommand::ApplyEdit {
            path: "a.md".into(),
            content: "late draft".into(),
            title: None,
        })
        .unwrap();
    w.resource_mut::<Notes>().unwrap().save();
    assert!(drain_commands(w).is_empty());
    moved(w, c, "folder/b.md");
    let n = w.resource::<Notes>().unwrap();
    assert_eq!(n.core.get("a.md").unwrap().content, "late draft");
    assert!(n.uncertain.contains("a.md"));
}
#[test]
fn clean_move_retires_old_document_identity() {
    let mut app = fixture();
    let w = app.world_mut();
    roots(w);
    load(w, "a.md", "saved", Some("v1"));
    manage(w, ManageAction::Move);
    form(w, "b.md");
    manage(w, ManageAction::SubmitMove("a.md".into()));
    let c = drain_commands(w).pop().unwrap();
    moved(w, c, "b.md");
    let n = w.resource::<Notes>().unwrap();
    assert!(n.core.get("a.md").is_none());
    assert!(!n.views.contains_key("a.md"));
    assert_eq!(n.selected.as_deref(), Some("b.md"));
}
#[test]
fn failed_delete_retains_document_without_retry() {
    let mut app = fixture();
    let w = app.world_mut();
    load(w, "a.md", "saved", Some("v1"));
    manage(w, ManageAction::ConfirmDelete("a.md".into()));
    let c = drain_commands(w).pop().unwrap();
    apply(
        w,
        NoteUpdate {
            command: c,
            result: Err("connection lost".into()),
        },
    );
    assert!(w.resource::<Notes>().unwrap().views.contains_key("a.md"));
    assert!(drain_commands(w).is_empty());
}

#[test]
fn root_switch_clears_recent_and_tree_identity_and_rejects_old_tree_events() {
    let mut app = fixture();
    let w = app.world_mut();
    roots(w);
    w.resource_mut::<Notes>().unwrap().list =
        vec![summary("same.md", "Duplicate", "2026-01-01T00:00:00Z")];
    w.resource_mut::<Notes>().unwrap().recent = vec!["same.md".into()];
    w.resource_mut::<Notes>().unwrap().redraw = true;
    tick(w);
    let n = w.resource::<Notes>().unwrap();
    let old_tree = n.tree.unwrap().root;
    let old_id = n.tree_ids["note:same.md"];
    manage(w, ManageAction::Switch("vault-b".into()));
    let c = drain_commands(w).pop().unwrap();
    apply(
        w,
        NoteUpdate {
            command: c,
            result: Ok(Response::Managed(
                json!({"active_root_id":"vault-b","roots":[]}),
            )),
        },
    );
    let page = drain_commands(w).pop().unwrap();
    apply(
        w,
        NoteUpdate {
            command: page,
            result: Ok(Response::List(VaultNotesListResponse {
                notes: vec![summary("same.md", "Duplicate", "2026-01-01T00:00:00Z")],
                vault_generation: Some(1),
                next_cursor: None,
                truncated: false,
                reset_required: false,
            })),
        },
    );
    let n = w.resource::<Notes>().unwrap();
    assert_eq!(n.core.workshop_scope_id(), "vault-b");
    assert!(n.recent.is_empty());
    assert_ne!(n.tree_ids["note:same.md"], old_id);
    send(
        w,
        TreeEvent {
            tree: old_tree,
            kind: TreeEventKind::Activated(old_id),
        },
    );
    app.update();
    let w = app.world_mut();
    tick(w);
    assert!(drain_commands(w).is_empty());
    assert!(w.resource::<Notes>().unwrap().selected.is_none());
}
