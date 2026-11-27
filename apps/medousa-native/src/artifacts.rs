//! Workshop-owned HTML library. Only metadata lives in this retained surface;
//! guest HTML goes through the isolated preview and bounded fetch contract.
use crate::{native_style as visual, surfaces::SurfaceId};
use medousa_sdk::MedousaClient;
use medousa_types::{ArtifactListUiResponse, ArtifactSummary};
use rndr_core::*;
use rndr_ui::{
    Typography, UiId,
    controls::{self, ButtonProps, HostIntent},
    text,
};
use std::{
    collections::HashMap,
    sync::atomic::{AtomicU64, Ordering},
};

const LIMIT: usize = 500;
static NEXT: AtomicU64 = AtomicU64::new(0x4152_0000);
fn next() -> u64 {
    NEXT.fetch_add(1, Ordering::Relaxed)
}
#[derive(Clone, Debug)]
pub struct Command {
    serial: u64,
    kind: Kind,
}
#[derive(Clone, Debug)]
enum Kind {
    List(String),
    Export(ArtifactSummary),
}
pub struct Update {
    command: Command,
    result: Result<Response, String>,
}
enum Response {
    Listed(Vec<ArtifactSummary>),
    Exported(bool),
}
#[derive(Clone)]
enum Action {
    Refresh,
    Search,
    Clear,
    Select(String, String),
    Preview,
    Source,
    Export,
}
#[derive(Clone)]
struct ActionTarget(Action);
impl Component for ActionTarget {}
fn same_action(a: &Action, b: &Action) -> bool {
    match (a, b) {
        (Action::Select(sa, ia), Action::Select(sb, ib)) => sa == sb && ia == ib,
        _ => std::mem::discriminant(a) == std::mem::discriminant(b),
    }
}
#[derive(Clone)]
struct State {
    list: Entity,
    detail: Entity,
    viewport: Option<Entity>,
    status: Entity,
    search: Entity,
    narrow: bool,
    connected: bool,
    rows: Vec<ArtifactSummary>,
    selected: Option<(String, String)>,
    query: String,
    server_query: String,
    started: bool,
    loading: Option<u64>,
    exporting: Option<u64>,
    error: Option<String>,
    notice: String,
    dirty: bool,
    actions: HashMap<u64, Action>,
    commands: Vec<Command>,
}
fn panel(w: &mut World, p: Entity, l: Layout) -> Entity {
    let e = w.spawn((l,));
    set_parent(w, e, p);
    e
}
fn column(w: &mut World, p: Entity, fill: bool) -> Entity {
    panel(
        w,
        p,
        Layout {
            width: Size::Fill,
            height: if fill { Size::Fill } else { Size::Hug },
            min_width: 0.,
            min_height: 0.,
            gap: 12.,
            ..Layout::column()
        },
    )
}
fn caption(w: &mut World, p: Entity, value: &str, style: Typography) -> Entity {
    let e = text(w, p, value, style).unwrap();
    w.get_mut::<Layout>(e).unwrap().width = Size::Fill;
    w.get_mut::<Label>(e).unwrap().overflow = TextOverflow::Ellipsis;
    e
}
fn paragraph(w: &mut World, p: Entity, value: &str, style: Typography) -> Entity {
    let e = caption(w, p, value, style);
    w.get_mut::<Label>(e).unwrap().overflow = TextOverflow::Wrap;
    e
}
fn button(
    w: &mut World,
    s: &mut State,
    p: Entity,
    label: &str,
    action: Action,
    enabled: bool,
) -> Entity {
    let id = next();
    let mut props = ButtonProps::new(label, HostIntent::Command(id));
    props.enabled = enabled;
    props.variant = controls::ButtonVariant::Quiet;
    let h = controls::Button::mount(w, p, UiId(id), props).unwrap();
    w.get_mut::<Layout>(h.root).unwrap().height = Size::Px(36.);
    w.insert(
        h.root,
        visual::FitLabel {
            label: h.label,
            extra: 8.,
            min: 64.,
            max: 220.,
        },
    );
    w.insert(h.root, ActionTarget(action.clone()));
    s.actions.insert(id, action);
    h.root
}
fn icon_button(
    w: &mut World,
    s: &mut State,
    parent: Entity,
    icon: Icon,
    label: &str,
    action: Action,
    enabled: bool,
) -> Entity {
    let id = next();
    let h = visual::icon_action(w, parent, id, icon, label, enabled);
    w.insert(h.root, ActionTarget(action.clone()));
    s.actions.insert(id, action);
    h.root
}
pub fn mount(w: &mut World, root: Entity) {
    let sidebar = panel(
        w,
        root,
        Layout {
            width: Size::Px(288.),
            height: Size::Fill,
            gap: 12.,
            ..Layout::column()
        },
    );
    visual::navigation_frame(w, sidebar);
    let header = panel(
        w,
        sidebar,
        Layout {
            width: Size::Fill,
            height: Size::Px(40.),
            gap: 8.,
            ..Layout::row()
        },
    );
    caption(w, header, "Artifacts", Typography::Emphasis);
    let refresh_id = next();
    visual::icon_action(
        w,
        header,
        refresh_id,
        Icon::RefreshCw,
        "Refresh artifacts",
        true,
    );
    let search = w.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(36.),
            ..Layout::row()
        },
        Label::new(""),
        Focusable,
        TextField,
        Placeholder::new("Search artifacts…"),
        Semantics {
            name: Some("Search workshop artifacts; Enter to search all".into()),
            ..Default::default()
        },
    ));
    set_parent(w, search, sidebar);
    visual::plane(w, search, visual::canvas());
    w.insert(search, visual::SemanticBorder(visual::stroke()));
    w.insert(search, Radius(visual::CONTROL_RADIUS));
    w.insert(
        search,
        LabelBox {
            inset_x: 10.,
            inset_y: 8.,
            min_height: 36.,
        },
    );
    let search_actions = panel(
        w,
        sidebar,
        Layout {
            width: Size::Fill,
            height: Size::Px(32.),
            gap: 4.,
            ..Layout::row()
        },
    );
    let count = caption(w, sidebar, "HTML · All conversations", Typography::Overline);
    let list = column(w, sidebar, true);
    w.insert(list, Scroll::default());
    let main = column(w, root, true);
    visual::plane(w, main, visual::canvas());
    // The shared window chrome already identifies this destination. Keep one
    // compact document toolbar instead of stacking another surface heading.
    w.get_mut::<Layout>(main).unwrap().gap = 0.;
    let detail = column(w, main, true);
    w.insert(
        detail,
        Padding {
            left: 12.,
            right: 12.,
            top: 4.,
            bottom: 0.,
        },
    );
    w.get_mut::<Layout>(detail).unwrap().gap = 8.;

    let mut s = State {
        list,
        detail,
        viewport: None,
        status: count,
        search,
        narrow: false,
        connected: true,
        rows: vec![],
        selected: None,
        query: String::new(),
        server_query: String::new(),
        started: false,
        loading: None,
        exporting: None,
        error: None,
        notice: String::new(),
        dirty: true,
        actions: HashMap::new(),
        commands: vec![],
    };
    s.actions.insert(refresh_id, Action::Refresh);
    button(
        w,
        &mut s,
        search_actions,
        "Search workshop",
        Action::Search,
        true,
    );
    button(w, &mut s, search_actions, "Clear", Action::Clear, true);
    w.insert_resource(s);
}
pub fn composing(w: &World) -> bool {
    w.resource::<State>().is_some_and(|s| {
        w.has::<ImePreview>(s.search)
            || (w
                .resource::<Focus>()
                .is_some_and(|f| f.entity == Some(s.search))
                && w.resource::<Input>()
                    .is_some_and(|i| !i.ime_preedit.is_empty() || !i.ime_commit.is_empty()))
    })
}
pub fn can_rebind(w: &World) -> bool {
    !composing(w) && w.resource::<State>().is_none_or(|s| s.exporting.is_none())
}
fn request(s: &mut State) {
    let serial = next();
    s.loading = Some(serial);
    s.started = true;
    s.error = None;
    s.server_query = s.query.trim().into();
    s.commands.push(Command {
        serial,
        kind: Kind::List(s.server_query.clone()),
    });
    s.dirty = true;
}
pub fn activate(w: &mut World) {
    if let Some(s) = w.resource_mut::<State>()
        && !s.started
    {
        request(s);
    }
}
pub fn take_commands(w: &mut World) -> Vec<Command> {
    w.resource_mut::<State>()
        .map(|s| std::mem::take(&mut s.commands))
        .unwrap_or_default()
}
fn selected(s: &State) -> Option<&ArtifactSummary> {
    s.rows.iter().find(|r| {
        s.selected
            .as_ref()
            .is_some_and(|k| k.0 == r.session_id && k.1 == r.artifact_id)
    })
}
fn session(w: &World, raw: &str) -> String {
    let owner = crate::workshops::binding(w, SurfaceId::Artifacts)
        .map(|b| b.authority)
        .unwrap_or_default();
    crate::routing::qualify(&owner, raw)
}
pub fn action(w: &mut World, intent: &HostIntent) -> bool {
    let HostIntent::Command(id) = intent else {
        return false;
    };
    let Some(a) = w
        .resource::<State>()
        .and_then(|s| s.actions.get(id))
        .cloned()
    else {
        return false;
    };
    let row = w.resource::<State>().and_then(selected).cloned();
    match a {
        Action::Refresh | Action::Search => {
            if composing(w) {
                return true;
            }
            let search = w.resource::<State>().unwrap().search;
            let query = w.get::<Label>(search).unwrap().text.clone();
            let s = w.resource_mut::<State>().unwrap();
            s.query = query;
            request(s);
        }
        Action::Clear => {
            let search = w.resource::<State>().unwrap().search;
            if composing(w) {
                return true;
            }
            replace_field_text(w, search, "");
            let s = w.resource_mut::<State>().unwrap();
            s.query.clear();
            request(s);
        }
        Action::Select(sid, id) => {
            let s = w.resource_mut::<State>().unwrap();
            s.selected = Some((sid, id));
            s.notice.clear();
            s.dirty = true;
        }
        Action::Preview => {
            crate::artifact_preview::close_inline(w);
            w.resource_mut::<State>().unwrap().dirty = true;
        }
        Action::Source => {
            if let Some(row) = row {
                let sid = session(w, &row.session_id);
                crate::ui::open_conversation(w, sid);
            }
        }
        Action::Export => {
            if let Some(row) = row {
                let s = w.resource_mut::<State>().unwrap();
                if s.exporting.is_none() {
                    let serial = next();
                    s.exporting = Some(serial);
                    s.notice = "Preparing HTML for Save As…".into();
                    s.commands.push(Command {
                        serial,
                        kind: Kind::Export(row),
                    });
                    s.dirty = true;
                }
            }
        }
    }
    true
}
fn matches(row: &ArtifactSummary, query: &str) -> bool {
    let q = query.trim().to_lowercase();
    row.label.to_lowercase().contains(&q) || row.artifact_id.to_lowercase().contains(&q)
}
fn size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else {
        format!("{:.1} KB", bytes as f64 / 1024.)
    }
}
pub fn sync(w: &mut World) {
    let Some(s) = w.resource::<State>() else {
        return;
    };
    let search = s.search;
    let query = w
        .get::<Label>(search)
        .map(|p| p.text.clone())
        .unwrap_or_default();
    let submitted = crate::surfaces::active(w) == SurfaceId::Artifacts
        && events::<Submitted>(w).iter().any(|s| s.entity == search);
    let narrow = w.get::<Rect>(s.detail).is_some_and(|r| r.w < 540.);
    let connected = w
        .resource::<crate::workshops::Workshops>()
        .is_none_or(|ws| {
            ws.bindings
                .get(&SurfaceId::Artifacts)
                .is_some_and(|b| ws.auxiliaries.contains_key(&b.authority))
        });
    let s = w.resource_mut::<State>().unwrap();
    if s.connected != connected {
        s.connected = connected;
        s.dirty = true;
    }
    if s.narrow != narrow {
        s.narrow = narrow;
        s.dirty = true;
    }
    if query != s.query {
        s.query = query;
        s.dirty = true;
    }
    if submitted {
        request(s);
    }
    let s = w.resource::<State>().unwrap();
    if crate::surfaces::active(w) == SurfaceId::Artifacts && s.connected {
        if let (Some(viewport), Some(row)) = (s.viewport, selected(s).cloned()) {
            let sid = session(w, &row.session_id);
            crate::artifact_preview::inline(w, viewport, &sid, &row.artifact_id, &row.label);
        }
    } else {
        crate::artifact_preview::close_inline(w);
    }
    let s = w.resource::<State>().unwrap();
    if !s.dirty || !w.entities_with::<Modal>().is_empty() {
        return;
    }
    let mut s = w.resource::<State>().unwrap().clone();
    s.dirty = false;
    let focus_action = w
        .resource::<Focus>()
        .and_then(|f| f.entity)
        .and_then(|focused| {
            w.query::<ActionTarget>()
                .iter()
                .find(|(e, _)| crate::surfaces::descendant(w, focused, *e))
                .map(|(_, a)| a.0.clone())
        });
    // Park the live guest outside the rebuilt subtree to preserve its DOM/scroll.
    let parking = crate::surfaces::root(w, SurfaceId::Artifacts).unwrap();
    crate::artifact_preview::park_inline(w, parking);
    s.viewport = None;
    // Search remains retained, so filtering cannot steal input focus.
    let toolbar_host = crate::ui::chrome::artifact_toolbar_host(w);
    if let Some(host) = toolbar_host {
        for child in children_of(w, host) {
            despawn(w, child);
        }
    }
    for root in [s.list, s.detail] {
        for c in children_of(w, root) {
            despawn(w, c);
        }
    }
    s.actions
        .retain(|_, a| matches!(a, Action::Refresh | Action::Search | Action::Clear));
    let visible = s
        .rows
        .iter()
        .filter(|r| matches(r, &s.query))
        .cloned()
        .collect::<Vec<_>>();
    if let Some(label) = w.get_mut::<Label>(s.status) {
        label.text = if s.error.is_some() && s.rows.is_empty() {
            "Library unavailable".into()
        } else if !s.connected {
            "Workshop offline".into()
        } else if s.loading.is_some() {
            "Updating artifacts…".into()
        } else {
            format!("{} HTML artifacts · recent first", visible.len())
        };
    }
    let list = s.list;
    w.get_mut::<Layout>(list).unwrap().gap = 4.;
    if let Some(error) = &s.error {
        caption(w, list, "Couldn’t update the library", Typography::Emphasis);
        paragraph(w, list, error, Typography::Secondary);
        button(w, &mut s, list, "Try again", Action::Refresh, true);
    }
    if visible.is_empty() && s.error.is_none() {
        let message = if !s.connected {
            "Connect this workshop to browse artifacts."
        } else if s.loading.is_some() {
            "Loading workshop artifacts…"
        } else if !s.query.is_empty() {
            "No matching artifacts"
        } else {
            "No HTML artifacts yet"
        };
        caption(w, list, message, Typography::Secondary);
    }
    for r in &visible {
        let id = next();
        let mut props = ButtonProps::new(&r.label, HostIntent::Command(id));
        props.variant = controls::ButtonVariant::Quiet;
        props.description = format!(
            "HTML · {} · {}",
            size(r.byte_size),
            r.stored_at_utc.format("%b %-d, %Y")
        );
        let h = controls::Button::mount(w, list, UiId(id), props).unwrap();
        *w.get_mut::<Layout>(h.root).unwrap() = Layout {
            width: Size::Fill,
            height: Size::Px(64.),
            padding: 1.,
            ..Layout::column()
        };
        // Keep RNDR's focus-ring layers, but use a two-line, full-row hit target.
        let face = w.get::<Parent>(h.label).unwrap().entity;
        let mut node = face;
        while node != h.root {
            w.get_mut::<Layout>(node).unwrap().height = Size::Fill;
            node = w.get::<Parent>(node).unwrap().entity;
        }
        w.get_mut::<Layout>(face).unwrap().padding = 6.;
        w.get_mut::<Layout>(face).unwrap().gap = 3.;
        w.get_mut::<Layout>(h.label).unwrap().width = Size::Fill;
        w.get_mut::<Layout>(h.label).unwrap().height = Size::Px(20.);
        w.get_mut::<Label>(h.label).unwrap().overflow = TextOverflow::Ellipsis;
        let chosen = s
            .selected
            .as_ref()
            .is_some_and(|k| k.0 == r.session_id && k.1 == r.artifact_id);
        w.insert(
            h.root,
            RowChrome {
                label: h.label,
                selected: chosen,
            },
        );
        let action = Action::Select(r.session_id.clone(), r.artifact_id.clone());
        w.insert(h.root, ActionTarget(action.clone()));
        s.actions.insert(id, action);
        let meta = caption(
            w,
            face,
            &format!(
                "HTML  ·  {}  ·  {}",
                size(r.byte_size),
                r.stored_at_utc.format("%b %-d")
            ),
            Typography::Caption,
        );
        w.insert(
            meta,
            LabelBox {
                inset_x: 6.,
                inset_y: 0.,
                min_height: 16.,
            },
        );
    }
    if s.rows.len() >= LIMIT {
        caption(
            w,
            list,
            "Showing the latest 500. Search workshop to find older artifacts.",
            Typography::Caption,
        );
    } else if s.query.trim() != s.server_query {
        caption(
            w,
            list,
            "Enter searches the entire workshop.",
            Typography::Caption,
        );
    }
    let detail = s.detail;
    if let Some(row) = selected(&s).cloned() {
        let toolbar = panel(
            w,
            toolbar_host.unwrap_or(detail),
            Layout {
                width: Size::Fill,
                height: if toolbar_host.is_some() {
                    Size::Fill
                } else {
                    Size::Px(40.)
                },
                gap: 12.,
                ..Layout::row()
            },
        );
        w.insert(
            toolbar,
            Align {
                items: Items::Center,
                ..Default::default()
            },
        );
        w.insert(toolbar, WindowDrag);
        let identity = column(w, toolbar, false);
        w.get_mut::<Layout>(identity).unwrap().gap = 2.;
        let title = caption(w, identity, &row.label, Typography::Emphasis);
        visual::action_hint(w, title, &row.label);
        caption(
            w,
            identity,
            &format!(
                "HTML  ·  {}  ·  {}",
                size(row.byte_size),
                row.stored_at_utc.format("%b %-d, %Y")
            ),
            Typography::Caption,
        );
        let actions = panel(
            w,
            toolbar,
            Layout {
                width: Size::Hug,
                height: Size::Px(36.),
                gap: 4.,
                ..Layout::row()
            },
        );
        let connected = s.connected;
        icon_button(
            w,
            &mut s,
            actions,
            Icon::RefreshCw,
            "Reload preview",
            Action::Preview,
            connected,
        );
        if s.narrow {
            icon_button(
                w,
                &mut s,
                actions,
                Icon::MessageCircle,
                "Open source chat",
                Action::Source,
                true,
            );
        } else {
            button(w, &mut s, actions, "Open chat", Action::Source, true);
        }
        let enabled = s.exporting.is_none() && connected;
        if s.narrow {
            icon_button(
                w,
                &mut s,
                actions,
                Icon::Download,
                "Export HTML to this device…",
                Action::Export,
                enabled,
            );
        } else {
            let export = button(w, &mut s, actions, "Export HTML…", Action::Export, enabled);
            visual::action_hint(
                w,
                export,
                "Save a copy to this device; the workshop original stays unchanged",
            );
        }
        let card = column(w, detail, true);
        s.viewport = Some(card);
        if s.connected && crate::surfaces::active(w) == SurfaceId::Artifacts {
            let sid = session(w, &row.session_id);
            crate::artifact_preview::inline(w, card, &sid, &row.artifact_id, &row.label);
        } else {
            crate::artifact_preview::close_inline(w);
            if !s.connected {
                paragraph(
                    w,
                    card,
                    "Connect this workshop to load the artifact preview.",
                    Typography::Secondary,
                );
            }
        }
    } else {
        crate::artifact_preview::close_inline(w);
        let l = visual::landing(
            w,
            detail,
            Icon::Layers,
            "Artifacts",
            "Every page Medousa has made for you, across this workshop's conversations.",
        );
        let id = next();
        visual::landing_tile(w, l.actions, id, Icon::RefreshCw, "Refresh", s.connected);
        s.actions.insert(id, Action::Refresh);
        // While searching, results belong to the sidebar list alone.
        let browsing = s.query.trim().is_empty();
        let mut recent: Vec<_> = s.rows.iter().filter(|_| browsing).collect();
        recent.sort_by_key(|r| std::cmp::Reverse(r.stored_at_utc));
        let recent: Vec<_> = recent
            .into_iter()
            .take(6)
            .map(|r| {
                (
                    r.session_id.clone(),
                    r.artifact_id.clone(),
                    r.label.clone(),
                    visual::relative_day(r.stored_at_utc),
                )
            })
            .collect();
        if !recent.is_empty() {
            visual::landing_section(w, l.list, "Recent");
        }
        for (session, artifact, label, when) in recent {
            let id = next();
            let title = if label.trim().is_empty() {
                "Untitled artifact".into()
            } else {
                label
            };
            visual::landing_row(w, l.list, id, &title, Some(when));
            s.actions.insert(id, Action::Select(session, artifact));
        }
    }
    if !s.notice.is_empty() {
        paragraph(w, detail, &s.notice, Typography::Secondary);
    }
    if let Some(action) = focus_action {
        let target = w
            .query::<ActionTarget>()
            .iter()
            .find(|(_, a)| same_action(&a.0, &action))
            .map(|(e, _)| *e);
        if let Some(target) = target {
            let _ = request_focus(w, target);
        } else {
            let _ = request_focus(w, s.search);
        }
    }
    w.insert_resource(s);
}
pub fn apply(w: &mut World, u: Update) {
    let Some(s) = w.resource_mut::<State>() else {
        return;
    };
    match u.command.kind {
        Kind::List(_) if s.loading == Some(u.command.serial) => {
            s.loading = None;
            match u.result {
                Ok(Response::Listed(rows)) => {
                    s.rows = rows;
                    if selected(s).is_none() {
                        s.selected = None;
                    }
                    s.error = None;
                }
                Err(e) => s.error = Some(e),
                _ => return,
            }
        }
        Kind::Export(_) if s.exporting == Some(u.command.serial) => {
            s.exporting = None;
            s.notice = match u.result {
                Ok(Response::Exported(true)) => "HTML saved to this device.".into(),
                Ok(Response::Exported(false)) => "Export cancelled. No file was saved.".into(),
                Err(e) => format!("Couldn’t export HTML: {e}"),
                _ => return,
            };
        }
        _ => return,
    }
    s.dirty = true;
}
fn filename(label: &str) -> String {
    let stem: String = label
        .chars()
        .take(48)
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '-' | '_' | ' ') {
                c
            } else {
                '-'
            }
        })
        .collect();
    let stem = stem.trim();
    format!("{}.html", if stem.is_empty() { "artifact" } else { stem })
}
pub async fn execute(client: &MedousaClient, command: Command) -> Update {
    let result = match &command.kind {
        Kind::List(query) => {
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(20),
                client.transport().post_json_bounded(
                    client.base_url(),
                    "/v1/runtime/artifact/list-ui",
                    serde_json::json!({"limit": LIMIT, "query": query}),
                    2 * 1024 * 1024,
                ),
            )
            .await;
            match result {
                Ok(Ok(v)) => serde_json::from_value::<ArtifactListUiResponse>(v)
                    .map(|r| Response::Listed(r.artifacts))
                    .map_err(|_| "Invalid artifact library response".into()),
                Ok(Err(_)) => {
                    Err("Could not reach the workshop. Check its connection and try again.".into())
                }
                Err(_) => Err("The workshop request timed out. Try again.".into()),
            }
        }
        Kind::Export(row) => export(client, row).await,
    };
    Update { command, result }
}
async fn export(client: &MedousaClient, row: &ArtifactSummary) -> Result<Response, String> {
    let html =
        crate::artifact_preview::fetch_html(client, &row.session_id, &row.artifact_id).await?;
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_title("Export HTML to this device")
        .set_file_name(filename(&row.label))
        .add_filter("HTML", &["html"])
        .save_file()
        .await
    else {
        return Ok(Response::Exported(false));
    };
    write_device_html(file.path().to_path_buf(), html).await?;
    Ok(Response::Exported(true))
}
async fn write_device_html(path: std::path::PathBuf, html: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || std::fs::write(path, html))
        .await
        .map_err(|_| "The device save task failed".to_owned())?
        .map_err(|_| {
            "The selected file could not be written. Choose a writable location.".to_owned()
        })?;
    Ok(())
}

struct RowChrome {
    label: Entity,
    selected: bool,
}
impl Component for RowChrome {}
pub fn chrome(w: &mut World) {
    let rows: Vec<_> = w
        .query::<RowChrome>()
        .iter()
        .map(|(e, r)| (*e, r.label, r.selected))
        .collect();
    let hover = w
        .resource::<Input>()
        .and_then(|i| hit_test(w, i.pointer.position[0], i.pointer.position[1]));
    for (root, label, selected) in rows {
        let pressed = interaction_pressed(w, root);
        let hovered = hover.is_some_and(|e| crate::surfaces::descendant(w, e, root));
        let fill = if pressed {
            rndr_ui::Token::ControlPressed
        } else if hovered {
            rndr_ui::Token::ControlHover
        } else if selected {
            rndr_ui::Token::Selected
        } else {
            visual::navigation()
        };
        let face = w.get::<Parent>(label).unwrap().entity;
        let border = w.get::<Parent>(face).unwrap().entity;
        for e in [face, border] {
            visual::plane(w, e, fill);
        }
    }
}

#[cfg(test)]
#[path = "artifacts_tests.rs"]
mod tests;
