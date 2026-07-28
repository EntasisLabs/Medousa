//! `cognition_browser_act` — click/type automation on the shared human webview via Agent Browser.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use stasis::application::orchestration::tool_registry::StasisTool;
use stasis::domain::errors::StasisError;
use tokio::sync::{mpsc, RwLock};

use crate::browser_host_client::browser_host_act;
use crate::browser_search::{client_executed, surface_from_scope};
use crate::browser_sessions::{
    attach_browser_act_request, create_browser_session, get_browser_session,
    BrowserSessionCreateRequest, BrowserSessionStatus,
};
use crate::browser_tools::{surface_supports_browser_host, COGNITION_BROWSER_ACT};
use crate::events::TuiEvent;
use crate::turn_continuation::TurnContinuationScope;

const ACT_ACTIONS: &[&str] = &["click", "type", "press", "scroll", "select", "wait"];
const HIGH_RISK_ACTIONS: &[&str] = &["click", "select"];
const CLIENT_ACT_WAIT_SECS: u64 = 120;
const CLIENT_ACT_POLL_MS: u64 = 500;

pub struct CognitionBrowserActTool {
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionBrowserActTool {
    pub fn new(
        turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
        event_tx: mpsc::Sender<TuiEvent>,
    ) -> Self {
        Self {
            turn_scope,
            event_tx,
        }
    }

    async fn browser_enabled(&self) -> bool {
        let scope = self.turn_scope.read().await.clone();
        surface_supports_browser_host(surface_from_scope(scope.as_ref()).as_ref())
    }
}

fn target_is_high_risk(action: &str, selector: Option<&str>) -> bool {
    if !HIGH_RISK_ACTIONS.contains(&action) {
        return false;
    }
    let Some(selector) = selector else {
        return false;
    };
    let selector = selector.to_lowercase();
    selector.contains("password")
        || selector.contains("type=submit")
        || selector.contains("submit")
        || selector.contains("checkout")
        || selector.contains("purchase")
        || selector.contains("delete")
}

#[async_trait]
impl StasisTool for CognitionBrowserActTool {
    fn name(&self) -> &'static str {
        COGNITION_BROWSER_ACT
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Act on the shared human Web tab (click, type, press, scroll, select, wait). \
             Requires a browser-capable client (Home desktop/iOS) and agent control of the tab. \
             Use cognition_browser_snapshot first to discover selectors.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ACT_ACTIONS,
                    "description": "Interaction to perform"
                },
                "selector": {
                    "type": "string",
                    "description": "CSS selector of the target element (required for click/type/press/select)"
                },
                "text": {
                    "type": "string",
                    "description": "Text to type (action=type)"
                },
                "key": {
                    "type": "string",
                    "description": "Key name such as Enter/Tab/Escape (action=press)"
                },
                "delta_y": {
                    "type": "integer",
                    "description": "Vertical scroll delta in px (action=scroll; positive = down)"
                },
                "value": {
                    "type": "string",
                    "description": "Option value to choose (action=select)"
                },
                "ms": {
                    "type": "integer",
                    "default": 1000,
                    "description": "Wait duration in milliseconds (action=wait)"
                },
                "allow_high_risk": {
                    "type": "boolean",
                    "default": false,
                    "description": "Set true to act on submit/password/checkout-like targets"
                }
            },
            "required": ["action"]
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        if !self.browser_enabled().await {
            return Err(StasisError::PortFailure(format!(
                "{COGNITION_BROWSER_ACT}: requires supports_browser_host client (Home desktop/iOS)"
            )));
        }

        let action = input
            .get("action")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .ok_or_else(|| {
                StasisError::PortFailure(format!("{COGNITION_BROWSER_ACT}: action is required"))
            })?;
        if !ACT_ACTIONS.contains(&action.as_str()) {
            return Err(StasisError::PortFailure(format!(
                "{COGNITION_BROWSER_ACT}: unsupported action '{action}' (expected one of {ACT_ACTIONS:?})"
            )));
        }

        let selector = input
            .get("selector")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let needs_selector = matches!(action.as_str(), "click" | "type" | "press" | "select");
        if needs_selector && selector.is_none() {
            return Err(StasisError::PortFailure(format!(
                "{COGNITION_BROWSER_ACT}: selector is required for action '{action}'"
            )));
        }

        let allow_high_risk = input
            .get("allow_high_risk")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        if !allow_high_risk && target_is_high_risk(&action, selector.as_deref()) {
            return Err(StasisError::PortFailure(format!(
                "{COGNITION_BROWSER_ACT}: target looks high-risk (submit/password/checkout-like). \
                 Re-run with allow_high_risk=true only if the operator asked for this action."
            )));
        }

        let mut body = json!({ "action": action });
        if let Some(selector) = selector {
            body["selector"] = json!(selector);
        }
        for key in ["text", "key", "value"] {
            if let Some(value) = input.get(key).and_then(|value| value.as_str()) {
                body[key] = json!(value);
            }
        }
        for key in ["delta_y", "ms"] {
            if let Some(value) = input.get(key).and_then(|value| value.as_i64()) {
                body[key] = json!(value);
            }
        }

        let summary = body
            .get("selector")
            .and_then(|value| value.as_str())
            .map(|selector| format!("{action} {selector}"))
            .unwrap_or_else(|| action.clone());
        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: self.name().to_string(),
                input_summary: summary,
            })
            .await;

        let scope = self.turn_scope.read().await.clone();
        if client_executed(scope.as_ref()) {
            return self.invoke_client_executed(body, &scope).await;
        }

        let outcome = browser_host_act(body)
            .await
            .map_err(StasisError::PortFailure)?;
        if outcome
            .get("ok")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
        {
            return Ok(outcome);
        }

        let code = outcome
            .get("code")
            .and_then(|value| value.as_str())
            .unwrap_or("act_failed")
            .to_string();
        let error = outcome
            .get("error")
            .and_then(|value| value.as_str())
            .unwrap_or("browser act failed")
            .to_string();
        Ok(json!({
            "ok": false,
            "code": code,
            "error": error,
            "binding_used": outcome.get("binding_used").cloned().unwrap_or(json!("browser_host")),
            "decision": "block",
        }))
    }
}

