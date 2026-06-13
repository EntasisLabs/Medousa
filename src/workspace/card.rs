//! Thin work card projections from Stasis jobs and turn-worker records.

use chrono::{DateTime, Duration, Utc};
use serde_json::Value;
use stasis::application::runtime::runtime_factory::RuntimeComposition;
use stasis::domain::runtime::job::{Job, JobState};
use stasis::ports::outbound::runtime::job_store::JobStore;

use crate::agent_runtime::turn_worker::{TurnWorkRecord, TurnWorkStatus, turn_worker_store};
use crate::daemon_api::{
    WorkBoardColumn, WorkCard, WorkCardDetail, WorkCardId, WorkCardKind,
};
use crate::workspace::ask_job_store::{AskJobRecord, AskJobStatus, ask_job_store};
use crate::workspace::retention::{self, WorkspaceRetentionConfig};
use crate::turn_budget_request::{turn_budget_request_store, TurnBudgetRequest, TurnBudgetRequestStatus};
use crate::openshell_sandbox_run::OPENSHELL_SANDBOX_RUN_JOB_TYPE;
use crate::workspace::store::workspace_store;

const WRAPPING_UP_STALE: Duration = Duration::hours(2);

pub struct ProjectedWorkItem {
    pub card: WorkCard,
    pub detail: WorkCardDetail,
}

pub async fn list_jobs_by_states(
    runtime: &RuntimeComposition,
    states: &[JobState],
) -> anyhow::Result<Vec<Job>> {
    let mut jobs = Vec::new();
    for state in states {
        let mut batch = match runtime {
            RuntimeComposition::InMemory(rt) => rt.job_store.list_by_state(state.clone()).await?,
            RuntimeComposition::Surreal(rt) => rt.job_store.list_by_state(state.clone()).await?,
        };
        jobs.append(&mut batch);
    }
    Ok(jobs)
}

pub async fn project_workspace_items(
    runtime: &RuntimeComposition,
    include_terminal: bool,
) -> anyhow::Result<Vec<ProjectedWorkItem>> {
    let states = [
        JobState::Enqueued,
        JobState::Leased,
        JobState::Running,
        JobState::Succeeded,
        JobState::Failed,
        JobState::DeadLetter,
        JobState::Canceled,
    ];
    let jobs = list_jobs_by_states(runtime, &states).await?;
    let workers = turn_worker_store().list_all(200);
    let worker_ids = workers
        .iter()
        .map(|record| record.work_id.clone())
        .collect::<std::collections::HashSet<_>>();
    let ask_parent_ids = ask_job_store()
        .list_for_workspace(true)
        .into_iter()
        .map(|record| record.job_id)
        .collect::<std::collections::HashSet<_>>();

    let retention = WorkspaceRetentionConfig::load();
    let hide_ttl = retention.hide_ttl();
    let jobs_by_id: std::collections::HashMap<String, Job> = jobs
        .iter()
        .map(|job| (job.id.clone(), job.clone()))
        .collect();

    let mut items = Vec::new();

    for worker in &workers {
        if worker
            .parent_turn_correlation_id
            .as_ref()
            .is_some_and(|parent_id| ask_parent_ids.contains(parent_id))
        {
            continue;
        }
        let stasis_job = worker
            .stasis_job_id
            .as_deref()
            .or(Some(worker.work_id.as_str()))
            .and_then(|job_id| jobs_by_id.get(job_id));
        if let Some(item) = project_turn_worker(worker, stasis_job, include_terminal, hide_ttl) {
            items.push(item);
        }
    }

    for job in jobs {
        if worker_ids.contains(&job.correlation_id) {
            continue;
        }
        if let Some(item) = project_job(&job, include_terminal, hide_ttl) {
            items.push(item);
        }
    }

    for ask in ask_job_store().list_for_workspace(include_terminal) {
        if let Some(item) = project_ask_job(&ask, include_terminal, hide_ttl) {
            items.push(item);
        }
    }

    for budget in turn_budget_request_store().list_for_workspace(include_terminal) {
        if let Some(item) = project_turn_budget_request(&budget, include_terminal) {
            items.push(item);
        }
    }

    items.sort_by(|left, right| right.card.updated_at_utc.cmp(&left.card.updated_at_utc));
    Ok(items)
}

