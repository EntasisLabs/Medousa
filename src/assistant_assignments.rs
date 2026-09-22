//! Projections from native executors into the durable Assistant assignment ledger.
//!
//! Projection failure never changes native execution: these records are an
//! inspectable ownership view, not a second scheduler.

use anyhow::{Context, Result};
use medousa_acp_client::coordination::store::CoordinationStore;
use medousa_types::assistant_assignment::*;
use medousa_types::{SessionId, SessionRef};

use crate::agent_runtime::turn_worker::{TurnWorkRecord, TurnWorkStatus};

fn ledger() -> Result<CoordinationStore> {
    CoordinationStore::open(&crate::paths::medousa_data_dir().join("coordination"))
}

fn worker_status(status: TurnWorkStatus) -> AssistantAssignmentStatus {
    match status {
        TurnWorkStatus::Pending => AssistantAssignmentStatus::Pending,
        TurnWorkStatus::Running => AssistantAssignmentStatus::Running,
        TurnWorkStatus::Completed => AssistantAssignmentStatus::Completed,
        TurnWorkStatus::Failed => AssistantAssignmentStatus::Failed,
        TurnWorkStatus::Cancelled => AssistantAssignmentStatus::Cancelled,
    }
}

fn worker_sequence(status: TurnWorkStatus) -> u64 {
    match status {
        TurnWorkStatus::Pending => 0,
        TurnWorkStatus::Running => 1,
        TurnWorkStatus::Completed | TurnWorkStatus::Failed | TurnWorkStatus::Cancelled => 2,
    }
}

/// Idempotently projects one current worker snapshot. Event ids are stable per
/// native state, so repeated persistence cannot inflate the journal.
pub fn project_turn_worker(record: &TurnWorkRecord) -> Result<()> {
    let authority = crate::workshop_authority::current()
        .map_err(anyhow::Error::msg)?
        .clone();
    let owner = record
        .identity_user_id
        .clone()
        .unwrap_or_else(|| format!("session:{}", record.session_id));
    let session = SessionId::parse(record.session_id.clone())
        .ok()
        .map(|session_id| SessionRef {
            authority_id: authority.clone(),
            session_id,
        });
    let correlation = record
        .parent_turn_correlation_id
        .clone()
        .unwrap_or_else(|| record.work_id.clone());
    let origin = AssistantAssignment {
        schema_version: ASSISTANT_ASSIGNMENT_SCHEMA_VERSION,
        assignment_id: format!("turn-worker:{}", record.work_id),
        owner_id: owner.clone(),
        principal_id: owner.clone(),
        intent: record.intent.clone(),
        source: AssistantAssignmentSource {
            authority_id: authority.clone(),
            session,
            coordination_channel_id: None,
        },
        execution: AssistantExecutionBinding {
            kind: AssistantAssignmentKind::InternalTurnWorker,
            native_id: record.work_id.clone(),
            runtime_id: Some(record.parent_runtime_id.clone()),
            execution_session: None,
            workshop_authority_id: authority.clone(),
            forge_work_id: record.parent_code_work_id.clone(),
        },
        budget: AssistantAssignmentBudget {
            max_tool_rounds: Some(record.max_tool_rounds.try_into().unwrap_or(u32::MAX)),
            ..Default::default()
        },
        status: AssistantAssignmentStatus::Pending,
        references: AssistantAssignmentReferences {
            grant_id: record
                .task_execution_grant
                .as_ref()
                .map(|grant| grant.grant_id.clone()),
            continuation_id: record.parent_turn_correlation_id.clone(),
            ..Default::default()
        },
        actor_id: owner,
        correlation_id: correlation.clone(),
        causation_id: record.parent_turn_correlation_id.clone(),
        next_action: Some("observe native turn worker".into()),
        created_at: record.created_at,
        updated_at: record.created_at,
    };
    let store = ledger()?;
    store
        .create_assistant_assignment(&origin)
        .context("create turn-worker assignment origin")?;
    let status = worker_status(record.status);
    let event = AssistantAssignmentEvent {
        event_id: format!(
            "turn-worker:{}:{}",
            record.work_id,
            worker_sequence(record.status)
        ),
        assignment_id: origin.assignment_id,
        native_sequence: worker_sequence(record.status),
        status,
        actor_id: format!("runtime:{}", record.parent_runtime_id),
        correlation_id: correlation,
        causation_id: record.stasis_job_id.clone(),
        detail: record
            .error
            .clone()
            .or_else(|| record.termination_reason.clone()),
        next_action: if status.is_terminal() {
            Some("inspect result or delivery".into())
        } else {
            Some("wait for native executor".into())
        },
        references: AssistantAssignmentReferencePatch::default(),
        observed_at: record.updated_at,
    };
    store
        .record_assistant_assignment_event(&authority, &event)
        .context("record turn-worker assignment event")?;
    Ok(())
}

fn assignment_prefix(kind: AssistantAssignmentKind) -> &'static str {
    match kind {
        AssistantAssignmentKind::InternalTurnWorker => "turn-worker",
        AssistantAssignmentKind::ExternalPeer => "peer",
        AssistantAssignmentKind::Job => "job",
        AssistantAssignmentKind::Workflow => "workflow",
        AssistantAssignmentKind::ScheduledOccurrence => "scheduled",
    }
}