impl CognitionBrowserActTool {
    /// iOS/Android: no :7422 host — hand the act to the client via a browser session and
    /// wait for Home to execute it in the overlay webview (mirrors client-executed search).
    async fn invoke_client_executed(
        &self,
        body: Value,
        scope: &Option<TurnContinuationScope>,
    ) -> stasis::prelude::Result<Value> {
        let Some(scope) = scope else {
            return Err(StasisError::PortFailure(format!(
                "{COGNITION_BROWSER_ACT}: missing turn scope for client-executed act"
            )));
        };
        let session = create_browser_session(BrowserSessionCreateRequest {
            turn_id: scope.turn_correlation_id.clone(),
            chat_session_id: scope.session_id.clone(),
            query: String::new(),
            max_results: 0,
            client_executed: true,
        });
        let _ = attach_browser_act_request(&session.session_id, body.clone());

        if let Some(sink) = crate::engine_adapters::active_tool_sink().await {
            sink.emit(medousa_engine::ToolSinkEvent::BrowserChallenge {
                turn_correlation_id: scope.turn_correlation_id.clone(),
                session_id: session.session_id.clone(),
                challenge_url: String::new(),
                reason: "client_act".to_string(),
            })
            .await;
        }

        let deadline = std::time::Duration::from_secs(CLIENT_ACT_WAIT_SECS);
        let started = std::time::Instant::now();
        while started.elapsed() < deadline {
            if let Some(current) = get_browser_session(&session.session_id) {
                match current.status {
                    BrowserSessionStatus::Completed => {
                        let outcome = current.act_result.ok_or_else(|| {
                            StasisError::PortFailure(
                                "browser act session completed without result".to_string(),
                            )
                        })?;
                        if outcome.ok {
                            return Ok(json!({
                                "ok": true,
                                "action": body.get("action").cloned().unwrap_or(Value::Null),
                                "selector": body.get("selector").cloned().unwrap_or(Value::Null),
                                "url": outcome.url,
                                "binding_used": "human_webview",
                                "decision": "allow",
                            }));
                        }
                        return Ok(json!({
                            "ok": false,
                            "code": "act_failed",
                            "error": outcome
                                .error
                                .unwrap_or_else(|| "browser act failed".to_string()),
                            "binding_used": "human_webview",
                            "decision": "block",
                        }));
                    }
                    BrowserSessionStatus::Failed => {
                        return Err(StasisError::PortFailure(
                            current
                                .error
                                .unwrap_or_else(|| "browser act session failed".to_string()),
                        ));
                    }
                    BrowserSessionStatus::ChallengeRequired
                    | BrowserSessionStatus::PendingClient => {}
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(CLIENT_ACT_POLL_MS)).await;
        }
        Err(StasisError::PortFailure(
            "browser act timed out waiting for client".to_string(),
        ))
    }
}

pub fn register_browser_act_tool(
    registry: &mut stasis::application::orchestration::tool_registry::InMemoryToolRegistry,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
    event_tx: mpsc::Sender<TuiEvent>,
) -> stasis::prelude::Result<()> {
    registry.register_tool(CognitionBrowserActTool::new(turn_scope, event_tx))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn high_risk_targets_flagged() {
        assert!(target_is_high_risk("click", Some("button[type=submit]")));
        assert!(target_is_high_risk("click", Some("input.password-field")));
        assert!(target_is_high_risk("select", Some("#checkout-plan")));
        assert!(target_is_high_risk("click", Some(".delete-account")));
        assert!(!target_is_high_risk("click", Some("#search-button")));
        assert!(!target_is_high_risk("type", Some("input.password")));
        assert!(!target_is_high_risk("click", None));
    }
}
