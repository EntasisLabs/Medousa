//! Chat history presentation mirrors SessionSidebar + sessionHistoryGroups.
//! Wire summaries stay intact; grouping never changes catalog indices/routes.
use super::*;
use chrono::{DateTime, Local, NaiveDate, Utc};
use medousa_types::SessionHistorySummary;
use std::collections::HashSet;

const SECTION: u64 = 80;
const LABELS: [&str; 5] = ["Today", "Yesterday", "Previous 7 days", "Older", "Pinned"];
type Pins = HashMap<String, HashSet<String>>;
use rndr_ui::menu::{open_menu, MenuAt, MenuHandle, MenuItem, MenuPicked};
const ROW_OPEN: u64 = 0x5347_1001;
const ROW_PIN: u64 = 0x5347_1002;
const ROW_RENAME: u64 = 0x5347_1003;
const ROW_DELETE: u64 = 0x5347_1004;
const ROW_FORK: u64 = 0x5347_1005;
const ROW_CONFIRM: u64 = 0x5347_1006;
const ROW_CANCEL: u64 = 0x5347_1007;
#[derive(Clone)]
struct RowTarget {
    session: String,
    authority: String,
}
impl Component for RowTarget {}
struct RowMenu {
    handle: MenuHandle,
    target: RowTarget,
    anchor: Entity,
}
struct RowDialog {
    root: Entity,
    field: Option<Entity>,
    target: RowTarget,
}
fn target_current(world: &World, target: &RowTarget) -> bool {
    scope(world).as_deref() == Some(target.authority.as_str())
        && world
            .resource::<Model>()
            .unwrap()
            .sessions
            .iter()
            .any(|r| r.0 == target.session)
}
/// Read the existing RNDR context gesture/menu events; no competing input system.
/// Invoked by the conversation event pass, including keyboard Menu/Shift+F10.
pub(super) fn input(world: &mut World) {
    ensure(world);
    let retired = world
        .resource::<State>()
        .unwrap()
        .dialog
        .as_ref()
        .filter(|d| !world.is_alive(d.root) || world.has::<Hidden>(d.root))
        .map(|d| d.root);
    if let Some(root) = retired {
        world.resource_mut::<State>().unwrap().dialog = None;
        if world.is_alive(root) {
            despawn(world, root);
        }
    }
    for event in rndr_core::events::<MenuPicked>(world).to_vec() {
        let matches = world
            .resource::<State>()
            .unwrap()
            .menu
            .as_ref()
            .is_some_and(|m| m.handle.root == event.menu);
        if !matches {
            continue;
        }
        let menu = world.resource_mut::<State>().unwrap().menu.take().unwrap();
        if !target_current(world, &menu.target) {
            continue;
        }
        match event.item {
            ROW_OPEN => conversations::open(world, Some(menu.target.session)),
            ROW_PIN => {
                let value = !pinned(world, &menu.target.session);
                set_pin(world, &menu.target.session, value);
            }
            ROW_FORK => dispatch(world, Command::Fork(menu.target.session)),
            ROW_RENAME | ROW_DELETE => {
                dialog(world, menu.anchor, menu.target, event.item == ROW_RENAME)
            }
            _ => {}
        }
    }
    for event in rndr_core::events::<SecondaryPress>(world).to_vec() {
        let Some(target) = world.get::<RowTarget>(event.entity).cloned() else {
            continue;
        };
        if !target_current(world, &target) {
            continue;
        }
        let ready = world.resource::<State>().unwrap().pins_ready;
        let entries = [
            MenuItem::new(ROW_OPEN, "Open conversation").into(),
            MenuItem::new(
                ROW_PIN,
                if pinned(world, &target.session) {
                    "Unpin conversation"
                } else {
                    "Pin conversation"
                },
            )
            .enabled(ready)
            .into(),
            MenuItem::new(ROW_RENAME, "Rename conversation…").into(),
            MenuItem::new(ROW_FORK, "Fork conversation").into(),
            MenuItem::new(ROW_DELETE, "Delete conversation…")
                .danger()
                .into(),
        ];
        if let Ok(handle) = open_menu(world, event.entity, MenuAt::Point(event.at), &entries) {
            world.resource_mut::<State>().unwrap().menu = Some(RowMenu {
                handle,
                target,
                anchor: event.entity,
            });
        }
    }
}
fn dialog(world: &mut World, anchor: Entity, target: RowTarget, rename: bool) {
    if let Some(old) = world.resource_mut::<State>().unwrap().dialog.take() {
        close_popup(world, old.root, false);
        despawn(world, old.root);
    }
    let root = world.spawn((
        Layout {
            width: Size::Px(320.),
            height: Size::Hug,
            padding: 12.,
            gap: 10.,
            ..Layout::column()
        },
        AnchoredPopup {
            anchor,
            max_height: 300.,
            closed: |_, _| {},
        },
        Hidden,
        rndr_ui::UiStyle {
            background: Some(rndr_ui::Token::Overlay.into()),
            ..Default::default()
        },
        Radius(visual::CONTROL_RADIUS),
        Semantics {
            name: Some(
                if rename {
                    "Rename conversation"
                } else {
                    "Delete conversation"
                }
                .into(),
            ),
            ..Default::default()
        },
    ));
    let title = world
        .resource::<Model>()
        .unwrap()
        .sessions
        .iter()
        .find(|r| r.0 == target.session)
        .map(|r| r.1.clone())
        .unwrap_or_default();
    text(
        world,
        root,
        if rename {
            "Rename conversation"
        } else {
            "Permanently delete this conversation?"
        },
        Typography::Label,
    )
    .unwrap();
    text(world, root, &title, Typography::Secondary).unwrap();
    let field = if rename {
        let field = world.spawn((
            Layout {
                width: Size::Fill,
                height: Size::Px(32.),
                ..Layout::row()
            },
            Label::new(&title),
            TextField,
            Focusable,
            Placeholder::new("Conversation title"),
        ));
        set_parent(world, field, root);
        Some(field)
    } else {
        text(
            world,
            root,
            "This removes its saved history. Closing a tab does not delete history.",
            Typography::Secondary,
        )
        .unwrap();
        None
    };
    let actions = container(
        world,
        Some(root),
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 8.,
            ..Layout::row()
        },
    );
    button(world, actions, ROW_CANCEL, "Cancel", true);
    let mut props = ButtonProps::new(
        if rename {
            "Save name"
        } else {
            "Delete conversation"
        },
        HostIntent::Command(ROW_CONFIRM),
    );
    if !rename {
        props.variant = controls::ButtonVariant::Destructive;
    }
    let confirm = controls::Button::mount(world, actions, UiId(ROW_CONFIRM), props).unwrap();
    world.resource_mut::<State>().unwrap().dialog = Some(RowDialog {
        root,
        field,
        target,
    });
    if open_popup(world, root) {
        request_focus(world, field.unwrap_or(confirm.root));
    }
}
fn dialog_action(world: &mut World, id: u64) -> bool {
    if id != ROW_CONFIRM && id != ROW_CANCEL {
        return false;
    }
    let Some(dialog) = world.resource_mut::<State>().unwrap().dialog.take() else {
        return true;
    };
    let valid = target_current(world, &dialog.target);
    let title = dialog
        .field
        .and_then(|e| world.get::<Label>(e))
        .map(|l| l.text.trim().to_owned());
    // Empty rename leaves the confirmation open and focused rather than silently closing it.
    if id == ROW_CONFIRM && valid && title.as_ref().is_some_and(|s| s.is_empty()) {
        if let Some(field) = dialog.field {
            request_focus(world, field);
        }
        world.resource_mut::<State>().unwrap().dialog = Some(dialog);
        return true;
    }
    close_popup(world, dialog.root, true);
    despawn(world, dialog.root);
    if id == ROW_CONFIRM && valid {
        let command = if let Some(title) = title {
            Command::Rename {
                session: dialog.target.session,
                title,
            }
        } else {
            Command::Delete(dialog.target.session)
        };
        dispatch(world, command);
    }
    true
}
/// Device-local preferences, scoped by daemon authority, never session facts.
/// Disk work runs off the render thread and failures remain visible.
struct PinStore {
    save: std::sync::mpsc::Sender<Pins>,
    updates: std::sync::Mutex<std::sync::mpsc::Receiver<Result<Option<Pins>, String>>>,
}
fn pin_store(path: std::path::PathBuf) -> PinStore {
    let (save, requests) = std::sync::mpsc::channel::<Pins>();
    let (out, updates) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let loaded = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<Pins>(&bytes).map_err(|e| e.to_string()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Pins::new()),
            Err(e) => Err(e.to_string()),
        };
        let _ = out.send(loaded.map(Some));
        while let Ok(mut pins) = requests.recv() {
            // Only the latest complete preference snapshot needs saving.
            while let Ok(newer) = requests.try_recv() {
                pins = newer;
            }
            let result = (|| -> Result<(), String> {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                let bytes = serde_json::to_vec(&pins).map_err(|e| e.to_string())?;
                let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
                std::fs::write(&temporary, bytes).map_err(|e| e.to_string())?;
                std::fs::rename(temporary, &path).map_err(|e| e.to_string())
            })();
            let _ = out.send(result.map(|_| None));
        }
    });
    PinStore {
        save,
        updates: std::sync::Mutex::new(updates),
    }
}
fn ensure(world: &mut World) {
    if world.resource::<State>().is_none() {
        let mut state = State::default();
        // Tests never read or write a principal's real preferences.
        if !cfg!(test) {
            state.store = Some(pin_store(
                medousa_desktop_host::paths::medousa_config_dir().join("native-chat-pins.json"),
            ));
        } else {
            state.pins_ready = true;
        }
        world.insert_resource(state);
    }
    let updates: Vec<_> = world
        .resource::<State>()
        .unwrap()
        .store
        .as_ref()
        .map(|s| s.updates.lock().unwrap().try_iter().collect())
        .unwrap_or_default();
    for update in updates {
        match update {
            Ok(Some(pins)) => {
                let s = world.resource_mut::<State>().unwrap();
                s.pins = pins;
                s.pins_ready = true;
                s.revision += 1;
            }
            Ok(None) => {}
            Err(error) => {
                // A corrupt/unreadable file is not silently overwritten.
                world.resource_mut::<Model>().unwrap().chat.status =
                    format!("Could not load/save Chat pins: {error}");
            }
        }
    }
}
fn scope(world: &World) -> Option<String> {
    world
        .resource::<Model>()?
        .chat
        .snapshot()
        .map(|s| s.authority_id.clone())
        .filter(|s| !s.is_empty())
}
fn pinned(world: &World, session: &str) -> bool {
    scope(world)
        .and_then(|scope| {
            world
                .resource::<State>()?
                .pins
                .get(&scope)
                .map(|p| p.contains(session))
        })
        .unwrap_or(false)
}
fn set_pin(world: &mut World, session: &str, value: bool) {
    ensure(world);
    let Some(scope) = scope(world) else {
        return;
    };
    let s = world.resource_mut::<State>().unwrap();
    if !s.pins_ready {
        world.resource_mut::<Model>().unwrap().chat.status =
            "Chat pins unavailable until local preferences load successfully.".into();
        return;
    }
    let pins = s.pins.entry(scope).or_default();
    if value {
        pins.insert(session.into());
    } else {
        pins.remove(session);
    }
    s.revision += 1;
    if let Some(store) = &s.store {
        if store.save.send(s.pins.clone()).is_err() {
            world.resource_mut::<Model>().unwrap().chat.status =
                "Could not save Chat pins; local preference writer unavailable.".into();
        }
    }
}
#[derive(Default)]
pub(super) struct State {
    summaries: HashMap<String, SessionHistorySummary>,
    collapsed: HashSet<usize>,
    initialized: bool,
    sections: [Option<Entity>; 5],
    revision: u64,
    pins: Pins,
    pins_ready: bool,
    store: Option<PinStore>,
    menu: Option<RowMenu>,
    dialog: Option<RowDialog>,
}
pub(super) fn receive(world: &mut World, summaries: Vec<SessionHistorySummary>, append: bool) {
    ensure(world);
    let s = world.resource_mut::<State>().unwrap();
    if !append {
        s.summaries.clear();
    }
    for row in summaries {
        s.summaries.insert(row.session_id.clone(), row);
    }
    s.revision += 1;
}
pub(super) fn action(world: &mut World, id: u64) -> bool {
    ensure(world);
    if dialog_action(world, id) {
        return true;
    }
    if id == PIN || id == UNPIN {
        if let Some(session) = world
            .resource::<Model>()
            .unwrap()
            .chat
            .session()
            .map(str::to_owned)
        {
            set_pin(world, &session, id == PIN);
        }
        return true;
    }
    if !(SECTION..SECTION + 5).contains(&id) {
        return false;
    }
    if !world.resource::<Model>().unwrap().search_query.is_empty() {
        return true;
    }
    let s = world.resource_mut::<State>().unwrap();
    let group = (id - SECTION) as usize;
    if !s.collapsed.remove(&group) {
        s.collapsed.insert(group);
    }
    s.revision += 1;
    true
}
fn bucket(timestamp: Option<DateTime<Utc>>, today: NaiveDate) -> usize {
    let Some(timestamp) = timestamp else {
        return 3;
    };
    let days = (today - timestamp.with_timezone(&Local).date_naive()).num_days();
    match days {
        ..=0 => 0,
        1 => 1,
        2..=7 => 2,
        _ => 3,
    }
}
pub(super) fn render(
    world: &mut World,
    parent: Entity,
    rows: &[(String, String)],
    selected: Option<&str>,
    authority: Option<String>,
) {
    ensure(world);
    let s = world.resource_mut::<State>().unwrap();
    if !s.initialized {
        s.collapsed.insert(3);
        s.initialized = true;
    }
    let revision = s.revision;
    let today = Local::now().date_naive();
    let key = format!(
        "{rows:?}{selected:?}{today}{revision}{authority:?}{}",
        world.resource::<Model>().unwrap().search_query
    );
    if world.resource::<Model>().unwrap().sidebar_key == key {
        return;
    }
    world.resource_mut::<Model>().unwrap().sidebar_key = key;
    let searching = !world.resource::<Model>().unwrap().search_query.is_empty();
    let s = world.resource::<State>().unwrap();
    let groups: Vec<Vec<usize>> = (0..5)
        .map(|g| {
            rows.iter()
                .enumerate()
                .filter_map(|(i, (id, _))| {
                    let group = if authority.as_ref().and_then(|a| s.pins.get(a)).is_some_and(|pins| pins.contains(id)) {
                        4
                    } else {
                        bucket(s.summaries.get(id).and_then(|s| s.last_timestamp), today)
                    };
                    (group == g).then_some(i)
                })
                .collect()
        })
        .collect();
    let collapsed = s.collapsed.clone();
    let metadata = s.summaries.clone();
    // Mount sections in canonical order even when the first catalog omits a bucket.
    for g in 0..5 {
        if world.resource::<State>().unwrap().sections[g].is_none() {
            let section = container(
                world,
                Some(parent),
                Layout {
                    width: Size::Fill,
                    height: Size::Hug,
                    gap: 2.,
                    ..Layout::column()
                },
            );
            world.resource_mut::<State>().unwrap().sections[g] = Some(section);
        }
    }
    // Pins are a destination above recency, not duplicated in chronological groups.
    if !groups[4].is_empty() {
        let sections = world.resource::<State>().unwrap().sections;
        if let Some(children) = world.get_mut::<Children>(parent) {
            let pin = sections[4].unwrap();
            children.list.retain(|e| *e != pin);
            children.list.insert(0, pin);
        }
    }
    for group in children_of(world, parent) {
        world.insert(group, Hidden);
    }
    for group in world
        .resource::<State>()
        .unwrap()
        .sections
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
    {
        for child in children_of(world, group) {
            world.insert(child, Hidden);
        }
    }
    for (g, indices) in groups.iter().enumerate().filter(|(_, v)| !v.is_empty()) {
        // A retained section survives selection/search/collapse without rebuilding controls.
        let section = if let Some(e) = world.resource::<State>().unwrap().sections[g] {
            e
        } else {
            let e = container(
                world,
                Some(parent),
                Layout {
                    width: Size::Fill,
                    height: Size::Hug,
                    gap: 2.,
                    ..Layout::column()
                },
            );
            world.resource_mut::<State>().unwrap().sections[g] = Some(e);
            e
        };
        world.remove::<Hidden>(section);
        let hidden = !searching && collapsed.contains(&g);
        let mut props = ButtonProps::new(
            format!(
                "{} {}",
                if hidden {
                    Icon::ChevronRight.scalar()
                } else {
                    Icon::ChevronDown.scalar()
                },
                LABELS[g]
            ),
            HostIntent::Command(SECTION + g as u64),
        );
        props.enabled = !searching;
        props.variant = controls::ButtonVariant::Quiet;
        let h = controls::NavigationRow::mount(
            world,
            section,
            UiId(SECTION + g as u64),
            props,
            NavigationMeta {
                secondary: Some(indices.len().to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        *world.get_mut::<Layout>(h.root).unwrap() = Layout {
            width: Size::Fill,
            height: Size::Px(26.),
            padding: 4.,
            ..Layout::row()
        };
        world.insert(
            h.label,
            UiRecipe(UiStyle {
                foreground: Some(visual::secondary().into()),
                typography: Some(Typography::Secondary),
                ..Default::default()
            }),
        );
        world.insert(
            h.root,
            Semantics {
                name: Some(LABELS[g].into()),
                description: if searching {
                    "Search results stay expanded"
                } else if hidden {
                    "Collapsed"
                } else {
                    "Expanded"
                }
                .into(),
                ..Default::default()
            },
        );
        world.remove::<Hidden>(h.root);
        if hidden {
            continue;
        }
        for &i in indices {
            let (id, title) = &rows[i];
            let props = ButtonProps::new(
                if title.trim().is_empty() {
                    "Untitled chat"
                } else {
                    title
                },
                HostIntent::Command(SESSION + i as u64),
            );
            let meta = metadata
                .get(id)
                .filter(|s| s.turns > 0)
                .map(|s| format!("{} turns", s.turns));
            let h = controls::NavigationRow::mount(
                world,
                section,
                UiId(SESSION + i as u64),
                props,
                NavigationMeta {
                    selected: selected == Some(id),
                    secondary: meta,
                    ..Default::default()
                },
            )
            .unwrap();
            world.get_mut::<Label>(h.label).unwrap().overflow = TextOverflow::Ellipsis;
            world.get_mut::<Layout>(h.root).unwrap().height = Size::Px(32.);
            world.insert(h.root, Radius(visual::CONTROL_RADIUS));
            if let Some(authority) = &authority {
                world.insert(h.root, ContextTarget);
                world.insert(
                    h.root,
                    RowTarget {
                        session: id.clone(),
                        authority: authority.clone(),
                    },
                );
            } else {
                world.remove::<ContextTarget>(h.root);
                world.remove::<RowTarget>(h.root);
            }
            world.remove::<Hidden>(h.root);
        }
    }
    if rows.is_empty() {
        // The empty state uses the same keyed controls as every other row.
        let h = controls::NavigationRow::mount(
            world,
            parent,
            UiId(0x5347_0100),
            ButtonProps {
                enabled: false,
                ..ButtonProps::new(
                    if searching {
                        "No matching conversations"
                    } else {
                        "No conversations yet"
                    },
                    HostIntent::Command(0),
                )
            },
            NavigationMeta::default(),
        )
        .unwrap();
        world.remove::<Hidden>(h.root);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> App {
        let mut app = crate::ui::app();
        app.update();
        let now = Utc::now();
        let rows: Vec<SessionHistorySummary> = [("today", 0), ("yesterday", 1), ("week", 4), ("old", 12)].into_iter().map(|(id, days)| serde_json::from_value(serde_json::json!({
            "session_id": id, "display_name": format!("{id} conversation"), "preview":"", "turns":12,
            "verification_runs":0, "last_timestamp": now - chrono::Duration::days(days)
        })).unwrap()).collect();
        app.world_mut().resource_mut::<Model>().unwrap().sessions = rows
            .iter()
            .map(|s| (s.session_id.clone(), s.display_name.clone().unwrap()))
            .collect();
        receive(app.world_mut(), rows, false);
        app.update();
        app
    }
    fn visible(w: &World, e: Entity) -> bool {
        !w.has::<Hidden>(e) && w.get::<Parent>(e).is_none_or(|p| visible(w, p.entity))
    }
    fn row(w: &World, group: usize, index: usize) -> Entity {
        let section = w.resource::<State>().unwrap().sections[group].unwrap();
        children_of(w, section)
            .into_iter()
            .find(|e| rndr_ui::identity(w, *e) == Some(UiId(SESSION + index as u64)))
            .unwrap()
    }
    fn hydrate(app: &mut App, session: &str, authority: &str) {
        app.world_mut()
            .resource_mut::<Model>()
            .unwrap()
            .chat
            .hydrate(session.into(), authority.into(), vec![], None)
            .unwrap();
        app.update();
    }
    #[test]
    fn pins_are_a_separate_scoped_destination_and_do_not_navigate_or_send() {
        let mut app = fixture();
        hydrate(&mut app, "today", "authority-a");
        let document = app.world().resource::<Model>().unwrap().document;
        set_pin(app.world_mut(), "old", true);
        app.update();
        assert!(visible(app.world(), row(app.world(), 4, 3)));
        let older = app.world().resource::<State>().unwrap().sections[3].unwrap();
        assert!(children_of(app.world(), older).into_iter().all(|e|
            rndr_ui::identity(app.world(), e) != Some(UiId(SESSION + 3)) || !visible(app.world(), e)
        ));
        let parent = app.world().resource::<Ui>().unwrap().sessions;
        assert_eq!(
            children_of(app.world(), parent)[0],
            app.world().resource::<State>().unwrap().sections[4].unwrap()
        );
        assert_eq!(app.world().resource::<Model>().unwrap().document, document);
        assert_eq!(
            app.world().resource::<Model>().unwrap().chat.session(),
            Some("today")
        );
        assert!(action(app.world_mut(), PIN));
        assert!(pinned(app.world(), "today"));
        hydrate(&mut app, "today", "authority-b");
        assert!(!pinned(app.world(), "old"));
        hydrate(&mut app, "today", "authority-a");
        assert!(pinned(app.world(), "old"));
        assert!(action(app.world_mut(), UNPIN));
        assert!(!pinned(app.world(), "today"));
    }
    #[test]
    fn pin_store_round_trips_atomically_and_reports_corrupt_preferences() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pins.json");
        let store = pin_store(path.clone());
        assert!(store
            .updates
            .lock()
            .unwrap()
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap()
            .unwrap()
            .unwrap()
            .is_empty());
        let pins = HashMap::from([("authority".into(), HashSet::from(["s".into()]))]);
        store.save.send(pins.clone()).unwrap();
        assert!(store
            .updates
            .lock()
            .unwrap()
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap()
            .unwrap()
            .is_none());
        let reopened = pin_store(path.clone());
        assert_eq!(
            reopened
                .updates
                .lock()
                .unwrap()
                .recv_timeout(std::time::Duration::from_secs(3))
                .unwrap()
                .unwrap(),
            Some(pins)
        );
        std::fs::write(&path, "not JSON").unwrap();
        let corrupt = pin_store(path);
        assert!(corrupt
            .updates
            .lock()
            .unwrap()
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap()
            .is_err());
    }
    #[test]
    fn row_menu_mutates_its_captured_identity_without_switching_the_composer() {
        let mut app = fixture();
        hydrate(&mut app, "week", "authority-a");
        let document = app.world().resource::<Model>().unwrap().document;
        let target = row(app.world(), 0, 0);
        send(
            app.world_mut(),
            SecondaryPress {
                entity: target,
                at: [40., 100.],
                keyboard: true,
            },
        );
        app.update();
        let menu = app
            .world()
            .resource::<State>()
            .unwrap()
            .menu
            .as_ref()
            .unwrap()
            .handle;
        assert!(menu.is_open(app.world()));
        send(
            app.world_mut(),
            MenuPicked {
                menu: menu.root,
                item: ROW_RENAME,
            },
        );
        app.update();
        let dlg = app
            .world()
            .resource::<State>()
            .unwrap()
            .dialog
            .as_ref()
            .unwrap();
        assert_eq!(dlg.target.session, "today");
        let field = dlg.field.unwrap();
        app.world_mut().get_mut::<Label>(field).unwrap().text = "Renamed row".into();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        app.world_mut().resource_mut::<Model>().unwrap().commands = Some(tx);
        assert!(action(app.world_mut(), ROW_CONFIRM));
        let request = rx.try_recv().unwrap();
        assert!(
            matches!(request.command, Command::Rename { session, title } if session == "today" && title == "Renamed row")
        );
        assert_eq!(app.world().resource::<Model>().unwrap().document, document);
        assert_eq!(
            app.world().resource::<Model>().unwrap().chat.session(),
            Some("week")
        );
        let target = RowTarget {
            session: "today".into(),
            authority: "authority-a".into(),
        };
        let anchor = row(app.world(), 0, 0);
        dialog(app.world_mut(), anchor, target.clone(), false);
        assert!(action(app.world_mut(), ROW_CANCEL));
        assert!(rx.try_recv().is_err());
        dialog(app.world_mut(), anchor, target, false);
        app.world_mut()
            .resource_mut::<Model>()
            .unwrap()
            .chat
            .hydrate("week".into(), "other-authority".into(), vec![], None)
            .unwrap();
        assert!(action(app.world_mut(), ROW_CONFIRM));
        assert!(rx.try_recv().is_err());
    }
    #[test]
    fn collapse_search_and_section_order_preserve_routes_and_document() {
        let mut app = fixture();
        let document = app.world().resource::<Model>().unwrap().document;
        assert!(visible(app.world(), row(app.world(), 0, 0)));
        assert!(!app
            .world()
            .resource::<State>()
            .unwrap()
            .collapsed
            .contains(&0));
        assert!(app
            .world()
            .resource::<State>()
            .unwrap()
            .collapsed
            .contains(&3));
        action(app.world_mut(), SECTION);
        app.update();
        assert!(!visible(app.world(), row(app.world(), 0, 0)));
        app.world_mut()
            .resource_mut::<Model>()
            .unwrap()
            .search_query = "conversation".into();
        app.update();
        assert!(visible(app.world(), row(app.world(), 0, 0)));
        assert!(visible(app.world(), row(app.world(), 3, 3)));
        assert!(action(app.world_mut(), SECTION)); // disclosure is inert during search
        assert!(app
            .world()
            .resource::<State>()
            .unwrap()
            .collapsed
            .contains(&0));
        app.world_mut()
            .resource_mut::<Model>()
            .unwrap()
            .search_query
            .clear();
        app.update();
        assert!(!visible(app.world(), row(app.world(), 3, 3)));
        assert_eq!(app.world().resource::<Model>().unwrap().document, document);
        let sections = app.world().resource::<State>().unwrap().sections;
        let parent = app.world().resource::<Ui>().unwrap().sessions;
        assert_eq!(
            &children_of(app.world(), parent)[..4],
            &sections[..4].iter().map(|s| s.unwrap()).collect::<Vec<_>>()
        );
    }
    #[test]
    fn metadata_append_replaces_identity_and_rejects_stale_queries() {
        let mut app = fixture();
        let rows = app.world().resource::<Model>().unwrap().sessions.clone();
        let mut summary = app.world().resource::<State>().unwrap().summaries["today"].clone();
        summary.turns = 99;
        receive(app.world_mut(), vec![summary.clone()], true);
        assert_eq!(app.world().resource::<State>().unwrap().summaries.len(), 4);
        assert_eq!(
            app.world().resource::<State>().unwrap().summaries["today"].turns,
            99
        );
        let generation = app.world().resource::<Model>().unwrap().generation;
        apply_current_reply(
            app.world_mut(),
            Reply {
                generation,
                update: Update::SessionsPage {
                    query: "stale".into(),
                    rows: vec![],
                    summaries: vec![],
                    cursor: None,
                    append: false,
                },
            },
        );
        assert_eq!(app.world().resource::<Model>().unwrap().sessions, rows);
        assert_eq!(app.world().resource::<State>().unwrap().summaries.len(), 4);
    }
    #[test]
    #[ignore = "Chat parity pinned/menu/dialog GPU visual acceptance"]
    fn chat_interaction_visual_acceptance() {
        let mut app = fixture();
        let engine: Arc<dyn TextEngine> = Arc::new(rndr_render::ParleyText::new());
        app.world_mut().insert_resource(engine);
        let mut exporter = rndr_render::Exporter::new().unwrap();
        hydrate(&mut app, "today", "authority-a");
        set_pin(app.world_mut(), "old", true);
        for (width, height, dark, name) in
            [(1200, 850, true, "dark"), (760, 650, true, "dark-narrow"),
             (1200, 850, false, "light"), (760, 650, false, "light-narrow")]
        {
            publish_system_appearance(
                app.world_mut(),
                SystemAppearance {
                    dark: Some(dark),
                    ..Default::default()
                },
            );
            app.world_mut()
                .insert_resource(Viewport::new(width as f32, height as f32));
            for phase in ["pinned", "menu", "rename"] {
                if phase == "menu" {
                    let target = row(app.world(), 0, 0);
                    send(
                        app.world_mut(),
                        SecondaryPress {
                            entity: target,
                            at: [110., 180.],
                            keyboard: true,
                        },
                    );
                } else if phase == "rename" {
                    let menu = app
                        .world()
                        .resource::<State>()
                        .unwrap()
                        .menu
                        .as_ref()
                        .unwrap()
                        .handle;
                    send(
                        app.world_mut(),
                        MenuPicked {
                            menu: menu.root,
                            item: ROW_RENAME,
                        },
                    );
                }
                for _ in 0..10 {
                    rndr_paint::frame(&mut app);
                }
                let (_, image) = rndr_paint::frame_with_snapshot_frame(&mut app, |world, frame| {
                    exporter
                        .render_frame(Some(world), frame, width, height, 1.)
                        .unwrap()
                });
                std::fs::create_dir_all("verification").unwrap();
                image::save_buffer(
                    format!("verification/native-chat-parity-{phase}-{name}.png"),
                    &image.rgba,
                    image.width,
                    image.height,
                    image::ColorType::Rgba8,
                )
                .unwrap();
            }
            assert!(action(app.world_mut(), ROW_CANCEL));
            app.update();
        }
    }
    #[test]
    #[ignore = "Synthetic history grouping GPU visual acceptance"]
    fn chat_sidebar_visual_acceptance() {
        let mut app = fixture();
        let engine: Arc<dyn TextEngine> = Arc::new(rndr_render::ParleyText::new());
        app.world_mut().insert_resource(engine);
        let mut exporter = rndr_render::Exporter::new().unwrap();
        for (width, height, dark, name) in
            [(1200, 850, true, "dark"), (760, 650, false, "light-narrow")]
        {
            publish_system_appearance(
                app.world_mut(),
                SystemAppearance {
                    dark: Some(dark),
                    ..Default::default()
                },
            );
            app.world_mut()
                .insert_resource(Viewport::new(width as f32, height as f32));
            for _ in 0..10 {
                rndr_paint::frame(&mut app);
            }
            let (_, image) = rndr_paint::frame_with_snapshot_frame(&mut app, |world, frame| {
                exporter
                    .render_frame(Some(world), frame, width, height, 1.)
                    .unwrap()
            });
            std::fs::create_dir_all("verification").unwrap();
            image::save_buffer(
                format!("verification/native-history-sidebar-{name}.png"),
                &image.rgba,
                image.width,
                image.height,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
    }
    #[test]
    fn history_uses_local_calendar_days_including_future_and_unknown_dates() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 16).unwrap();
        for (days, expected) in [(-1, 0), (0, 0), (1, 1), (2, 2), (7, 2), (8, 3)] {
            let day = today - chrono::Duration::days(days);
            let timestamp = day
                .and_hms_opt(12, 0, 0)
                .unwrap()
                .and_local_timezone(Local)
                .earliest()
                .unwrap()
                .with_timezone(&Utc);
            assert_eq!(bucket(Some(timestamp), today), expected);
        }
        assert_eq!(bucket(None, today), 3);
    }
}
