//! Linked assignment reading. Requests alone never establish chat identity.
use super::*;
#[cfg(test)]
#[path = "collaboration_peers_tests.rs"]
pub(crate) mod tests;
use medousa_sdk::generated::ops;
use medousa_types::coordination::*;

pub(crate) const EXECUTION: u64 = 0x636f6c05;
#[derive(Default)]
pub(super) struct State {
    rows: Vec<PeerProposalReviewRecord>,
    inflight: bool,
    pub(super) refreshed: Option<Instant>,
    error: Option<String>,
    history: Option<(String, Chat)>,
    history_error: Option<String>,
}
#[derive(Clone)]
pub(crate) struct Reading {
    rows: Vec<PeerProposalReviewRecord>,
    selected: Option<String>,
    history: Option<Result<History, String>>,
}
#[derive(Clone)]
struct History {
    session: String,
    authority: String,
    turns: Vec<serde_json::Value>,
    active: Option<medousa_types::ActiveSessionTurn>,
}
impl Reading {
    pub(crate) fn stream(&self) -> Option<(String, String, String)> {
        let history = self.history.as_ref()?.as_ref().ok()?;
        let active = history.active.as_ref()?;
        Some((
            self.selected.clone()?,
            active.turn_id.clone(),
            active.stream_url.clone(),
        ))
    }
}
fn key(row: &PeerProposalReviewRecord) -> String {
    // Coordination channel + proposal is the durable UI key, not provider/session labels.
    format!(
        "peer:{}",
        serde_json::to_string(&(&row.proposal.request.channel, &row.proposal.proposal_id)).unwrap()
    )
}
fn bound<'a>(row: &'a PeerProposalReviewRecord, session: &str, authority: &str) -> Option<&'a str> {
    let request = &row.proposal.request;
    let binding = row
        .binding
        .as_ref()
        .or_else(|| row.receipt.as_ref().map(|r| &r.binding))?;
    if authority.is_empty()
        || request.owner_session.session_id.as_str() != session
        || request.owner_session.authority_id.as_str() != authority
        || binding.execution_session.authority_id.as_str() != authority
        || binding.execution_session.session_id.as_str().is_empty()
        || binding.execution_session.session_id.as_str() == session
        || binding.assignment_id != request.assignment_id
        || binding.owner_principal_id != request.owner_principal_id
        || binding.channel != request.channel
        || binding.target != request.target
        || row.receipt.as_ref().is_some_and(|r| &r.binding != binding)
    {
        return None;
    }
    Some(binding.execution_session.session_id.as_str())
}
fn name(row: &PeerProposalReviewRecord) -> String {
    row.proposal
        .request
        .instructions
        .lines()
        .find(|s| !s.trim().is_empty())
        .unwrap_or("Related work")
        .trim()
        .chars()
        .take(80)
        .collect()
}
fn status(row: &PeerProposalReviewRecord) -> &'static str {
    if let Some(receipt) = &row.receipt {
        return match receipt.outcome {
            PeerAssignmentOutcome::Completed => "Result returned · Medousa review is separate",
            PeerAssignmentOutcome::Failed => "Execution failed",
            PeerAssignmentOutcome::Cancelled => "Execution cancelled",
            PeerAssignmentOutcome::Interrupted => "Execution interrupted",
        };
    }
    match row.progress.as_ref().map(|p| p.state) {
        Some(PeerExecutionState::Running) => "Working",
        Some(PeerExecutionState::Blocked) => "Needs attention",
        Some(PeerExecutionState::AwaitingReceipt) => "Awaiting recorded result",
        Some(PeerExecutionState::Unobserved) => "Execution status unavailable",
        _ => "Assignment accepted",
    }
}
fn anchor<'a>(snapshot: &'a ChatSnapshot, row: &PeerProposalReviewRecord) -> Option<&'a str> {
    snapshot.agent_groups.iter().find_map(|group| {
        let matches = group.entries.iter().any(|entry| matches!(entry,
            medousa_app_core::AgentPlacementEntry::Proposal { proposal_id, origin_proposal_id, .. }
                if proposal_id == &row.proposal.proposal_id || origin_proposal_id == &row.proposal.proposal_id));
        matches.then_some(group.anchor_item_id.as_deref()).flatten()
            .filter(|id| snapshot.items.iter().any(|item| item.id == *id))
    })
}
pub(super) fn request(
    state: &mut State,
    selected: Option<&str>,
    snapshot: &ChatSnapshot,
    model: &Model,
) {
    if state.inflight
        || state.refreshed.is_some_and(|t| {
            t.elapsed()
                < Duration::from_secs(if selected.is_some() || snapshot.active_turn_id.is_some() {
                    3
                } else {
                    15
                })
        })
    {
        return;
    }
    let Some(tx) = &model.commands else {
        return;
    };
    let command = Command::PeerCollaboration {
        session: snapshot.session_id.clone(),
        authority: snapshot.authority_id.clone(),
        selected: selected
            .filter(|s| s.starts_with("peer:"))
            .map(str::to_owned),
    };
    state.inflight = tx
        .send(Request {
            generation: model.generation,
            command,
        })
        .is_ok();
    if !state.inflight {
        state.error = Some("Collaboration connection unavailable. Refresh to retry.".into());
        state.refreshed = Some(Instant::now());
    }
}
pub(crate) fn receive(world: &mut World, session: &str, result: Result<Reading, String>) {
    let model = world.resource_mut::<Model>().unwrap();
    if model.collaboration.session != session {
        return;
    }
    let selected = model.collaboration.selected.clone();
    let state = &mut model.collaboration.peers;
    state.inflight = false;
    state.refreshed = Some(Instant::now());
    match result {
        Err(e) => state.error = Some(e),
        Ok(reading) => {
            state.error = None;
            state.rows = reading.rows;
            if selected == reading.selected {
                if let Some(result) = reading.history {
                    match result {
                        Ok(history) => {
                            let id = reading.selected.unwrap();
                            if state.history.as_ref().is_none_or(|(key, _)| key != &id) {
                                state.history = Some((id, Chat::default()));
                            }
                            let chat = &mut state.history.as_mut().unwrap().1;
                            let active = history.active.map(|t| medousa_app_core::TurnFact {
                                turn_id: t.turn_id,
                                mode: "interactive".into(),
                                phase: "streaming".into(),
                                message_id: None,
                                workspace_card_id: None,
                                reusable_stream: true,
                                budget_request_id: None,
                                requested_rounds: None,
                            });
                            state.history_error = chat
                                .hydrate(history.session, history.authority, history.turns, active)
                                .err();
                        }
                        Err(e) => state.history_error = Some(e),
                    }
                }
            }
        }
    }
}
pub(super) fn earlier(state: &State, snapshot: &ChatSnapshot, id: &str) -> bool {
    state
        .rows
        .iter()
        .any(|row| key(row) == id && anchor(snapshot, row).is_none())
}
pub(super) fn markers(
    world: &mut World,
    ui: Ui,
    snapshot: &ChatSnapshot,
    state: &State,
    open: bool,
    earlier_open: bool,
) -> usize {
    let mut count = 0;
    for row in &state.rows {
        let parent = if let Some(anchor) = anchor(snapshot, row) {
            if open {
                continue;
            }
            timeline::mounted_row(world, ui.timeline, anchor)
        } else {
            count += 1;
            earlier_open.then_some(ui.exchange.earlier_rows)
        };
        if let Some(parent) = parent {
            marker_entry(
                world,
                parent,
                &key(row),
                format!("Working with {} · {}  ›", name(row), status(row)),
            );
        }
    }
    count
}
fn project(row: &PeerProposalReviewRecord, history: Option<&ChatSnapshot>) -> ChatSnapshot {
    let id = key(row);
    let mut items = vec![
        serde_json::json!({"id":format!("{id}:assignment"),"role":"system","content":format!("Assignment\n{}",row.proposal.request.instructions)}),
    ];
    if let Some(history) = history {
        items.extend(history.items.iter().map(|item| {
            let mut item = serde_json::to_value(item).unwrap();
            item["id"] = serde_json::json!(format!(
                "{id}:execution:{}",
                item["id"].as_str().unwrap_or("")
            ));
            item
        }));
    }
    if let Some(receipt) = &row.receipt {
        items.push(serde_json::json!({"id":format!("{id}:receipt:{}",receipt.receipt_id),"role":"system","content":format!("Recorded result · {}\n{}",status(row),receipt.result)}));
    }
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_vec(&items).unwrap().hash(&mut hash);
    serde_json::from_value(serde_json::json!({"sessionId":id,"authorityId":"peer-reading-view","revision":hash.finish(),"items":items})).expect("peer reading snapshot")
}
pub(super) fn render(world: &mut World, ui: Ui, state: &State, id: &str, snapshot: &ChatSnapshot) {
    let Some(row) = state.rows.iter().find(|row| key(row) == id) else {
        set_label(
            world,
            ui.exchange.status,
            state
                .error
                .clone()
                .unwrap_or_else(|| "Linked assignment unavailable. Refresh to retry.".into()),
        );
        return;
    };
    let history = state
        .history
        .as_ref()
        .filter(|(key, _)| key == id)
        .and_then(|(_, chat)| chat.snapshot());
    let available = bound(row, &snapshot.session_id, &snapshot.authority_id).is_some();
    visible(world, ui.exchange.execution, available);
    set_label(world, ui.exchange.title, format!("Medousa ↔ {}", name(row)));
    let notice = state
        .error
        .as_deref()
        .or(state.history_error.as_deref())
        .unwrap_or(if history.is_some() {
            "Read-only exchange · Live execution history"
        } else {
            "Read-only exchange · Loading recorded execution history…"
        });
    set_label(
        world,
        ui.exchange.status,
        format!("{} · {notice}", status(row)),
    );
    world.insert(
        ui.exchange.timeline,
        timeline::ReadingIdentity {
            sender: "Execution input".into(),
            recipient: format!("{}", row.proposal.request.target.runtime.as_str()),
        },
    );
    timeline::sync(world, ui.exchange.timeline, Some(&project(row, history)));
}
pub(super) fn open_execution(world: &mut World) {
    let model = world.resource::<Model>().unwrap();
    let Some(snapshot) = model.chat.snapshot() else {
        return;
    };
    let target = model
        .collaboration
        .peers
        .rows
        .iter()
        .find(|row| model.collaboration.selected.as_deref() == Some(key(row).as_str()))
        .and_then(|row| bound(row, &snapshot.session_id, &snapshot.authority_id))
        .map(str::to_owned);
    if let Some(target) = target {
        conversations::open(world, Some(target));
    }
}

