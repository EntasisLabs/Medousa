//! Pull-based durable inboxes over the canonical retained journal. Reading does
//! not consume an event; an attributable acknowledgment commits with its cursor.
use super::*;

impl WorkGraphStore {
    pub fn events(
        &self,
        domain: &UserDomainRef,
        actor: &str,
        query: WorkEventsQuery,
    ) -> Result<WorkEventsPage> {
        identifier(actor)?;
        let snapshot = self.load(domain)?;
        let subscription = snapshot
            .subscriptions
            .get(&query.subscription_id)
            .filter(|subscription| subscription.recipient_actor_id == actor)
            .ok_or_else(|| invalid("subscription is not owned by this actor"))?;
        let limit = query.limit.unwrap_or(16).clamp(1, 32);
        let mut pending = snapshot.pending_events(subscription);
        let has_more = pending.len() > limit;
        pending.truncate(limit);
        Ok(WorkEventsPage {
            revision: snapshot.revision,
            subscription: subscription.clone(),
            status: snapshot.subscription_status(subscription),
            events: pending
                .into_iter()
                .map(|command| WorkGraphEvent {
                    command: command.command.clone(),
                    provenance: command.provenance.clone(),
                    receipt: command.receipt.clone(),
                })
                .collect(),
            has_more,
        })
    }
}

impl Snapshot {
    fn subscription_status(&self, subscription: &WorkEventSubscription) -> WorkSubscriptionStatus {
        let unit = &self.work_units[&subscription.input.work_unit_id];
        if subscription.stopped_at_revision.is_some() {
            WorkSubscriptionStatus::Stopped
        } else if subscription.input.expires_at <= Utc::now() {
            WorkSubscriptionStatus::Expired
        } else if unit.scope_revision != subscription.input.expected_scope_revision {
            WorkSubscriptionStatus::ScopeChanged
        } else if unit.state == WorkUnitState::Paused {
            WorkSubscriptionStatus::Paused
        } else if unit.state.is_terminal() {
            WorkSubscriptionStatus::Terminal
        } else {
            WorkSubscriptionStatus::Active
        }
    }

    fn pending_events(&self, subscription: &WorkEventSubscription) -> Vec<&CommittedCommand> {
        let mut events: Vec<_> = self
            .commands
            .values()
            .filter(|command| {
                command.receipt.revision > subscription.acknowledged_revision
                    && subscription
                        .stopped_at_revision
                        .is_none_or(|stop| command.receipt.revision < stop)
                    && command.receipt.committed_at < subscription.input.expires_at
                    && self.matches_subscription(subscription, command)
            })
            .collect();
        events.sort_by_key(|command| command.receipt.revision);
        events
    }

    fn matches_subscription(
        &self,
        subscription: &WorkEventSubscription,
        command: &CommittedCommand,
    ) -> bool {
        let input = &subscription.input;
        let kind = match &command.command.mutation {
            WorkGraphMutation::RecordResource { reference, .. }
                if command.provenance.source == RecordSource::SystemEvent
                    && input.resources.contains(reference) =>
            {
                WorkEventKind::ResourceObserved
            }
            WorkGraphMutation::SetState { work_unit_id, .. }
                if work_unit_id == &input.work_unit_id =>
            {
                WorkEventKind::WorkStateChanged
            }
            WorkGraphMutation::SetScope { work_unit_id, .. }
                if work_unit_id == &input.work_unit_id =>
            {
                WorkEventKind::WorkScopeChanged
            }
            WorkGraphMutation::RecordProviderEvent { event } => {
                let Some(record) = self.provider_requests.values().find(|record| {
                    record.request.conversation_id == event.conversation_id
                        && record.request.request_id == event.request_id
                }) else {
                    return false;
                };
                if record.request.input.work_unit_id != input.work_unit_id
                    || record.request.input.expected_scope_revision != input.expected_scope_revision
                {
                    return false;
                }
                match event.kind {
                    medousa_types::ExternalEventKind::Completed => WorkEventKind::ProviderCompleted,
                    medousa_types::ExternalEventKind::Failed => WorkEventKind::ProviderFailed,
                    medousa_types::ExternalEventKind::Progress
                    | medousa_types::ExternalEventKind::Question => {
                        if record.events.iter().any(|terminal| {
                            matches!(
                                terminal.kind,
                                medousa_types::ExternalEventKind::Completed
                                    | medousa_types::ExternalEventKind::Failed
                            ) && terminal.request_sequence < event.request_sequence
                        }) {
                            return false;
                        }
                        WorkEventKind::ProviderProgress
                    }
                    _ => return false,
                }
            }
            _ => return false,
        };
        input.event_kinds.contains(&kind)
    }

