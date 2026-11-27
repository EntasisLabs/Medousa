//! App-owned window band. RNDR owns native traffic lights, dragging, caption
//! actions, resizing, tab semantics and drag/drop; no simulated macOS buttons.
//!
//! The band floats over the window's top edge with no fill of its own. Every
//! pane below runs to the top and pads its contents clear of the band, so the
//! navigation column and the content beside it are split the full height of
//! the window rather than painted to look that way.
use super::*;
use rndr_ui::tabs::TabStripHandle;

pub const HEIGHT: f32 = 48.;
/// Titlebar tabs: slim chips, as medousa-home draws them.
const TAB_HEIGHT: f32 = 28.;
const TAB_TEXT: f32 = 13.;
const TAB_ICON: f32 = 14.;
const TAB_CLOSE: f32 = 12.;
const TAB_CLOSE_BOX: f32 = 16.;
/// RNDR's narrowest tab, below which the strip scrolls.
const TAB_MIN: f32 = 96.;
struct BandStrip(TabStripHandle);
impl Component for BandStrip {}
struct Chrome {
    band: Entity,
    side: Entity,
    more: Entity,
    body: Entity,
    leading: Entity,
    chat: Entity,
    tabs: Entity,
    fallback: Entity,
    destination: Entity,
    documents: [Entity; 4],
    caption: Entity,
}

