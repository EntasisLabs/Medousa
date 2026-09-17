//! Transport-independent peer admission and durable dispatch seams.
//!
//! Destination authorization and context visibility must be checked immediately
//! before dispatch. Persistence/idempotent command claiming belongs to the host,
//! not this port: calling it twice may execute twice.

use anyhow::{Result, bail};
pub mod context;
pub mod store;
use async_trait::async_trait;
use medousa_types::coordination::{
    ExternalPeerAssignmentBinding, ExternalPeerAssignmentRequest, ExternalPeerCandidate,
    PeerAvailability,
};

#[async_trait]
pub trait PeerAssignmentAuthority: Send + Sync {
    /// Must enforce current membership, source visibility, and the destination
    /// grant for this exact owner, target, context, and execution session.
    async fn authorize(&self, request: &ExternalPeerAssignmentRequest) -> Result<()>;
}

#[async_trait]
pub trait ExternalPeerExecutionPort: Send + Sync {
    async fn discover(&self) -> Result<Vec<ExternalPeerCandidate>>;
    /// Receives only an admitted request; must not silently change the target
    /// or fall back to a simulated agent when the requested adapter is absent.
    /// Production implementations must enforce the destination grant again at
    /// actual execution admission: discovery-time checks cannot fence dispatch.
    async fn assign(
        &self,
        request: &ExternalPeerAssignmentRequest,
    ) -> Result<ExternalPeerAssignmentBinding>;
}

/// Async host journal boundary; production hosts admit synchronous store I/O.
#[async_trait]
pub trait PeerDispatchJournal: Send + Sync {
    async fn claim(
        &self,
        request: &ExternalPeerAssignmentRequest,
    ) -> Result<store::AssignmentClaim>;
    async fn binding(
        &self,
        request: &ExternalPeerAssignmentRequest,
    ) -> Result<Option<ExternalPeerAssignmentBinding>>;
    async fn record(&self, binding: &ExternalPeerAssignmentBinding) -> Result<()>;
}

async fn prepare_assignment(
    authority: &dyn PeerAssignmentAuthority,
    adapter: &dyn ExternalPeerExecutionPort,
    request: &ExternalPeerAssignmentRequest,
) -> Result<()> {
    validate_assignment_request(request)?;
    authority.authorize(request).await?;
    let candidates = adapter.discover().await?;
    let candidate = candidates
        .iter()
        .find(|candidate| candidate.target == request.target)
        .ok_or_else(|| anyhow::anyhow!("requested external peer target is not available"))?;
    if let PeerAvailability::Unavailable { reason } = &candidate.availability {
        bail!("requested external peer target is unavailable: {reason}");
    }
    Ok(())
}

/// Claim before dispatch; never automatically retry an unresolved external effect.
pub async fn dispatch_external_peer_assignment(
    authority: &dyn PeerAssignmentAuthority,
    adapter: &dyn ExternalPeerExecutionPort,
    journal: &dyn PeerDispatchJournal,
    request: &ExternalPeerAssignmentRequest,
) -> Result<ExternalPeerAssignmentBinding> {
    validate_assignment_request(request)?;
    authority.authorize(request).await?;
    // Existing accepted work can be inspected even if its adapter is now offline.
    // A claim with no binding is uncertain, not another invitation to dispatch.
    if let Some(binding) = journal.binding(request).await? {
        validate_assignment_binding(request, &binding)?;
        // Also verify exact command replay; inspecting a binding alone must not
        // allow changed instructions to reuse an assignment identity.
        if journal.claim(request).await? != store::AssignmentClaim::Existing {
            bail!("peer binding exists without its dispatch claim; reconcile before retry");
        }
        return Ok(binding);
    }
    prepare_assignment(authority, adapter, request).await?;
    if journal.claim(request).await? == store::AssignmentClaim::Existing {
        if let Some(binding) = journal.binding(request).await? {
            validate_assignment_binding(request, &binding)?;
            return Ok(binding);
        }
        bail!("external peer dispatch is unresolved; reconcile before retry");
    }
    authority.authorize(request).await?;
    let binding = adapter.assign(request).await?;
    validate_assignment_binding(request, &binding)?;
    journal.record(&binding).await.map_err(|error| {
        anyhow::anyhow!(
            "external peer started but binding persistence failed; reconcile before retry: {error}"
        )
    })?;
    Ok(binding)
}

pub fn validate_assignment_request(request: &ExternalPeerAssignmentRequest) -> Result<()> {
    for value in [
        &request.assignment_id,
        &request.idempotency_key,
        &request.owner_principal_id,
        &request.channel.channel_id,
        &request.target.execution_runtime_id,
        &request.instructions,
        &request.execution_grant_id,
        &request.forge_work_id,
    ] {
        if value.trim().is_empty() {
            bail!("external peer assignment contains an empty required field");
        }
    }
    if request.execution_session.authority_id != request.target.authority_id {
        bail!("executor session authority does not match the destination workshop");
    }
    if request.execution_session == request.owner_session {
        bail!("external peer must not reuse the owner's interactive session");
    }
    if request.context.sources.is_empty() {
        bail!("external peer assignment requires explicit source context");
    }
    Ok(())
}

