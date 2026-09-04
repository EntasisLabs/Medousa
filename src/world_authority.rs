//! Daemon-owned authority for interactive browser and computer worlds.
//!
//! The first vertical slice governs the existing desktop BrowserHost action
//! path. Drivers remain responsible for platform checks and execution; only
//! this daemon service mints permits and advances authoritative world state.

use std::collections::BTreeSet;
use std::sync::{LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use medousa_world::{
    WorldActionIntent, WorldActionOutcome, WorldActionPermit, WorldAdmission, WorldAuthority,
    WorldAuthorityError, WorldAuthorityId, WorldCapability, WorldEffectClass, WorldGrantId,
    WorldGrantRequest, WorldId, WorldOwnership, WorldPrincipal, WorldPrincipalId, WorldResourceId,
    WorldResourceScope, WorldSessionSpec, WorldSurfaceKind, WorldTraceId,
};
use serde::Serialize;
use uuid::Uuid;

const BROWSER_AGENT_LEASE_MS: u64 = 5 * 60 * 1_000;
const BROWSER_ACTION_PERMIT_MS: u64 = 10_000;
const BROWSER_AGENT_PRINCIPAL: &str = "agent:medousa-foreground";

static AUTHORITY: LazyLock<Mutex<WorldAuthority>> =
    LazyLock::new(|| Mutex::new(WorldAuthority::default()));

#[derive(Debug, Clone)]
pub struct BrowserWorldAdmission {
    pub permit: WorldActionPermit,
}

#[derive(Debug, Clone, Serialize)]
pub struct BrowserWorldProvenance {
    pub world_id: WorldId,
    pub intent_id: medousa_world::WorldIntentId,
    pub trace_id: WorldTraceId,
    pub resource_id: WorldResourceId,
    pub admitted_revision: u64,
    pub control_generation: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<WorldActionOutcome>,
}

impl BrowserWorldAdmission {
    pub fn provenance(&self, outcome: Option<WorldActionOutcome>) -> BrowserWorldProvenance {
        BrowserWorldProvenance {
            world_id: self.permit.world_id.clone(),
            intent_id: self.permit.intent_id.clone(),
            trace_id: self.permit.trace_id.clone(),
            resource_id: self.permit.resource_id.clone(),
            admitted_revision: self.permit.admitted_revision,
            control_generation: self.permit.control_generation,
            outcome,
        }
    }
}

pub fn admit_browser_action(
    authority_id: &str,
    tab_group_id: &str,
    tab_id: &str,
    trace_id: &str,
    summary: &str,
    effect_class: WorldEffectClass,
) -> Result<BrowserWorldAdmission, String> {
    let now_ms = now_ms();
    let world_id = browser_world_id(authority_id, tab_group_id);
    let resource_id = WorldResourceId::new(format!("browser-tab:{tab_id}"));
    let system = WorldPrincipal::system(WorldPrincipalId::new(format!(
        "runtime:{authority_id}"
    )));
    let agent = WorldPrincipal::agent(WorldPrincipalId::new(BROWSER_AGENT_PRINCIPAL));
    let mut authority = AUTHORITY
        .lock()
        .map_err(|_| "world authority lock poisoned".to_string())?;

    if matches!(
        authority.world(&world_id),
        Err(WorldAuthorityError::WorldNotFound(_))
    ) {
        authority
            .create_world(
                WorldSessionSpec {
                    world_id: world_id.clone(),
                    authority_id: WorldAuthorityId::new(authority_id),
                    ownership: WorldOwnership::Managed,
                    surface: WorldSurfaceKind::Browser,
                },
                system.clone(),
                now_ms,
            )
            .map_err(|error| error.to_string())?;
    }

    let grant_id = WorldGrantId::new(format!("grant:browser-agent:{tab_group_id}"));
    match authority.grant_capabilities(
        &world_id,
        WorldGrantRequest {
            grant_id,
            issued_by: system,
            subject: agent.clone(),
            capabilities: [WorldCapability::Observe, WorldCapability::Interact]
                .into_iter()
                .collect::<BTreeSet<_>>(),
            resource_scope: WorldResourceScope::All,
            expires_at_ms: None,
        },
        now_ms,
    ) {
        Ok(_) | Err(WorldAuthorityError::GrantAlreadyExists(_)) => {}
        Err(error) => return Err(error.to_string()),
    }

    let lease = authority
        .acquire_control(
            &world_id,
            agent.clone(),
            Some(now_ms.saturating_add(BROWSER_AGENT_LEASE_MS)),
            now_ms,
        )
        .map_err(|error| error.to_string())?;
    let state = authority.world(&world_id).map_err(|error| error.to_string())?;
    let operation_id = Uuid::new_v4().to_string();
    let idempotency_key = format!("browser-action:{operation_id}");
    let intent = WorldActionIntent {
        intent_id: medousa_world::WorldIntentId::new(format!("intent:{operation_id}")),
        trace_id: WorldTraceId::new(trace_id),
        principal: agent,
        resource_id,
        expected_revision: state.revision,
        expected_control_generation: Some(lease.generation),
        required_capability: effect_class.required_capability(),
        effect_class,
        idempotency_key: idempotency_key.clone(),
        permit_expires_at_ms: now_ms.saturating_add(BROWSER_ACTION_PERMIT_MS),
        summary: summary.to_string(),
    };

    match authority
        .admit_action(&world_id, intent, now_ms)
        .map_err(|error| error.to_string())?
    {
        WorldAdmission::Admitted { permit } => Ok(BrowserWorldAdmission { permit }),
        WorldAdmission::Replay { .. } => {
            Err("new browser action unexpectedly resolved as an idempotent replay".to_string())
        }
    }
}

pub fn complete_browser_action(
    admission: &BrowserWorldAdmission,
    summary: &str,
) -> Result<WorldActionOutcome, String> {
    AUTHORITY
        .lock()
        .map_err(|_| "world authority lock poisoned".to_string())?
        .complete_action(
            &admission.permit,
            summary,
            now_ms(),
        )
        .map_err(|error| error.to_string())
}

pub fn fail_browser_action(
    admission: &BrowserWorldAdmission,
    error: &str,
) -> Result<(), String> {
    AUTHORITY
        .lock()
        .map_err(|_| "world authority lock poisoned".to_string())?
        .fail_action(
            &admission.permit,
            error,
            now_ms(),
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
}

pub fn mark_browser_action_indeterminate(
    admission: &BrowserWorldAdmission,
    summary: &str,
) -> Result<WorldActionOutcome, String> {
    AUTHORITY
        .lock()
        .map_err(|_| "world authority lock poisoned".to_string())?
        .mark_action_indeterminate(
            &admission.permit,
            summary,
            now_ms(),
        )
        .map_err(|error| error.to_string())
}

fn browser_world_id(authority_id: &str, tab_group_id: &str) -> WorldId {
    WorldId::new(format!("world:browser:{authority_id}:{tab_group_id}"))
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
    use super::*;

    #[test]
    fn browser_world_identity_is_bound_to_authority_and_tab_group() {
        assert_ne!(
            browser_world_id("workshop:a", "group:one"),
            browser_world_id("workshop:b", "group:one")
        );
        assert_ne!(
            browser_world_id("workshop:a", "group:one"),
            browser_world_id("workshop:a", "group:two")
        );
    }
}
