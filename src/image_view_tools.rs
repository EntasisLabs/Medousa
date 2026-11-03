//! Read workshop image files into verified, session-scoped vision observations.

use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use image::ImageDecoder as _;
use medousa_runtime::{
    HydratedToolObservation, RuntimePortFuture, ToolObservationHydrationRequest,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use stasis::prelude::{Result as StasisResult, StasisError};

use crate::typed_tools::{CompatOption, ToolId, medousa_tool};

pub const COGNITION_IMAGE_VIEW: &str = "cognition_image_view";
const IMAGE_VIEW_ID: ToolId = ToolId::new(COGNITION_IMAGE_VIEW);
const MAX_SOURCE_BYTES: u64 = crate::media_store::MAX_UPLOAD_BYTES;
const MAX_DECODE_BYTES: u64 = 256 * 1024 * 1024;

pub fn register_image_view_tool(
    registry: &mut impl crate::typed_tools::ToolRegistration,
) -> StasisResult<()> {
    registry.register_typed_tool(CognitionImageViewTool)?;
    Ok(())
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImageViewInput {
    /// Image path on the workshop daemon's disk, not the UI device. Relative to root.
    path: String,
    /// Existing authorized workshop root. Defaults to the admitted Coder checkout or scripts library.
    #[serde(default)]
    #[schemars(with = "String", skip_serializing_if = "CompatOption::is_none")]
    root: CompatOption<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImageViewReceipt {
    artifact_id: String,
    mime: String,
    byte_size: usize,
    sha256: String,
    width: u32,
    height: u32,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ImageViewOutput {
    ok: bool,
    session_id: String,
    /// True means pixels will be delivered through this turn's vision-capable model route.
    vision_available: bool,
    /// A bounded PNG rendition is saved; the source file is never modified.
    image: ImageViewReceipt,
}

pub struct CognitionImageViewTool;

#[medousa_tool(id = IMAGE_VIEW_ID)]
impl CognitionImageViewTool {
    /// View a PNG, JPEG, WebP, or GIF file produced or found through terminal tools. Sends actual pixels to vision-capable models, not just metadata. Paths are on the workshop daemon, confined to authorized coding roots; Coder uses its admitted checkout. No URLs or arbitrary host paths. Large images use a bounded PNG rendition; originals are unchanged. Invoke this tool directly: generic parallel wrappers may return metadata without forwarding image attachments. A receipt alone is not visual verification.
    async fn invoke_typed(&self, input: ImageViewInput) -> Result<ImageViewOutput, StasisError> {
        let context = crate::agent_runtime::execution_context::active_turn_execution_context()
            .ok_or_else(|| StasisError::PortFailure("image view requires an active turn".into()))?;
        // Never accidentally interpret a container/remote path on the daemon host.
        if context.work_environment().is_some() {
            return Err(StasisError::PortFailure(
                "image view requires a daemon-local image file; bound work-environment image export is not supported".into(),
            ));
        }
        let session_id = context.session_id().to_string();
        let vision_available = crate::media_vision::supports_vision(
            context.route().provider(),
            context.route().model(),
        );
        let coder_root = crate::coding_tools::admitted_coder_tool_root();
        let root = input.root.into_option().or_else(|| {
            coder_root
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned())
        });
        let output_session = session_id.clone();
        let receipt =
            // Admission accounts for the retained JSON receipt, not the source file.
            // Source and decoder allocations have separate hard bounds below.
            crate::media_vision::run_image_operation(4096, move || {
                let root = crate::coding_tools::resolve_read_root(root.as_deref(), coder_root)
                    .map_err(|error| medousa_forge::ForgeError::Store(error.to_string()))?;
                let bytes = read_image_file(&root, &input.path)
                    .map_err(medousa_forge::ForgeError::Store)?;
                let (bytes, width, height) =
                    render_image(bytes).map_err(medousa_forge::ForgeError::Store)?;
                snapshot_image(&output_session, &bytes, width, height)
                    .map_err(medousa_forge::ForgeError::Store)
            })
            .await
            .map_err(|error| StasisError::PortFailure(format!("image view failed: {error}")))?;
        Ok(ImageViewOutput {
            ok: true,
            session_id,
            vision_available,
            image: receipt,
        })
    }
}

struct ImageSourcePath(Vec<String>);

impl medousa_store::StoreRootPath for ImageSourcePath {
    fn segments(&self) -> &[String] {
        &self.0
    }
}

impl ImageSourcePath {
    fn parse(relative: &str) -> Result<Self, String> {
        let segments: Vec<_> = relative.split('/').collect();
        if relative.len() > 1024
            || segments.len() > 64
            || segments.iter().any(|part| {
                part.is_empty()
                    || matches!(*part, "." | "..")
                    || part.contains(['\\', ':'])
                    || part.chars().any(char::is_control)
            })
        {
            return Err("invalid relative image path".into());
        }
        Ok(Self(segments.into_iter().map(str::to_owned).collect()))
    }
}

fn read_image_file(root: &Path, path: &str) -> Result<Vec<u8>, String> {
    let path = PathBuf::from(path);
    let relative = if path.is_absolute() {
        path.strip_prefix(root)
            .map_err(|_| "image path escapes authorized root")?
    } else {
        &path
    };
    let relative = relative.to_str().ok_or("image path must be UTF-8")?;
    let path = ImageSourcePath::parse(relative)?;
    // Retain a directory capability and refuse symlink/hard-link substitution.
    let root = medousa_store::StoreRoot::open_nofollow(root).map_err(|error| error.to_string())?;
    let before_open = root.metadata(&path).map_err(|error| error.to_string())?;
    if before_open.kind != medousa_store::StoreEntryKind::File {
        return Err("image source must be a regular file".into());
    }
    let file = root
        .open_regular_read_file(&path)
        .map_err(|error| error.to_string())?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_SOURCE_BYTES {
        return Err("image must be a nonempty regular file of at most 25 MiB".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_SOURCE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_SOURCE_BYTES {
        return Err("image exceeds source byte budget or is empty".into());
    }
    Ok(bytes)
}

fn render_image(bytes: Vec<u8>) -> Result<(Vec<u8>, u32, u32), String> {
    let format = image::guess_format(&bytes).map_err(|_| "unsupported image format")?;
    if !matches!(
        format,
        image::ImageFormat::Png
            | image::ImageFormat::Jpeg
            | image::ImageFormat::WebP
            | image::ImageFormat::Gif
    ) {
        return Err("supported formats: PNG, JPEG, WebP, GIF".into());
    }
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(MAX_DECODE_BYTES);
    limits.max_image_width = Some(32_768);
    limits.max_image_height = Some(32_768);
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(limits.clone());
    let mut decoder = reader.into_decoder().map_err(|error| error.to_string())?;
    limits
        .reserve(decoder.total_bytes())
        .map_err(|error| error.to_string())?;
    decoder
        .set_limits(limits)
        .map_err(|error| error.to_string())?;
    let orientation = decoder
        .orientation()
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut decoded =
        image::DynamicImage::from_decoder(decoder).map_err(|error| error.to_string())?;
    decoded.apply_orientation(orientation);
    if decoded.width() > 4096 || decoded.height() > 4096 {
        decoded = decoded.thumbnail(4096, 4096);
    }
    loop {
        let mut output = Cursor::new(Vec::new());
        decoded
            .write_to(&mut output, image::ImageFormat::Png)
            .map_err(|error| error.to_string())?;
        let bytes = output.into_inner();
        if bytes.len() as u64 <= crate::media_vision::MAX_MODEL_IMAGE_BYTES {
            return Ok((bytes, decoded.width(), decoded.height()));
        }
        decoded = decoded.thumbnail((decoded.width() / 2).max(1), (decoded.height() / 2).max(1));
    }
}

fn snapshot_image(
    session_id: &str,
    bytes: &[u8],
    width: u32,
    height: u32,
) -> Result<ImageViewReceipt, String> {
    let record = crate::artifact_store::persist_binary_artifact(
        session_id,
        COGNITION_IMAGE_VIEW,
        "output",
        "image/png",
        Some("Workshop image observation"),
        bytes,
    )?;
    Ok(ImageViewReceipt {
        artifact_id: record.artifact_id,
        mime: "image/png".into(),
        byte_size: bytes.len(),
        sha256: format!("{:x}", Sha256::digest(bytes)),
        width,
        height,
    })
}

fn load_snapshot(session_id: &str, receipt: &ImageViewReceipt) -> Result<Vec<u8>, String> {
    if !receipt.artifact_id.starts_with("art:")
        || receipt.artifact_id.len() > 256
        || receipt.mime != "image/png"
        || receipt.byte_size == 0
        || receipt.byte_size as u64 > crate::media_vision::MAX_MODEL_IMAGE_BYTES
        || receipt.width == 0
        || receipt.height == 0
        || receipt.width > 4096
        || receipt.height > 4096
    {
        return Err("invalid image view receipt".into());
    }
    let stored = crate::artifact_store::fetch_binary_artifact(session_id, &receipt.artifact_id)
        .ok_or("image snapshot unavailable in the active session")?;
    if stored.record.session_id != session_id
        || stored.record.tool_name != COGNITION_IMAGE_VIEW
        || stored.record.direction != "output"
        || stored.mime != receipt.mime
        || stored.bytes.len() != receipt.byte_size
        || stored.record.byte_size != receipt.byte_size
        || format!("{:x}", Sha256::digest(&stored.bytes)) != receipt.sha256
    {
        return Err("image snapshot does not match receipt".into());
    }
    let dimensions =
        image::ImageReader::with_format(Cursor::new(&stored.bytes), image::ImageFormat::Png)
            .into_dimensions()
            .map_err(|error| error.to_string())?;
    if dimensions != (receipt.width, receipt.height) {
        return Err("image snapshot dimensions do not match receipt".into());
    }
    Ok(stored.bytes)
}

pub(crate) fn hydrate(
    request: ToolObservationHydrationRequest,
) -> RuntimePortFuture<Result<Option<HydratedToolObservation>, String>> {
    let context = crate::agent_runtime::execution_context::active_turn_execution_context();
    Box::pin(async move {
        let context = context.ok_or("image view hydration requires an active turn")?;
        let session_id = context.session_id().to_string();
        if request
            .tool_output
            .get("session_id")
            .and_then(serde_json::Value::as_str)
            != Some(session_id.as_str())
        {
            return Err("image view receipt is outside the active session".into());
        }
        let receipt: ImageViewReceipt = serde_json::from_value(
            request
                .tool_output
                .get("image")
                .cloned()
                .ok_or("missing image receipt")?,
        )
        .map_err(|error| error.to_string())?;
        crate::media_vision::run_image_operation(receipt.byte_size, move || {
            let bytes =
                load_snapshot(&session_id, &receipt).map_err(medousa_forge::ForgeError::Store)?;
            Ok(Some(HydratedToolObservation {
                tool_name: request.tool_name,
                source_call_id: request.source_call_id,
                artifact_id: receipt.artifact_id,
                content_type: receipt.mime,
                bytes,
                sha256: receipt.sha256,
                untrusted_content: true,
            }))
        })
        .await
        .map_err(|error| error.to_string())
    })
}

#[cfg(test)]
#[path = "image_view_tools/tests.rs"]
mod tests;
