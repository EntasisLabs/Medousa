//! HTTP handlers for recurring schedule APIs (`/v1/recurring`).

use std::collections::HashSet;
use std::sync::{LazyLock, RwLock};

use chrono::Utc;
use stasis::application::runtime::runtime_factory::RuntimeComposition;
use stasis::domain::errors::StasisError;
use stasis::domain::runtime::job::{Job, JobState};
use stasis::domain::runtime::recurring::RecurringDefinition;
use stasis::ports::outbound::runtime::job_attempt_store::JobAttemptStore;
use stasis::ports::outbound::runtime::job_store::JobStore;
use stasis::ports::outbound::runtime::recurring_store::RecurringStore;

use crate::channel_delivery;
use crate::daemon_api::{
    DeleteRecurringResponse, RecurringDefinitionEntry, RecurringDeliveryResponse,
    RecurringListQuery, RecurringListResponse, RecurringRunEntry, RecurringRunsQuery,
    RecurringRunsResponse, UpdateRecurringRequest, UpdateRecurringResponse,
};
use crate::recurring_agent_turn;
use crate::recurring_delivery;
use crate::recurring_feed;
use crate::turn_continuation::StoredDeliveryTarget;

static IN_MEMORY_DELETED_RECURRING: LazyLock<RwLock<HashSet<String>>> =
    LazyLock::new(|| RwLock::new(HashSet::new()));

async fn list_recurring_definitions(
    runtime: &RuntimeComposition,
) -> stasis::prelude::Result<Vec<RecurringDefinition>> {
    match runtime {
        RuntimeComposition::InMemory(rt) => rt.recurring_store.list().await,
        RuntimeComposition::Surreal(rt) => rt.recurring_store.list().await,
    }
}

async fn save_recurring_definition(
    runtime: &RuntimeComposition,
    definition: RecurringDefinition,
) -> stasis::prelude::Result<()> {
    match runtime {
        RuntimeComposition::InMemory(rt) => rt.recurring_store.save(definition).await,
        RuntimeComposition::Surreal(rt) => rt.recurring_store.save(definition).await,
    }
}

async fn list_jobs_by_state(
    runtime: &RuntimeComposition,
    state: JobState,
) -> stasis::prelude::Result<Vec<Job>> {
    match runtime {
        RuntimeComposition::InMemory(rt) => rt.job_store.list_by_state(state).await,
        RuntimeComposition::Surreal(rt) => rt.job_store.list_by_state(state).await,
    }
}

async fn list_job_attempts(
    runtime: &RuntimeComposition,
    job_id: &str,
) -> stasis::prelude::Result<Vec<stasis::domain::runtime::job_attempt::JobAttempt>> {
    match runtime {
        RuntimeComposition::InMemory(rt) => rt.job_attempt_store.list_by_job_id(job_id).await,
        RuntimeComposition::Surreal(rt) => rt.job_attempt_store.list_by_job_id(job_id).await,
    }
}

fn tombstoned_recurring_ids() -> HashSet<String> {
    IN_MEMORY_DELETED_RECURRING
        .read()
        .map(|guard| guard.clone())
        .unwrap_or_default()
}

fn tombstone_recurring_id(recurring_id: &str) {
    if let Ok(mut guard) = IN_MEMORY_DELETED_RECURRING.write() {
        guard.insert(recurring_id.to_string());
    }
}

fn clear_tombstone_recurring_id(recurring_id: &str) {
    if let Ok(mut guard) = IN_MEMORY_DELETED_RECURRING.write() {
        guard.remove(recurring_id);
    }
}

fn truncate_line(value: &str, max: usize) -> String {
    let line = value.split('\n').next().unwrap_or(value).trim();
    if line.chars().count() <= max {
        line.to_string()
    } else {
        format!("{}…", line.chars().take(max.saturating_sub(1)).collect::<String>())
    }
}

