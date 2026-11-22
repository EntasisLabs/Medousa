//! Retained native timeline. App-core owns chronology and visibility; this module
//! owns only projection, viewport mounting, and presentation-only clipboard actions.
use crate::native_style as visual;
use medousa_app_core::contract::{ChatSegment, ToolRunState};
use medousa_app_core::{ChatItem, ChatItemRole, ChatSnapshot, present_chat_items};
use rndr_core::*;
use std::collections::{HashMap, HashSet};
#[path = "tool_activity.rs"]
mod tool_activity;

#[derive(Clone, PartialEq)]
struct Part {
    id: String,
    text: String,
    details: Option<String>,
    tools: Vec<ToolRunState>,
    images: Vec<(String, String)>,
}
pub struct ReadingIdentity {
    pub sender: String,
    pub recipient: String,
}
impl Component for ReadingIdentity {}

/// Returns only mounted rows; offscreen collaboration markers are mounted with
/// their parent when it enters the viewport, not moved to a different turn.
pub fn mounted_row(world: &World, root: Entity, item_id: &str) -> Option<Entity> {
    world
        .get::<Timeline>(root)?
        .rows
        .iter()
        .find(|row| row.id == item_id && row.mounted)
        .map(|row| row.entity)
}

struct MountedPart {
    id: String,
    entity: Entity,
    part: Part,
}
struct Row {
    id: String,
    entity: Entity,
    height: f32,
    mounted: bool,
    parts: Vec<MountedPart>,
    copy: String,
    user: bool,
}
struct Timeline {
    session: String,
    revision: Option<medousa_app_core::ChatRevision>,
    rows: Vec<Row>,
    following: bool,
    layout_offset: f32,
    samples: u8,
    last_view: Option<[f32; 6]>,
    latest: Option<Entity>,
    navigation: Option<Entity>,
    // Stable turn identity, not an estimated pixel offset, during remeasurement.
    navigation_anchor: Option<String>,
}
impl Component for Timeline {}
struct DetailBody {
    text: String,
    title: String,
    dirty: bool,
    view: Option<Entity>,
}
impl Component for DetailBody {}
const COPY: u64 = 0x74696d6501;
const DISCLOSE: u64 = 0x74696d6502;
const LATEST: u64 = 0x74696d6503;
const PREVIOUS_TURN: u64 = 0x74696d6504;
const NEXT_TURN: u64 = 0x74696d6505;
struct NavigationTarget(Entity, u64);
impl Component for NavigationTarget {}
fn navigate(world: &mut World, root: Entity, command: u64) {
    let Some(state) = world.get::<Timeline>(root) else {
        return;
    };
    let scroll = world.get::<Scroll>(root).copied().unwrap_or_default();
    let rect = world.get::<Rect>(root).copied().unwrap_or_default();
    let target = turn_target(state, scroll, rect, command);
    let offset = if command == LATEST {
        Some(f32::MIN)
    } else {
        target.as_ref().map(|(_, y)| -y)
    };
    let Some(offset) = offset else {
        return;
    };
    let state = world.get_mut::<Timeline>(root).unwrap();
    state.following = command == LATEST;
    state.navigation_anchor = target.map(|(id, _)| id);
    state.last_view = None;
    state.layout_offset = offset;
    world.get_mut::<Scroll>(root).unwrap().offset[1] = offset;
    sync_navigation(world, root);
}

fn turn_target(
    state: &Timeline,
    scroll: Scroll,
    rect: Rect,
    command: u64,
) -> Option<(String, f32)> {
    let bottom = (scroll.content[1] - rect.h).max(0.);
    let top = (-scroll.offset[1]).clamp(0., bottom);
    let mut y = visual::INSET;
    let mut positions = Vec::new();
    for row in &state.rows {
        // Last turns can be below the highest reachable scroll offset.
        let reachable = y.min(bottom);
        if row.user
            && ((command == PREVIOUS_TURN && reachable < top - 1.)
                || (command == NEXT_TURN && reachable > top + 1.))
        {
            positions.push((row.id.clone(), reachable));
        }
        y += row.height + visual::ROW_GAP;
    }
    if command == PREVIOUS_TURN {
        positions.pop()
    } else {
        positions.into_iter().next()
    }
}

