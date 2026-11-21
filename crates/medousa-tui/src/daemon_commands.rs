use std::str::SplitWhitespace;
use std::sync::Arc;

use anyhow::Result;
use medousa_sdk::{HttpTransport, MedousaClient};

use medousa_tui::daemon_api::{
    TurnBudgetApproveRequest, TurnBudgetDenyRequest, TurnBudgetRequestListResponse,
    TurnBudgetRequestResponse,
};
use medousa_tui::{
    ArtifactCommandRequest, ArtifactCommandResponse, EnqueueAskRequest, EnqueueResponse,
    HealthResponse, InteractiveTurnRequest, InteractiveTurnResponse,
    RegisterRecurringPromptRequest, RegisterRecurringResponse, RuntimeConfigCommandRequest,
    RuntimeConfigCommandResponse, SessionAppendTurnRequest, SessionAppendTurnResponse,
    SessionHistoryListResponse, SessionHistoryResponse, SessionSetDisplayNameResponse,
    StageRouteCommandRequest, StageRouteCommandResponse,
};

use super::{
    EventOutcome, TuiState, WorkerCommand, next_worker_request_id, push_obs_alert,
    queue_worker_command,
};

pub(crate) fn daemon_client(daemon_url: &str) -> Result<MedousaClient> {
    let client = medousa_tui::local_daemon_auth::async_client(
        daemon_url,
        medousa_local_credential::TUI_LOCAL_NAME,
    )?;
    Ok(MedousaClient::with_transport(
        Arc::new(HttpTransport::with_client(client)),
        daemon_url,
    ))
}

pub(crate) async fn workshop_defaults(url: &str) -> medousa_tui::session::TuiDefaults {
    use medousa_tui::session::TuiDefaults;
    let local = super::load_tui_defaults();
    let mut defaults = TuiDefaults::default();
    if let Ok(client) = daemon_client(url) {
        let load = async {
            if let Ok(raw) = client
                .transport()
                .get_json(url, "/v1/runtime/tui-defaults")
                .await
            {
                defaults = serde_json::from_value(raw).unwrap_or_default();
            }
            if let Ok(raw) = client
                .transport()
                .get_json(url, "/v1/runtime/defaults")
                .await
                && let Ok(runtime) =
                    serde_json::from_value::<medousa_types::RuntimeDefaultsResponse>(raw)
            {
                defaults.provider = Some(runtime.provider);
                defaults.model = Some(runtime.model);
                defaults.backend = Some(runtime.backend);
                defaults.base_url = runtime.base_url;
                defaults.response_depth_mode = Some(runtime.response_depth_mode);
                defaults.reasoning_effort = Some(runtime.reasoning_effort);
                defaults.stage_routing = Some(runtime.stage_routing);
                defaults.inference_profiles = runtime
                    .inference_profiles
                    .or(defaults.inference_profiles.take());
            }
        };
        let _ = tokio::time::timeout(std::time::Duration::from_secs(3), load).await;
    }
    defaults.theme_id = local.theme_id;
    defaults.command_usage_counts = local.command_usage_counts;
    defaults
}

pub(crate) async fn update_workshop_settings(
    url: &str,
    edits: serde_json::Map<String, serde_json::Value>,
) -> Result<()> {
    let client = daemon_client(url)?;
    let path = "/v1/runtime/tui-defaults";
    let mut document = client.transport().get_json(url, path).await?;
    let settings = document
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("Invalid workshop settings"))?;
    if let Some(main) = settings
        .get_mut("inference_profiles")
        .and_then(|p| p.get_mut("main"))
        .and_then(|p| p.as_object_mut())
    {
        for field in ["provider", "model", "base_url"] {
            if let Some(value) = edits.get(field) {
                main.insert(field.to_owned(), value.clone());
            }
        }
    }
    settings.extend(edits);
    client.transport().put_json(url, path, document).await?;
    Ok(())
}

fn sdk_err(err: medousa_sdk::SdkError) -> anyhow::Error {
    anyhow::anyhow!(err.to_string())
}

