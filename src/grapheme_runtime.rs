//! Shared Stasis-backed execution path for portable Grapheme scripts.

use std::sync::Arc;

use chrono::Utc;
use serde_json::{Value, json};
use stasis::domain::errors::StasisError;
use stasis::domain::runtime::job_attempt::JobAttemptOutcome;
use stasis::prelude::RuntimeComposition;
use uuid::Uuid;

use crate::runtime_composition_ext::{RuntimeCompositionExt, process_once};
use crate::runtime_job_spec::ToolJobSpec;

pub async fn run_grapheme_via_runtime(
    runtime: &Arc<RuntimeComposition>,
    source: &str,
    causation: &str,
) -> stasis::prelude::Result<Value> {
    if crate::grapheme_grants::source_contains_secret_grant(source) {
        return Err(StasisError::PortFailure(
            "ephemeral Grapheme grants require grapheme.invoke with matching secret_grant_ids"
                .to_string(),
        ));
    }
    let job_id = format!("cognition-gph-runtime-{}", Uuid::new_v4().simple());
    let job = ToolJobSpec::new(
        job_id.clone(),
        "default",
        "workflow.grapheme.run",
        format!("grapheme:inline:{source}"),
        causation,
        Utc::now(),
    )
    .build();

    runtime.enqueue_job(job).await?;
    let _ = process_once(runtime, causation).await.map_err(|error| {
        StasisError::PortFailure(format!("runtime process_once failed: {error}"))
    })?;

    let attempts = runtime.as_ref().list_job_attempts(&job_id).await?;
    let last = attempts.last().ok_or_else(|| {
        StasisError::PortFailure(
            "runtime preflight did not produce a job attempt for grapheme source".to_string(),
        )
    })?;
    let succeeded = last.outcome == JobAttemptOutcome::Succeeded;
    let diagnostics = last
        .diagnostics
        .as_deref()
        .and_then(|diagnostics| serde_json::from_str::<Value>(diagnostics).ok())
        .unwrap_or_else(|| json!({ "raw": last.diagnostics.clone().unwrap_or_default() }));

    Ok(json!({
        "mode": "runtime",
        "job_id": job_id,
        "succeeded": succeeded,
        "attempt_outcome": format!("{:?}", last.outcome),
        "execution_id": last.execution_id,
        "diagnostics": diagnostics
    }))
}

pub async fn validate_grapheme_source_for_schedule(
    runtime: &Arc<RuntimeComposition>,
    source: &str,
) -> stasis::prelude::Result<Value> {
    let result = run_grapheme_via_runtime(runtime, source, "cognition_tui_preflight").await?;
    let succeeded = result
        .get("succeeded")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let diagnostics_value = result
        .get("diagnostics")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let diagnostics_preview = truncate_for_error(
        &serde_json::to_string_pretty(&diagnostics_value).unwrap_or_else(|_| "{}".to_string()),
        1_600,
    );

    Ok(json!({
        "validated": succeeded,
        "mode": "runtime_preflight",
        "job_id": result.get("job_id").cloned().unwrap_or(Value::Null),
        "execution_id": result.get("execution_id").cloned().unwrap_or(Value::Null),
        "attempt_outcome": result.get("attempt_outcome").cloned().unwrap_or(Value::Null),
        "diagnostics": diagnostics_value,
        "diagnostics_preview": diagnostics_preview
    }))
}

fn truncate_for_error(text: &str, max_chars: usize) -> String {
    let out: String = text.chars().take(max_chars).collect();
    if text.chars().count() > max_chars {
        format!("{out}...")
    } else {
        out
    }
}
