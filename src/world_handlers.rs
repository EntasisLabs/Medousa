//! Authenticated review surface for the daemon's governed-world causal ledger.

use axum::extract::{Extension, Query, State};
use axum::routing::{get, post};
use axum::Json;
use medousa_types::{
    WORLD_TIMELINE_EVENT_SCHEMA_VERSION, WorldRecipeDeriveResponse, WorldRecipeRunRequest,
    WorldRecipeRunResponse, WorldTimelineCheckpoint, WorldTimelineEvent, WorldTimelineRecovery,
    WorldTimelineResponse,
};
use medousa_world::{
    WorldActionCheckpoint, WorldActionStatus, WorldEffectClass, WorldEventKind, WorldOwnership,
    WorldPrincipalKind, WorldRecoveryPlan, WorldRecoveryStrategy, WorldSurfaceKind,
};
use serde::Deserialize;

use crate::daemon::route_policy::{
    BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RoutePolicy,
};
use crate::daemon::state::AppState;
use crate::request_principal::Capability;
use crate::request_principal::RequestPrincipal;
use crate::world_recipe_runner::{WorldRecipeRunError, run_world_recipe};
use crate::world_recipes::{WorldRecipeDerivationError, derive_world_recipe};
use crate::world_trace_store::DurableWorldEvent;

const DEFAULT_PAGE_SIZE: usize = 100;
const MAX_PAGE_SIZE: usize = 200;

#[derive(Debug, Deserialize)]
pub struct WorldTimelineQuery {
    #[serde(default)]
    after_sequence: u64,
    #[serde(default = "default_page_size")]
    limit: usize,
}

#[derive(Debug, Deserialize)]
pub struct WorldRecipeDeriveQuery {
    trace_id: String,
}

fn default_page_size() -> usize {
    DEFAULT_PAGE_SIZE
}

pub async fn list_world_timeline(
    State(state): State<AppState>,
    Query(query): Query<WorldTimelineQuery>,
) -> Result<Json<WorldTimelineResponse>, (axum::http::StatusCode, String)> {
    let limit = query.limit.clamp(1, MAX_PAGE_SIZE);
    let mut records = state
        .world_authority
        .durable_events_after(query.after_sequence, limit.saturating_add(1))
        .map_err(|error| (axum::http::StatusCode::SERVICE_UNAVAILABLE, error))?;
    let has_more = records.len() > limit;
    records.truncate(limit);
    let next_sequence = records
        .last()
        .map(|record| record.ledger_sequence)
        .unwrap_or(query.after_sequence);
    Ok(Json(WorldTimelineResponse {
        events: records.into_iter().map(public_event).collect(),
        next_sequence,
        has_more,
    }))
}

pub async fn derive_world_recipe_from_trace(
    State(state): State<AppState>,
    Query(query): Query<WorldRecipeDeriveQuery>,
) -> Result<Json<WorldRecipeDeriveResponse>, (axum::http::StatusCode, String)> {
    if query.trace_id.is_empty()
        || query.trace_id.trim() != query.trace_id
        || query.trace_id.len() > 512
        || query.trace_id.contains('\0')
    {
        return Err((
            axum::http::StatusCode::BAD_REQUEST,
            "trace_id is missing or malformed".to_string(),
        ));
    }
    let records = state
        .world_authority
        .durable_events_for_trace(&query.trace_id)
        .map_err(|error| (axum::http::StatusCode::SERVICE_UNAVAILABLE, error))?;
    let recipe = derive_world_recipe(&query.trace_id, &records).map_err(|error| match error {
        WorldRecipeDerivationError::TraceNotFound => {
            (axum::http::StatusCode::NOT_FOUND, error.to_string())
        }
        WorldRecipeDerivationError::NotReplayable(_) => {
            (axum::http::StatusCode::CONFLICT, error.to_string())
        }
    })?;
    Ok(Json(WorldRecipeDeriveResponse { recipe }))
}