pub fn project_ask_job(
    record: &AskJobRecord,
    include_terminal: bool,
    hide_ttl: Duration,
) -> Option<ProjectedWorkItem> {
    let (column, status_label, terminal) = column_for_ask_job(record, include_terminal, hide_ttl)?;

    let title = truncate_line(&record.prompt, 80);
    let card = WorkCard {
        id: WorkCardId(record.job_id.clone()),
        column,
        title: title.clone(),
        status_label: status_label.to_string(),
        created_at_utc: record.created_at_utc,
        updated_at_utc: record.updated_at_utc,
    };

    let result_excerpt = record
        .output_text
        .as_deref()
        .map(|text| truncate_line(text, 500));

    let detail = WorkCardDetail {
        card: card.clone(),
        kind: WorkCardKind::AskJob,
        subtitle: Some("background ask".to_string()),
        session_id: Some(record.session_id.clone()),
        correlation_id: None,
        manuscript_id: record.manuscript_id.clone(),
        job_id: Some(record.job_id.clone()),
        work_id: None,
        job_type: Some("daemon.ask".to_string()),
        user_ack: None,
        wrapping_up_reasons: Vec::new(),
        terminal,
        error: record.error.clone(),
        result_excerpt,
        task_line: Some(truncate_line(&record.prompt, 500)),
        tool_names: None,
        associations: workspace_store().associations(&record.job_id),
    };

    Some(ProjectedWorkItem { card, detail })
}

pub fn project_turn_budget_request(
    record: &TurnBudgetRequest,
    include_terminal: bool,
) -> Option<ProjectedWorkItem> {
    let (column, status_label, terminal) = match record.status {
        TurnBudgetRequestStatus::Pending => (WorkBoardColumn::Blocked, "needs approval", false),
        TurnBudgetRequestStatus::Approved => {
            if !include_terminal {
                return None;
            }
            (WorkBoardColumn::Done, "approved", true)
        }
        TurnBudgetRequestStatus::Denied => {
            if !include_terminal {
                return None;
            }
            (WorkBoardColumn::Blocked, "denied", true)
        }
        TurnBudgetRequestStatus::Expired => {
            if !include_terminal {
                return None;
            }
            (WorkBoardColumn::Blocked, "expired", true)
        }
    };

    let title = truncate_line(&record.reason, 80);
    let card = WorkCard {
        id: WorkCardId(record.request_id.clone()),
        column,
        title: title.clone(),
        status_label: status_label.to_string(),
        created_at_utc: record.created_at_utc,
        updated_at_utc: record.updated_at_utc,
    };

    let task_line = record.progress_summary.clone().or_else(|| {
        Some(format!(
            "At {}/{} rounds — requesting +{}",
            record.rounds_executed, record.max_tool_rounds, record.requested_rounds
        ))
    });

    let detail = WorkCardDetail {
        card: card.clone(),
        kind: WorkCardKind::TurnBudgetRequest,
        subtitle: record.channel.clone(),
        session_id: Some(record.session_id.clone()),
        correlation_id: record.turn_correlation_id.clone(),
        manuscript_id: None,
        job_id: None,
        work_id: None,
        job_type: Some("turn.budget_request".to_string()),
        user_ack: None,
        wrapping_up_reasons: Vec::new(),
        terminal,
        error: None,
        result_excerpt: record
            .granted_rounds
            .map(|granted| format!("Granted +{granted} tool rounds")),
        task_line,
        tool_names: None,
        associations: workspace_store().associations(&record.request_id),
    };

    Some(ProjectedWorkItem { card, detail })
}

fn column_for_ask_job(
    record: &AskJobRecord,
    include_terminal: bool,
    hide_ttl: Duration,
) -> Option<(WorkBoardColumn, &'static str, bool)> {
    match record.status {
        AskJobStatus::Pending => Some((WorkBoardColumn::Backlog, "queued", false)),
        AskJobStatus::Running => Some((WorkBoardColumn::InFlight, "running", false)),
        AskJobStatus::Succeeded => {
            if !include_terminal && terminal_ask_stale(record.finished_at_utc, hide_ttl) {
                return None;
            }
            Some((WorkBoardColumn::Done, "succeeded", true))
        }
        AskJobStatus::Failed => {
            if !include_terminal
                && terminal_ask_stale(record.finished_at_utc.or(Some(record.updated_at_utc)), hide_ttl)
            {
                return None;
            }
            Some((WorkBoardColumn::Blocked, "failed", true))
        }
        AskJobStatus::Canceled => {
            if !include_terminal
                && terminal_ask_stale(record.finished_at_utc.or(Some(record.updated_at_utc)), hide_ttl)
            {
                return None;
            }
            Some((WorkBoardColumn::Blocked, "canceled", true))
        }
    }
}

