//! App-owned menus built on rndr's canonical popup/focus/dismissal scope.
use crate::presentation::{button, label};
use crate::shell::{muted, row};
use crate::{presentation, state::Model};
use rndr_app::{child, column};
use rndr_core::*;
use rndr_ui::{Token, Typography};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Model,
    Runtime,
    Mode,
    Behavior,
    Depth,
    Appearance,
    Settings,
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
            // AnchoredPopup chooses its viewport from the anchor (at least
            // 240pt). A fixed wider content width creates horizontal overflow.
            width: Size::Fill,
            height: Size::Hug,
            padding: 10.,
            gap: 5.,
            ..Layout::column()
        },
        rndr_ui::UiStyle {
            background: Some(Token::Overlay.into()),
            corner: Some(rndr_ui::Corner::Overlay),
            ..Default::default()
        },
        Scroll::default(),
        AnchoredPopup {
            anchor,
            max_height: 380.,
            closed,
        },
    ));
    world.insert(
        root,
        Semantics {
            name: Some(
                if kind == Kind::Settings {
                    "Choose settings view"
                } else {
                    "Preview menu"
                }
                .into(),
            ),
            ..Default::default()
        },
    );
    presentation::outline(world, root);
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
            Kind::Appearance => "Window appearance",
            Kind::Settings => "Choose settings view",
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
            presentation::theme(world).typography(Typography::Label),
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
        Kind::Settings => {
            let current = world
                .resource::<crate::admin_model::Desktop>()
                .unwrap()
                .last_settings;
            for (index, (_, name, hint)) in crate::admin_model::SETTINGS.iter().enumerate() {
                if index == 0 || index == 6 {
                    label(
                        world,
                        body,
                        if index == 0 { "Medousa" } else { "This host" },
                        true,
                    );
                }
                let entry = item(
                    world,
                    body,
                    &format!("settings:{index}"),
                    &format!("{}{}", if index == current { "✓ " } else { "" }, name),
                    true,
                );
                world.insert(
                    entry,
                    Semantics {
                        name: Some(format!(
                            "{name}{} · {hint}",
                            if index == current {
                                ", current settings view"
                            } else {
                                ""
                            }
                        )),
                        ..Default::default()
                    },
                );
            }
        }
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
            item(world, body, "appearance:open", "Window appearance  >", true);
        }
        Kind::Appearance => {
            item(world, body, "behavior:main", "< Back to behavior", true);
            muted(world, body, "Local to this window · not saved");
            presentation::appearance_controls(world, body);
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
        let current_setting = if kind == Kind::Settings {
            let action = format!(
                "settings:{}",
                world
                    .resource::<crate::admin_model::Desktop>()
                    .unwrap()
                    .last_settings
            );
            children_of(world, body)
                .into_iter()
                .find(|e| world.get::<Button>(*e).is_some_and(|b| b.action == action))
        } else {
            None
        };
        let focus = current_setting
            .or(search)
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
/// App-local chooser order is semantic, independent of recycled entity indices.
/// Escape/outside dismissal and focus restoration remain canonical popup policy.
/// Only a single ordinary navigation key is handled here; never consume editor,
/// composition, submission or mixed batches on behalf of the native-input gate.
pub fn settings_keyboard(world: &mut World) {
    let Some(m) = world.resource::<Menus>() else {
        return;
    };
    let (Some(root), Some(body)) = (m.root, m.body) else {
        return;
    };
    if m.kind != Some(Kind::Settings) || world.has::<Hidden>(root) {
        return;
    }
    let input = world.resource::<Input>().unwrap();
    if input.keys.len() != 1
        || !input.chars.is_empty()
        || !input.ime_preedit.is_empty()
        || !input.ime_commit.is_empty()
    {
        return;
    }
    let key = &input.keys[0];
    if key.released
        || key.synthetic
        || key.modifiers.ctrl
        || key.modifiers.alt
        || key.modifiers.super_key
    {
        return;
    }
    let LogicalKey::Named(named) = key.logical else {
        return;
    };
    let shift = key.modifiers.shift;
    let entries: Vec<_> = (0..crate::admin_model::SETTINGS.len())
        .filter_map(|i| {
            let action = format!("settings:{i}");
            children_of(world, body).into_iter().find(|e| {
                world
                    .get::<Button>(*e)
                    .is_some_and(|b| b.enabled && b.action == action)
            })
        })
        .collect();
    if entries.is_empty() {
        return;
    }
    let focused = world.resource::<Focus>().and_then(|f| f.entity);
    let index = entries.iter().position(|e| Some(*e) == focused);
    let target = match named {
        NamedKey::ArrowDown => entries[(index.map_or(0, |i| i + 1)) % entries.len()],
        NamedKey::ArrowUp => {
            entries[index.map_or(entries.len() - 1, |i| {
                (i + entries.len() - 1) % entries.len()
            })]
        }
        NamedKey::Home => entries[0],
        NamedKey::End => entries[entries.len() - 1],
        NamedKey::Tab => {
            // Include the close button in the cyclic Tab order.
            let close = world.entities_with::<Button>().into_iter().find(|e| {
                world
                    .get::<Button>(*e)
                    .is_some_and(|b| b.action == "menu:close")
                    && world
                        .get::<Parent>(*e)
                        .and_then(|p| world.get::<Parent>(p.entity))
                        .is_some_and(|p| p.entity == root)
            });
            let mut order = entries.clone();
            if let Some(close) = close {
                order.insert(0, close);
            }
            let at = order.iter().position(|e| Some(*e) == focused).unwrap_or(0);
            order[(at + if shift { order.len() - 1 } else { 1 }) % order.len()]
        }
        _ => return,
    };
    if request_focus(world, target) {
        let input = world.resource_mut::<Input>().unwrap();
        input.keys.clear();
        input.key = None;
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
