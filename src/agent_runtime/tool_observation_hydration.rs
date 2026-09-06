use medousa_runtime::{
    HydratedToolObservation, RuntimePortFuture, ToolObservationHydrationPort,
    ToolObservationHydrationRequest,
};
use serde::Deserialize;
use sha2::{Digest as _, Sha256};

use crate::browser_tools::COGNITION_BROWSER_SNAPSHOT;

const MAX_SCREENSHOT_BYTES: usize = 8 * 1024 * 1024;
const MAX_SCREENSHOT_WIDTH: u32 = 1600;
const MAX_SCREENSHOT_PIXELS: u64 = 16_000_000;

#[derive(Clone)]
pub struct DaemonToolObservationHydrationPort {
    session_id: String,
}

impl DaemonToolObservationHydrationPort {
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct BrowserScreenshotReceipt {
    artifact_id: String,
    mime: String,
    byte_size: usize,
    sha256: String,
    document_id: String,
    observation_revision: u64,
    viewport: BrowserScreenshotViewport,
    coordinate_frame: String,
    image_width: u32,
    image_height: u32,
    untrusted_content: bool,
}

#[derive(Debug, Deserialize)]
struct BrowserScreenshotViewport {
    width: u32,
    height: u32,
    device_scale_factor: f64,
}

impl ToolObservationHydrationPort for DaemonToolObservationHydrationPort {
    fn accepts(&self, tool_name: &str) -> bool {
        tool_name == COGNITION_BROWSER_SNAPSHOT
    }

