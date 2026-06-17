//! Workspace card mutations — cancel, retry, vault associations (Phase W3).

use std::sync::Arc;

use chrono::Utc;
use stasis::application::runtime::runtime_factory::RuntimeComposition;
use stasis::sdk::runtime_sdk::RuntimeSdk;
use stasis::domain::runtime::job::{Job, JobState};
use stasis::ports::outbound::runtime::job_store::JobStore;

use crate::agent_runtime::turn_worker::{TurnWorkStatus, turn_worker_store};
use crate::daemon_api::{
    ReplayAndResumeResponse, WorkCardDetail, WorkCardKind, WorkspaceCardActionResponse,
};
use crate::turn_continuation;
use crate::workspace::ask_job_store::ask_job_store;
use crate::workspace::event::event_for_vault_link;
use crate::workspace::service::WorkspaceService;
use crate::workspace::store::workspace_store;

#[derive(Debug)]
pub enum CardActionError {
    NotFound,
    NotActionable(String),
    Internal(String),
}

impl std::fmt::Display for CardActionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl CardActionError {
    pub fn message(&self) -> String {
        match self {
            Self::NotFound => "card not found".to_string(),
            Self::NotActionable(reason) => reason.clone(),
            Self::Internal(reason) => reason.clone(),
        }
    }
}

pub fn normalize_vault_path(raw: &str) -> Result<String, CardActionError> {
    let trimmed = raw.trim().trim_start_matches('/');
    if trimmed.is_empty() {
        return Err(CardActionError::NotActionable(
            "vault_path is required".to_string(),
        ));
    }
    if trimmed.contains("..") {
        return Err(CardActionError::NotActionable(
            "vault_path must not contain '..'".to_string(),
        ));
    }
    if trimmed.contains('\\') {
        return Err(CardActionError::NotActionable(
            "vault_path must use forward slashes".to_string(),
        ));
    }
    Ok(trimmed.to_string())
}

pub async fn cancel_card(
    runtime: Arc<RuntimeComposition>,
    card_id: &str,
) -> Result<WorkspaceCardActionResponse, CardActionError> {
    let card_id = card_id.trim();
    if card_id.is_empty() {
        return Err(CardActionError::NotActionable(
            "card_id is required".to_string(),
        ));
    }

    let detail = resolve_card_detail(runtime.clone(), card_id).await?;
    let (ok, message, job_id) = match detail.kind {
        WorkCardKind::TurnWorker => {
            let work_id = detail.work_id.clone().ok_or_else(|| {
                CardActionError::NotActionable("turn worker card missing work_id".to_string())
            })?;
            cancel_turn_worker(&work_id)?
        }
        WorkCardKind::StasisJob => {
            let job_id = detail.job_id.clone().ok_or_else(|| {
                CardActionError::NotActionable("job card missing job_id".to_string())
            })?;
            let ok = cancel_runtime_job(runtime.as_ref(), &job_id).await;
            let message = if ok {
                format!("job {job_id} cancelled")
            } else {
                format!(
                    "job {job_id} is not cancelable (only enqueued or leased jobs can be canceled)"
                )
            };
            (ok, message, Some(job_id))
        }
        WorkCardKind::AskJob => {
            let job_id = detail.job_id.clone().ok_or_else(|| {
                CardActionError::NotActionable("ask card missing job_id".to_string())
            })?;
            ask_job_store().mark_canceled(&job_id);
            (true, format!("ask {job_id} canceled"), Some(job_id))
        }
        other => {
            return Err(CardActionError::NotActionable(format!(
                "cancel not supported for card kind {other:?}"
            )));
        }
    };

    WorkspaceService::refresh_now(runtime.as_ref()).await;
    Ok(WorkspaceCardActionResponse {
        workspace_revision: workspace_store().revision(),
        card_id: card_id.to_string(),
        action: "cancel".to_string(),
        ok,
        message,
        job_id,
        replayed: None,
        job_succeeded: None,
        associations: None,
    })
}

fn cancel_turn_worker(work_id: &str) -> Result<(bool, String, Option<String>), CardActionError> {
    let store = turn_worker_store();
    let updated = store
        .update(work_id, |record| {
            if matches!(
                record.status,
                TurnWorkStatus::Pending | TurnWorkStatus::Running
            ) {
                record.status = TurnWorkStatus::Cancelled;
            }
        })
        .ok_or(CardActionError::NotFound)?;

    let ok = updated.status == TurnWorkStatus::Cancelled;
    let message = if ok {
        format!("turn worker {work_id} cancelled")
    } else {
        format!(
            "turn worker {work_id} is not cancelable in state {:?}",
            updated.status
        )
    };
    Ok((ok, message, None))
}