fn validate_assignment_binding(
    request: &ExternalPeerAssignmentRequest,
    binding: &ExternalPeerAssignmentBinding,
) -> Result<()> {
    if binding.assignment_id != request.assignment_id
        || binding.owner_principal_id != request.owner_principal_id
        || binding.channel != request.channel
        || binding.target != request.target
        || binding.execution_session != request.execution_session
        || binding.agent_session_id.trim().is_empty()
    {
        bail!(
            "external peer adapter returned an invalid assignment binding; reconcile before retry"
        );
    }
    Ok(())
}

pub async fn admit_external_peer_assignment(
    authority: &dyn PeerAssignmentAuthority,
    adapter: &dyn ExternalPeerExecutionPort,
    request: &ExternalPeerAssignmentRequest,
) -> Result<ExternalPeerAssignmentBinding> {
    prepare_assignment(authority, adapter, request).await?;
    // Discovery may await I/O: revocation during discovery must block dispatch.
    authority.authorize(request).await?;
    let binding = adapter.assign(request).await?;
    validate_assignment_binding(request, &binding)?;
    Ok(binding)
}

#[cfg(test)]
mod tests {
    use super::*;
    use medousa_types::coordination::{
        CoordinationChannelRef, ExternalPeerRuntime, ExternalPeerTarget,
    };
    use medousa_types::{ContextManifest, ConversationRangeSelection, ResolvedConversationRange};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct FakeAuthority {
        calls: AtomicUsize,
        deny_on: usize,
    }

