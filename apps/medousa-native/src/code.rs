//! Native adapter over the canonical Forge source API. SourceDocuments owns
//! per-file history/selection/scroll; the daemon owns project files and leases.
use crate::native_style as visual;
use medousa_sdk::{MedousaClient, operations as ops};
use rndr_core::{
    editing::{DocumentId, Revision, SourceDocuments},
    *,
};
use rndr_ui::{
    Corner, Typography, UiId, UiRecipe, UiStyle,
    controls::{self, ButtonProps, HostIntent},
    editing::{EditorSurface, EditorSurfaceHandle, EditorSurfaceProps},
    tabs::{
        TabId, TabSpec, TabStrip, TabStripEvent, TabStripEventKind, TabStripHandle, TabStripProps,
    },
    text,
};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
static NEXT: AtomicU64 = AtomicU64::new(0x434f_0000);
fn next() -> u64 {
    NEXT.fetch_add(1, Ordering::Relaxed)
}
type Key = (String, String);
#[path = "code_workbench.rs"]
mod workbench;
#[derive(Clone, Debug)]
pub struct CodeCommand {
    serial: u64,
    operation: Operation,
}
#[derive(Clone, Debug)]
enum Operation {
    Workbench(workbench::Command),
    Projects,
    Tree(String),
    Load(Key),
    Begin {
        work: String,
        resume: bool,
    },
    Heartbeat {
        work: String,
        lease: Lease,
    },
    Save {
        key: Key,
        content: String,
        digest: String,
        lease: Lease,
    },
}
#[derive(Debug)]
pub struct CodeUpdate {
    command: CodeCommand,
    result: Result<Value, String>,
}
#[derive(Clone, Debug)]
struct Project {
    id: String,
    title: String,
    begin: bool,
    resume: bool,
}
#[derive(Clone, Debug, PartialEq)]
struct Lease {
    id: String,
    generation: u64,
}
#[derive(Clone)]
struct LeaseWake;
impl Event for LeaseWake {}