fn sync_navigation(world: &mut World, root: Entity) {
    let Some(state) = world.get::<Timeline>(root) else {
        return;
    };
    let Some(parent) = state.navigation else {
        return;
    };
    let scroll = world.get::<Scroll>(root).copied().unwrap_or_default();
    let rect = world.get::<Rect>(root).copied().unwrap_or_default();
    let availability = [
        (
            PREVIOUS_TURN,
            "↑",
            "Previous user turn",
            turn_target(state, scroll, rect, PREVIOUS_TURN).is_some(),
        ),
        (
            NEXT_TURN,
            "↓",
            "Next user turn",
            turn_target(state, scroll, rect, NEXT_TURN).is_some(),
        ),
    ];
    // Keep the retained native scroll target and keyboard route, but don't
    // paint a full-height thumb when there is nothing to scroll.
    for child in children_of(world, parent) {
        if world.has::<Scrollbar>(child) {
            let focused = world.resource::<Focus>().and_then(|f| f.entity) == Some(child);
            world.insert(
                child,
                Opacity(if scroll.content[1] > rect.h + 1. || focused {
                    1.
                } else {
                    0.
                }),
            );
        }
    }
    for (command, glyph, name, enabled) in availability {
        let mut props = rndr_ui::controls::ButtonProps::new(
            glyph,
            rndr_ui::controls::HostIntent::Command(command),
        );
        props.name = Some(name.into());
        props.variant = rndr_ui::controls::ButtonVariant::Quiet;
        props.enabled = enabled;
        let unchanged = children_of(world, parent).into_iter().any(|e| {
            world.get::<Button>(e).is_some_and(|b| b.enabled == enabled)
                && world
                    .get::<NavigationTarget>(e)
                    .is_some_and(|n| n.1 == command)
        });
        // Updating through RNDR also cancels pressed gestures and gates focus/activation.
        // Do not reproject unchanged controls from the post-layout callback.
        if !unchanged {
            rndr_ui::controls::Button::mount(world, parent, rndr_ui::UiId(command), props).unwrap();
        }
    }
}
struct CopyText(String);
impl Component for CopyText {}

/// UiPlugin supplies passive indicators to every Scroll viewport each frame.
/// The timeline already has an accessible, draggable Scrollbar in its navigation
/// column. Suppress only its redundant overlay after the toolkit styles run.
pub fn chrome(world: &mut World) {
    for root in world.entities_with::<Timeline>() {
        world.remove::<ScrollIndicator>(root);
        sync_navigation(world, root);
    }
}

/// Requires the host's MarkdownPlugin and native interaction plugins. The only
/// scrollport is this root; Markdown views hug content and retain selection.
pub fn setup(world: &mut World, parent: Entity) -> Entity {
    let mut theme = world
        .resource::<MarkdownTheme>()
        .copied()
        .unwrap_or_else(|| MarkdownTheme::from_theme(&theme_of(world)));
    theme.surface = Color::rgba(0., 0., 0., 0.);
    theme.text = visual::color(world, visual::text());
    world.insert_resource(theme);
    let frame = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::row()
    },));
    set_parent(world, frame, parent);
    let root = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            padding: visual::INSET,
            gap: visual::ROW_GAP,
            ..Layout::column()
        },
        Scroll::default(),
        ViewportRows(record_offset),
        Timeline {
            session: String::new(),
            revision: None,
            rows: vec![],
            following: true,
            layout_offset: 0.,
            samples: 0,
            last_view: None,
            latest: None,
            navigation: None,
            navigation_anchor: None,
        },
    ));
    set_parent(world, root, frame);
    let navigation = world.spawn((Layout {
        width: Size::Px(28.),
        height: Size::Fill,
        gap: 4.,
        ..Layout::column()
    },));
    set_parent(world, navigation, frame);
    world.get_mut::<Timeline>(root).unwrap().navigation = Some(navigation);
    for (command, glyph, name) in [
        (PREVIOUS_TURN, "↑", "Previous user turn"),
        (NEXT_TURN, "↓", "Next user turn"),
        (LATEST, "⇣", "Return to latest response"),
    ] {
        let mut props = rndr_ui::controls::ButtonProps::new(
            glyph,
            rndr_ui::controls::HostIntent::Command(command),
        );
        props.name = Some(name.into());
        props.variant = rndr_ui::controls::ButtonVariant::Quiet;
        let control =
            rndr_ui::controls::Button::mount(world, navigation, rndr_ui::UiId(command), props)
                .unwrap();
        world.insert(control.root, NavigationTarget(root, command));
        world.get_mut::<Layout>(control.root).unwrap().width = Size::Px(28.);
        world.get_mut::<Layout>(control.root).unwrap().height = Size::Px(28.);
        rndr_ui::inputs::Tooltip::attach(world, control.root, name).unwrap();
        if command == LATEST {
            world.get_mut::<Timeline>(root).unwrap().latest = Some(control.root);
            world.insert(control.root, Hidden);
        }
    }
    let rail = world.spawn((
        Layout {
            width: Size::Px(20.),
            height: Size::Fill,
            ..Layout::column()
        },
        Scrollbar {
            target: root,
            axis: Axis::Column,
        },
        Focusable,
        Semantics {
            name: Some("Chat history scroll position".into()),
            ..Default::default()
        },
    ));
    set_parent(world, rail, navigation);
    world.get_mut::<Layout>(rail).unwrap().width = Size::Px(8.);
    world.insert(rail, AlignSelf(Items::End));
    visual::plane(world, rail, visual::canvas());
    sync_navigation(world, root);
    root
}

