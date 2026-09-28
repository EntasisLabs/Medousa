//! Laya `/v1/systemone` adapter for Medousa's typed System 1 boundary.

use std::collections::BTreeMap;
use std::sync::LazyLock;
use std::time::Duration;

use async_trait::async_trait;
use medousa_runtime::{
    SystemOneDecision, SystemOneEngine, SystemOneError, SystemOneInput, TurnIntent,
};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SYSTEM_ONE_ENGINE_ENV: &str = "MEDOUSA_SYSTEM_ONE_ENGINE";
pub const LAYA_BASE_URL_ENV: &str = "MEDOUSA_LAYA_BASE_URL";
pub const LAYA_API_KEY_ENV: &str = "MEDOUSA_LAYA_API_KEY";
pub const LAYA_MODEL_ENV: &str = "MEDOUSA_LAYA_MODEL";
pub const LAYA_TIMEOUT_MS_ENV: &str = "MEDOUSA_LAYA_TIMEOUT_MS";

const DEFAULT_LAYA_BASE_URL: &str = "http://127.0.0.1:8000";
const DEFAULT_LAYA_MODEL: &str = "english";
const DEFAULT_LAYA_TIMEOUT_MS: u64 = 2_000;
const MIN_LAYA_TIMEOUT_MS: u64 = 100;
const MAX_LAYA_TIMEOUT_MS: u64 = 10_000;
const ENGINE_ID: &str = "laya-system-one";
const QUESTION_ID: &str = "intent";

static SELECTED_LAYA_ENGINE: LazyLock<Result<Option<LayaSystemOneEngine>, String>> =
    LazyLock::new(|| LayaSystemOneEngine::load_from_env().map_err(|error| error.to_string()));

#[derive(Debug, Clone)]
pub struct LayaSystemOneConfig {
    endpoint: Url,
    api_key: Option<String>,
    model: String,
    timeout: Duration,
}

impl LayaSystemOneConfig {
    fn from_env() -> Result<Self, SystemOneError> {
        let base_url =
            std::env::var(LAYA_BASE_URL_ENV).unwrap_or_else(|_| DEFAULT_LAYA_BASE_URL.to_string());
        let model = std::env::var(LAYA_MODEL_ENV)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_LAYA_MODEL.to_string());
        let api_key = std::env::var(LAYA_API_KEY_ENV)
            .ok()
            .filter(|value| !value.trim().is_empty());
        let timeout_ms = std::env::var(LAYA_TIMEOUT_MS_ENV)
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(DEFAULT_LAYA_TIMEOUT_MS)
            .clamp(MIN_LAYA_TIMEOUT_MS, MAX_LAYA_TIMEOUT_MS);
        Self::new(&base_url, api_key, model, Duration::from_millis(timeout_ms))
    }

    fn new(
        base_url: &str,
        api_key: Option<String>,
        model: String,
        timeout: Duration,
    ) -> Result<Self, SystemOneError> {
        let endpoint = system_one_endpoint(base_url)?;
        Ok(Self {
            endpoint,
            api_key,
            model,
            timeout,
        })
    }
}

#[derive(Debug, Clone)]
pub struct LayaSystemOneEngine {
    client: reqwest::Client,
    config: LayaSystemOneConfig,
}

