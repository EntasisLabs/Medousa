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

mod coordinator;
pub(crate) mod model_wake;
mod native_content;
mod native_graph;
mod native_project;
mod native_vault;
pub mod participant;
mod peer_coordination;
mod provider_dispatch;
mod provider_events;
mod project_lifecycle;
pub use project_lifecycle::ProjectLifecycleMutation;
pub use native_content::WorkContentResolveInput;
pub use native_project::{WorkProjectCreateInput, WorkProjectResolveInput};
pub use native_vault::{WorkNativeReconcileInput, WorkNativeResolveInput};

pub struct WorkUnitHost {
    store: Arc<WorkGraphStore>,
    execution: Arc<ForgeExecutionService>,
    forge: Arc<medousa_forge::forge::Forge>,
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
    forge: Arc<medousa_forge::forge::Forge>,
) -> Result<()> {
    let store = execution
        .run(ExecutionClass::StoreIo, MAX_SNAPSHOT_BYTES, move || {
            Ok(WorkGraphStore::open(&root))
        })
        .await??;
    HOST.set(Arc::new(WorkUnitHost {
        store: Arc::new(store),
        execution,
        forge,
    }))
    .map_err(|_| anyhow::anyhow!("work unit host already composed"))?;
    Ok(())
}

fn domain_owner(
    principal: &RequestPrincipal,
    admitted_identity: Option<&str>,
    write: bool,
) -> Result<String> {
    domain_owner_in_mode(
        principal,
        admitted_identity,
        write,
        crate::shared_mode::is_shared_mode(),
    )
}