    #[async_trait]
    impl PeerAssignmentAuthority for FakeAuthority {
        async fn authorize(&self, _: &ExternalPeerAssignmentRequest) -> Result<()> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            if call == self.deny_on {
                bail!("channel visibility does not grant execution authority");
            }
            Ok(())
        }
    }

    struct FakeAdapter {
        candidate: ExternalPeerCandidate,
        discoveries: AtomicUsize,
        assignments: AtomicUsize,
        invalid_binding: bool,
    }

    #[async_trait]
    impl ExternalPeerExecutionPort for FakeAdapter {
        async fn discover(&self) -> Result<Vec<ExternalPeerCandidate>> {
            self.discoveries.fetch_add(1, Ordering::SeqCst);
            Ok(vec![self.candidate.clone()])
        }

        async fn assign(
            &self,
            request: &ExternalPeerAssignmentRequest,
        ) -> Result<ExternalPeerAssignmentBinding> {
            self.assignments.fetch_add(1, Ordering::SeqCst);
            Ok(ExternalPeerAssignmentBinding {
                assignment_id: request.assignment_id.clone(),
                owner_principal_id: request.owner_principal_id.clone(),
                channel: request.channel.clone(),
                target: request.target.clone(),
                execution_session: request.execution_session.clone(),
                agent_session_id: if self.invalid_binding {
                    String::new()
                } else {
                    "fake-acp-session".into()
                },
            })
        }
    }

    fn fixture(
        runtime: ExternalPeerRuntime,
    ) -> (ExternalPeerAssignmentRequest, FakeAuthority, FakeAdapter) {
        let source = medousa_types::SessionRef {
            authority_id: format!("auth_{}", "a".repeat(64)).parse().unwrap(),
            session_id: "owner-session".parse().unwrap(),
        };
        let target = ExternalPeerTarget {
            authority_id: format!("auth_{}", "b".repeat(64)).parse().unwrap(),
            execution_runtime_id: "worker-host".into(),
            runtime,
        };
        let request = ExternalPeerAssignmentRequest {
            assignment_id: "assignment-1".into(),
            idempotency_key: "command-1".into(),
            owner_principal_id: "user:alice".into(),
            owner_session: source.clone(),
            channel: CoordinationChannelRef {
                authority_id: source.authority_id.clone(),
                channel_id: "project-review".into(),
            },
            target: target.clone(),
            context: ContextManifest {
                manifest_id: format!("ctx_{}", "a".repeat(32)).parse().unwrap(),
                sources: vec![ResolvedConversationRange {
                    selection: ConversationRangeSelection {
                        session: source,
                        after_entry_seq: None,
                        through_entry_seq: 1,
                    },
                    selection_digest: "sha256:fixture".into(),
                }],
                created_by: "user:alice".into(),
                created_at: serde_json::from_str("\"2026-09-17T00:00:00Z\"").unwrap(),
            },
            execution_session: medousa_types::SessionRef {
                authority_id: target.authority_id.clone(),
                session_id: "executor-session".parse().unwrap(),
            },
            instructions: "Review the designated checkpoint; do not publish changes".into(),
            execution_grant_id: "grant-1".into(),
            forge_work_id: "work-1".into(),
        };
        (
            request,
            FakeAuthority {
                calls: AtomicUsize::new(0),
                deny_on: 0,
            },
            FakeAdapter {
                candidate: ExternalPeerCandidate {
                    target,
                    availability: PeerAvailability::Ready,
                },
                discoveries: AtomicUsize::new(0),
                assignments: AtomicUsize::new(0),
                invalid_binding: false,
            },
        )
    }

    #[tokio::test]
    async fn all_external_peer_kinds_preserve_owner_channel_and_execution_binding() {
        for runtime in [
            ExternalPeerRuntime::Codex,
            ExternalPeerRuntime::Cursor,
            ExternalPeerRuntime::Hermes,
        ] {
            let (request, authority, adapter) = fixture(runtime);
            let binding = admit_external_peer_assignment(&authority, &adapter, &request)
                .await
                .unwrap();
            assert_eq!(binding.channel, request.channel);
            assert_eq!(binding.execution_session, request.execution_session);
            assert_eq!(binding.target.runtime, runtime);
            assert_eq!(adapter.assignments.load(Ordering::SeqCst), 1);
            assert_eq!(authority.calls.load(Ordering::SeqCst), 2);
        }
    }

    #[tokio::test]
    async fn membership_alone_cannot_discover_or_spawn() {
        let (request, mut authority, adapter) = fixture(ExternalPeerRuntime::Codex);
        authority.deny_on = 1;
        assert!(
            admit_external_peer_assignment(&authority, &adapter, &request)
                .await
                .is_err()
        );
        assert_eq!(adapter.discoveries.load(Ordering::SeqCst), 0);
        assert_eq!(adapter.assignments.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn revoked_grant_after_discovery_blocks_spawn() {
        let (request, mut authority, adapter) = fixture(ExternalPeerRuntime::Codex);
        authority.deny_on = 2;
        assert!(
            admit_external_peer_assignment(&authority, &adapter, &request)
                .await
                .is_err()
        );
        assert_eq!(adapter.discoveries.load(Ordering::SeqCst), 1);
        assert_eq!(adapter.assignments.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn missing_authentication_does_not_fall_back_to_stub() {
        let (request, authority, mut adapter) = fixture(ExternalPeerRuntime::Hermes);
        adapter.candidate.availability = PeerAvailability::Unavailable {
            reason: "not authenticated".into(),
        };
        let error = admit_external_peer_assignment(&authority, &adapter, &request)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("not authenticated"));
        assert_eq!(adapter.assignments.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn target_mismatch_cannot_silently_select_other_compute() {
        let (request, authority, mut adapter) = fixture(ExternalPeerRuntime::Cursor);
        adapter.candidate.target.execution_runtime_id = "different-host".into();
        assert!(
            admit_external_peer_assignment(&authority, &adapter, &request)
                .await
                .is_err()
        );
        assert_eq!(adapter.assignments.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn wrong_session_authority_is_rejected_before_adapter_io() {
        let (mut request, authority, adapter) = fixture(ExternalPeerRuntime::Codex);
        request.execution_session.authority_id = request.channel.authority_id.clone();
        assert!(
            admit_external_peer_assignment(&authority, &adapter, &request)
                .await
                .is_err()
        );
        assert_eq!(adapter.discoveries.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn invalid_binding_reports_uncertain_execution_without_retry() {
        let (request, authority, mut adapter) = fixture(ExternalPeerRuntime::Cursor);
        adapter.invalid_binding = true;
        let error = admit_external_peer_assignment(&authority, &adapter, &request)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("reconcile before retry"));
        assert_eq!(adapter.assignments.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn owner_session_cannot_be_reused_for_peer_execution() {
        let (mut request, authority, adapter) = fixture(ExternalPeerRuntime::Codex);
        request.owner_session = request.execution_session.clone();
        assert!(
            admit_external_peer_assignment(&authority, &adapter, &request)
                .await
                .is_err()
        );
        assert_eq!(adapter.discoveries.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn missing_context_or_instructions_never_dispatches() {
        let (mut request, authority, adapter) = fixture(ExternalPeerRuntime::Codex);
        let original_context = request.context.clone();
        request.context.sources.clear();
        assert!(
            admit_external_peer_assignment(&authority, &adapter, &request)
                .await
                .is_err()
        );
        request.context = original_context;
        request.instructions.clear();
        assert!(
            admit_external_peer_assignment(&authority, &adapter, &request)
                .await
                .is_err()
        );
        assert_eq!(adapter.assignments.load(Ordering::SeqCst), 0);
    }

    fn persisted_fixture() -> (
        tempfile::TempDir,
        store::CoordinationStore,
        ExternalPeerAssignmentRequest,
    ) {
        let (request, _, _) = fixture(ExternalPeerRuntime::Codex);
        let temp = tempfile::tempdir().unwrap();
        let store = store::CoordinationStore::open(temp.path()).unwrap();
        store
            .create_channel(&medousa_types::coordination::CoordinationChannelRecord {
                channel: request.channel.clone(),
                owner_principal_id: request.owner_principal_id.clone(),
                member_principal_ids: vec![request.owner_principal_id.clone()],
                attached_sessions: vec![request.owner_session.clone()],
            })
            .unwrap();
        store
            .approve_assignment(&medousa_types::coordination::ExternalPeerAssignmentGrant {
                request: request.clone(),
                expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
            })
            .unwrap();
        (temp, store, request)
    }

    #[test]
    fn unresolved_dispatch_survives_restart_without_being_claimed_again() {
        let (temp, store, request) = persisted_fixture();
        assert_eq!(
            store.claim_assignment(&request).unwrap(),
            store::AssignmentClaim::Claimed
        );
        drop(store);
        let reopened = store::CoordinationStore::open(temp.path()).unwrap();
        assert_eq!(
            reopened.claim_assignment(&request).unwrap(),
            store::AssignmentClaim::Existing
        );
        assert!(
            reopened
                .peer(&request.channel, &request.assignment_id)
                .is_err()
        );
    }

    #[test]
    fn changed_request_cannot_reuse_command_or_assignment_identity() {
        let (_temp, store, request) = persisted_fixture();
        store.claim_assignment(&request).unwrap();
        let mut changed = request.clone();
        changed.instructions = "different work".into();
        assert!(store.claim_assignment(&changed).is_err());
        changed.idempotency_key = "new-command".into();
        assert!(store.claim_assignment(&changed).is_err());
        changed = request.clone();
        changed.assignment_id = "different-assignment".into();
        assert!(store.claim_assignment(&changed).is_err());
    }

    #[test]
    fn concurrent_stores_claim_one_dispatch_only() {
        let (temp, _store, request) = persisted_fixture();
        let outcomes = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8)
                .map(|_| {
                    let path = temp.path();
                    let request = &request;
                    scope.spawn(move || {
                        store::CoordinationStore::open(path)
                            .unwrap()
                            .claim_assignment(request)
                            .unwrap()
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .collect::<Vec<_>>()
        });
        assert_eq!(
            outcomes
                .iter()
                .filter(|claim| **claim == store::AssignmentClaim::Claimed)
                .count(),
            1
        );
        assert_eq!(
            outcomes
                .iter()
                .filter(|claim| **claim == store::AssignmentClaim::Existing)
                .count(),
            7
        );
    }

    #[tokio::test]
    async fn peer_binding_persists_and_rejects_wrong_execution() {
        let (temp, store, request) = persisted_fixture();
        let (_, authority, adapter) = fixture(ExternalPeerRuntime::Codex);
        store.claim_assignment(&request).unwrap();
        let binding = admit_external_peer_assignment(&authority, &adapter, &request)
            .await
            .unwrap();
        assert!(store.record_peer(&binding).unwrap());
        drop(store);
        let reopened = store::CoordinationStore::open(temp.path()).unwrap();
        assert_eq!(
            reopened
                .peer(&request.channel, &request.assignment_id)
                .unwrap(),
            binding
        );
        let mut wrong = binding.clone();
        wrong.target.runtime = ExternalPeerRuntime::Cursor;
        assert!(reopened.record_peer(&wrong).is_err());
        assert_eq!(
            reopened
                .peer(&request.channel, &request.assignment_id)
                .unwrap(),
            binding
        );
    }

    #[test]
    fn grants_are_exact_expiring_and_durably_revocable() {
        let (temp, store, request) = persisted_fixture();
        store
            .require_assignment_grant(&request, chrono::Utc::now())
            .unwrap();
        let mut changed = request.clone();
        changed.instructions.push_str(" and deploy");
        assert!(
            store
                .require_assignment_grant(&changed, chrono::Utc::now())
                .is_err()
        );
        assert!(
            store
                .require_assignment_grant(&request, chrono::Utc::now() + chrono::Duration::hours(2))
                .is_err()
        );
        store
            .revoke_assignment_grant(&request.channel, &request.execution_grant_id)
            .unwrap();
        drop(store);
        let reopened = store::CoordinationStore::open(temp.path()).unwrap();
        assert!(
            reopened
                .require_assignment_grant(&request, chrono::Utc::now())
                .is_err()
        );
    }

    #[tokio::test]
    async fn revocation_blocks_binding_persistence() {
        let (_temp, store, request) = persisted_fixture();
        let (_, authority, adapter) = fixture(ExternalPeerRuntime::Codex);
        store.claim_assignment(&request).unwrap();
        let binding = admit_external_peer_assignment(&authority, &adapter, &request)
            .await
            .unwrap();
        store
            .revoke_assignment_grant(&request.channel, &request.execution_grant_id)
            .unwrap();
        assert!(store.record_peer(&binding).is_err());
        assert!(
            store
                .peer_if_recorded(&request.channel, &request.assignment_id)
                .unwrap()
                .is_none()
        );
    }

    #[derive(Default)]
    struct FakeJournal {
        request: tokio::sync::Mutex<Option<ExternalPeerAssignmentRequest>>,
        binding: tokio::sync::Mutex<Option<ExternalPeerAssignmentBinding>>,
        fail_record: bool,
    }

    #[async_trait]
    impl PeerDispatchJournal for FakeJournal {
        async fn claim(
            &self,
            request: &ExternalPeerAssignmentRequest,
        ) -> Result<store::AssignmentClaim> {
            let mut claimed = self.request.lock().await;
            match claimed.as_ref() {
                Some(existing) if existing == request => Ok(store::AssignmentClaim::Existing),
                Some(_) => bail!("conflicting request"),
                None => {
                    *claimed = Some(request.clone());
                    Ok(store::AssignmentClaim::Claimed)
                }
            }
        }
        async fn binding(
            &self,
            _: &ExternalPeerAssignmentRequest,
        ) -> Result<Option<ExternalPeerAssignmentBinding>> {
            Ok(self.binding.lock().await.clone())
        }
        async fn record(&self, binding: &ExternalPeerAssignmentBinding) -> Result<()> {
            if self.fail_record {
                bail!("persistence unavailable");
            }
            *self.binding.lock().await = Some(binding.clone());
            Ok(())
        }
    }

    #[tokio::test]
    async fn durable_dispatch_replays_even_when_runtime_goes_offline() {
        let (request, authority, mut adapter) = fixture(ExternalPeerRuntime::Codex);
        let journal = FakeJournal::default();
        let binding = dispatch_external_peer_assignment(&authority, &adapter, &journal, &request)
            .await
            .unwrap();
        adapter.candidate.availability = PeerAvailability::Unavailable {
            reason: "offline".into(),
        };
        assert_eq!(
            dispatch_external_peer_assignment(&authority, &adapter, &journal, &request)
                .await
                .unwrap(),
            binding
        );
        assert_eq!(adapter.assignments.load(Ordering::SeqCst), 1);
        let mut changed = request.clone();
        changed.instructions.push_str(" changed");
        assert!(
            dispatch_external_peer_assignment(&authority, &adapter, &journal, &changed)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn uncertain_dispatch_never_automatically_spawns_again() {
        for invalid_binding in [false, true] {
            let (request, authority, mut adapter) = fixture(ExternalPeerRuntime::Codex);
            adapter.invalid_binding = invalid_binding;
            let journal = FakeJournal {
                fail_record: true,
                ..Default::default()
            };
            for _ in 0..2 {
                assert!(
                    dispatch_external_peer_assignment(&authority, &adapter, &journal, &request)
                        .await
                        .is_err()
                );
            }
            assert_eq!(adapter.assignments.load(Ordering::SeqCst), 1);
        }
    }

    #[test]
    fn hydration_checks_provenance_visibility_and_budgets() {
        let (mut request, _, _) = fixture(ExternalPeerRuntime::Codex);
        let entry = medousa_types::TranscriptEntry {
            entry_id: format!("ent_{}", "c".repeat(32)).parse().unwrap(),
            entry_seq: 1,
            caused_by: None,
            source: None,
            content_digest: "sha256:committed".into(),
            turn: medousa_types::ConversationTurn::plain(
                "user",
                "check MCP tools".into(),
                chrono::Utc::now(),
                vec![],
                None,
            ),
        };
        let range = &mut request.context.sources[0];
        range.selection.after_entry_seq = None;
        range.selection.through_entry_seq = 1;
        let reference = medousa_types::TranscriptEntryRef {
            session: range.selection.session.clone(),
            entry_id: entry.entry_id.clone(),
            entry_seq: 1,
        };
        range.selection_digest = medousa_types::coordination::context::conversation_range_digest(
            &range.selection.session,
            [(&reference, entry.content_digest.as_str())],
        );
        let hydrate = |request: &ExternalPeerAssignmentRequest,
                       entry: &medousa_types::TranscriptEntry| {
            context::hydrate_assignment_context(request, |_| true, |_| vec![entry.clone()])
        };
        let rendered = hydrate(&request, &entry).unwrap();
        assert!(rendered.contains("not additional authority"));
        assert!(rendered.contains("check MCP tools"));
        assert!(
            context::hydrate_assignment_context(
                &request,
                |_| false,
                |_| panic!("invisible history must not be loaded")
            )
            .is_err()
        );
        let mut corrupted = entry.clone();
        corrupted.content_digest = "different".into();
        assert!(hydrate(&request, &corrupted).is_err());
        corrupted = entry.clone();
        corrupted.entry_seq = 2;
        assert!(hydrate(&request, &corrupted).is_err());
        corrupted = entry.clone();
        corrupted.turn.content = "x".repeat(context::MAX_PEER_CONTEXT_BYTES);
        assert!(hydrate(&request, &corrupted).is_err());
        request
            .context
            .sources
            .push(request.context.sources[0].clone());
        assert!(hydrate(&request, &entry).is_err());
    }

    fn receipt_fixture() -> (
        tempfile::TempDir,
        store::CoordinationStore,
        ExternalPeerAssignmentRequest,
        medousa_types::coordination::ExternalPeerAssignmentReceipt,
    ) {
        use medousa_types::coordination::*;
        let (temp, store, request) = persisted_fixture();
        store.claim_assignment(&request).unwrap();
        let binding = ExternalPeerAssignmentBinding {
            assignment_id: request.assignment_id.clone(),
            owner_principal_id: request.owner_principal_id.clone(),
            channel: request.channel.clone(),
            target: request.target.clone(),
            execution_session: request.execution_session.clone(),
            agent_session_id: "peer-1".into(),
        };
        store.record_peer(&binding).unwrap();
        let receipt = ExternalPeerAssignmentReceipt {
            receipt_id: store::intake::terminal_receipt_id(&binding),
            binding,
            outcome: PeerAssignmentOutcome::Completed,
            result: "Peer prompt finished; verification not performed.".into(),
        };
        store
            .approve_owner_continuation(&PeerOwnerContinuationGrant {
                request: request.clone(),
                expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
            })
            .unwrap();
        (temp, store, request, receipt)
    }

    #[test]
    fn terminal_receipts_survive_restart_and_dedupe_conflicting_events() {
        let (temp, store, request, receipt) = receipt_fixture();
        assert!(store.record_receipt(&receipt).unwrap());
        assert!(!store.record_receipt(&receipt).unwrap());
        let mut conflict = receipt.clone();
        conflict.outcome = medousa_types::coordination::PeerAssignmentOutcome::Failed;
        assert!(store.record_receipt(&conflict).is_err());
        conflict = receipt.clone();
        conflict.receipt_id.push('x');
        assert!(store.record_receipt(&conflict).is_err());
        conflict = receipt.clone();
        conflict.binding.agent_session_id = "another-peer".into();
        conflict.receipt_id = store::intake::terminal_receipt_id(&conflict.binding);
        assert!(store.record_receipt(&conflict).is_err());
        drop(store);
        let reopened = store::CoordinationStore::open(temp.path()).unwrap();
        assert_eq!(
            reopened
                .pending_owner_receipts(&request.channel, &request.owner_principal_id, 10)
                .unwrap(),
            vec![receipt]
        );
        assert!(
            reopened
                .pending_owner_receipts(&request.channel, "someone-else", 10)
                .is_err()
        );
    }

    #[test]
    fn every_terminal_outcome_is_preserved_even_after_dispatch_revocation() {
        use medousa_types::coordination::PeerAssignmentOutcome::*;
        for outcome in [Completed, Failed, Cancelled, Interrupted] {
            let (_temp, store, request, mut receipt) = receipt_fixture();
            receipt.outcome = outcome;
            store
                .revoke_assignment_grant(&request.channel, &request.execution_grant_id)
                .unwrap();
            assert!(store.record_receipt(&receipt).unwrap());
            assert_eq!(
                store
                    .receipt(&request.channel, &request.assignment_id)
                    .unwrap()
                    .outcome,
                outcome
            );
            assert!(
                store
                    .require_owner_continuation(&receipt, chrono::Utc::now())
                    .is_err()
            );
        }
    }

    #[test]
    fn concurrent_terminals_have_one_durable_winner() {
        let (temp, _store, _, receipt) = receipt_fixture();
        let outcomes = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8)
                .map(|_| {
                    let path = temp.path();
                    let receipt = &receipt;
                    scope.spawn(move || {
                        store::CoordinationStore::open(path)
                            .unwrap()
                            .record_receipt(receipt)
                            .unwrap()
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .collect::<Vec<_>>()
        });
        assert_eq!(outcomes.into_iter().filter(|created| *created).count(), 1);
    }

    #[test]
    fn dispatch_approval_does_not_authorize_owner_continuation() {
        let (_temp, store, request) = persisted_fixture();
        let binding = ExternalPeerAssignmentBinding {
            assignment_id: request.assignment_id.clone(),
            owner_principal_id: request.owner_principal_id.clone(),
            channel: request.channel.clone(),
            target: request.target.clone(),
            execution_session: request.execution_session.clone(),
            agent_session_id: "peer-1".into(),
        };
        store.claim_assignment(&request).unwrap();
        store.record_peer(&binding).unwrap();
        let receipt = medousa_types::coordination::ExternalPeerAssignmentReceipt {
            receipt_id: store::intake::terminal_receipt_id(&binding),
            binding,
            outcome: medousa_types::coordination::PeerAssignmentOutcome::Completed,
            result: "result".into(),
        };
        store.record_receipt(&receipt).unwrap();
        assert!(
            store
                .require_owner_continuation(&receipt, chrono::Utc::now())
                .is_err()
        );
        let lease = store.try_owner_intake_lease(&request).unwrap().unwrap();
        assert!(store.begin_owner_intake(&receipt, &lease).is_err());
    }

    #[test]
    fn owner_intake_requires_committed_receipt_and_current_approval() {
        let (_temp, store, request, receipt) = receipt_fixture();
        assert!(
            store
                .require_owner_continuation(&receipt, chrono::Utc::now())
                .is_err()
        );
        store.record_receipt(&receipt).unwrap();
        store
            .require_owner_continuation(&receipt, chrono::Utc::now())
            .unwrap();
        assert!(
            store
                .require_owner_continuation(
                    &receipt,
                    chrono::Utc::now() + chrono::Duration::hours(2)
                )
                .is_err()
        );
        let mut wrong_grant = medousa_types::coordination::PeerOwnerContinuationGrant {
            request,
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        };
        wrong_grant.request.instructions.push_str(" and deploy");
        assert!(store.approve_owner_continuation(&wrong_grant).is_err());
    }

    #[test]
    fn owner_session_fence_spans_store_instances_and_channels() {
        let (temp, store, request, _) = receipt_fixture();
        let lease = store.try_owner_intake_lease(&request).unwrap().unwrap();
        let reopened = store::CoordinationStore::open(temp.path()).unwrap();
        assert!(reopened.try_owner_intake_lease(&request).unwrap().is_none());
        let mut another = request.clone();
        another.channel.channel_id = "another-channel".into();
        reopened
            .create_channel(&medousa_types::coordination::CoordinationChannelRecord {
                channel: another.channel.clone(),
                owner_principal_id: another.owner_principal_id.clone(),
                member_principal_ids: vec![another.owner_principal_id.clone()],
                attached_sessions: vec![another.owner_session.clone()],
            })
            .unwrap();
        assert!(reopened.try_owner_intake_lease(&another).unwrap().is_none());
        another.owner_session.session_id = "different-owner-session".parse().unwrap();
        assert!(reopened.try_owner_intake_lease(&another).unwrap().is_some());
        drop(lease);
        assert!(reopened.try_owner_intake_lease(&request).unwrap().is_some());
    }

    #[test]
    fn unfinished_intake_survives_restart_without_replaying_the_turn() {
        let (temp, store, request, receipt) = receipt_fixture();
        store.record_receipt(&receipt).unwrap();
        let lease = store.try_owner_intake_lease(&request).unwrap().unwrap();
        let store::intake::OwnerIntakeClaim::Started(intake) =
            store.begin_owner_intake(&receipt, &lease).unwrap()
        else {
            panic!("expected new intake");
        };
        drop(lease);
        drop(store);
        let reopened = store::CoordinationStore::open(temp.path()).unwrap();
        let lease = reopened.try_owner_intake_lease(&request).unwrap().unwrap();
        assert_eq!(
            reopened.begin_owner_intake(&receipt, &lease).unwrap(),
            store::intake::OwnerIntakeClaim::Unresolved(intake)
        );
        assert_eq!(
            reopened
                .pending_owner_receipts(&request.channel, &request.owner_principal_id, 10)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn known_admission_rejection_can_retry_but_an_uncertain_turn_cannot() {
        let (_temp, store, request, receipt) = receipt_fixture();
        store.record_receipt(&receipt).unwrap();
        let lease = store.try_owner_intake_lease(&request).unwrap().unwrap();
        for attempt in 0..8 {
            let store::intake::OwnerIntakeClaim::Started(intake) =
                store.begin_owner_intake(&receipt, &lease).unwrap()
            else {
                panic!("expected fresh attempt");
            };
            assert_eq!(intake.attempt, attempt);
            store.reject_owner_admission(&intake, &lease).unwrap();
        }
        assert!(store.begin_owner_intake(&receipt, &lease).is_err());
    }

    #[test]
    fn only_durable_owner_decision_consumes_receipt() {
        let (temp, store, request, receipt) = receipt_fixture();
        store.record_receipt(&receipt).unwrap();
        let lease = store.try_owner_intake_lease(&request).unwrap().unwrap();
        let store::intake::OwnerIntakeClaim::Started(intake) =
            store.begin_owner_intake(&receipt, &lease).unwrap()
        else {
            panic!("expected intake");
        };
        let mut ack = medousa_types::coordination::PeerOwnerIntakeAcknowledgment {
            intake,
            decision: medousa_types::TranscriptEntryRef {
                session: request.execution_session.clone(),
                entry_id: format!("ent_{}", "c".repeat(32)).parse().unwrap(),
                entry_seq: 1,
            },
            decision_digest: "sha256:decision".into(),
        };
        assert!(store.acknowledge_owner_intake(&ack, &lease).is_err());
        ack.decision.session = request.owner_session.clone();
        assert!(store.acknowledge_owner_intake(&ack, &lease).unwrap());
        assert!(!store.acknowledge_owner_intake(&ack, &lease).unwrap());
        assert!(
            store
                .pending_owner_receipts(&request.channel, &request.owner_principal_id, 10)
                .unwrap()
                .is_empty()
        );
        drop(lease);
        drop(store);
        let reopened = store::CoordinationStore::open(temp.path()).unwrap();
        let lease = reopened.try_owner_intake_lease(&request).unwrap().unwrap();
        assert_eq!(
            reopened.begin_owner_intake(&receipt, &lease).unwrap(),
            store::intake::OwnerIntakeClaim::Consumed(ack)
        );
    }

    #[test]
    fn a_lease_from_another_store_cannot_acknowledge_or_start_intake() {
        let (temp, store, request, receipt) = receipt_fixture();
        store.record_receipt(&receipt).unwrap();
        let lease = store.try_owner_intake_lease(&request).unwrap().unwrap();
        let reopened = store::CoordinationStore::open(temp.path()).unwrap();
        assert!(reopened.begin_owner_intake(&receipt, &lease).is_err());
    }

    #[test]
    fn oversized_receipt_and_corrupt_index_fail_closed() {
        let (temp, store, request, mut receipt) = receipt_fixture();
        receipt.result = "x".repeat(store::intake::MAX_RECEIPT_BYTES + 1);
        assert!(store.record_receipt(&receipt).is_err());
        receipt.result = "done".into();
        store.record_receipt(&receipt).unwrap();
        let path = std::fs::read_dir(temp.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("r1-")
            })
            .unwrap();
        std::fs::write(path, b"corrupt").unwrap();
        assert!(
            store
                .pending_owner_receipts(&request.channel, &request.owner_principal_id, 10)
                .is_err()
        );
        assert!(store.record_receipt(&receipt).is_err());
    }

    #[test]
    fn closing_completed_custody_does_not_replace_terminal_or_wake_twice() {
        let (_temp, store, request, receipt) = receipt_fixture();
        assert!(store.observe_receipt_once(&receipt).unwrap());
        let mut closed = receipt.clone();
        closed.outcome = medousa_types::coordination::PeerAssignmentOutcome::Cancelled;
        closed.result = "custody closed after prompt completion".into();
        assert!(!store.observe_receipt_once(&closed).unwrap());
        assert_eq!(
            store
                .receipt(&request.channel, &request.assignment_id)
                .unwrap(),
            receipt
        );
        closed.receipt_id.push('x');
        assert!(store.observe_receipt_once(&closed).is_err());
    }

    #[test]
    fn recorded_custody_replay_is_a_noop_after_revocation_not_new_authority() {
        let (_temp, store, request, receipt) = receipt_fixture();
        store
            .revoke_assignment_grant(&request.channel, &request.execution_grant_id)
            .unwrap();
        assert!(!store.record_peer(&receipt.binding).unwrap());
        let mut changed = receipt.binding.clone();
        changed.agent_session_id = "new-peer".into();
        assert!(store.record_peer(&changed).is_err());
        assert!(
            store
                .require_assignment_grant(&request, chrono::Utc::now())
                .is_err()
        );
    }

    #[test]
    fn revoked_or_rejected_intake_cannot_consume_the_receipt() {
        for revoke in [false, true] {
            let (_temp, store, request, receipt) = receipt_fixture();
            store.record_receipt(&receipt).unwrap();
            let lease = store.try_owner_intake_lease(&request).unwrap().unwrap();
            let store::intake::OwnerIntakeClaim::Started(intake) =
                store.begin_owner_intake(&receipt, &lease).unwrap()
            else {
                panic!("expected intake");
            };
            if revoke {
                store
                    .revoke_assignment_grant(&request.channel, &request.execution_grant_id)
                    .unwrap();
            } else {
                store.reject_owner_admission(&intake, &lease).unwrap();
            }
            let ack = medousa_types::coordination::PeerOwnerIntakeAcknowledgment {
                intake,
                decision: medousa_types::TranscriptEntryRef {
                    session: request.owner_session.clone(),
                    entry_id: format!("ent_{}", "c".repeat(32)).parse().unwrap(),
                    entry_seq: 1,
                },
                decision_digest: "sha256:decision".into(),
            };
            assert!(store.acknowledge_owner_intake(&ack, &lease).is_err());
            assert_eq!(
                store
                    .pending_owner_receipts(&request.channel, &request.owner_principal_id, 10)
                    .unwrap()
                    .len(),
                1
            );
        }
    }
}
