use std::str::SplitWhitespace;
use std::sync::Arc;

use anyhow::Result;
use medousa_sdk::{HttpTransport, MedousaClient};

use medousa::{
    ArtifactCommandRequest, ArtifactCommandResponse, EnqueueAskRequest, EnqueueResponse,
    HealthResponse, InteractiveTurnRequest, InteractiveTurnResponse,
    RegisterRecurringPromptRequest, RegisterRecurringResponse, RuntimeConfigCommandRequest,
    RuntimeConfigCommandResponse, SessionAppendTurnRequest, SessionAppendTurnResponse,
    SessionHistoryListResponse, SessionHistoryResponse, SessionSetDisplayNameResponse,
    StageRouteCommandRequest, StageRouteCommandResponse,
};
use medousa::daemon_api::{
    TurnBudgetApproveRequest, TurnBudgetDenyRequest, TurnBudgetRequestListResponse,
    TurnBudgetRequestResponse,
};

use super::{
    EventOutcome, TuiState, WorkerCommand, next_worker_request_id, push_obs, queue_worker_command,
};

fn daemon_client(daemon_url: &str) -> MedousaClient {
    MedousaClient::with_transport(Arc::new(HttpTransport::new()), daemon_url)
}

fn sdk_err(err: medousa_sdk::SdkError) -> anyhow::Error {
    anyhow::anyhow!(err.to_string())
}

pub(crate) fn handle_daemon_command(
    parts: &mut SplitWhitespace<'_>,
    state: &mut TuiState,
) -> EventOutcome {
    let sub = parts.next().unwrap_or("");
    match sub {
        "" => {
            push_obs(
                state,
                format!(
                    "daemon url={} | commands: /daemon health | /daemon ask <prompt> | /daemon url <url>",
                    state.daemon_url
                ),
            );
        }
        "url" => {
            let next = parts.collect::<Vec<_>>().join(" ");
            if next.trim().is_empty() {
                push_obs(state, format!("daemon url={}", state.daemon_url));
            } else {
                state.daemon_url = next.trim().to_string();
                push_obs(state, format!("✓ daemon url set to {}", state.daemon_url));
            }
        }
        "health" => {
            let request_id = next_worker_request_id(state);
            state.latest_daemon_health_request_id = request_id;
            let daemon_url = state.daemon_url.clone();
            let queued = queue_worker_command(
                state,
                WorkerCommand::DaemonHealth {
                    request_id,
                    daemon_url: daemon_url.clone(),
                },
                true,
            );
            if queued {
                push_obs(
                    state,
                    format!("↻ daemon health check queued #{request_id}: {daemon_url}"),
                );
            }
        }
        "ask" => {
            let prompt = parts.collect::<Vec<_>>().join(" ");
            if prompt.trim().is_empty() {
                push_obs(state, "⚠ usage: /daemon ask <prompt>".to_string());
            } else {
                let request_id = next_worker_request_id(state);
                state.latest_daemon_ask_request_id = request_id;
                let daemon_url = state.daemon_url.clone();
                let queued = queue_worker_command(
                    state,
                    WorkerCommand::DaemonAsk {
                        request_id,
                        daemon_url,
                        prompt,
                    },
                    false,
                );
                if queued {
                    push_obs(state, format!("↻ daemon ask queued #{request_id}"));
                }
            }
        }
        _ => {
            push_obs(
                state,
                "⚠ unknown /daemon command. try /daemon health | /daemon ask <prompt> | /daemon url <url>"
                    .to_string(),
            );
        }
    }

    EventOutcome::Continue
}

pub(crate) fn handle_watch_command(
    parts: &mut SplitWhitespace<'_>,
    state: &mut TuiState,
) -> EventOutcome {
    let sub = parts.next().unwrap_or("");
    if sub != "add" {
        push_obs(
            state,
            "⚠ usage: /watch add <cron_expr> <prompt...>".to_string(),
        );
        return EventOutcome::Continue;
    }

    let cron_expr = match parts.next() {
        Some(value) => value,
        None => {
            push_obs(
                state,
                "⚠ usage: /watch add <cron_expr> <prompt...>".to_string(),
            );
            return EventOutcome::Continue;
        }
    };

    let prompt = parts.collect::<Vec<_>>().join(" ");
    if prompt.trim().is_empty() {
        push_obs(
            state,
            "⚠ usage: /watch add <cron_expr> <prompt...>".to_string(),
        );
        return EventOutcome::Continue;
    }

    let request_id = next_worker_request_id(state);
    state.latest_watch_add_request_id = request_id;
    let daemon_url = state.daemon_url.clone();
    let cron_expr = cron_expr.to_string();
    let queued = queue_worker_command(
        state,
        WorkerCommand::WatchAdd {
            request_id,
            daemon_url,
            cron_expr: cron_expr.clone(),
            prompt,
        },
        false,
    );
    if queued {
        push_obs(
            state,
            format!("↻ watch add queued #{request_id} ({cron_expr})"),
        );
    }

    EventOutcome::Continue
}

pub(crate) async fn daemon_health(daemon_url: &str) -> Result<HealthResponse> {
    daemon_client(daemon_url)
        .health()
        .get()
        .await
        .map_err(sdk_err)
}

