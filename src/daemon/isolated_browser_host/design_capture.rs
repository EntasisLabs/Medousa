//! Read-only visual projection of an owned browser, fenced with its pixel capture.
use super::*;

impl IsolatedBrowserHost {
    pub async fn capture_design(
        &self,
        owner: &str,
        world_id: &str,
    ) -> Result<(Value, BrowserObservation, BrowserScreenshotCapture), IsolatedBrowserError> {
        self.capture_design_mode(owner, world_id, false).await
    }

    async fn capture_design_mode(
        &self,
        owner: &str,
        world_id: &str,
        omit_pinned: bool,
    ) -> Result<(Value, BrowserObservation, BrowserScreenshotCapture), IsolatedBrowserError> {
        let observation = self.observe(owner, world_id, None, 1024).await?;
        let (world, websocket_url, target_id) =
            self.runtime_binding(owner, world_id, false).await?;
        let tab_id = world
            .tab_id
            .as_deref()
            .ok_or_else(|| IsolatedBrowserError::Conflict("browser has no page".into()))?;
        if tab_id != observation.tab_id {
            return Err(IsolatedBrowserError::Conflict(
                "browser page changed".into(),
            ));
        }
        let (mut connection, session_id) = connect_page(&websocket_url, &target_id).await?;
        let context = isolated_execution_context(&mut connection, &session_id).await?;
        let capture = evaluate(
            &mut connection,
            &session_id,
            Some(context),
            include_str!("design_capture.js").replace(
                "const omitPinned = false;",
                if omit_pinned {
                    "const omitPinned = true;"
                } else {
                    "const omitPinned = false;"
                },
            ),
        )
        .await?;
        let viewport = &capture["viewport"];
        if capture["url"].as_str() != Some(observation.url.as_str())
            || viewport["width"]
                .as_u64()
                .is_none_or(|v| v == 0 || v > 4096)
            || viewport["height"]
                .as_u64()
                .is_none_or(|v| v == 0 || v > 4096)
            || serde_json::to_vec(&capture)
                .map_err(|e| IsolatedBrowserError::Driver(e.to_string()))?
                .len()
                > 512 * 1024
        {
            return Err(IsolatedBrowserError::Invalid(
                "page capture exceeds its viewport or data budget".into(),
            ));
        }
        let screenshot = self
            .screenshot(
                owner,
                world_id,
                &observation.document_id,
                observation.revision,
                1600,
            )
            .await?;
        let after = evaluate(
            &mut connection,
            &session_id,
            Some(context),
            include_str!("design_capture.js").replace(
                "const omitPinned = false;",
                if omit_pinned {
                    "const omitPinned = true;"
                } else {
                    "const omitPinned = false;"
                },
            ),
        )
        .await?;
        let current = self.observe(owner, world_id, None, 1024).await?;
        if capture != after
            || current.document_id != observation.document_id
            || current.revision != observation.revision
            || current.viewport != observation.viewport
        {
            return Err(IsolatedBrowserError::Conflict(
                "The page changed during capture. Let it settle and try again.".into(),
            ));
        }
        Ok((capture, observation, screenshot))
    }
}

pub(super) type DesignSlice = (
    Value,
    BrowserObservation,
    BrowserScreenshotCapture,
    u32,
    u32,
    u32,
);
static PAGE_CAPTURE: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();

impl IsolatedBrowserHost {
    /// Finish restoration even if the HTTP client disconnects during a capture.
    pub async fn capture_page_design(
        self: &Arc<Self>,
        owner: &str,
        world_id: &str,
    ) -> Result<Vec<DesignSlice>, IsolatedBrowserError> {
        let permit = PAGE_CAPTURE
            .get_or_init(|| Arc::new(tokio::sync::Semaphore::new(1)))
            .clone()
            .try_acquire_owned()
            .map_err(|_| {
                IsolatedBrowserError::Conflict(
                    "Another full-page capture is running. Try again shortly.".into(),
                )
            })?;
        // Owned permit/task lifetime outlives the request; no user input or file work is replayed.
        let host = self.clone();
        let owner = owner.to_owned();
        let world_id = world_id.to_owned();
        tokio::spawn(async move {
            let _permit = permit;
            host.capture_page_inner(&owner, &world_id).await
        })
        .await
        .map_err(|e| IsolatedBrowserError::Driver(e.to_string()))?
    }

