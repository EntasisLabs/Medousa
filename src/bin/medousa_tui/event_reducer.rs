use chrono::Utc;
use serde_json::{Value, json};

use medousa::events::TuiEvent;

use super::{ConversationTurn, JobHistoryEntry, TuiState};

pub(crate) fn handle_tui_event(event: TuiEvent, state: &mut TuiState) {
    if !matches!(
        event,
        TuiEvent::AgentChunk { .. } | TuiEvent::AgentReasoningChunk { .. }
    ) {
        flush_pending_agent_chunks(state);
    }

    match event {
        TuiEvent::UiNotice(text) => {
            super::push_obs(state, text);
        }
        TuiEvent::AgentChunk { turn_id, delta } => {
            if !is_active_stream_turn(state, turn_id) {
                return;
            }
            if !delta.is_empty() {
                state.pending_agent_chunk_delta.push_str(&delta);
                state.pending_agent_chunk_count = state.pending_agent_chunk_count.saturating_add(1);
            }
        }
        TuiEvent::AgentReasoningChunk { turn_id, delta } => {
            if !is_active_stream_turn(state, turn_id) {
                return;
            }
            if !delta.is_empty() {
                state.received_native_reasoning = true;
                state.in_thinking_tag = false;
                state.stream_tag_tail.clear();
                super::push_thinking(state, delta);
            }
        }
        TuiEvent::AgentResponse {
            turn_id,
            text,
            tool_names,
        } => {
            if !is_active_stream_turn(state, turn_id) {
                return;
            }
            state.is_processing = false;
            state.active_request_task = None;
            state.open_stream_turn_id = None;
            let (visible_text, thinking_chunks) = strip_thinking_tags(&text);
            if !state.received_native_reasoning {
                for chunk in thinking_chunks {
                    super::push_thinking(state, chunk);
                }
            }

            if !state.stream_tag_tail.is_empty() {
                if state.in_thinking_tag {
                    let tail = std::mem::take(&mut state.stream_tag_tail);
                    super::push_thinking(state, tail);
                } else {
                    let tail = std::mem::take(&mut state.stream_tag_tail);
                    if let Some(idx) = state.active_agent_stream_turn {
                        if let Some(turn) = state.conversation.get_mut(idx) {
                            turn.content.push_str(&tail);
                        }
                    }
                }
            }
            state.in_thinking_tag = false;
            state.received_native_reasoning = false;
            super::flush_thinking_buffer(state);

            let answer_state = match state.pending_response_verified.take() {
                Some(true) => Some("verified".to_string()),
                Some(false) => Some("provisional".to_string()),
                None => None,
            };
            let final_text = visible_text;

            if let Some(idx) = state.active_agent_stream_turn.take() {
                if let Some(turn) = state.conversation.get_mut(idx) {
                    turn.content = merge_streamed_and_final_body(&turn.content, &final_text);
                    turn.tool_names = tool_names.clone();
                    turn.answer_state = answer_state.clone();
                    turn.timestamp = Utc::now();
                    super::append_turn(&state.session_id, turn);
                }
            } else {
                let turn = ConversationTurn {
                    role: "agent".to_string(),
                    content: final_text,
                    timestamp: Utc::now(),
                    tool_names,
                    answer_state,
                };
                super::append_turn(&state.session_id, &turn);
                state.conversation.push(turn);
            }
            if state.auto_scroll {
                state.conv_scroll = state.conv_max_scroll;
            }
            super::invalidate_markdown_cache(state);
        }
        TuiEvent::AgentError { turn_id, message } => {
            if !is_active_stream_turn(state, turn_id) {
                return;
            }
            state.is_processing = false;
            state.active_request_task = None;
            state.open_stream_turn_id = None;
            state.active_agent_stream_turn = None;
            state.in_thinking_tag = false;
            state.stream_tag_tail.clear();
            state.received_native_reasoning = false;
            super::flush_thinking_buffer(state);
            state.pending_response_verified = None;
            super::push_obs(state, format!("⚠ {message}"));
        }
        TuiEvent::JobEnqueued { job_id, job_type } => {
            super::push_obs(state, format!("+ {job_type}"));
            state.job_history.push_front(JobHistoryEntry {
                job_id,
                job_type,
                status: "enqueued".to_string(),
            });
            if state.job_history.len() > 100 {
                state.job_history.pop_back();
            }
            super::invalidate_markdown_cache(state);
        }
        TuiEvent::JobProcessed {
            job_id,
            succeeded,
            execution_id,
        } => {
            let symbol = if succeeded { "✓" } else { "✗" };
            let exec_hint = execution_id.as_deref().unwrap_or("—");
            super::push_obs(state, format!("{symbol} [{exec_hint:.12}]"));
            for entry in state.job_history.iter_mut() {
                if entry.job_id == job_id {
                    entry.status = if succeeded { "succeeded" } else { "failed" }.to_string();
                    break;
                }
            }
            super::invalidate_markdown_cache(state);
        }
        TuiEvent::ToolInvoked {
            tool_name,
            input_summary,
        } => {
            super::push_obs(state, format!("◆ {tool_name}  {input_summary}"));
        }
        TuiEvent::ToolPayload {
            tool_name,
            tool_input,
            tool_output,
            input_receipt,
            output_receipt,
        } => {
            let mut formatter_input = tool_input.clone();
            let mut formatter_output = tool_output.clone();

            if input_receipt.is_some() || output_receipt.is_some() {
                let input_summary = match input_receipt.as_ref() {
                    Some(meta) => format!(
                        "in(bytes={},hash={})",
                        meta.byte_size,
                        trim_hash(&meta.hash64)
                    ),
                    None => "in(inline)".to_string(),
                };
                let output_summary = match output_receipt.as_ref() {
                    Some(meta) => format!(
                        "out(bytes={},hash={})",
                        meta.byte_size,
                        trim_hash(&meta.hash64)
                    ),
                    None => "out(inline)".to_string(),
                };
                super::push_obs(
                    state,
                    format!("◈ receipt {tool_name}  {input_summary}  {output_summary}"),
                );
            }

            if let Some(meta) = input_receipt {
                let safe_input = medousa::settings_guard::redact_json_value(&tool_input);
                match medousa::artifact_store::persist_tool_artifact(
                    &state.session_id,
                    &tool_name,
                    "input",
                    &meta.hash64,
                    meta.byte_size,
                    &safe_input,
                ) {
                    Ok(record) => {
                        formatter_input = json!({
                            "artifact_ref": {
                                "artifact_id": record.artifact_id,
                                "session_id": record.session_id,
                                "tool_name": record.tool_name,
                                "direction": record.direction,
                                "hash64": record.hash64,
                                "byte_size": record.byte_size,
                                "stored_at_utc": record.stored_at_utc,
                            }
                        });
                        super::push_obs(state, format!("◈ artifact {}", record.artifact_id))
                    }
                    Err(err) => super::push_obs(
                        state,
                        format!("⚠ artifact store failed ({tool_name} input): {err}"),
                    ),
                }
            }

            if let Some(meta) = output_receipt {
                let safe_output = medousa::settings_guard::redact_json_value(&tool_output);
                match medousa::artifact_store::persist_tool_artifact(
                    &state.session_id,
                    &tool_name,
                    "output",
                    &meta.hash64,
                    meta.byte_size,
                    &safe_output,
                ) {
                    Ok(record) => {
                        let chunk_refs = medousa::artifact_chunking::chunk_json_payload(
                            &record.artifact_id,
                            &safe_output,
                            2400,
                            240,
                        );
                        let total_chunks = chunk_refs.len();
                        let preview_chunk_refs = chunk_refs.into_iter().take(8).collect::<Vec<_>>();

                        formatter_output = json!({
                            "artifact_ref": {
                                "artifact_id": record.artifact_id,
                                "session_id": record.session_id,
                                "tool_name": record.tool_name,
                                "direction": record.direction,
                                "hash64": record.hash64,
                                "byte_size": record.byte_size,
                                "stored_at_utc": record.stored_at_utc,
                                "chunk_refs": preview_chunk_refs,
                                "chunk_ref_count": total_chunks,
                            }
                        });
                        super::push_obs(state, format!("◈ artifact {}", record.artifact_id));
                        super::push_obs(
                            state,
                            format!("◈ chunk refs {} count={total_chunks}", record.artifact_id),
                        );
                    }
                    Err(err) => super::push_obs(
                        state,
                        format!("⚠ artifact store failed ({tool_name} output): {err}"),
                    ),
                }
            }

            let request_id = super::next_worker_request_id(state);
            let queued = super::queue_worker_command(
                state,
                super::WorkerCommand::FormatToolPayload {
                    request_id,
                    tool_name: tool_name.clone(),
                    tool_input: formatter_input,
                    tool_output: formatter_output,
                },
                true,
            );
            if !queued {
                super::push_obs(
                    state,
                    format!("◆ {tool_name}  payload omitted (formatter busy)"),
                );
            }

            if tool_name == "editor.gr.run" {
                let source = tool_output
                    .get("source")
                    .and_then(|v| v.as_str())
                    .unwrap_or("editor:buffer");
                let job_id = tool_output
                    .get("job_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown-job");
                let succeeded = tool_output
                    .get("succeeded")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let diagnostics = tool_output
                    .get("diagnostics")
                    .cloned()
                    .unwrap_or(Value::Null);
                super::push_grapheme_console_entry(state, source, job_id, succeeded, &diagnostics);
            }
        }
    }
}

