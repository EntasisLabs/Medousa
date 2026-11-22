//! Retained daemon-backed notes. SourceDocuments owns editor history;
//! VaultBufferCore owns unsaved buffers and opaque daemon save baselines.
use crate::native_style as visual;
use medousa_app_core::{NoteBufferFact, VaultBufferCommand, VaultBufferCore};
use medousa_sdk::MedousaClient;
use medousa_types::{
    VaultNoteContentResponse, VaultNoteSummary, VaultNotesListResponse, VaultNotesQuery,
    VaultPutQuery, VaultWriteResponse,
};
use rndr_core::{
    editing::{DocumentId, EditOrigin, Revision, SourceBatch, SourceChange, SourceDocuments},
    *,
};
use rndr_markdown_ui::{
    MarkdownCommand, MarkdownDocuments, MarkdownId, MarkdownSurface, MarkdownSurfaceHandle,
    MarkdownSurfaceProps,
};
use rndr_ui::{
    Typography, UiId, UiStyle,
    controls::HostIntent,
    editing::{EditorSurface, EditorSurfaceHandle, EditorSurfaceProps},
    tabs::{TabId, TabSpec, TabStrip, TabStripEvent, TabStripEventKind, TabStripProps},
    text,
    tree::{NodeId, TreeEvent, TreeEventKind, TreeHandle, TreeNode, TreeProps, VirtualTree},
};
use std::{
    collections::HashMap,
    sync::atomic::{AtomicU64, Ordering},
};
#[path = "notes_management.rs"]
mod management;

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
    Manage(management::Command),
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
    Managed(serde_json::Value),
    List(VaultNotesListResponse),
    Loaded(VaultNoteContentResponse),
    Saved(VaultWriteResponse),
}
#[derive(Clone)]
enum Action {
    Manage(management::Action),
    Refresh,
    More,
    Open(String),
    Save,
    Check,
    Acknowledge,
    ToggleSource,
    Write,
    Read,
    Format(MarkdownCommand),
    Find,
    Browse(Browse),
    Picker(Picker, Entity),
    Group(Option<&'static str>),
    SearchList,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Browse {
    All,
    Folders,
    #[default]
    Recent,
}
#[path = "notes_sidebar.rs"]
mod sidebar;
use sidebar::{Picker, open_picker, sidebar_header};
struct View {
    document: DocumentId,
    captured_revision: Option<Revision>,
    handle: EditorSurfaceHandle,
    preview: Entity,
    rich_document: MarkdownId,
    rich: MarkdownSurfaceHandle,
    rich_revision: u64,
    tab: u64,
    find: Option<rndr_ui::source_tools::FindBar>,
    find_open: bool,
}
#[derive(Default)]
struct Notes {
    management: management::Management,
    toolbar: Option<Entity>,
    formatting: Option<Entity>,
    find_tools: Option<Entity>,
    browse: Browse,
    group: Option<&'static str>,
    roots: Vec<medousa_types::VaultRootView>,
    menu: Option<(rndr_ui::menu::MenuHandle, HashMap<u64, Action>)>,
    searching: bool,
    search_row: Option<Entity>,
    header: Option<Entity>,
    recent: Vec<String>,
    status: Option<Entity>,
    live: bool,
    tabs: Option<Entity>,
    tabstrip: Option<Entity>,
    notice: Option<Entity>,
    source: bool,
    editors: Option<Entity>,
    search: Option<Entity>,
    rows: Option<Entity>,
    library: Option<Entity>,
    tree: Option<TreeHandle>,
    tree_ids: HashMap<String, NodeId>,
    tree_paths: HashMap<NodeId, String>,
    expanded: std::collections::HashSet<String>,
    core: VaultBufferCore,
    views: HashMap<String, View>,
    tab_order: Vec<String>,
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
        if self.management.pending.is_some()
            || self.saves.contains_key(&path)
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
        gap: 0.,
        padding: 0.,
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
    visual::document_action(world, parent, id, label, enabled);
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
    let sidebar = column(world, root, Size::Px(288.), Size::Fill);
    visual::navigation_frame(world, sidebar);
    world.insert(sidebar, visual::LibraryBreadcrumbs);
    let header = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Hug,
        ..Layout::row()
    },));
    world.insert(
        header,
        Align {
            items: Items::Center,
            justify: Justify::Start,
        },
    );
    set_parent(world, header, sidebar);
    let search_row = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Px(36.),
        ..Layout::row()
    },));
    set_parent(world, search_row, sidebar);
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
        Placeholder::new("Search loaded notes…"),
        Semantics {
            name: Some("Filter loaded notes".into()),
            ..Default::default()
        },
    ));
    set_parent(world, search, search_row);
    visual::plane(world, search, visual::rail());
    world.get_mut::<UiStyle>(search).unwrap().typography = Some(Typography::Label);
    world.insert(search, visual::SemanticBorder(visual::stroke()));
    world.insert(search, Radius(visual::CONTROL_RADIUS));
    world.insert(
        search,
        Border {
            color: visual::color(world, visual::stroke()),
            width: 1.,
        },
    );

    let manager = column(world, sidebar, Size::Fill, Size::Hug);
    world.insert(manager, Hidden);
    let library = column(world, sidebar, Size::Fill, Size::Fill);
    let rows = column(world, sidebar, Size::Fill, Size::Hug);
    let content = column(world, root, Size::Fill, Size::Fill);
    visual::flat(world, content, visual::canvas());
    world.insert(content, ResolvedColor);
    let tabs = column(world, content, Size::Fill, Size::Px(34.));
    *world.get_mut::<Layout>(tabs).unwrap() = Layout {
        width: Size::Fill,
        height: Size::Px(34.),
        gap: 2.,
        ..Layout::row()
    };
    let toolbar = column(world, content, Size::Fill, Size::Px(38.));
    *world.get_mut::<Layout>(toolbar).unwrap() = Layout {
        width: Size::Fill,
        height: Size::Hug,
        min_height: 38.,
        padding: 4.,
        gap: 4.,
        ..Layout::row()
    };
    // Measured actions flow onto another line instead of clipping or eliding.
    world.insert(toolbar, Wrap { line_gap: 4. });
    let formatting = column(world, content, Size::Fill, Size::Px(34.));
    *world.get_mut::<Layout>(formatting).unwrap() = Layout {
        width: Size::Fill,
        height: Size::Px(34.),
        gap: 4.,
        padding: 4.,
        ..Layout::row()
    };
    world.insert(formatting, Hidden);
    let find_tools = column(world, content, Size::Fill, Size::Hug);
    let notice = column(world, content, Size::Fill, Size::Hug);
    let editors = column(world, content, Size::Fill, Size::Fill);
    caption(
        world,
        editors,
        "Select a note to begin. Drafts stay here when you switch surfaces.",
    );
    let status = column(world, content, Size::Fill, Size::Px(28.));
    world.get_mut::<Layout>(status).unwrap().padding = 6.;
    world.insert_resource(Notes {
        management: management::Management::new(manager),
        toolbar: Some(toolbar),
        formatting: Some(formatting),
        find_tools: Some(find_tools),
        status: Some(status),
        tabs: Some(tabs),
        notice: Some(notice),
        editors: Some(editors),
        search: Some(search),
        rows: Some(rows),
        header: Some(header),
        search_row: Some(search_row),
        library: Some(library),
        core: VaultBufferCore::new("connected-workshop"),
        redraw: true,
        ..Default::default()
    });
    tick(world);
}
// Frontmatter is storage metadata, not document prose. Keep source bytes intact.
fn document_body(source: &str) -> &str {
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    let mut lines = source.split_inclusive('\n');
    if lines.next().is_some_and(|line| line.trim_end() == "---") {
        let mut offset = source.find('\n').map_or(source.len(), |i| i + 1);
        for line in lines {
            offset += line.len();
            if matches!(line.trim_end(), "---" | "...") {
                return &source[offset..];
            }
        }
    }
    source
}
fn note_statistics(source: &str) -> String {
    let body = document_body(source);
    let words = body.split_whitespace().count();
    // Match Svelte's JS string length, including supplementary Unicode pairs.
    let characters = body.encode_utf16().count();
    let minutes = words.div_ceil(200);
    if words == 0 && characters == 0 {
        "Empty note".into()
    } else {
        format!(
            "{words} {} · ~{minutes} min read · {characters} chars",
            if words == 1 { "word" } else { "words" }
        )
    }
}
fn can_deactivate_views(world: &World, n: &Notes) -> bool {
    n.views.values().all(|v| {
        world
            .resource::<SourceDocuments>()
            .and_then(|d| d.get(v.document))
            .is_none_or(|d| !d.is_composing())
            && world
                .resource::<MarkdownDocuments>()
                .and_then(|d| d.get(v.rich_document))
                .is_none_or(|d| !d.is_composing())
    })
}
/// Surface navigation must not interrupt native IME composition.
pub fn can_deactivate(world: &World) -> bool {
    world
        .resource::<Notes>()
        .is_none_or(|n| can_deactivate_views(world, n))
}
pub fn activate(world: &mut World) {
    let Some(n) = world.resource_mut::<Notes>() else {
        return;
    };
    if !n.started {
        n.started = true;
        management::initialize(n);
    }
}
fn capture(world: &mut World, n: &mut Notes) {
    for (path, v) in &mut n.views {
        if let Some(d) = world
            .resource::<SourceDocuments>()
            .and_then(|s| s.get(v.document))
        {
            if v.captured_revision == Some(d.revision()) {
                continue;
            }
            v.captured_revision = Some(d.revision());
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
// Rich mode edits only the body. Opaque metadata/BOM/newlines preceding it are
// retained verbatim; the shared rich engine never owns frontmatter serialization.
fn capture_rich(world: &mut World, n: &mut Notes) {
    for (path, v) in &mut n.views {
        let Some(session) = world
            .resource::<MarkdownDocuments>()
            .and_then(|d| d.get(v.rich_document))
        else {
            continue;
        };
        if session.revision() == v.rich_revision {
            continue;
        }
        v.rich_revision = session.revision();
        let body = session.bytes().to_owned();
        let Some(buffer) = n.core.get(path) else {
            continue;
        };
        let offset = buffer.content.len() - document_body(&buffer.content).len();
        let content = format!("{}{}", &buffer.content[..offset], body);
        if content == buffer.content {
            continue;
        }
        let _ = n.core.dispatch(VaultBufferCommand::ApplyEdit {
            path: path.clone(),
            content: content.clone(),
            title: None,
        });
        // A mode handoff is one explicit host edit, not a reload that erases
        // source undo. Each shared editor retains its own history and caret.
        if let Some(d) = world
            .resource_mut::<SourceDocuments>()
            .and_then(|d| d.get_mut(v.document))
        {
            let _ = d.commit(SourceBatch {
                document: v.document,
                base: d.revision(),
                origin: EditOrigin::Host(next()),
                changes: vec![SourceChange {
                    range: 0..d.text().len(),
                    replacement: content,
                }],
            });
            v.captured_revision = Some(d.revision());
        }
        n.redraw = true;
    }
}
fn sync_rich(world: &mut World, n: &mut Notes, path: &str) {
    let (Some(buffer), Some(v)) = (n.core.get(path), n.views.get_mut(path)) else {
        return;
    };
    let body = document_body(&buffer.content);
    if let Some(d) = world
        .resource_mut::<MarkdownDocuments>()
        .and_then(|d| d.get_mut(v.rich_document))
    {
        if d.bytes() != body && !d.is_composing() {
            let _ = d.edit(
                rndr_markdown::Command::Replace {
                    start: 0,
                    end: d.bytes().len(),
                    text: body.to_owned(),
                },
                next(),
            );
        }
        v.rich_revision = d.revision();
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
// RNDR VirtualTree owns realization, scrolling, keyboard/typeahead and a11y.
fn library_model(n: &mut Notes) -> (Vec<TreeNode>, Vec<NodeId>) {
    let mut folders = std::collections::BTreeSet::new();
    let mut matches = Vec::new();
    let query = n.filter.to_lowercase();
    // Daemon paths are identity. Defensively deduplicate before mounting the
    // shared tree, including malformed/repeated pages or fixture input.
    let mut seen = std::collections::HashSet::new();
    for item in &n.list {
        if n.group.is_some_and(|group| !item.path.starts_with(group))
            || !seen.insert(item.path.clone())
        {
            continue;
        }
        if !format!("{} {}", item.title, item.path)
            .to_lowercase()
            .contains(&query)
        {
            continue;
        }
        let mut parent = String::new();
        let segments: Vec<_> = item.path.split('/').collect();
        for segment in segments.iter().take(if n.browse == Browse::Folders {
            segments.len().saturating_sub(1)
        } else {
            0
        }) {
            if !parent.is_empty() {
                parent.push('/');
            }
            parent.push_str(segment);
            folders.insert(parent.clone());
        }
        matches.push(item.clone());
    }
    let mut nodes = Vec::new();
    let mut expanded = Vec::new();
    for folder in folders {
        let id = *n
            .tree_ids
            .entry(format!("folder:{folder}"))
            .or_insert_with(|| NodeId(next()));
        let mut node =
            TreeNode::new(id.0, folder.rsplit('/').next().unwrap_or(&folder)).expandable(true);
        if let Some((parent, _)) = folder.rsplit_once('/') {
            node.parent = n.tree_ids.get(&format!("folder:{parent}")).copied();
        }
        if !query.is_empty() || n.expanded.contains(&folder) {
            expanded.push(id);
        }
        nodes.push(node);
    }
    n.tree_paths.clear();
    if n.browse == Browse::Recent {
        // Svelte vaultBrowseController::recentNotesList: visited first, then
        // descending modification time. Only the loaded root catalog is eligible.
        matches.sort_by(|a, b| {
            let rank = |p: &str| n.recent.iter().position(|r| r == p).unwrap_or(usize::MAX);
            rank(&a.path)
                .cmp(&rank(&b.path))
                .then_with(|| b.modified_at_utc.cmp(&a.modified_at_utc))
                .then_with(|| a.path.cmp(&b.path))
        });
        matches.truncate(40);
    } else if n.browse == Browse::All {
        matches.sort_by(|a, b| {
            b.modified_at_utc
                .cmp(&a.modified_at_utc)
                .then_with(|| a.path.cmp(&b.path))
        });
    } else {
        matches.sort_by(|a, b| a.path.cmp(&b.path));
    }
    let now = chrono::Utc::now();
    for item in matches {
        let id = *n
            .tree_ids
            .entry(format!("note:{}", item.path))
            .or_insert_with(|| NodeId(next()));
        let mut node = TreeNode::new(
            id.0,
            if item.title.is_empty() {
                item.path
                    .rsplit('/')
                    .next()
                    .unwrap_or(&item.path)
                    .to_owned()
            } else {
                item.title.clone()
            },
        );
        if n.browse == Browse::Folders {
            if let Some((parent, _)) = item.path.rsplit_once('/') {
                node.parent = n.tree_ids.get(&format!("folder:{parent}")).copied();
            }
        }
        if n.core.get(&item.path).is_some_and(|b| b.dirty) {
            node.badge = "●".into();
        }
        if n.browse == Browse::Folders {
            node.secondary = item.path.clone();
        } else {
            let age = now - item.modified_at_utc;
            node.secondary = match age.num_minutes() {
                0 => "now".into(),
                m @ 1..=59 => format!("{m}m ago"),
                60..=1439 => format!("{}h ago", age.num_hours()),
                1440..=10079 => format!("{}d ago", age.num_days()),
                _ => item.modified_at_utc.format("%b %-d").to_string(),
            };
            node.secondary.push_str(" · ");
            node.secondary.push_str(if item.kind.trim().is_empty() {
                "Note"
            } else {
                item.kind.trim()
            });
        }
        n.tree_paths.insert(id, item.path);
        nodes.push(node);
    }
    (nodes, expanded)
}
fn render(world: &mut World, n: &mut Notes) {
    crate::ui::chrome::clear_document_tabs(world, true);
    let (Some(rows), Some(toolbar)) = (n.rows, n.toolbar) else {
        return;
    };
    let notice = n.notice.unwrap();
    let tabs = n.tabs.unwrap();
    let formatting = n.formatting.unwrap();
    let status = n.status.unwrap();
    let header = n.header.unwrap();
    for parent in [header, rows, toolbar, notice, formatting, status] {
        for c in children_of(world, parent) {
            despawn(world, c);
        }
    }
    n.actions.clear();
    sidebar_header(world, n, header);
    if n.list_pending.is_some() {
        caption(world, rows, "Loading vault notes…");
    }
    if !n.list_error.is_empty() {
        caption(world, rows, &n.list_error);
    }
    let (nodes, expanded) = library_model(n);
    let matches = n.tree_paths.len();
    if let Some(parent) = n.library {
        let handle = VirtualTree::mount(
            world,
            parent,
            UiId(0x4e4f_5452),
            TreeProps {
                name: "Notes library".into(),
                empty: if n.list_pending.is_some() {
                    "Loading notes…"
                } else {
                    "No matching notes"
                }
                .into(),
                row_height: 34.,
                ..TreeProps::new("Notes library")
            },
            &nodes,
            &expanded,
        )
        .expect("notes tree model has unique daemon paths");
        if let Some(layout) = world.get_mut::<Layout>(handle.root) {
            layout.width = Size::Fill;
            layout.height = Size::Fill;
        }
        let selected = n.selected.as_ref().and_then(|path| {
            n.tree_paths
                .iter()
                .find_map(|(id, p)| (p == path).then_some(*id))
        });
        if handle.selected(world) != selected {
            let _ = handle.select(world, selected);
        }
        n.tree = Some(handle);
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
        caption(world, rows, "Showing loaded notes only.");
    }
    if n.live && n.selected.is_some() {
        world.remove::<Hidden>(formatting);
    } else {
        world.insert(formatting, Hidden);
    }
    let Some(path) = n.selected.clone() else {
        caption(world, toolbar, "Notes");
        return;
    };
    sync_rich(world, n, &path);
    if n.live {
        for (label, command) in [
            ("Bold", MarkdownCommand::Strong),
            ("Italic", MarkdownCommand::Emphasis),
            ("Heading", MarkdownCommand::Heading(1)),
            ("List", MarkdownCommand::Bullet),
            ("Quote", MarkdownCommand::Quote),
            ("Undo", MarkdownCommand::Undo),
            ("Redo", MarkdownCommand::Redo),
        ] {
            button(
                world,
                formatting,
                n,
                label,
                Action::Format(command),
                can_deactivate_views(world, n),
            );
        }
    }
    // Retained open documents are navigation, not a pile of form controls.
    let open = n.tab_order.clone();
    if let Some(selected) = n.views.get(&path) {
        let specs: Vec<_> = open
            .iter()
            .map(|p| {
                let title = n
                    .core
                    .get(p)
                    .map(|b| b.title.as_str())
                    .filter(|t| !t.is_empty())
                    .unwrap_or_else(|| p.rsplit('/').next().unwrap_or(p));
                TabSpec::new(n.views[p].tab, title)
                    .closable(false)
                    .icon(Icon::Book)
                    .dirty(n.core.get(p).is_some_and(|b| b.dirty))
            })
            .collect();
        let strip = TabStrip::mount(
            world,
            tabs,
            UiId(0x4e54_0001),
            TabStripProps::new("Open notes"),
            &specs,
            TabId(selected.tab),
        )
        .unwrap();
        world.get_mut::<Layout>(strip.root).unwrap().height = Size::Px(34.);
        world.insert(strip.body, Hidden);
        crate::ui::chrome::document_tabs(world, strip, true);
        n.tabstrip = Some(strip.root);
    }
    let crumb = text(
        world,
        toolbar,
        &path.replace('/', "  ›  "),
        Typography::Secondary,
    )
    .unwrap();
    world.get_mut::<Layout>(crumb).unwrap().width = Size::Fill;
    world.get_mut::<Label>(crumb).unwrap().overflow = TextOverflow::Ellipsis;
    button(
        world,
        toolbar,
        n,
        if n.source { "Read" } else { "Edit source" },
        Action::ToggleSource,
        true,
    );
    button(
        world,
        toolbar,
        n,
        if n.live { "Read" } else { "Write" },
        if n.live { Action::Read } else { Action::Write },
        true,
    );
    button(
        world,
        toolbar,
        n,
        "Find / Replace",
        Action::Find,
        n.views.contains_key(&path),
    );
    if n.loads.contains_key(&path) {
        caption(world, notice, "Checking latest version…");
    }
    if let Some(b) = n.core.get(&path).cloned() {
        caption(world, status, &note_statistics(&b.content));
        if b.conflict || n.remote.contains_key(&path) {
            caption(
                world,
                notice,
                "Changed on workshop · your edits are preserved",
            );
        } else if b.dirty {
            caption(
                world,
                notice,
                "Unsaved changes · retained in this app window",
            );
        }
        let saving = n.saves.contains_key(&path);
        button(
            world,
            toolbar,
            n,
            if saving { "Saving…" } else { "Save" },
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
            "Refresh",
            Action::Check,
            !saving && !n.loads.contains_key(&path),
        );
        if let Some(remote) = n.remote.get(&path) {
            caption(
                world,
                notice,
                "Compare with daemon content below. Merge in your editor, then acknowledge the latest baseline before saving. Nothing is overwritten until Save.",
            );
            // Bounded preview; the local editor is never replaced by conflict bytes.
            let preview: String = remote.content.chars().take(1200).collect();
            caption(
                world,
                notice,
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
                notice,
                n,
                "Keep my edits · acknowledge latest baseline",
                Action::Acknowledge,
                true,
            );
        }
    }
    if let Some(error) = n.errors.get(&path) {
        caption(world, notice, error);
    }
    for (p, v) in &n.views {
        if let Some(bar) = v.find {
            if *p == path && n.source && v.find_open {
                world.remove::<Hidden>(bar.root);
            } else {
                world.insert(bar.root, Hidden);
            }
        }
        if *p == path && n.source {
            world.remove::<Hidden>(v.handle.root);
        } else {
            world.insert(v.handle.root, Hidden);
        }
        if *p == path && n.live {
            world.remove::<Hidden>(v.rich.root);
        } else {
            world.insert(v.rich.root, Hidden);
        }
        if *p == path && !n.source && !n.live {
            world.remove::<Hidden>(v.preview);
            if let Some(b) = n.core.get(p) {
                let source = document_body(&b.content);
                let view = world.get_mut::<MarkdownView>(v.preview).unwrap();
                if view.source() != source {
                    view.set_markdown(source);
                }
            }
        } else {
            world.insert(v.preview, Hidden);
        }
    }
}
fn open_note(world: &mut World, n: &mut Notes, path: String) {
    if n.management.pending.is_some() {
        return;
    }
    let composing = !can_deactivate_views(world, n);
    if composing && n.selected.as_ref() != Some(&path) {
        if let Some(p) = n.selected.clone() {
            n.errors
                .insert(p, "Finish text composition before switching notes.".into());
        }
        n.redraw = true;
    } else {
        if n.selected.as_ref() != Some(&path) {
            // Match VaultTreeNode's one-shot reveal: explicit note navigation
            // opens ancestors, but later user collapse stays authoritative.
            let mut ancestor = path.as_str();
            while let Some((parent, _)) = ancestor.rsplit_once('/') {
                n.expanded.insert(parent.to_owned());
                ancestor = parent;
            }
            let old_editors: Vec<_> = n
                .views
                .values()
                .flat_map(|v| [v.handle.editor, v.rich.root])
                .collect();
            if let Some(focus) = world.resource_mut::<Focus>() {
                if focus.entity.is_some_and(|e| old_editors.contains(&e)) {
                    focus.entity = None;
                }
            }
        }
        if n.selected.as_ref() != Some(&path) {
            n.recent.retain(|p| p != &path);
            n.recent.insert(0, path.clone());
            n.recent.truncate(60);
        }
        n.selected = Some(path.clone());
        if !n.views.contains_key(&path) {
            n.load(path);
        }
        n.redraw = true;
    }
}
pub fn tick(world: &mut World) {
    if world.resource::<Notes>().is_none() {
        return;
    }
    for ev in events::<rndr_ui::menu::MenuPicked>(world).to_vec() {
        let n = world.resource_mut::<Notes>().unwrap();
        if n.menu.as_ref().is_some_and(|(m, _)| m.root == ev.menu) {
            let (_, mut actions) = n.menu.take().unwrap();
            if let Some(choice) = actions.remove(&ev.item) {
                n.actions.insert(ev.item, choice);
                action(world, &HostIntent::Command(ev.item));
                return;
            }
        }
    }
    let mut n = std::mem::take(world.resource_mut::<Notes>().unwrap());
    capture(world, &mut n);
    capture_rich(world, &mut n);
    for event in events::<TreeEvent>(world).to_vec() {
        if n.tree.is_none_or(|tree| tree.root != event.tree) {
            continue;
        }
        match event.kind {
            TreeEventKind::Selected(Some(id)) | TreeEventKind::Activated(id) => {
                if let Some(path) = n.tree_paths.get(&id).cloned() {
                    if n.selected.as_ref() != Some(&path) {
                        open_note(world, &mut n, path);
                        // Restore authoritative selection after a composition or
                        // management gate rejects the tree's local selection.
                        n.redraw = true;
                    }
                }
            }
            TreeEventKind::Expanded(id, open) if n.filter.is_empty() => {
                if let Some(folder) = n.tree_ids.iter().find_map(|(key, v)| {
                    (*v == id)
                        .then(|| key.strip_prefix("folder:").map(str::to_owned))
                        .flatten()
                }) {
                    let changed = if open {
                        n.expanded.insert(folder)
                    } else {
                        n.expanded.remove(&folder)
                    };
                    n.redraw |= changed;
                }
            }
            _ => {}
        }
    }
    for event in events::<TabStripEvent>(world).to_vec() {
        if n.tabstrip == Some(event.strip) {
            match event.kind {
                TabStripEventKind::Activated(id) => {
                    if let Some(path) = n
                        .views
                        .iter()
                        .find_map(|(p, v)| (v.tab == id.0).then(|| p.clone()))
                    {
                        open_note(world, &mut n, path);
                    }
                }
                TabStripEventKind::Reordered(moved) if can_deactivate_views(world, &n) => {
                    let specs: Vec<_> = n
                        .tab_order
                        .iter()
                        .filter_map(|path| {
                            n.views
                                .get(path)
                                .map(|v| TabSpec::new(v.tab, path).closable(false))
                        })
                        .collect();
                    if let Some(order) =
                        rndr_ui::tabs::reorder_tabs(&specs, moved.id, moved.target, moved.place)
                    {
                        n.tab_order = order
                            .iter()
                            .filter_map(|tab| {
                                n.views
                                    .iter()
                                    .find_map(|(path, v)| (v.tab == tab.id.0).then(|| path.clone()))
                            })
                            .collect();
                        n.redraw = true;
                    }
                }
                _ => {}
            }
        }
    }
    capture(world, &mut n);
    capture_rich(world, &mut n);
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
    // Keep statistics current without remounting the library on every keystroke.
    if let Some(parent) = n.status {
        if let Some(buffer) = n.selected.as_ref().and_then(|p| n.core.get(p)) {
            let statistics = note_statistics(&buffer.content);
            for child in children_of(world, parent) {
                if let Some(label) = world.get_mut::<Label>(child) {
                    label.text.clone_from(&statistics);
                }
            }
        }
    }
    if n.redraw {
        n.redraw = false;
        render(world, &mut n);
    }
    *world.resource_mut::<Notes>().unwrap() = n;
}
/// Consume only Notes primary-F presses. Keep arrival order for other widgets.
pub fn keyboard(world: &mut World) {
    if crate::surfaces::active(world) != crate::surfaces::SurfaceId::Notes
        || !can_deactivate(world)
        || world
            .resource::<Input>()
            .is_some_and(|i| !i.ime_preedit.is_empty() || !i.ime_commit.is_empty())
    {
        return;
    }
    let platform = world
        .resource::<Platform>()
        .copied()
        .unwrap_or_else(Platform::host);
    let keys = world
        .resource_mut::<Input>()
        .map(|i| std::mem::take(&mut i.keys))
        .unwrap_or_default();
    let mut kept = Vec::with_capacity(keys.len());
    let mut consumed = false;
    for event in keys {
        if !event.released
            && !event.repeat
            && !event.synthetic
            && Keystroke::character('f')
                .primary()
                .matches(&event, platform)
            && shortcut(world, "find")
        {
            consumed = true;
        } else {
            kept.push(event);
        }
    }
    if let Some(input) = world.resource_mut::<Input>() {
        // rndr_render::Runner and Input::push_key use the FIRST press, not
        // the first mappable press. Reconstruct only after actual consumption;
        // legacy-only input and rejected events must remain untouched.
        if consumed {
            input.key = kept
                .iter()
                .find(|e| !e.released)
                .and_then(|e| rndr_core::legacy_key(e, platform));
        }
        input.keys = kept;
    }
}

/// Queue a host-routed Notes shortcut without crossing an active IME session.
/// The shared shell owns active-surface and key-event consumption policy.
pub fn shortcut(world: &mut World, command: &str) -> bool {
    let Some(n) = world.resource::<Notes>() else {
        return false;
    };
    if command != "find"
        || !can_deactivate_views(world, n)
        || !n.selected.as_ref().is_some_and(|p| n.views.contains_key(p))
    {
        return false;
    }
    let id = next();
    world
        .resource_mut::<Notes>()
        .unwrap()
        .actions
        .insert(id, Action::Find);
    action(world, &HostIntent::Command(id))
}

pub fn action(world: &mut World, host: &HostIntent) -> bool {
    let HostIntent::Command(id) = host else {
        return false;
    };
    let Some(a) = world
        .resource::<Notes>()
        .and_then(|n| n.actions.get(id).or_else(|| n.management.actions.get(id)))
        .cloned()
    else {
        return false;
    };
    let mut n = std::mem::take(world.resource_mut::<Notes>().unwrap());
    capture(world, &mut n);
    capture_rich(world, &mut n);
    if n.management.pending.is_some() && !matches!(a, Action::Manage(_)) {
        *world.resource_mut::<Notes>().unwrap() = n;
        return true;
    }
    match a {
        Action::Manage(a) => {
            management::action(world, &mut n, a);
            n.redraw = true;
        }
        Action::Picker(which, anchor) => open_picker(world, &mut n, which, anchor),
        Action::Group(group) => {
            n.group = group;
            n.redraw = true;
        }
        Action::SearchList if can_deactivate_views(world, &n) => {
            n.searching = !n.searching;
            if !n.searching {
                world
                    .get_mut::<Label>(n.search.unwrap())
                    .unwrap()
                    .text
                    .clear();
            }
            if let Some(f) = world.resource_mut::<Focus>() {
                f.entity = if n.searching { n.search } else { None };
            }
            n.redraw = true;
        }
        Action::SearchList => (),
        Action::Browse(mode) => {
            n.browse = mode;
            n.redraw = true;
        }
        Action::Find => {
            if can_deactivate_views(world, &n) {
                if let Some(v) = n.selected.as_ref().and_then(|p| n.views.get_mut(p)) {
                    if let Some(parent) = n.find_tools {
                        if v.find.is_none() {
                            v.find = rndr_ui::source_tools::FindBar::mount(
                                world,
                                parent,
                                v.handle.editor,
                            )
                            .ok();
                        }
                        if let Some(bar) = v.find {
                            // Rich was captured above. The source is now the single
                            // visible writer, using its existing retained history.
                            v.find_open = !v.find_open || !n.source;
                            n.source = true;
                            n.live = false;
                            if let Some(focus) = world.resource_mut::<Focus>() {
                                focus.entity = if v.find_open {
                                    Some(bar.query)
                                } else {
                                    Some(v.handle.editor)
                                };
                            }
                            n.redraw = true;
                        }
                    }
                }
            }
        }
        Action::Refresh => n.list_page(false),
        Action::More => {
            if n.cursor.is_some() {
                n.list_page(true);
            }
        }
        Action::Open(path) => open_note(world, &mut n, path),
        Action::ToggleSource => {
            // Never hide the platform's active marked-text owner.
            let composing = !can_deactivate_views(world, &n);
            if !composing {
                n.source = !n.source;
                n.live = false;
                if let Some(focus) = world.resource_mut::<Focus>() {
                    focus.entity = None;
                }
                n.redraw = true;
            }
        }
        Action::Write | Action::Read => {
            if can_deactivate_views(world, &n) {
                n.live = matches!(a, Action::Write);
                n.source = false;
                if let Some(focus) = world.resource_mut::<Focus>() {
                    focus.entity = None;
                }
                n.redraw = true;
            }
        }
        Action::Format(command) => {
            if n.live && can_deactivate_views(world, &n) {
                if let Some(path) = n.selected.clone() {
                    if let Some(v) = n.views.get(&path) {
                        if let Err(error) = v.rich.command(world, command) {
                            n.errors
                                .insert(path, format!("Formatting was not applied: {error:?}"));
                        }
                    }
                    capture_rich(world, &mut n);
                    n.redraw = true;
                }
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
    if let Operation::Manage(command) = &c.operation {
        let result = management::execute(client, command)
            .await
            .map(Response::Managed);
        return NoteUpdate { command: c, result };
    }
    let result = match &c.operation {
        Operation::Manage(_) => unreachable!(),
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
    capture_rich(world, &mut n);
    let NoteUpdate { command: c, result } = u;
    if let Operation::Manage(command) = &c.operation {
        management::apply(world, &mut n, c.serial, command, result);
        n.redraw = true;
        *world.resource_mut::<Notes>().unwrap() = n;
        tick(world);
        return;
    }
    let valid = match &c.operation {
        Operation::Manage(_) => false,
        Operation::List { .. } => n.list_pending == Some(c.serial),
        Operation::Load(path) => n.loads.get(path) == Some(&c.serial),
        Operation::Save { path, .. } => n.saves.get(path) == Some(&c.serial),
    };
    if !valid {
        *world.resource_mut::<Notes>().unwrap() = n;
        return;
    }
    match c.operation {
        Operation::Manage(_) => unreachable!(),
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
                                background: Some(visual::canvas().into()),
                                foreground: Some(visual::text().into()),
                                typography: Some(rndr_ui::Typography::Code),
                                ..Default::default()
                            },
                        );
                        world.insert(handle.editor, ResolvedColor);
                        // UiStyle owns the background across appearance changes.
                        visual::flat(world, handle.root, visual::canvas());
                        let preview = markdown_view(
                            world,
                            document_body(&n.core.get(&path).unwrap().content),
                        );
                        set_parent(world, preview, parent);
                        world.insert(preview, visual::ReadingDocument);
                        world.insert(
                            preview,
                            UiStyle {
                                typography: Some(Typography::Reading),
                                ..Default::default()
                            },
                        );
                        world.get_mut::<Layout>(preview).unwrap().padding = 16.;
                        world
                            .get_mut::<MarkdownView>(preview)
                            .unwrap()
                            .set_follow(false);
                        if world.resource::<MarkdownDocuments>().is_none() {
                            world.insert_resource(MarkdownDocuments::default());
                        }
                        let rich_document = world
                            .resource_mut::<MarkdownDocuments>()
                            .unwrap()
                            .open(document_body(&n.core.get(&path).unwrap().content));
                        let rich = MarkdownSurface::mount(
                            world,
                            parent,
                            UiId(next()),
                            MarkdownSurfaceProps::new(rich_document, format!("Write note {path}")),
                        )
                        .expect("notes shared rich document");
                        n.tab_order.push(path.clone());
                        n.views.insert(
                            path.clone(),
                            View {
                                document,
                                captured_revision: None,
                                handle,
                                preview,
                                rich_document,
                                rich,
                                rich_revision: 0,
                                tab: next(),
                                find: None,
                                find_open: false,
                            },
                        );
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
