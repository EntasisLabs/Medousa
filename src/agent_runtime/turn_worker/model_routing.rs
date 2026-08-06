//! Resolve worker LLM target from matrix roles, hints, and intent defaults.

use crate::model_route::{DelegatedLlmRoute, resolve_delegated_llm_route};
use crate::stage_routing::StageRoutingMatrix;

use super::policy::TurnWorkerIntent;

pub fn default_stage_role_for_intent(intent: TurnWorkerIntent) -> &'static str {
    match intent {
        TurnWorkerIntent::Research => "extractor",
        TurnWorkerIntent::MemoryContext | TurnWorkerIntent::MemoryAvecCalibrate => "summarizer",
        TurnWorkerIntent::General => "final_response",
    }
}

/// Resolve `(provider, model)` for a background worker.
///
/// Uses shared [`crate::model_route`] so bare model hints cannot silently attach
/// to the process-default provider (the `openai::deepseek-*` failure mode).
pub fn resolve_worker_llm_target(
    host_provider: &str,
    host_model: &str,
    intent: TurnWorkerIntent,
    stage_role: Option<&str>,
    model_hint: Option<&str>,
) -> (String, String) {
    resolve_worker_llm_target_with_matrix(
        host_provider,
        host_model,
        intent,
        stage_role,
        model_hint,
        None,
    )
}

pub fn resolve_worker_llm_target_with_matrix(
    host_provider: &str,
    host_model: &str,
    intent: TurnWorkerIntent,
    stage_role: Option<&str>,
    model_hint: Option<&str>,
    stage_matrix: Option<&StageRoutingMatrix>,
) -> (String, String) {
    let role = stage_role
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default_stage_role_for_intent(intent));
    resolve_delegated_llm_route(DelegatedLlmRoute {
        host_provider,
        host_model,
        model_hint,
        stage_role: Some(role),
        stage_matrix,
    })
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
        assert_eq!(model, crate::resolve_llm_model(Some("claude-sonnet-4")));
    }

    #[test]
    fn auto_hint_uses_stage_matrix_not_guessed_model() {
        let mut matrix = StageRoutingMatrix::default_for("openai", "gpt-5.6-luna");
        matrix.summarizer.provider = "deepseek".to_string();
        matrix.summarizer.model = "deepseek-v4-flash".to_string();
        let (provider, model) = resolve_worker_llm_target_with_matrix(
            "openai",
            "gpt-5.6-luna",
            TurnWorkerIntent::MemoryContext,
            None,
            Some("auto"),
            Some(&matrix),
        );
        assert_eq!(provider, "deepseek");
        assert_eq!(model, "deepseek-v4-flash");
    }

    #[test]
    fn bare_deepseek_hint_does_not_attach_to_openai() {
        let (provider, model) = resolve_worker_llm_target(
            "openai",
            "gpt-5.6-luna",
            TurnWorkerIntent::MemoryContext,
            None,
            Some("deepseek-v4-flash"),
        );
        assert_eq!(provider, "deepseek");
        assert_eq!(model, "deepseek-v4-flash");
    }

    #[test]
    fn stage_role_selects_matrix_route() {
        let matrix = StageRoutingMatrix::default_for("openai", "gpt-4o-mini");
        let (provider, model) = resolve_worker_llm_target_with_matrix(
            "openai",
            "gpt-4o-mini",
            TurnWorkerIntent::General,
            Some("verifier"),
            None,
            Some(&matrix),
        );
        assert_eq!(provider, "openai");
        assert_eq!(model, "gpt-4o-mini");
        assert_eq!(model, matrix.verifier.model);
    }

    #[test]
    fn configured_matrix_can_route_worker_off_host_provider() {
        let mut matrix = StageRoutingMatrix::default_for("openai", "gpt-5.6-luna");
        matrix.summarizer.provider = "deepseek".to_string();
        matrix.summarizer.model = "deepseek-v4-flash".to_string();
        let (provider, model) = resolve_worker_llm_target_with_matrix(
            "openai",
            "gpt-5.6-luna",
            TurnWorkerIntent::MemoryContext,
            None, // intent → summarizer
            None,
            Some(&matrix),
        );
        assert_eq!(provider, "deepseek");
        assert_eq!(model, "deepseek-v4-flash");
    }
}
