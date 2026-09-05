//! `cognition_browser_snapshot` — markdown snapshot of a URL via Agent Browser.

use base64::Engine as _;
use medousa_browser_bridge::{
    BROWSER_SCREENSHOT_SCHEMA_VERSION, BrowserObservation, BrowserScreenshotCapture,
};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use sha2::{Digest as _, Sha256};
use stasis::domain::errors::StasisError;
use tokio::sync::mpsc;

use crate::browser_host_client::{
    BrowserHostWorldContext, browser_host_current_context, browser_host_fetch,
    browser_host_healthy, browser_host_observe, browser_host_screenshot,
};
use crate::browser_search::surface_from_scope;
use crate::browser_tools::{
    BrowserUrlCommand, COGNITION_BROWSER_SNAPSHOT, surface_supports_browser_host,
};
use crate::events::TuiEvent;
use crate::typed_tools::{CompatOption, ToolId, medousa_tool};

const COGNITION_BROWSER_SNAPSHOT_ID: ToolId = ToolId::new(COGNITION_BROWSER_SNAPSHOT);

pub struct CognitionBrowserSnapshotTool {
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionBrowserSnapshotTool {
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
        let scope =
            crate::agent_runtime::execution_context::turn_continuation_scope(&self.turn_scope)
                .await;
        surface_supports_browser_host(surface_from_scope(scope.as_ref()).as_ref())
    }
}

fn default_browser_max_chars() -> usize {
    4_000
}

fn default_browser_screenshot_width() -> u32 {
    1280
}

fn deserialize_browser_max_chars<'de, D>(deserializer: D) -> Result<usize, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(value
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or_else(default_browser_max_chars))
}

#[derive(Debug, JsonSchema)]
pub struct BrowserSnapshotInput {
    /// Absolute URL to snapshot (required)
    #[schemars(required, with = "String")]
    url: CompatOption<String>,
    /// Maximum excerpt length in characters
    #[schemars(with = "i64", default = "default_browser_max_chars")]
    max_chars: usize,
    /// Return only semantic changes after this observation revision when possible
    #[serde(default)]
    #[schemars(
        with = "Option<u64>",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    since_revision: CompatOption<u64>,
    /// Capture the visible viewport as a redacted out-of-band PNG artifact
    #[serde(default)]
    capture_screenshot: bool,
    /// Maximum stored screenshot width in pixels
    #[serde(default = "default_browser_screenshot_width")]
    #[schemars(
        default = "default_browser_screenshot_width",
        range(min = 320, max = 1600)
    )]
    screenshot_max_width: u32,
}