fn record_offset(world: &mut World, root: Entity) -> bool {
    // RNDR calls this after layout. Read actual mounted heights before resolving
    // the stable navigation anchor; offscreen rows retain their last estimate.
    let mut changed = false;
    if let Some(mut state) = world.remove::<Timeline>(root) {
        let mut y = visual::INSET;
        let mut anchor_y = None;
        let old_offset = world.get::<Scroll>(root).map_or(0., |s| s.offset[1]);
        let mut correction = 0.;
        for row in &mut state.rows {
            if row.mounted {
                if let Some(rect) = world.get::<Rect>(row.entity) {
                    if rect.h > 0. {
                        if y + row.height < -old_offset {
                            correction += rect.h - row.height;
                        }
                        row.height = rect.h;
                    }
                }
            }
            if state.navigation_anchor.as_deref() == Some(row.id.as_str()) {
                anchor_y = Some(y);
            }
            y += row.height + visual::ROW_GAP;
        }
        if let Some(anchor_y) = anchor_y {
            let height = world.get::<Rect>(root).map_or(0., |r| r.h);
            let target = -anchor_y.min((y + visual::INSET - height).max(0.));
            if let Some(scroll) = world.get_mut::<Scroll>(root) {
                if (scroll.offset[1] - target).abs() > 0.5 {
                    scroll.offset[1] = target;
                    state.last_view = None;
                    changed = true;
                }
                state.layout_offset = target;
            }
        } else {
            state.navigation_anchor = None;
            if !state.following && correction.abs() > 0.5 {
                if let Some(scroll) = world.get_mut::<Scroll>(root) {
                    scroll.offset[1] -= correction;
                    state.layout_offset = scroll.offset[1];
                    state.last_view = None;
                    changed = true;
                }
            }
        }
        world.insert(root, state);
    }
    sync_navigation(world, root);
    let offset = world.get::<Scroll>(root).map(|s| s.offset[1]);
    if let (Some(offset), Some(state)) = (offset, world.get_mut::<Timeline>(root)) {
        if state.samples > 0 {
            state.samples -= 1;
            if state.samples == 0 {
                state.layout_offset = offset;
            }
        }
    }
    changed
}

