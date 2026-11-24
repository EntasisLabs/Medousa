//! ⌘K command palette. RNDR's palette owns the field, list, matching,
//! keyboard and focus scope; this module only supplies commands that already
//! exist elsewhere in the native shell and routes their activation. It takes
//! over the jump-to and utility items medousa-home kept in its status bar.
use super::*;
use rndr_ui::collections::ItemId;
use rndr_ui::palette::{
    CommandPalette, PaletteActivated, PaletteCommand, PaletteHandle, PaletteProps,
};

const OPEN: &str = "palette.open";
const WIDTH: f32 = 560.;
/// Command ids. Conversations follow [`CONVERSATION`] by index.
const GO: u64 = 0x504c_0000;
const NEW_CHAT: u64 = 0x504c_0100;
const SEARCH_CHATS: u64 = 0x504c_0101;
const TOGGLE_SIDEBAR: u64 = 0x504c_0102;
const RELOAD_CHAT: u64 = 0x504c_0103;
const CONNECTION: u64 = 0x504c_0104;
const ACTIVITY: u64 = 0x504c_0105;
const CONVERSATION: u64 = 0x504c_1000;
/// Recent conversations offered without typing a search.
const RECENT: usize = 50;

const DESTINATIONS: [(crate::surfaces::SurfaceId, Icon, &str); 8] = {
    use crate::surfaces::SurfaceId::*;
    [
        (Chat, Icon::MessageCircle, "Go to Chat"),
        (Notes, Icon::Book, "Go to Notes"),
        (Code, Icon::Code, "Go to Code"),
        (Create, Icon::PenLine, "Go to Create"),
        (Reviews, Icon::Check, "Go to Reviews"),
        (Calendar, Icon::Calendar, "Go to Calendar"),
        (Peers, Icon::Users, "Go to Peers"),
        (Settings, Icon::Settings, "Open Settings"),
    ]
};

struct Palette {
    handle: PaletteHandle,
    /// Session ids in the order their commands were offered.
    sessions: Vec<String>,
}

/// Mount a closed palette under the window band and bind ⌘K.
pub(super) fn mount(world: &mut World, band: Entity) {
    // An invisible, unfocusable-by-Tab strip the popup hangs from, centered
    // under the band each frame so the palette opens where the eye is.
    let anchor = world.spawn((
        Layout {
            width: Size::Px(WIDTH),
            height: Size::Px(1.),
            ..Layout::column()
        },
        Absolute {
            left: Some(0.),
            top: Some(chrome::HEIGHT - 1.),
            ..Default::default()
        },
        Focusable,
        TabStop(false),
    ));
    set_parent(world, anchor, band);
    let mut props = PaletteProps::new("Commands");
    props.placeholder = "Search commands and conversations".into();
    props.empty = "Nothing matches".into();
    props.width = WIDTH;
    props.list_height = 360.;
    let handle = CommandPalette::mount(world, anchor, UiId(0x504c), props).expect("palette");
    world.insert_resource(Palette {
        handle,
        sessions: vec![],
    });
    world
        .resource_mut::<Keymap>()
        .unwrap()
        .bind(KeyBinding::new(OPEN, [Keystroke::character('k').primary()]));
}

