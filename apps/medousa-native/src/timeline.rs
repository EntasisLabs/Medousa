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
    image: Option<(String, String)>,
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
}
struct Timeline {
    session: String,
    revision: Option<medousa_app_core::ChatRevision>,
    rows: Vec<Row>,
    following: bool,
    layout_offset: f32,
    samples: u8,
    last_view: Option<[f32; 6]>,
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
struct CopyText(String);
impl Component for CopyText {}

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
        },
    ));
    set_parent(world, root, frame);
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
    set_parent(world, rail, frame);
    root
}

fn record_offset(world: &mut World, root: Entity) -> bool {
    let offset = world.get::<Scroll>(root).map(|s| s.offset[1]);
    if let (Some(offset), Some(state)) = (offset, world.get_mut::<Timeline>(root)) {
        if state.samples > 0 {
            state.samples -= 1;
            if state.samples == 0 {
                state.layout_offset = offset;
            }
        }
    }
    false
}

/// Call each Update after input. Canonical IDs retain rows through insertion and
/// reconciliation. Only nearby rows mount Markdown, bounding history parse cost.
pub fn sync(world: &mut World, root: Entity, snapshot: Option<&ChatSnapshot>) {
    tool_activity::sync(world, snapshot);
    let Some(mut state) = world.remove::<Timeline>(root) else {
        return;
    };
    let rect = world.get::<Rect>(root).copied().unwrap_or_default();
    let scroll = world.get::<Scroll>(root).copied().unwrap_or_default();
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
    // Center in the whole pane, including the 20-point native scroll rail.
    let lane = visual::column_width(rect.w + 20.);
    let inset = ((rect.w + 20. - lane) / 2.).max(0.);
    world.insert(
        root,
        Padding {
            top: visual::INSET,
            bottom: visual::INSET,
            left: inset,
            right: (inset - 20.).max(0.),
        },
    );
    let switched = snapshot.map(|s| s.session_id.as_str()).unwrap_or("") != state.session;
    if switched {
        for row in state.rows.drain(..) {
            despawn(world, row.entity);
        }
        state.session = snapshot.map(|s| s.session_id.clone()).unwrap_or_default();
        state.revision = None;
        state.following = true;
    } else if (scroll.offset[1] - state.layout_offset).abs() > 1.5 {
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
    state.samples = 2;
    state.revision = snapshot.map(|s| s.revision);
    world.insert(root, state);
}

fn sync_row(world: &mut World, row: &mut Row, item: &ChatItem, session: &str) {
    let parts = project(item);
    let mut old: HashMap<_, _> = row.parts.drain(..).map(|p| (p.id.clone(), p)).collect();
    for part in parts {
        let retained = old.remove(&part.id);
        let mounted = match retained {
            Some(mut mounted)
                if mounted.part.details.is_some() == part.details.is_some()
                    && mounted.part.tools.is_empty() == part.tools.is_empty()
                    && mounted.part.image == part.image =>
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
                    } else if part.image.is_none() {
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
                let entity = if let Some((id, label)) = &part.image {
                    crate::media_preview::mount(world, row.entity, session, id, label, false)
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
        image: None,
    }
}
fn detail(id: impl Into<String>, title: String, text: String) -> Part {
    Part {
        id: id.into(),
        text: title,
        details: Some(text),
        tools: vec![],
        image: None,
    }
}
fn tool(run: &ToolRunState) -> Part {
    Part {
        id: format!("tool:{}", run.run_id),
        text: tool_activity::summary(std::slice::from_ref(run)),
        details: None,
        tools: vec![run.clone()],
        image: None,
    }
}
fn artifact_part(artifact: &medousa_app_core::contract::UiArtifact) -> Part {
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
                            parts.push(tool(run));
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
            parts.push(tool(run));
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
            part.image = Some((media.media_id.clone(), label.into()));
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
    for part in parts {
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
