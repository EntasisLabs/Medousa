//! Replaceable per-inference model view and payload-free principal projection.
//! Snapshots live on the request copy, never in the authoritative transcript.
use crate::context::{ContextCoordinator, ContextDecision, ContextDecisionReport};
use crate::inference_window::{
    ContextWindowOverflow, InferenceWindowPolicy, InferenceWindowReport, estimate,
};
use crate::perception::ToolPerceptionGovernor;
use genai::chat::{ChatMessage, ChatRequest, ChatRole};
use medousa_types::context_hud::{UserContextProjection, WorkContextProjection};

pub const MODEL_HUD_PREFIX: &str = "[MEDOUSA_CONTEXT_HUD]";
const HUD_RESERVE_BYTES: usize = 768;

fn pressure(report: &InferenceWindowReport) -> &'static str {
    if report.estimate.total_tokens > report.input_budget {
        "overflow"
    } else if report.estimate.total_tokens.saturating_mul(100)
        > report.input_budget.saturating_mul(85)
    {
        "high"
    } else {
        "normal"
    }
}

pub fn user_projection(
    report: &InferenceWindowReport,
    work: Option<WorkContextProjection>,
) -> UserContextProjection {
    UserContextProjection {
        work,
        pressure: pressure(report).into(),
        deferred_items: report
            .decisions
            .contributions
            .iter()
            .filter(|c| c.id != "model_context_hud")
            .filter(|c| {
                matches!(
                    c.decision,
                    ContextDecision::Omitted {
                        reason: crate::context::ContextOmissionReason::BudgetExcluded,
                        ..
                    } | ContextDecision::Excluded { .. }
                )
            })
            .count(),
        accounting: report.accounting.clone(),
    }
}

fn meaningful_changes(
    report: &InferenceWindowReport,
    view: &UserContextProjection,
    previous: Option<&InferenceWindowReport>,
) -> Vec<String> {
    let Some(previous) = previous else {
        return vec!["initial_snapshot".into()];
    };
    let mut changes = Vec::new();
    if previous.hud.as_ref().and_then(|h| h.work.as_ref()) != view.work.as_ref() {
        changes.push("authoritative_work_changed".into());
    }
    if previous.deferred_history_messages != report.deferred_history_messages
        || previous.deferred_pointers != report.deferred_pointers
    {
        changes.push("history_selection_changed".into());
    }
    if previous.hud.as_ref().map(|h| h.pressure.as_str()) != Some(view.pressure.as_str()) {
        changes.push("pressure_band_changed".into());
    }
    changes
}

fn model_snapshot(report: &InferenceWindowReport, view: &UserContextProjection) -> String {
    // Fixed bounded identifiers/counts, never evidence, user prose or a timestamp.
    let work = view.work.as_ref().map(|w| {
        serde_json::json!({
            "focus": w.work_id,
            "phase": w.phase,
            "progress": w.progress,
            "blockers": w.blockers, "decision_accounting": w.decisions,
        })
    });
    let route = report.deferred_pointers.first().map(|p| &p.retrieval);
    let snapshot = serde_json::json!({
        "work": work, "pressure": view.pressure,
        "deferred_items": view.deferred_items,
        "deferred_history_messages": report.deferred_history_messages,
        "deferred_retrieval": route,
        "meaningful_changes": report.meaningful_changes,
        "task_success": "not_inferred",
        "accounting": "conservative_estimate_not_provider_tokenization",
    });
    let text = format!("{MODEL_HUD_PREFIX}\n{snapshot}");
    // Producer IDs are normally bounded. On a legacy oversized route/progress,
    // omit optional details, never slice a retrieval ID into a fabricated route.
    if text.len() <= HUD_RESERVE_BYTES {
        text
    } else {
        format!(
            "{MODEL_HUD_PREFIX}\n{}",
            serde_json::json!({
                "work": null, "pressure": view.pressure, "deferred_items": view.deferred_items,
                "details": "oversized; available in compiled report and native domain retrieval",
                "task_success": "not_inferred",
            })
        )
    }
}

