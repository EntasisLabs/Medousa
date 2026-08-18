//! Durable Stasis jobs for background turn workers (`workflow.medousa.turn_worker`).

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use stasis::application::runtime::in_memory_runtime::{JobExecutionOutcome, JobHandler};
use stasis::domain::runtime::job::{Job, JobState};
use stasis::prelude::{Result as StasisResult, RuntimeComposition, StasisError};

use crate::agent_runtime::stream_sink::SharedAgentStreamSink;
use crate::agent_runtime::turn_worker::{
    TurnWorkRecord, TurnWorkStatus, WorkerRuntimeContext, resume_synthesis_if_needed,
    run_worker_turn, turn_worker_store,
};
use crate::session::{ConversationTurn, append_turn};
use crate::tools::TuiRuntime;
use crate::{runtime_composition_ext::RuntimeCompositionExt, runtime_job_spec::ToolJobSpec};

pub const TURN_WORKER_JOB_TYPE: &str = "workflow.medousa.turn_worker";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnWorkerJobPayload {
    pub work_id: String,
    pub stream_turn_id: u64,
}

impl TurnWorkerJobPayload {
    pub fn to_payload_ref(&self) -> StasisResult<String> {
        serde_json::to_string(self).map_err(|err| {
            StasisError::PortFailure(format!("failed to encode turn worker payload: {err}"))
        })
    }
}

pub async fn register_turn_worker_job_handler(
    composition: &RuntimeComposition,
    agent: Arc<TuiRuntime>,
) -> anyhow::Result<()> {
    let handler = TurnWorkerJobHandler { agent };
    match composition {
        RuntimeComposition::InMemory(rt) => rt.register_handler(handler)?,
        RuntimeComposition::Surreal(rt) => rt.register_handler(handler)?,
    }
    Ok(())
}

pub async fn enqueue_turn_worker_job(
    composition: &RuntimeComposition,
    work_id: &str,
    stream_turn_id: u64,
) -> StasisResult<()> {
    let payload = TurnWorkerJobPayload {
        work_id: work_id.to_string(),
        stream_turn_id,
    };
    let payload_ref = payload.to_payload_ref()?;
    let now = Utc::now();
    let job = ToolJobSpec::new(
        work_id,
        crate::daemon::worker_host::AGENT_QUEUE,
        TURN_WORKER_JOB_TYPE,
        payload_ref,
        "cognition_spawn_turn_worker",
        "sttp:in:medousa:turn_worker",
        now,
    )
    .max_attempts(3)
    .build();

    turn_worker_store().update(work_id, |record| {
        record.stasis_job_id = Some(work_id.to_string());
    });

    composition.enqueue_job(job).await?;
    Ok(())
}

pub async fn reconcile_durable_turn_workers(
    composition: &RuntimeComposition,
    agent: Arc<TuiRuntime>,
) {
    let store = turn_worker_store();
    let incomplete = store.list_incomplete();
    if incomplete.is_empty() {
        return;
    }

    eprintln!(
        "medousa-daemon: reconciling {} durable turn worker record(s)…",
        incomplete.len()
    );

    for record in incomplete {
        match record.status {
            TurnWorkStatus::Pending | TurnWorkStatus::Running => {
                if job_needs_enqueue(composition, &record.work_id).await {
                    eprintln!(
                        "medousa-daemon: re-enqueue turn worker work_id={} status={:?}",
                        record.work_id, record.status
                    );
                    if let Err(err) = enqueue_turn_worker_job(
                        composition,
                        &record.work_id,
                        record.parent_stream_turn_id,
                    )
                    .await
                    {
                        eprintln!(
                            "turn_worker reconcile: enqueue failed for {}: {err}",
                            record.work_id
                        );
                    }
                }
            }
            TurnWorkStatus::Completed if !record.synthesis_delivered => {
                eprintln!(
                    "medousa-daemon: resume synthesis for work_id={}",
                    record.work_id
                );
                resume_pending_synthesis(agent.clone(), record).await;
            }
            TurnWorkStatus::Failed | TurnWorkStatus::Cancelled
                if !record.synthesis_delivered
                    && record.disposition
                        == crate::agent_runtime::turn_worker::TurnWorkDisposition::Parallel =>
            {
                eprintln!(
                    "medousa-daemon: resume host intake for work_id={}",
                    record.work_id
                );
                resume_pending_synthesis(agent.clone(), record).await;
            }
            _ => {}
        }
    }
}

