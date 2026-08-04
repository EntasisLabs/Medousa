//! Least-authority Coder entry surface before a Forge project is bound.

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use genai::chat::Tool;
use medousa_forge::model::{WorkState, WorkTarget};
use serde_json::{Value, json};
use stasis::application::orchestration::tool_registry::{
    InMemoryToolRegistry, StasisTool, ToolRegistry,
};
use stasis::domain::errors::StasisError;
use stasis::prelude::Result;

use crate::daemon::state::AppState;
use crate::daemon_api::StartSessionCodeProjectRequest;

const PROJECT_LIST: &str = "cognition_project_list";
const PROJECT_BIND: &str = "cognition_project_bind";
const PROJECT_CREATE: &str = "cognition_project_create";
const TURN_CONTROL_TOOLS: &[&str] = &[
    "cognition_turn_update_user",
    "cognition_turn_checkpoint",
    "cognition_turn_finish",
    "cognition_turn_request_more_rounds",
    "cognition_turn_propose_mode",
];

#[derive(Clone)]
pub struct CoderSetupToolRegistry {
    inner: Arc<dyn ToolRegistry>,
    setup: Arc<dyn ToolRegistry>,
    allowed_inner: HashSet<String>,
}

impl CoderSetupToolRegistry {
    pub fn new(
        inner: Arc<dyn ToolRegistry>,
        state: AppState,
        session_id: impl Into<String>,
    ) -> Result<Self> {
        let session_id = session_id.into();
        let setup = InMemoryToolRegistry::default();
        setup.register_tool(CognitionProjectListTool {
            state: state.clone(),
        })?;
        setup.register_tool(CognitionProjectBindTool {
            state: state.clone(),
            session_id: session_id.clone(),
        })?;
        setup.register_tool(CognitionProjectCreateTool { state, session_id })?;
        Ok(Self {
            inner,
            setup: Arc::new(setup),
            allowed_inner: TURN_CONTROL_TOOLS
                .iter()
                .map(|name| (*name).to_string())
                .collect(),
        })
    }
}

#[async_trait]
impl ToolRegistry for CoderSetupToolRegistry {
    async fn list_tools(&self) -> Result<Vec<Tool>> {
        let mut tools = self
            .inner
            .list_tools()
            .await?
            .into_iter()
            .filter(|tool| self.allowed_inner.contains(tool.name.as_str()))
            .collect::<Vec<_>>();
        tools.extend(self.setup.list_tools().await?);
        Ok(tools)
    }

    async fn invoke_tool(&self, tool_name: &str, input: Value) -> Result<Value> {
        if matches!(tool_name, PROJECT_LIST | PROJECT_BIND | PROJECT_CREATE) {
            return self.setup.invoke_tool(tool_name, input).await;
        }
        if self.allowed_inner.contains(tool_name) {
            return self.inner.invoke_tool(tool_name, input).await;
        }
        Err(StasisError::PortFailure(format!(
            "tool is outside the unbound Coder setup contract: {tool_name}"
        )))
    }
}

struct CognitionProjectListTool {
    state: AppState,
}

#[async_trait]
impl StasisTool for CognitionProjectListTool {
    fn name(&self) -> &'static str {
        PROJECT_LIST
    }

    fn description(&self) -> Option<&'static str> {
        Some("List ready Forge projects that this Coder conversation can continue.")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({ "type": "object", "properties": {} }))
    }

    async fn invoke(&self, _input: Value) -> Result<Value> {
        let items = self
            .state
            .forge
            .list()
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        let projects = items
            .into_iter()
            .filter(|item| item.state == WorkState::Ready && item.environment.is_some())
            .take(20)
            .map(|item| {
                let WorkTarget::Git(target) = item.target;
                json!({
                    "work_id": item.id,
                    "title": item.title,
                    "brief": item.brief,
                    "repo_path": target.repo_path,
                    "base_ref": target.base_ref,
                })
            })
            .collect::<Vec<_>>();
        Ok(json!({ "ok": true, "projects": projects }))
    }
}

struct CognitionProjectBindTool {
    state: AppState,
    session_id: String,
}

#[async_trait]
impl StasisTool for CognitionProjectBindTool {
    fn name(&self) -> &'static str {
        PROJECT_BIND
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Bind this conversation to a ready Forge project selected by the user. The full Coder workspace becomes active on the next turn.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["work_id"],
            "properties": {
                "work_id": { "type": "string", "description": "Ready project id returned by cognition_project_list" }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> Result<Value> {
        let work_id = input
            .get("work_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StasisError::PortFailure("work_id is required".into()))?;
        let item = self
            .state
            .forge
            .load(&medousa_forge::model::WorkId::from(work_id.to_string()))
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        if item.state != WorkState::Ready || item.environment.is_none() {
            return Err(StasisError::PortFailure(
                "project must be ready with a governed worktree".into(),
            ));
        }
        crate::agent_mode_state::set_session_code_binding(&self.session_id, work_id)
            .map_err(StasisError::PortFailure)?;
        Ok(json!({
            "ok": true,
            "work_id": work_id,
            "title": item.title,
            "message": "Project bound. Full Coder tools become active on the next turn.",
        }))
    }
}

struct CognitionProjectCreateTool {
    state: AppState,
    session_id: String,
}

#[async_trait]
impl StasisTool for CognitionProjectCreateTool {
    fn name(&self) -> &'static str {
        PROJECT_CREATE
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Create, provision, and bind a code project after the user explicitly asks for project creation. Blank projects are initialized under the connected workshop's Medousa projects directory.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["title", "brief", "source"],
            "properties": {
                "title": { "type": "string" },
                "brief": { "type": "string", "description": "Concrete outcome the project should achieve" },
                "source": { "type": "string", "enum": ["blank", "repository"] },
                "repo_path": { "type": "string", "description": "Required only for source=repository" },
                "base_ref": { "type": "string", "default": "main" }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> Result<Value> {
        let request: StartSessionCodeProjectRequest = serde_json::from_value(input)
            .map_err(|err| StasisError::PortFailure(format!("invalid project request: {err}")))?;
        let state = self.state.clone();
        let session_id = self.session_id.clone();
        let response = tokio::task::spawn_blocking(move || {
            crate::daemon::forge_api::start_code_project_for_session(&state, &session_id, request)
        })
        .await
        .map_err(|err| StasisError::PortFailure(format!("project creation task failed: {err}")))?
        .map_err(StasisError::PortFailure)?;
        serde_json::to_value(response)
            .map(|value| json!({
                "ok": true,
                "project": value,
                "message": "Project created and bound. Full Coder tools become active on the next turn.",
            }))
            .map_err(|err| StasisError::PortFailure(err.to_string()))
    }
}
