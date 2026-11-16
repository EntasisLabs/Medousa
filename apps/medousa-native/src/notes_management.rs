//! Native vault operations. Forms live outside the rebuilt note list so typing
//! never destroys a prompt. Mutations are explicit, single-flight, and not retried.
use super::*;
use medousa_sdk::operations as ops;
use medousa_types::{
    VaultAddRootRequest, VaultRootsResponse, VaultSearchResponse, VaultSetActiveRootRequest,
    VaultTrashListResponse, VaultWriteRequest,
};
use serde_json::{Value, json};

#[derive(Clone, Debug)]
pub(super) enum Command {
    Identity,
    Move {
        root: String,
        from: String,
        to: String,
    },
    Create(String),
    Delete(String),
    Search(String),
    Trash,
    Restore(String),
    Roots,
    Switch(String),
    AddRoot {
        label: String,
        path: String,
    },
}
#[derive(Clone)]
pub(super) enum Action {
    Move,
    SubmitMove(String),
    New,
    Create,
    Delete,
    ConfirmDelete(String),
    Search,
    Trash,
    Restore(String),
    Roots,
    Switch(String),
    AddRoot,
    SubmitRoot,
    Close,
    CloseNote,
}
#[derive(Default)]
pub(super) struct Management {
    panel: Option<Entity>,
    pub(super) input: Option<Entity>,
    search_form: bool,
    second: Option<Entity>,
    pub pending: Option<u64>,
    pub actions: HashMap<u64, super::Action>,
}
impl Management {
    pub fn new(panel: Entity) -> Self {
        Self {
            panel: Some(panel),
            ..Default::default()
        }
    }
}
fn control(w: &mut World, p: Entity, n: &mut Notes, label: &str, a: Action, enabled: bool) {
    let id = next();
    visual::document_action(w, p, id, label, enabled);
    n.management.actions.insert(id, super::Action::Manage(a));
}
fn prompt(w: &mut World, p: Entity, name: &str) -> Entity {
    let e = w.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(34.),
            ..Layout::row()
        },
        Label::new(""),
        Focusable,
        Prompt {
            submit_on_enter: false,
            ..Prompt::new()
        },
        Placeholder::new(name),
        Semantics {
            name: Some(name.into()),
            ..Default::default()
        },
    ));
    set_parent(w, e, p);
    visual::plane(w, e, visual::rail());
    e
}
fn panel(w: &mut World, n: &mut Notes, title: &str) -> Option<Entity> {
    let p = n.management.panel?;
    // Release focus before removing a focused form control.
    if let Some(f) = w.resource_mut::<Focus>() {
        f.entity = None;
    }
    for e in children_of(w, p) {
        despawn(w, e);
    }
    w.remove::<Hidden>(p);
    n.management.actions.clear();
    n.management.search_form = false;
    n.management.input = None;
    n.management.second = None;
    caption(w, p, title);
    control(
        w,
        p,
        n,
        "Close",
        Action::Close,
        n.management.pending.is_none(),
    );
    Some(p)
}
fn message(w: &mut World, n: &mut Notes, text: &str) {
    let _ = panel(w, n, text);
}
fn text_input(w: &World, e: Option<Entity>) -> String {
    e.and_then(|e| w.get::<Label>(e))
        .map(|l| l.text.trim().to_owned())
        .unwrap_or_default()
}
pub(super) fn initialize(n: &mut Notes) {
    enqueue(n, Command::Identity);
}
fn enqueue(n: &mut Notes, command: Command) {
    if n.management.pending.is_none() {
        n.management.pending = Some(n.enqueue(Operation::Manage(command)));
        n.redraw = true;
    }
}
fn idle(n: &Notes) -> bool {
    n.management.pending.is_none()
        && n.loads.is_empty()
        && n.saves.is_empty()
        && n.list_pending.is_none()
}
fn clean(n: &Notes, path: &str) -> bool {
    n.core.get(path).is_some_and(|b| !b.dirty && !b.conflict)
        && !n.uncertain.contains(path)
        && !n.remote.contains_key(path)
}
pub(super) fn navigation(w: &mut World, p: Entity, n: &mut Notes) {
    for (label, a) in [
        ("New note", Action::New),
        ("Search vault", Action::Search),
        ("Trash", Action::Trash),
        ("Vaults", Action::Roots),
    ] {
        super::button(w, p, n, label, super::Action::Manage(a), idle(n));
    }
    if let Some(path) = n.selected.clone() {
        super::button(
            w,
            p,
            n,
            "Rename / move…",
            super::Action::Manage(Action::Move),
            idle(n) && clean(n, &path),
        );
        super::button(
            w,
            p,
            n,
            "Move to trash…",
            super::Action::Manage(Action::Delete),
            idle(n) && clean(n, &path),
        );
        super::button(
            w,
            p,
            n,
            "Close note",
            super::Action::Manage(Action::CloseNote),
            idle(n) && clean(n, &path),
        );
    }
}
fn forget(w: &mut World, n: &mut Notes, path: &str) {
    n.tab_order.retain(|p| p != path);
    if let Some(v) = n.views.remove(path) {
        despawn(w, v.handle.root);
        despawn(w, v.preview);
        despawn(w, v.rich.root);
        if let Some(bar) = v.find {
            despawn(w, bar.root);
        }
    }
    let _ = n
        .core
        .dispatch(VaultBufferCommand::DisposeBuffer { path: path.into() });
    n.errors.remove(path);
    n.remote.remove(path);
    n.uncertain.remove(path);
    if n.selected.as_deref() == Some(path) {
        n.selected = n.tab_order.first().cloned();
    }
}
pub(super) fn action(w: &mut World, n: &mut Notes, a: Action) {
    if !can_deactivate_views(w, n) || !idle(n) {
        return;
    }
    match a {
        Action::Move => {
            if let Some(path) = n.selected.clone().filter(|p| clean(n, p)) {
                if let Some(p) = panel(w, n, &format!("Rename or move {path} within this vault")) {
                    let input = prompt(w, p, "New folder/name.md");
                    n.management.input = Some(input);
                    control(w, p, n, "Move note", Action::SubmitMove(path), true);
                }
            }
        }
        Action::SubmitMove(from) => {
            let to = text_input(w, n.management.input);
            if n.selected.as_ref() != Some(&from)
                || !clean(n, &from)
                || !valid_path(&to)
                || from == to
            {
                return;
            }
            let root = n.core.workshop_scope_id().to_owned();
            if root == "connected-workshop" {
                message(
                    w,
                    n,
                    "Vault identity unavailable. Reopen Notes before moving.",
                );
                return;
            }
            enqueue(n, Command::Move { root, from, to });
        }
        Action::Close => {
            if let Some(p) = n.management.panel {
                w.insert(p, Hidden);
            }
            n.management.actions.clear();
        }
        Action::New => {
            if let Some(p) = panel(w, n, "New Markdown note · path on workshop") {
                n.management.input = Some(prompt(w, p, "Folder/name.md"));
                control(w, p, n, "Create note", Action::Create, true);
            }
        }
        Action::Create => {
            let path = text_input(w, n.management.input);
            if !valid_path(&path) {
                message(
                    w,
                    n,
                    "Use a relative .md path without empty, . or .. segments.",
                );
                return;
            }
            enqueue(n, Command::Create(path));
        }
        Action::Delete => {
            if let Some(path) = n.selected.clone().filter(|p| clean(n, p)) {
                if let Some(p) = panel(w, n, &format!("Move {path} to workshop trash?")) {
                    control(
                        w,
                        p,
                        n,
                        "Confirm move to trash",
                        Action::ConfirmDelete(path),
                        true,
                    );
                }
            }
        }
        Action::ConfirmDelete(path) => {
            if n.selected.as_ref() == Some(&path) && clean(n, &path) {
                enqueue(n, Command::Delete(path));
            }
        }
        Action::CloseNote => {
            if let Some(path) = n.selected.clone().filter(|p| clean(n, p)) {
                forget(w, n, &path);
            }
        }
        Action::Search => {
            // Search is explicit, never triggered by every keystroke.
            let query = text_input(w, n.management.input);
            let is_search = n.management.search_form;
            if is_search && !query.is_empty() {
                enqueue(n, Command::Search(query));
            } else if let Some(p) = panel(w, n, "Search the workshop vault") {
                n.management.search_form = true;
                n.management.input = Some(prompt(w, p, "Search note contents and titles"));
                control(w, p, n, "Search", Action::Search, true);
            }
        }
        Action::Trash => enqueue(n, Command::Trash),
        Action::Restore(path) => enqueue(n, Command::Restore(path)),
        Action::Roots => enqueue(n, Command::Roots),
        Action::Switch(id) => {
            // Root APIs address the active daemon root. Never rebind an open
            // document's relative path to another vault, even if it is clean.
            if n.views.is_empty() {
                enqueue(n, Command::Switch(id));
            } else {
                message(
                    w,
                    n,
                    "Save and close all notes before switching vaults. Drafts are never rebound to another vault.",
                );
            }
        }
        Action::AddRoot => {
            if let Some(p) = panel(w, n, "Link an existing directory on the workshop daemon") {
                n.management.input = Some(prompt(w, p, "Vault label"));
                n.management.second = Some(prompt(w, p, "Absolute directory path on workshop"));
                control(w, p, n, "Link vault", Action::SubmitRoot, true);
            }
        }
        Action::SubmitRoot => {
            let label = text_input(w, n.management.input);
            let path = text_input(w, n.management.second);
            if label.is_empty() || path.is_empty() {
                return;
            }
            enqueue(n, Command::AddRoot { label, path });
        }
    }
    n.redraw = true;
}
fn valid_path(path: &str) -> bool {
    path.ends_with(".md")
        && !path.contains(['\\', '\0', ':'])
        && path
            .split('/')
            .all(|s| !s.is_empty() && s != "." && s != "..")
}
pub(super) async fn execute(client: &MedousaClient, c: &Command) -> Result<Value, String> {
    async fn run(client: &MedousaClient, c: &Command) -> Result<Value, medousa_sdk::SdkError> {
        let http = client.http();
        match c {
            Command::Create(path) => client
                .vault()
                .create_note(&VaultWriteRequest {
                    path: Some(path.clone()),
                    content: format!(
                        "# {}\n",
                        path.rsplit('/')
                            .next()
                            .unwrap_or(path)
                            .trim_end_matches(".md")
                    ),
                    auto_workshop_tags: false,
                    ..Default::default()
                })
                .await
                .and_then(|v| {
                    serde_json::to_value(v).map_err(|e| medousa_sdk::SdkError::Serde(e.to_string()))
                }),
            Command::Delete(path) => client.vault().delete_note(path).await.and_then(|v| {
                serde_json::to_value(v).map_err(|e| medousa_sdk::SdkError::Serde(e.to_string()))
            }),
            Command::Search(q) => {
                http.get_query(
                    ops::VAULT_SEARCH_GET.path,
                    &[("q", q.clone()), ("limit", "100".into())],
                )
                .await
            }
            Command::Trash => {
                http.get_query(ops::VAULT_TRASH_GET.path, &[("limit", "100".into())])
                    .await
            }
            Command::Restore(path) => {
                http.post(ops::VAULT_TRASH_RESTORE_POST.path, &json!({"path":path}))
                    .await
            }
            Command::Identity | Command::Roots => http.get(ops::VAULT_ROOTS_GET.path).await,
            Command::Move { root, from, to } => client
                .vault()
                .move_note(&medousa_types::VaultMoveRequest {
                    root_id: root.clone(),
                    from_path: from.clone(),
                    to_path: to.clone(),
                })
                .await
                .and_then(|v| {
                    serde_json::to_value(v).map_err(|e| medousa_sdk::SdkError::Serde(e.to_string()))
                }),
            Command::Switch(id) => client
                .vault()
                .set_active_root(&VaultSetActiveRootRequest {
                    root_id: id.clone(),
                })
                .await
                .and_then(|v| {
                    serde_json::to_value(v).map_err(|e| medousa_sdk::SdkError::Serde(e.to_string()))
                }),
            Command::AddRoot { label, path } => client
                .vault()
                .add_root(&VaultAddRootRequest {
                    label: label.clone(),
                    path: path.clone(),
                    id: None,
                })
                .await
                .and_then(|v| {
                    serde_json::to_value(v).map_err(|e| medousa_sdk::SdkError::Serde(e.to_string()))
                }),
        }
    }
    run(client, c).await.map_err(|e| e.to_string())
}
pub(super) fn apply(
    w: &mut World,
    n: &mut Notes,
    serial: u64,
    c: &Command,
    result: Result<Response, String>,
) {
    if n.management.pending != Some(serial) {
        return;
    }
    n.management.pending = None;
    let value = match result {
        Ok(Response::Managed(v)) => v,
        Err(e) => {
            message(
                w,
                n,
                &format!(
                    "Operation not confirmed: {e}. No automatic retry. Refresh to check workshop state."
                ),
            );
            return;
        }
        _ => {
            message(w, n, "Unexpected vault response; no automatic retry.");
            return;
        }
    };
    match c {
        Command::Identity => match serde_json::from_value::<VaultRootsResponse>(value) {
            Ok(r) if n.views.is_empty() => {
                n.core = VaultBufferCore::new(r.active_root_id);
                n.list_page(false);
            }
            _ => message(
                w,
                n,
                "Vault identity could not be confirmed. Reopen Notes to retry.",
            ),
        },
        Command::Move { from, to, .. } => match serde_json::from_value::<VaultWriteResponse>(value)
        {
            Ok(r) if r.note.path == *to && !r.created => {
                if clean(n, from) {
                    forget(w, n, from);
                } else {
                    n.uncertain.insert(from.clone());
                    n.errors.insert(from.clone(), format!("Note moved to {to}. This newer local draft is retained at the old path; copy it before closing."));
                }
                message(
                    w,
                    n,
                    "Note moved on the workshop. Links and vault identity preserved.",
                );
                n.source = true;
                n.live = false;
                open_note(w, n, to.clone());
                n.list_page(false);
            }
            _ => message(
                w,
                n,
                "Move not confirmed. Refresh vault contents before another attempt.",
            ),
        },
        Command::Create(_) => match serde_json::from_value::<VaultWriteResponse>(value) {
            Ok(r) if r.created => {
                message(w, n, "Note created on workshop.");
                n.source = true;
                n.live = false;
                open_note(w, n, r.note.path);
                n.list_page(false);
            }
            _ => message(
                w,
                n,
                "Create response did not confirm creation. Refresh before retrying.",
            ),
        },
        Command::Delete(path) => {
            if value["deleted"].as_bool() == Some(true) && value["path"].as_str() == Some(path) {
                // Keep any draft typed after confirmation in its retained view.
                if clean(n, path) {
                    forget(w, n, path);
                }
                message(w, n, "Moved to trash. Restore is available in Trash.");
                n.list_page(false);
            } else {
                message(
                    w,
                    n,
                    "Deletion not confirmed. Refresh before another attempt.",
                );
            }
        }
        Command::Restore(_) => {
            if value["restored"].as_bool() == Some(true) {
                message(w, n, "Restored on workshop.");
                n.list_page(false);
            } else {
                message(
                    w,
                    n,
                    "Restore not confirmed. Refresh Trash before retrying.",
                );
            }
        }
        Command::Search(_) => match serde_json::from_value::<VaultSearchResponse>(value) {
            Ok(r) => {
                if let Some(p) = panel(
                    w,
                    n,
                    if r.indexing == Some(true) {
                        "Search index warming · results may be incomplete"
                    } else {
                        "Search results · at most 100"
                    },
                ) {
                    for hit in r.hits {
                        let id = next();
                        visual::document_action(w, p, id, &hit.note.path, true);
                        n.management
                            .actions
                            .insert(id, super::Action::Open(hit.note.path));
                        if let Some(s) = hit.snippet {
                            caption(w, p, &s.chars().take(180).collect::<String>());
                        }
                    }
                }
            }
            Err(e) => message(w, n, &format!("Invalid search response: {e}")),
        },
        Command::Trash => match serde_json::from_value::<VaultTrashListResponse>(value) {
            Ok(r) => {
                if let Some(p) = panel(w, n, "Workshop trash · latest 100") {
                    for entry in r.entries {
                        control(
                            w,
                            p,
                            n,
                            &format!("Restore {}", entry.path),
                            Action::Restore(entry.path),
                            true,
                        );
                    }
                }
            }
            Err(e) => message(w, n, &format!("Invalid trash response: {e}")),
        },
        Command::Roots | Command::Switch(_) | Command::AddRoot { .. } => {
            match serde_json::from_value::<VaultRootsResponse>(value) {
                Ok(r) => {
                    if matches!(c, Command::Switch(_)) {
                        n.core = VaultBufferCore::new(r.active_root_id.clone());
                        n.list.clear();
                        n.tree_ids.clear();
                        n.recent.clear();
                        n.filter.clear();
                        if let Some(label) = n.search.and_then(|e| w.get_mut::<Label>(e)) {
                            label.text.clear();
                        }
                        n.tree_paths.clear();
                        n.expanded.clear();
                        n.cursor = None;
                        n.generation = None;
                        n.list_page(false);
                    }
                    if let Some(p) = panel(w, n, "Vault directories on the workshop") {
                        for root in r.roots {
                            control(
                                w,
                                p,
                                n,
                                &format!("{}{}", if root.active { "✓ " } else { "" }, root.label),
                                Action::Switch(root.id),
                                !root.active,
                            );
                            caption(w, p, &root.path);
                        }
                        control(
                            w,
                            p,
                            n,
                            "Link existing workshop directory",
                            Action::AddRoot,
                            true,
                        );
                    }
                }
                Err(e) => message(w, n, &format!("Invalid roots response: {e}")),
            }
        }
    }
}
