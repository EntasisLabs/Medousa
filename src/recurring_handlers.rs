//! HTTP handlers for recurring schedule APIs (`/v1/recurring`).

use std::collections::HashSet;
use std::sync::{LazyLock, RwLock};

use chrono::Utc;
use stasis::application::runtime::runtime_factory::RuntimeComposition;
use stasis::domain::errors::StasisError;
use stasis::domain::runtime::recurring::RecurringDefinition;
use stasis::ports::outbound::runtime::job_store::JobStore;
use stasis::ports::outbound::runtime::recurring_store::RecurringStore;

use crate::daemon_api::{
    DeleteRecurringResponse, RecurringDefinitionEntry, RecurringListQuery, RecurringListResponse,
    UpdateRecurringRequest, UpdateRecurringResponse,
};
use crate::recurring_agent_turn;

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

fn prompt_excerpt_from_recurring(definition: &RecurringDefinition) -> Option<String> {
    let payload = serde_json::from_str::<serde_json::Value>(&definition.payload_template_ref).ok()?;
    let prompt = payload.get("user_prompt")?.as_str()?.trim();
    if prompt.is_empty() {
        return None;
    }
    Some(truncate_line(prompt, 120))
}

fn truncate_line(value: &str, max: usize) -> String {
    let line = value.split('\n').next().unwrap_or(value).trim();
    if line.chars().count() <= max {
        line.to_string()
    } else {
        format!("{}…", line.chars().take(max.saturating_sub(1)).collect::<String>())
    }
}

fn definition_to_entry(definition: RecurringDefinition) -> RecurringDefinitionEntry {
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

    let recurring = definitions
        .into_iter()
        .map(definition_to_entry)
        .collect::<Vec<_>>();

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

    if request.cron_expr.is_some() || request.timezone.is_some() {
        crate::recurring_delivery::validate_recurring_cron(
            &definition.cron_expr,
            &definition.timezone,
        )?;
        let now = Utc::now();
        definition.next_run_at = definition.compute_next_run_at(now)?;
    }

    save_recurring_definition(runtime, definition.clone()).await?;

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

    let _ = crate::recurring_delivery::remove_recurring_delivery_binding(recurring_id).await;

    Ok(DeleteRecurringResponse {
        recurring_id: recurring_id.to_string(),
        deleted: true,
    })
}