    pub(super) fn subscribe(
        &mut self,
        input: WorkSubscriptionInput,
        provenance: &RecordProvenance,
    ) -> Result<()> {
        identifier(&input.subscription_id)?;
        if self.subscriptions.contains_key(&input.subscription_id) {
            return Err(invalid(
                "subscription identity already registered; replay the original command",
            ));
        }
        let unit = self
            .work_units
            .get(&input.work_unit_id)
            .ok_or_else(|| invalid("unknown work unit"))?;
        if unit.scope_revision != input.expected_scope_revision || unit.state.is_terminal() {
            return Err(invalid(
                "subscription requires the exact nonterminal work scope",
            ));
        }
        let now = Utc::now();
        if input.after_revision >= self.revision
            || input.expires_at <= now
            || input.expires_at - now > chrono::Duration::days(30)
        {
            return Err(invalid(
                "subscription requires a retained cursor and expiry within 30 days",
            ));
        }
        self.validate_subscription_selection(&input, true)?;
        self.subscriptions.insert(
            input.subscription_id.clone(),
            WorkEventSubscription {
                acknowledged_revision: input.after_revision,
                input,
                recipient_actor_id: provenance.actor_id.clone(),
                stopped_at_revision: None,
                revision: self.revision,
            },
        );
        Ok(())
    }

    pub(super) fn subscription_for_actor(
        &mut self,
        id: &str,
        actor: &str,
    ) -> Result<&mut WorkEventSubscription> {
        self.subscriptions
            .get_mut(id)
            .filter(|subscription| subscription.recipient_actor_id == actor)
            .ok_or_else(|| invalid("subscription is not owned by this actor"))
    }

    pub(super) fn acknowledge_event(
        &mut self,
        id: &str,
        event_revision: u64,
        decision: &str,
        provenance: &RecordProvenance,
    ) -> Result<()> {
        text(decision, 4096)?;
        let subscription = self
            .subscriptions
            .get(id)
            .filter(|subscription| subscription.recipient_actor_id == provenance.actor_id)
            .ok_or_else(|| invalid("subscription is not owned by this actor"))?;
        // Only the next matched event can advance custody. Unrelated journal
        // entries are skipped, never another pending completion or observation.
        if self
            .pending_events(subscription)
            .first()
            .map(|command| command.receipt.revision)
            != Some(event_revision)
        {
            return Err(invalid("acknowledgment must name the next pending event"));
        }
        let revision = self.revision;
        let subscription = self.subscription_for_actor(id, &provenance.actor_id)?;
        subscription.acknowledged_revision = event_revision;
        subscription.revision = revision;
        Ok(())
    }

    fn validate_subscription_selection(
        &self,
        input: &WorkSubscriptionInput,
        current_scope: bool,
    ) -> Result<()> {
        identifier(&input.subscription_id)?;
        let unit = self
            .work_units
            .get(&input.work_unit_id)
            .ok_or_else(|| invalid("missing subscribed work"))?;
        if input.resources.len() > MAX_SCOPE_MEMBERS
            || input.event_kinds.is_empty()
            || input.event_kinds.len() > 6
        {
            return Err(invalid("subscription selection exceeds bounds"));
        }
        for (i, kind) in input.event_kinds.iter().enumerate() {
            if input.event_kinds[..i].contains(kind) {
                return Err(invalid("duplicate event kind"));
            }
        }
        if input.event_kinds.contains(&WorkEventKind::ResourceObserved)
            && input.resources.is_empty()
        {
            return Err(invalid("resource observations require exact selectors"));
        }
        for (i, reference) in input.resources.iter().enumerate() {
            self.require_resource(reference, false)?;
            if reference.authority_id != self.domain.authority_id
                || input.resources[..i].contains(reference)
                || (current_scope && !unit.scope.resources.contains(reference))
            {
                return Err(invalid(
                    "subscription resources must be unique local scope members",
                ));
            }
        }
        Ok(())
    }

    pub(super) fn validate_subscriptions(&self) -> Result<()> {
        for (id, subscription) in &self.subscriptions {
            identifier(&subscription.recipient_actor_id)?;
            self.validate_subscription_selection(&subscription.input, false)?;
            if id != &subscription.input.subscription_id
                || subscription.revision == 0
                || subscription.revision > self.revision
                || subscription.input.expected_scope_revision == 0
                || subscription.acknowledged_revision < subscription.input.after_revision
                || subscription.acknowledged_revision >= subscription.revision
            {
                return Err(invalid("invalid durable subscription cursor or revision"));
            }
        }
        Ok(())
    }
}
