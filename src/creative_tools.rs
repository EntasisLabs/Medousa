//! Creative actions extend the existing store tools. Model output is a proposal;
//! accepting it remains an explicit client action through the shared transaction path.
use medousa_types::{CreativeContextRequest, CreativeProposalRequest};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use stasis::prelude::{Result, StasisError};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreativeRead {
    pub document_id: String,
    /// Explicit objects to capture. Read a bounded structured selection, not bitmap bytes.
    pub selected_ids: Vec<String>,
}
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreativePropose {
    pub document_id: String,
    /// Use the captured revision/IDs from creative.read. Does not modify the canvas.
    pub proposal: CreativeProposalRequest,
}
impl CreativeRead {
    pub async fn execute(self) -> Result<Value> {
        let response =
            crate::creative_handlers::run(crate::creative_handlers::service()?, move |store| {
                store.context(
                    &self.document_id,
                    CreativeContextRequest {
                        selected_ids: self.selected_ids,
                    },
                )
            })
            .await
            .map_err(|(_, e)| StasisError::PortFailure(e))?;
        serde_json::to_value(response.0).map_err(|e| StasisError::PortFailure(e.to_string()))
    }
}
impl CreativePropose {
    pub async fn execute(self) -> Result<Value> {
        let response =
            crate::creative_handlers::run(crate::creative_handlers::service()?, move |store| {
                store.propose(&self.document_id, self.proposal)
            })
            .await
            .map_err(|(_, e)| StasisError::PortFailure(e))?;
        serde_json::to_value(response.0).map_err(|e| StasisError::PortFailure(e.to_string()))
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreativePromoteMedia {
    pub document_id: String,
    /// Image ID returned by the existing image-generation/upload flow in this conversation.
    pub media_id: String,
}
impl CreativePromoteMedia {
    pub async fn execute(self, session_id: String) -> Result<Value> {
        let response =
            crate::creative_handlers::run(crate::creative_handlers::service()?, move |store| {
                crate::creative_handlers::promote_media_into_project(
                    store,
                    &self.document_id,
                    &medousa_types::CreativePromoteMediaRequest {
                        session_id,
                        media_id: self.media_id,
                    },
                )
            })
            .await
            .map_err(|(_, e)| StasisError::PortFailure(e))?;
        serde_json::to_value(response.0).map_err(|e| StasisError::PortFailure(e.to_string()))
    }
}
