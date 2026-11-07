//! App-owned menus built on rndr's canonical popup/focus/dismissal scope.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Model,
    Runtime,
    Mode,
    Behavior,
    Depth,
}
#[derive(Default)]
pub struct Menus {
    pub root: Option<Entity>,
    kind: Option<Kind>,
    search: Option<Entity>,
    body: Option<Entity>,
    query: String,
}

fn closed(world: &mut World, _: Entity) {
    // The renderer returns focus to the live trigger. Retain the hidden scope
    // until the next opening so no current input dispatch uses dead entities.
    if let Some(m) = world.resource_mut::<Menus>() {
        m.search = None;
    }
}
pub fn close(world: &mut World) {
    if let Some(root) = world.resource::<Menus>().and_then(|m| m.root) {
        close_popup(world, root, true);
    }
}
fn item(world: &mut World, parent: Entity, action: &str, text: &str, enabled: bool) -> Entity {
    let e = button(world, parent, action, text);
    world.get_mut::<Layout>(e).unwrap().width = Size::Fill;
    world.get_mut::<Layout>(e).unwrap().height = Size::Px(36.);
    world.get_mut::<Layout>(e).unwrap().min_height = 0.;
    world.get_mut::<TextStyle>(e).unwrap().size = 13.;
    world.insert(e, Color::rgba(0., 0., 0., 0.));
    world.get_mut::<Button>(e).unwrap().enabled = enabled;
    e
}
pub fn open(world: &mut World, kind: Kind, anchor: Entity) {
    if let Some(old) = world.resource::<Menus>().and_then(|m| m.root) {
        let same = world
            .get::<AnchoredPopup>(old)
            .is_some_and(|p| p.anchor == anchor)
            && !world.has::<Hidden>(old)
            && world.resource::<Menus>().unwrap().kind == Some(kind);
        close(world);
        despawn(world, old);
        world.resource_mut::<Menus>().unwrap().root = None;
        if same {
            return;
        }
    }
    let root = world.spawn((
        Layout {
            width: Size::Px(340.),
            height: Size::Hug,
            padding: 10.,
            gap: 5.,
            ..Layout::column()
        },
        theme_of(world).pane,
        Radius(12.),
        Border {
            color: Color::rgb(0.25, 0.25, 0.27),
            width: 1.,
        },
        Scroll::default(),
        AnchoredPopup {
            anchor,
            max_height: 380.,
            closed,
        },
    ));
    world.insert(root, Hidden);
    let header = row(world, root, Size::Px(30.));
    let title = label(
        world,
        header,
        match kind {
            Kind::Model => "Choose model",
            Kind::Runtime => "Agent runtime",
            Kind::Mode => "Agent mode",
            Kind::Behavior => "Behavior",
            Kind::Depth => "Response depth",
        },
        true,
    );
    world.get_mut::<TextStyle>(title).unwrap().size = 14.;
    let x = item(world, header, "menu:close", "×", true);
    world.get_mut::<Layout>(x).unwrap().width = Size::Px(32.);
    world.get_mut::<Layout>(x).unwrap().height = Size::Px(28.);
    let mut search = None;
    if kind == Kind::Model {
        let e = world.spawn((
            Layout {
                width: Size::Fill,
                height: Size::Px(34.),
                padding: 6.,
                ..Layout::column()
            },
            Label::new(""),
            TextField,
            Focusable,
            TextStyle {
                size: 13.,
                ..Default::default()
            },
            Placeholder::new("Search provider or model…"),
        ));
        world.insert(
            e,
            Semantics {
                name: Some("Search models".into()),
                ..Default::default()
            },
        );
        child(world, root, e);
        search = Some(e);
    }
    let body = world.spawn((Layout {
        gap: 3.,
        ..column(Size::Hug)
    },));
    child(world, root, body);
    *world.resource_mut::<Menus>().unwrap() = Menus {
        root: Some(root),
        kind: Some(kind),
        search,
        body: Some(body),
        query: String::new(),
    };
    match kind {
        Kind::Model => model_rows(world, body, ""),
        Kind::Runtime => {
            item(world, body, "menu:close", "✓ Medousa · native turns", true);
            muted(
                world,
                body,
                "External runtimes · not connected in this port",
            );
            for name in [
                "Codex",
                "Cursor",
                "Hermes",
                "Claude Code",
                "Devin",
                "Pi",
                "Muse Code",
                "Hades",
            ] {
                item(
                    world,
                    body,
                    "unsupported",
                    &format!("{name} · unavailable here"),
                    false,
                );
            }
        }
        Kind::Mode => {
            let m = world.resource::<Model>().unwrap();
            let modes = m.modes.clone();
            let selected = m.mode.clone();
            let available = m.commands.is_some() && m.chat.session.is_some() && !m.loading;
            if modes.is_empty() {
                muted(world, body, "Mode registry unavailable · connect to load");
            }
            for (id, name, enabled) in modes {
                item(
                    world,
                    body,
                    &format!("mode:{id}"),
                    &format!(
                        "{} {name}",
                        if selected.as_deref() == Some(&id) {
                            "✓"
                        } else {
                            " "
                        }
                    ),
                    enabled && available,
                );
            }
        }
        Kind::Behavior => {
            let depth = world.resource::<Model>().unwrap().depth.clone();
            item(
                world,
                body,
                "behavior:depth",
                &format!("Response depth · {depth}  >"),
                true,
            );
            item(
                world,
                body,
                "unsupported",
                "Response style · not ported",
                false,
            );
            item(world, body, "unsupported", "Reasoning · not ported", false);
            item(
                world,
                body,
                "unsupported",
                "Read replies aloud · not ported",
                false,
            );
            muted(world, body, "Next-turn overrides in this window only");
        }
        Kind::Depth => {
            item(world, body, "behavior:main", "< Back to behavior", true);
            muted(world, body, "Next turn in this window");
            let depth = world.resource::<Model>().unwrap().depth.clone();
            for (id, name) in [
                ("concise", "Concise"),
                ("standard", "Standard"),
                ("deep", "Deep"),
            ] {
                item(
                    world,
                    body,
                    &format!("depth:{id}"),
                    &format!("{} {name}", if depth == id { "✓" } else { " " }),
                    true,
                );
            }
            muted(world, body, "Does not change workshop defaults");
        }
    }
    if open_popup(world, root) {
        let focus = search
            .or_else(|| {
                children_of(world, body)
                    .into_iter()
                    .find(|e| world.get::<Button>(*e).is_some_and(|b| b.enabled))
            })
            .unwrap_or(x);
        request_focus(world, focus);
    }
}
fn model_rows(world: &mut World, body: Entity, query: &str) {
    for e in children_of(world, body) {
        despawn(world, e);
    }
    let m = world.resource::<Model>().unwrap();
    let models = m.models.clone();
    let selection = m.selection;
    let query = query.to_lowercase();
    let mut provider = String::new();
    let mut count = 0;
    // Catalog order is stable and option actions carry the original catalog index.
    let mut indexes = (0..models.len()).collect::<Vec<_>>();
    indexes.sort_by_key(|i| (models[*i].0.clone(), models[*i].1.clone()));
    for i in indexes {
        let (p, name) = &models[i];
        if !format!("{p} {name}").to_lowercase().contains(&query) {
            continue;
        }
        if provider != *p {
            muted(world, body, p);
            provider.clone_from(p);
        }
        item(
            world,
            body,
            &format!("pick:{i}"),
            &format!("{} {name}", if selection == Some(i) { "✓" } else { " " }),
            true,
        );
        count += 1;
    }
    if count == 0 {
        muted(
            world,
            body,
            if models.is_empty() {
                "No models loaded · connect to your workshop"
            } else {
                "No matching models"
            },
        );
    }
}
pub fn update(world: &mut World) {
    let m = world.resource::<Menus>().unwrap();
    let Some(search) = m.search else {
        return;
    };
    let query = world.get::<Label>(search).unwrap().text.clone();
    if query != m.query {
        let body = m.body.unwrap();
        world.resource_mut::<Menus>().unwrap().query = query.clone();
        model_rows(world, body, &query);
    }
}
