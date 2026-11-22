//! Conversation-local collaboration reading. Durable worker records own identity,
//! placement and lifecycle; opening an exchange never navigates the parent Chat.
use super::*;
#[path = "collaboration_peers.rs"]
pub(crate) mod peers;
#[cfg(test)]
#[path = "collaboration_tests.rs"]
mod tests;
use medousa_app_core::{ChatItemRole, ChatSnapshot};
use medousa_types::{WorkBoardColumn, WorkCardDetail, WorkCardKind};
use std::{
    collections::HashSet,
    hash::{Hash, Hasher},
    time::{Duration, Instant},
};

const OPEN: u64 = 0x636f6c01;
const BACK: u64 = 0x636f6c02;
const RETRY: u64 = 0x636f6c03;
const EARLIER: u64 = 0x636f6c04;
const HISTORY_VISIBILITY: u64 = 0x636f6c08;
const HISTORY_DISMISS: u64 = 0x636f6c09;
const HISTORY_RESTORE: u64 = 0x636f6c0a;
// Window-local presentation only. Persistent reference keys additionally need
// the host's real workshop identity; never substitute an invented workshop ID.
#[derive(Default)]
struct HistoryPreferences {
    dismissed: HashMap<(String, String), HashSet<String>>,
    show_dismissed: bool,
}
impl HistoryPreferences {
    fn ids(&mut self, snapshot: &ChatSnapshot) -> &mut HashSet<String> {
        self.dismissed
            .entry((snapshot.authority_id.clone(), snapshot.session_id.clone()))
            .or_default()
    }
    fn reconcile(&mut self, snapshot: &ChatSnapshot, rows: &[WorkCardDetail]) {
        let ids = self.ids(snapshot);
        for row in rows {
            if !finished_history(row) {
                ids.remove(work_id(row));
            }
        }
    }
}
struct HistoryTarget {
    authority: String,
    session: String,
    work: String,
}
impl Component for HistoryTarget {}
fn finished_history(row: &WorkCardDetail) -> bool {
    row.terminal && row.card.column == WorkBoardColumn::Done
}
fn history_control(
    world: &mut World,
    parent: Entity,
    command: u64,
    work: &str,
    name: &str,
    snapshot: &ChatSnapshot,
) -> Entity {
    use rndr_ui::controls::{ButtonProps, HostIntent};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (command, &snapshot.authority_id, &snapshot.session_id, work).hash(&mut hasher);
    let control = rndr_ui::controls::Button::mount(
        world,
        parent,
        rndr_ui::UiId(hasher.finish()),
        ButtonProps::new(name, HostIntent::Command(command)),
    )
    .unwrap();
    world.insert(
        control.root,
        HistoryTarget {
            authority: snapshot.authority_id.clone(),
            session: snapshot.session_id.clone(),
            work: work.into(),
        },
    );
    visible(world, control.root, true);
    control.root
}
#[derive(Default)]
pub(super) struct State {
    session: String,
    authority: String,
    history: HistoryPreferences,
    peers: peers::State,
    rows: Vec<WorkCardDetail>,
    selected: Option<String>,
    inflight: bool,
    refreshed: Option<Instant>,
    error: Option<String>,
    return_focus: Option<Entity>,
    projection: Option<ChatSnapshot>,
    earlier_open: bool,
    force_selected: bool,
    drafts: HashMap<String, DocumentId>,
    reply_document: Option<DocumentId>,
    reply_pending: Option<(String, u64, DocumentId, rndr_core::editing::Revision)>,
    reply_notice: Option<String>,
    nonce: u64,
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
    execution: Entity,
    pub(super) approvals: Entity,
    reply: Entity,
    reply_editor: EditorSurfaceHandle,
    reply_note: Entity,
    reply_send: Entity,
    pub(super) main: Entity,
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
pub(crate) fn selected(world: &World) -> Option<&str> {
    world.resource::<Model>()?.collaboration.selected.as_deref()
}

pub(crate) fn open(world: &mut World, id: String, origin: Entity) {
    let ui = *world.resource::<Ui>().unwrap();
    let state = &mut world.resource_mut::<Model>().unwrap().collaboration;
    state.selected = Some(id);
    state.projection = None;
    state.force_selected = true;
    state.return_focus = Some(origin);
    state.refreshed = None;
    state.peers.refreshed = None;
    state.reply_notice = None;
    close_popup(world, ui.behavior, false);
    visible(world, ui.exchange.root, true);
    world.resource_mut::<Focus>().unwrap().entity = Some(ui.exchange.back);
}

pub(super) fn mount(world: &mut World, parent: Entity, thread_parent: Entity) -> View {
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
    let mut earlier_props =
        ButtonProps::new("Earlier collaborations", HostIntent::Command(EARLIER));
    earlier_props.variant = controls::ButtonVariant::Quiet;
    let earlier_button =
        controls::Button::mount(world, earlier, UiId(EARLIER), earlier_props).unwrap();
    // A quiet disclosure sized to its label, not a full-width bordered slab.
    world.insert(
        earlier_button.root,
        crate::native_style::FitLabel {
            label: earlier_button.label,
            extra: 4.,
            min: 48.,
            max: 360.,
        },
    );
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
        Some(thread_parent),
        Layout {
            width: Size::Px(0.),
            height: Size::Px(0.),
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
        ButtonProps::new("Close thread", HostIntent::Command(BACK)),
    )
    .unwrap()
    .root;
    world.get_mut::<Layout>(back).unwrap().width = Size::Px(108.);
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
    visual::plane(world, root, visual::raised());
    let approvals = container(
        world,
        Some(root),
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            padding: 12.,
            gap: 10.,
            ..Layout::column()
        },
    );
    world.insert(approvals, Scroll::default());
    world.insert(approvals, Hidden);
    let timeline = timeline::setup(world, root);
    world.insert(
        timeline,
        timeline::ReadingIdentity {
            sender: "Medousa · Assignment / follow-up".into(),
            recipient: "Agent".into(),
        },
    );
    let reply = container(
        world,
        Some(root),
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            padding: 12.,
            gap: 6.,
            ..Layout::column()
        },
    );
    let reply_note = label(world, reply, "Reply to this thread…");
    world.get_mut::<Label>(reply_note).unwrap().overflow = TextOverflow::Wrap;
    let doc = world.resource_mut::<SourceDocuments>().unwrap().open("");
    let mut props = EditorSurfaceProps::new(doc, "Reply to this thread");
    props.gutter = false;
    props.scrollbar = true;
    let editor = EditorSurface::mount(world, reply, UiId(0x636f6c06), props).unwrap();
    world.get_mut::<Layout>(editor.root).unwrap().height = Size::Px(72.);
    world.insert(editor.editor, Placeholder::new("Reply to this thread…"));
    world.get_mut::<Label>(editor.editor).unwrap().overflow = TextOverflow::Wrap;
    let reply_send = controls::Button::mount(
        world,
        reply,
        UiId(0x636f6c07),
        ButtonProps::new("Send reply", HostIntent::Command(0x636f6c07)),
    )
    .unwrap()
    .root;
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
    let execution = controls::Button::mount(
        world,
        footer,
        UiId(peers::EXECUTION),
        ButtonProps::new("Open execution chat", HostIntent::Command(peers::EXECUTION)),
    )
    .unwrap()
    .root;
    world.get_mut::<Layout>(execution).unwrap().width = Size::Px(174.);
    world.insert(execution, Hidden);
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
        execution,
        approvals,
        reply,
        reply_editor: editor,
        reply_note,
        reply_send,
        main: parent,
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
    if command == peers::EXECUTION {
        peers::open_execution(world);
        return true;
    }
    if command == 0x636f6c07 {
        send_reply(world);
        return true;
    }
    if ![
        OPEN,
        BACK,
        RETRY,
        EARLIER,
        HISTORY_VISIBILITY,
        HISTORY_DISMISS,
        HISTORY_RESTORE,
    ]
    .contains(&command)
    {
        return false;
    }
    let ui = *world.resource::<Ui>().unwrap();
    if command == EARLIER {
        let state = &mut world.resource_mut::<Model>().unwrap().collaboration;
        state.earlier_open = !state.earlier_open;
        return true;
    }
    if [HISTORY_VISIBILITY, HISTORY_DISMISS, HISTORY_RESTORE].contains(&command) {
        let Some(target) = world.get::<HistoryTarget>(intent.entity) else {
            return true;
        };
        let (authority, session, work) = (
            target.authority.clone(),
            target.session.clone(),
            target.work.clone(),
        );
        let model = world.resource_mut::<Model>().unwrap();
        let Some(snapshot) = model.chat.snapshot() else {
            return true;
        };
        // A queued click cannot alter another authority/session's presentation.
        if snapshot.authority_id != authority || snapshot.session_id != session {
            return true;
        }
        let state = &mut model.collaboration;
        if state.authority != authority || state.session != session {
            return true;
        }
        if command == HISTORY_VISIBILITY {
            state.history.show_dismissed = !state.history.show_dismissed;
        } else if state
            .rows
            .iter()
            .any(|row| work_id(row) == work && finished_history(row))
        {
            let ids = state
                .history
                .dismissed
                .entry((authority, session))
                .or_default();
            if command == HISTORY_DISMISS {
                ids.insert(work);
            } else {
                ids.remove(&work);
            }
        }
        // No Command/Reply or daemon mutation: dismissal is never cancellation.
        return true;
    }
    if command == OPEN {
        let Some(id) = world.get::<Target>(intent.entity).map(|t| t.0.clone()) else {
            return true;
        };
        open(world, id, intent.entity);
    } else if command == BACK {
        let state = &mut world.resource_mut::<Model>().unwrap().collaboration;
        state.selected = None;
        state.peers.refreshed = None;
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
        state.peers.refreshed = None;
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
    marker_entry(world, parent, work_id(row), text);
}
fn marker_entry(world: &mut World, parent: Entity, id: &str, text: String) {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut hash);
    if let Some(existing) = children_of(world, parent)
        .into_iter()
        .find(|e| world.get::<Target>(*e).is_some_and(|t| t.0 == id))
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
    world.insert(h.root, Target(id.into()));
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
        if state.session != snapshot.session_id || state.authority != snapshot.authority_id {
            let mut history = std::mem::take(&mut state.history);
            history.show_dismissed = false;
            state = State {
                session: snapshot.session_id.clone(),
                authority: snapshot.authority_id.clone(),
                history,
                ..Default::default()
            };
        }
        peers::request(&mut state.peers, state.selected.as_deref(), snapshot, model);
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
    state.history.reconcile(snapshot, &state.rows);
    let dismissed = state.history.ids(snapshot).clone();
    let show_dismissed = state.history.show_dismissed;
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
    // Compact windows use one lane, but both lanes remain mounted. A wide
    // window always keeps the main conversation and its composer usable.
    let available = world
        .get::<Parent>(ui.exchange.root)
        .and_then(|p| world.get::<Rect>(p.entity))
        .map_or(1000., |r| r.w);
    if let Some(parent) = world.get::<Parent>(ui.exchange.root).map(|p| p.entity) {
        world.get_mut::<Layout>(parent).unwrap().gap = if open { 4. } else { 0. };
    }
    let narrow = available < 860.;
    let show_main = !open || !narrow;
    visible(world, ui.exchange.main, show_main);
    world.get_mut::<Layout>(ui.exchange.main).unwrap().width =
        if show_main { Size::Fill } else { Size::Px(0.) };
    world.get_mut::<Layout>(ui.exchange.main).unwrap().height =
        if show_main { Size::Fill } else { Size::Px(0.) };
    let layout = world.get_mut::<Layout>(ui.exchange.root).unwrap();
    layout.width = if !open {
        Size::Px(0.)
    } else if narrow {
        Size::Fill
    } else {
        Size::Px((available * 0.40).clamp(340., 480.))
    };
    layout.height = if open { Size::Fill } else { Size::Px(0.) };
    let main_width = if open && !narrow {
        available - (available * 0.40).clamp(340., 480.) - 4.
    } else {
        available
    };
    if open {
        world.get_mut::<Layout>(ui.compose).unwrap().width =
            Size::Px(visual::column_width(main_width));
    }
    visible(world, ui.exchange.root, open);
    let approval = selected
        .as_deref()
        .is_some_and(|id| id.starts_with("approval:"));
    visible(world, ui.exchange.approvals, approval);
    if let Some(frame) = world.get::<Parent>(ui.exchange.timeline).map(|p| p.entity) {
        visible(world, frame, !approval);
    }
    visible(world, ui.exchange.reply, open && !approval);
    let mut earlier = false;
    // An earlier marker may gain a canonical anchor after history reconciliation.
    for entity in children_of(world, ui.exchange.earlier_rows) {
        let keep = world.get::<Target>(entity).is_some_and(|target| {
            rows.iter().any(|row| {
                work_id(row) == target.0
                    && anchor(snapshot, row).is_none()
                    && (!dismissed.contains(work_id(row)) || show_dismissed)
            }) || peers::earlier(&state.peers, snapshot, &target.0)
        });
        visible(world, entity, keep);
    }
    let dismissed_count = rows
        .iter()
        .filter(|row| {
            finished_history(row)
                && dismissed.contains(work_id(row))
                && anchor(snapshot, row).is_none()
        })
        .count();
    if state.earlier_open && dismissed_count > 0 {
        history_control(
            world,
            ui.exchange.earlier_rows,
            HISTORY_VISIBILITY,
            "",
            &format!(
                "{} dismissed finished work ({dismissed_count}) · this window",
                if show_dismissed { "Hide" } else { "Show" }
            ),
            snapshot,
        );
    }
    for row in rows {
        if let Some(id) = anchor(snapshot, row) {
            if let Some(parent) = timeline::mounted_row(world, ui.timeline, id) {
                marker(world, parent, row, false);
            }
        } else {
            if state.earlier_open && (!dismissed.contains(work_id(row)) || show_dismissed) {
                marker(world, ui.exchange.earlier_rows, row, true);
                if finished_history(row) {
                    let hidden = dismissed.contains(work_id(row));
                    history_control(
                        world,
                        ui.exchange.earlier_rows,
                        if hidden {
                            HISTORY_RESTORE
                        } else {
                            HISTORY_DISMISS
                        },
                        work_id(row),
                        if hidden {
                            "Restore finished work · this window"
                        } else {
                            "Dismiss finished work · this window"
                        },
                        snapshot,
                    );
                }
            }
            earlier = true;
        }
    }
    let peer_count = peers::markers(world, ui, snapshot, &state.peers, false, state.earlier_open);
    earlier |= peer_count > 0;
    visible(world, ui.exchange.earlier, earlier);
    visible(world, ui.exchange.earlier_rows, state.earlier_open);
    let count = rows
        .iter()
        .filter(|row| anchor(snapshot, row).is_none())
        .count()
        + peer_count;
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
    visible(world, ui.exchange.execution, false);
    if let Some(id) = selected {
        if id.starts_with("approval:") {
            set_label(world, ui.exchange.title, "Work request".into());
            set_label(
                world,
                ui.exchange.status,
                "Review this request · Approval never starts work".into(),
            );
        } else if id.starts_with("peer:") {
            peers::render(world, ui, &state.peers, &id, snapshot);
        } else if let Some(row) = rows.iter().find(|row| work_id(row) == id) {
            set_label(world, ui.exchange.title, title(row).to_owned());
            set_label(
                world,
                ui.exchange.status,
                error.unwrap_or_else(|| {
                    format!(
                        "{} · Medousa + {}",
                        lifecycle(row),
                        row.execution_runtime_id.as_deref().unwrap_or("Worker")
                    )
                }),
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
    sync_reply(world, ui, &mut state);
    world.resource_mut::<Model>().unwrap().collaboration = state;
}

fn sync_reply(world: &mut World, ui: Ui, state: &mut State) {
    let Some(id) = state
        .selected
        .clone()
        .filter(|s| !s.starts_with("approval:"))
    else {
        return;
    };
    let enabled = state
        .rows
        .iter()
        .any(|row| work_id(row) == id && !row.terminal);
    let document = *state
        .drafts
        .entry(id.clone())
        .or_insert_with(|| world.resource_mut::<SourceDocuments>().unwrap().open(""));
    let mut editor = ui.exchange.reply_editor;
    if state.reply_document != Some(document) {
        // Rebind the view, never the document: each work item keeps its own draft.
        let _ = editor.destroy(world);
        let mut props = EditorSurfaceProps::new(document, "Reply to this thread");
        props.gutter = false;
        props.scrollbar = true;
        editor = EditorSurface::mount(world, ui.exchange.reply, UiId(0x636f6c06), props).unwrap();
        world.get_mut::<Layout>(editor.root).unwrap().height = Size::Px(72.);
        world.insert(editor.editor, Placeholder::new("Reply to this thread…"));
        world.get_mut::<Label>(editor.editor).unwrap().overflow = TextOverflow::Wrap;
        // Remounting appends; keep the reply button below the editor.
        if let Some(children) = world.get_mut::<Children>(ui.exchange.reply) {
            children.list.retain(|e| *e != ui.exchange.reply_send);
            children.list.push(ui.exchange.reply_send);
        }
        world.resource_mut::<Ui>().unwrap().exchange.reply_editor = editor;
        state.reply_document = Some(document);
    }
    visible(world, editor.root, enabled);
    visible(
        world,
        ui.exchange.reply_send,
        enabled && state.reply_pending.is_none(),
    );
    let notice = state.reply_notice.clone().unwrap_or_else(|| {
        if state.reply_pending.is_some() { "Sending reply… Do not resend while delivery is unresolved." }
        else if enabled { "Reply to this thread · Sent to this worker only" }
        else if id.starts_with("peer:") { "Read-only thread · Use Open execution chat for supported agent controls. Opening does not grant permission." }
        else { "Work finished · This thread is read-only." }.into()
    });
    set_label(world, ui.exchange.reply_note, notice);
}

fn send_reply(world: &mut World) {
    let model = world.resource::<Model>().unwrap();
    let state = &model.collaboration;
    let Some(id) = state.selected.clone() else {
        return;
    };
    if state.reply_pending.is_some() || !state.rows.iter().any(|r| work_id(r) == id && !r.terminal)
    {
        return;
    }
    let Some(document) = state.reply_document else {
        return;
    };
    let Some(doc) = world.resource::<SourceDocuments>().unwrap().get(document) else {
        return;
    };
    if doc.is_composing() || doc.text().trim().is_empty() {
        return;
    }
    let message = doc.text().to_owned();
    let revision = doc.revision();
    let session = state.session.clone();
    if model.commands.is_none() {
        return;
    }
    let state = &mut world.resource_mut::<Model>().unwrap().collaboration;
    state.nonce += 1;
    let nonce = state.nonce;
    state.reply_pending = Some((id.clone(), nonce, document, revision));
    state.reply_notice = None;
    dispatch(
        world,
        Command::ThreadReply {
            session,
            work_id: id,
            message,
            nonce,
        },
    );
}

pub(crate) fn reply_received(
    world: &mut World,
    session: &str,
    id: &str,
    nonce: u64,
    result: Result<(), String>,
) {
    let state = &mut world.resource_mut::<Model>().unwrap().collaboration;
    if state.session != session {
        return;
    }
    let Some((pending_id, pending_nonce, document, revision)) = state.reply_pending.clone() else {
        return;
    };
    if pending_id != id || pending_nonce != nonce {
        return;
    }
    state.reply_pending = None;
    state.refreshed = None;
    match result {
        Ok(()) => {
            if state.selected.as_deref() == Some(id) {
                state.reply_notice = Some("Reply delivered to this worker.".into());
            }
            let doc = world
                .resource_mut::<SourceDocuments>()
                .unwrap()
                .get_mut(document)
                .unwrap();
            if doc.revision() == revision && !doc.is_composing() {
                let _ = doc.commit(rndr_core::editing::SourceBatch {
                    document,
                    base: revision,
                    origin: rndr_core::editing::EditOrigin::Host(0x636f6c07),
                    changes: vec![rndr_core::editing::SourceChange {
                        range: 0..doc.text().len(),
                        replacement: String::new(),
                    }],
                });
            }
        }
        Err(error) => {
            // Unknown delivery must not expose a replay button for the same draft.
            state.reply_pending = Some((pending_id, pending_nonce, document, revision));
            state.reply_notice = Some(error);
        }
    }
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