fn domain_owner_in_mode(
    principal: &RequestPrincipal,
    admitted_identity: Option<&str>,
    write: bool,
    shared_mode: bool,
) -> Result<String> {
    let capabilities = principal.capabilities();
    if !capabilities.contains(Capability::ContentRead)
        || !capabilities.contains(Capability::WorkshopInteract)
        || (write && !capabilities.contains(Capability::ContentWrite))
    {
        bail!("principal cannot access this work domain");
    }
    if let Some(owner) = principal
        .profile_id()
        .filter(|value| !value.trim().is_empty())
    {
        return Ok(owner.to_string());
    }
    // LocalApp carries operator authority without a profile field. Its owner
    // must come from the frozen turn identity, never today's selected profile.
    if principal.kind() == PrincipalKind::LocalApp {
        return admitted_identity
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
            .ok_or_else(|| {
                anyhow::anyhow!("work domain requires an identity captured at turn admission")
            });
    }
    // A personal Portal operator whose pairing lost its profile resolves the
    // same way the co-located app does. Shared mode does not.
    crate::request_principal::personal_portal_workshop_owner(principal, shared_mode)
        .ok_or_else(|| anyhow::anyhow!("work domain requires a bound owner identity"))
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
        WorkGraphMutation::ReserveBudget { .. }
            | WorkGraphMutation::SettleBudget { .. }
            | WorkGraphMutation::RegisterProviderRequest { .. }
            | WorkGraphMutation::ClaimProviderRequest { .. }
            | WorkGraphMutation::RecordProviderEvent { .. }
            | WorkGraphMutation::AdvanceProviderStage { .. }
            | WorkGraphMutation::RegisterProviderDispatch { .. }
            | WorkGraphMutation::CloseProviderDispatch { .. }
            | WorkGraphMutation::RegisterCoordinatorWake { .. }
            | WorkGraphMutation::ClaimCoordinatorWake { .. }
            | WorkGraphMutation::CompleteCoordinatorWake { .. }
            | WorkGraphMutation::BlockCoordinatorWake { .. }
    ) {
        bail!("native custody and provider evidence require an authenticated native adapter");
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

fn validate_peer_qualification(
    native: Option<&medousa_acp_client::coordination::store::CoordinationStore>,
    domain: &UserDomainRef,
    mutation: &WorkGraphMutation,
) -> Result<()> {
    if let WorkGraphMutation::SetState {
        work_unit_id,
        state: WorkUnitState::Satisfied,
        ..
    } = mutation
    {
        let native = native.ok_or_else(|| {
            anyhow::anyhow!(
                "native work-control registry unavailable; satisfaction cannot be qualified"
            )
        })?;
        if native.work_is_controlled(domain, work_unit_id)? {
            bail!(
                "registered executor/reviewer work requires a native review verdict; model claims cannot satisfy it"
            );
        }
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
    pub async fn events(
        &self,
        turn: &TurnExecutionContext,
        query: WorkEventsQuery,
    ) -> Result<serde_json::Value> {
        let domain = admitted_domain(turn, false)?;
        let actor = turn
            .principal()
            .credential_id()
            .map(|credential| credential.as_str().to_string())
            .unwrap_or_else(|| domain.user_id.clone());
        self.events_in_domain(domain, actor, query).await
    }

    async fn events_in_domain(
        &self,
        domain: UserDomainRef,
        actor: String,
        query: WorkEventsQuery,
    ) -> Result<serde_json::Value> {
        self.with_store(move |store| {
            bounded_response(serde_json::to_value(store.events(&domain, &actor, query)?)?)
        })
        .await
    }
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
        self.graph_in_domain(domain, query).await
    }

    async fn graph_in_domain(
        &self,
        domain: UserDomainRef,
        query: WorkGraphQuery,
    ) -> Result<serde_json::Value> {
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
        self.get_in_domain(domain, query.work_unit_id).await
    }

    async fn get_in_domain(
        &self,
        domain: UserDomainRef,
        work_unit_id: String,
    ) -> Result<serde_json::Value> {
        self.with_store(move |store| {
            let (unit, readiness_current, budget_usage) =
                store.inspect_work_unit(&domain, &work_unit_id)?;
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
        // Preserve actor/digest compatibility for existing model commands.
        let actor_id = if matches!(
            &input.command.mutation,
            WorkGraphMutation::Subscribe { .. }
                | WorkGraphMutation::AcknowledgeEvent { .. }
                | WorkGraphMutation::StopSubscription { .. }
        ) {
            turn.principal()
                .credential_id()
                .map(|credential| credential.as_str().to_string())
                .unwrap_or_else(|| domain.user_id.clone())
        } else {
            domain.user_id.clone()
        };
        self.record_in_domain(domain, actor_id, input.command).await
    }

    async fn record_in_domain(
        &self,
        domain: UserDomainRef,
        actor_id: String,
        command: WorkGraphCommand,
    ) -> Result<serde_json::Value> {
        let provenance = RecordProvenance {
            actor_id,
            source: RecordSource::ModelInferred,
            evidence: vec![],
        };
        let native =
            crate::daemon::coordination::local_coordination_host().map(|host| host.work_registry());
        self.with_store(move |store| {
            bounded_response(serde_json::to_value(store.apply_checked(
                &domain,
                command,
                provenance,
                |mutation| {
                    (|| -> Result<()> {
                        validate_model_mutation(&domain, mutation)?;
                        validate_peer_qualification(native.as_deref(), &domain, mutation)?;
                        Ok(())
                    })()
                    .map_err(|error| {
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
    use crate::request_principal::{RequestPrincipal, TransportClass};

    fn workshop_owner() -> String {
        crate::user_profiles::resolve_workshop_identity_user_id()
    }

    #[test]
    fn personal_portal_without_a_profile_owns_the_work_domain() {
        let owner = domain_owner_in_mode(
            &RequestPrincipal::portal_for_test(None, true),
            None,
            false,
            false,
        )
        .expect("personal portal");
        assert_eq!(owner, workshop_owner());
    }

    #[test]
    fn shared_portal_without_a_profile_cannot_own_the_work_domain() {
        let error = domain_owner_in_mode(
            &RequestPrincipal::portal_for_test(None, true),
            Some("user:admitted"),
            true,
            true,
        )
        .expect_err("shared portal");
        assert!(
            error
                .to_string()
                .contains("work domain requires a bound owner identity"),
            "{error}"
        );
    }

    #[test]
    fn unbound_non_portal_principals_cannot_own_the_work_domain() {
        let member = domain_owner_in_mode(
            &RequestPrincipal::portal_for_test(None, false),
            None,
            false,
            false,
        );
        assert!(
            member
                .unwrap_err()
                .to_string()
                .contains("bound owner identity")
        );
        for principal in [
            RequestPrincipal::anonymous(TransportClass::Direct),
            RequestPrincipal::peer_for_test(None),
            RequestPrincipal::external_agent_without_profile_for_test(),
        ] {
            assert!(
                domain_owner_in_mode(&principal, None, false, false).is_err(),
                "{:?} owned the domain",
                principal.kind()
            );
        }
    }

    #[test]
    fn a_bound_portal_profile_owns_the_work_domain() {
        let owner = domain_owner_in_mode(
            &RequestPrincipal::portal_for_test(Some("user:seat"), true),
            Some("user:admitted"),
            true,
            true,
        )
        .expect("bound portal");
        assert_eq!(owner, "user:seat");
    }

    #[test]
    fn local_app_work_owner_stays_the_admitted_turn_identity() {
        let owner = domain_owner_in_mode(
            &RequestPrincipal::local_app(
                std::sync::Arc::from("home-local"),
                TransportClass::Loopback,
            ),
            Some("user:admitted"),
            false,
            false,
        )
        .expect("local app");
        assert_eq!(owner, "user:admitted");
        let missing = domain_owner_in_mode(
            &RequestPrincipal::local_app(
                std::sync::Arc::from("home-local"),
                TransportClass::Loopback,
            ),
            None,
            false,
            false,
        );
        assert!(
            missing
                .unwrap_err()
                .to_string()
                .contains("identity captured at turn admission")
        );
    }

    pub(super) fn turn(owner: &str, session: &str) -> TurnExecutionContext {
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
        let (store, forge) = execution
            .run(ExecutionClass::StoreIo, MAX_SNAPSHOT_BYTES, move || {
                Ok((
                    WorkGraphStore::open(&root),
                    medousa_forge::forge::Forge::open(root.join("forge")),
                ))
            })
            .await
            .unwrap();
        let host = WorkUnitHost {
            store: Arc::new(store.unwrap()),
            execution,
            forge: Arc::new(forge.unwrap()),
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
