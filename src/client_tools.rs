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
use medousa_world::{
    WorldDriverId, WorldDriverKind, WorldDriverRegistration, WorldDriverTransport,
    WorldSurfaceKind,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use stasis::application::orchestration::tool_registry::ToolRegistry;
use stasis::domain::errors::StasisError;
use tokio::sync::{Notify, oneshot};

const MAX_CLIENT_TOOLS: usize = 32;
const MAX_WORLD_DRIVERS: usize = 8;
const MAX_DRIVER_ID_CHARS: usize = 160;
const MAX_DRIVER_DISPLAY_NAME_CHARS: usize = 120;
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
    /// Concrete world adapters offered by this exact client process. Driver
    /// mechanics are inventory only; they never become authority grants.
    #[serde(default)]
    pub world_drivers: Vec<WorldDriverRegistration>,
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
    #[serde(default)]
    pub world_drivers: Vec<WorldDriverRegistration>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RegisterClientResponse {
    pub ok: bool,
    pub browser_host_reachable: bool,
    pub registered_tools: Vec<String>,
    pub registered_world_drivers: Vec<WorldDriverId>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClientToolRequest {
    pub request_id: String,
    pub client_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world_driver_id: Option<WorldDriverId>,
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

#[derive(Debug, Clone)]
pub struct RegisteredWorldDriver {
    pub client_id: String,
    pub channel_surface: String,
    pub registration: WorldDriverRegistration,
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
        prune_expired(&mut guard);
        let client_id = registration.client_id.clone();
        for driver in &registration.world_drivers {
            let collision = guard.clients.values().any(|client| {
                client.client_id != client_id
                    && client
                        .world_drivers
                        .iter()
                        .any(|current| current.driver_id == driver.driver_id)
            });
            if collision {
                return Err(format!(
                    "world driver id is already registered by another client: {}",
                    driver.driver_id
                ));
            }
        }
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

    pub fn world_driver(&self, driver_id: &WorldDriverId) -> Option<RegisteredWorldDriver> {
        let mut guard = self.state.lock().expect("client registry");
        prune_expired(&mut guard);
        guard.clients.values().find_map(|client| {
            client
                .world_drivers
                .iter()
                .find(|driver| driver.driver_id == *driver_id)
                .cloned()
                .map(|registration| RegisteredWorldDriver {
                    client_id: client.client_id.clone(),
                    channel_surface: client.channel_surface.clone(),
                    registration,
                })
        })
    }

    pub fn world_drivers_for_surface(
        &self,
        channel_surface: Option<&str>,
    ) -> Vec<RegisteredWorldDriver> {
        let Some(surface) = channel_surface
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Vec::new();
        };
        let mut guard = self.state.lock().expect("client registry");
        prune_expired(&mut guard);
        let mut drivers = guard
            .clients
            .values()
            .filter(|client| client.channel_surface == surface)
            .flat_map(|client| {
                client
                    .world_drivers
                    .iter()
                    .cloned()
                    .map(|registration| RegisteredWorldDriver {
                        client_id: client.client_id.clone(),
                        channel_surface: client.channel_surface.clone(),
                        registration,
                    })
            })
            .collect::<Vec<_>>();
        drivers.sort_by(|left, right| {
            left.registration
                .driver_id
                .cmp(&right.registration.driver_id)
        });
        drivers
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

    pub fn tools_for_driver(&self, driver_id: &WorldDriverId) -> Vec<RegisteredClientTool> {
        let Some(driver) = self.world_driver(driver_id) else {
            return Vec::new();
        };
        let mut guard = self.state.lock().expect("client registry");
        prune_expired(&mut guard);
        guard
            .clients
            .get(&driver.client_id)
            .map(|client| {
                client
                    .tools
                    .iter()
                    .cloned()
                    .map(|definition| RegisteredClientTool {
                        client_id: client.client_id.clone(),
                        definition,
                    })
                    .collect()
            })
            .unwrap_or_default()
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
        self.enqueue_registered_tool_call(registered, None, tool_name, input, turn_id)
    }

    pub async fn enqueue_tool_call_for_driver(
        &self,
        driver_id: &WorldDriverId,
        tool_name: &str,
        input: Value,
        turn_id: String,
    ) -> Result<(ClientToolRequest, oneshot::Receiver<Result<Value, String>>), String> {
        let driver = self
            .world_driver(driver_id)
            .ok_or_else(|| format!("world driver is not registered: {driver_id}"))?;
        if driver.registration.transport != WorldDriverTransport::ClientQueue {
            return Err(format!(
                "world driver {driver_id} does not accept client queue requests"
            ));
        }
        let registered = self
            .tools_for_driver(driver_id)
            .into_iter()
            .find(|tool| tool.definition.name == tool_name)
            .ok_or_else(|| {
                format!("client tool not registered for world driver {driver_id}: {tool_name}")
            })?;
        self.enqueue_registered_tool_call(
            registered,
            Some(driver_id.clone()),
            tool_name,
            input,
            turn_id,
        )
    }

    fn enqueue_registered_tool_call(
        &self,
        registered: RegisteredClientTool,
        world_driver_id: Option<WorldDriverId>,
        tool_name: &str,
        input: Value,
        turn_id: String,
    ) -> Result<(ClientToolRequest, oneshot::Receiver<Result<Value, String>>), String> {
        let request_id = format!("client-tool-{}", uuid::Uuid::new_v4().simple());
        let request = ClientToolRequest {
            request_id: request_id.clone(),
            client_id: registered.client_id.clone(),
            world_driver_id,
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
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
}

impl ClientToolRegistry {
    pub fn new(
        inner: Arc<dyn ToolRegistry>,
        clients: ClientRegistry,
        turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    ) -> Self {
        Self {
            inner,
            clients,
            turn_scope,
        }
    }

    async fn turn_surface(&self) -> (Option<String>, Option<WorldDriverId>, String) {
        let scope =
            crate::agent_runtime::execution_context::turn_continuation_scope(&self.turn_scope)
                .await;
        let surface = scope
            .as_ref()
            .and_then(|scope| scope.channel_surface.clone());
        let world_driver_id = scope
            .as_ref()
            .and_then(|scope| scope.browser_driver_id.clone())
            .map(WorldDriverId::new);
        let turn_id = scope
            .map(|scope| scope.turn_correlation_id)
            .unwrap_or_else(|| "client-tool-turn".to_string());
        (surface, world_driver_id, turn_id)
    }

    fn tools_for_turn(
        &self,
        surface: Option<&str>,
        world_driver_id: Option<&WorldDriverId>,
    ) -> Vec<RegisteredClientTool> {
        world_driver_id
            .map(|driver_id| self.clients.tools_for_driver(driver_id))
            .unwrap_or_else(|| self.clients.tools_for_surface(surface))
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
        let (surface, world_driver_id, _) = self.turn_surface().await;
        for registered in self.tools_for_turn(surface.as_deref(), world_driver_id.as_ref()) {
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

        let (surface, world_driver_id, turn_id) = self.turn_surface().await;
        if self
            .tools_for_turn(surface.as_deref(), world_driver_id.as_ref())
            .iter()
            .all(|tool| tool.definition.name != tool_name)
        {
            return self.inner.invoke_tool(tool_name, input).await;
        }

        let (request, response_rx) = if let Some(driver_id) = world_driver_id.as_ref() {
            self.clients
                .enqueue_tool_call_for_driver(driver_id, tool_name, input, turn_id)
                .await
        } else {
            self.clients
                .enqueue_tool_call(surface.as_deref(), tool_name, input, turn_id)
                .await
        }
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
    if registration.world_drivers.len() > MAX_WORLD_DRIVERS {
        return Err(format!(
            "a client may register at most {MAX_WORLD_DRIVERS} world drivers"
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
    let mut driver_ids = HashSet::new();
    for driver in &registration.world_drivers {
        let driver_id = driver.driver_id.as_str();
        let valid_id = !driver_id.is_empty()
            && driver_id.chars().count() <= MAX_DRIVER_ID_CHARS
            && driver_id.chars().all(|character| {
                character.is_ascii_alphanumeric()
                    || matches!(character, ':' | '_' | '-' | '.')
            });
        if !valid_id {
            return Err(format!(
                "invalid world driver id '{driver_id}'; use at most {MAX_DRIVER_ID_CHARS} ASCII letters, numbers, ':', '_', '-' or '.'"
            ));
        }
        if !driver_ids.insert(driver.driver_id.clone()) {
            return Err(format!("duplicate world driver id '{driver_id}'"));
        }
        if driver.capabilities.is_empty() {
            return Err(format!(
                "world driver '{driver_id}' must advertise at least one mechanical capability"
            ));
        }
        if driver
            .display_name
            .as_deref()
            .is_some_and(|name| name.chars().count() > MAX_DRIVER_DISPLAY_NAME_CHARS)
        {
            return Err(format!(
                "world driver '{driver_id}' display name may not exceed {MAX_DRIVER_DISPLAY_NAME_CHARS} characters"
            ));
        }
        if driver.surface != WorldSurfaceKind::Browser {
            return Err(format!(
                "client world driver '{driver_id}' must currently use the browser surface"
            ));
        }
        match (driver.kind, driver.transport) {
            (WorldDriverKind::EmbeddedBrowser, WorldDriverTransport::LoopbackHttp)
                if registration.supports_browser_host
                    && registration.browser_host_url.is_some() => {}
            (
                WorldDriverKind::BrowserExtension | WorldDriverKind::MobileBrowser,
                WorldDriverTransport::ClientQueue,
            ) => {}
            _ => {
                return Err(format!(
                    "world driver '{driver_id}' kind and transport are not valid for client registration"
                ));
            }
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
    use medousa_world::{WorldDriverCapability, WorldOwnership};
    use serde_json::json;
    use std::collections::BTreeSet;

    fn browser_registration(client_id: &str, driver_id: &str) -> ClientRegistration {
        ClientRegistration {
            client_id: client_id.to_string(),
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
            world_drivers: vec![WorldDriverRegistration {
                driver_id: WorldDriverId::new(driver_id),
                kind: WorldDriverKind::BrowserExtension,
                surface: WorldSurfaceKind::Browser,
                ownership: WorldOwnership::Attached,
                transport: WorldDriverTransport::ClientQueue,
                capabilities: BTreeSet::from([WorldDriverCapability::SemanticObservation]),
                display_name: Some("Test browser".to_string()),
            }],
            registered_at_utc: Utc::now(),
            last_seen_at_utc: Utc::now(),
        }
    }

    fn registration(registry: &ClientRegistry) {
        registry
            .register(browser_registration("browser-one", "driver:browser-one"))
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
    async fn exact_driver_routing_cannot_be_clobbered_by_another_browser_client() {
        let registry = ClientRegistry::new();
        registration(&registry);
        registry
            .register(browser_registration("browser-two", "driver:browser-two"))
            .unwrap();

        let (_, response) = registry
            .enqueue_tool_call_for_driver(
                &WorldDriverId::new("driver:browser-one"),
                "browser_page_snapshot",
                json!({"include_text": true}),
                "turn-one".to_string(),
            )
            .await
            .unwrap();
        assert!(
            registry
                .next_tool_request("browser-two", Duration::ZERO)
                .await
                .unwrap()
                .is_none()
        );
        let request = registry
            .next_tool_request("browser-one", Duration::ZERO)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            request.world_driver_id,
            Some(WorldDriverId::new("driver:browser-one"))
        );
        assert!(registry.complete_tool_request(
            "browser-one",
            &request.request_id,
            Ok(json!({"title": "Exact"})),
        ));
        assert_eq!(response.await.unwrap().unwrap()["title"], "Exact");
    }

    #[test]
    fn rejects_driver_identity_owned_by_another_client() {
        let registry = ClientRegistry::new();
        registration(&registry);
        let error = registry
            .register(browser_registration("browser-two", "driver:browser-one"))
            .unwrap_err();
        assert!(error.contains("already registered by another client"));
    }

    #[tokio::test]
    async fn dynamic_registry_lists_tools_for_active_surface() {
        let clients = ClientRegistry::new();
        registration(&clients);
        let scope = crate::agent_runtime::execution_context::TurnScopeAccess::for_test(
            crate::turn_continuation::TurnContinuationScope {
                turn_correlation_id: "turn-one".to_string(),
                session_id: "session-one".to_string(),
                identity_user_id: None,
                original_prompt: "read the page".to_string(),
                delivery_target: None,
                provider: "test".to_string(),
                model: "test".to_string(),
                response_depth_mode: "balanced".to_string(),
                supports_ui_artifacts: false,
                supports_liquid_markdown: false,
                supports_browser_host: false,
                browser_driver_id: Some("driver:browser-one".to_string()),
                selected_worlds: Vec::new(),
                channel_surface: Some("browser".to_string()),
            },
        );
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
                world_drivers: Vec::new(),
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
                world_drivers: Vec::new(),
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
                world_drivers: Vec::new(),
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
