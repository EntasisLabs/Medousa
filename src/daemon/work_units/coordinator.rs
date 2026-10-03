//! Admitted deterministic work intake. It never creates a model turn, changes
//! credentials, sends a provider request, or delivers a user notification.
use super::*;
use medousa_work::{COORDINATOR_ACTOR, CoordinatorInbox, CoordinatorInboxPage};
use sha2::{Digest, Sha256};

impl WorkUnitHost {
    pub(crate) async fn coordinator_inboxes(
        &self,
        after: Option<String>,
        limit: usize,
    ) -> Result<CoordinatorInboxPage> {
        let authority = crate::workshop_authority::current()
            .map_err(anyhow::Error::msg)?
            .clone();
        self.with_store(move |store| {
            Ok(store.coordinator_inboxes(&authority, limit, after.as_deref())?)
        })
        .await
    }

    pub(crate) fn consume_provider_inbox(&self, inbox: &CoordinatorInbox) -> Result<()> {
        let page = self.store.events(
            &inbox.domain,
            COORDINATOR_ACTOR,
            WorkEventsQuery {
                subscription_id: inbox.subscription_id.clone(),
                limit: Some(1),
            },
        )?;
        let Some(event) = page.events.first() else {
            return Ok(());
        };
        // Terminal decisions retain custody until their stage controller can
        // publish an attributable outcome. Progress is safe to observe once.
        let WorkGraphMutation::RecordProviderEvent { event: provider } = &event.command.mutation
        else {
            bail!("runtime work inbox contains an unsupported event");
        };
        if matches!(
            provider.kind,
            medousa_types::ExternalEventKind::Completed | medousa_types::ExternalEventKind::Failed
        ) {
            return Ok(());
        }
        self.store.apply_native_command(&inbox.domain,
            format!("work-intake-ack:{:x}", Sha256::digest(serde_json::to_vec(&(&inbox.subscription_id, event.receipt.revision))?)),
            WorkGraphMutation::AcknowledgeEvent {
                subscription_id: inbox.subscription_id.clone(), event_revision: event.receipt.revision,
                decision: "Runtime observed correlated provider progress; no execution or delivery effect admitted".into(),
            }, RecordProvenance { actor_id: COORDINATOR_ACTOR.into(), source: RecordSource::SystemEvent, evidence: vec![] })?;
        Ok(())
    }
}