impl ContextCoordinator {
    /// Reserve a bounded replaceable tail before selection, then reconcile the
    /// rendered snapshot. No hidden append after budgeting, no stable-prefix churn.
    #[allow(clippy::too_many_arguments)]
    pub fn compile_inference_with_hud(
        request: &mut ChatRequest,
        policy: &InferenceWindowPolicy,
        system_messages: usize,
        prefix_messages: usize,
        assembly: Option<&ContextDecisionReport>,
        perception: &mut ToolPerceptionGovernor,
        work: Option<WorkContextProjection>,
        previous: Option<&InferenceWindowReport>,
    ) -> Result<InferenceWindowReport, ContextWindowOverflow> {
        // Only runtime-owned system snapshots in the volatile tail are replaced.
        let prefix_end = system_messages.saturating_add(prefix_messages);
        let mut i = 0;
        request.messages.retain(|m| {
            let tail = i >= prefix_end;
            i += 1;
            !(tail
                && m.role == ChatRole::System
                && m.content
                    .first_text()
                    .is_some_and(|t| t.starts_with(MODEL_HUD_PREFIX)))
        });
        request.messages.push(ChatMessage::system(format!(
            "{MODEL_HUD_PREFIX}\n{}",
            " ".repeat(HUD_RESERVE_BYTES - MODEL_HUD_PREFIX.len() - 1)
        )));
        let original = request.clone();
        let mut report = match Self::compile_inference(
            request,
            policy,
            system_messages,
            prefix_messages,
            assembly,
            perception,
        ) {
            Ok(report) => report,
            Err(_) => {
                // HUD is flexible. It must never cause a protected-material
                // overflow that would not occur without it.
                *request = original;
                request.messages.pop();
                let mut report = Self::compile_inference(
                    request,
                    policy,
                    system_messages,
                    prefix_messages,
                    assembly,
                    perception,
                )?;
                report
                    .decisions
                    .contributions
                    .push(crate::context::ContextContribution {
                        id: "model_context_hud".into(),
                        owner: crate::context::ContextOwner::Host,
                        provenance: crate::context::ContextProvenance {
                            source: "compiled_decision_report".into(),
                            scope: None,
                        },
                        retention: crate::context::Retention::Flexible,
                        decision: ContextDecision::Omitted {
                            reason: crate::context::ContextOmissionReason::BudgetExcluded,
                            source_text_chars: None,
                        },
                        cost: Default::default(),
                        budget_chars: None,
                    });
                let view = user_projection(&report, work);
                report.meaningful_changes = meaningful_changes(&report, &view, previous);
                report.hud = Some(view);
                return Ok(report);
            }
        };
        let view = user_projection(&report, work);
        report.meaningful_changes = meaningful_changes(&report, &view, previous);
        let mut snapshot = model_snapshot(&report, &view);
        // Keep the reserved byte footprint stable, so the model and user view
        // share the same pressure decision and the final guard sees no growth.
        snapshot.push_str(&" ".repeat(HUD_RESERVE_BYTES.saturating_sub(snapshot.len())));
        *request.messages.last_mut().expect("reserved HUD") = ChatMessage::system(snapshot);
        let omitted = report
            .decisions
            .contributions
            .iter()
            .filter(|c| c.id == "window_deferred_chat_history")
            .cloned()
            .collect::<Vec<_>>();
        let cost = crate::inference_usage::RequestFootprint::new(request, None, None)
            .content_cost
            .expect("request cost");
        let compiled = crate::request_context::compile_first_request(
            request,
            system_messages,
            prefix_messages.saturating_sub(report.deferred_history_messages),
            assembly,
            cost,
        );
        report.decisions = compiled.decisions;
        report.decisions.capacity.input_tokens = policy.input_tokens;
        report.decisions.contributions.extend(omitted);
        report.estimate = estimate(request);
        report.reconciled = compiled.reconciled;
        report.request_messages_digest =
            crate::context::prepared_messages_digest(&request.messages);
        report.hud = Some(user_projection(&report, view.work));
        debug_assert!(report.estimate.total_tokens <= report.input_budget);
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn semantic_deltas_are_coalesced_and_hud_can_defer_without_protected_overflow() {
        let mut request = ChatRequest::new(vec![
            ChatMessage::system("policy"),
            ChatMessage::user("intent"),
        ]);
        let base = estimate(&request).total_tokens;
        let policy = InferenceWindowPolicy {
            input_tokens: Some((base + 40) as u32),
            output_reserve: 0,
            safety_tokens: 0,
            observation_tokens: 0,
            ..Default::default()
        };
        let mut perception = ToolPerceptionGovernor::default();
        let first = ContextCoordinator::compile_inference_with_hud(
            &mut request,
            &policy,
            1,
            1,
            None,
            &mut perception,
            None,
            None,
        )
        .unwrap();
        assert_eq!(request.messages.len(), 2);
        assert!(
            first
                .decisions
                .contributions
                .iter()
                .any(|c| c.id == "model_context_hud"
                    && matches!(c.decision, ContextDecision::Omitted { .. }))
        );
        assert_eq!(first.hud.as_ref().unwrap().deferred_items, 0);
        let next = ContextCoordinator::compile_inference_with_hud(
            &mut request,
            &policy,
            1,
            1,
            None,
            &mut perception,
            None,
            Some(&first),
        )
        .unwrap();
        assert!(next.meaningful_changes.is_empty());
        let work = WorkContextProjection {
            work_id: "work-exact".into(),
            phase: "executing".into(),
            ..Default::default()
        };
        let changed = ContextCoordinator::compile_inference_with_hud(
            &mut request,
            &policy,
            1,
            1,
            None,
            &mut perception,
            Some(work),
            Some(&next),
        )
        .unwrap();
        assert_eq!(
            changed.meaningful_changes,
            vec!["authoritative_work_changed"]
        );
    }

    #[test]
    fn replacement_preserves_prefix_accounts_snapshot_and_never_completes_work() {
        let mut request = ChatRequest::new(vec![
            ChatMessage::system("policy"),
            ChatMessage::user("current"),
        ]);
        let mut perception = ToolPerceptionGovernor::default();
        let policy = InferenceWindowPolicy {
            input_tokens: Some(8000),
            shared_tokens: None,
            ..Default::default()
        };
        let work = WorkContextProjection {
            work_id: "work-1".into(),
            phase: "executing".into(),
            ..Default::default()
        };
        let report = ContextCoordinator::compile_inference_with_hud(
            &mut request,
            &policy,
            1,
            1,
            None,
            &mut perception,
            Some(work.clone()),
            None,
        )
        .unwrap();
        let first = request.messages.clone();
        let again = ContextCoordinator::compile_inference_with_hud(
            &mut request,
            &policy,
            1,
            1,
            None,
            &mut perception,
            Some(work),
            None,
        )
        .unwrap();
        assert_eq!(request.messages.len(), 3);
        let snapshot = request
            .messages
            .last()
            .unwrap()
            .content
            .first_text()
            .unwrap()
            .strip_prefix(MODEL_HUD_PREFIX)
            .unwrap();
        let snapshot: serde_json::Value = serde_json::from_str(snapshot).unwrap();
        assert_eq!(
            snapshot["work"]["decision_accounting"],
            serde_json::json!([])
        );
        assert_eq!(snapshot["work"]["blockers"], serde_json::json!([]));
        assert_eq!(
            crate::context::prepared_messages_digest(&first),
            crate::context::prepared_messages_digest(&request.messages)
        );
        assert!(again.reconciled);
        assert_eq!(again.estimate, estimate(&request));
        assert_eq!(
            report.hud.as_ref().unwrap().work.as_ref().unwrap().phase,
            "executing"
        );
        assert!(
            !serde_json::to_string(&report.hud)
                .unwrap()
                .contains("completed")
        );
        let mut fluctuation = report.hud.clone().unwrap();
        fluctuation.pressure = "high".into();
        fluctuation.accounting = "different estimate".into();
        assert!(!fluctuation.meaningful_change_from(report.hud.as_ref().unwrap()));
        fluctuation
            .work
            .as_mut()
            .unwrap()
            .blockers
            .push("awaiting_authority".into());
        assert!(fluctuation.meaningful_change_from(report.hud.as_ref().unwrap()));
    }
}