pub fn mount(world: &mut World, root: Entity) -> Entity {
    let band = container(
        world,
        Some(root),
        Layout {
            width: Size::Fill,
            height: Size::Px(HEIGHT),
            gap: 8.,
            padding: 6.,
            ..Layout::row()
        },
    );
    world.insert(band, WindowDrag);
    world.insert(
        band,
        Absolute {
            left: Some(0.),
            top: Some(0.),
            right: Some(0.),
            bottom: None,
        },
    );
    world.insert(
        band,
        Align {
            items: Items::Center,
            ..Default::default()
        },
    );
    // The leading segment runs edge to edge so navigation reaches the top.
    world.insert(
        band,
        Padding {
            right: 6.,
            ..Padding::default()
        },
    );
    // Reserves the traffic lights and whatever navigation runs beneath them,
    // so the band's controls start where the content pane does.
    let leading = container(
        world,
        Some(band),
        Layout {
            width: Size::Px(crate::surfaces::RAIL_WIDTH),
            height: Size::Fill,
            ..Layout::row()
        },
    );
    // Non-chat destinations' sidebar toggle, where chat keeps its own.
    let side = button(world, band, SURFACE_SIDEBAR, "Sidebar", true).root;
    let chat = container(
        world,
        Some(band),
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            gap: 6.,
            ..Layout::row()
        },
    );
    world.insert(
        chat,
        Align {
            items: Items::Center,
            ..Default::default()
        },
    );
    button(world, chat, SIDEBAR, "Chats", true);
    let tabs = container(
        world,
        Some(chat),
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            gap: 8.,
            ..Layout::row()
        },
    );
    world.insert(
        tabs,
        Align {
            items: Items::Center,
            ..Default::default()
        },
    );
    let fallback = text(world, tabs, "New conversation", Typography::Emphasis).unwrap();
    world.get_mut::<Layout>(fallback).unwrap().width = Size::Fill;
    button(world, chat, NEW, "New chat", true);
    let more = button(world, chat, CONVERSATION_MENU, "Conversation", true).root;
    let destination = text(world, band, "Notes", Typography::Emphasis).unwrap();
    world.get_mut::<Layout>(destination).unwrap().width = Size::Fill;
    let documents = std::array::from_fn(|_| {
        container(
            world,
            Some(band),
            Layout {
                width: Size::Fill,
                height: Size::Fill,
                ..Layout::row()
            },
        )
    });
    let caption = container(
        world,
        Some(band),
        Layout {
            width: Size::Hug,
            height: Size::Fill,
            gap: 2.,
            ..Layout::row()
        },
    );
    world.insert(
        caption,
        Align {
            items: Items::Center,
            ..Default::default()
        },
    );
    for (index, kind, icon, name) in [
        (0, WindowButton::Minimize, Icon::Minus, "Minimize window"),
        (
            1,
            WindowButton::Maximize,
            Icon::Square,
            "Maximize or restore window",
        ),
        (2, WindowButton::Close, Icon::X, "Close window"),
    ] {
        let props = ButtonProps::new(name, HostIntent::Command(0x5749_0000 + index));
        let h = controls::IconButton::icon(
            world,
            caption,
            UiId(0x5749_0000 + index),
            icon,
            name,
            props,
        )
        .unwrap();
        world.get_mut::<Layout>(h.root).unwrap().width = Size::Px(36.);
        world.get_mut::<LabelBox>(h.label).unwrap().inset_x = 0.;
        world.insert(h.root, kind);
        world.insert(h.root, ActivationHandler(caption_action));
    }
    let body = container(
        world,
        Some(root),
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            ..Layout::row()
        },
    );
    world.insert_resource(Chrome {
        band,
        side,
        more,
        body,
        leading,
        chat,
        tabs,
        fallback,
        destination,
        documents,
        caption,
    });
    body
}
/// The open conversation's ⋯ button, which its menu hangs from.
pub(crate) fn conversation_button(world: &World) -> Option<Entity> {
    world.resource::<Chrome>().map(|c| c.more)
}
/// The window band, for overlays that hang from it.
pub(crate) fn band(world: &World) -> Option<Entity> {
    world.resource::<Chrome>().map(|c| c.band)
}
fn caption_action(world: &mut World, entity: Entity) {
    let action = match world.get::<WindowButton>(entity) {
        Some(WindowButton::Minimize) => WindowAction::Minimize,
        Some(WindowButton::Maximize) => WindowAction::ToggleMaximize,
        Some(WindowButton::Close) => WindowAction::Close,
        None => return,
    };
    request_window_action(world, action);
}
/// Right edge of the visible navigation column: the rail plus whichever
/// sidebar the active destination shows. Read from the last layout.
fn navigation_edge(world: &World) -> f32 {
    let shown = |mut entity: Entity| loop {
        if world.has::<Hidden>(entity) {
            return false;
        }
        match world.get::<Parent>(entity) {
            Some(parent) => entity = parent.entity,
            None => return true,
        }
    };
    // Every navigation sidebar: document sidebars, chat's, and the others
    // marked as sidebars (Peers), so the band never covers one.
    let sidebars = world
        .entities_with::<visual::DocumentSidebar>()
        .into_iter()
        .chain(world.entities_with::<visual::SidebarPane>())
        .chain(world.resource::<Ui>().map(|ui| ui.sidebar));
    let edge = crate::surfaces::rail(world)
        .into_iter()
        .chain(sidebars)
        .filter(|&e| shown(e))
        .filter_map(|e| world.get::<Rect>(e))
        .filter(|r| r.w > 1.)
        .map(|r| r.x + r.w)
        .fold(0., f32::max);
    // A resize sash after the sidebar draws its hairline at its center; meet
    // that line instead of leaving a sliver of canvas before it.
    world
        .entities_with::<Splitter>()
        .into_iter()
        .filter(|&e| shown(e))
        .filter_map(|e| world.get::<Rect>(e))
        .filter(|r| r.h > r.w && r.x >= edge - 1. && r.x <= edge + 16.)
        .map(|r| r.x + r.w / 2.)
        .fold(edge, f32::max)
}
fn show(world: &mut World, entity: Entity, visible: bool) {
    if visible {
        world.remove::<Hidden>(entity);
    } else {
        world.insert(entity, Hidden);
    }
}
/// Bars live in a stable window-owned host, never in a disposable route view.
/// Their RNDR strip/body identity and drop-target relationships are unchanged.
pub(super) fn begin_tabs(world: &mut World) {
    let host = world.resource::<Chrome>().unwrap().tabs;
    for child in children_of(world, host) {
        world.insert(child, Hidden);
    }
    let fallback = world.resource::<Chrome>().unwrap().fallback;
    world.remove::<Hidden>(fallback);
}
pub(super) fn place_tabs(world: &mut World, strip: TabStripHandle) {
    let bar = world.get::<Parent>(strip.scroll).unwrap().entity;
    let state = world.resource::<Chrome>().unwrap();
    let host = state.tabs;
    let fallback = state.fallback;
    set_parent(world, bar, host);
    world.insert(bar, BandStrip(strip));
    world.remove::<Hidden>(bar);
    world.insert(fallback, Hidden);
    *world.get_mut::<Layout>(bar).unwrap() = Layout {
        width: Size::Fill,
        height: Size::Fill,
        gap: 4.,
        ..Layout::row()
    };
    world.insert(bar, WindowDrag);
    // Scroll/overflow stay RNDR-owned. Only the surrounding geometry changes.
    world.get_mut::<Layout>(strip.scroll).unwrap().height = Size::Px(32.);
    world.insert(strip.scroll, Color::rgba(0., 0., 0., 0.));
    world.insert(strip.scroll, ResolvedColor);
    world.insert(bar, Color::rgba(0., 0., 0., 0.));
    world.insert(bar, ResolvedColor);
}
/// Artifacts use the titlebar for their single document toolbar, not a tab row.
pub(crate) fn artifact_toolbar_host(world: &World) -> Option<Entity> {
    world.resource::<Chrome>().map(|state| state.documents[2])
}

