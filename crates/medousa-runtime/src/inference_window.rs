//! Hybrid window policy: stable turn baseline, soft maintenance, hard admission.
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
/// Working input ceiling, independent of a provider's potentially larger limit.
/// This is an operating policy, not a claim of a universally optimal model size.
pub const WORKING_INPUT_TOKENS: u32 = 280_000;
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
        let input = self
            .input_tokens
            .or(self.shared_tokens)
            .unwrap_or(UNKNOWN_INPUT_TOKENS);
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
    /// Crossing this target requests maintenance; only input_budget rejects input.
    pub fn working_budget(&self) -> u32 {
        self.input_budget().min(WORKING_INPUT_TOKENS)
    }

    fn uses_openai_proxy(&self) -> bool {
        let (provider, model) = self.target.split_once(':').unwrap_or(("", ""));
        matches!(
            provider,
            "openai-codex" | "openai" | "azure-openai" | "azure_openai"
        ) && (model.starts_with("gpt-5")
            || model.starts_with("gpt-6")
            || model.starts_with("gpt-4o")
            || model.starts_with("gpt-4.1")
            || model.starts_with("o1")
            || model.starts_with("o3")
            || model.starts_with("o4"))
    }

    pub fn text_tokens(&self, text: &str) -> u32 {
        let count = if self.uses_openai_proxy() {
            // GPT-6's exact encoding is not published. This offline o200k proxy
            // includes 25% headroom; it is never labeled provider tokenization.
            // Bound BPE merge work on long repeated logs/identifiers. Chunk
            // boundaries may add tokens, which is conservative for this proxy.
            let tokenizer = tiktoken_rs::o200k_base_singleton();
            let mut remaining = text;
            let mut tokens = 0usize;
            while !remaining.is_empty() {
                let mut end = remaining.len().min(2048);
                while !remaining.is_char_boundary(end) {
                    end -= 1;
                }
                let (chunk, rest) = remaining.split_at(end);
                tokens = tokens.saturating_add(tokenizer.encode_ordinary(chunk).len());
                remaining = rest;
            }
            tokens.saturating_mul(5).div_ceil(4)
        } else {
            text.len()
        };
        u32::try_from(count).unwrap_or(u32::MAX)
    }

    pub fn accounting(&self) -> &'static str {
        if self.uses_openai_proxy() {
            "estimated_o200k_proxy_plus25pct_protocol_media8192_framing32; not_provider_tokenization"
        } else {
            "estimated_utf8_bytes_plus_protocol_media8192_framing32; not_provider_tokenization"
        }
    }

    pub fn estimate(&self, request: &ChatRequest) -> WindowEstimate {
        estimate_with(request, |text| self.text_tokens(text))
    }

    /// Conversion for byte-based observation lanes only, not token admission.
    pub fn observation_byte_budget(&self) -> usize {
        self.observation_bytes_for_tokens(self.working_budget())
    }

    pub fn observation_bytes_for_tokens(&self, tokens: u32) -> usize {
        let budget = tokens as usize;
        if self.uses_openai_proxy() {
            budget.saturating_mul(2)
        } else {
            budget
        }
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
    estimate_with(request, |text| {
        u32::try_from(text.len()).unwrap_or(u32::MAX)
    })
}

fn estimate_with(request: &ChatRequest, count: impl Fn(&str) -> u32) -> WindowEstimate {
    let mut text = 0u32;
    let mut media_parts = 0u32;
    add(
        &mut text,
        request.system.as_ref().map_or(0, |s| count(s) as usize),
    );
    for tool in request.tools.iter().flatten() {
        add(
            &mut text,
            count(&serde_json::to_string(tool).expect("tool serializes")) as usize,
        );
    }
    for part in request.messages.iter().flat_map(|m| m.content.parts()) {
        match part {
            ContentPart::Text(value)
            | ContentPart::ReasoningContent(value)
            | ContentPart::ThoughtSignature(value) => add(&mut text, count(value) as usize),
            ContentPart::ToolCall(_) | ContentPart::ToolResponse(_) => {
                add(
                    &mut text,
                    count(&serde_json::to_string(part).expect("tool part serializes")) as usize,
                );
            }
            ContentPart::Binary(_) => media_parts = media_parts.saturating_add(1),
            ContentPart::Custom(value) => {
                add(
                    &mut text,
                    count(&serde_json::to_string(value).expect("custom part serializes")) as usize,
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
        Self::compile_inference_for_round(
            request,
            policy,
            system_messages,
            prefix_messages,
            assembly,
            perception,
            true,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn compile_inference_for_round(
        request: &mut ChatRequest,
        policy: &InferenceWindowPolicy,
        system_messages: usize,
        prefix_messages: usize,
        assembly: Option<&ContextDecisionReport>,
        perception: &mut ToolPerceptionGovernor,
        select_baseline: bool,
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
        let mut projected_tokens = policy.estimate(request).total_tokens;
        for group in groups {
            if !select_baseline || projected_tokens <= policy.working_budget() {
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
            projected_tokens = policy.estimate(&trial).total_tokens;
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
        // Only cold observations are eligible. Recent results, errors, arguments
        // and exact instructions survive soft maintenance even above the target.
        if policy.estimate(request).total_tokens > policy.working_budget() {
            perception.compact_tool_history_with_budget(&mut request.messages, 0, 0, 4, 1024);
        }
        let measured = policy.estimate(request);
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
            accounting: policy.accounting().into(),
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
