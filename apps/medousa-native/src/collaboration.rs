//! Conversation-local collaboration reading. Durable worker records own identity,
//! placement and lifecycle; opening an exchange never navigates the parent Chat.
use super::*;
#[cfg(test)]
#[path = "collaboration_tests.rs"]
mod tests;
use medousa_app_core::{ChatItemRole, ChatSnapshot};
use medousa_types::{WorkBoardColumn, WorkCardDetail, WorkCardKind};
use std::{
    hash::{Hash, Hasher},
    time::{Duration, Instant},
};

const OPEN: u64 = 0x636f6c01;
const BACK: u64 = 0x636f6c02;
const RETRY: u64 = 0x636f6c03;
const EARLIER: u64 = 0x636f6c04;
#[derive(Default)]
pub(super) struct State {
    session: String,
    rows: Vec<WorkCardDetail>,
    selected: Option<String>,
    inflight: bool,
    refreshed: Option<Instant>,
    error: Option<String>,
    return_focus: Option<Entity>,
    projection: Option<ChatSnapshot>,
    earlier_open: bool,
    force_selected: bool,
}
#[derive(Clone, Copy)]
pub(super) struct View {
    root: Entity,
    title: Entity,
    status: Entity,
    timeline: Entity,
    earlier: Entity,
    earlier_rows: Entity,
    earlier_label: Entity,
    back: Entity,
}
impl State {
    pub(super) fn is_open(&self) -> bool {
        self.selected.is_some()
    }
}
struct MarkerStamp(String);
impl Component for MarkerStamp {}
struct Target(String);
impl Component for Target {}

fn work_id(row: &WorkCardDetail) -> &str {
    row.work_id
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or(&row.card.id.0)
}
fn lifecycle(row: &WorkCardDetail) -> &str {
    match row.card.column {
        WorkBoardColumn::WrappingUp => "Result ready · Awaiting Medousa summary",
        WorkBoardColumn::Blocked => "Needs attention",
        WorkBoardColumn::Done if row.terminal => "Result recorded",
        WorkBoardColumn::InFlight => "Working",
        WorkBoardColumn::Backlog => "Queued",
        _ => "Status unavailable",
    }
}
fn title(row: &WorkCardDetail) -> &str {
    row.user_ack
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(&row.card.title)
}
fn visible(world: &mut World, entity: Entity, show: bool) {
    if world.has::<Hidden>(entity) != show {
        return;
    }
    if show {
        world.remove::<Hidden>(entity);
    } else {
        world.insert(entity, Hidden);
    }
}
fn set_label(world: &mut World, entity: Entity, text: String) {
    if world
        .get::<Label>(entity)
        .is_some_and(|label| label.text != text)
    {
        world.get_mut::<Label>(entity).unwrap().text = text;
    }
}
fn label(world: &mut World, parent: Entity, text: &str) -> Entity {
    let e = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            ..Layout::row()
        },
        Label::new(text),
    ));
    world.insert(
        e,
        UiStyle {
            typography: Some(Typography::Secondary),
            foreground: Some(visual::secondary().into()),
            ..Default::default()
        },
    );
    set_parent(world, e, parent);
    e
}
pub(super) fn mount(world: &mut World, parent: Entity) -> View {
    let earlier = container(
        world,
        Some(parent),
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            padding: 8.,
            gap: 4.,
            ..Layout::column()
        },
    );
    world.insert(earlier, Hidden);
    let earlier_button = controls::Button::mount(
        world,
        earlier,
        UiId(EARLIER),
        ButtonProps::new("Earlier collaborations", HostIntent::Command(EARLIER)),
    )
    .unwrap();
    let earlier_rows = container(
        world,
        Some(earlier),
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 4.,
            ..Layout::column()
        },
    );
    let root = container(
        world,
        Some(parent),
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            gap: 8.,
            ..Layout::column()
        },
    );
    world.insert(root, Hidden);
    let header = container(
        world,
        Some(root),
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            padding: 12.,
            gap: 12.,
            ..Layout::row()
        },
    );
    let back = controls::Button::mount(
        world,
        header,
        UiId(BACK),
        ButtonProps::new("Back to Medousa", HostIntent::Command(BACK)),
    )
    .unwrap()
    .root;
    world.get_mut::<Layout>(back).unwrap().width = Size::Px(152.);
    let title = label(world, header, "Medousa ↔ Agent");
    world.get_mut::<Label>(title).unwrap().overflow = TextOverflow::Ellipsis;
    let status = label(world, root, "Read-only exchange");
    world.get_mut::<Label>(status).unwrap().overflow = TextOverflow::Wrap;
    world.insert(
        status,
        Padding {
            left: 12.,
            right: 12.,
            top: 0.,
            bottom: 0.,
        },
    );
    let timeline = timeline::setup(world, root);
    world.insert(
        timeline,
        timeline::ReadingIdentity {
            sender: "Medousa · Assignment / follow-up".into(),
            recipient: "Agent".into(),
        },
    );
    let footer = container(
        world,
        Some(root),
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            padding: 12.,
            gap: 8.,
            ..Layout::row()
        },
    );
    let note = label(
        world,
        footer,
        "Read-only · Returned results are not automatically approved or applied.",
    );
    world.get_mut::<Label>(note).unwrap().overflow = TextOverflow::Wrap;
    let refresh = controls::Button::mount(
        world,
        footer,
        UiId(RETRY),
        ButtonProps::new("Refresh", HostIntent::Command(RETRY)),
    )
    .unwrap();
    world.get_mut::<Layout>(refresh.root).unwrap().width = Size::Px(80.);
    View {
        root,
        title,
        status,
        timeline,
        earlier,
        earlier_rows,
        earlier_label: earlier_button.label,
        back,
    }
}

