//! Daemon-owned broker for native computer drivers.
//!
//! Platform sidecars only sense and execute. This broker binds an exact driver
//! to the shared world authority, admits observation intent, revalidates its
//! permit immediately before dispatch, and records the resulting effect.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use medousa_computer_bridge::{
    COMPUTER_DRIVER_PROTOCOL_VERSION, ComputerAction, ComputerActionReceipt,
    ComputerActionRequest, ComputerDriverPreflight, ComputerObservation,
    ComputerObservationRequest, ComputerPermissionKind, ComputerScreenshotCapture,
    ComputerScreenshotRequest,
};
use medousa_world::{
    WorldActionIntent, WorldActionOutcome, WorldActionPermit, WorldAdmission, WorldAuthorityError,
    WorldAuthorityId, WorldCapability, WorldDriverCapability, WorldDriverId, WorldDriverKind,
    WorldDriverRegistration, WorldDriverTransport, WorldEffectClass, WorldGrantId,
    WorldGrantRequest, WorldId, WorldPrincipal, WorldPrincipalId, WorldResourceId,
    WorldResourceScope, WorldSessionSpec, WorldSurfaceKind, WorldTraceId,
};
use serde::Serialize;
use sha2::{Digest as _, Sha256};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::world_authority::WorldAuthorityService;

const COMPUTER_OBSERVATION_PERMIT_MS: u64 = 10_000;
const COMPUTER_ACTION_PERMIT_MS: u64 = 10_000;

#[async_trait]
pub trait ComputerDriver: Send + Sync {
    fn registration(&self) -> WorldDriverRegistration;

    /// Read-only permission/status probe. Implementations must never display a
    /// platform permission prompt from this method.
    async fn preflight(&self) -> Result<ComputerDriverPreflight, String>;

    async fn observe(
        &self,
        request: ComputerObservationRequest,
    ) -> Result<ComputerObservation, String>;

    async fn screenshot(
        &self,
        request: ComputerScreenshotRequest,
    ) -> Result<ComputerScreenshotCapture, String>;

    async fn act(
        &self,
        request: ComputerActionRequest,
    ) -> Result<ComputerActionReceipt, ComputerDriverActionError>;
}

#[derive(Debug, Clone)]
pub struct ComputerDriverActionError {
    pub message: String,
    /// The driver may have acted, but its acknowledgement was lost or invalid.
    pub indeterminate: bool,
}

impl ComputerDriverActionError {
    pub fn failed(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            indeterminate: false,
        }
    }

    pub fn indeterminate(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            indeterminate: true,
        }
    }
}

#[derive(Clone)]
struct RegisteredComputerDriver {
    registration: WorldDriverRegistration,
    desktop_session_id: String,
    driver: Arc<dyn ComputerDriver>,
}

#[derive(Clone)]
pub struct ComputerDriverBroker {
    authority: Arc<WorldAuthorityService>,
    drivers: Arc<RwLock<BTreeMap<WorldDriverId, RegisteredComputerDriver>>>,
    observation_fences:
        Arc<RwLock<BTreeMap<(WorldDriverId, WorldResourceId), ComputerObservationFence>>>,
}

#[derive(Debug, Clone)]
struct ComputerObservationFence {
    session_id: String,
    generation: String,
    revision: u64,
    focused_window_resource_id: Option<WorldResourceId>,
    elements: BTreeMap<String, ComputerObservedElement>,
}

#[derive(Debug, Clone)]
struct ComputerObservedElement {
    role: String,
    name: String,
    enabled: bool,
    sensitive: bool,
    actions: BTreeSet<ComputerAction>,
}

fn validate_pixel_fence(
    fence: Option<&ComputerObservationFence>,
    intent: &ComputerPixelObservationIntent,
) -> Result<(), String> {
    let fence = fence.ok_or_else(|| {
        "observation_required: observe the desktop before requesting pixels".to_string()
    })?;
    if fence.session_id != intent.desktop_session_id
        || fence.generation != intent.observation_generation
        || fence.revision != intent.observation_revision
    {
        return Err(
            "stale_observation: screenshot must target the daemon's latest exact observation"
                .to_string(),
        );
    }
    if fence.focused_window_resource_id.as_ref() != Some(&intent.window_resource_id) {
        return Err(
            "stale_observation: screenshot must target the exact observed focused window"
                .to_string(),
        );
    }
    Ok(())
}