async fn cancel_runtime_job(runtime: &RuntimeComposition, job_id: &str) -> bool {
    let Some(mut job) = get_job(runtime, job_id).await.ok().flatten() else {
        return false;
    };

    let cancelable = matches!(job.state, JobState::Enqueued | JobState::Leased);
    if !cancelable {
        return false;
    }

    job.state = JobState::Canceled;
    job.finished_at = Some(Utc::now());
    save_job(runtime, job).await.is_ok()
}

pub async fn archive_card(
    runtime: Arc<RuntimeComposition>,
    card_id: &str,
    purge_output: bool,
) -> Result<WorkspaceCardActionResponse, CardActionError> {
    let card_id = card_id.trim();
    if card_id.is_empty() {
        return Err(CardActionError::NotActionable(
            "card_id is required".to_string(),
        ));
    }

    let detail = resolve_card_detail(runtime.clone(), card_id).await?;
    let (ok, message, job_id) = match detail.kind {
        WorkCardKind::TurnWorker => {
            let work_id = detail.work_id.clone().ok_or_else(|| {
                CardActionError::NotActionable("turn worker card missing work_id".to_string())
            })?;
            turn_worker_store()
                .archive(&work_id, purge_output)
                .ok_or(CardActionError::NotFound)?;
            (
                true,
                format!("turn worker {work_id} archived"),
                None,
            )
        }
        WorkCardKind::AskJob => {
            let job_id = detail.job_id.clone().ok_or_else(|| {
                CardActionError::NotActionable("ask card missing job_id".to_string())
            })?;
            ask_job_store()
                .archive(&job_id, purge_output)
                .ok_or(CardActionError::NotFound)?;
            (
                true,
                format!("ask {job_id} archived"),
                Some(job_id),
            )
        }
        other => {
            return Err(CardActionError::NotActionable(format!(
                "archive not supported for card kind {other:?}"
            )));
        }
    };

    WorkspaceService::refresh_now(runtime.as_ref()).await;
    Ok(WorkspaceCardActionResponse {
        workspace_revision: workspace_store().revision(),
        card_id: card_id.to_string(),
        action: "archive".to_string(),
        ok,
        message,
        job_id,
        replayed: None,
        job_succeeded: None,
        associations: None,
    })
}

pub async fn retry_card(
    runtime: Arc<RuntimeComposition>,
    card_id: &str,
    worker_id: &str,
) -> Result<WorkspaceCardActionResponse, CardActionError> {
    let card_id = card_id.trim();
    if card_id.is_empty() {
        return Err(CardActionError::NotActionable(
            "card_id is required".to_string(),
        ));
    }

    let detail = resolve_card_detail(runtime.clone(), card_id).await?;
    if detail.kind != WorkCardKind::StasisJob {
        return Err(CardActionError::NotActionable(
            "retry is only supported for Stasis job cards".to_string(),
        ));
    }

    let job_id = detail.job_id.clone().ok_or_else(|| {
        CardActionError::NotActionable("job card missing job_id".to_string())
    })?;

    let replay = replay_runtime_job(runtime.clone(), &job_id, worker_id)
        .await
        .map_err(|err| CardActionError::Internal(err.to_string()))?;

    WorkspaceService::refresh_now(runtime.as_ref()).await;
    Ok(WorkspaceCardActionResponse {
        workspace_revision: workspace_store().revision(),
        card_id: card_id.to_string(),
        action: "retry".to_string(),
        ok: replay.replayed,
        message: replay.message.clone(),
        job_id: Some(replay.job_id),
        replayed: Some(replay.replayed),
        job_succeeded: Some(replay.job_succeeded),
        associations: None,
    })
}

pub async fn replay_runtime_job(
    runtime: Arc<RuntimeComposition>,
    job_id: &str,
    worker_id: &str,
) -> anyhow::Result<ReplayAndResumeResponse> {
    let job_id = job_id.trim().to_string();
    if job_id.is_empty() {
        anyhow::bail!("job_id is required");
    }

    let replayed = turn_continuation::replay_dead_letter_job(runtime.as_ref(), &job_id).await?;
    if !replayed {
        return Ok(ReplayAndResumeResponse {
            job_id,
            replayed: false,
            job_succeeded: false,
            agent_turn_resumed: false,
            message: "job is not in dead_letter state or does not exist".to_string(),
        });
    }

    let sdk = RuntimeSdk::new(runtime.as_ref().clone());
    for _ in 0..8 {
        let _ = sdk.process_once("default", worker_id).await;
        if job_succeeded(runtime.as_ref(), &job_id).await {
            break;
        }
    }
    let _ = sdk.publish_pending_events(50).await;

    let succeeded = job_succeeded(runtime.as_ref(), &job_id).await;
    let message = if !succeeded {
        "job replayed from dead_letter but did not reach succeeded state".to_string()
    } else {
        "job replayed and succeeded".to_string()
    };

    Ok(ReplayAndResumeResponse {
        job_id,
        replayed: true,
        job_succeeded: succeeded,
        agent_turn_resumed: false,
        message,
    })
}

