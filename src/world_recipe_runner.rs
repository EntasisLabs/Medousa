//! Governed execution for inert semantic world recipes.
//!
//! A client names a server-derived recipe, a durable run id, an exact world
//! for every step, and fresh values separately from the recipe. The runner
//! re-observes before each step, resolves semantic targets without fuzzy
//! matching, and dispatches only through the existing browser/computer world
//! adapters. It never reuses source refs, revisions, permits, or values.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::{LazyLock, Mutex};

use async_trait::async_trait;
use medousa_browser_bridge::{BrowserObservation, BrowserSemanticNode};
use medousa_computer_bridge::{ComputerAction, ComputerSemanticNode};
use medousa_types::{
    WORLD_RECIPE_RUN_SCHEMA_VERSION, WORLD_RECIPE_SCHEMA_VERSION, WorldRecipe,
    WorldRecipeInputKind, WorldRecipeOperation, WorldRecipeRunInputValue, WorldRecipeRunRequest,
    WorldRecipeRunResponse, WorldRecipeRunStatus, WorldRecipeRunStepResult, WorldRecipeRunStop,
    WorldRecipeStep,
};
use medousa_world::{
    WORLD_ACTION_RECIPE_HINT_SCHEMA_VERSION, WorldActionRecipeHint, WorldActionStatus,
    WorldEffectClass, WorldId, WorldOwnership, WorldPrincipal,
    WorldRecipeInputKind as AuthorityRecipeInputKind, WorldRecipeOperationHint, WorldSession,
    WorldSurfaceKind,
};
use serde_json::{Value, json};

use crate::browser_host_client::{
    BrowserHostWorldContext, browser_host_act, browser_host_current_context, browser_host_observe,
};
use crate::computer_driver::{
    ComputerActionIntent, ComputerObservationIntent, desktop_resource_id,
};
use crate::daemon::state::AppState;
use crate::world_recipes::{WorldRecipeDerivationError, derive_world_recipe};

const MAX_RUN_ID_BYTES: usize = 128;
const MAX_WORLD_ID_BYTES: usize = 512;
const MAX_RUN_OPERATIONS: usize = 32;
const MAX_FRESH_INPUT_BYTES: usize = 8 * 1024;
const MAX_BROWSER_RECIPE_NODES: usize = 512;
const MAX_COMPUTER_RECIPE_NODES: u32 = 512;
const RECIPE_EXECUTOR_PRINCIPAL: &str = "agent:medousa-recipe-runner";

static ACTIVE_RECIPE_RUNS: LazyLock<Mutex<BTreeSet<String>>> =
    LazyLock::new(|| Mutex::new(BTreeSet::new()));

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorldRecipeRunError {
    Invalid(String),
    NotFound(String),
    Conflict(String),
    Unavailable(String),
}

