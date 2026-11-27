//! Retained conversation workspaces. ShellCore owns tab/group topology; each
//! route owns one canonical Chat, editor document, retained view and transport.
//! The Model/Ui resources are a synchronous context, never a second state copy.
use super::*;
use medousa_app_core::shell::SplitEdge;
use medousa_app_core::{ShellCommand, ShellCore, ShellTab};
use rndr_ui::{
    split::{SplitSash, SplitSashHandle, SplitSashProps},
    tabs::{
        TabId, TabSpec, TabStrip, TabStripEvent, TabStripEventKind, TabStripHandle, TabStripProps,
    },
};

const SPLIT: u64 = 40;
const JOIN: u64 = 41;
struct BandRoute(u64);
impl Component for BandRoute {}
struct Parked {
    model: Model,
    ui: Ui,
}
struct Deck {
    shell: ShellCore,
    parked: HashMap<u64, Parked>,
    next: u64,
    workspace_visible: Vec<u64>,
    split: SplitSashHandle,
    parents: [Entity; 2],
    bodies: [Entity; 2],
    geometry: (u32, bool, usize),
    settling: u8,
    ratio: f64,
    strips: [Option<Entity>; 2],
    strip_cache: [Option<(Vec<TabSpec>, TabId, bool, TabStripHandle)>; 2],
    closed: std::collections::HashSet<u64>,
    /// Unsent text of released conversations, by session. Reopening one
    /// starts its composer with this instead of keeping the whole tab alive.
    stashed: HashMap<String, String>,
    empty: Entity,
}
fn command(world: &mut World, command: ShellCommand) {
    if let Err(e) = world
        .resource_mut::<Deck>()
        .unwrap()
        .shell
        .dispatch(command)
    {
        world.resource_mut::<Model>().unwrap().chat.status = format!("Workspace: {e}");
    }
}
fn snapshot(world: &World) -> medousa_app_core::ShellSnapshot {
    world
        .resource::<Deck>()
        .unwrap()
        .shell
        .snapshot()
        .expect("valid shell")
}
fn id(route: u64) -> String {
    format!("chat-{route}")
}
fn route(id: &str) -> Option<u64> {
    id.strip_prefix("chat-")?.parse().ok()
}
fn current(world: &World) -> u64 {
    world.resource::<Model>().unwrap().generation
}
fn shared(ui: Ui) -> Shared {
    Shared {
        sidebar: ui.sidebar,
        sessions: ui.sessions,
        search: ui.search,
        details: ui.details,
        context: ui.context,
    }
}
pub(super) fn mount(world: &mut World, parent: Entity) -> Entity {
    let mut props = SplitSashProps::new("Conversation panes");
    props.min_leading = 0.;
    props.min_trailing = 0.;
    props.enabled = false;
    let split = SplitSash::mount(world, parent, UiId(0x4348_0001), props, 1.).unwrap();
    let parents = [split.leading, split.trailing];
    let bodies = parents.map(|p| {
        container(
            world,
            Some(p),
            Layout {
                width: Size::Fill,
                height: Size::Fill,
                ..Layout::column()
            },
        )
    });
    for p in parents {
        *world.get_mut::<Layout>(p).unwrap() = Layout {
            width: Size::Fill,
            height: Size::Fill,
            ..Layout::column()
        };
    }
    let empty = container(
        world,
        Some(bodies[0]),
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            padding: 24.,
            gap: 12.,
            ..Layout::column()
        },
    );
    text(
        world,
        empty,
        "Open a conversation, or start a new chat.",
        Typography::Label,
    )
    .unwrap();
    button(world, empty, NEW, "New chat", true);
    world.insert_resource(Deck {
        shell: ShellCore::new("native-local"),
        parked: HashMap::new(),
        next: 1,
        workspace_visible: Vec::new(),
        split,
        parents,
        bodies,
        geometry: (0, false, 0),
        settling: 3,
        ratio: 0.5,
        strips: [None, None],
        strip_cache: [None, None],
        closed: Default::default(),
        stashed: HashMap::new(),
        empty,
    });
    bodies[0]
}
/// A retained chat view; the mixed workspace borrows its entity, never its model.
#[derive(Clone, Debug)]
pub(crate) struct WorkspaceChat {
    pub route: u64,
    pub session: Option<String>,
    pub owner: String,
    pub title: String,
    pub entity: Entity,
}
/// Known conversations, newest first, for the Workspace picker: (session, title).
pub(crate) fn workspace_catalog(world: &World) -> Vec<(String, String)> {
    world
        .resource::<Model>()
        .map(|m| m.sessions.clone())
        .unwrap_or_default()
}
pub(crate) fn workspace_items(world: &World) -> Vec<WorkspaceChat> {
    let Some(deck) = world.resource::<Deck>() else {
        return Vec::new();
    };
    let item = |route, m: &Model, ui: &Ui| {
        let session = m
            .chat
            .session()
            .map(str::to_owned)
            .or_else(|| m.requested_session.clone());
        let title = session
            .as_ref()
            .and_then(|s| m.sessions.iter().find(|row| &row.0 == s))
            .map(|row| row.1.clone())
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| {
                if session.is_some() {
                    "Untitled chat"
                } else {
                    "New chat"
                }
                .into()
            });
        WorkspaceChat {
            route,
            session,
            owner: m.owner.clone(),
            title,
            entity: ui.content,
        }
    };
    let mut items: Vec<_> = deck
        .parked
        .iter()
        .filter(|(r, _)| !deck.closed.contains(r))
        .map(|(r, p)| item(*r, &p.model, &p.ui))
        .collect();
    if let (Some(m), Some(ui)) = (world.resource::<Model>(), world.resource::<Ui>())
        && !deck.closed.contains(&m.generation)
    {
        items.push(item(m.generation, m, ui));
    }
    items.sort_by_key(|i| i.route);
    items
}
pub(crate) fn workspace_focus(world: &mut World, route: u64) -> bool {
    if !workspace_items(world).iter().any(|i| i.route == route) {
        return false;
    }
    if current(world) != route {
        focus(world, route);
    }
    if current(world) == route
        && !snapshot(world)
            .layout
            .tabs
            .iter()
            .any(|tab| tab.id() == id(route))
    {
        register(world);
    }
    current(world) == route
}
pub(crate) fn workspace_new(world: &mut World) {
    open(world, None);
}
/// Visibility and parent ownership remain with the mixed workspace coordinator.
pub(crate) fn workspace_show(world: &mut World, routes: &[u64]) {
    let known = workspace_items(world);
    let visible = routes
        .iter()
        .copied()
        .filter(|r| known.iter().any(|i| i.route == *r))
        .collect();
    if let Some(deck) = world.resource_mut::<Deck>() {
        deck.workspace_visible = visible;
    }
}
/// Swap the synchronous projection context, not UI entities or documents.
fn swap(world: &mut World, target: u64) -> bool {
    let old = current(world);
    if old == target {
        return true;
    }
    let Some(mut next) = world.resource_mut::<Deck>().unwrap().parked.remove(&target) else {
        return false;
    };
    let m = world.resource::<Model>().unwrap();
    // Only device chrome and the unified index are shared. Command channels and
    // catalogs remain with the immutable owner of each retained conversation.
    let selected = next
        .model
        .selected
        .and_then(|i| next.model.models.get(i))
        .cloned();
    next.model.sessions = m.sessions.clone();
    next.model.session_cursor = m.session_cursor.clone();
    next.model.search_query = m.search_query.clone();
    next.model.selected = selected.and_then(|v| next.model.models.iter().position(|x| x == &v));
    next.model.sidebar_open = m.sidebar_open;
    next.model.details_open = m.details_open;
    next.model.sidebar_key.clear(); // shared navigation paints the focused route last
    let model = std::mem::replace(world.resource_mut::<Model>().unwrap(), next.model);
    let ui = std::mem::replace(world.resource_mut::<Ui>().unwrap(), next.ui);
    world
        .resource_mut::<Deck>()
        .unwrap()
        .parked
        .insert(old, Parked { model, ui });
    true
}
pub(crate) fn workspace_can_deactivate(world: &World) -> bool {
    let Some(docs) = world.resource::<SourceDocuments>() else {
        return true;
    };
    let current = world
        .resource::<Model>()
        .is_some_and(|m| docs.get(m.document).is_some_and(|d| d.is_composing()));
    !current
        && world.resource::<Deck>().is_none_or(|deck| {
            deck.parked
                .values()
                .all(|p| docs.get(p.model.document).is_none_or(|d| !d.is_composing()))
        })
}
pub(crate) fn workspace_open(world: &mut World, owner: &str, session: &str) -> bool {
    if session.starts_with("pending:") || !workspace_can_deactivate(world) {
        return false;
    }
    if let Some(item) = workspace_items(world)
        .into_iter()
        .find(|i| i.owner == owner && i.session.as_deref() == Some(session))
    {
        return workspace_focus(world, item.route);
    }
    // The normal opener uses the authoritative session catalog. Never let its
    // lookup choose another daemon when a persisted reference names an owner.
    if crate::workshops::owner(world, Some(session)).as_deref() != Some(owner) {
        return false;
    }
    open(world, Some(session.into()));
    workspace_items(world)
        .iter()
        .any(|i| i.owner == owner && i.session.as_deref() == Some(session))
}
fn composing(world: &World) -> bool {
    let m = world.resource::<Model>().unwrap();
    world
        .resource::<SourceDocuments>()
        .unwrap()
        .get(m.document)
        .is_some_and(|d| d.is_composing())
}
fn focus(world: &mut World, target: u64) {
    if target != current(world) && composing(world) {
        return;
    }
    let previous_root = world.resource::<Ui>().unwrap().content;
    let previous = current(world);
    if swap(world, target) {
        if previous != target
            && world
                .resource::<Focus>()
                .and_then(|f| f.entity)
                .is_some_and(|e| within(world, e, previous_root))
        {
            world.resource_mut::<Focus>().unwrap().entity = None;
        }
        if snapshot(world)
            .layout
            .tabs
            .iter()
            .any(|t| t.id() == id(target))
        {
            command(world, ShellCommand::FocusTab { tab_id: id(target) });
        }
        // Runtime/attachment controls must resolve against this route before
        // an action is admitted, not the last pane painted in a frame.
        let ui = *world.resource::<Ui>().unwrap();
        let session = world
            .resource::<Model>()
            .unwrap()
            .chat
            .session()
            .map(str::to_owned);
        runtimes::show(world, session.as_deref());
        attachments::sync(world, ui.attachments, session.as_deref());
    }
}
fn within(world: &World, mut entity: Entity, root: Entity) -> bool {
    loop {
        if entity == root {
            return true;
        }
        let Some(p) = world.get::<Parent>(entity) else {
            return false;
        };
        entity = p.entity;
    }
}
pub(super) fn with_session(world: &mut World, session: &str, apply: impl FnOnce(&mut World)) {
    let old = current(world);
    let target = if world.resource::<Model>().unwrap().chat.session() == Some(session) {
        Some(old)
    } else {
        world.resource::<Deck>().and_then(|d| {
            d.parked
                .iter()
                .find(|(_, p)| p.model.chat.session() == Some(session))
                .map(|(r, _)| *r)
        })
    };
    if let Some(r) = target {
        swap(world, r);
        apply(world);
        swap(world, old);
    }
}
pub(super) fn focus_entity(world: &mut World, mut entity: Entity) {
    let active = *world.resource::<Ui>().unwrap();
    let mut roots = vec![(current(world), active.content)];
    roots.extend(
        world
            .resource::<Deck>()
            .unwrap()
            .parked
            .iter()
            .map(|(r, p)| (*r, p.ui.content)),
    );
    loop {
        if let Some(route) = world.get::<BandRoute>(entity).map(|r| r.0) {
            focus(world, route);
            return;
        }
        if let Some((r, _)) = roots.iter().find(|(_, e)| *e == entity) {
            focus(world, *r);
            return;
        }
        let Some(p) = world.get::<Parent>(entity) else {
            return;
        };
        entity = p.entity;
    }
}
/// Resolve only visible composer cards, never an offscreen retained tab.
/// Path-only native drops intentionally target the currently focused chat.
pub(super) fn drop_target(world: &World, point: Option<[f32; 2]>) -> Option<Entity> {
    if crate::surfaces::effective(world) != crate::surfaces::SurfaceId::Chat {
        return None;
    }
    let current = world.resource::<Ui>()?;
    let mut views = vec![current];
    if point.is_some() {
        views.extend(world.resource::<Deck>()?.parked.values().map(|p| &p.ui));
    }
    views.into_iter().find_map(|ui| {
        if world.has::<Hidden>(ui.content) || world.has::<Hidden>(ui.exchange.main) {
            return None;
        }
        let hit = match point {
            None => true,
            Some([x, y]) => world.get::<Rect>(ui.compose).is_some_and(|r| {
                x.is_finite()
                    && y.is_finite()
                    && x >= r.x
                    && x <= r.x + r.w
                    && y >= r.y
                    && y <= r.y + r.h
            }),
        };
        hit.then_some(ui.compose)
    })
}

