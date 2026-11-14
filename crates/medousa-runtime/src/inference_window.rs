//! Per-inference window policy. This is an estimate, never a provider tokenizer.
//! Producers retain storage, authority, and valid reductions. Only whole old chat
//! exchanges may be deferred here; tool evidence is reduced by perception.
use genai::chat::{ChatRequest, ChatRole, ContentPart};
use serde::{Deserialize, Serialize};

use crate::context::{ContextCapacity, ContextCoordinator, ContextDecisionReport, ContextOwner};
use crate::perception::ToolPerceptionGovernor;

#[cfg(test)]
#[path = "inference_window_tests.rs"]
mod tests;

pub const UNKNOWN_INPUT_TOKENS: u32 = 8_000;
pub const DEFAULT_OUTPUT_RESERVE: u32 = 4_096;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InferenceWindowPolicy {
    pub target: String,
    pub input_tokens: Option<u32>,
    /// Some catalogs expose only a shared context limit. When semantics are
    /// ambiguous the host supplies it here as well, conservatively.
    pub shared_tokens: Option<u32>,
    pub output_reserve: u32,
    /// False for bridges that reject output caps. Such targets reserve the
    /// catalog maximum, or explicitly estimated unknown-output policy.
    pub enforce_output_limit: bool,
    pub safety_tokens: u32,
    pub observation_tokens: u32,
}

impl Default for InferenceWindowPolicy {
    fn default() -> Self {
        Self {
            target: "unknown".into(),
            input_tokens: None,
            shared_tokens: None,
            output_reserve: DEFAULT_OUTPUT_RESERVE,
            enforce_output_limit: true,
            safety_tokens: 512,
            observation_tokens: 512,
        }
    }
}