impl fmt::Display for WorldRecipeRunError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message)
            | Self::NotFound(message)
            | Self::Conflict(message)
            | Self::Unavailable(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for WorldRecipeRunError {}

#[derive(Debug, Clone)]
struct PreparedRecipeRun {
    recipe_id: String,
    run_id: String,
    trace_id: String,
    steps: Vec<PreparedRecipeStep>,
}

#[derive(Debug, Clone)]
struct PreparedRecipeStep {
    recipe: WorldRecipeStep,
    target_world_id: String,
    operations: Vec<PreparedRecipeOperation>,
}

#[derive(Debug, Clone)]
struct PreparedRecipeOperation {
    recipe: WorldRecipeOperation,
    input: Option<WorldRecipeRunInputValue>,
    confirmed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExecutedRecipeStep {
    intent_id: String,
    committed_revision: u64,
    completion_sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RecipeStepFailure {
    code: String,
    reason: String,
    effect_may_have_applied: bool,
}

impl RecipeStepFailure {
    fn before_dispatch(code: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            reason: reason.into(),
            effect_may_have_applied: false,
        }
    }

    fn indeterminate(code: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            reason: reason.into(),
            effect_may_have_applied: true,
        }
    }
}

#[async_trait]
trait RecipeStepExecutor: Sync {
    async fn execute_step(
        &self,
        step: &PreparedRecipeStep,
        trace_id: &str,
    ) -> Result<ExecutedRecipeStep, RecipeStepFailure>;
}

struct ActiveRecipeRunGuard {
    run_id: String,
}

impl Drop for ActiveRecipeRunGuard {
    fn drop(&mut self) {
        if let Ok(mut active) = ACTIVE_RECIPE_RUNS.lock() {
            active.remove(&self.run_id);
        }
    }
}

/// Re-derive and execute a recipe using only destination-owned driver ports.
pub async fn run_world_recipe(
    state: &AppState,
    owner_profile_id: String,
    request: WorldRecipeRunRequest,
) -> Result<WorldRecipeRunResponse, WorldRecipeRunError> {
    validate_identifier(
        "source_trace_id",
        &request.source_trace_id,
        MAX_WORLD_ID_BYTES,
    )?;
    validate_identifier("recipe_id", &request.recipe_id, MAX_WORLD_ID_BYTES)?;
    let source_records = state
        .world_authority
        .durable_events_for_trace(&request.source_trace_id)
        .map_err(WorldRecipeRunError::Unavailable)?;
    let recipe = derive_world_recipe(&request.source_trace_id, &source_records).map_err(
        |error| match error {
            WorldRecipeDerivationError::TraceNotFound => {
                WorldRecipeRunError::NotFound(error.to_string())
            }
            WorldRecipeDerivationError::NotReplayable(_) => {
                WorldRecipeRunError::Conflict(error.to_string())
            }
        },
    )?;
    if recipe.recipe_id != request.recipe_id {
        return Err(WorldRecipeRunError::Conflict(
            "recipe_id does not match the destination daemon's current source trace".to_string(),
        ));
    }

    let prepared = prepare_recipe_run(&recipe, request)?;
    let _reservation = reserve_recipe_run(state, &prepared.run_id, &prepared.trace_id)?;
    let authority_id = crate::workshop_authority::current()
        .map_err(WorldRecipeRunError::Unavailable)?
        .to_string();
    let executor = DaemonRecipeStepExecutor {
        state,
        owner_profile_id,
        authority_id,
        principal: WorldPrincipal::agent(RECIPE_EXECUTOR_PRINCIPAL),
    };
    Ok(execute_prepared_recipe(&prepared, &executor).await)
}

fn reserve_recipe_run(
    state: &AppState,
    run_id: &str,
    trace_id: &str,
) -> Result<ActiveRecipeRunGuard, WorldRecipeRunError> {
    state
        .world_authority
        .ensure_durable_timeline_writable()
        .map_err(WorldRecipeRunError::Unavailable)?;
    let mut active = ACTIVE_RECIPE_RUNS
        .lock()
        .map_err(|_| WorldRecipeRunError::Unavailable("recipe run lock poisoned".to_string()))?;
    if active.contains(run_id) {
        return Err(WorldRecipeRunError::Conflict(
            "a recipe run with this run_id is already active".to_string(),
        ));
    }
    let prior = state
        .world_authority
        .durable_events_for_trace(trace_id)
        .map_err(WorldRecipeRunError::Unavailable)?;
    if !prior.is_empty() {
        return Err(WorldRecipeRunError::Conflict(
            "this run_id already crossed a durable world boundary; inspect its trace instead of replaying it"
                .to_string(),
        ));
    }
    active.insert(run_id.to_string());
    Ok(ActiveRecipeRunGuard {
        run_id: run_id.to_string(),
    })
}

fn prepare_recipe_run(
    recipe: &WorldRecipe,
    request: WorldRecipeRunRequest,
) -> Result<PreparedRecipeRun, WorldRecipeRunError> {
    if recipe.schema_version != WORLD_RECIPE_SCHEMA_VERSION
        || recipe.execution_model != "fresh_semantic_replay"
        || recipe.carries_authority
        || recipe.automatic_dispatch_allowed
    {
        return Err(WorldRecipeRunError::Conflict(
            "recipe does not satisfy the inert semantic replay contract".to_string(),
        ));
    }
    if !request.operator_approved {
        return Err(WorldRecipeRunError::Invalid(
            "operator_approved must be true for a fresh governed run".to_string(),
        ));
    }
    validate_run_id(&request.run_id)?;
    let trace_id = format!("world-recipe-run:{}", request.run_id);
    validate_identifier("recipe run trace", &trace_id, MAX_WORLD_ID_BYTES)?;

    let mut targets = BTreeMap::new();
    for target in request.targets {
        validate_identifier("target world_id", &target.world_id, MAX_WORLD_ID_BYTES)?;
        if targets
            .insert(target.step_ordinal, target.world_id)
            .is_some()
        {
            return Err(WorldRecipeRunError::Invalid(format!(
                "step {} has duplicate world targets",
                target.step_ordinal
            )));
        }
    }

    let mut inputs = BTreeMap::new();
    for input in request.inputs {
        validate_fresh_input(&input.input)?;
        if inputs
            .insert((input.step_ordinal, input.operation_index), input.input)
            .is_some()
        {
            return Err(WorldRecipeRunError::Invalid(format!(
                "step {} operation {} has duplicate fresh inputs",
                input.step_ordinal, input.operation_index
            )));
        }
    }

    let mut confirmations = BTreeSet::new();
    for confirmation in request.confirmations {
        if !confirmations.insert((confirmation.step_ordinal, confirmation.operation_index)) {
            return Err(WorldRecipeRunError::Invalid(format!(
                "step {} operation {} has duplicate confirmations",
                confirmation.step_ordinal, confirmation.operation_index
            )));
        }
    }

    if recipe.steps.is_empty() {
        return Err(WorldRecipeRunError::Conflict(
            "recipe contains no executable steps".to_string(),
        ));
    }
    let operation_count = recipe
        .steps
        .iter()
        .map(|step| step.operations.len())
        .sum::<usize>();
    if operation_count == 0 || operation_count > MAX_RUN_OPERATIONS {
        return Err(WorldRecipeRunError::Conflict(format!(
            "recipe run must contain 1 to {MAX_RUN_OPERATIONS} operations"
        )));
    }

    let mut seen_steps = BTreeSet::new();
    let mut valid_operations = BTreeSet::new();
    let mut prepared_steps = Vec::with_capacity(recipe.steps.len());
    for step in &recipe.steps {
        if step.ordinal == 0 || !seen_steps.insert(step.ordinal) {
            return Err(WorldRecipeRunError::Conflict(
                "recipe contains invalid or duplicate step ordinals".to_string(),
            ));
        }
        if !step.requires_fresh_observation || !step.requires_fresh_admission {
            return Err(WorldRecipeRunError::Conflict(format!(
                "step {} does not require fresh observation and admission",
                step.ordinal
            )));
        }
        validate_surface(&step.surface)?;
        validate_effect_class(&step.effect_class)?;
        let target_world_id = targets.remove(&step.ordinal).ok_or_else(|| {
            WorldRecipeRunError::Invalid(format!(
                "step {} requires an explicit target world",
                step.ordinal
            ))
        })?;
        if step.operations.is_empty() || step.operations.len() > 16 {
            return Err(WorldRecipeRunError::Conflict(format!(
                "step {} has an unsupported operation count",
                step.ordinal
            )));
        }

        let mut prepared_operations = Vec::with_capacity(step.operations.len());
        for (index, operation) in step.operations.iter().enumerate() {
            let operation_index = u32::try_from(index).unwrap_or(u32::MAX);
            let key = (step.ordinal, operation_index);
            valid_operations.insert(key);
            let input = inputs.remove(&key);
            validate_operation_input(step.ordinal, operation_index, operation, input.as_ref())?;
            let confirmed = confirmations.contains(&key);
            if (step.requires_operator_confirmation || operation.requires_operator_confirmation)
                && !confirmed
            {
                return Err(WorldRecipeRunError::Invalid(format!(
                    "step {} operation {} requires explicit operator confirmation",
                    step.ordinal, operation_index
                )));
            }
            prepared_operations.push(PreparedRecipeOperation {
                recipe: operation.clone(),
                input,
                confirmed,
            });
        }
        prepared_steps.push(PreparedRecipeStep {
            recipe: step.clone(),
            target_world_id,
            operations: prepared_operations,
        });
    }

    if !targets.is_empty() {
        return Err(WorldRecipeRunError::Invalid(
            "request contains a target for an unknown recipe step".to_string(),
        ));
    }
    if !inputs.is_empty() {
        return Err(WorldRecipeRunError::Invalid(
            "request contains input for an unknown or input-free operation".to_string(),
        ));
    }
    if confirmations
        .iter()
        .any(|confirmation| !valid_operations.contains(confirmation))
    {
        return Err(WorldRecipeRunError::Invalid(
            "request contains confirmation for an unknown operation".to_string(),
        ));
    }

    Ok(PreparedRecipeRun {
        recipe_id: recipe.recipe_id.clone(),
        run_id: request.run_id,
        trace_id,
        steps: prepared_steps,
    })
}

async fn execute_prepared_recipe(
    prepared: &PreparedRecipeRun,
    executor: &impl RecipeStepExecutor,
) -> WorldRecipeRunResponse {
    let mut steps = Vec::with_capacity(prepared.steps.len());
    for step in &prepared.steps {
        match executor.execute_step(step, &prepared.trace_id).await {
            Ok(receipt) => steps.push(WorldRecipeRunStepResult {
                step_ordinal: step.recipe.ordinal,
                world_id: step.target_world_id.clone(),
                surface: step.recipe.surface.clone(),
                operation_count: u32::try_from(step.operations.len()).unwrap_or(u32::MAX),
                intent_id: receipt.intent_id,
                committed_revision: receipt.committed_revision,
                completion_sequence: receipt.completion_sequence,
            }),
            Err(failure) => {
                return WorldRecipeRunResponse {
                    schema_version: WORLD_RECIPE_RUN_SCHEMA_VERSION,
                    recipe_id: prepared.recipe_id.clone(),
                    run_id: prepared.run_id.clone(),
                    trace_id: prepared.trace_id.clone(),
                    status: WorldRecipeRunStatus::Stopped,
                    completed_steps: u32::try_from(steps.len()).unwrap_or(u32::MAX),
                    steps,
                    stop: Some(WorldRecipeRunStop {
                        step_ordinal: step.recipe.ordinal,
                        code: failure.code,
                        reason: failure.reason,
                        effect_may_have_applied: failure.effect_may_have_applied,
                    }),
                };
            }
        }
    }
    WorldRecipeRunResponse {
        schema_version: WORLD_RECIPE_RUN_SCHEMA_VERSION,
        recipe_id: prepared.recipe_id.clone(),
        run_id: prepared.run_id.clone(),
        trace_id: prepared.trace_id.clone(),
        status: WorldRecipeRunStatus::Completed,
        completed_steps: u32::try_from(steps.len()).unwrap_or(u32::MAX),
        steps,
        stop: None,
    }
}

fn validate_run_id(run_id: &str) -> Result<(), WorldRecipeRunError> {
    validate_identifier("run_id", run_id, MAX_RUN_ID_BYTES)?;
    if !run_id
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(WorldRecipeRunError::Invalid(
            "run_id may contain only letters, numbers, '.', '-', and '_'".to_string(),
        ));
    }
    Ok(())
}

