//! Admitted-turn access to durable intent, separate from native execution.

use std::{
    path::PathBuf,
    sync::{Arc, OnceLock},
};

use anyhow::{Result, bail};
use medousa_forge::execution::{ExecutionClass, ForgeExecutionService};
use medousa_types::work_unit::*;
use medousa_work::{MAX_SNAPSHOT_BYTES, WorkGraphStore};

use crate::{
    agent_runtime::execution_context::TurnExecutionContext,
    request_principal::{Capability, PrincipalKind, RequestPrincipal},
};

static HOST: OnceLock<Arc<WorkUnitHost>> = OnceLock::new();

pub struct WorkUnitHost {
    store: Arc<WorkGraphStore>,
    execution: Arc<ForgeExecutionService>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkUnitGetQuery {
    pub work_unit_id: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkGraphMutateInput {
    pub command: WorkGraphCommand,
}

pub fn local_work_unit_host() -> Option<Arc<WorkUnitHost>> {
    HOST.get().cloned()
}

pub async fn compose_work_unit_host(
    execution: Arc<ForgeExecutionService>,
    root: PathBuf,
) -> Result<()> {
    let store = execution
        .run(ExecutionClass::StoreIo, MAX_SNAPSHOT_BYTES, move || {
            Ok(WorkGraphStore::open(&root))
        })
        .await??;
    HOST.set(Arc::new(WorkUnitHost {
        store: Arc::new(store),
        execution,
    }))
    .map_err(|_| anyhow::anyhow!("work unit host already composed"))?;
    Ok(())
}

fn domain_owner(
    principal: &RequestPrincipal,
    admitted_identity: Option<&str>,
    write: bool,
) -> Result<String> {
    let capabilities = principal.capabilities();
    if !capabilities.contains(Capability::ContentRead)
        || !capabilities.contains(Capability::WorkshopInteract)
        || (write && !capabilities.contains(Capability::ContentWrite))
    {
        bail!("principal cannot access this work domain");
    }
    // LocalApp carries operator authority without a profile field. Its owner
    // must come from the frozen turn identity, never today's selected profile.
    let owner = match principal.profile_id() {
        Some(owner) => owner,
        None if principal.kind() == PrincipalKind::LocalApp => {
            admitted_identity.ok_or_else(|| {
                anyhow::anyhow!("work domain requires an identity captured at turn admission")
            })?
        }
        _ => bail!("work domain requires a bound owner identity"),
    };
    if owner.trim().is_empty() {
        bail!("work domain owner is empty");
    }
    Ok(owner.to_string())
}

fn admitted_domain(turn: &TurnExecutionContext, write: bool) -> Result<UserDomainRef> {
    Ok(UserDomainRef {
        authority_id: crate::workshop_authority::current()
            .map_err(anyhow::Error::msg)?
            .clone(),
        user_id: domain_owner(
            turn.principal(),
            turn.legacy_scope().identity_user_id.as_deref(),
            write,
        )?,
    })
}

fn validate_model_mutation(domain: &UserDomainRef, mutation: &WorkGraphMutation) -> Result<()> {
    if matches!(
        mutation,
        WorkGraphMutation::ReserveBudget { .. } | WorkGraphMutation::SettleBudget { .. }
    ) {
        bail!("budget custody requires a native execution adapter");
    }
    if let WorkGraphMutation::RecordResource {
        reference,
        resolution,
        native_revision,
        ..
    } = mutation
    {
        if *resolution != ResourceResolution::Unresolved || native_revision.is_some() {
            bail!(
                "model resource claims must remain unresolved; native adapters own resolution and revisions"
            );
        }
        if reference.kind == ResourceKind::Session {
            let id = medousa_types::SessionId::parse(&reference.id)?;
            if reference.authority_id != domain.authority_id
                || !crate::session_catalog::session_visible_to_profile(id.as_str(), &domain.user_id)
            {
                bail!("session resource is not visible to this owner");
            }
        }
    }
    let session = match mutation {
        WorkGraphMutation::AcceptWork { origin, .. } => origin.as_ref(),
        WorkGraphMutation::AttachConversation { session, .. } => Some(session),
        _ => None,
    };
    if let Some(session) = session
        && (session.authority_id != domain.authority_id
            || !crate::session_catalog::session_visible_to_profile(
                session.session_id.as_str(),
                &domain.user_id,
            ))
    {
        bail!("work conversation is not visible to this owner");
    }
    Ok(())
}

fn bounded_response(value: serde_json::Value) -> Result<serde_json::Value> {
    if serde_json::to_vec(&value)?.len() > MAX_SNAPSHOT_BYTES {
        bail!("work domain response exceeds byte budget; use a smaller page");
    }
    Ok(value)
}

impl WorkUnitHost {
    async fn with_store<T: Send + 'static>(
        &self,
        work: impl FnOnce(&WorkGraphStore) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let store = self.store.clone();
        self.execution
            .run(ExecutionClass::StoreIo, MAX_SNAPSHOT_BYTES, move || {
                Ok(work(&store))
            })
            .await?
    }

    pub async fn graph(
        &self,
        turn: &TurnExecutionContext,
        query: WorkGraphQuery,
    ) -> Result<serde_json::Value> {
        let domain = admitted_domain(turn, false)?;
        self.with_store(move |store| {
            bounded_response(serde_json::to_value(store.query(&domain, query)?)?)
        })
        .await
    }

    pub async fn get(
        &self,
        turn: &TurnExecutionContext,
        query: WorkUnitGetQuery,
    ) -> Result<serde_json::Value> {
        let domain = admitted_domain(turn, false)?;
        self.with_store(move |store| {
            let (unit, readiness_current, budget_usage) =
                store.inspect_work_unit(&domain, &query.work_unit_id)?;
            let mut value = serde_json::to_value(unit)?;
            value["readiness_current"] = readiness_current.into();
            value["budget_usage"] = serde_json::to_value(budget_usage)?;
            bounded_response(value)
        })
        .await
    }

    pub async fn record(
        &self,
        turn: &TurnExecutionContext,
        input: WorkGraphMutateInput,
    ) -> Result<serde_json::Value> {
        let domain = admitted_domain(turn, true)?;
        let provenance = RecordProvenance {
            actor_id: domain.user_id.clone(),
            source: RecordSource::ModelInferred,
            evidence: vec![],
        };
        self.with_store(move |store| {
            bounded_response(serde_json::to_value(store.apply_checked(
                &domain,
                input.command,
                provenance,
                |mutation| {
                    validate_model_mutation(&domain, mutation).map_err(|error| {
                        medousa_store::PersistenceError::new(
                            medousa_store::PersistenceErrorKind::PermanentIo,
                            error.to_string(),
                        )
                    })
                },
            )?)?)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request_principal::TransportClass;

    fn turn(owner: &str, session: &str) -> TurnExecutionContext {
        let scope = crate::turn_continuation::TurnContinuationScope {
            turn_correlation_id: format!("turn-{session}"),
            session_id: session.into(),
            identity_user_id: Some(owner.into()),
            original_prompt: "Keep release documentation current".into(),
            delivery_target: None,
            provider: "test".into(),
            model: "test".into(),
            response_depth_mode: "standard".into(),
            supports_ui_artifacts: false,
            supports_liquid_markdown: false,
            supports_browser_host: false,
            browser_driver_id: None,
            selected_worlds: vec![],
            channel_surface: None,
        };
        TurnExecutionContext::from_scope(
            format!("turn-{session}"),
            RequestPrincipal::worker(owner),
            tokio_util::sync::CancellationToken::new(),
            None,
            scope,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn admitted_host_reads_saved_work_in_another_session_without_crossing_owners() {
        crate::workshop_authority::initialize(
            &medousa_types::secrets::InstallationId::parse(
                crate::workshop_authority::TEST_INSTALLATION_ID,
            )
            .unwrap(),
        )
        .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let execution = Arc::new(ForgeExecutionService::new());
        let store = execution
            .run(ExecutionClass::StoreIo, MAX_SNAPSHOT_BYTES, move || {
                Ok(WorkGraphStore::open(&root))
            })
            .await
            .unwrap()
            .unwrap();
        let host = WorkUnitHost {
            store: Arc::new(store),
            execution,
        };
        let first = turn("user:a", "work-origin");
        let later = turn("user:a", "work-later");
        let foreign = turn("user:b", "work-other-owner");
        let command = WorkGraphCommand {
            command_id: "accept-docs".into(),
            expected_revision: 0,
            mutation: WorkGraphMutation::AcceptWork {
                work_unit_id: "release-docs".into(),
                intent: "Keep release documentation current".into(),
                kind: WorkUnitKind::Maintenance,
                scope: WorkScope::default(),
                completion_condition: "Reflects accepted release changes".into(),
                contact: WorkContactPreference::Silent,
                origin: None,
                budget: None,
            },
        };
        let receipt = host
            .record(
                &first,
                WorkGraphMutateInput {
                    command: command.clone(),
                },
            )
            .await
            .unwrap();
        assert_eq!(receipt["revision"], 1);
        let read = host
            .get(
                &later,
                WorkUnitGetQuery {
                    work_unit_id: "release-docs".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(read["state"], "accepted");
        assert_eq!(read["contact"]["kind"], "silent");
        assert!(read["origin"].is_null());
        assert!(
            host.get(
                &foreign,
                WorkUnitGetQuery {
                    work_unit_id: "release-docs".into()
                }
            )
            .await
            .is_err()
        );
        assert_eq!(
            host.graph(&foreign, WorkGraphQuery::default())
                .await
                .unwrap()["revision"],
            0
        );
        assert_eq!(
            host.record(&later, WorkGraphMutateInput { command })
                .await
                .unwrap()["replayed"],
            true
        );
    }

    #[test]
    fn domain_uses_bound_or_frozen_identity_and_rejects_unbound_access() {
        let local = RequestPrincipal::local_app(Arc::from("local:test"), TransportClass::Loopback);
        assert!(domain_owner(&local, None, true).is_err());
        assert_eq!(
            domain_owner(&local, Some("user:frozen"), true).unwrap(),
            "user:frozen"
        );
        let worker = RequestPrincipal::worker("user:bound");
        assert_eq!(
            domain_owner(&worker, Some("user:other"), true).unwrap(),
            "user:bound"
        );
        assert!(
            domain_owner(
                &RequestPrincipal::anonymous(TransportClass::Direct),
                Some("user:other"),
                false
            )
            .is_err()
        );
        let read_only = RequestPrincipal::external_agent(
            Arc::from("external:test"),
            "user:owner".into(),
            false,
            TransportClass::Direct,
        );
        assert!(domain_owner(&read_only, Some("user:other"), true).is_err());
    }

    #[test]
    fn model_cannot_claim_native_resolution_or_revision() {
        let domain = UserDomainRef {
            authority_id: medousa_types::AuthorityId::parse(format!("auth_{}", "a".repeat(64)))
                .unwrap(),
            user_id: "user:test".into(),
        };
        let mut mutation = WorkGraphMutation::RecordResource {
            reference: ResourceRef {
                authority_id: domain.authority_id.clone(),
                kind: ResourceKind::Artifact,
                id: "artifact-1".into(),
            },
            locator: None,
            native_revision: None,
            resolution: ResourceResolution::Unresolved,
        };
        assert!(validate_model_mutation(&domain, &mutation).is_ok());
        if let WorkGraphMutation::RecordResource { resolution, .. } = &mut mutation {
            *resolution = ResourceResolution::Available;
        }
        assert!(validate_model_mutation(&domain, &mutation).is_err());
        if let WorkGraphMutation::RecordResource {
            resolution,
            native_revision,
            ..
        } = &mut mutation
        {
            *resolution = ResourceResolution::Unresolved;
            *native_revision = Some("invented".into());
        }
        assert!(validate_model_mutation(&domain, &mutation).is_err());
        for mutation in [
            WorkGraphMutation::ReserveBudget {
                reservation_id: "hold".into(),
                work_unit_id: "work".into(),
                execution: ResourceRef {
                    authority_id: domain.authority_id.clone(),
                    kind: ResourceKind::Assignment,
                    id: "execution".into(),
                },
                reserved_cost_microusd: 1,
            },
            WorkGraphMutation::SettleBudget {
                reservation_id: "hold".into(),
                disposition: WorkBudgetDisposition::NotStarted,
                actual_cost_microusd: 0,
            },
        ] {
            assert!(validate_model_mutation(&domain, &mutation).is_err());
        }
    }

    #[test]
    fn new_conversation_attachments_require_local_native_visibility() {
        let domain = UserDomainRef {
            authority_id: medousa_types::AuthorityId::parse(format!("auth_{}", "a".repeat(64)))
                .unwrap(),
            user_id: "user:test".into(),
        };
        let session = medousa_types::SessionRef {
            authority_id: medousa_types::AuthorityId::parse(format!("auth_{}", "b".repeat(64)))
                .unwrap(),
            session_id: medousa_types::SessionId::parse("session-foreign").unwrap(),
        };
        assert!(
            validate_model_mutation(
                &domain,
                &WorkGraphMutation::AttachConversation {
                    work_unit_id: "work".into(),
                    session,
                }
            )
            .is_err()
        );
        let missing = medousa_types::SessionRef {
            authority_id: domain.authority_id.clone(),
            session_id: medousa_types::SessionId::parse(format!(
                "missing-{}",
                uuid::Uuid::new_v4().simple()
            ))
            .unwrap(),
        };
        assert!(
            validate_model_mutation(
                &domain,
                &WorkGraphMutation::AttachConversation {
                    work_unit_id: "work".into(),
                    session: missing,
                }
            )
            .is_err()
        );
    }
}