fn terminal_ask_stale(finished_at: Option<DateTime<Utc>>, hide_ttl: Duration) -> bool {
    finished_at.is_some_and(|at| retention::terminal_card_stale(at, hide_ttl))
}

pub fn project_turn_worker(
    record: &TurnWorkRecord,
    stasis_job: Option<&Job>,
    include_terminal: bool,
    hide_ttl: Duration,
) -> Option<ProjectedWorkItem> {
    let effective_status = effective_turn_worker_status(record, stasis_job);
    let (column, status_label, wrapping_up_reasons, terminal) =
        column_for_turn_worker_status(record, effective_status, include_terminal, hide_ttl)?;

    let title = if !record.user_ack.trim().is_empty() {
        record.user_ack.trim().to_string()
    } else {
        truncate_line(&record.task_prompt, 80)
    };

    let card = WorkCard {
        id: WorkCardId(record.work_id.clone()),
        column,
        title: title.clone(),
        status_label: status_label.to_string(),
        created_at_utc: record.created_at,
        updated_at_utc: record.updated_at,
    };

    let detail = WorkCardDetail {
        card: card.clone(),
        kind: WorkCardKind::TurnWorker,
        subtitle: Some(record.intent.clone()),
        session_id: Some(record.session_id.clone()),
        correlation_id: record.parent_turn_correlation_id.clone(),
        manuscript_id: None,
        job_id: None,
        work_id: Some(record.work_id.clone()),
        job_type: None,
        user_ack: Some(record.user_ack.clone()),
        wrapping_up_reasons,
        terminal,
        error: record.error.clone(),
        result_excerpt: record
            .result_text
            .as_deref()
            .map(|text| truncate_line(text, 500)),
        task_line: Some(truncate_line(&record.task_prompt, 500)),
        tool_names: if record.tool_names.is_empty() {
            None
        } else {
            Some(record.tool_names.clone())
        },
        associations: workspace_store().associations(&record.work_id),
    };

    Some(ProjectedWorkItem { card, detail })
}

pub fn project_job(job: &Job, include_terminal: bool, hide_ttl: Duration) -> Option<ProjectedWorkItem> {
    let (column, status_label, wrapping_up_reasons, terminal) =
        column_for_job(job, include_terminal, hide_ttl)?;

    let payload = parse_payload(&job.payload_ref);
    let title = title_for_job(job, &payload);
    let created = job.scheduled_at;
    let updated = job
        .finished_at
        .or(job.started_at)
        .unwrap_or(job.scheduled_at);

    let card = WorkCard {
        id: WorkCardId(job.id.clone()),
        column,
        title: title.clone(),
        status_label: status_label.to_string(),
        created_at_utc: created,
        updated_at_utc: updated,
    };

    let manuscript_id = payload
        .get("manuscript_id")
        .and_then(|value| value.as_str())
        .map(str::to_string);
    let user_ack = payload
        .get("user_ack")
        .and_then(|value| value.as_str())
        .map(str::to_string);
    let task_line = user_ack
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| truncate_line(value, 500))
        .or_else(|| {
            payload
                .get("prompt")
                .or_else(|| payload.get("task"))
                .and_then(|value| value.as_str())
                .map(|value| truncate_line(value, 500))
        });

    let detail = WorkCardDetail {
        card: card.clone(),
        kind: WorkCardKind::StasisJob,
        subtitle: Some(job.job_type.clone()),
        session_id: None,
        correlation_id: Some(job.correlation_id.clone()),
        manuscript_id,
        job_id: Some(job.id.clone()),
        work_id: None,
        job_type: Some(job.job_type.clone()),
        user_ack,
        wrapping_up_reasons,
        terminal,
        error: job.last_error.clone(),
        result_excerpt: None,
        task_line,
        tool_names: None,
        associations: workspace_store().associations(&job.id),
    };

    Some(ProjectedWorkItem { card, detail })
}

