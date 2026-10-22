//! Read-only visual projection of an owned browser, fenced with its pixel capture.
use super::*;

impl IsolatedBrowserHost {
    pub async fn capture_design(
        &self,
        owner: &str,
        world_id: &str,
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
            include_str!("design_capture.js").into(),
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
            include_str!("design_capture.js").into(),
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
