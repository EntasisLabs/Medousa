use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

use thiserror::Error;

use crate::model::{
    WORLD_ACTION_CHECKPOINT_SCHEMA_VERSION, WORLD_ACTION_RECIPE_HINT_SCHEMA_VERSION,
    WORLD_EVENT_ENVELOPE_SCHEMA_VERSION, WORLD_SCHEMA_VERSION, WorldActionCheckpoint,
    WorldActionIntent, WorldActionOutcome, WorldActionPermit, WorldActionRecipeHint,
    WorldActionStatus, WorldAdmission, WorldCapability, WorldCapabilityGrant, WorldControlLease,
    WorldEvent, WorldEventEnvelope, WorldEventKind, WorldGrantId, WorldGrantRequest, WorldId,
    WorldIntentId, WorldPrincipal, WorldPrincipalKind, WorldResourceId, WorldResourceScope,
    WorldSession, WorldSessionSpec,
};

const DEFAULT_EVENT_CAPACITY: usize = 4_096;
const MAX_RECIPE_OPERATIONS: usize = 16;
const MAX_RECIPE_VERB_BYTES: usize = 64;
const MAX_RECIPE_SEMANTIC_TEXT_BYTES: usize = 512;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum WorldAuthorityError {
    #[error("world id is required")]
    EmptyWorldId,
    #[error("world already exists: {0}")]
    WorldAlreadyExists(WorldId),
    #[error("world driver id is required")]
    EmptyDriverId,
    #[error("world not found: {0}")]
    WorldNotFound(WorldId),
    #[error("principal id is required")]
    EmptyPrincipalId,
    #[error("resource id is required")]
    EmptyResourceId,
    #[error("grant id is required")]
    EmptyGrantId,
    #[error("intent id is required")]
    EmptyIntentId,
    #[error("idempotency key is required")]
    EmptyIdempotencyKey,
    #[error("action permit must expire after admission")]
    InvalidPermitExpiry,
    #[error("capability grant cannot be empty")]
    EmptyCapabilityGrant,
    #[error("principal {0} cannot administer this world")]
    AdministrationDenied(String),
    #[error("grant already exists: {0}")]
    GrantAlreadyExists(WorldGrantId),
    #[error("grant not found: {0}")]
    GrantNotFound(WorldGrantId),
    #[error("principal {principal} lacks {capability:?} for resource {resource}")]
    CapabilityDenied {
        principal: String,
        capability: WorldCapability,
        resource: WorldResourceId,
    },
    #[error("intent capability {declared:?} does not match effect requirement {required:?}")]
    EffectCapabilityMismatch {
        declared: WorldCapability,
        required: WorldCapability,
    },
    #[error("world revision is stale: expected {expected}, current {current}")]
    StaleRevision { expected: u64, current: u64 },
    #[error("control is held by {holder}")]
    ControlConflict { holder: String },
    #[error("world has no active control lease")]
    ControlRequired,
    #[error("control belongs to {holder}, not {principal}")]
    ControlHolderMismatch { holder: String, principal: String },
    #[error("control generation is stale: expected {expected}, current {current}")]
    StaleControlGeneration { expected: u64, current: u64 },
    #[error("control generation is required for a mutating action")]
    MissingControlGeneration,
    #[error("action with idempotency key is already pending: {0}")]
    ActionAlreadyPending(String),
    #[error("idempotency key was already used for a different action: {0}")]
    IdempotencyConflict(String),
    #[error("action intent is already pending: {0}")]
    IntentAlreadyPending(WorldIntentId),
    #[error("resource already has a pending mutating action: {0}")]
    ResourceActionAlreadyPending(WorldResourceId),
    #[error("action permit is not pending: {0}")]
    PermitNotPending(WorldIntentId),
    #[error("action permit does not match the admitted action")]
    PermitMismatch,
    #[error("action permit has expired")]
    PermitExpired,
    #[error("world action recipe hint is invalid: {0}")]
    InvalidRecipeHint(String),
}

#[derive(Debug)]
struct WorldRecord {
    state: WorldSession,
    grants: HashMap<WorldGrantId, WorldCapabilityGrant>,
    pending: HashMap<WorldIntentId, WorldActionPermit>,
    pending_idempotency: HashMap<String, WorldIntentId>,
    completed_idempotency: HashMap<String, WorldActionOutcome>,
    events: VecDeque<WorldEvent>,
    next_event_sequence: u64,
}

impl WorldRecord {
    fn new(state: WorldSession) -> Self {
        Self {
            state,
            grants: HashMap::new(),
            pending: HashMap::new(),
            pending_idempotency: HashMap::new(),
            completed_idempotency: HashMap::new(),
            events: VecDeque::new(),
            next_event_sequence: 1,
        }
    }
}

#[derive(Debug)]
pub struct WorldAuthority {
    worlds: HashMap<WorldId, WorldRecord>,
    event_capacity: usize,
}

impl Default for WorldAuthority {
    fn default() -> Self {
        Self::new(DEFAULT_EVENT_CAPACITY)
    }
}

impl WorldAuthority {
    pub fn new(event_capacity: usize) -> Self {
        Self {
            worlds: HashMap::new(),
            event_capacity: event_capacity.max(1),
        }
    }

    /// Revalidate a previously admitted permit immediately before driver
    /// dispatch. Drivers call this at guarded-batch boundaries so a human
    /// takeover, state revision, expiry, or replaced permit fences stale work
    /// before another effect is applied.
    pub fn validate_action_permit(
        &self,
        permit: &WorldActionPermit,
        now_ms: u64,
    ) -> Result<(), WorldAuthorityError> {
        let record = self
            .worlds
            .get(&permit.world_id)
            .ok_or_else(|| WorldAuthorityError::WorldNotFound(permit.world_id.clone()))?;
        let pending = record
            .pending
            .get(&permit.intent_id)
            .ok_or_else(|| WorldAuthorityError::PermitNotPending(permit.intent_id.clone()))?;
        if pending != permit || record.state.driver_id != permit.driver_id {
            return Err(WorldAuthorityError::PermitMismatch);
        }
        if permit.expires_at_ms <= now_ms {
            return Err(WorldAuthorityError::PermitExpired);
        }
        if record.state.revision != permit.admitted_revision {
            return Err(WorldAuthorityError::StaleRevision {
                expected: permit.admitted_revision,
                current: record.state.revision,
            });
        }
        if permit.effect_class.requires_control() {
            let expected = permit
                .control_generation
                .ok_or(WorldAuthorityError::MissingControlGeneration)?;
            let lease = record
                .state
                .active_control_lease
                .as_ref()
                .filter(|lease| lease.is_active_at(now_ms))
                .ok_or(WorldAuthorityError::ControlRequired)?;
            if lease.principal != permit.principal {
                return Err(WorldAuthorityError::ControlHolderMismatch {
                    holder: lease.principal.principal_id.to_string(),
                    principal: permit.principal.principal_id.to_string(),
                });
            }
            if lease.generation != expected {
                return Err(WorldAuthorityError::StaleControlGeneration {
                    expected,
                    current: lease.generation,
                });
            }
        }
        Ok(())
    }

