//! Job enqueue, result/report, recurring prompts, and workspace retry handlers.

use std::collections::HashSet;
use std::sync::Arc;

use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

use anyhow::Result as AnyhowResult;

use crate::artifact_chunking::chunk_json_payload;
use crate::artifact_extraction::{extract_claims_from_chunks, persist_extraction_run};
use crate::context_pack::{
    BuildContextPackInput, ContextPackBudgetProfile, build_context_pack, persist_context_pack,
};
use crate::channel_delivery;
use crate::daemon::identity::resolve_identity_context_for_request;
use crate::daemon::interactive::{build_interactive_request_from_ticket, spawn_turn_ticket};
use crate::daemon::ingest::{get_job_attempts_graceful, resolve_api_model_routing, spawn_daemon_api_agent_turn};
use crate::engine_context::{
    EngineExecutionLane, LaneSafetyActionClass, compile_default_lane_prompt,
    default_policy_profile_for_lane, validate_lane_action, validate_lane_policy_profile,
};
use crate::verifier::{VerificationPolicy, verify_context_pack};
use crate::verification_store::persist_verification;
use stasis::application::orchestration::runtime_job_payloads::PromptJobPayload;
use stasis::application::runtime::identity_context_compiler::prepend_identity_snapshot;
use stasis::application::orchestration::runtime_workflow_job_builder::RuntimeWorkflowJobBuilder;
use stasis::prelude::{RecurringDefinition, RuntimeComposition, RuntimeSdk};
use crate::daemon_api::{
    CreateTurnTicketRequest, DeleteRecurringResponse, EnqueueAskRequest, EnqueuePromptRequest, EnqueueReportRequest,
    EnqueueResponse, JobCitationResponse, JobEvidenceReportResponse, JobReportResponse,
    JobResultResponse, RecurringDeliveryResponse, RecurringListQuery, RecurringListResponse,
    RecurringRunsQuery, RecurringRunsResponse, RegisterRecurringPromptRequest,
    RegisterRecurringResponse, UpdateRecurringRequest, UpdateRecurringResponse,
};

use crate::daemon::http::internal_error;
use crate::daemon::state::{AgentTurnJobRecord, AppState};

const DAEMON_REPORT_SESSION_ID: &str = "medousa-daemon-reports";
const MAX_REPORT_CITATIONS: usize = 24;
pub async fn get_job_result(
    State(state): State<AppState>,
    AxumPath(job_id): AxumPath<String>,
) -> Result<Json<JobResultResponse>, (StatusCode, String)> {
    let job_id = job_id.trim().to_string();
    if job_id.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "job_id is required".to_string()));
    }

    if let Some(record) = crate::workspace::ask_job_store::ask_job_store().get(&job_id) {
        return Ok(Json(job_result_from_ask_job(&job_id, &record)));
    }

    if let Some(record) = state.agent_turn_jobs.read().await.get(&job_id) {
        return Ok(Json(job_result_from_agent_turn(&job_id, record)));
    }

    let attempts = match get_job_attempts_graceful(&state.composition(), &job_id).await {
        Ok(attempts) => attempts,
        Err(err) => return Err(err),
    };

    let latest = attempts.last();
    let latest_outcome = latest.map(|attempt| format!("{:?}", attempt.outcome));
    let latest_execution_id = latest.and_then(|attempt| attempt.execution_id.clone());
    let output_text = latest
        .and_then(|attempt| {
            channel_delivery::extract_output_text_from_diagnostics(attempt.diagnostics.as_deref())
        });

    let (status, is_terminal) = derive_job_result_status(latest_outcome.as_deref(), attempts.len());

    Ok(Json(JobResultResponse {
        job_id,
        status,
        is_terminal,
        attempt_count: attempts.len(),
        latest_outcome,
        latest_execution_id,
        output_text,
        interim_text: None,
    }))
}

