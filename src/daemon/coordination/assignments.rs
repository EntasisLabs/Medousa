//! Owner-scoped ledger inspection through the existing runtime query tool.

use super::{LocalPeerDispatcher, actor};
use crate::request_principal::RequestPrincipal;
use anyhow::{Result, bail};
use medousa_forge::execution::{ExecutionClass, MAX_STORE_PAYLOAD_BYTES};
use medousa_types::assistant_assignment::*;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssignmentListQuery {
    pub kind: Option<AssistantAssignmentKind>,
    pub terminal: Option<bool>,
    /// Page size, 1–100 (default 20).
    pub limit: Option<usize>,
    pub after_assignment_id: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssignmentGetQuery {
    pub assignment_id: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssignmentEventsQuery {
    pub assignment_id: String,
    pub limit: Option<usize>,
    /// Opaque next_cursor from the previous event page.
    pub cursor: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OwnerEventsQuery {
    /// Page size, 1–100 (default 20).
    pub limit: Option<usize>,
    /// Opaque next_cursor from the previous owner event page.
    pub cursor: Option<String>,
}

fn require_visible(
    value: &AssistantAssignment,
    principal: &str,
    visible: impl FnOnce(&str, &str) -> bool,
) -> Result<()> {
    require_visible_source(&value.principal_id, &value.source, principal, visible)
}

fn require_visible_source(
    owner: &str,
    source: &AssistantAssignmentSource,
    principal: &str,
    visible: impl FnOnce(&str, &str) -> bool,
) -> Result<()> {
    if owner != principal
        || source.session.as_ref().is_none_or(|session| {
            session.authority_id != source.authority_id
                || !visible(session.session_id.as_str(), principal)
        })
    {
        bail!("assignment is not visible to the authenticated owner");
    }
    Ok(())
}

fn page_size(limit: Option<usize>) -> Result<usize> {
    let limit = limit.unwrap_or(20);
    if !(1..=100).contains(&limit) {
        bail!("assignment page size must be between 1 and 100");
    }
    Ok(limit)
}

fn bounded_response(value: serde_json::Value) -> Result<serde_json::Value> {
    if serde_json::to_vec(&value)?.len() > MAX_STORE_PAYLOAD_BYTES {
        bail!("ownership query exceeds the response byte budget; use a smaller page");
    }
    Ok(value)
}

impl LocalPeerDispatcher {
    pub async fn owner_events(
        &self,
        principal: &RequestPrincipal,
        query: OwnerEventsQuery,
    ) -> Result<serde_json::Value> {
        let principal = actor(principal)?;
        let limit = page_size(query.limit)?;
        let authority = crate::workshop_authority::current()
            .map_err(anyhow::Error::msg)?
            .clone();
        self.with_assignment_store(move |store| {
            let mut events = store.list_owner_events(
                &authority,
                &principal,
                limit + 1,
                query.cursor.as_deref(),
            )?;
            let has_more = events.len() > limit;
            events.truncate(limit);
            // Cursor follows the scanned page so revoked sessions cannot trap
            // pagination. Visibility is checked again at the query boundary.
            let cursor = if has_more {
                events.last().map(|view| {
                    medousa_acp_client::coordination::store::owner_inbox::owner_event_cursor_key(
                        &view.event,
                    )
                })
            } else {
                None
            };
            events.retain(|view| {
                crate::session_catalog::session_visible_to_profile(
                    view.event.owner_session.session_id.as_str(),
                    &principal,
                )
            });
            bounded_response(serde_json::json!({
                "coverage": { "kind": "current_workshop", "authority_id": authority,
                    "complete_mesh": false },
                "events": events, "next_cursor": cursor,
            }))
        })
        .await
    }

    pub(crate) async fn with_assignment_store<T: Send + 'static>(
        &self,
        work: impl FnOnce(&medousa_acp_client::coordination::store::CoordinationStore) -> Result<T>
        + Send
        + 'static,
    ) -> Result<T> {
        let store = self.store.clone();
        self.state
            .forge_execution
            .run(
                ExecutionClass::StoreIo,
                MAX_STORE_PAYLOAD_BYTES,
                move || Ok(work(&store)),
            )
            .await?
    }

    pub async fn list_assignments(
        &self,
        principal: &RequestPrincipal,
        query: AssignmentListQuery,
    ) -> Result<serde_json::Value> {
        let principal = actor(principal)?;
        let limit = page_size(query.limit)?;
        let authority = crate::workshop_authority::current()
            .map_err(anyhow::Error::msg)?
            .clone();
        self.with_assignment_store(move |store| {
            let mut page = store.list_assistant_assignments(&authority, &AssistantAssignmentListFilter {
                principal_id: Some(principal.clone()),
                kind: query.kind,
                terminal: query.terminal,
                ..Default::default()
            }, limit, query.after_assignment_id.as_deref())?;
            page.assignments.retain(|view| require_visible(&view.assignment, &principal,
                crate::session_catalog::session_visible_to_profile).is_ok());
            bounded_response(serde_json::json!({
                "coverage": { "kind": "current_workshop", "authority_id": authority,
                    "complete_mesh": false, "source": "assistant_assignment_ledger" },
                "assignments": page.assignments,
                "next_cursor": page.next_cursor,
                "policy": "Native executors remain authoritative. A terminal execution does not prove review or verification."
            }))
        }).await
    }

    pub async fn get_assignment(
        &self,
        principal: &RequestPrincipal,
        query: AssignmentGetQuery,
    ) -> Result<serde_json::Value> {
        let principal = actor(principal)?;
        let authority = crate::workshop_authority::current()
            .map_err(anyhow::Error::msg)?
            .clone();
        self.with_assignment_store(move |store| {
            let view = store.assistant_assignment(&authority, &query.assignment_id)?;
            require_visible(
                &view.assignment,
                &principal,
                crate::session_catalog::session_visible_to_profile,
            )?;
            bounded_response(serde_json::to_value(view)?)
        })
        .await
    }

    pub async fn assignment_events(
        &self,
        principal: &RequestPrincipal,
        query: AssignmentEventsQuery,
    ) -> Result<serde_json::Value> {
        let principal = actor(principal)?;
        let limit = page_size(query.limit)?;
        let after = query
            .cursor
            .as_deref()
            .map(serde_json::from_str::<(u64, String)>)
            .transpose()?;
        let authority = crate::workshop_authority::current()
            .map_err(anyhow::Error::msg)?
            .clone();
        self.with_assignment_store(move |store| {
            let view = store.assistant_assignment(&authority, &query.assignment_id)?;
            require_visible(&view.assignment, &principal, crate::session_catalog::session_visible_to_profile)?;
            let mut events = store.assistant_assignment_events_after(&authority, &query.assignment_id,
                limit + 1, after.as_ref().map(|(seq, id)| (*seq, id.as_str())))?;
            let has_more = events.len() > limit;
            events.truncate(limit);
            let cursor = if has_more {
                events.last().map(|event| serde_json::to_string(&(event.native_sequence, &event.event_id))).transpose()?
            } else { None };
            bounded_response(serde_json::json!({"assignment_id": query.assignment_id, "events": events, "next_cursor": cursor}))
        }).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use medousa_types::{AuthorityId, SessionId, SessionRef};

    #[test]
    fn assignment_inspection_rechecks_owner_authority_and_source_visibility() {
        let authority = AuthorityId::parse(format!("auth_{}", "a".repeat(64))).unwrap();
        let mut source = AssistantAssignmentSource {
            authority_id: authority.clone(),
            session: Some(SessionRef {
                authority_id: authority,
                session_id: SessionId::parse("owner-chat").unwrap(),
            }),
            coordination_channel_id: None,
        };
        assert!(
            require_visible_source("alice", &source, "alice", |session, actor| {
                session == "owner-chat" && actor == "alice"
            })
            .is_ok()
        );
        assert!(require_visible_source("alice", &source, "bob", |_, _| true).is_err());
        assert!(require_visible_source("alice", &source, "alice", |_, _| false).is_err());
        source.session.as_mut().unwrap().authority_id =
            AuthorityId::parse(format!("auth_{}", "b".repeat(64))).unwrap();
        assert!(require_visible_source("alice", &source, "alice", |_, _| true).is_err());
        source.session = None;
        assert!(require_visible_source("alice", &source, "alice", |_, _| true).is_err());
    }
}
