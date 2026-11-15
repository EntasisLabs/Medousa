//! Explicit, session-scoped approvals. UI only queues owned commands; transport
//! executes off-thread through the authenticated SDK. Approval never starts work.
use medousa_app_core::ChatSnapshot;
use medousa_sdk::{
    MedousaClient,
    generated::{expand_path, ops},
};
use medousa_types::coordination::*;
use medousa_types::*;
use rndr_core::*;
use rndr_ui::{
    Surface, Typography, UiId,
    controls::{self, ButtonProps, HostIntent},
    text,
};
use std::{
    collections::{HashMap, HashSet},
    sync::atomic::{AtomicU64, Ordering},
};

static SERIAL: AtomicU64 = AtomicU64::new(100_000);
fn serial() -> u64 {
    SERIAL.fetch_add(1, Ordering::Relaxed)
}
#[derive(Clone, Debug)]
pub struct ApprovalCommand {
    pub session: String,
    serial: u64,
    operation: Operation,
}
#[derive(Clone, Debug)]
enum Operation {
    Refresh,
    Budget {
        id: String,
        rounds: Option<usize>,
        approve: bool,
    },
    Permission {
        id: String,
        option: Option<String>,
        approve: bool,
    },
    Proposal {
        channel: String,
        id: String,
        verb: ProposalVerb,
    },
}
#[derive(Clone, Copy, Debug)]
enum ProposalVerb {
    Approve,
    Deny,
    Start,
}
impl Operation {
    fn key(&self) -> String {
        match self {
            Self::Refresh => "refresh".into(),
            Self::Budget { id, .. } => format!("budget:{id}"),
            Self::Permission { id, .. } => format!("permission:{id}"),
            Self::Proposal { channel, id, .. } => format!("proposal:{channel}:{id}"),
        }
    }
}
pub fn refresh(session: impl Into<String>) -> ApprovalCommand {
    ApprovalCommand {
        session: session.into(),
        serial: serial(),
        operation: Operation::Refresh,
    }
}
#[derive(Debug)]
pub struct ApprovalUpdate {
    command: ApprovalCommand,
    result: Result<Payload, String>,
}
#[derive(Debug)]
enum Payload {
    Inbox {
        budgets: Result<Vec<TurnBudgetRequestRecord>, String>,
        permissions: Result<Vec<AgentPermissionRequestRecord>, String>,
        proposals: Result<Vec<PeerProposalReviewRecord>, String>,
    },
    Budget(TurnBudgetRequestRecord),
    Permission(AgentPermissionRequestRecord),
    Proposal(PeerProposalActionResponse),
}
#[derive(Default)]
struct Session {
    budgets: Vec<TurnBudgetRequestRecord>,
    permissions: Vec<AgentPermissionRequestRecord>,
    proposals: Vec<PeerProposalReviewRecord>,
    pending: HashSet<String>,
    errors: HashMap<String, String>,
    latest: HashMap<String, u64>,
    revision: u64,
}
#[derive(Default)]
struct Approvals {
    sessions: HashMap<String, Session>,
    queue: Vec<ApprovalCommand>,
    actions: HashMap<Entity, ApprovalCommand>,
    visible: Option<String>,
    rendered: u64,
    selected: Option<String>,
}
struct Root;
impl Component for Root {}
struct ThreadHost(Entity);
impl Component for ThreadHost {}
struct OpenRequest(String);
impl Component for OpenRequest {}
const OPEN_THREAD: u64 = 0x61707001;
pub(crate) fn thread_host(world: &mut World, root: Entity, detail: Entity) {
    world.insert(root, ThreadHost(detail));
}
struct RequestDetails {
    text: String,
    view: Option<Entity>,
}
impl Component for RequestDetails {}
const DETAILS: u64 = 0x617070726f76616c;

// An inbox is a bounded accessory to the transcript, never a second transcript.
fn size_inbox(world: &mut World, root: Entity) {
    let available = world
        .get::<Parent>(root)
        .and_then(|p| world.get::<Rect>(p.entity))
        .map_or(800., |r| r.h);
    let has_content = !children_of(world, root).is_empty();
    let compact = world.has::<ThreadHost>(root);
    world.get_mut::<Layout>(root).unwrap().height = Size::Px(if has_content {
        if compact {
            (available * 0.12).clamp(44., 88.)
        } else {
            (available * 0.22).clamp(64., 176.)
        }
    } else {
        0.
    });
}

