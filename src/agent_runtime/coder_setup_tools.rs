//! Least-authority Coder entry surface before a Forge project is bound.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use genai::chat::Tool;
use medousa_forge::model::{WorkState, WorkTarget};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use stasis::application::orchestration::tool_registry::{InMemoryToolRegistry, ToolRegistry};
use stasis::domain::errors::StasisError;
use stasis::prelude::Result;

use crate::daemon::state::AppState;
use crate::daemon_api::{
    CodeProjectSource, SessionCodeProjectResponse, StartSessionCodeProjectRequest,
};
#[cfg(test)]
use crate::typed_tools::TypedTool;
use crate::typed_tools::{ToolId, ToolRegistration, medousa_tool};

const PROJECT_LIST: &str = "cognition_project_list";
const PROJECT_BIND: &str = "cognition_project_bind";
const PROJECT_CREATE: &str = "cognition_project_create";
const PROJECT_LIST_ID: ToolId = ToolId::new(PROJECT_LIST);
const PROJECT_BIND_ID: ToolId = ToolId::new(PROJECT_BIND);
const PROJECT_CREATE_ID: ToolId = ToolId::new(PROJECT_CREATE);
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
        let mut setup = InMemoryToolRegistry::default();
        setup.register_typed_tool(CognitionProjectListTool {
            state: state.clone(),
        })?;
        setup.register_typed_tool(CognitionProjectBindTool {
            state: state.clone(),
            session_id: session_id.clone(),
        })?;
        setup.register_typed_tool(CognitionProjectCreateTool { state, session_id })?;
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

#[derive(Debug, Deserialize, JsonSchema)]
struct ProjectListInput {}

#[derive(Debug, Serialize, JsonSchema)]
struct ProjectListEntryOutput {
    work_id: String,
    title: String,
    brief: String,
    repo_path: PathBuf,
    base_ref: String,
}

#[derive(Debug, Serialize, JsonSchema)]
struct ProjectListOutput {
    ok: bool,
    projects: Vec<ProjectListEntryOutput>,
}

#[medousa_tool(id = PROJECT_LIST_ID)]
impl CognitionProjectListTool {
    /// List ready Forge projects that this Coder conversation can continue.
    async fn invoke_typed(&self, _input: ProjectListInput) -> Result<ProjectListOutput> {
        let items = self
            .state
            .forge
            .list()
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        let projects = items
            .into_iter()
            .filter(|item| {
                matches!(item.state, WorkState::Ready | WorkState::Executing)
                    && item.workspace_environment().is_some()
            })
            .take(20)
            .map(|item| {
                let WorkTarget::Git(target) = item.target;
                ProjectListEntryOutput {
                    work_id: item.id.to_string(),
                    title: item.title,
                    brief: item.brief,
                    repo_path: target.repo_path,
                    base_ref: target.base_ref,
                }
            })
            .collect::<Vec<_>>();
        Ok(ProjectListOutput { ok: true, projects })
    }
}

struct CognitionProjectBindTool {
    state: AppState,
    session_id: String,
}

#[derive(Debug, JsonSchema)]
struct ProjectBindInput {
    /// Ready project id returned by cognition_project_list
    #[schemars(required, with = "String")]
    work_id: Option<String>,
}

