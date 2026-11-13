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
    pub chooser: Entity,
    pub rail: Entity,
    pub section_nav: Entity,
    #[cfg(test)]
    pub primary_rail: Entity,
    #[cfg(test)]
    pub dock: Entity,
    pub body: Entity,
    pub search: Entity,
    pub footer: Entity,
    /// Offset retained across the temporary Loading placeholder, never across navigation.
    pub loading_offset: Option<[f32; 2]>,
    pub list_offset: Option<[f32; 2]>,
    pub next_focus: Option<String>,
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
        76.
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
    let rail = world.spawn((Layout {
        width: Size::Px(132.),
        height: Size::Fill,
        padding: 6.,
        gap: 3.,
        ..Layout::column()
    },));
    crate::presentation::surface(world, rail, Token::Raised, None);
    child(world, root, rail);
    let brand = label(world, rail, "M", true);
    world.get_mut::<TextStyle>(brand).unwrap().size = 24.;
    // Primary doors scroll independently; account/settings/status stay reachable.
    // Production folds automations and opens Activity from Spotlight. These are
    // explicit preview shortcuts, not a claim to reproduce native pane chrome.
    let primary_rail = world.spawn((column(Size::Fill), Scroll::default()));
    child(world, rail, primary_rail);
    let dock = world.spawn((column(Size::Hug),));
    child(world, rail, dock);
    let mut nav = Vec::new();
    for (id, name) in DOORS {
        let parent = if matches!(id, "history" | "settings" | "profiles") {
            dock
        } else {
            primary_rail
        };
        if id == "calendar" || id == "scripts" {
            let heading = label(
                world,
                parent,
                if id == "calendar" {
                    "Focus"
                } else {
                    "Automations"
                },
                false,
            );
            world.get_mut::<TextStyle>(heading).unwrap().size = 10.;
        }
        let e = nav_button(world, parent, &format!("desk:{id}"), name);
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
    let chooser = nav_button(world, top, "settings:choose", "Choose settings view ▾");
    world.get_mut::<Layout>(chooser).unwrap().width = Size::Px(172.);
    world.insert(chooser, Hidden);
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
        chooser,
        rail,
        section_nav,
        #[cfg(test)]
        primary_rail,
        #[cfg(test)]
        dock,
        body,
        search,
        footer,
        nav,
        loading_offset: None,
        list_offset: None,
        next_focus: None,
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
    world.resource_mut::<Handles>().unwrap().list_offset = None;
    world.resource_mut::<Handles>().unwrap().next_focus = None;
    if matches!(route, Route::Settings(_)) {
        world.remove::<Hidden>(h.chooser);
    }
    // Route focus must not land on a hidden editor or an unrelated Settings door.
    request_focus(
        world,
        if matches!(route, Route::Settings(_)) {
            h.chooser
        } else {
            rail_target(&h, route)
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
    if action == "settings:choose" {
        if matches!(
            world.resource::<Desktop>().unwrap().route,
            Route::Settings(_)
        ) {
            let anchor = world.resource::<Handles>().unwrap().chooser;
            crate::menus::open(world, crate::menus::Kind::Settings, anchor);
        }
        return true;
    }
    if let Some(id) = action.strip_prefix("desk:") {
        let route = match id {
            "chat" => Route::Chat,
            "settings" => Route::Settings(world.resource::<Desktop>().unwrap().last_settings),
            "history" => Route::Data(Resource::Activity),
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
            let selected = d.selected.take();
            d.reset_scroll = false;
            d.dirty = true;
            let h = world.resource_mut::<Handles>().unwrap();
            h.loading_offset = h.list_offset.take();
            h.next_focus = selected.map(|i| format!("record:{i}"));
        }
        _ if action.starts_with("record:") => {
            if let Ok(index) = action[7..].parse::<usize>() {
                let d = world.resource_mut::<Desktop>().unwrap();
                if d.route.resource().and_then(|r| d.reads.get(&r)).is_some_and(|s|
                    matches!(s, ReadState::Ready(rows) if rows.get(index).is_some_and(|r| r.matches(&d.query)))) {
                    d.selected = Some(index); d.reset_scroll = true; d.dirty = true;
                    let h = world.resource::<Handles>().unwrap().clone();
                    let offset = world.get::<Scroll>(h.body).unwrap().offset;
                    let h = world.resource_mut::<Handles>().unwrap();
                    h.list_offset = Some(offset);
                    h.next_focus = Some("admin:back".into());
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
        d.reset_scroll = true;
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
    let reset_scroll = std::mem::take(&mut d.reset_scroll);
    let loading = route
        .resource()
        .and_then(|r| d.reads.get(&r))
        .is_some_and(|s| matches!(s, ReadState::Loading(_)));
    world.get_mut::<Layout>(h.rail).unwrap().width = Size::Px(rail_width(world));
    for (e, name) in &h.nav {
        let active = match route {
            Route::Chat => *name == "Chat",
            Route::Settings(_) => *name == "Settings",
            Route::Data(Resource::Schedules) => *name == "Schedules",
            Route::Data(Resource::Activity) => *name == "Activity",
            Route::Data(_) => *name == "Settings",
            Route::Deferred(n) => *name == n,
        };
        world.get_mut::<Label>(*e).unwrap().text = (*name).into();
        world.insert(
            *e,
            Semantics {
                name: Some(format!(
                    "Open {name}{}",
                    if active { ", current destination" } else { "" }
                )),
                ..Default::default()
            },
        );
        world.get_mut::<TextStyle>(*e).unwrap().size = if narrow { 10. } else { 12. };
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
    if matches!(route, Route::Settings(_)) {
        world.remove::<Hidden>(h.chooser);
        world.insert(
            h.chooser,
            Semantics {
                name: Some(format!("Choose settings view, current: {}", route.title())),
                ..Default::default()
            },
        );
        // Reserve the title and chooser; no clipped subtitle at either width.
        world.insert(h.subtitle, Hidden);
    } else {
        world.insert(h.chooser, Hidden);
    }
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
    // Dynamic controls are keyed by action *and region*. Labels/row indices alone
    // must not move focus to a different semantic control. Dataset refresh clears
    // detail selection, so a removed record gets the visible search fallback.
    let focus = world.resource::<Focus>().and_then(|f| f.entity);
    let dynamic_focus = focus.and_then(|e| {
        let region = if descendant(world, e, h.section_nav) {
            h.section_nav
        } else if descendant(world, e, h.body) {
            h.body
        } else {
            return None;
        };
        Some((region, world.get::<Button>(e).map(|b| b.action.clone())))
    });
    if dynamic_focus.is_some() {
        request_focus(world, fallback(&h, route));
    }
    let offset = world.get::<Scroll>(h.body).unwrap().offset;
    let retained = if reset_scroll { None } else { h.loading_offset };
    let next_offset = if reset_scroll {
        [0., 0.]
    } else {
        retained.unwrap_or(offset)
    };
    world.resource_mut::<Handles>().unwrap().loading_offset =
        if loading { Some(next_offset) } else { None };
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
    } else if !world.has::<Scroll>(h.section_nav) {
        world.insert(h.section_nav, Scroll::default());
    }
    clear(world, h.section_nav);
    clear(world, h.body);
    world.get_mut::<Scroll>(h.body).unwrap().offset = next_offset;
    // Layout clamps to the newly measured content, including shorter/empty/error
    // results. Keeping the Loading offset separately avoids losing it to that
    // short placeholder; explicit navigation/filter/detail always clears it.
    crate::admin_views::render(world, &h, route, narrow);
    let next_focus = world.resource_mut::<Handles>().unwrap().next_focus.take();
    if let Some(action) = next_focus {
        let target = world.entities_with::<Button>().into_iter().find(|e| {
            descendant(world, *e, h.body)
                && world.get::<Button>(*e).is_some_and(|b| b.action == action)
        });
        request_focus(world, target.unwrap_or_else(|| fallback(&h, route)));
    } else if let Some((region, action)) = dynamic_focus {
        let replacement = action.and_then(|action| {
            world.entities_with::<Button>().into_iter().find(|e| {
                descendant(world, *e, region)
                    && world
                        .get::<Button>(*e)
                        .is_some_and(|b| b.action == action && b.enabled)
            })
        });
        let target = replacement.unwrap_or_else(|| fallback(&h, route));
        if !request_focus(world, target) {
            request_focus(world, fallback(&h, route));
        }
    }
}

/// Hierarchy membership, not a global action-name match (some links repeat).
fn descendant(world: &World, mut entity: Entity, root: Entity) -> bool {
    while let Some(parent) = world.get::<Parent>(entity) {
        if parent.entity == root {
            return true;
        }
        entity = parent.entity;
    }
    false
}
fn rail_target(h: &Handles, route: Route) -> Entity {
    let name = match route {
        Route::Chat => "Chat",
        Route::Settings(_) => "Settings",
        Route::Data(Resource::Schedules) => "Schedules",
        Route::Data(Resource::Activity) => "Activity",
        Route::Data(_) => "Settings",
        Route::Deferred(name) => name,
    };
    h.nav.iter().find(|(_, n)| *n == name).unwrap().0
}
fn fallback(h: &Handles, route: Route) -> Entity {
    if route.resource().is_some() {
        h.search
    } else {
        rail_target(h, route)
    }
}
