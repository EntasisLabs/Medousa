//! Create-only work registrations, stage indexes and exact review evidence.
//! Native assignment claims fence launches; a lease serializes coordinators.
use super::*;
use fs2::FileExt;
use medousa_types::{AuthorityId, coordination::PeerAssignmentOutcome, work_coordination::*};

pub struct WorkCoordinationLease {
    _file: std::fs::File,
}

#[cfg(test)]
mod tests;

/// Included in approved reviewer instructions before registration. This is a
/// narrow data contract, not authority to issue grants or follow-up work.
pub const WORK_REVIEW_CONTRACT: &str = "medousa-work-review-v1";
pub const WORK_FIX_CONTRACT: &str = "medousa-work-fix-v1";
pub const MAX_FIX_REVIEW_ROUNDS: usize = 3;

impl CoordinationStore {
    fn work_domain_scope(
        domain: &medousa_types::work_unit::UserDomainRef,
    ) -> CoordinationChannelRef {
        CoordinationChannelRef {
            authority_id: domain.authority_id.clone(),
            channel_id: format!("work:{}", domain.user_id),
        }
    }

    pub fn work_is_controlled(
        &self,
        domain: &medousa_types::work_unit::UserDomainRef,
        work_unit_id: &str,
    ) -> Result<bool> {
        Ok(self.work_optional::<bool>(&object_path(
            &Self::work_domain_scope(domain),
            "work-controlled",
            work_unit_id,
        )?)? == Some(true))
    }