fn details(world: &mut World, parent: Entity, body: String) {
    let node = world.spawn((
        Layout {
            width: Size::Fill,
            ..Layout::column()
        },
        RequestDetails {
            text: body,
            view: None,
        },
    ));
    set_parent(world, node, parent);
    controls::Button::mount(
        world,
        node,
        UiId(DETAILS),
        ButtonProps::new("Show request details", HostIntent::Command(DETAILS)),
    )
    .unwrap();
}

pub fn setup(world: &mut World, parent: Entity) -> Entity {
    if world.resource::<Approvals>().is_none() {
        world.insert_resource(Approvals::default());
    }
    let root = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 6.,
            ..Layout::column()
        },
        Root,
        Scroll::default(),
        Focusable,
        Semantics {
            name: Some("Pending decisions — scroll to review requests".into()),
            ..Default::default()
        },
    ));
    set_parent(world, root, parent);
    root
}

pub fn sync(
    world: &mut World,
    root: Entity,
    session: Option<&str>,
    _snapshot: Option<&ChatSnapshot>,
) {
    size_inbox(world, root);
    let host = world.get::<ThreadHost>(root).map(|h| h.0);
    let selected = host
        .and_then(|_| crate::ui::collaboration::selected(world))
        .and_then(|id| id.strip_prefix("approval:"))
        .map(str::to_owned);
    let mut state = std::mem::take(world.resource_mut::<Approvals>().expect("approvals setup"));
    let revision = session
        .and_then(|s| state.sessions.get(s))
        .map_or(0, |s| s.revision);
    if state.visible.as_deref() == session
        && state.rendered == revision
        && state.selected == selected
    {
        *world.resource_mut::<Approvals>().unwrap() = state;
        return;
    }
    for child in children_of(world, root) {
        despawn(world, child);
    }
    if let Some(detail) = host {
        for child in children_of(world, detail) {
            despawn(world, child);
        }
    }
    state.selected = selected.clone();
    state.actions.clear();
    state.visible = session.map(str::to_owned);
    state.rendered = revision;
    if let Some(session) = session {
        let s = state.sessions.entry(session.into()).or_default();
        let mut cards: Vec<(String, String, Vec<(String, Operation)>)> = vec![];
        for row in &s.budgets {
            if row.status != "pending" {
                continue;
            }
            let key = format!("budget:{}", row.request_id);
            let rounds = (row.requested_rounds > 0).then_some(row.requested_rounds);
            cards.push((
                key,
                format!(
                    "Needs approval: {}\nRequesting +{} tool rounds",
                    row.reason, row.requested_rounds
                ),
                vec![
                    (
                        "Approve".into(),
                        Operation::Budget {
                            id: row.request_id.clone(),
                            rounds,
                            approve: true,
                        },
                    ),
                    (
                        "Deny".into(),
                        Operation::Budget {
                            id: row.request_id.clone(),
                            rounds: None,
                            approve: false,
                        },
                    ),
                ],
            ));
        }
        for row in &s.permissions {
            if row.status != AgentPermissionRequestStatus::Pending {
                continue;
            }
            let mut choices = row
                .options
                .iter()
                .map(|o| {
                    (
                        o.name.clone(),
                        Operation::Permission {
                            id: row.request_id.clone(),
                            option: Some(o.option_id.clone()),
                            approve: true,
                        },
                    )
                })
                .collect::<Vec<_>>();
            if choices.is_empty() {
                choices.push((
                    "Allow".into(),
                    Operation::Permission {
                        id: row.request_id.clone(),
                        option: None,
                        approve: true,
                    },
                ));
            }
            choices.push((
                if row.options.is_empty() {
                    "Deny"
                } else {
                    "Cancel"
                }
                .into(),
                Operation::Permission {
                    id: row.request_id.clone(),
                    option: None,
                    approve: false,
                },
            ));
            cards.push((
                format!("permission:{}", row.request_id),
                format!("Agent permission · {}\n{}", row.runtime, row.summary),
                choices,
            ));
        }
        for row in &s.proposals {
            let p = &row.proposal;
            let approved = row.decision.as_ref().is_some_and(|d| d.approved);
            let denied = row.decision.as_ref().is_some_and(|d| !d.approved);
            // This sibling of the timeline is an action inbox, not delegation
            // history. Full historical prompts can consume the entire chat pane
            // when an older session's asynchronous inbox arrives. Accepted work
            // belongs in the compact collaboration markers / exchange reader.
            if denied || row.binding.is_some() || row.receipt.is_some() {
                continue;
            }
            let choices = vec![
                (
                    if approved { "Start" } else { "Approve" }.into(),
                    Operation::Proposal {
                        channel: p.request.channel.channel_id.clone(),
                        id: p.proposal_id.clone(),
                        verb: if approved {
                            ProposalVerb::Start
                        } else {
                            ProposalVerb::Approve
                        },
                    },
                ),
                (
                    "Deny".into(),
                    Operation::Proposal {
                        channel: p.request.channel.channel_id.clone(),
                        id: p.proposal_id.clone(),
                        verb: ProposalVerb::Deny,
                    },
                ),
            ];
            let status = if approved {
                "Approved — explicit Start required"
            } else {
                "Review requested"
            };
            cards.push((
                format!(
                    "proposal:{}:{}",
                    p.request.channel.channel_id, p.proposal_id
                ),
                format!(
                    "{status} · {}\n{}",
                    p.request
                        .instructions
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                        .chars()
                        .take(48)
                        .collect::<String>(),
                    p.request.instructions
                ),
                choices,
            ));
        }
        if host.is_none() && !cards.is_empty() {
            text(
                world,
                root,
                &format!("{} pending decisions · Scroll to review", cards.len()),
                Typography::Secondary,
            )
            .unwrap();
        }
        let mut selected_found = false;
        for (key, title, choices) in cards {
            let destination = if let Some(detail) = host {
                let heading = title.lines().next().unwrap_or("Work request");
                let mut props = ButtonProps::new(
                    format!(
                        "{}  ·  Open thread ›",
                        heading.chars().take(88).collect::<String>()
                    ),
                    HostIntent::Command(OPEN_THREAD),
                );
                props.variant = controls::ButtonVariant::Quiet;
                let button = controls::Button::mount(world, root, UiId(serial()), props).unwrap();
                world.get_mut::<Label>(button.label).unwrap().overflow = TextOverflow::Wrap;
                world.insert(button.root, OpenRequest(format!("approval:{key}")));
                if selected.as_deref() != Some(key.as_str()) {
                    continue;
                }
                selected_found = true;
                detail
            } else {
                root
            };
            let card = Surface::Content
                .mount(world, destination, UiId(serial()))
                .expect("approval card");
            // Keep even genuinely pending requests compact. Never discard or
            // truncate instructions used to decide whether to approve/start.
            let (heading, body) = title.split_once('\n').unwrap_or((&title, ""));
            let heading: String = heading.chars().take(120).collect();
            text(world, card.content, &heading, Typography::Secondary).unwrap();
            if !body.is_empty() || title.chars().count() > 120 {
                details(world, card.content, title);
            }
            if let Some(error) = s.errors.get(&key) {
                text(world, card.content, error, Typography::Secondary).unwrap();
            }
            let buttons = world.spawn((Layout {
                width: Size::Fill,
                gap: 8.,
                ..Layout::row()
            },));
            set_parent(world, buttons, card.content);
            for (label, operation) in choices {
                let id = serial();
                let mut props = ButtonProps::new(label, HostIntent::Command(id));
                props.enabled = !s.pending.contains(&key);
                let h = controls::Button::mount(world, buttons, UiId(id), props).unwrap();
                state.actions.insert(
                    h.root,
                    ApprovalCommand {
                        session: session.into(),
                        serial: id,
                        operation,
                    },
                );
            }
        }
        if let Some(detail) = host {
            if selected.is_some() && !selected_found {
                text(world, detail, "This request is no longer pending. Started work is available from its Working with… entry in the conversation.", Typography::Reading).unwrap();
            }
        }
        for (key, error) in &s.errors {
            if key.starts_with("refresh") {
                text(world, root, error, Typography::Secondary).unwrap();
            }
        }
        if !s.errors.is_empty() {
            let id = serial();
            let mut props = ButtonProps::new("Retry approvals", HostIntent::Command(id));
            props.variant = controls::ButtonVariant::Quiet;
            let h = controls::Button::mount(world, root, UiId(id), props).unwrap();
            state.actions.insert(h.root, refresh(session));
        }
    }
    size_inbox(world, root);
    *world.resource_mut::<Approvals>().unwrap() = state;
}

