//! Transport-independent peer admission seam. Not yet wired to production ACP.
//!
//! Destination authorization and context visibility must be checked immediately
//! before dispatch. Persistence/idempotent command claiming belongs to the host,
//! not this port: calling it twice may execute twice.

use anyhow::{Result, bail};
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

pub async fn admit_external_peer_assignment(
    authority: &dyn PeerAssignmentAuthority,
    adapter: &dyn ExternalPeerExecutionPort,
    request: &ExternalPeerAssignmentRequest,
) -> Result<ExternalPeerAssignmentBinding> {
    for value in [
        &request.assignment_id,
        &request.idempotency_key,
        &request.owner_principal_id,
        &request.channel.channel_id,
        &request.target.execution_runtime_id,
        &request.instructions,
        &request.execution_grant_id,
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
    // Authorize before discovery so an unauthorized caller cannot enumerate hosts.
    authority.authorize(request).await?;
    let candidates = adapter.discover().await?;
    let candidate = candidates
        .iter()
        .find(|candidate| candidate.target == request.target)
        .ok_or_else(|| anyhow::anyhow!("requested external peer target is not available"))?;
    if let PeerAvailability::Unavailable { reason } = &candidate.availability {
        bail!("requested external peer target is unavailable: {reason}");
    }
    // Discovery may await I/O: revocation during discovery must block dispatch.
    authority.authorize(request).await?;
    let binding = adapter.assign(request).await?;
    if binding.assignment_id != request.assignment_id
        || binding.owner_principal_id != request.owner_principal_id
        || binding.channel != request.channel
        || binding.target != request.target
        || binding.execution_session != request.execution_session
        || binding.agent_session_id.trim().is_empty()
    {
        // Dispatch already occurred. Host must reconcile this uncertain outcome;
        // never automatically retry on this error.
        bail!(
            "external peer adapter returned an invalid assignment binding; reconcile before retry"
        );
    }
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
}