pub fn column_for_job(
    job: &Job,
    include_terminal: bool,
    hide_ttl: Duration,
) -> Option<(WorkBoardColumn, &'static str, Vec<String>, bool)> {
    match job.state {
        JobState::Enqueued => Some((
            WorkBoardColumn::Backlog,
            "queued",
            Vec::new(),
            false,
        )),
        JobState::Leased => Some((
            WorkBoardColumn::InFlight,
            "leased",
            Vec::new(),
            false,
        )),
        JobState::Running => Some((
            WorkBoardColumn::InFlight,
            "running",
            Vec::new(),
            false,
        )),
        JobState::Succeeded => {
            if !include_terminal && terminal_job_stale(job.finished_at, hide_ttl) {
                return None;
            }
            Some((
                WorkBoardColumn::Done,
                "succeeded",
                Vec::new(),
                true,
            ))
        }
        JobState::Failed => Some((
            WorkBoardColumn::Blocked,
            "failed",
            Vec::new(),
            true,
        )),
        JobState::DeadLetter => Some((
            WorkBoardColumn::Blocked,
            "dead_letter",
            Vec::new(),
            true,
        )),
        JobState::Canceled => Some((
            WorkBoardColumn::Blocked,
            "canceled",
            Vec::new(),
            true,
        )),
    }
}

pub fn column_for_turn_worker(
    record: &TurnWorkRecord,
    include_terminal: bool,
    hide_ttl: Duration,
) -> Option<(WorkBoardColumn, &'static str, Vec<String>, bool)> {
    column_for_turn_worker_status(
        record,
        effective_turn_worker_status(record, None),
        include_terminal,
        hide_ttl,
    )
}

/// Align worker board column with durable Stasis job state when the worker record lags.
fn effective_turn_worker_status(record: &TurnWorkRecord, stasis_job: Option<&Job>) -> TurnWorkStatus {
    if record.synthesis_delivered {
        return TurnWorkStatus::Completed;
    }
    if let Some(job) = stasis_job {
        match job.state {
            JobState::Succeeded => {
                if matches!(
                    record.status,
                    TurnWorkStatus::Pending | TurnWorkStatus::Running | TurnWorkStatus::Completed
                ) {
                    return TurnWorkStatus::Completed;
                }
            }
            JobState::Failed | JobState::DeadLetter => {
                if matches!(
                    record.status,
                    TurnWorkStatus::Pending | TurnWorkStatus::Running
                ) {
                    return TurnWorkStatus::Failed;
                }
            }
            JobState::Canceled => {
                if matches!(
                    record.status,
                    TurnWorkStatus::Pending | TurnWorkStatus::Running
                ) {
                    return TurnWorkStatus::Cancelled;
                }
            }
            _ => {}
        }
    }
    record.status
}

fn column_for_turn_worker_status(
    record: &TurnWorkRecord,
    status: TurnWorkStatus,
    include_terminal: bool,
    hide_ttl: Duration,
) -> Option<(WorkBoardColumn, &'static str, Vec<String>, bool)> {
    match status {
        TurnWorkStatus::Pending => Some((
            WorkBoardColumn::Backlog,
            "pending",
            Vec::new(),
            false,
        )),
        TurnWorkStatus::Running => Some((
            WorkBoardColumn::InFlight,
            "running",
            Vec::new(),
            false,
        )),
        TurnWorkStatus::Completed => {
            if record.synthesis_delivered || wrapping_up_stale(record.updated_at) {
                if !include_terminal && wrapping_up_stale(record.updated_at) {
                    return None;
                }
                return Some((
                    WorkBoardColumn::Done,
                    "completed",
                    Vec::new(),
                    true,
                ));
            }
            Some((
                WorkBoardColumn::WrappingUp,
                "synthesis pending",
                vec!["synthesis_pending".to_string()],
                false,
            ))
        }
        TurnWorkStatus::Failed => {
            if !include_terminal && terminal_turn_worker_stale(record.updated_at, hide_ttl) {
                return None;
            }
            Some((
                WorkBoardColumn::Blocked,
                "failed",
                Vec::new(),
                true,
            ))
        }
        TurnWorkStatus::Cancelled => {
            if !include_terminal && terminal_turn_worker_stale(record.updated_at, hide_ttl) {
                return None;
            }
            Some((
                WorkBoardColumn::Blocked,
                "cancelled",
                Vec::new(),
                true,
            ))
        }
    }
}