async fn job_needs_enqueue(composition: &RuntimeComposition, work_id: &str) -> bool {
    let job = composition.get_job(work_id).await;
    let Ok(job) = job else {
        return true;
    };
    match job {
        None => true,
        Some(job) => matches!(
            job.state,
            JobState::Succeeded | JobState::Failed | JobState::DeadLetter | JobState::Canceled
        ),
    }
}

pub async fn resume_pending_synthesis(agent: Arc<TuiRuntime>, record: TurnWorkRecord) {
    if record.synthesis_delivered {
        return;
    }
    let terminal_parallel = record.disposition
        == crate::agent_runtime::turn_worker::TurnWorkDisposition::Parallel
        && matches!(
            record.status,
            TurnWorkStatus::Completed | TurnWorkStatus::Failed | TurnWorkStatus::Cancelled
        );
    if record.status != TurnWorkStatus::Completed && !terminal_parallel {
        return;
    }
    let ctx = WorkerRuntimeContext::from_tui_runtime(agent.as_ref());
    let sink = durable_worker_sink(&record);
    resume_synthesis_if_needed(
        &ctx,
        &agent.execution_registry,
        record,
        sink,
        Some(agent.as_ref()),
    )
    .await;
}

struct TurnWorkerJobHandler {
    agent: Arc<TuiRuntime>,
}

#[async_trait]
impl JobHandler for TurnWorkerJobHandler {
    fn job_type(&self) -> &'static str {
        TURN_WORKER_JOB_TYPE
    }

    async fn execute(&self, job: &Job) -> StasisResult<JobExecutionOutcome> {
        let payload: TurnWorkerJobPayload =
            serde_json::from_str(&job.payload_ref).map_err(|err| {
                StasisError::PortFailure(format!(
                    "invalid turn worker payload for job {}: {err}",
                    job.id
                ))
            })?;

        let store = turn_worker_store();
        let Some(record) = store.get(&payload.work_id) else {
            return Ok(fatal_outcome(format!(
                "turn worker record missing for work_id={}",
                payload.work_id
            )));
        };

        if record.synthesis_delivered {
            return Ok(success_outcome(
                &payload.work_id,
                format!("work_id={} already completed", payload.work_id),
            ));
        }

        let ctx = WorkerRuntimeContext::from_tui_runtime(self.agent.as_ref());
        let sink = durable_worker_sink(&record);

        if matches!(
            record.status,
            TurnWorkStatus::Completed | TurnWorkStatus::Failed | TurnWorkStatus::Cancelled
        ) {
            resume_synthesis_if_needed(
                &ctx,
                &self.agent.execution_registry,
                record,
                sink,
                Some(self.agent.as_ref()),
            )
            .await;
            return Ok(success_outcome(
                &payload.work_id,
                format!("work_id={} synthesis resumed", payload.work_id),
            ));
        }

        eprintln!(
            "medousa turn_worker job_id={} work_id={} session_id={}",
            job.id, payload.work_id, record.session_id
        );

        run_worker_turn(
            store,
            ctx,
            payload.work_id.clone(),
            sink,
            payload.stream_turn_id,
            self.agent.clone(),
        )
        .await;

        let final_record = turn_worker_store().get(&payload.work_id);
        match final_record.as_ref().map(|record| record.status) {
            Some(TurnWorkStatus::Completed) => Ok(success_outcome(
                &payload.work_id,
                format!("work_id={} completed", payload.work_id),
            )),
            Some(TurnWorkStatus::Failed) => Ok(fatal_outcome(
                final_record
                    .and_then(|record| record.error)
                    .unwrap_or_else(|| "worker failed".to_string()),
            )),
            Some(TurnWorkStatus::Cancelled) => Ok(success_outcome(
                &payload.work_id,
                format!("work_id={} cancelled during run", payload.work_id),
            )),
            _ => Ok(fatal_outcome(format!(
                "work_id={} ended in unexpected state",
                payload.work_id
            ))),
        }
    }
}

