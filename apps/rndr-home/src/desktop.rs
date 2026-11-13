//! Shared rail/chrome and view lifecycle. Chat remains mounted, draft/scroll intact.
use crate::admin_backend::{ReadReply, ReadRequest};
use crate::admin_model::{Desktop, ReadError, ReadState, Resource, Route, SETTINGS};
use crate::presentation::{button, label};
use crate::shell::{muted, row};
use rndr_app::{child, column};
use rndr_core::*;
use rndr_ui::{Token, Typography};
use tokio::sync::mpsc;

#[derive(Default)]
pub struct Connection {
    pub sender: Option<mpsc::UnboundedSender<ReadRequest>>,
}
#[derive(Clone)]
pub struct Handles {
    pub chat: Entity,
    pub page: Entity,
    pub title: Entity,
    pub subtitle: Entity,
    pub rail: Entity,
    pub section_nav: Entity,
    pub body: Entity,
    pub search: Entity,
    pub footer: Entity,
    pub nav: Vec<(Entity, &'static str)>,
}
pub const DOORS: [(&str, &str); 18] = [
    ("chat", "Chat"),
    ("code", "Code"),
    ("reviews", "Reviews"),
    ("notes", "Notes"),
    ("files", "Files"),
    ("artifacts", "Artifacts"),
    ("calendar", "Calendar"),
    ("web", "Web"),
    ("map", "Context"),
    ("peers", "Peers"),
    ("workshop", "Agents"),
    ("history", "Activity"),
    ("scripts", "Scripts"),
    ("flows", "Flows"),
    ("schedules", "Schedules"),
    ("create", "Create"),
    ("settings", "Settings"),
    ("profiles", "You"),
];
pub fn rail_width(world: &World) -> f32 {
    if world.resource::<Viewport>().is_some_and(|v| v.width < 800.) {
        58.
    } else {
        132.
    }
}
pub fn mount(world: &mut World, chat: Entity) {
    let root = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        gap: 0.,
        ..Layout::row()
    },));
    crate::presentation::surface(world, root, Token::Canvas, None);
    let rail = world.spawn((
        Layout {
            width: Size::Px(132.),
            height: Size::Fill,
            padding: 6.,
            gap: 3.,
            ..Layout::column()
        },
        Scroll::default(),
    ));
    crate::presentation::surface(world, rail, Token::Raised, None);
    child(world, root, rail);
    let brand = label(world, rail, "M", true);
    world.get_mut::<TextStyle>(brand).unwrap().size = 24.;
    let mut nav = Vec::new();
    for (id, name) in DOORS {
        let e = nav_button(world, rail, &format!("desk:{id}"), name);
        world.insert(
            e,
            Semantics {
                name: Some(format!("Open {name}")),
                ..Default::default()
            },
        );
        nav.push((e, name));
    }
    let main = world.spawn((Layout {
        gap: 0.,
        ..column(Size::Fill)
    },));
    child(world, root, main);
    let top = row(world, main, Size::Px(42.));
    let title = label(world, top, "Chat", true);
    world.get_mut::<TextStyle>(title).unwrap().size = 15.;
    let subtitle = muted(world, top, "Medousa · desktop preview");
    child(world, main, chat);
    let page = world.spawn((
        Layout {
            padding: 14.,
            gap: 10.,
            ..column(Size::Fill)
        },
        Hidden,
    ));
    child(world, main, page);
    let section_nav = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Hug,
        gap: 5.,
        ..Layout::column()
    },));
    child(world, page, section_nav);
    let content = world.spawn((Layout {
        gap: 10.,
        ..column(Size::Fill)
    },));
    child(world, page, content);
    let search = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(36.),
            padding: 8.,
            ..Layout::column()
        },
        Label::new(""),
        TextField,
        Focusable,
        Placeholder::new("Filter this read-only list…"),
        crate::presentation::theme(world).typography(Typography::Label),
    ));
    world.insert(
        search,
        Semantics {
            name: Some("Filter administrative list (window only)".into()),
            ..Default::default()
        },
    );
    crate::presentation::surface(world, search, Token::Raised, None);
    child(world, content, search);
    let body = world.spawn((
        Layout {
            gap: 12.,
            ..column(Size::Fill)
        },
        Scroll::default(),
    ));
    child(world, content, body);
    let footer = muted(world, main, "Preview · no native-input qualification");
    world.get_mut::<TextStyle>(footer).unwrap().size = 11.;
    world.insert_resource(Handles {
        chat,
        page,
        title,
        subtitle,
        rail,
        section_nav,
        body,
        search,
        footer,
        nav,
    });
}
pub fn nav_button(world: &mut World, parent: Entity, action: &str, text: &str) -> Entity {
    let e = button(world, parent, action, text);
    let l = world.get_mut::<Layout>(e).unwrap();
    l.width = Size::Fill;
    l.height = Size::Px(33.);
    l.min_height = 0.;
    world.get_mut::<TextStyle>(e).unwrap().size = 12.;
    e
}
fn clear(world: &mut World, parent: Entity) {
    if let Some(children) = world.get::<Children>(parent).cloned() {
        for entity in children.list {
            despawn(world, entity);
        }
    }
}
pub fn navigate(world: &mut World, route: Route) {
    crate::menus::close(world);
    world.resource_mut::<Desktop>().unwrap().navigate(route);
    let h = world.resource::<Handles>().unwrap().clone();
    crate::composer::reset_prompt(world, h.search, "");
    // Never return focus to a hidden editor or preserve a soon-to-die detail row.
    request_focus(
        world,
        if route.resource().is_some() {
            h.search
        } else {
            h.nav
                .iter()
                .find(|(_, name)| *name == "Settings")
                .unwrap()
                .0
        },
    );
    if let Some(resource) = route.resource()
        && !world
            .resource::<Desktop>()
            .unwrap()
            .reads
            .contains_key(&resource)
    {
        refresh(world, resource);
    }
}
pub fn refresh(world: &mut World, resource: Resource) {
    let sender = world.resource::<Connection>().unwrap().sender.clone();
    let Some(sender) = sender else {
        return;
    };
    let Some(serial) = world.resource_mut::<Desktop>().unwrap().begin(resource) else {
        return;
    };
    if sender.send(ReadRequest { resource, serial }).is_err() {
        world.resource_mut::<Connection>().unwrap().sender = None;
        world.resource_mut::<Desktop>().unwrap().settle(
            resource,
            serial,
            Err(ReadError::Disconnected),
        );
    }
}
/// Consumed before the chat controller. No native input gate for harmless view changes.
pub fn action(world: &mut World, action: &str) -> bool {
    if let Some(id) = action.strip_prefix("desk:") {
        let route = match id {
            "chat" => Route::Chat,
            "settings" => Route::Settings(0),
            "runtime" => Route::Data(Resource::Health),
            "schedules" => Route::Data(Resource::Schedules),
            _ => match DOORS.iter().find(|(key, _)| *key == id) {
                Some((_, name)) => Route::Deferred(name),
                None => return true,
            },
        };
        navigate(world, route);
        return true;
    }
    if let Some(index) = action
        .strip_prefix("settings:")
        .and_then(|s| s.parse::<usize>().ok())
    {
        if index < SETTINGS.len() {
            navigate(world, Route::Settings(index));
        }
        return true;
    }
    if let Some(index) = action
        .strip_prefix("data:")
        .and_then(|s| s.parse::<usize>().ok())
    {
        if let Some(resource) = Resource::ALL.get(index) {
            navigate(world, Route::Data(*resource));
        }
        return true;
    }
    match action {
        "admin:refresh" => {
            if let Some(r) = world.resource::<Desktop>().unwrap().route.resource() {
                refresh(world, r);
            }
        }
        "admin:back" => {
            let d = world.resource_mut::<Desktop>().unwrap();
            d.selected = None;
            d.dirty = true;
        }
        _ if action.starts_with("record:") => {
            if let Ok(index) = action[7..].parse::<usize>() {
                let d = world.resource_mut::<Desktop>().unwrap();
                if d.route.resource().and_then(|r| d.reads.get(&r)).is_some_and(|s|
                    matches!(s, ReadState::Ready(rows) if rows.get(index).is_some_and(|r| r.matches(&d.query)))) {
                    d.selected = Some(index); d.dirty = true;
                }
            }
        }
        _ => return false,
    }
    true
}
pub fn sync(world: &mut World) {
    for reply in events::<ReadReply>(world).to_vec() {
        world
            .resource_mut::<Desktop>()
            .unwrap()
            .settle(reply.resource, reply.serial, reply.result);
    }
    let h = world.resource::<Handles>().unwrap().clone();
    let narrow = rail_width(world) < 100.;
    if narrow {
        world.insert(h.subtitle, Hidden);
    } else {
        world.remove::<Hidden>(h.subtitle);
    }
    let query = world.get::<Label>(h.search).unwrap().text.clone();
    let d = world.resource_mut::<Desktop>().unwrap();
    if d.query != query {
        d.query = query;
        d.selected = None;
        d.dirty = true;
    }
    if d.narrow != narrow {
        d.narrow = narrow;
        d.dirty = true;
    }
    let route = d.route;
    let dirty = std::mem::take(&mut d.dirty);
    world.get_mut::<Layout>(h.rail).unwrap().width = Size::Px(rail_width(world));
    for (e, name) in &h.nav {
        let active = match route {
            Route::Chat => *name == "Chat",
            Route::Settings(_) => *name == "Settings",
            Route::Data(Resource::Schedules) => *name == "Schedules",
            Route::Data(_) => *name == "Settings",
            Route::Deferred(n) => *name == n,
        };
        world.get_mut::<Label>(*e).unwrap().text = (*name).into();
        world.get_mut::<TextStyle>(*e).unwrap().size = if narrow { 8. } else { 12. };
        world.get_mut::<Layout>(*e).unwrap().padding = if narrow { 2. } else { 8. };
        world.get_mut::<rndr_ui::UiStyle>(*e).unwrap().background =
            Some(if active { Token::Canvas } else { Token::Raised }.into());
    }
    let synthetic = world
        .resource::<crate::desktop_fixtures::SyntheticDesktop>()
        .is_some();
    world.get_mut::<Label>(h.title).unwrap().text = if synthetic {
        format!("FIXTURE · {}", route.title())
    } else {
        route.title().into()
    };
    if route == Route::Chat {
        world.remove::<Hidden>(h.chat);
        world.insert(h.page, Hidden);
    } else {
        world.insert(h.chat, Hidden);
        world.remove::<Hidden>(h.page);
    }
    if route.resource().is_some() {
        world.remove::<Hidden>(h.search);
    } else {
        world.insert(h.search, Hidden);
    }
    let connected = world.resource::<Connection>().unwrap().sender.is_some();
    world.get_mut::<Label>(h.footer).unwrap().text = if connected {
        "Read transport configured · per-view results below · no credential/config writes"
    } else {
        "Offline preview · no network · configure using existing Medousa app"
    }
    .into();
    if synthetic {
        world.get_mut::<Label>(h.footer).unwrap().text =
            "SYNTHETIC GPU RENDER · NO NETWORK · NOT NATIVE WINDOW / INPUT PROOF".into();
    }
    if !dirty || route == Route::Chat {
        return;
    }
    // Release focus before deleting dynamic body entities. Search/rail are stable.
    if world
        .resource::<Focus>()
        .and_then(|f| f.entity)
        .is_some_and(|e| {
            world.get::<Button>(e).is_some_and(|b| {
                b.action.starts_with("record:")
                    || b.action == "admin:back"
                    || b.action == "admin:refresh"
            })
        })
    {
        request_focus(world, h.search);
    }
    world.insert(
        h.page,
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            padding: 14.,
            gap: 14.,
            ..if narrow {
                Layout::column()
            } else {
                Layout::row()
            }
        },
    );
    world.insert(
        h.section_nav,
        Layout {
            width: if narrow { Size::Fill } else { Size::Px(202.) },
            height: if narrow { Size::Hug } else { Size::Fill },
            gap: 5.,
            ..Layout::column()
        },
    );
    if narrow {
        world.remove::<Scroll>(h.section_nav);
    } else {
        world.insert(h.section_nav, Scroll::default());
    }
    clear(world, h.section_nav);
    clear(world, h.body);
    world.get_mut::<Scroll>(h.body).unwrap().offset = [0., 0.];
    crate::admin_views::render(world, &h, route, narrow);
}
