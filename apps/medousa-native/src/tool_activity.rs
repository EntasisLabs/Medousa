//! Presentation-only tool thread. A round's inline summary opens a panel
//! docked beside the conversation (composer included), listing that turn's
//! rounds. Each call reads as what it was for, then what kind of tool did it;
//! a call expands in place to its parameters and result. RNDR owns scrolling,
//! keyboard/button activation and text selection; no network or tool execution.
use super::{COPY, CopyText, Part, Timeline, literal, project};
use crate::native_style as visual;
use medousa_app_core::{ChatRevision, ChatSnapshot, contract::ToolRunState};
use rndr_core::*;
use rndr_ui::{
    Token, Typography, UiId, UiStyle,
    controls::{self, ButtonProps, ButtonVariant, HostIntent, NavigationMeta},
    text,
};
use std::collections::HashSet;

const OPEN: u64 = 0x746f6f6c01;
const CLOSE: u64 = 0x746f6f6c02;
const TOGGLE: u64 = 0x746f6f6c03;
const ROUND: u64 = 0x746f6f6c04;
const JUMP: u64 = 0x746f6f6c05;
const FILTER: u64 = 0x746f6f6c06;
const FULL: u64 = 0x746f6f6c07;
const PANEL_DEFAULT: f32 = 380.;
const PANEL_MIN: f32 = 300.;
const PANEL_MAX: f32 = 640.;
/// The conversation keeps at least this much beside a docked panel; below
/// it the panel floats over the conversation instead of squeezing it.
const CONTENT_MIN: f32 = 420.;
const SASH: f32 = 6.;
/// A result longer than this shows its start and a "Show all".
const RESULT_PREVIEW: usize = 1200;
/// Parameters that say what a call was for; the first present is its title.
const INTENT_KEYS: [&str; 6] = [
    "intent",
    "description",
    "purpose",
    "reason",
    "goal",
    "summary",
];

/// Where a tool panel docks: the row holding a conversation lane. Without
/// one above a timeline, the panel docks in the timeline's own row.
pub struct ToolDock;
impl Component for ToolDock {}

/// A round's inline summary; activating it opens or retargets the panel.
#[derive(Clone)]
struct Group {
    item: String,
    part: Part,
}
impl Component for Group {}
struct ActivityChevron;
impl Component for ActivityChevron {}
/// A call's row; activating it expands or collapses the call.
struct RunToggle(String);
impl Component for RunToggle {}
/// A call's "Show all" for a long result.
struct FullToggle(String);
impl Component for FullToggle {}
/// A round of the open turn, listed as a heading to switch to.
struct RoundTarget(String);
impl Component for RoundTarget {}
/// A filter chip.
struct FilterTarget(Filter);
impl Component for FilterTarget {}
/// The panel's divider, dragged to resize it.
struct ResizeSash;
impl Component for ResizeSash {}

#[derive(Clone, Default, PartialEq, Debug)]
enum Filter {
    #[default]
    All,
    Failed,
}

#[derive(Default)]
struct Inspector {
    session: String,
    revision: Option<ChatRevision>,
    selected: Option<Group>,
    /// Every round of the selected turn, in order.
    rounds: Vec<Part>,
    /// Calls expanded in place, by run id. Several may be open.
    expanded: HashSet<String>,
    /// Calls whose long result is shown in full, by run id.
    full: HashSet<String>,
    filter: Filter,
    /// The panel's width this run; zero until first opened.
    width: f32,
    /// The timeline row this panel belongs to: only that timeline drives it.
    frame: Option<Entity>,
    /// The row the panel is docked into.
    dock: Option<Entity>,
    root: Option<Entity>,
    sash: Option<Entity>,
    stats: Option<Entity>,
    chips: Option<Entity>,
    body: Option<Entity>,
    chip: Option<Entity>,
    /// The newest round when the panel last caught up with the session. The
    /// panel never follows on its own; a newer round shows a chip instead.
    latest_seen: Option<String>,
    newer: Option<Group>,
    return_focus: Option<Entity>,
    highlighted: Option<Entity>,
    dragging: Option<f32>,
    cursor: bool,
}

