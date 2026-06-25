use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InferenceTarget {
    pub provider: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InferenceProfile {
    pub provider: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default)]
    pub fallbacks: Vec<InferenceTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct InferenceProfilesConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main: Option<InferenceProfile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vision: Option<InferenceProfile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stt: Option<InferenceProfile>,
}

pub const MAX_FALLBACKS: usize = 2;

impl InferenceTarget {
    pub fn trimmed(&self) -> Option<Self> {
        let provider = self.provider.trim();
        let model = self.model.trim();
        if provider.is_empty() || model.is_empty() {
            return None;
        }
        Some(Self {
            provider: provider.to_string(),
            model: model.to_string(),
            base_url: self
                .base_url
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
        })
    }
}

impl InferenceProfile {
    pub fn trimmed(&self) -> Option<Self> {
        let provider = self.provider.trim();
        let model = self.model.trim();
        if provider.is_empty() || model.is_empty() {
            return None;
        }
        let fallbacks = self
            .fallbacks
            .iter()
            .filter_map(|target| target.trimmed())
            .take(MAX_FALLBACKS)
            .collect();
        Some(Self {
            provider: provider.to_string(),
            model: model.to_string(),
            base_url: self
                .base_url
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
            fallbacks,
        })
    }

    pub fn as_target(&self) -> InferenceTarget {
        InferenceTarget {
            provider: self.provider.clone(),
            model: self.model.clone(),
            base_url: self.base_url.clone(),
        }
    }
}