fn prompt_excerpt_from_recurring(definition: &RecurringDefinition) -> Option<String> {
    let payload = serde_json::from_str::<serde_json::Value>(&definition.payload_template_ref).ok()?;
    let prompt = payload.get("user_prompt")?.as_str()?.trim();
    if prompt.is_empty() {
        return None;
    }
    Some(truncate_line(prompt, 120))
}

pub fn display_name_from_payload(payload_ref: &str) -> Option<String> {
    let payload = serde_json::from_str::<serde_json::Value>(payload_ref).ok()?;
    payload
        .get("display_name")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub fn inject_display_name_into_payload(payload_ref: &str, display_name: Option<&str>) -> String {
    let Some(display_name) = display_name.map(str::trim).filter(|value| !value.is_empty()) else {
        return payload_ref.to_string();
    };
    let Ok(mut payload) = serde_json::from_str::<serde_json::Value>(payload_ref) else {
        return payload_ref.to_string();
    };
    if let Some(object) = payload.as_object_mut() {
        object.insert(
            "display_name".to_string(),
            serde_json::Value::String(display_name.to_string()),
        );
    }
    payload.to_string()
}

pub fn execution_mode_from_definition(definition: &RecurringDefinition) -> String {
    if definition.job_type == recurring_agent_turn::RECURRING_AGENT_TURN_JOB_TYPE {
        "agent_turn".to_string()
    } else {
        "prompt".to_string()
    }
}

pub fn delivery_label_for_target(target: Option<&StoredDeliveryTarget>) -> String {
    let Some(target) = target else {
        return "In Medousa".to_string();
    };
    let channel = target.channel.trim();
    let short_id = target
        .channel_id
        .split(':')
        .next_back()
        .unwrap_or(target.channel_id.as_str());
    match channel {
        "telegram" => format!("Telegram · {short_id}"),
        "discord" => format!("Discord · {short_id}"),
        "slack" => format!("Slack · {short_id}"),
        "whatsapp" => format!("WhatsApp · {short_id}"),
        "cli" => "In Medousa".to_string(),
        other if other.is_empty() => "In Medousa".to_string(),
        other => format!("{other} · {short_id}"),
    }
}

fn derive_run_status(latest_outcome: Option<&str>, attempt_count: usize) -> (String, bool) {
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

async fn definition_to_entry(
    runtime: &RuntimeComposition,
    definition: RecurringDefinition,
) -> RecurringDefinitionEntry {
    let display_name = display_name_from_payload(&definition.payload_template_ref);
    let delivery = recurring_delivery::delivery_binding_for_recurring(&definition.id).await;
    let delivery_label = delivery_label_for_target(delivery.as_ref());
    let last_run_status = list_recurring_runs(
        runtime,
        &definition.id,
        RecurringRunsQuery { limit: Some(1) },
    )
        .await
        .ok()
        .and_then(|response| response.runs.first().map(|run| run.status.clone()));

    RecurringDefinitionEntry {
        recurring_id: definition.id.clone(),
        queue: definition.queue.clone(),
        job_type: definition.job_type.clone(),
        cron_expr: definition.cron_expr.clone(),
        timezone: definition.timezone.clone(),
        enabled: definition.enabled,
        next_run_at_utc: definition.next_run_at,
        last_run_at_utc: definition.last_run_at,
        manuscript_id: recurring_agent_turn::manuscript_id_from_recurring_payload(
            &definition.job_type,
            &definition.payload_template_ref,
        ),
        prompt_excerpt: prompt_excerpt_from_recurring(&definition),
        display_name,
        execution_mode: Some(execution_mode_from_definition(&definition)),
        delivery_label: Some(delivery_label),
        last_run_status,
    }
}

pub async fn list_recurring(
    runtime: &RuntimeComposition,
    query: RecurringListQuery,
) -> stasis::prelude::Result<RecurringListResponse> {
    let enabled_only = query.enabled_only.unwrap_or(false);
    let tombstones = tombstoned_recurring_ids();
    let mut definitions = list_recurring_definitions(runtime).await?;
    definitions.retain(|definition| !tombstones.contains(&definition.id));
    if enabled_only {
        definitions.retain(|definition| definition.enabled);
    }
    definitions.sort_by(|left, right| {
        left.next_run_at
            .cmp(&right.next_run_at)
            .then_with(|| left.id.cmp(&right.id))
    });

    let mut recurring = Vec::with_capacity(definitions.len());
    for definition in definitions {
        recurring.push(definition_to_entry(runtime, definition).await);
    }

    Ok(RecurringListResponse {
        count: recurring.len(),
        recurring,
    })
}

pub async fn get_recurring_definition(
    runtime: &RuntimeComposition,
    recurring_id: &str,
) -> stasis::prelude::Result<Option<RecurringDefinition>> {
    if tombstoned_recurring_ids().contains(recurring_id) {
        return Ok(None);
    }
    let definitions = list_recurring_definitions(runtime).await?;
    Ok(definitions
        .into_iter()
        .find(|definition| definition.id == recurring_id))
}

pub async fn list_recurring_runs(
    runtime: &RuntimeComposition,
    recurring_id: &str,
    query: RecurringRunsQuery,
) -> stasis::prelude::Result<RecurringRunsResponse> {
    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    if get_recurring_definition(runtime, recurring_id).await?.is_none() {
        return Err(StasisError::PortFailure(format!(
            "recurring_id={recurring_id} not found"
        )));
    }

    let states = [
        JobState::Succeeded,
        JobState::Failed,
        JobState::DeadLetter,
        JobState::Running,
        JobState::Leased,
        JobState::Enqueued,
        JobState::Canceled,
    ];
    let mut jobs = Vec::new();
    for state in states {
        let mut batch = list_jobs_by_state(runtime, state).await?;
        batch.retain(|job| job.correlation_id == recurring_id);
        jobs.append(&mut batch);
    }

    jobs.sort_by(|left, right| {
        let left_at = left
            .finished_at
            .or(left.started_at)
            .unwrap_or(left.scheduled_at);
        let right_at = right
            .finished_at
            .or(right.started_at)
            .unwrap_or(right.scheduled_at);
        right_at.cmp(&left_at)
    });
    jobs.truncate(limit);

    let mut runs = Vec::with_capacity(jobs.len());
    for job in jobs {
        let attempts = list_job_attempts(runtime, &job.id).await.unwrap_or_default();
        let latest = attempts.last();
        let latest_outcome = latest.map(|attempt| format!("{:?}", attempt.outcome));
        let output_text = latest.and_then(|attempt| {
            channel_delivery::extract_output_text_from_diagnostics(attempt.diagnostics.as_deref())
        });
        let (status, is_terminal) = derive_run_status(latest_outcome.as_deref(), attempts.len());
        runs.push(RecurringRunEntry {
            job_id: job.id,
            status,
            is_terminal,
            attempt_count: attempts.len(),
            latest_outcome,
            output_text,
            scheduled_at_utc: job.scheduled_at,
            updated_at_utc: job
                .finished_at
                .or(job.started_at)
                .unwrap_or(job.scheduled_at),
        });
    }

    Ok(RecurringRunsResponse {
        recurring_id: recurring_id.to_string(),
        count: runs.len(),
        runs,
    })
}

pub async fn get_recurring_delivery(
    recurring_id: &str,
) -> stasis::prelude::Result<RecurringDeliveryResponse> {
    let binding = recurring_delivery::delivery_binding_for_recurring(recurring_id).await;
    Ok(RecurringDeliveryResponse {
        recurring_id: recurring_id.to_string(),
        delivery_label: delivery_label_for_target(binding.as_ref()),
        delivery: binding
            .as_ref()
            .map(recurring_delivery::delivery_binding_to_json),
    })
}

pub async fn update_recurring(
    runtime: &RuntimeComposition,
    recurring_id: &str,
    request: UpdateRecurringRequest,
) -> stasis::prelude::Result<UpdateRecurringResponse> {
    let Some(mut definition) = get_recurring_definition(runtime, recurring_id).await? else {
        return Err(StasisError::PortFailure(format!(
            "recurring_id={recurring_id} not found"
        )));
    };

    if let Some(enabled) = request.enabled {
        definition.enabled = enabled;
    }

    if let Some(cron_expr) = request
        .cron_expr
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        definition.cron_expr = cron_expr.to_string();
    }

    if let Some(timezone) = request
        .timezone
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        definition.timezone = timezone.to_string();
    }

    if request.display_name.is_some() {
        definition.payload_template_ref = inject_display_name_into_payload(
            &definition.payload_template_ref,
            request.display_name.as_deref(),
        );
    }

    if request.cron_expr.is_some() || request.timezone.is_some() {
        recurring_delivery::validate_recurring_cron(&definition.cron_expr, &definition.timezone)?;
        let now = Utc::now();
        definition.next_run_at = definition.compute_next_run_at(now)?;
    }

    save_recurring_definition(runtime, definition.clone()).await?;

    if let Some(delivery) = request.delivery {
        if delivery.is_null() {
            let _ = recurring_delivery::remove_recurring_delivery_binding(recurring_id).await;
        } else {
            let fallback_session_id = format!("recurring-{recurring_id}");
            recurring_delivery::persist_recurring_delivery_binding(
                recurring_id,
                &serde_json::json!({ "delivery": delivery }),
                recurring_delivery::DeliveryResolveContext {
                    ambient: None,
                    fallback_session_id,
                },
            )
            .await?;
        }
    }

    if let Some(feeds) = request.feeds {
        if feeds.is_null() {
            let _ = recurring_feed::remove_recurring_feed_binding(recurring_id).await;
        } else {
            recurring_feed::persist_recurring_feed_binding(
                recurring_id,
                &serde_json::json!({ "feeds": feeds }),
            )
            .await?;
        }
    }

    Ok(UpdateRecurringResponse {
        recurring_id: definition.id,
        enabled: definition.enabled,
        cron_expr: definition.cron_expr,
        timezone: definition.timezone,
        next_run_at_utc: definition.next_run_at,
    })
}

