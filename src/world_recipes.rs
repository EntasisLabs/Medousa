//! Secret-free semantic recipes derived from confirmed governed-world traces.
//!
//! Recipes are intentionally inert. They preserve enough semantic intent for
//! a future runner to resolve a fresh target and request fresh admission, but
//! they never carry permits, grants, control generations, selectors, native
//! handles, coordinates, or user-supplied values from the source trace.

use std::collections::BTreeMap;
use std::fmt;

use medousa_types::{
    WORLD_RECIPE_SCHEMA_VERSION, WorldRecipe, WorldRecipeInputKind, WorldRecipeOperation,
    WorldRecipeStep,
};
use medousa_world::{
    WORLD_ACTION_RECIPE_HINT_SCHEMA_VERSION, WorldActionRecipeHint, WorldActionStatus,
    WorldEffectClass, WorldEventKind, WorldRecipeInputKind as AuthorityRecipeInputKind,
    WorldSurfaceKind,
};
use sha2::{Digest as _, Sha256};

use crate::world_trace_store::DurableWorldEvent;

const MAX_RECIPE_STEPS: usize = 128;
const MAX_RECIPE_OPERATIONS: usize = 16;
const MAX_RECIPE_VERB_BYTES: usize = 64;
const MAX_RECIPE_SEMANTIC_TEXT_BYTES: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorldRecipeDerivationError {
    TraceNotFound,
    NotReplayable(String),
}

impl fmt::Display for WorldRecipeDerivationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TraceNotFound => formatter.write_str("world trace was not found"),
            Self::NotReplayable(reason) => {
                write!(formatter, "world trace is not replayable: {reason}")
            }
        }
    }
}

impl std::error::Error for WorldRecipeDerivationError {}

#[derive(Debug, Clone)]
struct AdmittedStep {
    record: DurableWorldEvent,
    effect_class: WorldEffectClass,
    recipe_hint: WorldActionRecipeHint,
}

#[derive(Debug, Clone, Copy)]
enum TerminalKind {
    Committed(WorldActionStatus),
    Failed,
    Interrupted(WorldActionStatus),
}

#[derive(Debug, Clone, Copy)]
struct TerminalStep {
    ledger_sequence: u64,
    effect_class: WorldEffectClass,
    kind: TerminalKind,
}

