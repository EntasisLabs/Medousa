//! `cognition_browser_act` — click/type automation on the shared human webview via Agent Browser.


use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use stasis::domain::errors::StasisError;
use tokio::sync::mpsc;

use crate::browser_host_client::browser_host_act;
use crate::browser_search::{client_executed, surface_from_scope};
use crate::browser_sessions::{
    BrowserSessionCreateRequest, BrowserSessionStatus, attach_browser_act_request,
    create_browser_session, get_browser_session,
};
use crate::browser_tools::{COGNITION_BROWSER_ACT, surface_supports_browser_host};
use crate::events::TuiEvent;
use crate::semantic_values::TrimmedText;
use crate::turn_continuation::TurnContinuationScope;
use crate::typed_tools::{CompatOption, ExternalJson, ToolId, medousa_tool};

const ACT_ACTIONS: &[&str] = &["click", "type", "press", "scroll", "select", "wait"];
const HIGH_RISK_ACTIONS: &[&str] = &["click", "select"];
const CLIENT_ACT_WAIT_SECS: u64 = 120;
const CLIENT_ACT_POLL_MS: u64 = 500;
const COGNITION_BROWSER_ACT_ID: ToolId = ToolId::new(COGNITION_BROWSER_ACT);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BrowserActAction {
    Click,
    Type,
    Press,
    Scroll,
    Select,
    Wait,
}

impl BrowserActAction {
    fn parse(value: Option<&str>) -> stasis::prelude::Result<Self> {
        let action = value.map(str::trim).filter(|value| !value.is_empty());
        match action {
            Some("click") => Ok(Self::Click),
            Some("type") => Ok(Self::Type),
            Some("press") => Ok(Self::Press),
            Some("scroll") => Ok(Self::Scroll),
            Some("select") => Ok(Self::Select),
            Some("wait") => Ok(Self::Wait),
            Some(action) => Err(StasisError::PortFailure(format!(
                "{COGNITION_BROWSER_ACT}: unsupported action '{action}' (expected one of {ACT_ACTIONS:?})"
            ))),
            None => Err(StasisError::PortFailure(format!(
                "{COGNITION_BROWSER_ACT}: action is required"
            ))),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Click => "click",
            Self::Type => "type",
            Self::Press => "press",
            Self::Scroll => "scroll",
            Self::Select => "select",
            Self::Wait => "wait",
        }
    }

    fn needs_selector(self) -> bool {
        matches!(self, Self::Click | Self::Type | Self::Press | Self::Select)
    }
}

pub struct CognitionBrowserActTool {
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionBrowserActTool {
    pub fn new(
        turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
        event_tx: mpsc::Sender<TuiEvent>,
    ) -> Self {
        Self {
            turn_scope,
            event_tx,
        }
    }

    async fn browser_enabled(&self) -> bool {
        let scope = crate::agent_runtime::execution_context::turn_continuation_scope(
            &self.turn_scope,
        )
        .await;
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

#[allow(dead_code)]
#[derive(Debug, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum BrowserActActionSchema {
    Click,
    Type,
    Press,
    Scroll,
    Select,
    Wait,
}

fn default_browser_act_wait_ms() -> i64 {
    1000
}

fn default_browser_act_allow_high_risk() -> bool {
    false
}

#[derive(Debug, JsonSchema)]
pub struct BrowserActInput {
    /// Interaction to perform
    #[schemars(required, with = "BrowserActActionSchema")]
    action: CompatOption<String>,
    /// CSS selector of the target element (required for click/type/press/select)
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    selector: CompatOption<String>,
    /// Text to type (action=type)
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    text: CompatOption<String>,
    /// Key name such as Enter/Tab/Escape (action=press)
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    key: CompatOption<String>,
    /// Vertical scroll delta in px (action=scroll; positive = down)
    #[serde(default)]
    #[schemars(
        with = "i64",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    delta_y: CompatOption<i64>,
    /// Option value to choose (action=select)
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    value: CompatOption<String>,
    /// Wait duration in milliseconds (action=wait)
    #[schemars(with = "i64", default = "default_browser_act_wait_ms")]
    ms: CompatOption<i64>,
    /// Set true to act on submit/password/checkout-like targets
    #[schemars(with = "bool", default = "default_browser_act_allow_high_risk")]
    allow_high_risk: CompatOption<bool>,
}

impl<'de> Deserialize<'de> for BrowserActInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(default)]
            action: CompatOption<String>,
            #[serde(default)]
            selector: CompatOption<String>,
            #[serde(default)]
            text: CompatOption<String>,
            #[serde(default)]
            key: CompatOption<String>,
            #[serde(default)]
            delta_y: CompatOption<i64>,
            #[serde(default)]
            value: CompatOption<String>,
            #[serde(default)]
            ms: CompatOption<i64>,
            #[serde(default)]
            allow_high_risk: CompatOption<bool>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            action: input.action,
            selector: input.selector,
            text: input.text,
            key: input.key,
            delta_y: input.delta_y,
            value: input.value,
            ms: input.ms,
            allow_high_risk: input.allow_high_risk,
        })
    }
}