pub async fn run_world_recipe_from_trace(
    State(state): State<AppState>,
    Extension(principal): Extension<RequestPrincipal>,
    Json(request): Json<WorldRecipeRunRequest>,
) -> Result<Json<WorldRecipeRunResponse>, (axum::http::StatusCode, String)> {
    let owner_profile_id = principal
        .profile_id()
        .map(str::to_string)
        .unwrap_or_else(crate::user_profiles::resolve_workshop_identity_user_id);
    run_world_recipe(&state, owner_profile_id, request)
        .await
        .map(Json)
        .map_err(|error| match error {
            WorldRecipeRunError::Invalid(message) => (axum::http::StatusCode::BAD_REQUEST, message),
            WorldRecipeRunError::NotFound(message) => (axum::http::StatusCode::NOT_FOUND, message),
            WorldRecipeRunError::Conflict(message) => (axum::http::StatusCode::CONFLICT, message),
            WorldRecipeRunError::Unavailable(message) => {
                (axum::http::StatusCode::SERVICE_UNAVAILABLE, message)
            }
        })
}

pub fn world_timeline_surface() -> DeclaredRouter<AppState> {
    DeclaredRouter::default()
        .route(
            RoutePolicy {
                method: axum::http::Method::GET,
                path: "/v1/worlds/timeline",
                group: RouteGroup::Portal,
                required_capability: Some(Capability::WorkshopRead),
                bootstrap_public: false,
                browser_policy: BrowserPolicy::ExactOrigin,
                body_limit: 1024,
                rate_limit_class: RateLimitClass::Read,
            },
            get(list_world_timeline),
        )
        .route(
            RoutePolicy {
                method: axum::http::Method::GET,
                path: "/v1/worlds/recipes/derive",
                group: RouteGroup::Portal,
                required_capability: Some(Capability::WorkshopRead),
                bootstrap_public: false,
                browser_policy: BrowserPolicy::ExactOrigin,
                body_limit: 1024,
                rate_limit_class: RateLimitClass::Read,
            },
            get(derive_world_recipe_from_trace),
        )
        .route(
            RoutePolicy {
                method: axum::http::Method::POST,
                path: "/v1/worlds/recipes/run",
                group: RouteGroup::Portal,
                required_capability: Some(Capability::AdminExecute),
                bootstrap_public: false,
                browser_policy: BrowserPolicy::ExactOrigin,
                body_limit: 256 * 1024,
                rate_limit_class: RateLimitClass::Mutation,
            },
            post(run_world_recipe_from_trace),
        )
}

fn public_event(record: DurableWorldEvent) -> WorldTimelineEvent {
    let event = &record.envelope.event;
    let (event_type, effect_class, status, summary, checkpoint, recovery) =
        public_event_details(&event.event);
    WorldTimelineEvent {
        schema_version: WORLD_TIMELINE_EVENT_SCHEMA_VERSION,
        sequence: record.ledger_sequence,
        recorded_at_ms: record.recorded_at_ms,
        world_id: event.world_id.to_string(),
        authority_id: record.envelope.authority_id.to_string(),
        driver_id: record.envelope.driver_id.to_string(),
        ownership: ownership_label(record.envelope.ownership).to_string(),
        surface: surface_label(record.envelope.surface).to_string(),
        world_revision: event.world_revision,
        occurred_at_ms: event.at_ms,
        principal_id: event
            .principal
            .as_ref()
            .map(|principal| principal.principal_id.to_string()),
        principal_kind: event
            .principal
            .as_ref()
            .map(|principal| principal_kind_label(principal.kind).to_string()),
        resource_id: event.resource_id.as_ref().map(ToString::to_string),
        intent_id: event.intent_id.as_ref().map(ToString::to_string),
        trace_id: event.trace_id.as_ref().map(ToString::to_string),
        event_type: event_type.to_string(),
        effect_class: effect_class.map(|effect| effect_label(effect).to_string()),
        status,
        summary,
        checkpoint: checkpoint.map(public_checkpoint),
        recovery: recovery.map(public_recovery),
    }
}

type PublicEventDetails<'a> = (
    &'static str,
    Option<WorldEffectClass>,
    Option<String>,
    Option<String>,
    Option<&'a WorldActionCheckpoint>,
    Option<&'a WorldRecoveryPlan>,
);

