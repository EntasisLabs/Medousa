//! Evidence-based placement discovery. This module ranks observed candidates;
//! it never admits work or grants execution authority.

use std::collections::BTreeSet;

use anyhow::{Result, bail};
use medousa_types::assistant_placement::{
    ASSISTANT_PLACEMENT_SCHEMA_VERSION, AssistantCandidateAvailability, AssistantContextLocality,
    AssistantExecutorKind, AssistantPlacementCandidate, AssistantPlacementRequest,
    AssistantPlacementResult,
};

/// Rank observed candidate rows against explicit caller constraints. Candidate
/// order is deterministic; unavailable and unknown rows remain in the result.
pub fn rank_candidates(
    request: &AssistantPlacementRequest,
    complete: bool,
    mut candidates: Vec<AssistantPlacementCandidate>,
) -> Result<AssistantPlacementResult> {
    if request.schema_version != ASSISTANT_PLACEMENT_SCHEMA_VERSION {
        bail!("unsupported Assistant placement request schema version");
    }
    validate_constraint_size(request)?;

    let requested_runtime = normalized(request.requested_runtime_id.as_deref());
    let requested_workshop = normalized(request.requested_workshop_id.as_deref());
    let requested_authority = normalized(request.requested_workshop_authority_id.as_deref());
    let requested_adapter = normalized(request.requested_adapter.as_deref());
    let source_authority = normalized(request.source_workshop_authority_id.as_deref());

    for candidate in &mut candidates {
        candidate.eligible = true;
        candidate.rank = None;
        candidate.reasons.clear();
        if candidate.availability != AssistantCandidateAvailability::Available {
            candidate.eligible = false;
            candidate.reasons.push(match candidate.availability {
                AssistantCandidateAvailability::Unavailable => "target_unavailable".into(),
                AssistantCandidateAvailability::Unknown => "target_availability_unknown".into(),
                AssistantCandidateAvailability::Available => unreachable!(),
            });
        }
        if candidate
            .workshop_authority_id
            .as_deref()
            .is_none_or(str::is_empty)
        {
            candidate.eligible = false;
            candidate.reasons.push("workshop_authority_unknown".into());
        }

        if request
            .requested_executor
            .is_some_and(|executor| executor != candidate.executor)
        {
            reject(candidate, "requested_executor_mismatch");
        }
        if !request.forbidden_executors.is_empty()
            && request.forbidden_executors.contains(&candidate.executor)
        {
            reject(candidate, "executor_forbidden");
        }

        if requested_adapter
            .is_some_and(|adapter| candidate.adapter.as_deref().map(str::trim) != Some(adapter))
        {
            reject(candidate, "requested_adapter_mismatch");
        }
        if candidate.adapter.as_deref().is_some_and(|adapter| {
            request
                .forbidden_adapters
                .iter()
                .any(|forbidden| forbidden.trim() == adapter.trim())
        }) {
            reject(candidate, "adapter_forbidden");
        }

        if requested_runtime
            .is_some_and(|runtime| candidate.runtime_id.as_deref().map(str::trim) != Some(runtime))
        {
            reject(candidate, "requested_runtime_mismatch");
        }
        if candidate.runtime_id.as_deref().is_some_and(|runtime| {
            request
                .forbidden_runtime_ids
                .iter()
                .any(|forbidden| forbidden.trim() == runtime.trim())
        }) {
            reject(candidate, "runtime_forbidden");
        }

        if requested_authority.is_some_and(|authority| {
            candidate.workshop_authority_id.as_deref().map(str::trim) != Some(authority)
        }) {
            reject(candidate, "requested_workshop_mismatch");
        }
        if requested_workshop.is_some_and(|workshop| {
            candidate.workshop_id.as_deref().map(str::trim) != Some(workshop)
        }) {
            reject(candidate, "requested_workshop_mismatch");
        }
        if candidate.workshop_id.as_deref().is_some_and(|workshop| {
            request
                .forbidden_workshop_ids
                .iter()
                .any(|forbidden| forbidden.trim() == workshop.trim())
        }) {
            reject(candidate, "workshop_forbidden");
        }
        if candidate
            .workshop_authority_id
            .as_deref()
            .is_some_and(|authority| {
                request
                    .forbidden_workshop_authority_ids
                    .iter()
                    .any(|forbidden| forbidden.trim() == authority.trim())
            })
        {
            reject(candidate, "workshop_forbidden");
        }

        let missing_capabilities = request
            .required_capabilities
            .difference(&candidate.capabilities)
            .cloned()
            .collect::<Vec<_>>();
        if !missing_capabilities.is_empty() {
            reject(
                candidate,
                &format!(
                    "required_capabilities_unconfirmed:{}",
                    missing_capabilities.join(",")
                ),
            );
        }

        if let Some(work) = &request.governed_work {
            if candidate.workshop_authority_id.as_deref()
                != Some(work.workshop_authority_id.as_str())
            {
                reject(candidate, "governed_work_authority_mismatch");
            }
            if candidate.forge_work_id.as_deref() != Some(work.forge_work_id.as_str()) {
                reject(candidate, "governed_work_id_mismatch");
            }
        }

        if let Some(requested_session) =
            normalized(request.requested_adoptable_agent_session_id.as_deref())
            && candidate.adoptable_agent_session_id.as_deref() != Some(requested_session)
        {
            reject(candidate, "exact_adoption_not_available");
        }

        if request.context_locality == AssistantContextLocality::RequireSourceWorkshop {
            match (source_authority, candidate.workshop_authority_id.as_deref()) {
                (Some(source), Some(target)) if source == target.trim() => {}
                (Some(_), _) => reject(candidate, "source_workshop_required"),
                (None, _) => reject(candidate, "source_workshop_authority_missing"),
            }
        } else if request.context_locality == AssistantContextLocality::PreferSourceWorkshop
            && source_authority.is_some_and(|source| {
                candidate.workshop_authority_id.as_deref().map(str::trim) != Some(source)
            })
        {
            candidate
                .reasons
                .push("source_context_not_colocated".into());
        }

        if candidate.eligible && candidate.reasons.is_empty() {
            candidate
                .reasons
                .push("matches_requested_constraints".into());
        }
    }

    // Stable ordering: eligible candidates first, colocated candidates next,
    // then identity. Unknown or unavailable candidates remain inspectable.
    candidates.sort_by(|left, right| {
        let left_local = left
            .reasons
            .iter()
            .any(|reason| reason == "source_context_not_colocated");
        let right_local = right
            .reasons
            .iter()
            .any(|reason| reason == "source_context_not_colocated");
        right
            .eligible
            .cmp(&left.eligible)
            .then_with(|| left_local.cmp(&right_local))
            .then_with(|| left.candidate_id.cmp(&right.candidate_id))
    });
    let mut next_rank = 1u32;
    for candidate in &mut candidates {
        if candidate.eligible {
            candidate.rank = Some(next_rank);
            next_rank = next_rank.saturating_add(1);
        }
    }

    Ok(AssistantPlacementResult {
        schema_version: ASSISTANT_PLACEMENT_SCHEMA_VERSION,
        complete,
        candidates,
        policy: "Read-only ranking from observed inventory. Discovery is not execution authority; destination policy, grants, approval, and exact work admission remain required.".into(),
    })
}

