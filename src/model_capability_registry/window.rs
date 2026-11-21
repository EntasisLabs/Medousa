/// The current catalog conflates separate input ceilings and shared context
/// limits. Until it distinguishes them, reserve output against both: conservative
/// on separate-input providers, necessary for shared-window targets.
pub fn inference_window_policy(
    provider: &str,
    model: &str,
) -> medousa_runtime::inference_window::InferenceWindowPolicy {
    let record = crate::model_capability_registry::registry()
        .resolve(provider, model)
        .model;
    policy_for_record(provider, model, record.as_ref())
}

pub(crate) fn policy_for_record(
    provider: &str,
    model: &str,
    record: Option<&crate::model_capability_registry::types::ModelCapabilityRecord>,
) -> medousa_runtime::inference_window::InferenceWindowPolicy {
    // Older cached account catalogs contain names/reasoning but no limits. Keep
    // this an explicit policy estimate rather than inventing catalog metadata.
    let codex = provider.trim().eq_ignore_ascii_case("openai-codex");
    let codex_fallback = codex && (model.starts_with("gpt-5") || model.starts_with("gpt-6"));
    let input = record
        .and_then(|r| r.max_input_tokens)
        .map(|limit| limit.min(u32::MAX as u64) as u32);
    let output = record
        .and_then(|r| r.max_output_tokens)
        .map(|limit| limit.min(u32::MAX as u64) as u32)
        .unwrap_or(medousa_runtime::inference_window::DEFAULT_OUTPUT_RESERVE);
    medousa_runtime::inference_window::InferenceWindowPolicy {
        target: format!("{provider}:{model}"),
        input_tokens: input,
        shared_tokens: if record
            .is_some_and(|r| matches!(r.source.as_str(), "anthropic.models" | "google.models"))
        {
            None
        } else {
            Some(input.unwrap_or(if codex_fallback {
                272_000
            } else {
                medousa_runtime::inference_window::UNKNOWN_INPUT_TOKENS
            }))
        },
        // ChatGPT plan bridge deliberately rejects max_output_tokens. Reserving
        // the catalog maximum keeps its generation behavior unchanged; unknown
        // limits remain explicit estimates, never a claimed hard guarantee.
        enforce_output_limit: !codex,
        output_reserve: output,
        ..Default::default()
    }
}
