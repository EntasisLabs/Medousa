//! Workshop-wide signals at the foot of the rail. Medousa-home keeps these in a
//! status bar; here each appears only when it has something to say: active
//! work with a count (attention outranks it), and the connection when it is
//! not fine. Silent otherwise, so the rail never carries standing chrome.
use super::*;
use medousa_types::WorkBoardColumn;
use rndr_ui::menu::{MenuAt, MenuHandle, MenuItem, MenuPicked, open_menu};
use std::time::{Duration, Instant};

const ACTIVITY: u64 = 0x5753_0001;
const CONNECTION: u64 = 0x5753_0002;
/// Menu item ids are this plus the work's index in the current list.
const WORK_ITEM: u64 = 0x5753_1000;
/// Re-read the workshop's active work this often while connected.
const REFRESH: Duration = Duration::from_secs(10);
/// Enough to cover a busy workshop without an unbounded detail fan-out.
const LIMIT: usize = 12;

/// One piece of active work, already joined to its conversation.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Work {
    pub title: String,
    pub column: WorkBoardColumn,
    pub status: String,
    pub session: Option<String>,
}
impl Work {
    fn attention(&self) -> bool {
        self.column == WorkBoardColumn::Blocked
    }
    fn active(&self) -> bool {
        matches!(
            self.column,
            WorkBoardColumn::Backlog
                | WorkBoardColumn::InFlight
                | WorkBoardColumn::WrappingUp
                | WorkBoardColumn::Blocked
        )
    }
}

pub(crate) enum Command {
    Refresh,
}
pub(crate) struct Update(Result<Vec<Work>, String>);

struct State {
    work: Vec<Work>,
    fetched: Option<Instant>,
    inflight: bool,
    pending: Vec<Command>,
    activity: rndr_ui::controls::ControlHandle,
    badge: Entity,
    connection: rndr_ui::controls::ControlHandle,
    menu: Option<(MenuHandle, Vec<Work>)>,
}

fn icon_button(
    world: &mut World,
    parent: Entity,
    id: u64,
    icon: Icon,
    name: &str,
) -> rndr_ui::controls::ControlHandle {
    let mut props = ButtonProps::new(name, HostIntent::Command(id));
    props.variant = controls::ButtonVariant::Quiet;
    let handle = controls::IconButton::icon(world, parent, UiId(id), icon, name, props)
        .expect("rail status control");
    let layout = world.get_mut::<Layout>(handle.root).unwrap();
    layout.width = Size::Px(32.);
    layout.height = Size::Px(32.);
    world.insert(handle.root, AlignSelf(Items::Center));
    world.get_mut::<LabelBox>(handle.label).unwrap().inset_x = 0.;
    visual::action_hint(world, handle.root, name);
    world.insert(handle.root, Hidden);
    handle
}

/// Append the status foot to the rail, below its destinations.
pub(super) fn mount(world: &mut World, rail: Entity) {
    let spacer = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    },));
    set_parent(world, spacer, rail);
    let connection = icon_button(world, rail, CONNECTION, Icon::CircleAlert, "Connection");
    world.insert(
        connection.label,
        UiStyle {
            foreground: Some(Token::WarningText.into()),
            ..Default::default()
        },
    );
    let activity = icon_button(world, rail, ACTIVITY, Icon::Layers, "Workshop activity");
    // A count pinned to the icon's corner; decorative, the button's name
    // carries the same number for assistive technology.
    let badge = world.spawn((
        Layout {
            width: Size::Hug,
            height: Size::Px(14.),
            min_width: 14.,
            ..Layout::column()
        },
        Absolute {
            right: Some(0.),
            top: Some(0.),
            ..Default::default()
        },
        Label::new(""),
        LabelBox {
            inset_x: 3.,
            inset_y: 0.,
            min_height: 0.,
        },
        Radius(7.),
    ));
    set_parent(world, badge, activity.root);
    world.insert_resource(State {
        work: vec![],
        fetched: None,
        inflight: false,
        pending: vec![],
        activity,
        badge,
        connection,
        menu: None,
    });
}

fn connected(world: &World) -> bool {
    world
        .resource::<Model>()
        .is_some_and(|m| m.commands.is_some())
}

/// Each frame after UI reconciliation: queue a refresh when due, present the
/// signals, and route the work menu.
pub(super) fn update(world: &mut World) {
    if world.resource::<State>().is_none() {
        return;
    }
    let online = connected(world);
    {
        let state = world.resource_mut::<State>().unwrap();
        let due = state.fetched.is_none_or(|at| at.elapsed() >= REFRESH);
        if online && due && !state.inflight {
            state.inflight = true;
            state.pending.push(Command::Refresh);
        }
        if !online {
            state.work.clear();
        }
    }
    present(world, online);
    for event in rndr_core::events::<MenuPicked>(world).to_vec() {
        let state = world.resource_mut::<State>().unwrap();
        if !state.menu.as_ref().is_some_and(|(m, _)| m.root == event.menu) {
            continue;
        }
        let (_, work) = state.menu.take().unwrap();
        let Some(item) = event
            .item
            .checked_sub(WORK_ITEM)
            .and_then(|i| work.get(i as usize))
        else {
            continue;
        };
        if let Some(session) = item.session.clone() {
            crate::surfaces::select(world, crate::surfaces::SurfaceId::Chat);
            conversations::open(world, Some(session));
        }
    }
}

