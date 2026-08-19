use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct StageRoute {
    pub role: String,
    pub provider: String,
    pub model: String,
    #[serde(alias = "policyProfile")]
    pub policy_profile: String,
    #[serde(alias = "fallbackChain")]
    pub fallback_chain: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct StageRoutingMatrix {
    pub orchestrator: StageRoute,
    pub chunker: StageRoute,
    pub extractor: StageRoute,
    pub summarizer: StageRoute,
    pub verifier: StageRoute,
    pub packer: StageRoute,
    #[serde(alias = "finalResponse")]
    pub final_response: StageRoute,
}

impl StageRoutingMatrix {
    pub fn default_for(provider: &str, model: &str) -> Self {
        let base_policy = "balanced".to_string();
        Self {
            orchestrator: make_route(
                "orchestrator",
                provider,
                model,
                "orchestrator",
                &base_policy,
            ),
            chunker: make_route("chunker", provider, model, "chunker", "fast"),
            extractor: make_route("extractor", provider, model, "extractor", "analytical"),
            summarizer: make_route("summarizer", provider, model, "summarizer", "balanced"),
            verifier: make_route("verifier", provider, model, "verifier", "strict"),
            packer: make_route("packer", provider, model, "packer", "balanced"),
            final_response: make_route(
                "final_response",
                provider,
                model,
                "final_response",
                "balanced",
            ),
        }
    }

    pub fn get(&self, role: &str) -> Option<&StageRoute> {
        match normalize_role(role).as_str() {
            "orchestrator" => Some(&self.orchestrator),
            "chunker" => Some(&self.chunker),
            "extractor" => Some(&self.extractor),
            "summarizer" => Some(&self.summarizer),
            "verifier" => Some(&self.verifier),
            "packer" => Some(&self.packer),
            "final_response" => Some(&self.final_response),
            _ => None,
        }
    }

    pub fn get_mut(&mut self, role: &str) -> Option<&mut StageRoute> {
        match normalize_role(role).as_str() {
            "orchestrator" => Some(&mut self.orchestrator),
            "chunker" => Some(&mut self.chunker),
            "extractor" => Some(&mut self.extractor),
            "summarizer" => Some(&mut self.summarizer),
            "verifier" => Some(&mut self.verifier),
            "packer" => Some(&mut self.packer),
            "final_response" => Some(&mut self.final_response),
            _ => None,
        }
    }

    pub fn roles() -> &'static [&'static str] {
        &[
            "orchestrator",
            "chunker",
            "extractor",
            "summarizer",
            "verifier",
            "packer",
            "final_response",
        ]
    }

    fn routes(&self) -> [&StageRoute; 7] {
        [
            &self.orchestrator,
            &self.chunker,
            &self.extractor,
            &self.summarizer,
            &self.verifier,
            &self.packer,
            &self.final_response,
        ]
    }

    /// When every role shares the same provider+model, that pair.
    ///
    /// A uniform matrix is almost always a clone of an old host Chat model, not
    /// a per-role customization.
    pub fn uniform_target(&self) -> Option<(&str, &str)> {
        let routes = self.routes();
        let provider = routes[0].provider.trim();
        let model = routes[0].model.trim();
        if provider.is_empty() || model.is_empty() {
            return None;
        }
        routes
            .iter()
            .all(|route| {
                route.provider.trim().eq_ignore_ascii_case(provider) && route.model.trim() == model
            })
            .then_some((provider, model))
    }

    /// Keep Chat on the host picker model.
    ///
    /// Settings → Models / the composer picker write `provider`+`model` (and the
    /// main inference profile) without rewriting a leftover uniform stage matrix.
    /// Host turns used `final_response`, so Chat kept calling DeepSeek after the
    /// picker showed GPT Luna. Rebase a uniform leftover clone; pin only
    /// `final_response` when roles actually differ.
    pub fn aligned_with_host(self, provider: &str, model: &str) -> Self {
        let provider = provider.trim();
        let model = model.trim();
        if provider.is_empty() || model.is_empty() {
            return self;
        }
        if let Some((current_provider, current_model)) = self.uniform_target()
            && (!current_provider.eq_ignore_ascii_case(provider) || current_model != model)
        {
            return Self::default_for(provider, model);
        }
        let mut aligned = self;
        if let Some(route) = aligned.get_mut("final_response") {
            route.provider = provider.to_string();
            route.model = model.to_string();
        }
        aligned
    }
}

pub fn normalize_role(role: &str) -> String {
    role.trim().to_ascii_lowercase().replace('-', "_")
}

fn make_route(role: &str, provider: &str, model: &str, fallback: &str, policy: &str) -> StageRoute {
    StageRoute {
        role: role.to_string(),
        provider: provider.to_string(),
        model: model.to_string(),
        policy_profile: policy.to_string(),
        fallback_chain: vec![fallback.to_string(), "safe-default".to_string()],
    }
}

#[cfg(test)]
mod tests {
    use super::StageRoutingMatrix;

    #[test]
    fn matrix_defaults_for_provider_model() {
        let matrix = StageRoutingMatrix::default_for("openai", "gpt-4o-mini");
        assert_eq!(matrix.verifier.provider, "openai");
        assert_eq!(matrix.verifier.model, "gpt-4o-mini");
        assert!(!matrix.verifier.fallback_chain.is_empty());
    }

    #[test]
    fn gets_role_mutably() {
        let mut matrix = StageRoutingMatrix::default_for("openai", "gpt-4o-mini");
        let route = matrix
            .get_mut("final-response")
            .expect("route should exist");
        route.model = "gpt-4.1-mini".to_string();
        assert_eq!(matrix.final_response.model, "gpt-4.1-mini");
    }

    #[test]
    fn uniform_stale_matrix_rebases_to_host_chat() {
        let aligned = StageRoutingMatrix::default_for("deepseek", "deepseek-v4-flash")
            .aligned_with_host("openai", "gpt-5.6-luna");
        assert_eq!(aligned.final_response.provider, "openai");
        assert_eq!(aligned.final_response.model, "gpt-5.6-luna");
        assert_eq!(aligned.extractor.provider, "openai");
        assert_eq!(aligned.extractor.model, "gpt-5.6-luna");
    }

    #[test]
    fn mixed_matrix_keeps_worker_role_and_pins_chat() {
        let mut matrix = StageRoutingMatrix::default_for("openai", "gpt-5.6-luna");
        matrix.extractor.provider = "deepseek".to_string();
        matrix.extractor.model = "deepseek-v4-flash".to_string();
        matrix.final_response.provider = "deepseek".to_string();
        matrix.final_response.model = "deepseek-v4-flash".to_string();
        let aligned = matrix.aligned_with_host("openai", "gpt-5.6-luna");
        assert_eq!(aligned.extractor.provider, "deepseek");
        assert_eq!(aligned.extractor.model, "deepseek-v4-flash");
        assert_eq!(aligned.final_response.provider, "openai");
        assert_eq!(aligned.final_response.model, "gpt-5.6-luna");
    }
}
