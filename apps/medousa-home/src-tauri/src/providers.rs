use crate::provider_catalog::{self, ProviderValidation, ProvidersListResult};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

const DEFAULT_OLLAMA_BASE_URL: &str = "http://127.0.0.1:11434/v1/";
const OLLAMA_TAGS_URL: &str = "http://127.0.0.1:11434/api/tags";
const DEFAULT_OLLAMA_MODEL: &str = "llama3.2";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvidersProbeResult {
    pub ollama_detected: bool,
    pub ollama_base_url: String,
    pub ollama_models: Vec<String>,
    pub network_online: bool,
    pub suggested_ollama_model: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvidersValidateKeyRequest {
    pub provider: String,
    pub api_key: String,
    #[serde(default)]
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvidersValidateKeyResult {
    pub ok: bool,
    pub message: String,
    pub suggested_model: Option<String>,
}

fn probe_http_client() -> Result<Client, String> {
    Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|err| err.to_string())
}

fn detect_local_ollama_tcp() -> bool {
    if let Ok(mut addrs) = "127.0.0.1:11434".to_socket_addrs() {
        if let Some(addr) = addrs.next() {
            return TcpStream::connect_timeout(&addr, Duration::from_millis(250)).is_ok();
        }
    }
    false
}

fn default_model_for_provider(provider: &str) -> String {
    provider_catalog::find_provider(provider)
        .map(|spec| spec.default_model.to_string())
        .unwrap_or_else(|| "gpt-5.4-mini".to_string())
}

fn resolve_base_url(
    spec: Option<&provider_catalog::ProviderSpec>,
    request_base: Option<&str>,
) -> Option<String> {
    request_base
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            spec.and_then(|entry| entry.default_base_url.map(str::to_string))
        })
}

fn models_url(base: &str) -> String {
    let trimmed = base.trim().trim_end_matches('/');
    if trimmed.ends_with("/v1") {
        format!("{trimmed}/models")
    } else {
        format!("{trimmed}/v1/models")
    }
}

