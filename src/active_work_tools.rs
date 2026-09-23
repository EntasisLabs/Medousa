//! Read-only owner inventory across local and authorized connected workshops.

use crate::typed_tools::{ToolId, medousa_tool};
use medousa_types::assistant_placement::{
    AssistantExecutorKind, AssistantPlacementCandidate, AssistantPlacementRequest,
};
use stasis::prelude::{Result, StasisError};

pub const COGNITION_ACTIVE_WORK_DISCOVER: &str = "cognition_active_work_discover";
pub const COGNITION_ASSISTANT_PLACEMENT: &str = "cognition_assistant_placement";
const ACTIVE_WORK_DISCOVER_ID: ToolId = ToolId::new(COGNITION_ACTIVE_WORK_DISCOVER);
const ASSISTANT_PLACEMENT_ID: ToolId = ToolId::new(COGNITION_ASSISTANT_PLACEMENT);

pub fn register_active_work_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    delegation: Option<std::sync::Arc<crate::delegation::DelegationService>>,
) -> Result<()> {
    registry.register_typed_tool(ActiveWorkDiscoverTool {
        delegation: delegation.clone(),
    })?;
    registry.register_typed_tool(AssistantPlacementDiscoverTool { delegation })?;
    Ok(())
}

fn error(error: impl std::fmt::Display) -> StasisError {
    StasisError::PortFailure(error.to_string())
}