pub fn derive_world_recipe(
    trace_id: &str,
    records: &[DurableWorldEvent],
) -> Result<WorldRecipe, WorldRecipeDerivationError> {
    let mut trace_records = records
        .iter()
        .filter(|record| {
            record
                .envelope
                .event
                .trace_id
                .as_ref()
                .is_some_and(|candidate| candidate.as_str() == trace_id)
        })
        .cloned()
        .collect::<Vec<_>>();
    trace_records.sort_by_key(|record| record.ledger_sequence);
    if trace_records.is_empty() {
        return Err(WorldRecipeDerivationError::TraceNotFound);
    }

    let mut admissions = BTreeMap::<(String, String), AdmittedStep>::new();
    let mut terminals = BTreeMap::<(String, String), TerminalStep>::new();
    for record in &trace_records {
        let event = &record.envelope.event;
        let Some(intent_id) = event.intent_id.as_ref() else {
            continue;
        };
        let key = (event.world_id.to_string(), intent_id.to_string());
        match &event.event {
            WorldEventKind::ActionAdmitted {
                effect_class,
                recipe_hint,
                ..
            } if effect_is_replayable_candidate(*effect_class) => {
                let Some(recipe_hint) = recipe_hint.clone() else {
                    return Err(WorldRecipeDerivationError::NotReplayable(format!(
                        "admission {} has no semantic recipe metadata",
                        record.ledger_sequence
                    )));
                };
                validate_persisted_recipe_hint(&recipe_hint, record.ledger_sequence)?;
                if admissions
                    .insert(
                        key,
                        AdmittedStep {
                            record: record.clone(),
                            effect_class: *effect_class,
                            recipe_hint,
                        },
                    )
                    .is_some()
                {
                    return Err(WorldRecipeDerivationError::NotReplayable(
                        "the trace contains duplicate action admissions".to_string(),
                    ));
                }
            }
            WorldEventKind::ActionCommitted {
                effect_class,
                status,
                ..
            } if effect_is_replayable_candidate(*effect_class) => {
                insert_terminal(
                    &mut terminals,
                    key,
                    TerminalStep {
                        ledger_sequence: record.ledger_sequence,
                        effect_class: *effect_class,
                        kind: TerminalKind::Committed(*status),
                    },
                )?;
            }
            WorldEventKind::ActionFailed { effect_class, .. }
                if effect_is_replayable_candidate(*effect_class) =>
            {
                insert_terminal(
                    &mut terminals,
                    key,
                    TerminalStep {
                        ledger_sequence: record.ledger_sequence,
                        effect_class: *effect_class,
                        kind: TerminalKind::Failed,
                    },
                )?;
            }
            WorldEventKind::ActionInterrupted {
                effect_class,
                status,
                ..
            } if effect_is_replayable_candidate(*effect_class) => {
                insert_terminal(
                    &mut terminals,
                    key,
                    TerminalStep {
                        ledger_sequence: record.ledger_sequence,
                        effect_class: *effect_class,
                        kind: TerminalKind::Interrupted(*status),
                    },
                )?;
            }
            _ => {}
        }
    }

    if admissions.is_empty() && terminals.is_empty() {
        return Err(WorldRecipeDerivationError::NotReplayable(
            "the trace has no effectful world actions".to_string(),
        ));
    }
    if admissions.len() > MAX_RECIPE_STEPS {
        return Err(WorldRecipeDerivationError::NotReplayable(format!(
            "the trace exceeds the {MAX_RECIPE_STEPS}-step recipe limit"
        )));
    }
    if terminals.keys().any(|key| !admissions.contains_key(key)) {
        return Err(WorldRecipeDerivationError::NotReplayable(
            "the retained trace is missing an action admission".to_string(),
        ));
    }

    let mut admitted = admissions.into_values().collect::<Vec<_>>();
    admitted.sort_by_key(|step| step.record.ledger_sequence);
    let mut steps = Vec::with_capacity(admitted.len());
    for (index, admission) in admitted.into_iter().enumerate() {
        let event = &admission.record.envelope.event;
        let intent_id = event
            .intent_id
            .as_ref()
            .expect("effectful admission has an intent id");
        let key = (event.world_id.to_string(), intent_id.to_string());
        let terminal = terminals.get(&key).ok_or_else(|| {
            WorldRecipeDerivationError::NotReplayable(format!(
                "admission {} has no terminal receipt",
                admission.record.ledger_sequence
            ))
        })?;
        if terminal.ledger_sequence <= admission.record.ledger_sequence {
            return Err(WorldRecipeDerivationError::NotReplayable(
                "an action receipt precedes its admission".to_string(),
            ));
        }
        if terminal.effect_class != admission.effect_class {
            return Err(WorldRecipeDerivationError::NotReplayable(
                "an action receipt changed effect class".to_string(),
            ));
        }
        match terminal.kind {
            TerminalKind::Committed(WorldActionStatus::Confirmed) => {}
            TerminalKind::Committed(status) | TerminalKind::Interrupted(status) => {
                return Err(WorldRecipeDerivationError::NotReplayable(format!(
                    "action {} ended with {}",
                    intent_id,
                    action_status_label(status)
                )));
            }
            TerminalKind::Failed => {
                return Err(WorldRecipeDerivationError::NotReplayable(format!(
                    "action {intent_id} failed"
                )));
            }
        }

        let operations = admission
            .recipe_hint
            .operations
            .into_iter()
            .map(|operation| WorldRecipeOperation {
                verb: operation.verb,
                target_role: operation.target_role,
                target_name: operation.target_name,
                input_kind: operation.input_kind.map(public_input_kind),
                requires_operator_confirmation: operation.requires_operator_confirmation,
            })
            .collect::<Vec<_>>();
        let requires_operator_confirmation = matches!(
            admission.effect_class,
            WorldEffectClass::ExternalEffect | WorldEffectClass::Irreversible
        ) || operations
            .iter()
            .any(|operation| operation.requires_operator_confirmation);
        steps.push(WorldRecipeStep {
            ordinal: u32::try_from(index.saturating_add(1)).unwrap_or(u32::MAX),
            source_admission_sequence: admission.record.ledger_sequence,
            source_completion_sequence: terminal.ledger_sequence,
            source_intent_id: intent_id.to_string(),
            surface: surface_label(admission.record.envelope.surface).to_string(),
            effect_class: effect_label(admission.effect_class).to_string(),
            operations,
            requires_fresh_observation: true,
            requires_fresh_admission: true,
            requires_operator_confirmation,
        });
    }

    let source_start_sequence = trace_records
        .first()
        .map_or(0, |record| record.ledger_sequence);
    let source_end_sequence = trace_records
        .last()
        .map_or(source_start_sequence, |record| record.ledger_sequence);
    let digest_material = serde_json::to_vec(&(
        WORLD_RECIPE_SCHEMA_VERSION,
        trace_id,
        source_start_sequence,
        source_end_sequence,
        &steps,
        "fresh_semantic_replay",
    ))
    .map_err(|error| {
        WorldRecipeDerivationError::NotReplayable(format!(
            "could not encode recipe identity: {error}"
        ))
    })?;
    let digest = Sha256::digest(digest_material);

    Ok(WorldRecipe {
        schema_version: WORLD_RECIPE_SCHEMA_VERSION,
        recipe_id: format!("world-recipe:sha256:{digest:x}"),
        source_trace_id: trace_id.to_string(),
        source_start_sequence,
        source_end_sequence,
        steps,
        execution_model: "fresh_semantic_replay".to_string(),
        carries_authority: false,
        automatic_dispatch_allowed: false,
    })
}

