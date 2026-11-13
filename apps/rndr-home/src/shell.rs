//! Shell assembly and read-only state-to-view synchronization.
use crate::presentation::{button, label};
use crate::{conversation, menus, presentation, state::Model};
use rndr_app::{child, column};
use rndr_core::*;
use rndr_ui::{Token, Typography, UiStyle};

#[derive(Clone, Copy)]
pub(super) struct Ui {
    pub(super) empty: Entity,
    pub(super) composer: Entity,
    pub(super) status: Entity,
    pub(super) model: Entity,
    pub(super) center: Entity,
    pub(super) send: Entity,
    pub(super) cancel: Entity,
    pub(super) recover: Entity,
    pub(super) runtime: Entity,
    pub(super) mode: Entity,
    pub(super) behavior: Entity,
    pub(super) conversation: Entity,
    pub(super) latest: Entity,
}
pub(super) fn row(world: &mut World, parent: Entity, height: Size) -> Entity {
    let e = world.spawn((Layout {
        width: Size::Fill,
        height,
        gap: 10.,
        ..Layout::row()
    },));
    child(world, parent, e)
}
pub(super) fn muted(world: &mut World, parent: Entity, text: &str) -> Entity {
    let e = label(world, parent, text, false);
    presentation::secondary(world, e);
    world.insert(
        e,
        presentation::theme(world).typography(Typography::Secondary),
    );
    e
}
pub(super) fn setup(world: &mut World) {
    let root = world.spawn((Layout {
        padding: 12.,
        gap: 8.,
        ..column(Size::Fill)
    },));
    presentation::surface(world, root, Token::Canvas, None);
    presentation::outline(world, root);
    let tabs = row(world, root, Size::Px(38.));
    let tab = button(world, tabs, "reload", "Today");
    world.get_mut::<Layout>(tab).unwrap().width = Size::Px(125.);
    let add = button(world, tabs, "new", "+");
    world.get_mut::<Layout>(add).unwrap().width = Size::Px(42.);
    muted(world, tabs, "Medousa · native preview");
    let reload = button(world, tabs, "reload", "↻");
    world.get_mut::<Layout>(reload).unwrap().width = Size::Px(36.);
    world.insert(
        reload,
        Semantics {
            name: Some("Reload history".into()),
            ..Default::default()
        },
    );
    let content = world.spawn((Layout {
        gap: 0.,
        ..column(Size::Fill)
    },));
    child(world, root, content);
    let empty = world.spawn((Layout {
        gap: 10.,
        ..column(Size::Fill)
    },));
    child(world, content, empty);
    let top_air = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Px(90.),
        ..Layout::column()
    },));
    child(world, empty, top_air);
    let spacer = world.spawn((column(Size::Fill),));
    child(world, empty, spacer);
    let heading = label(world, empty, "What are we doing today?", true);
    world.get_mut::<TextStyle>(heading).unwrap().align = TextAlign::Center;
    world.get_mut::<TextStyle>(heading).unwrap().size = 21.;
    let space = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Px(30.),
        ..Layout::column()
    },));
    child(world, empty, space);
    let recent = button(world, empty, "recent", "Continue where we left off");
    world.insert(recent, Color::rgba(0., 0., 0., 0.));
    world.get_mut::<TextStyle>(recent).unwrap().align = TextAlign::Center;
    world.get_mut::<TextStyle>(recent).unwrap().size = 14.;
    presentation::secondary(world, recent);
    let spacer = world.spawn((column(Size::Fill),));
    child(world, empty, spacer);
    let conversation = conversation::setup(world, content);
    world.insert(conversation, Hidden);
    // Reserve the small control lane so hiding the button does not resize the
    // virtualized scrollport in the same frame as its explicit End operation.
    let latest_lane = row(world, root, Size::Px(28.));
    let latest = button(world, latest_lane, "history:latest", "↓ Jump to latest");
    world.get_mut::<Layout>(latest).unwrap().height = Size::Px(28.);
    world.get_mut::<Layout>(latest).unwrap().min_height = 0.;
    world.insert(
        latest,
        Semantics {
            name: Some("Jump to latest conversation message".into()),
            ..Default::default()
        },
    );
    world.insert(latest, Hidden);
    let centered = row(world, root, Size::Hug);
    let left = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Px(1.),
        ..Layout::column()
    },));
    child(world, centered, left);
    let center = world.spawn((Layout {
        width: Size::Px(670.),
        height: Size::Hug,
        gap: 5.,
        ..Layout::column()
    },));
    child(world, centered, center);
    let right = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Px(1.),
        ..Layout::column()
    },));
    child(world, centered, right);
    let crate::composer::Handles {
        composer,
        model,
        send,
        cancel,
        runtime,
        mode,
        behavior,
    } = crate::composer::setup(world, center);
    let bottom_space = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Px(4.),
        ..Layout::column()
    },));
    child(world, root, bottom_space);
    let recover = button(
        world,
        root,
        "recover",
        "Restore last unconfirmed prompt · empty composer only",
    );
    world.insert(recover, Hidden);
    let status = muted(world, root, "Offline preview");
    world.insert_resource(Ui {
        empty,
        composer,
        status,
        model,
        center,
        send,
        cancel,
        recover,
        runtime,
        mode,
        behavior,
        conversation,
        latest,
    });
    crate::desktop::mount(world, root);
    request_focus(world, composer);
}

