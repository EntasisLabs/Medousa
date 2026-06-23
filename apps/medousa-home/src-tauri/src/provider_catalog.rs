//! Curated LLM provider catalog for Home — aligned with rust-genai provider ids.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderValidation {
    OpenAiCompatible,
    Anthropic,
    Google,
    Ollama,
    /// Key stored; no remote probe (exotic or auth-heavy providers).
    AcceptKey,
}

#[derive(Debug, Clone)]
pub struct ProviderSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub category: &'static str,
    pub default_model: &'static str,
    pub needs_api_key: bool,
    pub supports_custom_base_url: bool,
    pub default_base_url: Option<&'static str>,
    pub key_hint: Option<&'static str>,
    pub blurb: &'static str,
    pub validation: ProviderValidation,
}

const PROVIDERS: &[ProviderSpec] = &[
    ProviderSpec {
        id: "openai",
        label: "OpenAI",
        category: "featured",
        default_model: "gpt-5.4-mini",
        needs_api_key: true,
        supports_custom_base_url: true,
        default_base_url: Some("https://api.openai.com/v1"),
        key_hint: Some("sk-…"),
        blurb: "GPT-5.4 family and reasoning models",
        validation: ProviderValidation::OpenAiCompatible,
    },
    ProviderSpec {
        id: "anthropic",
        label: "Anthropic",
        category: "featured",
        default_model: "claude-sonnet-4-6",
        needs_api_key: true,
        supports_custom_base_url: false,
        default_base_url: None,
        key_hint: Some("sk-ant-…"),
        blurb: "Claude 4.6 Opus, Sonnet, and Haiku",
        validation: ProviderValidation::Anthropic,
    },
    ProviderSpec {
        id: "google",
        label: "Google Gemini",
        category: "featured",
        default_model: "gemini-3.1-pro-preview",
        needs_api_key: true,
        supports_custom_base_url: false,
        default_base_url: None,
        key_hint: Some("AI…"),
        blurb: "Gemini 3.1 Pro and Flash",
        validation: ProviderValidation::Google,
    },
    ProviderSpec {
        id: "deepseek",
        label: "DeepSeek",
        category: "featured",
        default_model: "deepseek-v4-flash",
        needs_api_key: true,
        supports_custom_base_url: true,
        default_base_url: Some("https://api.deepseek.com/v1"),
        key_hint: Some("sk-…"),
        blurb: "DeepSeek V4 Pro and Flash",
        validation: ProviderValidation::OpenAiCompatible,
    },
    ProviderSpec {
        id: "groq",
        label: "Groq",
        category: "featured",
        default_model: "openai/gpt-oss-120b",
        needs_api_key: true,
        supports_custom_base_url: true,
        default_base_url: Some("https://api.groq.com/openai/v1"),
        key_hint: Some("gsk_…"),
        blurb: "Fast hosted open-weight models",
        validation: ProviderValidation::OpenAiCompatible,
    },
    ProviderSpec {
        id: "xai",
        label: "xAI",
        category: "featured",
        default_model: "grok-4-1-fast",
        needs_api_key: true,
        supports_custom_base_url: true,
        default_base_url: Some("https://api.x.ai/v1"),
        key_hint: Some("xai-…"),
        blurb: "Grok 4 and 4.1 Fast",
        validation: ProviderValidation::OpenAiCompatible,
    },
    ProviderSpec {
        id: "ollama",
        label: "Ollama (local)",
        category: "local",
        default_model: "llama3.2",
        needs_api_key: false,
        supports_custom_base_url: true,
        default_base_url: Some("http://127.0.0.1:11434/v1"),
        key_hint: None,
        blurb: "Local Ollama or Ollama Cloud — set the API URL below",
        validation: ProviderValidation::Ollama,
    },
    ProviderSpec {
        id: "custom",
        label: "Custom provider",
        category: "local",
        default_model: "default",
        needs_api_key: true,
        supports_custom_base_url: true,
        default_base_url: None,
        key_hint: Some("sk-…"),
        blurb: "Any OpenAI-compatible endpoint — vLLM, LiteLLM, etc.",
        validation: ProviderValidation::OpenAiCompatible,
    },
    ProviderSpec {
        id: "medousa-local",
        label: "Medousa private brain",
        category: "local",
        default_model: "gemma-4-12b-it",
        needs_api_key: false,
        supports_custom_base_url: true,
        default_base_url: Some("http://127.0.0.1:7421/v1"),
        key_hint: None,
        blurb: "Embedded Gemma via Medousa Engine",
        validation: ProviderValidation::AcceptKey,
    },
    ProviderSpec {
        id: "mistral",
        label: "Mistral",
        category: "cloud",
        default_model: "mistral-large-latest",
        needs_api_key: true,
        supports_custom_base_url: true,
        default_base_url: Some("https://api.mistral.ai/v1"),
        key_hint: Some("…"),
        blurb: "Mistral large and small models",
        validation: ProviderValidation::OpenAiCompatible,
    },
    ProviderSpec {
        id: "openrouter",
        label: "OpenRouter",
        category: "cloud",
        default_model: "openai/gpt-5.4-mini",
        needs_api_key: true,
        supports_custom_base_url: true,
        default_base_url: Some("https://openrouter.ai/api/v1"),
        key_hint: Some("sk-or-…"),
        blurb: "Unified router to many models",
        validation: ProviderValidation::OpenAiCompatible,
    },
    ProviderSpec {
        id: "together",
        label: "Together AI",
        category: "cloud",
        default_model: "meta-llama/Llama-3.3-70B-Instruct-Turbo",
        needs_api_key: true,
        supports_custom_base_url: true,
        default_base_url: Some("https://api.together.xyz/v1"),
        key_hint: Some("…"),
        blurb: "Open model hosting",
        validation: ProviderValidation::OpenAiCompatible,
    },
    ProviderSpec {
        id: "fireworks",
        label: "Fireworks AI",
        category: "cloud",
        default_model: "accounts/fireworks/models/llama-v3p3-70b-instruct",
        needs_api_key: true,
        supports_custom_base_url: true,
        default_base_url: Some("https://api.fireworks.ai/inference/v1"),
        key_hint: Some("fw_…"),
        blurb: "Fast inference for open weights",
        validation: ProviderValidation::OpenAiCompatible,
    },
    ProviderSpec {
        id: "perplexity",
        label: "Perplexity",
        category: "cloud",
        default_model: "sonar",
        needs_api_key: true,
        supports_custom_base_url: true,
        default_base_url: Some("https://api.perplexity.ai"),
        key_hint: Some("pplx-…"),
        blurb: "Search-grounded models",
        validation: ProviderValidation::OpenAiCompatible,
    },
    ProviderSpec {
        id: "cohere",
        label: "Cohere",
        category: "cloud",
        default_model: "command-r-plus",
        needs_api_key: true,
        supports_custom_base_url: false,
        default_base_url: None,
        key_hint: Some("…"),
        blurb: "Command and embed models",
        validation: ProviderValidation::AcceptKey,
    },
    ProviderSpec {
        id: "azure-openai",
        label: "Azure OpenAI",
        category: "cloud",
        default_model: "gpt-5.4-mini",
        needs_api_key: true,
        supports_custom_base_url: true,
        default_base_url: None,
        key_hint: Some("Azure key"),
        blurb: "Enterprise OpenAI deployments — set your resource URL",
        validation: ProviderValidation::OpenAiCompatible,
    },
    ProviderSpec {
        id: "cerebras",
        label: "Cerebras",
        category: "cloud",
        default_model: "llama-3.3-70b",
        needs_api_key: true,
        supports_custom_base_url: true,
        default_base_url: Some("https://api.cerebras.ai/v1"),
        key_hint: Some("csk-…"),
        blurb: "Cerebras-hosted inference",
        validation: ProviderValidation::OpenAiCompatible,
    },
    ProviderSpec {
        id: "hyperbolic",
        label: "Hyperbolic",
        category: "cloud",
        default_model: "meta-llama/Meta-Llama-3.1-70B-Instruct",
        needs_api_key: true,
        supports_custom_base_url: true,
        default_base_url: Some("https://api.hyperbolic.xyz/v1"),
        key_hint: Some("…"),
        blurb: "GPU cloud inference",
        validation: ProviderValidation::OpenAiCompatible,
    },
    ProviderSpec {
        id: "huggingface",
        label: "Hugging Face",
        category: "cloud",
        default_model: "meta-llama/Meta-Llama-3-8B-Instruct",
        needs_api_key: true,
        supports_custom_base_url: true,
        default_base_url: Some("https://api-inference.huggingface.co/v1"),
        key_hint: Some("hf_…"),
        blurb: "Inference API and hosted models",
        validation: ProviderValidation::AcceptKey,
    },
    ProviderSpec {
        id: "replicate",
        label: "Replicate",
        category: "cloud",
        default_model: "meta/meta-llama-3-8b-instruct",
        needs_api_key: true,
        supports_custom_base_url: false,
        default_base_url: None,
        key_hint: Some("r8_…"),
        blurb: "Run open models via Replicate",
        validation: ProviderValidation::AcceptKey,
    },
    ProviderSpec {
        id: "moonshot",
        label: "Moonshot (Kimi)",
        category: "cloud",
        default_model: "moonshot-v1-8k",
        needs_api_key: true,
        supports_custom_base_url: true,
        default_base_url: Some("https://api.moonshot.cn/v1"),
        key_hint: Some("sk-…"),
        blurb: "Kimi models",
        validation: ProviderValidation::OpenAiCompatible,
    },
    ProviderSpec {
        id: "qwen",
        label: "Qwen (DashScope)",
        category: "cloud",
        default_model: "qwen-plus",
        needs_api_key: true,
        supports_custom_base_url: true,
        default_base_url: Some("https://dashscope.aliyuncs.com/compatible-mode/v1"),
        key_hint: Some("sk-…"),
        blurb: "Alibaba Qwen models",
        validation: ProviderValidation::OpenAiCompatible,
    },
    ProviderSpec {
        id: "zhipu",
        label: "Zhipu GLM",
        category: "cloud",
        default_model: "glm-4-plus",
        needs_api_key: true,
        supports_custom_base_url: true,
        default_base_url: Some("https://open.bigmodel.cn/api/paas/v4"),
        key_hint: Some("…"),
        blurb: "GLM family",
        validation: ProviderValidation::AcceptKey,
    },
    ProviderSpec {
        id: "minimax",
        label: "MiniMax",
        category: "cloud",
        default_model: "abab6.5s-chat",
        needs_api_key: true,
        supports_custom_base_url: true,
        default_base_url: None,
        key_hint: Some("…"),
        blurb: "MiniMax chat models",
        validation: ProviderValidation::AcceptKey,
    },
    ProviderSpec {
        id: "bedrock",
        label: "AWS Bedrock",
        category: "cloud",
        default_model: "anthropic.claude-sonnet-4-6",
        needs_api_key: false,
        supports_custom_base_url: false,
        default_base_url: None,
        key_hint: None,
        blurb: "Use AWS credentials via env — not API key in app",
        validation: ProviderValidation::AcceptKey,
    },
    ProviderSpec {
        id: "gemini",
        label: "Gemini (alias)",
        category: "cloud",
        default_model: "gemini-3.1-pro-preview",
        needs_api_key: true,
        supports_custom_base_url: false,
        default_base_url: None,
        key_hint: Some("AI…"),
        blurb: "Same as Google — genai id alias",
        validation: ProviderValidation::Google,
    },
];