pub(super) fn receive(
    world: &mut World,
    session: &str,
    result: Result<Vec<WorkCardDetail>, String>,
) {
    let state = &mut world.resource_mut::<Model>().unwrap().collaboration;
    if state.session != session {
        return;
    }
    state.inflight = false;
    state.refreshed = Some(Instant::now());
    match result {
        Err(error) => state.error = Some(error),
        Ok(rows) => {
            state.error = None;
            if rows
                .iter()
                .any(|row| state.selected.as_deref() == Some(work_id(row)))
            {
                state.projection = None;
            }
            for row in rows {
                if row.kind != WorkCardKind::TurnWorker
                    || row.session_id.as_deref() != Some(session)
                {
                    continue;
                }
                if let Some(full) = &row.worker_transcript {
                    if full.work_id != work_id(&row) || full.session_id != session {
                        continue;
                    }
                }
                if let Some(old) = state
                    .rows
                    .iter_mut()
                    .find(|old| work_id(old) == work_id(&row))
                {
                    // Preserve complete recorded text if a later rolling preview omits it.
                    let mut row = row;
                    if row.worker_transcript.is_none() {
                        row.worker_transcript = old.worker_transcript.clone();
                    }
                    *old = row;
                } else {
                    state.rows.push(row);
                }
            }
            state.rows.sort_by(|a, b| {
                a.card
                    .created_at_utc
                    .cmp(&b.card.created_at_utc)
                    .then(a.card.id.0.cmp(&b.card.id.0))
            });
        }
    }
}

pub(super) fn action(world: &mut World, intent: &ActionIntent) -> bool {
    let Some(HostIntent::Command(command)) = controls::host_intent(world, intent) else {
        return false;
    };
    if ![OPEN, BACK, RETRY, EARLIER].contains(&command) {
        return false;
    }
    let ui = *world.resource::<Ui>().unwrap();
    if command == EARLIER {
        let state = &mut world.resource_mut::<Model>().unwrap().collaboration;
        state.earlier_open = !state.earlier_open;
        return true;
    }
    if command == OPEN {
        let Some(id) = world.get::<Target>(intent.entity).map(|t| t.0.clone()) else {
            return true;
        };
        let state = &mut world.resource_mut::<Model>().unwrap().collaboration;
        state.selected = Some(id);
        state.projection = None;
        state.force_selected = true;
        state.return_focus = Some(intent.entity);
        state.refreshed = None;
        close_popup(world, ui.behavior, false);
        // Focus a visible destination, not the now-hidden parent composer.
        visible(world, ui.exchange.root, true);
        world.resource_mut::<Focus>().unwrap().entity = Some(ui.exchange.back);
    } else if command == BACK {
        let state = &mut world.resource_mut::<Model>().unwrap().collaboration;
        state.selected = None;
        let focus = state.return_focus.take();
        visible(world, ui.exchange.root, false);
        if let Some(focus) = focus.filter(|e| world.get::<Layout>(*e).is_some()) {
            world.resource_mut::<Focus>().unwrap().entity = Some(focus);
        } else {
            world.resource_mut::<Focus>().unwrap().entity = Some(ui.composer.editor);
        }
    } else {
        let state = &mut world.resource_mut::<Model>().unwrap().collaboration;
        state.refreshed = None;
        state.force_selected = true;
    }
    true
}