    fn work_create<T: Serialize + DeserializeOwned + PartialEq>(
        &self,
        path: &StorePath,
        value: &T,
    ) -> Result<bool> {
        let created = self.create(path, value)?;
        self.root.sync_parent_of(path)?;
        Ok(created)
    }
    pub(super) fn work_assignment_lock(
        &self,
        channel: &CoordinationChannelRef,
        id: &str,
    ) -> Result<std::fs::File> {
        let file = self
            .root
            .open_lock_file(&object_path(channel, "work-assignment-lock", id)?)
            .map_err(|error| {
                if error.is_not_found() {
                    anyhow::Error::new(medousa_store::PersistenceError::new(
                        medousa_store::PersistenceErrorKind::RetryableIo,
                        format!("assignment lock creation raced: {error}"),
                    ))
                } else {
                    error.into()
                }
            })?;
        file.try_lock_exclusive().map_err(|error| {
            if error.kind() == std::io::ErrorKind::WouldBlock {
                anyhow::Error::new(medousa_store::PersistenceError::new(
                    medousa_store::PersistenceErrorKind::Overloaded,
                    "native assignment custody busy; retry this exact command",
                ))
            } else {
                error.into()
            }
        })?;
        Ok(file)
    }
    fn work_optional<T: DeserializeOwned>(&self, path: &StorePath) -> Result<Option<T>> {
        match self.read(path) {
            Ok(value) => Ok(Some(value)),
            Err(error)
                if error
                    .downcast_ref::<medousa_store::StoreRootError>()
                    .is_some_and(|e| e.is_not_found()) =>
            {
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    pub fn work_plan(
        &self,
        channel: &CoordinationChannelRef,
        id: &str,
    ) -> Result<WorkCoordinationPlan> {
        let plan: WorkCoordinationPlan = self.read(&object_path(channel, "work-plan", id)?)?;
        if plan.input.channel != *channel
            || plan.input.coordination_id != id
            || plan.domain.authority_id != channel.authority_id
            || plan.round_index != 0
            || plan.input.fix_review_rounds.len() > MAX_FIX_REVIEW_ROUNDS
            || plan.fix_review_assignments.len() != plan.input.fix_review_rounds.len()
        {
            bail!("work coordination identity mismatch");
        }
        self.require_owner(channel, &plan.domain.user_id)?;
        Ok(plan)
    }

    pub fn work_plan_for_assignment(
        &self,
        request: &ExternalPeerAssignmentRequest,
    ) -> Result<Option<WorkCoordinationPlan>> {
        let plan: Option<WorkCoordinationPlan> = self.work_optional(&object_path(
            &request.channel,
            "work-stage",
            &request.assignment_id,
        )?)?;
        let Some(root) = plan else {
            return Ok(None);
        };
        // Stage indexes publish first; a torn root registration fences dispatch.
        if self.work_plan(&request.channel, &root.input.coordination_id)? != root {
            bail!("work registration is not complete");
        }
        let plan = (0..=root.input.fix_review_rounds.len())
            .filter_map(|i| root.round(i as u8))
            .find(|round| {
                round.executor_assignment_id == request.assignment_id
                    || round.reviewer_assignment_id == request.assignment_id
            })
            .ok_or_else(|| anyhow::anyhow!("work assignment is absent from the admitted rounds"))?;
        if plan.domain.user_id != request.owner_principal_id
            || plan.input.channel != request.channel
            || plan.forge_work_id != request.forge_work_id
        {
            bail!("work stage does not match native request");
        }
        Ok(Some(plan))
    }

    /// Call while holding native work and graph admission custody. This does
    /// not authorize execution; every dispatch still requires its exact grant.
    pub fn register_work_plan(&self, plan: &WorkCoordinationPlan) -> Result<bool> {
        let input = &plan.input;
        let owner = &plan.domain.user_id;
        for id in [&input.coordination_id, &input.work_unit_id] {
            if id.is_empty()
                || id.len() > 256
                || id.trim() != id
                || id.chars().any(char::is_control)
            {
                bail!("invalid work coordination identity");
            }
        }
        self.require_owner(&input.channel, owner)?;
        if plan.domain.authority_id != input.channel.authority_id
            || input.expected_scope_revision == 0
        {
            bail!("work registration requires exact local scope");
        }
        let path = object_path(&input.channel, "work-plan", &input.coordination_id)?;
        if let Some(existing) = self.work_optional::<WorkCoordinationPlan>(&path)? {
            if existing != *plan {
                bail!("work coordination identity is immutable");
            }
            return self.work_create(&path, plan);
        }
        if plan.round_index != 0
            || input.fix_review_rounds.len() > MAX_FIX_REVIEW_ROUNDS
            || plan.fix_review_assignments.len() != input.fix_review_rounds.len()
        {
            bail!("invalid bounded fix/review root plan");
        }
        let rounds: Vec<_> = (0..=input.fix_review_rounds.len())
            .map(|i| plan.round(i as u8).unwrap())
            .collect();
        let mut stage_ids: Vec<_> = rounds
            .iter()
            .flat_map(|round| [&round.executor_assignment_id, &round.reviewer_assignment_id])
            .collect();
        stage_ids.sort();
        if stage_ids.windows(2).any(|ids| ids[0] == ids[1]) {
            bail!("each round requires distinct native assignments");
        }
        let mut _stage_locks = Vec::new();
        for id in &stage_ids {
            _stage_locks.push(self.work_assignment_lock(&input.channel, id)?);
        }
        if let Some(existing) = self.work_optional::<WorkCoordinationPlan>(&path)? {
            if existing != *plan {
                bail!("conflicting work registration");
            }
            return self.work_create(&path, plan);
        }
        let now = chrono::Utc::now();
        let mut sessions = std::collections::BTreeSet::new();
        let root_target = self
            .proposal(&input.channel, &input.executor_proposal_id)?
            .request
            .target;
        for round in &rounds {
            let executor = self.proposal(&input.channel, &round.input.executor_proposal_id)?;
            let reviewer = self.proposal(&input.channel, &round.input.reviewer_proposal_id)?;
            if input.deadline <= now
                || input.deadline > now + chrono::Duration::hours(24)
                || input.deadline > executor.expires_at
                || input.deadline > reviewer.expires_at
            {
                bail!("work coordination deadline exceeds current approval bounds");
            }
            if executor.request.assignment_id != round.executor_assignment_id
                || reviewer.request.assignment_id != round.reviewer_assignment_id
                || !reviewer.request.instructions.contains(WORK_REVIEW_CONTRACT)
                || (round.round_index > 0
                    && !executor.request.instructions.contains(WORK_FIX_CONTRACT))
            {
                bail!(
                    "work rounds require exact assignments and approved review/fix data contracts"
                );
            }
            for proposal in [&executor, &reviewer] {
                let request = &proposal.request;
                if proposal.continue_owner
                    || request.owner_principal_id != *owner
                    || request.forge_work_id != plan.forge_work_id
                    || request.target.authority_id != root_target.authority_id
                    || request.target.execution_runtime_id != root_target.execution_runtime_id
                    || !sessions.insert((
                        request.execution_session.authority_id.to_string(),
                        request.execution_session.session_id.to_string(),
                    ))
                    || request.existing_agent_session_id.is_some()
                {
                    bail!(
                        "work stages require fresh distinct sessions, one local runtime/Forge work and no chat continuation"
                    );
                }
                if let Some(decision) = self.proposal_decision(proposal)? {
                    if !decision.approved {
                        bail!("declined proposal cannot be registered for work");
                    }
                    self.require_assignment_grant(request, now)?;
                }
                if self
                    .work_optional::<ExternalPeerAssignmentRequest>(&object_path(
                        &request.channel,
                        "assignment",
                        &request.assignment_id,
                    )?)?
                    .is_some()
                {
                    bail!("register work before native dispatch claims");
                }
            }
        }
        if self.root.list_root_utf8()?.len() > 32768 - 32 * rounds.len() {
            bail!(
                "work registration has no recovery/publication headroom; retained records were not evicted"
            );
        }
        // Each stage and each work generation can belong to only one plan.
        // Publish indexes first: a torn registration blocks standalone dispatch.
        let unit_scope = Self::work_domain_scope(&plan.domain);
        self.create(
            &object_path(&unit_scope, "work-controlled", &input.work_unit_id)?,
            &true,
        )?;
        self.create(
            &object_path(
                &unit_scope,
                "work-generation",
                &format!("{}:{}", input.work_unit_id, input.expected_scope_revision),
            )?,
            plan,
        )?;
        for id in stage_ids {
            self.create(&object_path(&input.channel, "work-stage", id)?, plan)?;
        }
        self.work_create(&path, plan)
    }

    pub fn try_work_coordination_lease(
        &self,
        plan: &WorkCoordinationPlan,
    ) -> Result<Option<WorkCoordinationLease>> {
        self.require_work_round(plan)?;
        let file = self.root.open_lock_file(&object_path(
            &plan.input.channel,
            "work-lock",
            &plan.input.coordination_id,
        )?)?;
        match file.try_lock_exclusive() {
            Ok(()) => Ok(Some(WorkCoordinationLease { _file: file })),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn work_round_path(plan: &WorkCoordinationPlan, kind: &str) -> Result<StorePath> {
        if plan.round_index == 0 {
            object_path(&plan.input.channel, kind, &plan.input.coordination_id)
        } else {
            object_path(
                &plan.input.channel,
                &format!("{kind}-round"),
                &serde_json::to_string(&(&plan.input.coordination_id, plan.round_index))?,
            )
        }
    }

    pub fn require_work_round(&self, plan: &WorkCoordinationPlan) -> Result<()> {
        let root = self.work_plan(&plan.input.channel, &plan.input.coordination_id)?;
        if root.round(plan.round_index).as_ref() != Some(plan) {
            bail!("work round differs from immutable root intent");
        }
        Ok(())
    }

    pub fn work_round_predecessor(
        &self,
        plan: &WorkCoordinationPlan,
    ) -> Result<Option<WorkCoordinationResult>> {
        self.require_work_round(plan)?;
        if plan.round_index == 0 {
            return Ok(None);
        }
        let root = self.work_plan(&plan.input.channel, &plan.input.coordination_id)?;
        let previous = root.round(plan.round_index - 1).unwrap();
        let result = self
            .work_coordination_result(&previous)?
            .filter(|result| result.outcome == WorkCoordinationOutcome::ChangesRequested)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "fix round requires the exact predecessor changes_requested receipt"
                )
            })?;
        Ok(Some(result))
    }

    pub fn work_review_input(
        &self,
        plan: &WorkCoordinationPlan,
    ) -> Result<Option<WorkReviewInput>> {
        let input: Option<WorkReviewInput> =
            self.work_optional(&Self::work_round_path(plan, "work-review-input")?)?;
        if let Some(input) = &input {
            self.validate_work_review_input(plan, input)?;
        }
        Ok(input)
    }

    fn validate_work_review_input(
        &self,
        plan: &WorkCoordinationPlan,
        input: &WorkReviewInput,
    ) -> Result<()> {
        self.require_work_round(plan)?;
        let receipt = self.receipt(&plan.input.channel, &plan.executor_assignment_id)?;
        if receipt.outcome != PeerAssignmentOutcome::Completed
            || input.executor_receipt_id != receipt.receipt_id
            || input.executor_assignment_id != plan.executor_assignment_id
            || input.forge_work_id != plan.forge_work_id
            || input.coordination_id != plan.input.coordination_id
            || input.work_unit_id != plan.input.work_unit_id
            || input.branch.is_empty()
            || !matches!(input.head_oid.len(), 40 | 64)
            || !input.head_oid.bytes().all(|c| c.is_ascii_hexdigit())
        {
            bail!("review input must pin native executor completion and a committed checkout");
        }
        Ok(())
    }

    pub fn record_work_review_input(
        &self,
        plan: &WorkCoordinationPlan,
        input: &WorkReviewInput,
    ) -> Result<bool> {
        self.validate_work_review_input(plan, input)?;
        self.work_create(&Self::work_round_path(plan, "work-review-input")?, input)
    }

    pub fn work_coordination_result(
        &self,
        plan: &WorkCoordinationPlan,
    ) -> Result<Option<WorkCoordinationResult>> {
        let result: Option<WorkCoordinationResult> =
            self.work_optional(&Self::work_round_path(plan, "work-result")?)?;
        if let Some(result) = &result {
            self.validate_work_result(plan, result)?;
        }
        Ok(result)
    }

    fn validate_work_result(
        &self,
        plan: &WorkCoordinationPlan,
        result: &WorkCoordinationResult,
    ) -> Result<()> {
        self.require_work_round(plan)?;
        if result.coordination_id != plan.input.coordination_id {
            bail!("work result identity mismatch");
        }
        match result.outcome {
            WorkCoordinationOutcome::Approved | WorkCoordinationOutcome::ChangesRequested => {
                let input = self
                    .work_review_input(plan)?
                    .ok_or_else(|| anyhow::anyhow!("review input missing"))?;
                let receipt = self.receipt(&plan.input.channel, &plan.reviewer_assignment_id)?;
                let decision: WorkReviewDecision = serde_json::from_str(&receipt.result)?;
                let expected = if result.outcome == WorkCoordinationOutcome::Approved {
                    WorkReviewVerdict::Approved
                } else {
                    WorkReviewVerdict::ChangesRequested
                };
                if receipt.outcome != PeerAssignmentOutcome::Completed
                    || result.receipt_id.as_deref() != Some(&receipt.receipt_id)
                    || receipt.result.len() > 8192
                    || decision.reviewed != input
                    || decision.verdict != expected
                    || decision.summary.trim().is_empty()
                    || decision.summary.len() > 4096
                    || decision.summary.contains('\0')
                    || result.decision.as_ref() != Some(&decision)
                {
                    bail!("review verdict is not backed by the exact native receipt");
                }
            }
            WorkCoordinationOutcome::ExecutorFailed
            | WorkCoordinationOutcome::ReviewerFailed
            | WorkCoordinationOutcome::InvalidReview => {
                let id = if result.outcome == WorkCoordinationOutcome::ExecutorFailed {
                    &plan.executor_assignment_id
                } else {
                    &plan.reviewer_assignment_id
                };
                let receipt = self.receipt(&plan.input.channel, id)?;
                if result.receipt_id.as_deref() != Some(&receipt.receipt_id)
                    || result.decision.is_some()
                    || (result.outcome != WorkCoordinationOutcome::InvalidReview
                        && receipt.outcome == PeerAssignmentOutcome::Completed)
                {
                    bail!("work failure is not backed by the native receipt");
                }
            }
            WorkCoordinationOutcome::DeadlineExceeded => {
                if chrono::Utc::now() < plan.input.deadline
                    || result.decision.is_some()
                    || result.receipt_id.is_some()
                {
                    bail!("work deadline has not elapsed");
                }
            }
            WorkCoordinationOutcome::RevisionChanged => {
                if result.decision.is_some() || result.receipt_id.is_some() {
                    bail!("invalid revision observation");
                }
            }
        }
        Ok(())
    }

    pub fn record_work_coordination_result(
        &self,
        plan: &WorkCoordinationPlan,
        result: &WorkCoordinationResult,
    ) -> Result<bool> {
        self.validate_work_result(plan, result)?;
        self.work_create(&Self::work_round_path(plan, "work-result")?, result)
    }

    pub fn record_work_projection(&self, plan: &WorkCoordinationPlan) -> Result<bool> {
        self.work_create(
            &object_path(
                &plan.input.channel,
                "work-projected",
                &plan.input.coordination_id,
            )?,
            &true,
        )
    }

    /// Deterministic, bounded recovery; poisoned records do not starve later
    /// plans. Cursor rotation is the host's responsibility.
    pub fn local_work_plans(
        &self,
        authority: &AuthorityId,
        runtime_id: &str,
        limit: usize,
        after: Option<&str>,
    ) -> Result<Vec<WorkCoordinationPlan>> {
        if !(1..=32).contains(&limit) {
            bail!("invalid work recovery page");
        }
        let entries = self.root.list_root_utf8()?;
        if entries.len() > 32768 {
            bail!("work recovery scan budget exhausted");
        }
        let mut plans = std::collections::BTreeMap::new();
        for entry in entries {
            if !entry.name.starts_with("wp1-") || after.is_some_and(|a| entry.name.as_str() <= a) {
                continue;
            }
            let read = (|| -> Result<WorkCoordinationPlan> {
                let plan: WorkCoordinationPlan = self.read(&StorePath::parse(&entry.name)?)?;
                if object_path(
                    &plan.input.channel,
                    "work-plan",
                    &plan.input.coordination_id,
                )?
                .to_string()
                    != entry.name
                {
                    bail!("work recovery identity mismatch");
                }
                self.work_plan(&plan.input.channel, &plan.input.coordination_id)
            })();
            let plan = match read {
                Ok(plan) => plan,
                Err(error) => {
                    tracing::warn!(%error, record = %entry.name, "invalid work registration retained");
                    continue;
                }
            };
            if &plan.domain.authority_id != authority {
                continue;
            }
            let eligible = (|| -> Result<bool> {
                if self.work_optional::<bool>(&object_path(
                    &plan.input.channel,
                    "work-projected",
                    &plan.input.coordination_id,
                )?)? == Some(true)
                {
                    return Ok(false);
                }
                let executor =
                    self.proposal(&plan.input.channel, &plan.input.executor_proposal_id)?;
                let reviewer =
                    self.proposal(&plan.input.channel, &plan.input.reviewer_proposal_id)?;
                Ok(executor.request.target.execution_runtime_id == runtime_id
                    && reviewer.request.target.execution_runtime_id == runtime_id)
            })();
            match eligible {
                Ok(true) => {}
                Ok(false) => continue,
                Err(error) => {
                    tracing::warn!(%error, record = %entry.name, "invalid work stage retained; recovery continues");
                    continue;
                }
            }
            plans.insert(entry.name, plan);
            if plans.len() > limit {
                plans.pop_last();
            }
        }
        let page: Vec<_> = plans.into_values().collect();
        if serde_json::to_vec(&page)?.len() > 1024 * 1024 {
            bail!("work recovery byte budget exceeded");
        }
        Ok(page)
    }

    pub fn work_plan_cursor(plan: &WorkCoordinationPlan) -> Result<String> {
        Ok(object_path(
            &plan.input.channel,
            "work-plan",
            &plan.input.coordination_id,
        )?
        .to_string())
    }

    pub fn work_stage_claimed(&self, plan: &WorkCoordinationPlan, executor: bool) -> Result<bool> {
        let id = if executor {
            &plan.executor_assignment_id
        } else {
            &plan.reviewer_assignment_id
        };
        Ok(self
            .work_optional::<ExternalPeerAssignmentRequest>(&object_path(
                &plan.input.channel,
                "assignment",
                id,
            )?)?
            .is_some())
    }

    pub fn retain_work_terminal_for_controller(
        &self,
        receipt: &medousa_types::coordination::ExternalPeerAssignmentReceipt,
    ) -> Result<bool> {
        let request = self.assignment(&receipt.binding.channel, &receipt.binding.assignment_id)?;
        if self.work_plan_for_assignment(&request)?.is_none() {
            return Ok(false);
        }
        let event = self.peer_owner_event(receipt)?;
        self.block_owner_event(&event.channel, &medousa_types::coordination::OwnerEventBlocked {
            event_id: event.event_id, reason: "work-scoped controller owns this terminal; owner-chat continuation is not admitted".into(),
            blocked_at: chrono::Utc::now(), requires_user_decision: false,
        })?;
        Ok(true)
    }
}
