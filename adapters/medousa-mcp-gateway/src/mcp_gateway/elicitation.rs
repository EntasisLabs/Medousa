//! MCP elicitation: the gateway accepts server-driven confirmations and forms
//! instead of declining them, and waits for Home, the CLI, or another host
//! surface to answer.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use medousa_types::mcp_gateway_api::{McpElicitationPrompt, McpElicitationReply};
use rmcp::model::{
    ElicitRequestParams, ElicitResult, ElicitationAction, ElicitationCapability,
    FormElicitationCapability, UrlElicitationCapability,
};
use serde_json::{Value, json};
use tokio::sync::{Mutex, oneshot};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Debug, Clone, PartialEq)]
struct Decision {
    action: ElicitationAction,
    content: Option<Value>,
}

struct Pending {
    prompt: McpElicitationPrompt,
    reply: oneshot::Sender<Decision>,
}

pub struct ElicitationBroker {
    timeout: Duration,
    pending: Mutex<HashMap<String, Pending>>,
}

impl ElicitationBroker {
    pub fn new() -> Self {
        Self::with_timeout(DEFAULT_TIMEOUT)
    }

    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            timeout,
            pending: Mutex::new(HashMap::new()),
        }
    }

    pub async fn list(&self) -> Vec<McpElicitationPrompt> {
        let pending = self.pending.lock().await;
        let mut prompts = pending
            .values()
            .map(|pending| pending.prompt.clone())
            .collect::<Vec<_>>();
        prompts.sort_by(|left, right| left.id.cmp(&right.id));
        prompts
    }

    pub async fn resolve(
        &self,
        server_id: Option<String>,
        request: ElicitRequestParams,
    ) -> ElicitResult {
        let (prompt, schema_for_accept) = prompt_from_request(server_id, request);
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().await;
            pending.insert(
                prompt.id.clone(),
                Pending {
                    prompt: prompt.clone(),
                    reply: tx,
                },
            );
        }
        let decision = match tokio::time::timeout(self.timeout, rx).await {
            Ok(Ok(decision)) => decision,
            _ => {
                self.pending.lock().await.remove(&prompt.id);
                Decision {
                    action: ElicitationAction::Cancel,
                    content: None,
                }
            }
        };
        match decision.action {
            ElicitationAction::Accept => {
                let mut result = ElicitResult::new(ElicitationAction::Accept);
                if let Some(content) = decision.content.or(schema_for_accept) {
                    result = result.with_content(content);
                }
                result
            }
            other => ElicitResult::new(other),
        }
    }

    pub async fn respond(&self, id: &str, reply: McpElicitationReply) -> Result<(), String> {
        let pending = self
            .pending
            .lock()
            .await
            .remove(id.trim())
            .ok_or_else(|| "that confirmation is no longer waiting".to_string())?;
        let action = match reply.action.trim().to_ascii_lowercase().as_str() {
            "accept" => ElicitationAction::Accept,
            "decline" => ElicitationAction::Decline,
            "cancel" => ElicitationAction::Cancel,
            other => {
                return Err(format!(
                    "elicitation action must be accept, decline, or cancel (got {other})"
                ));
            }
        };
        let _ = pending.reply.send(Decision {
            action,
            content: reply.content,
        });
        Ok(())
    }
}

impl Default for ElicitationBroker {
    fn default() -> Self {
        Self::new()
    }
}

pub fn shared_broker() -> Arc<ElicitationBroker> {
    static BROKER: OnceLock<Arc<ElicitationBroker>> = OnceLock::new();
    BROKER
        .get_or_init(|| Arc::new(ElicitationBroker::new()))
        .clone()
}

pub fn capability() -> ElicitationCapability {
    ElicitationCapability::new()
        .with_form(FormElicitationCapability::new())
        .with_url(UrlElicitationCapability::new())
}

pub fn capability_value() -> Value {
    json!({
        "elicitation": {
            "form": {},
            "url": {}
        }
    })
}

/// Notice text for a chat channel. The form itself stays in Medousa; the
/// channel only learns that a confirmation is waiting.
pub fn channel_notice(prompt: &McpElicitationPrompt) -> String {
    let where_ = prompt
        .server_id
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(|server| format!(" for {server}"))
        .unwrap_or_default();
    format!(
        "Medousa needs a confirmation{where_}.\n{}\nOpen Medousa to answer, or run: medousa mcp elicitation respond {} --action accept",
        prompt.message.trim(),
        prompt.id
    )
}

fn prompt_from_request(
    server_id: Option<String>,
    request: ElicitRequestParams,
) -> (McpElicitationPrompt, Option<Value>) {
    let id = format!("elicit_{}", uuid::Uuid::new_v4().simple());
    match request {
        ElicitRequestParams::FormElicitationParams {
            message,
            requested_schema,
            ..
        } => {
            let schema = serde_json::to_value(&requested_schema).ok();
            (
                McpElicitationPrompt {
                    id,
                    server_id,
                    mode: "form".to_string(),
                    message,
                    requested_schema: schema,
                    url: None,
                },
                None,
            )
        }
        ElicitRequestParams::UrlElicitationParams { message, url, .. } => (
            McpElicitationPrompt {
                id,
                server_id,
                mode: "url".to_string(),
                message,
                requested_schema: None,
                url: Some(url),
            },
            None,
        ),
        _ => (
            McpElicitationPrompt {
                id,
                server_id,
                mode: "form".to_string(),
                message: "The server asked for a confirmation Medousa does not recognize yet."
                    .to_string(),
                requested_schema: None,
                url: None,
            },
            None,
        ),
    }
}

