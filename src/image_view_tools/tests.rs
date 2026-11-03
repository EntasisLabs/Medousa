use super::*;
use crate::agent_runtime::execution_context::{TurnExecutionContext, with_turn_execution_context};
use medousa_runtime::ToolObservationHydrationPort as _;
use serde_json::json;
use stasis::application::orchestration::tool_registry::{InMemoryToolRegistry, ToolRegistry};
use std::sync::Arc;

fn png(width: u32, height: u32) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    image::DynamicImage::new_rgba8(width, height)
        .write_to(&mut out, image::ImageFormat::Png)
        .unwrap();
    out.into_inner()
}

fn context(session: &str, vision: bool) -> Arc<TurnExecutionContext> {
    use crate::request_principal::{RequestPrincipal, TransportClass};
    let scope = crate::turn_scope::TurnContinuationScope {
        turn_correlation_id: "image-view-turn".into(),
        session_id: session.into(),
        identity_user_id: None,
        original_prompt: "inspect a generated image".into(),
        delivery_target: None,
        provider: "openai".into(),
        model: if vision { "gpt-4o" } else { "text-only" }.into(),
        response_depth_mode: "standard".into(),
        supports_ui_artifacts: false,
        supports_liquid_markdown: false,
        supports_browser_host: false,
        browser_driver_id: None,
        selected_worlds: vec![],
        channel_surface: None,
    };
    Arc::new(
        TurnExecutionContext::from_scope(
            "image-view-turn",
            RequestPrincipal::anonymous(TransportClass::Loopback),
            tokio_util::sync::CancellationToken::new(),
            std::time::Instant::now() + std::time::Duration::from_secs(60),
            scope,
        )
        .unwrap(),
    )
}

#[test]
fn confined_reader_rejects_escapes_empty_oversized_and_directories() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let bytes = png(2, 3);
    std::fs::write(root.join("image.png"), &bytes).unwrap();
    assert_eq!(read_image_file(&root, "image.png").unwrap(), bytes);
    std::fs::write(root.join("résultat 图.png"), &bytes).unwrap();
    assert_eq!(read_image_file(&root, "résultat 图.png").unwrap(), bytes);
    assert_eq!(
        read_image_file(&root, root.join("image.png").to_str().unwrap()).unwrap(),
        bytes
    );
    for path in [
        "../image.png",
        "/etc/passwd",
        ".",
        "",
        "https://example.com/image.png",
        "missing",
    ] {
        assert!(read_image_file(&root, path).is_err(), "{path}");
    }
    std::fs::write(root.join("empty"), []).unwrap();
    assert!(read_image_file(&root, "empty").is_err());
    let file = std::fs::File::create(root.join("large")).unwrap();
    file.set_len(MAX_SOURCE_BYTES + 1).unwrap();
    assert!(read_image_file(&root, "large").is_err());
    std::fs::create_dir(root.join("folder")).unwrap();
    assert!(read_image_file(&root, "folder").is_err());
}

#[cfg(unix)]
#[test]
fn confined_reader_rejects_links_and_fifo() {
    use std::os::unix::{ffi::OsStrExt, fs::symlink};
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("image.png"), png(2, 3)).unwrap();
    symlink(outside.path().join("image.png"), root.join("link")).unwrap();
    symlink(outside.path(), root.join("directory-link")).unwrap();
    std::fs::hard_link(outside.path().join("image.png"), root.join("hard-link")).unwrap();
    for path in ["link", "directory-link/image.png", "hard-link"] {
        assert!(read_image_file(&root, path).is_err(), "{path}");
    }
    let fifo = root.join("fifo");
    let name = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    assert!(read_image_file(&root, "fifo").is_err());
    let capability = medousa_store::StoreRoot::open_nofollow(&root).unwrap();
    assert!(
        capability
            .open_regular_read_file(&medousa_store::StorePath::parse("fifo").unwrap())
            .is_err()
    );
}

#[test]
fn validates_formats_and_caps_dimensions() {
    assert!(render_image(b"not an image".to_vec()).is_err());
    assert!(render_image(b"<svg></svg>".to_vec()).is_err());
    assert!(render_image(b"\x89PNG\r\n\x1a\ntruncated".to_vec()).is_err());
    let (bytes, width, height) = render_image(png(5000, 2)).unwrap();
    assert_eq!((width, height), (4096, 2));
    assert!(bytes.len() as u64 <= crate::media_vision::MAX_MODEL_IMAGE_BYTES);
    for format in [
        image::ImageFormat::Jpeg,
        image::ImageFormat::WebP,
        image::ImageFormat::Gif,
    ] {
        let mut source = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(4, 5)
            .write_to(&mut source, format)
            .unwrap();
        let (bytes, w, h) = render_image(source.into_inner()).unwrap();
        assert!(bytes.starts_with(b"\x89PNG"));
        assert_eq!((w, h), (4, 5));
    }
}

#[test]
fn decode_budget_rejects_huge_dimensions() {
    assert!(render_image(png(32_769, 1)).is_err());
    let huge = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(1, 1, vec![0; 4]).unwrap();
    let mut bytes = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(huge)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    let mut bytes = bytes.into_inner();
    // Corrupting the header is rejected before allocating a large canvas.
    bytes[16..20].copy_from_slice(&100_000u32.to_be_bytes());
    assert!(render_image(bytes).is_err());
}