fn validate_identifier(
    label: &str,
    value: &str,
    max_bytes: usize,
) -> Result<(), WorldRecipeRunError> {
    if value.is_empty()
        || value.trim() != value
        || value.len() > max_bytes
        || value.chars().any(char::is_control)
    {
        return Err(WorldRecipeRunError::Invalid(format!(
            "{label} is missing or malformed"
        )));
    }
    Ok(())
}

fn validate_fresh_input(input: &WorldRecipeRunInputValue) -> Result<(), WorldRecipeRunError> {
    match input {
        WorldRecipeRunInputValue::Text { text } => validate_secret_input("text", text, true),
        WorldRecipeRunInputValue::Selection { value } => {
            validate_secret_input("selection", value, false)
        }
        WorldRecipeRunInputValue::Key { key } => {
            if key.is_empty()
                || key.trim() != key
                || key.len() > 64
                || key.chars().any(char::is_control)
            {
                return Err(WorldRecipeRunError::Invalid(
                    "fresh key input is malformed".to_string(),
                ));
            }
            Ok(())
        }
        WorldRecipeRunInputValue::ScrollDelta { delta_y } => {
            if *delta_y == 0 || !(-10_000..=10_000).contains(delta_y) {
                return Err(WorldRecipeRunError::Invalid(
                    "fresh scroll delta must be between -10000 and 10000 and non-zero".to_string(),
                ));
            }
            Ok(())
        }
        WorldRecipeRunInputValue::WaitDuration { milliseconds } => {
            if !(1..=5_000).contains(milliseconds) {
                return Err(WorldRecipeRunError::Invalid(
                    "fresh wait duration must be between 1 and 5000 milliseconds".to_string(),
                ));
            }
            Ok(())
        }
    }
}

fn validate_secret_input(
    label: &str,
    value: &str,
    allow_empty: bool,
) -> Result<(), WorldRecipeRunError> {
    if (!allow_empty && value.is_empty())
        || value.len() > MAX_FRESH_INPUT_BYTES
        || value.contains('\0')
    {
        return Err(WorldRecipeRunError::Invalid(format!(
            "fresh {label} input is invalid or too large"
        )));
    }
    Ok(())
}

fn validate_operation_input(
    step_ordinal: u32,
    operation_index: u32,
    operation: &WorldRecipeOperation,
    input: Option<&WorldRecipeRunInputValue>,
) -> Result<(), WorldRecipeRunError> {
    let matches = matches!(
        (&operation.input_kind, input),
        (None, None)
            | (
                Some(WorldRecipeInputKind::Text),
                Some(WorldRecipeRunInputValue::Text { .. })
            )
            | (
                Some(WorldRecipeInputKind::Selection),
                Some(WorldRecipeRunInputValue::Selection { .. })
            )
            | (
                Some(WorldRecipeInputKind::Key),
                Some(WorldRecipeRunInputValue::Key { .. })
            )
            | (
                Some(WorldRecipeInputKind::ScrollDelta),
                Some(WorldRecipeRunInputValue::ScrollDelta { .. })
            )
            | (
                Some(WorldRecipeInputKind::WaitDuration),
                Some(WorldRecipeRunInputValue::WaitDuration { .. })
            )
    );
    if !matches {
        return Err(WorldRecipeRunError::Invalid(format!(
            "step {step_ordinal} operation {operation_index} requires exactly one matching fresh input"
        )));
    }
    Ok(())
}

fn validate_surface(surface: &str) -> Result<WorldSurfaceKind, WorldRecipeRunError> {
    match surface {
        "browser" => Ok(WorldSurfaceKind::Browser),
        "desktop" => Ok(WorldSurfaceKind::Desktop),
        "application" => Ok(WorldSurfaceKind::Application),
        "composite" => Ok(WorldSurfaceKind::Composite),
        _ => Err(WorldRecipeRunError::Conflict(format!(
            "recipe surface '{surface}' has no governed replay adapter"
        ))),
    }
}

fn validate_effect_class(effect: &str) -> Result<WorldEffectClass, WorldRecipeRunError> {
    match effect {
        "local_reversible" => Ok(WorldEffectClass::LocalReversible),
        "local_mutation" => Ok(WorldEffectClass::LocalMutation),
        _ => Err(WorldRecipeRunError::Conflict(format!(
            "recipe effect class '{effect}' is not executable by this runner"
        ))),
    }
}

struct DaemonRecipeStepExecutor<'a> {
    state: &'a AppState,
    owner_profile_id: String,
    authority_id: String,
    principal: WorldPrincipal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BrowserDestinationKind {
    Owned,
    Shared,
}

#[derive(Debug, Clone)]
struct BrowserDestination {
    kind: BrowserDestinationKind,
    driver_id: String,
    tab_group_id: String,
    tab_id: String,
}

#[async_trait]
impl RecipeStepExecutor for DaemonRecipeStepExecutor<'_> {
    async fn execute_step(
        &self,
        step: &PreparedRecipeStep,
        trace_id: &str,
    ) -> Result<ExecutedRecipeStep, RecipeStepFailure> {
        let session = self.resolve_world(step)?;
        match session.surface {
            WorldSurfaceKind::Browser => self.execute_browser_step(step, &session, trace_id).await,
            WorldSurfaceKind::Desktop
            | WorldSurfaceKind::Application
            | WorldSurfaceKind::Composite => {
                self.execute_computer_step(step, &session, trace_id).await
            }
            WorldSurfaceKind::Terminal => Err(RecipeStepFailure::before_dispatch(
                "unsupported_surface",
                "the selected world has no governed recipe adapter",
            )),
        }
    }
}