impl ComputerDriverBroker {
    pub fn new(authority: Arc<WorldAuthorityService>) -> Self {
        Self {
            authority,
            drivers: Arc::new(RwLock::new(BTreeMap::new())),
            observation_fences: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    pub async fn register(&self, driver: Arc<dyn ComputerDriver>) -> Result<(), String> {
        let registration = driver.registration();
        validate_registration(&registration)?;
        let preflight = driver.preflight().await?;
        validate_preflight(&registration, &preflight)?;

        let driver_id = registration.driver_id.clone();
        let mut drivers = self.drivers.write().await;
        if drivers.contains_key(&driver_id) {
            return Err(format!("computer driver '{driver_id}' is already registered"));
        }
        drivers.insert(
            driver_id,
            RegisteredComputerDriver {
                registration,
                desktop_session_id: preflight.session_id,
                driver,
            },
        );
        Ok(())
    }

    pub async fn registrations(&self) -> Vec<WorldDriverRegistration> {
        self.drivers
            .read()
            .await
            .values()
            .map(|driver| driver.registration.clone())
            .collect()
    }

    pub async fn preflight(
        &self,
        driver_id: &WorldDriverId,
    ) -> Result<ComputerDriverPreflight, String> {
        let registered = self.driver(driver_id).await?;
        let report = registered.driver.preflight().await?;
        validate_preflight(&registered.registration, &report)?;
        if report.session_id != registered.desktop_session_id {
            return Err(
                "computer driver desktop session changed; register the driver again".to_string(),
            );
        }
        Ok(report)
    }

    pub async fn observe(
        &self,
        intent: ComputerObservationIntent,
    ) -> Result<GovernedComputerObservation, String> {
        intent.validate()?;
        let registered = self.driver(&intent.driver_id).await?;
        if !registered
            .registration
            .capabilities
            .contains(&WorldDriverCapability::SemanticObservation)
        {
            return Err(format!(
                "computer driver '{}' does not provide semantic observation",
                intent.driver_id
            ));
        }
        if intent.desktop_session_id != registered.desktop_session_id {
            return Err("computer observation requested the wrong desktop session".to_string());
        }

        let request = ComputerObservationRequest {
            resource_id: intent.resource_id.clone(),
            session_id: intent.desktop_session_id.clone(),
            after_revision: intent.after_revision,
            max_nodes: intent.max_nodes,
        };
        request.validate()?;
        let admission = self.admit_observation(&registered.registration, &intent)?;

        if let Err(error) = self.authority.read(|authority| {
            authority
                .validate_action_permit(&admission.permit, now_ms())
                .map_err(|error| error.to_string())
        }) {
            let _ = self.fail(&admission.permit, &error);
            return Err(error);
        }

        let observation = match registered.driver.observe(request.clone()).await {
            Ok(observation) => observation,
            Err(error) => {
                let _ = self.fail(&admission.permit, &error);
                return Err(error);
            }
        };
        if let Err(error) = observation.validate_for(&intent.driver_id, &request) {
            let _ = self.fail(&admission.permit, &error);
            return Err(error);
        }

        let outcome = self.authority.write(|authority| {
            authority
                .complete_action(
                    &admission.permit,
                    "native computer observation recorded",
                    now_ms(),
                )
                .map_err(|error| error.to_string())
        })?;
        self.remember_observation(&observation).await;
        Ok(GovernedComputerObservation {
            observation,
            provenance: ComputerWorldProvenance::from_permit(&admission.permit, outcome),
        })
    }

    pub async fn act(
        &self,
        intent: ComputerActionIntent,
    ) -> Result<GovernedComputerAction, String> {
        intent.validate()?;
        let registered = self.driver(&intent.driver_id).await?;
        if !registered
            .registration
            .capabilities
            .contains(&WorldDriverCapability::Interaction)
        {
            return Err(format!(
                "computer driver '{}' does not provide semantic interaction",
                intent.driver_id
            ));
        }
        if intent.desktop_session_id != registered.desktop_session_id {
            return Err("computer action requested the wrong desktop session".to_string());
        }
        if intent.action == ComputerAction::ForegroundClick {
            let preflight = registered.driver.preflight().await?;
            validate_preflight(&registered.registration, &preflight)?;
            if preflight.session_id != registered.desktop_session_id {
                return Err(
                    "computer driver desktop session changed; register the driver again"
                        .to_string(),
                );
            }
            if !preflight.foreground_input_ready() {
                let guidance = preflight
                    .permissions
                    .iter()
                    .find(|permission| {
                        permission.permission == ComputerPermissionKind::InputControl
                    })
                    .and_then(|permission| permission.guidance.as_deref())
                    .unwrap_or("Grant the native computer driver input-control permission.");
                return Err(format!("input_control_required: {guidance}"));
            }
        }

        let request = ComputerActionRequest {
            resource_id: intent.resource_id.clone(),
            session_id: intent.desktop_session_id.clone(),
            observation_generation: intent.observation_generation.clone(),
            observation_revision: intent.observation_revision,
            element_ref: intent.element_ref.clone(),
            action: intent.action,
            value: intent.value.clone(),
        };
        request.validate()?;
        self.validate_action_fence(&intent).await?;
        let admission = self.admit_action(&registered.registration, &intent)?;

        if let Err(error) = self.authority.read(|authority| {
            authority
                .validate_action_permit(&admission.permit, now_ms())
                .map_err(|error| error.to_string())
        }) {
            let _ = self.fail(&admission.permit, &error);
            return Err(error);
        }

        if let Err(error) = self.consume_action_fence(&intent).await {
            let _ = self.fail(&admission.permit, &error);
            return Err(error);
        }

        let receipt = match registered.driver.act(request.clone()).await {
            Ok(receipt) => receipt,
            Err(error) => {
                if error.indeterminate {
                    let _ = self.mark_indeterminate(&admission.permit, &error.message);
                } else {
                    let _ = self.fail(&admission.permit, &error.message);
                }
                return Err(error.message);
            }
        };
        if let Err(error) = receipt.validate_for(&intent.driver_id, &request) {
            let _ = self.mark_indeterminate(&admission.permit, &error);
            return Err(error);
        }

        let outcome = self.authority.write(|authority| {
            authority
                .complete_action(
                    &admission.permit,
                    format!(
                        "native {} acknowledged",
                        intent.action.as_str()
                    ),
                    now_ms(),
                )
                .map_err(|error| error.to_string())
        })?;
        Ok(GovernedComputerAction {
            receipt,
            provenance: ComputerWorldProvenance::from_permit(&admission.permit, outcome),
        })
    }

    pub async fn capture_pixels(
        &self,
        intent: ComputerPixelObservationIntent,
    ) -> Result<GovernedComputerScreenshot, String> {
        intent.validate()?;
        let registered = self.driver(&intent.driver_id).await?;
        if !registered
            .registration
            .capabilities
            .contains(&WorldDriverCapability::PixelObservation)
        {
            return Err(format!(
                "computer driver '{}' does not provide pixel observation",
                intent.driver_id
            ));
        }
        if intent.desktop_session_id != registered.desktop_session_id {
            return Err("computer screenshot requested the wrong desktop session".to_string());
        }

        let request = ComputerScreenshotRequest {
            resource_id: intent.resource_id.clone(),
            session_id: intent.desktop_session_id.clone(),
            observation_generation: intent.observation_generation.clone(),
            observation_revision: intent.observation_revision,
            window_resource_id: intent.window_resource_id.clone(),
            max_width: intent.max_width,
        };
        request.validate()?;
        let admission = self.admit_pixel_observation(&registered.registration, &intent)?;

        if let Err(error) = self.authority.read(|authority| {
            authority
                .validate_action_permit(&admission.permit, now_ms())
                .map_err(|error| error.to_string())
        }) {
            let _ = self.fail(&admission.permit, &error);
            return Err(error);
        }

        // Hold the read fence until the driver returns so a concurrent
        // semantic mutation cannot consume this observation mid-capture.
        let fences = self.observation_fences.read().await;
        if let Err(error) = validate_pixel_fence(
            fences.get(&(intent.driver_id.clone(), intent.resource_id.clone())),
            &intent,
        ) {
            drop(fences);
            let _ = self.fail(&admission.permit, &error);
            return Err(error);
        }
        let capture = registered.driver.screenshot(request.clone()).await;
        drop(fences);
        let capture = match capture {
            Ok(capture) => capture,
            Err(error) => {
                let _ = self.fail(&admission.permit, &error);
                return Err(error);
            }
        };
        if let Err(error) = capture.validate_for(&intent.driver_id, &request) {
            let _ = self.fail(&admission.permit, &error);
            return Err(error);
        }

        let outcome = self.authority.write(|authority| {
            authority
                .complete_action(
                    &admission.permit,
                    "redacted focused-window pixels recorded",
                    now_ms(),
                )
                .map_err(|error| error.to_string())
        })?;
        Ok(GovernedComputerScreenshot {
            capture,
            provenance: ComputerWorldProvenance::from_permit(&admission.permit, outcome),
        })
    }

    async fn remember_observation(&self, observation: &ComputerObservation) {
        let key = (
            observation.driver_id.clone(),
            observation.resource_id.clone(),
        );
        let mut fences = self.observation_fences.write().await;
        let previous = fences.get(&key);
        let mut elements = if !observation.full
            && previous.is_some_and(|previous| {
                previous.session_id == observation.session_id
                    && previous.generation == observation.observation_generation
                    && observation.base_revision == Some(previous.revision)
            })
        {
            previous
                .map(|previous| previous.elements.clone())
                .unwrap_or_default()
        } else {
            BTreeMap::new()
        };
        for removed in &observation.removed_refs {
            elements.remove(removed);
        }
        for node in &observation.nodes {
            elements.insert(
                node.element_ref.clone(),
                ComputerObservedElement {
                    role: node.role.clone(),
                    name: node.name.clone(),
                    enabled: node.enabled,
                    sensitive: node.sensitive,
                    actions: node.actions.iter().copied().collect(),
                },
            );
        }
        fences.insert(
            key,
            ComputerObservationFence {
                session_id: observation.session_id.clone(),
                generation: observation.observation_generation.clone(),
                revision: observation.revision,
                focused_window_resource_id: observation.focused_window_resource_id.clone(),
                elements,
            },
        );
    }

    async fn validate_action_fence(&self, intent: &ComputerActionIntent) -> Result<(), String> {
        let fences = self.observation_fences.read().await;
        let fence = fences
            .get(&(intent.driver_id.clone(), intent.resource_id.clone()))
            .ok_or_else(|| {
                "observation_required: observe the desktop before requesting an action".to_string()
            })?;
        if fence.session_id != intent.desktop_session_id
            || fence.generation != intent.observation_generation
            || fence.revision != intent.observation_revision
        {
            return Err(
                "stale_observation: action must target the daemon's latest exact observation"
                    .to_string(),
            );
        }
        let element = fence.elements.get(&intent.element_ref).ok_or_else(|| {
            "element_not_found: action target was not present in the exact observation".to_string()
        })?;
        if !element.enabled {
            return Err("element_disabled: action target is not enabled".to_string());
        }
        if !element.actions.contains(&intent.action) {
            return Err(format!(
                "action_unavailable: exact observation did not advertise '{}' for this target",
                intent.action.as_str()
            ));
        }
        if !intent.allow_high_risk
            && (intent.action == ComputerAction::ForegroundClick
                || observed_element_is_high_risk(element))
        {
            return Err(
                "high_risk_target: observed element is effectful or requires foreground pointer \
                 input; rerun with allow_high_risk=true only when the operator explicitly \
                 requested this action"
                    .to_string(),
            );
        }
        Ok(())
    }

    async fn consume_action_fence(&self, intent: &ComputerActionIntent) -> Result<(), String> {
        let key = (intent.driver_id.clone(), intent.resource_id.clone());
        let mut fences = self.observation_fences.write().await;
        let fence = fences.get(&key).ok_or_else(|| {
            "observation_required: observe the desktop before requesting an action".to_string()
        })?;
        if fence.session_id != intent.desktop_session_id
            || fence.generation != intent.observation_generation
            || fence.revision != intent.observation_revision
            || !fence.elements.contains_key(&intent.element_ref)
        {
            return Err(
                "stale_observation: action must target the daemon's latest exact observation"
                    .to_string(),
            );
        }
        fences.remove(&key);
        Ok(())
    }

    async fn driver(
        &self,
        driver_id: &WorldDriverId,
    ) -> Result<RegisteredComputerDriver, String> {
        self.drivers
            .read()
            .await
            .get(driver_id)
            .cloned()
            .ok_or_else(|| format!("computer driver '{driver_id}' is not registered"))
    }

    fn admit_observation(
        &self,
        registration: &WorldDriverRegistration,
        request: &ComputerObservationIntent,
    ) -> Result<ComputerWorldAdmission, String> {
        let observed_at_ms = now_ms();
        let world_id = computer_world_id(
            &request.authority_id,
            &request.driver_id,
            &request.desktop_session_id,
        );
        let system = WorldPrincipal::system(WorldPrincipalId::new(format!(
            "runtime:{}",
            request.authority_id
        )));
        let observer = request.principal.clone();
        let resource_id = request.resource_id.clone();
        let driver_id = request.driver_id.clone();
        let authority_id = request.authority_id.clone();
        let trace_id = request.trace_id.clone();
        let summary = request.summary.clone();
        let ownership = registration.ownership;
        let surface = registration.surface;
        let desktop_session_id = request.desktop_session_id.clone();

        self.authority.write(move |authority| {
            if matches!(
                authority.world(&world_id),
                Err(WorldAuthorityError::WorldNotFound(_))
            ) {
                authority
                    .create_world(
                        WorldSessionSpec {
                            world_id: world_id.clone(),
                            authority_id: WorldAuthorityId::new(authority_id.clone()),
                            driver_id: driver_id.clone(),
                            ownership,
                            surface,
                        },
                        system.clone(),
                        observed_at_ms,
                    )
                    .map_err(|error| error.to_string())?;
            } else {
                let state = authority
                    .world(&world_id)
                    .map_err(|error| error.to_string())?;
                if state.authority_id.as_str() != authority_id
                    || state.driver_id != driver_id
                    || state.ownership != ownership
                    || state.surface != surface
                {
                    return Err(
                        "computer world identity conflicts with its registered driver".to_string(),
                    );
                }
            }

            let grant_id = WorldGrantId::new(format!(
                "grant:computer-observe:{desktop_session_id}:{}:{}",
                observer.principal_id, resource_id
            ));
            match authority.grant_capabilities(
                &world_id,
                WorldGrantRequest {
                    grant_id,
                    issued_by: system,
                    subject: observer.clone(),
                    capabilities: [WorldCapability::Observe]
                        .into_iter()
                        .collect::<BTreeSet<_>>(),
                    resource_scope: WorldResourceScope::exact([resource_id.clone()]),
                    expires_at_ms: None,
                },
                observed_at_ms,
            ) {
                Ok(_) | Err(WorldAuthorityError::GrantAlreadyExists(_)) => {}
                Err(error) => return Err(error.to_string()),
            }

            let state = authority
                .world(&world_id)
                .map_err(|error| error.to_string())?;
            let operation_id = Uuid::new_v4().to_string();
            let intent = WorldActionIntent {
                intent_id: medousa_world::WorldIntentId::new(format!(
                    "intent:computer-observe:{operation_id}"
                )),
                trace_id: WorldTraceId::new(trace_id),
                principal: observer,
                resource_id,
                expected_revision: state.revision,
                expected_control_generation: None,
                required_capability: WorldCapability::Observe,
                effect_class: WorldEffectClass::Observe,
                idempotency_key: format!("computer-observation:{operation_id}"),
                permit_expires_at_ms: observed_at_ms
                    .saturating_add(COMPUTER_OBSERVATION_PERMIT_MS),
                summary,
            };
            match authority
                .admit_action(&world_id, intent, observed_at_ms)
                .map_err(|error| error.to_string())?
            {
                WorldAdmission::Admitted { permit } => Ok(ComputerWorldAdmission { permit }),
                WorldAdmission::Replay { .. } => {
                    Err("new computer observation unexpectedly resolved as a replay".to_string())
                }
            }
        })
    }

    fn admit_action(
        &self,
        registration: &WorldDriverRegistration,
        request: &ComputerActionIntent,
    ) -> Result<ComputerWorldAdmission, String> {
        let admitted_at_ms = now_ms();
        let world_id = computer_world_id(
            &request.authority_id,
            &request.driver_id,
            &request.desktop_session_id,
        );
        let system = WorldPrincipal::system(WorldPrincipalId::new(format!(
            "runtime:{}",
            request.authority_id
        )));
        let actor = request.principal.clone();
        let resource_id = request.resource_id.clone();
        let driver_id = request.driver_id.clone();
        let authority_id = request.authority_id.clone();
        let trace_id = request.trace_id.clone();
        let summary = request.summary.clone();
        let desktop_session_id = request.desktop_session_id.clone();
        let ownership = registration.ownership;
        let surface = registration.surface;

        self.authority.write(move |authority| {
            let state = authority
                .world(&world_id)
                .map_err(|error| error.to_string())?;
            if state.authority_id.as_str() != authority_id
                || state.driver_id != driver_id
                || state.ownership != ownership
                || state.surface != surface
            {
                return Err("computer world identity conflicts with its registered driver".to_string());
            }

            let grant_id = WorldGrantId::new(format!(
                "grant:computer-interact:{desktop_session_id}:{}:{}",
                actor.principal_id, resource_id
            ));
            match authority.grant_capabilities(
                &world_id,
                WorldGrantRequest {
                    grant_id,
                    issued_by: system,
                    subject: actor.clone(),
                    capabilities: [WorldCapability::Interact]
                        .into_iter()
                        .collect::<BTreeSet<_>>(),
                    resource_scope: WorldResourceScope::exact([resource_id.clone()]),
                    expires_at_ms: None,
                },
                admitted_at_ms,
            ) {
                Ok(_) | Err(WorldAuthorityError::GrantAlreadyExists(_)) => {}
                Err(error) => return Err(error.to_string()),
            }

            let lease = authority
                .acquire_control(
                    &world_id,
                    actor.clone(),
                    Some(admitted_at_ms.saturating_add(COMPUTER_ACTION_PERMIT_MS)),
                    admitted_at_ms,
                )
                .map_err(|error| error.to_string())?;
            let state = authority
                .world(&world_id)
                .map_err(|error| error.to_string())?;
            let operation_id = Uuid::new_v4().to_string();
            let intent = WorldActionIntent {
                intent_id: medousa_world::WorldIntentId::new(format!(
                    "intent:computer-action:{operation_id}"
                )),
                trace_id: WorldTraceId::new(trace_id),
                principal: actor,
                resource_id,
                expected_revision: state.revision,
                expected_control_generation: Some(lease.generation),
                required_capability: WorldCapability::Interact,
                effect_class: WorldEffectClass::LocalMutation,
                idempotency_key: format!("computer-action:{operation_id}"),
                permit_expires_at_ms: admitted_at_ms
                    .saturating_add(COMPUTER_ACTION_PERMIT_MS),
                summary,
            };
            match authority
                .admit_action(&world_id, intent, admitted_at_ms)
                .map_err(|error| error.to_string())?
            {
                WorldAdmission::Admitted { permit } => Ok(ComputerWorldAdmission { permit }),
                WorldAdmission::Replay { .. } => {
                    Err("new computer action unexpectedly resolved as a replay".to_string())
                }
            }
        })
    }

    fn admit_pixel_observation(
        &self,
        registration: &WorldDriverRegistration,
        request: &ComputerPixelObservationIntent,
    ) -> Result<ComputerWorldAdmission, String> {
        let admitted_at_ms = now_ms();
        let world_id = computer_world_id(
            &request.authority_id,
            &request.driver_id,
            &request.desktop_session_id,
        );
        let system = WorldPrincipal::system(WorldPrincipalId::new(format!(
            "runtime:{}",
            request.authority_id
        )));
        let observer = request.principal.clone();
        let resource_id = request.resource_id.clone();
        let driver_id = request.driver_id.clone();
        let authority_id = request.authority_id.clone();
        let trace_id = request.trace_id.clone();
        let summary = request.summary.clone();
        let desktop_session_id = request.desktop_session_id.clone();
        let ownership = registration.ownership;
        let surface = registration.surface;

        self.authority.write(move |authority| {
            let state = authority
                .world(&world_id)
                .map_err(|error| error.to_string())?;
            if state.authority_id.as_str() != authority_id
                || state.driver_id != driver_id
                || state.ownership != ownership
                || state.surface != surface
            {
                return Err(
                    "computer world identity conflicts with its registered driver".to_string(),
                );
            }

            let grant_id = WorldGrantId::new(format!(
                "grant:computer-observe-pixels:{desktop_session_id}:{}:{}",
                observer.principal_id, resource_id
            ));
            match authority.grant_capabilities(
                &world_id,
                WorldGrantRequest {
                    grant_id,
                    issued_by: system,
                    subject: observer.clone(),
                    capabilities: [WorldCapability::ObservePixels]
                        .into_iter()
                        .collect::<BTreeSet<_>>(),
                    resource_scope: WorldResourceScope::exact([resource_id.clone()]),
                    expires_at_ms: None,
                },
                admitted_at_ms,
            ) {
                Ok(_) | Err(WorldAuthorityError::GrantAlreadyExists(_)) => {}
                Err(error) => return Err(error.to_string()),
            }

            let state = authority
                .world(&world_id)
                .map_err(|error| error.to_string())?;
            let operation_id = Uuid::new_v4().to_string();
            let intent = WorldActionIntent {
                intent_id: medousa_world::WorldIntentId::new(format!(
                    "intent:computer-observe-pixels:{operation_id}"
                )),
                trace_id: WorldTraceId::new(trace_id),
                principal: observer,
                resource_id,
                expected_revision: state.revision,
                expected_control_generation: None,
                required_capability: WorldCapability::ObservePixels,
                effect_class: WorldEffectClass::ObservePixels,
                idempotency_key: format!("computer-pixel-observation:{operation_id}"),
                permit_expires_at_ms: admitted_at_ms
                    .saturating_add(COMPUTER_OBSERVATION_PERMIT_MS),
                summary,
            };
            match authority
                .admit_action(&world_id, intent, admitted_at_ms)
                .map_err(|error| error.to_string())?
            {
                WorldAdmission::Admitted { permit } => Ok(ComputerWorldAdmission { permit }),
                WorldAdmission::Replay { .. } => Err(
                    "new computer pixel observation unexpectedly resolved as a replay".to_string(),
                ),
            }
        })
    }

    fn fail(&self, permit: &WorldActionPermit, message: &str) -> Result<(), String> {
        self.authority.write(|authority| {
            authority
                .fail_action(permit, message, now_ms())
                .map(|_| ())
                .map_err(|error| error.to_string())
        })
    }

    fn mark_indeterminate(
        &self,
        permit: &WorldActionPermit,
        message: &str,
    ) -> Result<(), String> {
        self.authority.write(|authority| {
            authority
                .mark_action_indeterminate(permit, message, now_ms())
                .map(|_| ())
                .map_err(|error| error.to_string())
        })
    }
}

#[derive(Debug, Clone)]
struct ComputerWorldAdmission {
    permit: WorldActionPermit,
}

#[derive(Debug, Clone)]
pub struct ComputerObservationIntent {
    pub authority_id: String,
    pub driver_id: WorldDriverId,
    pub desktop_session_id: String,
    pub principal: WorldPrincipal,
    pub resource_id: WorldResourceId,
    pub trace_id: String,
    pub summary: String,
    pub after_revision: Option<u64>,
    pub max_nodes: u32,
}

#[derive(Debug, Clone)]
pub struct ComputerActionIntent {
    pub authority_id: String,
    pub driver_id: WorldDriverId,
    pub desktop_session_id: String,
    pub principal: WorldPrincipal,
    pub resource_id: WorldResourceId,
    pub trace_id: String,
    pub summary: String,
    pub observation_generation: String,
    pub observation_revision: u64,
    pub element_ref: String,
    pub action: ComputerAction,
    pub value: Option<String>,
    pub allow_high_risk: bool,
}

#[derive(Debug, Clone)]
pub struct ComputerPixelObservationIntent {
    pub authority_id: String,
    pub driver_id: WorldDriverId,
    pub desktop_session_id: String,
    pub principal: WorldPrincipal,
    pub resource_id: WorldResourceId,
    pub trace_id: String,
    pub summary: String,
    pub observation_generation: String,
    pub observation_revision: u64,
    pub window_resource_id: WorldResourceId,
    pub max_width: u32,
}

impl ComputerPixelObservationIntent {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("world authority", &self.authority_id)?;
        validate_identifier("computer driver", self.driver_id.as_str())?;
        validate_identifier("desktop session", &self.desktop_session_id)?;
        validate_identifier("computer principal", self.principal.principal_id.as_str())?;
        validate_identifier("computer trace", &self.trace_id)?;
        ComputerScreenshotRequest {
            resource_id: self.resource_id.clone(),
            session_id: self.desktop_session_id.clone(),
            observation_generation: self.observation_generation.clone(),
            observation_revision: self.observation_revision,
            window_resource_id: self.window_resource_id.clone(),
            max_width: self.max_width,
        }
        .validate()?;
        if self.summary.trim().is_empty() || self.summary.len() > 1_024 {
            return Err("computer screenshot summary is invalid".to_string());
        }
        Ok(())
    }
}

impl ComputerActionIntent {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("world authority", &self.authority_id)?;
        validate_identifier("computer driver", self.driver_id.as_str())?;
        validate_identifier("desktop session", &self.desktop_session_id)?;
        validate_identifier("computer principal", self.principal.principal_id.as_str())?;
        validate_identifier("computer trace", &self.trace_id)?;
        validate_identifier("observation generation", &self.observation_generation)?;
        validate_identifier("computer element reference", &self.element_ref)?;
        if self.observation_revision == 0 {
            return Err("computer action observation revision must be positive".to_string());
        }
        ComputerActionRequest {
            resource_id: self.resource_id.clone(),
            session_id: self.desktop_session_id.clone(),
            observation_generation: self.observation_generation.clone(),
            observation_revision: self.observation_revision,
            element_ref: self.element_ref.clone(),
            action: self.action,
            value: self.value.clone(),
        }
        .validate()?;
        if self.summary.trim().is_empty() || self.summary.len() > 1_024 {
            return Err("computer action summary is invalid".to_string());
        }
        Ok(())
    }
}

