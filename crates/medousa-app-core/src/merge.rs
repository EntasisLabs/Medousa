//! Live ↔ persisted transcript reconciliation.

use crate::contract::ChatItem;

fn normalize_for_compare(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Keep first occurrence — keyed renderers throw on duplicate message ids.
pub fn dedupe_messages_by_id(messages: Vec<ChatItem>) -> Vec<ChatItem> {
    let mut seen = std::collections::HashSet::new();
    let mut output = Vec::with_capacity(messages.len());
    for message in messages {
        if seen.insert(message.id.clone()) {
            output.push(message);
        }
    }
    output
}

fn same_entry(left: &ChatItem, right: &ChatItem) -> bool {
    match (&left.transcript, &right.transcript) {
        (Some(a), Some(b)) => {
            a.authority_id == b.authority_id
                && a.session_id == b.session_id
                && a.entry_id == b.entry_id
        }
        _ => false,
    }
}

fn pick_preferred_assistant(left: &ChatItem, right: &ChatItem) -> bool {
    // true => keep right over left
    if left.is_streaming() && !right.is_streaming() {
        return false;
    }
    if right.is_streaming() && !left.is_streaming() {
        return true;
    }
    let left_len = left.content.trim().len();
    let right_len = right.content.trim().len();
    if left_len != right_len {
        return right_len > left_len;
    }
    true
}

fn dedupe_assistant_turns(messages: Vec<ChatItem>) -> Vec<ChatItem> {
    let mut seen_turns = std::collections::HashMap::<String, usize>::new();
    let mut output: Vec<ChatItem> = Vec::new();

    for message in messages {
        let turn_id = message
            .turn_id
            .as_ref()
            .map(|id| id.trim().to_string())
            .filter(|id| !id.is_empty());
        if message.role != crate::contract::ChatItemRole::Assistant || turn_id.is_none() {
            output.push(message);
            continue;
        }
        let turn_id = turn_id.unwrap();
        if let Some(&index) = seen_turns.get(&turn_id) {
            if pick_preferred_assistant(&output[index], &message) {
                output[index] = message;
            }
        } else {
            seen_turns.insert(turn_id, output.len());
            output.push(message);
        }
    }
    output
}

fn hydrate_assistant_metadata(existing: ChatItem, message: ChatItem) -> ChatItem {
    let streaming = existing.is_streaming();
    ChatItem {
        content: if streaming {
            existing.content
        } else {
            message.content
        },
        created_at: message.created_at.or(existing.created_at),
        turn_index: message.turn_index.or(existing.turn_index),
        turn_id: existing.turn_id.or(message.turn_id),
        failed: if streaming { existing.failed } else { message.failed.or(existing.failed) },
        error_line: if streaming {
            existing.error_line
        } else {
            message.error_line.or(existing.error_line)
        },
        media_attachments: message.media_attachments.or(existing.media_attachments),
        transcript: message.transcript.or(existing.transcript),
        live_transcripts: message.live_transcripts.or(existing.live_transcripts),
        answer_state: if streaming {
            existing.answer_state
        } else {
            message.answer_state.or(existing.answer_state)
        },
        tools: if streaming {
            existing.tools
        } else {
            message.tools.or(existing.tools)
        },
        tool_runs: if streaming {
            existing.tool_runs
        } else {
            message.tool_runs.or(existing.tool_runs)
        },
        // A history snapshot can be ahead of the SSE cursor. Replacing live text
        // here makes the pending append events append that same suffix again.
        segments: if streaming {
            existing.segments
        } else {
            message.segments.or(existing.segments)
        },
        ui_artifacts: if streaming {
            existing.ui_artifacts
        } else {
            message.ui_artifacts.or(existing.ui_artifacts)
        },
        reasoning: if streaming {
            existing.reasoning
        } else {
            message.reasoning.or(existing.reasoning)
        },
        response_provider: message.response_provider.or(existing.response_provider),
        response_model: message.response_model.or(existing.response_model),
        ..existing
    }
}

fn order_by_sequence(messages: Vec<ChatItem>) -> Vec<ChatItem> {
    let mut prefix: Vec<ChatItem> = Vec::new();
    let mut bundles: Vec<Vec<ChatItem>> = Vec::new();
    let mut scopes: std::collections::HashMap<String, Vec<usize>> =
        std::collections::HashMap::new();

    for message in messages {
        let Some(coordinate) = message.transcript.clone() else {
            if let Some(last) = bundles.last_mut() {
                last.push(message);
            } else {
                prefix.push(message);
            }
            continue;
        };
        let key = format!("{}:{}", coordinate.authority_id, coordinate.session_id);
        let indexes = scopes.entry(key).or_default();
        indexes.push(bundles.len());
        bundles.push(vec![message]);
    }

    for indexes in scopes.values() {
        let mut ordered: Vec<Vec<ChatItem>> = indexes.iter().map(|&index| bundles[index].clone()).collect();
        ordered.sort_by_key(|bundle| {
            bundle
                .first()
                .and_then(|item| item.transcript.as_ref().map(|t| t.entry_seq))
                .unwrap_or(0)
        });
        for (offset, &index) in indexes.iter().enumerate() {
            bundles[index] = ordered[offset].clone();
        }
    }

    prefix.extend(bundles.into_iter().flatten());
    prefix
}

/// Merge daemon history with local bubbles, preserving in-flight stream state.
pub fn merge_transcript(local: Vec<ChatItem>, daemon: Vec<ChatItem>) -> Vec<ChatItem> {
    if local.is_empty() {
        return dedupe_messages_by_id(daemon);
    }
    if daemon.is_empty() {
        return dedupe_messages_by_id(dedupe_assistant_turns(local));
    }

    let mut merged = local;
    let mut claimed = std::collections::HashSet::<String>::new();

    for message in daemon {
        let identity_index = merged.iter().position(|item| {
            item.role == message.role
                && (item.id == message.id
                    || same_entry(item, &message)
                    || (message.role == crate::contract::ChatItemRole::Assistant
                        && message.turn_id.as_ref().is_some_and(|turn| {
                            item.turn_id.as_ref().is_some_and(|existing| existing == turn)
                        }))
                    )
        });
        if let Some(index) = identity_index {
            claimed.insert(merged[index].id.clone());
            let existing = merged[index].clone();
            merged[index] = hydrate_assistant_metadata(existing, message);
            continue;
        }

        // Only unmatched optimistic rows may use the legacy content fallback.
        if message.role == crate::contract::ChatItemRole::User {
            let normalized = normalize_for_compare(&message.content);
            let duplicate_index = merged.iter().position(|existing| {
                existing.role == crate::contract::ChatItemRole::User
                    && existing.transcript.is_none()
                    && !claimed.contains(&existing.id)
                    && (existing.created_at.is_none()
                        || message.created_at.is_none()
                        || existing.created_at == message.created_at)
                    && normalize_for_compare(&existing.content) == normalized
            });
            if let Some(index) = duplicate_index {
                claimed.insert(merged[index].id.clone());
                let existing = merged[index].clone();
                merged[index] = hydrate_assistant_metadata(existing, message);
                continue;
            }
        }

        if message.turn_id.is_none()
            && message.role == crate::contract::ChatItemRole::Assistant
            && !message.content.trim().is_empty()
        {
            let normalized = normalize_for_compare(&message.content);
            let duplicate_index = merged.iter().position(|existing| {
                existing.role == crate::contract::ChatItemRole::Assistant
                    && existing.transcript.is_none()
                    && !claimed.contains(&existing.id)
                    && normalize_for_compare(&existing.content) == normalized
            });
            if let Some(index) = duplicate_index {
                claimed.insert(merged[index].id.clone());
                let existing = merged[index].clone();
                merged[index] = hydrate_assistant_metadata(existing, message);
                continue;
            }
        }

        if let Some(coordinate) = message.transcript.as_ref() {
            let next_index = merged.iter().position(|item| {
                item.transcript.as_ref().is_some_and(|item_coord| {
                    item_coord.authority_id == coordinate.authority_id
                        && item_coord.session_id == coordinate.session_id
                        && item_coord.entry_seq > coordinate.entry_seq
                })
            });
            if let Some(index) = next_index {
                merged.insert(index, message);
                continue;
            }

            // Recovered history precedes a coordinate-less live chat-lane shell.
            let mut live_index = merged.iter().position(|item| {
                item.role == crate::contract::ChatItemRole::Assistant
                    && item.is_streaming()
                    && item.transcript.is_none()
                    && matches!(
                        item.lane,
                        None | Some(crate::contract::ChatLane::Chat)
                    )
            });
            if let Some(index) = live_index
                && index > 0
                && merged[index - 1].role == crate::contract::ChatItemRole::User
                && merged[index - 1].transcript.is_none()
            {
                live_index = Some(index - 1);
            }
            if let Some(index) = live_index {
                merged.insert(index, message);
            } else {
                merged.push(message);
            }
        } else {
            merged.push(message);
        }
    }

    order_by_sequence(dedupe_messages_by_id(dedupe_assistant_turns(merged)))
}