    pub fn create_world(
        &mut self,
        spec: WorldSessionSpec,
        created_by: WorldPrincipal,
        now_ms: u64,
    ) -> Result<WorldSession, WorldAuthorityError> {
        validate_world_id(&spec.world_id)?;
        if spec.driver_id.is_empty() {
            return Err(WorldAuthorityError::EmptyDriverId);
        }
        validate_principal(&created_by)?;
        if self.worlds.contains_key(&spec.world_id) {
            return Err(WorldAuthorityError::WorldAlreadyExists(spec.world_id));
        }

        let state = WorldSession {
            schema_version: WORLD_SCHEMA_VERSION,
            world_id: spec.world_id.clone(),
            authority_id: spec.authority_id,
            driver_id: spec.driver_id.clone(),
            ownership: spec.ownership,
            surface: spec.surface,
            revision: 0,
            control_generation: 0,
            active_control_lease: None,
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
        };
        let mut record = WorldRecord::new(state.clone());
        let bootstrap_grant_id = WorldGrantId::new(format!("bootstrap:{}", spec.world_id));
        record.grants.insert(
            bootstrap_grant_id.clone(),
            WorldCapabilityGrant {
                grant_id: bootstrap_grant_id.clone(),
                world_id: spec.world_id.clone(),
                issued_by: created_by.clone(),
                subject: created_by.clone(),
                capabilities: all_capabilities(),
                resource_scope: WorldResourceScope::All,
                issued_at_ms: now_ms,
                expires_at_ms: None,
                revoked_at_ms: None,
            },
        );
        push_event(
            &mut record,
            self.event_capacity,
            now_ms,
            Some(created_by.clone()),
            None,
            None,
            None,
            WorldEventKind::WorldCreated {
                driver_id: spec.driver_id,
                ownership: spec.ownership,
                surface: spec.surface,
            },
        );
        push_event(
            &mut record,
            self.event_capacity,
            now_ms,
            Some(created_by.clone()),
            None,
            None,
            None,
            WorldEventKind::CapabilityGranted {
                grant_id: bootstrap_grant_id,
                subject: created_by,
            },
        );
        self.worlds.insert(spec.world_id, record);
        Ok(state)
    }

    pub fn world(&self, world_id: &WorldId) -> Result<WorldSession, WorldAuthorityError> {
        self.worlds
            .get(world_id)
            .map(|record| record.state.clone())
            .ok_or_else(|| WorldAuthorityError::WorldNotFound(world_id.clone()))
    }

    /// Remove a world after its owner has stopped and cleaned up the concrete
    /// driver. The in-memory journal and all outstanding permits disappear
    /// with the authority record.
    pub fn remove_world(
        &mut self,
        world_id: &WorldId,
        principal: &WorldPrincipal,
        now_ms: u64,
    ) -> Result<WorldSession, WorldAuthorityError> {
        let admin = self.principal_has_capability(
            world_id,
            principal,
            WorldCapability::Admin,
            &WorldResourceId::new("world"),
            now_ms,
        )?;
        if !admin {
            return Err(WorldAuthorityError::AdministrationDenied(
                principal.principal_id.to_string(),
            ));
        }
        self.worlds
            .remove(world_id)
            .map(|record| record.state)
            .ok_or_else(|| WorldAuthorityError::WorldNotFound(world_id.clone()))
    }

    pub fn grant_capabilities(
        &mut self,
        world_id: &WorldId,
        request: WorldGrantRequest,
        now_ms: u64,
    ) -> Result<WorldCapabilityGrant, WorldAuthorityError> {
        validate_principal(&request.issued_by)?;
        validate_principal(&request.subject)?;
        if request.grant_id.is_empty() {
            return Err(WorldAuthorityError::EmptyGrantId);
        }
        if request.capabilities.is_empty() {
            return Err(WorldAuthorityError::EmptyCapabilityGrant);
        }
        self.require_admin(world_id, &request.issued_by, now_ms)?;
        let record = self
            .worlds
            .get_mut(world_id)
            .ok_or_else(|| WorldAuthorityError::WorldNotFound(world_id.clone()))?;
        if record.grants.contains_key(&request.grant_id) {
            return Err(WorldAuthorityError::GrantAlreadyExists(request.grant_id));
        }

        let grant = WorldCapabilityGrant {
            grant_id: request.grant_id.clone(),
            world_id: world_id.clone(),
            issued_by: request.issued_by.clone(),
            subject: request.subject.clone(),
            capabilities: request.capabilities,
            resource_scope: request.resource_scope,
            issued_at_ms: now_ms,
            expires_at_ms: request.expires_at_ms,
            revoked_at_ms: None,
        };
        record.grants.insert(grant.grant_id.clone(), grant.clone());
        bump_revision(record, now_ms);
        push_event(
            record,
            self.event_capacity,
            now_ms,
            Some(request.issued_by),
            None,
            None,
            None,
            WorldEventKind::CapabilityGranted {
                grant_id: grant.grant_id.clone(),
                subject: grant.subject.clone(),
            },
        );
        Ok(grant)
    }

    pub fn revoke_grant(
        &mut self,
        world_id: &WorldId,
        grant_id: &WorldGrantId,
        revoked_by: &WorldPrincipal,
        now_ms: u64,
    ) -> Result<(), WorldAuthorityError> {
        self.require_admin(world_id, revoked_by, now_ms)?;
        let record = self
            .worlds
            .get_mut(world_id)
            .ok_or_else(|| WorldAuthorityError::WorldNotFound(world_id.clone()))?;
        let grant = record
            .grants
            .get_mut(grant_id)
            .ok_or_else(|| WorldAuthorityError::GrantNotFound(grant_id.clone()))?;
        grant.revoked_at_ms = Some(now_ms);
        bump_revision(record, now_ms);
        push_event(
            record,
            self.event_capacity,
            now_ms,
            Some(revoked_by.clone()),
            None,
            None,
            None,
            WorldEventKind::CapabilityRevoked {
                grant_id: grant_id.clone(),
            },
        );
        Ok(())
    }

    pub fn acquire_control(
        &mut self,
        world_id: &WorldId,
        principal: WorldPrincipal,
        expires_at_ms: Option<u64>,
        now_ms: u64,
    ) -> Result<WorldControlLease, WorldAuthorityError> {
        validate_principal(&principal)?;
        let admin = self.principal_has_capability(
            world_id,
            &principal,
            WorldCapability::Admin,
            &WorldResourceId::new("world"),
            now_ms,
        )?;
        let may_interact = admin
            || self.principal_has_any_capability(
                world_id,
                &principal,
                WorldCapability::Interact,
                now_ms,
            )?;
        if !may_interact {
            return Err(WorldAuthorityError::CapabilityDenied {
                principal: principal.principal_id.to_string(),
                capability: WorldCapability::Interact,
                resource: WorldResourceId::new("world"),
            });
        }

        let record = self
            .worlds
            .get_mut(world_id)
            .ok_or_else(|| WorldAuthorityError::WorldNotFound(world_id.clone()))?;
        let current = record
            .state
            .active_control_lease
            .as_ref()
            .filter(|lease| lease.is_active_at(now_ms))
            .cloned();
        if let Some(current) = &current {
            if current.principal == principal {
                return Ok(current.clone());
            }
            let human_preemption = principal.kind == WorldPrincipalKind::Human;
            let human_holder = current.principal.kind == WorldPrincipalKind::Human;
            if (human_holder && !human_preemption) || (!human_preemption && !admin) {
                return Err(WorldAuthorityError::ControlConflict {
                    holder: current.principal.principal_id.to_string(),
                });
            }
        }

        record.state.control_generation = record.state.control_generation.saturating_add(1);
        let lease = WorldControlLease {
            principal: principal.clone(),
            generation: record.state.control_generation,
            acquired_at_ms: now_ms,
            expires_at_ms,
        };
        record.state.active_control_lease = Some(lease.clone());
        bump_revision(record, now_ms);
        push_event(
            record,
            self.event_capacity,
            now_ms,
            Some(principal),
            None,
            None,
            None,
            WorldEventKind::ControlAcquired {
                generation: lease.generation,
                preempted: current.map(|lease| lease.principal),
            },
        );
        Ok(lease)
    }

