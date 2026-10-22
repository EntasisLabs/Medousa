use super::*;

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct VisualRecord {
    pub asset: CreativeAsset,
    pub revision: u64,
    pub fingerprint: String,
    pub selected_ids: Vec<String>,
}

impl CreativeStore {
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