/// Keep the strip root in its surface for event identity; only its bar moves.
/// Standalone editor fixtures without a window host retain the local tab row.
pub(crate) fn clear_document_tabs(world: &mut World, notes: bool) {
    let Some(state) = world.resource::<Chrome>() else {
        return;
    };
    let host = state.documents[if notes { 0 } else { 1 }];
    for bar in children_of(world, host) {
        world.insert(bar, Hidden);
    }
}

pub(crate) fn document_tabs(world: &mut World, strip: TabStripHandle, notes: bool) {
    place_document_tabs(world, strip, if notes { 0 } else { 1 });
}
pub(crate) fn terminal_tabs(world: &mut World, strip: TabStripHandle) {
    place_document_tabs(world, strip, 3);
}
pub(crate) fn clear_terminal_tabs(world: &mut World) {
    if let Some(host) = world.resource::<Chrome>().map(|s| s.documents[3]) {
        for bar in children_of(world, host) {
            world.insert(bar, Hidden);
        }
    }
}
pub(crate) fn reset_terminal_tabs(world: &mut World) {
    if let Some(host) = world.resource::<Chrome>().map(|s| s.documents[3]) {
        for bar in children_of(world, host) {
            despawn(world, bar);
        }
    }
}
fn place_document_tabs(world: &mut World, strip: TabStripHandle, index: usize) {
    let Some(state) = world.resource::<Chrome>() else {
        return;
    };
    let host = state.documents[index];
    let bar = world.get::<Parent>(strip.scroll).unwrap().entity;
    set_parent(world, bar, host);
    world.remove::<Hidden>(bar);
    world.insert(bar, BandStrip(strip));
    *world.get_mut::<Layout>(bar).unwrap() = Layout {
        width: Size::Fill,
        height: Size::Fill,
        gap: 4.,
        ..Layout::row()
    };
    world.insert(bar, WindowDrag);
    visual::flat(world, bar, visual::canvas());
    world.get_mut::<Layout>(strip.scroll).unwrap().height = Size::Px(32.);
    world.insert(strip.scroll, Color::rgba(0., 0., 0., 0.));
    world.insert(strip.scroll, ResolvedColor);
    world.insert(strip.root, Hidden);
    if let Some(parent) = world.get::<Parent>(strip.root).map(|p| p.entity) {
        world.get_mut::<Layout>(parent).unwrap().height = Size::Px(0.);
    }
}

pub(super) fn pane_commands(world: &mut World, multiple: bool, split: bool) {
    let host = world.resource::<Chrome>().unwrap().tabs;
    for bar in children_of(world, host) {
        if world.has::<Hidden>(bar) || world.has::<Label>(bar) {
            continue;
        }
        let a = button(world, bar, 40, "Split", multiple && !split);
        let b = button(world, bar, 41, "Join", split);
        show(world, a.root, !split);
        show(world, b.root, split);
    }
}

/// A pane padded clear of the band. Keeps any inset the app authored.
struct TitlebarInset {
    authored: Option<Padding>,
}
impl Component for TitlebarInset {}

