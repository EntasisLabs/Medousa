//! Durable Stasis jobs for background turn workers (`workflow.medousa.turn_worker`).

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use stasis::application::runtime::in_memory_runtime::{JobExecutionOutcome, JobHandler};
use stasis::domain::runtime::job::{BackoffPolicy, Job, JobState, NewJob};
use stasis::ports::outbound::runtime::job_store::JobStore;
use stasis::prelude::{Result as StasisResult, RuntimeComposition, StasisError};

use crate::agent_runtime::stream_sink::SharedAgentStreamSink;
use crate::agent_runtime::turn_worker::{
    TurnWorkRecord, TurnWorkStatus, WorkerRuntimeContext, resume_synthesis_if_needed,
    run_worker_turn, turn_worker_store,
};
use crate::session::{ConversationTurn, append_turn};
use crate::tools::TuiRuntime;

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
    let job = NewJob {
        id: work_id.to_string(),
        queue: "default".to_string(),
        job_type: TURN_WORKER_JOB_TYPE.to_string(),
        payload_ref,
        priority: 100,
        max_attempts: 3,
        idempotency_key: format!("idem-{work_id}"),
        correlation_id: work_id.to_string(),
        causation_id: "cognition_spawn_turn_worker".to_string(),
        trace_id: work_id.to_string(),
        sttp_input_node_id: "sttp:in:medousa:turn_worker".to_string(),
        scheduled_at: now,
        backoff_policy: BackoffPolicy::default(),
    };

    turn_worker_store().update(work_id, |record| {
        record.stasis_job_id = Some(work_id.to_string());
    });

    match composition {
        RuntimeComposition::InMemory(rt) => rt.enqueue(job).await?,
        RuntimeComposition::Surreal(rt) => rt.enqueue(job).await?,
    }
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
            _ => {}
        }
    }
}

async fn job_needs_enqueue(composition: &RuntimeComposition, work_id: &str) -> bool {
    let job = match composition {
        RuntimeComposition::InMemory(rt) => rt.job_store.get(work_id).await,
        RuntimeComposition::Surreal(rt) => rt.job_store.get(work_id).await,
    };
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
    if record.synthesis_delivered || record.status != TurnWorkStatus::Completed {
        return;
    }
    let ctx = WorkerRuntimeContext::from_tui_runtime(agent.as_ref());
    let sink = durable_worker_sink(&record);
    resume_synthesis_if_needed(&ctx, record, sink).await;
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

        if record.status == TurnWorkStatus::Cancelled {
            return Ok(success_outcome(
                &payload.work_id,
                format!("work_id={} cancelled before execution", payload.work_id),
            ));
        }

        if record.status == TurnWorkStatus::Completed && record.synthesis_delivered {
            return Ok(success_outcome(
                &payload.work_id,
                format!("work_id={} already completed", payload.work_id),
            ));
        }

        let ctx = WorkerRuntimeContext::from_tui_runtime(self.agent.as_ref());
        let sink = durable_worker_sink(&record);

        if record.status == TurnWorkStatus::Completed && !record.synthesis_delivered {
            resume_synthesis_if_needed(&ctx, record, sink).await;
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

struct DurableWorkerStreamSink {
    session_id: String,
    work_id: String,
}

fn durable_worker_sink(record: &TurnWorkRecord) -> SharedAgentStreamSink {
    Arc::new(DurableWorkerStreamSink {
        session_id: record.session_id.clone(),
        work_id: record.work_id.clone(),
    })
}

#[async_trait]
impl crate::agent_runtime::stream_sink::AgentStreamSink for DurableWorkerStreamSink {
    async fn content_chunk(&self, _turn_id: u64, _delta: String) {}

    async fn reasoning_chunk(&self, _turn_id: u64, _delta: String) {}

    async fn agent_response(&self, _turn_id: u64, text: String, tool_names: Vec<String>) {
        let turn = ConversationTurn::plain(
            "assistant",
            text.clone(),
            Utc::now(),
            tool_names.clone(),
            None,
        );
        append_turn(&self.session_id, &turn);

        if let Some(record) = turn_worker_store().get(&self.work_id) {
            if let Err(err) = crate::turn_worker_notify::deliver_worker_result_to_ingest_channel(
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
    }

    async fn agent_error(&self, _turn_id: u64, message: String) {
        eprintln!(
            "turn_worker durable sink error session_id={}: {message}",
            self.session_id
        );
    }

    async fn notice(&self, message: String) {
        eprintln!("{message}");
    }

    async fn tool_invoked(&self, _tool_name: String, _input_summary: String) {}

    async fn tool_run_finished(
        &self,
        _tool_run_id: String,
        tool_name: String,
        _status: String,
        _input_summary: String,
        _output_summary: Option<String>,
        tool_input: serde_json::Value,
        tool_output: serde_json::Value,
        input_receipt: Option<crate::payload_receipt::ArtifactReceiptMeta>,
        output_receipt: Option<crate::payload_receipt::ArtifactReceiptMeta>,
        _tool_round: usize,
    ) {
        // Default trait path only calls tool_payload (a no-op here). Forward UI
        // side-effects onto the parent interactive turn so Home can paint scenes
        // and artifacts authored in the Workshop.
        if let Some(record) = turn_worker_store().get(&self.work_id) {
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