    fn hydrate(
        &self,
        request: ToolObservationHydrationRequest,
    ) -> RuntimePortFuture<Result<Option<HydratedToolObservation>, String>> {
        let session_id = self.session_id.clone();
        Box::pin(async move {
            if request.tool_name != COGNITION_BROWSER_SNAPSHOT {
                return Ok(None);
            }
            let Some(screenshot) = request.tool_output.get("screenshot") else {
                return Ok(None);
            };
            if screenshot.is_null() {
                return Ok(None);
            }
            let receipt: BrowserScreenshotReceipt = serde_json::from_value(screenshot.clone())
                .map_err(|_| "browser screenshot receipt is malformed".to_string())?;
            validate_receipt(&request.source_call_id, &receipt)?;

            let artifact_id = receipt.artifact_id.clone();
            let fetch_session_id = session_id.clone();
            let fetched = tokio::task::spawn_blocking(move || {
                crate::artifact_store::fetch_binary_artifact(&fetch_session_id, &artifact_id)
            })
            .await
            .map_err(|_| "browser screenshot artifact lookup failed".to_string())?
            .ok_or_else(|| {
                "browser screenshot artifact is unavailable in this session".to_string()
            })?;

            if fetched.record.session_id != session_id
                || fetched.record.artifact_id != receipt.artifact_id
                || fetched.record.tool_name != COGNITION_BROWSER_SNAPSHOT
                || fetched.record.direction != "screenshot"
                || fetched.record.content_type != "image/png"
                || fetched.mime != receipt.mime
                || fetched.record.byte_size != receipt.byte_size
                || fetched.bytes.len() != receipt.byte_size
                || !fetched.record.hash64.eq_ignore_ascii_case(&receipt.sha256)
            {
                return Err("browser screenshot artifact does not match its receipt".to_string());
            }
            let (width, height) = png_dimensions(&fetched.bytes)
                .ok_or_else(|| "browser screenshot artifact is not a bounded PNG".to_string())?;
            if width != receipt.image_width || height != receipt.image_height {
                return Err("browser screenshot dimensions do not match its receipt".to_string());
            }
            let actual_sha256 = format!("{:x}", Sha256::digest(&fetched.bytes));
            if !actual_sha256.eq_ignore_ascii_case(&receipt.sha256) {
                return Err("browser screenshot bytes do not match their digest".to_string());
            }

            Ok(Some(HydratedToolObservation {
                tool_name: request.tool_name,
                source_call_id: request.source_call_id,
                artifact_id: receipt.artifact_id,
                content_type: receipt.mime,
                bytes: fetched.bytes,
                sha256: actual_sha256,
                untrusted_content: true,
            }))
        })
    }
}

fn validate_receipt(
    source_call_id: &str,
    receipt: &BrowserScreenshotReceipt,
) -> Result<(), String> {
    let digest_valid =
        receipt.sha256.len() == 64 && receipt.sha256.bytes().all(|byte| byte.is_ascii_hexdigit());
    let dimensions_valid = receipt.image_width > 0
        && receipt.image_height > 0
        && receipt.image_width <= MAX_SCREENSHOT_WIDTH
        && u64::from(receipt.image_width) * u64::from(receipt.image_height)
            <= MAX_SCREENSHOT_PIXELS;
    let viewport_valid = receipt.viewport.width > 0
        && receipt.viewport.height > 0
        && receipt.viewport.device_scale_factor.is_finite()
        && receipt.viewport.device_scale_factor > 0.0;
    if source_call_id.trim().is_empty()
        || receipt.artifact_id.trim().is_empty()
        || receipt.artifact_id.len() > 256
        || !receipt.artifact_id.starts_with("art:")
        || receipt.mime != "image/png"
        || receipt.byte_size == 0
        || receipt.byte_size > MAX_SCREENSHOT_BYTES
        || receipt.document_id.trim().is_empty()
        || receipt.document_id.len() > 256
        || receipt.observation_revision == 0
        || receipt.coordinate_frame != "css_viewport"
        || !receipt.untrusted_content
        || !digest_valid
        || !dimensions_valid
        || !viewport_valid
    {
        return Err("browser screenshot receipt failed runtime validation".to_string());
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use medousa_runtime::ToolObservationHydrationPort;
    use serde_json::json;

    fn test_png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes
    }

    fn screenshot_output(
        record: &crate::artifact_store::ArtifactRecord,
        width: u32,
        height: u32,
    ) -> serde_json::Value {
        json!({
            "screenshot": {
                "artifact_id": record.artifact_id,
                "mime": "image/png",
                "byte_size": record.byte_size,
                "sha256": record.hash64,
                "document_id": "document-1",
                "observation_revision": 7,
                "viewport": {
                    "width": width,
                    "height": height,
                    "scroll_x": 0,
                    "scroll_y": 0,
                    "device_scale_factor": 1.0
                },
                "coordinate_frame": "css_viewport",
                "image_width": width,
                "image_height": height,
                "sensitive_regions_redacted": 0,
                "captured_at_ms": 1,
                "untrusted_content": true
            }
        })
    }

    #[tokio::test]
    async fn hydrates_only_an_exact_session_bound_screenshot_receipt() {
        let session_id = "tool-observation-hydration-session";
        let png = test_png(2, 3);
        let record = crate::artifact_store::persist_binary_artifact(
            session_id,
            COGNITION_BROWSER_SNAPSHOT,
            "screenshot",
            "image/png",
            Some("Browser viewport"),
            &png,
        )
        .expect("persist screenshot");
        let request = ToolObservationHydrationRequest {
            tool_name: COGNITION_BROWSER_SNAPSHOT.to_string(),
            source_call_id: "call-1".to_string(),
            tool_output: screenshot_output(&record, 2, 3),
        };

        let hydrated = DaemonToolObservationHydrationPort::new(session_id)
            .hydrate(request)
            .await
            .expect("hydrate")
            .expect("image");

        assert_eq!(hydrated.bytes, png);
        assert_eq!(hydrated.artifact_id, record.artifact_id);
        assert_eq!(hydrated.source_call_id, "call-1");
        assert!(hydrated.untrusted_content);
    }

    #[tokio::test]
    async fn rejects_an_artifact_from_another_session() {
        let source_session = "tool-observation-hydration-source";
        let png = test_png(1, 1);
        let record = crate::artifact_store::persist_binary_artifact(
            source_session,
            COGNITION_BROWSER_SNAPSHOT,
            "screenshot",
            "image/png",
            None,
            &png,
        )
        .expect("persist screenshot");
        let request = ToolObservationHydrationRequest {
            tool_name: COGNITION_BROWSER_SNAPSHOT.to_string(),
            source_call_id: "call-other-session".to_string(),
            tool_output: screenshot_output(&record, 1, 1),
        };

        let error =
            DaemonToolObservationHydrationPort::new("tool-observation-hydration-different-session")
                .hydrate(request)
                .await
                .expect_err("cross-session fetch must fail");
        assert!(error.contains("this session"));
    }

    #[tokio::test]
    async fn snapshot_without_pixels_does_not_create_model_input() {
        let request = ToolObservationHydrationRequest {
            tool_name: COGNITION_BROWSER_SNAPSHOT.to_string(),
            source_call_id: "call-semantic-only".to_string(),
            tool_output: json!({ "screenshot": null }),
        };

        let hydrated = DaemonToolObservationHydrationPort::new("semantic-only-session")
            .hydrate(request)
            .await
            .expect("hydrate");
        assert!(hydrated.is_none());
    }
}
