//! Aggregate accounting is a custody ledger, not an executor. Native adapters
//! must reserve before dispatch and settle only against their own durable truth.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};

use super::*;

impl Snapshot {
    pub(super) fn budget_usage(&self, id: &str) -> WorkBudgetUsage {
        let mut usage = WorkBudgetUsage::default();
        for reservation in self
            .reservations
            .values()
            .filter(|r| r.charged_units.iter().any(|u| u == id))
        {
            if reservation.disposition == Some(WorkBudgetDisposition::NotStarted) {
                continue;
            }
            let cost = reservation
                .actual_cost_microusd
                .unwrap_or(reservation.reserved_cost_microusd);
            match usage.cost_microusd.checked_add(cost) {
                Some(cost) => usage.cost_microusd = cost,
                None => {
                    usage.cost_microusd = u64::MAX;
                    usage.cost_overflowed = true;
                }
            }
            // The ledger's 4096-record bound fits both counters.
            usage.execution_count += 1;
            if reservation.disposition.is_none() {
                usage.concurrent_executions += 1;
            }
        }
        usage
    }

    pub(super) fn check_budget_limits(
        &self,
        limits: &WorkBudgetLimits,
        usage: &WorkBudgetUsage,
    ) -> Result<()> {
        if usage.cost_overflowed
            || usage.cost_microusd > limits.cost_microusd
            || usage.execution_count > limits.execution_count
            || usage.concurrent_executions > limits.concurrent_executions
        {
            return Err(error(
                PersistenceErrorKind::Conflict,
                "aggregate budget exhausted or below already admitted usage",
            ));
        }
        Ok(())
    }

    // Only composition children carry execution charges. A dependency consumes
    // an existing result; merely referring to it does not make the parent pay.
    fn charged_ancestors(&self, id: &str) -> BTreeSet<String> {
        let mut parents: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for (parent, unit) in &self.work_units {
            if unit.state.is_terminal() {
                continue;
            }
            for child in &unit.scope.children {
                parents.entry(child).or_default().push(parent);
            }
        }
        let mut visited = BTreeSet::new();
        let mut pending = vec![id];
        while let Some(unit) = pending.pop() {
            if visited.insert(unit.to_string()) {
                pending.extend(parents.get(unit).into_iter().flatten().copied());
            }
        }
        visited
    }

    /// Charge prior child allocations to new aggregates. Charges stay after a
    /// later scope removal, so reparenting cannot erase spending or custody.
    pub(super) fn extend_budget_charges(&mut self, id: &str) -> Result<()> {
        let ancestors = self.charged_ancestors(id);
        let mut descendants = BTreeSet::new();
        let mut pending = vec![id];
        while let Some(child) = pending.pop() {
            if descendants.insert(child.to_string()) {
                pending.extend(
                    self.work_units[child]
                        .scope
                        .children
                        .iter()
                        .map(String::as_str),
                );
            }
        }
        let mut added = BTreeSet::new();
        for reservation in self.reservations.values_mut() {
            if !descendants.contains(&reservation.work_unit_id) {
                continue;
            }
            for ancestor in &ancestors {
                if !reservation.charged_units.contains(ancestor) {
                    reservation.charged_units.push(ancestor.clone());
                    reservation.revision = self.revision;
                    added.insert(ancestor.clone());
                }
            }
            reservation.charged_units.sort();
        }
        for id in added {
            let limits = self.work_units[&id].budget.as_ref().ok_or_else(|| {
                invalid(
                    "new aggregate requires explicit limits before attaching allocated children",
                )
            })?;
            self.check_budget_limits(limits, &self.budget_usage(&id))?;
        }
        Ok(())
    }