impl InferenceWindowPolicy {
    pub fn input_budget(&self) -> u32 {
        let input = self.input_tokens.unwrap_or(UNKNOWN_INPUT_TOKENS);
        let shared = self
            .shared_tokens
            .map(|limit| limit.saturating_sub(self.output_reserve))
            // Unknown capacity uses a shared 8k assumption, not an 8k input
            // guarantee plus unaccounted generation.
            .unwrap_or_else(|| {
                if self.input_tokens.is_none() {
                    UNKNOWN_INPUT_TOKENS.saturating_sub(self.output_reserve)
                } else {
                    u32::MAX
                }
            });
        input
            .min(shared)
            .saturating_sub(self.safety_tokens)
            .saturating_sub(self.observation_tokens)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowEstimate {
    pub text_and_protocol_tokens: u32,
    pub media_tokens: u32,
    pub media_parts: u32,
    pub framing_tokens: u32,
    pub total_tokens: u32,
}

fn add(total: &mut u32, value: usize) {
    *total = total.saturating_add(u32::try_from(value).unwrap_or(u32::MAX));
}

/// Byte-based upper-biased text estimate handles UTF-8 without chars/4's large
/// undercount for code, identifiers and non-Latin text. Media is an explicit
/// uncertain 8192-token allowance per part (not a claim of measured media cost).
/// Opaque payload bytes/base64 are deliberately not tokenized as text.
pub fn estimate(request: &ChatRequest) -> WindowEstimate {
    let mut text = 0u32;
    let mut media_parts = 0u32;
    add(&mut text, request.system.as_ref().map_or(0, String::len));
    for tool in request.tools.iter().flatten() {
        add(
            &mut text,
            serde_json::to_vec(tool).expect("tool serializes").len(),
        );
    }
    for part in request.messages.iter().flat_map(|m| m.content.parts()) {
        match part {
            ContentPart::Text(value)
            | ContentPart::ReasoningContent(value)
            | ContentPart::ThoughtSignature(value) => add(&mut text, value.len()),
            ContentPart::ToolCall(_) | ContentPart::ToolResponse(_) => {
                add(
                    &mut text,
                    serde_json::to_vec(part)
                        .expect("tool part serializes")
                        .len(),
                );
            }
            ContentPart::Binary(_) => media_parts = media_parts.saturating_add(1),
            ContentPart::Custom(value) => {
                add(
                    &mut text,
                    serde_json::to_vec(value)
                        .expect("custom part serializes")
                        .len(),
                );
                media_parts = media_parts.saturating_add(1);
            }
        }
    }
    let media_tokens = media_parts.saturating_mul(8192);
    let framing_tokens = u32::try_from(request.messages.len())
        .unwrap_or(u32::MAX)
        .saturating_mul(32)
        .saturating_add(128);
    WindowEstimate {
        text_and_protocol_tokens: text,
        media_tokens,
        media_parts,
        framing_tokens,
        total_tokens: text
            .saturating_add(media_tokens)
            .saturating_add(framing_tokens),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceWindowReport {
    pub policy: InferenceWindowPolicy,
    pub input_budget: u32,
    pub estimate: WindowEstimate,
    pub accounting: String,
    pub deferred_history_messages: usize,
    pub request_messages_digest: String,
    pub reconciled: bool,
    pub decisions: ContextDecisionReport,
    #[serde(default)]
    pub deferred_pointers: Vec<medousa_types::context_selection::PointerSelection>,
    #[serde(default)]
    pub hud: Option<medousa_types::context_hud::UserContextProjection>,
    #[serde(default)]
    pub meaningful_changes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[error(
    "context_window_overflow: target={target} estimated={estimated_tokens} budget={input_budget} (protected or irreducible material cannot fit)"
)]
pub struct ContextWindowOverflow {
    pub kind: String,
    pub target: String,
    pub input_budget: u32,
    pub estimated_tokens: u32,
    pub report: Box<InferenceWindowReport>,
}

impl ContextCoordinator {
    /// Operates on the final request copy, after every app-owned append. Lane
    /// offsets are producer metadata, never inferred from user marker text.
    /// Checkpoints keep the authoritative transcript, so larger resumed targets
    /// can recover history that was only deferred from an earlier request.
    pub fn compile_inference(
        request: &mut ChatRequest,
        policy: &InferenceWindowPolicy,
        system_messages: usize,
        prefix_messages: usize,
        assembly: Option<&ContextDecisionReport>,
        perception: &mut ToolPerceptionGovernor,
    ) -> Result<InferenceWindowReport, ContextWindowOverflow> {
        let budget = policy.input_budget();
        let history_end = system_messages
            .saturating_add(prefix_messages.saturating_sub(1))
            .min(request.messages.len());
        // Select oldest complete chat exchanges first. A group with system
        // state or a tool protocol part is protected as a whole.
        let mut groups = Vec::new();
        let mut start = system_messages.min(history_end);
        for (index, message) in request
            .messages
            .iter()
            .enumerate()
            .take(history_end)
            .skip(start)
        {
            if index > start && message.role == ChatRole::User {
                groups.push(start..index);
                start = index;
            }
        }
        if start < history_end {
            groups.push(start..history_end);
        }
        let mut remove = vec![false; request.messages.len()];
        let mut deferred = 0;
        for group in groups {
            if estimate(request).total_tokens <= budget {
                break;
            }
            if group.clone().any(|index| {
                let message = &request.messages[index];
                !matches!(message.role, ChatRole::User | ChatRole::Assistant)
                    || message.content.parts().iter().any(|part| {
                        matches!(
                            part,
                            ContentPart::ToolCall(_) | ContentPart::ToolResponse(_)
                        )
                    })
            }) {
                continue;
            }
            // Mark first, rebuild once below. Cost of marked groups is excluded
            // from the trial copy, keeping deterministic chronology and roles.
            for index in group {
                remove[index] = true;
            }
            let mut trial = request.clone();
            trial.messages = trial
                .messages
                .into_iter()
                .enumerate()
                .filter_map(|(i, m)| (!remove[i]).then_some(m))
                .collect();
            if estimate(&trial).total_tokens <= budget {
                break;
            }
        }
        let original_prefix = system_messages.saturating_add(prefix_messages);
        let removed_prefix = remove
            .iter()
            .take(original_prefix)
            .filter(|value| **value)
            .count();
        let deferred_chars: usize = request
            .messages
            .iter()
            .enumerate()
            .filter(|(index, _)| remove[*index])
            .map(|(_, message)| {
                crate::context::ContextCost::messages(std::slice::from_ref(message)).text_chars
            })
            .sum();
        let mut index = 0;
        request.messages.retain(|_| {
            let keep = !remove[index];
            index += 1;
            if !keep {
                deferred += 1;
            }
            keep
        });
        // Reclaim evidence only as needed, never delete tool calls or results.
        if estimate(request).total_tokens > budget {
            let before = estimate(request).total_tokens;
            let response_bytes: usize = request
                .messages
                .iter()
                .flat_map(|m| m.content.parts())
                .filter_map(|part| {
                    if let ContentPart::ToolResponse(r) = part {
                        Some(r.content.len())
                    } else {
                        None
                    }
                })
                .sum();
            let desired = response_bytes.saturating_sub(before.saturating_sub(budget) as usize);
            perception.compact_tool_history_with_budget(
                &mut request.messages,
                desired,
                desired,
                0,
                1024,
            );
        }
        let measured = estimate(request);
        let cost = crate::inference_usage::RequestFootprint::new(request, None, None)
            .content_cost
            .expect("request cost");
        let compiled = crate::request_context::compile_first_request(
            request,
            system_messages,
            prefix_messages.saturating_sub(removed_prefix),
            assembly,
            cost,
        );
        let reconciled = compiled.reconciled;
        let mut decisions = compiled.decisions;
        decisions.capacity = ContextCapacity {
            input_tokens: policy.input_tokens,
        };
        if deferred > 0 {
            decisions
                .contributions
                .push(crate::context::ContextContribution {
                    id: "window_deferred_chat_history".into(),
                    owner: crate::context::ContextOwner::Chat,
                    provenance: crate::context::ContextProvenance {
                        source: "prepared_chat_history".into(),
                        scope: assembly.and_then(|a| {
                            a.contributions
                                .iter()
                                .filter(|c| c.owner == ContextOwner::Chat)
                                .find_map(|c| c.provenance.scope.clone())
                        }),
                    },
                    retention: crate::context::Retention::Flexible,
                    decision: crate::context::ContextDecision::Omitted {
                        reason: crate::context::ContextOmissionReason::BudgetExcluded,
                        source_text_chars: Some(deferred_chars),
                    },
                    cost: Default::default(),
                    budget_chars: None,
                });
        }
        // Only Chat owns a session-history route. A work/tool scope is not a
        // session ID, even when it happens to look like one.
        let scope = assembly.and_then(|a| {
            a.contributions
                .iter()
                .filter(|c| c.owner == ContextOwner::Chat)
                .find_map(|c| c.provenance.scope.clone())
        });
        let deferred_pointers = scope
            .filter(|_| deferred > 0)
            .map(|session_id| {
                vec![medousa_types::context_selection::PointerSelection {
                    provenance: "prepared_chat_history".into(),
                    scope: session_id.clone(),
                    relevance: 1000,
                    recency: 0,
                    activity: 0,
                    reasons: vec![
                        "inference_budget_deferral; original_session_history_retained".into(),
                    ],
                    representation:
                        medousa_types::context_selection::PointerRepresentation::Deferred,
                    retrieval: medousa_types::context_selection::PointerRetrieval::ChatHistory {
                        session_id,
                    },
                    resolution: medousa_types::context_selection::PointerResolution::Unknown,
                }]
            })
            .unwrap_or_default();
        let report = InferenceWindowReport {
            policy: policy.clone(),
            deferred_pointers,
            hud: None,
            meaningful_changes: Vec::new(),
            input_budget: budget,
            estimate: measured.clone(),
            accounting:
                "estimated_utf8_bytes_plus_protocol_media8192_framing32; not_provider_tokenization"
                    .into(),
            deferred_history_messages: deferred,
            request_messages_digest: crate::context::prepared_messages_digest(&request.messages),
            reconciled,
            decisions,
        };
        if measured.total_tokens > budget {
            Err(ContextWindowOverflow {
                kind: "context_window_overflow".into(),
                target: policy.target.clone(),
                input_budget: budget,
                estimated_tokens: measured.total_tokens,
                report: Box::new(report),
            })
        } else {
            Ok(report)
        }
    }
}
