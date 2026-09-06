//! Daemon-owned authority for interactive browser and computer worlds.
//!
//! The first vertical slice governs the existing desktop BrowserHost action
//! path. Drivers remain responsible for platform checks and execution; only
//! this daemon service mints permits and advances authoritative world state.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use medousa_world::{
    WorldActionIntent, WorldActionOutcome, WorldActionPermit, WorldAdmission, WorldAuthority,
    WorldAuthorityError, WorldAuthorityId, WorldCapability, WorldDriverId, WorldEffectClass,
    WorldGrantId, WorldGrantRequest, WorldId, WorldOwnership, WorldPrincipal, WorldPrincipalId,
    WorldResourceId, WorldResourceScope, WorldSessionSpec, WorldSurfaceKind, WorldTraceId,
};
use medousa_browser_bridge::BrowserObservation;
use serde::Serialize;
use uuid::Uuid;

const BROWSER_AGENT_LEASE_MS: u64 = 5 * 60 * 1_000;
const BROWSER_ACTION_PERMIT_MS: u64 = 10_000;
const BROWSER_AGENT_PRINCIPAL: &str = "agent:medousa-foreground";

static AUTHORITY: LazyLock<Mutex<WorldAuthority>> =
    LazyLock::new(|| Mutex::new(WorldAuthority::default()));
static BROWSER_OBSERVATIONS: LazyLock<Mutex<HashMap<WorldId, BrowserObservation>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone)]
pub struct BrowserWorldAdmission {
    pub permit: WorldActionPermit,
}

