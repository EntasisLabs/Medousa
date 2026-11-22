//! Presentation-only inspector. RNDR owns modal hit isolation, Escape, scrolling,
//! keyboard/button activation and text selection; no network or tool execution.
use super::{Part, literal, project};
use medousa_app_core::{ChatRevision, ChatSnapshot, contract::ToolRunState};
use rndr_core::*;
use rndr_ui::{
    Typography, UiId,
    controls::{self, ButtonProps, ButtonVariant, HostIntent},
    text,
};

const OPEN: u64 = 0x746f6f6c01;
const CLOSE: u64 = 0x746f6f6c02;
#[derive(Clone)]
struct Group {
    item: String,
    part: Part,
}
impl Component for Group {}
struct ActivityChevron;
impl Component for ActivityChevron {}
#[derive(Default)]
struct Inspector {
    session: String,
    revision: Option<ChatRevision>,
    selected: Option<Group>,
    root: Option<Entity>,
    sheet: Option<Entity>,
    body: Option<Entity>,
    subtitle: Option<Entity>,
    return_focus: Option<Entity>,
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
fn subtitle(runs: &[ToolRunState]) -> String {
    let running = runs.iter().filter(|r| r.status == "running").count();
    let failed = runs.iter().filter(|r| r.status == "failed").count();
    if running > 0 {
        format!("{running} running · {} total", runs.len())
    } else if failed > 0 {
        format!("{failed} failed · {} total", runs.len())
    } else {
        format!("{} completed", runs.len())
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
        rndr_ui::UiStyle {
            typography: Some(Typography::Secondary),
            foreground: Some(rndr_ui::Token::Secondary.into()),
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
    if let Some(root) = state.root.take() {
        if world.is_alive(root) {
            despawn(world, root);
        }
    }
    if let Some(focus) = state.return_focus.take().filter(|e| world.is_alive(*e)) {
        if let Some(f) = world.resource_mut::<Focus>() {
            f.entity = Some(focus);
        }
    }
    state.selected = None;
}
pub fn action(world: &mut World, intent: &ActionIntent) -> bool {
    match controls::host_intent(world, intent) {
        Some(HostIntent::Command(OPEN)) => {
            let Some(group) = world.get::<Group>(intent.entity).cloned() else {
                return false;
            };
            let mut state = take(world);
            restore(world, &mut state);
            if let Some(session) = crate::timeline::owning_session(world, intent.entity) {
                state.session = session;
            }
            state.revision = None;
            state.return_focus = Some(intent.entity);
            state.selected = Some(group);
            open(world, &mut state);
            world.insert_resource(state);
            true
        }
        Some(HostIntent::Command(CLOSE)) => {
            let mut state = take(world);
            restore(world, &mut state);
            world.insert_resource(state);
            true
        }
        _ => false,
    }
}
pub fn sync_scoped(world: &mut World, root: Entity, snapshot: Option<&ChatSnapshot>) {
    if let Some(mut entity) = world.resource::<Inspector>().and_then(|s| s.return_focus) {
        let mut belongs = false;
        let mut hidden = false;
        loop {
            hidden |= world.has::<Hidden>(entity);
            belongs |= entity == root;
            let Some(parent) = world.get::<Parent>(entity) else {
                break;
            };
            entity = parent.entity;
        }
        if hidden {
            eprintln!("TOOL SOURCE HIDDEN root={root:?}");
            let mut state = take(world);
            restore(world, &mut state);
            world.insert_resource(state);
            return;
        }
        if !belongs {
            return;
        }
    }
    sync(world, snapshot);
}

pub fn sync(world: &mut World, snapshot: Option<&ChatSnapshot>) {
    let mut state = take(world);
    // Desktop input carries logical key events; core overlay dismissal still
    // reads the legacy `Input.key` shim. Consume Escape only while this sheet
    // owns the modal interaction, preserving normal chat/editor key handling.
    let escape = !crate::media_preview::viewer_open(world)
        && state.selected.is_some()
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
        if state.selected.is_some() {
            eprintln!("TOOL SESSION RESTORE {} vs {}", state.session, session);
        }
        restore(world, &mut state);
    }
    if state.selected.is_some() && state.revision != snapshot.map(|s| s.revision) {
        let selected = state.selected.as_ref().unwrap();
        let updated = snapshot
            .and_then(|s| s.items.iter().find(|i| i.id == selected.item))
            .and_then(|item| {
                project(item)
                    .into_iter()
                    .find(|p| p.id == selected.part.id && !p.tools.is_empty())
            });
        if let Some(part) = updated {
            if part != selected.part {
                state.selected.as_mut().unwrap().part = part;
                render_cards(world, &state);
            }
        } else {
            eprintln!("TOOL MISSING ITEM {} in {}", selected.item, session);
            restore(world, &mut state);
        }
    }
    if let Some(sheet) = state.sheet.filter(|e| world.is_alive(*e)) {
        let v = world.resource::<Viewport>().copied().unwrap_or_default();
        let layout = world.get_mut::<Layout>(sheet).unwrap();
        layout.width = Size::Px((v.width - 32.).clamp(1., 608.));
        layout.height = Size::Px((v.height * 0.8).min(736.).max(1.));
        world.insert(sheet, sheet_position(v));
    }
    state.session = session.into();
    state.revision = snapshot.map(|s| s.revision);
    world.insert_resource(state);
}
fn node(world: &mut World, parent: Entity, layout: Layout) -> Entity {
    let e = world.spawn((layout,));
    set_parent(world, e, parent);
    e
}
fn sheet_position(v: Viewport) -> Absolute {
    Absolute {
        left: Some((v.width - (v.width - 32.).clamp(1., 608.)) / 2.),
        top: Some((v.height - (v.height * 0.8).min(736.).max(1.)) / 2.),
        ..Absolute::default()
    }
}
fn open(world: &mut World, state: &mut Inspector) {
    let root = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            padding: 16.,
            ..Layout::column()
        },
        Align::center(),
        Layer(140),
        Modal,
        Color::rgba(0., 0., 0., 0.72),
        Semantics {
            name: Some("Tool activity".into()),
            ..Default::default()
        },
    ));
    // Backdrop is a sibling, never an ancestor of selectable receipt content.
    let backdrop = node(
        world,
        root,
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            ..Layout::column()
        },
    );
    world.insert(backdrop, Absolute::fill(0.));
    world.insert(backdrop, Button::new("tool-activity-backdrop"));
    world.insert(backdrop, ReleaseActivation);
    world.insert(
        backdrop,
        ActivationHandler(|world, _| {
            let mut state = take(world);
            restore(world, &mut state);
            world.insert_resource(state);
        }),
    );
    let v = world.resource::<Viewport>().copied().unwrap_or_default();
    let sheet = node(
        world,
        root,
        Layout {
            width: Size::Px((v.width - 32.).clamp(1., 608.)),
            height: Size::Px((v.height * 0.8).min(736.)),
            ..Layout::column()
        },
    );
    world.insert(sheet, Color::rgb(0.038, 0.038, 0.043));
    // Both siblings are absolute, so RNDR paints/hit-tests the later sheet
    // above the backdrop. A nested Layer does not change sibling paint order.
    world.insert(sheet, sheet_position(v));
    world.insert(sheet, Radius(21.));
    world.insert(
        sheet,
        Border {
            color: Color::rgb(0.12, 0.12, 0.13),
            width: 1.,
        },
    );
    let header = node(
        world,
        sheet,
        Layout {
            width: Size::Fill,
            padding: 20.,
            gap: 12.,
            ..Layout::row()
        },
    );
    let heading = node(
        world,
        header,
        Layout {
            width: Size::Fill,
            gap: 6.,
            ..Layout::column()
        },
    );
    text(world, heading, "Tool activity", Typography::Emphasis).unwrap();
    state.subtitle = Some(text(world, heading, "", Typography::Secondary).unwrap());
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
    world.insert(
        close.root,
        Semantics {
            name: Some("Close tool activity".into()),
            ..Default::default()
        },
    );
    let line = node(
        world,
        sheet,
        Layout {
            width: Size::Fill,
            height: Size::Px(1.),
            ..Layout::column()
        },
    );
    world.insert(line, Color::rgb(0.09, 0.09, 0.10));
    let frame = node(
        world,
        sheet,
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            ..Layout::row()
        },
    );
    let body = node(
        world,
        frame,
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            padding: 20.,
            gap: 12.,
            ..Layout::column()
        },
    );
    world.insert(body, Scroll::default());
    let rail = node(
        world,
        frame,
        Layout {
            width: Size::Px(10.),
            height: Size::Fill,
            ..Layout::column()
        },
    );
    world.insert(
        rail,
        Scrollbar {
            target: body,
            axis: Axis::Column,
        },
    );
    world.insert(rail, Focusable);
    world.insert(
        rail,
        Semantics {
            name: Some("Tool activity scroll position".into()),
            ..Default::default()
        },
    );
    state.root = Some(root);
    state.sheet = Some(sheet);
    state.body = Some(body);
    render_cards(world, state);
    if let Some(focus) = world.resource_mut::<Focus>() {
        focus.entity = Some(close.root);
    }
}
fn section(world: &mut World, card: Entity, title: &str) {
    let line = node(
        world,
        card,
        Layout {
            width: Size::Fill,
            height: Size::Px(1.),
            ..Layout::column()
        },
    );
    world.insert(line, Color::rgb(0.10, 0.10, 0.11));
    text(world, card, title, Typography::Secondary).unwrap();
}
fn selectable(world: &mut World, parent: Entity, value: &str) {
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
    world.insert(e, theme.typography(Typography::Secondary));
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

fn render_cards(world: &mut World, state: &Inspector) {
    let (Some(body), Some(group)) = (state.body, state.selected.as_ref()) else {
        return;
    };
    if !world.is_alive(body) {
        return;
    }
    let runs = &group.part.tools;
    if let Some(label) = state.subtitle.and_then(|e| world.get_mut::<Label>(e)) {
        label.text = subtitle(runs);
    }
    // Retain the body scrollport; receipt changes never move chat's scroll anchor.
    for child in children_of(world, body) {
        despawn(world, child);
    }
    for (index, run) in runs.iter().enumerate() {
        let card = node(
            world,
            body,
            Layout {
                width: Size::Fill,
                padding: 16.,
                gap: 14.,
                ..Layout::column()
            },
        );
        world.insert(card, Color::rgb(0.065, 0.065, 0.073));
        world.insert(card, Radius(16.));
        world.insert(
            card,
            Border {
                color: if run.status == "failed" {
                    Color::rgb(0.40, 0.15, 0.15)
                } else {
                    Color::rgb(0.12, 0.12, 0.13)
                },
                width: 1.,
            },
        );
        let header = node(
            world,
            card,
            Layout {
                width: Size::Fill,
                gap: 12.,
                ..Layout::row()
            },
        );
        let number = node(
            world,
            header,
            Layout {
                width: Size::Px(28.),
                height: Size::Px(30.),
                ..Layout::column()
            },
        );
        world.insert(number, Color::rgb(0.12, 0.12, 0.13));
        world.insert(number, Radius(8.));
        world.insert(number, Align::center());
        text(
            world,
            number,
            &(index + 1).to_string(),
            Typography::Secondary,
        )
        .unwrap();
        let title = node(
            world,
            header,
            Layout {
                width: Size::Fill,
                gap: 5.,
                ..Layout::column()
            },
        );
        text(world, title, &name(&run.tool_name), Typography::Emphasis).unwrap();
        text(world, title, &run.tool_name, Typography::Code).unwrap();
        let status = match run.status.as_str() {
            "running" => "Running",
            "failed" => "Failed",
            _ => "Done",
        };
        let status_node = node(
            world,
            header,
            Layout {
                width: Size::Px(62.),
                ..Layout::column()
            },
        );
        let mark = match run.status.as_str() {
            "running" => Icon::Loader.scalar(),
            "failed" => Icon::X.scalar(),
            _ => Icon::Check.scalar(),
        };
        text(
            world,
            status_node,
            &format!("{mark} {status}"),
            Typography::Secondary,
        )
        .unwrap();
        if let Some(params) = run.input_params.as_ref().filter(|p| !p.is_empty()) {
            section(world, card, "PARAMETERS");
            for param in params {
                let row = node(
                    world,
                    card,
                    Layout {
                        width: Size::Fill,
                        padding: 10.,
                        gap: 12.,
                        ..Layout::row()
                    },
                );
                world.insert(row, Color::rgb(0.038, 0.038, 0.043));
                world.insert(row, Radius(10.));
                let key = node(
                    world,
                    row,
                    Layout {
                        width: Size::Px(120.),
                        ..Layout::column()
                    },
                );
                text(world, key, &param.key, Typography::Code).unwrap();
                let value = node(
                    world,
                    row,
                    Layout {
                        width: Size::Fill,
                        ..Layout::column()
                    },
                );
                selectable(
                    world,
                    value,
                    &format!("{}{}", param.value, if param.truncated { "…" } else { "" }),
                );
            }
        } else if let Some(request) = run
            .input_summary
            .as_deref()
            .filter(|s| !s.trim().is_empty())
        {
            section(world, card, "REQUEST");
            selectable(world, card, request);
        }
        section(world, card, "RESULT");
        selectable(world, card, result(run));
        if run.tool_name == "cognition_image_view" {
            let images: Vec<_> = super::output_images(run)
                .map(|(id, label)| (id.to_owned(), label.to_owned()))
                .collect();
            if !images.is_empty() {
                crate::media_preview::gallery(world, card, &state.session, &images);
            }
        }
        let receipts = run.artifact_refs.as_ref().map_or(0, Vec::len);
        if receipts > 0 {
            text(
                world,
                card,
                &format!(
                    "{receipts} saved receipt{}",
                    if receipts == 1 { "" } else { "s" }
                ),
                Typography::Secondary,
            )
            .unwrap();
        }
    }
}
