//! Shared multi-provider / multi-model route resolution.
//!
//! Used by turn workers, job ingest, and any path that accepts a `model_hint`.
//! Hints must never silently pair a foreign model id with an unrelated provider
//! (e.g. bare `deepseek-v4-flash` must not become `openai::deepseek-v4-flash`).

use crate::stage_routing::StageRoutingMatrix;

/// Split `provider:model` or `provider::model` into trimmed parts.
///
/// Returns `None` for bare model ids (no provider separator).
pub fn split_provider_model(hint: &str) -> Option<(&str, &str)> {
    let hint = hint.trim();
    if hint.is_empty() {
        return None;
    }
    let (provider, model) = if let Some((provider, model)) = hint.split_once("::") {
        (provider, model)
    } else if let Some((provider, model)) = hint.split_once(':') {
        (provider, model)
    } else {
        return None;
    };
    let provider = provider.trim();
    let model = model.trim();
    if provider.is_empty() || model.is_empty() {
        return None;
    }
    // Reject URL-like left sides (`https://…`).
    if provider.contains('/') {
        return None;
    }
    Some((provider, model))
}

/// True when the hint means "do not override — use stage route / host prefs".
///
/// Models often guess a specific `provider:model` and get the combo wrong; `auto`
/// (also `default`, `prefer`, `user`, `workshop`) opts into the user's configured
/// StageRoutingMatrix / host turn target instead.
pub fn is_auto_model_hint(hint: &str) -> bool {
    matches!(
        hint.trim().to_ascii_lowercase().as_str(),
        "" | "auto"
            | "default"
            | "prefer"
            | "preferred"
            | "user"
            | "workshop"
            | "host"
            | "inherit"
            | "none"
    )
}

/// Infer a provider when the model id is unambiguous without a catalog lookup.
///
/// Slash-form ids (`openai/gpt-oss-120b`) are left unresolved — those need an
/// explicit provider prefix because the left token is a namespace, not Medousa's
/// provider key.
pub fn infer_provider_for_model(model: &str) -> Option<String> {
    let model = model.trim();
    if model.is_empty() || model.contains('/') {
        return None;
    }
    let lower = model.to_ascii_lowercase();
    let provider = if lower.starts_with("deepseek") {
        "deepseek"
    } else if lower.starts_with("claude") {
        "anthropic"
    } else if lower.starts_with("gemini") {
        "google"
    } else if lower.starts_with("grok") {
        "xai"
    } else if lower.starts_with("gpt-")
        || lower.starts_with("o1")
        || lower.starts_with("o3")
        || lower.starts_with("o4")
        || lower.starts_with("chatgpt")
    {
        "openai"
    } else if lower.starts_with("mistral")
        || lower.starts_with("mixtral")
        || lower.starts_with("codestral")
        || lower.starts_with("pixtral")
        || lower.starts_with("ministral")
    {
        "mistral"
    } else if lower.starts_with("command-") || lower == "command" {
        "cohere"
    } else if lower.starts_with("sonar") {
        "perplexity"
    } else if lower.starts_with("kimi") || lower.starts_with("moonshot") {
        "moonshot"
    } else if lower.starts_with("qwen") {
        "qwen"
    } else if lower.starts_with("glm") {
        "zhipu"
    } else if lower.starts_with("minimax") {
        "minimax"
    } else {
        return None;
    };
    Some(provider.to_string())
}

/// Resolve a model hint against the host (session/turn) provider.
///
/// Precedence:
/// 1. `auto` / `default` / … → no override (`None`) so stage matrix / host win
/// 2. `provider:model` / `provider::model` → both sides
/// 3. bare model + unambiguous inference → inferred provider + model
/// 4. bare model otherwise → **host** provider + model (never the process default)
pub fn resolve_model_hint(hint: Option<&str>, host_provider: &str) -> Option<(String, String)> {
    let hint = hint.map(str::trim).filter(|value| !value.is_empty())?;
    if is_auto_model_hint(hint) {
        return None;
    }
    if let Some((provider, model)) = split_provider_model(hint) {
        if is_auto_model_hint(provider) || is_auto_model_hint(model) {
            return None;
        }
        return Some((
            crate::resolve_llm_provider(Some(provider)),
            crate::resolve_llm_model(Some(model)),
        ));
    }
    let model = crate::resolve_llm_model(Some(hint));
    let provider = infer_provider_for_model(&model)
        .unwrap_or_else(|| crate::resolve_llm_provider(Some(host_provider)));
    Some((provider, model))
}

/// Resolve base URL for a routed target without leaking another provider's endpoint.
pub fn resolve_route_base_url(
    target_provider: &str,
    host_provider: &str,
    host_base_url: Option<&str>,
) -> Option<String> {
    if target_provider
        .trim()
        .eq_ignore_ascii_case(host_provider.trim())
    {
        crate::resolve_llm_base_url(Some(target_provider), host_base_url)
    } else {
        crate::resolve_llm_base_url(Some(target_provider), None)
    }
}