pub async fn get_job_report(
    State(state): State<AppState>,
    AxumPath(job_id): AxumPath<String>,
) -> Result<Json<JobReportResponse>, (StatusCode, String)> {
    let job_id = job_id.trim().to_string();
    if job_id.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "job_id is required".to_string()));
    }

    if let Some(record) = state.agent_turn_jobs.read().await.get(&job_id) {
        let base = job_result_from_agent_turn(&job_id, record);
        return Ok(Json(JobReportResponse {
            job_id: base.job_id,
            status: base.status,
            is_terminal: base.is_terminal,
            attempt_count: base.attempt_count,
            latest_outcome: base.latest_outcome,
            latest_execution_id: base.latest_execution_id,
            output_text: base.output_text,
            citations: Vec::new(),
            evidence_report: None,
        }));
    }

    let attempts = match get_job_attempts_graceful(&state.composition(), &job_id).await {
        Ok(attempts) => attempts,
        Err(err) => return Err(err),
    };

    let latest = attempts.last();
    let latest_outcome = latest.map(|attempt| format!("{:?}", attempt.outcome));
    let latest_execution_id = latest.and_then(|attempt| attempt.execution_id.clone());
    let output_text = latest
        .and_then(|attempt| {
            channel_delivery::extract_output_text_from_diagnostics(attempt.diagnostics.as_deref())
        });
    let diagnostics = latest
        .and_then(|attempt| attempt.diagnostics.as_deref())
        .and_then(parse_diagnostics_json);

    let (status, is_terminal) = derive_job_result_status(latest_outcome.as_deref(), attempts.len());
    let citations = diagnostics
        .as_ref()
        .map(extract_citations_from_payload)
        .unwrap_or_default();
    let evidence_report = if is_terminal && status == "succeeded" {
        diagnostics
            .as_ref()
            .and_then(|payload| build_job_evidence_report(&job_id, payload))
    } else {
        None
    };

    Ok(Json(JobReportResponse {
        job_id,
        status,
        is_terminal,
        attempt_count: attempts.len(),
        latest_outcome,
        latest_execution_id,
        output_text,
        citations,
        evidence_report,
    }))
}

fn normalize_ask_manuscript_ids(
    primary: Option<&str>,
    additional: Option<&[String]>,
) -> Vec<String> {
    let mut ids = Vec::new();
    if let Some(id) = primary.map(str::trim).filter(|value| !value.is_empty()) {
        ids.push(id.to_string());
    }
    if let Some(more) = additional {
        for id in more {
            let trimmed = id.trim();
            if trimmed.is_empty() || ids.iter().any(|existing| existing == trimmed) {
                continue;
            }
            ids.push(trimmed.to_string());
        }
    }
    ids
}

fn normalize_ask_capability_ids(ids: Option<Vec<String>>) -> Vec<String> {
    ids.unwrap_or_default()
        .into_iter()
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect()
}

fn resolve_enqueue_ask_prompt(prompt: &str, manuscript_ids: &[String]) -> Result<String, String> {
    let trimmed = prompt.trim();
    if !trimmed.is_empty() {
        return Ok(trimmed.to_string());
    }
    let primary = manuscript_ids
        .first()
        .ok_or_else(|| "prompt is required".to_string())?;
    let ctx = crate::identity_manuscript::build_manuscript_context(primary)
        .map_err(|err| err.to_string())?;
    crate::identity_manuscript::render_manuscript_task_prompt(&ctx, None)
        .map_err(|err| err.to_string())
}

