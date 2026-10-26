//! Native Coder adapter: independent Forge-fenced turns and native receipts.
use super::*;
use crate::agent_runtime::coder_tools::CoderExecutionGuard;
use stasis::prelude::StasisError;

struct NativeCoderGuard {
    host: LocalPeerDispatcher,
    request: ExternalPeerAssignmentRequest,
}

impl CoderExecutionGuard for NativeCoderGuard {
    fn allows_worker_spawns(&self) -> bool {
        false
    }
    fn admission_service(&self) -> Option<Arc<medousa_forge::execution::ForgeExecutionService>> {
        Some(self.host.state.forge_execution.clone())
    }
    fn verify(&self) -> stasis::prelude::Result<()> {
        let verify = || -> Result<()> {
            self.host
                .store
                .require_assignment_grant(&self.request, Utc::now())?;
            self.host
                .store
                .require_owner(&self.request.channel, &self.request.owner_principal_id)?;
            if !crate::session_catalog::session_visible_to_profile(
                self.request.owner_session.session_id.as_str(),
                &self.request.owner_principal_id,
            ) || self.request.context.sources.iter().any(|source| {
                !crate::session_catalog::session_visible_to_profile(
                    source.selection.session.session_id.as_str(),
                    &self.request.owner_principal_id,
                )
            }) {
                bail!("native Coder source context is no longer visible");
            }
            let work = self
                .host
                .state
                .forge
                .load(&medousa_forge::model::WorkId::from(
                    self.request.forge_work_id.clone(),
                ))?;
            if work.owner != self.request.owner_principal_id {
                bail!("native Coder project ownership changed");
            }
            if let Some(plan) = self.host.store.work_plan_for_assignment(&self.request)? {
                let host = crate::daemon::work_units::local_work_unit_host()
                    .ok_or_else(|| anyhow::anyhow!("work coordination unavailable"))?;
                host.peer_stage_context(
                    &self.host.store,
                    &self.host.state.forge,
                    &plan,
                    &self.request,
                )?;
            }
            Ok(())
        };
        verify().map_err(|error| StasisError::PortFailure(error.to_string()))
    }
}

impl LocalPeerDispatcher {
    pub(crate) async fn coding_preferences(&self) -> Result<CodingRuntimePreferences> {
        self.state
            .forge_execution
            .run(ExecutionClass::StoreIo, 64 * 1024, || {
                Ok((|| -> Result<_> {
                    let preferences = crate::session::load_tui_defaults()
                        .coding_runtime
                        .unwrap_or_default();
                    preferences.validate().map_err(anyhow::Error::msg)?;
                    Ok(preferences)
                })())
            })
            .await?
    }

    pub(crate) async fn native_coder_contract(
        &self,
        turn_id: &str,
        session_id: &str,
        owner: &str,
        work_id: Option<&str>,
    ) -> Result<Option<(Arc<dyn CoderExecutionGuard>, PeerReceiptSink)>> {
        if !turn_id.starts_with("medousa_coder_") {
            return Ok(None);
        }
        let store = self.store.clone();
        let authority = crate::workshop_authority::current()
            .map_err(anyhow::Error::msg)?
            .clone();
        let id = turn_id.to_string();
        let request = self
            .state
            .forge_execution
            .run(ExecutionClass::StoreIo, MAX_CONTEXT_BYTES, move || {
                Ok(store.native_coder_request(&authority, &id))
            })
            .await??;
        if request.execution_session.session_id.as_str() != session_id
            || request.owner_principal_id != owner
            || work_id != Some(request.forge_work_id.as_str())
        {
            bail!("native Coder turn does not match its admitted assignment");
        }
        self.hydrate(&RequestPrincipal::worker(owner.to_string()), &request, true)
            .await?;
        let binding = native_binding(&request);
        Ok(Some((
            Arc::new(NativeCoderGuard {
                host: self.clone(),
                request,
            }),
            PeerReceiptSink {
                host: self.clone(),
                binding,
            },
        )))
    }

