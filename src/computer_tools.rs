//! Model-facing tools for daemon-owned native computer drivers.
//!
//! These tools never talk to a platform sidecar directly. They resolve the
//! daemon's registered broker, preflight without prompting, and cross the same
//! world-authority boundary as the authenticated HTTP surface.

use base64::Engine as _;
use medousa_computer_bridge::{
    MAX_COMPUTER_SCREENSHOT_BASE64_BYTES, MAX_COMPUTER_SCREENSHOT_BYTES,
    MAX_COMPUTER_SCREENSHOT_HEIGHT, MAX_COMPUTER_SCREENSHOT_PIXELS,
    MAX_COMPUTER_SCREENSHOT_WIDTH, ComputerAction, ComputerPermissionKind,
    ComputerScreenshotCapture,
};
use medousa_world::{WorldDriverId, WorldPrincipal, WorldPrincipalId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest as _, Sha256};
use stasis::domain::errors::StasisError;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::computer_driver::{
    ComputerActionIntent, ComputerDriverBroker, ComputerObservationIntent,
    ComputerPixelObservationIntent, desktop_resource_id,
};
use crate::events::TuiEvent;
use crate::typed_tools::{ExternalJson, ToolId, medousa_tool};

pub const COGNITION_COMPUTER_SNAPSHOT: &str = "cognition_computer_snapshot";
pub const COGNITION_COMPUTER_ACT: &str = "cognition_computer_act";

const COGNITION_COMPUTER_SNAPSHOT_ID: ToolId = ToolId::new(COGNITION_COMPUTER_SNAPSHOT);
const COGNITION_COMPUTER_ACT_ID: ToolId = ToolId::new(COGNITION_COMPUTER_ACT);
const DEFAULT_AGENT_COMPUTER_NODES: u32 = 256;
const MAX_AGENT_COMPUTER_NODES: u32 = 512;