pub async fn enqueue_ask(
    State(state): State<AppState>,
    Json(request): Json<EnqueueAskRequest>,
) -> Result<Json<EnqueueResponse>, (StatusCode, String)> {
    let manuscript_ids = normalize_ask_manuscript_ids(
        request.manuscript_id.as_deref(),
        request.additional_manuscript_ids.as_deref(),
    );
    let suggested_capability_ids = normalize_ask_capability_ids(request.suggested_capability_ids);
    let prompt = resolve_enqueue_ask_prompt(&request.prompt, &manuscript_ids)
        .map_err(|message| (StatusCode::BAD_REQUEST, message))?;

    enforce_lane_safety(
        EngineExecutionLane::Interactive,
        LaneSafetyActionClass::InteractiveIngress,
        request.policy_profile.as_deref(),
    )?;

    let effective_policy_profile = request.policy_profile.unwrap_or_else(|| {
        default_policy_profile_for_lane(EngineExecutionLane::Interactive).to_string()
    });
    let _identity_context = resolve_identity_context_for_request(
        &state,
        request.identity_user_id.as_deref(),
        request.identity_persona_id.as_deref(),
        request.identity_channel_id.as_deref(),
        Some(effective_policy_profile.as_str()),
        8,
    )
    .await?;

    let now = Utc::now();
    let job_id = format!("medousa-daemon-ask-{}", now.timestamp_millis());
    let session_id = crate::workspace::ask_job_store::ask_job_session_id(&job_id);
    let (provider, model) =
        resolve_api_model_routing(request.model_hint.as_deref(), &state.default_runtime_config);
    let manuscript_id = manuscript_ids.first().cloned();
    let additional_manuscript_ids = if manuscript_ids.len() > 1 {
        Some(manuscript_ids.into_iter().skip(1).collect())
    } else {
        None
    };
    let suggested_capability_ids = if suggested_capability_ids.is_empty() {
        None
    } else {
        Some(suggested_capability_ids)
    };

    let stage_routing = crate::stage_routing::StageRoutingMatrix::default_for(
        if provider.is_empty() {
            "openai"
        } else {
            provider.as_str()
        },
        if model.is_empty() {
            "gpt-4o-mini"
        } else {
            model.as_str()
        },
    );
    let ticket_request = CreateTurnTicketRequest {
        session_id: session_id.clone(),
        prompt,
        mode: crate::turn_ticket::TurnTicketMode::Background,
        persist_user_turn: true,
        response_depth_mode: state.default_runtime_config.response_depth_mode.clone(),
        reasoning_effort: state.default_runtime_config.reasoning_effort.clone(),
        provider: provider.clone(),
        model: model.clone(),
        stage_routing: Some(stage_routing.clone()),
        surface: None,
        model_hint: request.model_hint.clone(),
        manuscript_id,
        additional_manuscript_ids,
        suggested_capability_ids,
        voice_preset_id: None,
        voice_appendix: None,
        media_refs: Vec::new(),
        identity_user_id: None,
    };
    let interactive_request =
        build_interactive_request_from_ticket(&ticket_request, provider, model, stage_routing);

    let ticket = spawn_turn_ticket(
        &state,
        job_id.clone(),
        crate::turn_ticket::TurnTicketMode::Background,
        interactive_request,
        Some(job_id.clone()),
    )
    .await?;

    Ok(Json(EnqueueResponse {
        job_id: ticket.turn_id,
        queue: "turn-ticket".to_string(),
        accepted_at_utc: ticket.accepted_at_utc,
    }))
}

fn map_workspace_card_action_error(
    err: crate::workspace::actions::CardActionError,
) -> (StatusCode, String) {
    match err {
        crate::workspace::actions::CardActionError::NotFound => {
            (StatusCode::NOT_FOUND, err.message())
        }
        crate::workspace::actions::CardActionError::NotActionable(reason) => {
            (StatusCode::BAD_REQUEST, reason)
        }
        crate::workspace::actions::CardActionError::Internal(reason) => {
            (StatusCode::INTERNAL_SERVER_ERROR, reason)
        }
    }
}

pub async fn retry_workspace_card(
    State(state): State<AppState>,
    AxumPath(card_id): AxumPath<String>,
) -> Result<Json<crate::daemon_api::WorkspaceCardActionResponse>, (StatusCode, String)> {
    let card_id = card_id.trim();
    if card_id.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "card_id is required".to_string()));
    }

    let composition = Arc::new(state.composition().clone());
    let detail = crate::workspace::WorkspaceService::get_card_detail(
        composition.clone(),
        card_id,
    )
    .await
    .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
    .ok_or_else(|| (StatusCode::NOT_FOUND, format!("card not found: {card_id}")))?;

    if detail.kind == crate::daemon_api::WorkCardKind::AskJob {
        return retry_ask_workspace_card(&state, card_id, &detail)
            .await
            .map(Json);
    }

    crate::workspace::actions::retry_card(composition, card_id, &state.worker_id)
        .await
        .map(Json)
        .map_err(map_workspace_card_action_error)
}

async fn retry_ask_workspace_card(
    state: &AppState,
    card_id: &str,
    detail: &crate::daemon_api::WorkCardDetail,
) -> Result<crate::daemon_api::WorkspaceCardActionResponse, (StatusCode, String)> {
    let job_id = detail.job_id.clone().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            "ask card missing job_id".to_string(),
        )
    })?;

    if !crate::workspace::ask_job_store::AskJobStore::is_ask_job_id(&job_id) {
        return Err((
            StatusCode::BAD_REQUEST,
            "retry is only supported for daemon ask job cards".to_string(),
        ));
    }

    let record = crate::workspace::ask_job_store::ask_job_store()
        .reset_for_retry(&job_id)
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                "retry is only supported for failed or canceled ask jobs".to_string(),
            )
        })?;

    let (provider, model) =
        resolve_api_model_routing(record.model_hint.as_deref(), &state.default_runtime_config);

    spawn_daemon_api_agent_turn(
        state,
        job_id.clone(),
        record.session_id.clone(),
        record.prompt.clone(),
        state.default_runtime_config.response_depth_mode.clone(),
        state.default_runtime_config.reasoning_effort.clone(),
        provider,
        model,
        record.manuscript_id.clone(),
        record.additional_manuscript_ids.clone(),
        record.suggested_capability_ids.clone(),
    )
    .await;

    crate::workspace::notify_workspace_event(crate::workspace::WorkspaceDomainEvent::AskJobChanged {
        job_id: job_id.clone(),
    });

    Ok(crate::daemon_api::WorkspaceCardActionResponse {
        workspace_revision: crate::workspace::store::workspace_store().revision(),
        card_id: card_id.to_string(),
        action: "retry".to_string(),
        ok: true,
        message: format!("ask {job_id} re-queued"),
        job_id: Some(job_id),
        replayed: Some(true),
        job_succeeded: None,
        associations: None,
    })
}