    pub(super) async fn assign_native_coder(
        &self,
        principal: &RequestPrincipal,
        request: &ExternalPeerAssignmentRequest,
        mut prompt: String,
    ) -> Result<ExternalPeerAssignmentBinding> {
        if request.existing_agent_session_id.is_some() {
            bail!("native Coder requires a fresh assignment");
        }
        let binding = native_binding(request);
        let store = self.store.clone();
        let saved = request.clone();
        let saved_binding = binding.clone();
        let forge = self.state.forge.clone();
        let turn_id = binding.agent_session_id.clone();
        self.state
            .forge_execution
            .run(ExecutionClass::StoreIo, MAX_CONTEXT_BYTES, move || {
                Ok((|| -> Result<()> {
                    // Reject known lifecycle blockers before recording custody or
                    // advertising a running peer. The real lease acquisition still
                    // checks state atomically, so this does not bypass fencing.
                    let work = forge.load(&medousa_forge::model::WorkId::from(
                        saved.forge_work_id.clone(),
                    ))?;
                    validate_native_work_state(work.state)?;
                    store.record_native_coder(&turn_id, &saved)?;
                    // Native custody is committed before a fast turn can finish.
                    store.record_peer(&saved_binding)?;
                    crate::session_catalog::ensure_named_session_for_profile(
                        saved.execution_session.session_id.as_str(),
                        Some("Medousa Coder".into()),
                        &saved.owner_principal_id,
                    )
                    .map_err(anyhow::Error::msg)?;
                    Ok(())
                })())
            })
            .await??;
        let launch = async {
            let config = super::super::ingest::resolve_session_runtime_config(
                &self.state,
                request.owner_session.session_id.as_str(),
            )
            .await;
            prompt.push_str(concat!(
                "\n\nNative assignment execution policy: complete this work in the current admitted Coder turn. ",
                "Do not spawn child workers or use turn.begin_work; their assignment grant is not inherited. ",
                "Additional parallel work requires separate scoped assignments from the coordinator.",
            ));
            let execution_session = request.execution_session.session_id.to_string();
            let mut turn = self
                .state
                .forge_execution
                .run(ExecutionClass::StoreIo, MAX_CONTEXT_BYTES, move || {
                    Ok(
                        crate::session_mapping::build_interactive_turn_request_for_ingest(
                            &execution_session,
                            prompt,
                            &config.draft_provider,
                            &config.draft_model,
                            &config.response_depth_mode,
                            &config.reasoning_effort,
                            None,
                            None,
                            None,
                            None,
                        ),
                    )
                })
                .await?;
            turn.persist_user_turn = false;
            turn.identity_user_id = Some(request.owner_principal_id.clone());
            turn.agent_mode = Some(medousa_types::AgentModeId::Coder);
            turn.code_context = Some(medousa_types::CodeIntentContext {
                work_id: Some(request.forge_work_id.clone()),
                ..Default::default()
            });
            self.hydrate(principal, request, true).await?;
            super::super::interactive::spawn_turn_ticket(
                &self.state,
                RequestPrincipal::worker(request.owner_principal_id.clone()),
                binding.agent_session_id.clone(),
                crate::turn_ticket::TurnTicketMode::Interactive,
                turn,
                None,
                None,
            )
            .await
            .map_err(|(_, message)| anyhow::anyhow!(message))?;
            Ok::<_, anyhow::Error>(())
        }
        .await;
        if let Err(error) = launch {
            let failure = crate::turn_failure::TurnFailure::from_debug(&format!("{error:#}"));
            let diagnostic =
                crate::agent_runtime::coder_evidence::redact_evidence_text(&failure.debug_message);
            tracing::warn!(
                target: "medousa::turn",
                turn_id = %binding.agent_session_id,
                session_id = %request.execution_session.session_id,
                category = failure.category_label(),
                retryable = failure.retryable,
                debug_message = %crate::text_budget::truncate_text_for_budget(&diagnostic, 4096),
                "native_coder_launch_failed"
            );
            let result = terminal_result(
                medousa_types::TurnCompletionOutcomeV3::Failed,
                "",
                Some(&failure.operator_message),
                &binding.agent_session_id,
                request.execution_session.session_id.as_str(),
            );
            PeerReceiptSink {
                host: self.clone(),
                binding: binding.clone(),
            }
            .terminal(PeerAssignmentOutcome::Interrupted, result.clone())
            .await?;
            return Err(anyhow::anyhow!(result));
        }
        Ok(binding)
    }
}

fn validate_native_work_state(state: medousa_forge::model::WorkState) -> Result<()> {
    use medousa_forge::model::WorkState;
    if !matches!(state, WorkState::Ready | WorkState::Executing) {
        let failure = crate::turn_failure::TurnFailure::from_debug(&format!(
            "invalid transition: work is {state}, cannot begin attempt"
        ));
        bail!("{}", failure.operator_message);
    }
    Ok(())
}

fn native_binding(request: &ExternalPeerAssignmentRequest) -> ExternalPeerAssignmentBinding {
    ExternalPeerAssignmentBinding {
        assignment_id: request.assignment_id.clone(),
        owner_principal_id: request.owner_principal_id.clone(),
        channel: request.channel.clone(),
        target: request.target.clone(),
        execution_session: request.execution_session.clone(),
        agent_session_id: format!("medousa_coder_{}", request.assignment_id),
    }
}