fn present(world: &mut World, online: bool) {
    let state = world.resource::<State>().unwrap();
    let (activity, badge, connection) = (state.activity, state.badge, state.connection);
    let active = state.work.iter().filter(|w| w.active()).count();
    let attention = state.work.iter().filter(|w| w.attention()).count();
    let show = |world: &mut World, entity: Entity, visible: bool| {
        if visible {
            world.remove::<Hidden>(entity);
        } else {
            world.insert(entity, Hidden);
        }
    };
    show(world, connection.root, !online);
    if !online {
        let status = world
            .resource::<Ui>()
            .and_then(|ui| world.get::<Label>(ui.status))
            .map(|l| l.text.clone())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "Not connected to Medousa".into());
        set_name(world, connection.root, &format!("{status} · Open connection details"));
    }
    show(world, activity.root, active > 0);
    if active == 0 {
        return;
    }
    let name = if attention > 0 {
        format!("{active} in motion · {attention} need your decision")
    } else {
        format!("{active} in motion")
    };
    set_name(world, activity.root, &name);
    world.get_mut::<Label>(badge).unwrap().text = active.to_string();
    let (fill, ink) = if attention > 0 {
        (Token::WarningFill, Token::WarningText)
    } else {
        (Token::Accent, Token::OnAccent)
    };
    world.insert(
        badge,
        UiStyle {
            background: Some(fill.into()),
            foreground: Some(ink.into()),
            typography: Some(Typography::Caption),
            ..Default::default()
        },
    );
}

fn set_name(world: &mut World, root: Entity, name: &str) {
    if let Some(semantics) = world.get_mut::<Semantics>(root) {
        if semantics.name.as_deref() != Some(name) {
            semantics.name = Some(name.into());
        }
    }
}

/// Active work right now, for other entry points such as the palette.
pub(super) fn active_count(world: &World) -> usize {
    world
        .resource::<State>()
        .map_or(0, |s| s.work.iter().filter(|w| w.active()).count())
}
/// Open the activity menu from somewhere other than the rail button.
pub(super) fn open_activity(world: &mut World) {
    action(world, &HostIntent::Command(ACTIVITY));
}

/// Host routes native button intents here before surface actions.
pub(super) fn action(world: &mut World, intent: &HostIntent) -> bool {
    let Some(state) = world.resource::<State>() else {
        return false;
    };
    match intent {
        HostIntent::Command(CONNECTION) => {
            crate::surfaces::select(world, crate::surfaces::SurfaceId::Chat);
            super::action(world, DETAILS);
            true
        }
        HostIntent::Command(ACTIVITY) => {
            let anchor = state.activity.root;
            // Attention first, then the most recently created.
            let mut work = state.work.iter().filter(|w| w.active()).cloned().collect::<Vec<_>>();
            work.sort_by_key(|w| !w.attention());
            let entries = work
                .iter()
                .enumerate()
                .map(|(i, w)| {
                    MenuItem::new(WORK_ITEM + i as u64, format!("{} · {}", w.title, w.status))
                        .enabled(w.session.is_some())
                        .into()
                })
                .collect::<Vec<_>>();
            let at = world
                .get::<Rect>(anchor)
                .map_or(MenuAt::Below, |r| MenuAt::Point([r.x + r.w + 4., r.y]));
            if let Ok(handle) = open_menu(world, anchor, at, &entries) {
                world.resource_mut::<State>().unwrap().menu = Some((handle, work));
            }
            true
        }
        _ => false,
    }
}

pub(crate) fn take_commands(world: &mut World) -> Vec<Command> {
    world
        .resource_mut::<State>()
        .map(|s| std::mem::take(&mut s.pending))
        .unwrap_or_default()
}

pub(crate) async fn execute(client: &medousa_sdk::MedousaClient, command: Command) -> Update {
    let Command::Refresh = command;
    use futures_util::{StreamExt, stream};
    let page = match client
        .workspace()
        .list_cards(&medousa_types::WorkspaceCardsQuery {
            include_terminal: Some(false),
            limit: Some(LIMIT),
            ..Default::default()
        })
        .await
    {
        Ok(page) => page,
        Err(_) => return Update(Err("Workshop activity could not be refreshed.".into())),
    };
    // Details carry the owning conversation; a card that can't be read keeps
    // its place in the count without a destination.
    let work = stream::iter(page.cards.into_iter().map(|card| async move {
        let session = client
            .workspace()
            .get_card(&card.id.0)
            .await
            .ok()
            .and_then(|detail| detail.session_id);
        Work {
            title: card.title,
            column: card.column,
            status: card.status_label,
            session,
        }
    }))
    .buffered(4)
    .collect::<Vec<_>>()
    .await;
    Update(Ok(work))
}