impl DaemonRecipeStepExecutor<'_> {
    fn resolve_world(&self, step: &PreparedRecipeStep) -> Result<WorldSession, RecipeStepFailure> {
        let world_id = WorldId::new(&step.target_world_id);
        let session = self
            .state
            .world_authority
            .read(|authority| {
                authority
                    .world(&world_id)
                    .map_err(|error| error.to_string())
            })
            .map_err(|_| {
                RecipeStepFailure::before_dispatch(
                    "world_not_found",
                    "the selected world is no longer available on this daemon",
                )
            })?;
        if session.authority_id.as_str() != self.authority_id {
            return Err(RecipeStepFailure::before_dispatch(
                "authority_mismatch",
                "the selected world belongs to another authority",
            ));
        }
        let recipe_surface = validate_surface(&step.recipe.surface).map_err(|_| {
            RecipeStepFailure::before_dispatch(
                "unsupported_surface",
                "the recipe step has no governed destination adapter",
            )
        })?;
        if session.surface != recipe_surface {
            return Err(RecipeStepFailure::before_dispatch(
                "surface_mismatch",
                "the selected world does not match the recipe step surface",
            ));
        }
        Ok(session)
    }

    async fn browser_destination(
        &self,
        session: &WorldSession,
    ) -> Result<BrowserDestination, RecipeStepFailure> {
        match session.ownership {
            WorldOwnership::Owned => {
                let world = self
                    .state
                    .isolated_browser
                    .get(&self.owner_profile_id, session.world_id.as_str())
                    .await
                    .map_err(|_| {
                        RecipeStepFailure::before_dispatch(
                            "world_unavailable",
                            "the selected isolated browser is unavailable to this profile",
                        )
                    })?;
                if world.authority_id != self.authority_id
                    || world.driver.driver_id != session.driver_id
                    || !world.agent_operable()
                {
                    return Err(RecipeStepFailure::before_dispatch(
                        "control_required",
                        "the selected isolated browser is not ready under agent control",
                    ));
                }
                let tab_id = world.tab_id.ok_or_else(|| {
                    RecipeStepFailure::before_dispatch(
                        "world_unavailable",
                        "the selected isolated browser has no active tab",
                    )
                })?;
                crate::world_authority::validate_browser_world_binding(
                    session.world_id.as_str(),
                    &self.authority_id,
                    world.driver.driver_id.as_str(),
                    &world.tab_group_id,
                )
                .map_err(|_| {
                    RecipeStepFailure::before_dispatch(
                        "world_mismatch",
                        "the selected isolated browser no longer matches its world binding",
                    )
                })?;
                Ok(BrowserDestination {
                    kind: BrowserDestinationKind::Owned,
                    driver_id: world.driver.driver_id.to_string(),
                    tab_group_id: world.tab_group_id,
                    tab_id,
                })
            }
            WorldOwnership::Managed | WorldOwnership::Attached => {
                let context = browser_host_current_context().await.map_err(|_| {
                    RecipeStepFailure::before_dispatch(
                        "world_unavailable",
                        "the selected shared browser is not attached",
                    )
                })?;
                validate_shared_browser_context(session, &self.authority_id, &context)?;
                if context.control != "agent" {
                    return Err(RecipeStepFailure::before_dispatch(
                        "control_required",
                        "the selected shared browser is not under agent control",
                    ));
                }
                Ok(BrowserDestination {
                    kind: BrowserDestinationKind::Shared,
                    driver_id: context.driver_id,
                    tab_group_id: context.tab_group_id,
                    tab_id: context.tab_id,
                })
            }
        }
    }

    async fn execute_browser_step(
        &self,
        step: &PreparedRecipeStep,
        session: &WorldSession,
        trace_id: &str,
    ) -> Result<ExecutedRecipeStep, RecipeStepFailure> {
        validate_browser_step_shape(step)?;
        let destination = self.browser_destination(session).await?;
        let observation_admission = crate::world_authority::admit_browser_observation(
            &self.authority_id,
            &destination.driver_id,
            &destination.tab_group_id,
            &destination.tab_id,
            trace_id,
            "recipe runner refreshed browser semantics",
        )
        .map_err(|_| {
            RecipeStepFailure::before_dispatch(
                "observation_denied",
                "fresh browser observation was denied",
            )
        })?;
        let observation = match destination.kind {
            BrowserDestinationKind::Owned => self
                .state
                .isolated_browser
                .observe(
                    &self.owner_profile_id,
                    session.world_id.as_str(),
                    None,
                    MAX_BROWSER_RECIPE_NODES,
                )
                .await
                .map_err(|_| {
                    let _ = crate::world_authority::fail_browser_action(
                        &observation_admission,
                        "recipe browser observation failed",
                    );
                    RecipeStepFailure::before_dispatch(
                        "observation_failed",
                        "fresh browser observation failed",
                    )
                })?,
            BrowserDestinationKind::Shared => {
                browser_host_observe(&destination.tab_group_id, None, MAX_BROWSER_RECIPE_NODES)
                    .await
                    .map_err(|_| {
                        let _ = crate::world_authority::fail_browser_action(
                            &observation_admission,
                            "recipe browser observation failed",
                        );
                        RecipeStepFailure::before_dispatch(
                            "observation_failed",
                            "fresh browser observation failed",
                        )
                    })?
            }
        };
        validate_fresh_browser_observation(&destination, &observation).inspect_err(|_| {
            let _ = crate::world_authority::fail_browser_action(
                &observation_admission,
                "recipe browser observation was incomplete",
            );
        })?;
        crate::world_authority::record_browser_observation(
            &observation_admission,
            observation.clone(),
        )
        .map_err(|_| {
            let _ = crate::world_authority::fail_browser_action(
                &observation_admission,
                "recipe browser mirror rejected observation",
            );
            RecipeStepFailure::before_dispatch(
                "observation_stale",
                "the fresh browser observation could not establish an exact action fence",
            )
        })?;
        crate::world_authority::complete_browser_action(
            &observation_admission,
            "recipe browser observation committed",
        )
        .map_err(|_| {
            RecipeStepFailure::before_dispatch(
                "observation_commit_failed",
                "the fresh browser observation could not be committed",
            )
        })?;

        let (mut body, recipe_hint, allow_high_risk) =
            build_browser_action(&self.authority_id, &destination, &observation, step)?;
        let action_admission = crate::world_authority::admit_browser_action(
            crate::world_authority::BrowserWorldActionRequest {
                authority_id: &self.authority_id,
                driver_id: &destination.driver_id,
                tab_group_id: &destination.tab_group_id,
                tab_id: &destination.tab_id,
                trace_id,
                summary: "recipe runner requested a freshly resolved browser step",
                effect_class: validate_effect_class(&step.recipe.effect_class).map_err(|_| {
                    RecipeStepFailure::before_dispatch(
                        "effect_mismatch",
                        "the recipe effect class is unsupported",
                    )
                })?,
                recipe_hint: Some(recipe_hint),
            },
        )
        .map_err(|_| {
            RecipeStepFailure::before_dispatch(
                "admission_denied",
                "fresh browser action admission was denied",
            )
        })?;
        body["world_permit"] = serde_json::to_value(&action_admission.permit).map_err(|_| {
            let _ = crate::world_authority::fail_browser_action(
                &action_admission,
                "recipe browser permit encoding failed",
            );
            RecipeStepFailure::before_dispatch(
                "permit_encoding_failed",
                "the fresh browser action permit could not be encoded",
            )
        })?;
        body["world_expected_url"] = json!(observation.url);
        body["document_id"] = json!(observation.document_id);
        body["observation_revision"] = json!(observation.revision);
        body["allow_high_risk"] = json!(allow_high_risk);

        let outcome = match destination.kind {
            BrowserDestinationKind::Owned => self
                .state
                .isolated_browser
                .act_for_driver(&self.owner_profile_id, &destination.driver_id, body)
                .await
                .map_err(|_| {
                    let _ = crate::world_authority::mark_browser_action_indeterminate(
                        &action_admission,
                        "isolated browser result unavailable during recipe run",
                    );
                    RecipeStepFailure::indeterminate(
                        "action_indeterminate",
                        "the browser result was unavailable after dispatch; inspect the run trace",
                    )
                })?,
            BrowserDestinationKind::Shared => browser_host_act(&destination.tab_group_id, body)
                .await
                .map_err(|_| {
                    let _ = crate::world_authority::mark_browser_action_indeterminate(
                        &action_admission,
                        "shared browser result unavailable during recipe run",
                    );
                    RecipeStepFailure::indeterminate(
                        "action_indeterminate",
                        "the browser result was unavailable after dispatch; inspect the run trace",
                    )
                })?,
        };
        finish_browser_step(&action_admission, outcome)
    }

    async fn execute_computer_step(
        &self,
        step: &PreparedRecipeStep,
        session: &WorldSession,
        trace_id: &str,
    ) -> Result<ExecutedRecipeStep, RecipeStepFailure> {
        if step.operations.len() != 1 {
            return Err(RecipeStepFailure::before_dispatch(
                "unsupported_batch",
                "native computer recipes currently require one operation per step",
            ));
        }
        if validate_effect_class(&step.recipe.effect_class).ok()
            != Some(WorldEffectClass::LocalMutation)
        {
            return Err(RecipeStepFailure::before_dispatch(
                "effect_mismatch",
                "native computer actions require a local mutation recipe step",
            ));
        }
        let driver_id = self
            .state
            .computer_drivers
            .resolve_world_driver(session.world_id.as_str(), &self.authority_id)
            .await
            .map_err(|_| {
                RecipeStepFailure::before_dispatch(
                    "world_unavailable",
                    "the selected computer driver could not be resolved",
                )
            })?
            .ok_or_else(|| {
                RecipeStepFailure::before_dispatch(
                    "world_mismatch",
                    "the selected computer world does not match a local driver",
                )
            })?;
        if driver_id != session.driver_id {
            return Err(RecipeStepFailure::before_dispatch(
                "world_mismatch",
                "the selected computer world changed drivers",
            ));
        }
        let preflight = self
            .state
            .computer_drivers
            .preflight(&driver_id)
            .await
            .map_err(|_| {
                RecipeStepFailure::before_dispatch(
                    "driver_unavailable",
                    "the native computer driver failed preflight",
                )
            })?;
        crate::computer_driver::validate_computer_world_binding(
            session.world_id.as_str(),
            &self.authority_id,
            &driver_id,
            &preflight.session_id,
        )
        .map_err(|_| {
            RecipeStepFailure::before_dispatch(
                "world_mismatch",
                "the selected computer world no longer matches its desktop session",
            )
        })?;
        let resource_id = desktop_resource_id(&driver_id, &preflight.session_id);
        let observed = self
            .state
            .computer_drivers
            .observe(ComputerObservationIntent {
                authority_id: self.authority_id.clone(),
                driver_id: driver_id.clone(),
                desktop_session_id: preflight.session_id.clone(),
                principal: self.principal.clone(),
                resource_id: resource_id.clone(),
                trace_id: trace_id.to_string(),
                summary: "recipe runner refreshed native computer semantics".to_string(),
                after_revision: None,
                max_nodes: MAX_COMPUTER_RECIPE_NODES,
            })
            .await
            .map_err(|_| {
                RecipeStepFailure::before_dispatch(
                    "observation_failed",
                    "fresh computer observation failed",
                )
            })?;
        if !observed.observation.full || observed.observation.truncated {
            return Err(RecipeStepFailure::before_dispatch(
                "observation_incomplete",
                "the computer observation was incomplete; no action was dispatched",
            ));
        }
        let operation = &step.operations[0];
        let action = computer_action_for(operation)?;
        let node = resolve_computer_target(&operation.recipe, &observed.observation.nodes)?;
        if !node.enabled || node.sensitive || !node.actions.contains(&action) {
            return Err(RecipeStepFailure::before_dispatch(
                "target_unavailable",
                "the exact computer target is disabled, sensitive, or lacks the requested action",
            ));
        }
        if crate::computer_driver::computer_semantic_action_requires_operator_confirmation(
            action, node,
        ) && !operation.confirmed
        {
            return Err(RecipeStepFailure::before_dispatch(
                "confirmation_required",
                "the freshly resolved computer target requires operator confirmation",
            ));
        }
        let value = match operation.input.as_ref() {
            Some(WorldRecipeRunInputValue::Text { text }) => Some(text.clone()),
            None => None,
            _ => {
                return Err(RecipeStepFailure::before_dispatch(
                    "input_mismatch",
                    "the computer operation received an incompatible fresh input",
                ));
            }
        };
        let result = self
            .state
            .computer_drivers
            .act(ComputerActionIntent {
                authority_id: self.authority_id.clone(),
                driver_id,
                desktop_session_id: preflight.session_id,
                principal: self.principal.clone(),
                resource_id,
                trace_id: trace_id.to_string(),
                summary: format!(
                    "recipe runner requested native computer action {}",
                    action.as_str()
                ),
                observation_generation: observed.observation.observation_generation,
                observation_revision: observed.observation.revision,
                element_ref: node.element_ref.clone(),
                action,
                value,
                allow_high_risk: operation.confirmed,
            })
            .await
            .map_err(|_| {
                RecipeStepFailure::indeterminate(
                    "action_indeterminate",
                    "the computer action did not return a confirmed receipt; inspect the run trace",
                )
            })?;
        let outcome = result.provenance.outcome;
        if outcome.status != WorldActionStatus::Confirmed {
            return Err(RecipeStepFailure::indeterminate(
                "action_unconfirmed",
                "the computer action was not confirmed; inspect the run trace",
            ));
        }
        Ok(ExecutedRecipeStep {
            intent_id: outcome.intent_id.to_string(),
            committed_revision: outcome.committed_revision,
            completion_sequence: outcome.event_sequence,
        })
    }
}

