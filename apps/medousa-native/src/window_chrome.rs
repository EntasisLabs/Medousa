//! App-owned window band. RNDR owns native traffic lights, dragging, caption
//! actions, resizing, tab semantics and drag/drop; no simulated macOS buttons.
use super::*;
use rndr_ui::tabs::TabStripHandle;

pub const HEIGHT: f32 = 48.;
struct BandStrip(TabStripHandle);
impl Component for BandStrip {}
struct Chrome {
    band: Entity,
    leading: Entity,
    chat: Entity,
    tabs: Entity,
    fallback: Entity,
    destination: Entity,
    documents: [Entity; 2],
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
        Align {
            items: Items::Center,
            ..Default::default()
        },
    );
    visual::flat(world, band, visual::navigation());
    let leading = container(
        world,
        Some(band),
        Layout {
            width: Size::Px(78.),
            height: Size::Px(1.),
            ..Layout::row()
        },
    );
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
    button(world, chat, DETAILS, "Info", true);
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
    world.insert_resource(Chrome {
        band,
        leading,
        chat,
        tabs,
        fallback,
        destination,
        documents,
        caption,
    });
    container(
        world,
        Some(root),
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            ..Layout::row()
        },
    )
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
    let Some(state) = world.resource::<Chrome>() else {
        return;
    };
    let host = state.documents[if notes { 0 } else { 1 }];
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
    visual::flat(world, bar, visual::navigation());
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

pub fn polish(world: &mut World) {
    let Some(state) = world.resource::<Chrome>() else {
        return;
    };
    let (band, leading, chat, destination, caption, tabs) = (
        state.band,
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
        frame.leading.max(78.)
    } else {
        frame.leading
    };
    world.get_mut::<Layout>(leading).unwrap().width = Size::Px(native);
    show(world, caption, frame.draws_controls);
    let active = crate::surfaces::active(world);
    show(world, chat, active == crate::surfaces::SurfaceId::Chat);
    let documents = world.resource::<Chrome>().unwrap().documents;
    let mut has_document = false;
    for (i, host) in documents.into_iter().enumerate() {
        let selected = active
            == if i == 0 {
                crate::surfaces::SurfaceId::Notes
            } else {
                crate::surfaces::SurfaceId::Code
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
        _ => "Medousa",
    }
    .into();
    // A fine boundary, not another rounded wrapper around the workspace.
    visual::flat(world, band, visual::navigation());
    let bars: Vec<_> = [tabs, documents[0], documents[1]]
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
        let required = count as f32 * 218. + count.saturating_sub(1) as f32 * 4.;
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
    let mut stack: Vec<_> = [tabs, documents[0], documents[1]]
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
            let token = if pressed {
                Token::ControlPressed
            } else if selected {
                Token::Selected
            } else if hovered {
                Token::ControlHover
            } else {
                visual::navigation()
            };
            world.insert(entity, visual::color(world, token));
            world.insert(entity, Radius(visual::CONTROL_RADIUS));
            let layout = world.get_mut::<Layout>(entity).unwrap();
            layout.width = Size::Px(218.);
            layout.height = Size::Px(30.);
            layout.padding = 6.;
            for child in children_of(world, entity) {
                if world.has::<Label>(child) {
                    world.insert(child, LabelBox::CONTENT);
                    let style = world
                        .resource::<UiTheme>()
                        .cloned()
                        .unwrap_or_default()
                        .typography(Typography::Label);
                    world.get_mut::<Layout>(child).unwrap().height = Size::Hug;
                    world.insert(child, style);
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
            assert!(w.get::<Rect>(ui.sidebar).unwrap().y >= HEIGHT);
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