/// Call each Update after input. Canonical IDs retain rows through insertion and
/// reconciliation. Only nearby rows mount Markdown, bounding history parse cost.
pub fn sync(world: &mut World, root: Entity, snapshot: Option<&ChatSnapshot>) {
    tool_activity::sync_scoped(world, root, snapshot);
    let Some(mut state) = world.remove::<Timeline>(root) else {
        return;
    };
    let rect = world.get::<Rect>(root).copied().unwrap_or_default();
    let mut scroll = world.get::<Scroll>(root).copied().unwrap_or_default();
    let view = [
        rect.w,
        rect.h,
        scroll.offset[0],
        scroll.offset[1],
        scroll.content[0],
        scroll.content[1],
    ];
    // Retain a settled viewport. Rebuilding Children and walking every tool on
    // every idle frame invalidates layout and keeps large conversations hot.
    if state.revision == snapshot.map(|s| s.revision)
        && state.session == snapshot.map(|s| s.session_id.as_str()).unwrap_or("")
        && state.last_view == Some(view)
    {
        world.insert(root, state);
        return;
    }
    state.last_view = Some(view);
    // Center in the whole pane, including the native navigation/scroll rail.
    let lane = visual::column_width(rect.w + 28.);
    let inset = ((rect.w + 28. - lane) / 2.).max(0.);
    world.insert(
        root,
        Padding {
            top: visual::INSET,
            bottom: visual::INSET,
            left: inset,
            right: (inset - 28.).max(0.),
        },
    );
    let switched = snapshot.map(|s| s.session_id.as_str()).unwrap_or("") != state.session;
    if switched {
        state.navigation_anchor = None;
        for row in state.rows.drain(..) {
            despawn(world, row.entity);
        }
        state.session = snapshot.map(|s| s.session_id.clone()).unwrap_or_default();
        state.revision = None;
        state.following = world.get::<ReadingIdentity>(root).is_none();
        if !state.following {
            scroll.offset = [0., 0.];
            world.get_mut::<Scroll>(root).unwrap().offset = scroll.offset;
            state.layout_offset = 0.;
        }
    } else if (scroll.offset[1] - state.layout_offset).abs() > 1.5 {
        state.navigation_anchor = None;
        // A user wheel/key/scrollbar movement, not a content-height change.
        let bottom = (scroll.content[1] - rect.h).max(0.);
        state.following = scroll.offset[1] <= -bottom + 1.5;
    }
    let items = snapshot
        .map(|s| present_chat_items(&s.items, &[]))
        .unwrap_or_default();
    let changed = switched || snapshot.map(|s| s.revision) != state.revision;
    let mut old: HashMap<_, _> = state.rows.drain(..).map(|r| (r.id.clone(), r)).collect();
    for item in &items {
        let row = old.remove(&item.id).unwrap_or_else(|| {
            let entity = world.spawn((Layout {
                width: Size::Fill,
                height: Size::Px(160.),
                gap: 8.,
                ..Layout::column()
            },));
            set_parent(world, entity, root);
            Row {
                id: item.id.clone(),
                entity,
                height: 160.,
                mounted: false,
                parts: vec![],
                copy: String::new(),
                user: item.role == ChatItemRole::User && !item.content.trim().is_empty(),
            }
        });
        state.rows.push(row);
    }
    for row in old.into_values() {
        despawn(world, row.entity);
    }
    if let Some(children) = world.get_mut::<Children>(root) {
        children.list = state.rows.iter().map(|r| r.entity).collect();
    }
    // Correct the anchor when previously estimated rows above the viewport gain
    // measured heights. Following uses the layout's normal bottom clamp.
    let mut y = visual::INSET;
    let mut correction = 0.;
    for row in &mut state.rows {
        if row.mounted {
            if let Some(rect) = world.get::<Rect>(row.entity) {
                if rect.h > 0. {
                    if y + row.height < -scroll.offset[1] {
                        correction += rect.h - row.height;
                    }
                    row.height = rect.h;
                }
            }
        }
        y += row.height + visual::ROW_GAP;
    }
    let offset = scroll.offset[1] - correction;
    let top = if state.following {
        (y + visual::INSET - rect.h).max(0.)
    } else {
        -offset
    };
    let overscan = rect.h.max(500.);
    let mut y = visual::INSET;
    for (row, item) in state.rows.iter_mut().zip(items) {
        let visible = y + row.height >= top - overscan && y <= top + rect.h + overscan;
        y += row.height + visual::ROW_GAP;
        if !visible {
            if row.mounted {
                for child in children_of(world, row.entity) {
                    despawn(world, child);
                }
                row.parts.clear();
                row.mounted = false;
                row.copy.clear();
                world.get_mut::<Layout>(row.entity).unwrap().height = Size::Px(row.height);
            }
            continue;
        }
        let fresh = !row.mounted;
        if fresh {
            row.mounted = true;
            world.get_mut::<Layout>(row.entity).unwrap().height = Size::Hug;
        }
        let available = lane;
        let user = item.role == ChatItemRole::User;
        world.get_mut::<Layout>(row.entity).unwrap().width = Size::Px(if user {
            (available * 0.88).min(620.)
        } else {
            available.min(760.)
        });
        world.insert(
            row.entity,
            AlignSelf(if user { Items::End } else { Items::Start }),
        );
        world.insert(row.entity, Padding::all(if user { 12. } else { 0. }));
        world.insert(row.entity, Radius(if user { 12. } else { 0. }));
        world.insert(
            row.entity,
            rndr_ui::UiStyle {
                background: Some(if user {
                    visual::user().into()
                } else {
                    rndr_ui::UiColor::Explicit(Color::rgba(0., 0., 0., 0.))
                }),
                ..Default::default()
            },
        );
        if fresh || changed {
            sync_row(world, row, item, &state.session);
        }
        for part in &row.parts {
            if part.part.details.is_some() {
                sync_detail(world, part.entity);
            }
        }
    }
    if let Some(scroll) = world.get_mut::<Scroll>(root) {
        scroll.offset[1] = if state.following { f32::MIN } else { offset };
    }
    if let Some(latest) = state.latest {
        if state.following || state.rows.is_empty() {
            world.insert(latest, Hidden);
        } else {
            world.remove::<Hidden>(latest);
        }
    }
    state.samples = 2;
    state.revision = snapshot.map(|s| s.revision);
    world.insert(root, state);
    sync_navigation(world, root);
}