fn validate_shared_browser_context(
    session: &WorldSession,
    authority_id: &str,
    context: &BrowserHostWorldContext,
) -> Result<(), RecipeStepFailure> {
    crate::world_authority::validate_browser_world_binding(
        session.world_id.as_str(),
        authority_id,
        &context.driver_id,
        &context.tab_group_id,
    )
    .map_err(|_| {
        RecipeStepFailure::before_dispatch(
            "world_mismatch",
            "the selected shared browser is not the BrowserHost's exact current world",
        )
    })?;
    if session.driver_id.as_str() != context.driver_id {
        return Err(RecipeStepFailure::before_dispatch(
            "world_mismatch",
            "the selected shared browser changed drivers",
        ));
    }
    Ok(())
}

fn validate_fresh_browser_observation(
    destination: &BrowserDestination,
    observation: &BrowserObservation,
) -> Result<(), RecipeStepFailure> {
    if observation.tab_id != destination.tab_id
        || !observation.full
        || observation.base_revision.is_some()
        || observation.truncated
    {
        return Err(RecipeStepFailure::before_dispatch(
            "observation_incomplete",
            "the browser observation was stale or incomplete; no action was dispatched",
        ));
    }
    Ok(())
}

fn validate_browser_step_shape(step: &PreparedRecipeStep) -> Result<(), RecipeStepFailure> {
    let expected = validate_effect_class(&step.recipe.effect_class).map_err(|_| {
        RecipeStepFailure::before_dispatch(
            "effect_mismatch",
            "the browser recipe effect class is unsupported",
        )
    })?;
    let actual = if step.operations.iter().any(|operation| {
        matches!(
            operation.recipe.verb.as_str(),
            "click" | "type" | "press" | "select"
        )
    }) {
        WorldEffectClass::LocalMutation
    } else if step
        .operations
        .iter()
        .all(|operation| matches!(operation.recipe.verb.as_str(), "scroll" | "wait"))
    {
        WorldEffectClass::LocalReversible
    } else {
        return Err(RecipeStepFailure::before_dispatch(
            "unsupported_operation",
            "the browser recipe contains an unsupported semantic operation",
        ));
    };
    if actual != expected {
        return Err(RecipeStepFailure::before_dispatch(
            "effect_mismatch",
            "the browser recipe operations do not match their effect class",
        ));
    }
    Ok(())
}

