//! Retained conversation workspaces. ShellCore owns tab/group topology; each
//! route owns one canonical Chat, editor document, retained view and transport.
//! The Model/Ui resources are a synchronous context, never a second state copy.
use super::*;
use medousa_app_core::shell::SplitEdge;
use medousa_app_core::{ShellCommand, ShellCore, ShellTab};
use rndr_ui::{
    split::{SplitSash, SplitSashHandle, SplitSashProps},
    tabs::{TabId, TabSpec, TabStrip, TabStripEvent, TabStripEventKind, TabStripProps},
};

const SPLIT: u64 = 40;
const JOIN: u64 = 41;
struct Parked {
    model: Model,
    ui: Ui,
}
struct Deck {
    shell: ShellCore,
    parked: HashMap<u64, Parked>,
    next: u64,
    split: SplitSashHandle,
    parents: [Entity; 2],
    bodies: [Entity; 2],
    geometry: (u32, bool, usize),
    settling: u8,
    ratio: f64,
    strips: [Option<Entity>; 2],
    closed: std::collections::HashSet<u64>,
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
        rename: ui.rename,
        session_actions: ui.session_actions,
        context: ui.context,
        runtime: ui.runtime,
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
        split,
        parents,
        bodies,
        geometry: (0, false, 0),
        settling: 3,
        ratio: 0.5,
        strips: [None, None],
        closed: Default::default(),
        empty,
    });
    bodies[0]
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
    // Catalogs and device-local chrome are shared. Selection, drafts, admission,
    // mode/depth and canonical Chat remain strictly conversation-owned.
    let selected = next
        .model
        .selected
        .and_then(|i| next.model.models.get(i))
        .cloned();
    next.model.commands = m.commands.clone();
    next.model.models = m.models.clone();
    next.model.modes = m.modes.clone();
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
        runtimes::sync(world, ui.runtime, session.as_deref());
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
    let document = world.resource_mut::<SourceDocuments>().unwrap().open("");
    let mut next = Model::new(document);
    let m = world.resource::<Model>().unwrap();
    next.commands = m.commands.clone();
    next.models = m.models.clone();
    next.modes = m.modes.clone();
    next.sessions = m.sessions.clone();
    next.selected = m.selected;
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
        Update::Models(_)
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
pub(super) fn tab_events(world: &mut World) {
    for event in events::<TabStripEvent>(world).to_vec() {
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
                    g.tab_ids.retain(|t| t != &source);
                    if let Some(at) = g.tab_ids.iter().position(|t| t == &target) {
                        let after = matches!(m.place, rndr_ui::drag::DropPlace::After);
                        g.tab_ids.insert(at + usize::from(after), source);
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
pub(super) fn sync_all(world: &mut World) {
    if current(world) == 0
        && world.resource::<Model>().unwrap().chat.session().is_some()
        && snapshot(world).layout.tabs.is_empty()
        && !world.resource::<Deck>().unwrap().closed.contains(&0)
    {
        register(world);
    }
    let old = current(world);
    let routes: Vec<_> = snapshot(world)
        .layout
        .groups
        .iter()
        .filter_map(|g| g.active_tab_id.as_deref().and_then(route))
        .filter(|r| *r != old)
        .collect();
    // Paint each retained view against its own context; focused shared chrome
    // is deliberately painted last. No editor or timeline is remounted.
    for r in routes {
        swap(world, r);
        sync(world);
    }
    swap(world, old);
    sync(world);
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
                    Some(TabSpec::new(r, tab.title()).dirty(dirty))
                })
                .collect();
            if let Some(selected) = g.active_tab_id.as_deref().and_then(route) {
                let strip = TabStrip::mount(
                    world,
                    parents[i],
                    UiId(0x5441_4200 + i as u64),
                    TabStripProps::new("Conversation tabs").active(i == active_group),
                    &specs,
                    TabId(selected),
                )
                .unwrap();
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
    let mut headers: Vec<_> = world
        .resource::<Deck>()
        .unwrap()
        .parked
        .values()
        .map(|p| p.ui.header)
        .collect();
    headers.push(world.resource::<Ui>().unwrap().header);
    for header in headers {
        let row = world.get::<Parent>(header).unwrap().entity;
        let split_button = button(
            world,
            row,
            SPLIT,
            "Split",
            snap.layout.tabs.len() > 1 && snap.layout.groups.len() == 1,
        );
        let join_button = button(world, row, JOIN, "Join", snap.layout.groups.len() > 1);
        for (e, visible) in [
            (split_button.root, snap.layout.groups.len() == 1),
            (join_button.root, snap.layout.groups.len() > 1),
        ] {
            world.get_mut::<Layout>(e).unwrap().width = Size::Px(52.);
            if visible {
                world.remove::<Hidden>(e);
            } else {
                world.insert(e, Hidden);
            }
        }
    }
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
        app.update();
        assert!(snapshot(app.world()).layout.tabs.is_empty());
        open(app.world_mut(), Some("a".into()));
        assert_eq!(current(app.world()), a);
        assert_eq!(draft(app.world()), "keep me");
        assert_eq!(snapshot(app.world()).layout.tabs.len(), 1);
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
    fn notes_switch_retains_split_routes_drafts_and_background_replies() {
        use crate::surfaces::{self, SurfaceId};
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
        surfaces::select(app.world_mut(), SurfaceId::Notes);
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
        assert_eq!(surfaces::active(app.world()), SurfaceId::Notes);
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