pub fn action(world: &mut World, intent: &ActionIntent) -> bool {
    if controls::host_intent(world, intent) == Some(HostIntent::Command(OPEN_THREAD)) {
        if let Some(id) = world.get::<OpenRequest>(intent.entity).map(|r| r.0.clone()) {
            crate::ui::collaboration::open(world, id, intent.entity);
        }
        return true;
    }
    if controls::host_intent(world, intent) == Some(HostIntent::Command(DETAILS)) {
        let Some(node) = world.get::<Parent>(intent.entity).map(|p| p.entity) else {
            return false;
        };
        let Some(mut body) = world.remove::<RequestDetails>(node) else {
            return false;
        };
        let open = if let Some(view) = body.view {
            if world.has::<Hidden>(view) {
                world.remove::<Hidden>(view);
                true
            } else {
                world.insert(view, Hidden);
                false
            }
        } else {
            body.view = Some(text(world, node, &body.text, Typography::Reading).unwrap());
            true
        };
        controls::Button::mount(
            world,
            node,
            UiId(DETAILS),
            ButtonProps::new(
                if open {
                    "Hide request details"
                } else {
                    "Show request details"
                },
                HostIntent::Command(DETAILS),
            ),
        )
        .unwrap();
        world.insert(node, body);
        return true;
    }
    if controls::host_intent(world, intent).is_none() {
        return false;
    }
    let Some(state) = world.resource_mut::<Approvals>() else {
        return false;
    };
    let Some(mut command) = state.actions.get(&intent.entity).cloned() else {
        return false;
    };
    if state.visible.as_deref() != Some(command.session.as_str()) {
        return true;
    }
    let s = state.sessions.entry(command.session.clone()).or_default();
    let key = command.operation.key();
    if s.pending.insert(key.clone()) {
        command.serial = serial();
        s.latest.insert(key, command.serial);
        s.revision += 1;
        state.queue.push(command);
    }
    true
}
pub fn drain_commands(world: &mut World) -> Vec<ApprovalCommand> {
    world
        .resource_mut::<Approvals>()
        .map(|s| std::mem::take(&mut s.queue))
        .unwrap_or_default()
}

