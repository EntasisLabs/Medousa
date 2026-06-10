use chrono::Utc;
use serde_json::{Value, json};

use medousa::events::TuiEvent;

use super::{ConversationTurn, JobHistoryEntry, TuiState};

pub(crate) async fn handle_tui_event(event: TuiEvent, state: &mut TuiState) {
    if !matches!(
        event,
        TuiEvent::AgentChunk { .. } | TuiEvent::AgentReasoningChunk { .. }
    ) {
        flush_pending_agent_chunks(state);
    }

    if matches!(event, TuiEvent::AgentScratchReset { .. }) {
        flush_pending_agent_chunks(state);
    }

    match event {
        TuiEvent::UiNotice(text) => {
            super::push_obs(state, text);
        }
        TuiEvent::ApprovalRequired {
            server_id,
            tool_name,
            reason,
        } => {
            super::push_obs(
                state,
                format!(
                    "⏸ approval required: {server_id}.{tool_name} — {reason}. \
                     Reply yes/approve to retry with approval_granted: true."
                ),
            );
        }
        TuiEvent::TurnBudgetApprovalRequired {
            turn_id: _,
            request_id,
            rounds_executed,
            max_tool_rounds,
            requested_rounds,
            reason,
            progress_summary,
        } => {
            state.pending_budget_request_id = Some(request_id.clone());
            state.pending_budget_requested_rounds = Some(requested_rounds);
            let progress = progress_summary
                .map(|value| format!(" — {value}"))
                .unwrap_or_default();
            super::push_obs(
                state,
                format!(
                    "⏸ turn budget request {request_id}: at {rounds_executed}/{max_tool_rounds}, \
                     asking +{requested_rounds} rounds — {reason}{progress}. \
                     Approve: /budget approve  Deny: /budget deny  List: /budget list"
                ),
            );
        }
        TuiEvent::AgentScratchReset { turn_id } => {
            if !is_active_stream_turn(state, turn_id) {
                return;
            }
            if let Some(idx) = state.active_agent_stream_turn {
                if let Some(turn) = state.conversation.get_mut(idx) {
                    turn.content.clear();
                }
            }
            state.pending_agent_chunk_delta.clear();
            state.pending_agent_chunk_count = 0;
            state.turn_parts.scratch_reset();
            super::invalidate_markdown_cache(state);
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
                state.turn_parts.push_reasoning_delta(&delta);
                super::push_thinking(state, delta);
            }
        }
        TuiEvent::AgentFinalPending {
            turn_id,
            text,
            tool_names,
        } => {
            if !is_active_stream_turn(state, turn_id) {
                return;
            }
            super::push_obs(state, format!("◈ {text}"));
            if let Some(idx) = state.active_agent_stream_turn {
                if let Some(turn) = state.conversation.get_mut(idx) {
                    if turn.content.trim().is_empty() {
                        turn.content = text.clone();
                    }
                    turn.tool_names = tool_names;
                    turn.answer_state = Some("tool_loop".to_string());
                    turn.timestamp = Utc::now();
                }
            }
            if state.auto_scroll {
                state.conv_scroll = state.conv_max_scroll;
            }
            super::invalidate_markdown_cache(state);
        }
        TuiEvent::AgentTurnProgress {
            turn_id,
            message,
            tool_names,
        } => {
            if !is_active_stream_turn(state, turn_id) {
                return;
            }
            super::push_obs(state, format!("◈ {message}"));
            if let Some(idx) = state.active_agent_stream_turn {
                if let Some(turn) = state.conversation.get_mut(idx) {
                    if turn.content.trim().is_empty() {
                        turn.content = message.clone();
                    }
                    turn.tool_names = tool_names;
                    turn.answer_state = Some("tool_loop".to_string());
                    turn.timestamp = Utc::now();
                }
            }
            if state.auto_scroll {
                state.conv_scroll = state.conv_max_scroll;
            }
            super::invalidate_markdown_cache(state);
        }
        TuiEvent::AgentNeedsInput {
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
            state.in_thinking_tag = false;
            state.received_native_reasoning = false;
            super::flush_thinking_buffer(state);

            let final_text = visible_text;
            let finalized = if let Some(idx) = state.active_agent_stream_turn {
                let content = state
                    .conversation
                    .get(idx)
                    .map(|turn| resolve_agent_turn_content(&turn.content, &final_text, true))
                    .unwrap_or_else(|| final_text.clone());
                let turn = state.turn_parts.finalize_assistant_turn(
                    content,
                    tool_names.clone(),
                    Some("needs_input".to_string()),
                );
                if let Some(existing) = state.conversation.get_mut(idx) {
                    *existing = turn.clone();
                }
                state.active_agent_stream_turn = None;
                turn
            } else {
                let turn = state.turn_parts.finalize_assistant_turn(
                    final_text.clone(),
                    tool_names.clone(),
                    Some("needs_input".to_string()),
                );
                state.conversation.push(turn.clone());
                turn
            };
            let session_id = state.session_id.clone();
            super::history_services::append_turn_daemon_first(state, &session_id, &finalized)
                .await;
            if state.auto_scroll {
                state.conv_scroll = state.conv_max_scroll;
            }
            super::invalidate_markdown_cache(state);
        }
        TuiEvent::AgentResponse {
            turn_id,
            text,
            tool_names,
            terminal,
            work_id,
        } => {
            if !is_active_stream_turn(state, turn_id) {
                return;
            }
            if terminal {
                state.is_processing = false;
                state.active_request_task = None;
                state.open_stream_turn_id = None;
            }
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
            let is_worker_handoff = !terminal && work_id.is_some();

            let persisted = if is_worker_handoff {
                let turn = state.turn_parts.finalize_worker_ack_turn(
                    final_text.clone(),
                    tool_names.clone(),
                    work_id.clone(),
                );
                state.conversation.push(turn.clone());
                state.active_agent_stream_turn = None;
                turn
            } else if let Some(idx) = state.active_agent_stream_turn {
                let content = state
                    .conversation
                    .get(idx)
                    .map(|turn| resolve_agent_turn_content(&turn.content, &final_text, terminal))
                    .unwrap_or_else(|| final_text.clone());
                let turn = state.turn_parts.finalize_assistant_turn(
                    content,
                    tool_names.clone(),
                    answer_state.clone(),
                );
                if let Some(existing) = state.conversation.get_mut(idx) {
                    *existing = turn.clone();
                }
                if terminal {
                    state.active_agent_stream_turn = None;
                }
                turn
            } else {
                let turn = state.turn_parts.finalize_assistant_turn(
                    final_text.clone(),
                    tool_names.clone(),
                    answer_state.clone(),
                );
                state.conversation.push(turn.clone());
                if !terminal {
                    state.active_agent_stream_turn =
                        Some(state.conversation.len().saturating_sub(1));
                }
                turn
            };

            let session_id = state.session_id.clone();
            super::history_services::append_turn_daemon_first(state, &session_id, &persisted).await;
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
        TuiEvent::ToolRunStarted {
            tool_run_id,
            tool_name,
            input_summary,
            tool_round,
        } => {
            state.turn_parts.tool_started(
                &tool_run_id,
                &tool_name,
                &input_summary,
                tool_round,
            );
            let label = super::tui_presentation::format_tool_name(&tool_name);
            super::push_obs(state, format!("◆ {label}  {input_summary}"));
            super::invalidate_markdown_cache(state);
        }
        TuiEvent::ToolRunFinished {
            tool_run_id,
            tool_name,
            status,
            input_summary,
            output_summary,
            tool_round: _,
        } => {
            state.turn_parts.tool_finished(
                &tool_run_id,
                &status,
                output_summary.clone(),
                Vec::new(),
            );
            let label = super::tui_presentation::format_tool_name(&tool_name);
            let suffix = output_summary
                .filter(|value| !value.trim().is_empty())
                .map(|value| format!(" → {value}"))
                .unwrap_or_default();
            super::push_obs(
                state,
                format!("◈ tool {label} {status}  {input_summary}{suffix}"),
            );
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

/// Mid-turn `AgentResponse` (e.g. worker ack) replaces the draft; terminal keeps streamed body (Phase 7A).
fn resolve_agent_turn_content(streamed_body: &str, final_body: &str, terminal: bool) -> String {
    if !terminal {
        return final_body.to_string();
    }

    if !streamed_body.trim().is_empty() {
        return streamed_body.to_string();
    }

    if !final_body.trim().is_empty() {
        return final_body.to_string();
    }

    streamed_body.to_string()
}

#[cfg(test)]
mod resolve_content_tests {
    use super::resolve_agent_turn_content;

    #[test]
    fn terminal_keeps_substantive_stream_over_divergent_final() {
        let streamed = "Here is what I found about locus: STTP nodes under session medousa-ux.";
        let final_answer = "Different rewrite from a synthesis pass that the user never saw stream.";
        let merged = resolve_agent_turn_content(streamed, final_answer, true);
        assert_eq!(merged, streamed);
    }

    #[test]
    fn terminal_keeps_stream_even_when_final_differs() {
        let streamed = "Let me dig into memory for you.";
        let final_answer = "Here is what I found about locus: the project uses STTP nodes stored under session medousa-ux with several architecture notes from May.";
        let merged = resolve_agent_turn_content(streamed, final_answer, true);
        assert_eq!(merged, streamed);
    }

    #[test]
    fn non_terminal_replaces_draft() {
        let streamed = "Let me check that for you.";
        let ack = "Delegated to background worker — I'll synthesize when done.";
        let out = resolve_agent_turn_content(streamed, ack, false);
        assert_eq!(out, ack);
    }
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
        state.conversation.push(ConversationTurn::plain(
            "agent",
            visible_delta,
            Utc::now(),
            vec![],
            None,
        ));
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
