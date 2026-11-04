//! Vault note buffer dirty / conflict reconcile tests.

use medousa_app_core::vault_buffer::{
    NoteBufferFact, VaultBufferCommand, VaultBufferCore, VaultBufferUpdate,
};

fn seed(path: &str, content: &str) -> NoteBufferFact {
    NoteBufferFact {
        path: path.into(),
        content: content.into(),
        baseline_content: content.into(),
        content_hash: Some("hash-1".into()),
        title: path.into(),
        dirty: false,
        content_revision: 1,
        conflict: false,
    }
}

#[test]
fn apply_edit_marks_dirty_and_mark_saved_clears() {
    let mut core = VaultBufferCore::new("personal");
    core.dispatch(VaultBufferCommand::SeedBuffer {
        buffer: seed("notes/a.md", "hello"),
    })
    .unwrap();

    let VaultBufferUpdate::Snapshot { snapshot } = core
        .dispatch(VaultBufferCommand::ApplyEdit {
            path: "notes/a.md".into(),
            content: "hello world".into(),
            title: None,
        })
        .unwrap();
    let buffer = &snapshot.buffers[0];
    assert!(buffer.dirty);
    assert!(!buffer.conflict);
    assert_eq!(buffer.content, "hello world");
    assert_eq!(buffer.baseline_content, "hello");

    let VaultBufferUpdate::Snapshot { snapshot } = core
        .dispatch(VaultBufferCommand::MarkSaved {
            path: "notes/a.md".into(),
            content: "hello world".into(),
            content_hash: Some("hash-2".into()),
            title: None,
        })
        .unwrap();
    let buffer = &snapshot.buffers[0];
    assert!(!buffer.dirty);
    assert_eq!(buffer.baseline_content, "hello world");
    assert_eq!(buffer.content_hash.as_deref(), Some("hash-2"));
}

#[test]
fn reconcile_clean_adopts_disk_dirty_conflicts() {
    let mut core = VaultBufferCore::new("personal");
    core.dispatch(VaultBufferCommand::SeedBuffer {
        buffer: seed("notes/a.md", "hello"),
    })
    .unwrap();

    core.dispatch(VaultBufferCommand::ReconcileFromDisk {
        path: "notes/a.md".into(),
        content: "from disk".into(),
        content_hash: Some("hash-disk".into()),
        title: Some("A".into()),
    })
    .unwrap();
    assert_eq!(core.get("notes/a.md").unwrap().content, "from disk");
    assert!(!core.get("notes/a.md").unwrap().dirty);

    core.dispatch(VaultBufferCommand::ApplyEdit {
        path: "notes/a.md".into(),
        content: "local edit".into(),
        title: None,
    })
    .unwrap();

    let VaultBufferUpdate::Snapshot { snapshot } = core
        .dispatch(VaultBufferCommand::ReconcileFromDisk {
            path: "notes/a.md".into(),
            content: "agent write".into(),
            content_hash: Some("hash-agent".into()),
            title: None,
        })
        .unwrap();
    let buffer = &snapshot.buffers[0];
    assert_eq!(buffer.content, "local edit");
    assert!(buffer.dirty);
    assert!(buffer.conflict);
    assert_eq!(buffer.content_hash.as_deref(), Some("hash-agent"));
}

#[test]
fn dispose_and_reset_clear_buffers() {
    let mut core = VaultBufferCore::new("personal");
    core.dispatch(VaultBufferCommand::SeedBuffer {
        buffer: seed("notes/a.md", "hello"),
    })
    .unwrap();
    core.dispatch(VaultBufferCommand::DisposeBuffer {
        path: "notes/a.md".into(),
    })
    .unwrap();
    assert!(core.get("notes/a.md").is_none());

    core.dispatch(VaultBufferCommand::SeedBuffer {
        buffer: seed("notes/b.md", "x"),
    })
    .unwrap();
    core.dispatch(VaultBufferCommand::Reset {
        workshop_scope_id: "workshop-b".into(),
    })
    .unwrap();
    assert_eq!(core.workshop_scope_id(), "workshop-b");
    assert!(core.get("notes/b.md").is_none());
}