    pub fn release_control(
        &mut self,
        world_id: &WorldId,
        principal: &WorldPrincipal,
        now_ms: u64,
    ) -> Result<u64, WorldAuthorityError> {
        let admin = self.principal_has_capability(
            world_id,
            principal,
            WorldCapability::Admin,
            &WorldResourceId::new("world"),
            now_ms,
        )?;
        let record = self
            .worlds
            .get_mut(world_id)
            .ok_or_else(|| WorldAuthorityError::WorldNotFound(world_id.clone()))?;
        if let Some(current) = record.state.active_control_lease.as_ref() {
            if current.principal != *principal && !admin {
                return Err(WorldAuthorityError::ControlHolderMismatch {
                    holder: current.principal.principal_id.to_string(),
                    principal: principal.principal_id.to_string(),
                });
            }
        } else {
            return Err(WorldAuthorityError::ControlRequired);
        }

        record.state.control_generation = record.state.control_generation.saturating_add(1);
        record.state.active_control_lease = None;
        bump_revision(record, now_ms);
        let generation = record.state.control_generation;
        push_event(
            record,
            self.event_capacity,
            now_ms,
            Some(principal.clone()),
            None,
            None,
            None,
            WorldEventKind::ControlReleased { generation },
        );
        Ok(generation)
    }

    pub fn admit_action(
        &mut self,
        world_id: &WorldId,
        intent: WorldActionIntent,
        now_ms: u64,
    ) -> Result<WorldAdmission, WorldAuthorityError> {
        self.admit_action_with_recipe_hint(world_id, intent, None, now_ms)
    }

    pub fn admit_action_with_recipe_hint(
        &mut self,
        world_id: &WorldId,
        intent: WorldActionIntent,
        recipe_hint: Option<WorldActionRecipeHint>,
        now_ms: u64,
    ) -> Result<WorldAdmission, WorldAuthorityError> {
        validate_principal(&intent.principal)?;
        if let Some(recipe_hint) = recipe_hint.as_ref() {
            validate_recipe_hint(recipe_hint)?;
        }
        if intent.intent_id.is_empty() {
            return Err(WorldAuthorityError::EmptyIntentId);
        }
        if intent.resource_id.is_empty() {
            return Err(WorldAuthorityError::EmptyResourceId);
        }
        let idempotency_key = intent.idempotency_key.trim();
        if idempotency_key.is_empty() {
            return Err(WorldAuthorityError::EmptyIdempotencyKey);
        }
        if intent.permit_expires_at_ms <= now_ms {
            return Err(WorldAuthorityError::InvalidPermitExpiry);
        }
        let required = intent.effect_class.required_capability();
        if intent.required_capability != required {
            return Err(WorldAuthorityError::EffectCapabilityMismatch {
                declared: intent.required_capability,
                required,
            });
        }

        let record = self
            .worlds
            .get_mut(world_id)
            .ok_or_else(|| WorldAuthorityError::WorldNotFound(world_id.clone()))?;
        expire_pending_permits(record, self.event_capacity, now_ms);
        if let Some(outcome) = record.completed_idempotency.get(idempotency_key) {
            if outcome.principal != intent.principal
                || outcome.resource_id != intent.resource_id
                || outcome.effect_class != intent.effect_class
            {
                return Err(WorldAuthorityError::IdempotencyConflict(
                    idempotency_key.to_string(),
                ));
            }
            return Ok(WorldAdmission::Replay {
                outcome: outcome.clone(),
            });
        }
        if record.pending_idempotency.contains_key(idempotency_key) {
            return Err(WorldAuthorityError::ActionAlreadyPending(
                idempotency_key.to_string(),
            ));
        }
        if record.pending.contains_key(&intent.intent_id) {
            return Err(WorldAuthorityError::IntentAlreadyPending(
                intent.intent_id.clone(),
            ));
        }
        if intent.effect_class.requires_control()
            && record.pending.values().any(|permit| {
                permit.effect_class.requires_control() && permit.resource_id == intent.resource_id
            })
        {
            return Err(WorldAuthorityError::ResourceActionAlreadyPending(
                intent.resource_id.clone(),
            ));
        }
        if intent.expected_revision != record.state.revision {
            return Err(WorldAuthorityError::StaleRevision {
                expected: intent.expected_revision,
                current: record.state.revision,
            });
        }

        let grant_id = record
            .grants
            .values()
            .find(|grant| grant.admits(&intent.principal, required, &intent.resource_id, now_ms))
            .map(|grant| grant.grant_id.clone())
            .ok_or_else(|| WorldAuthorityError::CapabilityDenied {
                principal: intent.principal.principal_id.to_string(),
                capability: required,
                resource: intent.resource_id.clone(),
            })?;

        let control_generation = if intent.effect_class.requires_control() {
            let expected = intent
                .expected_control_generation
                .ok_or(WorldAuthorityError::MissingControlGeneration)?;
            let lease = record
                .state
                .active_control_lease
                .as_ref()
                .filter(|lease| lease.is_active_at(now_ms))
                .ok_or(WorldAuthorityError::ControlRequired)?;
            if lease.principal != intent.principal {
                return Err(WorldAuthorityError::ControlHolderMismatch {
                    holder: lease.principal.principal_id.to_string(),
                    principal: intent.principal.principal_id.to_string(),
                });
            }
            if lease.generation != expected {
                return Err(WorldAuthorityError::StaleControlGeneration {
                    expected,
                    current: lease.generation,
                });
            }
            Some(expected)
        } else {
            None
        };

        let next_sequence = record.next_event_sequence;
        let checkpoint = WorldActionCheckpoint {
            schema_version: WORLD_ACTION_CHECKPOINT_SCHEMA_VERSION,
            surface: record.state.surface,
            world_revision: intent.expected_revision,
            control_generation,
            admitted_at_ms: now_ms,
            permit_expires_at_ms: intent.permit_expires_at_ms,
        };
        let recovery = intent.effect_class.recovery_plan();
        let permit = WorldActionPermit {
            world_id: world_id.clone(),
            driver_id: record.state.driver_id.clone(),
            intent_id: intent.intent_id.clone(),
            trace_id: intent.trace_id.clone(),
            principal: intent.principal.clone(),
            resource_id: intent.resource_id.clone(),
            grant_id: grant_id.clone(),
            admitted_revision: intent.expected_revision,
            control_generation,
            effect_class: intent.effect_class,
            idempotency_key: idempotency_key.to_string(),
            expires_at_ms: intent.permit_expires_at_ms,
            admitted_event_sequence: next_sequence,
            summary: intent.summary.clone(),
            checkpoint: checkpoint.clone(),
            recovery: recovery.clone(),
            recipe_hint: recipe_hint.clone(),
        };
        push_event(
            record,
            self.event_capacity,
            now_ms,
            Some(intent.principal),
            Some(intent.resource_id),
            Some(intent.intent_id.clone()),
            Some(intent.trace_id),
            WorldEventKind::ActionAdmitted {
                grant_id,
                effect_class: intent.effect_class,
                summary: intent.summary,
                checkpoint,
                recovery,
                recipe_hint,
            },
        );
        record
            .pending_idempotency
            .insert(idempotency_key.to_string(), intent.intent_id.clone());
        record.pending.insert(intent.intent_id, permit.clone());
        Ok(WorldAdmission::Admitted { permit })
    }

    pub fn complete_action(
        &mut self,
        permit: &WorldActionPermit,
        summary: impl Into<String>,
        now_ms: u64,
    ) -> Result<WorldActionOutcome, WorldAuthorityError> {
        self.finish_effectful_action(permit, summary.into(), false, now_ms)
    }

    pub fn mark_action_indeterminate(
        &mut self,
        permit: &WorldActionPermit,
        summary: impl Into<String>,
        now_ms: u64,
    ) -> Result<WorldActionOutcome, WorldAuthorityError> {
        self.finish_effectful_action(permit, summary.into(), true, now_ms)
    }