#[derive(Debug, Clone, Serialize)]
pub struct BrowserWorldProvenance {
    pub world_id: WorldId,
    pub driver_id: WorldDriverId,
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
            driver_id: self.permit.driver_id.clone(),
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
    driver_id: &str,
    tab_group_id: &str,
    tab_id: &str,
    trace_id: &str,
    summary: &str,
    effect_class: WorldEffectClass,
) -> Result<BrowserWorldAdmission, String> {
    let now_ms = now_ms();
    let resource_id = WorldResourceId::new(format!("browser-tab:{tab_id}"));
    let mut authority = AUTHORITY
        .lock()
        .map_err(|_| "world authority lock poisoned".to_string())?;
    let (world_id, agent) = ensure_browser_world(
        &mut authority,
        authority_id,
        driver_id,
        tab_group_id,
        WorldOwnership::Managed,
        now_ms,
    )?;

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

pub fn admit_browser_observation(
    authority_id: &str,
    driver_id: &str,
    tab_group_id: &str,
    tab_id: &str,
    trace_id: &str,
    summary: &str,
) -> Result<BrowserWorldAdmission, String> {
    let now_ms = now_ms();
    let resource_id = WorldResourceId::new(format!("browser-tab:{tab_id}"));
    let mut authority = AUTHORITY
        .lock()
        .map_err(|_| "world authority lock poisoned".to_string())?;
    let (world_id, agent) = ensure_browser_world(
        &mut authority,
        authority_id,
        driver_id,
        tab_group_id,
        WorldOwnership::Managed,
        now_ms,
    )?;
    let state = authority.world(&world_id).map_err(|error| error.to_string())?;
    let operation_id = Uuid::new_v4().to_string();
    let intent = WorldActionIntent {
        intent_id: medousa_world::WorldIntentId::new(format!("intent:{operation_id}")),
        trace_id: WorldTraceId::new(trace_id),
        principal: agent,
        resource_id,
        expected_revision: state.revision,
        expected_control_generation: None,
        required_capability: WorldCapability::Observe,
        effect_class: WorldEffectClass::Observe,
        idempotency_key: format!("browser-observation:{operation_id}"),
        permit_expires_at_ms: now_ms.saturating_add(BROWSER_ACTION_PERMIT_MS),
        summary: summary.to_string(),
    };
    match authority
        .admit_action(&world_id, intent, now_ms)
        .map_err(|error| error.to_string())?
    {
        WorldAdmission::Admitted { permit } => Ok(BrowserWorldAdmission { permit }),
        WorldAdmission::Replay { .. } => {
            Err("new browser observation unexpectedly resolved as a replay".to_string())
        }
    }
}

pub fn admit_browser_pixel_observation(
    authority_id: &str,
    driver_id: &str,
    tab_group_id: &str,
    tab_id: &str,
    trace_id: &str,
    summary: &str,
) -> Result<BrowserWorldAdmission, String> {
    let now_ms = now_ms();
    let resource_id = WorldResourceId::new(format!("browser-tab:{tab_id}"));
    let mut authority = AUTHORITY
        .lock()
        .map_err(|_| "world authority lock poisoned".to_string())?;
    let (world_id, agent) = ensure_browser_world(
        &mut authority,
        authority_id,
        driver_id,
        tab_group_id,
        WorldOwnership::Managed,
        now_ms,
    )?;
    let state = authority.world(&world_id).map_err(|error| error.to_string())?;
    let operation_id = Uuid::new_v4().to_string();
    let intent = WorldActionIntent {
        intent_id: medousa_world::WorldIntentId::new(format!("intent:{operation_id}")),
        trace_id: WorldTraceId::new(trace_id),
        principal: agent,
        resource_id,
        expected_revision: state.revision,
        expected_control_generation: None,
        required_capability: WorldCapability::ObservePixels,
        effect_class: WorldEffectClass::ObservePixels,
        idempotency_key: format!("browser-pixel-observation:{operation_id}"),
        permit_expires_at_ms: now_ms.saturating_add(BROWSER_ACTION_PERMIT_MS),
        summary: summary.to_string(),
    };
    match authority
        .admit_action(&world_id, intent, now_ms)
        .map_err(|error| error.to_string())?
    {
        WorldAdmission::Admitted { permit } => Ok(BrowserWorldAdmission { permit }),
        WorldAdmission::Replay { .. } => {
            Err("new browser pixel observation unexpectedly resolved as a replay".to_string())
        }
    }
}

fn ensure_browser_world(
    authority: &mut WorldAuthority,
    authority_id: &str,
    driver_id: &str,
    tab_group_id: &str,
    ownership: WorldOwnership,
    now_ms: u64,
) -> Result<(WorldId, WorldPrincipal), String> {
    let world_id = browser_world_id(authority_id, driver_id, tab_group_id);
    let system = WorldPrincipal::system(WorldPrincipalId::new(format!(
        "runtime:{authority_id}"
    )));
    let agent = WorldPrincipal::agent(WorldPrincipalId::new(BROWSER_AGENT_PRINCIPAL));
    if matches!(
        authority.world(&world_id),
        Err(WorldAuthorityError::WorldNotFound(_))
    ) {
        authority
            .create_world(
                WorldSessionSpec {
                    world_id: world_id.clone(),
                    authority_id: WorldAuthorityId::new(authority_id),
                    driver_id: WorldDriverId::new(driver_id),
                    ownership,
                    surface: WorldSurfaceKind::Browser,
                },
                system.clone(),
                now_ms,
            )
            .map_err(|error| error.to_string())?;
    } else {
        let state = authority.world(&world_id).map_err(|error| error.to_string())?;
        if state.authority_id.as_str() != authority_id
            || state.driver_id.as_str() != driver_id
            || state.surface != WorldSurfaceKind::Browser
        {
            return Err("browser world identity conflicts with its registered authority".to_string());
        }
    }
    let grant_id = WorldGrantId::new(format!("grant:browser-agent:{tab_group_id}"));
    match authority.grant_capabilities(
        &world_id,
        WorldGrantRequest {
            grant_id,
            issued_by: system,
            subject: agent.clone(),
            capabilities: [
                WorldCapability::Observe,
                WorldCapability::ObservePixels,
                WorldCapability::Interact,
            ]
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
    Ok((world_id, agent))
}

/// Register a daemon-owned browser world before its Chromium process begins.
///
/// This keeps ownership explicit: a later agent admission cannot accidentally
/// recreate an isolated world as a managed shared-browser attachment.
pub fn register_owned_browser_world(
    authority_id: &str,
    driver_id: &str,
    tab_group_id: &str,
    owner_principal_id: &str,
) -> Result<WorldId, String> {
    let now_ms = now_ms();
    let mut authority = AUTHORITY
        .lock()
        .map_err(|_| "world authority lock poisoned".to_string())?;
    let (world_id, _) = ensure_browser_world(
        &mut authority,
        authority_id,
        driver_id,
        tab_group_id,
        WorldOwnership::Owned,
        now_ms,
    )?;
    let state = authority.world(&world_id).map_err(|error| error.to_string())?;
    if state.ownership != WorldOwnership::Owned {
        return Err("isolated browser driver is already bound to a non-owned world".to_string());
    }
    let system = WorldPrincipal::system(WorldPrincipalId::new(format!(
        "runtime:{authority_id}"
    )));
    let owner = WorldPrincipal::human(WorldPrincipalId::new(owner_principal_id));
    let grant_id = WorldGrantId::new(format!("grant:browser-owner:{tab_group_id}"));
    match authority.grant_capabilities(
        &world_id,
        WorldGrantRequest {
            grant_id,
            issued_by: system,
            subject: owner,
            capabilities: [
                WorldCapability::Observe,
                WorldCapability::ObservePixels,
                WorldCapability::Interact,
                WorldCapability::Admin,
            ]
            .into_iter()
            .collect::<BTreeSet<_>>(),
            resource_scope: WorldResourceScope::All,
            expires_at_ms: None,
        },
        now_ms,
    ) {
        Ok(_) | Err(WorldAuthorityError::GrantAlreadyExists(_)) => Ok(world_id),
        Err(error) => Err(error.to_string()),
    }
}

pub struct OwnedBrowserHumanIntent<'a> {
    pub authority_id: &'a str,
    pub driver_id: &'a str,
    pub tab_group_id: &'a str,
    pub tab_id: &'a str,
    pub owner_principal_id: &'a str,
    pub trace_id: &'a str,
    pub summary: &'a str,
    pub effect_class: WorldEffectClass,
}

pub fn admit_owned_browser_human_intent(
    request: OwnedBrowserHumanIntent<'_>,
) -> Result<BrowserWorldAdmission, String> {
    let now_ms = now_ms();
    let world_id = browser_world_id(
        request.authority_id,
        request.driver_id,
        request.tab_group_id,
    );
    let owner = WorldPrincipal::human(WorldPrincipalId::new(request.owner_principal_id));
    let resource_id = WorldResourceId::new(format!("browser-tab:{}", request.tab_id));
    let mut authority = AUTHORITY
        .lock()
        .map_err(|_| "world authority lock poisoned".to_string())?;
    let control_generation = if request.effect_class.requires_control() {
        Some(
            authority
                .acquire_control(&world_id, owner.clone(), None, now_ms)
                .map_err(|error| error.to_string())?
                .generation,
        )
    } else {
        None
    };
    let state = authority.world(&world_id).map_err(|error| error.to_string())?;
    let operation_id = Uuid::new_v4().to_string();
    let intent = WorldActionIntent {
        intent_id: medousa_world::WorldIntentId::new(format!("intent:{operation_id}")),
        trace_id: WorldTraceId::new(request.trace_id),
        principal: owner,
        resource_id,
        expected_revision: state.revision,
        expected_control_generation: control_generation,
        required_capability: request.effect_class.required_capability(),
        effect_class: request.effect_class,
        idempotency_key: format!("browser-human:{operation_id}"),
        permit_expires_at_ms: now_ms.saturating_add(BROWSER_ACTION_PERMIT_MS),
        summary: request.summary.to_string(),
    };
    match authority
        .admit_action(&world_id, intent, now_ms)
        .map_err(|error| error.to_string())?
    {
        WorldAdmission::Admitted { permit } => Ok(BrowserWorldAdmission { permit }),
        WorldAdmission::Replay { .. } => {
            Err("new human browser intent unexpectedly resolved as a replay".to_string())
        }
    }
}

/// Fence agent work and hand an owned browser world to its authenticated
/// human owner. The world kernel records the preemption and increments the
/// canonical control generation before the driver sees human input.
pub fn take_owned_browser_control(
    authority_id: &str,
    driver_id: &str,
    tab_group_id: &str,
    owner_principal_id: &str,
) -> Result<u64, String> {
    let now_ms = now_ms();
    let world_id = browser_world_id(authority_id, driver_id, tab_group_id);
    let mut authority = AUTHORITY
        .lock()
        .map_err(|_| "world authority lock poisoned".to_string())?;
    let lease = authority
        .acquire_control(
            &world_id,
            WorldPrincipal::human(WorldPrincipalId::new(owner_principal_id)),
            None,
            now_ms,
        )
        .map_err(|error| error.to_string())?;
    Ok(lease.generation)
}

/// Release an owned browser from its human owner. The next agent action must
/// acquire a fresh generation; an old queued permit can never become live
/// again merely because the UI returned control quickly.
pub fn return_owned_browser_control(
    authority_id: &str,
    driver_id: &str,
    tab_group_id: &str,
    owner_principal_id: &str,
) -> Result<u64, String> {
    let now_ms = now_ms();
    let world_id = browser_world_id(authority_id, driver_id, tab_group_id);
    let owner = WorldPrincipal::human(WorldPrincipalId::new(owner_principal_id));
    let mut authority = AUTHORITY
        .lock()
        .map_err(|_| "world authority lock poisoned".to_string())?;
    let state = authority.world(&world_id).map_err(|error| error.to_string())?;
    match state.active_control_lease {
        Some(lease) if lease.principal == owner => authority
            .release_control(&world_id, &owner, now_ms)
            .map_err(|error| error.to_string()),
        Some(lease) if lease.principal.kind == medousa_world::WorldPrincipalKind::Human => Err(
            "owned browser is controlled by another human principal".to_string(),
        ),
        _ => Ok(state.control_generation),
    }
}

pub fn validate_browser_action_permit(permit: &WorldActionPermit) -> Result<(), String> {
    AUTHORITY
        .lock()
        .map_err(|_| "world authority lock poisoned".to_string())?
        .validate_action_permit(permit, now_ms())
        .map_err(|error| error.to_string())
}

pub fn forget_owned_browser_world(
    world_id: &str,
    owner_principal_id: &str,
) -> Result<(), String> {
    let world_id = WorldId::new(world_id);
    let owner = WorldPrincipal::human(WorldPrincipalId::new(owner_principal_id));
    match AUTHORITY
        .lock()
        .map_err(|_| "world authority lock poisoned".to_string())?
        .remove_world(&world_id, &owner, now_ms())
    {
        Ok(_) | Err(WorldAuthorityError::WorldNotFound(_)) => {}
        Err(error) => return Err(error.to_string()),
    };
    BROWSER_OBSERVATIONS
        .lock()
        .map_err(|_| "browser observation mirror lock poisoned".to_string())?
        .remove(&world_id);
    Ok(())
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

pub fn record_browser_observation(
    admission: &BrowserWorldAdmission,
    observation: BrowserObservation,
) -> Result<(), String> {
    let expected_resource = format!("browser-tab:{}", observation.tab_id);
    if admission.permit.resource_id.as_str() != expected_resource {
        return Err("browser observation does not match its admitted resource".to_string());
    }
    let mut mirrors = BROWSER_OBSERVATIONS
        .lock()
        .map_err(|_| "browser observation mirror lock poisoned".to_string())?;
    match mirrors.get_mut(&admission.permit.world_id) {
        Some(current) => merge_browser_observation(current, observation)?,
        None if observation.full => {
            mirrors.insert(admission.permit.world_id.clone(), observation);
        }
        None => {
            return Err("browser observation delta arrived before a full mirror".to_string());
        }
    }
    Ok(())
}

pub fn validate_browser_pixel_fence(
    authority_id: &str,
    driver_id: &str,
    tab_group_id: &str,
    tab_id: &str,
    expected_url: &str,
    document_id: &str,
    observation_revision: u64,
) -> Result<(), String> {
    let world_id = browser_world_id(authority_id, driver_id, tab_group_id);
    let mirrors = BROWSER_OBSERVATIONS
        .lock()
        .map_err(|_| "browser observation mirror lock poisoned".to_string())?;
    let observation = mirrors
        .get(&world_id)
        .ok_or_else(|| "daemon has no semantic observation for this browser world".to_string())?;
    if observation.tab_id != tab_id
        || observation.document_id != document_id
        || observation.revision != observation_revision
        || !same_browser_url(&observation.url, expected_url)
    {
        return Err("pixel capture belongs to stale browser state".to_string());
    }
    Ok(())
}

pub struct BrowserElementRefFence<'a> {
    pub authority_id: &'a str,
    pub driver_id: &'a str,
    pub tab_group_id: &'a str,
    pub tab_id: &'a str,
    pub expected_url: &'a str,
    pub document_id: &'a str,
    pub revision: u64,
    pub targets: &'a [(String, String)],
    pub allow_high_risk: bool,
}

pub fn validate_browser_element_refs(fence: BrowserElementRefFence<'_>) -> Result<(), String> {
    let world_id = browser_world_id(fence.authority_id, fence.driver_id, fence.tab_group_id);
    let mirrors = BROWSER_OBSERVATIONS
        .lock()
        .map_err(|_| "browser observation mirror lock poisoned".to_string())?;
    let observation = mirrors
        .get(&world_id)
        .ok_or_else(|| "daemon has no semantic observation for this browser world".to_string())?;
    if observation.tab_id != fence.tab_id
        || observation.document_id != fence.document_id
        || observation.revision != fence.revision
        || !same_browser_url(&observation.url, fence.expected_url)
    {
        return Err("semantic target belongs to stale browser state".to_string());
    }
    for (action, element_ref) in fence.targets {
        let node = observation
            .nodes
            .iter()
            .find(|node| node.element_ref == *element_ref)
            .ok_or_else(|| {
                format!(
                    "opaque element ref is not in revision {}",
                    fence.revision
                )
            })?;
        if node.sensitive {
            return Err("credential, payment, and file inputs remain operator-only".to_string());
        }
        if !fence.allow_high_risk && semantic_action_is_high_risk(action, node) {
            return Err(
                "resolved target semantics require explicit high-risk authorization".to_string(),
            );
        }
    }
    Ok(())
}

fn semantic_action_is_high_risk(
    action: &str,
    node: &medousa_browser_bridge::BrowserSemanticNode,
) -> bool {
    if !matches!(action, "click" | "press" | "select") {
        return false;
    }
    let semantics = format!("{} {} {}", node.role, node.name, node.tag).to_lowercase();
    [
        "submit",
        "checkout",
        "purchase",
        "delete",
        "remove account",
        "pay now",
        "confirm order",
    ]
    .iter()
    .any(|marker| semantics.contains(marker))
}

fn same_browser_url(left: &str, right: &str) -> bool {
    left.trim().trim_end_matches('/') == right.trim().trim_end_matches('/')
}

fn merge_browser_observation(
    current: &mut BrowserObservation,
    next: BrowserObservation,
) -> Result<(), String> {
    if next.full {
        if next.revision < current.revision {
            return Err("browser observation revision moved backwards".to_string());
        }
        *current = next;
        return Ok(());
    }
    if next.document_id != current.document_id {
        return Err("browser observation delta does not extend the daemon mirror".to_string());
    }
    if next.revision == current.revision {
        current.captured_at_ms = current.captured_at_ms.max(next.captured_at_ms);
        return Ok(());
    }
    if next.revision < current.revision
        || next.base_revision.is_none_or(|base| base > current.revision)
    {
        return Err("browser observation delta does not extend the daemon mirror".to_string());
    }
    let mut nodes = current
        .nodes
        .drain(..)
        .map(|node| (node.element_ref.clone(), node))
        .collect::<BTreeMap<_, _>>();
    for element_ref in &next.removed_refs {
        nodes.remove(element_ref);
    }
    for node in next.nodes {
        nodes.insert(node.element_ref.clone(), node);
    }
    current.schema_version = next.schema_version;
    current.tab_id = next.tab_id;
    current.url = next.url;
    current.title = next.title;
    current.revision = next.revision;
    current.base_revision = None;
    current.full = true;
    current.viewport = next.viewport;
    current.nodes = nodes.into_values().collect();
    current.removed_refs.clear();
    current.truncated |= next.truncated;
    current.captured_at_ms = next.captured_at_ms;
    current.untrusted_content = true;
    Ok(())
}

fn browser_world_id(authority_id: &str, driver_id: &str, tab_group_id: &str) -> WorldId {
    WorldId::new(format!(
        "world:browser:{authority_id}:{driver_id}:{tab_group_id}"
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
    use super::*;

    fn semantic_node(element_ref: &str, name: &str) -> medousa_browser_bridge::BrowserSemanticNode {
        medousa_browser_bridge::BrowserSemanticNode {
            element_ref: element_ref.to_string(),
            parent_ref: None,
            role: "button".to_string(),
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

    fn observation(
        document_id: &str,
        revision: u64,
        base_revision: Option<u64>,
        full: bool,
        nodes: Vec<medousa_browser_bridge::BrowserSemanticNode>,
        removed_refs: Vec<&str>,
    ) -> BrowserObservation {
        BrowserObservation {
            schema_version: medousa_browser_bridge::BROWSER_OBSERVATION_SCHEMA_VERSION,
            tab_id: "tab-one".to_string(),
            url: "https://example.test/".to_string(),
            title: "Example".to_string(),
            document_id: document_id.to_string(),
            revision,
            base_revision,
            full,
            viewport: medousa_browser_bridge::BrowserObservationViewport {
                width: 1280,
                height: 720,
                scroll_x: 0,
                scroll_y: 0,
                device_scale_factor: 2.0,
            },
            nodes,
            removed_refs: removed_refs.into_iter().map(str::to_string).collect(),
            truncated: false,
            captured_at_ms: revision,
            untrusted_content: true,
        }
    }

    #[test]
    fn browser_world_identity_is_bound_to_authority_and_tab_group() {
        assert_ne!(
            browser_world_id("workshop:a", "driver:one", "group:one"),
            browser_world_id("workshop:b", "driver:one", "group:one")
        );
        assert_ne!(
            browser_world_id("workshop:a", "driver:one", "group:one"),
            browser_world_id("workshop:a", "driver:one", "group:two")
        );
        assert_ne!(
            browser_world_id("workshop:a", "driver:one", "group:one"),
            browser_world_id("workshop:a", "driver:two", "group:one")
        );
    }

    #[test]
    fn browser_observation_delta_advances_the_daemon_mirror() {
        let mut current = observation(
            "doc-one",
            1,
            None,
            true,
            vec![semantic_node("ref-one", "Before"), semantic_node("ref-two", "Remove")],
            Vec::new(),
        );
        merge_browser_observation(
            &mut current,
            observation(
                "doc-one",
                2,
                Some(1),
                false,
                vec![semantic_node("ref-one", "After")],
                vec!["ref-two"],
            ),
        )
        .expect("delta should merge");

        assert!(current.full);
        assert_eq!(current.revision, 2);
        assert_eq!(current.base_revision, None);
        assert_eq!(current.nodes.len(), 1);
        assert_eq!(current.nodes[0].name, "After");
    }

    #[test]
    fn browser_observation_delta_cannot_cross_documents() {
        let mut current = observation(
            "doc-one",
            1,
            None,
            true,
            vec![semantic_node("ref-one", "Before")],
            Vec::new(),
        );
        let error = merge_browser_observation(
            &mut current,
            observation(
                "doc-two",
                2,
                Some(1),
                false,
                vec![semantic_node("ref-two", "After")],
                Vec::new(),
            ),
        )
        .expect_err("cross-document delta must fail");
        assert!(error.contains("does not extend"));
    }

    #[test]
    fn browser_element_refs_are_resolved_against_the_daemon_mirror() {
        let authority_id = "workshop:semantic-test";
        let driver_id = "driver:semantic-test";
        let tab_group_id = "group:semantic-test";
        let mut observed = observation(
            "doc-one",
            4,
            None,
            true,
            vec![semantic_node("ref-one", "Continue")],
            Vec::new(),
        );
        observed.tab_id = "tab-one".to_string();
        BROWSER_OBSERVATIONS
            .lock()
            .expect("browser observations")
            .insert(
                browser_world_id(authority_id, driver_id, tab_group_id),
                observed,
            );

        validate_browser_element_refs(BrowserElementRefFence {
            authority_id,
            driver_id,
            tab_group_id,
            tab_id: "tab-one",
            expected_url: "https://example.test",
            document_id: "doc-one",
            revision: 4,
            targets: &[("click".to_string(), "ref-one".to_string())],
            allow_high_risk: false,
        })
        .expect("known ref should pass");
        let error = validate_browser_element_refs(BrowserElementRefFence {
            authority_id,
            driver_id,
            tab_group_id,
            tab_id: "tab-one",
            expected_url: "https://example.test",
            document_id: "doc-one",
            revision: 3,
            targets: &[("click".to_string(), "ref-one".to_string())],
            allow_high_risk: false,
        })
        .expect_err("stale revision must fail");
        assert!(error.contains("stale browser state"));
    }

    #[test]
    fn browser_element_semantics_require_explicit_high_risk_authority() {
        let authority_id = "workshop:risk-test";
        let driver_id = "driver:risk-test";
        let tab_group_id = "group:risk-test";
        BROWSER_OBSERVATIONS
            .lock()
            .expect("browser observations")
            .insert(
                browser_world_id(authority_id, driver_id, tab_group_id),
                observation(
                    "doc-one",
                    1,
                    None,
                    true,
                    vec![semantic_node("ref-delete", "Delete account")],
                    Vec::new(),
                ),
            );

        let targets = [("click".to_string(), "ref-delete".to_string())];
        let error = validate_browser_element_refs(BrowserElementRefFence {
            authority_id,
            driver_id,
            tab_group_id,
            tab_id: "tab-one",
            expected_url: "https://example.test",
            document_id: "doc-one",
            revision: 1,
            targets: &targets,
            allow_high_risk: false,
        })
        .expect_err("high-risk target must fail");
        assert!(error.contains("high-risk authorization"));
        validate_browser_element_refs(BrowserElementRefFence {
            authority_id,
            driver_id,
            tab_group_id,
            tab_id: "tab-one",
            expected_url: "https://example.test",
            document_id: "doc-one",
            revision: 1,
            targets: &targets,
            allow_high_risk: true,
        })
        .expect("explicit high-risk authority should pass");
    }

    #[test]
    fn owned_browser_human_intents_share_the_authority_lifecycle() {
        let suffix = Uuid::new_v4().simple().to_string();
        let authority_id = format!("workshop:owned-{suffix}");
        let driver_id = format!("isolated-browser:{suffix}");
        let tab_group_id = format!("group:{suffix}");
        let owner_principal_id = format!("human:profile-{suffix}");
        let world_id = register_owned_browser_world(
            &authority_id,
            &driver_id,
            &tab_group_id,
            &owner_principal_id,
        )
        .expect("owned world should register");

        let observation = admit_owned_browser_human_intent(OwnedBrowserHumanIntent {
            authority_id: &authority_id,
            driver_id: &driver_id,
            tab_group_id: &tab_group_id,
            tab_id: "tab-one",
            owner_principal_id: &owner_principal_id,
            trace_id: "trace:human-observe",
            summary: "human observation",
            effect_class: WorldEffectClass::Observe,
        })
        .expect("owner should observe");
        assert_eq!(observation.permit.world_id, world_id);
        assert_eq!(observation.permit.control_generation, None);
        complete_browser_action(&observation, "observation committed")
            .expect("observation should commit");

        let navigation = admit_owned_browser_human_intent(OwnedBrowserHumanIntent {
            authority_id: &authority_id,
            driver_id: &driver_id,
            tab_group_id: &tab_group_id,
            tab_id: "tab-one",
            owner_principal_id: &owner_principal_id,
            trace_id: "trace:human-navigate",
            summary: "human navigation",
            effect_class: WorldEffectClass::LocalReversible,
        })
        .expect("owner should navigate");
        assert!(navigation.permit.control_generation.is_some());
        complete_browser_action(&navigation, "navigation committed")
            .expect("navigation should commit");

        forget_owned_browser_world(world_id.as_str(), &owner_principal_id)
            .expect("owner should clean up the world");
        assert!(
            admit_owned_browser_human_intent(OwnedBrowserHumanIntent {
                authority_id: &authority_id,
                driver_id: &driver_id,
                tab_group_id: &tab_group_id,
                tab_id: "tab-one",
                owner_principal_id: &owner_principal_id,
                trace_id: "trace:after-cleanup",
                summary: "stale action",
                effect_class: WorldEffectClass::Observe,
            })
            .is_err()
        );
    }
}