pub(crate) async fn handle_daemon_command(
    parts: &mut SplitWhitespace<'_>,
    state: &mut TuiState,
) -> EventOutcome {
    let sub = parts.next().unwrap_or("");
    match sub {
        "" => {
            push_obs_alert(
                state,
                format!(
                    "daemon url={}  connection={}  scope={} | /daemon health | /daemon ask | /daemon url <url> | /connection",
                    state.daemon_url, state.workshop_label, state.workshop_scope
                ),
            );
        }
        "url" => {
            let next = parts.collect::<Vec<_>>().join(" ");
            if next.trim().is_empty() {
                push_obs_alert(
                    state,
                    format!(
                        "daemon url={}  connection={}  scope={}",
                        state.daemon_url, state.workshop_label, state.workshop_scope
                    ),
                );
            } else {
                let _ = super::connection_runtime::apply_connection(state, next.trim(), None, None)
                    .await;
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
                push_obs_alert(
                    state,
                    format!("↻ daemon health check queued #{request_id}: {daemon_url}"),
                );
            }
        }
        "ask" => {
            let prompt = parts.collect::<Vec<_>>().join(" ");
            if prompt.trim().is_empty() {
                push_obs_alert(state, "⚠ usage: /daemon ask <prompt>".to_string());
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
                    push_obs_alert(state, format!("↻ daemon ask queued #{request_id}"));
                }
            }
        }
        _ => {
            push_obs_alert(
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
        push_obs_alert(
            state,
            "⚠ usage: /watch add <cron_expr> <prompt...>".to_string(),
        );
        return EventOutcome::Continue;
    }

    let cron_expr = match parts.next() {
        Some(value) => value,
        None => {
            push_obs_alert(
                state,
                "⚠ usage: /watch add <cron_expr> <prompt...>".to_string(),
            );
            return EventOutcome::Continue;
        }
    };

    let prompt = parts.collect::<Vec<_>>().join(" ");
    if prompt.trim().is_empty() {
        push_obs_alert(
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
        push_obs_alert(
            state,
            format!("↻ watch add queued #{request_id} ({cron_expr})"),
        );
    }

    EventOutcome::Continue
}

pub(crate) async fn daemon_health(daemon_url: &str) -> Result<HealthResponse> {
    daemon_client(daemon_url)?
        .health()
        .get()
        .await
        .map_err(sdk_err)
}

pub(crate) async fn daemon_enqueue_ask(daemon_url: &str, prompt: &str) -> Result<EnqueueResponse> {
    let request = EnqueueAskRequest {
        host_context: None,
        prompt: prompt.to_string(),
        idempotency_key: None,
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
    daemon_client(daemon_url)?
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
        system_prompt: None,
        cron_expr: cron_expr.to_string(),
        timezone: Some("UTC".to_string()),
        jitter_seconds: Some(0),
        enabled: Some(true),
        max_attempts: Some(1),
        policy_profile: Some("scheduled".to_string()),
        model_hint: None,
        delivery: None,
        feeds: None,
        session_id: None,
        execution_mode: None,
        manuscript_id: None,
        display_name: None,
        notify_on_delivery: None,
    };
    daemon_client(daemon_url)?
        .recurring()
        .register_prompt(&request)
        .await
        .map_err(sdk_err)
}

pub(crate) async fn daemon_artifact_command(
    daemon_url: &str,
    request: &ArtifactCommandRequest,
) -> Result<ArtifactCommandResponse> {
    daemon_client(daemon_url)?
        .runtime()
        .artifact_command(request)
        .await
        .map_err(sdk_err)
}

pub(crate) async fn daemon_stage_route_command(
    daemon_url: &str,
    request: &StageRouteCommandRequest,
) -> Result<StageRouteCommandResponse> {
    daemon_client(daemon_url)?
        .runtime()
        .stage_route_command(request)
        .await
        .map_err(sdk_err)
}

pub(crate) async fn daemon_runtime_config_command(
    daemon_url: &str,
    request: &RuntimeConfigCommandRequest,
) -> Result<RuntimeConfigCommandResponse> {
    daemon_client(daemon_url)?
        .runtime()
        .config_command(request)
        .await
        .map_err(sdk_err)
}

pub(crate) async fn daemon_start_interactive_turn(
    daemon_url: &str,
    request: &InteractiveTurnRequest,
) -> Result<InteractiveTurnResponse> {
    daemon_client(daemon_url)?
        .interactive()
        .start_turn(request)
        .await
        .map_err(sdk_err)
}

pub(crate) async fn daemon_list_history_sessions(
    daemon_url: &str,
    limit: usize,
) -> Result<SessionHistoryListResponse> {
    daemon_client(daemon_url)?
        .sessions()
        .list(limit)
        .await
        .map_err(sdk_err)
}

pub(crate) async fn daemon_load_session_history(
    daemon_url: &str,
    session_id: &str,
) -> Result<SessionHistoryResponse> {
    daemon_client(daemon_url)?
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
    daemon_client(daemon_url)?
        .sessions()
        .set_display_name(session_id, display_name)
        .await
        .map_err(sdk_err)
}

pub(crate) async fn daemon_list_budget_requests(
    daemon_url: &str,
    pending_only: bool,
) -> Result<TurnBudgetRequestListResponse> {
    daemon_client(daemon_url)?
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
    daemon_client(daemon_url)?
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
    daemon_client(daemon_url)?
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
    daemon_client(daemon_url)?
        .sessions()
        .append_turn(session_id, request)
        .await
        .map_err(sdk_err)
}

/// The provider credential stays on the selected workshop. Only explicit edits send a secret.
pub(crate) async fn save_api_key(daemon_url: &str, value: Option<&str>) -> Result<()> {
    use medousa_types::secrets::{IntegrationConnection, IntegrationConnectionListResponse};
    let url = reqwest::Url::parse(daemon_url)?;
    let loopback = url.host_str().is_some_and(|h| {
        h == "localhost"
            || h.trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    });
    anyhow::ensure!(
        url.scheme() == "https" || loopback,
        "Use HTTPS or loopback to update a provider credential"
    );
    let client = medousa_tui::local_daemon_auth::async_client(
        daemon_url,
        medousa_local_credential::TUI_LOCAL_NAME,
    )?;
    let endpoint = format!("{}/v1/integrations", daemon_url.trim_end_matches('/'));
    let response: IntegrationConnectionListResponse = client
        .get(&endpoint)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let mut matches = response
        .connections
        .into_iter()
        .filter(|c| c.kind == "openai");
    let connection = match matches.next() {
        Some(c) => {
            anyhow::ensure!(
                matches.next().is_none(),
                "Multiple OpenAI connections. Select the connection in Medousa Settings."
            );
            c
        }
        None if value.is_none() => return Ok(()),
        None => {
            client
                .post(&endpoint)
                .json(&serde_json::json!({"kind":"openai"}))
                .send()
                .await?
                .error_for_status()?
                .json::<IntegrationConnection>()
                .await?
        }
    };
    let path = format!(
        "{endpoint}/{}/secrets/api_key",
        connection.connection_id.as_str()
    );
    match value {
        Some(value) => client.put(path).json(&serde_json::json!({"value": value})),
        None => client.delete(path),
    }
    .send()
    .await?
    .error_for_status()?;
    Ok(())
}