pub(crate) fn stream_event(
    world: &mut World,
    session: &str,
    id: &str,
    event: Result<medousa_types::TurnStreamEnvelopeV3, String>,
) {
    let model = world.resource_mut::<Model>().unwrap();
    if model.collaboration.session != session || model.collaboration.selected.as_deref() != Some(id)
    {
        return;
    }
    let state = &mut model.collaboration.peers;
    let Some((key, chat)) = &mut state.history else {
        return;
    };
    if key != id {
        return;
    }
    state.history_error = match event {
        Ok(event) => chat.apply(event).err(),
        Err(error) => Some(error),
    };
}

/// Each retained parent route has at most one read-only stream attachment.
/// Selection changes abort only the reader, never the executing turn.
#[derive(Default)]
pub(crate) struct Subscriptions(std::sync::Mutex<HashMap<u64, Subscription>>);
struct Subscription {
    selected: Option<String>,
    task: Option<(String, tokio::task::JoinHandle<()>)>,
}
impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some((_, task)) = &self.task {
            task.abort();
        }
    }
}
impl Subscriptions {
    pub(crate) fn select(&self, generation: u64, selected: Option<String>) {
        let mut subscriptions = self.0.lock().unwrap();
        let slot = subscriptions
            .entry(generation)
            .or_insert_with(|| Subscription {
                selected: None,
                task: None,
            });
        if slot.selected != selected {
            if let Some((_, task)) = slot.task.take() {
                task.abort();
            }
            slot.selected = selected;
        }
    }
    pub(crate) fn attach(
        &self,
        generation: u64,
        session: String,
        reading: &Reading,
        client: std::sync::Arc<medousa_sdk::MedousaClient>,
        tx: rndr_core::feed::Sender<Reply>,
    ) {
        let Some((key, turn, url)) = reading.stream() else {
            return;
        };
        let Ok(path) = crate::backend::stream_path(&url, client.base_url()).map(str::to_owned)
        else {
            return;
        };
        let mut subscriptions = self.0.lock().unwrap();
        let Some(slot) = subscriptions
            .get_mut(&generation)
            .filter(|s| s.selected.as_deref() == Some(&key))
        else {
            return;
        };
        if slot
            .task
            .as_ref()
            .is_some_and(|(id, task)| id == &turn && !task.is_finished())
        {
            return;
        }
        if let Some((_, task)) = slot.task.take() {
            task.abort();
        }
        let expected_turn = turn.clone();
        slot.task = Some((
            turn,
            tokio::spawn(async move {
                use futures_util::StreamExt;
                let api = client.interactive();
                let mut stream = api.stream_reconnecting_v3(path);
                while let Some(event) = stream.next().await {
                    let terminal = event.as_ref().is_ok_and(|event| event.event.is_terminal());
                    let event = event.map_err(|_| "Live execution connection interrupted. Refresh to reconnect; recorded text is retained.".to_string()).and_then(|event| {
                    if event.turn_id != expected_turn { Err("Execution stream identity did not match its ticket.".into()) } else { Ok(event) }
                });
                    let failed = event.is_err();
                    if tx
                        .send(Reply {
                            generation,
                            update: Update::PeerStream {
                                session: session.clone(),
                                key: key.clone(),
                                event,
                            },
                        })
                        .is_err()
                    {
                        break;
                    }
                    if terminal || failed {
                        break;
                    }
                }
            }),
        ));
    }
}