pub struct CognitionComputerSnapshotTool {
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionComputerSnapshotTool {
    pub fn new(
        turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
        event_tx: mpsc::Sender<TuiEvent>,
    ) -> Self {
        Self {
            turn_scope,
            event_tx,
        }
    }
}

pub struct CognitionComputerActTool {
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionComputerActTool {
    pub fn new(
        turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
        event_tx: mpsc::Sender<TuiEvent>,
    ) -> Self {
        Self {
            turn_scope,
            event_tx,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ComputerSnapshotInput {
    /// Exact driver id. Omit when this workshop has only one native driver.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    driver_id: Option<String>,
    /// Return only changes after this revision when the driver can do so.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    since_revision: Option<u64>,
    /// Maximum accessibility nodes returned to this turn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1, max = 512))]
    max_nodes: Option<u32>,
    /// Include a redacted screenshot of the focused window.
    #[serde(default)]
    capture_screenshot: bool,
    /// Screenshot width limit (320-1600 pixels).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 320, max = 1600))]
    screenshot_max_width: Option<u32>,
}

#[derive(Debug, Serialize)]
struct ComputerScreenshotArtifactOutput {
    artifact_id: String,
    mime: String,
    byte_size: usize,
    sha256: String,
    observation_generation: String,
    observation_revision: u64,
    window_resource_id: medousa_world::WorldResourceId,
    coordinate_frame: String,
    image_width: u32,
    image_height: u32,
    sensitive_regions_redacted: usize,
    captured_at_ms: u64,
    untrusted_content: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ComputerToolAction {
    Press,
    Focus,
    SetValue,
    ShowMenu,
    Increment,
    Decrement,
    ScrollToVisible,
    ForegroundClick,
}

impl From<ComputerToolAction> for ComputerAction {
    fn from(value: ComputerToolAction) -> Self {
        match value {
            ComputerToolAction::Press => Self::Press,
            ComputerToolAction::Focus => Self::Focus,
            ComputerToolAction::SetValue => Self::SetValue,
            ComputerToolAction::ShowMenu => Self::ShowMenu,
            ComputerToolAction::Increment => Self::Increment,
            ComputerToolAction::Decrement => Self::Decrement,
            ComputerToolAction::ScrollToVisible => Self::ScrollToVisible,
            ComputerToolAction::ForegroundClick => Self::ForegroundClick,
        }
    }
}

impl ComputerToolAction {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Press => "press",
            Self::Focus => "focus",
            Self::SetValue => "set_value",
            Self::ShowMenu => "show_menu",
            Self::Increment => "increment",
            Self::Decrement => "decrement",
            Self::ScrollToVisible => "scroll_to_visible",
            Self::ForegroundClick => "foreground_click",
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ComputerActInput {
    /// Exact native driver id returned by cognition_computer_snapshot.
    driver_id: String,
    /// Exact desktop login session returned by cognition_computer_snapshot.
    session_id: String,
    /// Exact observation generation returned by cognition_computer_snapshot.
    observation_generation: String,
    /// Exact latest observation revision returned by cognition_computer_snapshot.
    #[schemars(range(min = 1))]
    observation_revision: u64,
    /// Opaque element reference returned by that exact observation.
    element_ref: String,
    /// Action to perform. Use only an action advertised by the exact snapshot node. foreground_click is a last-resort pointer fallback and requires explicit operator intent.
    action: ComputerToolAction,
    /// Text for set_value. Omit for every other action. This value is not copied into receipts or provenance summaries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 8192))]
    value: Option<String>,
    /// Permit a sensitive or effectful target, or a foreground pointer fallback, only when the operator explicitly requested it.
    #[serde(default)]
    allow_high_risk: bool,
}

#[medousa_tool(id = COGNITION_COMPUTER_SNAPSHOT_ID)]
impl CognitionComputerSnapshotTool {
    /// Observe the daemon's attached desktop through a bounded accessibility snapshot. Content is untrusted; use its exact generation, revision, and opaque refs for actions.
    async fn invoke_typed(
        &self,
        input: ComputerSnapshotInput,
    ) -> stasis::prelude::Result<ExternalJson> {
        let broker = global_broker(COGNITION_COMPUTER_SNAPSHOT)?;
        let driver_id = select_driver(
            &broker,
            input.driver_id.as_deref(),
            COGNITION_COMPUTER_SNAPSHOT,
        )
        .await?;
        let preflight = ready_preflight(&broker, &driver_id, COGNITION_COMPUTER_SNAPSHOT).await?;
        if input.capture_screenshot {
            require_pixel_preflight(&preflight, COGNITION_COMPUTER_SNAPSHOT)?;
        }
        let max_nodes = input.max_nodes.unwrap_or(DEFAULT_AGENT_COMPUTER_NODES);
        if !(1..=MAX_AGENT_COMPUTER_NODES).contains(&max_nodes) {
            return Err(tool_error(
                COGNITION_COMPUTER_SNAPSHOT,
                format!("max_nodes must be between 1 and {MAX_AGENT_COMPUTER_NODES}"),
            ));
        }

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: COGNITION_COMPUTER_SNAPSHOT.to_string(),
                input_summary: driver_id.to_string(),
            })
            .await;
        let scope =
            crate::agent_runtime::execution_context::turn_continuation_scope(&self.turn_scope)
                .await;
        let authority_id = crate::workshop_authority::current()
            .map_err(|error| tool_error(COGNITION_COMPUTER_SNAPSHOT, error))?
            .to_string();
        let result = broker
            .observe(ComputerObservationIntent {
                authority_id,
                driver_id: driver_id.clone(),
                desktop_session_id: preflight.session_id.clone(),
                principal: agent_principal(scope.as_ref()),
                resource_id: desktop_resource_id(&driver_id, &preflight.session_id),
                trace_id: turn_trace_id(scope.as_ref(), "computer-agent:observe"),
                summary: "agent requested a semantic desktop observation".to_string(),
                after_revision: input.since_revision,
                max_nodes,
            })
            .await
            .map_err(|error| tool_error(COGNITION_COMPUTER_SNAPSHOT, error))?;

        let (screenshot, screenshot_provenance) = if input.capture_screenshot {
            let session_id = scope
                .as_ref()
                .map(|scope| scope.session_id.as_str())
                .filter(|session_id| !session_id.trim().is_empty())
                .ok_or_else(|| {
                    tool_error(
                        COGNITION_COMPUTER_SNAPSHOT,
                        "screenshot capture requires an admitted turn session",
                    )
                })?;
            let window_resource_id = result
                .observation
                .focused_window_resource_id
                .clone()
                .ok_or_else(|| {
                    tool_error(
                        COGNITION_COMPUTER_SNAPSHOT,
                        "the semantic observation did not contain a focused window",
                    )
                })?;
            let pixels = broker
                .capture_pixels(ComputerPixelObservationIntent {
                    authority_id: crate::workshop_authority::current()
                        .map_err(|error| tool_error(COGNITION_COMPUTER_SNAPSHOT, error))?
                        .to_string(),
                    driver_id: driver_id.clone(),
                    desktop_session_id: preflight.session_id.clone(),
                    principal: agent_principal(scope.as_ref()),
                    resource_id: desktop_resource_id(&driver_id, &preflight.session_id),
                    trace_id: turn_trace_id(scope.as_ref(), "computer-agent:observe-pixels"),
                    summary: "agent requested redacted focused-window pixels".to_string(),
                    observation_generation: result.observation.observation_generation.clone(),
                    observation_revision: result.observation.revision,
                    window_resource_id,
                    max_width: input.screenshot_max_width.unwrap_or(1_280),
                })
                .await
                .map_err(|error| tool_error(COGNITION_COMPUTER_SNAPSHOT, error))?;
            let receipt = persist_screenshot_artifact(session_id, pixels.capture).await?;
            (Some(receipt), Some(pixels.provenance))
        } else {
            (None, None)
        };

        Ok(ExternalJson::new(json!({
            "ok": true,
            "observation": result.observation,
            "provenance": result.provenance,
            "screenshot": screenshot,
            "screenshot_provenance": screenshot_provenance,
        })))
    }
}

#[medousa_tool(id = COGNITION_COMPUTER_ACT_ID)]
impl CognitionComputerActTool {
    /// Perform one desktop action against an exact opaque ref from the daemon's latest cognition_computer_snapshot; semantic actions stay preferred, foreground_click requires explicit operator intent, stale targets fail, and ambiguous actions are never retried.
    async fn invoke_typed(
        &self,
        input: ComputerActInput,
    ) -> stasis::prelude::Result<ExternalJson> {
        let broker = global_broker(COGNITION_COMPUTER_ACT)?;
        let driver_id =
            select_driver(&broker, Some(&input.driver_id), COGNITION_COMPUTER_ACT).await?;
        let preflight = ready_preflight(&broker, &driver_id, COGNITION_COMPUTER_ACT).await?;
        if preflight.session_id != input.session_id {
            return Err(tool_error(
                COGNITION_COMPUTER_ACT,
                "desktop session changed; observe the computer again",
            ));
        }

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: COGNITION_COMPUTER_ACT.to_string(),
                input_summary: format!("{} {}", input.action.as_str(), input.element_ref),
            })
            .await;
        let scope =
            crate::agent_runtime::execution_context::turn_continuation_scope(&self.turn_scope)
                .await;
        let authority_id = crate::workshop_authority::current()
            .map_err(|error| tool_error(COGNITION_COMPUTER_ACT, error))?
            .to_string();
        let result = broker
            .act(ComputerActionIntent {
                authority_id,
                driver_id: driver_id.clone(),
                desktop_session_id: input.session_id.clone(),
                principal: agent_principal(scope.as_ref()),
                resource_id: desktop_resource_id(&driver_id, &input.session_id),
                trace_id: turn_trace_id(scope.as_ref(), "computer-agent:act"),
                summary: format!(
                    "agent requested semantic desktop action {}",
                    input.action.as_str()
                ),
                observation_generation: input.observation_generation,
                observation_revision: input.observation_revision,
                element_ref: input.element_ref,
                action: input.action.into(),
                value: input.value,
                allow_high_risk: input.allow_high_risk,
            })
            .await
            .map_err(|error| tool_error(COGNITION_COMPUTER_ACT, error))?;

        Ok(ExternalJson::new(json!({
            "ok": true,
            "receipt": result.receipt,
            "provenance": result.provenance,
        })))
    }
}

pub fn register_computer_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    event_tx: mpsc::Sender<TuiEvent>,
) -> stasis::prelude::Result<()> {
    registry.register_typed_tool(CognitionComputerSnapshotTool::new(
        turn_scope.clone(),
        event_tx.clone(),
    ))?;
    registry.register_typed_tool(CognitionComputerActTool::new(turn_scope, event_tx))?;
    Ok(())
}

fn global_broker(
    tool: &str,
) -> stasis::prelude::Result<std::sync::Arc<ComputerDriverBroker>> {
    crate::daemon::computer_driver_host::global_computer_broker().ok_or_else(|| {
        tool_error(
            tool,
            "native computer broker is unavailable on this daemon",
        )
    })
}

async fn select_driver(
    broker: &ComputerDriverBroker,
    requested: Option<&str>,
    tool: &str,
) -> stasis::prelude::Result<WorldDriverId> {
    let registrations = broker.registrations().await;
    if let Some(requested) = requested {
        let requested = requested.trim();
        if requested.is_empty()
            || requested.len() > 256
            || requested.chars().any(char::is_control)
        {
            return Err(tool_error(
                tool,
                "computer driver id is invalid",
            ));
        }
        return registrations
            .into_iter()
            .find(|registration| registration.driver_id.as_str() == requested)
            .map(|registration| registration.driver_id)
            .ok_or_else(|| {
                tool_error(
                    tool,
                    format!("computer driver '{requested}' is not registered"),
                )
            });
    }
    match registrations.as_slice() {
        [] => Err(tool_error(
            tool,
            "no native computer driver is registered; install medousa-computer and restart the daemon",
        )),
        [registration] => Ok(registration.driver_id.clone()),
        _ => Err(tool_error(
            tool,
            format!(
                "multiple native computer drivers are registered; choose driver_id from [{}]",
                registrations
                    .iter()
                    .map(|registration| registration.driver_id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )),
    }
}

async fn ready_preflight(
    broker: &ComputerDriverBroker,
    driver_id: &WorldDriverId,
    tool: &str,
) -> stasis::prelude::Result<medousa_computer_bridge::ComputerDriverPreflight> {
    let preflight = broker
        .preflight(driver_id)
        .await
        .map_err(|error| tool_error(tool, error))?;
    if preflight.semantic_observation_ready() {
        return Ok(preflight);
    }
    let guidance = preflight
        .permissions
        .iter()
        .find(|permission| permission.permission == ComputerPermissionKind::Accessibility)
        .and_then(|permission| permission.guidance.as_deref())
        .unwrap_or("Grant the native computer driver Accessibility permission.");
    Err(tool_error(tool, guidance))
}

fn require_pixel_preflight(
    preflight: &medousa_computer_bridge::ComputerDriverPreflight,
    tool: &str,
) -> stasis::prelude::Result<()> {
    if preflight.pixel_observation_ready() {
        return Ok(());
    }
    let guidance = preflight
        .permissions
        .iter()
        .find(|permission| permission.permission == ComputerPermissionKind::ScreenCapture)
        .and_then(|permission| permission.guidance.as_deref())
        .unwrap_or("Grant the native computer driver Screen Recording permission.");
    Err(tool_error(tool, guidance))
}

async fn persist_screenshot_artifact(
    session_id: &str,
    capture: ComputerScreenshotCapture,
) -> stasis::prelude::Result<ComputerScreenshotArtifactOutput> {
    if capture.image_base64.len() > MAX_COMPUTER_SCREENSHOT_BASE64_BYTES {
        return Err(tool_error(
            COGNITION_COMPUTER_SNAPSHOT,
            "native screenshot exceeded its transport bound",
        ));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&capture.image_base64)
        .map_err(|error| {
            tool_error(
                COGNITION_COMPUTER_SNAPSHOT,
                format!("native screenshot contained invalid base64: {error}"),
            )
        })?;
    if bytes.is_empty()
        || bytes.len() > MAX_COMPUTER_SCREENSHOT_BYTES
        || bytes.len() != capture.byte_size
    {
        return Err(tool_error(
            COGNITION_COMPUTER_SNAPSHOT,
            "native screenshot bytes did not match their receipt",
        ));
    }
    let (width, height) = png_dimensions(&bytes).ok_or_else(|| {
        tool_error(
            COGNITION_COMPUTER_SNAPSHOT,
            "native screenshot was not a bounded PNG",
        )
    })?;
    if width != capture.image_width
        || height != capture.image_height
        || width > MAX_COMPUTER_SCREENSHOT_WIDTH
        || height > MAX_COMPUTER_SCREENSHOT_HEIGHT
        || u64::from(width) * u64::from(height) > MAX_COMPUTER_SCREENSHOT_PIXELS
    {
        return Err(tool_error(
            COGNITION_COMPUTER_SNAPSHOT,
            "native screenshot dimensions did not match their receipt",
        ));
    }
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    if !sha256.eq_ignore_ascii_case(&capture.sha256) {
        return Err(tool_error(
            COGNITION_COMPUTER_SNAPSHOT,
            "native screenshot digest did not match its pixels",
        ));
    }

    let stored_session_id = session_id.to_string();
    let record = tokio::task::spawn_blocking(move || {
        crate::artifact_store::persist_binary_artifact(
            &stored_session_id,
            COGNITION_COMPUTER_SNAPSHOT,
            "screenshot",
            "image/png",
            Some("Focused desktop window"),
            &bytes,
        )
    })
    .await
    .map_err(|error| {
        tool_error(
            COGNITION_COMPUTER_SNAPSHOT,
            format!("native screenshot persistence task failed: {error}"),
        )
    })?
    .map_err(|error| tool_error(COGNITION_COMPUTER_SNAPSHOT, error))?;
    if record.hash64 != capture.sha256 || record.byte_size != capture.byte_size {
        return Err(tool_error(
            COGNITION_COMPUTER_SNAPSHOT,
            "persisted native screenshot did not match its receipt",
        ));
    }

    Ok(ComputerScreenshotArtifactOutput {
        artifact_id: record.artifact_id,
        mime: capture.mime,
        byte_size: capture.byte_size,
        sha256: capture.sha256,
        observation_generation: capture.observation_generation,
        observation_revision: capture.observation_revision,
        window_resource_id: capture.window_resource_id,
        coordinate_frame: capture.coordinate_frame,
        image_width: capture.image_width,
        image_height: capture.image_height,
        sensitive_regions_redacted: capture.sensitive_regions_redacted,
        captured_at_ms: capture.captured_at_ms,
        untrusted_content: capture.untrusted_content,
    })
}

fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 24
        || !bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || bytes.get(12..16) != Some(b"IHDR".as_slice())
    {
        return None;
    }
    let width = u32::from_be_bytes(bytes.get(16..20)?.try_into().ok()?);
    let height = u32::from_be_bytes(bytes.get(20..24)?.try_into().ok()?);
    (width > 0 && height > 0).then_some((width, height))
}

fn agent_principal(
    scope: Option<&crate::turn_continuation::TurnContinuationScope>,
) -> WorldPrincipal {
    let identity = scope
        .and_then(|scope| scope.identity_user_id.as_deref())
        .or_else(|| scope.map(|scope| scope.session_id.as_str()))
        .unwrap_or("workshop-operator");
    let digest = Sha256::digest(identity.as_bytes());
    WorldPrincipal::agent(WorldPrincipalId::new(format!(
        "agent:medousa:sha256:{digest:x}"
    )))
}

fn turn_trace_id(
    scope: Option<&crate::turn_continuation::TurnContinuationScope>,
    fallback_prefix: &str,
) -> String {
    scope
        .map(|scope| scope.turn_correlation_id.trim())
        .filter(|value| {
            !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
        })
        .map(str::to_string)
        .unwrap_or_else(|| format!("{fallback_prefix}:{}", Uuid::new_v4()))
}

fn tool_error(tool: &str, error: impl std::fmt::Display) -> StasisError {
    StasisError::PortFailure(format!("{tool}: {error}"))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use medousa_computer_bridge::{
        COMPUTER_DRIVER_PROTOCOL_VERSION, ComputerDriverPreflight, ComputerPermissionReport,
        ComputerPermissionStatus,
    };
    use medousa_world::{
        WorldDriverCapability, WorldDriverKind, WorldDriverRegistration, WorldDriverTransport,
        WorldOwnership, WorldSurfaceKind,
    };

    use super::*;

    fn registration(driver_id: &str) -> WorldDriverRegistration {
        WorldDriverRegistration {
            driver_id: WorldDriverId::new(driver_id),
            kind: WorldDriverKind::NativeDesktop,
            surface: WorldSurfaceKind::Desktop,
            ownership: WorldOwnership::Attached,
            transport: WorldDriverTransport::InProcess,
            capabilities: [WorldDriverCapability::SemanticObservation]
                .into_iter()
                .collect(),
            display_name: None,
        }
    }

    struct SelectionDriver(WorldDriverRegistration);

    #[async_trait::async_trait]
    impl crate::computer_driver::ComputerDriver for SelectionDriver {
        fn registration(&self) -> WorldDriverRegistration {
            self.0.clone()
        }

        async fn preflight(&self) -> Result<ComputerDriverPreflight, String> {
            Ok(ComputerDriverPreflight {
                protocol_version: COMPUTER_DRIVER_PROTOCOL_VERSION,
                driver_id: self.0.driver_id.clone(),
                platform: "test".to_string(),
                session_id: "login:test".to_string(),
                permissions: vec![ComputerPermissionReport {
                    permission: ComputerPermissionKind::Accessibility,
                    status: ComputerPermissionStatus::Granted,
                    can_request: false,
                    guidance: None,
                }],
                checked_at_ms: 1,
            })
        }

        async fn observe(
            &self,
            _request: medousa_computer_bridge::ComputerObservationRequest,
        ) -> Result<medousa_computer_bridge::ComputerObservation, String> {
            unreachable!("selection test does not observe")
        }

        async fn screenshot(
            &self,
            _request: medousa_computer_bridge::ComputerScreenshotRequest,
        ) -> Result<medousa_computer_bridge::ComputerScreenshotCapture, String> {
            unreachable!("selection test does not capture pixels")
        }

        async fn act(
            &self,
            _request: medousa_computer_bridge::ComputerActionRequest,
        ) -> Result<
            medousa_computer_bridge::ComputerActionReceipt,
            crate::computer_driver::ComputerDriverActionError,
        > {
            unreachable!("selection test does not act")
        }
    }

    #[tokio::test]
    async fn one_registered_driver_is_selected_without_ceremony() {
        let broker = ComputerDriverBroker::new(Arc::new(
            crate::world_authority::WorldAuthorityService::default(),
        ));
        broker
            .register(Arc::new(SelectionDriver(registration("driver:one"))))
            .await
            .expect("register");

        assert_eq!(
            select_driver(&broker, None, COGNITION_COMPUTER_SNAPSHOT)
                .await
                .expect("automatic selection")
                .as_str(),
            "driver:one"
        );
    }

    #[tokio::test]
    async fn multiple_drivers_require_an_exact_selection() {
        let broker = ComputerDriverBroker::new(Arc::new(
            crate::world_authority::WorldAuthorityService::default(),
        ));
        for driver_id in ["driver:one", "driver:two"] {
            broker
                .register(Arc::new(SelectionDriver(registration(driver_id))))
                .await
                .expect("register");
        }

        let error = select_driver(&broker, None, COGNITION_COMPUTER_SNAPSHOT)
            .await
            .expect_err("selection must be explicit");
        assert!(error.to_string().contains("multiple native computer drivers"));
    }
}