/// Keep raw errors out of coordinator/model context, but never return an
/// untraceable generic failure. The reference joins to the daemon diagnostic.
pub(crate) fn terminal_result(
    outcome: medousa_types::TurnCompletionOutcomeV3,
    aggregate_text: &str,
    operator_message: Option<&str>,
    turn_id: &str,
    session_id: &str,
) -> String {
    if matches!(
        outcome,
        medousa_types::TurnCompletionOutcomeV3::Failed
            | medousa_types::TurnCompletionOutcomeV3::Fatal
            | medousa_types::TurnCompletionOutcomeV3::FuseExhausted
    ) {
        // A failure without explicit operator copy must not expose partial
        // model output or a raw provider error as its explanation.
        let fallback = crate::turn_failure::TurnFailure::from_debug(aggregate_text);
        let message = operator_message
            .filter(|message| !message.trim().is_empty())
            .unwrap_or(&fallback.operator_message);
        return format!(
            "{message}\nFailure reference: turn_id={turn_id} session_id={session_id}. Inspect this turn's error details or the matching daemon diagnostic before retrying."
        );
    }
    operator_message.unwrap_or(aggregate_text).to_string()
}

pub(crate) fn terminal_outcome(
    outcome: medousa_types::TurnCompletionOutcomeV3,
) -> PeerAssignmentOutcome {
    match outcome {
        medousa_types::TurnCompletionOutcomeV3::Completed => PeerAssignmentOutcome::Completed,
        medousa_types::TurnCompletionOutcomeV3::Cancelled => PeerAssignmentOutcome::Cancelled,
        medousa_types::TurnCompletionOutcomeV3::Checkpointed
        | medousa_types::TurnCompletionOutcomeV3::NeedsInput => PeerAssignmentOutcome::Interrupted,
        _ => PeerAssignmentOutcome::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use medousa_types::TurnCompletionOutcomeV3 as Outcome;

    #[test]
    fn native_admission_rejects_non_executable_states() {
        use medousa_forge::model::WorkState;
        for state in [WorkState::Ready, WorkState::Executing] {
            assert!(validate_native_work_state(state).is_ok());
        }
        for state in [
            WorkState::Draft,
            WorkState::Provisioning,
            WorkState::Sealing,
            WorkState::AwaitingReview,
            WorkState::ApplyingDecision,
            WorkState::Accepted,
            WorkState::Discarded,
            WorkState::Failed,
        ] {
            assert!(validate_native_work_state(state).is_err(), "{state}");
        }
        assert!(
            validate_native_work_state(WorkState::Accepted)
                .unwrap_err()
                .to_string()
                .contains("already accepted")
        );
    }

    #[test]
    fn failed_receipt_without_operator_copy_never_exposes_raw_output() {
        let result = terminal_result(
            Outcome::Failed,
            "private partial output secret=do-not-surface",
            None,
            "turn-test",
            "session-test",
        );
        assert!(!result.contains("private partial output"));
        assert!(!result.contains("do-not-surface"));
        assert!(result.contains("turn_id=turn-test"));
        let failure = crate::turn_failure::TurnFailure::from_debug(
            "invalid transition: work work-test is accepted, cannot begin attempt",
        );
        let result = terminal_result(
            Outcome::Failed,
            "",
            Some(&failure.operator_message),
            "turn-test",
            "session-test",
        );
        assert!(result.contains("already accepted"));
        assert!(result.contains("new undertaking"));
    }

    #[test]
    fn failed_native_receipts_include_a_traceable_reference() {
        for outcome in [Outcome::Failed, Outcome::Fatal, Outcome::FuseExhausted] {
            let result = terminal_result(
                outcome,
                "partial work",
                Some("Medousa couldn't complete this turn."),
                "turn-test",
                "session-test",
            );
            assert!(result.contains("Medousa couldn't complete"));
            assert!(result.contains("turn_id=turn-test session_id=session-test"));
            assert!(!result.contains("partial work"));
        }
        assert_eq!(
            terminal_result(
                Outcome::Completed,
                "done",
                None,
                "turn-test",
                "session-test"
            ),
            "done"
        );
        assert_eq!(
            terminal_result(
                Outcome::NeedsInput,
                "question",
                None,
                "turn-test",
                "session-test"
            ),
            "question"
        );
    }

    #[test]
    fn only_completed_native_turns_satisfy_the_executor_stage() {
        assert_eq!(
            terminal_outcome(Outcome::Completed),
            PeerAssignmentOutcome::Completed
        );
        for outcome in [Outcome::Checkpointed, Outcome::NeedsInput] {
            assert_eq!(
                terminal_outcome(outcome),
                PeerAssignmentOutcome::Interrupted
            );
        }
        for outcome in [Outcome::Fatal, Outcome::Failed, Outcome::FuseExhausted] {
            assert_eq!(terminal_outcome(outcome), PeerAssignmentOutcome::Failed);
        }
        assert_eq!(
            terminal_outcome(Outcome::Cancelled),
            PeerAssignmentOutcome::Cancelled
        );
    }
}