/// Answer a server-initiated `elicitation/create` JSON-RPC request, if this
/// message is one. Other messages return `None`.
pub async fn response_for_server_request(
    broker: &ElicitationBroker,
    message: &Value,
    server_id: Option<&str>,
) -> Option<Value> {
    if message.get("method").and_then(Value::as_str) != Some("elicitation/create") {
        return None;
    }
    let id = message.get("id").cloned()?;
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let request = match serde_json::from_value::<ElicitRequestParams>(params) {
        Ok(request) => request,
        Err(error) => {
            return Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32602, "message": error.to_string() }
            }));
        }
    };
    let result = broker.resolve(server_id.map(str::to_string), request).await;
    let result = serde_json::to_value(result).unwrap_or(json!({ "action": "cancel" }));
    Some(json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::{ElicitationSchema, PrimitiveSchemaDefinition, StringSchema};

    fn confirm_request() -> ElicitRequestParams {
        ElicitRequestParams::FormElicitationParams {
            meta: None,
            message: "Send this email?".into(),
            requested_schema: ElicitationSchema::builder()
                .required_property(
                    "confirm",
                    PrimitiveSchemaDefinition::String(StringSchema::new()),
                )
                .build()
                .expect("schema"),
        }
    }

    #[tokio::test]
    async fn elicitation_round_trip_with_mock_server() {
        let broker = Arc::new(ElicitationBroker::with_timeout(Duration::from_secs(5)));
        let (server_tx, mut client_rx) = tokio::sync::mpsc::unbounded_channel::<Value>();
        let (client_tx, mut server_rx) = tokio::sync::mpsc::unbounded_channel::<Value>();

        let server = tokio::spawn(async move {
            let call = server_rx.recv().await.expect("tools/call");
            assert_eq!(call["method"], "tools/call");
            let call_id = call["id"].clone();
            server_tx
                .send(json!({
                    "jsonrpc": "2.0",
                    "id": "elicit-1",
                    "method": "elicitation/create",
                    "params": {
                        "mode": "form",
                        "message": "Send this email?",
                        "requestedSchema": {
                            "type": "object",
                            "properties": { "confirm": { "type": "boolean" } },
                            "required": ["confirm"]
                        }
                    }
                }))
                .expect("send elicitation");
            let response = server_rx.recv().await.expect("elicitation response");
            assert_eq!(response["id"], "elicit-1");
            assert_eq!(response["result"]["action"], "accept");
            assert_eq!(response["result"]["content"]["confirm"], true);
            server_tx
                .send(json!({
                    "jsonrpc": "2.0",
                    "id": call_id,
                    "result": { "content": [{ "type": "text", "text": "sent" }] }
                }))
                .expect("tool result");
        });

        let user = {
            let broker = broker.clone();
            tokio::spawn(async move {
                let pending = loop {
                    let pending = broker.list().await;
                    if let Some(prompt) = pending.into_iter().next() {
                        break prompt;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                };
                assert_eq!(pending.mode, "form");
                assert!(pending.message.contains("Send this email"));
                broker
                    .respond(
                        &pending.id,
                        McpElicitationReply {
                            action: "accept".into(),
                            content: Some(json!({ "confirm": true })),
                        },
                    )
                    .await
                    .expect("respond");
            })
        };

        client_tx
            .send(json!({
                "jsonrpc": "2.0",
                "id": 7,
                "method": "tools/call",
                "params": { "name": "send_message" }
            }))
            .expect("call");

        let elicitation = client_rx.recv().await.expect("server request");
        let response = response_for_server_request(&broker, &elicitation, Some("gmail"))
            .await
            .expect("elicitation response");
        client_tx.send(response).expect("reply to server");
        let tool_result = client_rx.recv().await.expect("tool result");
        assert_eq!(tool_result["result"]["content"][0]["text"], "sent");

        user.await.expect("user");
        server.await.expect("server");
        assert!(broker.list().await.is_empty());
    }

    #[tokio::test]
    async fn unanswered_elicitation_cancels_instead_of_declining() {
        let broker = ElicitationBroker::with_timeout(Duration::from_millis(30));
        let result = broker
            .resolve(Some("gmail".into()), confirm_request())
            .await;
        assert_eq!(result.action, ElicitationAction::Cancel);
    }

    #[test]
    fn advertised_capability_includes_form_and_url() {
        let value = capability_value();
        assert!(value["elicitation"]["form"].is_object());
        assert!(value["elicitation"]["url"].is_object());
        let capability = capability();
        assert!(capability.form.is_some());
        assert!(capability.url.is_some());
    }
}