    pub fn fail_action(
        &mut self,
        permit: &WorldActionPermit,
        error: impl Into<String>,
        now_ms: u64,
    ) -> Result<WorldEvent, WorldAuthorityError> {
        let record = self
            .worlds
            .get_mut(&permit.world_id)
            .ok_or_else(|| WorldAuthorityError::WorldNotFound(permit.world_id.clone()))?;
        take_matching_permit(record, permit)?;
        Ok(push_event(
            record,
            self.event_capacity,
            now_ms,
            Some(permit.principal.clone()),
            Some(permit.resource_id.clone()),
            Some(permit.intent_id.clone()),
            Some(permit.trace_id.clone()),
            WorldEventKind::ActionFailed {
                effect_class: permit.effect_class,
                error: error.into(),
            },
        ))
    }

    pub fn record_external_mutation(
        &mut self,
        world_id: &WorldId,
        principal: WorldPrincipal,
        resource_id: WorldResourceId,
        summary: impl Into<String>,
        now_ms: u64,
    ) -> Result<WorldEvent, WorldAuthorityError> {
        validate_principal(&principal)?;
        if resource_id.is_empty() {
            return Err(WorldAuthorityError::EmptyResourceId);
        }
        let record = self
            .worlds
            .get_mut(world_id)
            .ok_or_else(|| WorldAuthorityError::WorldNotFound(world_id.clone()))?;
        bump_revision(record, now_ms);
        Ok(push_event(
            record,
            self.event_capacity,
            now_ms,
            Some(principal),
            Some(resource_id),
            None,
            None,
            WorldEventKind::ExternalMutationObserved {
                summary: summary.into(),
            },
        ))
    }

    pub fn events_after(
        &self,
        world_id: &WorldId,
        sequence: u64,
        limit: usize,
    ) -> Result<Vec<WorldEvent>, WorldAuthorityError> {
        let record = self
            .worlds
            .get(world_id)
            .ok_or_else(|| WorldAuthorityError::WorldNotFound(world_id.clone()))?;
        Ok(record
            .events
            .iter()
            .filter(|event| event.sequence > sequence)
            .take(limit)
            .cloned()
            .collect())
    }

    /// Capture the per-world event cursors before a mutation. The daemon uses
    /// these cursors to append only the events produced by that mutation to its
    /// durable cross-world ledger.
    pub fn event_cursors(&self) -> BTreeMap<WorldId, u64> {
        self.worlds
            .iter()
            .map(|(world_id, record)| {
                (
                    world_id.clone(),
                    record.next_event_sequence.saturating_sub(1),
                )
            })
            .collect()
    }

    pub fn event_envelopes_after(
        &self,
        cursors: &BTreeMap<WorldId, u64>,
    ) -> Vec<WorldEventEnvelope> {
        let mut envelopes = self
            .worlds
            .iter()
            .flat_map(|(world_id, record)| {
                let after = cursors.get(world_id).copied().unwrap_or(0);
                record
                    .events
                    .iter()
                    .filter(move |event| event.sequence > after)
                    .map(|event| WorldEventEnvelope {
                        schema_version: WORLD_EVENT_ENVELOPE_SCHEMA_VERSION,
                        authority_id: record.state.authority_id.clone(),
                        driver_id: record.state.driver_id.clone(),
                        ownership: record.state.ownership,
                        surface: record.state.surface,
                        event: event.clone(),
                    })
            })
            .collect::<Vec<_>>();
        envelopes.sort_by(|left, right| {
            left.event
                .at_ms
                .cmp(&right.event.at_ms)
                .then_with(|| left.event.world_id.cmp(&right.event.world_id))
                .then_with(|| left.event.sequence.cmp(&right.event.sequence))
        });
        envelopes
    }

    fn finish_effectful_action(
        &mut self,
        permit: &WorldActionPermit,
        summary: String,
        indeterminate: bool,
        now_ms: u64,
    ) -> Result<WorldActionOutcome, WorldAuthorityError> {
        let record = self
            .worlds
            .get_mut(&permit.world_id)
            .ok_or_else(|| WorldAuthorityError::WorldNotFound(permit.world_id.clone()))?;
        take_matching_permit(record, permit)?;

        let revision_matches = record.state.revision == permit.admitted_revision;
        let control_matches = match permit.control_generation {
            Some(generation) => record
                .state
                .active_control_lease
                .as_ref()
                .is_some_and(|lease| {
                    lease.generation == generation
                        && lease.principal == permit.principal
                        && lease.is_active_at(now_ms)
                }),
            None => true,
        };
        let status = if indeterminate {
            WorldActionStatus::Indeterminate
        } else if revision_matches && control_matches && permit.expires_at_ms > now_ms {
            WorldActionStatus::Confirmed
        } else {
            WorldActionStatus::NeedsReconciliation
        };
        bump_revision(record, now_ms);
        let event_sequence = record.next_event_sequence;
        let outcome = WorldActionOutcome {
            world_id: permit.world_id.clone(),
            driver_id: permit.driver_id.clone(),
            intent_id: permit.intent_id.clone(),
            trace_id: permit.trace_id.clone(),
            principal: permit.principal.clone(),
            resource_id: permit.resource_id.clone(),
            effect_class: permit.effect_class,
            status,
            committed_revision: record.state.revision,
            event_sequence,
            summary: summary.clone(),
            recovery: (status != WorldActionStatus::Confirmed).then(|| permit.recovery.clone()),
        };
        push_event(
            record,
            self.event_capacity,
            now_ms,
            Some(permit.principal.clone()),
            Some(permit.resource_id.clone()),
            Some(permit.intent_id.clone()),
            Some(permit.trace_id.clone()),
            WorldEventKind::ActionCommitted {
                effect_class: permit.effect_class,
                status,
                summary,
                recovery: (status != WorldActionStatus::Confirmed).then(|| permit.recovery.clone()),
            },
        );
        record
            .completed_idempotency
            .insert(permit.idempotency_key.clone(), outcome.clone());
        Ok(outcome)
    }

    fn require_admin(
        &self,
        world_id: &WorldId,
        principal: &WorldPrincipal,
        now_ms: u64,
    ) -> Result<(), WorldAuthorityError> {
        let has_admin = self.principal_has_capability(
            world_id,
            principal,
            WorldCapability::Admin,
            &WorldResourceId::new("world"),
            now_ms,
        )?;
        if has_admin {
            Ok(())
        } else {
            Err(WorldAuthorityError::AdministrationDenied(
                principal.principal_id.to_string(),
            ))
        }
    }

    fn principal_has_capability(
        &self,
        world_id: &WorldId,
        principal: &WorldPrincipal,
        capability: WorldCapability,
        resource_id: &WorldResourceId,
        now_ms: u64,
    ) -> Result<bool, WorldAuthorityError> {
        let record = self
            .worlds
            .get(world_id)
            .ok_or_else(|| WorldAuthorityError::WorldNotFound(world_id.clone()))?;
        Ok(record.grants.values().any(|grant| {
            grant.admits(principal, capability, resource_id, now_ms)
                || (capability == WorldCapability::Interact
                    && grant.admits(principal, WorldCapability::Admin, resource_id, now_ms))
        }))
    }

    fn principal_has_any_capability(
        &self,
        world_id: &WorldId,
        principal: &WorldPrincipal,
        capability: WorldCapability,
        now_ms: u64,
    ) -> Result<bool, WorldAuthorityError> {
        let record = self
            .worlds
            .get(world_id)
            .ok_or_else(|| WorldAuthorityError::WorldNotFound(world_id.clone()))?;
        Ok(record.grants.values().any(|grant| {
            grant.subject == *principal
                && grant.is_active_at(now_ms)
                && (grant.capabilities.contains(&capability)
                    || grant.capabilities.contains(&WorldCapability::Admin))
        }))
    }
}

