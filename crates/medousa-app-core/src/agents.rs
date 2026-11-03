//! Tool / agent causal relationships and turn placement projection.

use std::collections::{HashMap, HashSet};

use crate::contract::{
    AgentPlacementEntry, AgentPlacementGroup, ChatItem, ChatItemRole, ChatLane, ChatRelationship,
    ChatSegment, ProposalFact, WorkerPlacementFact,
};

fn is_delegation_tool(name: &str) -> bool {
    // Match the renderer contract: peer[_.]?propose|handoff, workshop[_.]spawn,
    // cognition_workshop_mutate (case-insensitive).
    let lower = name.to_ascii_lowercase();
    lower.contains("cognition_workshop_mutate")
        || lower.contains("workshop_spawn")
        || lower.contains("workshop.spawn")
        || {
            let peer = lower.find("peer").map(|idx| &lower[idx + 4..]).unwrap_or("");
            let rest = peer
                .strip_prefix('_')
                .or_else(|| peer.strip_prefix('.'))
                .unwrap_or(peer);
            rest.starts_with("propose") || rest.starts_with("handoff")
        }
}

/// Worker causal link: work_id, parent_turn_id, parent_item_id, reply_to_work_id.
pub type WorkerCausalLink = (String, Option<String>, Option<String>, Option<String>);

/// Collect stable tool→turn and agent spawn links from timeline items.
pub fn relationships_from_items(
    items: &[ChatItem],
    workers: &[WorkerCausalLink],
) -> Vec<ChatRelationship> {
    let mut relationships = Vec::new();

    for item in items {
        let Some(turn_id) = item.turn_id.as_ref().map(|id| id.trim()).filter(|id| !id.is_empty())
        else {
            continue;
        };
        let mut runs = Vec::new();
        if let Some(tool_runs) = &item.tool_runs {
            runs.extend(tool_runs.iter());
        }
        if let Some(segments) = &item.segments {
            for segment in segments {
                if let ChatSegment::ToolGroup { runs: group_runs, .. } = segment {
                    runs.extend(group_runs.iter());
                }
            }
        }
        let mut seen = HashSet::new();
        for run in runs {
            if !seen.insert(run.run_id.clone()) {
                continue;
            }
            relationships.push(ChatRelationship::ToolBelongsTo {
                tool_run_id: run.run_id.clone(),
                turn_id: turn_id.to_string(),
                item_id: item.id.clone(),
            });
            let _ = is_delegation_tool(&run.tool_name);
        }
    }

    for (work_id, parent_turn_id, parent_item_id, reply_to) in workers {
        if let Some(reply_to_work_id) = reply_to.as_ref().filter(|id| !id.is_empty()) {
            relationships.push(ChatRelationship::AgentFollowUp {
                work_id: work_id.clone(),
                reply_to_work_id: reply_to_work_id.clone(),
                parent_turn_id: parent_turn_id.clone(),
                parent_item_id: parent_item_id.clone(),
            });
        } else {
            relationships.push(ChatRelationship::AgentSpawn {
                work_id: work_id.clone(),
                parent_turn_id: parent_turn_id.clone(),
                parent_item_id: parent_item_id.clone(),
            });
        }
    }

    relationships
}

/// Prefer an immutable spawn parent: never reinterpret a worker-lane result bubble
/// as the spawn location.
pub fn resolve_worker_parent_item_id<'a>(
    items: &'a [ChatItem],
    parent_item_id: Option<&str>,
    parent_turn_id: Option<&str>,
    work_id: Option<&str>,
) -> Option<&'a str> {
    if let Some(id) = parent_item_id
        && let Some(item) = items.iter().find(|item| {
            item.id == id && !matches!(item.lane, Some(ChatLane::Worker))
        })
    {
        return Some(item.id.as_str());
    }
    if let Some(turn_id) = parent_turn_id
        && let Some(item) = items.iter().find(|item| {
            item.turn_id.as_deref() == Some(turn_id)
                && !matches!(item.lane, Some(ChatLane::Worker))
        })
    {
        return Some(item.id.as_str());
    }
    if parent_item_id.is_none()
        && parent_turn_id.is_none()
        && let Some(work_id) = work_id
        && let Some(item) = items.iter().find(|item| {
            item.work_id.as_deref() == Some(work_id)
                && !matches!(item.lane, Some(ChatLane::Worker))
        })
    {
        return Some(item.id.as_str());
    }
    None
}