fn is_active_stream_turn(state: &TuiState, turn_id: u64) -> bool {
    state.open_stream_turn_id == Some(turn_id)
}

fn trim_hash(hash: &str) -> &str {
    const MAX: usize = 12;
    if hash.len() <= MAX {
        return hash;
    }
    &hash[..MAX]
}

fn merge_streamed_and_final_body(streamed_body: &str, final_body: &str) -> String {
    let streamed_trimmed = streamed_body.trim();
    let final_trimmed = final_body.trim();

    if final_trimmed.is_empty() {
        return streamed_body.to_string();
    }
    if streamed_trimmed.is_empty() {
        return final_body.to_string();
    }
    if final_trimmed.starts_with(streamed_trimmed) {
        return final_body.to_string();
    }
    if streamed_trimmed.starts_with(final_trimmed) {
        return streamed_body.to_string();
    }

    let overlap = suffix_prefix_overlap(streamed_body, final_body);
    if overlap > 0 {
        let mut merged = String::with_capacity(streamed_body.len() + final_body.len() - overlap);
        merged.push_str(streamed_body);
        merged.push_str(&final_body[overlap..]);
        return merged;
    }

    format!("{streamed_body}\n\n[final synthesis]\n{final_body}")
}

fn suffix_prefix_overlap(left: &str, right: &str) -> usize {
    let left_chars = left.chars().collect::<Vec<_>>();
    let right_chars = right.chars().collect::<Vec<_>>();
    let max = left_chars.len().min(right_chars.len());
    for size in (1..=max).rev() {
        if left_chars[left_chars.len() - size..] == right_chars[..size] {
            return right_chars[..size].iter().map(|c| c.len_utf8()).sum();
        }
    }
    0
}

