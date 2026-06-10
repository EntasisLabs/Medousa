use medousa::daemon_api::{
    TurnBudgetApproveRequest, TurnBudgetDenyRequest, TurnBudgetRequestListResponse,
    TurnBudgetRequestResponse,
};

use super::daemon_commands::{
    daemon_approve_budget_request, daemon_deny_budget_request, daemon_list_budget_requests,
};
use super::{EventOutcome, TuiState, push_obs};

fn default_tui_resolved_by() -> String {
    "tui".to_string()
}

fn resolve_budget_request_id(state: &TuiState, explicit: &str) -> Option<String> {
    let trimmed = explicit.trim();
    if !trimmed.is_empty() {
        return Some(trimmed.to_string());
    }
    state.pending_budget_request_id.clone()
}

pub(crate) async fn handle_budget_command(
    sub: &str,
    rest: &str,
    state: &mut TuiState,
) -> EventOutcome {
    match sub {
        "" | "list" => match daemon_list_budget_requests(&state.daemon_url, true).await {
            Ok(TurnBudgetRequestListResponse { requests }) if requests.is_empty() => {
                push_obs(state, "No pending budget approvals.".to_string());
            }
            Ok(TurnBudgetRequestListResponse { requests }) => {
                for row in requests {
                    let progress = row
                        .progress_summary
                        .map(|value| format!(" — {value}"))
                        .unwrap_or_default();
                    push_obs(
                        state,
                        format!(
                            "⏸ {}… +{} rounds at {}/{} — {}{progress} \
                             (/budget approve {} /budget deny {})",
                            &row.request_id[..row.request_id.len().min(8)],
                            row.requested_rounds,
                            row.rounds_executed,
                            row.max_tool_rounds,
                            row.reason,
                            row.request_id,
                            row.request_id
                        ),
                    );
                }
            }
            Err(err) => push_obs(state, format!("⚠ budget list failed: {err}")),
        },
        "approve" => {
            let request_id = match resolve_budget_request_id(state, rest) {
                Some(value) => value,
                None => {
                    push_obs(
                        state,
                        "⚠ no pending budget approval — try /budget list".to_string(),
                    );
                    return EventOutcome::Continue;
                }
            };
            let extra_rounds = state.pending_budget_requested_rounds;
            let body = TurnBudgetApproveRequest {
                extra_rounds,
                resolved_by: Some(default_tui_resolved_by()),
            };
            match daemon_approve_budget_request(&state.daemon_url, &request_id, &body).await {
                Ok(TurnBudgetRequestResponse { message, .. }) => {
                    state.pending_budget_request_id = None;
                    state.pending_budget_requested_rounds = None;
                    push_obs(
                        state,
                        format!("✓ budget approved ({message}) — turn resuming"),
                    );
                }
                Err(err) => push_obs(state, format!("⚠ budget approve failed: {err}")),
            }
        }
        "deny" => {
            let request_id = match resolve_budget_request_id(state, rest) {
                Some(value) => value,
                None => {
                    push_obs(
                        state,
                        "⚠ no pending budget approval — try /budget list".to_string(),
                    );
                    return EventOutcome::Continue;
                }
            };
            let body = TurnBudgetDenyRequest {
                resolved_by: Some(default_tui_resolved_by()),
            };
            match daemon_deny_budget_request(&state.daemon_url, &request_id, &body).await {
                Ok(TurnBudgetRequestResponse { message, .. }) => {
                    state.pending_budget_request_id = None;
                    state.pending_budget_requested_rounds = None;
                    push_obs(state, format!("✓ budget denied ({message})"));
                }
                Err(err) => push_obs(state, format!("⚠ budget deny failed: {err}")),
            }
        }
        _ => {
            push_obs(
                state,
                "⚠ usage: /budget list | /budget approve [id] | /budget deny [id]".to_string(),
            );
        }
    }

    EventOutcome::Continue
}
