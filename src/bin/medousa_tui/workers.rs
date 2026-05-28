use serde_json::Value;
use tokio::sync::mpsc;

use super::{
    TuiState,
    daemon_commands::{daemon_enqueue_ask, daemon_health, daemon_register_recurring_prompt},
    push_obs,
};

#[derive(Debug)]
pub(crate) enum WorkerCommand {
    DaemonHealth {
        request_id: u64,
        daemon_url: String,
    },
    DaemonAsk {
        request_id: u64,
        daemon_url: String,
        prompt: String,
    },
    WatchAdd {
        request_id: u64,
        daemon_url: String,
        cron_expr: String,
        prompt: String,
    },
    FormatToolPayload {
        request_id: u64,
        tool_name: String,
        tool_input: Value,
        tool_output: Value,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkerRequestKind {
    DaemonHealth,
    DaemonAsk,
    WatchAdd,
}

#[derive(Debug)]
pub(crate) enum WorkerResult {
    Notice {
        request_id: u64,
        kind: WorkerRequestKind,
        text: String,
    },
    FormattedToolPayload {
        request_id: u64,
        text: String,
    },
}

pub(crate) fn next_worker_request_id(state: &mut TuiState) -> u64 {
    let next = state.next_worker_request_id.saturating_add(1);
    state.next_worker_request_id = next;
    next
}

pub(crate) fn queue_worker_command(
    state: &mut TuiState,
    command: WorkerCommand,
    low_priority: bool,
) -> bool {
    match state.worker_cmd_tx.try_send(command) {
        Ok(()) => {
            state.perf.worker_queue_depth = state.perf.worker_queue_depth.saturating_add(1);
            state.perf.worker_queue_peak = state
                .perf
                .worker_queue_peak
                .max(state.perf.worker_queue_depth);
            true
        }
        Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
            state.perf.dropped_events = state.perf.dropped_events.saturating_add(1);
            if !low_priority {
                push_obs(state, "⚠ worker queue busy: command dropped".to_string());
            }
            false
        }
        Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
            state.perf.dropped_events = state.perf.dropped_events.saturating_add(1);
            push_obs(state, "⚠ worker queue unavailable".to_string());
            false
        }
    }
}

pub(crate) fn handle_worker_result(result: WorkerResult, state: &mut TuiState) {
    match result {
        WorkerResult::Notice {
            request_id,
            kind,
            text,
        } => {
            state.perf.worker_queue_depth = state.perf.worker_queue_depth.saturating_sub(1);
            let latest = match kind {
                WorkerRequestKind::DaemonHealth => state.latest_daemon_health_request_id,
                WorkerRequestKind::DaemonAsk => state.latest_daemon_ask_request_id,
                WorkerRequestKind::WatchAdd => state.latest_watch_add_request_id,
            };

            if request_id < latest {
                state.perf.dropped_events = state.perf.dropped_events.saturating_add(1);
                return;
            }

            push_obs(state, text);
        }
        WorkerResult::FormattedToolPayload { request_id, text } => {
            state.perf.worker_queue_depth = state.perf.worker_queue_depth.saturating_sub(1);
            let _ = request_id;
            push_obs(state, text);
        }
    }
}

pub(crate) async fn worker_loop(
    mut command_rx: mpsc::Receiver<WorkerCommand>,
    result_tx: mpsc::Sender<WorkerResult>,
) {
    while let Some(command) = command_rx.recv().await {
        let result = match command {
            WorkerCommand::DaemonHealth {
                request_id,
                daemon_url,
            } => {
                let text = match daemon_health(&daemon_url).await {
                    Ok(payload) => format!(
                        "✓ daemon {} backend={} worker={}",
                        payload.status, payload.backend, payload.worker_id
                    ),
                    Err(err) => format!("⚠ daemon health failed: {err}"),
                };
                WorkerResult::Notice {
                    request_id,
                    kind: WorkerRequestKind::DaemonHealth,
                    text,
                }
            }
            WorkerCommand::DaemonAsk {
                request_id,
                daemon_url,
                prompt,
            } => {
                let text = match daemon_enqueue_ask(&daemon_url, &prompt).await {
                    Ok(payload) => format!("✓ daemon job enqueued {}", payload.job_id),
                    Err(err) => format!("⚠ daemon ask failed: {err}"),
                };
                WorkerResult::Notice {
                    request_id,
                    kind: WorkerRequestKind::DaemonAsk,
                    text,
                }
            }
            WorkerCommand::WatchAdd {
                request_id,
                daemon_url,
                cron_expr,
                prompt,
            } => {
                let text = match daemon_register_recurring_prompt(&daemon_url, &cron_expr, &prompt)
                    .await
                {
                    Ok(payload) => {
                        format!(
                            "✓ watch {} next={}",
                            payload.recurring_id, payload.next_run_at_utc
                        )
                    }
                    Err(err) => format!("⚠ watch add failed: {err}"),
                };
                WorkerResult::Notice {
                    request_id,
                    kind: WorkerRequestKind::WatchAdd,
                    text,
                }
            }
            WorkerCommand::FormatToolPayload {
                request_id,
                tool_name,
                tool_input,
                tool_output,
            } => {
                let safe_input = medousa::settings_guard::redact_json_value(&tool_input);
                let safe_output = medousa::settings_guard::redact_json_value(&tool_output);
                let (safe_input, input_is_receipt) = medousa::payload_receipt::inline_or_receipt(
                    &safe_input,
                    medousa::payload_receipt::DEFAULT_MAX_INLINE_BYTES,
                    medousa::payload_receipt::DEFAULT_PREVIEW_CHARS,
                );
                let (safe_output, output_is_receipt) = medousa::payload_receipt::inline_or_receipt(
                    &safe_output,
                    medousa::payload_receipt::DEFAULT_MAX_INLINE_BYTES,
                    medousa::payload_receipt::DEFAULT_PREVIEW_CHARS,
                );
                let input = serde_json::to_string_pretty(&safe_input)
                    .unwrap_or_else(|_| safe_input.to_string());
                let output = serde_json::to_string_pretty(&safe_output)
                    .unwrap_or_else(|_| safe_output.to_string());
                let receipt_hint = if input_is_receipt || output_is_receipt {
                    "\n(note: large payload replaced with artifact receipt)"
                } else {
                    ""
                };
                let text = format!(
                    "◆ {tool_name}\ninput:\n{input}\noutput:\n{output}{receipt_hint}\n────────────────"
                );
                WorkerResult::FormattedToolPayload { request_id, text }
            }
        };

        let _ = result_tx.send(result).await;
    }
}