fn insert_terminal(
    terminals: &mut BTreeMap<(String, String), TerminalStep>,
    key: (String, String),
    terminal: TerminalStep,
) -> Result<(), WorldRecipeDerivationError> {
    if terminals.insert(key, terminal).is_some() {
        return Err(WorldRecipeDerivationError::NotReplayable(
            "the trace contains duplicate action receipts".to_string(),
        ));
    }
    Ok(())
}

fn validate_persisted_recipe_hint(
    hint: &WorldActionRecipeHint,
    admission_sequence: u64,
) -> Result<(), WorldRecipeDerivationError> {
    if hint.schema_version != WORLD_ACTION_RECIPE_HINT_SCHEMA_VERSION
        || hint.operations.is_empty()
        || hint.operations.len() > MAX_RECIPE_OPERATIONS
    {
        return Err(WorldRecipeDerivationError::NotReplayable(format!(
            "admission {admission_sequence} has unsupported semantic recipe metadata"
        )));
    }
    for operation in &hint.operations {
        if operation.verb.is_empty()
            || operation.verb.trim() != operation.verb
            || operation.verb.len() > MAX_RECIPE_VERB_BYTES
            || operation.verb.contains('\0')
        {
            return Err(WorldRecipeDerivationError::NotReplayable(format!(
                "admission {admission_sequence} has malformed semantic recipe metadata"
            )));
        }
        if [
            operation.target_role.as_deref(),
            operation.target_name.as_deref(),
        ]
        .into_iter()
        .flatten()
        .any(|value| {
            value.is_empty()
                || value.trim() != value
                || value.len() > MAX_RECIPE_SEMANTIC_TEXT_BYTES
                || value.contains('\0')
        })
        {
            return Err(WorldRecipeDerivationError::NotReplayable(format!(
                "admission {admission_sequence} has malformed semantic target metadata"
            )));
        }
    }
    Ok(())
}

fn effect_is_replayable_candidate(effect: WorldEffectClass) -> bool {
    !matches!(
        effect,
        WorldEffectClass::Observe | WorldEffectClass::ObservePixels
    )
}

fn public_input_kind(kind: AuthorityRecipeInputKind) -> WorldRecipeInputKind {
    match kind {
        AuthorityRecipeInputKind::Text => WorldRecipeInputKind::Text,
        AuthorityRecipeInputKind::Selection => WorldRecipeInputKind::Selection,
        AuthorityRecipeInputKind::Key => WorldRecipeInputKind::Key,
        AuthorityRecipeInputKind::ScrollDelta => WorldRecipeInputKind::ScrollDelta,
        AuthorityRecipeInputKind::WaitDuration => WorldRecipeInputKind::WaitDuration,
    }
}

fn surface_label(surface: WorldSurfaceKind) -> &'static str {
    match surface {
        WorldSurfaceKind::Browser => "browser",
        WorldSurfaceKind::Desktop => "desktop",
        WorldSurfaceKind::Application => "application",
        WorldSurfaceKind::Terminal => "terminal",
        WorldSurfaceKind::Composite => "composite",
    }
}

fn effect_label(effect: WorldEffectClass) -> &'static str {
    match effect {
        WorldEffectClass::Observe => "observe",
        WorldEffectClass::ObservePixels => "observe_pixels",
        WorldEffectClass::LocalReversible => "local_reversible",
        WorldEffectClass::LocalMutation => "local_mutation",
        WorldEffectClass::ExternalEffect => "external_effect",
        WorldEffectClass::Irreversible => "irreversible",
    }
}

