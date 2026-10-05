//! Bounded observations of exact assignment custody. Execution observations
//! are advisory; a receipt remains the sole assignment outcome authority.
use super::*;
use crate::agent_runtime::coder_activity::{
    CoderActivityEvent, CoderActivityKind, CoderAgentPresence,
};
use medousa_forge::model::{Attempt, AttemptState, WorkId};
use medousa_types::turn_ticket::TurnTicketPhase;

impl LocalPeerDispatcher {
    pub(super) async fn assignment_progress(
        &self,
        row: &PeerProposalReviewRecord,
    ) -> Result<PeerAssignmentProgress> {
        let binding = row.binding.as_ref().expect("assigned proposal");
        let request = &row.proposal.request;
        let now = Utc::now();
        let mut progress = PeerAssignmentProgress {
            state: PeerExecutionState::Unobserved,
            observed_at: now,
            last_activity_at: None,
            current_activity: None,
            last_activity: None,
            last_activity_status: None,
        };
        if binding.assignment_id != request.assignment_id
            || binding.owner_principal_id != request.owner_principal_id
            || binding.channel != request.channel
            || binding.target != request.target
            || binding.execution_session != request.execution_session
        {
            return Ok(progress);
        }
        let native = request.target.runtime == ExternalPeerRuntime::Medousa;
        let ticket = if native {
            crate::turn_ticket::get_turn(&self.state.turn_tickets, &binding.agent_session_id)
                .await
                .filter(|ticket| ticket.session_id == binding.execution_session.session_id.as_str())
        } else {
            None
        };
        let live_state = if native {
            ticket.as_ref().map(|ticket| match ticket.phase {
                TurnTicketPhase::Accepted => PeerExecutionState::Accepted,
                TurnTicketPhase::BudgetBlocked => PeerExecutionState::Blocked,
                TurnTicketPhase::Done | TurnTicketPhase::Error | TurnTicketPhase::Cancelled => {
                    PeerExecutionState::AwaitingReceipt
                }
                _ => PeerExecutionState::Running,
            })
        } else {
            super::super::agents::peer_execution_state(binding, &request.forge_work_id).await
        };
        let forge = self.state.forge.clone();
        let work_id = WorkId::from(request.forge_work_id.clone());
        let saved_binding = binding.clone();
        let observations = self
            .state
            .forge_execution
            .run(
                ExecutionClass::StoreIo,
                medousa_forge::execution::MAX_STORE_PAYLOAD_BYTES,
                move || {
                    let work = forge
                        .load(&work_id)
                        .ok()
                        .filter(|work| work.owner == saved_binding.owner_principal_id);
                    let Some(work) = work else {
                        return Ok(None);
                    };
                    let attempt = work
                        .attempts
                        .iter()
                        .rev()
                        .find(|attempt| matches_attempt(attempt, &saved_binding))
                        .cloned();
                    let activity = if native {
                        crate::agent_runtime::coder_activity::coder_activity_store()
                            .activity_for_turn(
                                work_id.as_str(),
                                saved_binding.execution_session.session_id.as_str(),
                                &saved_binding.agent_session_id,
                            )
                            .ok()
                            .flatten()
                            .filter(|(presence, _)| {
                                attempt.as_ref().is_some_and(|attempt| {
                                    presence.attempt_id == attempt.id.as_str()
                                })
                            })
                    } else {
                        None
                    };
                    Ok(Some((attempt, activity)))
                },
            )
            .await?;
        let Some((attempt, activity)) = observations else {
            return Ok(progress);
        };
        progress.state = execution_state(live_state, attempt.as_ref().map(|attempt| attempt.state));
        progress.last_activity_at = ticket.as_ref().map(|ticket| ticket.updated_at);
        if let Some(attempt) = attempt {
            let at = attempt
                .ended_at
                .or_else(|| attempt.lease.map(|lease| lease.heartbeat_at))
                .unwrap_or(attempt.started_at);
            progress.last_activity_at =
                Some(progress.last_activity_at.map_or(at, |seen| seen.max(at)));
        }
        if let Some((presence, event)) = activity {
            apply_activity(&mut progress, &presence, event.as_ref());
        }
        Ok(progress)
    }
}

