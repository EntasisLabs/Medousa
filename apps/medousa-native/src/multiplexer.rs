//! Mixed-content Workspace. Arrangement owns references, never backing resources.
//! Retained content is parked before topology changes; no tab panel owns its lifetime.
use crate::{
    multiplexer_model::{self as model, Edge, Multiplexer, Resource, SplitNode},
    surfaces::SurfaceId,
};
use rndr_core::*;
use rndr_ui::{
    controls::{self, ButtonProps, HostIntent},
    split::{SplitSash, SplitSashChange, SplitSashHandle, SplitSashProps},
    tabs::{
        SplitDirection, TabId, TabSpec, TabStrip, TabStripEvent, TabStripEventKind, TabStripHandle,
        TabStripProps,
    },
    *,
};
use std::{collections::HashMap, path::PathBuf, sync::Arc};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Local {
    Chat(u64),
    Terminal(u64),
    Note(String),
    Code(String, String),
    Create(String),
    Review(String),
}
#[derive(Clone)]
struct Item {
    resource: Resource,
    title: String,
    entity: Entity,
    local: Local,
    dirty: bool,
}
#[derive(Clone)]
enum Action {
    Open(Resource),
    Browse(SurfaceId),
    NewChat,
    NewShell,
    Desktop(u64),
    NewDesktop,
    Duplicate,
    Rename,
    Delete,
    SaveTemplate,
    RetrySave,
    ApplyTemplate(u64),
    DeleteTemplate(u64),
    Split(Edge),
    Zoom,
    Focus(u64),
    CloseGroup(u64),
    Reopen(Resource),
    Content(Resource, &'static str),
}
struct GroupView {
    id: u64,
    root: Entity,
    body: Entity,
    strip: Option<TabStripHandle>,
}
struct Chrome {
    name: Entity,
    catalog: Entity,
    body: Entity,
    parking: Entity,
    groups: Vec<GroupView>,
    splits: Vec<(Vec<bool>, SplitSashHandle)>,
}
pub(crate) struct State {
    model: Multiplexer,
    chrome: Option<Chrome>,
    actions: HashMap<u64, Action>,
    group_actions: HashMap<u64, Action>,
    next: u64,
    items: Vec<Item>,
    originals: HashMap<Entity, Entity>,
    identities: HashMap<Entity, Resource>,
    requested: std::collections::HashSet<Resource>,
    rendered: String,
    catalog_key: String,
    saved: serde_json::Value,
    notice: String,
    store: Option<Arc<crate::multiplexer_storage::Store>>,
    queued: Option<serde_json::Value>,
    save_failed: bool,
}
impl Default for State {
    fn default() -> Self {
        let model = Multiplexer::default();
        let saved = model.to_json();
        Self {
            model,
            chrome: None,
            actions: HashMap::new(),
            group_actions: HashMap::new(),
            next: 0x4d58_1000,
            items: vec![],
            originals: HashMap::new(),
            identities: HashMap::new(),
            requested: std::collections::HashSet::new(),
            rendered: String::new(),
            catalog_key: String::new(),
            saved,
            notice: String::new(),
            store: None,
            queued: None,
            save_failed: false,
        }
    }
}
/// Device-local bounded restore. No daemon calls, credentials, or filesystem contents are persisted.
pub fn install(w: &mut World, path: PathBuf) {
    let mut state = State::default();
    match crate::multiplexer_storage::load(&path)
        .and_then(|v| v.map(|v| Multiplexer::from_json(&v)).transpose())
    {
        Ok(Some(model)) => {
            state.saved = model.to_json();
            state.model = model;
        }
        Ok(None) => {}
        Err(error) => state.notice = format!("Could not restore desktops: {error}"),
    }
    state.store = Some(Arc::new(crate::multiplexer_storage::Store::new(path)));
    w.insert_resource(state);
}
pub(crate) fn storage(w: &World) -> Option<Arc<crate::multiplexer_storage::Store>> {
    w.resource::<State>()?.store.clone()
}
fn panel(w: &mut World, p: Entity, layout: Layout) -> Entity {
    let e = w.spawn((layout,));
    set_parent(w, e, p);
    e
}
fn column(w: &mut World, p: Entity) -> Entity {
    panel(
        w,
        p,
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            ..Layout::column()
        },
    )
}
fn caption(w: &mut World, p: Entity, value: &str) {
    let e = text(w, p, value, Typography::Secondary).unwrap();
    w.get_mut::<Layout>(e).unwrap().width = Size::Fill;
    w.get_mut::<Label>(e).unwrap().overflow = TextOverflow::Wrap;
}
fn button(w: &mut World, p: Entity, s: &mut State, label: &str, action: Action) {
    s.next += 1;
    let id = s.next;
    let mut props = ButtonProps::new(label, HostIntent::Command(id));
    props.variant = controls::ButtonVariant::Quiet;
    let h = controls::Button::mount(w, p, UiId(id), props).unwrap();
    w.get_mut::<Layout>(h.root).unwrap().width = Size::Fill;
    if matches!(
        action,
        Action::Focus(_) | Action::CloseGroup(_) | Action::Reopen(_) | Action::Content(..)
    ) {
        s.group_actions.insert(id, action);
    } else {
        s.actions.insert(id, action);
    }
}
fn icon_row(w: &mut World, parent: Entity, s: &mut State, actions: &[(Icon, &str, Action)]) {
    let row = panel(
        w,
        parent,
        Layout {
            width: Size::Fill,
            height: Size::Px(34.),
            gap: 4.,
            ..Layout::row()
        },
    );
    for (icon, label, action) in actions {
        s.next += 1;
        let id = s.next;
        crate::native_style::icon_action(w, row, id, *icon, label, true);
        if matches!(action, Action::Content(..)) {
            s.group_actions.insert(id, action.clone());
        } else {
            s.actions.insert(id, action.clone());
        }
    }
}
pub fn mount(w: &mut World, parent: Entity) {
    if let Some(layout) = w.get_mut::<Layout>(parent) {
        *layout = Layout {
            width: Size::Fill,
            height: Size::Fill,
            ..Layout::row()
        };
    }
    let sidebar = panel(
        w,
        parent,
        Layout {
            width: Size::Px(252.),
            height: Size::Fill,
            padding: 12.,
            gap: 8.,
            ..Layout::column()
        },
    );
    crate::native_style::navigation_frame(w, sidebar);
    w.insert(sidebar, crate::native_style::SidebarPane);
    text(w, sidebar, "Workspace", Typography::Emphasis).unwrap();
    let name = w.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(34.),
            padding: 6.,
            ..Layout::column()
        },
        Label::new("Desktop 1"),
        Focusable,
        Prompt::new(),
        Placeholder::new("Desktop or layout name"),
        Semantics {
            name: Some("Desktop or layout name".into()),
            ..Default::default()
        },
    ));
    set_parent(w, name, sidebar);
    let catalog = column(w, sidebar);
    w.get_mut::<Layout>(catalog).unwrap().gap = 6.;
    w.insert(catalog, Scroll::default());
    let body = column(w, parent);
    crate::native_style::flat(w, body, crate::native_style::canvas());
    let parking = column(w, parent);
    w.insert(parking, Hidden);
    let mut s = w
        .resource_mut::<State>()
        .map(std::mem::take)
        .unwrap_or_default();
    w.get_mut::<Label>(name).unwrap().text = s.model.active().name.clone();
    s.chrome = Some(Chrome {
        name,
        catalog,
        body,
        parking,
        groups: vec![],
        splits: vec![],
    });
    w.insert_resource(s);
}
fn owner(w: &World, surface: SurfaceId) -> Option<String> {
    crate::workshops::binding(w, surface).map(|b| b.authority)
}
fn items(w: &World) -> Vec<Item> {
    let mut out = vec![];
    for c in crate::ui::conversations::workspace_items(w) {
        out.push(Item {
            resource: Resource::Chat {
                session_id: c.session.unwrap_or_else(|| format!("pending:{}", c.route)),
                workshop_id: c.owner,
            },
            title: c.title,
            entity: c.entity,
            local: Local::Chat(c.route),
            dirty: false,
        });
    }
    if let Some(workshop_id) = crate::terminal::workspace_owner(w) {
        for (id, title, entity, work, session) in crate::terminal::workspace_panes(w) {
            out.push(Item {
                resource: Resource::Terminal {
                    session_id: session.unwrap_or_else(|| format!("pending:{id}")),
                    workshop_id: workshop_id.clone(),
                    work_id: (!work.is_empty()).then_some(work),
                },
                title,
                entity,
                local: Local::Terminal(id),
                dirty: false,
            });
        }
    }
    if let Some(workshop_id) = owner(w, SurfaceId::Notes) {
        for n in crate::notes::workspace_items(w) {
            out.push(Item {
                resource: Resource::Note {
                    path: n.path.clone(),
                    workshop_id: workshop_id.clone(),
                },
                title: n.title,
                entity: n.entity,
                local: Local::Note(n.path),
                dirty: n.dirty,
            });
        }
    }
    if let Some(workshop_id) = owner(w, SurfaceId::Code) {
        for c in crate::code::workspace_items(w) {
            out.push(Item {
                resource: Resource::Code {
                    path: c.path.clone(),
                    work_id: c.work.clone(),
                    workshop_id: workshop_id.clone(),
                },
                title: c.title,
                entity: c.entity,
                local: Local::Code(c.work, c.path),
                dirty: c.dirty,
            });
        }
    }
    if let (Some(workshop_id), Some((document_id, entity))) = (
        owner(w, SurfaceId::Create),
        crate::create::workspace_item(w),
    ) {
        out.push(Item {
            resource: Resource::Create {
                document_id: document_id.clone(),
                workshop_id,
            },
            title: "Canvas".into(),
            entity,
            local: Local::Create(document_id),
            dirty: false,
        });
    }
    if let (Some(workshop_id), Some((work_id, entity))) = (
        owner(w, SurfaceId::Reviews),
        crate::reviews::workspace_item(w),
    ) {
        out.push(Item {
            resource: Resource::Review {
                work_id: work_id.clone(),
                workshop_id,
            },
            title: "Review".into(),
            entity,
            local: Local::Review(work_id),
            dirty: false,
        });
    }
    out
}
#[cfg(test)]
#[path = "multiplexer_tests.rs"]
mod tests;
fn pending_promotes(old: &Resource, new: &Resource) -> bool {
    match (old, new) {
        (
            Resource::Chat {
                session_id: a,
                workshop_id: x,
            },
            Resource::Chat {
                session_id: b,
                workshop_id: y,
            },
        ) => a.starts_with("pending:") && !b.starts_with("pending:") && x == y,
        (
            Resource::Terminal {
                session_id: a,
                workshop_id: x,
                work_id: w,
            },
            Resource::Terminal {
                session_id: b,
                workshop_id: y,
                work_id: v,
            },
        ) => a.starts_with("pending:") && !b.starts_with("pending:") && x == y && w == v,
        _ => false,
    }
}
fn move_at(m: &mut Multiplexer, tab: u64, group: u64, boundary: usize) -> Result<(), String> {
    let source = m.active().tab_group(tab);
    let from = source.and_then(|g| m.active().group(g)?.tabs.iter().position(|t| t.id == tab));
    let adjusted = boundary.saturating_sub(usize::from(
        source == Some(group) && from.is_some_and(|i| i < boundary),
    ));
    m.move_tab(tab, group, adjusted)
}
fn surface(r: &Resource) -> SurfaceId {
    match r {
        Resource::Chat { .. } => SurfaceId::Chat,
        Resource::Terminal { .. } => SurfaceId::Terminal,
        Resource::Note { .. } => SurfaceId::Notes,
        Resource::Code { .. } => SurfaceId::Code,
        Resource::Create { .. } => SurfaceId::Create,
        Resource::Review { .. } => SurfaceId::Reviews,
    }
}
pub fn focused_surface(w: &World) -> Option<SurfaceId> {
    let s = w.resource::<State>()?;
    let d = s.model.active();
    let resource = &d.group(d.focused_group)?.active()?.resource;
    s.items
        .iter()
        .any(|i| &i.resource == resource)
        .then(|| surface(resource))
}
fn focus_item(w: &mut World, item: &Item) {
    match &item.local {
        Local::Chat(r) => {
            crate::ui::conversations::workspace_focus(w, *r);
        }
        Local::Note(p) => {
            crate::notes::workspace_focus(w, p);
        }
        Local::Code(work, p) => {
            crate::code::workspace_focus(w, work, p);
        }
        _ => {}
    }
}
pub fn focus_entity(w: &mut World, e: Entity) {
    if crate::surfaces::active(w) != SurfaceId::Workspace {
        return;
    }
    let target = w.resource::<State>().and_then(|s| {
        let c = s.chrome.as_ref()?;
        let g = c
            .groups
            .iter()
            .find(|g| crate::surfaces::descendant(w, e, g.root))?;
        let item = s
            .model
            .active()
            .group(g.id)?
            .active()
            .and_then(|t| s.items.iter().find(|i| i.resource == t.resource))
            .cloned();
        Some((g.id, item))
    });
    if let Some((id, item)) = target {
        if !can_deactivate(w) {
            return;
        }
        let _ = w.resource_mut::<State>().unwrap().model.focus_group(id);
        if let Some(item) = item {
            focus_item(w, &item);
        }
    }
}
pub fn can_deactivate(w: &World) -> bool {
    crate::ui::conversations::workspace_can_deactivate(w)
        && crate::notes::can_deactivate(w)
        && crate::code::can_deactivate(w)
        && crate::create::can_deactivate(w)
}
fn show(w: &mut World, visible: &[Item]) {
    let chats: Vec<_> = visible
        .iter()
        .filter_map(|i| {
            if let Local::Chat(r) = i.local {
                Some(r)
            } else {
                None
            }
        })
        .collect();
    crate::ui::conversations::workspace_show(w, &chats);
    let terminals: Vec<_> = visible
        .iter()
        .filter_map(|i| {
            if let Local::Terminal(r) = i.local {
                Some(r)
            } else {
                None
            }
        })
        .collect();
    crate::terminal::workspace_show(w, &terminals);
    let notes: Vec<_> = visible
        .iter()
        .filter_map(|i| {
            if let Local::Note(ref p) = i.local {
                Some(p.clone())
            } else {
                None
            }
        })
        .collect();
    crate::notes::workspace_show(w, &notes);
    let code: Vec<_> = visible
        .iter()
        .filter_map(|i| {
            if let Local::Code(ref a, ref b) = i.local {
                Some((a.clone(), b.clone()))
            } else {
                None
            }
        })
        .collect();
    crate::code::workspace_show(w, &code);
}
fn park(w: &mut World, s: &mut State) {
    let Some(c) = s.chrome.as_ref() else {
        return;
    };
    for (&e, &original) in &s.originals {
        if w.get::<Layout>(e).is_some() {
            if w.get::<Layout>(original).is_none() {
                continue;
            }
            set_parent(w, e, c.parking);
            w.insert(e, Hidden);
        }
    }
}
pub fn leaving(w: &mut World, target: SurfaceId) {
    if target == SurfaceId::Workspace {
        return;
    }
    let Some(mut s) = w.resource_mut::<State>().map(std::mem::take) else {
        return;
    };
    for (e, p) in s.originals.drain() {
        if w.get::<Layout>(e).is_some() && w.get::<Layout>(p).is_some() {
            set_parent(w, e, p);
            w.remove::<Hidden>(e);
        }
    }
    s.rendered.clear();
    w.insert_resource(s);
    show(w, &[]);
    crate::terminal::workspace_restore(w);
}
pub fn activate(w: &mut World) {
    if let Some(s) = w.resource_mut::<State>() {
        s.rendered.clear();
        s.catalog_key.clear();
    }
    restore_active(w);
    sync(w);
}
/// Declared before adapters consume input. Focus follows the pressed pane, not the nav icon.
pub fn input(w: &mut World) {
    if crate::surfaces::active(w) != SurfaceId::Workspace {
        return;
    }
    if let Some(e) = w
        .resource::<Input>()
        .filter(|i| i.pointer.pressed)
        .and_then(|i| hit_test(w, i.pointer.position[0], i.pointer.position[1]))
    {
        focus_entity(w, e);
    } else if let Some(e) = w.resource::<Focus>().and_then(|f| f.entity) {
        focus_entity(w, e);
    }
    let tab_events = events::<TabStripEvent>(w).to_vec();
    for e in tab_events {
        if !can_deactivate(w) {
            continue;
        }
        let Some(mut s) = w.resource_mut::<State>().map(std::mem::take) else {
            return;
        };
        let group = s.chrome.as_ref().and_then(|c| {
            c.groups
                .iter()
                .find(|g| g.strip.is_some_and(|h| h.root == e.strip))
                .map(|g| g.id)
        });
        if let Some(group) = group {
            let result = match e.kind {
                TabStripEventKind::Activated(TabId(tab)) => s.model.activate_tab(tab),
                TabStripEventKind::CloseRequested(TabId(tab)) => s.model.close_tab(tab),
                TabStripEventKind::Reordered(m) => {
                    let tabs = &s.model.active().group(group).unwrap().tabs;
                    let at = tabs
                        .iter()
                        .position(|t| t.id == m.target.0)
                        .unwrap_or(tabs.len());
                    move_at(
                        &mut s.model,
                        m.id.0,
                        group,
                        at + usize::from(m.place == rndr_ui::drag::DropPlace::After),
                    )
                }
                TabStripEventKind::Moved(m) => {
                    let tabs = &s.model.active().group(group).unwrap().tabs;
                    let at = m
                        .target
                        .and_then(|id| tabs.iter().position(|t| t.id == id.0))
                        .unwrap_or(tabs.len());
                    move_at(
                        &mut s.model,
                        m.id.0,
                        group,
                        at + usize::from(m.place == rndr_ui::drag::DropPlace::After),
                    )
                }
                TabStripEventKind::SplitRequested(req) => {
                    let edge = match req.direction {
                        SplitDirection::Left => Edge::Left,
                        SplitDirection::Right => Edge::Right,
                        SplitDirection::Above => Edge::Top,
                        SplitDirection::Below => Edge::Bottom,
                    };
                    s.model
                        .split_group(group, edge)
                        .and_then(|g| s.model.move_tab(req.id.0, g, 0))
                }
            };
            if let Err(e) = result {
                s.notice = e;
            }
            s.rendered.clear();
        }
        w.insert_resource(s);
    }
    if let Some(drop) = rndr_ui::drag::committed(w) {
        empty_drop(w, drop);
    }
    let changes = events::<SplitSashChange>(w).to_vec();
    for e in changes {
        let path = w.resource::<State>().and_then(|s| {
            s.chrome
                .as_ref()?
                .splits
                .iter()
                .find(|(_, h)| h.sash == e.sash)
                .map(|(p, _)| p.clone())
        });
        if let Some(path) = path {
            let _ = w
                .resource_mut::<State>()
                .unwrap()
                .model
                .set_split_ratio(&path, e.ratio);
        }
    }
}
fn empty_drop(w: &mut World, drop: rndr_ui::drag::DragDrop) -> bool {
    if drop.payload.kind != rndr_ui::tabs::TAB || !can_deactivate(w) {
        return false;
    }
    let target = w.resource::<State>().and_then(|s| {
        s.chrome
            .as_ref()?
            .groups
            .iter()
            .find(|g| g.strip.is_none() && drop.target == g.root)
            .map(|g| g.id)
    });
    let owned = w.resource::<State>().is_some_and(|s| {
        s.chrome.as_ref().is_some_and(|c| {
            c.groups
                .iter()
                .any(|g| g.strip.is_some_and(|h| Some(h.root) == drop.payload.owner))
        })
    });
    if let Some(group) = target
        && owned
    {
        let s = w.resource_mut::<State>().unwrap();
        if let Err(error) = s.model.move_tab(drop.payload.value, group, 0) {
            s.notice = error;
            return false;
        }
        s.rendered.clear();
        return true;
    }
    false
}
fn named(w: &World, s: &State) -> String {
    s.chrome
        .as_ref()
        .and_then(|c| w.get::<Label>(c.name))
        .map(|l| l.text.trim().to_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| s.model.active().name.clone())
}
pub fn action(w: &mut World, intent: &HostIntent) -> bool {
    let HostIntent::Command(id) = intent else {
        return false;
    };
    let Some(a) = w
        .resource::<State>()
        .and_then(|s| s.actions.get(id).or_else(|| s.group_actions.get(id)))
        .cloned()
    else {
        return false;
    };
    if !can_deactivate(w) {
        return true;
    }
    match a {
        Action::Browse(surface) => {
            crate::surfaces::select(w, surface);
            return true;
        }
        Action::NewChat => {
            crate::ui::conversations::workspace_new(w);
            let entries = items(w);
            if let Some(item) = entries
                .iter()
                .filter(|i| matches!(i.local, Local::Chat(_)))
                .last()
            {
                let _ = w
                    .resource_mut::<State>()
                    .unwrap()
                    .model
                    .open(item.resource.clone(), item.title.clone());
            }
        }
        Action::NewShell => {
            if let Some(id) = crate::terminal::workspace_new(w) {
                if let Some(item) = items(w)
                    .into_iter()
                    .find(|i| i.local == Local::Terminal(id))
                {
                    let _ = w
                        .resource_mut::<State>()
                        .unwrap()
                        .model
                        .open(item.resource, item.title);
                }
            }
        }
        Action::Reopen(resource) => {
            if let Some(s) = w.resource_mut::<State>() {
                s.requested.remove(&resource);
            }
            reopen(w, &resource);
        }
        Action::Content(resource, command) => {
            let item = items(w).into_iter().find(|i| i.resource == resource);
            if let Some(item) = item {
                focus_item(w, &item);
                match surface(&resource) {
                    SurfaceId::Notes => {
                        crate::notes::shortcut(w, command);
                    }
                    SurfaceId::Code => {
                        crate::code::shortcut(w, command);
                    }
                    _ => {}
                }
            }
        }
        other => {
            let mut s = std::mem::take(w.resource_mut::<State>().unwrap());
            let name = named(w, &s);
            let desktop = s.model.active_desktop;
            let group = s.model.active().focused_group;
            let result = match other {
                Action::Open(r) => {
                    let title = s
                        .items
                        .iter()
                        .find(|i| i.resource == r)
                        .map(|i| i.title.clone())
                        .unwrap_or_else(|| r.kind().into());
                    let existing = s
                        .model
                        .active()
                        .groups
                        .iter()
                        .flat_map(|g| &g.tabs)
                        .find(|t| t.resource == r)
                        .map(|t| t.id);
                    if let Some(tab) = existing {
                        if s.model.active().tab_group(tab) == Some(group) {
                            s.model.activate_tab(tab)
                        } else {
                            s.model.move_tab(tab, group, usize::MAX)
                        }
                    } else {
                        s.model.open(r, title).map(|_| ())
                    }
                }
                Action::Desktop(id) => s.model.select_desktop(id),
                Action::NewDesktop => {
                    let name = if name == s.model.active().name {
                        (1..=33)
                            .map(|n| format!("Desktop {n}"))
                            .find(|name| !s.model.desktops.iter().any(|d| &d.name == name))
                            .unwrap_or(name)
                    } else {
                        name
                    };
                    s.model.create_desktop(name).map(|_| ())
                }
                Action::Duplicate => s
                    .model
                    .duplicate_desktop(desktop, format!("{} copy", s.model.active().name))
                    .map(|_| ()),
                Action::Rename => s.model.rename_desktop(desktop, name),
                Action::Delete => s.model.delete_desktop(desktop),
                Action::SaveTemplate => s.model.save_template(name).map(|_| ()),
                Action::RetrySave => {
                    s.save_failed = false;
                    s.queued = None;
                    s.notice.clear();
                    Ok(())
                }
                Action::ApplyTemplate(id) => s.model.apply_template(id),
                Action::DeleteTemplate(id) => s.model.delete_template(id),
                Action::Split(edge) => s.model.split_group(group, edge).map(|_| ()),
                Action::Zoom => s.model.toggle_zoom(group),
                Action::Focus(id) => s.model.focus_group(id),
                Action::CloseGroup(id) => s.model.remove_empty_group(id),
                _ => Ok(()),
            };
            if let Err(e) = result {
                s.notice = e;
            }
            if let Some(c) = &s.chrome {
                w.get_mut::<Label>(c.name).unwrap().text = s.model.active().name.clone();
            }
            s.rendered.clear();
            s.catalog_key.clear();
            w.insert_resource(s);
        }
    }
    if let Some(s) = w.resource_mut::<State>() {
        s.rendered.clear();
        s.catalog_key.clear();
    }
    sync(w);
    true
}
fn restore_active(w: &mut World) {
    let missing: Vec<_> = w
        .resource::<State>()
        .map(|s| {
            s.model
                .active()
                .groups
                .iter()
                .filter_map(|g| g.active())
                .map(|t| t.resource.clone())
                .filter(|r| !s.items.iter().any(|i| &i.resource == r) && !s.requested.contains(r))
                .collect()
        })
        .unwrap_or_default();
    let mut singletons = std::collections::HashSet::new();
    for r in missing {
        if matches!(r, Resource::Create { .. } | Resource::Review { .. })
            && !singletons.insert(r.kind())
        {
            continue;
        }
        reopen(w, &r);
    }
}
fn reopen(w: &mut World, r: &Resource) {
    let owner_id = match r {
        Resource::Chat { workshop_id, .. }
        | Resource::Terminal { workshop_id, .. }
        | Resource::Note { workshop_id, .. }
        | Resource::Code { workshop_id, .. }
        | Resource::Create { workshop_id, .. }
        | Resource::Review { workshop_id, .. } => workshop_id,
    };
    let target = surface(r);
    if target != SurfaceId::Chat && owner(w, target).as_deref() != Some(owner_id) {
        w.resource_mut::<State>().unwrap().notice="Open this resource from its owning workshop in the focused view, then return to Workspace. Its reference has been preserved.".into();
        return;
    }
    let accepted = match r {
        Resource::Chat {
            session_id,
            workshop_id,
        } => crate::ui::conversations::workspace_open(w, workshop_id, session_id),
        Resource::Terminal {
            session_id,
            work_id,
            ..
        } if !session_id.starts_with("pending:") => crate::terminal::workspace_attach(
            w,
            work_id.clone().unwrap_or_default(),
            session_id.clone(),
        )
        .is_some(),
        Resource::Note { path, .. } => crate::notes::workspace_open(w, path),
        Resource::Code { work_id, path, .. } => crate::code::workspace_open(w, work_id, path),
        Resource::Create { document_id, .. } => crate::create::workspace_open(w, document_id),
        Resource::Review { work_id, .. } => crate::reviews::workspace_open(w, work_id),
        _ => false,
    };
    let s = w.resource_mut::<State>().unwrap();
    if accepted {
        s.requested.insert(r.clone());
    } else {
        s.notice="Resource could not be reopened yet. Check its owning workshop and try again; no replacement was created.".into();
    }
}
fn clear(w: &mut World, p: Entity) {
    for e in children_of(w, p) {
        despawn(w, e);
    }
}
fn render_catalog(w: &mut World, s: &mut State) {
    let c = s.chrome.as_ref().unwrap();
    let catalog = c.catalog;
    clear(w, catalog);
    s.actions.clear();
    caption(w, catalog, "VIRTUAL DESKTOPS");
    for d in s.model.desktops.clone() {
        let label = if d.id == s.model.active_desktop {
            format!("● {}", d.name)
        } else {
            d.name
        };
        button(w, catalog, s, &label, Action::Desktop(d.id));
    }
    icon_row(
        w,
        catalog,
        s,
        &[
            (Icon::Plus, "New desktop", Action::NewDesktop),
            (Icon::Pencil, "Rename desktop", Action::Rename),
            (Icon::Copy, "Duplicate desktop", Action::Duplicate),
            (Icon::Trash, "Delete desktop", Action::Delete),
        ],
    );
    caption(w, catalog, "ARRANGE");
    icon_row(
        w,
        catalog,
        s,
        &[
            (Icon::Code, "Split right", Action::Split(Edge::Right)),
            (Icon::Code, "Split down", Action::Split(Edge::Bottom)),
            (Icon::Maximize2, "Zoom / restore pane", Action::Zoom),
        ],
    );
    caption(w, catalog, "NEW / OPEN EXISTING");
    button(w, catalog, s, "New chat", Action::NewChat);
    button(w, catalog, s, "New shell", Action::NewShell);
    for (label, surface) in [
        ("Browse chats", SurfaceId::Chat),
        ("Browse notes", SurfaceId::Notes),
        ("Browse code", SurfaceId::Code),
        ("Browse reviews", SurfaceId::Reviews),
        ("Browse Create", SurfaceId::Create),
        ("Browse shells", SurfaceId::Terminal),
    ] {
        button(w, catalog, s, label, Action::Browse(surface));
    }
    for item in s.items.clone() {
        button(
            w,
            catalog,
            s,
            &format!("{} · {}", item.resource.kind(), item.title),
            Action::Open(item.resource),
        );
    }
    caption(w, catalog, "SAVED LAYOUTS");
    button(w, catalog, s, "Save arrangement", Action::SaveTemplate);
    for t in s.model.templates.clone() {
        button(
            w,
            catalog,
            s,
            &format!("Apply {}", t.name),
            Action::ApplyTemplate(t.id),
        );
        button(
            w,
            catalog,
            s,
            &format!("Delete {}", t.name),
            Action::DeleteTemplate(t.id),
        );
    }
    if !s.notice.is_empty() {
        caption(w, catalog, &s.notice);
    }
    if s.save_failed {
        button(
            w,
            catalog,
            s,
            "Retry saving desktop layout",
            Action::RetrySave,
        );
    }
    s.catalog_key = catalog_key(s);
}
fn catalog_key(s: &State) -> String {
    format!(
        "{}|{}|{:?}|{}",
        serde_json::json!({"active":s.model.active_desktop,"desktops":s.model.desktops.iter().map(|d|serde_json::json!([d.id,d.name])).collect::<Vec<_>>(),"templates":s.model.templates.iter().map(|t|serde_json::json!([t.id,t.name])).collect::<Vec<_>>()}),
        s.notice,
        s.items
            .iter()
            .map(|i| (&i.resource, &i.title, i.dirty))
            .collect::<Vec<_>>(),
        s.save_failed
    )
}
fn build(w: &mut World, s: &mut State, parent: Entity, node: &SplitNode, path: Vec<bool>) {
    match node {
        SplitNode::Split {
            axis,
            ratio,
            first,
            second,
        } => {
            let mut props = SplitSashProps::new("Workspace split");
            props.axis = match axis {
                model::Axis::Horizontal => Axis::Row,
                model::Axis::Vertical => Axis::Column,
            };
            props.min_leading = 80.;
            props.min_trailing = 80.;
            let h = SplitSash::mount(w, parent, UiId(0x4d58_0000 + s.next), props, *ratio).unwrap();
            s.next += 1;
            s.chrome.as_mut().unwrap().splits.push((path.clone(), h));
            let mut a = path.clone();
            a.push(false);
            build(w, s, h.leading, first, a);
            let mut b = path;
            b.push(true);
            build(w, s, h.trailing, second, b);
        }
        SplitNode::Group(id) => {
            let Some(g) = s.model.active().group(*id).cloned() else {
                return;
            };
            let root = column(w, parent);
            let body;
            let strip;
            if g.tabs.is_empty() {
                caption(
                    w,
                    root,
                    "Open an existing resource from the sidebar, or create a chat or shell.",
                );
                button(w, root, s, "Focus this group", Action::Focus(*id));
                if s.model.active().groups.len() > 1 {
                    button(w, root, s, "Close empty group", Action::CloseGroup(*id));
                }
                body = column(w, root);
                w.insert(
                    root,
                    rndr_ui::drag::DropTarget {
                        enabled: true,
                        kinds: vec![rndr_ui::tabs::TAB],
                        owner: None,
                        geometry: rndr_ui::drag::DropGeometry {
                            axis: Axis::Column,
                            inside: 1.,
                            edge: 0.,
                            cross_edge: 0.,
                        },
                        gate: None,
                    },
                );
                strip = None;
            } else {
                let specs: Vec<_> = g
                    .tabs
                    .iter()
                    .map(|t| {
                        let dirty = s
                            .items
                            .iter()
                            .find(|i| i.resource == t.resource)
                            .is_some_and(|i| i.dirty);
                        TabSpec::new(t.id, &t.title).dirty(dirty)
                    })
                    .collect();
                let h = TabStrip::mount(
                    w,
                    root,
                    UiId(*id + 0x4d58_0000),
                    TabStripProps::new("Workspace tabs")
                        .active(s.model.active().focused_group == *id),
                    &specs,
                    TabId(g.active_tab.unwrap()),
                )
                .unwrap();
                body = h.panel(w, TabId(g.active_tab.unwrap())).unwrap();
                strip = Some(h);
                if let Some(tab) = g.active() {
                    if matches!(tab.resource, Resource::Note { .. } | Resource::Code { .. })
                        && s.items.iter().any(|i| i.resource == tab.resource)
                    {
                        let mut actions = vec![
                            (
                                Icon::Save,
                                "Save",
                                Action::Content(tab.resource.clone(), "save"),
                            ),
                            (
                                Icon::Search,
                                "Find",
                                Action::Content(tab.resource.clone(), "find"),
                            ),
                        ];
                        if matches!(tab.resource, Resource::Note { .. }) {
                            actions.extend([
                                (
                                    Icon::Pencil,
                                    "Write",
                                    Action::Content(tab.resource.clone(), "write"),
                                ),
                                (
                                    Icon::Book,
                                    "Read",
                                    Action::Content(tab.resource.clone(), "read"),
                                ),
                                (
                                    Icon::Code,
                                    "Source",
                                    Action::Content(tab.resource.clone(), "source"),
                                ),
                            ]);
                        }
                        icon_row(w, body, s, &actions);
                    }
                    if !s.items.iter().any(|i| i.resource == tab.resource) {
                        caption(
                            w,
                            body,
                            "This resource is not open in this window. Its owning workshop and identity are preserved.",
                        );
                        button(
                            w,
                            body,
                            s,
                            "Open resource",
                            Action::Reopen(tab.resource.clone()),
                        );
                    }
                }
            }
            s.chrome.as_mut().unwrap().groups.push(GroupView {
                id: *id,
                root,
                body,
                strip,
            });
        }
    }
}
pub fn sync(w: &mut World) {
    if crate::surfaces::active(w) != SurfaceId::Workspace {
        return;
    }
    let fresh = items(w);
    let Some(mut s) = w.resource_mut::<State>().map(std::mem::take) else {
        return;
    };
    if s.chrome.is_none() {
        w.insert_resource(s);
        return;
    }
    if let Some(result) = s.store.as_ref().and_then(|store| store.poll()) {
        match result {
            Ok(value) => {
                if s.queued.as_ref() == Some(&value) {
                    s.queued = None;
                }
                s.saved = value;
                s.save_failed = false;
            }
            Err(error) => {
                s.notice = format!("Desktop layout is not saved: {error}");
                s.queued = None;
                s.save_failed = true;
            }
        }
    }
    // Admission promotes the same draft/attachment reference instead of opening a duplicate.
    for item in &fresh {
        if let Some(old) = s.identities.get(&item.entity) {
            if pending_promotes(old, &item.resource) {
                if let Err(error) = s.model.replace_resource(old, item.resource.clone()) {
                    s.notice = error;
                }
            }
        }
        s.identities.insert(item.entity, item.resource.clone());
    }
    for item in &fresh {
        for d in &mut s.model.desktops {
            for g in &mut d.groups {
                for t in &mut g.tabs {
                    if t.resource == item.resource {
                        t.title = item.title.clone();
                    }
                }
            }
        }
    }
    s.items = fresh;
    let key = catalog_key(&s);
    let mut geometry = s.model.to_json(); // Ratios resize in-place; topology/focus/tab changes rebuild.
    fn scrub(v: &mut serde_json::Value) {
        if let Some(o) = v.as_object_mut() {
            o.remove("ratio");
            for v in o.values_mut() {
                scrub(v);
            }
        } else if let Some(a) = v.as_array_mut() {
            for v in a {
                scrub(v);
            }
        }
    }
    scrub(&mut geometry);
    let render_key = format!(
        "{geometry}|{:?}",
        s.items
            .iter()
            .map(|i| (&i.resource, i.entity, i.dirty))
            .collect::<Vec<_>>()
    );
    if s.catalog_key != key {
        render_catalog(w, &mut s);
    }
    if s.rendered != render_key {
        park(w, &mut s);
        let c = s.chrome.as_mut().unwrap();
        let body = c.body;
        c.groups.clear();
        c.splits.clear();
        s.group_actions.clear();
        clear(w, body);
        let d = s.model.active();
        let tree = if let Some(g) = d.zoomed_group {
            SplitNode::Group(g)
        } else {
            d.root.clone()
        };
        build(w, &mut s, body, &tree, vec![]);
        s.rendered = render_key;
    }
    let mut visible = vec![];
    let groups: Vec<_> = s
        .chrome
        .as_ref()
        .unwrap()
        .groups
        .iter()
        .map(|g| (g.id, g.body))
        .collect();
    for (id, body) in groups {
        if let Some(tab) = s.model.active().group(id).and_then(|g| g.active()) {
            if let Some(item) = s.items.iter().find(|i| i.resource == tab.resource).cloned() {
                if !s.originals.contains_key(&item.entity) {
                    if let Some(p) = w.get::<Parent>(item.entity) {
                        s.originals.insert(item.entity, p.entity);
                    }
                }
                set_parent(w, item.entity, body);
                w.remove::<Hidden>(item.entity);
                visible.push(item);
            }
        }
    }
    // Park hidden tabs/desktops while retaining sessions, editor documents, undo and scroll.
    let parking = s.chrome.as_ref().unwrap().parking;
    for e in s.originals.keys().copied().collect::<Vec<_>>() {
        if !visible.iter().any(|i| i.entity == e) && w.get::<Layout>(e).is_some() {
            set_parent(w, e, parking);
            w.insert(e, Hidden);
        }
    }
    let focused = s
        .model
        .active()
        .group(s.model.active().focused_group)
        .and_then(|g| g.active())
        .and_then(|t| s.items.iter().find(|i| i.resource == t.resource))
        .cloned();
    let value = s.model.to_json();
    if value != s.saved && s.queued.as_ref() != Some(&value) && !s.save_failed {
        if let Some(store) = &s.store {
            if store.queue(value.clone()) {
                s.queued = Some(value);
            } else {
                s.notice = "Desktop layout is not saved: writer unavailable.".into();
                s.save_failed = true;
            }
        }
    }
    if s.queued.is_some() {
        frame_wake(w).wake();
    }
    w.insert_resource(s);
    show(w, &visible);
    if let Some(item) = focused {
        focus_item(w, &item);
    }
}