fn terminal_job_stale(finished_at: Option<DateTime<Utc>>, hide_ttl: Duration) -> bool {
    finished_at.is_some_and(|at| retention::terminal_card_stale(at, hide_ttl))
}

fn terminal_turn_worker_stale(updated_at: DateTime<Utc>, hide_ttl: Duration) -> bool {
    retention::terminal_card_stale(updated_at, hide_ttl)
}

fn wrapping_up_stale(updated_at: DateTime<Utc>) -> bool {
    Utc::now().signed_duration_since(updated_at) > WRAPPING_UP_STALE
}

fn parse_payload(payload_ref: &str) -> Value {
    serde_json::from_str(payload_ref).unwrap_or(Value::Null)
}

pub fn title_for_job(job: &Job, payload: &Value) -> String {
    if job.job_type == OPENSHELL_SANDBOX_RUN_JOB_TYPE {
        if let Some(script) = payload.get("skill_script").and_then(|v| v.as_str()) {
            let manuscript = payload
                .get("manuscript_id")
                .and_then(|v| v.as_str())
                .unwrap_or("skill");
            return format!("Skill: {manuscript} — {script}");
        }
        if let Some(command) = payload
            .get("command")
            .and_then(|v| v.as_array())
            .and_then(|items| items.first())
            .and_then(|v| v.as_str())
        {
            return format!("Sandbox: {command}");
        }
    }

    if let Some(manuscript) = payload.get("manuscript_id").and_then(|v| v.as_str()) {
        if manuscript.contains("brief") {
            return "Scheduled: morning brief".to_string();
        }
    }

    if job.job_type.contains("workflow") {
        return format!(
            "Workflow: {}",
            job.correlation_id.chars().take(8).collect::<String>()
        );
    }

    if job.job_type.contains("agent") {
        return format!(
            "Chat turn {}",
            job.correlation_id.chars().take(8).collect::<String>()
        );
    }

    job.job_type
        .split('.')
        .last()
        .unwrap_or(&job.job_type)
        .to_string()
}

fn truncate_line(text: &str, max: usize) -> String {
    let trimmed = text.trim().replace('\n', " ");
    if trimmed.chars().count() <= max {
        trimmed
    } else {
        format!("{}…", trimmed.chars().take(max).collect::<String>())
    }
}

pub fn counts_by_column(cards: &[WorkCard]) -> std::collections::HashMap<String, u32> {
    let mut counts = std::collections::HashMap::new();
    for card in cards {
        let key = column_key(card.column);
        *counts.entry(key).or_insert(0) += 1;
    }
    counts
}

pub fn column_key(column: WorkBoardColumn) -> String {
    match column {
        WorkBoardColumn::Backlog => "backlog".to_string(),
        WorkBoardColumn::InFlight => "in_flight".to_string(),
        WorkBoardColumn::WrappingUp => "wrapping_up".to_string(),
        WorkBoardColumn::Done => "done".to_string(),
        WorkBoardColumn::Blocked => "blocked".to_string(),
    }
}