fn apply_agent_chunk_delta(delta: &str, state: &mut TuiState) {
    if delta.is_empty() {
        return;
    }

    let (visible_delta, thinking_chunks) = if state.received_native_reasoning {
        (delta.to_string(), Vec::new())
    } else {
        extract_thinking_from_stream(
            delta,
            &mut state.in_thinking_tag,
            &mut state.stream_tag_tail,
        )
    };
    if !state.received_native_reasoning {
        for chunk in thinking_chunks {
            super::push_thinking(state, chunk);
        }
    }

    if visible_delta.is_empty() {
        return;
    }

    if let Some(idx) = state.active_agent_stream_turn {
        if let Some(turn) = state.conversation.get_mut(idx) {
            turn.content.push_str(&visible_delta);
        }
    } else {
        state.conversation.push(ConversationTurn {
            role: "agent".to_string(),
            content: visible_delta,
            timestamp: Utc::now(),
            tool_names: vec![],
            answer_state: None,
        });
        state.active_agent_stream_turn = Some(state.conversation.len().saturating_sub(1));
    }

    if state.auto_scroll {
        state.conv_scroll = state.conv_max_scroll;
    }
    super::invalidate_markdown_cache(state);
}

pub(crate) fn flush_pending_agent_chunks(state: &mut TuiState) {
    if state.pending_agent_chunk_delta.is_empty() {
        state.pending_agent_chunk_count = 0;
        return;
    }

    let delta = std::mem::take(&mut state.pending_agent_chunk_delta);
    if state.pending_agent_chunk_count > 1 {
        state.perf.coalesced_agent_chunks = state
            .perf
            .coalesced_agent_chunks
            .saturating_add(state.pending_agent_chunk_count.saturating_sub(1));
    }
    state.pending_agent_chunk_count = 0;
    apply_agent_chunk_delta(&delta, state);
}