pub(crate) async fn daemon_enqueue_ask(daemon_url: &str, prompt: &str) -> Result<EnqueueResponse> {
    let request = EnqueueAskRequest {
        prompt: prompt.to_string(),
        policy_profile: Some("interactive".to_string()),
        model_hint: None,
        max_turns: Some(1),
        identity_user_id: None,
        identity_persona_id: None,
        identity_channel_id: None,
        manuscript_id: None,
        additional_manuscript_ids: None,
        suggested_capability_ids: None,
    };
    daemon_client(daemon_url)
        .jobs()
        .enqueue_ask(&request)
        .await
        .map_err(sdk_err)
}

pub(crate) async fn daemon_register_recurring_prompt(
    daemon_url: &str,
    cron_expr: &str,
    prompt: &str,
) -> Result<RegisterRecurringResponse> {
    let request = RegisterRecurringPromptRequest {
        id: None,
        queue: Some("default".to_string()),
        prompt: prompt.to_string(),
        system_prompt: Some(
            medousa::agent_runtime::LIGHTWEIGHT_CHANNEL_SYSTEM_PROMPT.to_string(),
        ),
        cron_expr: cron_expr.to_string(),
        timezone: Some("UTC".to_string()),
        jitter_seconds: Some(0),
        enabled: Some(true),
        max_attempts: Some(1),
        policy_profile: Some("scheduled".to_string()),
        model_hint: None,
        delivery: None,
        session_id: None,
        execution_mode: None,
        manuscript_id: None,
        display_name: None,
    };
    daemon_client(daemon_url)
        .recurring()
        .register_prompt(&request)
        .await
        .map_err(sdk_err)
}

pub(crate) async fn daemon_artifact_command(
    daemon_url: &str,
    request: &ArtifactCommandRequest,
) -> Result<ArtifactCommandResponse> {
    daemon_client(daemon_url)
        .runtime()
        .artifact_command(request)
        .await
        .map_err(sdk_err)
}

pub(crate) async fn daemon_stage_route_command(
    daemon_url: &str,
    request: &StageRouteCommandRequest,
) -> Result<StageRouteCommandResponse> {
    daemon_client(daemon_url)
        .runtime()
        .stage_route_command(request)
        .await
        .map_err(sdk_err)
}

pub(crate) async fn daemon_runtime_config_command(
    daemon_url: &str,
    request: &RuntimeConfigCommandRequest,
) -> Result<RuntimeConfigCommandResponse> {
    daemon_client(daemon_url)
        .runtime()
        .config_command(request)
        .await
        .map_err(sdk_err)
}

pub(crate) async fn daemon_start_interactive_turn(
    daemon_url: &str,
    request: &InteractiveTurnRequest,
) -> Result<InteractiveTurnResponse> {
    daemon_client(daemon_url)
        .interactive()
        .start_turn(request)
        .await
        .map_err(sdk_err)
}

pub(crate) async fn daemon_list_history_sessions(
    daemon_url: &str,
    limit: usize,
) -> Result<SessionHistoryListResponse> {
    daemon_client(daemon_url)
        .sessions()
        .list(limit)
        .await
        .map_err(sdk_err)
}

pub(crate) async fn daemon_load_session_history(
    daemon_url: &str,
    session_id: &str,
) -> Result<SessionHistoryResponse> {
    daemon_client(daemon_url)
        .sessions()
        .history(session_id)
        .await
        .map_err(sdk_err)
}

pub(crate) async fn daemon_set_session_display_name(
    daemon_url: &str,
    session_id: &str,
    display_name: &str,
) -> Result<SessionSetDisplayNameResponse> {
    daemon_client(daemon_url)
        .sessions()
        .set_display_name(session_id, display_name)
        .await
        .map_err(sdk_err)
}

pub(crate) async fn daemon_list_budget_requests(
    daemon_url: &str,
    pending_only: bool,
) -> Result<TurnBudgetRequestListResponse> {
    daemon_client(daemon_url)
        .budget()
        .list(pending_only)
        .await
        .map_err(sdk_err)
}

pub(crate) async fn daemon_approve_budget_request(
    daemon_url: &str,
    request_id: &str,
    body: &TurnBudgetApproveRequest,
) -> Result<TurnBudgetRequestResponse> {
    daemon_client(daemon_url)
        .budget()
        .approve(request_id, body)
        .await
        .map_err(sdk_err)
}

pub(crate) async fn daemon_deny_budget_request(
    daemon_url: &str,
    request_id: &str,
    body: &TurnBudgetDenyRequest,
) -> Result<TurnBudgetRequestResponse> {
    daemon_client(daemon_url)
        .budget()
        .deny(request_id, body)
        .await
        .map_err(sdk_err)
}

pub(crate) async fn daemon_append_session_turn(
    daemon_url: &str,
    session_id: &str,
    request: &SessionAppendTurnRequest,
) -> Result<SessionAppendTurnResponse> {
    daemon_client(daemon_url)
        .sessions()
        .append_turn(session_id, request)
        .await
        .map_err(sdk_err)
}