pub async fn enqueue_report(
    State(state): State<AppState>,
    Json(request): Json<EnqueueReportRequest>,
) -> Result<Json<EnqueueResponse>, (StatusCode, String)> {
    if request.query.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "query is required".to_string()));
    }

    enforce_lane_safety(
        EngineExecutionLane::Interactive,
        LaneSafetyActionClass::InteractiveIngress,
        request.policy_profile.as_deref(),
    )?;

    let effective_policy_profile = request.policy_profile.unwrap_or_else(|| {
        default_policy_profile_for_lane(EngineExecutionLane::Interactive).to_string()
    });
    let identity_context = resolve_identity_context_for_request(
        &state,
        request.identity_user_id.as_deref(),
        request.identity_persona_id.as_deref(),
        request.identity_channel_id.as_deref(),
        Some(effective_policy_profile.as_str()),
        8,
    )
    .await?;

    let now = Utc::now();
    let job_id = format!("medousa-daemon-report-{}", now.timestamp_millis());
    let session_id = format!("daemon-report:{}", identity_context.user_id);
    let (provider, model) =
        resolve_api_model_routing(request.model_hint.as_deref(), &state.default_runtime_config);
    let prompt = build_report_prompt(&request.query);

    spawn_daemon_api_agent_turn(
        &state,
        job_id.clone(),
        session_id,
        prompt,
        state.default_runtime_config.response_depth_mode.clone(),
        state.default_runtime_config.reasoning_effort.clone(),
        provider,
        model,
        None,
        None,
        None,
    )
    .await;

    Ok(Json(EnqueueResponse {
        job_id,
        queue: "agent-runtime".to_string(),
        accepted_at_utc: now,
    }))
}

pub async fn enqueue_prompt(
    State(state): State<AppState>,
    Json(request): Json<EnqueuePromptRequest>,
) -> Result<Json<EnqueueResponse>, (StatusCode, String)> {
    if request.prompt.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "prompt is required".to_string()));
    }

    enforce_lane_safety(
        EngineExecutionLane::Interactive,
        LaneSafetyActionClass::InteractiveIngress,
        request.policy_profile.as_deref(),
    )?;

    let effective_policy_profile = request.policy_profile.unwrap_or_else(|| {
        default_policy_profile_for_lane(EngineExecutionLane::Interactive).to_string()
    });
    let identity_context = resolve_identity_context_for_request(
        &state,
        request.identity_user_id.as_deref(),
        request.identity_persona_id.as_deref(),
        request.identity_channel_id.as_deref(),
        Some(effective_policy_profile.as_str()),
        8,
    )
    .await?;

    let now = Utc::now();
    let job_id = format!("medousa-daemon-prompt-{}", now.timestamp_millis());
    let prompt_with_identity = prepend_identity_snapshot(&request.prompt, Some(&identity_context.summary));
    let compiled_prompt =
        compile_default_lane_prompt(EngineExecutionLane::Interactive, &prompt_with_identity);

    let payload = PromptJobPayload {
        user_prompt: compiled_prompt,
        system_prompt: request.system_prompt.or(Some(
            crate::agent_runtime::LIGHTWEIGHT_CHANNEL_SYSTEM_PROMPT.to_string(),
        )),
        policy_profile: Some(effective_policy_profile),
        model_hint: request.model_hint,
        reasoning_effort: None,
        memory_policy: None,
    };

    let new_job = RuntimeWorkflowJobBuilder::for_prompt(job_id.clone(), &payload)
        .map_err(internal_error)?
        .with_correlation_id(identity_context.user_id)
        .with_causation_id("medousa-daemon-api:interactive")
        .with_sttp_input_node_id("sttp:in:medousa:daemon:interactive:prompt")
        .with_scheduled_at(now)
        .build();

    enqueue_runtime_job(state.composition(), new_job)
        .await
        .map_err(internal_error)?;

    Ok(Json(EnqueueResponse {
        job_id,
        queue: "default".to_string(),
        accepted_at_utc: now,
    }))
}

