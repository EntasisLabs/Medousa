//! Native metadata publication with exact replay and bounded graph CAS retries.
use super::*;
use sha2::{Digest, Sha256};

pub(super) fn publish(
    store: &WorkGraphStore,
    domain: &UserDomainRef,
    reference: ResourceRef,
    locator: String,
    native_revision: String,
    resolution: ResourceResolution,
    actor: &str,
) -> Result<serde_json::Value> {
    let mutation = WorkGraphMutation::RecordResource {
        reference: reference.clone(),
        locator: Some(locator.clone()),
        native_revision: Some(native_revision.clone()),
        resolution,
    };
    let provenance = RecordProvenance {
        actor_id: actor.into(),
        source: RecordSource::SystemEvent,
        evidence: vec![],
    };
    for _ in 0..3 {
        let page = store.query(
            domain,
            WorkGraphQuery {
                anchor: Some(reference.clone()),
                ..Default::default()
            },
        )?;
        let existing = match page.items.first() {
            Some(WorkGraphItem::Resource(r)) => Some(r),
            _ => None,
        };
        let unchanged = existing.is_some_and(|r| {
            r.locator.as_deref() == Some(&locator)
                && r.native_revision.as_deref() == Some(&native_revision)
                && r.resolution == resolution
                && r.provenance == provenance
        });
        let expected_revision = if unchanged {
            existing.unwrap().revision.saturating_sub(1)
        } else {
            page.revision
        };
        // Include CAS in the key: the same physical facts may recur after a
        // different observation or an unresolved model claim.
        let command = WorkGraphCommand {
            command_id: format!(
                "native-observe:{:x}",
                Sha256::digest(serde_json::to_vec(&(
                    domain,
                    &mutation,
                    &provenance,
                    expected_revision
                ))?)
            ),
            expected_revision,
            mutation: mutation.clone(),
        };
        match store.apply(domain, command, provenance.clone()) {
            Ok(receipt) => {
                let current = store.query(
                    domain,
                    WorkGraphQuery {
                        anchor: Some(reference.clone()),
                        ..Default::default()
                    },
                )?;
                let resource = match current.items.first() {
                    Some(WorkGraphItem::Resource(r)) => r,
                    _ => bail!("native graph observation is missing"),
                };
                return bounded_response(
                    serde_json::json!({"resource":resource,"graph_revision":current.revision,"receipt":receipt}),
                );
            }
            Err(error) if error.kind == medousa_store::PersistenceErrorKind::Conflict => continue,
            Err(error) => return Err(error.into()),
        }
    }
    bail!("native graph observation conflicted with concurrent changes; retry resolution")
}
