//! Single-card workspace projections — O(1) source lookups.

use stasis::application::runtime::runtime_factory::RuntimeComposition;
use stasis::domain::runtime::job::Job;
use stasis::ports::outbound::runtime::job_store::JobStore;

use crate::agent_runtime::turn_worker::{turn_worker_store, TurnWorkRecord};
use crate::daemon_api::WorkCardKind;
use crate::turn_budget_request::turn_budget_request_store;
use crate::workspace::ask_job_store::ask_job_store;
use crate::workspace::card::{
    project_ask_job, project_job, project_turn_budget_request, project_turn_worker,
    ProjectedWorkItem,
};
use crate::workspace::domain_event::WorkspaceDomainEvent;
use crate::workspace::retention::WorkspaceRetentionConfig;

pub async fn get_stasis_job(
    runtime: &RuntimeComposition,
    job_id: &str,
) -> Option<Job> {
    match runtime {
        RuntimeComposition::InMemory(rt) => rt.job_store.get(job_id).await.ok().flatten(),
        RuntimeComposition::Surreal(rt) => rt.job_store.get(job_id).await.ok().flatten(),
    }
}

fn retention_hide_ttl() -> chrono::Duration {
    WorkspaceRetentionConfig::load().hide_ttl()
}

fn worker_hidden_for_ask_parent(record: &TurnWorkRecord) -> bool {
    let Some(parent_id) = record.parent_turn_correlation_id.as_ref() else {
        return false;
    };
    ask_job_store()
        .list_for_workspace(true)
        .into_iter()
        .any(|ask| ask.job_id == *parent_id)
}

fn stasis_job_owned_by_worker(job: &Job) -> bool {
    turn_worker_store().get(&job.correlation_id).is_some()
}

pub async fn project_domain_event(
    runtime: &RuntimeComposition,
    event: &WorkspaceDomainEvent,
) -> Option<(String, Option<ProjectedWorkItem>)> {
    match event {
        WorkspaceDomainEvent::TurnWorkerChanged { work_id } => {
            project_turn_worker_card(runtime, work_id).await
        }
        WorkspaceDomainEvent::AskJobChanged { job_id } => project_ask_job_card(job_id),
        WorkspaceDomainEvent::BudgetRequestChanged { request_id } => {
            project_budget_card(request_id)
        }
        WorkspaceDomainEvent::StasisJobChanged { job_id } => {
            project_stasis_job_card(runtime, job_id).await
        }
    }
}

pub fn domain_event_for_kind(card_id: &str, kind: WorkCardKind) -> WorkspaceDomainEvent {
    match kind {
        WorkCardKind::TurnWorker => WorkspaceDomainEvent::TurnWorkerChanged {
            work_id: card_id.to_string(),
        },
        WorkCardKind::AskJob => WorkspaceDomainEvent::AskJobChanged {
            job_id: card_id.to_string(),
        },
        WorkCardKind::TurnBudgetRequest => WorkspaceDomainEvent::BudgetRequestChanged {
            request_id: card_id.to_string(),
        },
        WorkCardKind::StasisJob
        | WorkCardKind::InteractiveTurn
        | WorkCardKind::RecurringTick => WorkspaceDomainEvent::StasisJobChanged {
            job_id: card_id.to_string(),
        },
    }
}

async fn project_turn_worker_card(
    runtime: &RuntimeComposition,
    work_id: &str,
) -> Option<(String, Option<ProjectedWorkItem>)> {
    let Some(record) = turn_worker_store().get(work_id) else {
        return Some((work_id.to_string(), None));
    };
    if record.archived || worker_hidden_for_ask_parent(&record) {
        return Some((work_id.to_string(), None));
    }
    let stasis_job_id = record
        .stasis_job_id
        .as_deref()
        .unwrap_or(record.work_id.as_str());
    let stasis_job = get_stasis_job(runtime, stasis_job_id).await;
    let item = project_turn_worker(&record, stasis_job.as_ref(), true, retention_hide_ttl());
    Some((work_id.to_string(), item))
}

fn project_ask_job_card(job_id: &str) -> Option<(String, Option<ProjectedWorkItem>)> {
    let Some(record) = ask_job_store().get(job_id) else {
        return Some((job_id.to_string(), None));
    };
    if record.archived {
        return Some((job_id.to_string(), None));
    }
    let item = project_ask_job(&record, true, retention_hide_ttl());
    Some((job_id.to_string(), item))
}

fn project_budget_card(request_id: &str) -> Option<(String, Option<ProjectedWorkItem>)> {
    let Some(record) = turn_budget_request_store().get(request_id) else {
        return Some((request_id.to_string(), None));
    };
    let item = project_turn_budget_request(&record, true);
    Some((request_id.to_string(), item))
}

async fn project_stasis_job_card(
    runtime: &RuntimeComposition,
    job_id: &str,
) -> Option<(String, Option<ProjectedWorkItem>)> {
    let Some(job) = get_stasis_job(runtime, job_id).await else {
        return Some((job_id.to_string(), None));
    };
    if stasis_job_owned_by_worker(&job) {
        return Some((job_id.to_string(), None));
    }
    let item = project_job(&job, true, retention_hide_ttl());
    Some((job_id.to_string(), item))
}
