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
        reorder_tabs,
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
    find: Option<rndr_ui::source_tools::FindBar>,
}
#[derive(Clone)]
enum Action {
    Workbench(workbench::Action),
    Find,
    Refresh,
    Project(String),
    File(String),
    Begin,
    Save,
    Check,
    Acknowledge,
    Folder(String),
    Repository(String),
    ChooseProjects,
    ChooseUndertakings,
    ClosePicker,
    CopyRelative(Key),
    Reveal(Key),
    More,
}
#[derive(Clone, Debug, PartialEq)]
enum ExplorerPicker {
    Projects,
    Undertakings(String),
}
#[derive(Default)]
struct Code {
    repositories: HashMap<String, (String, String, String)>,
    explorer_picker: Option<ExplorerPicker>,
    explorer_context: Option<Entity>,
    explorer_filter: Option<Entity>,
    explorer_query: String,
    context_targets: HashMap<Entity, Key>,
    context_menu: Option<(rndr_ui::menu::MenuHandle, Key, HashMap<u64, Action>)>,
    // Overflow anchor and the commands it offers, rebuilt with the toolbar.
    more: Option<(Entity, Vec<Command>)>,
    explorer_actions: Option<Entity>,
    workbench: workbench::Workbench,
    rows: Option<Entity>,
    toolbar: Option<Entity>,
    editors: Option<Entity>,
    find_tools: Option<Entity>,
    /// Quiet editor-scoped status under the source: caret and language.
    status_line: Option<Entity>,
    find_open: HashSet<Key>,
    tabs: Option<Entity>,
    strip: Option<TabStripHandle>,
    projects: Vec<Project>,
    selected: Option<String>,
    files: HashMap<String, Vec<String>>,
    truncated: HashSet<String>,
    expanded: HashSet<(String, String)>,
    current: HashMap<String, String>,
    // Visible tabs are independent of retained drafts: closing never discards
    // one. Closing a clean file releases its buffer, editor and document.
    open_tabs: Vec<Key>,
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
    fn open_tab(&mut self, key: &Key) {
        if !self.open_tabs.contains(key) {
            self.open_tabs.push(key.clone());
        }
    }
    fn tab_specs(&self) -> Vec<TabSpec> {
        self.open_tabs
            .iter()
            .filter(|k| Some(&k.0) == self.selected.as_ref())
            .filter_map(|k| {
                let view = self.views.get(k)?;
                Some(
                    TabSpec::new(view.id.0, k.1.rsplit('/').next().unwrap_or(&k.1))
                        .icon(Icon::FileCode)
                        .dirty(self.buffers[k].dirty()),
                )
            })
            .collect()
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
const INDENT: &str = "    ";
/// A labelled toolbar command and whether it is currently available.
type Command = (&'static str, Action, bool);
/// The header's read of the current buffer; the buffer itself stays owned.
struct FileState {
    readonly: bool,
    remote: Option<Source>,
    uncertain: bool,
    dirty: bool,
    saveable: bool,
    error: String,
}
/// Explanatory band text under the header, inset to the location's left edge.
fn notice(w: &mut World, p: Entity, s: &str) {
    let e = text(w, p, s, Typography::Secondary).expect("code notice");
    w.insert(
        e,
        LabelBox {
            inset_x: 12.,
            inset_y: 6.,
            min_height: 0.,
        },
    );
}
fn caption(w: &mut World, p: Entity, s: &str) {
    let e = text(w, p, s, Typography::Secondary).expect("code text");
    if w.get::<Layout>(p).is_some_and(|l| l.axis == Axis::Row) {
        w.insert(
            e,
            Layout {
                height: Size::Px(28.),
                ..Layout::column()
            },
        );
        w.insert(
            e,
            visual::FitLabel {
                label: e,
                extra: 0.,
                min: 0.,
                max: 280.,
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
    if let (Action::File(path), Some(work)) = (&a, &c.selected) {
        w.insert(h.root, ContextTarget);
        c.context_targets
            .insert(h.root, (work.clone(), path.clone()));
    }
    c.actions.insert(id, a);
}
/// Explorer rows are rebuilt on redraw. A folder's chevron records which
/// folder it shows, so the rebuilt chevron starts at the presented angle.
struct FolderChevron(String);
impl Component for FolderChevron {}

fn turn_key(work: &str, folder: &str) -> String {
    format!("{work}\u{0}{folder}")
}

fn presented_turns(w: &World) -> HashMap<String, f32> {
    w.query::<FolderChevron>()
        .iter()
        .map(|(e, f)| {
            let angle = w.get::<Transform>(*e).map_or(0., |t| t.rotate);
            (f.0.clone(), angle)
        })
        .collect()
}

struct Folder<'a> {
    work: &'a str,
    path: &'a str,
    name: &'a str,
    depth: usize,
    expanded: bool,
}

/// A folder row: indentation, one right-pointing chevron turned a quarter
/// when expanded, then the name. The row keeps its name and Folder action.
fn folder_row(w: &mut World, p: Entity, c: &mut Code, f: Folder<'_>, turns: &HashMap<String, f32>) {
    let Folder {
        work,
        path: folder,
        name,
        depth,
        expanded,
    } = f;
    let id = next();
    let h = controls::NavigationRow::mount(
        w,
        p,
        UiId(id),
        ButtonProps::new(format!(" {name}"), HostIntent::Command(id)),
        controls::NavigationMeta::default(),
    )
    .expect("folder row");
    compact(w, h, Size::Fill);
    c.actions.insert(id, Action::Folder(folder.into()));
    let Some(face) = w.get::<Parent>(h.label).map(|p| p.entity) else {
        return;
    };
    if let Some(l) = w.get_mut::<Layout>(face) {
        l.axis = Axis::Row;
    }
    w.insert(
        face,
        Align {
            items: Items::Center,
            ..Default::default()
        },
    );
    // Same leading inset and indentation as a file row's text, so the
    // chevron sits where a file icon would at this depth.
    let inset = w.get::<LabelBox>(h.label).map_or(0., |b| b.inset_x);
    let typography = UiStyle {
        typography: Some(Typography::Label),
        ..Default::default()
    };
    let indent = w.spawn((
        Layout::column(),
        Label::new(INDENT.repeat(depth)),
        LabelBox {
            inset_x: inset * 0.5,
            ..LabelBox::CONTENT
        },
        typography.clone(),
    ));
    let angle = if expanded {
        std::f32::consts::FRAC_PI_2
    } else {
        0.
    };
    let chevron = w.spawn((
        Layout::column(),
        Label::new(Icon::ChevronRight.scalar().to_string()),
        LabelBox::CONTENT,
        typography,
        Transform {
            rotate: turns.get(&turn_key(work, folder)).copied().unwrap_or(angle),
            ..Default::default()
        },
        FolderChevron(turn_key(work, folder)),
    ));
    for e in [indent, chevron] {
        set_parent(w, e, face);
    }
    if let Some(children) = w.get_mut::<Children>(face) {
        children.list.retain(|e| *e != indent && *e != chevron);
        children.list.splice(0..0, [indent, chevron]);
    }
    if let Some(b) = w.get_mut::<LabelBox>(h.label) {
        b.inset_x = 1.;
    }
    visual::ease(w, chevron, motion::Prop::Rotate, angle);
}
fn row(w: &mut World, p: Entity) -> Entity {
    // Measured actions flow onto another line instead of clipping or eliding.
    let e = w.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 4.,
            padding: 4.,
            ..Layout::row()
        },
        Wrap { line_gap: 4. },
    ));
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
    let runs = crate::diff_presentation::syntax_spans(w, content);
    w.insert(editor, Spans { runs });
}
pub fn mount(w: &mut World, parent: Entity) {
    // Source rebind has already checked drafts/leases and removed the old UI.
    if w.resource::<Code>()
        .and_then(|s| s.rows)
        .is_some_and(|e| w.get::<Layout>(e).is_some())
    {
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
    w.remove::<Wrap>(heading);
    w.get_mut::<Layout>(heading).unwrap().height = Size::Px(40.);
    w.insert(
        heading,
        Align {
            items: Items::Center,
            ..Default::default()
        },
    );
    let title = text(w, heading, "Explorer", Typography::Emphasis).unwrap();
    w.get_mut::<Layout>(title).unwrap().width = Size::Fill;
    let explorer_actions = w.spawn((Layout::row(),));
    set_parent(w, explorer_actions, heading);
    let explorer_context = column(w, sidebar, Size::Fill, Size::Hug);
    let filter = w.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(32.),
            ..Layout::row()
        },
        Label::new(""),
        Focusable,
        Prompt {
            submit_on_enter: false,
            ..Prompt::new()
        },
        Placeholder::new("Filter files…"),
        Semantics {
            name: Some("Filter explorer".into()),
            ..Default::default()
        },
    ));
    set_parent(w, filter, sidebar);
    // Same quiet field as the conversation search.
    w.get_mut::<Layout>(filter).unwrap().padding = 6.;
    visual::plane(w, filter, visual::rail());
    w.get_mut::<UiStyle>(filter).unwrap().typography = Some(Typography::Label);
    w.insert(filter, visual::SemanticBorder(visual::stroke()));
    w.insert(filter, Radius(visual::CONTROL_RADIUS));
    let stroke = visual::color(w, visual::stroke());
    w.insert(
        filter,
        Border {
            color: stroke,
            width: 1.,
        },
    );
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
    let find_tools = column(w, content, Size::Fill, Size::Hug);
    let editor_dock = crate::terminal::mount(w, content);
    let editors = column(w, editor_dock, Size::Fill, Size::Fill);
    // Editor status lives with the editor rather than in a window-wide bar,
    // and only while a file is open.
    let status_line = text(w, content, "", Typography::Caption).unwrap();
    w.get_mut::<Layout>(status_line).unwrap().width = Size::Fill;
    w.insert(
        status_line,
        LabelBox {
            inset_x: 12.,
            inset_y: 4.,
            ..LabelBox::CONTENT
        },
    );
    w.insert(
        status_line,
        UiStyle {
            foreground: Some(rndr_ui::Token::Tertiary.into()),
            ..Default::default()
        },
    );
    w.insert(status_line, Hidden);
    caption(
        w,
        editors,
        "Select a project and file. Drafts remain here while you switch surfaces.",
    );
    w.insert_resource(Code {
        workbench: workbench::Workbench::new(panel),
        rows: Some(rows),
        explorer_filter: Some(filter),
        explorer_context: Some(explorer_context),
        explorer_actions: Some(explorer_actions),
        toolbar: Some(toolbar),
        editors: Some(editors),
        find_tools: Some(find_tools),
        status_line: Some(status_line),
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
pub(crate) fn can_rebind(w: &mut World) -> bool {
    if !can_deactivate(w) || !crate::terminal::can_rebind(w) {
        return false;
    }
    let Some(mut c) = w.resource_mut::<Code>().map(std::mem::take) else {
        return true;
    };
    capture(w, &mut c);
    let safe = c
        .buffers
        .values()
        .all(|b| !b.dirty() && !b.uncertain && b.remote.is_none())
        && c.sessions.is_empty()
        && c.pending.is_empty();
    w.insert_resource(c);
    safe
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
    w.insert(handle.editor, ContextTarget);
    c.views.insert(
        key.clone(),
        View {
            document,
            captured_revision: None,
            handle,
            id,
            find: None,
        },
    );
}
// The picker is navigation, never a change to the active editor/lease scope.
fn repository(c: &Code, work: &str) -> (String, String) {
    c.repositories
        .get(work)
        .map(|r| (r.0.clone(), r.1.clone()))
        .unwrap_or_else(|| (format!("work:{work}"), "Project".into()))
}
fn reset_explorer_query(w: &mut World, c: &mut Code) {
    c.explorer_query.clear();
    if let Some(e) = c.explorer_filter {
        w.get_mut::<Label>(e).unwrap().text.clear();
    }
}
fn render_projects(w: &mut World, parent: Entity, c: &mut Code) {
    let query = c.explorer_query.trim().to_lowercase();
    let scope = match &c.explorer_picker {
        Some(ExplorerPicker::Undertakings(id)) => Some(id.clone()),
        _ => None,
    };
    let heading = scope
        .as_ref()
        .map(|id| {
            let label = c
                .repositories
                .values()
                .find(|r| &r.0 == id)
                .map(|r| r.1.as_str())
                .unwrap_or("Project");
            format!("{label} / Choose an undertaking")
        })
        .unwrap_or_else(|| "Choose a project".into());
    caption(w, parent, &heading);
    let mut shown = HashSet::new();
    let mut count = 0;
    for p in c.projects.clone() {
        let (id, label) = repository(c, &p.id);
        if let Some(scope) = &scope {
            if &id != scope || (!query.is_empty() && !p.title.to_lowercase().contains(&query)) {
                continue;
            }
            let key = next();
            let h = controls::NavigationRow::mount(
                w,
                parent,
                UiId(key),
                ButtonProps::new(&p.title, HostIntent::Command(key)),
                controls::NavigationMeta {
                    selected: c.selected.as_ref() == Some(&p.id),
                    secondary: c
                        .repositories
                        .get(&p.id)
                        .map(|r| r.2.clone())
                        .filter(|s| !s.is_empty()),
                    ..Default::default()
                },
            )
            .unwrap();
            compact(w, h, Size::Fill);
            w.get_mut::<Layout>(h.root).unwrap().height = Size::Px(visual::ROW_HEIGHT);
            visual::trailing_meta(w, h);
            c.actions.insert(key, Action::Project(p.id));
        } else {
            if !query.is_empty()
                && !label.to_lowercase().contains(&query)
                && !id.to_lowercase().contains(&query)
            {
                continue;
            }
            if !shown.insert(id.clone()) {
                continue;
            }
            let key = next();
            let selected = c
                .selected
                .as_ref()
                .is_some_and(|work| repository(c, work).0 == id);
            let h = controls::NavigationRow::mount(
                w,
                parent,
                UiId(key),
                ButtonProps::new(&label, HostIntent::Command(key)),
                controls::NavigationMeta {
                    selected,
                    // Full identity disambiguates repositories with the same basename.
                    secondary: Some(id.clone()),
                    ..Default::default()
                },
            )
            .unwrap();
            compact(w, h, Size::Fill);
            w.get_mut::<Layout>(h.root).unwrap().height = Size::Px(visual::ROW_HEIGHT);
            visual::trailing_meta(w, h);
            rndr_ui::inputs::Tooltip::attach(w, h.root, &id).unwrap();
            c.actions.insert(key, Action::Repository(id));
        }
        count += 1;
    }
    if count == 0 && !c.projects.is_empty() {
        caption(w, parent, "No matches. Try a different filter.");
    }
}
fn render_explorer_context(w: &mut World, c: &mut Code) {
    let Some(parent) = c.explorer_context else {
        return;
    };
    for child in children_of(w, parent) {
        despawn(w, child);
    }
    // Fixed context stays above the scrolling tree. Both segments are real,
    // keyboard-accessible controls; full names remain available in tooltips.
    let breadcrumb = row(w, parent);
    w.remove::<Wrap>(breadcrumb);
    let selected = c.selected.clone();
    let (project, undertaking) = selected
        .as_ref()
        .map(|work| {
            let (_, label) = repository(c, work);
            let title = c
                .projects
                .iter()
                .find(|p| &p.id == work)
                .map(|p| p.title.clone())
                .unwrap_or_else(|| "Undertaking".into());
            (label, title)
        })
        .unwrap_or_else(|| ("Choose project".into(), "Choose undertaking".into()));
    for (label, action, enabled, description) in [
        (
            format!("{project}  {}", Icon::ChevronDown.scalar()),
            Action::ChooseProjects,
            true,
            "Switch project",
        ),
        (
            format!("/  {undertaking}  {}", Icon::ChevronDown.scalar()),
            Action::ChooseUndertakings,
            selected.is_some(),
            "Switch undertaking within project",
        ),
    ] {
        let id = next();
        let mut props = ButtonProps::new(&label, HostIntent::Command(id));
        props.enabled = enabled;
        props.variant = controls::ButtonVariant::Quiet;
        let h = controls::Button::mount(w, breadcrumb, UiId(id), props).unwrap();
        compact(w, h, Size::Fill);
        // The project hugs its name; the undertaking takes what remains, so
        // the separator sits beside the project rather than mid-row.
        if matches!(action, Action::ChooseProjects) {
            w.insert(
                h.root,
                visual::FitLabel {
                    label: h.label,
                    extra: 4.,
                    min: 48.,
                    max: 160.,
                },
            );
        }
        w.insert(
            h.root,
            Semantics {
                name: Some(format!("{description}: {label}")),
                ..Default::default()
            },
        );
        rndr_ui::inputs::Tooltip::attach(w, h.root, &format!("{description}: {label}")).unwrap();
        c.actions.insert(id, action);
    }
    if c.explorer_picker.is_some() && selected.is_some() {
        button(w, parent, c, "Back to files", Action::ClosePicker, true);
    }
}
fn render(w: &mut World, c: &mut Code) {
    crate::ui::chrome::clear_document_tabs(w, false);
    let (Some(rows), Some(toolbar)) = (c.rows, c.toolbar) else {
        return;
    };
    let turns = presented_turns(w);
    for p in [rows, toolbar] {
        for child in children_of(w, p) {
            despawn(w, child);
        }
    }
    c.actions.clear();
    c.context_targets.clear();
    if let Some(actions) = c.explorer_actions {
        for child in children_of(w, actions) {
            despawn(w, child);
        }
        let id = next();
        visual::icon_action(
            w,
            actions,
            id,
            Icon::RefreshCw,
            "Refresh projects",
            !c.pending.contains_key("projects"),
        );
        c.actions.insert(id, Action::Refresh);
    }
    render_explorer_context(w, c);
    let picking = c.explorer_picker.is_some() || c.selected.is_none();
    if let Some(filter) = c.explorer_filter {
        w.insert(
            filter,
            Placeholder::new(if picking {
                "Filter choices…"
            } else {
                "Filter files…"
            }),
        );
    }
    if c.pending.contains_key("projects") {
        caption(w, rows, "Loading projects…");
    }
    if picking {
        render_projects(w, rows, c);
    }
    if c.projects.is_empty() && c.started && !c.pending.contains_key("projects") {
        caption(w, rows, "No projects loaded. Refresh to retry.");
    }
    let project = c
        .selected
        .as_ref()
        .and_then(|work| c.projects.iter().find(|p| &p.id == work))
        .cloned();
    if let Some(work) = c.selected.clone().filter(|_| !picking) {
        if c.pending.contains_key(&format!("tree:{work}")) {
            caption(w, rows, "Loading source tree…");
        }
        let heading = text(w, rows, "FILES", Typography::Overline).unwrap();
        w.insert(
            heading,
            LabelBox {
                inset_x: 8.,
                inset_y: 8.,
                min_height: 0.,
            },
        );
        let files = c.files.get(&work).cloned().unwrap_or_default();
        let mut shown = HashSet::new();
        let query = c.explorer_query.trim().to_lowercase();
        if files.is_empty()
            && c.files.contains_key(&work)
            && !c.pending.contains_key(&format!("tree:{work}"))
        {
            caption(w, rows, "No source files in this workspace.");
        } else if !query.is_empty() && !files.iter().any(|p| p.to_lowercase().contains(&query)) {
            caption(w, rows, "No matching files. Try a different filter.");
        }
        for path in files {
            if !query.is_empty() && !path.to_lowercase().contains(&query) {
                continue;
            }
            let parts: Vec<_> = path.split('/').collect();
            let mut visible = true;
            for i in 1..parts.len() {
                let folder = parts[..i].join("/");
                if shown.insert(folder.clone()) {
                    let expanded =
                        !query.is_empty() || c.expanded.contains(&(work.clone(), folder.clone()));
                    let f = Folder {
                        work: &work,
                        path: &folder,
                        name: parts[i - 1],
                        depth: i - 1,
                        expanded,
                    };
                    folder_row(w, rows, c, f, &turns);
                }
                if query.is_empty() && !c.expanded.contains(&(work.clone(), folder)) {
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
                        "{}{} {}{}",
                        INDENT.repeat(parts.len() - 1),
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
    }
    // One header, as in medousa-home: location on the left, frequent actions as
    // icons, everything else in the overflow menu. Explanations live below it.
    let header = w.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(40.),
            gap: 2.,
            padding: 4.,
            ..Layout::row()
        },
        Align {
            items: Items::Center,
            ..Default::default()
        },
    ));
    set_parent(w, header, toolbar);
    // The file name must survive narrowing: the explorer already shows the
    // project, so a file location is its path, collapsed from the left.
    let segments: Vec<String> = match (&project, c.key()) {
        (_, Some(key)) => key.1.split('/').map(String::from).collect(),
        (Some(p), None) => vec![p.title.clone()],
        (None, None) => vec!["Select a project".into()],
    };
    let crumb =
        text(w, header, &segments.join("  ›  "), Typography::Label).expect("source location");
    w.insert(crumb, visual::PathCrumb(segments));
    w.get_mut::<Label>(crumb).unwrap().overflow = TextOverflow::Ellipsis;
    w.get_mut::<Layout>(crumb).unwrap().width = Size::Fill;
    w.insert(
        crumb,
        LabelBox {
            inset_x: 8.,
            ..LabelBox::CONTENT
        },
    );
    let editing = c
        .selected
        .as_ref()
        .is_some_and(|w| c.sessions.contains_key(w));
    let buffer = c.key().and_then(|k| c.buffers.get(&k)).map(|b| FileState {
        readonly: b.baseline.readonly,
        remote: b.remote.clone(),
        uncertain: b.uncertain,
        dirty: b.dirty(),
        saveable: b.saveable(),
        error: b.error.clone(),
    });
    let short = match &buffer {
        Some(b) if b.readonly => "Read-only",
        Some(b) if b.remote.is_some() => "Changed on workshop",
        Some(b) if b.uncertain => "Save unconfirmed",
        Some(b) if b.dirty => "Unsaved",
        _ if editing => "Editing",
        _ => "",
    };
    if !short.is_empty() {
        let status = text(w, header, short, Typography::Caption).unwrap();
        w.insert(
            status,
            LabelBox {
                inset_x: 8.,
                ..LabelBox::CONTENT
            },
        );
        w.get_mut::<Layout>(status).unwrap().width = Size::Hug;
    }
    crate::terminal::toolbar(w, header, c.selected.is_some());
    let commands = workbench::commands(c);
    let mut overflow = Vec::new();
    let has_editor = c.key().is_some_and(|k| c.views.contains_key(&k));
    for (label, action, enabled) in commands {
        let icon = match label {
            "Search" => Icon::Search,
            "Changes" => Icon::GitBranch,
            "Problems" => Icon::CircleAlert,
            _ => {
                overflow.push((label, action, enabled));
                continue;
            }
        };
        let id = next();
        visual::icon_action(w, header, id, icon, label, enabled);
        c.actions.insert(id, action);
    }
    overflow.push(("Find in file", Action::Find, has_editor));
    if let Some(key) = c.key()
        && buffer.is_some()
    {
        let saveable = buffer.as_ref().is_some_and(|b| b.saveable)
            && c.sessions
                .get(&key.0)
                .is_some_and(|s| s.checked.elapsed() < Duration::from_secs(45))
            && !c.busy("load", &key)
            && !c.busy("save", &key);
        if editing {
            let id = next();
            let label = if c.busy("save", &key) {
                "Saving…"
            } else {
                "Save"
            };
            visual::icon_action(w, header, id, Icon::Save, label, saveable);
            c.actions.insert(id, Action::Save);
        }
        let id = next();
        visual::icon_action(
            w,
            header,
            id,
            Icon::RefreshCw,
            "Check latest",
            !c.busy("load", &key) && !c.busy("save", &key),
        );
        c.actions.insert(id, Action::Check);
    }
    if let (Some(p), false) = (&project, editing) {
        let id = next();
        visual::icon_action(
            w,
            header,
            id,
            Icon::Pencil,
            if p.resume {
                "Continue editing"
            } else {
                "Begin editing"
            },
            (p.begin || p.resume)
                && !c.pending.contains_key(&format!("begin:{}", p.id))
                && !c.begin_blocked.contains(&p.id),
        );
        c.actions.insert(id, Action::Begin);
    }
    let id = next();
    let more = visual::icon_action(w, header, id, Icon::Ellipsis, "More actions", true);
    c.actions.insert(id, Action::More);
    c.more = Some((more.root, overflow));
    visual::rule(w, toolbar);
    if let Some(key) = c.key() {
        if let Some(b) = buffer {
            if b.readonly {
                notice(
                    w,
                    toolbar,
                    "Read-only preview · binary, lossy or truncated content",
                );
            } else if b.uncertain {
                notice(
                    w,
                    toolbar,
                    "Save not confirmed · check latest before another save",
                );
            }
            if let Some(remote) = b.remote {
                notice(
                    w,
                    toolbar,
                    &format!(
                        "Latest daemon content (bounded comparison):\n{}",
                        remote.content.chars().take(1200).collect::<String>()
                    ),
                );
                notice(
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
            if !b.error.is_empty() {
                notice(w, toolbar, &b.error);
            }
        } else {
            notice(w, toolbar, "Loading source…");
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
        notice(w, toolbar, &c.status);
    }
    workbench::retain_panel(w, c);

    if let Some(parent) = c.tabs {
        w.insert(parent, Hidden);
    }
    if let (Some(parent), Some(key)) = (c.tabs, c.key()) {
        if c.views.contains_key(&key) {
            w.remove::<Hidden>(parent);
            let specs = c.tab_specs();
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
        if let Some(bar) = v.find {
            if selected.as_ref() == Some(key) && c.find_open.contains(key) {
                w.remove::<Hidden>(bar.root);
            } else {
                w.insert(bar.root, Hidden);
            }
        }
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
    if let Some(query) = c
        .explorer_filter
        .and_then(|e| w.get::<Label>(e))
        .map(|l| l.text.clone())
    {
        if c.explorer_query != query {
            c.explorer_query = query;
            c.redraw = true;
        }
    }
    if crate::surfaces::active(w) == crate::surfaces::SurfaceId::Code {
        context_events(w, &mut c);
    }
    for event in events::<TabStripEvent>(w).to_vec() {
        if !c.strip.is_some_and(|s| s.root == event.strip) {
            continue;
        }
        // Requests are idempotent, scoped to currently open tabs, and never
        // change the canonical document while an IME transaction is active.
        c.redraw = true;
        if composing(w, &c) {
            continue;
        }
        let owned_key = |c: &Code, id: TabId| {
            c.open_tabs
                .iter()
                .find(|k| {
                    Some(&k.0) == c.selected.as_ref()
                        && c.views.get(*k).is_some_and(|v| v.id.0 == id.0)
                })
                .cloned()
        };
        match event.kind {
            TabStripEventKind::Activated(id) => {
                if let Some(key) = owned_key(&c, id) {
                    c.current.insert(key.0, key.1);
                    clear_focus(w, &c);
                }
            }
            TabStripEventKind::CloseRequested(id) => {
                if let Some(key) = owned_key(&c, id) {
                    let index = c.open_tabs.iter().position(|k| k == &key).unwrap();
                    c.open_tabs.remove(index);
                    if c.key().as_ref() == Some(&key) {
                        let next = c.open_tabs[index..]
                            .iter()
                            .find(|k| k.0 == key.0)
                            .or_else(|| c.open_tabs[..index].iter().rev().find(|k| k.0 == key.0))
                            .cloned();
                        if let Some(next) = next {
                            c.current.insert(next.0, next.1);
                        } else {
                            c.current.remove(&key.0);
                        }
                        clear_focus(w, &c);
                    }
                    // A clean file gives everything back; reopening loads it
                    // again. A draft keeps its editor and undo history.
                    let (w0, p0) = (&key.0, &key.1);
                    let busy = c.pending.contains_key(&format!("load:{w0}:{p0}"))
                        || c.pending.contains_key(&format!("save:{w0}:{p0}"));
                    let clean = c
                        .buffers
                        .get(&key)
                        .is_some_and(|b| !b.dirty() && !b.uncertain && b.remote.is_none());
                    if clean && !busy {
                        if let Some(v) = c.views.remove(&key) {
                            if let Some(bar) = v.find {
                                despawn(w, bar.root);
                            }
                            despawn(w, v.handle.root);
                            if let Some(docs) = w.resource_mut::<SourceDocuments>() {
                                docs.close(v.document);
                            }
                        }
                        c.buffers.remove(&key);
                        c.status = format!("Closed {}.", key.1);
                    } else {
                        c.status = format!(
                            "Closed {} · draft and undo history retained; reopen from Explorer.",
                            key.1
                        );
                    }
                }
            }
            TabStripEventKind::Reordered(moved) => {
                if let Some(specs) =
                    reorder_tabs(&c.tab_specs(), moved.id, moved.target, moved.place)
                {
                    let order: Vec<_> = specs.iter().filter_map(|s| owned_key(&c, s.id)).collect();
                    let mut order = order.into_iter();
                    for key in &mut c.open_tabs {
                        if Some(&key.0) == c.selected.as_ref() && c.views.contains_key(key) {
                            *key = order.next().expect("RNDR reorder preserves membership");
                        }
                    }
                }
            }
            // Multiple editor groups require an explicit host layout contract.
            TabStripEventKind::Moved(_) | TabStripEventKind::SplitRequested(_) => {}
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
    sync_status_line(w, &c);
    *w.resource_mut::<Code>().unwrap() = c;
    for update in deferred {
        apply(w, update);
    }
    let selected = w.resource::<Code>().and_then(|c| c.selected.clone());
    crate::terminal::sync(w, selected);
}
/// `Ln 12, Col 4 · Rust` for the open file; hidden with no file open.
fn sync_status_line(w: &mut World, c: &Code) {
    let Some(line) = c.status_line else {
        return;
    };
    let text = c.key().and_then(|key| {
        let view = c.views.get(&key)?;
        let doc = w.resource::<SourceDocuments>()?.get(view.document)?;
        let source = doc.text();
        let head = doc.selection().head.min(source.len());
        let before = source.get(..head)?;
        let row = before.matches('\n').count() + 1;
        let col = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
        Some(match language(&key.1) {
            Some(language) => format!("Ln {row}, Col {col} · {language}"),
            None => format!("Ln {row}, Col {col}"),
        })
    });
    match text {
        Some(text) => {
            w.remove::<Hidden>(line);
            let label = w.get_mut::<Label>(line).unwrap();
            if label.text != text {
                label.text = text;
            }
        }
        None => w.insert(line, Hidden),
    }
}
fn language(path: &str) -> Option<&'static str> {
    let file = path.rsplit('/').next().unwrap_or(path);
    let ext = file.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    Some(match (file, ext.as_deref()) {
        ("Dockerfile", _) => "Dockerfile",
        ("Makefile", _) => "Makefile",
        (_, Some("rs")) => "Rust",
        (_, Some("ts" | "tsx")) => "TypeScript",
        (_, Some("js" | "jsx" | "mjs" | "cjs")) => "JavaScript",
        (_, Some("svelte")) => "Svelte",
        (_, Some("py")) => "Python",
        (_, Some("go")) => "Go",
        (_, Some("swift")) => "Swift",
        (_, Some("kt" | "kts")) => "Kotlin",
        (_, Some("java")) => "Java",
        (_, Some("c" | "h")) => "C",
        (_, Some("cc" | "cpp" | "hpp")) => "C++",
        (_, Some("md")) => "Markdown",
        (_, Some("json")) => "JSON",
        (_, Some("toml")) => "TOML",
        (_, Some("yaml" | "yml")) => "YAML",
        (_, Some("html")) => "HTML",
        (_, Some("css" | "postcss")) => "CSS",
        (_, Some("sh" | "bash" | "zsh")) => "Shell",
        (_, Some("sql")) => "SQL",
        _ => return None,
    })
}
fn context_events(w: &mut World, c: &mut Code) {
    use rndr_ui::menu::{MenuAt, MenuItem, MenuPicked, open_menu};
    for event in events::<MenuPicked>(w).to_vec() {
        if !c
            .context_menu
            .as_ref()
            .is_some_and(|(menu, _, _)| menu.root == event.menu)
        {
            continue;
        }
        let (_, key, mut actions) = c.context_menu.take().unwrap();
        if c.selected.as_ref() != Some(&key.0) || composing(w, c) {
            continue;
        }
        if matches!(
            actions.get(&event.item),
            Some(Action::Workbench(workbench::Action::Language(_)))
        ) && c.key().as_ref() != Some(&key)
        {
            continue;
        }
        if let Some(action) = actions.remove(&event.item) {
            // Route through the same authority/draft-safe action adapter as toolbar controls.
            let id = next();
            c.actions.insert(id, action);
            let saved = std::mem::take(c);
            *w.resource_mut::<Code>().unwrap() = saved;
            self::action(w, &HostIntent::Command(id));
            *c = std::mem::take(w.resource_mut::<Code>().unwrap());
        }
    }
    for event in events::<SecondaryPress>(w).to_vec() {
        let key = c.context_targets.get(&event.entity).cloned().or_else(|| {
            c.views
                .iter()
                .find(|(_, v)| v.handle.editor == event.entity)
                .map(|(key, _)| key.clone())
        });
        let Some(key) = key.filter(|key| c.selected.as_ref() == Some(&key.0)) else {
            continue;
        };
        if composing(w, c) {
            continue;
        }
        let current = c.key().as_ref() == Some(&key);
        let mut actions = HashMap::new();
        let mut entries = Vec::new();
        for (label, action, enabled) in [
            ("Open file", Action::File(key.1.clone()), true),
            (
                "Copy Relative Path",
                Action::CopyRelative(key.clone()),
                true,
            ),
            ("Reveal in Explorer", Action::Reveal(key.clone()), true),
            (
                "Go to Definition",
                Action::Workbench(workbench::Action::Language(workbench::Feature::Definition)),
                current,
            ),
            (
                "Find Uses",
                Action::Workbench(workbench::Action::Language(workbench::Feature::References)),
                current,
            ),
        ] {
            let id = next();
            entries.push(MenuItem::new(id, label).enabled(enabled).into());
            actions.insert(id, action);
        }
        if let Ok(menu) = open_menu(w, event.entity, MenuAt::Point(event.at), &entries) {
            c.context_menu = Some((menu, key, actions));
        }
    }
}
/// Surface-scoped keyboard integration. The shared host chooses the active
/// surface and key chord; this adapter never installs another input dispatcher.
pub fn keyboard(w: &mut World) {
    if crate::terminal::focused(w) {
        return;
    }
    if crate::surfaces::active(w) != crate::surfaces::SurfaceId::Code
        || w.resource::<Code>().is_none_or(|c| composing(w, c))
        || w.resource::<Input>()
            .is_some_and(|i| !i.ime_preedit.is_empty() || !i.ime_commit.is_empty())
    {
        return;
    }
    let platform = w
        .resource::<Platform>()
        .copied()
        .unwrap_or_else(Platform::host);
    let keys = w
        .resource_mut::<Input>()
        .map(|input| std::mem::take(&mut input.keys))
        .unwrap_or_default();
    let mut kept = Vec::new();
    let first_pressed = keys.iter().position(|event| !event.released);
    let mut consumed_first = false;
    for (index, event) in keys.into_iter().enumerate() {
        let command = (!event.released && !event.repeat && !event.synthetic)
            .then(|| {
                [('f', "find"), ('p', "quick-open"), ('s', "save")]
                    .into_iter()
                    .find(|(ch, _)| {
                        Keystroke::character(*ch)
                            .primary()
                            .matches(&event, platform)
                    })
                    .map(|(_, command)| command)
            })
            .flatten();
        if !command.is_some_and(|command| shortcut(w, command)) {
            kept.push(event);
        } else if Some(index) == first_pressed {
            consumed_first = true;
        }
    }
    if let Some(input) = w.resource_mut::<Input>() {
        // The runner derives the legacy shim from the first pressed event.
        // Do not erase an unrelated earlier key when consuming a later chord.
        if consumed_first {
            input.key = kept
                .iter()
                .find(|event| !event.released)
                .and_then(|event| rndr_core::legacy_key(event, platform));
        }
        input.keys = kept;
    }
}
pub fn shortcut(w: &mut World, command: &str) -> bool {
    let Some(c) = w.resource::<Code>() else {
        return false;
    };
    if composing(w, c) {
        return false;
    }
    let requested = match command {
        "find" if c.key().is_some_and(|key| c.views.contains_key(&key)) => Action::Find,
        "quick-open" if c.selected.is_some() => Action::Workbench(workbench::Action::QuickOpen),
        "save" if c.key().is_some() => Action::Save,
        _ => return false,
    };
    let id = next();
    w.resource_mut::<Code>()
        .unwrap()
        .actions
        .insert(id, requested);
    action(w, &HostIntent::Command(id))
}
pub fn action(w: &mut World, host: &HostIntent) -> bool {
    if crate::terminal::action(w, host) {
        return true;
    }
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
        Action::More => {
            use rndr_ui::menu::{MenuAt, MenuItem, open_menu};
            if let (Some((anchor, commands)), Some(work)) = (c.more.clone(), c.selected.clone()) {
                let mut actions = HashMap::new();
                let mut entries = Vec::new();
                for (label, action, enabled) in commands {
                    let id = next();
                    entries.push(MenuItem::new(id, label).enabled(enabled).into());
                    actions.insert(id, action);
                }
                let key = c.key().unwrap_or((work, String::new()));
                if let Ok(menu) = open_menu(w, anchor, MenuAt::Below, &entries) {
                    c.context_menu = Some((menu, key, actions));
                }
            }
        }
        Action::Workbench(a) => workbench::action(w, &mut c, a),
        Action::Find => {
            if !composing(w, &c) {
                if let Some(key) = c.key() {
                    if let (Some(parent), Some(view)) = (c.find_tools, c.views.get_mut(&key)) {
                        if view.find.is_none() {
                            view.find = rndr_ui::source_tools::FindBar::mount(
                                w,
                                parent,
                                view.handle.editor,
                            )
                            .ok();
                        }
                        if let Some(bar) = view.find {
                            if !c.find_open.remove(&key) {
                                c.find_open.insert(key);
                                if let Some(focus) = w.resource_mut::<Focus>() {
                                    focus.entity = Some(bar.query);
                                }
                            }
                        }
                    }
                }
            }
        }
        Action::CopyRelative(key) => {
            if c.selected.as_ref() == Some(&key.0) {
                if let Some(clipboard) = w.resource_mut::<Clipboard>() {
                    clipboard.text = key.1;
                    clipboard.html.clear();
                    clipboard.html_text.clear();
                    clipboard.dirty = true;
                }
            }
        }
        Action::Reveal(key) => {
            if c.selected.as_ref() == Some(&key.0) && !composing(w, &c) {
                c.explorer_query.clear();
                if let Some(e) = c.explorer_filter {
                    w.get_mut::<Label>(e).unwrap().text.clear();
                }
                let parts: Vec<_> = key.1.split('/').collect();
                for i in 1..parts.len() {
                    c.expanded.insert((key.0.clone(), parts[..i].join("/")));
                }
                c.status = format!("Revealed {} in Explorer", key.1);
            }
        }
        Action::Refresh => {
            c.enqueue(Operation::Projects);
            if let Some(work) = c.selected.clone() {
                c.enqueue(Operation::Tree(work));
            }
        }
        Action::ChooseProjects => {
            c.explorer_picker = Some(ExplorerPicker::Projects);
            reset_explorer_query(w, &mut c);
        }
        Action::ChooseUndertakings => {
            if let Some(work) = &c.selected {
                c.explorer_picker = Some(ExplorerPicker::Undertakings(repository(&c, work).0));
                reset_explorer_query(w, &mut c);
            }
        }
        Action::ClosePicker => {
            c.explorer_picker = None;
            reset_explorer_query(w, &mut c);
        }
        Action::Repository(repository) => {
            c.explorer_picker = Some(ExplorerPicker::Undertakings(repository));
            reset_explorer_query(w, &mut c);
        }
        Action::Project(work) => {
            if !composing(w, &c) {
                c.selected = Some(work.clone());
                c.explorer_picker = None;
                reset_explorer_query(w, &mut c);
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
                    c.open_tab(&key);
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
                    c.repositories = items
                        .iter()
                        .filter_map(|p| {
                            let work = p["id"].as_str()?;
                            let repo = (p["target"]["kind"].as_str() == Some("git"))
                                .then(|| p["target"]["repo_path"].as_str())
                                .flatten()
                                .map(str::trim)
                                .filter(|s| !s.is_empty());
                            // Unknown repository identity remains work-scoped: never merge by title.
                            let group = repo
                                .map(str::to_owned)
                                .unwrap_or_else(|| format!("work:{work}"));
                            let label = repo
                                .and_then(|s| {
                                    s.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next()
                                })
                                .unwrap_or("Project");
                            Some((
                                work.to_owned(),
                                (
                                    group,
                                    label.to_owned(),
                                    p["human_phase"].as_str().unwrap_or("").replace('_', " "),
                                ),
                            ))
                        })
                        .collect();
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
