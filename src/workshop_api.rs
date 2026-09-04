//! Public workshop primitives: one query tool and one mutate tool.
//!
//! The model-facing entry is a tagged action enum. Parameter schemas live on
//! each variant type — `cognition_schema` reads those types, not a parallel catalog.

use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use schemars::schema::Schema;
use serde::Deserialize;
use serde_json::Value;

#[cfg(feature = "full-daemon")]
use crate::agent_runtime::turn_worker::TurnWorkerScheduler;
#[cfg(feature = "full-daemon")]
use crate::agent_runtime::turn_worker_tools::{
    CognitionSpawnTurnWorkerTool, CognitionTurnWorkerCancelTool, CognitionTurnWorkerStatusTool,
    CognitionWorkshopSteerTool, SpawnTurnWorkerInput, TurnWorkerCancelInput, TurnWorkerStatusInput,
    WorkshopSteerInput,
};
use crate::public_api::{COGNITION_WORKSHOP_MUTATE, COGNITION_WORKSHOP_QUERY};
use crate::schema_api::{
    TypedActionSchema, advertised_object_schema, string_enum_schema, typed_action_schema,
};
#[cfg(feature = "full-daemon")]
use crate::typed_tools::{CompatOption, TypedTool, serialize_output};
use crate::typed_tools::{ExternalJson, ToolId, medousa_tool};
use crate::workshop_contract::{
    EXECUTION_TARGET_INVENTORY_SCHEMA_VERSION, ExecutionPlacementResolution,
    ExecutionResolutionReason, ExecutionTargetCandidate, ExecutionTargetInventory,
    ExecutionTargetResolutionError, ExecutionTargetSelection, WorkshopSpawn,
    resolve_execution_target, workshop_spawn_type_schema,
};

const WORKSHOP_QUERY_ID: ToolId = ToolId::new(COGNITION_WORKSHOP_QUERY);
const WORKSHOP_MUTATE_ID: ToolId = ToolId::new(COGNITION_WORKSHOP_MUTATE);

#[derive(Debug, Deserialize)]
#[serde(tag = "action")]
pub enum WorkshopQueryAction {
    #[serde(rename = "workshop.status")]
    Status(WorkshopStatus),
    #[serde(rename = "workshop.targets")]
    Targets(WorkshopTargets),
}