fn build_browser_action(
    authority_id: &str,
    destination: &BrowserDestination,
    observation: &BrowserObservation,
    step: &PreparedRecipeStep,
) -> Result<(Value, WorldActionRecipeHint, bool), RecipeStepFailure> {
    let mut action_bodies = Vec::with_capacity(step.operations.len());
    let mut hints = Vec::with_capacity(step.operations.len());
    let mut allow_high_risk = false;
    for operation in &step.operations {
        let node = resolve_browser_target(&operation.recipe, &observation.nodes)?;
        let mut body = browser_operation_body(operation, node)?;
        let (target_role, target_name, requires_current_confirmation) = if let Some(node) = node {
            let targets = vec![(operation.recipe.verb.clone(), node.element_ref.clone())];
            let resolved = crate::world_authority::validate_browser_element_refs(
                crate::world_authority::BrowserElementRefFence {
                    authority_id,
                    driver_id: &destination.driver_id,
                    tab_group_id: &destination.tab_group_id,
                    tab_id: &destination.tab_id,
                    expected_url: &observation.url,
                    document_id: &observation.document_id,
                    revision: observation.revision,
                    targets: &targets,
                    allow_high_risk: true,
                },
            )
            .map_err(|_| {
                RecipeStepFailure::before_dispatch(
                    "target_stale",
                    "the exact browser target failed its fresh semantic fence",
                )
            })?;
            let resolved = resolved.first().ok_or_else(|| {
                RecipeStepFailure::before_dispatch(
                    "target_stale",
                    "the exact browser target disappeared before admission",
                )
            })?;
            if resolved.requires_operator_confirmation && !operation.confirmed {
                return Err(RecipeStepFailure::before_dispatch(
                    "confirmation_required",
                    "the freshly resolved browser target requires operator confirmation",
                ));
            }
            allow_high_risk |= resolved.requires_operator_confirmation;
            body["target_ref"] = json!(node.element_ref);
            body["guard"] = json!({
                "role": node.role,
                "name": node.name,
            });
            (
                resolved.role.clone(),
                resolved.name.clone(),
                resolved.requires_operator_confirmation,
            )
        } else {
            (None, None, false)
        };
        hints.push(WorldRecipeOperationHint {
            verb: operation.recipe.verb.clone(),
            target_role,
            target_name,
            input_kind: operation.recipe.input_kind.map(authority_input_kind),
            requires_operator_confirmation: requires_current_confirmation,
        });
        action_bodies.push(body);
    }
    let body = if action_bodies.len() == 1 {
        action_bodies.pop().expect("one browser action body")
    } else {
        json!({ "actions": action_bodies })
    };
    Ok((
        body,
        WorldActionRecipeHint {
            schema_version: WORLD_ACTION_RECIPE_HINT_SCHEMA_VERSION,
            operations: hints,
        },
        allow_high_risk,
    ))
}

fn browser_operation_body(
    operation: &PreparedRecipeOperation,
    target: Option<&BrowserSemanticNode>,
) -> Result<Value, RecipeStepFailure> {
    let mut body = json!({ "action": operation.recipe.verb });
    match (&operation.recipe.verb[..], operation.input.as_ref()) {
        ("click", None) if target.is_some() => {}
        ("type", Some(WorldRecipeRunInputValue::Text { text })) if target.is_some() => {
            body["text"] = json!(text);
        }
        ("press", Some(WorldRecipeRunInputValue::Key { key })) if target.is_some() => {
            body["key"] = json!(key);
        }
        ("select", Some(WorldRecipeRunInputValue::Selection { value })) if target.is_some() => {
            body["value"] = json!(value);
        }
        ("scroll", Some(WorldRecipeRunInputValue::ScrollDelta { delta_y })) if target.is_none() => {
            body["delta_y"] = json!(delta_y);
        }
        ("wait", Some(WorldRecipeRunInputValue::WaitDuration { milliseconds }))
            if target.is_none() =>
        {
            body["ms"] = json!(milliseconds);
        }
        _ => {
            return Err(RecipeStepFailure::before_dispatch(
                "operation_mismatch",
                "the browser operation shape no longer matches the governed adapter",
            ));
        }
    }
    Ok(body)
}

fn resolve_browser_target<'a>(
    operation: &WorldRecipeOperation,
    nodes: &'a [BrowserSemanticNode],
) -> Result<Option<&'a BrowserSemanticNode>, RecipeStepFailure> {
    let targeted = matches!(
        operation.verb.as_str(),
        "click" | "type" | "press" | "select"
    );
    if !targeted {
        if operation.target_role.is_some() || operation.target_name.is_some() {
            return Err(RecipeStepFailure::before_dispatch(
                "operation_mismatch",
                "the untargeted browser operation unexpectedly carries target semantics",
            ));
        }
        return Ok(None);
    }
    if operation.target_role.is_none() && operation.target_name.is_none() {
        return Err(RecipeStepFailure::before_dispatch(
            "target_missing",
            "the browser recipe has no stable semantic target",
        ));
    }
    let mut matches = nodes.iter().filter(|node| {
        operation
            .target_role
            .as_deref()
            .is_none_or(|role| node.role == role)
            && operation
                .target_name
                .as_deref()
                .is_none_or(|name| node.name == name)
    });
    let node = matches.next().ok_or_else(|| {
        RecipeStepFailure::before_dispatch(
            "target_not_found",
            "no browser element exactly matches the recipe semantics",
        )
    })?;
    if matches.next().is_some() {
        return Err(RecipeStepFailure::before_dispatch(
            "target_ambiguous",
            "more than one browser element exactly matches the recipe semantics",
        ));
    }
    if node.disabled || node.sensitive {
        return Err(RecipeStepFailure::before_dispatch(
            "target_unavailable",
            "the exact browser target is disabled or sensitive",
        ));
    }
    Ok(Some(node))
}

