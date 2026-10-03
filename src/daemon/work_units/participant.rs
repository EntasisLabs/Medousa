//! Authenticated HTTP participation without a synthetic chat turn or admin grant.
use super::*;
use crate::daemon::route_policy::{
    BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RoutePolicy,
};
use axum::{
    Json,
    extract::{Extension, State},
    http::{Method, StatusCode},
    routing::post,
};
use medousa_types::work_participant::*;

type HttpError = (StatusCode, String);

pub fn surface() -> DeclaredRouter {
    surface_with_host(None)
}

fn surface_with_host(host: Option<Arc<WorkUnitHost>>) -> DeclaredRouter {
    DeclaredRouter::default()
        .route(policy("/v1/work/query", false), post(query))
        .route(policy("/v1/work/mutate", true), post(mutate))
        .with_state(host)
}

fn policy(path: &'static str, write: bool) -> RoutePolicy {
    RoutePolicy {
        method: Method::POST,
        path,
        group: RouteGroup::Portal,
        required_capability: Some(if write {
            Capability::ContentWrite
        } else {
            Capability::ContentRead
        }),
        bootstrap_public: false,
        browser_policy: BrowserPolicy::NativeOnly,
        body_limit: MAX_SNAPSHOT_BYTES,
        rate_limit_class: if write {
            RateLimitClass::Mutation
        } else {
            RateLimitClass::Read
        },
    }
}

fn participant_domain(
    principal: &RequestPrincipal,
    write: bool,
) -> Result<UserDomainRef, HttpError> {
    let caps = principal.capabilities();
    if !caps.contains(Capability::ContentRead)
        || !caps.contains(Capability::WorkshopRead)
        || (write
            && (!caps.contains(Capability::ContentWrite)
                || !caps.contains(Capability::WorkshopInteract)))
    {
        return Err((
            StatusCode::FORBIDDEN,
            "participant cannot access this work domain".into(),
        ));
    }
    let owner = principal
        .profile_id()
        .filter(|owner| !owner.trim().is_empty())
        .ok_or((
            StatusCode::FORBIDDEN,
            "work participation requires a bound owner credential".into(),
        ))?;
    Ok(UserDomainRef {
        authority_id: crate::workshop_authority::current()
            .map_err(|_| {
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "workshop authority unavailable".into(),
                )
            })?
            .clone(),
        user_id: owner.into(),
    })
}

fn host(injected: Option<Arc<WorkUnitHost>>) -> Result<Arc<WorkUnitHost>, HttpError> {
    injected.or_else(local_work_unit_host).ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "work unit host unavailable".into(),
    ))
}

fn peer_host() -> Result<Arc<crate::daemon::coordination::LocalPeerDispatcher>, HttpError> {
    crate::daemon::coordination::local_coordination_host().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "native coordination unavailable".into(),
    ))
}

fn conflict(error: anyhow::Error) -> HttpError {
    (StatusCode::CONFLICT, error.to_string())
}

async fn query(
    State(injected): State<Option<Arc<WorkUnitHost>>>,
    Extension(principal): Extension<RequestPrincipal>,
    Json(input): Json<WorkParticipantQuery>,
) -> Result<Json<WorkParticipantResponse>, HttpError> {
    let domain = participant_domain(&principal, false)?;
    let result = match input {
        WorkParticipantQuery::Graph { query } => {
            host(injected)?.graph_in_domain(domain, query).await
        }
        WorkParticipantQuery::Get { work_unit_id } => {
            host(injected)?.get_in_domain(domain, work_unit_id).await
        }
        WorkParticipantQuery::Events { query } => {
            let actor = principal
                .credential_id()
                .ok_or((
                    StatusCode::FORBIDDEN,
                    "participant credential unavailable".into(),
                ))?
                .as_str()
                .to_string();
            host(injected)?.events_in_domain(domain, actor, query).await
        }
        WorkParticipantQuery::Coordination { query } => {
            peer_host()?.get_work_coordination(domain, query).await
        }
        WorkParticipantQuery::Discover { session_id } => {
            participant_domain(&principal, true)?;
            peer_host()?.discover_for_turn(&principal, session_id).await
        }
    }
    .map_err(conflict)?;
    Ok(Json(WorkParticipantResponse {
        result: bounded_response(result).map_err(conflict)?,
    }))
}

async fn mutate(
    State(injected): State<Option<Arc<WorkUnitHost>>>,
    Extension(principal): Extension<RequestPrincipal>,
    Json(input): Json<WorkParticipantMutation>,
) -> Result<Json<WorkParticipantResponse>, HttpError> {
    let domain = participant_domain(&principal, true)?;
    let result = match input {
        WorkParticipantMutation::Record { command } => {
            let actor = principal
                .credential_id()
                .ok_or((
                    StatusCode::FORBIDDEN,
                    "participant credential unavailable".into(),
                ))?
                .as_str()
                .to_string();
            host(injected)?
                .record_in_domain(domain, actor, command)
                .await
        }
        WorkParticipantMutation::Coordinate { input } => {
            let plan = peer_host()?
                .register_work_coordination(domain, input)
                .await
                .map_err(conflict)?;
            Ok(serde_json::to_value(plan).map_err(|error| conflict(error.into()))?)
        }
        WorkParticipantMutation::Propose { session_id, intent } => {
            if intent.continue_owner {
                return Err((
                    StatusCode::FORBIDDEN,
                    "work participants cannot request owner-chat continuation".into(),
                ));
            }
            let peer = peer_host()?;
            let proposal = peer
                .propose_for_turn(&principal, session_id, intent)
                .await
                .map_err(conflict)?;
            peer.proposal_tool_result(&principal, proposal).await
        }
    }
    .map_err(conflict)?;
    Ok(Json(WorkParticipantResponse {
        result: bounded_response(result).map_err(conflict)?,
    }))
}

#[cfg(test)]
mod tests;