pub fn parse_column_filter(value: &str) -> Option<WorkBoardColumn> {
    match value.trim().to_ascii_lowercase().as_str() {
        "backlog" => Some(WorkBoardColumn::Backlog),
        "in_flight" | "inflight" => Some(WorkBoardColumn::InFlight),
        "wrapping_up" | "wrappingup" => Some(WorkBoardColumn::WrappingUp),
        "done" => Some(WorkBoardColumn::Done),
        "blocked" => Some(WorkBoardColumn::Blocked),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn sample_job(state: JobState, job_type: &str, payload: &str) -> Job {
        Job {
            id: "job-1".to_string(),
            queue: "default".to_string(),
            job_type: job_type.to_string(),
            payload_ref: payload.to_string(),
            state,
            priority: 100,
            attempts: 0,
            max_attempts: 1,
            idempotency_key: "idem".to_string(),
            correlation_id: "corr-1".to_string(),
            causation_id: "cause".to_string(),
            trace_id: "trace".to_string(),
            sttp_input_node_id: "in".to_string(),
            scheduled_at: Utc.with_ymd_and_hms(2026, 5, 30, 9, 0, 0).unwrap(),
            started_at: None,
            finished_at: None,
            heartbeat_at: None,
            lease_owner: None,
            lease_expires_at: None,
            sttp_output_node_id: None,
            last_error: None,
            backoff_policy: Default::default(),
        }
    }

    #[test]
    fn title_for_openshell_skill() {
        let job = sample_job(
            JobState::Running,
            OPENSHELL_SANDBOX_RUN_JOB_TYPE,
            r#"{"manuscript_id":"echo-skill","skill_script":"scripts/echo.sh"}"#,
        );
        let payload = parse_payload(&job.payload_ref);
        assert_eq!(
            title_for_job(&job, &payload),
            "Skill: echo-skill — scripts/echo.sh"
        );
    }

    #[test]
    fn turn_worker_completed_maps_to_wrapping_up() {
        let record = TurnWorkRecord {
            work_id: "work-1".to_string(),
            session_id: "sess".to_string(),
            parent_turn_correlation_id: None,
            parent_stream_turn_id: 0,
            intent: "research".to_string(),
            task_prompt: "run skill".to_string(),
            status: TurnWorkStatus::Completed,
            result_text: Some("ok".to_string()),
            tool_names: vec![],
            termination_reason: None,
            error: None,
            user_ack: "Running skill".to_string(),
            provider: "openai".to_string(),
            model: "gpt".to_string(),
            response_depth_mode: "normal".to_string(),
            max_tool_rounds: 10,
            delivery_target: None,
            parent_user_prompt: None,
            handoff_capsule: None,
            worker_scratch: None,
            synthesis_delivered: false,
            stasis_job_id: None,
            thread_id: None,
            stage_role: None,
            model_hint: None,
            manuscript_id: None,
            branch_group_id: None,
            archived: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let hide_ttl = Duration::hours(24);
        let (column, status, reasons, terminal) =
            column_for_turn_worker(&record, true, hide_ttl).expect("column");
        assert_eq!(column, WorkBoardColumn::WrappingUp);
        assert_eq!(status, "synthesis pending");
        assert_eq!(reasons, vec!["synthesis_pending"]);
        assert!(!terminal);
    }

    #[test]
    fn turn_worker_synthesis_delivered_maps_to_done() {
        let record = TurnWorkRecord {
            work_id: "work-2".to_string(),
            session_id: "sess".to_string(),
            parent_turn_correlation_id: None,
            parent_stream_turn_id: 0,
            intent: "research".to_string(),
            task_prompt: "run skill".to_string(),
            status: TurnWorkStatus::Completed,
            result_text: Some("ok".to_string()),
            tool_names: vec![],
            termination_reason: None,
            error: None,
            user_ack: "Running skill".to_string(),
            provider: "openai".to_string(),
            model: "gpt".to_string(),
            response_depth_mode: "normal".to_string(),
            max_tool_rounds: 10,
            delivery_target: None,
            parent_user_prompt: None,
            handoff_capsule: None,
            worker_scratch: None,
            synthesis_delivered: true,
            stasis_job_id: None,
            thread_id: None,
            stage_role: None,
            model_hint: None,
            manuscript_id: None,
            branch_group_id: None,
            archived: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let hide_ttl = Duration::hours(24);
        let (column, status, _, terminal) =
            column_for_turn_worker(&record, true, hide_ttl).expect("column");
        assert_eq!(column, WorkBoardColumn::Done);
        assert_eq!(status, "completed");
        assert!(terminal);
    }

    #[test]
    fn job_enqueued_is_backlog() {
        let job = sample_job(JobState::Enqueued, "openshell.sandbox.run", "{}");
        let hide_ttl = Duration::hours(24);
        let (column, _, _, _) = column_for_job(&job, true, hide_ttl).expect("column");
        assert_eq!(column, WorkBoardColumn::Backlog);
    }
}