struct Session {
    lease: Lease,
    checked: Instant,
}
#[derive(Clone, Debug)]
struct Source {
    content: String,
    digest: String,
    readonly: bool,
}
fn source(v: &Value, key: &Key) -> Result<Source, String> {
    if v["work_id"].as_str() != Some(&key.0) || v["path"].as_str() != Some(&key.1) {
        return Err("Daemon returned a different file identity".into());
    }
    let content = v["content"]
        .as_str()
        .ok_or("Missing source content")?
        .to_owned();
    let digest = v["digest"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("Missing source digest")?
        .to_owned();
    let encoding = v["encoding"].as_str();
    let readonly = v["preview"].as_bool().unwrap_or(false)
        || v["truncated"].as_bool().unwrap_or(false)
        || encoding.is_some_and(|e| e != "utf-8")
        || content.contains('\0');
    Ok(Source {
        content,
        digest,
        readonly,
    })
}
struct Buffer {
    baseline: Source,
    content: String,
    remote: Option<Source>,
    uncertain: bool,
    error: String,
}
impl Buffer {
    fn dirty(&self) -> bool {
        self.content != self.baseline.content
    }
    fn saveable(&self) -> bool {
        self.dirty() && !self.baseline.readonly && self.remote.is_none() && !self.uncertain
    }
    fn reconcile(&mut self, incoming: Source) {
        if self.dirty()
            && self.content != incoming.content
            && self.baseline.content != incoming.content
        {
            self.remote = Some(incoming);
        } else {
            if !self.dirty() {
                self.content = incoming.content.clone();
            }
            self.baseline = incoming;
            self.remote = None;
            self.uncertain = false;
        }
        self.error.clear();
    }
    fn saved(&mut self, sent: &str, incoming: Source) {
        if self.content == sent {
            self.content = incoming.content.clone();
        }
        self.baseline = incoming;
        self.remote = None;
        self.uncertain = false;
        self.error.clear();
    }
}
struct View {
    document: DocumentId,
    captured_revision: Option<Revision>,
    handle: EditorSurfaceHandle,
    id: UiId,
}
#[derive(Clone)]
enum Action {
    Workbench(workbench::Action),
    Refresh,
    Project(String),
    File(String),
    Begin,
    Save,
    Check,
    Acknowledge,
    Folder(String),
}
#[derive(Default)]
struct Code {
    workbench: workbench::Workbench,
    rows: Option<Entity>,
    toolbar: Option<Entity>,
    editors: Option<Entity>,
    tabs: Option<Entity>,
    strip: Option<TabStripHandle>,
    projects: Vec<Project>,
    selected: Option<String>,
    files: HashMap<String, Vec<String>>,
    truncated: HashSet<String>,
    expanded: HashSet<(String, String)>,
    current: HashMap<String, String>,
    buffers: HashMap<Key, Buffer>,
    views: HashMap<Key, View>,
    sessions: HashMap<String, Session>,
    pending: HashMap<String, u64>,
    queue: Vec<CodeCommand>,
    actions: HashMap<u64, Action>,
    deferred: Vec<CodeUpdate>,
    begin_blocked: HashSet<String>,
    started: bool,
    redraw: bool,
    status: String,
    lease_wake: Option<(Instant, TimerHandle)>,
}
fn tag(op: &Operation) -> String {
    match op {
        Operation::Workbench(c) => format!("workbench:{}", c.work),
        Operation::Projects => "projects".into(),
        Operation::Tree(w) => format!("tree:{w}"),
        Operation::Load((w, p)) => format!("load:{w}:{p}"),
        Operation::Begin { work, .. } => format!("begin:{work}"),
        Operation::Heartbeat { work, .. } => format!("heartbeat:{work}"),
        Operation::Save { key: (w, p), .. } => format!("save:{w}:{p}"),
    }
}
impl Code {
    fn enqueue(&mut self, operation: Operation) {
        let key = tag(&operation);
        if self.pending.contains_key(&key) {
            return;
        }
        let serial = next();
        self.pending.insert(key, serial);
        self.queue.push(CodeCommand { serial, operation });
        self.redraw = true;
    }
    fn key(&self) -> Option<Key> {
        let w = self.selected.as_ref()?;
        Some((w.clone(), self.current.get(w)?.clone()))
    }
    fn busy(&self, prefix: &str, key: &Key) -> bool {
        self.pending
            .contains_key(&format!("{prefix}:{}:{}", key.0, key.1))
    }
    fn save(&mut self) {
        let Some(key) = self.key() else {
            return;
        };
        if self.busy("save", &key) || self.busy("load", &key) {
            return;
        }
        let Some(b) = self.buffers.get(&key).filter(|b| b.saveable()) else {
            return;
        };
        let Some(s) = self
            .sessions
            .get(&key.0)
            .filter(|s| s.checked.elapsed() < Duration::from_secs(45))
        else {
            return;
        };
        self.enqueue(Operation::Save {
            key: key.clone(),
            content: b.content.clone(),
            digest: b.baseline.digest.clone(),
            lease: s.lease.clone(),
        });
    }
}
fn column(w: &mut World, p: Entity, width: Size, height: Size) -> Entity {
    let e = w.spawn((Layout {
        width,
        height,
        gap: 0.,
        padding: 0.,
        ..Layout::column()
    },));
    set_parent(w, e, p);
    e
}
fn caption(w: &mut World, p: Entity, s: &str) {
    let e = text(w, p, s, Typography::Secondary).expect("code text");
    if w.get::<Layout>(p).is_some_and(|l| l.axis == Axis::Row) {
        w.insert(
            e,
            Layout {
                width: Size::Px((s.chars().count() as f32 * 7. + 8.).clamp(40., 280.)),
                height: Size::Px(28.),
                ..Layout::column()
            },
        );
        w.get_mut::<Label>(e).unwrap().overflow = TextOverflow::Ellipsis;
    }
}
// Controls keep RNDR keyboard/focus/intent handling; only their authored
// outer geometry and label overflow are adapted to the dense workspace.
fn compact(w: &mut World, h: controls::ControlHandle, width: Size) {
    w.insert(
        h.root,
        Layout {
            width,
            height: Size::Px(28.),
            padding: 0.,
            ..Layout::column()
        },
    );
    w.get_mut::<Label>(h.label).unwrap().overflow = TextOverflow::Ellipsis;
}
fn button(w: &mut World, p: Entity, c: &mut Code, s: &str, a: Action, enabled: bool) {
    let id = next();
    visual::document_action(w, p, id, s, enabled);
    c.actions.insert(id, a);
}
fn tree_row(w: &mut World, p: Entity, c: &mut Code, s: &str, a: Action, selected: bool) {
    let id = next();
    let h = controls::NavigationRow::mount(
        w,
        p,
        UiId(id),
        ButtonProps::new(s, HostIntent::Command(id)),
        controls::NavigationMeta {
            selected,
            ..Default::default()
        },
    )
    .expect("source row");
    compact(w, h, Size::Fill);
    c.actions.insert(id, a);
}
fn row(w: &mut World, p: Entity) -> Entity {
    let e = w.spawn((Layout {
        width: Size::Fill,
        height: Size::Hug,
        gap: 4.,
        padding: 4.,
        ..Layout::row()
    },));
    set_parent(w, e, p);
    e
}
fn style_editor(w: &mut World, handle: EditorSurfaceHandle) {
    visual::flat(w, handle.root, visual::canvas());
    // No parallel text projection: core retains composition, selection and undo.
    w.insert(
        handle.editor,
        UiRecipe(UiStyle {
            background: Some(visual::canvas().into()),
            foreground: Some(visual::text().into()),
            typography: Some(Typography::Code),
            corner: Some(Corner::Square),
            padding: Some(rndr_ui::Space::Zero),
            ..UiStyle::default()
        }),
    );
    for child in children_of(w, handle.root) {
        if w.has::<Gutter>(child) || w.has::<Scrollbar>(child) {
            w.insert(
                child,
                UiRecipe(UiStyle {
                    background: Some(visual::canvas().into()),
                    foreground: Some(visual::secondary().into()),
                    typography: Some(Typography::Code),
                    ..UiStyle::default()
                }),
            );
        }
    }
}
fn highlight(w: &mut World, editor: Entity, content: &str) {
    // Reuse RNDR's bounded generic code-token colors, not a second parser/editor.
    // Full language-aware semantic tokens remain outside this presentation pass.
    let palette = w
        .resource::<rndr_ui::UiTheme>()
        .cloned()
        .unwrap_or_default()
        .markdown_theme();
    let original = MarkdownTheme::default();
    let mut runs = code_token_spans(content);
    // RNDR's public generic lexer currently uses its default palette. Keep its
    // byte ranges and remap only its documented four token colors.
    for run in &mut runs {
        let color = run.color;
        run.color = if color == original.keyword {
            palette.keyword
        } else if color == original.string {
            palette.string
        } else if color == original.number {
            palette.number
        } else if color == original.comment {
            palette.comment
        } else {
            color
        };
    }
    w.insert(editor, Spans { runs });
}
pub fn mount(w: &mut World, parent: Entity) {
    if w.resource::<Code>().is_some() {
        return;
    }
    let root = w.spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        gap: 0.,
        ..Layout::row()
    },));
    set_parent(w, root, parent);
    let sidebar = column(w, root, Size::Px(300.), Size::Fill);
    visual::navigation_frame(w, sidebar);
    let heading = row(w, sidebar);
    text(w, heading, "Explorer", Typography::Emphasis).unwrap();
    w.get_mut::<Layout>(heading).unwrap().height = Size::Px(40.);
    let rows = column(w, sidebar, Size::Fill, Size::Fill);
    w.get_mut::<Layout>(rows).unwrap().padding = 4.;
    w.insert(rows, Scroll::default());
    let content = column(w, root, Size::Fill, Size::Fill);
    visual::flat(w, content, visual::canvas());
    let tabs = column(w, content, Size::Fill, Size::Hug);
    let toolbar = column(w, content, Size::Fill, Size::Hug);
    let panel = column(w, content, Size::Fill, Size::Px(240.));
    w.insert(panel, Hidden);
    w.insert(panel, Scroll::default());
    let editors = column(w, content, Size::Fill, Size::Fill);
    caption(
        w,
        editors,
        "Select a project and file. Drafts remain here while you switch surfaces.",
    );
    w.insert_resource(Code {
        workbench: workbench::Workbench::new(panel),
        rows: Some(rows),
        toolbar: Some(toolbar),
        editors: Some(editors),
        tabs: Some(tabs),
        redraw: true,
        ..Default::default()
    });
    sync(w);
}
pub fn activate(w: &mut World) {
    if let Some(c) = w.resource_mut::<Code>() {
        if !c.started {
            c.started = true;
            c.enqueue(Operation::Projects);
        }
    }
}
pub fn can_deactivate(w: &World) -> bool {
    w.resource::<Code>().is_none_or(|c| !composing(w, c))
}
fn composing(w: &World, c: &Code) -> bool {
    c.views.values().any(|v| {
        w.resource::<SourceDocuments>()
            .and_then(|d| d.get(v.document))
            .is_some_and(|d| d.is_composing())
    })
}
fn capture(w: &mut World, c: &mut Code) {
    let mut changed = Vec::new();
    for (key, v) in &mut c.views {
        if let (Some(b), Some(d)) = (
            c.buffers.get_mut(key),
            w.resource_mut::<SourceDocuments>()
                .and_then(|s| s.get_mut(v.document)),
        ) {
            // Selection/scroll frames do not require a full source comparison.
            // Still drain the adapter's document events on unchanged revisions.
            if v.captured_revision == Some(d.revision()) {
                d.drain_events().for_each(drop);
                continue;
            }
            v.captured_revision = Some(d.revision());
            if !b.baseline.readonly && b.content != d.text() {
                let before = b.dirty();
                b.content = d.text().into();
                changed.push((v.handle.editor, b.content.clone()));
                c.redraw |= before != b.dirty();
            }
            d.drain_events().for_each(drop);
        }
    }
    for (editor, content) in changed {
        highlight(w, editor, &content);
    }
}
fn sync_view(w: &mut World, c: &Code, key: &Key) {
    let (Some(b), Some(v)) = (c.buffers.get(key), c.views.get(key)) else {
        return;
    };
    if let Some(d) = w
        .resource_mut::<SourceDocuments>()
        .and_then(|s| s.get_mut(v.document))
    {
        if d.text() != b.content && !d.is_composing() {
            let _ = d.reload(d.revision(), b.content.clone());
        }
    }
    let mut props = EditorSurfaceProps::new(v.document, format!("Source {} · {}", key.1, key.0));
    props.readonly = b.baseline.readonly;
    let _ = EditorSurface::mount(w, c.editors.unwrap(), v.id, props);
    style_editor(w, v.handle);
    highlight(w, v.handle.editor, &b.content);
}
fn ensure_view(w: &mut World, c: &mut Code, key: &Key) {
    if c.views.contains_key(key) {
        sync_view(w, c, key);
        return;
    }
    if w.resource::<SourceDocuments>().is_none() {
        w.insert_resource(SourceDocuments::default());
    }
    let Some(b) = c.buffers.get(key) else {
        return;
    };
    let document = w
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .open(&b.content);
    let parent = c.editors.unwrap();
    if c.views.is_empty() {
        for child in children_of(w, parent) {
            despawn(w, child);
        }
    }
    let id = UiId(next());
    let mut props = EditorSurfaceProps::new(document, format!("Source {} · {}", key.1, key.0));
    props.readonly = b.baseline.readonly;
    let handle = EditorSurface::mount(w, parent, id, props).expect("source document");
    style_editor(w, handle);
    highlight(w, handle.editor, &b.content);
    w.insert(
        handle.root,
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            ..Layout::row()
        },
    );
    c.views.insert(
        key.clone(),
        View {
            document,
            captured_revision: None,
            handle,
            id,
        },
    );
}
fn render(w: &mut World, c: &mut Code) {
    crate::ui::chrome::clear_document_tabs(w, false);
    let (Some(rows), Some(toolbar)) = (c.rows, c.toolbar) else {
        return;
    };
    for p in [rows, toolbar] {
        for child in children_of(w, p) {
            despawn(w, child);
        }
    }
    c.actions.clear();
    let header = row(w, rows);
    button(
        w,
        header,
        c,
        "Refresh",
        Action::Refresh,
        !c.pending.contains_key("projects"),
    );
    if c.pending.contains_key("projects") {
        caption(w, rows, "Loading projects…");
    }
    for p in c.projects.clone() {
        let selected = c.selected.as_ref() == Some(&p.id);
        tree_row(w, rows, c, &p.title, Action::Project(p.id), selected);
    }
    if c.projects.is_empty() && c.started && !c.pending.contains_key("projects") {
        caption(w, rows, "No projects loaded. Refresh to retry.");
    }
    let tools = row(w, toolbar);
    workbench::navigation(w, tools, c);
    let actions = row(w, toolbar);
    if let Some(work) = c.selected.clone() {
        if c.pending.contains_key(&format!("tree:{work}")) {
            caption(w, rows, "Loading source tree…");
        }
        let files = c.files.get(&work).cloned().unwrap_or_default();
        let mut shown = HashSet::new();
        for path in files {
            let parts: Vec<_> = path.split('/').collect();
            let mut visible = true;
            for i in 1..parts.len() {
                let folder = parts[..i].join("/");
                if shown.insert(folder.clone()) {
                    let expanded = c.expanded.contains(&(work.clone(), folder.clone()));
                    tree_row(
                        w,
                        rows,
                        c,
                        &format!(
                            "{}{} {}",
                            "  ".repeat(i - 1),
                            if expanded {
                                Icon::ChevronDown.scalar()
                            } else {
                                Icon::ChevronRight.scalar()
                            },
                            parts[i - 1]
                        ),
                        Action::Folder(folder.clone()),
                        false,
                    );
                }
                if !c.expanded.contains(&(work.clone(), folder)) {
                    visible = false;
                    break;
                }
            }
            if visible {
                let key = (work.clone(), path.clone());
                let dirty = c.buffers.get(&key).is_some_and(Buffer::dirty);
                tree_row(
                    w,
                    rows,
                    c,
                    &format!(
                        "{}{}{}{}",
                        "  ".repeat(parts.len() - 1),
                        Icon::FileCode.scalar(),
                        parts.last().unwrap_or(&""),
                        if dirty { " •" } else { "" }
                    ),
                    Action::File(path.clone()),
                    c.current.get(&work) == Some(&path),
                );
            }
        }
        if c.truncated.contains(&work) {
            caption(w, rows, "Bounded tree · not every project file is shown.");
        }
        let project = c.projects.iter().find(|p| p.id == work).cloned();
        if c.sessions.contains_key(&work) {
            caption(w, actions, "Editing · human");
        } else if let Some(p) = project {
            button(
                w,
                actions,
                c,
                if p.resume {
                    "Continue editing"
                } else {
                    "Begin editing"
                },
                Action::Begin,
                (p.begin || p.resume)
                    && !c.pending.contains_key(&format!("begin:{work}"))
                    && !c.begin_blocked.contains(&work),
            );
        }
    }
    if let Some(key) = c.key() {
        let crumb = text(
            w,
            actions,
            &key.1.replace('/', "  ›  "),
            Typography::Secondary,
        )
        .expect("source breadcrumb");
        w.get_mut::<Label>(crumb).unwrap().overflow = TextOverflow::Ellipsis;
        w.get_mut::<Layout>(crumb).unwrap().height = Size::Px(24.);
        w.get_mut::<Layout>(crumb).unwrap().width = Size::Fill;
        if let Some(b) = c.buffers.get(&key) {
            let status = if b.baseline.readonly {
                "Read-only preview · binary, lossy or truncated content"
            } else if b.remote.is_some() {
                "Daemon changed · local draft preserved"
            } else if b.uncertain {
                "Save not confirmed · check latest before another save"
            } else if b.dirty() {
                "Unsaved"
            } else {
                "Up to date"
            };
            caption(w, toolbar, status);
            let saveable = b.saveable()
                && c.sessions
                    .get(&key.0)
                    .is_some_and(|s| s.checked.elapsed() < Duration::from_secs(45))
                && !c.busy("load", &key)
                && !c.busy("save", &key);
            let error = b.error.clone();
            let remote = b.remote.clone();
            button(
                w,
                actions,
                c,
                if c.busy("save", &key) {
                    "Saving…"
                } else {
                    "Save"
                },
                Action::Save,
                saveable,
            );
            button(
                w,
                actions,
                c,
                "Check latest",
                Action::Check,
                !c.busy("load", &key) && !c.busy("save", &key),
            );
            if let Some(remote) = remote {
                caption(
                    w,
                    toolbar,
                    &format!(
                        "Latest daemon content (bounded comparison):\n{}",
                        remote.content.chars().take(1200).collect::<String>()
                    ),
                );
                caption(
                    w,
                    toolbar,
                    "Merge in your editor, then explicitly acknowledge the latest baseline. Save remains a separate action.",
                );
                button(
                    w,
                    toolbar,
                    c,
                    "Keep draft · acknowledge latest baseline",
                    Action::Acknowledge,
                    !remote.readonly,
                );
            }
            if !error.is_empty() {
                caption(w, toolbar, &error);
            }
        } else {
            caption(w, toolbar, "Loading source…");
            button(
                w,
                toolbar,
                c,
                "Retry file read",
                Action::Check,
                !c.busy("load", &key),
            );
        }
    }
    if !c.status.is_empty() {
        caption(w, toolbar, &c.status);
    }
    let mut keys: Vec<_> = c
        .views
        .keys()
        .filter(|k| Some(&k.0) == c.selected.as_ref())
        .cloned()
        .collect();
    keys.sort();
    if let Some(parent) = c.tabs {
        w.insert(parent, Hidden);
    }
    if let (Some(parent), Some(key)) = (c.tabs, c.key()) {
        if c.views.contains_key(&key) {
            w.remove::<Hidden>(parent);
            let specs: Vec<_> = keys
                .iter()
                .map(|k| {
                    TabSpec::new(c.views[k].id.0, k.1.rsplit('/').next().unwrap_or(&k.1))
                        .icon(Icon::FileCode)
                        .dirty(c.buffers[k].dirty())
                        .closable(false)
                })
                .collect();
            let strip = TabStrip::mount(
                w,
                parent,
                UiId(0x434f_5441),
                TabStripProps::new("Source files"),
                &specs,
                TabId(c.views[&key].id.0),
            )
            .expect("source tabs");
            w.insert(
                strip.root,
                Layout {
                    width: Size::Fill,
                    height: Size::Px(36.),
                    ..Layout::column()
                },
            );
            w.insert(strip.body, Hidden);
            crate::ui::chrome::document_tabs(w, strip, false);
            c.strip = Some(strip);
        }
    }
    let selected = c.key();
    for (key, v) in &c.views {
        if selected.as_ref() == Some(key) {
            w.remove::<Hidden>(v.handle.root);
        } else {
            w.insert(v.handle.root, Hidden);
        }
    }
}
/// Called only when the effective theme changes, including retained hidden files.
pub fn refresh_appearance(w: &mut World) {
    let views = w
        .resource::<Code>()
        .map(|c| {
            c.views
                .iter()
                .filter_map(|(key, v)| {
                    c.buffers
                        .get(key)
                        .map(|b| (v.handle.editor, b.content.clone()))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for (editor, content) in views {
        highlight(w, editor, &content);
    }
}

pub fn sync(w: &mut World) {
    let Some(r) = w.resource_mut::<Code>() else {
        return;
    };
    let mut c = std::mem::take(r);
    capture(w, &mut c);
    for event in events::<TabStripEvent>(w).to_vec() {
        if c.strip.is_some_and(|s| s.root == event.strip) {
            if let TabStripEventKind::Activated(id) = event.kind {
                // RNDR requests activation; composition keeps the host's current
                // document authoritative and restores its selected tab.
                c.redraw = true;
                if composing(w, &c) {
                    continue;
                }
                if let Some(key) = c
                    .views
                    .iter()
                    .find(|(k, v)| Some(&k.0) == c.selected.as_ref() && v.id.0 == id.0)
                    .map(|(k, _)| k.clone())
                {
                    if c.key().as_ref() != Some(&key) {
                        c.selected = Some(key.0.clone());
                        c.current.insert(key.0, key.1);
                        clear_focus(w, &c);
                        c.redraw = true;
                    }
                }
            }
        }
    }

    let heartbeats: Vec<_> = c
        .sessions
        .iter()
        .filter(|(_, s)| s.checked.elapsed() >= Duration::from_secs(20))
        .map(|(work, s)| (work.clone(), s.lease.clone()))
        .collect();
    for (work, lease) in heartbeats {
        c.enqueue(Operation::Heartbeat { work, lease });
    }
    // A parked native window does not run Update. Arm the runtime deadline
    // for the next lease check, including while another surface is visible.
    // Pending requests wake us through their completion instead of a busy loop.
    let deadline = c
        .sessions
        .iter()
        .filter(|(work, _)| !c.pending.contains_key(&format!("heartbeat:{work}")))
        .map(|(_, session)| session.checked + Duration::from_secs(20))
        .min();
    if c.lease_wake.as_ref().map(|(at, _)| *at) != deadline {
        c.lease_wake = deadline.map(|at| {
            let delay = at
                .saturating_duration_since(Instant::now())
                .max(Duration::from_millis(1));
            (at, after(w, delay, LeaseWake))
        });
    }
    if c.redraw {
        c.redraw = false;
        render(w, &mut c);
    }
    let deferred = if !composing(w, &c) {
        std::mem::take(&mut c.deferred)
    } else {
        Vec::new()
    };
    *w.resource_mut::<Code>().unwrap() = c;
    for update in deferred {
        apply(w, update);
    }
}
pub fn action(w: &mut World, host: &HostIntent) -> bool {
    let HostIntent::Command(id) = host else {
        return false;
    };
    let Some(a) = w
        .resource::<Code>()
        .and_then(|c| c.actions.get(id).or_else(|| c.workbench.actions.get(id)))
        .cloned()
    else {
        return false;
    };
    let mut c = std::mem::take(w.resource_mut::<Code>().unwrap());
    capture(w, &mut c);
    match a {
        Action::Workbench(a) => workbench::action(w, &mut c, a),
        Action::Refresh => {
            c.enqueue(Operation::Projects);
            if let Some(work) = c.selected.clone() {
                c.enqueue(Operation::Tree(work));
            }
        }
        Action::Project(work) => {
            if !composing(w, &c) {
                c.selected = Some(work.clone());
                if !c.files.contains_key(&work) {
                    c.enqueue(Operation::Tree(work));
                }
                clear_focus(w, &c);
            }
        }
        Action::File(path) => {
            if !composing(w, &c) {
                if let Some(work) = c.selected.clone() {
                    let key = (work.clone(), path.clone());
                    c.current.insert(work, path);
                    if !c.buffers.contains_key(&key) {
                        c.enqueue(Operation::Load(key));
                    }
                    clear_focus(w, &c);
                }
            }
        }
        Action::Begin => {
            if let Some(p) = c
                .selected
                .as_ref()
                .and_then(|id| c.projects.iter().find(|p| &p.id == id))
                .cloned()
            {
                if (p.begin || p.resume)
                    && !c.sessions.contains_key(&p.id)
                    && !c.begin_blocked.contains(&p.id)
                {
                    c.enqueue(Operation::Begin {
                        work: p.id,
                        resume: !p.begin && p.resume,
                    });
                }
            }
        }
        Action::Save => {
            if !composing(w, &c) {
                c.save();
            }
        }
        Action::Check => {
            if let Some(key) = c.key() {
                if !c.busy("save", &key) {
                    c.enqueue(Operation::Load(key));
                }
            }
        }
        Action::Acknowledge => {
            if let Some(key) = c.key() {
                if let Some(b) = c.buffers.get_mut(&key) {
                    if let Some(remote) = b.remote.take() {
                        if !remote.readonly {
                            b.baseline = remote;
                            b.uncertain = false;
                            b.error.clear();
                        } else {
                            b.remote = Some(remote);
                        }
                    }
                }
            }
        }
        Action::Folder(path) => {
            if let Some(work) = c.selected.clone() {
                let key = (work, path);
                if !c.expanded.remove(&key) {
                    c.expanded.insert(key);
                }
            }
        }
    }
    c.redraw = true;
    *w.resource_mut::<Code>().unwrap() = c;
    sync(w);
    true
}
fn clear_focus(w: &mut World, c: &Code) {
    let editors: Vec<_> = c.views.values().map(|v| v.handle.editor).collect();
    if let Some(f) = w.resource_mut::<Focus>() {
        if f.entity.is_some_and(|e| editors.contains(&e)) {
            f.entity = None;
        }
    }
}
pub fn take_commands(w: &mut World) -> Vec<CodeCommand> {
    w.resource_mut::<Code>()
        .map(|c| std::mem::take(&mut c.queue))
        .unwrap_or_default()
}
fn route(op: ops::Operation, work: &str) -> String {
    op.path.replace("{work_id}", &urlencoding::encode(work))
}
pub async fn execute(client: &MedousaClient, command: CodeCommand) -> CodeUpdate {
    let http = client.http();
    let result:Result<Value,medousa_sdk::SdkError>=match &command.operation {
        Operation::Workbench(c)=>workbench::execute(client,c).await,
        Operation::Projects=>list_projects(client).await,
        Operation::Tree(work)=>http.get(&route(ops::FORGE_ITEMS_BY_WORK_ID_TREE_GET,work)).await,
        Operation::Load((work,path))=>http.get_query(&route(ops::FORGE_ITEMS_BY_WORK_ID_SOURCE_GET,work),&[("path",path.clone())]).await,
        Operation::Begin{work,resume}=>http.post(&route(if *resume {ops::FORGE_ITEMS_BY_WORK_ID_REVIEW_CONTINUE_EDITING_POST}else{ops::FORGE_ITEMS_BY_WORK_ID_ATTEMPTS_POST},work),&if *resume{json!({})}else{json!({"executor":{"kind":"human","detail":{}}})}).await,
        Operation::Heartbeat{lease,..}=>http.post(&ops::FORGE_LEASES_BY_LEASE_ID_HEARTBEAT_POST.path.replace("{lease_id}",&urlencoding::encode(&lease.id)),&json!({"generation":lease.generation})).await,
        Operation::Save{key:(work,path),content,digest,lease}=>http.put(&route(ops::FORGE_ITEMS_BY_WORK_ID_SOURCE_PUT,work),&json!({"path":path,"content":content,"expected_digest":digest,"lease_id":lease.id,"generation":lease.generation})).await,
    };
    CodeUpdate {
        command,
        result: result.map_err(|e| e.to_string()),
    }
}
async fn list_projects(client: &MedousaClient) -> Result<Value, medousa_sdk::SdkError> {
    let mut items = Vec::new();
    let mut cursor = None;
    let mut seen = HashSet::new();
    loop {
        let mut query = vec![("limit", "256".into())];
        if let Some(s) = cursor.take() {
            query.push(("cursor", s));
        }
        let v: Value = client
            .http()
            .get_query(ops::FORGE_ITEMS_GET.path, &query)
            .await?;
        if let Some(array) = v.as_array() {
            items.extend(array.iter().cloned());
            break;
        }
        let array = v["items"]
            .as_array()
            .ok_or_else(|| medousa_sdk::SdkError::Serde("Missing project items".into()))?;
        items.extend(array.iter().cloned());
        let next = v["next_cursor"]
            .as_str()
            .filter(|_| v["truncated"].as_bool() == Some(true));
        if let Some(next) = next {
            if !seen.insert(next.to_owned()) || seen.len() > 64 {
                return Err(medousa_sdk::SdkError::Serde(
                    "Project pagination did not terminate within bounds".into(),
                ));
            }
            cursor = Some(next.to_owned());
        } else {
            break;
        }
    }
    Ok(Value::Array(items))
}
pub fn apply(w: &mut World, u: CodeUpdate) {
    let Some(r) = w.resource_mut::<Code>() else {
        return;
    };
    let mut c = std::mem::take(r);
    capture(w, &mut c);
    if matches!(
        &u.command.operation,
        Operation::Load(_) | Operation::Save { .. }
    ) && composing(w, &c)
    {
        c.deferred.push(u);
        *w.resource_mut::<Code>().unwrap() = c;
        return;
    }
    let command = u.command;
    let pending = tag(&command.operation);
    if c.pending.get(&pending) != Some(&command.serial) {
        *w.resource_mut::<Code>().unwrap() = c;
        return;
    }
    c.pending.remove(&pending);
    match command.operation {
        Operation::Workbench(command) => workbench::apply(w, &mut c, command, u.result),
        Operation::Projects => match u.result {
            Ok(v) => {
                if let Some(items) = v.as_array() {
                    c.begin_blocked.clear();
                    c.projects = items
                        .iter()
                        .filter_map(|p| {
                            Some(Project {
                                id: p["id"].as_str()?.into(),
                                title: p["title"].as_str().unwrap_or("Untitled project").into(),
                                begin: p["allowed_actions"]["begin_attempt"]["allowed"].as_bool()
                                    == Some(true),
                                resume: p["allowed_actions"]["continue_editing"]["allowed"]
                                    .as_bool()
                                    == Some(true),
                            })
                        })
                        .collect();
                    c.status.clear();
                }
            }
            Err(e) => c.status = format!("Could not list projects: {e}"),
        },
        Operation::Tree(work) => match u.result {
            Ok(v) => {
                if v["work_id"].as_str() == Some(&work) {
                    if let Some(files) = v["files"].as_array() {
                        let mut paths: Vec<String> = files
                            .iter()
                            .filter_map(|f| f["path"].as_str().map(str::to_owned))
                            .collect();
                        paths.sort();
                        paths.dedup();
                        c.files.insert(work.clone(), paths);
                        if v["truncated"].as_bool() == Some(true) {
                            c.truncated.insert(work);
                        } else {
                            c.truncated.remove(&work);
                        }
                        c.status.clear();
                    } else {
                        c.status = "Malformed source tree".into();
                    }
                } else {
                    c.status = "Source tree identity mismatch".into();
                }
            }
            Err(e) => c.status = format!("Could not load source tree: {e}"),
        },
        Operation::Load(key) => match u.result.and_then(|v| source(&v, &key)) {
            Ok(s) => {
                if let Some(b) = c.buffers.get_mut(&key) {
                    b.reconcile(s);
                } else {
                    c.buffers.insert(
                        key.clone(),
                        Buffer {
                            content: s.content.clone(),
                            baseline: s,
                            remote: None,
                            uncertain: false,
                            error: String::new(),
                        },
                    );
                }
                ensure_view(w, &mut c, &key);
                workbench::apply_jump(w, &mut c);
                c.status.clear();
            }
            Err(e) => {
                if let Some(b) = c.buffers.get_mut(&key) {
                    b.error = format!("Read failed: {e}. Draft unchanged.");
                } else {
                    c.status = format!("Read failed: {e}");
                }
            }
        },
        Operation::Begin { work, .. } => {
            c.begin_blocked.insert(work.clone());
            match u.result {
                Ok(v) => {
                    let lease = &v["lease"];
                    if lease["work_id"].as_str() == Some(&work) {
                        if let (Some(id), Some(generation)) =
                            (lease["lease_id"].as_str(), lease["generation"].as_u64())
                        {
                            c.sessions.insert(
                                work,
                                Session {
                                    lease: Lease {
                                        id: id.into(),
                                        generation,
                                    },
                                    checked: Instant::now(),
                                },
                            );
                            c.status.clear();
                        } else {
                            c.status =
                                "Editing response missing lease; refresh before trying again"
                                    .into();
                        }
                    } else {
                        c.status =
                            "Editing response identity mismatch; refresh before trying again"
                                .into();
                    }
                }
                Err(e) => {
                    c.status = format!(
                        "Editing session not confirmed: {e}. Refresh project state before trying again; no automatic retry."
                    )
                }
            }
        }
        Operation::Heartbeat { work, lease } => {
            if c.sessions.get(&work).is_some_and(|s| s.lease == lease) {
                match u.result {
                    Ok(_) => c.sessions.get_mut(&work).unwrap().checked = Instant::now(),
                    Err(e) => {
                        c.sessions.remove(&work);
                        c.status = format!(
                            "Editing ownership no longer confirmed: {e}. Drafts retained; refresh project state."
                        );
                    }
                }
            }
        }
        Operation::Save { key, content, .. } => match u.result.and_then(|v| source(&v, &key)) {
            Ok(s) => {
                if let Some(b) = c.buffers.get_mut(&key) {
                    b.saved(&content, s);
                }
                sync_view(w, &c, &key);
            }
            Err(e) => {
                if let Some(b) = c.buffers.get_mut(&key) {
                    b.uncertain = true;
                    b.error = format!("Save not confirmed: {e}. Check latest; no automatic retry.");
                }
            }
        },
    }
    c.redraw = true;
    *w.resource_mut::<Code>().unwrap() = c;
    sync(w);
}
#[cfg(test)]
#[path = "code_tests.rs"]
mod tests;