#[test]
fn snapshot_is_session_bound_and_digest_verified() {
    let session = format!("image-view-snapshot-{}", uuid::Uuid::new_v4());
    let bytes = png(2, 3);
    let receipt = snapshot_image(&session, &bytes, 2, 3).unwrap();
    assert_eq!(load_snapshot(&session, &receipt).unwrap(), bytes);
    assert!(load_snapshot("other-session", &receipt).is_err());
    let mut tampered = receipt.clone();
    tampered.sha256 = "0".repeat(64);
    assert!(load_snapshot(&session, &tampered).is_err());
    let mut tampered = receipt.clone();
    tampered.width = 3;
    assert!(load_snapshot(&session, &tampered).is_err());
    let mut tampered = receipt.clone();
    tampered.mime = "image/jpeg".into();
    assert!(load_snapshot(&session, &tampered).is_err());
    let mut tampered = receipt.clone();
    tampered.byte_size += 1;
    assert!(load_snapshot(&session, &tampered).is_err());
    let foreign = crate::artifact_store::persist_binary_artifact(
        &session,
        "cognition_browser_snapshot",
        "output",
        "image/png",
        None,
        &bytes,
    )
    .unwrap();
    let mut tampered = receipt;
    tampered.artifact_id = foreign.artifact_id;
    assert!(load_snapshot(&session, &tampered).is_err());
}

#[tokio::test]
async fn file_to_native_tool_to_hydrated_pixels_survives_source_deletion() {
    let session = format!("image-view-integration-{}", uuid::Uuid::new_v4());
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let source = png(6, 7);
    std::fs::write(root.join("terminal-output.png"), &source).unwrap();
    with_turn_execution_context(context(&session, true), async {
        crate::coding_tools::with_coder_tool_root(root.clone(), async {
            let mut registry = InMemoryToolRegistry::default();
            register_image_view_tool(&mut registry).unwrap();
            let output = registry.invoke_tool(COGNITION_IMAGE_VIEW, json!({"path":"terminal-output.png"})).await.unwrap();
            assert_eq!(output["image"]["width"], 6);
            assert_eq!(output["vision_available"], true);
            std::fs::remove_file(root.join("terminal-output.png")).unwrap();
            let hydrated = crate::agent_runtime::tool_observation_hydration::DaemonToolObservationHydrationPort::new(&session)
                .hydrate(ToolObservationHydrationRequest { tool_name: COGNITION_IMAGE_VIEW.into(),
                    source_call_id: "image-call".into(), tool_output: output.clone() }).await.unwrap().unwrap();
            assert_eq!(hydrated.bytes, render_image(source).unwrap().0);
            assert_eq!(hydrated.source_call_id, "image-call");
            assert!(hydrated.untrusted_content);
            let mut bad = output.clone();
            bad["session_id"] = json!("other-session");
            assert!(hydrate(ToolObservationHydrationRequest { tool_name: COGNITION_IMAGE_VIEW.into(),
                source_call_id: "bad-call".into(), tool_output: bad }).await.is_err());
            assert!(registry.invoke_tool(COGNITION_IMAGE_VIEW, json!({"path":"file.png","root":"/"})).await.is_err());
        }).await;
    }).await;
}

#[tokio::test]
async fn tool_requires_active_turn_and_strict_input() {
    let mut registry = InMemoryToolRegistry::default();
    register_image_view_tool(&mut registry).unwrap();
    assert!(
        registry
            .invoke_tool(COGNITION_IMAGE_VIEW, json!({"path":"x.png"}))
            .await
            .is_err()
    );
    assert!(
        registry
            .invoke_tool(
                COGNITION_IMAGE_VIEW,
                json!({"path":"x.png", "session_id":"forged"})
            )
            .await
            .is_err()
    );
    assert!(
        hydrate(ToolObservationHydrationRequest {
            tool_name: COGNITION_IMAGE_VIEW.into(),
            source_call_id: "call".into(),
            tool_output: json!({})
        })
        .await
        .is_err()
    );
}

#[tokio::test]
async fn bound_environment_is_rejected_without_host_fallback() {
    let binding = medousa_runtime::WorkEnvironmentBinding {
        port: Arc::new(medousa_runtime::InMemoryWorkEnvironmentPort::new()),
        handle: medousa_runtime::WorkEnvironmentHandle::new_local(
            medousa_runtime::WorkEnvironmentId::parse("image-test-env").unwrap(),
            "test-token",
        ),
        fence: medousa_runtime::WorkEnvironmentFence {
            stasis_attempt: stasis::domain::runtime::resource_lease::FencingToken(1),
            forge_environment_generation: None,
            forge_execution_generation: None,
        },
    };
    let context = Arc::new(
        Arc::try_unwrap(context("image-view-environment", true))
            .unwrap()
            .with_work_environment(binding),
    );
    with_turn_execution_context(context, async {
        let error = CognitionImageViewTool
            .invoke_typed(ImageViewInput {
                path: "should-not-open.png".into(),
                root: CompatOption::default(),
            })
            .await
            .unwrap_err();
        assert!(error.to_string().contains("daemon-local"));
    })
    .await;
}

#[test]
fn catalogs_expose_host_and_coder_without_remote_or_mobile_expansion() {
    use crate::agent_runtime::turn_worker::host_bus_tool_names;
    use crate::agent_runtime::turn_worker::{
        mobile_foreground_tool_ceiling, remote_delegated_tool_ceiling,
    };
    assert!(host_bus_tool_names().contains(COGNITION_IMAGE_VIEW));
    assert!(!mobile_foreground_tool_ceiling().contains(COGNITION_IMAGE_VIEW));
    assert!(!remote_delegated_tool_ceiling().contains(COGNITION_IMAGE_VIEW));
    assert!(
        crate::tool_bootstrap::host_tool_domain_catalog()
            .iter()
            .any(|domain| domain.domain == "media" && domain.tools.contains(&COGNITION_IMAGE_VIEW))
    );
}