fn matches_attempt(attempt: &Attempt, binding: &ExternalPeerAssignmentBinding) -> bool {
    let detail = &attempt.executor.detail;
    if binding.target.runtime == ExternalPeerRuntime::Medousa {
        attempt.executor.kind == "medousa-coder"
            && detail.get("session_id").and_then(serde_json::Value::as_str)
                == Some(binding.execution_session.session_id.as_str())
            && detail.get("turn_id").and_then(serde_json::Value::as_str)
                == Some(binding.agent_session_id.as_str())
    } else {
        attempt.executor.kind == format!("acp-{}", binding.target.runtime.as_str())
            && detail
                .get("chat_session_id")
                .and_then(serde_json::Value::as_str)
                == Some(binding.execution_session.session_id.as_str())
            && detail
                .get("agent_session_id")
                .and_then(serde_json::Value::as_str)
                == Some(binding.agent_session_id.as_str())
    }
}

fn execution_state(
    live: Option<PeerExecutionState>,
    attempt: Option<AttemptState>,
) -> PeerExecutionState {
    live.unwrap_or(match attempt {
        Some(AttemptState::Completed | AttemptState::Failed | AttemptState::Interrupted) => {
            PeerExecutionState::AwaitingReceipt
        }
        _ => PeerExecutionState::Unobserved,
    })
}

fn bounded_activity(text: &str) -> String {
    text.chars().take(320).collect()
}