pub async fn list_recurring_definitions(
    State(state): State<AppState>,
    Query(query): Query<RecurringListQuery>,
) -> Result<Json<RecurringListResponse>, (StatusCode, String)> {
    crate::recurring_handlers::list_recurring(state.composition(), query)
        .await
        .map(Json)
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))
}

pub async fn update_recurring_definition(
    State(state): State<AppState>,
    AxumPath(recurring_id): AxumPath<String>,
    Json(request): Json<UpdateRecurringRequest>,
) -> Result<Json<UpdateRecurringResponse>, (StatusCode, String)> {
    crate::recurring_handlers::update_recurring(
        state.composition(),
        recurring_id.trim(),
        request,
    )
    .await
    .map(Json)
    .map_err(|err| {
        let message = err.to_string();
        if message.contains("not found") {
            (StatusCode::NOT_FOUND, message)
        } else {
            (StatusCode::BAD_REQUEST, message)
        }
    })
}

pub async fn delete_recurring_definition(
    State(state): State<AppState>,
    AxumPath(recurring_id): AxumPath<String>,
) -> Result<Json<DeleteRecurringResponse>, (StatusCode, String)> {
    crate::recurring_handlers::delete_recurring(state.composition(), recurring_id.trim())
        .await
        .map(Json)
        .map_err(|err| {
            let message = err.to_string();
            if message.contains("not found") {
                (StatusCode::NOT_FOUND, message)
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, message)
            }
        })
}

pub async fn list_recurring_runs_handler(
    State(state): State<AppState>,
    AxumPath(recurring_id): AxumPath<String>,
    Query(query): Query<RecurringRunsQuery>,
) -> Result<Json<RecurringRunsResponse>, (StatusCode, String)> {
    crate::recurring_handlers::list_recurring_runs(
        state.composition(),
        recurring_id.trim(),
        query,
    )
    .await
    .map(Json)
    .map_err(|err| {
        let message = err.to_string();
        if message.contains("not found") {
            (StatusCode::NOT_FOUND, message)
        } else {
            (StatusCode::INTERNAL_SERVER_ERROR, message)
        }
    })
}

pub async fn get_recurring_delivery_handler(
    AxumPath(recurring_id): AxumPath<String>,
) -> Result<Json<RecurringDeliveryResponse>, (StatusCode, String)> {
    crate::recurring_handlers::get_recurring_delivery(recurring_id.trim())
        .await
        .map(Json)
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))
}