/// Newest tool runs kept on a worker record.
const TOOL_ACTIVITY_CAP: usize = 64;
/// Characters of live thinking/output retained per worker.
const LIVE_TEXT_CAP: usize = 4_000;
/// Flush buffered chunks once this much text is pending.
const LIVE_FLUSH_CHARS: usize = 400;
/// …or once this long has passed, whichever comes first.
const LIVE_FLUSH_INTERVAL: Duration = Duration::from_millis(250);

/// Chunks buffered in memory between store writes.
///
/// Every `store.update` rewrites `turn_workers.json` and fires a projector
/// event, so writing per token would hammer the disk at streaming rate.
#[derive(Default)]
struct LiveTextPending {
    thinking: String,
    output: String,
    thinking_started_at: Option<DateTime<Utc>>,
    thinking_last_at: Option<DateTime<Utc>>,
    last_flush: Option<Instant>,
}

impl LiveTextPending {
    fn is_empty(&self) -> bool {
        self.thinking.is_empty() && self.output.is_empty()
    }

    fn due(&self) -> bool {
        if self.is_empty() {
            return false;
        }
        if self.thinking.chars().count() + self.output.chars().count() >= LIVE_FLUSH_CHARS {
            return true;
        }
        self.last_flush
            .is_none_or(|at| at.elapsed() >= LIVE_FLUSH_INTERVAL)
    }
}

struct DurableWorkerStreamSink {
    session_id: String,
    work_id: String,
    live: Mutex<LiveTextPending>,
}

impl DurableWorkerStreamSink {
    /// Buffer a chunk, writing through only when the flush budget is spent.
    fn buffer_live_text(&self, delta: &str, reasoning: bool) {
        let flush = {
            let Ok(mut pending) = self.live.lock() else {
                return;
            };
            if reasoning {
                pending.thinking.push_str(delta);
                let now = Utc::now();
                pending.thinking_started_at.get_or_insert(now);
                pending.thinking_last_at = Some(now);
            } else {
                pending.output.push_str(delta);
            }
            pending.due().then(|| Self::take(&mut pending))
        };
        if let Some(flush) = flush {
            self.write_live_text(flush);
        }
    }

    fn flush_live_text(&self) {
        let flush = {
            let Ok(mut pending) = self.live.lock() else {
                return;
            };
            if pending.is_empty() {
                return;
            }
            Self::take(&mut pending)
        };
        self.write_live_text(flush);
    }

    fn take(pending: &mut LiveTextPending) -> LiveTextFlush {
        pending.last_flush = Some(Instant::now());
        LiveTextFlush {
            thinking: std::mem::take(&mut pending.thinking),
            output: std::mem::take(&mut pending.output),
            thinking_started_at: pending.thinking_started_at,
            thinking_last_at: pending.thinking_last_at,
        }
    }

    fn write_live_text(&self, flush: LiveTextFlush) {
        turn_worker_store().update(&self.work_id, |record| {
            if !flush.thinking.is_empty() {
                append_capped(&mut record.live_thinking, &flush.thinking);
            }
            if !flush.output.is_empty() {
                append_capped(&mut record.live_output, &flush.output);
            }
            if record.thinking_started_at.is_none() {
                record.thinking_started_at = flush.thinking_started_at;
            }
            if flush.thinking_last_at.is_some() {
                record.thinking_finished_at = flush.thinking_last_at;
            }
        });
    }
}

struct LiveTextFlush {
    thinking: String,
    output: String,
    thinking_started_at: Option<DateTime<Utc>>,
    thinking_last_at: Option<DateTime<Utc>>,
}

fn durable_worker_sink(record: &TurnWorkRecord) -> SharedAgentStreamSink {
    Arc::new(DurableWorkerStreamSink {
        session_id: record.session_id.clone(),
        work_id: record.work_id.clone(),
        live: Mutex::new(LiveTextPending::default()),
    })
}

#[async_trait]
impl crate::agent_runtime::stream_sink::AgentStreamSink for DurableWorkerStreamSink {
    async fn content_chunk(&self, _turn_id: u64, delta: String) {
        if delta.is_empty() {
            return;
        }
        self.buffer_live_text(&delta, false);
    }

    async fn reasoning_chunk(&self, _turn_id: u64, delta: String) {
        if delta.is_empty() {
            return;
        }
        self.buffer_live_text(&delta, true);
    }