#[derive(Debug)]
struct BrowserActCommand {
    action: BrowserActAction,
    selector: Option<TrimmedText>,
    text: Option<String>,
    key: Option<String>,
    delta_y: Option<i64>,
    value: Option<String>,
    ms: Option<i64>,
    allow_high_risk: bool,
}

impl TryFrom<BrowserActInput> for BrowserActCommand {
    type Error = stasis::prelude::StasisError;

    fn try_from(input: BrowserActInput) -> Result<Self, Self::Error> {
        let action = BrowserActAction::parse(input.action.into_option().as_deref())?;
        let selector_value = input.selector.into_option();
        let selector = selector_value
            .as_deref()
            .and_then(|value| TrimmedText::new(value).ok());
        if action.needs_selector() && selector.is_none() {
            return Err(StasisError::PortFailure(format!(
                "{COGNITION_BROWSER_ACT}: selector is required for action '{}'",
                action.as_str()
            )));
        }

        Ok(Self {
            action,
            selector,
            text: input.text.into_option(),
            key: input.key.into_option(),
            delta_y: input.delta_y.into_option(),
            value: input.value.into_option(),
            ms: input.ms.into_option(),
            allow_high_risk: input.allow_high_risk.into_option().unwrap_or(false),
        })
    }
}

#[medousa_tool(id = COGNITION_BROWSER_ACT_ID)]
impl CognitionBrowserActTool {
    /// Act on the shared human Web tab (click, type, press, scroll, select, wait). Requires a browser-capable client (Home desktop/iOS) and agent control of the tab. Use cognition_browser_snapshot first to discover selectors.
    async fn invoke_typed(&self, input: BrowserActInput) -> stasis::prelude::Result<ExternalJson> {
        if !self.browser_enabled().await {
            return Err(StasisError::PortFailure(format!(
                "{COGNITION_BROWSER_ACT}: requires supports_browser_host client (Home desktop/iOS)"
            )));
        }

        let command = BrowserActCommand::try_from(input)?;

        if !command.allow_high_risk
            && target_is_high_risk(
                command.action.as_str(),
                command.selector.as_ref().map(|selector| selector.as_str()),
            )
        {
            return Err(StasisError::PortFailure(format!(
                "{COGNITION_BROWSER_ACT}: target looks high-risk (submit/password/checkout-like). \
                 Re-run with allow_high_risk=true only if the operator asked for this action."
            )));
        }

        let mut body = json!({ "action": command.action.as_str() });
        if let Some(selector) = command.selector {
            body["selector"] = json!(selector.into_string());
        }
        if let Some(value) = command.text {
            body["text"] = json!(value);
        }
        if let Some(value) = command.key {
            body["key"] = json!(value);
        }
        if let Some(value) = command.value {
            body["value"] = json!(value);
        }
        if let Some(value) = command.delta_y {
            body["delta_y"] = json!(value);
        }
        if let Some(value) = command.ms {
            body["ms"] = json!(value);
        }

        let summary = body
            .get("selector")
            .and_then(|value| value.as_str())
            .map(|selector| format!("{} {selector}", command.action.as_str()))
            .unwrap_or_else(|| command.action.as_str().to_string());
        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: COGNITION_BROWSER_ACT.to_string(),
                input_summary: summary,
            })
            .await;

        let scope = crate::agent_runtime::execution_context::turn_continuation_scope(
            &self.turn_scope,
        )
        .await;
        if client_executed(scope.as_ref()) {
            return self
                .invoke_client_executed(body, &scope)
                .await
                .map(ExternalJson::new);
        }

        let outcome = browser_host_act(body)
            .await
            .map_err(StasisError::PortFailure)?;
        if outcome
            .get("ok")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
        {
            return Ok(ExternalJson::new(outcome));
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
        Ok(ExternalJson::new(json!({
            "ok": false,
            "code": code,
            "error": error,
            "binding_used": outcome.get("binding_used").cloned().unwrap_or(json!("browser_host")),
            "decision": "block",
        })))
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
    registry: &mut impl crate::typed_tools::ToolRegistration,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    event_tx: mpsc::Sender<TuiEvent>,
) -> stasis::prelude::Result<()> {
    registry.register_typed_tool(CognitionBrowserActTool::new(turn_scope, event_tx))?;
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

    #[test]
    fn browser_act_command_normalizes_controls_and_preserves_text() {
        let command = BrowserActCommand::try_from(BrowserActInput {
            action: Some("  type  ".to_string()).into(),
            selector: Some("  #search  ".to_string()).into(),
            text: Some("  keep surrounding text  ".to_string()).into(),
            key: Some(" Enter ".to_string()).into(),
            delta_y: None.into(),
            value: None.into(),
            ms: None.into(),
            allow_high_risk: None.into(),
        })
        .expect("command");

        assert_eq!(command.action, BrowserActAction::Type);
        assert_eq!(
            command.selector.as_ref().map(TrimmedText::as_str),
            Some("#search")
        );
        assert_eq!(command.text.as_deref(), Some("  keep surrounding text  "));
        assert_eq!(command.key.as_deref(), Some(" Enter "));
        assert!(!command.allow_high_risk);
    }

    #[test]
    fn browser_act_command_requires_selector_for_targeted_actions() {
        let error = BrowserActCommand::try_from(BrowserActInput {
            action: Some("click".to_string()).into(),
            selector: None.into(),
            text: None.into(),
            key: None.into(),
            delta_y: None.into(),
            value: None.into(),
            ms: None.into(),
            allow_high_risk: None.into(),
        })
        .expect_err("click without selector should fail");

        assert!(
            error
                .to_string()
                .contains("selector is required for action 'click'")
        );
    }

    #[test]
    fn browser_act_wire_optionals_remain_lenient_for_legacy_values() {
        let input: BrowserActInput = serde_json::from_value(serde_json::json!({
            "action": "click",
            "selector": 42,
            "text": false,
            "key": [],
            "delta_y": "10",
            "value": null,
            "ms": "1000",
            "allow_high_risk": "true",
        }))
        .expect("browser act input");
        assert_eq!(input.action.into_option().as_deref(), Some("click"));
        assert!(input.selector.into_option().is_none());
        assert!(input.text.into_option().is_none());
        assert!(input.key.into_option().is_none());
        assert!(input.delta_y.into_option().is_none());
        assert!(input.value.into_option().is_none());
        assert!(input.ms.into_option().is_none());
        assert!(input.allow_high_risk.into_option().is_none());
    }
}