fn public_event_details(event: &WorldEventKind) -> PublicEventDetails<'_> {
    match event {
        WorldEventKind::WorldCreated { .. } => {
            ("world_created", None, None, None, None, None)
        }
        WorldEventKind::CapabilityGranted { subject, .. } => (
            "capability_granted",
            None,
            None,
            Some(format!("granted world access to {}", subject.principal_id)),
            None,
            None,
        ),
        WorldEventKind::CapabilityRevoked { .. } => {
            ("capability_revoked", None, None, None, None, None)
        }
        WorldEventKind::ControlAcquired { .. } => {
            ("control_acquired", None, None, None, None, None)
        }
        WorldEventKind::ControlReleased { .. } => {
            ("control_released", None, None, None, None, None)
        }
        WorldEventKind::ActionAdmitted {
            effect_class,
            summary,
            checkpoint,
            recovery,
            ..
        } => (
            "action_admitted",
            Some(*effect_class),
            Some("admitted".to_string()),
            Some(summary.clone()),
            Some(checkpoint),
            Some(recovery),
        ),
        WorldEventKind::ActionCommitted {
            effect_class,
            status,
            summary,
            recovery,
        } => (
            "action_committed",
            Some(*effect_class),
            Some(status_label(*status).to_string()),
            Some(summary.clone()),
            None,
            recovery.as_ref(),
        ),
        WorldEventKind::ActionFailed {
            effect_class,
            error,
        } => (
            "action_failed",
            Some(*effect_class),
            Some("failed".to_string()),
            Some(error.clone()),
            None,
            None,
        ),
        WorldEventKind::ExternalMutationObserved { summary } => (
            "external_mutation_observed",
            None,
            None,
            Some(summary.clone()),
            None,
            None,
        ),
        WorldEventKind::ActionInterrupted {
            effect_class,
            status,
            summary,
            checkpoint,
            recovery,
        } => (
            "action_interrupted",
            Some(*effect_class),
            Some(status_label(*status).to_string()),
            Some(summary.clone()),
            Some(checkpoint),
            Some(recovery),
        ),
    }
}

fn public_checkpoint(checkpoint: &WorldActionCheckpoint) -> WorldTimelineCheckpoint {
    WorldTimelineCheckpoint {
        surface: surface_label(checkpoint.surface).to_string(),
        world_revision: checkpoint.world_revision,
        control_generation: checkpoint.control_generation,
        admitted_at_ms: checkpoint.admitted_at_ms,
        permit_expires_at_ms: checkpoint.permit_expires_at_ms,
    }
}

fn public_recovery(recovery: &WorldRecoveryPlan) -> WorldTimelineRecovery {
    WorldTimelineRecovery {
        strategy: recovery_strategy_label(recovery.strategy).to_string(),
        requires_fresh_admission: recovery.requires_fresh_admission,
    }
}

