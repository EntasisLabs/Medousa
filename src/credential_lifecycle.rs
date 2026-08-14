//! Runtime credential revocation, bounded audit evidence, and stream leases.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::body::Body;
use axum::http::header::CONTENT_TYPE;
use axum::response::Response;
use futures_util::StreamExt;
use serde::Serialize;
use tokio::sync::broadcast;

use crate::request_principal::RequestPrincipal;

const AUDIT_CAPACITY: usize = 256;
const REVOCATION_CHANNEL_CAPACITY: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialKind {
    Pairing,
    LocalApp,
}

#[derive(Clone, Debug)]
pub struct RevocationEvent {
    pub credential_id: Arc<str>,
    pub generation: u64,
    pub kind: CredentialKind,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialAuditEvent {
    pub sequence: u64,
    pub occurred_at_unix_ms: u128,
    pub action: &'static str,
    pub kind: CredentialKind,
    pub credential_id: String,
    pub generation: u64,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialLifecycleSnapshot {
    pub revocation_epoch: u64,
    pub active_leases: usize,
    pub denials: CredentialDenialMetrics,
    pub audit_events: Vec<CredentialAuditEvent>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialDenialMetrics {
    pub authentication_required: u64,
    pub invalid_credential: u64,
    pub forbidden: u64,
}

#[derive(Default)]
struct LifecycleState {
    epoch: u64,
    active_leases: usize,
    revoked_through: HashMap<Arc<str>, u64>,
    audit: VecDeque<CredentialAuditEvent>,
    denials: CredentialDenialMetrics,
}

#[derive(Clone)]
pub struct CredentialLifecycle {
    state: Arc<Mutex<LifecycleState>>,
    revocations: broadcast::Sender<RevocationEvent>,
}

impl Default for CredentialLifecycle {
    fn default() -> Self {
        let (revocations, _) = broadcast::channel(REVOCATION_CHANNEL_CAPACITY);
        Self {
            state: Arc::new(Mutex::new(LifecycleState::default())),
            revocations,
        }
    }
}

impl CredentialLifecycle {
    pub fn revoke(
        &self,
        credential_id: impl Into<Arc<str>>,
        generation: u64,
        kind: CredentialKind,
        reason: impl Into<String>,
    ) {
        let credential_id = credential_id.into();
        let event = RevocationEvent {
            credential_id: credential_id.clone(),
            generation,
            kind,
        };
        {
            let mut state = self.state.lock().expect("credential lifecycle lock");
            state.epoch = state.epoch.saturating_add(1);
            state
                .revoked_through
                .entry(credential_id.clone())
                .and_modify(|known| *known = (*known).max(generation))
                .or_insert(generation);
            let sequence = state.epoch;
            push_audit(
                &mut state.audit,
                CredentialAuditEvent {
                    sequence,
                    occurred_at_unix_ms: now_unix_ms(),
                    action: "revoked",
                    kind,
                    credential_id: credential_id.to_string(),
                    generation,
                    reason: reason.into(),
                },
            );
        }
        let _ = self.revocations.send(event);
    }

    pub fn record_rotation(
        &self,
        credential_id: impl Into<Arc<str>>,
        generation: u64,
        kind: CredentialKind,
    ) {
        let credential_id = credential_id.into();
        let mut state = self.state.lock().expect("credential lifecycle lock");
        state.epoch = state.epoch.saturating_add(1);
        let sequence = state.epoch;
        push_audit(
            &mut state.audit,
            CredentialAuditEvent {
                sequence,
                occurred_at_unix_ms: now_unix_ms(),
                action: "rotated",
                kind,
                credential_id: credential_id.to_string(),
                generation,
                reason: "operator_rotation".to_string(),
            },
        );
    }

    pub fn lease(&self, principal: &RequestPrincipal) -> Option<CredentialLease> {
        Some(CredentialLease {
            lifecycle: self.clone(),
            credential_id: Arc::from(principal.credential_id()?.as_str()),
            generation: principal.revocation_generation(),
        })
    }

    pub fn snapshot(&self) -> CredentialLifecycleSnapshot {
        let state = self.state.lock().expect("credential lifecycle lock");
        CredentialLifecycleSnapshot {
            revocation_epoch: state.epoch,
            active_leases: state.active_leases,
            denials: state.denials.clone(),
            audit_events: state.audit.iter().cloned().collect(),
        }
    }

    pub fn record_denial(&self, reason: &'static str) {
        let mut state = self.state.lock().expect("credential lifecycle lock");
        match reason {
            "authentication_required" => {
                state.denials.authentication_required =
                    state.denials.authentication_required.saturating_add(1);
            }
            "invalid_credential" => {
                state.denials.invalid_credential =
                    state.denials.invalid_credential.saturating_add(1);
            }
            "forbidden" => {
                state.denials.forbidden = state.denials.forbidden.saturating_add(1);
            }
            _ => {}
        }
    }

    fn is_revoked(&self, credential_id: &str, generation: u64) -> bool {
        self.state
            .lock()
            .expect("credential lifecycle lock")
            .revoked_through
            .get(credential_id)
            .is_some_and(|revoked| generation <= *revoked)
    }
}

#[derive(Clone)]
pub struct CredentialLease {
    lifecycle: CredentialLifecycle,
    credential_id: Arc<str>,
    generation: u64,
}

impl CredentialLease {
    pub fn credential_id(&self) -> &str {
        &self.credential_id
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub async fn revoked(&self) {
        self.watcher().revoked().await;
    }

    pub fn watcher(&self) -> CredentialRevocationWatcher {
        CredentialRevocationWatcher {
            lease: self.clone(),
            receiver: self.lifecycle.revocations.subscribe(),
        }
    }

    pub fn wrap_response(&self, response: Response) -> Response {
        let is_sse = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("text/event-stream"));
        if !is_sse {
            return response;
        }
        let (parts, body) = response.into_parts();
        let watcher = self.watcher();
        let guard = ActiveLeaseGuard::new(self.lifecycle.clone());
        let stream = futures_util::stream::unfold(
            (body.into_data_stream(), watcher, guard),
            |(mut body, mut watcher, guard)| async move {
                tokio::select! {
                    _ = watcher.revoked() => None,
                    item = body.next() => item.map(|item| (item, (body, watcher, guard))),
                }
            },
        );
        Response::from_parts(parts, Body::from_stream(stream))
    }
}

pub struct CredentialRevocationWatcher {
    lease: CredentialLease,
    receiver: broadcast::Receiver<RevocationEvent>,
}

impl CredentialRevocationWatcher {
    pub async fn revoked(&mut self) {
        if self
            .lease
            .lifecycle
            .is_revoked(&self.lease.credential_id, self.lease.generation)
        {
            return;
        }
        loop {
            match self.receiver.recv().await {
                Ok(event)
                    if event.credential_id.as_ref() == self.lease.credential_id.as_ref()
                        && self.lease.generation <= event.generation =>
                {
                    return;
                }
                Ok(_) => {}
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    if self
                        .lease
                        .lifecycle
                        .is_revoked(&self.lease.credential_id, self.lease.generation)
                    {
                        return;
                    }
                }
                Err(broadcast::error::RecvError::Closed) => return,
            }
        }
    }
}

struct ActiveLeaseGuard {
    lifecycle: CredentialLifecycle,
}

impl ActiveLeaseGuard {
    fn new(lifecycle: CredentialLifecycle) -> Self {
        lifecycle
            .state
            .lock()
            .expect("credential lifecycle lock")
            .active_leases += 1;
        Self { lifecycle }
    }
}

impl Drop for ActiveLeaseGuard {
    fn drop(&mut self) {
        let mut state = self
            .lifecycle
            .state
            .lock()
            .expect("credential lifecycle lock");
        state.active_leases = state.active_leases.saturating_sub(1);
    }
}

fn push_audit(audit: &mut VecDeque<CredentialAuditEvent>, event: CredentialAuditEvent) {
    if audit.len() == AUDIT_CAPACITY {
        audit.pop_front();
    }
    audit.push_back(event);
}

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use super::*;
    use crate::request_principal::TransportClass;
    use axum::body::Bytes;

    #[tokio::test]
    async fn revocation_closes_matching_generation_only() {
        let lifecycle = CredentialLifecycle::default();
        let old = lifecycle
            .lease(&RequestPrincipal::local_app_with_generation(
                Arc::from("home"),
                TransportClass::Loopback,
                1,
            ))
            .unwrap();
        let current = CredentialLease {
            generation: 2,
            ..old.clone()
        };
        lifecycle.revoke("home", 1, CredentialKind::LocalApp, "test");
        tokio::time::timeout(std::time::Duration::from_millis(50), old.revoked())
            .await
            .expect("old generation closes");
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(20), current.revoked())
                .await
                .is_err()
        );
    }

    #[test]
    fn audit_ring_is_bounded() {
        let lifecycle = CredentialLifecycle::default();
        for generation in 0..(AUDIT_CAPACITY + 10) as u64 {
            lifecycle.revoke("peer", generation, CredentialKind::Pairing, "test");
        }
        let snapshot = lifecycle.snapshot();
        assert_eq!(snapshot.audit_events.len(), AUDIT_CAPACITY);
        assert_eq!(snapshot.revocation_epoch, (AUDIT_CAPACITY + 10) as u64);
    }

    #[tokio::test]
    async fn matching_revocation_terminates_sse_body() {
        let lifecycle = CredentialLifecycle::default();
        let lease = lifecycle
            .lease(&RequestPrincipal::local_app_with_generation(
                Arc::from("home"),
                TransportClass::Loopback,
                1,
            ))
            .unwrap();
        let source = futures_util::stream::once(async {
            Ok::<Bytes, Infallible>(Bytes::from_static(b"data: ready\n\n"))
        })
        .chain(futures_util::stream::pending());
        let response = Response::builder()
            .header(CONTENT_TYPE, "text/event-stream")
            .body(Body::from_stream(source))
            .unwrap();
        let mut body = lease.wrap_response(response).into_body().into_data_stream();
        assert!(body.next().await.is_some());
        assert_eq!(lifecycle.snapshot().active_leases, 1);

        lifecycle.revoke("home", 1, CredentialKind::LocalApp, "test");
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), body.next())
                .await
                .expect("stream closes promptly")
                .is_none()
        );
        assert_eq!(lifecycle.snapshot().active_leases, 0);
    }

    #[test]
    fn denial_metrics_have_fixed_cardinality() {
        let lifecycle = CredentialLifecycle::default();
        lifecycle.record_denial("authentication_required");
        lifecycle.record_denial("authentication_required");
        lifecycle.record_denial("invalid_credential");
        lifecycle.record_denial("attacker_controlled_label");
        let metrics = lifecycle.snapshot().denials;
        assert_eq!(metrics.authentication_required, 2);
        assert_eq!(metrics.invalid_credential, 1);
        assert_eq!(metrics.forbidden, 0);
    }
}