pub fn name(tool: &str) -> String {
    tool.strip_prefix("cognition_")
        .unwrap_or(tool)
        .replace('_', " ")
}
fn readable(tool: &str) -> String {
    let label = name(tool)
        .replace(['.', ':', '/'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let short: String = label.chars().take(39).collect();
    let label = if label.chars().count() > 40 {
        format!("{short}…")
    } else {
        label
    };
    let mut chars = label.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => label,
    }
}
fn activity(tool: &str) -> Option<String> {
    let lower = tool.to_lowercase();
    let words: Vec<_> = lower
        .split(|c: char| c.is_whitespace() || "._:/-".contains(c))
        .collect();
    let has = |terms: &[&str]| terms.iter().any(|term| words.contains(term));
    if has(&["turn"]) && has(&["begin", "finish"]) {
        return None;
    }
    for (terms, label) in [
        (&["search", "discover", "find"][..], "Search"),
        (
            &["edit", "write", "patch", "create", "delete", "remove"][..],
            "Edit",
        ),
        (
            &["read", "get", "list", "fetch", "open", "stat"][..],
            "Read",
        ),
        (&["test", "tests"][..], "Test"),
        (&["check", "verify", "lint"][..], "Check"),
        (&["build"][..], "Build"),
        (&["spawn", "delegate", "invoke", "ask"][..], "Delegate"),
        (&["exec", "execute", "run", "shell", "command"][..], "Run"),
    ] {
        if has(terms) {
            return Some(label.into());
        }
    }
    Some(readable(tool))
}
pub fn summary(runs: &[ToolRunState]) -> String {
    let context = if let Some(run) = runs.iter().rev().find(|r| r.status == "running") {
        match activity(&run.tool_name).as_deref() {
            Some("Search") => "Searching".into(),
            Some("Read") => "Reading".into(),
            Some("Edit") => "Editing".into(),
            Some("Test") => "Running tests".into(),
            Some("Check") => "Checking".into(),
            Some("Build") => "Building".into(),
            Some("Delegate") => "Delegating".into(),
            Some("Run") => "Running commands".into(),
            Some(label) => format!("Running {label}"),
            None => "Running".into(),
        }
    } else if let Some(run) = runs.iter().rev().find(|r| r.status == "failed") {
        readable(&run.tool_name)
    } else {
        let mut labels = vec![];
        for run in runs {
            if let Some(label) = activity(&run.tool_name) {
                if !labels.contains(&label) {
                    labels.push(label);
                }
            }
        }
        let mut result = labels
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join(" · ");
        if labels.len() > 3 {
            result.push_str(" · …");
        }
        result
    };
    format!(
        "{} tool{}{}{}",
        runs.len(),
        if runs.len() == 1 { "" } else { "s" },
        if context.is_empty() { "" } else { "  " },
        context
    )
}
fn calls(n: usize) -> String {
    format!("{n} call{}", if n == 1 { "" } else { "s" })
}
fn one_line(value: &str, max: usize) -> String {
    let line = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() > max {
        format!("{}…", line.chars().take(max - 1).collect::<String>())
    } else {
        line
    }
}
/// The parameter that says what the call was for, if it has one.
fn intent_param(run: &ToolRunState) -> Option<&str> {
    let params = run.input_params.as_ref()?;
    INTENT_KEYS.iter().find_map(|key| {
        params
            .iter()
            .find(|p| p.key.eq_ignore_ascii_case(key) && !p.value.trim().is_empty())
            .map(|p| p.key.as_str())
    })
}
/// What the call was for: its intent, else its request, else its tool.
fn intent(run: &ToolRunState) -> String {
    let stated = intent_param(run).and_then(|key| {
        run.input_params
            .as_ref()?
            .iter()
            .find(|p| p.key == key)
            .map(|p| p.value.clone())
    });
    let raw = stated
        .or_else(|| run.input_summary.clone().filter(|s| !s.trim().is_empty()))
        .unwrap_or_else(|| readable(&run.tool_name));
    one_line(&raw, 120)
}
/// What kind of tool did it, without the namespace: "Shell", "Read batch".
fn kind(tool: &str) -> String {
    let base = tool.strip_prefix("cognition_").unwrap_or(tool);
    let base = base.strip_prefix("coder_").unwrap_or(base);
    let lower = base.to_lowercase();
    if ["shell", "exec", "command"]
        .iter()
        .any(|t| lower.contains(t))
    {
        return "Shell".into();
    }
    readable(base)
}
fn failed(run: &ToolRunState) -> bool {
    run.status == "failed"
}
fn running(run: &ToolRunState) -> bool {
    run.status == "running"
}
fn shows(filter: &Filter, run: &ToolRunState) -> bool {
    match filter {
        Filter::All => true,
        Filter::Failed => failed(run),
    }
}
/// Chips worth offering for a round: All and Failed, only when something
/// failed. Kind chips were more chrome than help.
fn filters(runs: &[ToolRunState]) -> Vec<(Filter, String)> {
    if runs.iter().any(failed) {
        vec![
            (Filter::All, "All".into()),
            (Filter::Failed, "Failed".into()),
        ]
    } else {
        vec![]
    }
}
fn mark(run: &ToolRunState) -> char {
    match run.status.as_str() {
        "running" => Icon::Loader.scalar(),
        "failed" => Icon::X.scalar(),
        _ => Icon::Check.scalar(),
    }
}
fn button(
    world: &mut World,
    parent: Entity,
    id: u64,
    label: &str,
    command: u64,
) -> controls::ControlHandle {
    let mut props = ButtonProps::new(label, HostIntent::Command(command));
    props.variant = ButtonVariant::Quiet;
    controls::Button::mount(world, parent, UiId(id), props).expect("activity button")
}
fn style(world: &mut World, e: Entity, typography: Typography, foreground: Token) {
    world.insert(
        e,
        UiStyle {
            typography: Some(typography),
            foreground: Some(foreground.into()),
            ..Default::default()
        },
    );
}
fn caption(world: &mut World, parent: Entity, value: &str, ink: Token) -> Entity {
    let e = text(world, parent, value, Typography::Caption).unwrap();
    style(world, e, Typography::Caption, ink);
    e
}
pub fn mount(
    world: &mut World,
    parent: Entity,
    existing: Option<Entity>,
    item: &str,
    part: &Part,
) -> Entity {
    let root = existing.unwrap_or_else(|| {
        let e = world.spawn((Layout {
            width: Size::Fill,
            ..Layout::column()
        },));
        set_parent(world, e, parent);
        e
    });
    let h = button(world, root, OPEN, &part.text, OPEN);
    world.insert(
        h.label,
        UiStyle {
            typography: Some(Typography::Secondary),
            foreground: Some(Token::Secondary.into()),
            ..Default::default()
        },
    );
    // A trailing native icon is independent of the summary's ellipsis width.
    let _chevron = children_of(world, h.root)
        .into_iter()
        .find(|&e| world.has::<ActivityChevron>(e))
        .unwrap_or_else(|| {
            let e = text(
                world,
                h.root,
                &Icon::ChevronRight.scalar().to_string(),
                Typography::Secondary,
            )
            .unwrap();
            world.insert(e, ActivityChevron);
            world.insert(
                e,
                Absolute {
                    right: Some(8.),
                    top: Some(8.),
                    ..Default::default()
                },
            );
            world.get_mut::<Layout>(e).unwrap().width = Size::Px(16.);
            e
        });
    world.get_mut::<Layout>(h.root).unwrap().width = Size::Fill;
    world.get_mut::<Label>(h.label).unwrap().overflow = TextOverflow::Ellipsis;
    world.insert(
        h.root,
        Semantics {
            name: Some(format!("Open tool activity: {}", part.text)),
            ..Default::default()
        },
    );
    world.insert(
        h.root,
        Group {
            item: item.into(),
            part: part.clone(),
        },
    );
    root
}
fn take(world: &mut World) -> Inspector {
    world
        .resource_mut::<Inspector>()
        .map(std::mem::take)
        .unwrap_or_default()
}
fn restore(world: &mut World, state: &mut Inspector) {
    // Focus inside the panel goes back to the summary that opened it.
    let inside = world
        .resource::<Focus>()
        .and_then(|f| f.entity)
        .is_some_and(|f| {
            state
                .root
                .is_some_and(|r| world.is_alive(r) && within(world, f, r))
        });
    for e in [state.root.take(), state.sash.take()].into_iter().flatten() {
        if world.is_alive(e) {
            despawn(world, e);
        }
    }
    if let Some(pill) = state.highlighted.take().filter(|e| world.is_alive(*e)) {
        world.remove::<UiStyle>(pill);
    }
    if state.cursor {
        world.insert_resource(CursorStyle::Arrow);
        state.cursor = false;
    }
    let focus = state.return_focus.take().filter(|e| world.is_alive(*e));
    if let (true, Some(focus), Some(f)) = (inside, focus, world.resource_mut::<Focus>()) {
        f.entity = Some(focus);
    }
    state.selected = None;
    state.rounds.clear();
    state.filter = Filter::All;
    state.newer = None;
    state.latest_seen = None;
    state.frame = None;
    state.dock = None;
    state.stats = None;
    state.chips = None;
    state.body = None;
    state.chip = None;
    state.dragging = None;
}
/// The row the timeline sits in, beside its navigation column.
fn timeline_frame(world: &World, mut entity: Entity) -> Option<Entity> {
    loop {
        if world.has::<Timeline>(entity) {
            return world.get::<Parent>(entity).map(|p| p.entity);
        }
        entity = world.get::<Parent>(entity)?.entity;
    }
}
/// The nearest [`ToolDock`] above the timeline row, else that row.
fn dock_for(world: &World, frame: Entity) -> Entity {
    let mut entity = frame;
    loop {
        if world.has::<ToolDock>(entity) {
            return entity;
        }
        match world.get::<Parent>(entity) {
            Some(p) => entity = p.entity,
            None => return frame,
        }
    }
}
fn within(world: &World, mut entity: Entity, ancestor: Entity) -> bool {
    loop {
        if entity == ancestor {
            return true;
        }
        match world.get::<Parent>(entity) {
            Some(p) => entity = p.entity,
            None => return false,
        }
    }
}
fn retarget(state: &mut Inspector, group: Group) {
    if state.selected.as_ref().is_none_or(|s| s.item != group.item) {
        state.rounds = vec![group.part.clone()];
    }
    state.filter = Filter::All;
    state.selected = Some(group);
    state.revision = None;
}
pub fn action(world: &mut World, intent: &ActionIntent) -> bool {
    let Some(HostIntent::Command(command)) = controls::host_intent(world, intent) else {
        return false;
    };
    let target = intent.entity;
    match command {
        OPEN => {
            let Some(group) = world.get::<Group>(target).cloned() else {
                return false;
            };
            let Some(frame) = timeline_frame(world, target) else {
                return false;
            };
            let mut state = take(world);
            let open_here =
                state.root.is_some_and(|e| world.is_alive(e)) && state.frame == Some(frame);
            let same = state
                .selected
                .as_ref()
                .is_some_and(|s| s.item == group.item && s.part.id == group.part.id);
            if open_here && same {
                // A second click on the open round's summary closes it.
                restore(world, &mut state);
            } else if open_here {
                // Another round retargets the open panel, as another thread does.
                retarget(&mut state, group);
                state.return_focus = Some(target);
                render(world, &mut state);
            } else {
                restore(world, &mut state);
                if let Some(session) = super::owning_session(world, target) {
                    state.session = session;
                }
                retarget(&mut state, group);
                state.return_focus = Some(target);
                state.frame = Some(frame);
                state.dock = Some(dock_for(world, frame));
                open(world, &mut state);
            }
            world.insert_resource(state);
            true
        }
        CLOSE => {
            let mut state = take(world);
            restore(world, &mut state);
            world.insert_resource(state);
            true
        }
        TOGGLE | FULL => {
            let id = if command == TOGGLE {
                world.get::<RunToggle>(target).map(|r| r.0.clone())
            } else {
                world.get::<FullToggle>(target).map(|r| r.0.clone())
            };
            let Some(id) = id else {
                return false;
            };
            let mut state = take(world);
            let set = if command == TOGGLE {
                &mut state.expanded
            } else {
                &mut state.full
            };
            if !set.remove(&id) {
                set.insert(id);
            }
            render(world, &mut state);
            world.insert_resource(state);
            true
        }
        ROUND => {
            let Some(id) = world.get::<RoundTarget>(target).map(|r| r.0.clone()) else {
                return false;
            };
            let mut state = take(world);
            if let Some(part) = state.rounds.iter().find(|p| p.id == id).cloned() {
                if let Some(selected) = state.selected.as_mut() {
                    selected.part = part;
                }
                state.filter = Filter::All;
                render(world, &mut state);
            }
            world.insert_resource(state);
            true
        }
        FILTER => {
            let Some(filter) = world.get::<FilterTarget>(target).map(|f| f.0.clone()) else {
                return false;
            };
            let mut state = take(world);
            state.filter = filter;
            render(world, &mut state);
            world.insert_resource(state);
            true
        }
        JUMP => {
            let mut state = take(world);
            if let Some(newer) = state.newer.take() {
                state.latest_seen = Some(newer.part.id.clone());
                retarget(&mut state, newer);
                render(world, &mut state);
                if let Some(scroll) = state.body.and_then(|b| world.get_mut::<Scroll>(b)) {
                    scroll.offset[1] = 0.;
                }
            }
            world.insert_resource(state);
            true
        }
        _ => false,
    }
}
/// Called by each timeline every frame. Only the timeline the panel was
/// opened from drives it; a hidden tab keeps its panel for when it returns.
pub fn sync_scoped(world: &mut World, root: Entity, snapshot: Option<&ChatSnapshot>) {
    let frame = world.get::<Parent>(root).map(|p| p.entity);
    let docked = world.resource::<Inspector>().and_then(|s| s.frame);
    if docked.is_some() && docked != frame {
        return;
    }
    sync(world, snapshot);
}

/// The newest round in the session.
fn latest(snapshot: &ChatSnapshot) -> Option<Group> {
    snapshot.items.iter().rev().find_map(|item| {
        project(item)
            .into_iter()
            .rev()
            .find(|p| !p.tools.is_empty())
            .map(|part| Group {
                item: item.id.clone(),
                part,
            })
    })
}

pub fn sync(world: &mut World, snapshot: Option<&ChatSnapshot>) {
    let mut state = take(world);
    // Escape closes the panel while focus is in it or on the summary that
    // opened it; elsewhere chat and editors keep the key.
    let focus = world.resource::<Focus>().and_then(|f| f.entity);
    let focused = focus.is_some_and(|f| {
        state
            .root
            .is_some_and(|r| world.is_alive(r) && within(world, f, r))
            || state.return_focus == Some(f)
    });
    let escape = !crate::media_preview::viewer_open(world)
        && state.selected.is_some()
        && focused
        && world.resource::<Input>().is_some_and(|i| {
            i.keys.iter().any(|k| {
                !k.released && !k.synthetic && k.logical == LogicalKey::Named(NamedKey::Escape)
            })
        });
    if escape {
        restore(world, &mut state);
        if let Some(input) = world.resource_mut::<Input>() {
            input
                .keys
                .retain(|k| k.logical != LogicalKey::Named(NamedKey::Escape));
            input.key = None;
        }
    }
    let session = snapshot.map(|s| s.session_id.as_str()).unwrap_or("");
    if state.session != session || state.root.is_some_and(|e| !world.is_alive(e)) {
        restore(world, &mut state);
    }
    if let Some(snapshot) =
        snapshot.filter(|s| state.selected.is_some() && state.revision != Some(s.revision))
    {
        reconcile(world, &mut state, snapshot);
    }
    if state.selected.is_some() {
        place(world, &mut state);
        resize(world, &mut state);
        highlight(world, &mut state);
    }
    state.session = session.into();
    state.revision = snapshot.map(|s| s.revision);
    world.insert_resource(state);
}

/// Bring the open turn's rounds up to date, and note a newer round.
fn reconcile(world: &mut World, state: &mut Inspector, snapshot: &ChatSnapshot) {
    let selected = state.selected.clone().unwrap();
    let rounds: Vec<Part> = snapshot
        .items
        .iter()
        .find(|i| i.id == selected.item)
        .map(|item| {
            project(item)
                .into_iter()
                .filter(|p| !p.tools.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let Some(part) = rounds.iter().find(|p| p.id == selected.part.id).cloned() else {
        restore(world, state);
        return;
    };
    let newest = latest(snapshot);
    let newest_id = newest.as_ref().map(|g| g.part.id.clone());
    if state.latest_seen.is_none() || newest_id.as_ref() == Some(&part.id) {
        state.latest_seen = newest_id.clone();
    }
    let newer = newest.filter(|g| Some(&g.part.id) != state.latest_seen.as_ref());
    let changed = rounds != state.rounds
        || part != selected.part
        || newer.as_ref().map(|g| &g.part) != state.newer.as_ref().map(|g| &g.part);
    state.rounds = rounds;
    state.newer = newer;
    if changed {
        state.selected.as_mut().unwrap().part = part;
        render(world, state);
    }
}

fn node(world: &mut World, parent: Entity, layout: Layout) -> Entity {
    let e = world.spawn((layout,));
    set_parent(world, e, parent);
    e
}
fn hairline(world: &mut World, parent: Entity) {
    let line = node(
        world,
        parent,
        Layout {
            width: Size::Fill,
            height: Size::Px(1.),
            ..Layout::column()
        },
    );
    let stroke = visual::color(world, visual::stroke());
    world.insert(line, stroke);
}
fn open(world: &mut World, state: &mut Inspector) {
    if state.width <= 0. {
        state.width = PANEL_DEFAULT;
    }
    let Some(dock) = state.dock else {
        return;
    };
    let (mut bar, _) = splitter();
    bar.width = Size::Px(SASH);
    bar.height = Size::Fill;
    let sash = world.spawn((
        bar,
        ResizeSash,
        Align {
            items: Items::Center,
            justify: Justify::Center,
        },
        Semantics {
            name: Some("Tool activity width".into()),
            ..Default::default()
        },
    ));
    set_parent(world, sash, dock);
    let line = node(
        world,
        sash,
        Layout {
            width: Size::Px(1.),
            height: Size::Fill,
            ..Layout::column()
        },
    );
    let stroke = visual::color(world, visual::stroke());
    world.insert(line, stroke);
    let root = node(
        world,
        dock,
        Layout {
            width: Size::Px(state.width),
            height: Size::Fill,
            ..Layout::column()
        },
    );
    // Opaque, as the sidebars: it also floats over the conversation.
    visual::plane(world, root, visual::navigation());
    world.insert(root, Radius(12.));
    world.insert(
        root,
        Semantics {
            name: Some("Tool activity".into()),
            ..Default::default()
        },
    );
    let top = node(
        world,
        root,
        Layout {
            width: Size::Fill,
            padding: 16.,
            gap: 10.,
            ..Layout::column()
        },
    );
    let header = node(
        world,
        top,
        Layout {
            width: Size::Fill,
            gap: 8.,
            ..Layout::row()
        },
    );
    world.insert(
        header,
        Align {
            items: Items::Center,
            ..Default::default()
        },
    );
    let title = text(world, header, "Tool activity", Typography::Emphasis).unwrap();
    world.get_mut::<Layout>(title).unwrap().width = Size::Fill;
    let mut close_props = ButtonProps::new("Close tool activity", HostIntent::Command(CLOSE));
    close_props.variant = ButtonVariant::Quiet;
    let close = controls::IconButton::icon(
        world,
        header,
        UiId(CLOSE),
        Icon::X,
        "Close tool activity",
        close_props,
    )
    .unwrap();
    world.get_mut::<Layout>(close.root).unwrap().width = Size::Px(28.);
    let stats = node(
        world,
        top,
        Layout {
            width: Size::Fill,
            gap: 10.,
            ..Layout::row()
        },
    );
    let chips = node(
        world,
        top,
        Layout {
            width: Size::Fill,
            gap: 4.,
            ..Layout::row()
        },
    );
    hairline(world, root);
    // One scrollport; RNDR's indicator is its only scrollbar, as in the
    // sidebars. A second, draggable rail beside it read as a double edge.
    let body = node(
        world,
        root,
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            padding: 8.,
            gap: 2.,
            ..Layout::column()
        },
    );
    world.insert(body, Scroll::default());
    let chip = node(
        world,
        root,
        Layout {
            width: Size::Fill,
            padding: 12.,
            ..Layout::column()
        },
    );
    state.root = Some(root);
    state.sash = Some(sash);
    state.stats = Some(stats);
    state.chips = Some(chips);
    state.body = Some(body);
    state.chip = Some(chip);
    render(world, state);
    place(world, state);
}
/// Selectable text for parameters and results.
fn selectable(world: &mut World, parent: Entity, value: &str, typography: Typography) {
    let e = markdown_view(world, &literal(value));
    world.remove::<Scroll>(e);
    let layout = world.get_mut::<Layout>(e).unwrap();
    layout.height = Size::Hug;
    layout.padding = 0.;
    world.get_mut::<MarkdownView>(e).unwrap().set_follow(false);
    let theme = world
        .resource::<rndr_ui::UiTheme>()
        .cloned()
        .unwrap_or_default();
    world.insert(e, theme.typography(typography));
    set_parent(world, e, parent);
}
fn result(run: &ToolRunState) -> &str {
    run.output_summary
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(match run.status.as_str() {
            "running" => "Waiting for the tool to finish…",
            "failed" => "The tool did not return a result summary.",
            _ => "Completed without a result summary.",
        })
}
#[cfg(test)]
#[path = "tool_activity_tests.rs"]
mod tests;

/// Dock beside the conversation, or float over it when the tab is too
/// narrow to keep both readable. The width the user chose is kept either way.
fn place(world: &mut World, state: &mut Inspector) {
    let (Some(root), Some(sash), Some(dock)) = (state.root, state.sash, state.dock) else {
        return;
    };
    if !world.is_alive(root) {
        return;
    }
    let available = world.get::<Rect>(dock).map_or(0., |r| r.w);
    let floating = available > 0. && available - SASH - state.width < CONTENT_MIN;
    let width = if floating {
        state
            .width
            .min(available - 24.)
            .max(PANEL_MIN.min(available))
    } else {
        state.width
    };
    // Write only a change, so a settled panel does not dirty layout each frame.
    if world
        .get::<Layout>(root)
        .is_some_and(|l| l.width != Size::Px(width))
    {
        world.get_mut::<Layout>(root).unwrap().width = Size::Px(width);
    }
    if floating {
        if !world.has::<Absolute>(root) {
            world.insert(
                root,
                Absolute {
                    right: Some(0.),
                    top: Some(0.),
                    bottom: Some(0.),
                    ..Default::default()
                },
            );
            world.insert(root, Layer(30));
            world.insert(sash, Hidden);
        }
    } else if world.has::<Absolute>(root) {
        world.remove::<Absolute>(root);
        world.remove::<Layer>(root);
        world.remove::<Hidden>(sash);
    }
}
/// Drag the divider to widen or narrow the panel.
fn resize(world: &mut World, state: &mut Inspector) {
    let (Some(sash), Some(input)) = (state.sash, world.resource::<Input>().cloned()) else {
        return;
    };
    let pointer = input.pointer;
    let over = !world.has::<Hidden>(sash)
        && hit_test(world, pointer.position[0], pointer.position[1])
            .and_then(|hit| ancestor_with::<ResizeSash>(world, hit))
            == Some(sash);
    if !pointer.down {
        state.dragging = None;
    } else if pointer.pressed && over {
        state.dragging = Some(pointer.position[0]);
        if let Some(input) = world.resource_mut::<Input>() {
            input.pointer.pressed = false;
        }
    }
    if let Some(last) = state.dragging {
        let available = state
            .dock
            .and_then(|d| world.get::<Rect>(d))
            .map_or(f32::MAX, |r| r.w - SASH - CONTENT_MIN);
        let width = (state.width - (pointer.position[0] - last))
            .min(available)
            .clamp(PANEL_MIN, PANEL_MAX);
        state.width = width;
        state.dragging = Some(pointer.position[0]);
        place(world, state);
    }
    if over || state.dragging.is_some() {
        world.insert_resource(CursorStyle::ResizeColumn);
        world.insert_resource(MotionWatch { active: true });
        state.cursor = true;
    } else if state.cursor {
        world.insert_resource(CursorStyle::Arrow);
        state.cursor = false;
    }
}
/// Mark the summary whose round is open, so you can see where you are.
fn highlight(world: &mut World, state: &mut Inspector) {
    let Some(selected) = state.selected.as_ref() else {
        return;
    };
    let target = world.entities_with::<Group>().into_iter().find(|&e| {
        world
            .get::<Group>(e)
            .is_some_and(|g| g.item == selected.item && g.part.id == selected.part.id)
    });
    if target == state.highlighted {
        return;
    }
    if let Some(old) = state.highlighted.filter(|e| world.is_alive(*e)) {
        world.remove::<UiStyle>(old);
    }
    if let Some(pill) = target {
        world.insert(
            pill,
            UiStyle {
                background: Some(Token::Raised.into()),
                ..Default::default()
            },
        );
    }
    state.highlighted = target;
}

fn clear(world: &mut World, parent: Option<Entity>) -> Option<Entity> {
    let parent = parent.filter(|e| world.is_alive(*e))?;
    for child in children_of(world, parent) {
        despawn(world, child);
    }
    Some(parent)
}
fn render(world: &mut World, state: &mut Inspector) {
    let Some(group) = state.selected.clone() else {
        return;
    };
    let runs = &group.part.tools;
    let rounds = if state.rounds.is_empty() {
        vec![group.part.clone()]
    } else {
        state.rounds.clone()
    };
    let index = rounds
        .iter()
        .position(|p| p.id == group.part.id)
        .unwrap_or(0);
    // Status: which round, how many calls, and how they went.
    if let Some(stats) = clear(world, state.stats) {
        let label = caption(
            world,
            stats,
            &format!("Round {} · {}", index + 1, calls(runs.len())),
            Token::Secondary,
        );
        world.get_mut::<Layout>(label).unwrap().width = Size::Fill;
        let done = runs.iter().filter(|r| !failed(r) && !running(r)).count();
        let busy = runs.iter().filter(|r| running(r)).count();
        let broke = runs.iter().filter(|r| failed(r)).count();
        for (count, icon, ink) in [
            (busy, Icon::Loader, Token::Secondary),
            (done, Icon::Check, Token::HueGreen),
            (broke, Icon::X, Token::HueRed),
        ] {
            if count > 0 {
                let e = caption(world, stats, &format!("{} {count}", icon.scalar()), ink);
                world.get_mut::<Layout>(e).unwrap().width = Size::Hug;
            }
        }
    }
    // Filters narrow the open round; a filter that no longer applies resets.
    let chips = filters(runs);
    if !chips.iter().any(|(f, _)| *f == state.filter) {
        state.filter = Filter::All;
    }
    if let Some(row) = clear(world, state.chips) {
        for (i, (filter, label)) in chips.iter().enumerate() {
            let h = button(world, row, FILTER + ((i as u64 + 1) << 40), label, FILTER);
            world.insert(h.root, FilterTarget(filter.clone()));
            world.insert(
                h.root,
                visual::FitLabel {
                    label: h.label,
                    extra: 8.,
                    min: 36.,
                    max: 140.,
                },
            );
            world.insert(h.root, Radius(visual::CONTROL_RADIUS));
            let on = *filter == state.filter;
            if on {
                world.insert(
                    h.root,
                    UiStyle {
                        background: Some(Token::Raised.into()),
                        ..Default::default()
                    },
                );
            }
            style(
                world,
                h.label,
                Typography::Secondary,
                if on { Token::Text } else { Token::Secondary },
            );
        }
        if chips.is_empty() {
            world.insert(row, Hidden);
        } else {
            world.remove::<Hidden>(row);
        }
    }
    let Some(body) = clear(world, state.body) else {
        return;
    };
    let headed = rounds.len() > 1;
    for (i, round) in rounds.iter().enumerate() {
        let open = round.id == group.part.id;
        if headed {
            round_heading(world, body, i, round, open);
        }
        if !open {
            continue;
        }
        let shown: Vec<_> = round
            .tools
            .iter()
            .filter(|r| shows(&state.filter, r))
            .collect();
        let mut kinds: Vec<String> = round.tools.iter().map(|r| kind(&r.tool_name)).collect();
        kinds.dedup();
        let mixed = kinds.len() > 1;
        for run in &shown {
            render_run(world, body, run, state, mixed);
        }
        if shown.is_empty() {
            let none = caption(world, body, "No calls match this filter.", Token::Tertiary);
            world.insert(
                none,
                Padding {
                    left: 12.,
                    top: 8.,
                    right: 12.,
                    bottom: 8.,
                },
            );
        }
    }
    let Some(chip) = clear(world, state.chip) else {
        return;
    };
    if let Some(newer) = &state.newer {
        let h = button(
            world,
            chip,
            JUMP,
            &format!("New round · {}", calls(newer.part.tools.len())),
            JUMP,
        );
        world.get_mut::<Layout>(h.root).unwrap().width = Size::Fill;
        world.insert(h.root, Radius(visual::CONTROL_RADIUS));
        world.insert(
            h.root,
            UiStyle {
                background: Some(Token::Raised.into()),
                ..Default::default()
            },
        );
        world.remove::<Hidden>(chip);
    } else {
        world.insert(chip, Hidden);
    }
}

/// A round of the open turn: "Round 2   48 calls", with a disclosure.
fn round_heading(world: &mut World, body: Entity, i: usize, round: &Part, open: bool) {
    let broke = round.tools.iter().filter(|r| failed(r)).count();
    let meta = if broke > 0 {
        format!("{} · {broke} failed", calls(round.tools.len()))
    } else {
        calls(round.tools.len())
    };
    let mut props = ButtonProps::new(format!("Round {}", i + 1), HostIntent::Command(ROUND));
    props.variant = ButtonVariant::Quiet;
    let h = controls::NavigationRow::mount(
        world,
        body,
        UiId(ROUND + ((i as u64 + 1) << 40)),
        props,
        NavigationMeta {
            secondary: Some(meta.clone()),
            ..Default::default()
        },
    )
    .unwrap();
    *world.get_mut::<Layout>(h.root).unwrap() = Layout {
        width: Size::Fill,
        height: Size::Px(visual::ROW_HEIGHT),
        padding: 4.,
        ..Layout::row()
    };
    world.insert(h.root, Radius(visual::CONTROL_RADIUS));
    world.insert(h.root, RoundTarget(round.id.clone()));
    style(
        world,
        h.label,
        Typography::Emphasis,
        if open { Token::Text } else { Token::Secondary },
    );
    visual::trailing_meta(world, h);
    if let Some(face) = visual::label_face(world, h.root) {
        visual::disclosure(world, h.root, face, open);
    }
    world.insert(
        h.root,
        Semantics {
            name: Some(format!("Round {}, {meta}", i + 1)),
            description: if open { "Expanded" } else { "Collapsed" }.into(),
            ..Default::default()
        },
    );
}

/// One call: what it was for over what kind of tool did it. Expanded, its
/// parameters and result follow in place.
fn render_run(world: &mut World, body: Entity, run: &ToolRunState, state: &Inspector, mixed: bool) {
    let expanded = state.expanded.contains(&run.run_id);
    let card = node(
        world,
        body,
        Layout {
            width: Size::Fill,
            ..Layout::column()
        },
    );
    let title = intent(run);
    let mut props = ButtonProps::new(title.clone(), HostIntent::Command(TOGGLE));
    props.variant = ButtonVariant::Quiet;
    props.name = Some(format!(
        "{} {title}",
        if expanded { "Collapse" } else { "Expand" }
    ));
    // One line: what the call was for. The tool's kind trails it only when
    // the round mixes kinds; a column of identical "Shell" says nothing.
    let h = controls::NavigationRow::mount(
        world,
        card,
        UiId(TOGGLE),
        props,
        NavigationMeta {
            secondary: mixed.then(|| kind(&run.tool_name)),
            ..Default::default()
        },
    )
    .unwrap();
    *world.get_mut::<Layout>(h.root).unwrap() = Layout {
        width: Size::Fill,
        height: Size::Px(visual::ROW_HEIGHT),
        ..Layout::row()
    };
    world.insert(
        h.root,
        Padding {
            left: 22.,
            right: 6.,
            top: 0.,
            bottom: 0.,
        },
    );
    world.insert(h.root, Radius(visual::CONTROL_RADIUS));
    world.insert(h.root, controls::QuietSelection);
    world.insert(h.root, RunToggle(run.run_id.clone()));
    if mixed {
        visual::trailing_meta(world, h);
    }
    let ink = if failed(run) {
        Token::HueRed
    } else {
        Token::Text
    };
    style(
        world,
        h.label,
        if expanded {
            Typography::Emphasis
        } else {
            Typography::Label
        },
        ink,
    );
    // Only a call that needs attention carries a mark.
    if failed(run) || running(run) {
        let status = caption(
            world,
            h.root,
            &mark(run).to_string(),
            if failed(run) {
                Token::HueRed
            } else {
                Token::Tertiary
            },
        );
        world.insert(
            status,
            Absolute {
                left: Some(6.),
                top: Some(9.),
                ..Default::default()
            },
        );
    }
    if !expanded {
        return;
    }
    let detail = node(
        world,
        card,
        Layout {
            width: Size::Fill,
            gap: 8.,
            ..Layout::column()
        },
    );
    world.insert(
        detail,
        Padding {
            left: 30.,
            right: 12.,
            top: 6.,
            bottom: 12.,
        },
    );
    // The intent is already the title; the rest are the call's parameters.
    let skip = intent_param(run);
    let params: Vec<_> = run
        .input_params
        .iter()
        .flatten()
        .filter(|p| Some(p.key.as_str()) != skip)
        .collect();
    if !params.is_empty() {
        for param in params {
            let row = node(
                world,
                detail,
                Layout {
                    width: Size::Fill,
                    gap: 2.,
                    ..Layout::column()
                },
            );
            caption(world, row, &param.key, Token::Tertiary);
            selectable(
                world,
                row,
                &format!("{}{}", param.value, if param.truncated { "…" } else { "" }),
                Typography::Secondary,
            );
        }
    } else if let Some(request) = run
        .input_summary
        .as_deref()
        .filter(|s| !s.trim().is_empty() && one_line(s, 120) != title)
    {
        caption(world, detail, "request", Token::Tertiary);
        selectable(world, detail, request, Typography::Secondary);
    }
    let heading = node(
        world,
        detail,
        Layout {
            width: Size::Fill,
            gap: 4.,
            ..Layout::row()
        },
    );
    world.insert(
        heading,
        Align {
            items: Items::Center,
            ..Default::default()
        },
    );
    let label = caption(world, heading, "result", Token::Tertiary);
    world.get_mut::<Layout>(label).unwrap().width = Size::Fill;
    let output = result(run).to_string();
    let mut copy_props = ButtonProps::new("Copy result", HostIntent::Command(COPY));
    copy_props.variant = ButtonVariant::Quiet;
    let copy = controls::IconButton::icon(
        world,
        heading,
        UiId(COPY),
        Icon::Copy,
        "Copy result",
        copy_props,
    )
    .unwrap();
    world.get_mut::<Layout>(copy.root).unwrap().width = Size::Px(24.);
    world.get_mut::<Layout>(copy.root).unwrap().height = Size::Px(24.);
    world.insert(copy.root, CopyText(output.clone()));
    let block = node(
        world,
        detail,
        Layout {
            width: Size::Fill,
            padding: 8.,
            ..Layout::column()
        },
    );
    world.insert(
        block,
        UiStyle {
            background: Some(Token::CodeFill.into()),
            ..Default::default()
        },
    );
    world.insert(block, Radius(visual::CONTROL_RADIUS));
    let long = output.chars().count() > RESULT_PREVIEW;
    let full = state.full.contains(&run.run_id);
    let shown = if long && !full {
        format!(
            "{}…",
            output.chars().take(RESULT_PREVIEW).collect::<String>()
        )
    } else {
        output.clone()
    };
    selectable(world, block, &shown, Typography::Code);
    if long {
        let more = button(
            world,
            detail,
            FULL,
            if full { "Show less" } else { "Show all" },
            FULL,
        );
        world.insert(
            more.root,
            visual::FitLabel {
                label: more.label,
                extra: 8.,
                min: 48.,
                max: 120.,
            },
        );
        world.insert(more.root, FullToggle(run.run_id.clone()));
    }
    if run.tool_name == "cognition_image_view" {
        let images: Vec<_> = super::output_images(run)
            .map(|(id, label)| (id.to_owned(), label.to_owned()))
            .collect();
        if !images.is_empty() {
            crate::media_preview::gallery(world, detail, &state.session, &images);
        }
    }
    let receipts = run.artifact_refs.as_ref().map_or(0, Vec::len);
    let mut foot = run.tool_name.clone();
    if receipts > 0 {
        foot.push_str(&format!(
            " · {receipts} saved receipt{}",
            if receipts == 1 { "" } else { "s" }
        ));
    }
    caption(world, detail, &foot, Token::Tertiary);
}