async fn fetch_ollama_models(client: &Client) -> Vec<String> {
    let response = match client.get(OLLAMA_TAGS_URL).send().await {
        Ok(response) if response.status().is_success() => response,
        _ => return Vec::new(),
    };
    let payload: serde_json::Value = match response.json().await {
        Ok(value) => value,
        Err(_) => return Vec::new(),
    };
    payload
        .get("models")
        .and_then(|models| models.as_array())
        .map(|models| {
            models
                .iter()
                .filter_map(|entry| {
                    entry
                        .get("name")
                        .and_then(|name| name.as_str())
                        .map(str::trim)
                        .filter(|name| !name.is_empty())
                        .map(str::to_string)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

async fn probe_network_online(client: &Client) -> bool {
    client
        .head("https://cloudflare.com/cdn-cgi/trace")
        .send()
        .await
        .map(|response| response.status().is_success() || response.status().is_redirection())
        .unwrap_or(false)
}

#[tauri::command]
pub fn providers_list() -> ProvidersListResult {
    provider_catalog::providers_catalog()
}

#[tauri::command]
pub async fn providers_probe() -> Result<ProvidersProbeResult, String> {
    let client = probe_http_client()?;
    let ollama_detected = detect_local_ollama_tcp();
    let ollama_models = if ollama_detected {
        fetch_ollama_models(&client).await
    } else {
        Vec::new()
    };
    let suggested_ollama_model = ollama_models
        .iter()
        .find(|name| name.contains("llama3.2") || name.starts_with("llama3"))
        .or_else(|| ollama_models.first())
        .cloned()
        .or_else(|| ollama_detected.then(|| DEFAULT_OLLAMA_MODEL.to_string()));
    let network_online = probe_network_online(&client).await;

    Ok(ProvidersProbeResult {
        ollama_detected,
        ollama_base_url: DEFAULT_OLLAMA_BASE_URL.to_string(),
        ollama_models,
        network_online,
        suggested_ollama_model,
    })
}

async fn validate_openai_compatible(
    client: &Client,
    api_key: &str,
    base_url: &str,
    provider_label: &str,
    suggested_model: &str,
) -> ProvidersValidateKeyResult {
    let url = models_url(base_url);
    let response = client.get(url).bearer_auth(api_key).send().await;

    match response {
        Ok(response) if response.status().is_success() => ProvidersValidateKeyResult {
            ok: true,
            message: format!("{provider_label} key verified"),
            suggested_model: Some(suggested_model.to_string()),
        },
        Ok(response) if response.status().as_u16() == 401 => ProvidersValidateKeyResult {
            ok: false,
            message: format!("Invalid {provider_label} API key"),
            suggested_model: None,
        },
        Ok(response) => ProvidersValidateKeyResult {
            ok: false,
            message: format!("{provider_label} returned HTTP {}", response.status()),
            suggested_model: None,
        },
        Err(err) => ProvidersValidateKeyResult {
            ok: false,
            message: format!("Could not reach {provider_label}: {err}"),
            suggested_model: None,
        },
    }
}

async fn validate_anthropic_key(
    client: &Client,
    api_key: &str,
    suggested_model: &str,
) -> ProvidersValidateKeyResult {
    let response = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&serde_json::json!({
            "model": "claude-3-5-haiku-latest",
            "max_tokens": 1,
            "messages": [{"role": "user", "content": "ping"}]
        }))
        .send()
        .await;

    match response {
        Ok(response) if response.status().is_success() => ProvidersValidateKeyResult {
            ok: true,
            message: "Anthropic key verified".to_string(),
            suggested_model: Some(suggested_model.to_string()),
        },
        Ok(response) if response.status().as_u16() == 401 => ProvidersValidateKeyResult {
            ok: false,
            message: "Invalid Anthropic API key".to_string(),
            suggested_model: None,
        },
        Ok(response) => ProvidersValidateKeyResult {
            ok: false,
            message: format!("Anthropic returned HTTP {}", response.status()),
            suggested_model: None,
        },
        Err(err) => ProvidersValidateKeyResult {
            ok: false,
            message: format!("Could not reach Anthropic: {err}"),
            suggested_model: None,
        },
    }
}

async fn validate_google_key(
    client: &Client,
    api_key: &str,
    suggested_model: &str,
) -> ProvidersValidateKeyResult {
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models?key={}",
        urlencoding::encode(api_key)
    );
    let response = client.get(url).send().await;

    match response {
        Ok(response) if response.status().is_success() => ProvidersValidateKeyResult {
            ok: true,
            message: "Google Gemini key verified".to_string(),
            suggested_model: Some(suggested_model.to_string()),
        },
        Ok(response) if response.status().as_u16() == 400 || response.status().as_u16() == 403 => {
            ProvidersValidateKeyResult {
                ok: false,
                message: "Invalid Google Gemini API key".to_string(),
                suggested_model: None,
            }
        }
        Ok(response) => ProvidersValidateKeyResult {
            ok: false,
            message: format!("Google returned HTTP {}", response.status()),
            suggested_model: None,
        },
        Err(err) => ProvidersValidateKeyResult {
            ok: false,
            message: format!("Could not reach Google: {err}"),
            suggested_model: None,
        },
    }
}

async fn validate_ollama(client: &Client, base_url: Option<&str>) -> ProvidersValidateKeyResult {
    let detected = detect_local_ollama_tcp();
    if !detected {
        return ProvidersValidateKeyResult {
            ok: false,
            message: "Ollama is not running on 127.0.0.1:11434 — install from ollama.com and run `ollama serve`".to_string(),
            suggested_model: None,
        };
    }

    let models = fetch_ollama_models(client).await;
    let suggested = models
        .iter()
        .find(|name| name.contains("llama3.2") || name.starts_with("llama3"))
        .or_else(|| models.first())
        .cloned()
        .unwrap_or_else(|| DEFAULT_OLLAMA_MODEL.to_string());

    let base = base_url
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_OLLAMA_BASE_URL);

    ProvidersValidateKeyResult {
        ok: true,
        message: format!("Ollama ready at {base}"),
        suggested_model: Some(suggested),
    }
}

#[tauri::command]
pub async fn providers_validate_key(
    request: ProvidersValidateKeyRequest,
) -> Result<ProvidersValidateKeyResult, String> {
    let provider_id = request.provider.trim().to_ascii_lowercase();
    let api_key = request.api_key.trim();
    let spec = provider_catalog::find_provider(&provider_id);
    let suggested_model = default_model_for_provider(&provider_id);

    if provider_id == "ollama" {
        let client = probe_http_client()?;
        return Ok(validate_ollama(&client, request.base_url.as_deref()).await);
    }

    if provider_id == "medousa-local" {
        return Ok(ProvidersValidateKeyResult {
            ok: true,
            message: "Use Settings → Voice to download and load the private brain".to_string(),
            suggested_model: Some(suggested_model),
        });
    }

    if provider_id == "bedrock" {
        return Ok(ProvidersValidateKeyResult {
            ok: true,
            message: "Configure AWS credentials in your environment — no API key stored here".to_string(),
            suggested_model: Some(suggested_model),
        });
    }

    let needs_key = spec.map(|entry| entry.needs_api_key).unwrap_or(true);
    if needs_key && api_key.is_empty() {
        return Ok(ProvidersValidateKeyResult {
            ok: false,
            message: "API key is required".to_string(),
            suggested_model: None,
        });
    }

    let client = probe_http_client()?;
    let label = spec.map(|entry| entry.label).unwrap_or(&provider_id);

    let result = match spec.map(|entry| entry.validation) {
        Some(ProviderValidation::Ollama) => validate_ollama(&client, request.base_url.as_deref()).await,
        Some(ProviderValidation::Anthropic) => {
            validate_anthropic_key(&client, api_key, &suggested_model).await
        }
        Some(ProviderValidation::Google) => {
            validate_google_key(&client, api_key, &suggested_model).await
        }
        Some(ProviderValidation::OpenAiCompatible) => {
            let Some(base) = resolve_base_url(spec, request.base_url.as_deref()) else {
                return Ok(ProvidersValidateKeyResult {
                    ok: false,
                    message: format!("Set an API base URL for {label}"),
                    suggested_model: None,
                });
            };
            validate_openai_compatible(&client, api_key, &base, label, &suggested_model).await
        }
        Some(ProviderValidation::AcceptKey) | None => ProvidersValidateKeyResult {
            ok: true,
            message: format!(
                "{label} key saved — verify on your first chat turn"
            ),
            suggested_model: Some(suggested_model),
        },
    };

    Ok(result)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvidersListModelsRequest {
    pub provider: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvidersListModelsResult {
    pub models: Vec<String>,
    pub source: String,
}

async fn fetch_openai_compatible_model_ids(
    client: &Client,
    api_key: &str,
    base_url: &str,
) -> Result<Vec<String>, String> {
    let url = models_url(base_url);
    let response = client
        .get(url)
        .bearer_auth(api_key)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(format!("Provider returned HTTP {}", response.status()));
    }
    let payload: serde_json::Value = response.json().await.map_err(|err| err.to_string())?;
    let models = payload
        .get("data")
        .and_then(|data| data.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| {
                    entry
                        .get("id")
                        .and_then(|id| id.as_str())
                        .map(str::trim)
                        .filter(|id| !id.is_empty())
                        .map(str::to_string)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(models)
}

#[tauri::command]
pub async fn providers_list_models(
    request: ProvidersListModelsRequest,
) -> Result<ProvidersListModelsResult, String> {
    let provider_id = request.provider.trim().to_ascii_lowercase();
    let spec = provider_catalog::find_provider(&provider_id);
    let client = probe_http_client()?;

    if provider_id == "ollama" {
        let models = fetch_ollama_models(&client).await;
        return Ok(ProvidersListModelsResult {
            source: "ollama.tags".to_string(),
            models,
        });
    }

    if provider_id == "medousa-local" {
        return Ok(ProvidersListModelsResult {
            source: "catalog.default".to_string(),
            models: vec![default_model_for_provider(&provider_id)],
        });
    }

    let api_key = request
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| load_provider_api_key_for_listing(&provider_id))
        .unwrap_or_default();
    let needs_key = spec.map(|entry| entry.needs_api_key).unwrap_or(false);
    if needs_key && api_key.trim().is_empty() {
        return Err("API key is required to list models — add one in Settings → Models".to_string());
    }

    let base_url = resolve_base_url(spec, request.base_url.as_deref())
        .or_else(|| read_tui_defaults_base_url(&provider_id))
        .or_else(|| load_provider_base_url_for_listing(&provider_id));

    let validation = spec.map(|entry| entry.validation);
    match validation {
        Some(ProviderValidation::Google) => {
            let url = format!(
                "https://generativelanguage.googleapis.com/v1beta/models?key={}",
                urlencoding::encode(api_key.trim())
            );
            let response = client.get(url).send().await.map_err(|err| err.to_string())?;
            if !response.status().is_success() {
                return Err(format!("Google returned HTTP {}", response.status()));
            }
            let payload: serde_json::Value = response.json().await.map_err(|err| err.to_string())?;
            let models = payload
                .get("models")
                .and_then(|models| models.as_array())
                .map(|entries| {
                    entries
                        .iter()
                        .filter_map(|entry| {
                            entry
                                .get("name")
                                .and_then(|name| name.as_str())
                                .map(|name| name.trim_start_matches("models/").to_string())
                                .filter(|name| !name.is_empty())
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            Ok(ProvidersListModelsResult {
                source: "google.models".to_string(),
                models,
            })
        }
        Some(ProviderValidation::OpenAiCompatible) => {
            let Some(base) = base_url else {
                return Err("Set an API base URL for this provider".to_string());
            };
            let models = fetch_openai_compatible_model_ids(&client, api_key.trim(), &base).await?;
            Ok(ProvidersListModelsResult {
                source: "openai_compatible.models".to_string(),
                models,
            })
        }
        Some(ProviderValidation::Ollama) => {
            let models = fetch_ollama_models(&client).await;
            Ok(ProvidersListModelsResult {
                source: "ollama.tags".to_string(),
                models,
            })
        }
        None if base_url.is_some() => {
            let base = base_url.expect("checked above");
            let models = fetch_openai_compatible_model_ids(&client, api_key.trim(), &base).await?;
            Ok(ProvidersListModelsResult {
                source: "openai_compatible.models".to_string(),
                models,
            })
        }
        _ => Ok(ProvidersListModelsResult {
            source: "unsupported".to_string(),
            models: vec![default_model_for_provider(&provider_id)],
        }),
    }
}

fn load_provider_base_url_for_listing(provider_id: &str) -> Option<String> {
    let secret_id = format!("base_url_{}", provider_id.trim().to_ascii_lowercase());
    crate::messaging::secrets::load_secret_value(&secret_id)
        .ok()
        .flatten()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn load_provider_api_key_for_listing(provider_id: &str) -> Option<String> {
    let provider_id = provider_id.trim().to_ascii_lowercase();
    if provider_id.is_empty() {
        return None;
    }
    let per_provider = format!("api_key_{provider_id}");
    if let Ok(Some(value)) = crate::messaging::secrets::load_secret_value(&per_provider) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    crate::messaging::secrets::load_secret_value("api_key")
        .ok()
        .flatten()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn read_tui_defaults_base_url(provider_id: &str) -> Option<String> {
    let defaults = crate::medousa_paths::load_tui_defaults();
    let configured_provider = defaults
        .provider
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let target = provider_id.trim().to_ascii_lowercase();
    let from_file = if configured_provider == target {
        defaults
            .base_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    } else {
        None
    };
    from_file.or_else(|| {
        provider_catalog::find_provider(provider_id)
            .and_then(|entry| entry.default_base_url.map(str::to_string))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_models_from_catalog() {
        assert_eq!(default_model_for_provider("deepseek"), "deepseek-chat");
        assert_eq!(default_model_for_provider("ollama"), DEFAULT_OLLAMA_MODEL);
    }

    #[test]
    fn models_url_normalizes_base() {
        assert_eq!(
            models_url("https://api.deepseek.com/v1"),
            "https://api.deepseek.com/v1/models"
        );
        assert_eq!(
            models_url("https://api.openai.com"),
            "https://api.openai.com/v1/models"
        );
    }
}
