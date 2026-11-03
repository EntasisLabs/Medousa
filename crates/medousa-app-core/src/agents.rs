//! Tool / agent causal relationships derived from the canonical timeline.

use crate::contract::{ChatItem, ChatRelationship, ChatSegment};

fn is_delegation_tool(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("peer_propose")
        || lower.contains("peer.propose")
        || lower.contains("peer_handoff")
        || lower.contains("peer.handoff")
        || lower.contains("workshop_spawn")
        || lower.contains("workshop.spawn")
        || lower.contains("cognition_workshop_mutate")
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
        let mut seen = std::collections::HashSet::new();
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
            item.id == id && !matches!(item.lane, Some(crate::contract::ChatLane::Worker))
        })
    {
        return Some(item.id.as_str());
    }
    if let Some(turn_id) = parent_turn_id
        && let Some(item) = items.iter().find(|item| {
            item.turn_id.as_deref() == Some(turn_id)
                && !matches!(item.lane, Some(crate::contract::ChatLane::Worker))
        })
    {
        return Some(item.id.as_str());
    }
    if parent_item_id.is_none()
        && parent_turn_id.is_none()
        && let Some(work_id) = work_id
        && let Some(item) = items.iter().find(|item| {
            item.work_id.as_deref() == Some(work_id)
                && !matches!(item.lane, Some(crate::contract::ChatLane::Worker))
        })
    {
        return Some(item.id.as_str());
    }
    None
}