#[derive(Debug, Deserialize)]
#[serde(tag = "action")]
pub enum WorkshopMutateAction {
    #[serde(rename = "workshop.spawn")]
    Spawn(WorkshopSpawn),
    #[serde(rename = "workshop.cancel")]
    Cancel(WorkshopCancel),
    #[serde(rename = "workshop.steer")]
    Steer(WorkshopSteer),
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct WorkshopStatus {
    #[serde(default)]
    pub(crate) work_id: Option<String>,
    #[serde(default)]
    pub(crate) session_id: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct WorkshopTargets {}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WorkshopCancel {
    pub(crate) work_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WorkshopSteer {
    /// Exact bound-workshop generation returned by begin-work
    pub(crate) work_id: String,
    /// Steer text for the bound workshop
    pub(crate) message: String,
}

impl JsonSchema for WorkshopQueryAction {
    fn schema_name() -> String {
        "WorkshopQueryAction".to_string()
    }

    fn json_schema(_: &mut schemars::r#gen::SchemaGenerator) -> Schema {
        advertised_object_schema(&[(
            "action",
            string_enum_schema(&["workshop.status", "workshop.targets"]),
            true,
        )])
    }
}

impl JsonSchema for WorkshopMutateAction {
    fn schema_name() -> String {
        "WorkshopMutateAction".to_string()
    }

    fn json_schema(_: &mut schemars::r#gen::SchemaGenerator) -> Schema {
        advertised_object_schema(&[(
            "action",
            string_enum_schema(&["workshop.spawn", "workshop.cancel", "workshop.steer"]),
            true,
        )])
    }
}

pub fn workshop_type_schemas() -> Vec<TypedActionSchema> {
    vec![
        typed_action_schema::<WorkshopStatus>(
            WORKSHOP_QUERY_ID,
            "workshop.status",
            "List or fetch status of background turn workers / bound workshop",
        ),
        typed_action_schema::<WorkshopTargets>(
            WORKSHOP_QUERY_ID,
            "workshop.targets",
            "List execution targets authorized for agent-selected worker placement",
        ),
        workshop_spawn_type_schema(),
        typed_action_schema::<WorkshopCancel>(
            WORKSHOP_MUTATE_ID,
            "workshop.cancel",
            "Cancel a worker generation owned by the active host session",
        ),
        typed_action_schema::<WorkshopSteer>(
            WORKSHOP_MUTATE_ID,
            "workshop.steer",
            "Forward principal guidance into the active bound workshop",
        ),
    ]
}

#[async_trait]
pub trait WorkshopExecution: Send + Sync {
    async fn inventory(
        &self,
        agent_only: bool,
    ) -> stasis::prelude::Result<ExecutionTargetInventory>;
    async fn status(&self, input: WorkshopStatus) -> stasis::prelude::Result<Value>;
    async fn spawn(&self, input: WorkshopSpawn) -> stasis::prelude::Result<Value>;
    async fn cancel(&self, input: WorkshopCancel) -> stasis::prelude::Result<Value>;
    async fn steer(&self, input: WorkshopSteer) -> stasis::prelude::Result<Value>;
}

/// One concrete execution authority beneath the location-neutral workshop
/// tool. The router resolves placement before handing work to an adapter.
#[async_trait]
pub trait WorkshopExecutionTarget: Send + Sync {
    async fn candidates(&self) -> stasis::prelude::Result<Vec<ExecutionTargetCandidate>>;
    async fn ingress_default_runtime_id(&self) -> stasis::prelude::Result<Option<String>> {
        Ok(None)
    }
    async fn spawn_resolved(
        &self,
        input: WorkshopSpawn,
        parent_runtime_id: &str,
        resolution: ExecutionPlacementResolution,
    ) -> stasis::prelude::Result<Value>;
    async fn status(&self, input: WorkshopStatus) -> stasis::prelude::Result<Value>;
    async fn cancel(&self, input: WorkshopCancel) -> stasis::prelude::Result<Value>;
    async fn steer(&self, input: WorkshopSteer) -> stasis::prelude::Result<Value>;
}

/// Supplies the runtime identity of the host turn at spawn time. Full daemons
/// bind this to the scheduler so it always matches the Stasis worker host.
pub trait WorkshopParentRuntime: Send + Sync {
    fn runtime_id(&self) -> String;
}

struct FixedWorkshopParentRuntime(String);

impl WorkshopParentRuntime for FixedWorkshopParentRuntime {
    fn runtime_id(&self) -> String {
        self.0.clone()
    }
}

#[cfg(feature = "full-daemon")]
impl WorkshopParentRuntime for TurnWorkerScheduler {
    fn runtime_id(&self) -> String {
        self.execution_runtime_id()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkshopIngressDefault {
    SameAsParent,
    /// Compatibility bridge for Personal/mobile deployments that historically
    /// exposed one explicitly bound remote daemon as their only worker.
    BoundRemote,
}

pub struct WorkshopExecutionRouter {
    parent_runtime: Arc<dyn WorkshopParentRuntime>,
    ingress_default: WorkshopIngressDefault,
    targets: Vec<Arc<dyn WorkshopExecutionTarget>>,
}

impl WorkshopExecutionRouter {
    pub fn new(
        parent_runtime_id: impl Into<String>,
        ingress_default: WorkshopIngressDefault,
        targets: Vec<Arc<dyn WorkshopExecutionTarget>>,
    ) -> Self {
        Self::with_parent_runtime(
            Arc::new(FixedWorkshopParentRuntime(parent_runtime_id.into())),
            ingress_default,
            targets,
        )
    }

    pub fn with_parent_runtime(
        parent_runtime: Arc<dyn WorkshopParentRuntime>,
        ingress_default: WorkshopIngressDefault,
        targets: Vec<Arc<dyn WorkshopExecutionTarget>>,
    ) -> Self {
        Self {
            parent_runtime,
            ingress_default,
            targets,
        }
    }

    async fn candidates(
        &self,
    ) -> stasis::prelude::Result<Vec<(Arc<dyn WorkshopExecutionTarget>, ExecutionTargetCandidate)>>
    {
        let mut candidates = Vec::new();
        for target in &self.targets {
            for candidate in target.candidates().await? {
                candidates.push((target.clone(), candidate));
            }
        }
        candidates.sort_by(|left, right| left.1.runtime_id.cmp(&right.1.runtime_id));
        Ok(candidates)
    }

    async fn resolve_spawn(
        &self,
        input: &WorkshopSpawn,
    ) -> stasis::prelude::Result<(
        Arc<dyn WorkshopExecutionTarget>,
        String,
        ExecutionPlacementResolution,
    )> {
        let candidates = self.candidates().await?;
        let parent_runtime_id = self.parent_runtime.runtime_id();
        let requested = match (&input.execution_target, self.ingress_default) {
            (Some(requested), _) => requested.clone(),
            (None, WorkshopIngressDefault::SameAsParent) => ExecutionTargetSelection::SameAsParent,
            (None, WorkshopIngressDefault::BoundRemote) => {
                let mut bound_runtime_id = None;
                for target in &self.targets {
                    if let Some(runtime_id) = target.ingress_default_runtime_id().await? {
                        bound_runtime_id = Some(runtime_id);
                        break;
                    }
                }
                let runtime_id = bound_runtime_id.ok_or_else(|| {
                    target_resolution_error(ExecutionTargetResolutionError::UnsupportedTarget {
                        detail: "the legacy bound remote workshop is not configured".to_string(),
                    })
                })?;
                ExecutionTargetSelection::Exact { runtime_id }
            }
        };
        let candidate_values = candidates
            .iter()
            .map(|(_, candidate)| candidate.clone())
            .collect::<Vec<_>>();
        let mut resolution =
            resolve_execution_target(requested, &parent_runtime_id, &candidate_values)
                .map_err(target_resolution_error)?;
        if input.execution_target.is_none()
            && self.ingress_default == WorkshopIngressDefault::BoundRemote
        {
            resolution.resolution_reason = ExecutionResolutionReason::IngressDefault;
        }
        let target = candidates
            .into_iter()
            .find(|(_, candidate)| candidate.runtime_id == resolution.resolved_runtime_id)
            .map(|(target, _)| target)
            .ok_or_else(|| {
                target_resolution_error(ExecutionTargetResolutionError::ExactUnavailable {
                    runtime_id: resolution.resolved_runtime_id.clone(),
                })
            })?;
        Ok((target, parent_runtime_id, resolution))
    }

    fn control_target(&self) -> stasis::prelude::Result<Arc<dyn WorkshopExecutionTarget>> {
        self.targets.first().cloned().ok_or_else(|| {
            target_resolution_error(ExecutionTargetResolutionError::UnsupportedTarget {
                detail: "this deployment has no workshop execution target".to_string(),
            })
        })
    }
}

fn target_resolution_error(
    error: ExecutionTargetResolutionError,
) -> stasis::domain::errors::StasisError {
    stasis::domain::errors::StasisError::PortFailure(error.to_string())
}

#[async_trait]
impl WorkshopExecution for WorkshopExecutionRouter {
    async fn inventory(
        &self,
        agent_only: bool,
    ) -> stasis::prelude::Result<ExecutionTargetInventory> {
        let parent_runtime_id = self.parent_runtime.runtime_id();
        let mut targets = self
            .candidates()
            .await?
            .into_iter()
            .map(|(_, candidate)| candidate)
            .filter(|candidate| {
                if agent_only {
                    candidate.agent_selectable
                } else {
                    candidate.user_selectable
                }
            })
            .map(|candidate| candidate.inventory_entry())
            .collect::<Vec<_>>();
        targets.dedup_by(|left, right| left.runtime_id == right.runtime_id);
        Ok(ExecutionTargetInventory {
            schema_version: EXECUTION_TARGET_INVENTORY_SCHEMA_VERSION,
            parent_runtime_id,
            targets,
        })
    }

    async fn status(&self, input: WorkshopStatus) -> stasis::prelude::Result<Value> {
        self.control_target()?.status(input).await
    }

    async fn spawn(&self, input: WorkshopSpawn) -> stasis::prelude::Result<Value> {
        let (target, parent_runtime_id, resolution) = self.resolve_spawn(&input).await?;
        target
            .spawn_resolved(input, &parent_runtime_id, resolution)
            .await
    }

    async fn cancel(&self, input: WorkshopCancel) -> stasis::prelude::Result<Value> {
        self.control_target()?.cancel(input).await
    }

    async fn steer(&self, input: WorkshopSteer) -> stasis::prelude::Result<Value> {
        self.control_target()?.steer(input).await
    }
}

pub struct CognitionWorkshopQueryTool {
    execution: Arc<dyn WorkshopExecution>,
}

pub struct CognitionWorkshopMutateTool {
    execution: Arc<dyn WorkshopExecution>,
}

pub fn register_workshop_execution_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    execution: Arc<dyn WorkshopExecution>,
) -> stasis::prelude::Result<()> {
    registry.register_typed_tool(CognitionWorkshopQueryTool {
        execution: execution.clone(),
    })?;
    registry.register_typed_tool(CognitionWorkshopMutateTool { execution })?;
    Ok(())
}

#[cfg(feature = "full-daemon")]
struct LocalWorkshopExecution {
    scheduler: Arc<TurnWorkerScheduler>,
}

#[cfg(feature = "full-daemon")]
#[async_trait]
impl WorkshopExecutionTarget for LocalWorkshopExecution {
    async fn candidates(&self) -> stasis::prelude::Result<Vec<ExecutionTargetCandidate>> {
        let runtime_id = self.scheduler.execution_runtime_id();
        Ok(vec![ExecutionTargetCandidate::local(
            runtime_id.clone(),
            stasis::domain::runtime::placement::WorkerCapabilities::any()
                .node_id(&runtime_id)
                .platform(std::env::consts::OS)
                .architecture(std::env::consts::ARCH)
                .with_capability("assistant.work"),
        )])
    }

    async fn status(&self, input: WorkshopStatus) -> stasis::prelude::Result<Value> {
        let output = CognitionTurnWorkerStatusTool::new(self.scheduler.clone())
            .invoke_typed(TurnWorkerStatusInput {
                work_id: CompatOption::from(input.work_id),
                session_id: CompatOption::from(input.session_id),
            })
            .await?;
        serialize_output(CognitionTurnWorkerStatusTool::tool_id(), output)
    }

    async fn spawn_resolved(
        &self,
        mut input: WorkshopSpawn,
        parent_runtime_id: &str,
        resolution: ExecutionPlacementResolution,
    ) -> stasis::prelude::Result<Value> {
        let runtime_id = self.scheduler.execution_runtime_id();
        if resolution.resolved_runtime_id != runtime_id || parent_runtime_id != runtime_id {
            return Err(target_resolution_error(
                ExecutionTargetResolutionError::ExactUnavailable {
                    runtime_id: resolution.resolved_runtime_id,
                },
            ));
        }
        input.execution_target = Some(resolution.requested);
        let output = CognitionSpawnTurnWorkerTool::new(self.scheduler.clone())
            .invoke_typed(SpawnTurnWorkerInput {
                intent: CompatOption::from(input.intent),
                task: CompatOption::from(Some(input.task)),
                user_ack: CompatOption::from(Some(input.user_ack)),
                manuscript_id: CompatOption::from(input.manuscript_id),
                stage_role: CompatOption::from(input.stage_role),
                model_hint: CompatOption::from(input.model_hint),
                execution_target: CompatOption::from(input.execution_target),
            })
            .await?;
        serialize_output(CognitionSpawnTurnWorkerTool::tool_id(), output)
    }

    async fn cancel(&self, input: WorkshopCancel) -> stasis::prelude::Result<Value> {
        let output = CognitionTurnWorkerCancelTool::new(self.scheduler.clone())
            .invoke_typed(TurnWorkerCancelInput {
                work_id: input.work_id,
            })
            .await?;
        serialize_output(CognitionTurnWorkerCancelTool::tool_id(), output)
    }

    async fn steer(&self, input: WorkshopSteer) -> stasis::prelude::Result<Value> {
        let output = CognitionWorkshopSteerTool::new(self.scheduler.clone())
            .invoke_typed(WorkshopSteerInput {
                work_id: input.work_id,
                message: input.message,
            })
            .await?;
        serialize_output(CognitionWorkshopSteerTool::tool_id(), output)
    }
}

#[cfg(feature = "full-daemon")]
pub fn register_workshop_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    scheduler: Arc<TurnWorkerScheduler>,
) -> stasis::prelude::Result<()> {
    let target: Arc<dyn WorkshopExecutionTarget> = Arc::new(LocalWorkshopExecution {
        scheduler: scheduler.clone(),
    });
    register_workshop_execution_tools(
        registry,
        Arc::new(WorkshopExecutionRouter::with_parent_runtime(
            scheduler,
            WorkshopIngressDefault::SameAsParent,
            vec![target],
        )),
    )
}

#[medousa_tool(id = WORKSHOP_QUERY_ID)]
impl CognitionWorkshopQueryTool {
    /// Read workshop/worker status and authorized targets. action is a typed name (workshop.status, workshop.targets). Fetch fields with cognition_schema types=[...].
    async fn invoke_typed(
        &self,
        action: WorkshopQueryAction,
    ) -> stasis::prelude::Result<ExternalJson> {
        Ok(ExternalJson::new(dispatch_query(self, action).await?))
    }
}

#[medousa_tool(id = WORKSHOP_MUTATE_ID)]
impl CognitionWorkshopMutateTool {
    /// Write workshop/worker control: spawn, cancel, or steer. action is a typed name (workshop.spawn, workshop.cancel, workshop.steer). Fetch fields with cognition_schema types=[...].
    async fn invoke_typed(
        &self,
        action: WorkshopMutateAction,
    ) -> stasis::prelude::Result<ExternalJson> {
        Ok(ExternalJson::new(dispatch_mutate(self, action).await?))
    }
}

async fn dispatch_query(
    tool: &CognitionWorkshopQueryTool,
    action: WorkshopQueryAction,
) -> stasis::prelude::Result<Value> {
    match action {
        WorkshopQueryAction::Status(params) => params.execute(tool).await,
        WorkshopQueryAction::Targets(params) => params.execute(tool).await,
    }
}

async fn dispatch_mutate(
    tool: &CognitionWorkshopMutateTool,
    action: WorkshopMutateAction,
) -> stasis::prelude::Result<Value> {
    match action {
        WorkshopMutateAction::Spawn(params) => params.execute(tool).await,
        WorkshopMutateAction::Cancel(params) => params.execute(tool).await,
        WorkshopMutateAction::Steer(params) => params.execute(tool).await,
    }
}

impl WorkshopStatus {
    async fn execute(self, tool: &CognitionWorkshopQueryTool) -> stasis::prelude::Result<Value> {
        tool.execution.status(self).await
    }
}

impl WorkshopTargets {
    async fn execute(self, tool: &CognitionWorkshopQueryTool) -> stasis::prelude::Result<Value> {
        serde_json::to_value(tool.execution.inventory(true).await?)
            .map_err(|error| stasis::domain::errors::StasisError::PortFailure(error.to_string()))
    }
}

impl WorkshopSpawn {
    async fn execute(self, tool: &CognitionWorkshopMutateTool) -> stasis::prelude::Result<Value> {
        tool.execution.spawn(self).await
    }
}

impl WorkshopCancel {
    async fn execute(self, tool: &CognitionWorkshopMutateTool) -> stasis::prelude::Result<Value> {
        tool.execution.cancel(self).await
    }
}

impl WorkshopSteer {
    async fn execute(self, tool: &CognitionWorkshopMutateTool) -> stasis::prelude::Result<Value> {
        tool.execution.steer(self).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct FakeExecutionTarget {
        runtime_id: String,
        spawn_count: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl WorkshopExecutionTarget for FakeExecutionTarget {
        async fn candidates(&self) -> stasis::prelude::Result<Vec<ExecutionTargetCandidate>> {
            Ok(vec![ExecutionTargetCandidate {
                runtime_id: self.runtime_id.clone(),
                label: self.runtime_id.clone(),
                capabilities: stasis::domain::runtime::placement::WorkerCapabilities::any()
                    .node_id(&self.runtime_id)
                    .with_capability("assistant.work"),
                user_selectable: true,
                agent_selectable: true,
            }])
        }

        async fn spawn_resolved(
            &self,
            _input: WorkshopSpawn,
            parent_runtime_id: &str,
            resolution: ExecutionPlacementResolution,
        ) -> stasis::prelude::Result<Value> {
            self.spawn_count.fetch_add(1, Ordering::SeqCst);
            Ok(json!({
                "parent_runtime_id": parent_runtime_id,
                "execution_placement": resolution,
            }))
        }

        async fn status(&self, _input: WorkshopStatus) -> stasis::prelude::Result<Value> {
            Ok(json!({ "ok": true }))
        }

        async fn cancel(&self, _input: WorkshopCancel) -> stasis::prelude::Result<Value> {
            Ok(json!({ "ok": true }))
        }

        async fn steer(&self, _input: WorkshopSteer) -> stasis::prelude::Result<Value> {
            Ok(json!({ "ok": true }))
        }
    }

    fn spawn_for(target: Option<ExecutionTargetSelection>) -> WorkshopSpawn {
        WorkshopSpawn {
            intent: Some("research".to_string()),
            task: "look this up".to_string(),
            user_ack: "On it".to_string(),
            manuscript_id: None,
            stage_role: None,
            model_hint: None,
            execution_target: target,
        }
    }

    #[test]
    fn workshop_actions_carry_their_params() {
        let query: WorkshopQueryAction = serde_json::from_value(json!({
            "action": "workshop.status",
            "work_id": "work-1"
        }))
        .expect("status");
        match query {
            WorkshopQueryAction::Status(WorkshopStatus { work_id, .. }) => {
                assert_eq!(work_id.as_deref(), Some("work-1"));
            }
            WorkshopQueryAction::Targets(_) => panic!("expected workshop.status"),
        }
        let mutate: WorkshopMutateAction = serde_json::from_value(json!({
            "action": "workshop.spawn",
            "intent": "research",
            "task": "look this up",
            "user_ack": "On it."
        }))
        .expect("spawn");
        match mutate {
            WorkshopMutateAction::Spawn(WorkshopSpawn { intent, task, .. }) => {
                assert_eq!(intent.as_deref(), Some("research"));
                assert_eq!(task, "look this up");
            }
            other => panic!("expected workshop.spawn, got {other:?}"),
        }
    }

    #[test]
    fn advertised_schemas_are_action_enums_only() {
        let query =
            serde_json::to_value(schemars::schema_for!(WorkshopQueryAction)).expect("query");
        let mutate =
            serde_json::to_value(schemars::schema_for!(WorkshopMutateAction)).expect("mutate");
        for schema in [&query, &mutate] {
            let props = schema["properties"].as_object().expect("properties");
            assert_eq!(props.len(), 1);
            assert!(
                props["action"]["enum"]
                    .as_array()
                    .is_some_and(|values| !values.is_empty())
            );
            assert_eq!(schema["additionalProperties"], true);
        }
    }

    #[tokio::test]
    async fn exact_unavailable_target_fails_before_execution() {
        let spawn_count = Arc::new(AtomicUsize::new(0));
        let router = WorkshopExecutionRouter::new(
            "runtime-local",
            WorkshopIngressDefault::SameAsParent,
            vec![Arc::new(FakeExecutionTarget {
                runtime_id: "runtime-local".to_string(),
                spawn_count: spawn_count.clone(),
            })],
        );
        let error = router
            .spawn(spawn_for(Some(ExecutionTargetSelection::Exact {
                runtime_id: "runtime-offline".to_string(),
            })))
            .await
            .expect_err("unavailable target");
        assert!(error.to_string().contains("execution_target_unavailable"));
        assert_eq!(spawn_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn exact_target_routes_once_with_matching_provenance() {
        let spawn_count = Arc::new(AtomicUsize::new(0));
        let router = WorkshopExecutionRouter::new(
            "runtime-parent",
            WorkshopIngressDefault::SameAsParent,
            vec![Arc::new(FakeExecutionTarget {
                runtime_id: "runtime-remote".to_string(),
                spawn_count: spawn_count.clone(),
            })],
        );
        let output = router
            .spawn(spawn_for(Some(ExecutionTargetSelection::Exact {
                runtime_id: "runtime-remote".to_string(),
            })))
            .await
            .expect("route exact target");
        assert_eq!(spawn_count.load(Ordering::SeqCst), 1);
        assert_eq!(
            output["execution_placement"]["resolved_runtime_id"],
            "runtime-remote"
        );
        assert_eq!(output["parent_runtime_id"], "runtime-parent");
    }
}