pub fn find_provider(id: &str) -> Option<&'static ProviderSpec> {
    let normalized = id.trim().to_ascii_lowercase();
    PROVIDERS.iter().find(|entry| entry.id.eq_ignore_ascii_case(&normalized))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCategory {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCatalogEntry {
    pub id: String,
    pub label: String,
    pub category: String,
    pub default_model: String,
    pub needs_api_key: bool,
    pub supports_custom_base_url: bool,
    pub default_base_url: Option<String>,
    pub key_hint: Option<String>,
    pub blurb: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvidersListResult {
    pub categories: Vec<ProviderCategory>,
    pub providers: Vec<ProviderCatalogEntry>,
}

pub fn providers_catalog() -> ProvidersListResult {
    ProvidersListResult {
        categories: vec![
            ProviderCategory {
                id: "featured".to_string(),
                label: "Popular".to_string(),
            },
            ProviderCategory {
                id: "local".to_string(),
                label: "On this device".to_string(),
            },
            ProviderCategory {
                id: "cloud".to_string(),
                label: "More providers".to_string(),
            },
        ],
        providers: PROVIDERS
            .iter()
            .map(|spec| ProviderCatalogEntry {
                id: spec.id.to_string(),
                label: spec.label.to_string(),
                category: spec.category.to_string(),
                default_model: spec.default_model.to_string(),
                needs_api_key: spec.needs_api_key,
                supports_custom_base_url: spec.supports_custom_base_url,
                default_base_url: spec.default_base_url.map(str::to_string),
                key_hint: spec.key_hint.map(str::to_string),
                blurb: spec.blurb.to_string(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_includes_deepseek_and_ollama() {
        assert!(find_provider("deepseek").is_some());
        assert!(find_provider("ollama").is_some());
        assert!(find_provider("custom").is_some());
    }

    #[test]
    fn catalog_serializes() {
        let catalog = providers_catalog();
        assert!(catalog.providers.len() >= 20);
        let json = serde_json::to_string(&catalog).expect("serialize");
        assert!(json.contains("deepseek"));
    }
}
