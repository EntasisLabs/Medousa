//! Retained daemon-backed notes. SourceDocuments owns editor history;
//! VaultBufferCore owns unsaved buffers and opaque daemon save baselines.
use medousa_app_core::{NoteBufferFact, VaultBufferCommand, VaultBufferCore};
use medousa_sdk::MedousaClient;
use medousa_types::{
    VaultNoteContentResponse, VaultNoteSummary, VaultNotesListResponse, VaultNotesQuery,
    VaultPutQuery, VaultWriteResponse,
};
use rndr_core::{
    editing::{DocumentId, SourceDocuments},
    *,
};
use rndr_ui::{
    Typography, UiColor, UiId, UiStyle,
    controls::{self, ButtonProps, HostIntent},
    editing::{EditorSurface, EditorSurfaceHandle, EditorSurfaceProps},
    text,
};
use std::{
    collections::HashMap,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0x4e4f_0000);
fn next() -> u64 {
    NEXT.fetch_add(1, Ordering::Relaxed)
}

#[derive(Clone, Debug)]
pub struct NoteCommand {
    serial: u64,
    operation: Operation,
}
#[derive(Clone, Debug)]
enum Operation {
    List {
        cursor: Option<String>,
        generation: Option<u64>,
    },
    Load(String),
    Save {
        path: String,
        content: String,
        token: String,
    },
}
#[derive(Debug)]
pub struct NoteUpdate {
    command: NoteCommand,
    result: Result<Response, String>,
}
#[derive(Debug)]
enum Response {
    List(VaultNotesListResponse),
    Loaded(VaultNoteContentResponse),
    Saved(VaultWriteResponse),
}
#[derive(Clone)]
enum Action {
    Refresh,
    More,
    Open(String),
    Save,
    Check,
    Acknowledge,
}
struct View {
    document: DocumentId,
    handle: EditorSurfaceHandle,
}
#[derive(Default)]
struct Notes {
    toolbar: Option<Entity>,
    editors: Option<Entity>,
    search: Option<Entity>,
    rows: Option<Entity>,
    core: VaultBufferCore,
    views: HashMap<String, View>,
    selected: Option<String>,
    actions: HashMap<u64, Action>,
    queue: Vec<NoteCommand>,
    started: bool,
    list: Vec<VaultNoteSummary>,
    cursor: Option<String>,
    generation: Option<u64>,
    truncated: bool,
    list_pending: Option<u64>,
    loads: HashMap<String, u64>,
    saves: HashMap<String, u64>,
    remote: HashMap<String, VaultNoteContentResponse>,
    errors: HashMap<String, String>,
    uncertain: std::collections::HashSet<String>,
    list_error: String,
    filter: String,
    redraw: bool,
}
impl Notes {
    fn enqueue(&mut self, operation: Operation) -> u64 {
        let serial = next();
        self.queue.push(NoteCommand { serial, operation });
        serial
    }
    fn list_page(&mut self, more: bool) {
        if self.list_pending.is_some() {
            return;
        }
        if !more {
            self.cursor = None;
            self.generation = None;
        }
        self.list_pending = Some(self.enqueue(Operation::List {
            cursor: if more { self.cursor.clone() } else { None },
            generation: self.generation,
        }));
        self.list_error.clear();
        self.redraw = true;
    }
    fn load(&mut self, path: String) {
        if self.loads.contains_key(&path) || self.saves.contains_key(&path) {
            return;
        }
        let id = self.enqueue(Operation::Load(path.clone()));
        self.loads.insert(path.clone(), id);
        self.errors.remove(&path);
        self.redraw = true;
    }
    fn save(&mut self) {
        let Some(path) = self.selected.clone() else {
            return;
        };
        if self.saves.contains_key(&path)
            || self.loads.contains_key(&path)
            || self.uncertain.contains(&path)
            || self.remote.contains_key(&path)
        {
            return;
        }
        let Some(b) = self.core.get(&path) else {
            return;
        };
        if !b.dirty || b.conflict {
            return;
        }
        let Some(token) = b.content_hash.clone().filter(|t| !t.is_empty()) else {
            self.errors.insert(
                path,
                "No daemon version available. Check latest before saving.".into(),
            );
            self.redraw = true;
            return;
        };
        let op = Operation::Save {
            path: path.clone(),
            content: b.content.clone(),
            token,
        };
        let id = self.enqueue(op);
        self.saves.insert(path.clone(), id);
        self.errors.remove(&path);
        self.redraw = true;
    }
}
fn column(world: &mut World, parent: Entity, width: Size, height: Size) -> Entity {
    let e = world.spawn((Layout {
        width,
        height,
        gap: 10.,
        padding: 14.,
        ..Layout::column()
    },));
    set_parent(world, e, parent);
    e
}
fn caption(world: &mut World, parent: Entity, s: &str) {
    text(world, parent, s, Typography::Secondary).expect("notes text");
}
fn button(
    world: &mut World,
    parent: Entity,
    n: &mut Notes,
    label: &str,
    action: Action,
    enabled: bool,
) {
    let id = next();
    let mut props = ButtonProps::new(label, HostIntent::Command(id));
    props.enabled = enabled;
    controls::Button::mount(world, parent, UiId(id), props).expect("notes button");
    n.actions.insert(id, action);
}
pub fn mount(world: &mut World, parent: Entity) {
    if world.resource::<Notes>().is_some() {
        return;
    }
    let root = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        gap: 0.,
        ..Layout::row()
    },));
    set_parent(world, root, parent);
    let sidebar = column(world, root, Size::Px(260.), Size::Fill);
    world.insert(sidebar, Color::rgb(0.067, 0.067, 0.082));
    text(world, sidebar, "Notes", Typography::Title).expect("notes title");
    caption(world, sidebar, "Your connected workshop vault");
    let search = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(36.),
            ..Layout::row()
        },
        Label::new(""),
        Focusable,
        Prompt {
            submit_on_enter: false,
            ..Prompt::new()
        },
        Placeholder::new("Filter loaded notes…"),
        Semantics {
            name: Some("Filter loaded notes".into()),
            ..Default::default()
        },
    ));
    set_parent(world, search, sidebar);
    let rows = column(world, sidebar, Size::Fill, Size::Fill);
    world.insert(rows, Scroll::default());
    let content = column(world, root, Size::Fill, Size::Fill);
    let toolbar = column(world, content, Size::Fill, Size::Hug);
    let editors = column(world, content, Size::Fill, Size::Fill);
    caption(
        world,
        editors,
        "Select a note to begin. Drafts stay here when you switch surfaces.",
    );
    world.insert_resource(Notes {
        toolbar: Some(toolbar),
        editors: Some(editors),
        search: Some(search),
        rows: Some(rows),
        core: VaultBufferCore::new("connected-workshop"),
        redraw: true,
        ..Default::default()
    });
    tick(world);
}
/// Surface navigation must not interrupt native IME composition.
pub fn can_deactivate(world: &World) -> bool {
    world.resource::<Notes>().is_none_or(|n| {
        n.views.values().all(|v| {
            world
                .resource::<SourceDocuments>()
                .and_then(|d| d.get(v.document))
                .is_none_or(|d| !d.is_composing())
        })
    })
}
pub fn activate(world: &mut World) {
    let Some(n) = world.resource_mut::<Notes>() else {
        return;
    };
    if !n.started {
        n.started = true;
        n.list_page(false);
    }
}
fn capture(world: &World, n: &mut Notes) {
    for (path, v) in &n.views {
        if let Some(d) = world
            .resource::<SourceDocuments>()
            .and_then(|s| s.get(v.document))
        {
            if n.core.get(path).is_some_and(|b| b.content != d.text()) {
                let before = n.core.get(path).map(|b| (b.dirty, b.conflict));
                let _ = n.core.dispatch(VaultBufferCommand::ApplyEdit {
                    path: path.clone(),
                    content: d.text().into(),
                    title: None,
                });
                n.redraw |= before != n.core.get(path).map(|b| (b.dirty, b.conflict));
            }
        }
    }
}
fn sync_document(world: &mut World, n: &Notes, path: &str) {
    if let (Some(b), Some(v)) = (n.core.get(path), n.views.get(path)) {
        if let Some(d) = world
            .resource_mut::<SourceDocuments>()
            .and_then(|s| s.get_mut(v.document))
        {
            // Never interrupt IME composition. Capture preserves its local draft.
            if d.text() != b.content && !d.is_composing() {
                let _ = d.reload(d.revision(), b.content.clone());
            }
        }
    }
}
fn render(world: &mut World, n: &mut Notes) {
    let (Some(rows), Some(toolbar)) = (n.rows, n.toolbar) else {
        return;
    };
    for parent in [rows, toolbar] {
        for c in children_of(world, parent) {
            despawn(world, c);
        }
    }
    n.actions.clear();
    button(
        world,
        rows,
        n,
        "Refresh notes",
        Action::Refresh,
        n.list_pending.is_none(),
    );
    if n.list_pending.is_some() {
        caption(world, rows, "Loading vault notes…");
    }
    if !n.list_error.is_empty() {
        caption(world, rows, &n.list_error);
    }
    let filter = n.filter.to_lowercase();
    let list = n.list.clone();
    let mut matches = 0;
    for item in &list {
        if !format!("{} {}", item.title, item.path)
            .to_lowercase()
            .contains(&filter)
        {
            continue;
        }
        matches += 1;
        let selected = n.selected.as_deref() == Some(&item.path);
        let dirty = n.core.get(&item.path).is_some_and(|b| b.dirty);
        let title = if item.title.is_empty() {
            &item.path
        } else {
            &item.title
        };
        button(
            world,
            rows,
            n,
            &format!(
                "{}{}{}",
                if selected { "› " } else { "" },
                title,
                if dirty { " •" } else { "" }
            ),
            Action::Open(item.path.clone()),
            true,
        );
    }
    if matches == 0 && n.list_pending.is_none() {
        caption(
            world,
            rows,
            if n.list.is_empty() && !n.list_error.is_empty() {
                "No notes loaded. Refresh to retry."
            } else if n.list.is_empty() && !n.truncated && n.cursor.is_none() && n.started {
                "No notes in the connected vault yet."
            } else if n.list.is_empty() {
                "No notes loaded."
            } else {
                "No matching loaded notes."
            },
        );
    }
    caption(
        world,
        rows,
        &format!(
            "{} loaded · filter searches loaded titles and paths",
            n.list.len()
        ),
    );
    if n.cursor.is_some() {
        button(
            world,
            rows,
            n,
            "Load more notes",
            Action::More,
            n.list_pending.is_none(),
        );
    }
    if n.truncated {
        caption(
            world,
            rows,
            "The daemon returned a bounded listing; not all notes may be shown.",
        );
    }
    let Some(path) = n.selected.clone() else {
        caption(world, toolbar, "Notes");
        return;
    };
    caption(world, toolbar, &path);
    if n.loads.contains_key(&path) {
        caption(world, toolbar, "Checking daemon version…");
    }
    if let Some(b) = n.core.get(&path).cloned() {
        caption(
            world,
            toolbar,
            if b.conflict || n.remote.contains_key(&path) {
                "Changed on daemon · your edits are preserved"
            } else if b.dirty {
                "Unsaved changes · retained in this app"
            } else {
                "Saved to connected workshop"
            },
        );
        let saving = n.saves.contains_key(&path);
        button(
            world,
            toolbar,
            n,
            if saving { "Saving…" } else { "Save changes" },
            Action::Save,
            b.dirty
                && !b.conflict
                && !saving
                && !n.loads.contains_key(&path)
                && !n.uncertain.contains(&path)
                && !n.remote.contains_key(&path)
                && b.content_hash.as_ref().is_some_and(|s| !s.is_empty()),
        );
        button(
            world,
            toolbar,
            n,
            "Check latest (keep edits)",
            Action::Check,
            !saving && !n.loads.contains_key(&path),
        );
        if let Some(remote) = n.remote.get(&path) {
            caption(
                world,
                toolbar,
                "Compare with daemon content below. Merge in your editor, then acknowledge the latest baseline before saving. Nothing is overwritten until Save.",
            );
            // Bounded preview; the local editor is never replaced by conflict bytes.
            let preview: String = remote.content.chars().take(1200).collect();
            caption(
                world,
                toolbar,
                &format!(
                    "Latest daemon version:\n{}{}",
                    preview,
                    if remote.content.chars().count() > 1200 {
                        "\n[preview truncated]"
                    } else {
                        ""
                    }
                ),
            );
            button(
                world,
                toolbar,
                n,
                "Keep my edits · acknowledge latest baseline",
                Action::Acknowledge,
                true,
            );
        }
    }
    if let Some(error) = n.errors.get(&path) {
        caption(world, toolbar, error);
    }
    for (p, v) in &n.views {
        if *p == path {
            world.remove::<Hidden>(v.handle.root);
        } else {
            world.insert(v.handle.root, Hidden);
        }
    }
}
pub fn tick(world: &mut World) {
    if world.resource::<Notes>().is_none() {
        return;
    }
    let mut n = std::mem::take(world.resource_mut::<Notes>().unwrap());
    capture(world, &mut n);
    if let Some(s) = n
        .search
        .and_then(|s| world.get::<Label>(s))
        .map(|l| l.text.clone())
    {
        if s != n.filter {
            n.filter = s;
            n.redraw = true;
        }
    }
    if n.redraw {
        n.redraw = false;
        render(world, &mut n);
    }
    *world.resource_mut::<Notes>().unwrap() = n;
}
pub fn action(world: &mut World, host: &HostIntent) -> bool {
    let HostIntent::Command(id) = host else {
        return false;
    };
    let Some(a) = world
        .resource::<Notes>()
        .and_then(|n| n.actions.get(id))
        .cloned()
    else {
        return false;
    };
    let mut n = std::mem::take(world.resource_mut::<Notes>().unwrap());
    capture(world, &mut n);
    match a {
        Action::Refresh => n.list_page(false),
        Action::More => {
            if n.cursor.is_some() {
                n.list_page(true);
            }
        }
        Action::Open(path) => {
            let composing = n.views.values().any(|v| {
                world
                    .resource::<SourceDocuments>()
                    .and_then(|d| d.get(v.document))
                    .is_some_and(|d| d.is_composing())
            });
            if composing && n.selected.as_ref() != Some(&path) {
                if let Some(p) = n.selected.clone() {
                    n.errors
                        .insert(p, "Finish text composition before switching notes.".into());
                }
                n.redraw = true;
            } else {
                if n.selected.as_ref() != Some(&path) {
                    let old_editors: Vec<_> = n.views.values().map(|v| v.handle.editor).collect();
                    if let Some(focus) = world.resource_mut::<Focus>() {
                        if focus.entity.is_some_and(|e| old_editors.contains(&e)) {
                            focus.entity = None;
                        }
                    }
                }
                n.selected = Some(path.clone());
                if !n.views.contains_key(&path) {
                    n.load(path);
                }
                n.redraw = true;
            }
        }
        Action::Save => n.save(),
        Action::Check => {
            if let Some(p) = n.selected.clone() {
                n.load(p);
            }
        }
        Action::Acknowledge => {
            if let Some(path) = n.selected.clone() {
                if let (Some(remote), Some(local)) =
                    (n.remote.remove(&path), n.core.get(&path).cloned())
                {
                    let token = version(remote.note_version, remote.note.content_hash);
                    let _ = n.core.dispatch(VaultBufferCommand::SeedBuffer {
                        buffer: NoteBufferFact {
                            baseline_content: remote.content,
                            content_hash: token,
                            conflict: false,
                            ..local
                        },
                    });
                    n.uncertain.remove(&path);
                    n.errors.remove(&path);
                    n.redraw = true;
                }
            }
        }
    }
    *world.resource_mut::<Notes>().unwrap() = n;
    tick(world);
    true
}
pub fn drain_commands(world: &mut World) -> Vec<NoteCommand> {
    world
        .resource_mut::<Notes>()
        .map(|n| std::mem::take(&mut n.queue))
        .unwrap_or_default()
}
fn version(opaque: Option<String>, hash: String) -> Option<String> {
    opaque
        .filter(|s| !s.is_empty())
        .or_else(|| (!hash.is_empty()).then_some(hash))
}
pub async fn execute(client: &MedousaClient, c: NoteCommand) -> NoteUpdate {
    let result = match &c.operation {
        Operation::List { cursor, generation } => client
            .vault()
            .list_notes(&VaultNotesQuery {
                limit: Some(100),
                cursor: cursor.clone(),
                generation: *generation,
                ..Default::default()
            })
            .await
            .map(Response::List),
        Operation::Load(path) => client.vault().get_note(path).await.map(Response::Loaded),
        Operation::Save {
            path,
            content,
            token,
        } => client
            .vault()
            .update_note(
                path,
                content,
                &VaultPutQuery {
                    auto_workshop_tags: Some(false),
                    ..Default::default()
                },
                Some(token),
            )
            .await
            .map(Response::Saved),
    }
    .map_err(|e| e.to_string());
    NoteUpdate { command: c, result }
}
fn saved(n: &mut Notes, path: &str, sent: &str, r: VaultWriteResponse) {
    // MarkSaved overwrites content. Capture the live buffer first, then restore
    // any edits made while the request was in flight against the new baseline.
    let live = n.core.get(path).map(|b| b.content.clone());
    let canonical = r.content.unwrap_or_else(|| sent.into());
    let _ = n.core.dispatch(VaultBufferCommand::MarkSaved {
        path: path.into(),
        content: canonical,
        content_hash: version(r.note_version, r.note.content_hash),
        title: Some(r.note.title),
    });
    if let Some(live) = live.filter(|live| live != sent) {
        let _ = n.core.dispatch(VaultBufferCommand::ApplyEdit {
            path: path.into(),
            content: live,
            title: None,
        });
    }
}
pub fn apply(world: &mut World, u: NoteUpdate) {
    let Some(resource) = world.resource_mut::<Notes>() else {
        return;
    };
    let mut n = std::mem::take(resource);
    capture(world, &mut n);
    let NoteUpdate { command: c, result } = u;
    let valid = match &c.operation {
        Operation::List { .. } => n.list_pending == Some(c.serial),
        Operation::Load(path) => n.loads.get(path) == Some(&c.serial),
        Operation::Save { path, .. } => n.saves.get(path) == Some(&c.serial),
    };
    if !valid {
        *world.resource_mut::<Notes>().unwrap() = n;
        return;
    }
    match c.operation {
        Operation::List { cursor, .. } => {
            n.list_pending = None;
            match result {
                Ok(Response::List(r)) => {
                    if r.reset_required {
                        n.list.clear();
                        n.cursor = None;
                        n.generation = None;
                        n.list_error =
                            "Vault changed during pagination. Refresh to restart the listing."
                                .into();
                    } else {
                        if cursor.is_none() {
                            n.list.clear();
                        }
                        for item in r.notes {
                            if let Some(old) = n.list.iter_mut().find(|i| i.path == item.path) {
                                *old = item;
                            } else {
                                n.list.push(item);
                            }
                        }
                        n.cursor = r.next_cursor;
                        n.generation = r.vault_generation;
                        n.truncated = r.truncated;
                    }
                }
                Err(e) => n.list_error = format!("Could not list notes: {e}"),
                _ => {}
            }
        }
        Operation::Load(path) => {
            n.loads.remove(&path);
            match result {
                Ok(Response::Loaded(r)) => {
                    let token = version(r.note_version.clone(), r.note.content_hash.clone());
                    if n.core.get(&path).is_some() {
                        if let Some(local) =
                            n.core.get(&path).cloned().filter(|b| {
                                b.dirty && !b.conflict && b.baseline_content == r.content
                            })
                        {
                            let _ = n.core.dispatch(VaultBufferCommand::SeedBuffer {
                                buffer: NoteBufferFact {
                                    content_hash: token,
                                    title: r.note.title.clone(),
                                    ..local
                                },
                            });
                        } else {
                            let _ = n.core.dispatch(VaultBufferCommand::ReconcileFromDisk {
                                path: path.clone(),
                                content: r.content.clone(),
                                content_hash: token,
                                title: Some(r.note.title.clone()),
                            });
                        }
                        if n.core.get(&path).is_some_and(|b| b.conflict) {
                            n.remote.insert(path.clone(), r);
                        } else {
                            n.remote.remove(&path);
                            n.uncertain.remove(&path);
                        }
                    } else {
                        let _ = n.core.dispatch(VaultBufferCommand::SeedBuffer {
                            buffer: NoteBufferFact {
                                path: path.clone(),
                                content: r.content.clone(),
                                baseline_content: r.content,
                                content_hash: token,
                                title: r.note.title,
                                dirty: false,
                                content_revision: 0,
                                conflict: false,
                            },
                        });
                        if world.resource::<SourceDocuments>().is_none() {
                            world.insert_resource(SourceDocuments::default());
                        }
                        let document = world
                            .resource_mut::<SourceDocuments>()
                            .unwrap()
                            .open(n.core.get(&path).map_or("", |b| b.content.as_str()));
                        let parent = n.editors.expect("notes mounted");
                        if n.views.is_empty() {
                            for child in children_of(world, parent) {
                                despawn(world, child);
                            }
                        }
                        let handle = EditorSurface::mount(
                            world,
                            parent,
                            UiId(next()),
                            EditorSurfaceProps::new(document, format!("Edit note {path}")),
                        )
                        .expect("notes source document");
                        world.insert(
                            handle.root,
                            Layout {
                                width: Size::Fill,
                                height: Size::Fill,
                                ..Layout::row()
                            },
                        );
                        world.insert(
                            handle.editor,
                            UiStyle {
                                background: Some(UiColor::Explicit(Color::rgb(0.075, 0.075, 0.09))),
                                ..Default::default()
                            },
                        );
                        world.insert(handle.editor, ResolvedColor);
                        world.insert(handle.editor, Color::rgb(0.075, 0.075, 0.09));
                        n.views.insert(path.clone(), View { document, handle });
                    }
                    sync_document(world, &n, &path);
                }
                Err(e) => {
                    n.errors.insert(
                        path,
                        format!("Could not load note: {e}. Your edits are unchanged."),
                    );
                }
                _ => {}
            }
        }
        Operation::Save { path, content, .. } => {
            n.saves.remove(&path);
            match result {
                Ok(Response::Saved(r)) => {
                    saved(&mut n, &path, &content, r);
                    sync_document(world, &n, &path);
                    n.errors.remove(&path);
                    n.remote.remove(&path);
                }
                Err(e) => {
                    n.uncertain.insert(path.clone());
                    n.errors.insert(path,format!("Save not confirmed: {e}. Your edits are preserved. Check latest before another save; no automatic retry."));
                }
                _ => {}
            }
        }
    }
    n.redraw = true;
    *world.resource_mut::<Notes>().unwrap() = n;
    tick(world);
}

#[cfg(test)]
#[path = "notes_tests.rs"]
mod tests;
