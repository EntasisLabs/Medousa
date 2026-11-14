//! App-level navigation. Each destination retains its own root and state; a
//! conversation tab is never repurposed as a non-chat screen.
use rndr_core::{editing, *};
use rndr_ui::{
    controls::{self, ButtonProps, HostIntent},
    inputs::Tooltip,
    *,
};

pub const RAIL_WIDTH: f32 = 56.;
const CHAT: u64 = 0x5355_0001;
const NOTES: u64 = 0x5355_0002;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceId {
    Chat,
    Notes,
}
impl SurfaceId {
    fn index(self) -> usize {
        match self {
            Self::Chat => 0,
            Self::Notes => 1,
        }
    }
}
struct SurfaceHost {
    active: SurfaceId,
    roots: [Entity; 2],
    rail: Entity,
    tiles: [Entity; 2],
    buttons: [controls::ControlHandle; 2],
    focus: [Option<Entity>; 2],
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
    world.insert(rail, Color::rgb(0.035, 0.035, 0.042));
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
    for (command, icon, name) in [
        (CHAT, Icon::MessageCircle, "Chat"),
        (NOTES, Icon::Book, "Notes"),
    ] {
        let tile = panel(
            world,
            rail,
            Layout {
                width: Size::Px(40.),
                height: Size::Px(40.),
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
            ..Layout::column()
        };
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
    let roots = ["Chat", "Notes"].map(|name| {
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
    world.insert(roots[1], Hidden);
    world.insert_resource(SurfaceHost {
        active: SurfaceId::Chat,
        roots,
        rail,
        tiles: [tiles[0], tiles[1]],
        buttons: [buttons[0], buttons[1]],
        focus: [None, None],
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
    };
    if composing {
        return;
    }
    remember_focus(world);
    let host = world.resource::<SurfaceHost>().unwrap();
    let roots = host.roots;
    let restore = host.focus[target.index()];
    world.insert(roots[1 - target.index()], Hidden);
    world.remove::<Hidden>(roots[target.index()]);
    world.resource_mut::<SurfaceHost>().unwrap().active = target;
    // Clear focus from the now-hidden editor, but preserve its document, undo,
    // selection and scroll. Do not leave chat shortcuts armed on another screen.
    let focus = restore.filter(|e| {
        world.get::<Layout>(*e).is_some() && descendant(world, *e, roots[target.index()])
    });
    world.resource_mut::<Focus>().unwrap().entity = focus;
    if target == SurfaceId::Notes {
        crate::notes::activate(world);
    }
    frame_wake(world).wake();
}
pub fn action(world: &mut World, intent: &HostIntent) -> bool {
    let target = match intent {
        HostIntent::Command(CHAT) => SurfaceId::Chat,
        HostIntent::Command(NOTES) => SurfaceId::Notes,
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
    world.insert(rail, Color::rgb(0.035, 0.035, 0.042));
    for i in 0..2 {
        world.insert(
            tiles[i],
            if i == active {
                Color::rgb(0.13, 0.13, 0.15)
            } else {
                Color::rgb(0.035, 0.035, 0.042)
            },
        );
        world.insert(
            buttons[i].label,
            UiStyle {
                foreground: Some(UiColor::Explicit(if i == active {
                    Color::rgb(0.95, 0.94, 0.91)
                } else {
                    Color::rgb(0.56, 0.56, 0.60)
                })),
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
