//! Chat-local context, using RNDR controls and the authenticated daemon contract.
//! Context usage/pointers are source facts supplied by the host, never guessed
//! from text. Stickies currently have no public Rust DTO: keep their wire JSON
//! opaque rather than inventing a second serde schema in the shell.
use medousa_sdk::{MedousaClient, generated::expand_path};
use medousa_types::{ContextPointerDigest, ContextUsageReport};
use rndr_core::*;
use rndr_ui::{
    Typography, UiId,
    controls::{self, ButtonProps, HostIntent},
    text,
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0x434f_0000);
fn next() -> u64 {
    NEXT.fetch_add(1, Ordering::Relaxed)
}
fn request_key() -> String {
    // Unique across app launches as well as requests, without a new dependency.
    format!(
        "native-sticky-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        next()
    )
}
#[derive(Clone, Debug)]
pub struct ContextCommand {
    pub session: String,
    serial: u64,
    body: Option<Value>,
}
#[derive(Debug)]
pub struct ContextUpdate {
    command: ContextCommand,
    result: Result<Value, String>,
}
pub fn refresh(session: impl Into<String>) -> ContextCommand {
    ContextCommand {
        session: session.into(),
        serial: next(),
        body: None,
    }
}
#[derive(Default)]
struct Session {
    board: Option<Value>,
    report: Option<ContextUsageReport>,
    pointers: Option<ContextPointerDigest>,
    revision: u64,
    latest: u64,
    busy: bool,
    error: String,
    // Unknown outcomes retain the exact idempotent request. No implicit replay.
    retry: Option<ContextCommand>,
    editing: Option<Value>,
    edit_revision: Option<u64>,
    draft: String,
    pinned: bool,
    exact: bool,
    removing: Option<String>,
}
#[derive(Clone)]
enum Action {
    Toggle,
    Refresh,
    Retry,
    Add,
    Edit(Value),
    Save,
    Cancel,
    Pin(String, bool),
    Exact(Value),
    Resolve(String, bool),
    Remove(String),
    ConfirmRemove(String),
    TogglePinned,
    ToggleExact,
}
#[derive(Default)]
struct Context {
    sessions: HashMap<String, Session>,
    visible: Option<String>,
    expanded: bool,
    rendered: Option<(Option<String>, u64, bool)>,
    actions: HashMap<u64, (String, Action)>,
    input: Option<Entity>,
    queue: Vec<ContextCommand>,
}
pub fn setup(world: &mut World, parent: Entity) -> Entity {
    if world.resource::<Context>().is_none() {
        world.insert_resource(Context::default());
    }
    let root = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Hug,
        gap: 4.,
        ..Layout::column()
    },));
    set_parent(world, root, parent);
    root
}
fn capture(world: &World, c: &mut Context) {
    if let (Some(session), Some(input)) = (&c.visible, c.input) {
        if let Some(label) = world.get::<Label>(input) {
            c.sessions.entry(session.clone()).or_default().draft = label.text.clone();
        }
    }
}
fn button(
    world: &mut World,
    root: Entity,
    c: &mut Context,
    session: &str,
    label: &str,
    action: Action,
    enabled: bool,
) {
    let id = next();
    let mut props = ButtonProps::new(label, HostIntent::Command(id));
    props.enabled = enabled;
    controls::Button::mount(world, root, UiId(id), props).expect("context button");
    c.actions.insert(id, (session.into(), action));
}
fn label(world: &mut World, root: Entity, value: &str) {
    text(world, root, value, Typography::Secondary).expect("context text");
}
fn board_revision(s: &Session) -> Option<u64> {
    s.board.as_ref()?.get("revision")?.as_u64()
}
pub fn sync(world: &mut World, root: Entity, session: Option<&str>) {
    let mut c = std::mem::take(world.resource_mut::<Context>().expect("context setup"));
    let revision = session
        .and_then(|s| c.sessions.get(s))
        .map_or(0, |s| s.revision);
    let key = (session.map(str::to_owned), revision, c.expanded);
    if c.rendered.as_ref() == Some(&key) {
        *world.resource_mut::<Context>().unwrap() = c;
        return;
    }
    capture(world, &mut c);
    for child in children_of(world, root) {
        despawn(world, child);
    }
    c.actions.clear();
    c.input = None;
    c.visible = session.map(str::to_owned);
    c.rendered = Some(key);
    if let Some(session) = session {
        let expanded = c.expanded;
        button(
            world,
            root,
            &mut c,
            session,
            if expanded {
                "Hide context / sticky notes"
            } else {
                "Context / sticky notes"
            },
            Action::Toggle,
            true,
        );
        if c.expanded {
            // Take the session out while mounting controls; all draft state remains session-owned.
            let s = c.sessions.remove(session).unwrap_or_default();
            if let Some(r) = &s.report {
                label(
                    world,
                    root,
                    &format!(
                        "Context estimate: {} tokens · window {} · {} tools · estimator {}",
                        r.total_tokens_estimate,
                        r.context_limit_tokens
                            .map_or_else(|| "unknown".into(), |v| v.to_string()),
                        r.tool_count,
                        r.estimator
                    ),
                );
                for layer in &r.layers {
                    label(
                        world,
                        root,
                        &format!(
                            "{}: {} estimated tokens",
                            layer.label, layer.tokens_estimate
                        ),
                    );
                }
            } else {
                label(
                    world,
                    root,
                    "Context usage is available after the daemon reports a turn's compiled context.",
                );
            }
            if let Some(digest) = &s.pointers {
                for p in &digest.pointers {
                    label(
                        world,
                        root,
                        &format!(
                            "{} · {} · {}\n{}",
                            p.kind,
                            p.label,
                            p.age_human,
                            p.topic.as_deref().unwrap_or("")
                        ),
                    );
                }
            }
            if !s.error.is_empty() {
                label(world, root, &s.error);
            }
            if s.busy {
                label(world, root, "Updating sticky notes…");
            }
            button(
                world,
                root,
                &mut c,
                session,
                "Reload latest sticky board",
                Action::Refresh,
                !s.busy,
            );
            if s.retry.is_some() {
                button(
                    world,
                    root,
                    &mut c,
                    session,
                    "Retry same change",
                    Action::Retry,
                    !s.busy,
                );
            }
            let enabled = !s.busy && s.retry.is_none();
            if let Some(board) = &s.board {
                if let Some(notes) = board.get("notes").and_then(Value::as_array) {
                    for note in notes {
                        let Some(id) = note.get("id").and_then(Value::as_str) else {
                            continue;
                        };
                        let resolved =
                            note.get("status").and_then(Value::as_str) == Some("resolved");
                        let pinned = note.get("pinned").and_then(Value::as_bool).unwrap_or(false);
                        label(
                            world,
                            root,
                            &format!(
                                "{}{}{}",
                                if resolved { "Resolved · " } else { "" },
                                if pinned { "Pinned · " } else { "" },
                                note.get("text").and_then(Value::as_str).unwrap_or("")
                            ),
                        );
                        button(
                            world,
                            root,
                            &mut c,
                            session,
                            "Edit note",
                            Action::Edit(note.clone()),
                            enabled && s.edit_revision.is_none(),
                        );
                        button(
                            world,
                            root,
                            &mut c,
                            session,
                            if pinned { "Unpin" } else { "Pin" },
                            Action::Pin(id.into(), !pinned),
                            enabled && s.edit_revision.is_none(),
                        );
                        button(
                            world,
                            root,
                            &mut c,
                            session,
                            if note["exact"] == true {
                                "Allow summarizing"
                            } else {
                                "Preserve exact text"
                            },
                            Action::Exact(note.clone()),
                            enabled && s.edit_revision.is_none(),
                        );
                        button(
                            world,
                            root,
                            &mut c,
                            session,
                            if resolved { "Reopen" } else { "Resolve" },
                            Action::Resolve(id.into(), resolved),
                            enabled && s.edit_revision.is_none(),
                        );
                        button(
                            world,
                            root,
                            &mut c,
                            session,
                            "Remove note…",
                            Action::Remove(id.into()),
                            enabled && s.edit_revision.is_none(),
                        );
                        if s.removing.as_deref() == Some(id) {
                            button(
                                world,
                                root,
                                &mut c,
                                session,
                                "Confirm removal",
                                Action::ConfirmRemove(id.into()),
                                enabled,
                            );
                        }
                    }
                }
                if s.edit_revision.is_some() {
                    label(
                        world,
                        root,
                        if s.editing.is_some() {
                            "Edit sticky note"
                        } else {
                            "Add sticky note"
                        },
                    );
                    if s.edit_revision != board_revision(&s) {
                        label(
                            world,
                            root,
                            "Board changed since editing began. Save will require the original revision; cancel and reopen to use latest.",
                        );
                    }
                    let input = world.spawn((
                        Layout {
                            width: Size::Fill,
                            height: Size::Px(72.),
                            ..Layout::column()
                        },
                        Label::new(&s.draft),
                        Focusable,
                        Placeholder::new("Something worth keeping in mind…"),
                    ));
                    if enabled {
                        world.insert(
                            input,
                            Prompt {
                                submit_on_enter: false,
                                ..Prompt::new()
                            },
                        );
                    }
                    set_parent(world, input, root);
                    c.input = Some(input);
                    button(
                        world,
                        root,
                        &mut c,
                        session,
                        if s.pinned {
                            "Pinned: yes"
                        } else {
                            "Pinned: no"
                        },
                        Action::TogglePinned,
                        enabled,
                    );
                    button(
                        world,
                        root,
                        &mut c,
                        session,
                        if s.exact {
                            "Exact text: yes"
                        } else {
                            "Exact text: no"
                        },
                        Action::ToggleExact,
                        enabled,
                    );
                    button(
                        world,
                        root,
                        &mut c,
                        session,
                        "Save note",
                        Action::Save,
                        enabled,
                    );
                    button(
                        world,
                        root,
                        &mut c,
                        session,
                        "Cancel edit",
                        Action::Cancel,
                        enabled,
                    );
                } else {
                    button(
                        world,
                        root,
                        &mut c,
                        session,
                        "+ Add sticky note",
                        Action::Add,
                        enabled,
                    );
                }
                label(
                    world,
                    root,
                    "Working context for this chat, not permanent memory",
                );
            }
            c.sessions.insert(session.into(), s);
        }
    }
    *world.resource_mut::<Context>().unwrap() = c;
}
fn enqueue(c: &mut Context, session: &str, mut command: ContextCommand) {
    let s = c.sessions.entry(session.into()).or_default();
    if s.busy {
        return;
    }
    command.serial = next();
    s.latest = command.serial;
    s.busy = true;
    s.revision += 1;
    s.error.clear();
    if command.body.is_some() {
        s.retry = Some(command.clone());
    }
    c.queue.push(command);
}
fn mutation(session: &str, s: &Session, mut body: Value) -> Option<ContextCommand> {
    body["expected_revision"] = json!(s.edit_revision.or_else(|| board_revision(s))?);
    body["request_key"] = json!(request_key());
    Some(ContextCommand {
        session: session.into(),
        serial: next(),
        body: Some(body),
    })
}
/// Consume the HostIntent returned by controls::host_intent. Command ids are
/// globally distinct, so unrelated controls are not intercepted.
pub fn action(world: &mut World, intent: &HostIntent) -> bool {
    let HostIntent::Command(id) = intent else {
        return false;
    };
    let Some(mut c) = world.resource_mut::<Context>().map(std::mem::take) else {
        return false;
    };
    let Some((session, a)) = c.actions.get(id).cloned() else {
        *world.resource_mut::<Context>().unwrap() = c;
        return false;
    };
    capture(world, &mut c);
    if c.visible.as_deref() != Some(&session) {
        *world.resource_mut::<Context>().unwrap() = c;
        return true;
    }
    let s = c.sessions.entry(session.clone()).or_default();
    let mut command = None;
    match a {
        Action::Toggle => {
            c.expanded = !c.expanded;
            if c.expanded && s.board.is_none() && !s.busy {
                command = Some(refresh(&session));
            }
        }
        Action::Refresh if !s.busy => command = Some(refresh(&session)),
        Action::Retry if !s.busy => command = s.retry.clone(),
        Action::Add if !s.busy && s.retry.is_none() => {
            c.input = None;
            s.edit_revision = board_revision(s);
            s.editing = None;
            s.draft.clear();
            s.pinned = true;
            s.exact = false;
            s.revision += 1;
        }
        Action::Edit(note) if !s.busy && s.retry.is_none() => {
            c.input = None;
            s.edit_revision = board_revision(s);
            s.draft = note["text"].as_str().unwrap_or("").into();
            s.pinned = note["pinned"] == true;
            s.exact = note["exact"] == true;
            s.editing = Some(note);
            s.revision += 1;
        }
        Action::Cancel if !s.busy && s.retry.is_none() => {
            c.input = None;
            s.editing = None;
            s.edit_revision = None;
            s.draft.clear();
            s.revision += 1;
        }
        Action::TogglePinned if !s.busy && s.retry.is_none() => {
            s.pinned = !s.pinned;
            s.revision += 1;
        }
        Action::ToggleExact if !s.busy && s.retry.is_none() => {
            s.exact = !s.exact;
            s.revision += 1;
        }
        Action::Remove(id) if !s.busy && s.retry.is_none() => {
            s.removing = Some(id);
            s.revision += 1;
        }
        a if !s.busy && s.retry.is_none() => {
            let body = match a {
                Action::Save if !s.draft.trim().is_empty() && s.edit_revision.is_some() => {
                    let mut note = s.editing.clone().unwrap_or_else(|| json!({"id":request_key(), "kind":"detail", "status":"active", "source_refs":[]}));
                    note["text"] = json!(s.draft);
                    note["pinned"] = json!(s.pinned);
                    note["exact"] = json!(s.exact);
                    Some(json!({"action":"sticky.put", "note":note}))
                }
                Action::Pin(id, pinned) => {
                    Some(json!({"action":"sticky.pin", "id":id, "pinned":pinned}))
                }
                Action::Exact(mut note) => {
                    note["exact"] = json!(note["exact"] != true);
                    Some(json!({"action":"sticky.put", "note":note}))
                }
                Action::Resolve(id, reopen) => Some(
                    json!({"action":if reopen {"sticky.reopen"} else {"sticky.resolve"}, "id":id}),
                ),
                Action::ConfirmRemove(id) if s.removing.as_deref() == Some(&id) => {
                    Some(json!({"action":"sticky.remove", "id":id}))
                }
                _ => None,
            };
            command = body.and_then(|body| mutation(&session, s, body));
        }
        _ => {}
    }
    if let Some(command) = command {
        enqueue(&mut c, &session, command);
    }
    *world.resource_mut::<Context>().unwrap() = c;
    true
}
pub fn drain_commands(world: &mut World) -> Vec<ContextCommand> {
    world
        .resource_mut::<Context>()
        .map(|c| std::mem::take(&mut c.queue))
        .unwrap_or_default()
}
pub async fn execute(
    client: &MedousaClient,
    command: ContextCommand,
) -> Result<ContextUpdate, String> {
    let result = async {
        let path = expand_path(
            "/v1/sessions/{session_id}/stickies",
            &[("session_id", &command.session)],
        )?;
        let value = match &command.body {
            Some(body) => {
                client
                    .transport()
                    .post_json(client.base_url(), &path, body.clone())
                    .await
            }
            None => client.transport().get_json(client.base_url(), &path).await,
        }
        .map_err(|e| e.to_string())?;
        validate_board(&value)?;
        Ok(value)
    }
    .await;
    // Failures remain scoped updates so the host can always settle busy state.
    Ok(ContextUpdate { command, result })
}
fn validate_board(value: &Value) -> Result<(), String> {
    if value.get("revision").and_then(Value::as_u64).is_none()
        || value.get("notes").and_then(Value::as_array).is_none()
    {
        return Err("Invalid sticky board response".into());
    }
    for note in value["notes"].as_array().unwrap() {
        if note.get("id").and_then(Value::as_str).is_none()
            || note.get("text").and_then(Value::as_str).is_none()
            || !matches!(
                note.get("status").and_then(Value::as_str),
                Some("active" | "resolved")
            )
        {
            return Err("Invalid sticky note response".into());
        }
    }
    Ok(())
}
pub fn apply(world: &mut World, update: ContextUpdate) {
    let Some(c) = world.resource_mut::<Context>() else {
        return;
    };
    let s = c.sessions.entry(update.command.session).or_default();
    if update.command.serial < s.latest {
        return;
    }
    s.latest = update.command.serial;
    s.busy = false;
    s.revision += 1;
    match update.result {
        Ok(board) => {
            // A server revision must never move backwards, including a refresh
            // initiated outside this module before an explicit mutation.
            if board_revision(s).is_some_and(|old| board["revision"].as_u64().unwrap_or(0) < old) {
                return;
            }
            s.board = Some(board);
            s.error.clear();
            s.retry = None;
            if update.command.body.is_some() {
                s.editing = None;
                s.edit_revision = None;
                s.draft.clear();
                s.removing = None;
            }
        }
        Err(error) => {
            s.error = if update.command.body.is_some() {
                format!(
                    "{error}. Change not confirmed. Retry same change or reload latest; no automatic retry."
                )
            } else {
                error
            };
        }
    }
}
/// Feed the canonical stream fact using the source session, not selected chat.
/// Does not invalidate mounted editors: unrelated usage events cannot steal focus.
pub fn usage(world: &mut World, session: &str, report: ContextUsageReport) {
    if let Some(c) = world.resource_mut::<Context>() {
        let s = c.sessions.entry(session.into()).or_default();
        if s.report.as_ref() != Some(&report) {
            s.report = Some(report);
            if s.edit_revision.is_none() {
                s.revision += 1;
            }
        }
    }
}
/// Feed actual daemon pointer facts if/when the host receives them. No guessed
/// retrieval endpoint or local filesystem authority is introduced here.
pub fn pointers(world: &mut World, session: &str, digest: ContextPointerDigest) {
    if let Some(c) = world.resource_mut::<Context>() {
        let s = c.sessions.entry(session.into()).or_default();
        s.pointers = Some(digest);
        if s.edit_revision.is_none() {
            s.revision += 1;
        }
    }
}
#[cfg(test)]
#[path = "context_tests.rs"]
mod tests;