/// Structure that only arranges panes side by side: rows, and a column whose
/// one visible child is such a row (a surface host or a split's pane).
fn passes_through(world: &World, entity: Entity) -> bool {
    let row = |e: Entity| {
        !world.has::<Scroll>(e) && world.get::<Layout>(e).is_some_and(|l| l.axis == Axis::Row)
    };
    if row(entity) {
        return true;
    }
    let shown: Vec<_> = children_of(world, entity)
        .into_iter()
        .filter(|&c| !world.has::<Hidden>(c))
        .collect();
    !world.has::<Scroll>(entity) && shown.len() == 1 && row(shown[0])
}

/// Holds a scrolling pane below the band. Padding on the scroller itself
/// would scroll away with its content and slide it under the band.
struct TitlebarWrap {
    pane: Entity,
}
impl Component for TitlebarWrap {}

/// Put `pane` inside a wrapper that takes its place among its siblings.
fn wrap(world: &mut World, pane: Entity) -> Entity {
    let parent = world.get::<Parent>(pane).unwrap().entity;
    let wrapper = world.spawn((Layout::column(), TitlebarWrap { pane }));
    world.insert(wrapper, Parent { entity: parent });
    let children = world.get_mut::<Children>(parent).unwrap();
    let index = children.list.iter().position(|&c| c == pane).unwrap();
    children.list[index] = wrapper;
    world.remove::<Parent>(pane);
    set_parent(world, pane, wrapper);
    sync_wrap(world, wrapper, pane);
    wrapper
}

/// Keep a wrapper sized, painted and shown like its pane. A remounted surface
/// drops the old pane; its empty wrapper goes with it.
fn sync_wrap(world: &mut World, wrapper: Entity, pane: Entity) -> bool {
    if !world.is_alive(pane) || world.get::<Parent>(pane).map(|p| p.entity) != Some(wrapper) {
        despawn(world, wrapper);
        return false;
    }
    let inner = *world.get::<Layout>(pane).unwrap();
    let layout = world.get_mut::<Layout>(wrapper).unwrap();
    layout.width = if inner.width == Size::Fill {
        Size::Fill
    } else {
        Size::Hug
    };
    layout.height = Size::Fill;
    layout.min_width = inner.min_width;
    world.get_mut::<Layout>(pane).unwrap().height = Size::Fill;
    match world.get::<UiStyle>(pane).and_then(|s| s.background) {
        Some(background) => world.insert(
            wrapper,
            UiStyle {
                background: Some(background),
                ..Default::default()
            },
        ),
        None => {
            world.remove::<UiStyle>(wrapper);
        }
    }
    show(world, wrapper, !world.has::<Hidden>(pane));
    true
}