fn normalized(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn validate_constraint_size(request: &AssistantPlacementRequest) -> Result<()> {
    if request.required_capabilities.len() > 64
        || request.forbidden_executors.len() > 16
        || request.forbidden_adapters.len() > 16
        || request.forbidden_runtime_ids.len() > 64
        || request.forbidden_workshop_ids.len() > 64
        || request.forbidden_workshop_authority_ids.len() > 64
    {
        bail!("Assistant placement constraints exceed the bounded inventory limits");
    }
    for value in request
        .required_capabilities
        .iter()
        .chain(request.forbidden_adapters.iter())
        .chain(request.forbidden_runtime_ids.iter())
        .chain(request.forbidden_workshop_ids.iter())
        .chain(request.forbidden_workshop_authority_ids.iter())
    {
        if value.trim().is_empty() || value.len() > 256 {
            bail!("Assistant placement identifiers must be non-empty and at most 256 bytes");
        }
    }
    for value in [
        request.requested_adapter.as_deref(),
        request.requested_runtime_id.as_deref(),
        request.requested_workshop_id.as_deref(),
        request.requested_workshop_authority_id.as_deref(),
        request.source_workshop_authority_id.as_deref(),
        request.requested_adoptable_agent_session_id.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if value.trim().is_empty() || value.len() > 256 {
            bail!("Assistant placement identifiers must be non-empty and at most 256 bytes");
        }
    }
    if let Some(work) = &request.governed_work
        && (work.workshop_authority_id.trim().is_empty()
            || work.workshop_authority_id.len() > 256
            || work.forge_work_id.trim().is_empty()
            || work.forge_work_id.len() > 256)
    {
        bail!("governed work binding must include bounded workshop and Forge ids");
    }
    Ok(())
}

fn reject(candidate: &mut AssistantPlacementCandidate, reason: &str) {
    candidate.eligible = false;
    if !candidate.reasons.iter().any(|item| item == reason) {
        candidate.reasons.push(reason.to_string());
    }
}

/// Capability strings must come from an explicit execution-target inventory.
pub fn capability_set(values: impl IntoIterator<Item = String>) -> BTreeSet<String> {
    values.into_iter().collect()
}

/// Shared candidate initializer for JSON-derived workshop and ACP evidence.
#[expect(
    clippy::too_many_arguments,
    reason = "keeps observed placement evidence fields explicit at this shared initializer"
)]
pub fn candidate(
    candidate_id: String,
    label: String,
    executor: AssistantExecutorKind,
    adapter: Option<String>,
    runtime_id: Option<String>,
    workshop_id: Option<String>,
    workshop_authority_id: Option<String>,
    forge_work_id: Option<String>,
    capabilities: BTreeSet<String>,
    availability: AssistantCandidateAvailability,
    unavailable_reason: Option<String>,
    adoptable_agent_session_id: Option<String>,
) -> AssistantPlacementCandidate {
    AssistantPlacementCandidate {
        candidate_id,
        label,
        executor,
        adapter,
        runtime_id,
        workshop_id,
        workshop_authority_id,
        forge_work_id,
        capabilities,
        availability,
        unavailable_reason,
        adoptable_agent_session_id,
        eligible: false,
        rank: None,
        reasons: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req() -> AssistantPlacementRequest {
        AssistantPlacementRequest::default()
    }

    fn row(id: &str, runtime: &str, authority: &str) -> AssistantPlacementCandidate {
        candidate(
            id.into(),
            id.into(),
            AssistantExecutorKind::Workshop,
            None,
            Some(runtime.into()),
            Some("workshop-a".into()),
            Some(authority.into()),
            Some("work-1".into()),
            ["assistant.work".to_string()].into_iter().collect(),
            AssistantCandidateAvailability::Available,
            None,
            None,
        )
    }

    #[test]
    fn exact_runtime_pin_ranks_only_the_pinned_target() {
        let mut request = req();
        request.requested_runtime_id = Some("runtime-b".into());
        let result = rank_candidates(
            &request,
            true,
            vec![
                row("a", "runtime-a", "authority-a"),
                row("b", "runtime-b", "authority-b"),
            ],
        )
        .unwrap();
        assert!(
            !result
                .candidates
                .iter()
                .find(|row| row.candidate_id == "a")
                .unwrap()
                .eligible
        );
        let selected = result
            .candidates
            .iter()
            .find(|row| row.candidate_id == "b")
            .unwrap();
        assert!(selected.eligible);
        assert_eq!(selected.rank, Some(1));
    }

    #[test]
    fn forbidden_workshop_stays_visible_but_is_ineligible() {
        let mut request = req();
        request.forbidden_workshop_ids.insert("workshop-a".into());
        let result =
            rank_candidates(&request, true, vec![row("a", "runtime-a", "authority-a")]).unwrap();
        assert!(!result.candidates[0].eligible);
        assert!(
            result.candidates[0]
                .reasons
                .iter()
                .any(|reason| reason == "workshop_forbidden")
        );
    }

    #[test]
    fn requested_workshop_and_forbidden_runtime_are_applied_exactly() {
        let mut request = req();
        request.requested_workshop_id = Some("workshop-a".into());
        request.forbidden_runtime_ids.insert("runtime-a".into());
        let result =
            rank_candidates(&request, true, vec![row("a", "runtime-a", "authority-a")]).unwrap();
        assert!(!result.candidates[0].eligible);
        assert!(
            result.candidates[0]
                .reasons
                .iter()
                .any(|reason| reason == "runtime_forbidden")
        );
    }

    #[test]
    fn unavailable_target_is_retained_without_rank() {
        let mut candidate = row("offline", "runtime-offline", "authority-offline");
        candidate.availability = AssistantCandidateAvailability::Unavailable;
        candidate.unavailable_reason = Some("connection timed out".into());
        let result = rank_candidates(&req(), false, vec![candidate]).unwrap();
        assert_eq!(result.candidates.len(), 1);
        assert!(!result.candidates[0].eligible);
        assert_eq!(result.candidates[0].rank, None);
        assert_eq!(
            result.candidates[0].unavailable_reason.as_deref(),
            Some("connection timed out")
        );
    }

    #[test]
    fn exact_adoption_requires_the_exact_session_and_work_binding() {
        let mut candidate = row("peer", "runtime-a", "authority-a");
        candidate.executor = AssistantExecutorKind::LocalAcp;
        candidate.adapter = Some("cursor".into());
        candidate.adoptable_agent_session_id = Some("agent-exact".into());
        let mut request = req();
        request.requested_executor = Some(AssistantExecutorKind::LocalAcp);
        request.requested_adapter = Some("cursor".into());
        request.governed_work = Some(
            medousa_types::assistant_placement::AssistantGovernedWorkBinding {
                workshop_authority_id: "authority-a".into(),
                forge_work_id: "work-1".into(),
            },
        );
        request.requested_adoptable_agent_session_id = Some("agent-exact".into());
        let result = rank_candidates(&request, true, vec![candidate]).unwrap();
        assert!(result.candidates[0].eligible);
        assert_eq!(
            result.candidates[0].adoptable_agent_session_id.as_deref(),
            Some("agent-exact")
        );
    }

    #[test]
    fn missing_authority_and_unconfirmed_capability_fail_closed() {
        let mut candidate = row("unknown", "runtime-a", "authority-a");
        candidate.workshop_authority_id = None;
        candidate.capabilities.clear();
        let mut request = req();
        request.required_capabilities.insert("coder.work".into());
        let result = rank_candidates(&request, false, vec![candidate]).unwrap();
        assert!(!result.candidates[0].eligible);
        assert!(
            result.candidates[0]
                .reasons
                .iter()
                .any(|reason| reason == "workshop_authority_unknown")
        );
        assert!(
            result.candidates[0]
                .reasons
                .iter()
                .any(|reason| reason.starts_with("required_capabilities_unconfirmed"))
        );
    }
}