pub(super) fn focus_from_input(world: &mut World) {
    if let Some(e) = world.resource::<Focus>().and_then(|f| f.entity) {
        focus_entity(world, e);
    }
}
pub(super) fn navigate(world: &mut World, action: u64) {
    if composing(world) {
        world.resource_mut::<Model>().unwrap().chat.status =
            "Finish composing before switching conversations.".into();
        return;
    }
    if action == RELOAD {
        let session = world
            .resource::<Model>()
            .unwrap()
            .chat
            .session()
            .map(str::to_owned);
        if let Some(s) = session {
            world.resource_mut::<Model>().unwrap().loading = true;
            dispatch(world, Command::Open(s));
        } else {
            dispatch(world, Command::List(String::new()));
        }
    } else {
        let session = if action == NEW {
            None
        } else {
            world
                .resource::<Model>()
                .unwrap()
                .sessions
                .get((action - SESSION) as usize)
                .map(|x| x.0.clone())
        };
        if action != NEW && session.is_none() {
            return;
        }
        open(world, session);
    }
}
pub(super) fn open(world: &mut World, session: Option<String>) {
    if composing(world) {
        return;
    }
    if let Some(s) = &session {
        if world.resource::<Model>().unwrap().chat.session() == Some(s)
            || world
                .resource::<Model>()
                .unwrap()
                .requested_session
                .as_ref()
                == Some(s)
        {
            let r = current(world);
            world.resource_mut::<Deck>().unwrap().closed.remove(&r);
            register(world);
            return;
        }
        let known = world
            .resource::<Deck>()
            .unwrap()
            .parked
            .iter()
            .find(|(_, p)| {
                p.model.chat.session() == Some(s) || p.model.requested_session.as_ref() == Some(s)
            })
            .map(|(r, _)| *r);
        if let Some(r) = known {
            world.resource_mut::<Deck>().unwrap().closed.remove(&r);
            focus(world, r);
            register(world);
            return;
        }
    }
    if world.resource::<Deck>().unwrap().parked.len() >= 24 {
        world.resource_mut::<Model>().unwrap().chat.status = "This window retains up to 24 conversations, including closed drafts. Reopen an existing conversation or open a fresh window.".into();
        return;
    }
    let ui = *world.resource::<Ui>().unwrap();
    let shared = shared(ui);
    // A released conversation's draft comes back as its composer text.
    let draft = session
        .as_ref()
        .and_then(|s| world.resource_mut::<Deck>().unwrap().stashed.remove(s))
        .unwrap_or_default();
    let document = world.resource_mut::<SourceDocuments>().unwrap().open(draft);
    let mut next = Model::new(document);
    if let Some(s) = &session {
        next.drafts.insert(s.clone(), document);
    }
    let m = world.resource::<Model>().unwrap();
    if world.resource::<crate::workshops::Workshops>().is_some() {
        let Some(owner) = crate::workshops::owner(world, session.as_deref()) else {
            return;
        };
        next.owner = owner.clone();
        next.commands = crate::workshops::commands(world, &owner);
        (next.models, next.modes) = crate::workshops::catalog(world, &owner);
        next.selected = (!next.models.is_empty()).then_some(0);
        next.defaults_pending = crate::workshops::chat_defaults(world, &owner).is_none();
        if let Some(defaults) = crate::workshops::chat_defaults(world, &owner) {
            apply_defaults(&mut next, defaults);
        }
    } else {
        next.owner = m.owner.clone();
        next.commands = m.commands.clone();
        next.models = m.models.clone();
        next.modes = m.modes.clone();
        next.selected = m.selected;
        if let Some(defaults) = m.defaults.clone() {
            apply_defaults(&mut next, defaults);
        }
    }
    next.sessions = m.sessions.clone();
    next.session_cursor = m.session_cursor.clone();
    next.search_query = m.search_query.clone();
    next.sidebar_open = m.sidebar_open;
    next.details_open = m.details_open;
    let r = world.resource::<Deck>().unwrap().next;
    world.resource_mut::<Deck>().unwrap().next += 1;
    next.generation = r;
    next.requested_session = session.clone();
    next.loading = true;
    next.chat.status = "Loading conversation…".into();
    let model = std::mem::replace(world.resource_mut::<Model>().unwrap(), next);
    let old = model.generation;
    let parent = world.resource::<Deck>().unwrap().bodies[0];
    let new_ui = mount_view(world, parent, shared);
    world.insert_resource(new_ui);
    world
        .resource_mut::<Deck>()
        .unwrap()
        .parked
        .insert(old, Parked { model, ui });
    world.insert(ui.content, Hidden);
    dispatch(world, session.map(Command::Open).unwrap_or(Command::New));
    world.resource_mut::<Focus>().unwrap().entity = None;
}
pub(super) fn defaults(world: &mut World, owner: &str, defaults: medousa_types::TuiDefaults) {
    let m = world.resource_mut::<Model>().unwrap();
    if m.owner == owner {
        apply_defaults(m, defaults.clone());
    }
    for p in world.resource_mut::<Deck>().unwrap().parked.values_mut() {
        if p.model.owner == owner {
            apply_defaults(&mut p.model, defaults.clone());
        }
    }
}
pub(super) fn catalog(
    world: &mut World,
    owner: &str,
    models: Vec<(String, String)>,
    modes: Vec<(String, String, bool)>,
    rows: Vec<(String, String)>,
    cursor: Option<String>,
) {
    fn update(
        m: &mut Model,
        owner: &str,
        models: &[(String, String)],
        modes: &[(String, String, bool)],
        rows: &[(String, String)],
        cursor: &Option<String>,
    ) {
        m.sessions = rows.into();
        m.session_cursor = cursor.clone();
        m.sidebar_key.clear();
        if m.owner == owner {
            let selected = m.selected.and_then(|i| m.models.get(i)).cloned();
            m.selected = selected
                .and_then(|s| models.iter().position(|v| *v == s))
                .or_else(|| (!models.is_empty()).then_some(0));
            m.models = models.into();
            m.modes = modes.into();
            restore_history_model(m);
        }
    }
    update(
        world.resource_mut::<Model>().unwrap(),
        owner,
        &models,
        &modes,
        &rows,
        &cursor,
    );
    for parked in world.resource_mut::<Deck>().unwrap().parked.values_mut() {
        update(&mut parked.model, owner, &models, &modes, &rows, &cursor);
    }
}
fn register(world: &mut World) {
    if world
        .resource::<Deck>()
        .unwrap()
        .closed
        .contains(&current(world))
    {
        return;
    }
    let m = world.resource::<Model>().unwrap();
    let Some(session) = m.chat.session().map(str::to_owned) else {
        return;
    };
    let title = m
        .sessions
        .iter()
        .find(|x| x.0 == session)
        .map(|x| x.1.clone())
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| "Untitled chat".into());
    let r = current(world);
    let snap = snapshot(world);
    if !snap.layout.tabs.iter().any(|t| t.id() == id(r)) {
        command(
            world,
            ShellCommand::OpenTab {
                tab: ShellTab::Chat {
                    id: id(r),
                    session_id: session,
                    title,
                },
                group_id: Some(snap.layout.active_group_id),
                activate: true,
            },
        );
    } else {
        command(
            world,
            ShellCommand::PatchTabTitle {
                tab_id: id(r),
                title,
            },
        );
    }
}
pub(super) fn reply(world: &mut World, reply: Reply) {
    if let Update::Forked { session, .. } = &reply.update {
        open(world, Some(session.clone()));
        return;
    }
    if let Update::Deleted(session) = &reply.update {
        let session = session.clone();
        with_session(world, &session, |world| {
            let r = current(world);
            command(world, ShellCommand::CloseTab { tab_id: id(r) });
            world.resource_mut::<Deck>().unwrap().closed.insert(r);
            let m = world.resource_mut::<Model>().unwrap();
            m.chat = Chat::connecting();
            m.chat.status = "Conversation deleted".into();
        });
        world
            .resource_mut::<Model>()
            .unwrap()
            .sessions
            .retain(|r| r.0 != session);
        let snap = snapshot(world);
        if let Some(r) = snap
            .layout
            .groups
            .iter()
            .find(|g| g.id == snap.layout.active_group_id)
            .and_then(|g| g.active_tab_id.as_deref())
            .and_then(route)
        {
            focus(world, r);
        }
        return;
    }
    let old = current(world);
    let global = matches!(
        &reply.update,
        Update::Defaults(_)
            | Update::Models(_)
            | Update::Modes(_)
            | Update::Unavailable(_)
            | Update::Sessions(_)
            | Update::SessionsPage { .. }
            | Update::Renamed { .. }
    );
    if !global && !swap(world, reply.generation) {
        return;
    }
    // Global catalogs have no conversation route. Current handler retains its
    // search-query fence; duplicate stale history can never navigate another tab.
    let generation = if global {
        current(world)
    } else {
        reply.generation
    };
    let history = matches!(&reply.update, Update::History(..));
    apply_current_reply(
        world,
        Reply {
            generation,
            update: reply.update,
        },
    );
    if history {
        register(world);
    }
    swap(world, old);
    if old != generation
        && snapshot(world)
            .layout
            .tabs
            .iter()
            .any(|t| t.id() == id(old))
    {
        command(world, ShellCommand::FocusTab { tab_id: id(old) });
    }
}
pub(super) fn action(world: &mut World, action: u64) -> bool {
    match action {
        SPLIT => {
            let snap = snapshot(world);
            if snap.layout.groups.len() == 1 && snap.layout.tabs.len() > 1 {
                command(
                    world,
                    ShellCommand::SplitPane {
                        group_id: snap.layout.active_group_id,
                        edge: SplitEdge::Right,
                        new_group_id: "side".into(),
                        move_tab_id: Some(id(current(world))),
                    },
                );
            } else {
                world.resource_mut::<Model>().unwrap().chat.status =
                    "Open two conversations, then split the selected tab.".into();
            }
        }
        JOIN => {
            if snapshot(world).layout.groups.len() > 1 {
                command(
                    world,
                    ShellCommand::ClosePane {
                        group_id: "side".into(),
                    },
                );
            }
        }
        _ => return false,
    }
    true
}
// RNDR exposes tab headers but not its decorative underline as a handle.
// Suppress only the full-width bottom rule. Selection fill, dirty marks,
// accessibility and keyboard focus remain RNDR-owned.
fn quiet_tab_underline(world: &mut World, list: Entity) {
    for tab in children_of(world, list) {
        if !world.has::<Tab>(tab) {
            continue;
        }
        for child in children_of(world, tab) {
            if world
                .get::<Absolute>(child)
                .is_some_and(|a| a.left == Some(0.) && a.right == Some(0.) && a.bottom == Some(0.))
                && let Some(layout) = world.get_mut::<Layout>(child)
            {
                layout.height = Size::Px(0.);
            }
        }
    }
}
/// Validate drag identities before removing anything. Self/stale/cross-group
/// targets must not drop the source from the canonical ShellCore layout.
fn reorder(ids: &mut Vec<String>, source: &str, target: &str, after: bool) -> bool {
    if source == target || !ids.iter().any(|id| id == source) || !ids.iter().any(|id| id == target)
    {
        return false;
    }
    ids.retain(|id| id != source);
    let at = ids.iter().position(|id| id == target).unwrap();
    ids.insert(at + usize::from(after), source.to_owned());
    true
}
pub(super) fn tab_events(world: &mut World) {
    chat_sidebar::input(world);
    let session = world
        .resource::<Model>()
        .and_then(|m| m.chat.session().map(str::to_owned));
    runtimes::menu_events(world, session.as_deref());
    for event in events::<TabStripEvent>(world).to_vec() {
        if !world
            .resource::<Deck>()
            .unwrap()
            .strips
            .contains(&Some(event.strip))
        {
            continue;
        }
        // RNDR may optimistically change selection before the host rejects a
        // request (e.g. IME). Reassert host state on the next reconciliation.
        let deck = world.resource_mut::<Deck>().unwrap();
        for i in 0..2 {
            if deck.strips[i] == Some(event.strip) {
                deck.strip_cache[i] = None;
            }
        }
        match event.kind {
            TabStripEventKind::Activated(t) => focus(world, t.0),
            TabStripEventKind::CloseRequested(t) => {
                if composing(world) {
                    continue;
                }
                // Closing a tab never cancels a turn or deletes its draft.
                command(world, ShellCommand::CloseTab { tab_id: id(t.0) });
                world.resource_mut::<Deck>().unwrap().closed.insert(t.0);
                let snap = snapshot(world);
                let next = snap
                    .layout
                    .groups
                    .iter()
                    .find(|g| g.id == snap.layout.active_group_id)
                    .and_then(|g| g.active_tab_id.as_deref())
                    .and_then(route);
                if let Some(r) = next {
                    focus(world, r);
                } else {
                    focus(world, 0);
                }
                world.resource_mut::<Focus>().unwrap().entity = None;
            }
            TabStripEventKind::Reordered(m) => {
                // ShellCore has no reorder command; hydrate only the exact
                // reordered native snapshot, keeping canonical group invariants.
                let mut snap = snapshot(world);
                if let Some(g) = snap
                    .layout
                    .groups
                    .iter_mut()
                    .find(|g| g.tab_ids.contains(&id(m.id.0)))
                {
                    let source = id(m.id.0);
                    let target = id(m.target.0);
                    let after = matches!(m.place, rndr_ui::drag::DropPlace::After);
                    if !reorder(&mut g.tab_ids, &source, &target, after) {
                        continue;
                    }
                }
                if let Some(d) = snap
                    .desktops
                    .iter_mut()
                    .find(|d| d.id == snap.active_desktop_id)
                {
                    d.layout = snap.layout;
                }
                command(
                    world,
                    ShellCommand::Hydrate {
                        workspace_scope_id: snap.workspace_scope_id,
                        desktops: snap.desktops,
                        active_desktop_id: snap.active_desktop_id,
                    },
                );
            }
            TabStripEventKind::SplitRequested(request) => {
                focus(world, request.id.0);
                action(world, SPLIT);
            }
            TabStripEventKind::Moved(m) => {
                let snap = snapshot(world);
                let deck = world.resource::<Deck>().unwrap();
                let index = deck
                    .parents
                    .iter()
                    .position(|p| children_of(world, *p).contains(&m.to));
                if let Some(i) = index {
                    if let Some(g) = snap.layout.groups.get(i) {
                        command(
                            world,
                            ShellCommand::MoveTab {
                                tab_id: id(m.id.0),
                                to_group_id: g.id.clone(),
                            },
                        );
                    }
                }
            }
        }
    }
}
/// A closed conversation that is idle gives back everything but its unsent
/// text: history, view tree (with its tooltips and menus) and documents.
/// Reopening reloads history; a draft returns as composer text. One with a
/// turn running, or a send not yet confirmed, stays parked until it settles.
fn release_closed(world: &mut World) {
    let deck = world.resource::<Deck>().unwrap();
    let idle: Vec<u64> = deck
        .closed
        .iter()
        .copied()
        .filter(|r| {
            deck.parked
                .get(r)
                .is_some_and(|p| !p.model.chat.busy() && p.model.pending_draft.is_none())
        })
        .collect();
    for r in idle {
        release(world, r);
    }
}
fn release(world: &mut World, route: u64) {
    let deck = world.resource_mut::<Deck>().unwrap();
    let Some(Parked { model, ui }) = deck.parked.remove(&route) else {
        return;
    };
    deck.closed.remove(&route);
    let mut documents: std::collections::HashSet<DocumentId> = [model.document].into();
    documents.extend(model.drafts.values().copied());
    documents.extend(model.external_drafts.values().map(|d| d.0));
    documents.extend(model.collaboration.documents());
    // Editors mounted in the view may hold documents no model field names.
    let mut stack = vec![ui.content];
    while let Some(e) = stack.pop() {
        documents.extend(rndr_core::editing::attached_source(world, e));
        stack.extend(children_of(world, e));
    }
    let current_session = model
        .chat
        .session()
        .map(str::to_owned)
        .or(model.requested_session.clone());
    let docs = world.resource::<SourceDocuments>().unwrap();
    let text = |d: DocumentId| {
        docs.get(d)
            .map(|d| d.text().to_string())
            .filter(|t| !t.trim().is_empty())
    };
    let mut stash: Vec<(String, String)> = model
        .drafts
        .iter()
        .filter_map(|(s, d)| Some((s.clone(), text(*d)?)))
        .collect();
    if let (Some(session), Some(draft)) = (current_session, text(model.document)) {
        stash.push((session, draft));
    }
    world.resource_mut::<Deck>().unwrap().stashed.extend(stash);
    despawn(world, ui.content);
    let docs = world.resource_mut::<SourceDocuments>().unwrap();
    for d in documents {
        docs.close(d);
    }
    drop(model);
}
pub(super) fn sync_all(world: &mut World) {
    release_closed(world);
    if current(world) == 0
        && world.resource::<Model>().unwrap().chat.session().is_some()
        && snapshot(world).layout.tabs.is_empty()
        && !world.resource::<Deck>().unwrap().closed.contains(&0)
    {
        register(world);
    }
    let old = current(world);
    let workspace = crate::surfaces::active(world) == crate::surfaces::SurfaceId::Workspace;
    let routes: Vec<_> = if workspace {
        world.resource::<Deck>().unwrap().workspace_visible.clone()
    } else {
        snapshot(world)
            .layout
            .groups
            .iter()
            .filter_map(|g| g.active_tab_id.as_deref().and_then(route))
            .collect()
    };
    let mut painted = std::collections::HashSet::new();
    // Paint each retained view against its own context; focused shared chrome
    // is deliberately painted last. No editor or timeline is remounted.
    for r in routes {
        if r != old && painted.insert(r) && swap(world, r) {
            sync(world);
        }
    }
    swap(world, old);
    sync(world);
    // The coordinator owns mixed-layout parents and visibility, including chats
    // which are not selected in the focused Chat surface's ShellCore topology.
    if workspace {
        return;
    }
    let snap = snapshot(world);
    let empty = world.resource::<Deck>().unwrap().empty;
    let no_tabs = snap.layout.tabs.is_empty()
        && !world.resource::<Model>().unwrap().loading
        && world
            .resource::<Deck>()
            .unwrap()
            .closed
            .contains(&current(world));
    if no_tabs {
        world.remove::<Hidden>(empty);
    } else {
        world.insert(empty, Hidden);
    }
    let deck = world.resource::<Deck>().unwrap();
    let parents = deck.parents;
    let bodies = deck.bodies;
    let split = deck.split;
    let width = world.get::<Rect>(split.root).map(|r| r.w).unwrap_or(1200.);
    let two = snap.layout.groups.len() > 1 && width >= 720.;
    let active_group = snap
        .layout
        .groups
        .iter()
        .position(|g| g.id == snap.layout.active_group_id)
        .unwrap_or(0);
    let geometry = (
        width.round() as u32,
        two,
        if two { 0 } else { active_group },
    );
    let deck = world.resource_mut::<Deck>().unwrap();
    if geometry != deck.geometry {
        deck.geometry = geometry;
        deck.settling = 4;
    }
    let reconfigure = deck.settling > 0;
    deck.settling = deck.settling.saturating_sub(1);
    let mut ratio = deck.ratio;
    if !reconfigure && two {
        if let Some(change) = events::<rndr_ui::split::SplitSashChange>(world)
            .iter()
            .find(|c| c.sash == split.sash)
        {
            ratio = change.ratio;
            world.resource_mut::<Deck>().unwrap().ratio = ratio;
        }
    }
    if reconfigure {
        let mut props = SplitSashProps::new("Conversation panes");
        props.min_leading = if two { 340. } else { 0. };
        props.min_trailing = props.min_leading;
        props.enabled = two;
        SplitSash::mount(
            world,
            world.get::<Parent>(split.root).unwrap().entity,
            UiId(0x4348_0001),
            props,
            if two {
                ratio
            } else if active_group == 0 {
                1.
            } else {
                0.
            },
        )
        .unwrap();
        frame_wake(world).wake();
    }

    chrome::begin_tabs(world);
    for i in 0..2 {
        let visible = if two { true } else { i == active_group };
        if visible {
            world.remove::<Hidden>(parents[i]);
        } else {
            world.insert(parents[i], Hidden);
        }
        if !two && visible {
            world.get_mut::<Layout>(parents[i]).unwrap().width = Size::Fill;
        }
        // The retained view host is never a tab-owned panel: closing/moving a
        // tab must not despawn its draft or detach an in-flight admission.
        set_parent(world, bodies[i], parents[i]);
        if let Some(strip) = world.resource::<Deck>().unwrap().strips[i] {
            world.insert(strip, Hidden);
        }
        if let Some(g) = snap.layout.groups.get(i) {
            let specs: Vec<_> = g
                .tab_ids
                .iter()
                .filter_map(|t| {
                    let tab = snap.layout.tabs.iter().find(|tab| tab.id() == t)?;
                    let r = route(t)?;
                    let m = if r == current(world) {
                        world.resource::<Model>().unwrap()
                    } else {
                        &world.resource::<Deck>().unwrap().parked.get(&r)?.model
                    };
                    let dirty = world
                        .resource::<SourceDocuments>()
                        .unwrap()
                        .get(m.document)
                        .is_some_and(|d| !d.text().is_empty());
                    Some(
                        TabSpec::new(r, tab.title())
                            .icon(Icon::MessageCircle)
                            .dirty(dirty),
                    )
                })
                .collect();
            if let Some(selected) = g.active_tab_id.as_deref().and_then(route) {
                let selected = TabId(selected);
                let active = i == active_group;
                let cached = world.resource::<Deck>().unwrap().strip_cache[i]
                    .as_ref()
                    .filter(|(previous, tab, engaged, _)| {
                        *previous == specs && *tab == selected && *engaged == active
                    })
                    .map(|(_, _, _, handle)| *handle);
                let strip = if let Some(handle) = cached {
                    handle
                } else {
                    let handle = TabStrip::mount(
                        world,
                        parents[i],
                        UiId(0x5441_4200 + i as u64),
                        TabStripProps::new("Conversation tabs").active(active),
                        &specs,
                        selected,
                    )
                    .unwrap();
                    quiet_tab_underline(world, handle.list);
                    world.resource_mut::<Deck>().unwrap().strip_cache[i] =
                        Some((specs.clone(), selected, active, handle));
                    handle
                };
                if visible {
                    let bar = world.get::<Parent>(strip.scroll).unwrap().entity;
                    world.insert(bar, BandRoute(selected.0));
                    chrome::place_tabs(world, strip);
                }
                world.get_mut::<Layout>(strip.root).unwrap().height = Size::Fill;
                world.remove::<Hidden>(strip.root);
                world.resource_mut::<Deck>().unwrap().strips[i] = Some(strip.root);
                world.remove::<Hidden>(strip.body);
                for spec in &specs {
                    if let Some(panel) = strip.panel(world, spec.id) {
                        world.insert(panel, Hidden);
                    }
                }
                set_parent(world, bodies[i], strip.body);
                // The native pane body remains a real drop target for tabs.
                // Retained body follows the tab bar, including after first mount.
                if let Some(children) = world.get_mut::<Children>(parents[i]) {
                    children.list.retain(|e| *e != strip.root);
                    children.list.insert(0, strip.root);
                }
            }
        }
    }
    if two {
        world.remove::<Hidden>(split.sash);
    } else {
        world.insert(split.sash, Hidden);
    }
    let mut views: Vec<_> = world
        .resource::<Deck>()
        .unwrap()
        .parked
        .iter()
        .map(|(r, p)| (*r, p.ui.content))
        .collect();
    views.push((current(world), world.resource::<Ui>().unwrap().content));
    for (r, root) in views {
        let group = snap
            .layout
            .groups
            .iter()
            .position(|g| g.active_tab_id.as_deref() == Some(&id(r)));
        let visible = group.is_some_and(|i| two || i == active_group)
            || (r == current(world)
                && world.resource::<Model>().unwrap().chat.session().is_none()
                && !no_tabs);
        if let Some(i) = group {
            set_parent(world, root, bodies[i]);
        }
        if visible {
            world.remove::<Hidden>(root);
        } else {
            world.insert(root, Hidden);
        }
    }
    // Compact explicit commands supplement RNDR's drag-to-split gestures.
    chrome::pane_commands(
        world,
        snap.layout.tabs.len() > 1,
        snap.layout.groups.len() > 1,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> App {
        let mut app = super::super::app();
        app.world_mut().insert_resource(Viewport {
            width: 1400.,
            height: 850.,
            ..Default::default()
        });
        app.update();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        app.world_mut().resource_mut::<Model>().unwrap().commands = Some(tx);
        app
    }
    fn loaded(app: &mut App, session: &str) -> u64 {
        open(app.world_mut(), Some(session.into()));
        let generation = current(app.world());
        reply(
            app.world_mut(),
            Reply {
                generation,
                update: Update::History(session.into(), "a".into(), vec![], None),
            },
        );
        for _ in 0..6 {
            app.update();
        }
        generation
    }
    fn text(world: &mut World, text: &str) {
        let id = world.resource::<Model>().unwrap().document;
        let d = world
            .resource_mut::<SourceDocuments>()
            .unwrap()
            .get_mut(id)
            .unwrap();
        d.reload(d.revision(), text).unwrap();
    }
    fn draft(world: &World) -> String {
        let id = world.resource::<Model>().unwrap().document;
        world
            .resource::<SourceDocuments>()
            .unwrap()
            .get(id)
            .unwrap()
            .text()
            .to_string()
    }
    #[test]
    fn workspace_catalog_and_focus_keep_canonical_drafts_and_entities() {
        let mut app = fixture();
        let a = loaded(&mut app, "a");
        text(app.world_mut(), "draft a");
        let root_a = app.world().resource::<Ui>().unwrap().content;
        let b = loaded(&mut app, "b");
        text(app.world_mut(), "draft b");
        let root_b = app.world().resource::<Ui>().unwrap().content;
        let items = workspace_items(app.world());
        assert_eq!(items.iter().find(|i| i.route == a).unwrap().entity, root_a);
        assert_eq!(
            items
                .iter()
                .find(|i| i.route == b)
                .unwrap()
                .session
                .as_deref(),
            Some("b")
        );
        assert!(workspace_focus(app.world_mut(), a));
        assert_eq!(draft(app.world()), "draft a");
        assert_eq!(app.world().resource::<Ui>().unwrap().content, root_a);
        assert!(!workspace_focus(app.world_mut(), u64::MAX));
        assert_eq!(current(app.world()), a);
        assert!(workspace_focus(app.world_mut(), b));
        assert_eq!(draft(app.world()), "draft b");
        assert_eq!(app.world().resource::<Ui>().unwrap().content, root_b);
        workspace_new(app.world_mut());
        let new = current(app.world());
        let items = workspace_items(app.world());
        let draft_item = items.iter().find(|i| i.route == new).unwrap();
        assert_eq!(draft_item.title, "New chat");
        assert!(draft_item.session.is_none());
    }

    #[test]
    fn workspace_sync_does_not_reclaim_borrowed_roots_or_visibility() {
        use crate::surfaces::{self, SurfaceId};
        let mut app = fixture();
        let a = loaded(&mut app, "a");
        let view_a = *app.world().resource::<Ui>().unwrap();
        let b = loaded(&mut app, "b");
        let view_b = *app.world().resource::<Ui>().unwrap();
        let original_a = app.world().get::<Parent>(view_a.content).unwrap().entity;
        let original_b = app.world().get::<Parent>(view_b.content).unwrap().entity;
        let topology = snapshot(app.world()).layout;
        surfaces::select(app.world_mut(), SurfaceId::Workspace);
        let host = container(app.world_mut(), None, Layout::row());
        for root in [view_a.content, view_b.content] {
            set_parent(app.world_mut(), root, host);
            app.world_mut().remove::<Hidden>(root);
        }
        workspace_show(app.world_mut(), &[a, b, a, u64::MAX]);
        reply(
            app.world_mut(),
            Reply {
                generation: a,
                update: Update::Notice("Background workspace update".into()),
            },
        );
        sync_all(app.world_mut());
        assert_eq!(current(app.world()), b);
        assert_eq!(snapshot(app.world()).layout, topology);
        for root in [view_a.content, view_b.content] {
            assert_eq!(app.world().get::<Parent>(root).unwrap().entity, host);
            assert!(!app.world().has::<Hidden>(root));
        }
        assert_eq!(
            app.world().get::<Label>(view_a.status).unwrap().text,
            "Background workspace update"
        );
        // Hidden mixed tabs stay hidden too, even when they are the canonical
        // current chat. The mixed coordinator remains the only visibility owner.
        app.world_mut().insert(view_b.content, Hidden);
        workspace_show(app.world_mut(), &[a]);
        sync_all(app.world_mut());
        assert!(app.world().has::<Hidden>(view_b.content));
        set_parent(app.world_mut(), view_a.content, original_a);
        set_parent(app.world_mut(), view_b.content, original_b);
        workspace_show(app.world_mut(), &[]);
        surfaces::select(app.world_mut(), SurfaceId::Chat);
        sync_all(app.world_mut());
        assert_eq!(
            app.world().get::<Parent>(view_b.content).unwrap().entity,
            original_b
        );
        assert!(!app.world().has::<Hidden>(view_b.content));
        assert!(app.world().has::<Hidden>(view_a.content));
    }

    #[test]
    fn background_initial_runtime_creation_does_not_open_menu_over_another_tab() {
        let mut app = fixture();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        app.world_mut().resource_mut::<Model>().unwrap().commands = Some(tx);
        super::super::action(app.world_mut(), RUNTIME);
        assert!(matches!(rx.try_recv().unwrap().command, Command::New));
        let initial = current(app.world());
        let other = loaded(&mut app, "other");
        reply(
            app.world_mut(),
            Reply {
                generation: initial,
                update: Update::History("created".into(), "a".into(), vec![], None),
            },
        );
        for _ in 0..3 {
            app.update();
        }
        assert_eq!(current(app.world()), other);
        assert!(
            !app.world()
                .query::<Label>()
                .iter()
                .any(|(_, l)| l.text == "Refresh runtimes")
        );
        focus(app.world_mut(), initial);
        for _ in 0..3 {
            app.update();
        }
        assert!(
            app.world()
                .query::<Label>()
                .iter()
                .any(|(_, l)| l.text == "Refresh runtimes")
        );
    }
    #[test]
    fn conversation_tabs_keep_selection_without_accent_underline() {
        let mut app = fixture();
        loaded(&mut app, "one");
        loaded(&mut app, "two");
        for _ in 0..4 {
            app.update();
        }
        let mut checked = 0;
        for (_, _, _, strip) in app
            .world()
            .resource::<Deck>()
            .unwrap()
            .strip_cache
            .iter()
            .flatten()
        {
            for tab in children_of(app.world(), strip.list) {
                if !app.world().has::<Tab>(tab) {
                    continue;
                }
                for child in children_of(app.world(), tab) {
                    if app.world().get::<Absolute>(child).is_some_and(|a| {
                        a.left == Some(0.) && a.right == Some(0.) && a.bottom == Some(0.)
                    }) {
                        assert_eq!(
                            app.world().get::<Layout>(child).unwrap().height,
                            Size::Px(0.)
                        );
                        if let Some(rect) = app.world().get::<Rect>(child) {
                            assert_eq!(rect.h, 0.);
                        }
                        checked += 1;
                    }
                }
            }
            assert!(strip.selected(app.world()).is_some());
        }
        assert!(checked >= 2);
    }
    #[test]
    #[ignore = "Synthetic native chat tab GPU visual acceptance"]
    fn quiet_chat_tabs_visual_acceptance() {
        let mut app = fixture();
        app.world_mut()
            .insert_resource(rndr_ui::InteractionMotion::snap());
        let engine: std::sync::Arc<dyn TextEngine> =
            std::sync::Arc::new(rndr_render::ParleyText::new());
        app.world_mut().insert_resource(engine);
        loaded(&mut app, "one");
        loaded(&mut app, "two");
        let mut exporter = rndr_render::Exporter::new().unwrap();
        for _ in 0..10 {
            rndr_paint::frame(&mut app);
        }
        let (_, image) = rndr_paint::frame_with_snapshot_frame(&mut app, |world, frame| {
            exporter
                .render_frame(Some(world), frame, 1400, 850, 1.)
                .unwrap()
        });
        std::fs::create_dir_all("verification").unwrap();
        image::save_buffer(
            "verification/native-chat-quiet-tabs.png",
            &image.rgba,
            image.width,
            image.height,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
    #[test]
    fn new_tabs_use_saved_defaults_not_previous_conversation_selection() {
        let mut app = fixture();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let m = app.world_mut().resource_mut::<Model>().unwrap();
        m.commands = Some(tx);
        apply_defaults(
            m,
            medousa_types::TuiDefaults {
                provider: Some("preferred".into()),
                model: Some("saved".into()),
                response_depth_mode: Some("deep".into()),
                ..Default::default()
            },
        );
        m.models.push(("other".into(), "override".into()));
        m.selected = Some(1);
        m.model_chosen = true;
        m.depth = 0;
        open(app.world_mut(), None);
        let m = app.world().resource::<Model>().unwrap();
        assert_eq!(
            m.models[m.selected.unwrap()],
            ("preferred".into(), "saved".into())
        );
        assert_eq!(m.depth, 2);
        assert!(matches!(rx.try_recv().unwrap().command, Command::New));
    }
    #[test]
    fn defaults_only_update_their_own_workshop_including_parked_initial_draft() {
        let mut app = fixture();
        app.world_mut().resource_mut::<Model>().unwrap().owner = "a".into();
        loaded(&mut app, "a-session");
        let saved = medousa_types::TuiDefaults {
            provider: Some("provider-a".into()),
            model: Some("model-a".into()),
            ..Default::default()
        };
        defaults(app.world_mut(), "a", saved);
        let m = app.world().resource::<Model>().unwrap();
        assert_eq!(
            m.models[m.selected.unwrap()],
            ("provider-a".into(), "model-a".into())
        );
        assert!(
            app.world()
                .resource::<Deck>()
                .unwrap()
                .parked
                .get(&0)
                .unwrap()
                .model
                .defaults
                .is_some()
        );
        defaults(
            app.world_mut(),
            "b",
            medousa_types::TuiDefaults {
                provider: Some("provider-b".into()),
                model: Some("model-b".into()),
                ..Default::default()
            },
        );
        assert_eq!(app.world().resource::<Model>().unwrap().models.len(), 1);
    }
    #[test]
    fn integrated_tab_band_survives_splits_route_focus_and_overflow() {
        let mut app = fixture();
        let a = loaded(&mut app, "a");
        let b = loaded(&mut app, "b");
        let strip = app.world().resource::<Deck>().unwrap().strip_cache[0]
            .as_ref()
            .unwrap()
            .3;
        let bar = app.world().get::<Parent>(strip.scroll).unwrap().entity;
        assert_ne!(app.world().get::<Parent>(bar).unwrap().entity, strip.root);
        let t = children_of(app.world(), strip.list)[0];
        let r = app.world().get::<Rect>(t).unwrap();
        assert!(r.y >= 0. && r.y + r.h <= chrome::HEIGHT);
        for child in children_of(app.world(), t) {
            // The title keeps text insets; the icon and close glyphs sit flush.
            let title = app
                .world()
                .get::<Label>(child)
                .is_some_and(|l| !l.text.chars().all(rndr_core::is_icon_scalar));
            if title && !app.world().has::<Hidden>(child) {
                assert_eq!(
                    *app.world().get::<LabelBox>(child).unwrap(),
                    LabelBox::CONTENT
                );
            }
        }
        focus(app.world_mut(), a);
        action(app.world_mut(), SPLIT);
        for _ in 0..8 {
            app.update();
        }
        let deck = app.world().resource::<Deck>().unwrap();
        assert_eq!(snapshot(app.world()).layout.groups.len(), 2);
        let first = deck.strip_cache[0].as_ref().unwrap().3;
        let first_bar = app.world().get::<Parent>(first.scroll).unwrap().entity;
        let route = app.world().get::<BandRoute>(first_bar).unwrap().0;
        focus(app.world_mut(), a);
        focus_entity(app.world_mut(), first_bar);
        assert_eq!(current(app.world()), route);
        assert!(route == a || route == b);
        for width in [760., 1400.] {
            app.world_mut().insert_resource(Viewport::new(width, 850.));
            for _ in 0..8 {
                app.update();
            }
            let w = app.world();
            for (tab, _) in w.query::<Tab>() {
                let r = w.get::<Rect>(tab).unwrap();
                assert!(r.h <= chrome::HEIGHT);
            }
        }
        action(app.world_mut(), JOIN);
        for _ in 0..8 {
            app.update();
        }
        for i in 0..6 {
            loaded(&mut app, &format!("extra-{i}"));
        }
        let strip = app.world().resource::<Deck>().unwrap().strip_cache[0]
            .as_ref()
            .unwrap()
            .3;
        let overflow = strip.overflow_button(app.world()).unwrap();
        assert!(!app.world().has::<Hidden>(overflow));
        assert!(app.world().get::<Button>(overflow).unwrap().enabled);
        // Existing route/document identity wasn't replaced by the window host.
        open(app.world_mut(), Some("a".into()));
        assert_eq!(current(app.world()), a);
    }
    #[test]
    fn routes_retain_distinct_editor_timeline_and_draft_without_refetch() {
        let mut app = fixture();
        let a = loaded(&mut app, "a");
        text(app.world_mut(), "draft a");
        let view = *app.world().resource::<Ui>().unwrap();
        let b = loaded(&mut app, "b");
        text(app.world_mut(), "draft b");
        assert_ne!(a, b);
        assert_ne!(
            view.timeline,
            app.world().resource::<Ui>().unwrap().timeline
        );
        open(app.world_mut(), Some("a".into()));
        assert_eq!(current(app.world()), a);
        assert_eq!(draft(app.world()), "draft a");
        assert_eq!(
            app.world().resource::<Ui>().unwrap().timeline,
            view.timeline
        );
        open(app.world_mut(), Some("b".into()));
        assert_eq!(draft(app.world()), "draft b");
        assert_eq!(snapshot(app.world()).layout.tabs.len(), 2);
    }
    #[test]
    fn late_history_updates_its_route_without_changing_focus() {
        let mut app = fixture();
        let a = loaded(&mut app, "a");
        let b = loaded(&mut app, "b");
        reply(
            app.world_mut(),
            Reply {
                generation: a,
                update: Update::History("a".into(), "a".into(), vec![], None),
            },
        );
        assert_eq!(current(app.world()), b);
        assert_eq!(
            app.world().resource::<Model>().unwrap().chat.session(),
            Some("b")
        );
        assert_eq!(
            snapshot(app.world()).layout.groups[0]
                .active_tab_id
                .as_deref(),
            Some(id(b).as_str())
        );
    }
    #[test]
    fn reorder_preserves_every_tab_for_self_stale_and_foreign_targets() {
        let original = vec!["a".into(), "b".into(), "c".into()];
        for (source, target) in [("a", "a"), ("a", "missing"), ("foreign", "b")] {
            let mut tabs = original.clone();
            assert!(!reorder(&mut tabs, source, target, false));
            assert_eq!(tabs, original);
        }
        let mut tabs = original.clone();
        assert!(reorder(&mut tabs, "a", "c", true));
        assert_eq!(tabs, vec!["b", "c", "a"]);
        assert!(reorder(&mut tabs, "a", "b", false));
        assert_eq!(tabs, original);
    }
    #[test]
    fn foreign_source_tab_events_cannot_close_chat_tabs() {
        let mut app = fixture();
        let route = loaded(&mut app, "chat");
        let other = app.world_mut().spawn((Layout::row(),));
        send(
            app.world_mut(),
            TabStripEvent {
                strip: other,
                kind: TabStripEventKind::CloseRequested(TabId(route)),
            },
        );
        app.update();
        assert_eq!(snapshot(app.world()).layout.tabs.len(), 1);
        assert!(
            !app.world()
                .resource::<Deck>()
                .unwrap()
                .closed
                .contains(&route)
        );
    }
    #[test]
    fn close_is_not_delete_and_reopening_restores_draft() {
        let mut app = fixture();
        let a = loaded(&mut app, "a");
        text(app.world_mut(), "keep me");
        let strip = app.world().resource::<Deck>().unwrap().strips[0].unwrap();
        send(
            app.world_mut(),
            TabStripEvent {
                strip,
                kind: TabStripEventKind::CloseRequested(TabId(a)),
            },
        );
        let view = app
            .world()
            .resource::<Deck>()
            .unwrap()
            .parked
            .get(&a)
            .map(|p| p.ui.content);
        app.update();
        assert!(snapshot(app.world()).layout.tabs.is_empty());
        // Closed and idle: its history, view and documents are released and
        // only the unsent text is kept.
        assert!(
            !app.world()
                .resource::<Deck>()
                .unwrap()
                .parked
                .contains_key(&a)
        );
        assert!(view.is_none_or(|v| !app.world().is_alive(v)));
        assert_eq!(
            app.world()
                .resource::<Deck>()
                .unwrap()
                .stashed
                .get("a")
                .map(String::as_str),
            Some("keep me")
        );
        open(app.world_mut(), Some("a".into()));
        assert_eq!(draft(app.world()), "keep me");
        assert!(app.world().resource::<Deck>().unwrap().stashed.is_empty());
    }
    #[test]
    fn closing_idle_long_chats_releases_their_history_views_and_documents() {
        let mut app = fixture();
        let layouts = |w: &World| w.entities_with::<Layout>().len();
        let documents = |w: &World| w.resource::<SourceDocuments>().unwrap().ids().count();
        let (base_layouts, base_documents) = (layouts(app.world()), documents(app.world()));
        let body = "Long reply with **markdown** and `code`. ".repeat(40);
        let mut routes = vec![];
        for n in 0..3 {
            let session = format!("long-{n}");
            open(app.world_mut(), Some(session.clone()));
            let generation = current(app.world());
            let turns: Vec<_> = (0..120)
                .map(|i| {
                    serde_json::json!({"entry_id": format!("{session}-{i}"), "entry_seq": i + 1,
                        "role": if i % 2 == 0 {"user"} else {"assistant"}, "content": body})
                })
                .collect();
            reply(
                app.world_mut(),
                Reply {
                    generation,
                    update: Update::History(session, "a".into(), turns, None),
                },
            );
            for _ in 0..6 {
                app.update();
            }
            routes.push(generation);
        }
        assert!(layouts(app.world()) > base_layouts + 900);
        let strip = app.world().resource::<Deck>().unwrap().strips[0].unwrap();
        for r in routes {
            send(
                app.world_mut(),
                TabStripEvent {
                    strip,
                    kind: TabStripEventKind::CloseRequested(TabId(r)),
                },
            );
            for _ in 0..3 {
                app.update();
            }
        }
        let w = app.world();
        let deck = w.resource::<Deck>().unwrap();
        let held: usize = deck
            .parked
            .values()
            .filter_map(|p| p.model.chat.snapshot())
            .map(|s| s.items.len())
            .sum();
        assert_eq!(held, 0, "no closed conversation keeps its history");
        assert!(deck.parked.is_empty());
        // Back to the empty window: every view and document is gone.
        assert!(
            layouts(w) <= base_layouts + 100,
            "{} vs base {base_layouts}",
            layouts(w)
        );
        assert_eq!(documents(w), base_documents);
    }
    #[test]
    fn navigation_failure_notice_stays_with_emitting_conversation() {
        let mut app = fixture();
        let a = loaded(&mut app, "a");
        let b = loaded(&mut app, "b");
        let before = app.world().resource::<Model>().unwrap().chat.status.clone();
        super::super::link_notice(app.world_mut(), "a", "Use a work-qualified code reference");
        assert_eq!(current(app.world()), b);
        assert_eq!(app.world().resource::<Model>().unwrap().chat.status, before);
        focus(app.world_mut(), a);
        assert_eq!(
            app.world().resource::<Model>().unwrap().chat.status,
            "Use a work-qualified code reference"
        );
    }

    #[test]
    fn file_drop_positions_follow_visible_split_composer_not_focused_tab() {
        let mut app = fixture();
        let a = loaded(&mut app, "a");
        let view_a = *app.world().resource::<Ui>().unwrap();
        let b = loaded(&mut app, "b");
        let view_b = *app.world().resource::<Ui>().unwrap();
        action(app.world_mut(), SPLIT);
        for _ in 0..8 {
            app.update();
        }
        let rect = *app.world().get::<Rect>(view_a.compose).unwrap();
        let point = Some([rect.x + rect.w / 2., rect.y + rect.h / 2.]);
        assert_eq!(current(app.world()), b);
        assert_eq!(drop_target(app.world(), point), Some(view_a.compose));
        assert_eq!(drop_target(app.world(), None), Some(view_b.compose));
        rndr_core::send(
            app.world_mut(),
            FileDrop::from_report("/device/a.txt", point),
        );
        app.update();
        assert_eq!(current(app.world()), a);
        // Normal orchestration may already drain upload commands into the
        // owning auxiliary transport; assert the retained admission, not its queue.
        assert!(
            attachments::media_refs(app.world(), "a")
                .unwrap_err()
                .contains("Wait")
        );
        assert!(
            attachments::media_refs(app.world(), "b")
                .unwrap()
                .is_empty()
        );
        // Responsive collapse leaves the other retained pane with stale
        // geometry. It must not remain eligible for positioned drops.
        let rect_b = *app.world().get::<Rect>(view_b.compose).unwrap();
        let stale_b = Some([rect_b.x + rect_b.w / 2., rect_b.y + rect_b.h / 2.]);
        app.world_mut().resource_mut::<Viewport>().unwrap().width = 760.;
        for _ in 0..8 {
            app.update();
        }
        assert!(!app.world().has::<Hidden>(view_a.content));
        assert!(app.world().has::<Hidden>(view_b.content));
        assert_ne!(drop_target(app.world(), stale_b), Some(view_b.compose));
    }

    #[test]
    fn split_join_and_responsive_collapse_preserve_both_conversations() {
        let mut app = fixture();
        let a = loaded(&mut app, "a");
        let view_a = *app.world().resource::<Ui>().unwrap();
        let b = loaded(&mut app, "b");
        let view_b = *app.world().resource::<Ui>().unwrap();
        action(app.world_mut(), SPLIT);
        for _ in 0..8 {
            app.update();
        }
        assert_eq!(snapshot(app.world()).layout.groups.len(), 2);
        assert!(!app.world().has::<Hidden>(view_a.content));
        assert!(!app.world().has::<Hidden>(view_b.content));
        for view in [view_a, view_b] {
            let root = app.world().get::<Rect>(view.content).unwrap();
            let compose = app.world().get::<Rect>(view.compose).unwrap();
            assert!(root.w >= 340., "{root:?}");
            assert!(
                compose.x + compose.w <= root.x + root.w + 1.,
                "{root:?} {compose:?}"
            );
        }
        app.world_mut().resource_mut::<Viewport>().unwrap().width = 760.;
        for _ in 0..8 {
            app.update();
        }
        assert!(app.world().has::<Hidden>(view_a.content));
        assert!(!app.world().has::<Hidden>(view_b.content));
        focus(app.world_mut(), a);
        for _ in 0..8 {
            app.update();
        }
        assert!(!app.world().has::<Hidden>(view_a.content));
        assert!(app.world().has::<Hidden>(view_b.content));
        action(app.world_mut(), JOIN);
        for _ in 0..6 {
            app.update();
        }
        assert_eq!(snapshot(app.world()).layout.groups.len(), 1);
        assert_eq!(snapshot(app.world()).layout.tabs.len(), 2);
        open(app.world_mut(), Some("b".into()));
        assert_eq!(current(app.world()), b);
    }
    #[test]
    fn nonchat_switch_retains_split_routes_drafts_and_background_replies() {
        use crate::surfaces::{self, SurfaceId};
        for surface in [SurfaceId::Notes, SurfaceId::Code] {
            let mut app = fixture();
            let a = loaded(&mut app, "a");
            text(app.world_mut(), "draft a");
            let view_a = *app.world().resource::<Ui>().unwrap();
            let b = loaded(&mut app, "b");
            text(app.world_mut(), "draft b");
            let view_b = *app.world().resource::<Ui>().unwrap();
            action(app.world_mut(), SPLIT);
            for _ in 0..6 {
                app.update();
            }
            let layout = snapshot(app.world()).layout;
            surfaces::select(app.world_mut(), surface);
            reply(
                app.world_mut(),
                Reply {
                    generation: a,
                    update: Update::Notice("Background update".into()),
                },
            );
            for _ in 0..6 {
                app.update();
            }
            assert_eq!(surfaces::active(app.world()), surface);
            assert_eq!(current(app.world()), b);
            assert_eq!(draft(app.world()), "draft b");
            surfaces::select(app.world_mut(), SurfaceId::Chat);
            for _ in 0..6 {
                app.update();
            }
            assert_eq!(snapshot(app.world()).layout, layout);
            assert_eq!(
                app.world().resource::<Ui>().unwrap().composer,
                view_b.composer
            );
            open(app.world_mut(), Some("a".into()));
            assert_eq!(draft(app.world()), "draft a");
            assert_eq!(
                app.world().resource::<Ui>().unwrap().composer,
                view_a.composer
            );
            assert_eq!(
                app.world().resource::<Model>().unwrap().chat.status,
                "Background update"
            );
        }
    }
    #[test]
    fn focusing_other_pane_clears_previous_editor_focus() {
        let mut app = fixture();
        let a = loaded(&mut app, "a");
        let view = *app.world().resource::<Ui>().unwrap();
        loaded(&mut app, "b");
        action(app.world_mut(), SPLIT);
        let editor = app.world().resource::<Ui>().unwrap().composer.editor;
        app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(editor);
        focus_entity(app.world_mut(), view.composer.editor);
        assert_eq!(current(app.world()), a);
        assert!(app.world().resource::<Focus>().unwrap().entity.is_none());
    }
}
