//! Canonical model-facing tool for explicit daemon delegation.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::Deserialize;

use crate::delegation::{COGNITION_DELEGATE, DelegationService};
use crate::typed_tools::{ExternalJson, ToolId, ToolRegistration, medousa_tool};

const COGNITION_DELEGATE_ID: ToolId = ToolId::new(COGNITION_DELEGATE);

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DelegationRequest {
    /// Work to send to the explicitly bound Medousa workshop
    #[schemars(required, with = "String")]
    pub task: Option<String>,
}

pub struct CognitionDelegateTool {
    service: Arc<DelegationService>,
}

impl CognitionDelegateTool {
    pub fn new(service: Arc<DelegationService>) -> Self {
        Self { service }
    }
}

#[medousa_tool(id = COGNITION_DELEGATE_ID)]
impl CognitionDelegateTool {
    /// Send bounded work to the explicitly bound Medousa workshop.
    async fn invoke_typed(
        &self,
        request: DelegationRequest,
    ) -> stasis::prelude::Result<ExternalJson> {
        let task = request.task.as_deref().unwrap_or_default();
        self.service.delegate(task).await.map(ExternalJson::new)
    }
}

pub fn register_delegation_tools(
    registry: &mut impl ToolRegistration,
    service: Arc<DelegationService>,
) -> stasis::prelude::Result<()> {
    registry.register_typed_tool(CognitionDelegateTool::new(service))
}