/// Workshop / turn stage matrix, falling back to a host-cloned default matrix.
pub fn stage_matrix_for_host(host_provider: &str, host_model: &str) -> StageRoutingMatrix {
    crate::session::load_tui_defaults()
        .stage_routing
        .filter(|matrix| {
            !matrix.orchestrator.provider.trim().is_empty()
                && !matrix.orchestrator.model.trim().is_empty()
        })
        .unwrap_or_else(|| StageRoutingMatrix::default_for(host_provider, host_model))
}

/// Inputs for delegated (worker / job) LLM routing.
pub struct DelegatedLlmRoute<'a> {
    pub host_provider: &'a str,
    pub host_model: &'a str,
    pub model_hint: Option<&'a str>,
    pub stage_role: Option<&'a str>,
    /// When set, used for role lookup; otherwise [`stage_matrix_for_host`].
    pub stage_matrix: Option<&'a StageRoutingMatrix>,
}

/// Resolve provider+model for a delegated worker or job.
///
/// Order: explicit hint → stage role matrix → host turn target.
pub fn resolve_delegated_llm_route(req: DelegatedLlmRoute<'_>) -> (String, String) {
    if let Some(target) = resolve_model_hint(req.model_hint, req.host_provider) {
        return target;
    }

    let owned_matrix;
    let matrix = if let Some(matrix) = req.stage_matrix {
        matrix
    } else {
        owned_matrix = stage_matrix_for_host(req.host_provider, req.host_model);
        &owned_matrix
    };

    if let Some(role) = req
        .stage_role
        .map(str::trim)
        .filter(|value| !value.is_empty())
        && let Some(route) = matrix.get(role)
    {
        return (
            crate::resolve_llm_provider(Some(&route.provider)),
            crate::resolve_llm_model(Some(&route.model)),
        );
    }

    (
        crate::resolve_llm_provider(Some(req.host_provider)),
        crate::resolve_llm_model(Some(req.host_model)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_single_and_double_colon() {
        assert_eq!(
            split_provider_model("deepseek:deepseek-v4-flash"),
            Some(("deepseek", "deepseek-v4-flash"))
        );
        assert_eq!(
            split_provider_model("openai::gpt-5.6-luna"),
            Some(("openai", "gpt-5.6-luna"))
        );
        assert_eq!(split_provider_model("deepseek-v4-flash"), None);
    }

    #[test]
    fn auto_hint_defers_to_preferences() {
        assert!(resolve_model_hint(Some("auto"), "openai").is_none());
        assert!(resolve_model_hint(Some("DEFAULT"), "openai").is_none());
        assert!(resolve_model_hint(Some("auto:whatever"), "openai").is_none());
        assert!(is_auto_model_hint("prefer"));
    }

    #[test]
    fn bare_deepseek_hint_infers_provider_not_openai() {
        let (provider, model) =
            resolve_model_hint(Some("deepseek-v4-flash"), "openai").expect("hint");
        assert_eq!(provider, "deepseek");
        assert_eq!(model, "deepseek-v4-flash");
    }

    #[test]
    fn bare_unknown_hint_keeps_host_provider() {
        let (provider, model) =
            resolve_model_hint(Some("my-custom-finetune"), "anthropic").expect("hint");
        assert_eq!(provider, "anthropic");
        assert_eq!(model, "my-custom-finetune");
    }

    #[test]
    fn prefixed_hint_overrides_host() {
        let (provider, model) =
            resolve_model_hint(Some("anthropic:claude-sonnet-4"), "openai").expect("hint");
        assert_eq!(provider, "anthropic");
        assert_eq!(model, "claude-sonnet-4");
    }

    #[test]
    fn openai_api_and_chatgpt_oauth_routes_remain_distinct() {
        let api = resolve_model_hint(Some("openai:gpt-5.6-sol"), "anthropic").expect("api");
        let oauth =
            resolve_model_hint(Some("openai-codex:gpt-5.6-sol"), "anthropic").expect("oauth");
        assert_eq!(api, ("openai".into(), "gpt-5.6-sol".into()));
        assert_eq!(oauth, ("openai-codex".into(), "gpt-5.6-sol".into()));
        assert_ne!(api, oauth);
    }

    #[test]
    fn delegated_stage_route_preserves_chatgpt_oauth_provider() {
        let mut matrix = StageRoutingMatrix::default_for("openai", "gpt-5.6-sol");
        matrix.verifier.provider = "openai-codex".into();
        matrix.verifier.model = "gpt-5.6-sol".into();

        let route = resolve_delegated_llm_route(DelegatedLlmRoute {
            host_provider: "openai",
            host_model: "gpt-5.6-sol",
            model_hint: None,
            stage_role: Some("verifier"),
            stage_matrix: Some(&matrix),
        });

        assert_eq!(route, ("openai-codex".into(), "gpt-5.6-sol".into()));
    }

    #[test]
    fn cross_provider_base_url_does_not_inherit_host_endpoint() {
        let url = resolve_route_base_url("deepseek", "openai", Some("https://api.openai.com/v1"));
        // DeepSeek should resolve its own endpoint (or None/env), never OpenAI's.
        assert_ne!(url.as_deref(), Some("https://api.openai.com/v1"));
    }

    #[test]
    fn same_provider_keeps_host_base_url() {
        let url = resolve_route_base_url("openai", "openai", Some("https://proxy.example/v1"));
        assert_eq!(url.as_deref(), Some("https://proxy.example/v1"));
    }
}
