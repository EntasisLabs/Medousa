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

fn default_model_for_provider(provider: &str) -> &'static str {
    match provider.trim().to_ascii_lowercase().as_str() {
        "anthropic" => "claude-3-7-sonnet-latest",
        "google" | "gemini" => "gemini-2.5-pro",
        "xai" => "grok-3-mini",
        "ollama" => DEFAULT_OLLAMA_MODEL,
        _ => "gpt-4o-mini",
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

async fn validate_openai_key(client: &Client, api_key: &str) -> ProvidersValidateKeyResult {
    let response = client
        .get("https://api.openai.com/v1/models")
        .bearer_auth(api_key)
        .send()
        .await;

    match response {
        Ok(response) if response.status().is_success() => ProvidersValidateKeyResult {
            ok: true,
            message: "OpenAI key verified".to_string(),
            suggested_model: Some(default_model_for_provider("openai").to_string()),
        },
        Ok(response) if response.status().as_u16() == 401 => ProvidersValidateKeyResult {
            ok: false,
            message: "Invalid OpenAI API key".to_string(),
            suggested_model: None,
        },
        Ok(response) => ProvidersValidateKeyResult {
            ok: false,
            message: format!("OpenAI returned HTTP {}", response.status()),
            suggested_model: None,
        },
        Err(err) => ProvidersValidateKeyResult {
            ok: false,
            message: format!("Could not reach OpenAI: {err}"),
            suggested_model: None,
        },
    }
}

async fn validate_anthropic_key(client: &Client, api_key: &str) -> ProvidersValidateKeyResult {
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
            suggested_model: Some(default_model_for_provider("anthropic").to_string()),
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

async fn validate_google_key(client: &Client, api_key: &str) -> ProvidersValidateKeyResult {
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models?key={}",
        urlencoding::encode(api_key)
    );
    let response = client.get(url).send().await;

    match response {
        Ok(response) if response.status().is_success() => ProvidersValidateKeyResult {
            ok: true,
            message: "Google Gemini key verified".to_string(),
            suggested_model: Some(default_model_for_provider("google").to_string()),
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
    let provider = request.provider.trim().to_ascii_lowercase();
    let api_key = request.api_key.trim();

    if provider == "ollama" {
        let client = probe_http_client()?;
        return Ok(validate_ollama(&client, request.base_url.as_deref()).await);
    }

    if api_key.is_empty() {
        return Ok(ProvidersValidateKeyResult {
            ok: false,
            message: "API key is required".to_string(),
            suggested_model: None,
        });
    }

    let client = probe_http_client()?;
    let result = match provider.as_str() {
        "openai" => validate_openai_key(&client, api_key).await,
        "anthropic" => validate_anthropic_key(&client, api_key).await,
        "google" | "gemini" => validate_google_key(&client, api_key).await,
        other => ProvidersValidateKeyResult {
            ok: false,
            message: format!("Provider '{other}' validation is not supported yet"),
            suggested_model: None,
        },
    };
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_models_match_workshop() {
        assert_eq!(default_model_for_provider("openai"), "gpt-4o-mini");
        assert_eq!(default_model_for_provider("ollama"), DEFAULT_OLLAMA_MODEL);
    }
}
