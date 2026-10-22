use super::*;

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct VisualRecord {
    pub asset: CreativeAsset,
    pub revision: u64,
    pub fingerprint: String,
    pub selected_ids: Vec<String>,
}

impl CreativeStore {
    /// Freeze a selected frame and its matching visual for the existing Code turn path.
    pub fn code_context(
        &self,
        id: &str,
        request: &CreativeCodeContextRequest,
    ) -> Result<(CreativeSelectionContext, CreativeAsset)> {
        validate_id(&request.session_id)?;
        let _lock = self.lock(id)?;
        let record = self.read(id)?;
        Self::check_revision(&record, request.expected_revision)?;
        let frame = record
            .document
            .scene
            .nodes
            .iter()
            .find(|n| {
                n.id == request.frame_id
                    && n.visible
                    && matches!(n.content, CreativeContent::Frame { .. })
            })
            .ok_or_else(|| invalid("select a visible frame to build"))?;
        let context = Self::selection_context(
            &record.document,
            CreativeContextRequest {
                selected_ids: vec![frame.id.clone()],
            },
        )?;
        let visual = record
            .visuals
            .get(&request.visual_asset_id)
            .ok_or_else(|| invalid("capture the frame again"))?;
        if visual.revision != request.expected_revision
            || visual.selected_ids != vec![frame.id.clone()]
            || visual.fingerprint != context.fingerprint
        {
            return Err(invalid("frame preview does not match the design revision"));
        }
        Ok((context, visual.asset.clone()))
    }

    /// A context image is staged separately from editable assets and undo history.
    pub fn import_visual_context(
        &self,
        id: &str,
        request: CreativeVisualContextRequest,
        bytes: &[u8],
    ) -> Result<CreativeAsset> {
        let _lock = self.lock(id)?;
        let mut record = self.read(id)?;
        Self::check_revision(&record, request.expected_revision)?;
        let context = Self::selection_context(
            &record.document,
            CreativeContextRequest {
                selected_ids: request.selected_ids.clone(),
            },
        )?;
        let asset = Self::bitmap_asset(bytes)?;
        if asset.mime != "image/png" || bytes.len() > 4 * 1024 * 1024 {
            return Err(invalid("selection preview must be a PNG up to 4 MiB"));
        }
        // A preview from a previous saved revision can never be admitted again.
        record
            .visuals
            .retain(|_, visual| visual.revision == request.expected_revision);
        if record.visuals.len() >= 32 && !record.visuals.contains_key(&asset.id) {
            return Err(invalid("too many unsubmitted selection previews"));
        }
        self.publish_asset(id, &asset, bytes)?;
        record.visuals.insert(
            asset.id.clone(),
            VisualRecord {
                asset: asset.clone(),
                revision: request.expected_revision,
                fingerprint: context.fingerprint,
                selected_ids: request.selected_ids,
            },
        );
        self.write(&record)?;
        Ok(asset)
    }
}