fn finish_browser_step(
    admission: &crate::world_authority::BrowserWorldAdmission,
    mut driver_outcome: Value,
) -> Result<ExecutedRecipeStep, RecipeStepFailure> {
    if !driver_outcome
        .get("ok")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        let code = driver_outcome
            .get("code")
            .and_then(Value::as_str)
            .unwrap_or("act_failed");
        let applied = driver_outcome
            .get("executed_steps")
            .and_then(Value::as_u64)
            .is_some_and(|count| count > 0)
            || matches!(code, "act_failed" | "stale_surface_lease" | "batch_partial");
        if applied {
            let _ = crate::world_authority::mark_browser_action_indeterminate(
                admission,
                "browser recipe step could not prove its outcome",
            );
            return Err(RecipeStepFailure::indeterminate(
                "action_indeterminate",
                "the browser step may have partially applied; inspect the run trace",
            ));
        }
        let _ = crate::world_authority::fail_browser_action(
            admission,
            "browser recipe step was rejected before applying",
        );
        return Err(RecipeStepFailure::before_dispatch(
            "action_rejected",
            "the browser driver rejected the step without reporting applied operations",
        ));
    }
    if let Some(raw_observation) = driver_outcome.get_mut("observation").map(Value::take) {
        let observation =
            serde_json::from_value::<BrowserObservation>(raw_observation).map_err(|_| {
                let _ = crate::world_authority::mark_browser_action_indeterminate(
                    admission,
                    "browser recipe result contained an invalid observation",
                );
                RecipeStepFailure::indeterminate(
                    "observation_invalid",
                    "the browser acted but its resulting observation was invalid",
                )
            })?;
        crate::world_authority::record_browser_observation(admission, observation).map_err(
            |_| {
                let _ = crate::world_authority::mark_browser_action_indeterminate(
                    admission,
                    "browser recipe result could not advance the semantic mirror",
                );
                RecipeStepFailure::indeterminate(
                    "observation_stale",
                    "the browser acted but its resulting state could not be committed",
                )
            },
        )?;
    }
    let outcome =
        crate::world_authority::complete_browser_action(admission, "browser recipe step confirmed")
            .map_err(|_| {
                RecipeStepFailure::indeterminate(
                    "action_commit_failed",
                    "the browser acted but its authoritative receipt could not be committed",
                )
            })?;
    if outcome.status != WorldActionStatus::Confirmed {
        return Err(RecipeStepFailure::indeterminate(
            "action_unconfirmed",
            "the browser action was not confirmed; inspect the run trace",
        ));
    }
    Ok(ExecutedRecipeStep {
        intent_id: outcome.intent_id.to_string(),
        committed_revision: outcome.committed_revision,
        completion_sequence: outcome.event_sequence,
    })
}

fn computer_action_for(
    operation: &PreparedRecipeOperation,
) -> Result<ComputerAction, RecipeStepFailure> {
    let action = match operation.recipe.verb.as_str() {
        "press" => ComputerAction::Press,
        "focus" => ComputerAction::Focus,
        "set_value" => ComputerAction::SetValue,
        "show_menu" => ComputerAction::ShowMenu,
        "increment" => ComputerAction::Increment,
        "decrement" => ComputerAction::Decrement,
        "scroll_to_visible" => ComputerAction::ScrollToVisible,
        "foreground_click" => ComputerAction::ForegroundClick,
        _ => {
            return Err(RecipeStepFailure::before_dispatch(
                "unsupported_operation",
                "the computer recipe contains an unsupported semantic operation",
            ));
        }
    };
    let input_matches = matches!(
        (action, operation.input.as_ref()),
        (
            ComputerAction::SetValue,
            Some(WorldRecipeRunInputValue::Text { .. })
        ) | (
            ComputerAction::Press
                | ComputerAction::Focus
                | ComputerAction::ShowMenu
                | ComputerAction::Increment
                | ComputerAction::Decrement
                | ComputerAction::ScrollToVisible
                | ComputerAction::ForegroundClick,
            None
        )
    );
    if !input_matches {
        return Err(RecipeStepFailure::before_dispatch(
            "operation_mismatch",
            "the computer operation shape no longer matches the governed adapter",
        ));
    }
    Ok(action)
}

fn resolve_computer_target<'a>(
    operation: &WorldRecipeOperation,
    nodes: &'a [ComputerSemanticNode],
) -> Result<&'a ComputerSemanticNode, RecipeStepFailure> {
    if operation.target_role.is_none() && operation.target_name.is_none() {
        return Err(RecipeStepFailure::before_dispatch(
            "target_missing",
            "the computer recipe has no stable semantic target",
        ));
    }
    let mut matches = nodes.iter().filter(|node| {
        operation
            .target_role
            .as_deref()
            .is_none_or(|role| node.role == role)
            && operation
                .target_name
                .as_deref()
                .is_none_or(|name| node.name == name)
    });
    let node = matches.next().ok_or_else(|| {
        RecipeStepFailure::before_dispatch(
            "target_not_found",
            "no computer element exactly matches the recipe semantics",
        )
    })?;
    if matches.next().is_some() {
        return Err(RecipeStepFailure::before_dispatch(
            "target_ambiguous",
            "more than one computer element exactly matches the recipe semantics",
        ));
    }
    Ok(node)
}