fn anchor<'a>(snapshot: &'a ChatSnapshot, row: &WorkCardDetail) -> Option<&'a str> {
    // Prefer canonical placement when the app core already knows this worker.
    if let Some(link) = snapshot
        .workers
        .iter()
        .find(|link| link.work_id == work_id(row))
    {
        if let Some(id) = link.parent_item_id.as_deref() {
            if let Some(item) = snapshot.items.iter().find(|item| item.id == id) {
                return Some(&item.id);
            }
        }
    }
    let turn = row.correlation_id.as_deref()?;
    snapshot
        .items
        .iter()
        .find(|item| item.role == ChatItemRole::Assistant && item.turn_id.as_deref() == Some(turn))
        .map(|item| item.id.as_str())
}
fn marker(world: &mut World, parent: Entity, row: &WorkCardDetail, earlier: bool) {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    work_id(row).hash(&mut hash);
    let text = format!(
        "{}{} · {}  ›",
        if earlier {
            "Earlier collaboration · "
        } else {
            "Working with "
        },
        title(row),
        lifecycle(row)
    );
    if let Some(existing) = children_of(world, parent)
        .into_iter()
        .find(|e| world.get::<Target>(*e).is_some_and(|t| t.0 == work_id(row)))
    {
        visible(world, existing, true);
        if world
            .get::<MarkerStamp>(existing)
            .is_some_and(|stamp| stamp.0 == text)
        {
            return;
        }
    }
    let mut props = ButtonProps::new(&text, HostIntent::Command(OPEN));
    props.variant = controls::ButtonVariant::Quiet;
    let h = controls::Button::mount(world, parent, UiId(hash.finish()), props).unwrap();
    world.insert(h.root, MarkerStamp(text));
    world.insert(h.root, Target(work_id(row).into()));
    world.get_mut::<Layout>(h.root).unwrap().width = Size::Fill;
    world.get_mut::<Layout>(h.root).unwrap().height = Size::Hug;
    world.get_mut::<Label>(h.label).unwrap().overflow = TextOverflow::Wrap;
    world.insert(
        h.root,
        UiStyle {
            foreground: Some(visual::secondary().into()),
            background: Some(UiColor::Explicit(Color::rgba(0., 0., 0., 0.))),
            ..Default::default()
        },
    );
    // sync_row may have rebuilt its ordered children while retaining this keyed
    // control. Restore the marker at the end without duplicating its entity.
    if let Some(children) = world.get_mut::<Children>(parent) {
        if !children.list.contains(&h.root) {
            children.list.push(h.root);
        }
    }
}

pub(super) fn sync(world: &mut World, ui: Ui, snapshot: Option<&ChatSnapshot>) {
    let Some(snapshot) = snapshot else {
        return;
    };
    let mut state = std::mem::take(&mut world.resource_mut::<Model>().unwrap().collaboration);
    let request = {
        let model = world.resource::<Model>().unwrap();
        if state.session != snapshot.session_id {
            state = State {
                session: snapshot.session_id.clone(),
                ..Default::default()
            };
        }
        let interval = if snapshot.active_turn_id.is_some() || state.selected.is_some() {
            3
        } else {
            15
        };
        let due = !state.inflight
            && state
                .refreshed
                .is_none_or(|at| at.elapsed() >= Duration::from_secs(interval))
            && model.commands.is_some();
        let request = if due {
            state.inflight = true;
            Some(Command::Collaboration {
                session: state.session.clone(),
                known: state
                    .rows
                    .iter()
                    .filter(|row| {
                        state.selected.as_deref() != Some(work_id(row))
                            || (row.terminal && !state.force_selected)
                    })
                    .map(|row| (row.card.id.0.clone(), row.card.updated_at_utc.to_rfc3339()))
                    .collect(),
            })
        } else {
            None
        };
        request
    };
    let selected = state.selected.clone();
    let error = state.error.clone();
    let rows = &state.rows;
    if let Some(request) = request {
        state.force_selected = false;
        let model = world.resource::<Model>().unwrap();
        if model.commands.as_ref().is_none_or(|tx| {
            tx.send(Request {
                generation: model.generation,
                command: request,
            })
            .is_err()
        }) {
            state.inflight = false;
            state.refreshed = Some(Instant::now());
            state.error = Some("Collaboration connection unavailable. Refresh to retry.".into());
        }
    }
    let open = selected.is_some();
    // Retain, do not reparent or remount, the parent timeline and SourceDocument.
    if let Some(parent) = world.get::<Parent>(ui.timeline).map(|p| p.entity) {
        visible(world, parent, !open);
    }
    if let Some(parent) = world.get::<Parent>(ui.compose).map(|p| p.entity) {
        visible(world, parent, !open);
    }
    visible(world, ui.approvals, !open);
    visible(world, ui.exchange.root, open);
    let mut earlier = false;
    // An earlier marker may gain a canonical anchor after history reconciliation.
    for entity in children_of(world, ui.exchange.earlier_rows) {
        let keep = world.get::<Target>(entity).is_some_and(|target| {
            rows.iter()
                .any(|row| work_id(row) == target.0 && anchor(snapshot, row).is_none())
        });
        visible(world, entity, keep);
    }
    for row in rows {
        if let Some(id) = anchor(snapshot, row) {
            if !open {
                if let Some(parent) = timeline::mounted_row(world, ui.timeline, id) {
                    marker(world, parent, row, false);
                }
            }
        } else {
            if state.earlier_open {
                marker(world, ui.exchange.earlier_rows, row, true);
            }
            earlier = true;
        }
    }
    visible(world, ui.exchange.earlier, earlier && !open);
    visible(world, ui.exchange.earlier_rows, state.earlier_open);
    let count = rows
        .iter()
        .filter(|row| anchor(snapshot, row).is_none())
        .count();
    let earlier_text = format!(
        "{} Earlier collaborations ({count})",
        if state.earlier_open { "▾" } else { "▸" }
    );
    if world.get::<Label>(ui.exchange.earlier_label).unwrap().text != earlier_text {
        world
            .get_mut::<Label>(ui.exchange.earlier_label)
            .unwrap()
            .text = earlier_text;
    }
    if let Some(id) = selected {
        if let Some(row) = rows.iter().find(|row| work_id(row) == id) {
            set_label(
                world,
                ui.exchange.title,
                format!("Medousa ↔ {}", title(row)),
            );
            set_label(
                world,
                ui.exchange.status,
                error.unwrap_or_else(|| format!("{} · Read-only exchange", lifecycle(row))),
            );
            if state.projection.is_none() {
                world.insert(
                    ui.exchange.timeline,
                    timeline::ReadingIdentity {
                        sender: "Medousa".into(),
                        recipient: title(row).into(),
                    },
                );
                state.projection = Some(project(row));
            }
            timeline::sync(world, ui.exchange.timeline, state.projection.as_ref());
        } else {
            set_label(
                world,
                ui.exchange.status,
                error.unwrap_or_else(|| "Loading exchange…".into()),
            );
        }
    }
    world.resource_mut::<Model>().unwrap().collaboration = state;
}