fn apply_activity(
    progress: &mut PeerAssignmentProgress,
    presence: &CoderAgentPresence,
    event: Option<&CoderActivityEvent>,
) {
    let at = presence.heartbeat_at_utc;
    progress.last_activity_at = Some(progress.last_activity_at.map_or(at, |seen| seen.max(at)));
    if presence.active && progress.state == PeerExecutionState::Running {
        progress.current_activity = presence.current_intent.as_deref().map(bounded_activity);
    }
    progress.last_activity = event
        .and_then(|event| event.intent.as_deref())
        .or(presence.last_intent.as_deref())
        .map(bounded_activity);
    progress.last_activity_status = event.and_then(|event| match event.kind {
        CoderActivityKind::ToolPlanned => Some(PeerActivityStatus::Running),
        // Older ledgers recorded a successful tool return as ToolCompleted
        // even when the returned action had ok:false. Truncated legacy
        // summaries are insufficient evidence of success.
        CoderActivityKind::ToolCompleted => event
            .detail
            .as_deref()
            .and_then(|detail| serde_json::from_str::<serde_json::Value>(detail).ok())
            .and_then(|output| {
                output.get("ok").and_then(serde_json::Value::as_bool)?;
                Some(
                    if crate::agent_runtime::tool_stream::tool_status_from_output(&output)
                        == "succeeded"
                    {
                        PeerActivityStatus::Succeeded
                    } else {
                        PeerActivityStatus::Failed
                    },
                )
            }),
        CoderActivityKind::ToolFailed => Some(PeerActivityStatus::Failed),
        CoderActivityKind::ToolBlocked => Some(PeerActivityStatus::Blocked),
        _ => None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn liveness_and_tool_errors_do_not_establish_terminal_outcomes() {
        assert_eq!(
            execution_state(None, Some(AttemptState::Running)),
            PeerExecutionState::Unobserved
        );
        assert_eq!(
            execution_state(None, Some(AttemptState::Failed)),
            PeerExecutionState::AwaitingReceipt
        );
        assert_eq!(
            execution_state(
                Some(PeerExecutionState::Running),
                Some(AttemptState::Failed)
            ),
            PeerExecutionState::Running
        );
        assert_eq!(
            execution_state(
                Some(PeerExecutionState::Blocked),
                Some(AttemptState::Running)
            ),
            PeerExecutionState::Blocked
        );
        assert_eq!(bounded_activity(&"界".repeat(400)).chars().count(), 320);
    }

    #[test]
    fn progress_matches_native_and_external_attempt_identity() {
        use medousa_forge::model::{AttemptId, ExecutorDescriptor};
        let authority: AuthorityId = format!("auth_{}", "a".repeat(64)).parse().unwrap();
        let mut binding = ExternalPeerAssignmentBinding {
            assignment_id: "assignment".into(),
            owner_principal_id: "owner".into(),
            channel: CoordinationChannelRef {
                authority_id: authority.clone(),
                channel_id: "channel".into(),
            },
            target: ExternalPeerTarget {
                authority_id: authority.clone(),
                execution_runtime_id: "workshop".into(),
                runtime: ExternalPeerRuntime::Medousa,
            },
            execution_session: SessionRef {
                authority_id: authority,
                session_id: "ses_executor".parse().unwrap(),
            },
            agent_session_id: "exact-turn".into(),
        };
        let mut attempt = Attempt {
            id: AttemptId::from("attempt".to_string()),
            seq: 1,
            state: AttemptState::Running,
            executor: ExecutorDescriptor {
                kind: "medousa-coder".into(),
                detail: serde_json::json!({"session_id":"ses_executor", "turn_id":"exact-turn"}),
            },
            environment: None,
            lease: None,
            recovery: None,
            evidence_id: None,
            started_at: Utc::now(),
            ended_at: None,
        };
        assert!(matches_attempt(&attempt, &binding));
        attempt.executor.detail["turn_id"] = serde_json::json!("other-turn");
        assert!(!matches_attempt(&attempt, &binding));
        binding.target.runtime = ExternalPeerRuntime::Codex;
        attempt.executor.kind = "acp-codex".into();
        attempt.executor.detail =
            serde_json::json!({"chat_session_id":"ses_executor", "agent_session_id":"exact-turn"});
        assert!(matches_attempt(&attempt, &binding));
        attempt.executor.detail["chat_session_id"] = serde_json::json!("ses_other");
        assert!(!matches_attempt(&attempt, &binding));
    }

    #[test]
    fn failed_previous_action_does_not_replace_current_work_or_expose_payloads() {
        let now = Utc::now();
        let presence: CoderAgentPresence = serde_json::from_value(serde_json::json!({
            "agent_id": "agent", "session_id": "ses_executor", "turn_id": "turn",
            "attempt_id": "attempt", "active": true, "joined_at_utc": now,
            "heartbeat_at_utc": now, "current_intent": "Repair package",
        }))
        .unwrap();
        let mut event: CoderActivityEvent = serde_json::from_value(serde_json::json!({
            "event_id": "event", "work_id": "work", "agent_id": "agent",
            "session_id": "ses_executor", "turn_id": "turn", "attempt_id": "attempt",
            "kind": "tool_failed", "occurred_at_utc": now, "intent": "Check package",
            "detail": "private tool payload",
        }))
        .unwrap();
        let mut progress: PeerAssignmentProgress = serde_json::from_value(serde_json::json!({
            "state": "running", "observed_at": now,
        }))
        .unwrap();
        apply_activity(&mut progress, &presence, Some(&event));
        assert_eq!(progress.state, PeerExecutionState::Running);
        assert_eq!(progress.current_activity.as_deref(), Some("Repair package"));
        assert_eq!(progress.last_activity.as_deref(), Some("Check package"));
        assert_eq!(
            progress.last_activity_status,
            Some(PeerActivityStatus::Failed)
        );
        assert!(
            !serde_json::to_string(&progress)
                .unwrap()
                .contains("private tool payload")
        );
        event.kind = CoderActivityKind::ToolCompleted;
        event.detail = Some(serde_json::json!({"ok":false}).to_string());
        apply_activity(&mut progress, &presence, Some(&event));
        assert_eq!(
            progress.last_activity_status,
            Some(PeerActivityStatus::Failed)
        );
        event.detail = Some(serde_json::json!({"ok":true,"error":"action failed"}).to_string());
        apply_activity(&mut progress, &presence, Some(&event));
        assert_eq!(
            progress.last_activity_status,
            Some(PeerActivityStatus::Failed)
        );
        event.detail = Some("truncated legacy summary".into());
        apply_activity(&mut progress, &presence, Some(&event));
        assert_eq!(progress.last_activity_status, None);
    }
}