fn parse_millis(value: Option<&str>) -> i64 {
    let Some(raw) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return 0;
    };
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(raw) {
        return dt.timestamp_millis();
    }
    0
}

fn turn_anchor(items: &[ChatItem], visible: &HashSet<&str>, index: isize) -> Option<String> {
    if index < 0 {
        return None;
    }
    let mut start = index as usize;
    while start > 0 && items[start].role != ChatItemRole::User {
        start -= 1;
    }
    // Use the stable user turn even when its transient handoff shell disappears.
    if items[start].role == ChatItemRole::User && visible.contains(items[start].id.as_str()) {
        return Some(items[start].id.clone());
    }
    for item in items.iter().take(items.len()).skip(start + 1) {
        if item.role == ChatItemRole::User {
            break;
        }
        if item.role == ChatItemRole::Assistant
            && !matches!(item.lane, Some(ChatLane::Worker))
            && visible.contains(item.id.as_str())
        {
            return Some(item.id.clone());
        }
    }
    if visible.contains(items[start].id.as_str()) {
        return Some(items[start].id.clone());
    }
    let idx = index as usize;
    if visible.contains(items[idx].id.as_str()) {
        Some(items[idx].id.clone())
    } else {
        None
    }
}

fn worker_index(items: &[ChatItem], worker: &WorkerPlacementFact) -> isize {
    // A turn's active message may have advanced to its worker result. Never
    // reinterpret that arrival as the spawn location.
    if let Some(parent) = worker.parent_item_id.as_deref()
        && let Some((index, _)) = items.iter().enumerate().find(|(_, item)| {
            item.id == parent && !matches!(item.lane, Some(ChatLane::Worker))
        })
    {
        return index as isize;
    }
    if let Some(turn_id) = worker.parent_turn_id.as_deref()
        && let Some((index, _)) = items.iter().enumerate().find(|(_, item)| {
            item.turn_id.as_deref() == Some(turn_id)
                && !matches!(item.lane, Some(ChatLane::Worker))
        })
    {
        return index as isize;
    }
    if worker.parent_item_id.is_none()
        && worker.parent_turn_id.is_none()
        && let Some((index, _)) = items.iter().enumerate().find(|(_, item)| {
            item.work_id.as_deref() == Some(worker.work_id.as_str())
                && !matches!(item.lane, Some(ChatLane::Worker))
        })
    {
        return index as isize;
    }
    -1
}

fn proposal_thread_key(proposal: &ProposalFact) -> String {
    match (
        proposal.binding_authority_id.as_deref(),
        proposal.binding_session_id.as_deref(),
        proposal.execution_runtime_id.as_deref(),
    ) {
        (Some(authority), Some(session), runtime) => serde_json::to_string(&(
            authority,
            runtime.unwrap_or(""),
            session,
        ))
        .unwrap_or_else(|_| format!("proposal:{}", proposal.proposal_id)),
        _ => format!("proposal:{}", proposal.proposal_id),
    }
}

fn matches_source_coordinate(
    coordinate_authority: &str,
    coordinate_session: &str,
    coordinate_seq: u64,
    selection_authority: &str,
    selection_session: &str,
    after_entry_seq: Option<u64>,
    through_entry_seq: u64,
) -> bool {
    coordinate_authority == selection_authority
        && coordinate_session == selection_session
        && coordinate_seq > after_entry_seq.unwrap_or(0)
        && coordinate_seq <= through_entry_seq
}

