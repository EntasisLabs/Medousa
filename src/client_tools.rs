//! Registered client tools.
//!
//! A host integration can advertise a small, explicit tool surface to the
//! daemon. The daemon exposes those definitions to the model and routes each
//! invocation back to the owning client through a pull-based request queue.
//! This keeps the daemon authoritative for the turn loop while allowing
//! browser, editor, and vault hosts to remain in their native runtimes.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use genai::chat::Tool;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use stasis::application::orchestration::tool_registry::ToolRegistry;
use stasis::domain::errors::StasisError;
use tokio::sync::{Notify, oneshot};

use crate::turn_continuation::TurnContinuationScope;

const MAX_CLIENT_TOOLS: usize = 32;
const MAX_TOOL_NAME_CHARS: usize = 64;
const MAX_DESCRIPTION_CHARS: usize = 2000;
const CLIENT_TTL: Duration = Duration::from_secs(90);
const TOOL_CALL_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClientToolDefinition {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub input_schema: Option<Value>,
    #[serde(default)]
    pub output_schema: Option<Value>,
    #[serde(default)]
    pub effect_class: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientRegistration {
    pub client_id: String,
    pub channel_surface: String,
    pub supports_browser_host: bool,
    #[serde(default)]
    pub browser_host_url: Option<String>,
    #[serde(default)]
    pub tools: Vec<ClientToolDefinition>,
    pub registered_at_utc: DateTime<Utc>,
    pub last_seen_at_utc: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RegisterClientRequest {
    pub client_id: String,
    pub channel_surface: String,
    pub supports_browser_host: bool,
    #[serde(default)]
    pub browser_host_url: Option<String>,
    #[serde(default)]
    pub tools: Vec<ClientToolDefinition>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RegisterClientResponse {
    pub ok: bool,
    pub browser_host_reachable: bool,
    pub registered_tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClientToolRequest {
    pub request_id: String,
    pub client_id: String,
    pub tool_name: String,
    pub input: Value,
    pub turn_id: String,
    pub created_at_utc: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClientToolResultRequest {
    #[serde(default)]
    pub output: Option<Value>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClientToolResultResponse {
    pub ok: bool,
    pub accepted: bool,
}

#[derive(Debug, Clone)]
pub struct RegisteredClientTool {
    pub client_id: String,
    pub definition: ClientToolDefinition,
}

struct PendingClientToolCall {
    request: ClientToolRequest,
    response_tx: oneshot::Sender<Result<Value, String>>,
}

struct ClientRegistryState {
    clients: HashMap<String, ClientRegistration>,
    pending: HashMap<String, PendingClientToolCall>,
    queues: HashMap<String, VecDeque<String>>,
}

#[derive(Clone)]
pub struct ClientRegistry {
    state: Arc<Mutex<ClientRegistryState>>,
    notify: Arc<Notify>,
}

impl ClientRegistry {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(ClientRegistryState {
                clients: HashMap::new(),
                pending: HashMap::new(),
                queues: HashMap::new(),
            })),
            notify: Arc::new(Notify::new()),
        }
    }

    pub fn register(&self, mut registration: ClientRegistration) -> Result<Vec<String>, String> {
        validate_registration(&registration)?;
        registration.client_id = registration.client_id.trim().to_string();
        registration.channel_surface = registration.channel_surface.trim().to_string();
        registration.last_seen_at_utc = Utc::now();
        let names = registration
            .tools
            .iter()
            .map(|tool| tool.name.clone())
            .collect::<Vec<_>>();

        let mut guard = self.state.lock().expect("client registry");
        let client_id = registration.client_id.clone();
        guard.clients.insert(client_id.clone(), registration);
        guard.queues.entry(client_id).or_default();
        drop(guard);
        self.notify.notify_waiters();
        Ok(names)
    }

    pub fn list(&self) -> Vec<ClientRegistration> {
        let mut guard = self.state.lock().expect("client registry");
        prune_expired(&mut guard);
        let mut clients = guard.clients.values().cloned().collect::<Vec<_>>();
        clients.sort_by(|left, right| left.client_id.cmp(&right.client_id));
        clients
    }

    pub fn browser_host_available(&self) -> bool {
        let mut guard = self.state.lock().expect("client registry");
        prune_expired(&mut guard);
        guard
            .clients
            .values()
            .any(|entry| entry.supports_browser_host)
    }

    pub fn touch(&self, client_id: &str) -> bool {
        let mut guard = self.state.lock().expect("client registry");
        prune_expired(&mut guard);
        let Some(client) = guard.clients.get_mut(client_id.trim()) else {
            return false;
        };
        client.last_seen_at_utc = Utc::now();
        true
    }

    pub fn tools_for_surface(&self, channel_surface: Option<&str>) -> Vec<RegisteredClientTool> {
        let Some(surface) = channel_surface
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Vec::new();
        };

        let mut guard = self.state.lock().expect("client registry");
        prune_expired(&mut guard);
        let mut clients = guard
            .clients
            .values()
            .filter(|client| client.channel_surface == surface)
            .collect::<Vec<_>>();
        clients.sort_by(|left, right| {
            right
                .last_seen_at_utc
                .cmp(&left.last_seen_at_utc)
                .then_with(|| left.client_id.cmp(&right.client_id))
        });

        let mut seen = HashSet::new();
        let mut tools = Vec::new();
        for client in clients {
            for definition in &client.tools {
                if seen.insert(definition.name.clone()) {
                    tools.push(RegisteredClientTool {
                        client_id: client.client_id.clone(),
                        definition: definition.clone(),
                    });
                }
            }
        }
        tools
    }

    pub fn tool_names_for_surface(&self, channel_surface: Option<&str>) -> HashSet<String> {
        self.tools_for_surface(channel_surface)
            .into_iter()
            .map(|tool| tool.definition.name)
            .collect()
    }

    pub async fn enqueue_tool_call(
        &self,
        channel_surface: Option<&str>,
        tool_name: &str,
        input: Value,
        turn_id: String,
    ) -> Result<(ClientToolRequest, oneshot::Receiver<Result<Value, String>>), String> {
        let registered = self
            .tools_for_surface(channel_surface)
            .into_iter()
            .find(|tool| tool.definition.name == tool_name)
            .ok_or_else(|| format!("client tool not registered for surface: {tool_name}"))?;
        let request_id = format!("client-tool-{}", uuid::Uuid::new_v4().simple());
        let request = ClientToolRequest {
            request_id: request_id.clone(),
            client_id: registered.client_id.clone(),
            tool_name: tool_name.to_string(),
            input,
            turn_id,
            created_at_utc: Utc::now(),
        };
        let (response_tx, response_rx) = oneshot::channel();
        let mut guard = self.state.lock().expect("client registry");
        guard.pending.insert(
            request_id.clone(),
            PendingClientToolCall {
                request: request.clone(),
                response_tx,
            },
        );
        guard
            .queues
            .entry(registered.client_id)
            .or_default()
            .push_back(request_id);
        drop(guard);
        self.notify.notify_waiters();
        Ok((request, response_rx))
    }

    pub async fn next_tool_request(
        &self,
        client_id: &str,
        wait: Duration,
    ) -> Result<Option<ClientToolRequest>, String> {
        if !self.touch(client_id) {
            return Err("client is not registered or has expired".to_string());
        }
        let deadline = Instant::now() + wait;
        loop {
            let notified = self.notify.notified();
            if let Some(request) = self.take_next(client_id) {
                return Ok(Some(request));
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(None);
            }
            tokio::select! {
                _ = notified => {}
                _ = tokio::time::sleep(remaining) => return Ok(None),
            }
        }
    }

    pub fn complete_tool_request(
        &self,
        client_id: &str,
        request_id: &str,
        result: Result<Value, String>,
    ) -> bool {
        let pending = {
            let mut guard = self.state.lock().expect("client registry");
            let Some(pending) = guard.pending.get(request_id) else {
                return false;
            };
            if pending.request.client_id != client_id {
                return false;
            }
            guard.pending.remove(request_id)
        };
        let Some(pending) = pending else {
            return false;
        };
        pending.response_tx.send(result).is_ok()
    }

    pub fn cancel_tool_request(&self, request_id: &str) {
        let mut guard = self.state.lock().expect("client registry");
        guard.pending.remove(request_id);
        for queue in guard.queues.values_mut() {
            queue.retain(|id| id != request_id);
        }
    }

    fn take_next(&self, client_id: &str) -> Option<ClientToolRequest> {
        let mut guard = self.state.lock().expect("client registry");
        loop {
            let request_id = guard
                .queues
                .get_mut(client_id.trim())
                .and_then(VecDeque::pop_front)?;
            if let Some(pending) = guard.pending.get(&request_id) {
                return Some(pending.request.clone());
            }
        }
    }
}

impl Default for ClientRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Tool registry that merges daemon-local tools with the currently registered
/// client tools for the active turn surface.
#[derive(Clone)]
pub struct ClientToolRegistry {
    inner: Arc<dyn ToolRegistry>,
    clients: ClientRegistry,
    turn_scope: Arc<tokio::sync::RwLock<Option<TurnContinuationScope>>>,
}

impl ClientToolRegistry {
    pub fn new(
        inner: Arc<dyn ToolRegistry>,
        clients: ClientRegistry,
        turn_scope: Arc<tokio::sync::RwLock<Option<TurnContinuationScope>>>,
    ) -> Self {
        Self {
            inner,
            clients,
            turn_scope,
        }
    }

    async fn turn_surface(&self) -> (Option<String>, String) {
        let scope = self.turn_scope.read().await.clone();
        let surface = scope
            .as_ref()
            .and_then(|scope| scope.channel_surface.clone());
        let turn_id = scope
            .map(|scope| scope.turn_correlation_id)
            .unwrap_or_else(|| "client-tool-turn".to_string());
        (surface, turn_id)
    }
}

#[async_trait]
impl ToolRegistry for ClientToolRegistry {
    async fn list_tools(&self) -> stasis::prelude::Result<Vec<Tool>> {
        let mut tools = self.inner.list_tools().await?;
        let existing = tools
            .iter()
            .map(|tool| tool.name.as_ref().to_string())
            .collect::<HashSet<_>>();
        let (surface, _) = self.turn_surface().await;
        for registered in self.clients.tools_for_surface(surface.as_deref()) {
            if existing.contains(&registered.definition.name) {
                tracing::warn!(
                    tool = %registered.definition.name,
                    client_id = %registered.client_id,
                    "skipping client tool that collides with a daemon tool"
                );
                continue;
            }
            let mut tool = Tool::new(registered.definition.name);
            if let Some(description) = registered.definition.description {
                tool = tool.with_description(description);
            }
            if let Some(schema) = registered.definition.input_schema {
                tool = tool.with_schema(schema);
            }
            tools.push(tool);
        }
        Ok(tools)
    }

    async fn invoke_tool(&self, tool_name: &str, input: Value) -> stasis::prelude::Result<Value> {
        let local_tools = self.inner.list_tools().await?;
        if local_tools
            .iter()
            .any(|tool| tool.name.as_ref() == tool_name)
        {
            return self.inner.invoke_tool(tool_name, input).await;
        }

        let (surface, turn_id) = self.turn_surface().await;
        if self
            .clients
            .tools_for_surface(surface.as_deref())
            .iter()
            .all(|tool| tool.definition.name != tool_name)
        {
            return self.inner.invoke_tool(tool_name, input).await;
        }

        let (request, response_rx) = self
            .clients
            .enqueue_tool_call(surface.as_deref(), tool_name, input, turn_id)
            .await
            .map_err(StasisError::PortFailure)?;
        match tokio::time::timeout(TOOL_CALL_TIMEOUT, response_rx).await {
            Ok(Ok(Ok(output))) => Ok(output),
            Ok(Ok(Err(error))) => Err(StasisError::PortFailure(error)),
            Ok(Err(_)) => Err(StasisError::PortFailure(
                "client disconnected before completing the tool request".to_string(),
            )),
            Err(_) => {
                self.clients.cancel_tool_request(&request.request_id);
                Err(StasisError::PortFailure(
                    "client tool timed out waiting for a response".to_string(),
                ))
            }
        }
    }
}

fn validate_registration(registration: &ClientRegistration) -> Result<(), String> {
    if registration.client_id.trim().is_empty() {
        return Err("client_id is required".to_string());
    }
    if registration.channel_surface.trim().is_empty() {
        return Err("channel_surface is required".to_string());
    }
    if registration.tools.len() > MAX_CLIENT_TOOLS {
        return Err(format!(
            "a client may register at most {MAX_CLIENT_TOOLS} tools"
        ));
    }
    let mut names = HashSet::new();
    for tool in &registration.tools {
        let name = tool.name.trim();
        let mut characters = name.chars();
        let valid_first = characters
            .next()
            .is_some_and(|character| character.is_ascii_alphabetic());
        let valid_rest = characters.all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == '-'
        });
        if name.is_empty()
            || name.chars().count() > MAX_TOOL_NAME_CHARS
            || !valid_first
            || !valid_rest
        {
            return Err(format!(
                "invalid client tool name '{name}'; use an ASCII letter followed by letters, numbers, '_' or '-'"
            ));
        }
        if !names.insert(name.to_string()) {
            return Err(format!("duplicate client tool name '{name}'"));
        }
        if tool
            .description
            .as_deref()
            .is_some_and(|description| description.chars().count() > MAX_DESCRIPTION_CHARS)
        {
            return Err(format!(
                "client tool descriptions may not exceed {MAX_DESCRIPTION_CHARS} characters"
            ));
        }
        match tool.effect_class.as_deref() {
            Some("external_read") => {}
            Some(effect) => {
                return Err(format!(
                    "client tool '{name}' uses unsupported effect_class '{effect}'; only external_read is enabled"
                ));
            }
            None => {
                return Err(format!(
                    "client tool '{name}' must declare effect_class='external_read'"
                ));
            }
        }
        if let Some(schema) = tool.input_schema.as_ref()
            && !schema.is_object()
        {
            return Err(format!(
                "client tool '{name}' input_schema must be a JSON object"
            ));
        }
    }
    Ok(())
}