pub async fn register_recurring_prompt(
    State(state): State<AppState>,
    Json(request): Json<RegisterRecurringPromptRequest>,
) -> Result<Json<RegisterRecurringResponse>, (StatusCode, String)> {
    let manuscript_id = request
        .manuscript_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let manuscript_ctx = if let Some(id) = manuscript_id.as_deref() {
        Some(
            crate::identity_manuscript::build_manuscript_context(id).map_err(|err| {
                (StatusCode::BAD_REQUEST, err.to_string())
            })?,
        )
    } else {
        None
    };
    if let Some(ctx) = manuscript_ctx.as_ref() {
        crate::identity_manuscript::validate_manuscript_for_scheduled_lane(ctx).map_err(
            |err| (StatusCode::BAD_REQUEST, err.to_string()),
        )?;
    }

    let prompt = if let Some(ctx) = manuscript_ctx.as_ref() {
        crate::identity_manuscript::render_manuscript_task_prompt(ctx, Some(&request.prompt))
            .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?
    } else if request.prompt.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "prompt is required".to_string()));
    } else {
        request.prompt.trim().to_string()
    };

    let cron_expr = if request.cron_expr.trim().is_empty() {
        manuscript_ctx
            .as_ref()
            .and_then(|ctx| ctx.schedule_cron.clone())
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    "cron_expr is required (or provide manuscript spec.schedule.cron)".to_string(),
                )
            })?
    } else {
        request.cron_expr.trim().to_string()
    };

    let timezone = request.timezone.as_deref().unwrap_or("UTC");
    crate::recurring_delivery::validate_recurring_cron(&cron_expr, timezone)
        .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;

    enforce_lane_safety(
        EngineExecutionLane::Scheduled,
        LaneSafetyActionClass::RecurringRegistration,
        request.policy_profile.as_deref(),
    )?;

    let now = Utc::now();
    let queue = request.queue.unwrap_or_else(|| "default".to_string());
    let timezone = timezone.to_string();
    let recurring_id = request
        .id
        .unwrap_or_else(|| format!("medousa-recurring-{}", Uuid::new_v4().simple()));
    let fallback_session_id = request
        .session_id
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("recurring-{recurring_id}"));
    let execution_mode = request
        .execution_mode
        .as_deref()
        .or_else(|| {
            manuscript_ctx
                .as_ref()
                .and_then(|ctx| ctx.schedule_execution_mode.as_deref())
        })
        .unwrap_or("agent_turn")
        .trim()
        .to_ascii_lowercase();

    let scheduled_tool_allowlist = manuscript_ctx
        .as_ref()
        .map(|ctx| {
            crate::identity_manuscript::scheduled_tool_allowlist_for_manuscript(ctx)
                .into_iter()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let max_tool_rounds = manuscript_ctx
        .as_ref()
        .and_then(|ctx| ctx.max_tool_rounds);

    let (job_type, payload_template_ref) = match execution_mode.as_str() {
        "prompt" => {
            let compiled_prompt =
                compile_default_lane_prompt(EngineExecutionLane::Scheduled, &prompt);
            let prompt_payload = PromptJobPayload {
                user_prompt: compiled_prompt,
                system_prompt: request.system_prompt.clone(),
                policy_profile: request.policy_profile.or_else(|| {
                    Some(
                        default_policy_profile_for_lane(EngineExecutionLane::Scheduled)
                            .to_string(),
                    )
                }),
                model_hint: request.model_hint.clone(),
                reasoning_effort: None,
                memory_policy: None,
            };
            (
                "workflow.stasis.prompt".to_string(),
                prompt_payload.to_payload_ref().map_err(internal_error)?,
            )
        }
        "agent_turn" | "agent-turn" => {
            let provider = crate::resolve_llm_provider(None);
            let model = crate::resolve_llm_model(
                request
                    .model_hint
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty()),
            );
            let agent_payload = crate::recurring_agent_turn::build_recurring_agent_turn_payload(
                &prompt,
                &fallback_session_id,
                request.system_prompt.clone(),
                request.policy_profile.clone(),
                request.model_hint.clone(),
                Some(provider),
                Some(model),
                manuscript_id.clone(),
                scheduled_tool_allowlist,
                max_tool_rounds,
            );
            (
                crate::recurring_agent_turn::RECURRING_AGENT_TURN_JOB_TYPE.to_string(),
                agent_payload.to_payload_ref().map_err(|err| {
                    (StatusCode::BAD_REQUEST, err.to_string())
                })?,
            )
        }
        other => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!(
                    "execution_mode={other} is invalid; use agent_turn or prompt"
                ),
            ));
        }
    };

    let payload_template_ref = crate::recurring_handlers::inject_display_name_into_payload(
        &payload_template_ref,
        request.display_name.as_deref(),
    );

    let mut definition = RecurringDefinition {
        id: recurring_id.clone(),
        queue: queue.clone(),
        job_type,
        payload_template_ref,
        cron_expr: cron_expr.clone(),
        timezone: timezone.clone(),
        jitter_seconds: request.jitter_seconds.unwrap_or(0),
        enabled: request.enabled.unwrap_or(true),
        max_attempts: request.max_attempts.unwrap_or(1),
        next_run_at: now,
        last_run_at: None,
        lease_owner: None,
        lease_expires_at: None,
    };

    definition.next_run_at = definition
        .compute_next_run_at(now)
        .map_err(internal_error)?;

    let delivery_input = serde_json::json!({ "delivery": request.delivery });
    crate::recurring_delivery::persist_recurring_delivery_binding(
        &recurring_id,
        &delivery_input,
        crate::recurring_delivery::DeliveryResolveContext {
            ambient: None,
            fallback_session_id,
        },
    )
    .await
    .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;

    let sdk = RuntimeSdk::new(state.composition().clone());
    sdk.register_recurring(definition.clone())
        .await
        .map_err(internal_error)?;

    Ok(Json(RegisterRecurringResponse {
        recurring_id,
        queue,
        next_run_at_utc: definition.next_run_at,
        cron_expr: definition.cron_expr,
        timezone,
    }))
}
fn job_result_from_agent_turn(job_id: &str, record: &AgentTurnJobRecord) -> JobResultResponse {
    let is_terminal = record.status != "pending";
    JobResultResponse {
        job_id: job_id.to_string(),
        status: record.status.clone(),
        is_terminal,
        attempt_count: usize::from(is_terminal),
        latest_outcome: record
            .error
            .clone()
            .or_else(|| Some(format!("status={}", record.status))),
        latest_execution_id: None,
        output_text: record.output_text.clone(),
        interim_text: None,
    }
}

