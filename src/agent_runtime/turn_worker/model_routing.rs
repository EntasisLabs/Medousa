//! Resolve worker LLM target from matrix roles, hints, and intent defaults.

use crate::stage_routing::{StageRoutingMatrix, normalize_role};

use super::policy::TurnWorkerIntent;

pub fn default_stage_role_for_intent(intent: TurnWorkerIntent) -> &'static str {
    match intent {
        TurnWorkerIntent::Research => "extractor",
        TurnWorkerIntent::MemoryContext | TurnWorkerIntent::MemoryAvecCalibrate => "summarizer",
        TurnWorkerIntent::General => "final_response",
    }
}

/// Resolve `(provider, model)` for a background worker.
pub fn resolve_worker_llm_target(
    host_provider: &str,
    host_model: &str,
    intent: TurnWorkerIntent,
    stage_role: Option<&str>,
    model_hint: Option<&str>,
) -> (String, String) {
    if let Some((provider, model)) = resolve_explicit_model_hint(model_hint) {
        return (provider, model);
    }

    let matrix = StageRoutingMatrix::default_for(host_provider, host_model);
    let role = stage_role
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(normalize_role)
        .or_else(|| Some(default_stage_role_for_intent(intent).to_string()))
        .unwrap_or_else(|| default_stage_role_for_intent(intent).to_string());

    if let Some(route) = matrix.get(&role) {
        return (route.provider.clone(), route.model.clone());
    }

    (
        crate::resolve_llm_provider(Some(host_provider)),
        crate::resolve_llm_model(Some(host_model)),
    )
}

fn resolve_explicit_model_hint(model_hint: Option<&str>) -> Option<(String, String)> {
    let hint = model_hint.map(str::trim).filter(|value| !value.is_empty())?;
    if let Some((provider, model)) = hint.split_once(':') {
        let provider = provider.trim();
        let model = model.trim();
        if !provider.is_empty() && !model.is_empty() {
            return Some((
                crate::resolve_llm_provider(Some(provider)),
                crate::resolve_llm_model(Some(model)),
            ));
        }
    }
    Some((
        crate::resolve_llm_provider(None),
        crate::resolve_llm_model(Some(hint)),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intent_defaults_to_extractor_for_research() {
        assert_eq!(
            default_stage_role_for_intent(TurnWorkerIntent::Research),
            "extractor"
        );
    }

    #[test]
    fn explicit_hint_overrides_matrix() {
        let (provider, model) = resolve_worker_llm_target(
            "openai",
            "gpt-4o-mini",
            TurnWorkerIntent::Research,
            Some("chunker"),
            Some("anthropic:claude-sonnet-4"),
        );
        assert_eq!(provider, crate::resolve_llm_provider(Some("anthropic")));
        assert_eq!(
            model,
            crate::resolve_llm_model(Some("claude-sonnet-4"))
        );
    }

    #[test]
    fn stage_role_selects_matrix_route() {
        let (provider, model) = resolve_worker_llm_target(
            "openai",
            "gpt-4o-mini",
            TurnWorkerIntent::General,
            Some("verifier"),
            None,
        );
        assert_eq!(provider, "openai");
        assert_eq!(model, "gpt-4o-mini");
        let matrix = StageRoutingMatrix::default_for("openai", "gpt-4o-mini");
        assert_eq!(model, matrix.verifier.model);
    }
}