fn ownership_label(ownership: WorldOwnership) -> &'static str {
    match ownership {
        WorldOwnership::Owned => "owned",
        WorldOwnership::Managed => "managed",
        WorldOwnership::Attached => "attached",
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

fn principal_kind_label(kind: WorldPrincipalKind) -> &'static str {
    match kind {
        WorldPrincipalKind::Human => "human",
        WorldPrincipalKind::Agent => "agent",
        WorldPrincipalKind::Bot => "bot",
        WorldPrincipalKind::Worker => "worker",
        WorldPrincipalKind::Peer => "peer",
        WorldPrincipalKind::System => "system",
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

fn status_label(status: WorldActionStatus) -> &'static str {
    match status {
        WorldActionStatus::Confirmed => "confirmed",
        WorldActionStatus::NeedsReconciliation => "needs_reconciliation",
        WorldActionStatus::Indeterminate => "indeterminate",
    }
}

fn recovery_strategy_label(strategy: WorldRecoveryStrategy) -> &'static str {
    match strategy {
        WorldRecoveryStrategy::Reobserve => "reobserve",
        WorldRecoveryStrategy::ReconcileFromFreshObservation => {
            "reconcile_from_fresh_observation"
        }
        WorldRecoveryStrategy::OperatorReview => "operator_review",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use medousa_world::{
        WORLD_ACTION_CHECKPOINT_SCHEMA_VERSION, WORLD_EVENT_ENVELOPE_SCHEMA_VERSION,
        WorldActionCheckpoint, WorldAuthorityId, WorldDriverId, WorldEvent, WorldEventEnvelope,
        WorldId, WorldOwnership, WorldPrincipal, WorldRecoveryPlan, WorldResourceId,
        WorldTraceId,
    };

    #[test]
    fn recipe_derivation_is_an_authenticated_exact_origin_read() {
        let surface = world_timeline_surface();
        let route = surface
            .inventory()
            .entries()
            .find(|entry| entry.path == "/v1/worlds/recipes/derive")
            .expect("recipe derivation route");
        assert_eq!(route.method, "GET");
        assert_eq!(route.group, RouteGroup::Portal);
        assert_eq!(route.required_capability, Some("workshop.read"));
        assert_eq!(route.browser_policy, BrowserPolicy::ExactOrigin);
        assert!(!route.bootstrap_public);
    }

    #[test]
    fn recipe_run_is_an_operator_only_exact_origin_mutation() {
        let surface = world_timeline_surface();
        let route = surface
            .inventory()
            .entries()
            .find(|entry| entry.path == "/v1/worlds/recipes/run")
            .expect("recipe run route");
        assert_eq!(route.method, "POST");
        assert_eq!(route.group, RouteGroup::Portal);
        assert_eq!(route.required_capability, Some("admin.execute"));
        assert_eq!(route.browser_policy, BrowserPolicy::ExactOrigin);
        assert_eq!(route.rate_limit_class, RateLimitClass::Mutation);
        assert!(!route.bootstrap_public);
    }

    #[test]
    fn public_interruption_keeps_causality_without_driver_secrets() {
        let event = public_event(DurableWorldEvent {
            schema_version: crate::world_trace_store::WORLD_TIMELINE_SCHEMA_VERSION,
            ledger_sequence: 9,
            recorded_at_ms: 30,
            envelope: WorldEventEnvelope {
                schema_version: WORLD_EVENT_ENVELOPE_SCHEMA_VERSION,
                authority_id: WorldAuthorityId::new("workshop:test"),
                driver_id: WorldDriverId::new("driver:test"),
                ownership: WorldOwnership::Owned,
                surface: WorldSurfaceKind::Browser,
                event: WorldEvent {
                    sequence: 4,
                    world_id: WorldId::new("world:test"),
                    world_revision: 3,
                    at_ms: 29,
                    principal: Some(WorldPrincipal::agent("agent:test")),
                    resource_id: Some(WorldResourceId::new("browser-tab:test")),
                    intent_id: Some(medousa_world::WorldIntentId::new("intent:test")),
                    trace_id: Some(WorldTraceId::new("trace:test")),
                    event: WorldEventKind::ActionInterrupted {
                        effect_class: WorldEffectClass::LocalMutation,
                        status: WorldActionStatus::Indeterminate,
                        summary: "driver receipt missing".to_string(),
                        checkpoint: WorldActionCheckpoint {
                            schema_version: WORLD_ACTION_CHECKPOINT_SCHEMA_VERSION,
                            surface: WorldSurfaceKind::Browser,
                            world_revision: 3,
                            control_generation: Some(2),
                            admitted_at_ms: 20,
                            permit_expires_at_ms: 25,
                        },
                        recovery: WorldRecoveryPlan {
                            strategy: WorldRecoveryStrategy::ReconcileFromFreshObservation,
                            requires_fresh_admission: true,
                        },
                    },
                },
            },
        });
        assert_eq!(event.event_type, "action_interrupted");
        assert_eq!(event.principal_kind.as_deref(), Some("agent"));
        assert_eq!(event.status.as_deref(), Some("indeterminate"));
        assert_eq!(
            event.recovery.as_ref().map(|recovery| recovery.strategy.as_str()),
            Some("reconcile_from_fresh_observation")
        );
        let encoded = serde_json::to_string(&event).expect("serialize public event");
        assert!(!encoded.contains("idempotency_key"));
        assert!(!encoded.contains("grant_id"));
    }
}