/// Persist the exact admitted owner/session before a native runtime side effect
/// can outlive the turn that requested it.
pub fn project_runtime_origin(
    kind: AssistantAssignmentKind,
    native_id: &str,
    intent: &str,
    max_attempts: Option<u32>,
    scope: &crate::turn_scope::TurnContinuationScope,
    runtime_id: Option<String>,
    causation_id: Option<String>,
) -> Result<()> {
    let authority = crate::workshop_authority::current()
        .map_err(anyhow::Error::msg)?
        .clone();
    let principal = crate::agent_runtime::execution_context::active_turn_execution_context()
        .and_then(|context| {
            context
                .principal()
                .profile_id()
                .map(str::to_string)
                .or_else(|| {
                    context
                        .principal()
                        .credential_id()
                        .map(|credential| credential.as_str().to_string())
                })
        })
        .or_else(|| scope.identity_user_id.clone())
        .unwrap_or_else(|| format!("session:{}", scope.session_id));
    let owner = scope
        .identity_user_id
        .clone()
        .unwrap_or_else(|| principal.clone());
    let session = SessionId::parse(scope.session_id.clone())
        .ok()
        .map(|session_id| SessionRef {
            authority_id: authority.clone(),
            session_id,
        });
    let assignment_id = format!("{}:{native_id}", assignment_prefix(kind));
    let now = chrono::Utc::now();
    let origin = AssistantAssignment {
        schema_version: ASSISTANT_ASSIGNMENT_SCHEMA_VERSION,
        assignment_id,
        owner_id: owner.clone(),
        principal_id: principal,
        intent: intent.to_string(),
        source: AssistantAssignmentSource {
            authority_id: authority.clone(),
            session,
            coordination_channel_id: scope
                .delivery_target
                .as_ref()
                .map(|target| target.channel_id.clone()),
        },
        execution: AssistantExecutionBinding {
            kind,
            native_id: native_id.to_string(),
            runtime_id,
            execution_session: None,
            workshop_authority_id: authority,
            forge_work_id: None,
        },
        budget: AssistantAssignmentBudget {
            max_attempts,
            ..Default::default()
        },
        status: AssistantAssignmentStatus::Pending,
        references: AssistantAssignmentReferences {
            continuation_id: Some(scope.turn_correlation_id.clone()),
            ..Default::default()
        },
        actor_id: owner,
        correlation_id: scope.turn_correlation_id.clone(),
        causation_id,
        next_action: Some("await native executor".into()),
        created_at: now,
        updated_at: now,
    };
    ledger()?
        .create_assistant_assignment(&origin)
        .context("create runtime assignment origin")?;
    Ok(())
}

pub fn project_runtime_event(
    kind: AssistantAssignmentKind,
    native_id: &str,
    status: AssistantAssignmentStatus,
    native_sequence: u64,
    actor_id: &str,
    detail: Option<String>,
) -> Result<()> {
    let authority = crate::workshop_authority::current()
        .map_err(anyhow::Error::msg)?
        .clone();
    let assignment_id = format!("{}:{native_id}", assignment_prefix(kind));
    let store = ledger()?;
    let view = store
        .assistant_assignment(&authority, &assignment_id)
        .context("read runtime assignment origin")?;
    store
        .record_assistant_assignment_event(
            &authority,
            &AssistantAssignmentEvent {
                event_id: format!("{assignment_id}:{native_sequence}:{status:?}"),
                assignment_id,
                native_sequence,
                status,
                actor_id: actor_id.to_string(),
                correlation_id: view.assignment.correlation_id,
                causation_id: Some(native_id.to_string()),
                detail,
                next_action: Some(
                    if status.is_terminal() {
                        "inspect native result"
                    } else {
                        "await native executor"
                    }
                    .into(),
                ),
                references: AssistantAssignmentReferencePatch::default(),
                observed_at: chrono::Utc::now(),
            },
        )
        .context("record runtime assignment event")?;
    Ok(())
}

/// Create one occurrence record from the durable workflow origin retained when
/// the recurring definition was registered.
pub fn project_scheduled_occurrence(parent_workflow_id: &str, job_id: &str) -> Result<()> {
    let authority = crate::workshop_authority::current()
        .map_err(anyhow::Error::msg)?
        .clone();
    let store = ledger()?;
    let parent = store
        .assistant_assignment(&authority, &format!("workflow:{parent_workflow_id}"))?
        .assignment;
    let now = chrono::Utc::now();
    let origin = AssistantAssignment {
        assignment_id: format!("scheduled:{job_id}"),
        intent: parent.intent.clone(),
        source: parent.source.clone(),
        execution: AssistantExecutionBinding {
            kind: AssistantAssignmentKind::ScheduledOccurrence,
            native_id: job_id.to_string(),
            runtime_id: parent.execution.runtime_id.clone(),
            execution_session: parent.execution.execution_session.clone(),
            workshop_authority_id: parent.execution.workshop_authority_id.clone(),
            forge_work_id: parent.execution.forge_work_id.clone(),
        },
        status: AssistantAssignmentStatus::Pending,
        references: parent.references.clone(),
        correlation_id: parent_workflow_id.to_string(),
        causation_id: Some(parent.assignment_id),
        next_action: Some("await scheduled executor".into()),
        created_at: now,
        updated_at: now,
        ..parent
    };
    store.create_assistant_assignment(&origin)?;
    Ok(())
}