fn prune_expired(state: &mut ClientRegistryState) {
    let cutoff = Utc::now() - chrono::Duration::from_std(CLIENT_TTL).unwrap_or_default();
    let expired = state
        .clients
        .iter()
        .filter(|(_, client)| client.last_seen_at_utc < cutoff)
        .map(|(client_id, _)| client_id.clone())
        .collect::<Vec<_>>();
    for client_id in expired {
        state.clients.remove(&client_id);
        state.queues.remove(&client_id);
        let pending = state
            .pending
            .iter()
            .filter(|(_, request)| request.request.client_id == client_id)
            .map(|(request_id, _)| request_id.clone())
            .collect::<Vec<_>>();
        for request_id in pending {
            if let Some(request) = state.pending.remove(&request_id) {
                let _ = request
                    .response_tx
                    .send(Err("client registration expired".to_string()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn registration(registry: &ClientRegistry) {
        registry
            .register(ClientRegistration {
                client_id: "browser-one".to_string(),
                channel_surface: "browser".to_string(),
                supports_browser_host: false,
                browser_host_url: None,
                tools: vec![ClientToolDefinition {
                    name: "browser_page_snapshot".to_string(),
                    description: Some("read the active tab".to_string()),
                    input_schema: Some(json!({"type": "object"})),
                    output_schema: None,
                    effect_class: Some("external_read".to_string()),
                }],
                registered_at_utc: Utc::now(),
                last_seen_at_utc: Utc::now(),
            })
            .unwrap();
    }

    #[tokio::test]
    async fn routes_registered_tool_to_client_and_returns_result() {
        let registry = ClientRegistry::new();
        registration(&registry);
        let registry_for_client = registry.clone();
        let client = tokio::spawn(async move {
            let request = registry_for_client
                .next_tool_request("browser-one", Duration::from_secs(1))
                .await
                .unwrap()
                .unwrap();
            assert_eq!(request.tool_name, "browser_page_snapshot");
            assert_eq!(request.input["include_text"], true);
            assert!(registry_for_client.complete_tool_request(
                "browser-one",
                &request.request_id,
                Ok(json!({"title": "Example"})),
            ));
        });
        let (_, response) = registry
            .enqueue_tool_call(
                Some("browser"),
                "browser_page_snapshot",
                json!({"include_text": true}),
                "turn-one".to_string(),
            )
            .await
            .unwrap();
        assert_eq!(response.await.unwrap().unwrap()["title"], "Example");
        client.await.unwrap();
    }

    #[tokio::test]
    async fn dynamic_registry_lists_tools_for_active_surface() {
        let clients = ClientRegistry::new();
        registration(&clients);
        let scope = Arc::new(tokio::sync::RwLock::new(Some(TurnContinuationScope {
            turn_correlation_id: "turn-one".to_string(),
            session_id: "session-one".to_string(),
            original_prompt: "read the page".to_string(),
            delivery_target: None,
            provider: "test".to_string(),
            model: "test".to_string(),
            response_depth_mode: "balanced".to_string(),
            supports_ui_artifacts: false,
            supports_liquid_markdown: false,
            supports_browser_host: false,
            channel_surface: Some("browser".to_string()),
        })));
        let inner: Arc<dyn ToolRegistry> = Arc::new(
            stasis::application::orchestration::tool_registry::InMemoryToolRegistry::default(),
        );
        let registry = ClientToolRegistry::new(inner, clients, scope);
        let tools = registry.list_tools().await.unwrap();
        assert!(
            tools
                .iter()
                .any(|tool| tool.name.as_ref() == "browser_page_snapshot")
        );
    }

    #[test]
    fn rejects_unsafe_names() {
        let registry = ClientRegistry::new();
        let error = registry
            .register(ClientRegistration {
                client_id: "browser-one".to_string(),
                channel_surface: "browser".to_string(),
                supports_browser_host: false,
                browser_host_url: None,
                tools: vec![ClientToolDefinition {
                    name: "cognition.browser.snapshot".to_string(),
                    description: None,
                    input_schema: None,
                    output_schema: None,
                    effect_class: None,
                }],
                registered_at_utc: Utc::now(),
                last_seen_at_utc: Utc::now(),
            })
            .unwrap_err();
        assert!(error.contains("invalid client tool name"));
    }

    #[test]
    fn rejects_write_effects_until_approval_is_available() {
        let registry = ClientRegistry::new();
        let error = registry
            .register(ClientRegistration {
                client_id: "browser-one".to_string(),
                channel_surface: "browser".to_string(),
                supports_browser_host: false,
                browser_host_url: None,
                tools: vec![ClientToolDefinition {
                    name: "browser_click".to_string(),
                    description: None,
                    input_schema: None,
                    output_schema: None,
                    effect_class: Some("external_side_effect".to_string()),
                }],
                registered_at_utc: Utc::now(),
                last_seen_at_utc: Utc::now(),
            })
            .unwrap_err();
        assert!(error.contains("only external_read is enabled"));
    }

    #[test]
    fn exposes_tools_only_on_the_registered_surface() {
        let registry = ClientRegistry::new();
        registration(&registry);
        registry
            .register(ClientRegistration {
                client_id: "obsidian-one".to_string(),
                channel_surface: "obsidian".to_string(),
                supports_browser_host: false,
                browser_host_url: None,
                tools: vec![ClientToolDefinition {
                    name: "obsidian_active_note".to_string(),
                    description: None,
                    input_schema: None,
                    output_schema: None,
                    effect_class: Some("external_read".to_string()),
                }],
                registered_at_utc: Utc::now(),
                last_seen_at_utc: Utc::now(),
            })
            .unwrap();

        assert_eq!(
            registry.tool_names_for_surface(Some("browser")),
            HashSet::from(["browser_page_snapshot".to_string()])
        );
        assert_eq!(
            registry.tool_names_for_surface(Some("obsidian")),
            HashSet::from(["obsidian_active_note".to_string()])
        );
        assert!(registry.tool_names_for_surface(Some("vscode")).is_empty());
    }
}