impl LayaSystemOneEngine {
    pub fn from_env_if_selected() -> Result<Option<&'static Self>, SystemOneError> {
        match &*SELECTED_LAYA_ENGINE {
            Ok(engine) => Ok(engine.as_ref()),
            Err(error) => Err(SystemOneError::Engine(error.clone())),
        }
    }

    fn load_from_env() -> Result<Option<Self>, SystemOneError> {
        let selection = std::env::var(SYSTEM_ONE_ENGINE_ENV).unwrap_or_else(|_| "host".to_string());
        match selection.trim().to_ascii_lowercase().as_str() {
            "host" | "host_model" | "" => Ok(None),
            "laya" => Self::new(LayaSystemOneConfig::from_env()?).map(Some),
            other => Err(SystemOneError::Engine(format!(
                "unknown {SYSTEM_ONE_ENGINE_ENV} value: {other}"
            ))),
        }
    }

    fn new(config: LayaSystemOneConfig) -> Result<Self, SystemOneError> {
        let client = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(engine_error)?;
        Ok(Self { client, config })
    }

    pub(crate) fn model(&self) -> &str {
        &self.config.model
    }

    /// POST an arbitrary `/v1/systemone` body and return the raw JSON.
    ///
    /// Turn perception and Locus memory reflex share this client so both
    /// catalogs hit the same endpoint, timeout, and API key.
    pub(crate) async fn post_systemone(&self, body: &Value) -> Result<Value, SystemOneError> {
        let mut request = self.client.post(self.config.endpoint.clone()).json(body);
        if let Some(api_key) = &self.config.api_key {
            request = request.bearer_auth(api_key);
        }
        let response = request.send().await.map_err(engine_error)?;
        let status = response.status();
        if !status.is_success() {
            let detail = response.text().await.unwrap_or_default();
            let detail = detail.chars().take(240).collect::<String>();
            return Err(SystemOneError::Engine(format!(
                "Laya returned HTTP {status}: {detail}"
            )));
        }
        response.json::<Value>().await.map_err(engine_error)
    }

    fn request<'a>(&'a self, input: &'a SystemOneInput) -> LayaRequest<'a> {
        LayaRequest {
            state: LayaState {
                current_user_message: &input.current_user_message,
                recent_context: &input.recent_context,
            },
            model: &self.config.model,
            questions: BTreeMap::from([(
                QUESTION_ID,
                LayaChoiceQuestion {
                    kind: "choice",
                    instructions: "Which execution posture best fits the current user turn right now?",
                    criteria: BTreeMap::from([
                        (
                            TurnIntent::Conversational.as_str(),
                            "Answer directly without tools; no external state needs inspection or change.",
                        ),
                        (
                            TurnIntent::ToolRequired.as_str(),
                            "Use tools now to inspect, retrieve, compute, or change external state.",
                        ),
                        (
                            TurnIntent::Clarify.as_str(),
                            "Ask one focused question because a required target, goal, or scope is missing.",
                        ),
                        (
                            TurnIntent::Mixed.as_str(),
                            "The turn mixes discussion with possible work and should retain the existing heuristic.",
                        ),
                    ]),
                },
            )]),
        }
    }

    fn decode_response(&self, response: LayaResponse) -> Result<SystemOneDecision, SystemOneError> {
        let answer = response
            .answers
            .get(QUESTION_ID)
            .ok_or_else(|| SystemOneError::Engine("Laya omitted intent answer".to_string()))?;
        if answer.kind != "choice" {
            return Err(SystemOneError::Engine(format!(
                "Laya intent answer had type {}",
                answer.kind
            )));
        }
        let intent = answer.choice.parse::<TurnIntent>()?;
        let probabilities = answer
            .probabilities
            .iter()
            .map(|(label, probability)| {
                label
                    .parse::<TurnIntent>()
                    .map(|intent| (intent, *probability))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        let selected_probability = probabilities.get(&intent).copied().ok_or_else(|| {
            SystemOneError::Engine("Laya probabilities omitted selected intent".to_string())
        })?;
        let max_probability = probabilities
            .values()
            .copied()
            .fold(f32::NEG_INFINITY, f32::max);
        if selected_probability + f32::EPSILON < max_probability {
            return Err(SystemOneError::Engine(
                "Laya choice did not match its maximum probability".to_string(),
            ));
        }
        let reason = response
            .routing
            .and_then(|routing| routing.reason)
            .map(|reason| format!("Laya typed choice; {reason}"))
            .unwrap_or_else(|| "Laya typed choice".to_string());
        SystemOneDecision::new(intent, answer.answer_confidence, reason, self.id())?
            .with_probabilities(probabilities)
    }
}

#[async_trait]
impl SystemOneEngine for LayaSystemOneEngine {
    fn id(&self) -> &str {
        ENGINE_ID
    }

    async fn decide(&self, input: &SystemOneInput) -> Result<SystemOneDecision, SystemOneError> {
        let mut request = self
            .client
            .post(self.config.endpoint.clone())
            .json(&self.request(input));
        if let Some(api_key) = &self.config.api_key {
            request = request.bearer_auth(api_key);
        }
        let response = request.send().await.map_err(engine_error)?;
        let status = response.status();
        if !status.is_success() {
            let detail = response.text().await.unwrap_or_default();
            let detail = detail.chars().take(240).collect::<String>();
            return Err(SystemOneError::Engine(format!(
                "Laya returned HTTP {status}: {detail}"
            )));
        }
        let response = response
            .json::<LayaResponse>()
            .await
            .map_err(engine_error)?;
        self.decode_response(response)
    }
}

fn system_one_endpoint(base_url: &str) -> Result<Url, SystemOneError> {
    let trimmed = base_url.trim().trim_end_matches('/');
    let endpoint = if trimmed.ends_with("/v1/systemone") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/v1/systemone")
    };
    let endpoint = Url::parse(&endpoint).map_err(engine_error)?;
    if !matches!(endpoint.scheme(), "http" | "https") {
        return Err(SystemOneError::Engine(
            "Laya endpoint must use http or https".to_string(),
        ));
    }
    Ok(endpoint)
}