fn action_status_label(status: WorldActionStatus) -> &'static str {
    match status {
        WorldActionStatus::Confirmed => "confirmed",
        WorldActionStatus::NeedsReconciliation => "needs_reconciliation",
        WorldActionStatus::Indeterminate => "indeterminate",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use medousa_world::{
        WORLD_ACTION_CHECKPOINT_SCHEMA_VERSION, WORLD_EVENT_ENVELOPE_SCHEMA_VERSION,
        WorldActionCheckpoint, WorldAuthorityId, WorldDriverId, WorldEvent, WorldEventEnvelope,
        WorldGrantId, WorldId, WorldIntentId, WorldOwnership, WorldPrincipal,
        WorldRecipeOperationHint, WorldRecoveryPlan, WorldResourceId, WorldTraceId,
    };

    fn durable_event(
        sequence: u64,
        intent_id: &str,
        event: WorldEventKind,
    ) -> DurableWorldEvent {
        DurableWorldEvent {
            schema_version: crate::world_trace_store::WORLD_TIMELINE_SCHEMA_VERSION,
            ledger_sequence: sequence,
            recorded_at_ms: sequence * 10,
            envelope: WorldEventEnvelope {
                schema_version: WORLD_EVENT_ENVELOPE_SCHEMA_VERSION,
                authority_id: WorldAuthorityId::new("workshop:test"),
                driver_id: WorldDriverId::new("driver:test"),
                ownership: WorldOwnership::Managed,
                surface: WorldSurfaceKind::Browser,
                event: WorldEvent {
                    sequence,
                    world_id: WorldId::new("world:test"),
                    world_revision: sequence,
                    at_ms: sequence * 10,
                    principal: Some(WorldPrincipal::agent("agent:test")),
                    resource_id: Some(WorldResourceId::new("browser-tab:opaque")),
                    intent_id: Some(WorldIntentId::new(intent_id)),
                    trace_id: Some(WorldTraceId::new("trace:test")),
                    event,
                },
            },
        }
    }

    fn admission(intent_id: &str, operator_confirmation: bool) -> DurableWorldEvent {
        durable_event(
            1,
            intent_id,
            WorldEventKind::ActionAdmitted {
                grant_id: WorldGrantId::new("grant:must-not-escape"),
                effect_class: WorldEffectClass::LocalMutation,
                summary: "type super-secret-value into #opaque-selector".to_string(),
                checkpoint: WorldActionCheckpoint {
                    schema_version: WORLD_ACTION_CHECKPOINT_SCHEMA_VERSION,
                    surface: WorldSurfaceKind::Browser,
                    world_revision: 1,
                    control_generation: Some(7),
                    admitted_at_ms: 10,
                    permit_expires_at_ms: 20,
                },
                recovery: WorldRecoveryPlan {
                    strategy: WorldEffectClass::LocalMutation.recovery_strategy(),
                    requires_fresh_admission: true,
                },
                recipe_hint: Some(WorldActionRecipeHint {
                    schema_version: WORLD_ACTION_RECIPE_HINT_SCHEMA_VERSION,
                    operations: vec![WorldRecipeOperationHint {
                        verb: "type".to_string(),
                        target_role: Some("textbox".to_string()),
                        target_name: Some("Display name".to_string()),
                        input_kind: Some(AuthorityRecipeInputKind::Text),
                        requires_operator_confirmation: operator_confirmation,
                    }],
                }),
            },
        )
    }

    fn completion(intent_id: &str, status: WorldActionStatus) -> DurableWorldEvent {
        durable_event(
            2,
            intent_id,
            WorldEventKind::ActionCommitted {
                effect_class: WorldEffectClass::LocalMutation,
                status,
                summary: "super-secret-value applied".to_string(),
                recovery: None,
            },
        )
    }

    #[test]
    fn derives_only_semantic_inert_recipe_material() {
        let recipe = derive_world_recipe(
            "trace:test",
            &[admission("intent:test", true), completion("intent:test", WorldActionStatus::Confirmed)],
        )
        .expect("confirmed trace derives");

        assert_eq!(recipe.steps.len(), 1);
        assert_eq!(recipe.steps[0].operations[0].target_role.as_deref(), Some("textbox"));
        assert_eq!(recipe.steps[0].operations[0].input_kind, Some(WorldRecipeInputKind::Text));
        assert!(recipe.steps[0].requires_operator_confirmation);
        assert!(recipe.steps[0].requires_fresh_observation);
        assert!(recipe.steps[0].requires_fresh_admission);
        assert!(!recipe.carries_authority);
        assert!(!recipe.automatic_dispatch_allowed);

        let encoded = serde_json::to_string(&recipe).expect("serialize recipe");
        for forbidden in [
            "super-secret-value",
            "#opaque-selector",
            "element_ref",
            "selector",
            "grant:must-not-escape",
            "control_generation",
            "idempotency_key",
            "permit",
        ] {
            assert!(!encoded.contains(forbidden), "recipe leaked {forbidden}");
        }
    }

    #[test]
    fn refuses_missing_or_uncertain_terminal_receipts() {
        let missing = derive_world_recipe("trace:test", &[admission("intent:test", false)])
            .expect_err("missing receipt must fail closed");
        assert!(missing.to_string().contains("no terminal receipt"));

        let uncertain = derive_world_recipe(
            "trace:test",
            &[
                admission("intent:test", false),
                completion("intent:test", WorldActionStatus::Indeterminate),
            ],
        )
        .expect_err("indeterminate receipt must fail closed");
        assert!(uncertain.to_string().contains("indeterminate"));
    }
}