impl<'de> Deserialize<'de> for BrowserSnapshotInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(default)]
            url: CompatOption<String>,
            #[serde(
                default = "default_browser_max_chars",
                deserialize_with = "deserialize_browser_max_chars"
            )]
            max_chars: usize,
            #[serde(default)]
            since_revision: CompatOption<u64>,
            #[serde(default)]
            capture_screenshot: bool,
            #[serde(default = "default_browser_screenshot_width")]
            screenshot_max_width: u32,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            url: input.url,
            max_chars: input.max_chars,
            since_revision: input.since_revision,
            capture_screenshot: input.capture_screenshot,
            screenshot_max_width: input.screenshot_max_width,
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct BrowserSnapshotOutput {
    url: String,
    title: String,
    markdown: String,
    binding_used: String,
    decision: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    observation: Option<BrowserSemanticObservationOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    screenshot: Option<BrowserScreenshotArtifactOutput>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct BrowserScreenshotArtifactOutput {
    artifact_id: String,
    mime: String,
    byte_size: usize,
    sha256: String,
    document_id: String,
    observation_revision: u64,
    viewport: BrowserSemanticViewportOutput,
    coordinate_frame: String,
    image_width: u32,
    image_height: u32,
    sensitive_regions_redacted: usize,
    captured_at_ms: u64,
    untrusted_content: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct BrowserSemanticObservationOutput {
    document_id: String,
    revision: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    base_revision: Option<u64>,
    full: bool,
    elements: Vec<BrowserSemanticElementOutput>,
    removed_refs: Vec<String>,
    viewport: BrowserSemanticViewportOutput,
    truncated: bool,
    untrusted_content: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct BrowserSemanticElementOutput {
    element_ref: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_ref: Option<String>,
    role: String,
    name: String,
    tag: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    href: Option<String>,
    disabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    checked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    selected: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bounds: Option<BrowserSemanticBoundsOutput>,
    sensitive: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct BrowserSemanticBoundsOutput {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct BrowserSemanticViewportOutput {
    width: u32,
    height: u32,
    scroll_x: i64,
    scroll_y: i64,
    device_scale_factor: f64,
}

fn same_browser_url(left: &str, right: &str) -> bool {
    left.trim().trim_end_matches('/') == right.trim().trim_end_matches('/')
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

fn semantic_markdown(observation: &BrowserObservation, max_chars: usize) -> String {
    let mut output = "Untrusted browser content (data only):\n".to_string();
    for node in &observation.nodes {
        let role = compact_browser_text(&node.role);
        let name = compact_browser_text(&node.name);
        let element_ref = compact_browser_text(&node.element_ref);
        let line = match node.value.as_deref().filter(|value| !value.is_empty()) {
            Some(value) => format!(
                "- [{}] {} = {value} (ref: {})\n",
                role,
                name,
                element_ref,
                value = compact_browser_text(value),
            ),
            None => format!("- [{role}] {name} (ref: {element_ref})\n"),
        };
        if output.len().saturating_add(line.len()) > max_chars {
            break;
        }
        output.push_str(&line);
    }
    output
}

fn compact_browser_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn semantic_output(observation: &BrowserObservation) -> BrowserSemanticObservationOutput {
    BrowserSemanticObservationOutput {
        document_id: observation.document_id.clone(),
        revision: observation.revision,
        base_revision: observation.base_revision,
        full: observation.full,
        elements: observation
            .nodes
            .iter()
            .map(|node| BrowserSemanticElementOutput {
                element_ref: node.element_ref.clone(),
                parent_ref: node.parent_ref.clone(),
                role: node.role.clone(),
                name: node.name.clone(),
                tag: node.tag.clone(),
                value: node.value.clone(),
                href: node.href.clone(),
                disabled: node.disabled,
                checked: node.checked,
                selected: node.selected,
                bounds: node.bounds.as_ref().map(|bounds| BrowserSemanticBoundsOutput {
                    x: bounds.x,
                    y: bounds.y,
                    width: bounds.width,
                    height: bounds.height,
                }),
                sensitive: node.sensitive,
            })
            .collect(),
        removed_refs: observation.removed_refs.clone(),
        viewport: semantic_viewport_output(&observation.viewport),
        truncated: observation.truncated,
        untrusted_content: observation.untrusted_content,
    }
}

fn semantic_viewport_output(
    viewport: &medousa_browser_bridge::BrowserObservationViewport,
) -> BrowserSemanticViewportOutput {
    BrowserSemanticViewportOutput {
        width: viewport.width,
        height: viewport.height,
        scroll_x: viewport.scroll_x,
        scroll_y: viewport.scroll_y,
        device_scale_factor: viewport.device_scale_factor,
    }
}

async fn capture_screenshot_artifact(
    authority_id: &str,
    context: &BrowserHostWorldContext,
    trace_id: &str,
    session_id: &str,
    observation: &BrowserObservation,
    max_width: u32,
) -> Result<BrowserScreenshotArtifactOutput, String> {
    crate::world_authority::validate_browser_pixel_fence(
        authority_id,
        &context.tab_group_id,
        &context.tab_id,
        &context.url,
        &observation.document_id,
        observation.revision,
    )?;
    let admission = crate::world_authority::admit_browser_pixel_observation(
        authority_id,
        &context.tab_group_id,
        &context.tab_id,
        trace_id,
        "capture current browser viewport pixels",
    )?;
    let result = capture_screenshot_artifact_inner(
        &admission,
        context,
        session_id,
        observation,
        max_width,
    )
    .await;
    match result {
        Ok(output) => {
            crate::world_authority::complete_browser_action(
                &admission,
                "redacted browser screenshot artifact persisted",
            )?;
            Ok(output)
        }
        Err(error) => {
            let _ = crate::world_authority::fail_browser_action(&admission, &error);
            Err(error)
        }
    }
}

async fn capture_screenshot_artifact_inner(
    admission: &crate::world_authority::BrowserWorldAdmission,
    context: &BrowserHostWorldContext,
    session_id: &str,
    observation: &BrowserObservation,
    max_width: u32,
) -> Result<BrowserScreenshotArtifactOutput, String> {
    const MAX_SCREENSHOT_BYTES: usize = 8 * 1024 * 1024;
    const MAX_SCREENSHOT_BASE64_BYTES: usize = 12 * 1024 * 1024;
    const MAX_SCREENSHOT_PIXELS: u64 = 16_000_000;

    let capture: BrowserScreenshotCapture = browser_host_screenshot(
        &context.tab_group_id,
        serde_json::json!({
            "expected_document_id": observation.document_id,
            "expected_observation_revision": observation.revision,
            "max_width": max_width.clamp(320, 1600),
            "world_permit": admission.permit,
            "world_expected_url": context.url,
        }),
    )
    .await?;
    if capture.schema_version != BROWSER_SCREENSHOT_SCHEMA_VERSION
        || capture.tab_id != context.tab_id
        || !same_browser_url(&capture.url, &context.url)
        || capture.document_id != observation.document_id
        || capture.observation_revision != observation.revision
        || capture.viewport != observation.viewport
        || capture.coordinate_frame != "css_viewport"
        || capture.mime != "image/png"
        || !capture.untrusted_content
    {
        return Err("BrowserHost returned screenshot metadata for the wrong browser state".to_string());
    }
    if capture.image_width == 0
        || capture.image_height == 0
        || capture.image_width > 1600
        || capture.image_base64.len() > MAX_SCREENSHOT_BASE64_BYTES
    {
        return Err("BrowserHost screenshot exceeds its declared bounds".to_string());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&capture.image_base64)
        .map_err(|error| format!("BrowserHost returned invalid screenshot bytes: {error}"))?;
    if bytes.is_empty()
        || bytes.len() > MAX_SCREENSHOT_BYTES
        || bytes.len() != capture.byte_size
    {
        return Err("BrowserHost screenshot payload does not match its receipt".to_string());
    }
    let (png_width, png_height) = png_dimensions(&bytes)
        .ok_or_else(|| "BrowserHost screenshot payload is not a bounded PNG".to_string())?;
    if png_width != capture.image_width
        || png_height != capture.image_height
        || u64::from(png_width) * u64::from(png_height) > MAX_SCREENSHOT_PIXELS
    {
        return Err("BrowserHost screenshot dimensions do not match its receipt".to_string());
    }
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    if sha256 != capture.sha256 {
        return Err("BrowserHost screenshot digest does not match its pixels".to_string());
    }

    let stored_session_id = session_id.to_string();
    let label = format!("Browser viewport — {}", capture.title);
    let record = tokio::task::spawn_blocking(move || {
        crate::artifact_store::persist_binary_artifact(
            &stored_session_id,
            COGNITION_BROWSER_SNAPSHOT,
            "screenshot",
            "image/png",
            Some(&label),
            &bytes,
        )
    })
    .await
    .map_err(|error| format!("browser screenshot persistence task failed: {error}"))??;
    if record.hash64 != capture.sha256 || record.byte_size != capture.byte_size {
        return Err("persisted browser screenshot receipt does not match the capture".to_string());
    }

    Ok(BrowserScreenshotArtifactOutput {
        artifact_id: record.artifact_id,
        mime: capture.mime,
        byte_size: capture.byte_size,
        sha256: capture.sha256,
        document_id: capture.document_id,
        observation_revision: capture.observation_revision,
        viewport: semantic_viewport_output(&capture.viewport),
        coordinate_frame: capture.coordinate_frame,
        image_width: capture.image_width,
        image_height: capture.image_height,
        sensitive_regions_redacted: capture.sensitive_regions_redacted,
        captured_at_ms: capture.captured_at_ms,
        untrusted_content: true,
    })
}

#[medousa_tool(id = COGNITION_BROWSER_SNAPSHOT_ID)]
impl CognitionBrowserSnapshotTool {
    /// Observe the current shared page as a bounded semantic projection with opaque element refs and revisioned deltas; optionally persist a redacted viewport screenshot; falls back to markdown for other URLs.
    async fn invoke_typed(
        &self,
        input: BrowserSnapshotInput,
    ) -> stasis::prelude::Result<BrowserSnapshotOutput> {
        if !self.browser_enabled().await {
            return Err(StasisError::PortFailure(format!(
                "{COGNITION_BROWSER_SNAPSHOT}: requires supports_browser_host client (Home desktop/iOS)"
            )));
        }

        let command = BrowserUrlCommand::new(
            input.url.into_option(),
            input.max_chars,
            COGNITION_BROWSER_SNAPSHOT,
        )?;
        let url = command.url.into_string();
        let max_chars = command.max_chars;
        let since_revision = input.since_revision.into_option();
        let capture_screenshot = input.capture_screenshot;
        let screenshot_max_width = input.screenshot_max_width.clamp(320, 1600);

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: COGNITION_BROWSER_SNAPSHOT.to_string(),
                input_summary: url.clone(),
            })
            .await;

        let scope =
            crate::agent_runtime::execution_context::turn_continuation_scope(&self.turn_scope)
                .await;
        let trace_id = scope
            .as_ref()
            .map(|scope| scope.turn_correlation_id.as_str())
            .filter(|trace_id| !trace_id.trim().is_empty())
            .unwrap_or("browser-observation");
        let session_id = scope
            .as_ref()
            .map(|scope| scope.session_id.as_str())
            .filter(|session_id| !session_id.trim().is_empty());

        if browser_host_healthy().await {
            if let Ok(context) = browser_host_current_context().await
                && same_browser_url(&context.url, &url)
            {
                let authority_id = crate::workshop_authority::current()
                    .map_err(StasisError::PortFailure)?
                    .to_string();
                let admission = crate::world_authority::admit_browser_observation(
                    &authority_id,
                    &context.tab_group_id,
                    &context.tab_id,
                    trace_id,
                    "observe current browser tab",
                )
                .map_err(|error| {
                    StasisError::PortFailure(format!(
                        "{COGNITION_BROWSER_SNAPSHOT}: world observation denied: {error}"
                    ))
                })?;
                match browser_host_observe(&context.tab_group_id, since_revision, 256).await {
                    Ok(mut observation) => {
                        if let Err(initial_error) =
                            crate::world_authority::record_browser_observation(
                                &admission,
                                observation.clone(),
                            )
                        {
                            if !observation.full {
                                observation = browser_host_observe(&context.tab_group_id, None, 256)
                                    .await
                                    .map_err(|error| {
                                        let _ = crate::world_authority::fail_browser_action(
                                            &admission,
                                            &error,
                                        );
                                        StasisError::PortFailure(format!(
                                            "{COGNITION_BROWSER_SNAPSHOT}: could not recover a full browser observation: {error}"
                                        ))
                                    })?;
                                if let Err(error) =
                                    crate::world_authority::record_browser_observation(
                                        &admission,
                                        observation.clone(),
                                    )
                                {
                                    let _ = crate::world_authority::fail_browser_action(
                                        &admission,
                                        &error,
                                    );
                                    return Err(StasisError::PortFailure(format!(
                                        "{COGNITION_BROWSER_SNAPSHOT}: could not recover browser mirror after {initial_error}: {error}"
                                    )));
                                }
                            } else {
                                let _ = crate::world_authority::fail_browser_action(
                                    &admission,
                                    &initial_error,
                                );
                                return Err(StasisError::PortFailure(format!(
                                    "{COGNITION_BROWSER_SNAPSHOT}: could not advance browser mirror: {initial_error}"
                                )));
                            }
                        }
                        crate::world_authority::complete_browser_action(
                            &admission,
                            "browser semantic observation committed",
                        )
                        .map_err(StasisError::PortFailure)?;
                        let screenshot = if capture_screenshot {
                            let session_id = session_id.ok_or_else(|| {
                                StasisError::PortFailure(format!(
                                    "{COGNITION_BROWSER_SNAPSHOT}: screenshot capture requires an admitted turn session"
                                ))
                            })?;
                            Some(
                                capture_screenshot_artifact(
                                    &authority_id,
                                    &context,
                                    trace_id,
                                    session_id,
                                    &observation,
                                    screenshot_max_width,
                                )
                                .await
                                .map_err(|error| {
                                    StasisError::PortFailure(format!(
                                        "{COGNITION_BROWSER_SNAPSHOT}: pixel observation failed: {error}"
                                    ))
                                })?,
                            )
                        } else {
                            None
                        };
                        let semantic = semantic_output(&observation);
                        return Ok(BrowserSnapshotOutput {
                            url: context.url,
                            title: observation.title.clone(),
                            markdown: semantic_markdown(&observation, max_chars),
                            binding_used: "browser_host_semantic".to_string(),
                            decision: "allow".to_string(),
                            observation: Some(semantic),
                            screenshot,
                        });
                    }
                    Err(error) => {
                        crate::world_authority::fail_browser_action(&admission, &error)
                            .map_err(StasisError::PortFailure)?;
                        if capture_screenshot {
                            return Err(StasisError::PortFailure(format!(
                                "{COGNITION_BROWSER_SNAPSHOT}: semantic observation required before pixel capture: {error}"
                            )));
                        }
                    }
                }
            }
            if capture_screenshot {
                return Err(StasisError::PortFailure(format!(
                    "{COGNITION_BROWSER_SNAPSHOT}: screenshot capture is limited to the current shared browser tab"
                )));
            }
            let fetched = browser_host_fetch(&url, max_chars)
                .await
                .map_err(StasisError::PortFailure)?;
            return Ok(BrowserSnapshotOutput {
                url: fetched.url,
                title: fetched.title,
                markdown: fetched.markdown,
                binding_used: "browser_host".to_string(),
                decision: "allow".to_string(),
                observation: None,
                screenshot: None,
            });
        }

        if capture_screenshot {
            return Err(StasisError::PortFailure(format!(
                "{COGNITION_BROWSER_SNAPSHOT}: screenshot capture requires a healthy shared BrowserHost"
            )));
        }

        let fetched = tokio::task::spawn_blocking(move || {
            medousa_browser_lite::fetch_url_markdown(&url, max_chars)
        })
        .await
        .map_err(|err| StasisError::PortFailure(err.to_string()))?
        .map_err(StasisError::PortFailure)?;

        Ok(BrowserSnapshotOutput {
            url: fetched.url,
            title: fetched.title,
            markdown: fetched.markdown,
            binding_used: "browser_host_lite".to_string(),
            decision: "allow".to_string(),
            observation: None,
            screenshot: None,
        })
    }
}

pub fn register_browser_snapshot_tool(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    event_tx: mpsc::Sender<TuiEvent>,
) -> stasis::prelude::Result<()> {
    registry.register_typed_tool(CognitionBrowserSnapshotTool::new(turn_scope, event_tx))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_wire_url_remains_lenient_for_legacy_values() {
        let input: BrowserSnapshotInput = serde_json::from_value(serde_json::json!({
            "url": 42,
            "max_chars": "4000",
            "since_revision": "1",
        }))
        .expect("snapshot input");
        assert!(input.url.into_option().is_none());
        assert_eq!(input.max_chars, 4_000);
        assert!(input.since_revision.into_option().is_none());
        assert!(!input.capture_screenshot);
        assert_eq!(input.screenshot_max_width, 1_280);
    }

    #[test]
    fn screenshot_capture_is_explicit_and_width_is_bounded_at_execution() {
        let input: BrowserSnapshotInput = serde_json::from_value(serde_json::json!({
            "url": "https://example.test",
            "capture_screenshot": true,
            "screenshot_max_width": 9000,
        }))
        .expect("snapshot input");
        assert!(input.capture_screenshot);
        assert_eq!(input.screenshot_max_width.clamp(320, 1600), 1600);
    }

    #[test]
    fn png_dimensions_require_a_nonempty_ihdr() {
        let mut header = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        header.extend_from_slice(&640_u32.to_be_bytes());
        header.extend_from_slice(&480_u32.to_be_bytes());
        assert_eq!(png_dimensions(&header), Some((640, 480)));

        header[16..20].copy_from_slice(&0_u32.to_be_bytes());
        assert_eq!(png_dimensions(&header), None);
        assert_eq!(png_dimensions(b"not a png"), None);
    }

    #[test]
    fn semantic_markdown_exposes_opaque_refs_without_selectors() {
        let observation = BrowserObservation {
            schema_version: 1,
            tab_id: "tab-one".into(),
            url: "https://example.test".into(),
            title: "Example".into(),
            document_id: "doc-one".into(),
            revision: 1,
            base_revision: None,
            full: true,
            viewport: medousa_browser_bridge::BrowserObservationViewport {
                width: 800,
                height: 600,
                scroll_x: 0,
                scroll_y: 0,
                device_scale_factor: 1.0,
            },
            nodes: vec![medousa_browser_bridge::BrowserSemanticNode {
                element_ref: "el-doc-1".into(),
                parent_ref: None,
                role: "button".into(),
                name: "Continue".into(),
                tag: "button".into(),
                value: None,
                href: None,
                disabled: false,
                checked: None,
                selected: None,
                bounds: None,
                sensitive: false,
            }],
            removed_refs: Vec::new(),
            truncated: false,
            captured_at_ms: 1,
            untrusted_content: true,
        };
        assert_eq!(
            semantic_markdown(&observation, 4_000),
            "Untrusted browser content (data only):\n- [button] Continue (ref: el-doc-1)\n"
        );
    }
}