pub async fn execute(
    client: &MedousaClient,
    command: ApprovalCommand,
) -> Result<ApprovalUpdate, String> {
    let result = execute_inner(client, &command).await;
    // Transport errors are scoped updates too, so UI busy states always settle.
    Ok(ApprovalUpdate { command, result })
}
async fn execute_inner(
    client: &MedousaClient,
    command: &ApprovalCommand,
) -> Result<Payload, String> {
    let err = |e: medousa_sdk::SdkError| e.to_string();
    match &command.operation {
        Operation::Refresh => {
            let budgets = client
                .budget()
                .list(true)
                .await
                .map(|r| {
                    r.requests
                        .into_iter()
                        .filter(|r| r.session_id == command.session)
                        .collect()
                })
                .map_err(err);
            let permissions = client
                .agents()
                .list_permission_requests(Some("pending"), Some(100))
                .await
                .map(|r| {
                    r.requests
                        .into_iter()
                        .filter(|r| r.session_id == command.session)
                        .collect()
                })
                .map_err(err);
            let proposals = async {
                let mut rows = vec![];
                let mut cursor = None;
                let mut seen = HashSet::new();
                loop {
                    let mut query = vec![("session_id", command.session.clone())];
                    if let Some(after) = cursor {
                        query.push(("after", after));
                    }
                    let path = medousa_sdk::path_with_query(
                        &expand_path(ops::COORDINATION_PROPOSALS_GET.path, &[])
                            .map_err(|e| e.to_string())?,
                        &query,
                    );
                    let value = client
                        .transport()
                        .get_json(client.base_url(), &path)
                        .await
                        .map_err(err)?;
                    let page: PeerProposalInboxResponse =
                        serde_json::from_value(value).map_err(|e| e.to_string())?;
                    rows.extend(page.proposals);
                    if let Some(next) = page.next_cursor.filter(|c| seen.insert(c.clone())) {
                        cursor = Some(next);
                    } else {
                        break;
                    }
                }
                Ok(rows)
            }
            .await;
            Ok(Payload::Inbox {
                budgets,
                permissions,
                proposals,
            })
        }
        Operation::Budget {
            id,
            rounds,
            approve,
        } => {
            let r = if *approve {
                client
                    .budget()
                    .approve(
                        id,
                        &TurnBudgetApproveRequest {
                            extra_rounds: *rounds,
                            resolved_by: Some("home-desktop".into()),
                        },
                    )
                    .await
            } else {
                client
                    .budget()
                    .deny(
                        id,
                        &TurnBudgetDenyRequest {
                            resolved_by: Some("home-desktop".into()),
                        },
                    )
                    .await
            }
            .map_err(err)?;
            Ok(Payload::Budget(r.request))
        }
        Operation::Permission {
            id,
            option,
            approve,
        } => {
            let body = AgentPermissionResolveRequest {
                option_id: option.clone(),
                resolved_by: Some("home-desktop".into()),
            };
            let r = if *approve {
                client.agents().approve_permission(id, &body).await
            } else {
                client.agents().deny_permission(id, &body).await
            }
            .map_err(err)?;
            Ok(Payload::Permission(r.request))
        }
        Operation::Proposal { channel, id, verb } => {
            let op = match verb {
                ProposalVerb::Approve => {
                    &ops::COORDINATION_CHANNELS_BY_CHANNEL_ID_PROPOSALS_BY_PROPOSAL_ID_APPROVE_POST
                }
                ProposalVerb::Deny => {
                    &ops::COORDINATION_CHANNELS_BY_CHANNEL_ID_PROPOSALS_BY_PROPOSAL_ID_DENY_POST
                }
                ProposalVerb::Start => {
                    &ops::COORDINATION_CHANNELS_BY_CHANNEL_ID_PROPOSALS_BY_PROPOSAL_ID_DISPATCH_POST
                }
            };
            let path = expand_path(op.path, &[("channel_id", channel), ("proposal_id", id)])?;
            let value = client
                .transport()
                .post_json(client.base_url(), &path, serde_json::json!({}))
                .await
                .map_err(err)?;
            Ok(Payload::Proposal(
                serde_json::from_value(value).map_err(|e| e.to_string())?,
            ))
        }
    }
}