fn authority_input_kind(kind: WorldRecipeInputKind) -> AuthorityRecipeInputKind {
    match kind {
        WorldRecipeInputKind::Text => AuthorityRecipeInputKind::Text,
        WorldRecipeInputKind::Selection => AuthorityRecipeInputKind::Selection,
        WorldRecipeInputKind::Key => AuthorityRecipeInputKind::Key,
        WorldRecipeInputKind::ScrollDelta => AuthorityRecipeInputKind::ScrollDelta,
        WorldRecipeInputKind::WaitDuration => AuthorityRecipeInputKind::WaitDuration,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use medousa_browser_bridge::BrowserSemanticNode;
    use medousa_types::{WorldRecipeRunConfirmation, WorldRecipeRunInput, WorldRecipeRunTarget};

    use super::*;

    fn operation(
        verb: &str,
        role: Option<&str>,
        name: Option<&str>,
        input_kind: Option<WorldRecipeInputKind>,
        confirmation: bool,
    ) -> WorldRecipeOperation {
        WorldRecipeOperation {
            verb: verb.to_string(),
            target_role: role.map(str::to_string),
            target_name: name.map(str::to_string),
            input_kind,
            requires_operator_confirmation: confirmation,
        }
    }

    fn recipe(steps: Vec<WorldRecipeStep>) -> WorldRecipe {
        WorldRecipe {
            schema_version: WORLD_RECIPE_SCHEMA_VERSION,
            recipe_id: "world-recipe:sha256:test".to_string(),
            source_trace_id: "source:trace".to_string(),
            source_start_sequence: 1,
            source_end_sequence: 8,
            steps,
            execution_model: "fresh_semantic_replay".to_string(),
            carries_authority: false,
            automatic_dispatch_allowed: false,
        }
    }

    fn step(
        ordinal: u32,
        effect_class: &str,
        operations: Vec<WorldRecipeOperation>,
    ) -> WorldRecipeStep {
        WorldRecipeStep {
            ordinal,
            source_admission_sequence: u64::from(ordinal) * 2,
            source_completion_sequence: u64::from(ordinal) * 2 + 1,
            source_intent_id: format!("intent:source:{ordinal}"),
            surface: "browser".to_string(),
            effect_class: effect_class.to_string(),
            operations,
            requires_fresh_observation: true,
            requires_fresh_admission: true,
            requires_operator_confirmation: false,
        }
    }

    fn request(target_ordinals: &[u32]) -> WorldRecipeRunRequest {
        WorldRecipeRunRequest {
            recipe_id: "world-recipe:sha256:test".to_string(),
            source_trace_id: "source:trace".to_string(),
            run_id: "run-test-1".to_string(),
            operator_approved: true,
            targets: target_ordinals
                .iter()
                .map(|ordinal| WorldRecipeRunTarget {
                    step_ordinal: *ordinal,
                    world_id: format!("world:target:{ordinal}"),
                })
                .collect(),
            inputs: Vec::new(),
            confirmations: Vec::new(),
        }
    }

    fn browser_node(element_ref: &str, role: &str, name: &str) -> BrowserSemanticNode {
        BrowserSemanticNode {
            element_ref: element_ref.to_string(),
            parent_ref: None,
            role: role.to_string(),
            name: name.to_string(),
            tag: "button".to_string(),
            value: None,
            href: None,
            disabled: false,
            checked: None,
            selected: None,
            bounds: None,
            sensitive: false,
        }
    }

    struct CountingExecutor {
        calls: AtomicUsize,
        fail_on: Option<u32>,
    }

    #[async_trait]
    impl RecipeStepExecutor for CountingExecutor {
        async fn execute_step(
            &self,
            step: &PreparedRecipeStep,
            _trace_id: &str,
        ) -> Result<ExecutedRecipeStep, RecipeStepFailure> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail_on == Some(step.recipe.ordinal) {
                return Err(RecipeStepFailure::before_dispatch(
                    "test_stop",
                    "test runner stopped",
                ));
            }
            Ok(ExecutedRecipeStep {
                intent_id: format!("intent:run:{}", step.recipe.ordinal),
                committed_revision: u64::from(step.recipe.ordinal),
                completion_sequence: u64::from(step.recipe.ordinal) * 10,
            })
        }
    }

    #[test]
    fn prepare_requires_separate_fresh_input_and_confirmation() {
        let recipe = recipe(vec![step(
            1,
            "local_mutation",
            vec![operation(
                "type",
                Some("textbox"),
                Some("Message"),
                Some(WorldRecipeInputKind::Text),
                true,
            )],
        )]);
        let mut request = request(&[1]);
        request.inputs.push(WorldRecipeRunInput {
            step_ordinal: 1,
            operation_index: 0,
            input: WorldRecipeRunInputValue::Text {
                text: "fresh-private-value".to_string(),
            },
        });
        let error = prepare_recipe_run(&recipe, request.clone()).unwrap_err();
        assert!(matches!(error, WorldRecipeRunError::Invalid(_)));

        request.confirmations.push(WorldRecipeRunConfirmation {
            step_ordinal: 1,
            operation_index: 0,
        });
        let prepared = prepare_recipe_run(&recipe, request).expect("prepared run");
        assert_eq!(prepared.steps.len(), 1);
        assert!(prepared.steps[0].operations[0].confirmed);
    }

    #[tokio::test]
    async fn execution_stops_before_later_steps_and_never_returns_fresh_values() {
        let recipe = recipe(vec![
            step(
                1,
                "local_mutation",
                vec![operation(
                    "type",
                    Some("textbox"),
                    Some("Message"),
                    Some(WorldRecipeInputKind::Text),
                    false,
                )],
            ),
            step(
                2,
                "local_reversible",
                vec![operation(
                    "wait",
                    None,
                    None,
                    Some(WorldRecipeInputKind::WaitDuration),
                    false,
                )],
            ),
            step(
                3,
                "local_reversible",
                vec![operation(
                    "wait",
                    None,
                    None,
                    Some(WorldRecipeInputKind::WaitDuration),
                    false,
                )],
            ),
        ]);
        let mut request = request(&[1, 2, 3]);
        request.inputs = vec![
            WorldRecipeRunInput {
                step_ordinal: 1,
                operation_index: 0,
                input: WorldRecipeRunInputValue::Text {
                    text: "fresh-private-value".to_string(),
                },
            },
            WorldRecipeRunInput {
                step_ordinal: 2,
                operation_index: 0,
                input: WorldRecipeRunInputValue::WaitDuration { milliseconds: 10 },
            },
            WorldRecipeRunInput {
                step_ordinal: 3,
                operation_index: 0,
                input: WorldRecipeRunInputValue::WaitDuration { milliseconds: 10 },
            },
        ];
        let prepared = prepare_recipe_run(&recipe, request).expect("prepared run");
        let executor = CountingExecutor {
            calls: AtomicUsize::new(0),
            fail_on: Some(2),
        };
        let response = execute_prepared_recipe(&prepared, &executor).await;
        assert_eq!(executor.calls.load(Ordering::SeqCst), 2);
        assert_eq!(response.status, WorldRecipeRunStatus::Stopped);
        assert_eq!(response.completed_steps, 1);
        assert_eq!(response.stop.as_ref().unwrap().step_ordinal, 2);
        let encoded = serde_json::to_string(&response).unwrap();
        assert!(!encoded.contains("fresh-private-value"));
    }

    #[test]
    fn browser_target_resolution_is_exact_and_unique() {
        let exact = operation("click", Some("button"), Some("Publish"), None, false);
        let nodes = vec![
            browser_node("ref:1", "button", "Publish"),
            browser_node("ref:2", "button", "Publish draft"),
        ];
        assert_eq!(
            resolve_browser_target(&exact, &nodes)
                .unwrap()
                .unwrap()
                .element_ref,
            "ref:1"
        );

        let case_changed = operation("click", Some("button"), Some("publish"), None, false);
        assert_eq!(
            resolve_browser_target(&case_changed, &nodes)
                .unwrap_err()
                .code,
            "target_not_found"
        );

        let duplicate_nodes = vec![
            browser_node("ref:1", "button", "Publish"),
            browser_node("ref:3", "button", "Publish"),
        ];
        assert_eq!(
            resolve_browser_target(&exact, &duplicate_nodes)
                .unwrap_err()
                .code,
            "target_ambiguous"
        );
    }

    #[test]
    fn request_cannot_smuggle_input_into_an_input_free_operation() {
        let recipe = recipe(vec![step(
            1,
            "local_mutation",
            vec![operation(
                "click",
                Some("button"),
                Some("Continue"),
                None,
                false,
            )],
        )]);
        let mut request = request(&[1]);
        request.inputs.push(WorldRecipeRunInput {
            step_ordinal: 1,
            operation_index: 0,
            input: WorldRecipeRunInputValue::Text {
                text: "not-allowed".to_string(),
            },
        });
        assert!(matches!(
            prepare_recipe_run(&recipe, request),
            Err(WorldRecipeRunError::Invalid(_))
        ));
    }
}