/// Every pane whose top is the window's top pads its contents below the band.
/// Sashes stay full height so their hairline runs to the top too.
/// A navigation sidebar: no traffic lights above it, so its header takes
/// the titlebar row instead of sitting below an empty band.
fn sidebar(world: &World, pane: Entity) -> bool {
    let mut stack = vec![(pane, 0)];
    while let Some((e, depth)) = stack.pop() {
        if world.has::<visual::DocumentSidebar>(e) || world.has::<visual::SidebarPane>(e) {
            return true;
        }
        if depth < 3 {
            stack.extend(children_of(world, e).into_iter().map(|c| (c, depth + 1)));
        }
    }
    false
}
/// The sidebar's first header row or title, which then drags the window as
/// the band does over the content column.
fn header(world: &World, pane: Entity) -> Option<Entity> {
    let mut node = pane;
    for _ in 0..6 {
        let first = children_of(world, node)
            .into_iter()
            .find(|c| !world.has::<Hidden>(*c) && !world.has::<Absolute>(*c))?;
        let row = world
            .get::<Layout>(first)
            .is_some_and(|l| l.axis == Axis::Row);
        if row || world.has::<Label>(first) {
            return Some(first);
        }
        node = first;
    }
    None
}
fn inset_panes(world: &mut World) {
    let Some(body) = world.resource::<Chrome>().map(|c| c.body) else {
        return;
    };
    let mut panes = Vec::new();
    let mut stack = children_of(world, body);
    while let Some(entity) = stack.pop() {
        if world.has::<Splitter>(entity) || world.get::<Layout>(entity).is_none() {
            continue;
        }
        if let Some(wrap) = world.get::<TitlebarWrap>(entity).map(|w| w.pane) {
            if sync_wrap(world, entity, wrap) {
                panes.push(entity);
            }
        } else if passes_through(world, entity) {
            stack.extend(children_of(world, entity));
        } else if world.has::<Scroll>(entity) {
            panes.push(wrap(world, entity));
        } else {
            panes.push(entity);
        }
    }
    for &pane in &panes {
        if !world.has::<TitlebarInset>(pane) {
            let authored = world.get::<Padding>(pane).copied();
            world.insert(pane, TitlebarInset { authored });
        }
        let base = world
            .get::<TitlebarInset>(pane)
            .and_then(|i| i.authored)
            .unwrap_or_else(|| Padding::all(world.get::<Layout>(pane).unwrap().padding));
        let navigation = sidebar(world, pane);
        if navigation {
            if let Some(header) = header(world, pane) {
                if !world.has::<WindowDrag>(header) {
                    world.insert(header, WindowDrag);
                }
            }
        }
        let padded = Padding {
            top: base.top + if navigation { 0. } else { HEIGHT },
            ..base
        };
        if world.get::<Padding>(pane) != Some(&padded) {
            world.insert(pane, padded);
        }
    }
    let stale: Vec<_> = world
        .entities_with::<TitlebarInset>()
        .into_iter()
        .filter(|e| !panes.contains(e))
        .collect();
    for entity in stale {
        match world.get::<TitlebarInset>(entity).and_then(|i| i.authored) {
            Some(authored) => world.insert(entity, authored),
            None => {
                world.remove::<Padding>(entity);
            }
        }
        world.remove::<TitlebarInset>(entity);
    }
}