    pub(super) fn reserve_budget(
        &mut self,
        reservation_id: String,
        work_unit_id: String,
        execution: ResourceRef,
        reserved_cost_microusd: u64,
        provenance: RecordProvenance,
        now: DateTime<Utc>,
    ) -> Result<()> {
        identifier(&reservation_id)?;
        if provenance.source != RecordSource::SystemEvent {
            return Err(invalid("budget custody requires system provenance"));
        }
        if self.reservations.contains_key(&reservation_id)
            || self.reservations.values().any(|r| r.execution == execution)
        {
            return Err(error(
                PersistenceErrorKind::Conflict,
                "execution or reservation already has durable budget custody",
            ));
        }
        let unit = self
            .work_units
            .get(&work_unit_id)
            .ok_or_else(|| invalid("unknown work unit"))?;
        if unit.state != WorkUnitState::Active {
            return Err(invalid("budget reservation requires active work"));
        }
        self.require_resource(&execution, true)?;
        let resource = &self.resources[&reference_key(&execution)?];
        if execution.authority_id != self.domain.authority_id
            || !matches!(execution.kind, ResourceKind::Assignment | ResourceKind::Job)
            || resource.resolution != ResourceResolution::Available
            || resource.provenance.source != RecordSource::SystemEvent
        {
            return Err(invalid(
                "reservation requires an adapter-owned local execution identity",
            ));
        }
        let charged_units: Vec<_> = self.charged_ancestors(&work_unit_id).into_iter().collect();
        self.reservations.insert(
            reservation_id.clone(),
            WorkBudgetReservation {
                reservation_id,
                work_unit_id,
                execution,
                reserved_cost_microusd,
                charged_units: charged_units.clone(),
                disposition: None,
                actual_cost_microusd: None,
                revision: self.revision,
                provenance,
            },
        );
        for id in charged_units {
            let unit = &self.work_units[&id];
            if matches!(
                unit.state,
                WorkUnitState::Paused | WorkUnitState::NeedsAttention
            ) {
                return Err(invalid("aggregate is paused or needs attention"));
            }
            let limits = unit
                .budget
                .as_ref()
                .ok_or_else(|| invalid("all charged work units require explicit budget limits"))?;
            if limits.deadline <= now {
                return Err(invalid("aggregate execution deadline elapsed"));
            }
            self.check_budget_limits(limits, &self.budget_usage(&id))?;
        }
        Ok(())
    }

    pub(super) fn settle_budget(
        &mut self,
        id: &str,
        disposition: WorkBudgetDisposition,
        actual_cost_microusd: u64,
        provenance: RecordProvenance,
    ) -> Result<()> {
        if provenance.source != RecordSource::SystemEvent {
            return Err(invalid("budget settlement requires system provenance"));
        }
        if disposition == WorkBudgetDisposition::NotStarted && actual_cost_microusd != 0 {
            return Err(invalid("not-started custody cannot have execution cost"));
        }
        let reservation = self
            .reservations
            .get_mut(id)
            .ok_or_else(|| invalid("unknown budget reservation"))?;
        if reservation.disposition.is_some() {
            return Err(error(
                PersistenceErrorKind::Conflict,
                "budget settlement is immutable; replay the exact command",
            ));
        }
        // Never discard native cost truth because an executor overran a hold.
        // Future admission uses the larger actual total and will fail closed.
        reservation.disposition = Some(disposition);
        reservation.actual_cost_microusd = Some(actual_cost_microusd);
        reservation.revision = self.revision;
        reservation.provenance = provenance;
        Ok(())
    }

    pub(super) fn validate_reservations(&self) -> Result<()> {
        let mut executions = BTreeSet::new();
        for (id, reservation) in &self.reservations {
            identifier(id)?;
            self.require_resource(&reservation.execution, false)?;
            if id != &reservation.reservation_id
                || reservation.revision == 0
                || reservation.revision > self.revision
                || !self.work_units.contains_key(&reservation.work_unit_id)
                || !reservation
                    .charged_units
                    .contains(&reservation.work_unit_id)
                || reservation.charged_units.len() > MAX_RECORDS
                || reservation.provenance.source != RecordSource::SystemEvent
                || reservation.execution.authority_id != self.domain.authority_id
                || !matches!(
                    reservation.execution.kind,
                    ResourceKind::Assignment | ResourceKind::Job
                )
                || !executions.insert(reference_key(&reservation.execution)?)
                || reservation.disposition.is_some() != reservation.actual_cost_microusd.is_some()
                || (reservation.disposition == Some(WorkBudgetDisposition::NotStarted)
                    && reservation.actual_cost_microusd != Some(0))
            {
                return Err(invalid("invalid durable budget custody record"));
            }
            for (i, unit) in reservation.charged_units.iter().enumerate() {
                if !self.work_units.contains_key(unit)
                    || reservation.charged_units[..i].contains(unit)
                {
                    return Err(invalid("invalid aggregate budget charges"));
                }
            }
        }
        Ok(())
    }
}
