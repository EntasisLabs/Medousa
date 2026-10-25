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
    /// Propose document edits against a revision. selected_ids is context only. Does not modify the canvas.
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
                create_project(store, self)
            })
            .await
            .map_err(|(_, e)| StasisError::PortFailure(e))?;
        Ok(response.0)
    }
}

fn create_project(
    store: &medousa_create::CreativeStore,
    request: CreativeCreate,
) -> medousa_create::Result<Value> {
    use medousa_types::{CreativeContent, CreativeNode, CreativeScene, CreativeTransform};
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
            width: request.width.unwrap_or(1280.),
            height: request.height.unwrap_or(800.),
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
    let document = store.create(medousa_types::CreativeCreateRequest {
        id: request.id,
        title: request.title,
        scene: CreativeScene {
            nodes: vec![frame],
            ..Default::default()
        },
    })?;
    Ok(serde_json::json!({
        "document_id": document.id, "title": document.title, "revision": document.revision, "selected_ids": ["frame"],
        "proposal_template": {
            "document_id": document.id,
            "proposal": { "id": "initial-design", "expected_revision": document.revision, "selected_ids": [], "label": "Build editable design", "operations": [] }
        },
        "next": "Fetch cognition_schema types=[creative.propose], fill proposal_template.proposal.operations, and call cognition_store_write action=creative.propose. Recreate UI screenshots/wireframes as separate editable text, shapes, groups, and frames. Use local coordinates within each parent; top-level objects and project-wide edits are supported. Name layers by their UI role. Match layout, typography, colors, and bounds. Image nodes are for actual photo/illustration assets, never the flattened wireframe. Return open_url for preview/apply; do not apply it yourself.",
        "open_url": format!("medousa://create/{}", document.id)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn screenshot_wireframe_is_an_editable_native_proposal_with_preview_apply_and_undo() {
        let dir = tempfile::tempdir().unwrap();
        let store =
            medousa_create::CreativeStore::open(&dir.path().canonicalize().unwrap()).unwrap();
        let create = || CreativeCreate {
            id: "screenshot-ui".into(),
            title: "UI wireframe".into(),
            width: Some(1024.),
            height: Some(768.),
        };
        let output = create_project(&store, create()).unwrap();
        assert_eq!(
            output,
            create_project(&store, create()).unwrap(),
            "exact retries retain the proposal identity"
        );
        let mut template = output["proposal_template"].clone();
        let node = |id: &str,
                    parent: &str,
                    x: f64,
                    y: f64,
                    width: f64,
                    height: f64,
                    content: Value| {
            json!({
                "op": "put_node", "node": { "id": id, "name": id, "parent_id": parent,
                    "transform": { "x": x, "y": y, "width": width, "height": height, "rotation": 0. },
                    "opacity": 1., "visible": true, "locked": false, "content": content }
            })
        };
        template["proposal"]["operations"] = json!([
            node(
                "card",
                "frame",
                64.,
                80.,
                600.,
                240.,
                json!({"kind": "group"})
            ),
            node(
                "card-background",
                "card",
                0.,
                0.,
                600.,
                240.,
                json!({"kind": "shape", "shape": "rectangle", "fill": "#f4f4f5", "stroke": "#d4d4d8", "stroke_width": 1.})
            ),
            node(
                "heading",
                "card",
                24.,
                24.,
                520.,
                52.,
                json!({"kind": "text", "text": "Welcome back", "color": "#18181b", "font_family": "Inter", "font_size": 28., "font_weight": 600})
            ),
            node(
                "button",
                "card",
                24.,
                150.,
                160.,
                44.,
                json!({"kind": "shape", "shape": "rectangle", "fill": "#4f46e5", "stroke": "#4f46e5", "stroke_width": 0.})
            ),
            node(
                "button-label",
                "card",
                40.,
                160.,
                128.,
                24.,
                json!({"kind": "text", "text": "Continue", "color": "#ffffff", "font_family": "Inter", "font_size": 16.})
            ),
            {"op": "set_background", "color": "#e4e4e7"},
            {"op": "rename", "title": "Editable UI design"},
            {"op": "reorder", "ids": ["frame", "card", "card-background", "heading", "button", "button-label"]}
        ]);
        // This is the same JSON contract consumed by the model-facing creative.propose action.
        let proposal: CreativePropose = serde_json::from_value(template).unwrap();
        assert_eq!(proposal.document_id, "screenshot-ui");
        let suggested = store
            .propose(&proposal.document_id, proposal.proposal)
            .unwrap();
        assert_eq!(
            store
                .get("screenshot-ui")
                .unwrap()
                .document
                .scene
                .nodes
                .len(),
            1,
            "preview leaves the saved frame untouched"
        );
        let applied = store
            .decide(
                "screenshot-ui",
                &suggested.request.id,
                medousa_types::CreativeProposalDecision {
                    expected_revision: 0,
                    accept: true,
                },
            )
            .unwrap();
        assert_eq!(applied.document.scene.nodes.len(), 6);
        assert!(applied.document.assets.is_empty());
        assert!(
            !applied
                .document
                .scene
                .nodes
                .iter()
                .any(|n| matches!(n.content, medousa_types::CreativeContent::Image { .. }))
        );
        let mut heading = applied
            .document
            .scene
            .nodes
            .iter()
            .find(|n| n.id == "heading")
            .unwrap()
            .clone();
        assert_eq!(heading.parent_id.as_deref(), Some("card"));
        assert!(
            matches!(&heading.content, medousa_types::CreativeContent::Text { text, .. } if text == "Welcome back")
        );
        heading.transform.x += 30.;
        if let medousa_types::CreativeContent::Text { text, .. } = &mut heading.content {
            *text = "My revised heading".into();
        }
        let edited = store
            .transact(
                "screenshot-ui",
                medousa_types::CreativeTransaction {
                    id: "edit-heading".into(),
                    expected_revision: applied.document.revision,
                    label: "Edit heading".into(),
                    operations: vec![medousa_types::CreativeOperation::PutNode {
                        node: Box::new(heading.clone()),
                    }],
                },
            )
            .unwrap();
        assert!(edited.document.scene.nodes.contains(&heading));
        let undone = store
            .history(
                "screenshot-ui",
                medousa_types::CreativeHistoryRequest {
                    id: "undo-text-edit".into(),
                    expected_revision: edited.document.revision,
                    redo: false,
                },
            )
            .unwrap();
        assert_ne!(
            undone
                .document
                .scene
                .nodes
                .iter()
                .find(|n| n.id == "heading")
                .unwrap(),
            &heading
        );
        let undone = store
            .history(
                "screenshot-ui",
                medousa_types::CreativeHistoryRequest {
                    id: "undo-wireframe".into(),
                    expected_revision: undone.document.revision,
                    redo: false,
                },
            )
            .unwrap();
        assert_eq!(undone.document.scene.nodes.len(), 1);
        let redone = store
            .history(
                "screenshot-ui",
                medousa_types::CreativeHistoryRequest {
                    id: "redo-wireframe".into(),
                    expected_revision: undone.document.revision,
                    redo: true,
                },
            )
            .unwrap();
        assert_eq!(redone.document.scene, applied.document.scene);
    }
}