fn engine_error(error: impl std::fmt::Display) -> SystemOneError {
    SystemOneError::Engine(error.to_string())
}

#[derive(Debug, Serialize)]
struct LayaRequest<'a> {
    state: LayaState<'a>,
    model: &'a str,
    questions: BTreeMap<&'static str, LayaChoiceQuestion<'a>>,
}

#[derive(Debug, Serialize)]
struct LayaState<'a> {
    current_user_message: &'a str,
    recent_context: &'a str,
}

#[derive(Debug, Serialize)]
struct LayaChoiceQuestion<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    instructions: &'static str,
    criteria: BTreeMap<&'a str, &'static str>,
}

#[derive(Debug, Deserialize)]
struct LayaResponse {
    answers: BTreeMap<String, LayaChoiceAnswer>,
    #[serde(default)]
    routing: Option<LayaRouting>,
}

#[derive(Debug, Deserialize)]
struct LayaChoiceAnswer {
    #[serde(rename = "type")]
    kind: String,
    choice: String,
    probabilities: BTreeMap<String, f32>,
    #[serde(rename = "confidence")]
    _entropy_confidence: f32,
    answer_confidence: f32,
}

#[derive(Debug, Deserialize)]
struct LayaRouting {
    #[serde(default)]
    reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(base_url: &str) -> LayaSystemOneConfig {
        LayaSystemOneConfig::new(
            base_url,
            None,
            DEFAULT_LAYA_MODEL.to_string(),
            Duration::from_secs(1),
        )
        .unwrap()
    }

    #[test]
    fn normalizes_laya_endpoint() {
        assert_eq!(
            config("http://127.0.0.1:8000").endpoint.as_str(),
            "http://127.0.0.1:8000/v1/systemone"
        );
        assert_eq!(
            config("https://api.laya.studio/v1/systemone/")
                .endpoint
                .as_str(),
            "https://api.laya.studio/v1/systemone"
        );
    }