pub(super) fn sync(world: &mut World) {
    let ui = *world.resource::<Ui>().unwrap();
    let m = world.resource::<Model>().unwrap();
    let status = if m.loading {
        "Loading history…".into()
    } else {
        m.chat.status.clone()
    };
    let status = m
        .chat
        .session
        .as_ref()
        .map_or_else(|| status.clone(), |id| format!("Local · {id} · {status}"));
    let input_ready = crate::composer::native_input_ready(world);
    let status = if input_ready {
        status
    } else {
        format!("{status} · Input safety pending: Send / Restore / New / Recent / Reload disabled")
    };
    let selected = m
        .selection
        .filter(|i| *i < m.models.len())
        .map(|i| format!("{} · {} v", m.models[i].1, m.models[i].0))
        .unwrap_or_else(|| "Select model v".into());
    let mode_label = m
        .mode
        .as_ref()
        .and_then(|id| m.modes.iter().find(|(value, _, _)| value == id))
        .map(|(_, label, _)| format!("{label} v"))
        .unwrap_or_else(|| "Mode v".into());
    let mut notices = Vec::new();
    if !m.chat.busy {
        for (session, prompt) in &m.recovery {
            notices.push(("notice".into(), format!("Recoverable prompt · {session}\n\nAdmission not confirmed. Check history before restoring.\n\n{prompt}")));
        }
    }
    let busy = m.chat.busy;
    let uncertain = m.chat.uncertain;
    let connected = m.commands.is_some();
    // Temporarily move ownership out of the resource so ECS mutation can borrow
    // canonical strings. No full-history/live-text clones on an idle frame.
    let m = world.resource_mut::<Model>().unwrap();
    let history = std::mem::take(&mut m.history_messages);
    let live = std::mem::take(&mut m.chat.live_messages);
    if history.is_empty() && live.is_empty() && notices.is_empty() {
        world.insert(ui.conversation, Hidden);
        world.remove::<Hidden>(ui.empty);
    } else {
        world.remove::<Hidden>(ui.conversation);
        world.insert(ui.empty, Hidden);
    }
    conversation::sync_iter(
        world,
        ui.conversation,
        history
            .iter()
            .chain(&live)
            .chain(&notices)
            .map(|(role, text)| (role.as_str(), text.as_str())),
    );
    let m = world.resource_mut::<Model>().unwrap();
    m.history_messages = history;
    m.chat.live_messages = live;
    let away = conversation::away_from_latest(world, ui.conversation);
    world.get_mut::<Button>(ui.latest).unwrap().enabled = away;
    if away {
        world.remove::<Hidden>(ui.latest);
    } else {
        world.insert(ui.latest, Hidden);
    }
    menus::update(world);
    world.get_mut::<Label>(ui.mode).unwrap().text = mode_label;
    world.get_mut::<Label>(ui.status).unwrap().text = status;
    world.get_mut::<Label>(ui.model).unwrap().text = selected;
    world.get_mut::<Label>(ui.send).unwrap().text = if busy { "…" } else { "↑" }.into();
    if busy || uncertain {
        world.remove::<Hidden>(ui.cancel);
    } else {
        world.insert(ui.cancel, Hidden);
    }
    // Use rndr's reading measure rather than a second fixed-width policy.
    let width = world.resource::<Viewport>().unwrap().width - crate::desktop::rail_width(world);
    let center_width =
        presentation::reading_width((width - 48.).max(0.), &presentation::theme(world));
    world.get_mut::<Layout>(ui.center).unwrap().width = Size::Px(center_width);
    world.get_mut::<Layout>(ui.model).unwrap().width =
        Size::Px((center_width - 136.).clamp(100., 340.));
    let m = world.resource::<Model>().unwrap();
    let has_session = m.chat.session.is_some();
    let can_send = input_ready
        && connected
        && !busy
        && !uncertain
        && !m.loading
        && has_session
        && m.selection.is_some()
        && !world
            .get::<Label>(ui.composer)
            .unwrap()
            .text
            .trim()
            .is_empty();
    let can_navigate = input_ready && connected && !busy && !m.loading;
    let can_settings = !busy && !m.loading;
    let can_select = !busy && !m.models.is_empty();
    let can_cancel = connected && !m.loading && has_session && (busy || uncertain);
    let has_recovery = !m.recovery.is_empty();
    let can_recover = input_ready
        && has_recovery
        && !busy
        && !m.loading
        && world.get::<Label>(ui.composer).unwrap().text.is_empty();
    if has_recovery {
        world.remove::<Hidden>(ui.recover);
    } else {
        world.insert(ui.recover, Hidden);
    }
    for e in world.entities_with::<Button>() {
        let b = world.get_mut::<Button>(e).unwrap();
        b.enabled = match b.action.as_str() {
            "send" => can_send,
            "model" => !busy,
            "runtime" | "behavior" => can_settings,
            "mode" => can_settings && has_session && connected,
            "attachments" => false,
            _ if b.action.starts_with("pick:") => can_select,
            "new" | "recent" => can_navigate,
            "reload" => can_navigate && has_session,
            "cancel" => can_cancel,
            "recover" => can_recover,
            _ => b.enabled,
        };
    }
    let style = world.get_mut::<UiStyle>(ui.send).unwrap();
    style.background = Some(if can_send { Token::Text } else { Token::Raised }.into());
    style.foreground = Some(
        if can_send {
            Token::Canvas
        } else {
            Token::Disabled
        }
        .into(),
    );
}