fn proposal_source_index(
    items: &[ChatItem],
    proposal: &ProposalFact,
    scope_authority: Option<&str>,
    scope_session: Option<&str>,
) -> isize {
    let sources = &proposal.sources;
    let mut index: isize = -1;
    for (i, item) in items.iter().enumerate() {
        if item.role != ChatItemRole::User {
            continue;
        }
        let Some(transcript) = &item.transcript else {
            continue;
        };
        let coordinates = std::iter::once((
            transcript.authority_id.as_str(),
            transcript.session_id.as_str(),
            transcript.entry_seq,
        ))
        .chain(transcript.source.as_ref().map(|source| {
            (
                source.authority_id.as_str(),
                source.session_id.as_str(),
                source.entry_seq,
            )
        }));
        if sources.iter().any(|selection| {
            coordinates.clone().any(|(authority, session, seq)| {
                matches_source_coordinate(
                    authority,
                    session,
                    seq,
                    &selection.authority_id,
                    &selection.session_id,
                    selection.after_entry_seq,
                    selection.through_entry_seq,
                )
            })
        }) {
            index = i as isize;
        }
    }

    // Live user rows have an admitted timestamp before history has supplied
    // their entry coordinate. Only use it for a range beyond this daemon's
    // loaded history, never to reinterpret an older selected context range.
    if let (Some(scope_authority), Some(scope_session)) = (scope_authority, scope_session) {
        let local_sources: Vec<_> = sources
            .iter()
            .filter(|selection| {
                selection.authority_id == scope_authority && selection.session_id == scope_session
            })
            .collect();
        let latest_seq = items
            .iter()
            .filter_map(|item| {
                let transcript = item.transcript.as_ref()?;
                if transcript.authority_id == scope_authority
                    && transcript.session_id == scope_session
                {
                    Some(transcript.entry_seq)
                } else {
                    None
                }
            })
            .max()
            .unwrap_or(0);
        let created = parse_millis(proposal.created_at.as_deref());
        if local_sources
            .iter()
            .any(|selection| selection.through_entry_seq > latest_seq)
        {
            for (i, item) in items.iter().enumerate().skip((index + 1).max(0) as usize) {
                if item.role == ChatItemRole::User
                    && item.transcript.is_none()
                    && parse_millis(item.created_at.as_deref()) > 0
                    && parse_millis(item.created_at.as_deref()) <= created
                {
                    index = i as isize;
                }
            }
        }
    }
    index
}

fn delegation_ids_for_proposal(proposal: &ProposalFact, origin: &ProposalFact) -> Vec<String> {
    [
        Some(origin.proposal_id.clone()),
        origin.assignment_id.clone(),
        Some(proposal.proposal_id.clone()),
        proposal.assignment_id.clone(),
    ]
    .into_iter()
    .flatten()
    .filter(|id| !id.is_empty())
    .collect()
}

fn find_segment_position(
    turn: &[(usize, &ChatItem)],
    visible: &HashSet<&str>,
    ids: &[String],
) -> Option<(usize, usize, usize, Option<String>)> {
    for &(message_index, message) in turn {
        if !visible.contains(message.id.as_str()) {
            continue;
        }
        let Some(segments) = &message.segments else {
            continue;
        };
        for (segment_index, segment) in segments.iter().enumerate() {
            match segment {
                ChatSegment::Handoff {
                    text,
                    work_id,
                    handoff_kind,
                } => {
                    let handoff_id = work_id.as_deref().unwrap_or("");
                    if !ids.iter().any(|id| id == handoff_id) {
                        continue;
                    }
                    // TS: nodeId only when handoff text is non-empty;
                    // key is workId ?? handoffKind.
                    let node_id = if text.trim().is_empty() {
                        None
                    } else {
                        let key = work_id
                            .as_deref()
                            .filter(|id| !id.is_empty())
                            .unwrap_or(handoff_kind.as_str());
                        Some(format!(
                            "{}:segment:handoff:{key}:{segment_index}",
                            message.id
                        ))
                    };
                    return Some((message_index, segment_index, 0, node_id));
                }
                ChatSegment::ToolGroup { group_id, runs, .. } => {
                    let run_index = runs.iter().position(|run| {
                        if !is_delegation_tool(&run.tool_name) {
                            return false;
                        }
                        let summary = run.output_summary.as_deref().unwrap_or("");
                        let params = run
                            .input_params
                            .as_ref()
                            .and_then(|params| serde_json::to_string(params).ok())
                            .unwrap_or_default();
                        ids.iter()
                            .any(|id| summary.contains(id.as_str()) || params.contains(id.as_str()))
                    });
                    if let Some(run_index) = run_index {
                        let node_id = Some(format!("{}:segment:{group_id}", message.id));
                        return Some((message_index, segment_index, run_index, node_id));
                    }
                }
                _ => {}
            }
        }
    }
    None
}