pub(crate) fn apply(world: &mut World, update: Update) {
    let Some(state) = world.resource_mut::<State>() else {
        return;
    };
    state.inflight = false;
    state.fetched = Some(Instant::now());
    // A failed refresh keeps the last known work rather than flashing empty.
    if let Ok(work) = update.0 {
        state.work = work;
    }
}

#[cfg(test)]
pub(super) fn set_work(world: &mut World, work: Vec<Work>) {
    let state = world.resource_mut::<State>().unwrap();
    state.work = work;
    state.fetched = Some(Instant::now());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(online: bool) -> App {
        let mut app = crate::ui::app();
        app.world_mut().insert_resource(Viewport::new(1200., 850.));
        if online {
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            std::mem::forget(rx);
            app.world_mut().resource_mut::<Model>().unwrap().commands = Some(tx);
        }
        for _ in 0..3 {
            app.update();
        }
        app
    }
    fn work(title: &str, column: WorkBoardColumn, session: &str) -> Work {
        Work {
            title: title.into(),
            column,
            status: "Working".into(),
            session: Some(session.into()),
        }
    }
    fn state(app: &App) -> &State {
        app.world().resource::<State>().unwrap()
    }
    fn name(app: &App, root: Entity) -> String {
        app.world()
            .get::<Semantics>(root)
            .and_then(|s| s.name.clone())
            .unwrap_or_default()
    }

    #[test]
    fn rail_foot_is_silent_when_online_and_idle() {
        let mut app = app(true);
        set_work(app.world_mut(), vec![work("Done", WorkBoardColumn::Done, "s")]);
        app.update();
        let s = state(&app);
        assert!(app.world().has::<Hidden>(s.activity.root));
        assert!(app.world().has::<Hidden>(s.connection.root));
        // Online and due: a refresh is requested and gated until it answers,
        // rather than queued every frame.
        let mut app = self::app(true);
        assert!(state(&app).inflight);
        for _ in 0..3 {
            app.update();
        }
        assert!(state(&app).inflight && state(&app).pending.is_empty());
        apply(app.world_mut(), Update(Err("offline".into())));
        assert!(!state(&app).inflight && state(&app).fetched.is_some());
    }

    #[test]
    fn active_work_shows_a_count_and_attention_outranks_it() {
        let mut app = app(true);
        set_work(
            app.world_mut(),
            vec![
                work("Index repo", WorkBoardColumn::InFlight, "a"),
                work("Draft notes", WorkBoardColumn::Backlog, "b"),
            ],
        );
        app.update();
        let (root, badge) = (state(&app).activity.root, state(&app).badge);
        assert!(!app.world().has::<Hidden>(root));
        assert_eq!(app.world().get::<Label>(badge).unwrap().text, "2");
        assert_eq!(name(&app, root), "2 in motion");
        assert_eq!(
            app.world().get::<UiStyle>(badge).unwrap().background,
            Some(Token::Accent.into())
        );
        set_work(
            app.world_mut(),
            vec![
                work("Index repo", WorkBoardColumn::InFlight, "a"),
                work("Approve deploy", WorkBoardColumn::Blocked, "b"),
            ],
        );
        app.update();
        assert_eq!(name(&app, root), "2 in motion · 1 need your decision");
        assert_eq!(
            app.world().get::<UiStyle>(badge).unwrap().background,
            Some(Token::WarningFill.into())
        );
    }

    #[test]
    fn connection_glyph_appears_only_offline_and_names_the_problem() {
        let app = app(false);
        let root = state(&app).connection.root;
        assert!(!app.world().has::<Hidden>(root));
        assert!(name(&app, root).ends_with("Open connection details"));
        assert!(app.world().has::<Hidden>(state(&app).activity.root));
    }

    #[test]
    fn picking_work_opens_its_conversation_in_chat() {
        let mut app = app(true);
        set_work(
            app.world_mut(),
            vec![
                work("Index repo", WorkBoardColumn::InFlight, "first"),
                work("Approve deploy", WorkBoardColumn::Blocked, "second"),
            ],
        );
        app.update();
        let root = state(&app).activity.root;
        assert!(action(app.world_mut(), &HostIntent::Command(ACTIVITY)));
        let (menu, listed) = state(&app).menu.clone().unwrap();
        // Attention leads the list.
        assert_eq!(listed[0].title, "Approve deploy");
        crate::surfaces::select(app.world_mut(), crate::surfaces::SurfaceId::Notes);
        send(
            app.world_mut(),
            MenuPicked {
                menu: menu.root,
                item: WORK_ITEM,
            },
        );
        app.update();
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
            Some("second")
        );
        let _ = root;
    }
}