impl<'de> Deserialize<'de> for ProjectBindInput {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            work_id: Option<String>,
        }

        Ok(Self {
            work_id: WireInput::deserialize(deserializer)?.work_id,
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
struct ProjectBindOutput {
    ok: bool,
    work_id: String,
    title: String,
    message: String,
}

#[medousa_tool(id = PROJECT_BIND_ID)]
impl CognitionProjectBindTool {
    /// Bind this conversation to a ready Forge project selected by the user. The full Coder workspace becomes active on the next turn.
    async fn invoke_typed(&self, input: ProjectBindInput) -> Result<ProjectBindOutput> {
        let work_id = input
            .work_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StasisError::PortFailure("work_id is required".into()))?;
        let item = self
            .state
            .forge
            .load(&medousa_forge::model::WorkId::from(work_id.to_string()))
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        if !matches!(item.state, WorkState::Ready | WorkState::Executing)
            || item.workspace_environment().is_none()
        {
            return Err(StasisError::PortFailure(
                "project must be available with a governed worktree".into(),
            ));
        }
        crate::agent_mode_state::set_session_code_binding(&self.session_id, work_id)
            .map_err(StasisError::PortFailure)?;
        Ok(ProjectBindOutput {
            ok: true,
            work_id: work_id.to_string(),
            title: item.title,
            message: "Project bound. Full Coder tools become active on the next turn.".to_string(),
        })
    }
}

struct CognitionProjectCreateTool {
    state: AppState,
    session_id: String,
}

#[derive(Debug, JsonSchema)]
struct ProjectCreateInput {
    #[schemars(required, with = "String")]
    title: Option<String>,
    /// Concrete outcome the project should achieve
    #[schemars(required, with = "String")]
    brief: Option<String>,
    #[schemars(required)]
    source: CodeProjectSource,
    /// Required only for source=repository
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    repo_path: Option<String>,
    #[schemars(with = "String", default = "default_project_base_ref")]
    base_ref: Option<String>,
}

fn default_project_base_ref() -> String {
    "main".to_string()
}

impl<'de> Deserialize<'de> for ProjectCreateInput {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(default)]
            title: Option<String>,
            #[serde(default)]
            brief: Option<String>,
            #[serde(default)]
            source: CodeProjectSource,
            #[serde(default)]
            repo_path: Option<String>,
            #[serde(default)]
            base_ref: Option<String>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            title: input.title,
            brief: input.brief,
            source: input.source,
            repo_path: input.repo_path,
            base_ref: input.base_ref,
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
struct ProjectCreateOutput {
    ok: bool,
    project: SessionCodeProjectResponse,
    message: String,
}

#[medousa_tool(id = PROJECT_CREATE_ID)]
impl CognitionProjectCreateTool {
    /// Create, provision, and bind a code project after the user explicitly asks for project creation. Blank projects are initialized under the connected workshop's Medousa projects directory.
    async fn invoke_typed(&self, input: ProjectCreateInput) -> Result<ProjectCreateOutput> {
        let request = StartSessionCodeProjectRequest {
            title: input.title.ok_or_else(|| {
                StasisError::PortFailure(
                    "invalid project request: missing field `title`".to_string(),
                )
            })?,
            brief: input.brief.ok_or_else(|| {
                StasisError::PortFailure(
                    "invalid project request: missing field `brief`".to_string(),
                )
            })?,
            source: input.source,
            repo_path: input.repo_path,
            base_ref: input.base_ref,
        };
        let state = self.state.clone();
        let session_id = self.session_id.clone();
        let response = tokio::task::spawn_blocking(move || {
            crate::daemon::forge_api::start_code_project_for_session(&state, &session_id, request)
        })
        .await
        .map_err(|err| StasisError::PortFailure(format!("project creation task failed: {err}")))?
        .map_err(StasisError::PortFailure)?;
        Ok(ProjectCreateOutput {
            ok: true,
            project: response,
            message: "Project created and bound. Full Coder tools become active on the next turn."
                .to_string(),
        })
    }
}

#[cfg(test)]
fn contract_tool_definition<T: TypedTool>() -> Tool {
    let contract = T::contract();
    Tool::new(contract.id.as_str())
        .with_description(contract.description)
        .with_schema(contract.input_schema.clone())
}

#[cfg(test)]
pub(crate) fn contract_tool_definitions() -> Vec<Tool> {
    vec![
        contract_tool_definition::<CognitionProjectListTool>(),
        contract_tool_definition::<CognitionProjectBindTool>(),
        contract_tool_definition::<CognitionProjectCreateTool>(),
    ]
}

#[cfg(test)]
pub(crate) fn typed_contract_ids() -> [ToolId; 3] {
    [PROJECT_LIST_ID, PROJECT_BIND_ID, PROJECT_CREATE_ID]
}