fn sync_row(world: &mut World, row: &mut Row, item: &ChatItem, session: &str) {
    let mut parts = project(item);
    if let Some(root) = world.get::<Parent>(row.entity).map(|p| p.entity) {
        if let Some(identity) = world.get::<ReadingIdentity>(root) {
            parts[0].text = literal(match item.role {
                ChatItemRole::User => &identity.sender,
                ChatItemRole::Assistant => &identity.recipient,
                ChatItemRole::System => "Recorded activity",
            });
        }
    }
    let mut old: HashMap<_, _> = row.parts.drain(..).map(|p| (p.id.clone(), p)).collect();
    for part in parts {
        let retained = old.remove(&part.id);
        let mounted = match retained {
            Some(mut mounted)
                if mounted.part.details.is_some() == part.details.is_some()
                    && mounted.part.tools.is_empty() == part.tools.is_empty()
                    && mounted.part.images == part.images =>
            {
                if mounted.part != part {
                    if !part.tools.is_empty() {
                        tool_activity::mount(
                            world,
                            row.entity,
                            Some(mounted.entity),
                            &item.id,
                            &part,
                        );
                    } else if let Some(details) = &part.details {
                        if let Some(body) = world.get_mut::<DetailBody>(mounted.entity) {
                            body.text = details.clone();
                            body.title = part.text.clone();
                            body.dirty = true;
                        }
                    } else if part.images.is_empty() {
                        let view = world.get_mut::<MarkdownView>(mounted.entity).unwrap();
                        if let Some(suffix) = part.text.strip_prefix(view.source()) {
                            view.push_str(suffix);
                        } else {
                            view.set_markdown(&part.text);
                        }
                    }
                    mounted.part = part;
                }
                mounted
            }
            other => {
                if let Some(old) = other {
                    despawn(world, old.entity);
                }
                let entity = if !part.images.is_empty() {
                    crate::media_preview::gallery(world, row.entity, session, &part.images)
                } else if !part.tools.is_empty() {
                    tool_activity::mount(world, row.entity, None, &item.id, &part)
                } else if let Some(details) = &part.details {
                    let node = world.spawn((
                        Layout {
                            width: Size::Fill,
                            height: Size::Hug,
                            gap: 4.,
                            ..Layout::column()
                        },
                        Disclosure::new(vec![]),
                        DetailBody {
                            text: details.clone(),
                            title: part.text.clone(),
                            dirty: true,
                            view: None,
                        },
                    ));
                    timeline_button(world, node, 1, &part.text, DISCLOSE);
                    node
                } else {
                    let view = markdown_view(world, &part.text);
                    world.insert(
                        view,
                        rndr_ui::UiStyle {
                            typography: Some(if part.id == "header" {
                                rndr_ui::Typography::Secondary
                            } else {
                                rndr_ui::Typography::Reading
                            }),
                            ..Default::default()
                        },
                    );
                    world.remove::<Scroll>(view);
                    world.get_mut::<Layout>(view).unwrap().height = Size::Hug;
                    world.get_mut::<Layout>(view).unwrap().padding = 0.;
                    world
                        .get_mut::<MarkdownView>(view)
                        .unwrap()
                        .set_follow(false);
                    view
                };
                set_parent(world, entity, row.entity);
                MountedPart {
                    id: part.id.clone(),
                    entity,
                    part,
                }
            }
        };
        row.parts.push(mounted);
    }
    for old in old.into_values() {
        despawn(world, old.entity);
    }
    // Copy is a stable native button, not an action inferred from message text.
    let copy = children_of(world, row.entity)
        .into_iter()
        .find(|&e| world.has::<CopyText>(e))
        .unwrap_or_else(|| timeline_button(world, row.entity, 2, "Copy message", COPY));
    row.copy = row
        .parts
        .iter()
        .map(|p| match &p.part.details {
            Some(details) => format!("{}\n{details}", p.part.text),
            None => p.part.text.clone(),
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    if item.role == ChatItemRole::User {
        row.copy = item.content.clone();
    }
    world.insert(copy, CopyText(row.copy.clone()));
    if let Some(children) = world.get_mut::<Children>(row.entity) {
        children.list = row
            .parts
            .iter()
            .map(|p| p.entity)
            .chain(std::iter::once(copy))
            .collect();
    }
}

/// Host routes native button intents here before domain actions. Markdown's own
/// text selection/copy remains handled by MarkdownPlugin.
pub fn action(world: &mut World, intent: &ActionIntent) -> bool {
    if tool_activity::action(world, intent) {
        return true;
    }
    let command = rndr_ui::controls::host_intent(world, intent);
    if let Some(rndr_ui::controls::HostIntent::Command(id)) = command {
        if matches!(id, LATEST | PREVIOUS_TURN | NEXT_TURN) {
            if let Some(root) = world.get::<NavigationTarget>(intent.entity).map(|n| n.0) {
                navigate(world, root, id);
                return true;
            }
        }
    }
    if command == Some(rndr_ui::controls::HostIntent::Command(DISCLOSE)) {
        let Some(parent) = world.get::<Parent>(intent.entity).map(|p| p.entity) else {
            return false;
        };
        if !world.has::<DetailBody>(parent) {
            return false;
        }
        toggle_disclosure(world, parent);
        sync_detail(world, parent);
        return true;
    }
    if command != Some(rndr_ui::controls::HostIntent::Command(COPY)) {
        return false;
    }
    let Some(text) = world.get::<CopyText>(intent.entity).map(|t| t.0.clone()) else {
        return false;
    };
    if let Some(clipboard) = world.resource_mut::<Clipboard>() {
        clipboard.text = text;
        clipboard.dirty = true;
    } else {
        world.insert_resource(Clipboard {
            text,
            dirty: true,
            ..Default::default()
        });
    }
    true
}

fn timeline_button(
    world: &mut World,
    parent: Entity,
    id: u64,
    label: &str,
    command: u64,
) -> Entity {
    let mut props = rndr_ui::controls::ButtonProps::new(
        if command == COPY { "Copy" } else { label },
        rndr_ui::controls::HostIntent::Command(command),
    );
    props.name = Some(label.into());
    props.variant = rndr_ui::controls::ButtonVariant::Quiet;
    if command == COPY {
        let h = rndr_ui::controls::IconButton::icon(
            world,
            parent,
            rndr_ui::UiId(id),
            Icon::Copy,
            label,
            props,
        )
        .expect("timeline copy control");
        world.get_mut::<Layout>(h.root).unwrap().width = Size::Px(28.);
        world.get_mut::<Layout>(h.root).unwrap().height = Size::Px(28.);
        world.get_mut::<LabelBox>(h.label).unwrap().inset_x = 0.;
        world.insert(
            h.label,
            rndr_ui::UiStyle {
                foreground: Some(visual::secondary().into()),
                ..Default::default()
            },
        );
        rndr_ui::inputs::Tooltip::attach(world, h.root, label).unwrap();
        h.root
    } else {
        rndr_ui::controls::Button::mount(world, parent, rndr_ui::UiId(id), props)
            .expect("timeline owns a live layout parent and nonempty button label")
            .root
    }
}

// RNDR owns disclosure state and button interaction. Body is a lazily mounted
// native MarkdownView rather than NodeSpec's fixed-height label: long tool
// receipts wrap correctly and participate in native text selection/copy.
fn sync_detail(world: &mut World, entity: Entity) {
    let open = world.get::<Disclosure>(entity).is_some_and(|d| d.open);
    let Some(mut body) = world.remove::<DetailBody>(entity) else {
        return;
    };
    if open && body.view.is_none() {
        let view = markdown_view(world, &literal(&body.text));
        world.remove::<Scroll>(view);
        world.get_mut::<Layout>(view).unwrap().height = Size::Hug;
        world.get_mut::<Layout>(view).unwrap().padding = 0.;
        world
            .get_mut::<MarkdownView>(view)
            .unwrap()
            .set_follow(false);
        set_parent(world, view, entity);
        body.view = Some(view);
        body.dirty = false;
    }
    if let Some(view) = body.view {
        if open {
            world.remove::<Hidden>(view);
            if body.dirty {
                let source = literal(&body.text);
                let markdown = world.get_mut::<MarkdownView>(view).unwrap();
                if markdown.source() != source {
                    if let Some(suffix) = source.strip_prefix(markdown.source()) {
                        markdown.push_str(suffix);
                    } else {
                        markdown.set_markdown(&source);
                    }
                }
                body.dirty = false;
            }
        } else {
            world.insert(view, Hidden);
        }
    }
    let title = format!("{} {}", if open { "▾" } else { "▸" }, body.title);
    timeline_button(world, entity, 1, &title, DISCLOSE);
    world.insert(entity, body);
}

fn literal(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for c in text.chars() {
        if c == '\n' {
            result.push_str("  \n");
        } else {
            if c.is_ascii_punctuation() {
                result.push('\\');
            }
            result.push(c);
        }
    }
    result
}
fn plain(id: impl Into<String>, text: String) -> Part {
    Part {
        id: id.into(),
        text,
        details: None,
        tools: vec![],
        images: vec![],
    }
}
fn detail(id: impl Into<String>, title: String, text: String) -> Part {
    Part {
        id: id.into(),
        text: title,
        details: Some(text),
        tools: vec![],
        images: vec![],
    }
}
fn tool(run: &ToolRunState) -> Part {
    Part {
        id: format!("tool:{}", run.run_id),
        text: tool_activity::summary(std::slice::from_ref(run)),
        details: None,
        tools: vec![run.clone()],
        images: vec![],
    }
}
fn tool_parts(run: &ToolRunState, parts: &mut Vec<Part>, images: &mut HashSet<String>) {
    parts.push(tool(run));
    for (id, label) in output_images(run) {
        if images.insert(id.to_owned()) {
            let mut part = plain(format!("artifact:{id}"), literal(label));
            part.images.push((id.into(), label.into()));
            parts.push(part);
        }
    }
}
fn output_images(run: &ToolRunState) -> impl Iterator<Item = (&str, &str)> {
    run.artifact_refs.iter().flatten().filter_map(|r| {
        let id = r.artifact_id.as_deref()?;
        (run.status == "succeeded"
            && r.role == "output"
            && r.content_type.starts_with("image/")
            && id.starts_with("art:"))
        .then(|| (id, r.label.as_deref().unwrap_or("Medousa image")))
    })
}

// Group successful inspections by assistant item, in tool order. Markdown
// references may caption a thumbnail, but never gate it or duplicate it.
fn present_observations(parts: Vec<Part>) -> Vec<Part> {
    use pulldown_cmark::{Event, Parser, Tag};
    let mut observed = HashMap::new();
    let mut gallery = plain("observations", String::new());
    for run in parts
        .iter()
        .flat_map(|p| &p.tools)
        .filter(|r| r.tool_name == "cognition_image_view")
    {
        for (id, label) in output_images(run) {
            let index = gallery.images.len();
            observed.entry(id.to_owned()).or_insert_with(|| {
                gallery.images.push((id.to_owned(), label.to_owned()));
                index
            });
        }
    }
    if observed.is_empty() {
        return parts;
    }
    let mut work = plain("work", String::new());
    let mut result = vec![];
    let mut shared = HashSet::new();
    for p in parts {
        if !p.tools.is_empty() {
            work.tools.extend(p.tools);
            continue;
        }
        if p.images
            .first()
            .is_some_and(|(id, _)| observed.contains_key(id))
        {
            continue;
        }
        if p.id != "body" && !p.id.starts_with("text:") {
            result.push(p);
            continue;
        }
        let mut cursor = 0;
        for (event, span) in Parser::new(&p.text).into_offset_iter() {
            let Event::Start(Tag::Image { dest_url, .. }) = event else {
                continue;
            };
            let Some(&index) = observed.get(dest_url.as_ref()) else {
                continue;
            };
            if span.start < cursor {
                continue;
            }
            let before = &p.text[cursor..span.start];
            if !before.trim().is_empty() {
                result.push(plain(format!("{}:{cursor}", p.id), before.to_owned()));
            }
            if shared.insert(dest_url.to_string()) {
                if let Some(caption) = p.text[span.clone()]
                    .strip_prefix("![")
                    .and_then(|s| s.split_once("]("))
                    .map(|(s, _)| s)
                    .filter(|s| !s.is_empty())
                {
                    gallery.images[index].1 = caption.to_owned();
                }
            }
            cursor = span.end;
        }
        if cursor == 0 {
            result.push(p);
        } else if !p.text[cursor..].trim().is_empty() {
            result.push(plain(
                format!("{}:{cursor}", p.id),
                p.text[cursor..].to_owned(),
            ));
        }
    }
    work.id = format!("tool:{}", work.tools[0].run_id);
    work.text = if work.tools.iter().any(|r| r.status == "running") {
        tool_activity::summary(&work.tools)
    } else {
        format!(
            "View work · {} image{}",
            observed.len(),
            if observed.len() == 1 { "" } else { "s" }
        )
    };
    result.push(gallery);
    result.push(work);
    result
}
fn artifact_part(artifact: &medousa_app_core::contract::UiArtifact) -> Part {
    if artifact.mime.starts_with("image/") && artifact.artifact_id.starts_with("art:") {
        let mut part = plain(
            format!("artifact:{}", artifact.artifact_id),
            literal(&artifact.label),
        );
        part.images
            .push((artifact.artifact_id.clone(), artifact.label.clone()));
        return part;
    }
    plain(
        format!("artifact:{}", artifact.artifact_id),
        literal(&format!(
            "Artifact: {} · {} · {} (reference; not an embedded preview)",
            artifact.label, artifact.mime, artifact.artifact_id
        )),
    )
}
fn project(item: &ChatItem) -> Vec<Part> {
    let mut parts = vec![];
    let role = match item.role {
        ChatItemRole::User => "You",
        ChatItemRole::Assistant => "Medousa",
        ChatItemRole::System => "System",
    };
    // Identity is useful; provider, phase and raw timestamps on every row
    // compete with the conversation. Keep them in the session/tool inspector.
    parts.push(plain("header", literal(role)));
    if let Some(reasoning) = item.reasoning.as_ref().filter(|s| !s.trim().is_empty()) {
        parts.push(detail("reasoning", "Reasoning".into(), reasoning.clone()));
    }
    let mut runs = HashSet::new();
    let mut artifacts = HashSet::new();
    let mut segment_text = false;
    if item.role != ChatItemRole::User {
        for segment in item.segments.iter().flatten() {
            match segment {
                ChatSegment::Text {
                    segment_id,
                    markdown,
                    ..
                } => {
                    segment_text = true;
                    parts.push(plain(format!("text:{segment_id}"), markdown.clone()));
                }
                ChatSegment::Progress {
                    progress_id,
                    markdown,
                } => parts.push(plain(format!("progress:{progress_id}"), markdown.clone())),
                ChatSegment::ToolGroup { runs: group, .. } => {
                    for run in group {
                        if runs.insert(run.run_id.clone()) {
                            tool_parts(run, &mut parts, &mut artifacts);
                        }
                    }
                }
                ChatSegment::Artifact { artifact } => {
                    if artifacts.insert(artifact.artifact_id.clone()) {
                        parts.push(artifact_part(artifact));
                    }
                }
                ChatSegment::Handoff {
                    handoff_kind,
                    text,
                    work_id,
                } => parts.push(plain(
                    format!(
                        "handoff:{handoff_kind}:{}",
                        work_id.as_deref().unwrap_or("")
                    ),
                    literal(&format!("{handoff_kind}: {text}")),
                )),
            }
        }
    }
    if !segment_text && !item.content.is_empty() {
        parts.push(plain(
            "body",
            if item.role == ChatItemRole::User {
                literal(&item.content)
            } else {
                item.content.clone()
            },
        ));
    }
    for run in item.tool_runs.iter().flatten() {
        if runs.insert(run.run_id.clone()) {
            tool_parts(run, &mut parts, &mut artifacts);
        }
    }
    for artifact in item.ui_artifacts.iter().flatten() {
        if artifacts.insert(artifact.artifact_id.clone()) {
            parts.push(artifact_part(artifact));
        }
    }
    for media in item.media_attachments.iter().flatten() {
        let label = media.label.as_deref().unwrap_or("Attachment");
        let mut part = plain(
            format!("media:{}", media.media_id),
            literal(&format!("{} · {}", label, media.mime)),
        );
        if media.mime.starts_with("image/") {
            part.images.push((media.media_id.clone(), label.into()));
        }
        parts.push(part);
    }
    if let Some(status) =
        medousa_app_core::display::visible_chat_status_line(item.status_line.as_deref())
    {
        parts.push(plain("status", literal(&status)));
    }
    if item.is_streaming() {
        parts.push(plain("streaming", "Receiving response…".into()));
    }
    if item.failed.unwrap_or(false) || item.error_line.is_some() {
        parts.push(plain(
            "error",
            literal(
                item.error_line
                    .as_deref()
                    .unwrap_or("This response failed."),
            ),
        ));
    }
    if let Some(error) = &item.error_detail {
        parts.push(detail(
            "error-detail",
            "Error details".into(),
            error.clone(),
        ));
    }
    if let Some(id) = &item.budget_request_id {
        parts.push(plain(
            "budget",
            literal(&format!("Waiting for budget approval · {id}")),
        ));
    }
    // Coalesce adjacent tool rounds, never across narrative/progress/artifacts.
    // Keep the first canonical run ID so streaming appends retain the control.
    let mut grouped: Vec<Part> = vec![];
    let parts = if item.role == ChatItemRole::Assistant {
        present_observations(parts)
    } else {
        parts
    };
    for part in parts {
        if !part.images.is_empty()
            && let Some(last) = grouped.last_mut().filter(|p| !p.images.is_empty())
        {
            last.images.extend(part.images);
            last.text.push_str("\n");
            last.text.push_str(&part.text);
            continue;
        }
        if !part.tools.is_empty() {
            if let Some(last) = grouped.last_mut().filter(|p| !p.tools.is_empty()) {
                last.tools.extend(part.tools);
                last.text = tool_activity::summary(&last.tools);
                continue;
            }
        }
        grouped.push(part);
    }
    grouped
}

#[cfg(test)]
#[path = "timeline_tests.rs"]
mod tests;

pub fn owning_session(world: &World, mut entity: Entity) -> Option<String> {
    loop {
        if let Some(timeline) = world.get::<Timeline>(entity) {
            return Some(timeline.session.clone());
        }
        entity = world.get::<Parent>(entity)?.entity;
    }
}