pub fn link_vault_card(
    card_id: &str,
    vault_path: &str,
) -> Result<WorkspaceCardActionResponse, CardActionError> {
    let card_id = card_id.trim();
    if card_id.is_empty() {
        return Err(CardActionError::NotActionable(
            "card_id is required".to_string(),
        ));
    }

    let path = normalize_vault_path(vault_path)
        .map_err(|err| CardActionError::NotActionable(err.to_string()))?;
    if !crate::vault::store::vault_store().note_exists(&path) {
        return Err(CardActionError::NotActionable(format!(
            "vault note not found: {path}"
        )));
    }
    workspace_store().set_vault_association(card_id, path.clone());

    if let Some(event) = event_for_vault_link(card_id, &path) {
        workspace_store().append_event(event);
    } else {
        workspace_store().bump_revision();
    }

    let associations = workspace_store().associations(card_id);
    Ok(WorkspaceCardActionResponse {
        workspace_revision: workspace_store().revision(),
        card_id: card_id.to_string(),
        action: "link-vault".to_string(),
        ok: true,
        message: format!("linked vault note {path}"),
        job_id: None,
        replayed: None,
        job_succeeded: None,
        associations: Some(associations),
    })
}

async fn resolve_card_detail(
    runtime: Arc<RuntimeComposition>,
    card_id: &str,
) -> Result<WorkCardDetail, CardActionError> {
    WorkspaceService::get_card_detail(runtime, card_id)
        .await
        .map_err(|err| CardActionError::Internal(err.to_string()))?
        .ok_or(CardActionError::NotFound)
}

async fn get_job(
    runtime: &RuntimeComposition,
    job_id: &str,
) -> Result<Option<Job>, stasis::domain::errors::StasisError> {
    match runtime {
        RuntimeComposition::InMemory(rt) => rt.job_store.get(job_id).await,
        RuntimeComposition::Surreal(rt) => rt.job_store.get(job_id).await,
    }
}

async fn save_job(runtime: &RuntimeComposition, job: Job) -> Result<(), stasis::domain::errors::StasisError> {
    match runtime {
        RuntimeComposition::InMemory(rt) => rt.job_store.save(job).await,
        RuntimeComposition::Surreal(rt) => rt.job_store.save(job).await,
    }
}

async fn job_succeeded(runtime: &RuntimeComposition, job_id: &str) -> bool {
    get_job(runtime, job_id)
        .await
        .ok()
        .flatten()
        .is_some_and(|job| job.state == JobState::Succeeded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_runtime::turn_worker::TurnWorkRecord;

    #[test]
    fn normalize_vault_path_rejects_traversal() {
        assert!(normalize_vault_path("../secret").is_err());
        assert!(normalize_vault_path("journal/2026-05-30.md").is_ok());
    }

    #[test]
    fn cancel_turn_worker_marks_cancelled() {
        let store = turn_worker_store();
        let record = TurnWorkRecord {
            work_id: "work-cancel-test".to_string(),
            session_id: "sess".to_string(),
            parent_turn_correlation_id: None,
            parent_stream_turn_id: 0,
            intent: "research".to_string(),
            task_prompt: "task".to_string(),
            status: TurnWorkStatus::Running,
            result_text: None,
            tool_names: vec![],
            termination_reason: None,
            error: None,
            user_ack: "run".to_string(),
            provider: "openai".to_string(),
            model: "gpt".to_string(),
            response_depth_mode: "normal".to_string(),
            max_tool_rounds: 4,
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
        store.insert(record);

        let (ok, _, _) = cancel_turn_worker("work-cancel-test").expect("cancel");
        assert!(ok);
        let updated = store.get("work-cancel-test").expect("record");
        assert_eq!(updated.status, TurnWorkStatus::Cancelled);
    }

    #[test]
    fn link_vault_persists_association() {
        let note_path = format!("journal/link-test-{}.md", uuid::Uuid::new_v4().simple());
        let request = crate::daemon_api::VaultWriteRequest {
            path: Some(note_path.clone()),
            content: "# Link test\n".to_string(),
        };
        crate::vault::VaultService::write_note(Some(&note_path), &request, None).expect("seed");

        let card_id = format!("card-link-{}", uuid::Uuid::new_v4().simple());
        let response = link_vault_card(&card_id, &note_path).expect("link");
        assert!(response.ok);
        let assoc = workspace_store().associations(&card_id);
        assert_eq!(assoc.vault_paths, vec![note_path]);
    }
}