fn extract_thinking_from_stream(
    delta: &str,
    in_thinking: &mut bool,
    tail: &mut String,
) -> (String, Vec<String>) {
    let mut buffer = String::with_capacity(tail.len() + delta.len());
    buffer.push_str(tail);
    buffer.push_str(delta);
    tail.clear();

    let mut visible = String::new();
    let mut thinking = Vec::new();

    loop {
        if *in_thinking {
            if let Some((idx, marker_len)) =
                find_earliest_marker(&buffer, &["</think>", "</thinking>"])
            {
                let chunk = &buffer[..idx];
                if !chunk.is_empty() {
                    thinking.push(chunk.to_string());
                }
                buffer = buffer[idx + marker_len..].to_string();
                *in_thinking = false;
                continue;
            }

            let keep = trailing_prefix_len(&buffer, &["</think>", "</thinking>"]);
            if buffer.len() > keep {
                thinking.push(buffer[..buffer.len() - keep].to_string());
            }
            *tail = if keep > 0 {
                buffer[buffer.len() - keep..].to_string()
            } else {
                String::new()
            };
            break;
        }

        if let Some((idx, marker_len)) = find_earliest_marker(&buffer, &["<think>", "<thinking>"]) {
            visible.push_str(&buffer[..idx]);
            buffer = buffer[idx + marker_len..].to_string();
            *in_thinking = true;
            continue;
        }

        let keep = trailing_prefix_len(&buffer, &["<think>", "<thinking>"]);
        if buffer.len() > keep {
            visible.push_str(&buffer[..buffer.len() - keep]);
        }
        *tail = if keep > 0 {
            buffer[buffer.len() - keep..].to_string()
        } else {
            String::new()
        };
        break;
    }

    (visible, thinking)
}

fn strip_thinking_tags(text: &str) -> (String, Vec<String>) {
    let mut remaining = text.to_string();
    let mut visible = String::new();
    let mut thinking = Vec::new();
    let mut in_thinking = false;

    loop {
        if remaining.is_empty() {
            break;
        }

        if in_thinking {
            if let Some((idx, marker_len)) =
                find_earliest_marker(&remaining, &["</think>", "</thinking>"])
            {
                let chunk = &remaining[..idx];
                if !chunk.is_empty() {
                    thinking.push(chunk.to_string());
                }
                remaining = remaining[idx + marker_len..].to_string();
                in_thinking = false;
            } else {
                thinking.push(remaining);
                break;
            }
        } else if let Some((idx, marker_len)) =
            find_earliest_marker(&remaining, &["<think>", "<thinking>"])
        {
            visible.push_str(&remaining[..idx]);
            remaining = remaining[idx + marker_len..].to_string();
            in_thinking = true;
        } else {
            visible.push_str(&remaining);
            break;
        }
    }

    (visible, thinking)
}

fn find_earliest_marker(haystack: &str, markers: &[&str]) -> Option<(usize, usize)> {
    markers
        .iter()
        .filter_map(|m| haystack.find(m).map(|idx| (idx, m.len())))
        .min_by_key(|(idx, _)| *idx)
}

fn trailing_prefix_len(s: &str, markers: &[&str]) -> usize {
    for start in s
        .char_indices()
        .map(|(idx, _)| idx)
        .chain(std::iter::once(s.len()))
        .rev()
    {
        if start == s.len() {
            continue;
        }
        let suffix = &s[start..];
        if markers.iter().any(|m| m.starts_with(suffix)) {
            return s.len() - start;
        }
    }
    0
}
