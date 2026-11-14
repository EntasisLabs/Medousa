//! Payload-free attribution of the first assembled request; no selection policy.
use std::sync::Arc;

use genai::chat::ChatRequest;
use serde::{Deserialize, Serialize};

use crate::context::{
    ContextCapacity, ContextContribution, ContextCoordinator, ContextCost, ContextDecision,
    ContextDecisionReport, ContextOwner, ContextProvenance, Retention,
};
use crate::inference_usage::RequestFootprint;

pub type RequestContextFuture = std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>;
pub type FirstRequestObserver = Arc<dyn Fn(RequestFootprint) -> RequestContextFuture + Send + Sync>;

#[derive(Clone, Default)]
pub struct FirstRequestContextConfig {
    pub assembly: Option<ContextDecisionReport>,
    pub observer: Option<FirstRequestObserver>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirstRequestContextReport {
    pub decisions: ContextDecisionReport,
    pub reconciled: bool,
    /// None for callers without an assembly seed; false leaves the actual
    /// prepared lane explicitly unreconciled instead of inheriting stale spans.
    pub prepared_provenance_preserved: Option<bool>,
    /// None distinguishes no-tools requests from an explicitly empty catalog.
    pub tool_count: Option<usize>,
    pub schema_chars: usize,
}

/// Lane offsets come from the producer, never from marker-looking user prose.
/// Only the runtime-owned tail is classified by its producer-owned markers.
pub fn compile_first_request(
    request: &ChatRequest,
    system_messages: usize,
    prefix_messages: usize,
    assembly: Option<&ContextDecisionReport>,
    expected: ContextCost,
) -> FirstRequestContextReport {
    let mut coordinator = ContextCoordinator::default();
    let scope = assembly
        .and_then(|report| report.contributions.first())
        .and_then(|item| item.provenance.scope.clone());
    let mut include = |id: &str, owner, cost, retention| {
        coordinator.include(ContextContribution {
            id: id.into(),
            owner,
            cost,
            retention,
            provenance: ContextProvenance {
                source: id.into(),
                scope: scope.clone(),
            },
            decision: ContextDecision::Included,
            budget_chars: None,
        });
    };
    include(
        "request_system",
        ContextOwner::Host,
        ContextCost::text(
            request
                .system
                .as_ref()
                .map_or(0, |text| text.chars().count()),
        ),
        Retention::Required,
    );
    for (index, message) in request.messages.iter().enumerate() {
        let (id, owner, retention) = if index < system_messages {
            ("system_prompt", ContextOwner::Host, Retention::Required)
        } else if index < system_messages + prefix_messages {
            (
                "prepared_conversation",
                ContextOwner::Chat,
                Retention::Required,
            )
        } else {
            let text = message.content.first_text().unwrap_or("");
            if message.role == genai::chat::ChatRole::System {
                if text.starts_with(crate::turn_context::STICKY_CONTEXT_PREFIX) {
                    ("chat_stickies", ContextOwner::Chat, Retention::Protected)
                } else if text.starts_with(crate::turn_context::SCRATCH_PREFIX) {
                    (
                        "turn_scratch",
                        ContextOwner::UnitOfWork,
                        Retention::Required,
                    )
                } else if text.starts_with(crate::loop_state::TURN_CONTROL_PREFIX) {
                    ("turn_control", ContextOwner::Host, Retention::Required)
                } else {
                    (
                        "round_context",
                        ContextOwner::UnitOfWork,
                        Retention::Required,
                    )
                }
            } else {
                (
                    "runtime_tool_history",
                    ContextOwner::Tool,
                    Retention::Required,
                )
            }
        };
        include(
            id,
            owner,
            ContextCost::messages(std::slice::from_ref(message)),
            retention,
        );
    }
    let schema_chars = request
        .tools
        .iter()
        .flatten()
        .map(|tool| {
            serde_json::to_string(tool)
                .expect("GenAI tool serializes")
                .chars()
                .count()
        })
        .sum();
    include(
        "tool_definitions",
        ContextOwner::Tool,
        ContextCost::text(schema_chars),
        Retention::Required,
    );
    // Preserve the original domain provenance when the immutable prepared lane
    // still matches. A new projection must not silently inherit stale attribution.
    let mut provenance_matches = true;
    if let Some(assembly) = assembly {
        let prepared: Vec<_> = assembly
            .contributions
            .iter()
            .filter(|item| {
                !matches!(
                    item.id.as_str(),
                    "system_prompt" | "tool_definitions" | "unknown_tool_schemas" | "chat_stickies"
                )
            })
            .cloned()
            .collect();
        let offered = prepared.iter().fold(ContextCost::default(), |sum, item| {
            sum.saturating_add(item.cost)
        });
        let actual = coordinator
            .contributions
            .iter()
            .filter(|item| item.id == "prepared_conversation")
            .fold(ContextCost::default(), |sum, item| {
                sum.saturating_add(item.cost)
            });
        provenance_matches = offered == actual
            && !prepared
                .iter()
                .any(|item| item.decision == ContextDecision::Unattributed);
        if provenance_matches {
            coordinator
                .contributions
                .retain(|item| item.id != "prepared_conversation");
            coordinator.contributions.extend(prepared);
        }
    }
    let decisions =
        coordinator.report(assembly.map_or(ContextCapacity::default(), |report| report.capacity));
    FirstRequestContextReport {
        reconciled: decisions.cost == expected && provenance_matches,
        prepared_provenance_preserved: assembly.map(|_| provenance_matches),
        decisions,
        tool_count: request.tools.as_ref().map(Vec::len),
        schema_chars,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use genai::chat::ChatMessage;

    #[test]
    fn lane_boundaries_prevent_marker_quotations_from_claiming_runtime_ownership() {
        let request = ChatRequest::new(vec![
            ChatMessage::system("policy"),
            ChatMessage::user("[MEDOUSA_TURN_CONTROL] quoted, not runtime control"),
            ChatMessage::system(format!(
                "{}\nprotected",
                crate::turn_context::STICKY_CONTEXT_PREFIX
            )),
            ChatMessage::system(format!("{}\ngoal", crate::turn_context::SCRATCH_PREFIX)),
            ChatMessage::system(format!(
                "{}\nbudget",
                crate::loop_state::TURN_CONTROL_PREFIX
            )),
            ChatMessage::system("[MEDOUSA_ROUND_CONTEXT]\nwork"),
        ]);
        let footprint = RequestFootprint::new(&request, None, None)
            .with_first_request_context(&request, 1, 1, None);
        let report = footprint.first_request_context.unwrap();
        assert!(report.reconciled);
        let items = report.decisions.contributions;
        assert_eq!(
            items
                .iter()
                .filter(|item| item.id == "turn_control")
                .count(),
            1
        );
        assert!(
            items
                .iter()
                .any(|item| item.id == "prepared_conversation" && item.owner == ContextOwner::Chat)
        );
        assert!(
            items
                .iter()
                .any(|item| item.id == "chat_stickies" && item.retention == Retention::Protected)
        );
    }

    #[test]
    fn changed_prepared_projection_never_inherits_stale_provenance() {
        let request = ChatRequest::from_user("new projection");
        let mut seed = ContextCoordinator::default();
        seed.include(ContextContribution {
            id: "original_user".into(),
            owner: ContextOwner::Chat,
            provenance: ContextProvenance {
                source: "original_user".into(),
                scope: Some("session".into()),
            },
            retention: Retention::Required,
            decision: ContextDecision::Included,
            cost: ContextCost::text(3),
            budget_chars: None,
        });
        let footprint = RequestFootprint::new(&request, None, None).with_first_request_context(
            &request,
            0,
            1,
            Some(&seed.report(Default::default())),
        );
        let report = footprint.first_request_context.unwrap();
        assert!(!report.reconciled);
        assert_eq!(report.prepared_provenance_preserved, Some(false));
        assert_eq!(Some(report.decisions.cost), footprint.content_cost);
        assert!(
            !report
                .decisions
                .contributions
                .iter()
                .any(|item| item.id == "original_user")
        );
    }
}
