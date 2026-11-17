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
const SURFACE_COUNT: usize = 3;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceId {
    Chat,
    Notes,
    Code,
}
impl SurfaceId {
    fn index(self) -> usize {
        match self {
            Self::Chat => 0,
            Self::Notes => 1,
            Self::Code => 2,
        }
    }
}
struct SurfaceHost {
    active: SurfaceId,
    roots: [Entity; SURFACE_COUNT],
    rail: Entity,
    tiles: [Entity; SURFACE_COUNT],
    buttons: [controls::ControlHandle; SURFACE_COUNT],
    indicators: [Entity; SURFACE_COUNT],
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
    let mut indicators = Vec::new();
    for (command, icon, name) in [
        (CHAT, Icon::MessageCircle, "Chat"),
        (NOTES, Icon::Book, "Notes"),
        (CODE, Icon::Code, "Code"),
    ] {
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
        let indicator = world.spawn((
            Layout {
                width: Size::Px(3.),
                height: Size::Px(16.),
                ..Layout::column()
            },
            Absolute {
                left: Some(0.),
                top: Some(12.),
                ..Default::default()
            },
            Radius(1.5),
            ResolvedColor,
        ));
        set_parent(world, indicator, tile);
        crate::native_style::plane(world, indicator, Token::Accent);
        indicators.push(indicator);
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
    let roots = ["Chat", "Notes", "Code"].map(|name| {
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
    for root in &roots[1..] {
        world.insert(*root, Hidden);
    }
    world.insert_resource(SurfaceHost {
        active: SurfaceId::Chat,
        roots,
        rail,
        tiles: tiles.try_into().ok().expect("one tile per surface"),
        buttons: buttons.try_into().ok().expect("one button per surface"),
        indicators: indicators.try_into().ok().expect("one marker per surface"),
        focus: [None; SURFACE_COUNT],
    });
    roots[0]
}
pub fn active(world: &World) -> SurfaceId {
    world
        .resource::<SurfaceHost>()
        .map(|h| h.active)
        .unwrap_or(SurfaceId::Chat)
}
fn descendant(world: &World, mut entity: Entity, root: Entity) -> bool {
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
    }
    frame_wake(world).wake();
}
pub fn action(world: &mut World, intent: &HostIntent) -> bool {
    let target = match intent {
        HostIntent::Command(CHAT) => SurfaceId::Chat,
        HostIntent::Command(NOTES) => SurfaceId::Notes,
        HostIntent::Command(CODE) => SurfaceId::Code,
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
    let indicators = host.indicators;
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
        // Keep a non-color selection cue; hover and keyboard focus are separate.
        if i == active {
            world.remove::<Hidden>(indicators[i]);
        } else {
            world.insert(indicators[i], Hidden);
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
                    Some(["Chat", "Notes", "Code"][i])
                );
                assert_eq!(
                    w.has::<Hidden>(host.indicators[i]),
                    i != host.active.index()
                );
            }
        }
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
        for (width, height, dark, suffix) in [
            (1200, 850, true, "wide"),
            (760, 650, true, "narrow"),
            (1200, 850, false, "light"),
        ] {
            publish_system_appearance(
                app.world_mut(),
                SystemAppearance {
                    dark: Some(dark),
                    ..Default::default()
                },
            );
            app.world_mut()
                .insert_resource(Viewport::new(width as f32, height as f32));
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
                        .render_frame(Some(world), frame, width, height, 1.)
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
