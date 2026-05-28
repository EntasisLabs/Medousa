use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StageRoute {
    pub role: String,
    pub provider: String,
    pub model: String,
    pub policy_profile: String,
    pub fallback_chain: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StageRoutingMatrix {
    pub orchestrator: StageRoute,
    pub chunker: StageRoute,
    pub extractor: StageRoute,
    pub summarizer: StageRoute,
    pub verifier: StageRoute,
    pub packer: StageRoute,
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
}