impl ComputerObservationIntent {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("world authority", &self.authority_id)?;
        validate_identifier("computer driver", self.driver_id.as_str())?;
        validate_identifier("desktop session", &self.desktop_session_id)?;
        validate_identifier("computer principal", self.principal.principal_id.as_str())?;
        validate_identifier("computer trace", &self.trace_id)?;
        if self.summary.trim().is_empty() || self.summary.len() > 1_024 {
            return Err("computer observation summary is invalid".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ComputerWorldProvenance {
    pub world_id: WorldId,
    pub driver_id: WorldDriverId,
    pub intent_id: medousa_world::WorldIntentId,
    pub trace_id: WorldTraceId,
    pub resource_id: WorldResourceId,
    pub admitted_revision: u64,
    pub outcome: WorldActionOutcome,
}

impl ComputerWorldProvenance {
    fn from_permit(permit: &WorldActionPermit, outcome: WorldActionOutcome) -> Self {
        Self {
            world_id: permit.world_id.clone(),
            driver_id: permit.driver_id.clone(),
            intent_id: permit.intent_id.clone(),
            trace_id: permit.trace_id.clone(),
            resource_id: permit.resource_id.clone(),
            admitted_revision: permit.admitted_revision,
            outcome,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct GovernedComputerObservation {
    pub observation: ComputerObservation,
    pub provenance: ComputerWorldProvenance,
}

#[derive(Debug, Clone, Serialize)]
pub struct GovernedComputerAction {
    pub receipt: ComputerActionReceipt,
    pub provenance: ComputerWorldProvenance,
}

#[derive(Debug, Clone, Serialize)]
pub struct GovernedComputerScreenshot {
    pub capture: ComputerScreenshotCapture,
    pub provenance: ComputerWorldProvenance,
}

fn validate_registration(registration: &WorldDriverRegistration) -> Result<(), String> {
    validate_identifier("computer driver", registration.driver_id.as_str())?;
    if registration.kind != WorldDriverKind::NativeDesktop {
        return Err("computer broker only accepts native desktop drivers".to_string());
    }
    if !matches!(
        registration.surface,
        WorldSurfaceKind::Desktop | WorldSurfaceKind::Application | WorldSurfaceKind::Composite
    ) {
        return Err("native computer driver advertised an incompatible surface".to_string());
    }
    if !matches!(
        registration.transport,
        WorldDriverTransport::InProcess | WorldDriverTransport::LocalSidecar
    ) {
        return Err("native computer driver must be colocated with its world authority".to_string());
    }
    if !registration
        .capabilities
        .contains(&WorldDriverCapability::SemanticObservation)
    {
        return Err("native computer driver must provide semantic observation".to_string());
    }
    Ok(())
}

fn validate_preflight(
    registration: &WorldDriverRegistration,
    preflight: &ComputerDriverPreflight,
) -> Result<(), String> {
    if preflight.protocol_version != COMPUTER_DRIVER_PROTOCOL_VERSION {
        return Err(format!(
            "computer driver protocol {} is unsupported",
            preflight.protocol_version
        ));
    }
    if preflight.driver_id != registration.driver_id {
        return Err("computer preflight came from the wrong driver".to_string());
    }
    validate_identifier("computer platform", &preflight.platform)?;
    validate_identifier("desktop session", &preflight.session_id)?;
    let mut permissions = BTreeSet::new();
    for permission in &preflight.permissions {
        if !permissions.insert(permission.permission) {
            return Err("computer preflight contains duplicate permissions".to_string());
        }
    }
    if !permissions.contains(&ComputerPermissionKind::Accessibility) {
        return Err("computer preflight omitted accessibility status".to_string());
    }
    Ok(())
}

fn validate_identifier(label: &str, value: &str) -> Result<(), String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > 256 || trimmed.chars().any(char::is_control) {
        return Err(format!("{label} identity is invalid"));
    }
    Ok(())
}

fn observed_element_is_high_risk(element: &ComputerObservedElement) -> bool {
    if element.sensitive {
        return true;
    }
    let semantic_label = format!("{} {}", element.role, element.name).to_ascii_lowercase();
    let tokens = semantic_label
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect::<BTreeSet<_>>();
    let risky_token = [
        "password",
        "submit",
        "send",
        "checkout",
        "purchase",
        "pay",
        "payment",
        "buy",
        "delete",
        "remove",
        "erase",
        "confirm",
        "authorize",
        "allow",
        "install",
        "uninstall",
        "publish",
        "transfer",
    ]
    .iter()
    .any(|needle| tokens.contains(needle));
    risky_token
        || ["place order", "sign in", "log in"]
            .iter()
            .any(|phrase| semantic_label.contains(phrase))
}

pub fn desktop_resource_id(
    driver_id: &WorldDriverId,
    desktop_session_id: &str,
) -> WorldResourceId {
    let digest = Sha256::digest(format!("{driver_id}\0{desktop_session_id}").as_bytes());
    WorldResourceId::new(format!("desktop:sha256:{digest:x}"))
}

fn computer_world_id(
    authority_id: &str,
    driver_id: &WorldDriverId,
    desktop_session_id: &str,
) -> WorldId {
    WorldId::new(format!(
        "world:computer:{authority_id}:{driver_id}:{desktop_session_id}"
    ))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use medousa_computer_bridge::{
        COMPUTER_OBSERVATION_SCHEMA_VERSION, COMPUTER_SCREENSHOT_SCHEMA_VERSION,
        ComputerApplication, ComputerPermissionReport, ComputerPermissionStatus, ComputerRect,
        ComputerSemanticNode, ComputerWindow,
    };
    use medousa_world::{
        WorldActionStatus, WorldEventKind, WorldOwnership, WorldPrincipalKind,
    };

    use super::*;

    struct FakeComputerDriver {
        registration: WorldDriverRegistration,
        observations: AtomicUsize,
        actions: AtomicUsize,
        spoof_driver: bool,
        spoof_action_revision: bool,
    }

    #[async_trait]
    impl ComputerDriver for FakeComputerDriver {
        fn registration(&self) -> WorldDriverRegistration {
            self.registration.clone()
        }

        async fn preflight(&self) -> Result<ComputerDriverPreflight, String> {
            Ok(ComputerDriverPreflight {
                protocol_version: COMPUTER_DRIVER_PROTOCOL_VERSION,
                driver_id: self.registration.driver_id.clone(),
                platform: "test-os".to_string(),
                session_id: "login:test".to_string(),
                permissions: [
                    ComputerPermissionKind::Accessibility,
                    ComputerPermissionKind::InputControl,
                ]
                .into_iter()
                .map(|permission| ComputerPermissionReport {
                    permission,
                    status: ComputerPermissionStatus::Granted,
                    can_request: false,
                    guidance: None,
                })
                .collect(),
                checked_at_ms: 1,
            })
        }

        async fn observe(
            &self,
            request: ComputerObservationRequest,
        ) -> Result<ComputerObservation, String> {
            self.observations.fetch_add(1, Ordering::SeqCst);
            let window_id = WorldResourceId::new("window:test");
            Ok(ComputerObservation {
                schema_version: COMPUTER_OBSERVATION_SCHEMA_VERSION,
                driver_id: if self.spoof_driver {
                    WorldDriverId::new("driver:computer:spoofed")
                } else {
                    self.registration.driver_id.clone()
                },
                resource_id: request.resource_id,
                session_id: "login:test".to_string(),
                observation_generation: "generation:test".to_string(),
                revision: 1,
                base_revision: None,
                full: true,
                unchanged: false,
                active_application_resource_id: Some(WorldResourceId::new("application:test")),
                focused_window_resource_id: Some(window_id.clone()),
                displays: Vec::new(),
                applications: vec![ComputerApplication {
                    resource_id: WorldResourceId::new("application:test"),
                    name: "Test App".to_string(),
                    application_id: Some("dev.medousa.test".to_string()),
                    process_id: Some(42),
                    active: true,
                }],
                windows: vec![ComputerWindow {
                    resource_id: window_id.clone(),
                    application_resource_id: WorldResourceId::new("application:test"),
                    title: "Test Window".to_string(),
                    frame: ComputerRect {
                        x: 0,
                        y: 0,
                        width: 1280,
                        height: 720,
                    },
                    minimized: false,
                    focused: true,
                }],
                nodes: vec![ComputerSemanticNode {
                    element_ref: "ax:test:button".to_string(),
                    parent_ref: None,
                    window_resource_id: window_id,
                    role: "button".to_string(),
                    name: "Continue".to_string(),
                    value: None,
                    bounds: None,
                    enabled: true,
                    focused: false,
                    selected: None,
                    sensitive: false,
                    actions: vec![ComputerAction::Press],
                }],
                removed_refs: Vec::new(),
                truncated: false,
                captured_at_ms: 1,
                untrusted_content: true,
            })
        }

        async fn screenshot(
            &self,
            request: ComputerScreenshotRequest,
        ) -> Result<ComputerScreenshotCapture, String> {
            Ok(ComputerScreenshotCapture {
                schema_version: COMPUTER_SCREENSHOT_SCHEMA_VERSION,
                driver_id: self.registration.driver_id.clone(),
                resource_id: request.resource_id,
                session_id: request.session_id,
                observation_generation: request.observation_generation,
                observation_revision: request.observation_revision,
                window_resource_id: request.window_resource_id,
                coordinate_frame: "focused_window_pixels".to_string(),
                mime: "image/png".to_string(),
                image_width: request.max_width,
                image_height: 720,
                byte_size: 1,
                sha256: "a".repeat(64),
                sensitive_regions_redacted: 0,
                captured_at_ms: 2,
                untrusted_content: true,
                image_base64: "eA==".to_string(),
            })
        }

        async fn act(
            &self,
            request: ComputerActionRequest,
        ) -> Result<ComputerActionReceipt, ComputerDriverActionError> {
            self.actions.fetch_add(1, Ordering::SeqCst);
            if request.observation_generation != "generation:test"
                || request.observation_revision != 1
                || request.element_ref != "ax:test:button"
            {
                return Err(ComputerDriverActionError::failed(
                    "stale or unknown observation target",
                ));
            }
            Ok(ComputerActionReceipt {
                driver_id: self.registration.driver_id.clone(),
                resource_id: request.resource_id,
                session_id: request.session_id,
                observation_generation: request.observation_generation,
                observation_revision: request.observation_revision
                    + u64::from(self.spoof_action_revision),
                element_ref: request.element_ref,
                action: request.action,
                completed_at_ms: 2,
            })
        }
    }

    fn registration(driver_id: &str) -> WorldDriverRegistration {
        WorldDriverRegistration {
            driver_id: WorldDriverId::new(driver_id),
            kind: WorldDriverKind::NativeDesktop,
            surface: WorldSurfaceKind::Desktop,
            ownership: WorldOwnership::Attached,
            transport: WorldDriverTransport::InProcess,
            capabilities: [
                WorldDriverCapability::SemanticObservation,
                WorldDriverCapability::PixelObservation,
                WorldDriverCapability::Interaction,
            ]
                .into_iter()
                .collect(),
            display_name: Some("Fake computer".to_string()),
        }
    }

    fn intent(driver_id: &str) -> ComputerObservationIntent {
        ComputerObservationIntent {
            authority_id: "workshop:test".to_string(),
            driver_id: WorldDriverId::new(driver_id),
            desktop_session_id: "login:test".to_string(),
            principal: WorldPrincipal::new(
                "agent:test",
                WorldPrincipalKind::Agent,
            ),
            resource_id: WorldResourceId::new("desktop:login"),
            trace_id: "turn:test".to_string(),
            summary: "Observe the attached desktop".to_string(),
            after_revision: None,
            max_nodes: 64,
        }
    }

    fn pixel_intent(driver_id: &str, window_resource_id: &str) -> ComputerPixelObservationIntent {
        ComputerPixelObservationIntent {
            authority_id: "workshop:test".to_string(),
            driver_id: WorldDriverId::new(driver_id),
            desktop_session_id: "login:test".to_string(),
            principal: WorldPrincipal::new("agent:test", WorldPrincipalKind::Agent),
            resource_id: WorldResourceId::new("desktop:login"),
            trace_id: "turn:test:pixels".to_string(),
            summary: "Capture the exact focused window".to_string(),
            observation_generation: "generation:test".to_string(),
            observation_revision: 1,
            window_resource_id: WorldResourceId::new(window_resource_id),
            max_width: 1_280,
        }
    }

    #[tokio::test]
    async fn admitted_observation_crosses_the_exact_driver_and_completes() {
        let authority = Arc::new(WorldAuthorityService::default());
        let broker = ComputerDriverBroker::new(authority);
        let driver = Arc::new(FakeComputerDriver {
            registration: registration("driver:computer:test"),
            observations: AtomicUsize::new(0),
            actions: AtomicUsize::new(0),
            spoof_driver: false,
            spoof_action_revision: false,
        });
        broker
            .register(driver.clone())
            .await
            .expect("register fake driver");

        let result = broker
            .observe(intent("driver:computer:test"))
            .await
            .expect("governed observation");

        assert_eq!(driver.observations.load(Ordering::SeqCst), 1);
        assert_eq!(
            result.provenance.outcome.status,
            WorldActionStatus::Confirmed
        );
        assert_eq!(
            result.provenance.world_id.as_str(),
            "world:computer:workshop:test:driver:computer:test:login:test"
        );
        assert_eq!(result.observation.nodes[0].name, "Continue");
    }

    #[tokio::test]
    async fn pixels_require_and_preserve_the_exact_focused_window_fence() {
        let authority = Arc::new(WorldAuthorityService::default());
        let broker = ComputerDriverBroker::new(authority);
        broker
            .register(Arc::new(FakeComputerDriver {
                registration: registration("driver:computer:pixels"),
                observations: AtomicUsize::new(0),
                actions: AtomicUsize::new(0),
                spoof_driver: false,
                spoof_action_revision: false,
            }))
            .await
            .expect("register fake driver");
        broker
            .observe(intent("driver:computer:pixels"))
            .await
            .expect("governed observation");

        let wrong_window = broker
            .capture_pixels(pixel_intent("driver:computer:pixels", "window:other"))
            .await
            .expect_err("wrong focused window must fail");
        assert!(wrong_window.contains("exact observed focused window"));

        let result = broker
            .capture_pixels(pixel_intent("driver:computer:pixels", "window:test"))
            .await
            .expect("governed pixels");
        assert_eq!(result.capture.window_resource_id.as_str(), "window:test");
        assert_eq!(
            result.provenance.outcome.status,
            WorldActionStatus::Confirmed
        );

        // Pixel reads do not consume the semantic action fence.
        broker
            .act(action_intent(
                "driver:computer:pixels",
                "ax:test:button",
            ))
            .await
            .expect("semantic action after pixels");
    }

    #[tokio::test]
    async fn driver_cannot_return_an_observation_for_another_identity() {
        let authority = Arc::new(WorldAuthorityService::default());
        let broker = ComputerDriverBroker::new(authority);
        broker
            .register(Arc::new(FakeComputerDriver {
                registration: registration("driver:computer:honest"),
                observations: AtomicUsize::new(0),
                actions: AtomicUsize::new(0),
                spoof_driver: true,
                spoof_action_revision: false,
            }))
            .await
            .expect("register fake driver");

        let error = broker
            .observe(intent("driver:computer:honest"))
            .await
            .expect_err("spoofed observation must fail");
        assert!(error.contains("wrong driver"));
    }

    #[tokio::test]
    async fn browser_driver_cannot_register_as_a_computer_driver() {
        let authority = Arc::new(WorldAuthorityService::default());
        let broker = ComputerDriverBroker::new(authority);
        let mut browser = registration("driver:browser:test");
        browser.kind = WorldDriverKind::EmbeddedBrowser;
        browser.surface = WorldSurfaceKind::Browser;

        let error = broker
            .register(Arc::new(FakeComputerDriver {
                registration: browser,
                observations: AtomicUsize::new(0),
                actions: AtomicUsize::new(0),
                spoof_driver: false,
                spoof_action_revision: false,
            }))
            .await
            .expect_err("browser registration must fail");
        assert!(error.contains("native desktop"));
    }

    fn action_intent(driver_id: &str, element_ref: &str) -> ComputerActionIntent {
        ComputerActionIntent {
            authority_id: "workshop:test".to_string(),
            driver_id: WorldDriverId::new(driver_id),
            desktop_session_id: "login:test".to_string(),
            principal: WorldPrincipal::new("agent:test", WorldPrincipalKind::Agent),
            resource_id: WorldResourceId::new("desktop:login"),
            trace_id: "turn:action".to_string(),
            summary: "Press the observed button".to_string(),
            observation_generation: "generation:test".to_string(),
            observation_revision: 1,
            element_ref: element_ref.to_string(),
            action: ComputerAction::Press,
            value: None,
            allow_high_risk: false,
        }
    }

    #[tokio::test]
    async fn semantic_action_uses_the_exact_observed_element() {
        let authority = Arc::new(WorldAuthorityService::default());
        let broker = ComputerDriverBroker::new(authority);
        let driver = Arc::new(FakeComputerDriver {
            registration: registration("driver:computer:test"),
            observations: AtomicUsize::new(0),
            actions: AtomicUsize::new(0),
            spoof_driver: false,
            spoof_action_revision: false,
        });
        broker.register(driver.clone()).await.expect("register driver");
        broker
            .observe(intent("driver:computer:test"))
            .await
            .expect("establish observation");

        let result = broker
            .act(action_intent("driver:computer:test", "ax:test:button"))
            .await
            .expect("governed action");

        assert_eq!(driver.actions.load(Ordering::SeqCst), 1);
        assert_eq!(result.receipt.action, ComputerAction::Press);
        assert_eq!(result.provenance.outcome.status, WorldActionStatus::Confirmed);
    }

    #[tokio::test]
    async fn semantic_action_must_be_advertised_by_the_exact_observation() {
        let authority = Arc::new(WorldAuthorityService::default());
        let broker = ComputerDriverBroker::new(authority);
        let driver = Arc::new(FakeComputerDriver {
            registration: registration("driver:computer:test"),
            observations: AtomicUsize::new(0),
            actions: AtomicUsize::new(0),
            spoof_driver: false,
            spoof_action_revision: false,
        });
        broker.register(driver.clone()).await.expect("register driver");
        broker
            .observe(intent("driver:computer:test"))
            .await
            .expect("establish observation");
        let mut action = action_intent("driver:computer:test", "ax:test:button");
        action.action = ComputerAction::ShowMenu;

        let error = broker
            .act(action)
            .await
            .expect_err("unadvertised semantic action must fail");

        assert!(error.contains("action_unavailable"));
        assert_eq!(driver.actions.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn admitted_semantic_action_consumes_its_observation_fence() {
        let authority = Arc::new(WorldAuthorityService::default());
        let broker = ComputerDriverBroker::new(authority);
        let driver = Arc::new(FakeComputerDriver {
            registration: registration("driver:computer:test"),
            observations: AtomicUsize::new(0),
            actions: AtomicUsize::new(0),
            spoof_driver: false,
            spoof_action_revision: false,
        });
        broker.register(driver.clone()).await.expect("register driver");
        broker
            .observe(intent("driver:computer:test"))
            .await
            .expect("establish observation");
        let action = action_intent("driver:computer:test", "ax:test:button");

        broker.act(action.clone()).await.expect("first action");
        let error = broker
            .act(action)
            .await
            .expect_err("replayed action must require another observation");

        assert!(error.contains("observation_required"));
        assert_eq!(driver.actions.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn semantic_action_requires_explicit_operator_intent_for_high_risk_target() {
        let authority = Arc::new(WorldAuthorityService::default());
        let broker = ComputerDriverBroker::new(authority);
        let driver_id = WorldDriverId::new("driver:computer:test");
        let resource_id = WorldResourceId::new("desktop:login");
        let driver = Arc::new(FakeComputerDriver {
            registration: registration(driver_id.as_str()),
            observations: AtomicUsize::new(0),
            actions: AtomicUsize::new(0),
            spoof_driver: false,
            spoof_action_revision: false,
        });
        broker.register(driver.clone()).await.expect("register driver");
        broker
            .observe(intent(driver_id.as_str()))
            .await
            .expect("establish observation");
        broker
            .observation_fences
            .write()
            .await
            .get_mut(&(driver_id.clone(), resource_id))
            .expect("observation fence")
            .elements
            .get_mut("ax:test:button")
            .expect("observed element")
            .name = "Delete account".to_string();

        let denied = broker
            .act(action_intent(driver_id.as_str(), "ax:test:button"))
            .await
            .expect_err("high-risk action needs explicit intent");
        assert!(denied.contains("high_risk_target"));
        assert_eq!(driver.actions.load(Ordering::SeqCst), 0);

        let mut allowed = action_intent(driver_id.as_str(), "ax:test:button");
        allowed.allow_high_risk = true;
        broker.act(allowed).await.expect("explicit high-risk action");
        assert_eq!(driver.actions.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn foreground_click_always_requires_explicit_operator_intent() {
        let authority = Arc::new(WorldAuthorityService::default());
        let broker = ComputerDriverBroker::new(authority);
        let driver_id = WorldDriverId::new("driver:computer:test");
        let resource_id = WorldResourceId::new("desktop:login");
        let driver = Arc::new(FakeComputerDriver {
            registration: registration(driver_id.as_str()),
            observations: AtomicUsize::new(0),
            actions: AtomicUsize::new(0),
            spoof_driver: false,
            spoof_action_revision: false,
        });
        broker.register(driver.clone()).await.expect("register driver");
        broker
            .observe(intent(driver_id.as_str()))
            .await
            .expect("establish observation");
        let mut fences = broker.observation_fences.write().await;
        let actions = &mut fences
            .get_mut(&(driver_id.clone(), resource_id))
            .expect("observation fence")
            .elements
            .get_mut("ax:test:button")
            .expect("observed element")
            .actions;
        actions.clear();
        actions.insert(ComputerAction::ForegroundClick);
        drop(fences);

        let mut action = action_intent(driver_id.as_str(), "ax:test:button");
        action.action = ComputerAction::ForegroundClick;
        let denied = broker
            .act(action.clone())
            .await
            .expect_err("foreground input needs explicit intent");
        assert!(denied.contains("high_risk_target"));
        assert_eq!(driver.actions.load(Ordering::SeqCst), 0);

        action.allow_high_risk = true;
        broker.act(action).await.expect("explicit foreground click");
        assert_eq!(driver.actions.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn high_risk_matching_uses_word_boundaries() {
        assert!(!observed_element_is_high_risk(&ComputerObservedElement {
            role: "button".to_string(),
            name: "Display settings".to_string(),
            enabled: true,
            sensitive: false,
            actions: [ComputerAction::Press].into_iter().collect(),
        }));
        assert!(observed_element_is_high_risk(&ComputerObservedElement {
            role: "button".to_string(),
            name: "Pay now".to_string(),
            enabled: true,
            sensitive: false,
            actions: [ComputerAction::Press].into_iter().collect(),
        }));
    }

    #[tokio::test]
    async fn semantic_action_rejects_an_unknown_observation_reference() {
        let authority = Arc::new(WorldAuthorityService::default());
        let broker = ComputerDriverBroker::new(authority);
        let driver = Arc::new(FakeComputerDriver {
            registration: registration("driver:computer:test"),
            observations: AtomicUsize::new(0),
            actions: AtomicUsize::new(0),
            spoof_driver: false,
            spoof_action_revision: false,
        });
        broker
            .register(driver.clone())
            .await
            .expect("register driver");
        broker
            .observe(intent("driver:computer:test"))
            .await
            .expect("establish observation");

        let error = broker
            .act(action_intent("driver:computer:test", "ax:test:missing"))
            .await
            .expect_err("unknown element must fail");
        assert!(error.contains("element_not_found"));
        assert_eq!(driver.actions.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn semantic_action_rejects_a_stale_observation_revision_before_dispatch() {
        let authority = Arc::new(WorldAuthorityService::default());
        let broker = ComputerDriverBroker::new(authority);
        let driver = Arc::new(FakeComputerDriver {
            registration: registration("driver:computer:test"),
            observations: AtomicUsize::new(0),
            actions: AtomicUsize::new(0),
            spoof_driver: false,
            spoof_action_revision: false,
        });
        broker.register(driver.clone()).await.expect("register driver");
        broker
            .observe(intent("driver:computer:test"))
            .await
            .expect("establish observation");
        let mut action = action_intent("driver:computer:test", "ax:test:button");
        action.observation_revision = 2;

        let error = broker
            .act(action)
            .await
            .expect_err("stale revision must fail");
        assert!(error.contains("stale_observation"));
        assert_eq!(driver.actions.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn invalid_action_acknowledgement_is_recorded_as_indeterminate() {
        let authority = Arc::new(WorldAuthorityService::default());
        let broker = ComputerDriverBroker::new(authority.clone());
        let driver = Arc::new(FakeComputerDriver {
            registration: registration("driver:computer:test"),
            observations: AtomicUsize::new(0),
            actions: AtomicUsize::new(0),
            spoof_driver: false,
            spoof_action_revision: true,
        });
        broker.register(driver).await.expect("register driver");
        broker
            .observe(intent("driver:computer:test"))
            .await
            .expect("establish observation");

        broker
            .act(action_intent("driver:computer:test", "ax:test:button"))
            .await
            .expect_err("invalid receipt must not be trusted");
        let world_id = computer_world_id(
            "workshop:test",
            &WorldDriverId::new("driver:computer:test"),
            "login:test",
        );
        let events = authority
            .read(|authority| {
                authority
                    .events_after(&world_id, 0, 64)
                    .map_err(|error| error.to_string())
            })
            .expect("world events");
        assert!(events.iter().any(|event| matches!(
            &event.event,
            WorldEventKind::ActionCommitted {
                status: WorldActionStatus::Indeterminate,
                ..
            }
        )));
    }
}
