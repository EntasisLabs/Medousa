use std::path::{Path, PathBuf};

use chrono::Utc;
use serde_json::Value;
use tokio::sync::mpsc;
use uuid::Uuid;

use medousa::{TuiRuntime, events::TuiEvent, process_once};
use stasis::domain::runtime::job_attempt::JobAttemptOutcome;
use stasis::ports::outbound::runtime::job_attempt_store::JobAttemptStore;
use stasis::prelude::{BackoffPolicy, NewJob, RuntimeComposition};

use super::TuiState;

pub(crate) fn save_editor_buffer(state: &mut TuiState, path_override: Option<&str>) {
    if let Some(path_raw) = path_override {
        if !path_raw.trim().is_empty() {
            state.editor_file_path = Some(PathBuf::from(path_raw.trim()));
        }
    }

    let Some(path) = state.editor_file_path.clone() else {
        state.editor_status = "Save failed: no path. Use /save <path>".to_string();
        super::push_obs(
            state,
            "⚠ save failed: no target path. use /save <path>".to_string(),
        );
        return;
    };

    match write_editor_file(&path, state.editor_buffer.as_text()) {
        Ok(_) => {
            state.editor_dirty = false;
            state.editor_status = format!("Saved {}", path.display());
            super::push_obs(state, format!("✓ saved {}", path.display()));
        }
        Err(err) => {
            state.editor_status = format!("Save failed: {err}");
            super::push_obs(state, format!("⚠ save failed: {err}"));
        }
    }
}

pub(crate) fn load_editor_file(path: &Path) -> std::io::Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(Some(content)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

pub(crate) fn write_editor_file(path: &Path, content: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}

pub(crate) fn resolve_editor_run_source(
    path_override: Option<&str>,
    editor_file_path: Option<&Path>,
    editor_text: &str,
) -> std::result::Result<(String, String), String> {
    let source_target = path_override
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);

    if let Some(path) = source_target {
        return match load_editor_file(&path) {
            Ok(Some(content)) => Ok((content, format!("file:{}", path.display()))),
            Ok(None) => Err(format!("run failed: file not found {}", path.display())),
            Err(err) => Err(format!("run failed: {err}")),
        };
    }

    let label = editor_file_path
        .map(|path| format!("editor:{}", path.display()))
        .unwrap_or_else(|| "editor:buffer".to_string());
    Ok((editor_text.to_string(), label))
}

pub(crate) fn validate_editor_run_allowlist(
    source: &str,
    allowed_modules_csv: &str,
) -> std::result::Result<Vec<String>, String> {
    let analysis = super::analyze_allowlist_preview(source, allowed_modules_csv);
    if !analysis.invalid_allowlist.is_empty() {
        return Err(format!(
            "run blocked: invalid allowlist entries: {}",
            analysis.invalid_allowlist.join(", ")
        ));
    }

    if !analysis.blocked_ops.is_empty() {
        return Err(format!(
            "run blocked by allowlist: {}",
            analysis.blocked_ops.join(", ")
        ));
    }

    Ok(analysis.referenced_ops)
}

pub(crate) async fn run_editor_source_via_runtime(
    state: &mut TuiState,
    tui_rt: &TuiRuntime,
    event_tx: &mpsc::Sender<TuiEvent>,
    path_override: Option<&str>,
) {
    let (source, source_label) = match resolve_editor_run_source(
        path_override,
        state.editor_file_path.as_deref(),
        state.editor_buffer.as_text(),
    ) {
        Ok(values) => values,
        Err(message) => {
            super::push_obs(state, format!("⚠ {message}"));
            return;
        }
    };

    if source.trim().is_empty() {
        super::push_obs(state, "⚠ run failed: source is empty".to_string());
        return;
    }

    let referenced_ops =
        match validate_editor_run_allowlist(&source, &state.settings.allowed_modules) {
            Ok(ops) => ops,
            Err(message) => {
                super::push_obs(state, format!("⚠ {message}"));
                return;
            }
        };

    let runtime = tui_rt.runtime.clone();
    let source_bytes = source.len();
    let allowed_modules = super::parse_allowed_modules(&state.settings.allowed_modules);
    let queued_label = source_label.clone();
    let tx = event_tx.clone();

    super::push_obs(state, format!("↻ run queued: {queued_label}"));

    tokio::spawn(async move {
        if let Err(message) = execute_editor_run_task(
            runtime,
            tx.clone(),
            source,
            source_label,
            source_bytes,
            referenced_ops,
            allowed_modules,
        )
        .await
        {
            let _ = tx.send(TuiEvent::UiNotice(format!("⚠ {message}"))).await;
        }
    });
}