pub async fn delete_recurring(
    runtime: &RuntimeComposition,
    recurring_id: &str,
) -> stasis::prelude::Result<DeleteRecurringResponse> {
    let exists = get_recurring_definition(runtime, recurring_id).await?.is_some();
    if !exists {
        return Err(StasisError::PortFailure(format!(
            "recurring_id={recurring_id} not found"
        )));
    }

    match runtime {
        RuntimeComposition::Surreal(rt) => {
            let record_id = recurring_id.to_string();
            rt.job_store
                .db()
                .query("DELETE type::record($table, $id)")
                .bind(("table", "recurring_definition"))
                .bind(("id", record_id))
                .await
                .map_err(|err| {
                    StasisError::PortFailure(format!("delete recurring definition: {err}"))
                })?;
            clear_tombstone_recurring_id(recurring_id);
        }
        RuntimeComposition::InMemory(_) => {
            tombstone_recurring_id(recurring_id);
        }
    }

    let _ = recurring_delivery::remove_recurring_delivery_binding(recurring_id).await;
    let _ = recurring_feed::remove_recurring_feed_binding(recurring_id).await;

    Ok(DeleteRecurringResponse {
        recurring_id: recurring_id.to_string(),
        deleted: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injects_display_name_into_payload_json() {
        let payload = r#"{"user_prompt":"Run daily brief","session_id":"recurring-x"}"#;
        let encoded = inject_display_name_into_payload(payload, Some("Morning brief"));
        let value: serde_json::Value = serde_json::from_str(&encoded).expect("json");
        assert_eq!(value["display_name"], "Morning brief");
    }

    #[test]
    fn delivery_label_defaults_to_in_medousa() {
        assert_eq!(delivery_label_for_target(None), "In Medousa");
    }
}
