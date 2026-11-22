//! Hybrid window policy: stable turn baseline, soft maintenance, hard admission.
//! Producers retain storage, authority, and valid reductions. Only whole old chat
//! exchanges may be deferred here; tool evidence is reduced by perception.
use crate::inference_usage::{RequestFootprint, RequestSection, section};
use genai::chat::{ChatRequest, ChatResponse, ChatRole, ContentPart};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

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

// Both selection and the final dispatch guard run in this turn's scope.
// Concurrent turns, nested tool loops and checkpoint resumes start fresh.
tokio::task_local! {
    static LAST_INPUT: RefCell<Option<MeasuredInput>>;
}

pub(crate) async fn with_turn_window<T>(turn: impl Future<Output = T>) -> T {
    LAST_INPUT.scope(RefCell::default(), turn).await
}

struct MeasuredInput {
    target: String,
    input_tokens: u32,
    system: RequestSection,
    tools: RequestSection,
    messages: HashMap<String, usize>,
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
        let codex = self.target.starts_with("openai-codex:");
        let count = |text: &str| self.text_tokens(text);
        if codex
            && let Ok(estimate) = LAST_INPUT.try_with(|last| {
                let last = last.borrow();
                let comparable = last.as_ref().filter(|m| {
                    m.target == self.target
                        && m.system == section(&request.system)
                        && m.tools == section(&request.tools)
                });
                estimate_with(request, count, true, comparable)
            })
        {
            return estimate;
        }
        estimate_with(request, count, codex, None)
    }

    pub(crate) fn observe_input(&self, footprint: &RequestFootprint, response: &ChatResponse) {
        let Some(model) = self.target.strip_prefix("openai-codex:") else {
            return;
        };
        let _ = LAST_INPUT.try_with(|last| {
            let input = response
                .usage
                .prompt_tokens
                .and_then(|n| u32::try_from(n).ok())
                .filter(|n| {
                    *n > 0
                        && response.provider_model_iden.adapter_kind
                            == genai::adapter::AdapterKind::OpenAIResp
                        && response.provider_model_iden.model_name.as_str() == model
                });
            last.replace(input.map(|input_tokens| {
                let mut messages = HashMap::new();
                for message in &footprint.messages {
                    *messages.entry(message.sha256.clone()).or_insert(0) += 1;
                }
                MeasuredInput {
                    target: self.target.clone(),
                    input_tokens,
                    system: footprint.system.clone(),
                    tools: footprint.tools.clone(),
                    messages,
                }
            }));
        });
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
    /// Subset of text_and_protocol_tokens, available on the Codex bridge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replay_signature_tokens: Option<u32>,
    pub media_tokens: u32,
    pub media_parts: u32,
    pub framing_tokens: u32,
    /// Full proxy breakdown above, with the admission total below adjusted only
    /// when a comparable measured request gives a smaller conservative bound.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_input_tokens: Option<u32>,
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
    estimate_with(
        request,
        |text| u32::try_from(text.len()).unwrap_or(u32::MAX),
        false,
        None,
    )
}

fn estimate_with(
    request: &ChatRequest,
    count: impl Fn(&str) -> u32,
    codex: bool,
    measured: Option<&MeasuredInput>,
) -> WindowEstimate {
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
    let mut retained = measured.map(|last| last.messages.clone());
    let mut added_tokens = 0u32;
    let mut signature_tokens = 0u32;
    for message in &request.messages {
        let mut signatures = HashSet::new();
        let mut message_text = 0u32;
        let mut message_media = 0u32;
        for part in message.content.parts() {
            // ChatGPT sign-in sends one reasoning item per distinct signature
            // in a message, even when each parallel tool call also carries it.
            if codex {
                let replay_signatures: &[String] = match part {
                    ContentPart::ThoughtSignature(value) => std::slice::from_ref(value),
                    ContentPart::ToolCall(call) => {
                        call.thought_signatures.as_deref().unwrap_or(&[])
                    }
                    _ => &[],
                };
                for signature in replay_signatures {
                    if signatures.insert(signature.as_str()) {
                        let cost = count(signature);
                        signature_tokens = signature_tokens.saturating_add(cost);
                        message_text = message_text.saturating_add(cost);
                    }
                }
            }
            let tokens = match part {
                ContentPart::ThoughtSignature(_) | ContentPart::ReasoningContent(_) if codex => 0,
                ContentPart::Text(value)
                | ContentPart::ThoughtSignature(value)
                | ContentPart::ReasoningContent(value) => count(value),
                ContentPart::ToolCall(call) if codex => {
                    let wire_call = serde_json::json!({"type":"function_call", "namespace":"medousa",
                        "call_id":call.call_id, "name":call.fn_name, "arguments":call.fn_arguments.to_string()});
                    count(&wire_call.to_string())
                }
                ContentPart::ToolCall(_) | ContentPart::ToolResponse(_) => {
                    count(&serde_json::to_string(part).expect("tool part serializes"))
                }
                ContentPart::Binary(_) => {
                    message_media = message_media.saturating_add(1);
                    0
                }
                ContentPart::Custom(value) => {
                    message_media = message_media.saturating_add(1);
                    count(&serde_json::to_string(value).expect("custom part serializes"))
                }
            };
            message_text = message_text.saturating_add(tokens);
        }
        text = text.saturating_add(message_text);
        media_parts = media_parts.saturating_add(message_media);
        if let Some(retained) = retained.as_mut() {
            if let Some(remaining) = retained.get_mut(&section(message).sha256)
                && *remaining > 0
            {
                *remaining -= 1;
            } else {
                added_tokens = added_tokens
                    .saturating_add(message_text)
                    .saturating_add(message_media.saturating_mul(8192))
                    .saturating_add(32);
            }
        }
    }
    let media_tokens = media_parts.saturating_mul(8192);
    let framing_tokens = u32::try_from(request.messages.len())
        .unwrap_or(u32::MAX)
        .saturating_mul(32)
        .saturating_add(128);
    let full = text
        .saturating_add(media_tokens)
        .saturating_add(framing_tokens);
    // Count all previously measured input (including cached input), plus the
    // whole cost of every new/replaced message. No estimated removal credits.
    let anchored = measured.map(|m| m.input_tokens.saturating_add(added_tokens));
    WindowEstimate {
        previous_input_tokens: measured
            .filter(|_| anchored.is_some_and(|n| n < full))
            .map(|m| m.input_tokens),
        replay_signature_tokens: codex.then_some(signature_tokens),
        text_and_protocol_tokens: text,
        media_tokens,
        media_parts,
        framing_tokens,
        total_tokens: anchored.unwrap_or(full).min(full),
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