async fn execute_editor_run_task(
    runtime: std::sync::Arc<RuntimeComposition>,
    event_tx: mpsc::Sender<TuiEvent>,
    source: String,
    source_label: String,
    source_bytes: usize,
    referenced_ops: Vec<String>,
    allowed_modules: Vec<String>,
) -> std::result::Result<(), String> {
    let _ = event_tx
        .send(TuiEvent::ToolInvoked {
            tool_name: "editor.gr.run".to_string(),
            input_summary: format!("{source_label}  {source_bytes} byte(s)"),
        })
        .await;

    let job_id = format!("editor-gr-run-{}", Uuid::new_v4().simple());
    let now = Utc::now();
    let job = NewJob {
        id: job_id.clone(),
        queue: "default".to_string(),
        job_type: "workflow.grapheme.run".to_string(),
        payload_ref: format!("grapheme:inline:{source}"),
        priority: 100,
        max_attempts: 1,
        idempotency_key: format!("idem-{job_id}"),
        correlation_id: job_id.clone(),
        causation_id: "medousa_tui.editor_run".to_string(),
        trace_id: job_id.clone(),
        sttp_input_node_id: "sttp:in:cognition:grapheme:editor-run".to_string(),
        scheduled_at: now,
        backoff_policy: BackoffPolicy::default(),
    };

    let enqueue_result = match &*runtime {
        RuntimeComposition::InMemory(rt) => rt.enqueue(job).await,
        RuntimeComposition::Surreal(rt) => rt.enqueue(job).await,
    };

    if let Err(err) = enqueue_result {
        return Err(format!("run enqueue failed: {err}"));
    }

    let _ = event_tx
        .send(TuiEvent::JobEnqueued {
            job_id: job_id.clone(),
            job_type: "workflow.grapheme.run".to_string(),
        })
        .await;

    if let Err(err) = process_once(&runtime, "medousa_tui.editor_run").await {
        return Err(format!("run processing failed: {err}"));
    }

    let attempts_result = match &*runtime {
        RuntimeComposition::InMemory(rt) => rt.job_attempt_store.list_by_job_id(&job_id).await,
        RuntimeComposition::Surreal(rt) => rt.job_attempt_store.list_by_job_id(&job_id).await,
    };

    let attempts = match attempts_result {
        Ok(list) => list,
        Err(err) => return Err(format!("run diagnostics failed: {err}")),
    };

    let Some(last_attempt) = attempts.last() else {
        return Err("run failed: no attempt recorded".to_string());
    };

    let succeeded = last_attempt.outcome == JobAttemptOutcome::Succeeded;
    let _ = event_tx
        .send(TuiEvent::JobProcessed {
            job_id: job_id.clone(),
            succeeded,
            execution_id: last_attempt.execution_id.clone(),
        })
        .await;

    let diagnostics_json = last_attempt
        .diagnostics
        .as_deref()
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .unwrap_or_else(|| {
            Value::String(
                last_attempt
                    .diagnostics
                    .clone()
                    .unwrap_or_else(|| "".to_string()),
            )
        });

    let output = serde_json::json!({
        "source": source_label,
        "job_id": job_id,
        "succeeded": succeeded,
        "attempt_outcome": format!("{:?}", last_attempt.outcome),
        "execution_id": last_attempt.execution_id,
        "diagnostics": diagnostics_json,
    });

    let tool_input = serde_json::json!({
        "source_bytes": source_bytes,
        "referenced_ops": referenced_ops,
        "allowed_modules": allowed_modules,
    });

    let _ = event_tx
        .send(TuiEvent::ToolPayload {
            tool_name: "editor.gr.run".to_string(),
            tool_input: tool_input.clone(),
            tool_output: output.clone(),
            input_receipt: medousa::payload_receipt::receipt_meta(
                &tool_input,
                medousa::payload_receipt::DEFAULT_MAX_INLINE_BYTES,
            ),
            output_receipt: medousa::payload_receipt::receipt_meta(
                &output,
                medousa::payload_receipt::DEFAULT_MAX_INLINE_BYTES,
            ),
        })
        .await;

    Ok(())
}