    async fn agent_response(&self, _turn_id: u64, text: String, tool_names: Vec<String>) {
        self.flush_live_text();
        let turn = ConversationTurn::plain(
            "assistant",
            text.clone(),
            Utc::now(),
            tool_names.clone(),
            None,
        );
        if let Err(error) = append_turn(&self.session_id, &turn).await {
            tracing::error!(session_id = %self.session_id, %error, "worker turn persistence failed");
            return;
        }

        if let Some(record) = turn_worker_store().get(&self.work_id)
            && let Err(err) = crate::turn_worker_notify::deliver_worker_result_to_ingest_channel(
                &record,
                &text,
                &tool_names,
            )
            .await
        {
            eprintln!(
                "turn worker channel synthesis delivery failed work_id={}: {err:#}",
                self.work_id
            );
        }
    }

    async fn agent_error(&self, _turn_id: u64, message: String) {
        self.flush_live_text();
        eprintln!(
            "turn_worker durable sink error session_id={}: {message}",
            self.session_id
        );
    }

    async fn notice(&self, message: String) {
        eprintln!("{message}");
    }

    /// Legacy entry point — only reached when a caller skips the structured
    /// `tool_run_started` path. Synthesizes a run id so the row still correlates.
    async fn tool_invoked(&self, tool_name: String, input_summary: String) {
        self.tool_run_started(
            crate::agent_runtime::tool_stream::new_tool_run_id(),
            tool_name,
            input_summary,
            Vec::new(),
            0,
        )
        .await;
    }

    async fn tool_run_started(
        &self,
        tool_run_id: String,
        tool_name: String,
        input_summary: String,
        input_params: Vec<medousa_types::daemon_api::ToolInputParam>,
        tool_round: usize,
    ) {
        // Land the reasoning that led here before the tool row, so the
        // transcript reads in the order it happened.
        self.flush_live_text();
        let store = turn_worker_store();
        store.update(&self.work_id, |record| {
            record
                .live_tool_activity
                .push(crate::agent_runtime::turn_worker::WorkerToolActivity {
                    run_id: tool_run_id.clone(),
                    name: tool_name.clone(),
                    round: tool_round,
                    status: "running".to_string(),
                    input_summary: (!input_summary.trim().is_empty())
                        .then(|| truncate_line(&input_summary, 160)),
                    input_params: input_params.clone(),
                    output_summary: None,
                    started_at: Utc::now(),
                    finished_at: None,
                });
            trim_tool_activity(&mut record.live_tool_activity);
        });
        if let Some(record) = store.get(&self.work_id) {
            crate::feed_adapters::publish_workshop_progress_activity(
                &record, &tool_name, "started", None,
            )
            .await;
        }
    }

    async fn tool_run_finished(
        &self,
        tool_run_id: String,
        tool_name: String,
        status: String,
        input_summary: String,
        output_summary: Option<String>,
        tool_input: serde_json::Value,
        tool_output: serde_json::Value,
        input_receipt: Option<crate::payload_receipt::ArtifactReceiptMeta>,
        output_receipt: Option<crate::payload_receipt::ArtifactReceiptMeta>,
        tool_round: usize,
    ) {
        let store = turn_worker_store();
        store.update(&self.work_id, |record| {
            let output_line = output_summary
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| truncate_line(value, 200));
            let existing = record
                .live_tool_activity
                .iter_mut()
                .find(|activity| activity.run_id == tool_run_id);
            match existing {
                Some(activity) => {
                    activity.status = status.clone();
                    activity.output_summary = output_line;
                    activity.finished_at = Some(Utc::now());
                }
                None => {
                    // Start row aged out of the ring (or never arrived) — keep the
                    // evidence rather than dropping the run entirely.
                    record.live_tool_activity.push(
                        crate::agent_runtime::turn_worker::WorkerToolActivity {
                            run_id: tool_run_id.clone(),
                            name: tool_name.clone(),
                            round: tool_round,
                            status: status.clone(),
                            input_summary: (!input_summary.trim().is_empty())
                                .then(|| truncate_line(&input_summary, 160)),
                            input_params: crate::agent_runtime::tool_stream::preview_tool_input(
                                &tool_input,
                            ),
                            output_summary: output_line,
                            started_at: Utc::now(),
                            finished_at: Some(Utc::now()),
                        },
                    );
                    trim_tool_activity(&mut record.live_tool_activity);
                }
            }
        });
        if let Some(record) = store.get(&self.work_id) {
            crate::feed_adapters::publish_workshop_progress_activity(
                &record,
                &tool_name,
                "finished",
                output_summary.as_deref(),
            )
            .await;
        }

