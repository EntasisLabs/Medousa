//! Final dispatch guard for prompt-only, legacy synthesis and tool pipelines.
//! The compiler selects earlier with producer-owned lane offsets; this boundary
//! never guesses which messages are expendable and never mutates the request.
use crate::inference_window::InferenceWindowPolicy;
use async_trait::async_trait;
use genai::chat::{ChatOptions, ChatRequest, ChatResponse};
use stasis::domain::errors::{Result, StasisError};
use stasis::ports::outbound::ai_chat_client::{AiChatClient, StreamDelta};
use std::sync::Arc;
use tokio::sync::mpsc;

#[cfg(test)]
#[path = "windowed_client_tests.rs"]
mod tests;

pub struct WindowedChatClient {
    inner: Arc<dyn AiChatClient>,
    policy: InferenceWindowPolicy,
}

impl WindowedChatClient {
    pub fn new(inner: Arc<dyn AiChatClient>, policy: InferenceWindowPolicy) -> Self {
        Self { inner, policy }
    }

    fn options(&self, request: &ChatRequest, options: Option<&ChatOptions>) -> Result<ChatOptions> {
        let mut options = options.cloned().unwrap_or_default();
        // A shared-limit reserve must be enforceable, not merely telemetry.
        // Explicit caller limits are honored up to this target's configured cap.
        let mut policy = self.policy.clone();
        if policy.enforce_output_limit {
            options.max_tokens = Some(
                options
                    .max_tokens
                    .unwrap_or(policy.output_reserve)
                    .min(policy.output_reserve),
            );
            policy.output_reserve = options.max_tokens.unwrap_or(0);
        }
        let overrides_capacity = options.extra_body.as_ref().is_some_and(|value| {
            [
                "max_tokens",
                "max_completion_tokens",
                "max_output_tokens",
                "previous_response_id",
                "messages",
                "input",
                "tools",
            ]
            .iter()
            .any(|key| value.get(key).is_some())
        });
        if request.previous_response_id.is_some() || overrides_capacity {
            return Err(StasisError::PortFailure(
                serde_json::json!({
                    "kind": "context_window_overflow", "boundary": "dispatch",
                    "target": policy.target,
                    "reason": "unaccounted_cached_state_or_reserved_request_override",
                    "protected_material_unchanged": true,
                })
                .to_string(),
            ));
        }
        let estimate = policy.estimate(request);
        // Extra provider payload can add input outside the typed ChatRequest.
        // Account it rather than leaving an invisible append after selection.
        let extra = options
            .extra_body
            .as_ref()
            .map_or(0, |value| policy.text_tokens(&value.to_string()));
        let format_cost = options.response_format.as_ref().map_or(0, |value| {
            policy.text_tokens(&serde_json::to_string(value).expect("format serializes"))
        });
        let choice_cost = options.tool_choice.as_ref().map_or(0, |value| {
            policy.text_tokens(&serde_json::to_string(value).expect("choice serializes"))
        });
        let tokens = estimate
            .total_tokens
            .saturating_add(extra)
            .saturating_add(format_cost)
            .saturating_add(choice_cost);
        if tokens > policy.input_budget() {
            return Err(StasisError::PortFailure(
                serde_json::json!({
                    "kind": "context_window_overflow", "boundary": "dispatch",
                    "target": policy.target, "input_budget": policy.input_budget(),
                    "estimated_tokens": tokens, "estimate": estimate,
                    "extra_body_tokens_estimate": extra,
                    "response_format_tokens_estimate": format_cost,
                    "tool_choice_tokens_estimate": choice_cost,
                    "accounting": policy.accounting(),
                    "protected_material_unchanged": true,
                })
                .to_string(),
            ));
        }
        Ok(options)
    }
}

#[async_trait]
impl AiChatClient for WindowedChatClient {
    async fn complete(
        &self,
        request: ChatRequest,
        options: Option<&ChatOptions>,
    ) -> Result<ChatResponse> {
        let options = self.options(&request, options)?;
        self.inner.complete(request, Some(&options)).await
    }

    async fn complete_stream(
        &self,
        request: ChatRequest,
        options: Option<&ChatOptions>,
        chunk_tx: Option<&mpsc::Sender<StreamDelta>>,
    ) -> Result<ChatResponse> {
        let options = self.options(&request, options)?;
        self.inner
            .complete_stream(request, Some(&options), chunk_tx)
            .await
    }
}
