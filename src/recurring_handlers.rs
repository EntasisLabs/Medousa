//! HTTP handlers for recurring schedule APIs (`/v1/recurring`).

use stasis::application::runtime::runtime_factory::RuntimeComposition;
use stasis::domain::runtime::recurring::RecurringDefinition;
use stasis::ports::outbound::runtime::recurring_store::RecurringStore;

use crate::daemon_api::{
    RecurringDefinitionEntry, RecurringListQuery, RecurringListResponse,
};
use crate::recurring_agent_turn;

async fn list_recurring_definitions(
    runtime: &RuntimeComposition,
) -> stasis::prelude::Result<Vec<RecurringDefinition>> {
    match runtime {
        RuntimeComposition::InMemory(rt) => rt.recurring_store.list().await,
        RuntimeComposition::Surreal(rt) => rt.recurring_store.list().await,
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

pub async fn list_recurring(
    runtime: &RuntimeComposition,
    query: RecurringListQuery,
) -> stasis::prelude::Result<RecurringListResponse> {
    let enabled_only = query.enabled_only.unwrap_or(false);
    let mut definitions = list_recurring_definitions(runtime).await?;
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
        .map(|definition| RecurringDefinitionEntry {
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
        })
        .collect::<Vec<_>>();

    Ok(RecurringListResponse {
        count: recurring.len(),
        recurring,
    })
}