    async fn capture_page_inner(
        &self,
        owner: &str,
        world_id: &str,
    ) -> Result<Vec<DesignSlice>, IsolatedBrowserError> {
        let before = self.observe(owner, world_id, None, 1024).await?;
        let (_, websocket, target) = self.runtime_binding(owner, world_id, false).await?;
        let (mut connection, session) = connect_page(&websocket, &target).await?;
        let context = isolated_execution_context(&mut connection, &session).await?;
        let result = tokio::time::timeout(Duration::from_secs(30), async {
            let mut slices = Vec::new(); let mut page_y = 0_u32; let mut bytes = 0; let mut regions = 0; let mut nodes = 0;
            loop {
                let script = format!(r#"(async () => {{
                  await Promise.race([document.fonts.ready, new Promise(r => setTimeout(r, 1500))]);
                  window.scrollTo({{ left: 0, top: {page_y}, behavior: 'instant' }});
                  await new Promise(r => setTimeout(r, 120));
                  await Promise.race([Promise.all([...document.images].filter(i => {{const b=i.getBoundingClientRect();return b.bottom>0&&b.top<innerHeight;}}).map(i => i.decode().catch(()=>{{}}))), new Promise(r => setTimeout(r, 1200))]);
                  return {{ height: Math.max(document.documentElement.scrollHeight, document.body?.scrollHeight || 0), y: Math.round(scrollY), viewport: innerHeight }};
                }})()"#);
                let position = evaluate(&mut connection, &session, Some(context), script).await?;
                let full_height = position["height"].as_u64().unwrap_or(0) as u32;
                let viewport = position["viewport"].as_u64().unwrap_or(0) as u32;
                let actual = position["y"].as_u64().unwrap_or(0) as u32;
                if viewport == 0 || full_height > viewport.saturating_mul(12) || actual > page_y || page_y - actual >= viewport {
                    return Err(IsolatedBrowserError::Invalid("Full-page capture supports up to 12 screenfuls and a vertically scrolling page.".into()));
                }
                let (raw, observation, screenshot) = self.capture_design_mode(owner, world_id, page_y > 0).await?;
                if observation.document_id != before.document_id || observation.viewport.width != before.viewport.width || observation.viewport.height != before.viewport.height || observation.viewport.scroll_y != i64::from(actual) {
                    return Err(IsolatedBrowserError::Conflict("The page changed during capture. Try again once it settles.".into()));
                }
                bytes += screenshot.byte_size;
                regions += raw["raster_regions"].as_array().map_or(0, Vec::len);
                nodes += raw["scene"]["nodes"].as_array().map_or(0,Vec::len);
                if bytes > 24 * 1024 * 1024 || regions > 20 || nodes > 9500 {
                    return Err(IsolatedBrowserError::Invalid("This page exceeds the full-page image budget. Import a viewport instead.".into()));
                }
                let clip_top = page_y - actual;
                let height = viewport.min(full_height.saturating_sub(page_y));
                slices.push((raw, observation, screenshot, clip_top, height, page_y));
                page_y = page_y.saturating_add(height);
                if page_y >= full_height { break; }
                if slices.len() == 12 || height == 0 { return Err(IsolatedBrowserError::Invalid("The page kept growing during capture. Import a viewport instead.".into())); }
            }
            Ok(slices)
        }).await.unwrap_or_else(|_| Err(IsolatedBrowserError::Conflict("Full-page capture timed out; no design was published.".into())));
        // Never move a different document after a navigation.
        if let Ok(current) = self.observe(owner, world_id, None, 1024).await
            && current.document_id == before.document_id
        {
            let restore = format!(
                "window.scrollTo({{left:{},top:{},behavior:'instant'}}); true",
                before.viewport.scroll_x, before.viewport.scroll_y
            );
            evaluate(&mut connection, &session, Some(context), restore).await?;
        }
        result
    }
}