/// Presentation-only snapshot. Never admitted to ChatCore, never sendable. IDs
/// are based on persisted work/steer/tool coordinates rather than body content.
fn project(row: &WorkCardDetail) -> ChatSnapshot {
    let mut items = vec![];
    let full = row.worker_transcript.as_ref();
    let mut push = |id: String, role: &str, text: String| {
        items.push(serde_json::json!({"id":id,"role":role,"content":text}));
    };
    if full.is_none() {
        push(
            "notice".into(),
            "system",
            "Only the card preview is available. Complete worker history could not be loaded."
                .into(),
        );
    }
    push(
        format!("{}:assignment", work_id(row)),
        "user",
        full.map(|f| f.task_prompt.clone())
            .or_else(|| row.task_line.clone())
            .unwrap_or_else(|| title(row).into()),
    );
    if let Some(full) = full {
        for (index, followup) in full.steer_messages.iter().enumerate() {
            push(
                format!("{}:steer:{}:{index}", work_id(row), followup.at),
                "user",
                followup.text.clone(),
            );
        }
    }
    let answer = full
        .and_then(|full| full.result_text.as_deref())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            if row.live_output.len() >= row.result_excerpt.as_deref().unwrap_or("").len() {
                &row.live_output
            } else {
                row.result_excerpt.as_deref().unwrap_or("")
            }
        });
    push(
        format!("{}:output", work_id(row)),
        "assistant",
        if answer.is_empty() {
            if row.terminal {
                "No written result was recorded."
            } else {
                "The agent is working. Its output will appear here."
            }
            .into()
        } else {
            answer.into()
        },
    );
    if let Some(error) = &row.error {
        push("error".into(), "system", error.clone());
    }
    if let Some(full) = full {
        if !full.retention_note.is_empty() {
            push("retention".into(), "system", full.retention_note.clone());
        }
    }
    let tools = row.live_tool_activity.iter().map(|run| serde_json::json!({
        "runId": run.run_id, "toolName":run.name, "status":run.status, "round":run.round,
        "inputSummary":run.input_summary,"inputParams":run.input_params,"outputSummary":run.output_summary
    })).collect::<Vec<_>>();
    items.push(serde_json::json!({"id":"technical","role":"system","content":"Recorded evidence · Technical details", "toolRuns":tools}));
    // The standard timeline renders tools lazily behind disclosure. Never copy
    // private rolling thinking into the main conversation or the reading view.
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_vec(&items).unwrap().hash(&mut hash);
    serde_json::from_value(serde_json::json!({"sessionId":format!("worker:{}",work_id(row)),"authorityId":"worker-reading-view","revision":hash.finish(),"items":items})).expect("worker reading projection follows ChatSnapshot schema")
}