fn admitted()
-> Result<std::sync::Arc<crate::agent_runtime::execution_context::TurnExecutionContext>> {
    crate::agent_runtime::execution_context::active_turn_execution_context()
        .ok_or_else(|| error("active work discovery requires an admitted owner turn"))
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct ActiveWorkDiscoverInput {
    /// Include accepted, discarded, and failed Forge work.
    #[serde(default)]
    include_terminal: bool,
}

struct ActiveWorkDiscoverTool {
    delegation: Option<std::sync::Arc<crate::delegation::DelegationService>>,
}

#[medousa_tool(id = ACTIVE_WORK_DISCOVER_ID)]
impl ActiveWorkDiscoverTool {
    /// List projects and agent sessions visible to the user across the local and authorized connected workshops. Use this for “what are we working on?” before asking the user to identify a project. The response is read-only and preserves unavailable workshops explicitly; never describe partial coverage as complete.
    async fn invoke_typed(&self, input: ActiveWorkDiscoverInput) -> Result<serde_json::Value> {
        let turn = admitted()?;
        let mut workshops = Vec::new();
        #[cfg(feature = "full-daemon")]
        if let Some(host) = crate::daemon::coordination::local_coordination_host() {
            let inventory = host
                .active_work_for_turn(turn.principal(), input.include_terminal)
                .await
                .map_err(error)?;
            workshops.push(serde_json::json!({
                "available": true,
                "target": {
                    "execution_runtime_id": inventory["coverage"]["execution_runtime_id"],
                    "authority_id": inventory["coverage"]["authority_id"],
                    "label": "Current workshop"
                },
                "inventory": inventory,
            }));
        }
        #[cfg(not(feature = "full-daemon"))]
        let _ = &turn;
        if let Some(delegation) = &self.delegation {
            workshops.extend(
                delegation
                    .active_work_inventories(input.include_terminal)
                    .await
                    .map_err(error)?,
            );
        }
        if workshops.is_empty() {
            return Err(error(
                "active work discovery has no local or connected workshops",
            ));
        }
        let complete_mesh = workshops.iter().all(|row| {
            row.get("available")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
        });
        Ok(serde_json::json!({
            "coverage": {"kind": "authorized_mesh", "complete_mesh": complete_mesh},
            "workshops": workshops,
            "policy": "Read-only inventory. Unavailable workshops remain explicit and are never treated as empty."
        }))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PlacementTargetObservation {
    pub runtime_id: String,
    pub workshop_id: String,
    pub workshop_authority_id: Option<String>,
    pub capabilities: std::collections::BTreeSet<String>,
    pub available: bool,
    pub agent_selectable: bool,
    pub unavailable_reason: Option<String>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct AssistantPlacementDiscoverInput {
    /// Explicit placement constraints. This query cannot authorize or launch work.
    requirements: AssistantPlacementRequest,
    /// Include accepted, discarded, and failed Forge work as candidate bindings.
    #[serde(default)]
    include_terminal: bool,
}

struct AssistantPlacementDiscoverTool {
    delegation: Option<std::sync::Arc<crate::delegation::DelegationService>>,
}

#[medousa_tool(id = ASSISTANT_PLACEMENT_ID)]
impl AssistantPlacementDiscoverTool {
    /// Rank currently observed workshop and local ACP candidates against explicit task needs, requested or forbidden runtime/workshop constraints, governed work, and context locality. The result is read-only placement guidance; it never grants authority, adopts work, approves, or dispatches an executor.
    async fn invoke_typed(
        &self,
        input: AssistantPlacementDiscoverInput,
    ) -> Result<serde_json::Value> {
        let turn = admitted()?;
        let mut workshops = Vec::new();
        let mut target_observations = Vec::new();
        #[cfg(feature = "full-daemon")]
        let mut local_peer_discovery = None;
        #[cfg(not(feature = "full-daemon"))]
        let local_peer_discovery = None;
        #[cfg(feature = "full-daemon")]
        let mut local_peer_coverage_complete = true;
        #[cfg(not(feature = "full-daemon"))]
        let local_peer_coverage_complete = true;
        #[cfg(feature = "full-daemon")]
        if let Some(host) = crate::daemon::coordination::local_coordination_host() {
            let inventory = host
                .active_work_for_turn(turn.principal(), input.include_terminal)
                .await
                .map_err(error)?;
            let targets = host.execution_target_inventory().await;
            for target in &targets.targets {
                target_observations.push(PlacementTargetObservation {
                    runtime_id: target.runtime_id.clone(),
                    workshop_id: inventory["coverage"]["authority_id"]
                        .as_str()
                        .unwrap_or("unknown")
                        .to_string(),
                    workshop_authority_id: inventory["coverage"]["authority_id"]
                        .as_str()
                        .map(str::to_string),
                    capabilities: target.capabilities.clone(),
                    available: true,
                    agent_selectable: target.agent_selectable,
                    unavailable_reason: None,
                });
            }
            local_peer_discovery = match host
                .discover_for_turn(
                    turn.principal(),
                    medousa_types::SessionId::parse(turn.session_id().as_str()).map_err(error)?,
                )
                .await
            {
                Ok(discovery) => Some(discovery),
                Err(_) => {
                    local_peer_coverage_complete = false;
                    None
                }
            };
            workshops.push(serde_json::json!({
                "available": true,
                "target": {
                    "execution_runtime_id": inventory["coverage"]["execution_runtime_id"],
                    "authority_id": inventory["coverage"]["authority_id"],
                    "label": "Current workshop"
                },
                "inventory": inventory,
            }));
        }
        #[cfg(not(feature = "full-daemon"))]
        let _ = &turn;

        let mut target_probe_complete = true;
        if let Some(delegation) = &self.delegation {
            workshops.extend(
                delegation
                    .active_work_inventories(input.include_terminal)
                    .await
                    .map_err(error)?,
            );
            match delegation.authorized_targets().await {
                Ok(targets) => {
                    for target in targets {
                        target_observations.push(PlacementTargetObservation {
                            runtime_id: target.target.peer_device_id,
                            workshop_id: target.target.route_ref,
                            workshop_authority_id: None,
                            capabilities: target.candidate.capabilities.capabilities.clone(),
                            available: true,
                            agent_selectable: target.candidate.agent_selectable,
                            unavailable_reason: (!target.candidate.agent_selectable).then(|| {
                                "destination policy does not permit agent targeting".into()
                            }),
                        });
                    }
                }
                Err(_) => target_probe_complete = false,
            }
        }
        if workshops.is_empty() {
            return Err(error(
                "Assistant placement has no local or connected workshop inventory",
            ));
        }
        let targets_confirmed = workshops.iter().all(|row| {
            if row["available"].as_bool() != Some(true) {
                return true;
            }
            let runtime_id = row["target"]["execution_runtime_id"].as_str();
            let workshop_id = row["target"]["route_ref"].as_str();
            target_observations.iter().any(|target| {
                Some(target.runtime_id.as_str()) == runtime_id
                    || workshop_id == Some(target.workshop_id.as_str())
            })
        });
        let coverage_complete = target_probe_complete
            && targets_confirmed
            && local_peer_coverage_complete
            && workshops
                .iter()
                .all(|row| row["available"].as_bool() == Some(true));
        let candidates = build_placement_candidates(
            &workshops,
            &target_observations,
            local_peer_discovery.as_ref(),
        );
        let mut requirements = input.requirements;
        requirements.source_workshop_authority_id = Some(
            crate::workshop_authority::current()
                .map_err(error)?
                .to_string(),
        );
        let result = crate::assistant_placement::rank_candidates(
            &requirements,
            coverage_complete,
            candidates,
        )
        .map_err(error)?;
        serde_json::to_value(result).map_err(error)
    }
}

pub(crate) fn build_placement_candidates(
    rows: &[serde_json::Value],
    targets: &[PlacementTargetObservation],
    peer_discovery: Option<&serde_json::Value>,
) -> Vec<AssistantPlacementCandidate> {
    use medousa_types::assistant_placement::AssistantCandidateAvailability as Availability;
    use std::collections::BTreeSet;

    let mut candidates = Vec::new();
    for row in rows {
        let target = &row["target"];
        let runtime_id = target["execution_runtime_id"].as_str().map(str::to_string);
        let workshop_id = target["route_ref"].as_str().map(str::to_string);
        let label = target["label"].as_str().unwrap_or("Workshop");
        let inventory = &row["inventory"];
        let authority = inventory["coverage"]["authority_id"]
            .as_str()
            .map(str::to_string);
        let observed = targets.iter().find(|candidate| {
            Some(candidate.runtime_id.as_str()) == runtime_id.as_deref()
                || workshop_id.as_deref() == Some(candidate.workshop_id.as_str())
        });
        let availability = match row["available"].as_bool() {
            Some(false) => Availability::Unavailable,
            Some(true)
                if observed.is_some_and(|target| target.available && target.agent_selectable) =>
            {
                Availability::Available
            }
            Some(true)
                if observed.is_some_and(|target| !target.available || !target.agent_selectable) =>
            {
                Availability::Unavailable
            }
            Some(true) => Availability::Unknown,
            None => Availability::Unknown,
        };
        let unavailable_reason = if availability == Availability::Unavailable {
            row["error"]
                .as_str()
                .map(str::to_string)
                .or_else(|| observed.and_then(|target| target.unavailable_reason.clone()))
        } else if availability == Availability::Unknown {
            Some("execution target is not currently confirmed agent-selectable".into())
        } else {
            None
        };
        let capabilities = observed
            .map(|target| target.capabilities.clone())
            .unwrap_or_default();
        let authority =
            authority.or_else(|| observed.and_then(|target| target.workshop_authority_id.clone()));
        let workshop_id = workshop_id.or_else(|| observed.map(|target| target.workshop_id.clone()));
        let projects = inventory["projects"].as_array();
        if let Some(projects) = projects.filter(|projects| !projects.is_empty()) {
            for project in projects {
                let work_id = project["forge_work_id"].as_str().map(str::to_string);
                candidates.push(crate::assistant_placement::candidate(
                    format!(
                        "workshop:{}:{}:{}",
                        workshop_id.as_deref().unwrap_or("unknown"),
                        runtime_id.as_deref().unwrap_or("unknown"),
                        work_id.as_deref().unwrap_or("unknown")
                    ),
                    format!(
                        "{label} · {}",
                        project["title"].as_str().unwrap_or("Governed work")
                    ),
                    AssistantExecutorKind::Workshop,
                    None,
                    runtime_id.clone(),
                    workshop_id.clone().or_else(|| authority.clone()),
                    authority.clone(),
                    work_id,
                    capabilities.clone(),
                    availability,
                    unavailable_reason.clone(),
                    None,
                ));
            }
        } else {
            candidates.push(crate::assistant_placement::candidate(
                format!(
                    "workshop:{}:{}",
                    workshop_id.as_deref().unwrap_or("unknown"),
                    runtime_id.as_deref().unwrap_or("unknown")
                ),
                label.to_string(),
                AssistantExecutorKind::Workshop,
                None,
                runtime_id.clone(),
                workshop_id.clone().or_else(|| authority.clone()),
                authority.clone(),
                None,
                capabilities,
                availability,
                unavailable_reason,
                None,
            ));
        }
    }

    if let Some(peers) = peer_discovery.and_then(|discovery| discovery["peers"].as_array()) {
        let local_workshops = rows.iter().filter(|row| {
            row["target"]["route_ref"].is_null()
                && row["inventory"]["coverage"]["kind"].as_str() == Some("current_workshop")
        });
        for row in local_workshops {
            let inventory = &row["inventory"];
            let authority = inventory["coverage"]["authority_id"]
                .as_str()
                .map(str::to_string);
            let runtime = inventory["coverage"]["execution_runtime_id"]
                .as_str()
                .map(str::to_string);
            let projects = inventory["projects"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            for peer in peers {
                let target = &peer["target"];
                let adapter = target["runtime"].as_str().map(str::to_ascii_lowercase);
                let peer_runtime = target["execution_runtime_id"].as_str().map(str::to_string);
                let state = peer["availability"]["state"]
                    .as_str()
                    .unwrap_or("unavailable");
                let availability = if state == "ready" {
                    Availability::Available
                } else {
                    Availability::Unavailable
                };
                let unavailable_reason =
                    peer["availability"]["reason"].as_str().map(str::to_string);
                let mut added_unbound = false;
                for project in projects {
                    let work_id = project["forge_work_id"].as_str().map(str::to_string);
                    let sessions = project["agent_sessions"]
                        .as_array()
                        .map(Vec::as_slice)
                        .unwrap_or(&[]);
                    let exact_sessions = sessions
                        .iter()
                        .filter(|session| {
                            session["adoptable"].as_bool() == Some(true)
                                && session["medousa_owned"].as_bool() == Some(false)
                                && session["cancelled"].as_bool() == Some(false)
                                && session["runtime"].as_str().map(str::to_ascii_lowercase)
                                    == adapter
                                && session["forge_work_id"].as_str() == work_id.as_deref()
                        })
                        .collect::<Vec<_>>();
                    candidates.push(crate::assistant_placement::candidate(
                        format!(
                            "acp:{}:{}:{}",
                            adapter.as_deref().unwrap_or("unknown"),
                            peer_runtime.as_deref().unwrap_or("unknown"),
                            work_id.as_deref().unwrap_or("unknown")
                        ),
                        format!(
                            "{} on current workshop · {}",
                            adapter.as_deref().unwrap_or("ACP"),
                            project["title"].as_str().unwrap_or("Governed work")
                        ),
                        AssistantExecutorKind::LocalAcp,
                        adapter.clone(),
                        peer_runtime.clone().or_else(|| runtime.clone()),
                        authority.clone(),
                        authority.clone(),
                        work_id.clone(),
                        BTreeSet::new(),
                        availability,
                        unavailable_reason.clone(),
                        None,
                    ));
                    for session in exact_sessions {
                        let session_id = session["agent_session_id"].as_str().map(str::to_string);
                        candidates.push(crate::assistant_placement::candidate(
                            format!("acp-adopt:{}", session_id.as_deref().unwrap_or("unknown")),
                            format!(
                                "Adopt {} session on {}",
                                adapter.as_deref().unwrap_or("ACP"),
                                project["title"].as_str().unwrap_or("governed work")
                            ),
                            AssistantExecutorKind::LocalAcp,
                            adapter.clone(),
                            peer_runtime.clone().or_else(|| runtime.clone()),
                            authority.clone(),
                            authority.clone(),
                            work_id.clone(),
                            BTreeSet::new(),
                            availability,
                            unavailable_reason.clone(),
                            session_id,
                        ));
                    }
                    added_unbound = true;
                }
                if !added_unbound {
                    candidates.push(crate::assistant_placement::candidate(
                        format!(
                            "acp:{}:{}",
                            adapter.as_deref().unwrap_or("unknown"),
                            peer_runtime.as_deref().unwrap_or("unknown")
                        ),
                        format!(
                            "{} on current workshop",
                            adapter.as_deref().unwrap_or("ACP")
                        ),
                        AssistantExecutorKind::LocalAcp,
                        adapter,
                        peer_runtime.or_else(|| runtime.clone()),
                        authority.clone(),
                        authority.clone(),
                        None,
                        BTreeSet::new(),
                        availability,
                        unavailable_reason,
                        None,
                    ));
                }
            }
        }
    }
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn active_work_discovery_requires_an_admitted_owner_turn() {
        assert!(
            ActiveWorkDiscoverTool { delegation: None }
                .invoke_typed(ActiveWorkDiscoverInput {
                    include_terminal: false,
                })
                .await
                .unwrap_err()
                .to_string()
                .contains("admitted owner turn")
        );
    }

    #[test]
    fn placement_exposes_only_exact_adoptable_sessions_bound_to_the_work() {
        let workshops = serde_json::json!([{
            "available": true,
            "target": {
                "execution_runtime_id": "runtime-a",
                "authority_id": "authority-a",
                "label": "Current workshop"
            },
            "inventory": {
                "coverage": {"kind":"current_workshop", "authority_id": "authority-a", "execution_runtime_id": "runtime-a"},
                "projects": [{
                    "forge_work_id": "work-1",
                    "title": "Exact project",
                    "agent_sessions": [
                        {"agent_session_id":"adopt-this", "runtime":"cursor", "forge_work_id":"work-1", "adoptable":true, "medousa_owned":false, "cancelled":false},
                        {"agent_session_id":"owned-session", "runtime":"cursor", "forge_work_id":"work-1", "adoptable":false, "medousa_owned":true, "cancelled":false},
                        {"agent_session_id":"other-work", "runtime":"cursor", "forge_work_id":"work-2", "adoptable":true, "medousa_owned":false, "cancelled":false}
                    ]
                }]
            }
        }]);
        let peers = serde_json::json!({
            "peers": [{
                "target": {"runtime":"cursor", "execution_runtime_id":"runtime-a"},
                "availability": {"state":"ready"}
            }]
        });
        let targets = [PlacementTargetObservation {
            runtime_id: "runtime-a".into(),
            workshop_id: "authority-a".into(),
            workshop_authority_id: Some("authority-a".into()),
            capabilities: Default::default(),
            available: true,
            agent_selectable: true,
            unavailable_reason: None,
        }];
        let candidates =
            build_placement_candidates(workshops.as_array().unwrap(), &targets, Some(&peers));
        let exact = candidates
            .iter()
            .find(|candidate| candidate.adoptable_agent_session_id.as_deref() == Some("adopt-this"))
            .expect("exact adoptable session");
        assert_eq!(exact.forge_work_id.as_deref(), Some("work-1"));
        assert!(!candidates.iter().any(|candidate| {
            candidate.adoptable_agent_session_id.as_deref() == Some("owned-session")
                || candidate.adoptable_agent_session_id.as_deref() == Some("other-work")
        }));
        let request = AssistantPlacementRequest {
            requested_executor: Some(AssistantExecutorKind::LocalAcp),
            requested_adapter: Some("cursor".into()),
            requested_adoptable_agent_session_id: Some("adopt-this".into()),
            governed_work: Some(
                medousa_types::assistant_placement::AssistantGovernedWorkBinding {
                    workshop_authority_id: "authority-a".into(),
                    forge_work_id: "work-1".into(),
                },
            ),
            ..AssistantPlacementRequest::default()
        };
        let placement =
            crate::assistant_placement::rank_candidates(&request, true, candidates).unwrap();
        let selected = placement
            .candidates
            .iter()
            .find(|candidate| candidate.adoptable_agent_session_id.as_deref() == Some("adopt-this"))
            .unwrap();
        assert!(selected.eligible);
        assert_eq!(selected.rank, Some(1));
    }
}
