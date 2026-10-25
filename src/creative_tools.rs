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
    #[serde(default)]
    pub selected_ids: Vec<String>,
    /// Frozen context pointer returned by a Create -> Code handoff.
    #[serde(default)]
    pub snapshot_id: Option<String>,
}
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreativePropose {
    pub document_id: String,
    /// Use the captured revision/IDs from creative.read. Does not modify the canvas.
    pub proposal: CreativeProposalRequest,
}
impl CreativeRead {
    pub async fn execute(self) -> Result<Value> {
        let job = crate::creative_jobs::bound_job(Some(&self.document_id)).await?;
        let session_id = crate::agent_runtime::execution_context::active_turn_execution_context()
            .map(|context| context.session_id().to_string());
        let response =
            crate::creative_handlers::run(crate::creative_handlers::service()?, move |store| {
                if let Some(job) = job {
                    let context =
                        store.job_context(&self.document_id, &job.job_id, &job.turn_id)?;
                    let captured = store.job(&self.document_id, &job.job_id, &job.owner)?;
                    if !self.selected_ids.is_empty()
                        && self.selected_ids != captured.request.selected_ids
                        && self.selected_ids != context.selected_ids
                    {
                        return Err(medousa_create::CreativeError::Invalid(
                            "read must use the admitted selection".into(),
                        ));
                    }
                    let mut value = serde_json::to_value(context).map_err(|e| medousa_create::CreativeError::Storage(e.to_string()))?;
                    value["proposal_binding"] = serde_json::json!({ "id": captured.request.id, "expected_revision": captured.context.revision, "selected_ids": captured.request.selected_ids });
                    return Ok(value);
                }
                if let Some(snapshot) = self.snapshot_id {
                    let context = store.read_context_snapshot(
                        &snapshot,
                        &self.document_id,
                        session_id.as_deref().ok_or_else(|| {
                            medousa_create::CreativeError::Invalid(
                                "design snapshots require an active conversation".into(),
                            )
                        })?,
                    )?;
                    return serde_json::to_value(context).map_err(|e| medousa_create::CreativeError::Storage(e.to_string()));
                }
                let selected_ids = if self.selected_ids.is_empty() {
                    store
                        .get(&self.document_id)?
                        .document
                        .scene
                        .nodes
                        .into_iter()
                        .filter(|node| node.visible && node.parent_id.is_none())
                        .map(|node| node.id)
                        .collect()
                } else {
                    self.selected_ids
                };
                let context = store.context(&self.document_id, CreativeContextRequest { selected_ids })?;
                serde_json::to_value(context).map_err(|e| medousa_create::CreativeError::Storage(e.to_string()))
            })
            .await
            .map_err(|(_, e)| StasisError::PortFailure(e))?;
        Ok(response.0)
    }
}
impl CreativePropose {
    pub async fn execute(self) -> Result<Value> {
        let job = crate::creative_jobs::bound_job(Some(&self.document_id)).await?;
        let document_id = self.document_id.clone();
        let response =
            crate::creative_handlers::run(crate::creative_handlers::service()?, move |store| {
                let value = match job {
                    Some(job) => serde_json::to_value(store.complete_job(
                        &self.document_id,
                        &job.job_id,
                        &job.turn_id,
                        self.proposal,
                    )?),
                    None => serde_json::to_value(store.propose(&self.document_id, self.proposal)?),
                };
                value.map_err(|e| medousa_create::CreativeError::Storage(e.to_string()))
            })
            .await
            .map_err(|(_, e)| StasisError::PortFailure(e))?;
        let mut value = serde_json::to_value(response.0)
            .map_err(|e| StasisError::PortFailure(e.to_string()))?;
        value["open_url"] = Value::String(format!("medousa://create/{document_id}"));
        Ok(value)
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
        let job = crate::creative_jobs::bound_job(Some(&self.document_id)).await?;
        let response =
            crate::creative_handlers::run(crate::creative_handlers::service()?, move |store| {
                let request = medousa_types::CreativePromoteMediaRequest {
                    session_id,
                    media_id: self.media_id,
                };
                match job {
                    Some(job) => {
                        if request.session_id != job.session_id {
                            return Err(medousa_create::CreativeError::Invalid(
                                "media must belong to the creative job session".into(),
                            ));
                        }
                        crate::creative_handlers::promote_media_for_job(
                            store,
                            &self.document_id,
                            &request,
                            &job,
                        )
                    }
                    None => crate::creative_handlers::promote_media_into_project(
                        store,
                        &self.document_id,
                        &request,
                    ),
                }
            })
            .await
            .map_err(|(_, e)| StasisError::PortFailure(e))?;
        serde_json::to_value(response.0).map_err(|e| StasisError::PortFailure(e.to_string()))
    }
}

/// Create a new editable project. The store owns validation and exact retry identity.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreativeCreate {
    /// New stable id; reuse only for an exact retry. Never overwrites a project.
    pub id: String,
    pub title: String,
    /// Initial frame width in canvas units; defaults to 1280.
    #[serde(default)]
    pub width: Option<f64>,
    /// Initial frame height in canvas units; defaults to 800.
    #[serde(default)]
    pub height: Option<f64>,
}
impl CreativeCreate {
    pub async fn execute(self) -> Result<Value> {
        if crate::agent_runtime::execution_context::active_turn_execution_context().is_none() {
            return Err(StasisError::PortFailure(
                "creating a project requires an active turn".into(),
            ));
        }
        if crate::creative_jobs::in_creative_turn() {
            return Err(StasisError::PortFailure(
                "selection jobs cannot create unrelated projects".into(),
            ));
        }
        let response =
            crate::creative_handlers::run(crate::creative_handlers::service()?, move |store| {
                use medousa_types::{
                    CreativeContent, CreativeNode, CreativeScene, CreativeTransform,
                };
                let frame = CreativeNode {
                    id: "frame".into(),
                    name: None,
                    parent_id: None,
                    appearance: None,
                    design_source: None,
                    text_layout: None,
                    transform: CreativeTransform {
                        x: 0.,
                        y: 0.,
                        width: self.width.unwrap_or(1280.),
                        height: self.height.unwrap_or(800.),
                        rotation: 0.,
                    },
                    opacity: 1.,
                    visible: true,
                    locked: false,
                    content: CreativeContent::Frame {
                        name: "Frame 1".into(),
                        background: "#ffffff".into(),
                    },
                };
                store.create(medousa_types::CreativeCreateRequest {
                    id: self.id,
                    title: self.title,
                    scene: CreativeScene {
                        nodes: vec![frame],
                        ..Default::default()
                    },
                })
            })
            .await
            .map_err(|(_, e)| StasisError::PortFailure(e))?;
        Ok(
            serde_json::json!({ "document_id": response.0.id, "title": response.0.title, "revision": response.0.revision, "selected_ids": ["frame"],
            "next": "Use creative.propose with this frame selection to generate editable objects for explicit preview/apply.",
            "open_url": format!("medousa://create/{}", response.0.id) }),
        )
    }
}