fn all_capabilities() -> BTreeSet<WorldCapability> {
    [
        WorldCapability::Observe,
        WorldCapability::ObservePixels,
        WorldCapability::Interact,
        WorldCapability::ExternalEffect,
        WorldCapability::IrreversibleEffect,
        WorldCapability::UseCredentials,
        WorldCapability::ClipboardRead,
        WorldCapability::ClipboardWrite,
        WorldCapability::FileTransfer,
        WorldCapability::Debug,
        WorldCapability::Admin,
    ]
    .into_iter()
    .collect()
}

fn validate_world_id(world_id: &WorldId) -> Result<(), WorldAuthorityError> {
    if world_id.is_empty() {
        Err(WorldAuthorityError::EmptyWorldId)
    } else {
        Ok(())
    }
}

fn validate_principal(principal: &WorldPrincipal) -> Result<(), WorldAuthorityError> {
    if principal.principal_id.is_empty() {
        Err(WorldAuthorityError::EmptyPrincipalId)
    } else {
        Ok(())
    }
}

fn validate_recipe_hint(hint: &WorldActionRecipeHint) -> Result<(), WorldAuthorityError> {
    if hint.schema_version != WORLD_ACTION_RECIPE_HINT_SCHEMA_VERSION {
        return Err(WorldAuthorityError::InvalidRecipeHint(format!(
            "unsupported schema {}",
            hint.schema_version
        )));
    }
    if hint.operations.is_empty() || hint.operations.len() > MAX_RECIPE_OPERATIONS {
        return Err(WorldAuthorityError::InvalidRecipeHint(format!(
            "operations must contain 1 to {MAX_RECIPE_OPERATIONS} entries"
        )));
    }
    for operation in &hint.operations {
        let verb = operation.verb.trim();
        if verb.is_empty()
            || verb != operation.verb
            || verb.len() > MAX_RECIPE_VERB_BYTES
            || verb.contains('\0')
        {
            return Err(WorldAuthorityError::InvalidRecipeHint(
                "operation verb is missing or malformed".to_string(),
            ));
        }
        for (label, value) in [
            ("target role", operation.target_role.as_deref()),
            ("target name", operation.target_name.as_deref()),
        ] {
            let Some(value) = value else {
                continue;
            };
            if value.is_empty()
                || value.trim() != value
                || value.len() > MAX_RECIPE_SEMANTIC_TEXT_BYTES
                || value.contains('\0')
            {
                return Err(WorldAuthorityError::InvalidRecipeHint(format!(
                    "operation {label} is malformed"
                )));
            }
        }
    }
    Ok(())
}

fn bump_revision(record: &mut WorldRecord, now_ms: u64) {
    record.state.revision = record.state.revision.saturating_add(1);
    record.state.updated_at_ms = now_ms;
}

#[allow(clippy::too_many_arguments)]
fn push_event(
    record: &mut WorldRecord,
    capacity: usize,
    at_ms: u64,
    principal: Option<WorldPrincipal>,
    resource_id: Option<WorldResourceId>,
    intent_id: Option<WorldIntentId>,
    trace_id: Option<crate::model::WorldTraceId>,
    event: WorldEventKind,
) -> WorldEvent {
    let item = WorldEvent {
        sequence: record.next_event_sequence,
        world_id: record.state.world_id.clone(),
        world_revision: record.state.revision,
        at_ms,
        principal,
        resource_id,
        intent_id,
        trace_id,
        event,
    };
    record.next_event_sequence = record.next_event_sequence.saturating_add(1);
    record.events.push_back(item.clone());
    while record.events.len() > capacity {
        record.events.pop_front();
    }
    item
}

fn expire_pending_permits(record: &mut WorldRecord, capacity: usize, now_ms: u64) {
    let expired = record
        .pending
        .values()
        .filter(|permit| permit.expires_at_ms <= now_ms)
        .cloned()
        .collect::<Vec<_>>();
    for permit in expired {
        record.pending.remove(&permit.intent_id);
        record
            .pending_idempotency
            .remove(permit.idempotency_key.as_str());
        push_event(
            record,
            capacity,
            now_ms,
            Some(permit.principal),
            Some(permit.resource_id),
            Some(permit.intent_id),
            Some(permit.trace_id),
            WorldEventKind::ActionFailed {
                effect_class: permit.effect_class,
                error: "action permit expired before completion".to_string(),
            },
        );
    }
}