/// Project proposals and workers onto initiating turns with mixed spawn order.
///
/// `visible_item_ids` are the painted timeline ids (handoff shells may be hidden).
/// `scope_*` enable live-user anchoring for ranges beyond loaded history.
pub fn project_agent_placement(
    items: &[ChatItem],
    visible_item_ids: &[String],
    proposals: &[ProposalFact],
    workers: &[WorkerPlacementFact],
    scope_authority: Option<&str>,
    scope_session: Option<&str>,
) -> Vec<AgentPlacementGroup> {
    let visible: HashSet<&str> = visible_item_ids.iter().map(String::as_str).collect();

    #[derive(Clone)]
    enum PendingKind {
        Proposal {
            proposal_id: String,
            origin_proposal_id: String,
            created_at_ms: i64,
        },
        Worker {
            work_id: String,
        },
    }

    struct PendingEntry {
        kind: PendingKind,
        node_id: Option<String>,
        position: Option<(usize, usize, usize)>,
    }

    struct GroupBuilder {
        entries: Vec<PendingEntry>,
    }

    let mut groups: HashMap<Option<String>, GroupBuilder> = HashMap::new();
    let mut anchor_order: Vec<Option<String>> = Vec::new();

    // Collapse follow-ups that share a durable execution conversation onto the
    // original occurrence while retaining the latest proposal row id.
    let mut sorted_proposals = proposals.to_vec();
    sorted_proposals.sort_by_key(|proposal| parse_millis(proposal.created_at.as_deref()));

    let mut threads: HashMap<String, (ProposalFact, ProposalFact)> = HashMap::new();
    let mut thread_order: Vec<String> = Vec::new();
    for proposal in sorted_proposals {
        let key = proposal_thread_key(&proposal);
        if let Some(existing) = threads.get_mut(&key) {
            existing.0 = proposal;
        } else {
            thread_order.push(key.clone());
            threads.insert(key, (proposal.clone(), proposal));
        }
    }

    let origins: HashMap<String, ProposalFact> = threads
        .values()
        .map(|(row, origin)| (row.proposal_id.clone(), origin.clone()))
        .collect();

    for key in thread_order {
        let Some((proposal, origin)) = threads.remove(&key) else {
            continue;
        };
        let index = proposal_source_index(items, &origin, scope_authority, scope_session);
        let anchor = turn_anchor(items, &visible, index);
        let created_at_ms = parse_millis(proposal.created_at.as_deref());
        if !groups.contains_key(&anchor) {
            anchor_order.push(anchor.clone());
        }
        groups
            .entry(anchor)
            .or_insert_with(|| GroupBuilder {
                entries: Vec::new(),
            })
            .entries
            .push(PendingEntry {
                kind: PendingKind::Proposal {
                    proposal_id: proposal.proposal_id,
                    origin_proposal_id: origin.proposal_id,
                    created_at_ms,
                },
                node_id: None,
                position: None,
            });
    }

    for worker in workers {
        let index = worker_index(items, worker);
        let anchor = turn_anchor(items, &visible, index);
        if !groups.contains_key(&anchor) {
            anchor_order.push(anchor.clone());
        }
        groups
            .entry(anchor)
            .or_insert_with(|| GroupBuilder {
                entries: Vec::new(),
            })
            .entries
            .push(PendingEntry {
                kind: PendingKind::Worker {
                    work_id: worker.work_id.clone(),
                },
                node_id: None,
                position: None,
            });
    }

    let mut result = Vec::new();
    for anchor in anchor_order {
        let Some(mut builder) = groups.remove(&anchor) else {
            continue;
        };
        let start = anchor
            .as_ref()
            .and_then(|id| items.iter().position(|item| &item.id == id))
            .map(|index| index as isize)
            .unwrap_or(-1);
        let next_user = if start < 0 {
            -1
        } else {
            items
                .iter()
                .enumerate()
                .skip(start as usize + 1)
                .find(|(_, item)| item.role == ChatItemRole::User)
                .map(|(index, _)| index as isize)
                .unwrap_or(-1)
        };
        let turn: Vec<(usize, &ChatItem)> = if start < 0 {
            Vec::new()
        } else {
            let end = if next_user < 0 {
                items.len()
            } else {
                next_user as usize
            };
            items[start as usize..end].iter().enumerate().collect()
        };

        for entry in &mut builder.entries {
            let ids: Vec<String> = match &entry.kind {
                PendingKind::Worker { work_id } => vec![work_id.clone()],
                PendingKind::Proposal {
                    proposal_id,
                    origin_proposal_id,
                    ..
                } => {
                    let origin = origins.get(proposal_id).cloned().unwrap_or_else(|| {
                        ProposalFact {
                            proposal_id: origin_proposal_id.clone(),
                            assignment_id: None,
                            created_at: None,
                            execution_runtime_id: None,
                            binding_authority_id: None,
                            binding_session_id: None,
                            sources: Vec::new(),
                        }
                    });
                    let live = proposals
                        .iter()
                        .find(|row| row.proposal_id == *proposal_id)
                        .cloned()
                        .unwrap_or_else(|| ProposalFact {
                            proposal_id: proposal_id.clone(),
                            assignment_id: None,
                            created_at: None,
                            execution_runtime_id: None,
                            binding_authority_id: None,
                            binding_session_id: None,
                            sources: Vec::new(),
                        });
                    delegation_ids_for_proposal(&live, &origin)
                }
            };
            if let Some((message_index, segment_index, run_index, node_id)) =
                find_segment_position(&turn, &visible, &ids)
            {
                entry.position = Some((message_index, segment_index, run_index));
                entry.node_id = node_id;
            }
        }

        // Segment coordinates beat unstable card/work IDs. Missing coordinates keep
        // supplied insertion order, never ID sort.
        let indexed: Vec<(usize, PendingEntry)> = builder.entries.into_iter().enumerate().collect();
        let mut indexed = indexed;
        indexed.sort_by(|(ai, a), (bi, b)| {
            match (a.position, b.position) {
                (Some(first), Some(second)) => first
                    .0
                    .cmp(&second.0)
                    .then(first.1.cmp(&second.1))
                    .then(first.2.cmp(&second.2)),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => match (&a.kind, &b.kind) {
                    (
                        PendingKind::Proposal {
                            created_at_ms: left,
                            ..
                        },
                        PendingKind::Proposal {
                            created_at_ms: right,
                            ..
                        },
                    ) => left.cmp(right).then(ai.cmp(bi)),
                    _ => ai.cmp(bi),
                },
            }
        });

        let entries = indexed
            .into_iter()
            .map(|(_, entry)| match entry.kind {
                PendingKind::Proposal {
                    proposal_id,
                    origin_proposal_id,
                    ..
                } => AgentPlacementEntry::Proposal {
                    proposal_id,
                    origin_proposal_id,
                    node_id: entry.node_id,
                },
                PendingKind::Worker { work_id } => AgentPlacementEntry::Worker {
                    work_id,
                    node_id: entry.node_id,
                },
            })
            .collect();

        result.push(AgentPlacementGroup {
            anchor_item_id: anchor,
            entries,
        });
    }

    // Emit ProposalAnchored-compatible order: groups sorted with known anchors first.
    result
}

/// Relationships including proposal anchors derived from placement groups.
pub fn proposal_relationships_from_groups(
    groups: &[AgentPlacementGroup],
) -> Vec<ChatRelationship> {
    let mut relationships = Vec::new();
    for group in groups {
        for entry in &group.entries {
            if let AgentPlacementEntry::Proposal {
                proposal_id,
                origin_proposal_id,
                ..
            } = entry
            {
                relationships.push(ChatRelationship::ProposalAnchored {
                    proposal_id: proposal_id.clone(),
                    anchor_item_id: group.anchor_item_id.clone(),
                    origin_proposal_id: origin_proposal_id.clone(),
                });
            }
        }
    }
    relationships
}