    #[test]
    fn request_uses_one_typed_choice_with_all_medousa_intents() {
        let engine = LayaSystemOneEngine::new(config("http://127.0.0.1:8000")).unwrap();
        let input = SystemOneInput {
            current_user_message: "look this up".to_string(),
            recent_context: "we were discussing Laya".to_string(),
        };
        let encoded = serde_json::to_value(engine.request(&input)).unwrap();
        let question = &encoded["questions"][QUESTION_ID];
        assert_eq!(question["type"], "choice");
        assert_eq!(question["criteria"].as_object().unwrap().len(), 4);
        assert_eq!(encoded["model"], DEFAULT_LAYA_MODEL);
        assert_eq!(encoded["state"]["current_user_message"], "look this up");
    }

    #[test]
    fn decodes_choice_and_preserves_full_distribution() {
        let engine = LayaSystemOneEngine::new(config("http://127.0.0.1:8000")).unwrap();
        let response: LayaResponse = serde_json::from_value(serde_json::json!({
            "answers": {
                "intent": {
                    "type": "choice",
                    "choice": "tool_required",
                    "probabilities": {
                        "conversational": 0.05,
                        "tool_required": 0.80,
                        "clarify": 0.10,
                        "mixed": 0.05
                    },
                    "confidence": 0.31,
                    "answer_confidence": 0.84
                }
            },
            "routing": {"reason": "English Latin text"}
        }))
        .unwrap();

        let decision = engine.decode_response(response).unwrap();
        assert_eq!(decision.intent, TurnIntent::ToolRequired);
        assert_eq!(decision.confidence, 0.84);
        assert_eq!(decision.probabilities.len(), 4);
        assert_eq!(decision.probabilities[&TurnIntent::ToolRequired], 0.80);
        assert!(decision.reason.contains("English Latin text"));
    }

    #[tokio::test]
    async fn posts_wire_contract_to_systemone_endpoint() {
        use std::sync::{Arc, Mutex};

        use axum::{Json, Router, extract::State, routing::post};
        use serde_json::{Value, json};

        async fn respond(
            State(seen): State<Arc<Mutex<Option<Value>>>>,
            Json(request): Json<Value>,
        ) -> Json<Value> {
            *seen.lock().expect("request lock") = Some(request);
            Json(json!({
                "answers": {
                    "intent": {
                        "type": "choice",
                        "choice": "clarify",
                        "probabilities": {
                            "conversational": 0.10,
                            "tool_required": 0.15,
                            "clarify": 0.65,
                            "mixed": 0.10
                        },
                        "confidence": 0.42,
                        "answer_confidence": 0.78,
                        "action": {"act_probability": 0.99}
                    }
                },
                "usage": {"input_tokens": 12}
            }))
        }

        let seen = Arc::new(Mutex::new(None));
        let app = Router::new()
            .route("/v1/systemone", post(respond))
            .with_state(seen.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let engine = LayaSystemOneEngine::new(config(&format!("http://{address}"))).unwrap();

        let decision = engine
            .decide(&SystemOneInput {
                current_user_message: "do the thing".to_string(),
                recent_context: "target is unclear".to_string(),
            })
            .await
            .unwrap();

        assert_eq!(decision.intent, TurnIntent::Clarify);
        assert_eq!(decision.confidence, 0.78);
        let request = seen.lock().expect("request lock").clone().unwrap();
        assert_eq!(request["questions"][QUESTION_ID]["type"], "choice");
        assert_eq!(request["model"], DEFAULT_LAYA_MODEL);
        server.abort();
    }

    #[test]
    fn rejects_incomplete_or_inconsistent_distribution() {
        let engine = LayaSystemOneEngine::new(config("http://127.0.0.1:8000")).unwrap();
        let response: LayaResponse = serde_json::from_value(serde_json::json!({
            "answers": {
                "intent": {
                    "type": "choice",
                    "choice": "conversational",
                    "probabilities": {
                        "conversational": 0.40,
                        "tool_required": 0.60
                    },
                    "confidence": 0.20,
                    "answer_confidence": 0.40
                }
            }
        }))
        .unwrap();

        assert!(engine.decode_response(response).is_err());
    }
}