fn job_result_from_ask_job(
    job_id: &str,
    record: &crate::workspace::ask_job_store::AskJobRecord,
) -> JobResultResponse {
    use crate::workspace::ask_job_store::AskJobStatus;

    let status = match record.status {
        AskJobStatus::Pending => "pending",
        AskJobStatus::Running => "running",
        AskJobStatus::Succeeded => "succeeded",
        AskJobStatus::Failed => "failed",
        AskJobStatus::Canceled => "canceled",
    };
    let is_terminal = matches!(
        record.status,
        AskJobStatus::Succeeded | AskJobStatus::Failed | AskJobStatus::Canceled
    );
    JobResultResponse {
        job_id: job_id.to_string(),
        status: status.to_string(),
        is_terminal,
        attempt_count: usize::from(is_terminal),
        latest_outcome: record
            .error
            .clone()
            .or_else(|| Some(format!("status={status}"))),
        latest_execution_id: None,
        output_text: record.output_text.clone(),
        interim_text: record.interim_text.clone(),
    }
}

pub async fn complete_ask_job_actions(
    State(state): State<AppState>,
    AxumPath(job_id): AxumPath<String>,
    Json(request): Json<crate::AskJobCompleteActionsRequest>,
) -> Result<Json<crate::AskJobCompleteActionsResponse>, (StatusCode, String)> {
    let job_id = job_id.trim().to_string();
    if job_id.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "job_id is required".to_string()));
    }
    if !crate::workspace::ask_job_store::AskJobStore::is_ask_job_id(&job_id) {
        return Err((
            StatusCode::BAD_REQUEST,
            "complete-actions is only supported for daemon ask jobs".to_string(),
        ));
    }

    let result = crate::workspace::ask_job_finalize::apply_ask_job_complete_actions(
        &job_id,
        crate::workspace::ask_job_finalize::AskJobCompleteActions {
            write_journal_path: request.write_journal_path,
            notify_channel: request.notify_channel,
        },
        &state.channel_dispatch_client,
    )
    .await
    .map_err(internal_error)?;

    Ok(Json(crate::AskJobCompleteActionsResponse {
        job_id,
        ok: true,
        message: result.message,
        journal_path: result.journal_path,
        notified_channel: result.notified_channel,
    }))
}

pub async fn archive_ask_job(
    AxumPath(job_id): AxumPath<String>,
    Json(request): Json<crate::ArchiveAskJobRequest>,
) -> Result<Json<crate::ArchiveAskJobResponse>, (StatusCode, String)> {
    let job_id = job_id.trim().to_string();
    if job_id.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "job_id is required".to_string()));
    }
    if !crate::workspace::ask_job_store::AskJobStore::is_ask_job_id(&job_id) {
        return Err((
            StatusCode::BAD_REQUEST,
            "archive is only supported for daemon ask jobs".to_string(),
        ));
    }

    crate::workspace::ask_job_store::ask_job_store()
        .archive(&job_id, request.purge_output)
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("ask job not found: {job_id}")))?;

    Ok(Json(crate::ArchiveAskJobResponse {
        job_id: job_id.clone(),
        archived: true,
        message: if request.purge_output {
            format!("archived ask {job_id} and cleared stored output")
        } else {
            format!("archived ask {job_id}")
        },
    }))
}

async fn enqueue_runtime_job(
    runtime: &RuntimeComposition,
    job: stasis::prelude::NewJob,
) -> AnyhowResult<()> {
    let sdk = RuntimeSdk::new(runtime.clone());
    sdk.enqueue(job).await?;
    Ok(())
}

fn parse_diagnostics_json(raw: &str) -> Option<Value> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    serde_json::from_str::<Value>(trimmed).ok()
}

pub fn build_report_prompt(query: &str) -> String {
    format!(
        "research question:\n{query}\n\nproduce a concise evidence-first report using this structure:\n1) executive summary\n2) key findings\n3) evidence table with explicit citations [C1], [C2], ...\n4) risks and unknowns\n5) next actions\n\nrequirements:\n- every non-trivial claim must include at least one citation marker\n- include a final citations section mapping markers to sources\n- if evidence is weak, say so explicitly"
    )
}

