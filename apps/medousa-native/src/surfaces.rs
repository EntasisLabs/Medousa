//! App-level navigation. Each destination retains its own root and state; a
//! conversation tab is never repurposed as a non-chat screen.
use rndr_core::{editing, *};
use rndr_ui::{
    controls::{self, ButtonProps, HostIntent},
    inputs::Tooltip,
    *,
};

pub const RAIL_WIDTH: f32 = 56.;
const NAV_TARGET: f32 = 40.;
const NAV_ICON: f32 = 20.;
const CHAT: u64 = 0x5355_0001;
const NOTES: u64 = 0x5355_0002;
const CODE: u64 = 0x5355_0003;
const REVIEWS: u64 = 0x5355_0004;
const CALENDAR: u64 = 0x5355_0005;
const PEERS: u64 = 0x5355_0006;
const CREATE: u64 = 0x5355_0007;
const SETTINGS: u64 = 0x5355_0008;
const SURFACE_COUNT: usize = 8;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceId {
    Chat,
    Notes,
    Code,
    Create,
    Reviews,
    Calendar,
    Peers,
    Settings,
}
impl SurfaceId {
    fn index(self) -> usize {
        match self {
            Self::Chat => 0,
            Self::Notes => 1,
            Self::Code => 2,
            Self::Create => 3,
            Self::Reviews => 4,
            Self::Calendar => 5,
            Self::Peers => 6,
            Self::Settings => 7,
        }
    }
}
struct SurfaceHost {
    active: SurfaceId,
    roots: [Entity; SURFACE_COUNT],
    rail: Entity,
    tiles: [Entity; SURFACE_COUNT],
    buttons: [controls::ControlHandle; SURFACE_COUNT],
    icons: [Entity; SURFACE_COUNT],
    icon_keys: [Option<(u32, [u8; 4])>; SURFACE_COUNT],
    focus: [Option<Entity>; SURFACE_COUNT],
}
fn panel(world: &mut World, parent: Entity, layout: Layout) -> Entity {
    let entity = world.spawn((layout,));
    set_parent(world, entity, parent);
    entity
}
/// Mount registered destinations once. Only real destinations get rail entries.
/// Further surfaces can supply their own retained root without changing Chat.
pub fn mount(world: &mut World, root: Entity) -> Entity {
    let rail = panel(
        world,
        root,
        Layout {
            width: Size::Px(RAIL_WIDTH),
            height: Size::Fill,
            padding: 8.,
            gap: 8.,
            ..Layout::column()
        },
    );
    crate::native_style::plane(world, rail, crate::native_style::rail());
    world.insert(rail, ResolvedColor);
    world.insert(
        rail,
        Semantics {
            name: Some("App navigation".into()),
            ..Default::default()
        },
    );
    let mut tiles = Vec::new();
    let mut buttons = Vec::new();
    let mut icons = Vec::new();
    for (command, icon, name) in [
        (CHAT, Icon::MessageCircle, "Chat"),
        (NOTES, Icon::Book, "Notes"),
        (CODE, Icon::Code, "Code"),
        (CREATE, Icon::PenLine, "Create"),
        (REVIEWS, Icon::Check, "Reviews"),
        (CALENDAR, Icon::Clock, "Calendar"),
        (PEERS, Icon::MessageCircle, "Peers"),
        (SETTINGS, Icon::Settings, "Settings"),
    ] {
        // Settings sits at the foot of the rail, above the workshop glyph.
        if command == SETTINGS {
            panel(
                world,
                rail,
                Layout {
                    width: Size::Px(NAV_TARGET),
                    height: Size::Fill,
                    ..Layout::column()
                },
            );
        }
        let tile = panel(
            world,
            rail,
            Layout {
                width: Size::Px(NAV_TARGET),
                height: Size::Px(NAV_TARGET),
                ..Layout::column()
            },
        );
        world.insert(tile, Radius(10.));
        world.insert(tile, ResolvedColor);
        let mut props = ButtonProps::new(name, HostIntent::Command(command));
        props.variant = controls::ButtonVariant::Quiet;
        props.description = format!("Open {name}. Other workspaces remain open in the background.");
        let handle =
            controls::IconButton::icon(world, tile, UiId(command), icon, name, props).unwrap();
        *world.get_mut::<Layout>(handle.root).unwrap() = Layout {
            width: Size::Fill,
            height: Size::Fill,
            padding: 2.,
            ..Layout::column()
        };
        // Preserve the canonical control's two-color focus ring. Give every
        // inner layer the same vertical extent instead of a Hug-height face.
        let mut nodes = children_of(world, handle.root);
        while let Some(node) = nodes.pop() {
            world.get_mut::<Layout>(node).unwrap().height = Size::Fill;
            nodes.extend(children_of(world, node));
        }
        world.insert(
            handle.label,
            LabelBox {
                inset_x: 5.,
                inset_y: 5.,
                min_height: 0.,
            },
        );
        world.insert(
            handle.label,
            TextStyle {
                size: NAV_ICON,
                align: TextAlign::Center,
                line_height: Some(1.),
                ..Default::default()
            },
        );
        world.insert(handle.root, motion::PressScale(0.96));
        // Keep RNDR's real button and accessible name, but paint vector-source
        // artwork instead of enlarging the toolkit's baked 16px glyph atlas.
        let image = world.spawn((
            Layout {
                width: Size::Px(NAV_ICON),
                height: Size::Px(NAV_ICON),
                ..Layout::column()
            },
            Absolute {
                left: Some((NAV_TARGET - NAV_ICON) / 2.),
                top: Some((NAV_TARGET - NAV_ICON) / 2.),
                ..Default::default()
            },
            ImageView::new(0),
        ));
        set_parent(world, image, handle.root);
        icons.push(image);
        Tooltip::attach(world, handle.root, name).unwrap();
        tiles.push(tile);
        buttons.push(handle);
    }
    // Do not imply unavailable destinations or settings with dead controls.
    let content = panel(
        world,
        root,
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            ..Layout::column()
        },
    );
    let roots = [
        "Chat", "Notes", "Code", "Create", "Reviews", "Calendar", "Peers", "Settings",
    ]
    .map(|name| {
        let entity = panel(
            world,
            content,
            Layout {
                width: Size::Fill,
                height: Size::Fill,
                ..Layout::row()
            },
        );
        world.insert(entity, Context::new(name));
        entity
    });
    crate::notes::mount(world, roots[1]);
    crate::code::mount(world, roots[2]);
    crate::create::mount(world, roots[3]);
    crate::reviews::mount(world, roots[4]);
    crate::calendar::mount(world, roots[5]);
    crate::peers::mount(world, roots[6]);
    crate::settings::mount(world, roots[7]);
    for root in &roots[1..] {
        world.insert(*root, Hidden);
    }
    world.insert_resource(SurfaceHost {
        active: SurfaceId::Chat,
        roots,
        rail,
        tiles: tiles.try_into().ok().expect("one tile per surface"),
        buttons: buttons.try_into().ok().expect("one button per surface"),
        icons: icons.try_into().ok().expect("one icon per surface"),
        icon_keys: [None; SURFACE_COUNT],
        focus: [None; SURFACE_COUNT],
    });
    roots[0]
}
/// The destination rail, once mounted.
pub fn rail(world: &World) -> Option<Entity> {
    world.resource::<SurfaceHost>().map(|h| h.rail)
}
pub fn active(world: &World) -> SurfaceId {
    world
        .resource::<SurfaceHost>()
        .map(|h| h.active)
        .unwrap_or(SurfaceId::Chat)
}
pub(crate) fn descendant(world: &World, mut entity: Entity, root: Entity) -> bool {
    for _ in 0..128 {
        if entity == root {
            return true;
        }
        let Some(parent) = world.get::<Parent>(entity) else {
            return false;
        };
        entity = parent.entity;
    }
    false
}
pub fn remember_focus(world: &mut World) {
    let Some(host) = world.resource::<SurfaceHost>() else {
        return;
    };
    let index = host.active.index();
    let root = host.roots[index];
    if let Some(entity) = world.resource::<Focus>().and_then(|f| f.entity) {
        if descendant(world, entity, root) {
            world.resource_mut::<SurfaceHost>().unwrap().focus[index] = Some(entity);
        }
    }
}
pub fn select(world: &mut World, target: SurfaceId) {
    let Some(host) = world.resource::<SurfaceHost>() else {
        return;
    };
    if target == host.active {
        return;
    }
    let composing = match host.active {
        SurfaceId::Chat => {
            let doc = world.resource::<crate::ui::Model>().unwrap().document_id();
            world
                .resource::<editing::SourceDocuments>()
                .and_then(|d| d.get(doc))
                .is_some_and(|d| d.is_composing())
        }
        SurfaceId::Notes => !crate::notes::can_deactivate(world),
        SurfaceId::Code => !crate::code::can_deactivate(world),
        SurfaceId::Create => !crate::create::can_deactivate(world),
        SurfaceId::Reviews => false,
        SurfaceId::Calendar => !crate::calendar::can_deactivate(world),
        SurfaceId::Peers => !crate::peers::can_deactivate(world),
        SurfaceId::Settings => !crate::settings::can_deactivate(world),
    };
    if composing {
        return;
    }
    remember_focus(world);
    let host = world.resource::<SurfaceHost>().unwrap();
    let roots = host.roots;
    let restore = host.focus[target.index()];
    for (index, root) in roots.iter().enumerate() {
        if index == target.index() {
            world.remove::<Hidden>(*root);
        } else {
            world.insert(*root, Hidden);
        }
    }
    world.resource_mut::<SurfaceHost>().unwrap().active = target;
    // Clear focus from the now-hidden editor, but preserve its document, undo,
    // selection and scroll. Do not leave chat shortcuts armed on another screen.
    let focus = restore.filter(|e| {
        world.get::<Layout>(*e).is_some() && descendant(world, *e, roots[target.index()])
    });
    world.resource_mut::<Focus>().unwrap().entity = focus;
    match target {
        SurfaceId::Chat => {}
        SurfaceId::Notes => crate::notes::activate(world),
        SurfaceId::Code => crate::code::activate(world),
        SurfaceId::Create => crate::create::activate(world),
        SurfaceId::Reviews => crate::reviews::activate(world),
        SurfaceId::Calendar => crate::calendar::activate(world),
        SurfaceId::Peers => crate::peers::activate(world),
        SurfaceId::Settings => crate::settings::activate(world),
    }
    frame_wake(world).wake();
}
pub fn action(world: &mut World, intent: &HostIntent) -> bool {
    let target = match intent {
        HostIntent::Command(CHAT) => SurfaceId::Chat,
        HostIntent::Command(NOTES) => SurfaceId::Notes,
        HostIntent::Command(CODE) => SurfaceId::Code,
        HostIntent::Command(CREATE) => SurfaceId::Create,
        HostIntent::Command(REVIEWS) => SurfaceId::Reviews,
        HostIntent::Command(CALENDAR) => SurfaceId::Calendar,
        HostIntent::Command(PEERS) => SurfaceId::Peers,
        HostIntent::Command(SETTINGS) => SurfaceId::Settings,
        _ => return false,
    };
    select(world, target);
    true
}
pub fn chat_width(world: &World) -> f32 {
    let width = world
        .resource::<Viewport>()
        .map(|v| v.width)
        .unwrap_or(1200.);
    if world.resource::<SurfaceHost>().is_some() {
        (width - RAIL_WIDTH).max(0.)
    } else {
        width
    }
}
/// Late app-owned chrome preserves RNDR's keyboard focus and hover treatment.
pub fn chrome(world: &mut World) {
    let Some(host) = world.resource::<SurfaceHost>() else {
        return;
    };
    let active = host.active.index();
    let tiles = host.tiles;
    let buttons = host.buttons;
    let rail = host.rail;
    let icons = host.icons;
    let icon_keys = host.icon_keys;
    let px = (NAV_ICON
        * world
            .resource::<Viewport>()
            .copied()
            .unwrap_or_default()
            .pixels_per_point())
    .ceil()
    .clamp(20., 320.) as u32;
    let hover = world
        .resource::<Input>()
        .and_then(|input| hit_test(world, input.pointer.position[0], input.pointer.position[1]));
    let theme = world.resource::<UiTheme>().cloned().unwrap_or_default();
    let curve = motion::Curve::tween(0.12, motion::Easing::EaseOut);
    crate::native_style::plane(world, rail, crate::native_style::rail());
    for i in 0..SURFACE_COUNT {
        let pressed = interaction_pressed(world, buttons[i].root);
        let hovered = hover.is_some_and(|e| descendant(world, e, buttons[i].root));
        let fill = if pressed && theme.contrast == Contrast::High {
            Token::Text
        } else if pressed {
            Token::ControlPressed
        } else if hovered {
            Token::ControlHover
        } else if i == active {
            Token::Selected
        } else {
            crate::native_style::rail()
        };
        // Style only the tile and inner face/border. The canonical root/gap
        // still owns keyboard focus, release activation and cancellation.
        let face = world.get::<Parent>(buttons[i].label).unwrap().entity;
        let border = world.get::<Parent>(face).unwrap().entity;
        let color = crate::native_style::color(world, fill);
        for entity in [tiles[i], border, face] {
            crate::native_style::plane(world, entity, fill);
            if world.get::<Color>(entity).is_none() {
                world.insert(entity, color);
            }
            motion::animate_color(world, entity, motion::ColorProp::Fill, color, curve);
        }
        let ink = if pressed && theme.contrast == Contrast::High {
            Token::Canvas
        } else if i == active || hovered || pressed {
            crate::native_style::text()
        } else {
            crate::native_style::secondary()
        };
        world.insert(
            buttons[i].label,
            UiStyle {
                foreground: Some(ink.into()),
                ..Default::default()
            },
        );
        // Controls refresh their visible scalar during theme resolution. Clear
        // only its paint text here; the root's name/focus/activation stay native.
        world
            .get_mut::<Label>(buttons[i].label)
            .unwrap()
            .text
            .clear();
        let color = crate::native_style::color(world, ink);
        let rgba =
            [color.r, color.g, color.b, color.a].map(|v| (v.clamp(0., 1.) * 255.).round() as u8);
        if icon_keys[i] != Some((px, rgba)) {
            let raster = nav_raster(i, px, rgba);
            let id = nav_image_id(i, px, rgba);
            rndr_render::load_rgba(world, id, raster.width, raster.height, raster.rgba)
                .expect("valid bundled nav raster");
            world.get_mut::<ImageView>(icons[i]).unwrap().image = id;
            world.resource_mut::<SurfaceHost>().unwrap().icon_keys[i] = Some((px, rgba));
        }
        debug_assert!(world.has::<ImageView>(icons[i]));
        if let Some(semantics) = world.get_mut::<Semantics>(buttons[i].root) {
            semantics.description = if i == active {
                "Current workspace"
            } else {
                "Open workspace; other work stays open"
            }
            .into();
        }
    }
}