fn take_matching_permit(
    record: &mut WorldRecord,
    permit: &WorldActionPermit,
) -> Result<(), WorldAuthorityError> {
    let pending = record
        .pending
        .remove(&permit.intent_id)
        .ok_or_else(|| WorldAuthorityError::PermitNotPending(permit.intent_id.clone()))?;
    if pending != *permit {
        record.pending.insert(pending.intent_id.clone(), pending);
        return Err(WorldAuthorityError::PermitMismatch);
    }
    record
        .pending_idempotency
        .remove(permit.idempotency_key.as_str());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        WorldAuthorityId, WorldDriverId, WorldEffectClass, WorldOwnership, WorldPrincipalId,
        WorldSurfaceKind, WorldTraceId,
    };

    const NOW: u64 = 1_000;

    fn human() -> WorldPrincipal {
        WorldPrincipal::human(WorldPrincipalId::new("human:owner"))
    }

    fn agent() -> WorldPrincipal {
        WorldPrincipal::agent(WorldPrincipalId::new("agent:turn-1"))
    }

    fn world_id() -> WorldId {
        WorldId::new("world:test")
    }

    fn create_world(authority: &mut WorldAuthority) {
        authority
            .create_world(
                WorldSessionSpec {
                    world_id: world_id(),
                    authority_id: WorldAuthorityId::new("workshop:test"),
                    driver_id: WorldDriverId::new("driver:test"),
                    ownership: WorldOwnership::Managed,
                    surface: WorldSurfaceKind::Browser,
                },
                human(),
                NOW,
            )
            .unwrap();
    }

    fn grant_agent(authority: &mut WorldAuthority, capabilities: &[WorldCapability]) {
        authority
            .grant_capabilities(
                &world_id(),
                WorldGrantRequest {
                    grant_id: WorldGrantId::new("grant:agent"),
                    issued_by: human(),
                    subject: agent(),
                    capabilities: capabilities.iter().copied().collect(),
                    resource_scope: WorldResourceScope::All,
                    expires_at_ms: Some(NOW + 10_000),
                },
                NOW,
            )
            .unwrap();
    }

    fn admit_agent_action(
        authority: &mut WorldAuthority,
        idempotency_key: &str,
    ) -> WorldActionPermit {
        admit_agent_action_with_expiry(authority, idempotency_key, NOW + 1_000)
    }

    fn admit_agent_action_with_expiry(
        authority: &mut WorldAuthority,
        idempotency_key: &str,
        permit_expires_at_ms: u64,
    ) -> WorldActionPermit {
        let state = authority.world(&world_id()).unwrap();
        let lease = state.active_control_lease.unwrap();
        match authority
            .admit_action(
                &world_id(),
                WorldActionIntent {
                    intent_id: WorldIntentId::new(format!("intent:{idempotency_key}")),
                    trace_id: WorldTraceId::new("trace:turn-1"),
                    principal: agent(),
                    resource_id: WorldResourceId::new("tab:one"),
                    expected_revision: state.revision,
                    expected_control_generation: Some(lease.generation),
                    required_capability: WorldCapability::Interact,
                    effect_class: WorldEffectClass::LocalMutation,
                    idempotency_key: idempotency_key.to_string(),
                    permit_expires_at_ms,
                    summary: "click tab:one".to_string(),
                },
                NOW + 2,
            )
            .unwrap()
        {
            WorldAdmission::Admitted { permit } => permit,
            WorldAdmission::Replay { .. } => panic!("unexpected replay"),
        }
    }

    #[test]
    fn admission_emits_a_surface_checkpoint_and_fresh_recovery_plan() {
        let mut authority = WorldAuthority::default();
        create_world(&mut authority);
        grant_agent(&mut authority, &[WorldCapability::Interact]);
        authority
            .acquire_control(&world_id(), agent(), Some(NOW + 5_000), NOW + 1)
            .unwrap();

        let cursors = authority.event_cursors();
        let permit = admit_agent_action(&mut authority, "checkpointed-click");
        assert_eq!(permit.summary, "click tab:one");
        assert_eq!(permit.checkpoint.surface, WorldSurfaceKind::Browser);
        assert_eq!(
            permit.checkpoint.control_generation,
            permit.control_generation
        );
        assert_eq!(
            permit.recovery.strategy,
            crate::model::WorldRecoveryStrategy::ReconcileFromFreshObservation
        );
        assert!(permit.recovery.requires_fresh_admission);
        assert_eq!(
            permit.recovery.compensation.strategy,
            crate::model::WorldCompensationStrategy::OperatorDirected
        );
        assert!(!permit.recovery.compensation.automatic_dispatch_allowed);
        assert!(permit.recovery.compensation.requires_new_intent);

        let envelopes = authority.event_envelopes_after(&cursors);
        assert_eq!(envelopes.len(), 1);
        assert_eq!(envelopes[0].authority_id.as_str(), "workshop:test");
        assert_eq!(envelopes[0].driver_id.as_str(), "driver:test");
        assert!(matches!(
            &envelopes[0].event.event,
            WorldEventKind::ActionAdmitted { checkpoint, recovery, .. }
                if checkpoint == &permit.checkpoint && recovery == &permit.recovery
        ));
    }

    #[test]
    fn admission_validates_and_preserves_an_inert_recipe_hint() {
        let mut authority = WorldAuthority::default();
        create_world(&mut authority);
        grant_agent(&mut authority, &[WorldCapability::Interact]);
        authority
            .acquire_control(&world_id(), agent(), Some(NOW + 5_000), NOW + 1)
            .unwrap();
        let state = authority.world(&world_id()).unwrap();
        let lease = state.active_control_lease.unwrap();
        let intent = WorldActionIntent {
            intent_id: WorldIntentId::new("intent:recipe"),
            trace_id: WorldTraceId::new("trace:recipe"),
            principal: agent(),
            resource_id: WorldResourceId::new("tab:one"),
            expected_revision: state.revision,
            expected_control_generation: Some(lease.generation),
            required_capability: WorldCapability::Interact,
            effect_class: WorldEffectClass::LocalMutation,
            idempotency_key: "recipe-click".to_string(),
            permit_expires_at_ms: NOW + 1_000,
            summary: "click opaque ref".to_string(),
        };
        let hint = WorldActionRecipeHint {
            schema_version: WORLD_ACTION_RECIPE_HINT_SCHEMA_VERSION,
            operations: vec![crate::model::WorldRecipeOperationHint {
                verb: "click".to_string(),
                target_role: Some("button".to_string()),
                target_name: Some("Continue".to_string()),
                input_kind: None,
                requires_operator_confirmation: false,
            }],
        };

        let admission = authority
            .admit_action_with_recipe_hint(&world_id(), intent.clone(), Some(hint.clone()), NOW + 2)
            .expect("semantic hint is admitted");
        let WorldAdmission::Admitted { permit } = admission else {
            panic!("unexpected replay")
        };
        assert_eq!(permit.recipe_hint.as_ref(), Some(&hint));
        assert!(matches!(
            authority.events_after(&world_id(), 0, 20).unwrap().last(),
            Some(WorldEvent {
                event: WorldEventKind::ActionAdmitted {
                    recipe_hint: Some(event_hint),
                    ..
                },
                ..
            }) if event_hint == &hint
        ));

        let invalid = WorldActionRecipeHint {
            schema_version: WORLD_ACTION_RECIPE_HINT_SCHEMA_VERSION,
            operations: Vec::new(),
        };
        let mut invalid_intent = intent;
        invalid_intent.intent_id = WorldIntentId::new("intent:invalid-recipe");
        invalid_intent.idempotency_key = "invalid-recipe".to_string();
        assert!(matches!(
            authority.admit_action_with_recipe_hint(
                &world_id(),
                invalid_intent,
                Some(invalid),
                NOW + 3,
            ),
            Err(WorldAuthorityError::InvalidRecipeHint(_))
        ));
    }

    #[test]
    fn human_preempts_agent_and_late_success_requires_reconciliation() {
        let mut authority = WorldAuthority::default();
        create_world(&mut authority);
        grant_agent(&mut authority, &[WorldCapability::Interact]);
        authority
            .acquire_control(&world_id(), agent(), Some(NOW + 5_000), NOW + 1)
            .unwrap();
        let permit = admit_agent_action(&mut authority, "click-1");

        let lease = authority
            .acquire_control(&world_id(), human(), None, NOW + 3)
            .unwrap();
        assert_eq!(lease.principal, human());

        let outcome = authority
            .complete_action(&permit, "driver reported click", NOW + 4)
            .unwrap();
        assert_eq!(outcome.status, WorldActionStatus::NeedsReconciliation);
        assert_eq!(
            authority
                .world(&world_id())
                .unwrap()
                .active_control_lease
                .unwrap()
                .principal,
            human()
        );
    }

    #[test]
    fn human_takeover_permanently_fences_an_admitted_permit() {
        let mut authority = WorldAuthority::default();
        create_world(&mut authority);
        grant_agent(&mut authority, &[WorldCapability::Interact]);
        authority
            .acquire_control(&world_id(), agent(), Some(NOW + 5_000), NOW + 1)
            .unwrap();
        let permit = admit_agent_action(&mut authority, "click-before-takeover");
        authority.validate_action_permit(&permit, NOW + 3).unwrap();

        authority
            .acquire_control(&world_id(), human(), None, NOW + 4)
            .unwrap();
        authority
            .release_control(&world_id(), &human(), NOW + 5)
            .unwrap();

        assert!(matches!(
            authority.validate_action_permit(&permit, NOW + 6),
            Err(WorldAuthorityError::StaleRevision { .. })
                | Err(WorldAuthorityError::StaleControlGeneration { .. })
        ));
    }

    #[test]
    fn expired_control_lease_fences_a_still_live_permit() {
        let mut authority = WorldAuthority::default();
        create_world(&mut authority);
        grant_agent(&mut authority, &[WorldCapability::Interact]);
        authority
            .acquire_control(&world_id(), agent(), Some(NOW + 10), NOW + 1)
            .unwrap();
        let permit = admit_agent_action_with_expiry(&mut authority, "lease-expiry", NOW + 1_000);

        assert_eq!(
            authority
                .validate_action_permit(&permit, NOW + 11)
                .unwrap_err(),
            WorldAuthorityError::ControlRequired
        );
    }

    #[test]
    fn mutating_permits_serialize_per_resource_and_expired_work_is_reaped() {
        let mut authority = WorldAuthority::default();
        create_world(&mut authority);
        grant_agent(&mut authority, &[WorldCapability::Interact]);
        let lease = authority
            .acquire_control(&world_id(), agent(), Some(NOW + 5_000), NOW + 1)
            .unwrap();
        let first = admit_agent_action_with_expiry(&mut authority, "first", NOW + 10);
        let state = authority.world(&world_id()).unwrap();
        let second_intent =
            |intent_id: &str, idempotency_key: &str, expires_at_ms: u64| WorldActionIntent {
                intent_id: WorldIntentId::new(intent_id),
                trace_id: WorldTraceId::new("trace:serialized"),
                principal: agent(),
                resource_id: first.resource_id.clone(),
                expected_revision: state.revision,
                expected_control_generation: Some(lease.generation),
                required_capability: WorldCapability::Interact,
                effect_class: WorldEffectClass::LocalMutation,
                idempotency_key: idempotency_key.to_string(),
                permit_expires_at_ms: expires_at_ms,
                summary: "second click".to_string(),
            };

        assert_eq!(
            authority
                .admit_action(
                    &world_id(),
                    second_intent("intent:second", "second", NOW + 1_000),
                    NOW + 3,
                )
                .unwrap_err(),
            WorldAuthorityError::ResourceActionAlreadyPending(first.resource_id.clone())
        );
        assert!(matches!(
            authority
                .admit_action(
                    &world_id(),
                    second_intent("intent:after-expiry", "after-expiry", NOW + 2_000),
                    NOW + 11,
                )
                .unwrap(),
            WorldAdmission::Admitted { .. }
        ));
    }

    #[test]
    fn agent_cannot_preempt_human_control() {
        let mut authority = WorldAuthority::default();
        create_world(&mut authority);
        grant_agent(&mut authority, &[WorldCapability::Interact]);
        authority
            .acquire_control(&world_id(), human(), None, NOW + 1)
            .unwrap();

        let error = authority
            .acquire_control(&world_id(), agent(), Some(NOW + 100), NOW + 2)
            .unwrap_err();
        assert_eq!(
            error,
            WorldAuthorityError::ControlConflict {
                holder: "human:owner".to_string()
            }
        );
    }

    #[test]
    fn system_admin_cannot_silently_preempt_human_control() {
        let mut authority = WorldAuthority::default();
        create_world(&mut authority);
        let system = WorldPrincipal::system(WorldPrincipalId::new("system:runtime"));
        authority
            .grant_capabilities(
                &world_id(),
                WorldGrantRequest {
                    grant_id: WorldGrantId::new("grant:system-admin"),
                    issued_by: human(),
                    subject: system.clone(),
                    capabilities: [WorldCapability::Admin].into_iter().collect(),
                    resource_scope: WorldResourceScope::All,
                    expires_at_ms: None,
                },
                NOW,
            )
            .unwrap();
        authority
            .acquire_control(&world_id(), human(), None, NOW + 1)
            .unwrap();

        let error = authority
            .acquire_control(&world_id(), system, None, NOW + 2)
            .unwrap_err();
        assert_eq!(
            error,
            WorldAuthorityError::ControlConflict {
                holder: "human:owner".to_string()
            }
        );
    }

    #[test]
    fn stale_revision_fails_before_dispatch() {
        let mut authority = WorldAuthority::default();
        create_world(&mut authority);
        grant_agent(&mut authority, &[WorldCapability::Interact]);
        let lease = authority
            .acquire_control(&world_id(), agent(), Some(NOW + 5_000), NOW + 1)
            .unwrap();
        let current = authority.world(&world_id()).unwrap();

        let error = authority
            .admit_action(
                &world_id(),
                WorldActionIntent {
                    intent_id: WorldIntentId::new("intent:stale"),
                    trace_id: WorldTraceId::new("trace:stale"),
                    principal: agent(),
                    resource_id: WorldResourceId::new("tab:one"),
                    expected_revision: current.revision.saturating_sub(1),
                    expected_control_generation: Some(lease.generation),
                    required_capability: WorldCapability::Interact,
                    effect_class: WorldEffectClass::LocalMutation,
                    idempotency_key: "stale".to_string(),
                    permit_expires_at_ms: NOW + 1_000,
                    summary: "stale click".to_string(),
                },
                NOW + 2,
            )
            .unwrap_err();
        assert_eq!(
            error,
            WorldAuthorityError::StaleRevision {
                expected: current.revision - 1,
                current: current.revision,
            }
        );
    }

    #[test]
    fn external_effect_requires_an_explicit_capability() {
        let mut authority = WorldAuthority::default();
        create_world(&mut authority);
        grant_agent(&mut authority, &[WorldCapability::Interact]);
        let lease = authority
            .acquire_control(&world_id(), agent(), Some(NOW + 5_000), NOW + 1)
            .unwrap();
        let state = authority.world(&world_id()).unwrap();

        let error = authority
            .admit_action(
                &world_id(),
                WorldActionIntent {
                    intent_id: WorldIntentId::new("intent:submit"),
                    trace_id: WorldTraceId::new("trace:submit"),
                    principal: agent(),
                    resource_id: WorldResourceId::new("tab:one"),
                    expected_revision: state.revision,
                    expected_control_generation: Some(lease.generation),
                    required_capability: WorldCapability::ExternalEffect,
                    effect_class: WorldEffectClass::ExternalEffect,
                    idempotency_key: "submit".to_string(),
                    permit_expires_at_ms: NOW + 1_000,
                    summary: "submit form".to_string(),
                },
                NOW + 2,
            )
            .unwrap_err();
        assert!(matches!(
            error,
            WorldAuthorityError::CapabilityDenied {
                capability: WorldCapability::ExternalEffect,
                ..
            }
        ));
    }

    #[test]
    fn completed_idempotency_key_replays_without_a_second_permit() {
        let mut authority = WorldAuthority::default();
        create_world(&mut authority);
        grant_agent(&mut authority, &[WorldCapability::Interact]);
        authority
            .acquire_control(&world_id(), agent(), Some(NOW + 5_000), NOW + 1)
            .unwrap();
        let permit = admit_agent_action(&mut authority, "click-once");
        let outcome = authority
            .complete_action(&permit, "clicked", NOW + 3)
            .unwrap();
        let state = authority.world(&world_id()).unwrap();

        let replay = authority
            .admit_action(
                &world_id(),
                WorldActionIntent {
                    intent_id: WorldIntentId::new("intent:different"),
                    trace_id: WorldTraceId::new("trace:different"),
                    principal: agent(),
                    resource_id: WorldResourceId::new("tab:one"),
                    expected_revision: state.revision,
                    expected_control_generation: state
                        .active_control_lease
                        .as_ref()
                        .map(|lease| lease.generation),
                    required_capability: WorldCapability::Interact,
                    effect_class: WorldEffectClass::LocalMutation,
                    idempotency_key: "click-once".to_string(),
                    permit_expires_at_ms: NOW + 1_000,
                    summary: "click again".to_string(),
                },
                NOW + 4,
            )
            .unwrap();
        assert_eq!(replay, WorldAdmission::Replay { outcome });
    }

    #[test]
    fn completed_idempotency_key_cannot_be_reused_for_another_resource() {
        let mut authority = WorldAuthority::default();
        create_world(&mut authority);
        grant_agent(&mut authority, &[WorldCapability::Interact]);
        authority
            .acquire_control(&world_id(), agent(), Some(NOW + 5_000), NOW + 1)
            .unwrap();
        let permit = admit_agent_action(&mut authority, "click-once");
        authority
            .complete_action(&permit, "clicked", NOW + 3)
            .unwrap();
        let state = authority.world(&world_id()).unwrap();

        let error = authority
            .admit_action(
                &world_id(),
                WorldActionIntent {
                    intent_id: WorldIntentId::new("intent:other-resource"),
                    trace_id: WorldTraceId::new("trace:other-resource"),
                    principal: agent(),
                    resource_id: WorldResourceId::new("tab:two"),
                    expected_revision: state.revision,
                    expected_control_generation: state
                        .active_control_lease
                        .as_ref()
                        .map(|lease| lease.generation),
                    required_capability: WorldCapability::Interact,
                    effect_class: WorldEffectClass::LocalMutation,
                    idempotency_key: "click-once".to_string(),
                    permit_expires_at_ms: NOW + 1_000,
                    summary: "click another tab".to_string(),
                },
                NOW + 4,
            )
            .unwrap_err();
        assert_eq!(
            error,
            WorldAuthorityError::IdempotencyConflict("click-once".to_string())
        );
    }

    #[test]
    fn pending_intent_id_cannot_overwrite_an_existing_permit() {
        let mut authority = WorldAuthority::default();
        create_world(&mut authority);
        grant_agent(&mut authority, &[WorldCapability::Interact]);
        authority
            .acquire_control(&world_id(), agent(), Some(NOW + 5_000), NOW + 1)
            .unwrap();
        let permit = admit_agent_action(&mut authority, "first-key");
        let state = authority.world(&world_id()).unwrap();

        let error = authority
            .admit_action(
                &world_id(),
                WorldActionIntent {
                    intent_id: permit.intent_id.clone(),
                    trace_id: WorldTraceId::new("trace:duplicate-intent"),
                    principal: agent(),
                    resource_id: WorldResourceId::new("tab:one"),
                    expected_revision: state.revision,
                    expected_control_generation: state
                        .active_control_lease
                        .as_ref()
                        .map(|lease| lease.generation),
                    required_capability: WorldCapability::Interact,
                    effect_class: WorldEffectClass::LocalMutation,
                    idempotency_key: "second-key".to_string(),
                    permit_expires_at_ms: NOW + 1_000,
                    summary: "duplicate intent".to_string(),
                },
                NOW + 3,
            )
            .unwrap_err();
        assert_eq!(
            error,
            WorldAuthorityError::IntentAlreadyPending(permit.intent_id)
        );
    }

    #[test]
    fn permit_that_finishes_after_expiry_requires_reconciliation() {
        let mut authority = WorldAuthority::default();
        create_world(&mut authority);
        grant_agent(&mut authority, &[WorldCapability::Interact]);
        authority
            .acquire_control(&world_id(), agent(), Some(NOW + 5_000), NOW + 1)
            .unwrap();
        let permit = admit_agent_action_with_expiry(&mut authority, "slow", NOW + 3);

        let outcome = authority
            .complete_action(&permit, "late driver success", NOW + 4)
            .unwrap();
        assert_eq!(outcome.status, WorldActionStatus::NeedsReconciliation);
    }

    #[test]
    fn exact_resource_grant_does_not_authorize_another_resource() {
        let mut authority = WorldAuthority::default();
        create_world(&mut authority);
        authority
            .grant_capabilities(
                &world_id(),
                WorldGrantRequest {
                    grant_id: WorldGrantId::new("grant:exact"),
                    issued_by: human(),
                    subject: agent(),
                    capabilities: [WorldCapability::Observe].into_iter().collect(),
                    resource_scope: WorldResourceScope::exact([WorldResourceId::new("tab:one")]),
                    expires_at_ms: Some(NOW + 1_000),
                },
                NOW,
            )
            .unwrap();
        let state = authority.world(&world_id()).unwrap();

        let error = authority
            .admit_action(
                &world_id(),
                WorldActionIntent {
                    intent_id: WorldIntentId::new("intent:other-tab"),
                    trace_id: WorldTraceId::new("trace:other-tab"),
                    principal: agent(),
                    resource_id: WorldResourceId::new("tab:two"),
                    expected_revision: state.revision,
                    expected_control_generation: None,
                    required_capability: WorldCapability::Observe,
                    effect_class: WorldEffectClass::Observe,
                    idempotency_key: "observe-other-tab".to_string(),
                    permit_expires_at_ms: NOW + 100,
                    summary: "observe another tab".to_string(),
                },
                NOW + 1,
            )
            .unwrap_err();
        assert!(matches!(
            error,
            WorldAuthorityError::CapabilityDenied {
                resource,
                capability: WorldCapability::Observe,
                ..
            } if resource == WorldResourceId::new("tab:two")
        ));
    }

    #[test]
    fn pixel_observation_is_distinct_and_does_not_require_control() {
        let mut authority = WorldAuthority::default();
        create_world(&mut authority);
        grant_agent(&mut authority, &[WorldCapability::Observe]);
        let state = authority.world(&world_id()).unwrap();
        let intent = WorldActionIntent {
            intent_id: WorldIntentId::new("intent:pixels"),
            trace_id: WorldTraceId::new("trace:pixels"),
            principal: agent(),
            resource_id: WorldResourceId::new("tab:one"),
            expected_revision: state.revision,
            expected_control_generation: None,
            required_capability: WorldCapability::ObservePixels,
            effect_class: WorldEffectClass::ObservePixels,
            idempotency_key: "observe-pixels".to_string(),
            permit_expires_at_ms: NOW + 100,
            summary: "capture viewport".to_string(),
        };

        let error = authority
            .admit_action(&world_id(), intent.clone(), NOW + 1)
            .unwrap_err();
        assert!(matches!(
            error,
            WorldAuthorityError::CapabilityDenied {
                capability: WorldCapability::ObservePixels,
                ..
            }
        ));

        authority
            .grant_capabilities(
                &world_id(),
                WorldGrantRequest {
                    grant_id: WorldGrantId::new("grant:pixels"),
                    issued_by: human(),
                    subject: agent(),
                    capabilities: [WorldCapability::ObservePixels].into_iter().collect(),
                    resource_scope: WorldResourceScope::All,
                    expires_at_ms: Some(NOW + 1_000),
                },
                NOW + 1,
            )
            .unwrap();
        let state = authority.world(&world_id()).unwrap();
        let admission = authority
            .admit_action(
                &world_id(),
                WorldActionIntent {
                    intent_id: WorldIntentId::new("intent:pixels-admitted"),
                    expected_revision: state.revision,
                    idempotency_key: "observe-pixels-admitted".to_string(),
                    ..intent
                },
                NOW + 2,
            )
            .unwrap();
        assert!(matches!(admission, WorldAdmission::Admitted { .. }));
    }

    #[test]
    fn expired_grant_is_not_admitted() {
        let mut authority = WorldAuthority::default();
        create_world(&mut authority);
        authority
            .grant_capabilities(
                &world_id(),
                WorldGrantRequest {
                    grant_id: WorldGrantId::new("grant:short"),
                    issued_by: human(),
                    subject: agent(),
                    capabilities: [WorldCapability::Observe].into_iter().collect(),
                    resource_scope: WorldResourceScope::All,
                    expires_at_ms: Some(NOW + 1),
                },
                NOW,
            )
            .unwrap();
        let state = authority.world(&world_id()).unwrap();

        let error = authority
            .admit_action(
                &world_id(),
                WorldActionIntent {
                    intent_id: WorldIntentId::new("intent:expired-grant"),
                    trace_id: WorldTraceId::new("trace:expired-grant"),
                    principal: agent(),
                    resource_id: WorldResourceId::new("tab:one"),
                    expected_revision: state.revision,
                    expected_control_generation: None,
                    required_capability: WorldCapability::Observe,
                    effect_class: WorldEffectClass::Observe,
                    idempotency_key: "expired-grant".to_string(),
                    permit_expires_at_ms: NOW + 100,
                    summary: "observe with expired grant".to_string(),
                },
                NOW + 2,
            )
            .unwrap_err();
        assert!(matches!(
            error,
            WorldAuthorityError::CapabilityDenied {
                capability: WorldCapability::Observe,
                ..
            }
        ));
    }

    #[test]
    fn journal_is_bounded_but_sequence_remains_monotonic() {
        let mut authority = WorldAuthority::new(3);
        create_world(&mut authority);
        authority
            .record_external_mutation(
                &world_id(),
                human(),
                WorldResourceId::new("tab:one"),
                "one",
                NOW + 1,
            )
            .unwrap();
        authority
            .record_external_mutation(
                &world_id(),
                human(),
                WorldResourceId::new("tab:one"),
                "two",
                NOW + 2,
            )
            .unwrap();

        let events = authority.events_after(&world_id(), 0, 10).unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].sequence, 2);
        assert_eq!(events[2].sequence, 4);
    }
}