pub fn extract_citations_from_payload(payload: &Value) -> Vec<JobCitationResponse> {
    let mut seen = HashSet::new();
    let mut citations = Vec::new();
    collect_citations(payload, &mut seen, &mut citations);
    citations.truncate(MAX_REPORT_CITATIONS);
    citations
}

fn collect_citations(
    value: &Value,
    seen: &mut HashSet<String>,
    citations: &mut Vec<JobCitationResponse>,
) {
    match value {
        Value::Object(map) => {
            let source = map
                .get("source")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
                .or_else(|| {
                    map.get("url")
                        .and_then(|value| value.as_str())
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(ToString::to_string)
                });

            if let Some(source) = source {
                let title = map
                    .get("title")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToString::to_string);
                let key = format!("{}|{}", source, title.clone().unwrap_or_default());
                if seen.insert(key) {
                    citations.push(JobCitationResponse { source, title });
                }
            }

            for nested in map.values() {
                collect_citations(nested, seen, citations);
            }
        }
        Value::Array(values) => {
            for nested in values {
                collect_citations(nested, seen, citations);
            }
        }
        _ => {}
    }
}

fn build_job_evidence_report(job_id: &str, payload: &Value) -> Option<JobEvidenceReportResponse> {
    let artifact_id = format!("artifact:{job_id}:diagnostics");
    let chunk_refs = chunk_json_payload(&artifact_id, payload, 360, 40);
    if chunk_refs.is_empty() {
        return None;
    }

    let claims = extract_claims_from_chunks(&artifact_id, payload, &chunk_refs);
    let extraction_record = persist_extraction_run(DAEMON_REPORT_SESSION_ID, &artifact_id, &claims)
        .map_err(|err| {
            eprintln!(
                "medousa-daemon report extraction persist error job_id={} err={err}",
                job_id
            );
            err
        })
        .ok();

    let pack = build_context_pack(BuildContextPackInput {
        session_id: DAEMON_REPORT_SESSION_ID.to_string(),
        artifact_id: artifact_id.clone(),
        claims,
        chunk_refs,
        budget_profile: ContextPackBudgetProfile {
            max_tokens: 6000,
            max_claims: 12,
            max_chunks: 24,
        },
    });

    if let Err(err) = persist_context_pack(&pack) {
        eprintln!(
            "medousa-daemon report context-pack persist error job_id={} err={err}",
            job_id
        );
    }

    let policy = VerificationPolicy::default();
    let verification = verify_context_pack(&pack, &policy);
    let verification_record = persist_verification(
        DAEMON_REPORT_SESSION_ID,
        job_id,
        "daemon_job_report",
        &policy,
        &verification,
    )
    .map_err(|err| {
        eprintln!(
            "medousa-daemon report verification persist error job_id={} err={err}",
            job_id
        );
        err
    })
    .ok();

    Some(JobEvidenceReportResponse {
        session_id: DAEMON_REPORT_SESSION_ID.to_string(),
        artifact_id,
        extraction_id: extraction_record.map(|record| record.extraction_id),
        pack_id: pack.pack_id,
        verification_id: verification_record.map(|record| record.verification_id),
        verification_state: if verification.is_verified {
            "verified".to_string()
        } else {
            "provisional".to_string()
        },
        confidence_score: verification.confidence_score,
        citation_coverage: verification.citation_coverage,
        supported_claim_ratio: verification.supported_claim_ratio,
        total_claims: verification.total_claims,
        supported_claims: verification.supported_claims,
    })
}

/// Fetches job attempts, gracefully handling the case where the backend table
/// does not exist yet (fresh database without auto-migration).

pub fn derive_job_result_status(latest_outcome: Option<&str>, attempt_count: usize) -> (String, bool) {
    if attempt_count == 0 {
        return ("queued".to_string(), false);
    }

    match latest_outcome {
        Some("Succeeded") => ("succeeded".to_string(), true),
        Some("FatalFailure") => ("failed".to_string(), true),
        Some("RetryableFailure") => ("running".to_string(), false),
        _ => ("running".to_string(), false),
    }
}

pub fn enforce_lane_safety(
    lane: EngineExecutionLane,
    action: LaneSafetyActionClass,
    policy_profile: Option<&str>,
) -> Result<(), (StatusCode, String)> {
    if let Err(reason) = validate_lane_action(lane, action) {
        return Err((
            StatusCode::FORBIDDEN,
            format!("lane safety violation: {reason}"),
        ));
    }

    if let Err(reason) = validate_lane_policy_profile(lane, policy_profile) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("lane safety violation: {reason}"),
        ));
    }

    Ok(())
}