// The GPU caches textures by identity, so appearance and DPI variants need
// distinct IDs (re-uploading changed pixels under the same ID stays stale).
fn nav_image_id(index: usize, px: u32, rgba: [u8; 4]) -> u64 {
    0x5355_0000_0000_0000
        | ((index as u64) << 42)
        | ((px as u64) << 32)
        | u32::from_be_bytes(rgba) as u64
}
fn nav_raster(index: usize, px: u32, rgba: [u8; 4]) -> rndr_render::Raster {
    let source = [
        include_str!("nav_icons/message-circle.svg"),
        include_str!("nav_icons/book.svg"),
        include_str!("nav_icons/code.svg"),
        include_str!("nav_icons/pen-tool.svg"),
        include_str!("nav_icons/reviews.svg"),
        include_str!("nav_icons/calendar.svg"),
        include_str!("nav_icons/users.svg"),
        include_str!("nav_icons/settings.svg"),
    ][index];
    let color = format!("#{:02x}{:02x}{:02x}", rgba[0], rgba[1], rgba[2]);
    let svg = source.replace("currentColor", &color).replace(
        "fill=\"none\"",
        &format!("opacity=\"{}\" fill=\"none\"", rgba[3] as f32 / 255.),
    );
    rndr_render::decode_svg(svg.as_bytes(), px, px).expect("valid bundled nav SVG")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> App {
        let mut app = crate::ui::app();
        app.world_mut().insert_resource(Viewport {
            width: 1200.,
            height: 850.,
            ..Default::default()
        });
        app.update();
        app
    }
    fn frames(app: &mut App, clock: &ManualClock, n: usize) {
        for _ in 0..n {
            clock.advance(std::time::Duration::from_millis(16));
            app.update();
        }
    }
    fn pointer(app: &mut App, point: [f32; 2], pressed: bool, released: bool) {
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.pointer.position = point;
        input.pointer.press_at = pressed.then_some(point);
        input.pointer.button = PointerButton::Left;
        input.pointer.pressed = pressed;
        input.pointer.down = pressed;
        input.pointer.released = released;
    }
    #[test]
    fn nav_icons_are_twenty_points_centered_in_equal_targets() {
        let mut app = app();
        for width in [1200., 760.] {
            app.world_mut().insert_resource(Viewport::new(width, 650.));
            for _ in 0..5 {
                app.update();
            }
            let w = app.world();
            let host = w.resource::<SurfaceHost>().unwrap();
            for (i, button) in host.buttons.iter().enumerate() {
                let tile = w.get::<Rect>(host.tiles[i]).unwrap();
                let rect = w.get::<Rect>(button.root).unwrap();
                let label = w.get::<Rect>(button.label).unwrap();
                let style = w.get::<TextStyle>(button.label).unwrap();
                assert_eq!(style.size, NAV_ICON);
                assert_eq!(style.align, TextAlign::Center);
                assert_eq!(rect.w, NAV_TARGET);
                assert_eq!(rect.h, NAV_TARGET);
                assert!((rect.x + rect.w / 2. - RAIL_WIDTH / 2.).abs() < 1.);
                assert!((label.x + label.w / 2. - tile.x - tile.w / 2.).abs() < 1.);
                assert!(
                    (label.y + label.h / 2. - tile.y - tile.h / 2.).abs() < 1.,
                    "{tile:?} {label:?}"
                );
                assert!(w.has::<Focusable>(button.root));
                assert_eq!(
                    w.get::<Semantics>(button.root).unwrap().name.as_deref(),
                    Some(
                        [
                            "Chat", "Notes", "Code", "Create", "Reviews", "Calendar", "Peers",
                            "Settings"
                        ][i]
                    )
                );
                let icon = w.get::<Rect>(host.icons[i]).unwrap();
                assert_eq!((icon.w, icon.h), (NAV_ICON, NAV_ICON));
                assert!((icon.x + icon.w / 2. - tile.x - tile.w / 2.).abs() < 1.);
                assert!((icon.y + icon.h / 2. - tile.y - tile.h / 2.).abs() < 1.);
                assert!(w.get::<Label>(button.label).unwrap().text.is_empty());
                assert_eq!(
                    children_of(w, host.tiles[i]),
                    vec![button.root],
                    "no extra selection marker"
                );
            }
        }
    }
    #[test]
    fn nav_svg_raster_tracks_dpi_and_appearance_without_glyph_upscaling() {
        let mut app = app();
        let mut previous = None;
        for scale in [1., 1.5, 2., 3.] {
            app.world_mut().resource_mut::<Viewport>().unwrap().scale = scale;
            app.update();
            let host = app.world().resource::<SurfaceHost>().unwrap();
            for i in 0..SURFACE_COUNT {
                let (px, rgba) = host.icon_keys[i].unwrap();
                assert_eq!(px, (NAV_ICON * scale).ceil() as u32);
                let raster = nav_raster(i, px, rgba);
                assert_eq!((raster.width, raster.height), (px, px));
                assert!(
                    raster.rgba.chunks_exact(4).any(|p| p[3] > 0 && p[3] < 255),
                    "anti-aliased edges"
                );
                assert_eq!(
                    app.world().get::<ImageView>(host.icons[i]).unwrap().image,
                    nav_image_id(i, px, rgba)
                );
            }
            let id = app.world().get::<ImageView>(host.icons[0]).unwrap().image;
            assert_ne!(previous, Some(id));
            previous = Some(id);
        }
        publish_system_appearance(
            app.world_mut(),
            SystemAppearance {
                dark: Some(true),
                ..Default::default()
            },
        );
        app.update();
        let before = app.world().resource::<SurfaceHost>().unwrap().icon_keys;
        publish_system_appearance(
            app.world_mut(),
            SystemAppearance {
                dark: Some(false),
                ..Default::default()
            },
        );
        app.update();
        assert_ne!(
            before,
            app.world().resource::<SurfaceHost>().unwrap().icon_keys
        );
        let light = app.world().resource::<SurfaceHost>().unwrap().icon_keys;
        app.update();
        assert_eq!(
            light,
            app.world().resource::<SurfaceHost>().unwrap().icon_keys,
            "unchanged frames reuse cached rasters"
        );
    }
    #[test]
    fn nav_release_cancel_hover_transition_and_keyboard_activation() {
        let mut app = app();
        let clock = ManualClock::handle();
        app.set_clock(clock.clone());
        frames(&mut app, &clock, 20);
        let host = app.world().resource::<SurfaceHost>().unwrap();
        let button = host.buttons[1];
        let rect = *app.world().get::<Rect>(button.root).unwrap();
        let point = [rect.x + rect.w / 2., rect.y + rect.h / 2.];
        let face = app.world().get::<Parent>(button.label).unwrap().entity;
        let idle = motion::color_value(app.world(), face, motion::ColorProp::Fill).unwrap();
        pointer(&mut app, point, false, false);
        frames(&mut app, &clock, 1);
        assert!(motion::is_animating(app.world(), face));
        frames(&mut app, &clock, 12);
        let hover = motion::color_value(app.world(), face, motion::ColorProp::Fill).unwrap();
        assert_ne!(idle, hover);
        pointer(&mut app, point, true, false);
        frames(&mut app, &clock, 1);
        assert_eq!(
            active(app.world()),
            SurfaceId::Chat,
            "press must not navigate"
        );
        pointer(&mut app, [500., 400.], false, true);
        frames(&mut app, &clock, 2);
        assert_eq!(
            active(app.world()),
            SurfaceId::Chat,
            "release outside cancels"
        );
        pointer(&mut app, point, true, false);
        frames(&mut app, &clock, 1);
        pointer(&mut app, point, false, true);
        frames(&mut app, &clock, 2);
        assert_eq!(active(app.world()), SurfaceId::Notes);
        pointer(&mut app, [500., 400.], false, false);
        let chat = app.world().resource::<SurfaceHost>().unwrap().buttons[0].root;
        request_focus(app.world_mut(), chat);
        app.world_mut().resource_mut::<Input>().unwrap().key = Some(Key::Enter);
        frames(&mut app, &clock, 1);
        app.world_mut().resource_mut::<Input>().unwrap().key = None;
        frames(&mut app, &clock, 1);
        assert_eq!(active(app.world()), SurfaceId::Chat);
    }
    #[test]
    fn nav_reduced_motion_disables_press_scaling_but_keeps_feedback() {
        let mut app = app();
        let clock = ManualClock::handle();
        app.set_clock(clock.clone());
        publish_system_appearance(
            app.world_mut(),
            SystemAppearance {
                reduced_motion: true,
                ..Default::default()
            },
        );
        frames(&mut app, &clock, 20);
        let button = app.world().resource::<SurfaceHost>().unwrap().buttons[1];
        let rect = *app.world().get::<Rect>(button.root).unwrap();
        let point = [rect.x + rect.w / 2., rect.y + rect.h / 2.];
        pointer(&mut app, point, true, false);
        frames(&mut app, &clock, 12);
        let transform = app
            .world()
            .get::<Transform>(button.root)
            .copied()
            .unwrap_or_default();
        assert_eq!(transform.scale, [1., 1.]);
        assert_eq!(*app.world().get::<Rect>(button.root).unwrap(), rect);
        let face = app.world().get::<Parent>(button.label).unwrap().entity;
        assert_eq!(
            app.world().get::<UiStyle>(face).unwrap().background,
            Some(Token::ControlPressed.into())
        );
        assert_eq!(active(app.world()), SurfaceId::Chat);
        pointer(&mut app, point, false, true);
        frames(&mut app, &clock, 2);
        assert_eq!(active(app.world()), SurfaceId::Notes);
    }
    #[test]
    #[ignore = "Navigation rail interaction GPU acceptance"]
    fn nav_polish_visual_acceptance() {
        use std::sync::Arc;
        let mut app = app();
        let clock = ManualClock::handle();
        app.set_clock(clock.clone());
        app.world_mut()
            .insert_resource(Arc::new(rndr_render::ParleyText::new()) as Arc<dyn TextEngine>);
        let mut exporter = rndr_render::Exporter::new().unwrap();
        for (width, height, scale, dark, suffix) in [
            (1200, 850, 1., true, "wide"),
            (760, 650, 1., true, "narrow"),
            (1200, 850, 1., false, "light"),
            (760, 650, 2., true, "retina"),
            (760, 650, 1.5, false, "fractional"),
        ] {
            publish_system_appearance(
                app.world_mut(),
                SystemAppearance {
                    dark: Some(dark),
                    ..Default::default()
                },
            );
            app.world_mut().insert_resource(Viewport {
                width: width as f32,
                height: height as f32,
                scale,
                ..Default::default()
            });
            frames(&mut app, &clock, 20);
            let host = app.world().resource::<SurfaceHost>().unwrap();
            let rect = *app.world().get::<Rect>(host.buttons[1].root).unwrap();
            let point = [rect.x + rect.w / 2., rect.y + rect.h / 2.];
            for state in ["rest", "hover", "press", "selected"] {
                match state {
                    "rest" => pointer(&mut app, [500., 400.], false, false),
                    "hover" => pointer(&mut app, point, false, false),
                    "press" => pointer(&mut app, point, true, false),
                    _ => pointer(&mut app, point, false, true),
                }
                frames(&mut app, &clock, 20);
                let (_, image) = rndr_paint::frame_with_snapshot_frame(&mut app, |world, frame| {
                    exporter
                        .render_frame(
                            Some(world),
                            frame,
                            (width as f32 * scale) as u32,
                            (height as f32 * scale) as u32,
                            scale,
                        )
                        .unwrap()
                });
                image::save_buffer(
                    format!("verification/native-nav-{suffix}-{state}.png"),
                    &image.rgba,
                    image.width,
                    image.height,
                    image::ColorType::Rgba8,
                )
                .unwrap();
            }
            select(app.world_mut(), SurfaceId::Chat);
        }
    }
    #[test]
    fn surface_switch_retains_chat_document_and_clears_hidden_focus() {
        let mut app = app();
        let doc = app
            .world()
            .resource::<crate::ui::Model>()
            .unwrap()
            .document_id();
        let d = app
            .world_mut()
            .resource_mut::<editing::SourceDocuments>()
            .unwrap()
            .get_mut(doc)
            .unwrap();
        d.reload(d.revision(), "unsent chat draft").unwrap();
        let roots = app.world().resource::<SurfaceHost>().unwrap().roots;
        let entity = panel(app.world_mut(), roots[0], Layout::column());
        app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(entity);
        select(app.world_mut(), SurfaceId::Notes);
        assert_eq!(active(app.world()), SurfaceId::Notes);
        assert!(app.world().get::<Hidden>(roots[0]).is_some());
        assert!(app.world().resource::<Focus>().unwrap().entity.is_none());
        for _ in 0..4 {
            app.update();
        }
        select(app.world_mut(), SurfaceId::Chat);
        assert_eq!(
            app.world().resource::<Focus>().unwrap().entity,
            Some(entity)
        );
        assert_eq!(
            app.world()
                .resource::<editing::SourceDocuments>()
                .unwrap()
                .get(doc)
                .unwrap()
                .text()
                .to_string(),
            "unsent chat draft"
        );
        assert_eq!(
            app.world()
                .resource::<crate::ui::Model>()
                .unwrap()
                .document_id(),
            doc
        );
    }
    #[test]
    fn three_way_switch_has_one_visible_root_and_restores_each_surface_focus() {
        let mut app = app();
        let roots = app.world().resource::<SurfaceHost>().unwrap().roots;
        let chat_focus = panel(app.world_mut(), roots[0], Layout::column());
        let notes_focus = panel(app.world_mut(), roots[1], Layout::column());
        let code_focus = panel(app.world_mut(), roots[2], Layout::column());
        app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(chat_focus);
        select(app.world_mut(), SurfaceId::Notes);
        app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(notes_focus);
        select(app.world_mut(), SurfaceId::Code);
        app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(code_focus);
        for (target, expected_focus) in [
            (SurfaceId::Chat, chat_focus),
            (SurfaceId::Notes, notes_focus),
            (SurfaceId::Code, code_focus),
            (SurfaceId::Notes, notes_focus),
            (SurfaceId::Chat, chat_focus),
        ] {
            select(app.world_mut(), target);
            assert_eq!(
                app.world().resource::<Focus>().unwrap().entity,
                Some(expected_focus)
            );
            for (index, root) in roots.iter().enumerate() {
                assert_eq!(app.world().has::<Hidden>(*root), index != target.index());
            }
            app.update();
            assert_eq!(active(app.world()), target);
        }
    }
    #[test]
    fn code_fetch_is_lazy_and_returning_does_not_restart_it() {
        let mut app = app();
        assert!(crate::code::take_commands(app.world_mut()).is_empty());
        select(app.world_mut(), SurfaceId::Code);
        assert!(!crate::code::take_commands(app.world_mut()).is_empty());
        select(app.world_mut(), SurfaceId::Notes);
        select(app.world_mut(), SurfaceId::Code);
        assert!(crate::code::take_commands(app.world_mut()).is_empty());
    }
    #[test]
    fn rail_is_fixed_width_and_does_not_fetch_notes_until_selected() {
        let mut app = app();
        assert!(crate::notes::drain_commands(app.world_mut()).is_empty());
        assert_eq!(chat_width(app.world()), 1144.);
        select(app.world_mut(), SurfaceId::Notes);
        let commands = crate::notes::drain_commands(app.world_mut());
        assert!(!commands.is_empty());
        select(app.world_mut(), SurfaceId::Chat);
        select(app.world_mut(), SurfaceId::Notes);
        assert!(crate::notes::drain_commands(app.world_mut()).is_empty());
    }
}