pub fn polish(world: &mut World) {
    let Some(state) = world.resource::<Chrome>() else {
        return;
    };
    let (leading, chat, destination, caption, tabs) = (
        state.leading,
        state.chat,
        state.destination,
        state.caption,
        state.tabs,
    );
    let frame = world
        .resource::<WindowChrome>()
        .copied()
        .unwrap_or_default();
    // Headless captures reserve the expected macOS controls but never draw fakes.
    let native = if cfg!(target_os = "macos") && !frame.draws_controls {
        frame.leading.max(crate::surfaces::RAIL_WIDTH)
    } else {
        frame.leading
    };
    // The band floats over the content column only. The navigation column
    // owns its own top: the rail holds the traffic lights and a sidebar's
    // header rises into the titlebar row beside them.
    let edge = navigation_edge(world);
    let band = world.resource::<Chrome>().unwrap().band;
    if let Some(pin) = world.get_mut::<Absolute>(band) {
        if pin.left != Some(edge) {
            pin.left = Some(edge);
        }
    }
    world.get_mut::<Layout>(leading).unwrap().width = Size::Px((native - edge).max(0.));
    inset_panes(world);
    show(world, caption, frame.draws_controls);
    let active = crate::surfaces::active(world);
    show(world, chat, active == crate::surfaces::SurfaceId::Chat);
    let side = world.resource::<Chrome>().unwrap().side;
    let toggle = active != crate::surfaces::SurfaceId::Chat && crate::sidebars::available(world);
    show(world, side, toggle);
    let documents = world.resource::<Chrome>().unwrap().documents;
    let mut has_document = false;
    for (i, host) in documents.into_iter().enumerate() {
        let selected = active
            == match i {
                0 => crate::surfaces::SurfaceId::Notes,
                1 => crate::surfaces::SurfaceId::Code,
                2 => crate::surfaces::SurfaceId::Artifacts,
                _ => crate::surfaces::SurfaceId::Terminal,
            };
        let populated = children_of(world, host)
            .iter()
            .any(|e| !world.has::<Hidden>(*e));
        show(world, host, selected && populated);
        has_document |= selected && populated;
    }
    show(
        world,
        destination,
        active != crate::surfaces::SurfaceId::Chat && !has_document,
    );
    world.get_mut::<Label>(destination).unwrap().text = match active {
        crate::surfaces::SurfaceId::Notes => "Notes",
        crate::surfaces::SurfaceId::Code => "Code",
        crate::surfaces::SurfaceId::Artifacts => "Artifacts",
        crate::surfaces::SurfaceId::Terminal => "Terminal",
        _ => "Medousa",
    }
    .into();
    let bars: Vec<_> = [tabs, documents[0], documents[1], documents[3]]
        .into_iter()
        .flat_map(|host| children_of(world, host))
        .collect();
    for bar in bars {
        let Some(strip) = world.get::<BandStrip>(bar).map(|s| s.0) else {
            continue;
        };
        let count = children_of(world, strip.list)
            .iter()
            .filter(|e| world.has::<Tab>(**e))
            .count();
        // Decide against the space WITHOUT overflow chrome, avoiding a visibility
        // feedback loop when a lone tab fits but the three extra buttons do not.
        let required = count as f32 * TAB_MIN + count.saturating_sub(1) as f32 * 4.;
        let overflow = world.get::<Rect>(bar).is_some_and(|r| required > r.w - 48.);
        for control in [
            strip.scroll_back(world),
            strip.scroll_forward(world),
            strip.overflow_button(world),
        ]
        .into_iter()
        .flatten()
        {
            show(world, control, overflow);
        }
    }
    let mut stack: Vec<_> = [tabs, documents[0], documents[1], documents[3]]
        .into_iter()
        .flat_map(|host| children_of(world, host))
        .collect();
    let focus = world.resource::<Focus>().and_then(|f| f.entity);
    let pointer = world
        .resource::<Input>()
        .map(|i| i.pointer.position)
        .unwrap_or([-1., -1.]);
    while let Some(entity) = stack.pop() {
        stack.extend(children_of(world, entity));
        if let Some(tab) = world.get::<Tab>(entity).copied() {
            let selected = world
                .get::<Parent>(entity)
                .and_then(|p| world.get::<Tabs>(p.entity))
                .is_some_and(|t| t.selected == tab.id);
            let hovered = world.get::<Rect>(entity).is_some_and(|r| {
                pointer[0] >= r.x
                    && pointer[0] < r.x + r.w
                    && pointer[1] >= r.y
                    && pointer[1] < r.y + r.h
            });
            let pressed = hovered && world.resource::<Input>().is_some_and(|i| i.pointer.down);
            // Quiet chips, as medousa-home's titlebar tabs: the open tab is a
            // soft neutral plane, the others are bare text until hovered.
            let token = if pressed {
                Token::ControlPressed
            } else if selected {
                Token::Raised
            } else if hovered {
                Token::ControlHover
            } else {
                visual::canvas()
            };
            let fill = visual::color(world, token);
            world.insert(
                entity,
                if selected || hovered || pressed {
                    fill
                } else {
                    Color::rgba(0., 0., 0., 0.)
                },
            );
            world.insert(entity, Radius(visual::CONTROL_RADIUS));
            // Width stays RNDR's: sized to the title within its 96-200pt range.
            let layout = world.get_mut::<Layout>(entity).unwrap();
            layout.height = Size::Px(TAB_HEIGHT);
            layout.padding = 0.;
            layout.gap = 6.;
            world.insert(
                entity,
                Padding {
                    left: 10.,
                    right: 4.,
                    top: 0.,
                    bottom: 0.,
                },
            );
            world.insert(
                entity,
                Align {
                    items: Items::Center,
                    ..Default::default()
                },
            );
            let theme = world.resource::<UiTheme>().cloned().unwrap_or_default();
            let ink = visual::color(
                world,
                if selected || hovered {
                    visual::text()
                } else {
                    visual::secondary()
                },
            );
            let quiet = visual::color(world, visual::secondary());
            for child in children_of(world, entity) {
                if !world.has::<Label>(child) {
                    continue;
                }
                let glyph = world.get::<Label>(child).is_some_and(|l| {
                    let mut chars = l.text.chars();
                    matches!((chars.next(), chars.next()), (Some(ch), None) if rndr_core::is_icon_scalar(ch))
                });
                let mut style = theme.typography(Typography::Label);
                if world.has::<Button>(child) {
                    // Close: small, and only there when the tab is in play.
                    let shown = selected || hovered || focus == Some(child);
                    style.size = TAB_CLOSE;
                    style.line_height = Some(1.);
                    style.align = TextAlign::Center;
                    world.insert(child, Opacity(if shown { 1. } else { 0. }));
                    let label = world.get_mut::<Label>(child).unwrap();
                    label.color = Some(quiet);
                    label.overflow = TextOverflow::Visible;
                    // An even inset centres the glyph in its hover square.
                    let inset = (TAB_CLOSE_BOX - TAB_CLOSE) / 2.;
                    world.insert(
                        child,
                        LabelBox {
                            inset_x: inset,
                            inset_y: inset,
                            min_height: 0.,
                        },
                    );
                    let layout = world.get_mut::<Layout>(child).unwrap();
                    layout.width = Size::Px(TAB_CLOSE_BOX);
                    layout.height = Size::Px(TAB_CLOSE_BOX);
                    world.insert(child, Radius(4.));
                } else if glyph {
                    style.size = TAB_ICON;
                    style.line_height = Some(1.);
                    let label = world.get_mut::<Label>(child).unwrap();
                    label.color = Some(quiet);
                    label.overflow = TextOverflow::Visible;
                    world.insert(
                        child,
                        LabelBox {
                            inset_x: 0.,
                            inset_y: 0.,
                            min_height: 0.,
                        },
                    );
                    let layout = world.get_mut::<Layout>(child).unwrap();
                    layout.width = Size::Px(TAB_ICON);
                    layout.height = Size::Px(TAB_ICON);
                } else {
                    style.size = TAB_TEXT;
                    world.insert(child, LabelBox::CONTENT);
                    world.get_mut::<Layout>(child).unwrap().height = Size::Hug;
                    world.get_mut::<Label>(child).unwrap().color = Some(ink);
                }
                world.insert(child, style);
            }
            // The selected plane is the cue; no underline beneath it.
            for child in children_of(world, entity) {
                if world.has::<Absolute>(child) && !world.has::<Label>(child) {
                    world.insert(child, Hidden);
                }
            }
            // RNDR's tab focus border stays authoritative.
            if focus != Some(entity) {
                world.remove::<Border>(entity);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> App {
        let mut app = super::super::app();
        app.world_mut().insert_resource(Viewport::new(1200., 850.));
        for _ in 0..8 {
            app.update();
        }
        app
    }
    fn visible(world: &World, mut entity: Entity) -> bool {
        loop {
            if world.has::<Hidden>(entity) {
                return false;
            }
            let Some(p) = world.get::<Parent>(entity) else {
                return true;
            };
            entity = p.entity;
        }
    }
    #[test]
    fn titlebar_keeps_native_mac_controls_and_provides_frameless_captions() {
        let mut app = fixture();
        let config = app.world().resource::<WindowConfig>().unwrap().clone();
        assert_eq!(config.titlebar.unwrap().height, HEIGHT);
        assert!(config.decorations && !config.transparent);
        for controls in [false, true] {
            app.world_mut().insert_resource(WindowChrome {
                draws_controls: controls,
                ..Default::default()
            });
            for _ in 0..3 {
                app.update();
            }
            let w = app.world();
            let c = w.resource::<Chrome>().unwrap();
            assert_eq!(visible(w, c.caption), controls);
            assert_eq!(w.get::<Rect>(c.band).unwrap().h, HEIGHT);
            let ui = w.resource::<Ui>().unwrap();
            // The band covers the content column only, past the sidebar.
            let sidebar = *w.get::<Rect>(ui.sidebar).unwrap();
            assert!(w.get::<Rect>(c.band).unwrap().x >= sidebar.x + sidebar.w - 1.);
            if controls {
                for (button, _) in w.query::<WindowButton>() {
                    assert!(visible(w, button));
                    assert!(w.has::<Focusable>(button));
                    let rect = w.get::<Rect>(button).unwrap();
                    assert!(rect.x + rect.w <= 1200. && rect.y + rect.h <= HEIGHT);
                }
            }
        }
        for surface in [
            crate::surfaces::SurfaceId::Notes,
            crate::surfaces::SurfaceId::Code,
            crate::surfaces::SurfaceId::Chat,
        ] {
            crate::surfaces::select(app.world_mut(), surface);
            for _ in 0..3 {
                app.update();
            }
            let w = app.world();
            let c = w.resource::<Chrome>().unwrap();
            assert_eq!(
                visible(w, c.chat),
                surface == crate::surfaces::SurfaceId::Chat
            );
        }
    }
    #[test]
    fn panes_run_to_the_top_and_sidebar_headers_take_the_titlebar_row() {
        let mut app = fixture();
        for surface in [
            crate::surfaces::SurfaceId::Chat,
            crate::surfaces::SurfaceId::Notes,
            crate::surfaces::SurfaceId::Settings,
        ] {
            crate::surfaces::select(app.world_mut(), surface);
            for _ in 0..3 {
                app.update();
            }
            let w = app.world();
            let c = w.resource::<Chrome>().unwrap();
            // The band floats with no fill, so panes below show through it.
            assert!(
                w.get::<UiStyle>(c.band)
                    .and_then(|s| s.background)
                    .is_none()
            );
            let rail = crate::surfaces::rail(w).unwrap();
            assert_eq!(w.get::<Rect>(rail).unwrap().y, 0.);
            assert!(w.get::<Padding>(rail).unwrap().top >= HEIGHT);
            // The navigation column is rail + sidebar, side by side from y=0.
            let sidebar = w
                .entities_with::<TitlebarInset>()
                .into_iter()
                .filter(|&e| visible(w, e) && e != rail)
                .filter_map(|e| w.get::<Rect>(e).map(|r| (e, *r)))
                .find(|(_, r)| (r.x - crate::surfaces::RAIL_WIDTH).abs() < 0.5)
                .expect("a pane beside the rail");
            assert_eq!(sidebar.1.y, 0., "{surface:?}");
            // No traffic lights above a sidebar: its header takes the
            // titlebar row and drags the window like the band beside it.
            let head = header(w, sidebar.0).expect("a sidebar header");
            let r = *w.get::<Rect>(head).unwrap();
            assert!(r.y < HEIGHT && r.y + r.h <= HEIGHT, "{surface:?} {r:?}");
            assert!(w.has::<WindowDrag>(head), "{surface:?}");
            let band = *w.get::<Rect>(c.band).unwrap();
            assert!(band.x >= sidebar.1.x + sidebar.1.w - 1., "{surface:?}");
        }
    }
    #[test]
    fn empty_band_drags_but_operating_controls_do_not() {
        let mut app = fixture();
        app.world_mut().insert_resource(WindowChrome::default());
        let band = app.world().resource::<Chrome>().unwrap().band;
        let r = *app.world().get::<Rect>(band).unwrap();
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.pointer.position = [r.x + r.w / 2., r.y + 2.];
        input.pointer.press_at = Some(input.pointer.position);
        input.pointer.pressed = true;
        input.pointer.button = PointerButton::Left;
        apply_window_chrome(app.world_mut());
        assert_eq!(
            take_window_actions(app.world_mut()),
            vec![WindowAction::Drag]
        );
        let chat = app.world().resource::<Chrome>().unwrap().chat;
        let control = children_of(app.world(), chat)[0];
        let r = *app.world().get::<Rect>(control).unwrap();
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.pointer.position = [r.x + r.w / 2., r.y + r.h / 2.];
        input.pointer.press_at = Some(input.pointer.position);
        input.pointer.pressed = true;
        apply_window_chrome(app.world_mut());
        assert!(take_window_actions(app.world_mut()).is_empty());
        // Keyboard activation reaches the exact same native caption action.
        let close = app
            .world()
            .query::<WindowButton>()
            .iter()
            .find(|(_, b)| **b == WindowButton::Close)
            .unwrap()
            .0;
        caption_action(app.world_mut(), close);
        assert_eq!(
            take_window_actions(app.world_mut()),
            vec![WindowAction::Close]
        );
    }
}