pub(crate) async fn fetch(
    client: &medousa_sdk::MedousaClient,
    session: &str,
    authority: &str,
    selected: Option<&str>,
) -> Result<Reading, String> {
    let mut rows = Vec::new();
    let mut cursor = None;
    let mut seen = std::collections::HashSet::new();
    loop {
        let mut query = vec![("session_id", session.to_string())];
        if let Some(cursor) = cursor {
            query.push(("after", cursor));
        }
        let path = medousa_sdk::path_with_query(ops::COORDINATION_PROPOSALS_GET.path, &query);
        let value = client.transport().get_json(client.base_url(), &path).await.map_err(|_| "Linked collaboration activity could not be refreshed. Recorded activity remains available.".to_string())?;
        let page: PeerProposalInboxResponse = serde_json::from_value(value)
            .map_err(|_| "Invalid linked collaboration response.".to_string())?;
        rows.extend(
            page.proposals
                .into_iter()
                .filter(|row| bound(row, session, authority).is_some()),
        );
        match page.next_cursor {
            Some(next) if seen.insert(next.clone()) => cursor = Some(next),
            Some(_) => {
                return Err("Linked collaboration pagination repeated. Refresh to retry.".into());
            }
            None => break,
        }
        if seen.len() >= 100 {
            return Err("Linked collaboration discovery exceeded its page bound.".into());
        }
    }
    rows.sort_by_key(key);
    rows.dedup_by_key(|row| key(row));
    let mut reading = Reading {
        rows,
        selected: selected.map(str::to_owned),
        history: None,
    };
    if let Some(row) = reading
        .rows
        .iter()
        .find(|row| Some(key(row).as_str()) == selected)
    {
        let target = bound(row, session, authority).unwrap();
        reading.history = Some(async {
            let before = client.sessions().active_turn(target).await.map_err(|_| "Execution status unavailable. Refresh to retry.".to_string())?;
            let history = client.sessions().history(target).await.map_err(|_| "Execution history unavailable. The assignment and recorded result remain available.".to_string())?;
            let active = client.sessions().active_turn(target).await.map_err(|_| "Execution status unavailable. Refresh to retry.".to_string())?;
            if before.turn.as_ref().map(|t| &t.turn_id) != active.turn.as_ref().map(|t| &t.turn_id)
                || active.turn.as_ref().is_some_and(|t| t.session_id != target) {
                return Err("Execution changed during reconciliation. Refresh to retry.".into());
            }
            if history.session_id != target || history.authority_id.as_str() != authority {
                return Err("Execution history identity did not match its binding.".into());
            }
            Ok(History { session: target.into(), authority: authority.into(), turns: history.turns.into_iter().map(|entry| serde_json::to_value(entry).unwrap()).collect(), active: active.turn })
        }.await);
    }
    Ok(reading)
}