pub fn apply(world: &mut World, update: ApprovalUpdate) {
    let Some(state) = world.resource_mut::<Approvals>() else {
        return;
    };
    let command = update.command;
    let key = command.operation.key();
    let s = state.sessions.entry(command.session.clone()).or_default();
    if s.latest.get(&key).is_some_and(|v| *v > command.serial) {
        return;
    }
    s.latest.insert(key.clone(), command.serial);
    s.pending.remove(&key);
    s.revision += 1;
    match update.result {
        Err(error) => {
            s.errors.insert(key, error);
        }
        Ok(payload) => {
            s.errors.remove(&key);
            match payload {
                Payload::Inbox {
                    budgets,
                    permissions,
                    proposals,
                } => {
                    // A refresh begun before any explicit action cannot resurrect
                    // the old pending state when it arrives after that action.
                    let stale = s
                        .latest
                        .iter()
                        .any(|(k, v)| k != "refresh" && *v > command.serial);
                    if !stale {
                        match budgets {
                            Ok(rows) => {
                                s.budgets = rows;
                                s.errors.remove("refresh:budget");
                            }
                            Err(e) => {
                                s.errors.insert("refresh:budget".into(), e);
                            }
                        }
                        match permissions {
                            Ok(rows) => {
                                s.permissions = rows;
                                s.errors.remove("refresh:permission");
                            }
                            Err(e) => {
                                s.errors.insert("refresh:permission".into(), e);
                            }
                        }
                        match proposals {
                            Ok(rows) => {
                                s.proposals = rows;
                                s.errors.remove("refresh:proposal");
                            }
                            Err(e) => {
                                s.errors.insert("refresh:proposal".into(), e);
                            }
                        }
                    }
                }
                Payload::Budget(row) => {
                    if row.session_id == command.session {
                        s.budgets.retain(|r| r.request_id != row.request_id);
                        if row.status == "pending" {
                            s.budgets.push(row);
                        }
                    }
                }
                Payload::Permission(row) => {
                    if row.session_id == command.session {
                        s.permissions.retain(|r| r.request_id != row.request_id);
                        if row.status == AgentPermissionRequestStatus::Pending {
                            s.permissions.push(row);
                        }
                    }
                }
                Payload::Proposal(response) => {
                    if let Operation::Proposal { channel, id, verb } = command.operation {
                        if response.proposal_id == id {
                            if let Some(row) = s.proposals.iter_mut().find(|r| {
                                r.proposal.proposal_id == id
                                    && r.proposal.request.channel.channel_id == channel
                            }) {
                                match verb {
                                    ProposalVerb::Approve | ProposalVerb::Deny => {
                                        row.decision = Some(PeerProposalDecision {
                                            proposal_id: id,
                                            owner_principal_id: row
                                                .proposal
                                                .request
                                                .owner_principal_id
                                                .clone(),
                                            approved: matches!(verb, ProposalVerb::Approve),
                                        })
                                    }
                                    ProposalVerb::Start => row.binding = response.binding,
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
#[cfg(test)]
#[path = "approvals_tests.rs"]
mod tests;