        // Forward UI side-effects onto the parent interactive turn so Home can
        // paint scenes and artifacts authored in the Workshop.
        if let Some(record) = store.get(&self.work_id) {
            crate::turn_worker_notify::publish_worker_ui_side_effects_to_parent_turn(
                &record,
                &tool_name,
                &tool_output,
            )
            .await;
        }
        self.tool_payload(
            tool_name,
            tool_input,
            tool_output,
            input_receipt,
            output_receipt,
        )
        .await;
    }

    async fn tool_payload(
        &self,
        _tool_name: String,
        _tool_input: serde_json::Value,
        _tool_output: serde_json::Value,
        _input_receipt: Option<crate::payload_receipt::ArtifactReceiptMeta>,
        _output_receipt: Option<crate::payload_receipt::ArtifactReceiptMeta>,
    ) {
    }
}

/// Keep the newest `TOOL_ACTIVITY_CAP` runs; older evidence scrolls off.
fn trim_tool_activity(activity: &mut Vec<crate::agent_runtime::turn_worker::WorkerToolActivity>) {
    if activity.len() > TOOL_ACTIVITY_CAP {
        let drop = activity.len() - TOOL_ACTIVITY_CAP;
        activity.drain(0..drop);
    }
}

/// Append to a live transcript tail, keeping only the last `LIVE_TEXT_CAP` chars.
fn append_capped(target: &mut String, delta: &str) {
    target.push_str(delta);
    let len = target.chars().count();
    if len > LIVE_TEXT_CAP {
        *target = target.chars().skip(len - LIVE_TEXT_CAP).collect();
    }
}

fn truncate_line(value: &str, max: usize) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= max {
        return trimmed.to_string();
    }
    trimmed.chars().take(max).collect::<String>() + "…"
}

fn success_outcome(work_id: &str, summary: String) -> JobExecutionOutcome {
    JobExecutionOutcome::Success {
        sttp_output_node_id: format!("sttp:out:turn-worker:{work_id}"),
        execution_id: None,
        diagnostics: Some(summary),
    }
}

fn fatal_outcome(message: String) -> JobExecutionOutcome {
    JobExecutionOutcome::FatalFailure {
        message,
        execution_id: None,
        diagnostics: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_runtime::turn_worker::WorkerToolActivity;

    fn activity(run_id: &str) -> WorkerToolActivity {
        WorkerToolActivity {
            run_id: run_id.to_string(),
            name: "web_search".to_string(),
            round: 1,
            status: "running".to_string(),
            input_summary: None,
            input_params: Vec::new(),
            output_summary: None,
            started_at: Utc::now(),
            finished_at: None,
        }
    }

    /// The ring drops the oldest runs, so `run_id` correlation must survive on
    /// whatever is still in the window.
    #[test]
    fn trim_tool_activity_keeps_newest_runs() {
        let mut runs: Vec<WorkerToolActivity> = (0..(TOOL_ACTIVITY_CAP + 5))
            .map(|index| activity(&format!("run-{index}")))
            .collect();
        trim_tool_activity(&mut runs);
        assert_eq!(runs.len(), TOOL_ACTIVITY_CAP);
        assert_eq!(runs[0].run_id, "run-5");
        assert_eq!(
            runs.last().expect("last").run_id,
            format!("run-{}", TOOL_ACTIVITY_CAP + 4)
        );
    }

    #[test]
    fn append_capped_keeps_the_tail_not_the_head() {
        let mut text = String::new();
        append_capped(&mut text, &"a".repeat(LIVE_TEXT_CAP));
        append_capped(&mut text, "TAIL");
        assert_eq!(text.chars().count(), LIVE_TEXT_CAP);
        assert!(text.ends_with("TAIL"));
        assert!(!text.starts_with("TAIL"));
    }

    /// Multi-byte input must not panic or split a character mid-way.
    #[test]
    fn append_capped_is_char_safe() {
        let mut text = String::new();
        append_capped(&mut text, &"é".repeat(LIVE_TEXT_CAP + 10));
        assert_eq!(text.chars().count(), LIVE_TEXT_CAP);
        assert!(text.chars().all(|ch| ch == 'é'));
    }
}