fn commands(world: &World) -> (Vec<PaletteCommand>, Vec<String>) {
    let mut out = vec![];
    let command = |id: u64, title: &str, group: &str, icon: Icon| {
        let mut c = PaletteCommand::new(id, title);
        c.group = Some(group.into());
        c.icon = Some(icon);
        c
    };
    let current = crate::surfaces::active(world);
    // Only places you can go: the current destination isn't offered.
    for (i, (surface, icon, title)) in DESTINATIONS.into_iter().enumerate() {
        if surface != current {
            out.push(command(GO + i as u64, title, "Go to", icon));
        }
    }
    let online = world
        .resource::<Model>()
        .is_some_and(|m| m.commands.is_some());
    out.push(command(NEW_CHAT, "New chat", "Conversation", Icon::Plus));
    out.push(command(
        SEARCH_CHATS,
        "Search conversations",
        "Conversation",
        Icon::Search,
    ));
    out.push(command(
        TOGGLE_SIDEBAR,
        "Toggle conversation sidebar",
        "Conversation",
        Icon::PanelLeft,
    ));
    let mut reload = command(
        RELOAD_CHAT,
        "Reload conversation",
        "Conversation",
        Icon::RefreshCw,
    );
    reload.enabled = online;
    out.push(reload);
    out.push(command(
        CONNECTION,
        if online {
            "Connection settings"
        } else {
            "Reconnect · connection settings"
        },
        "Workshop",
        Icon::Info,
    ));
    let active = workshop_status::active_count(world);
    if active > 0 {
        out.push(command(
            ACTIVITY,
            &format!("Workshop activity · {active} in motion"),
            "Workshop",
            Icon::Layers,
        ));
    }
    let sessions = world
        .resource::<Model>()
        .map(|m| m.sessions.iter().take(RECENT).cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    let mut ids = vec![];
    for (i, (id, title)) in sessions.into_iter().enumerate() {
        let title = if title.trim().is_empty() {
            "Untitled chat".to_string()
        } else {
            title
        };
        out.push(command(
            CONVERSATION + i as u64,
            &title,
            "Conversations",
            Icon::MessageCircle,
        ));
        ids.push(id);
    }
    (out, ids)
}

/// Each frame: center the anchor, open on ⌘K with a fresh catalog, and run
/// whatever was activated.
pub(super) fn update(world: &mut World) {
    let Some(handle) = world.resource::<Palette>().map(|p| p.handle) else {
        return;
    };
    if let Some(band) = world.get::<Rect>(world.get::<Parent>(handle.anchor).unwrap().entity) {
        // Centred on the window, though the band now starts past the
        // navigation column: offset from wherever the band begins.
        let window = world.resource::<Viewport>().map_or(band.x + band.w, |v| v.width);
        let left = ((window - WIDTH) / 2.).max(0.) - band.x;
        let absolute = world.get_mut::<Absolute>(handle.anchor).unwrap();
        if absolute.left != Some(left) {
            absolute.left = Some(left);
        }
    }
    let fired = world
        .resource::<KeymapFired>()
        .is_some_and(|k| k.actions.iter().any(|a| a == OPEN));
    if fired {
        if handle.is_open(world) {
            handle.close(world);
        } else {
            open(world);
        }
    }
    for event in rndr_core::events::<PaletteActivated>(world).to_vec() {
        if event.palette == handle.root {
            handle.close(world);
            run(world, event.id);
        }
    }
}

/// Refresh the catalog from current state and show the palette.
pub(super) fn open(world: &mut World) {
    let (catalog, sessions) = commands(world);
    let state = world.resource_mut::<Palette>().unwrap();
    state.sessions = sessions;
    let handle = state.handle;
    let _ = handle.set_commands(world, &catalog);
    handle.open(world);
}

fn run(world: &mut World, id: ItemId) {
    let chat = |world: &mut World| crate::surfaces::select(world, crate::surfaces::SurfaceId::Chat);
    match id.0 {
        id if (GO..GO + DESTINATIONS.len() as u64).contains(&id) => {
            crate::surfaces::select(world, DESTINATIONS[(id - GO) as usize].0);
        }
        NEW_CHAT => {
            chat(world);
            super::action(world, NEW);
        }
        SEARCH_CHATS => {
            chat(world);
            super::action(world, TOGGLE_SEARCH);
        }
        TOGGLE_SIDEBAR => {
            chat(world);
            super::action(world, SIDEBAR);
        }
        RELOAD_CHAT => {
            chat(world);
            super::action(world, RELOAD);
        }
        CONNECTION => crate::settings::show_connection(world),
        ACTIVITY => workshop_status::open_activity(world),
        id if id >= CONVERSATION => {
            let index = (id - CONVERSATION) as usize;
            let session = world
                .resource::<Palette>()
                .and_then(|p| p.sessions.get(index).cloned());
            if let Some(session) = session {
                chat(world);
                conversations::open(world, Some(session));
            }
        }
        _ => {}
    }
}

#[cfg(test)]
pub(super) fn handle(world: &World) -> PaletteHandle {
    world.resource::<Palette>().unwrap().handle
}
#[cfg(test)]
pub(super) fn activate(world: &mut World, id: u64) {
    run(world, ItemId(id));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let mut app = crate::ui::app();
        app.world_mut().insert_resource(Viewport::new(1200., 850.));
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        std::mem::forget(rx);
        let m = app.world_mut().resource_mut::<Model>().unwrap();
        m.commands = Some(tx);
        m.sessions = vec![
            ("alpha".into(), "Ship the palette".into()),
            ("beta".into(), "  ".into()),
        ];
        for _ in 0..3 {
            app.update();
        }
        app
    }
    fn press(app: &mut App) {
        app.world_mut().insert_resource(KeymapFired {
            actions: vec![OPEN.into()],
        });
        update(app.world_mut());
        app.world_mut().insert_resource(KeymapFired::default());
    }

    #[test]
    fn command_k_toggles_a_catalog_of_existing_actions_and_conversations() {
        let mut app = app();
        let palette = handle(app.world());
        assert!(!palette.is_open(app.world()));
        press(&mut app);
        assert!(palette.is_open(app.world()));
        let (catalog, sessions) = commands(app.world());
        let titles = catalog.iter().map(|c| c.title.as_str()).collect::<Vec<_>>();
        for expected in [
            "Go to Notes",
            "New chat",
            "Ship the palette",
            "Untitled chat",
        ] {
            assert!(titles.contains(&expected), "{expected} in {titles:?}");
        }
        assert_eq!(sessions, ["alpha", "beta"]);
        // The surface you're on isn't offered as a destination.
        assert!(!titles.contains(&"Go to Chat"));
        // No active work, no activity command.
        assert!(!titles.iter().any(|t| t.starts_with("Workshop activity")));
        press(&mut app);
        assert!(!palette.is_open(app.world()));
    }

    #[test]
    fn activations_route_to_the_shell_actions_they_name() {
        let mut app = app();
        press(&mut app);
        activate(app.world_mut(), GO + 1);
        assert_eq!(
            crate::surfaces::active(app.world()),
            crate::surfaces::SurfaceId::Notes
        );
        // Opening a conversation returns to Chat first.
        press(&mut app);
        activate(app.world_mut(), CONVERSATION);
        assert_eq!(
            crate::surfaces::active(app.world()),
            crate::surfaces::SurfaceId::Chat
        );
        assert_eq!(
            app.world()
                .resource::<Model>()
                .unwrap()
                .requested_session
                .as_deref(),
            Some("alpha")
        );
    }

    #[test]
    fn the_palette_hangs_centered_under_the_window_band() {
        let mut app = app();
        update(app.world_mut());
        let anchor = handle(app.world()).anchor;
        let left = app.world().get::<Absolute>(anchor).unwrap().left.unwrap();
        // Centred on the window, wherever the band itself begins.
        let band = app.world().get::<Parent>(anchor).unwrap().entity;
        let band_x = app.world().get::<Rect>(band).unwrap().x;
        assert_eq!(band_x + left, (1200. - WIDTH) / 2.);
    }
}
