//! Restart-safe retrieval of committed remote answers for desktop coordinators.
//! Queries never dispatch work or start another owner model turn.

use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::{Context, Result, bail};
use medousa_types::AuthorityId;
use tokio::{sync::watch, time::Instant};

use crate::{
    daemon::state::AppState,
    daemon_worker::DaemonWorkerPairing,
    peer_completion_delivery::{apply_remote_peer_completion, completion_execution_service},
    peer_coordination_mesh::{
        RemotePeerOriginAssociation, mark_remote_peer_origin_applied_admitted,
        next_remote_peer_origin_sync_page_admitted, record_remote_peer_origin_response_admitted,
    },
    session_store::SessionStore,
    user_profiles::UserProfileRegistry,
};

const PAGE_SIZE: usize = 4;
const QUERY_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Debug, Default)]
pub(crate) struct PullReport {
    pub checked: usize,
    pub applied: usize,
    pub pending: usize,
    pub unavailable: usize,
}

pub(crate) struct CompletionPuller {
    pairing: Arc<DaemonWorkerPairing>,
    store: Arc<dyn SessionStore>,
    authority: AuthorityId,
    profiles: Arc<std::sync::RwLock<UserProfileRegistry>>,
    retry_after: HashMap<String, (Instant, u32)>,
}

impl CompletionPuller {
    pub(crate) fn new(
        pairing: Arc<DaemonWorkerPairing>,
        store: Arc<dyn SessionStore>,
        authority: AuthorityId,
        profiles: Arc<std::sync::RwLock<UserProfileRegistry>>,
    ) -> Self {
        Self {
            pairing,
            store,
            authority,
            profiles,
            retry_after: HashMap::new(),
        }
    }

    fn owner_available(&self, origin: &RemotePeerOriginAssociation) -> bool {
        self.profiles
            .read()
            .expect("profile registry lock")
            .list_profiles()
            .iter()
            .any(|profile| profile.profile_id == origin.owner_profile_id)
    }

    pub(crate) async fn pull_origin(
        &self,
        mut origin: RemotePeerOriginAssociation,
    ) -> Result<Option<bool>> {
        if origin.request.source_execution.authority_id != self.authority
            || !self.owner_available(&origin)
        {
            bail!("completion source authority or owner profile is unavailable");
        }
        let source_session = origin.request.owner_session_id.clone();
        let owner_profile = origin.owner_profile_id.clone();
        let visible = completion_execution_service()
            .run(
                medousa_forge::execution::ExecutionClass::StoreIo,
                1024,
                move || {
                    Ok(crate::session_catalog::session_visible_to_profile(
                        source_session.as_str(),
                        &owner_profile,
                    ))
                },
            )
            .await?;
        if !visible {
            bail!("completion source chat is no longer visible to its owner");
        }
        let response =
            tokio::time::timeout(QUERY_TIMEOUT, self.pairing.query_peer_completion(&origin))
                .await
                .context("remote completion query timed out")??;
        if let Some(proposal) = response.proposal.as_ref() {
            origin = record_remote_peer_origin_response_admitted(
                &origin.target_device_id,
                &origin.request,
                proposal,
            )
            .await?;
        }
        let Some(completion) = response.completion else {
            return Ok(None);
        };
        if !self.owner_available(&origin) {
            bail!("completion owner profile is no longer available");
        }
        let appended = apply_remote_peer_completion(
            self.store.clone(),
            &origin,
            &completion,
            &self.authority,
            &origin.owner_profile_id,
        )
        .await?;
        // A crash after append but before this mark is repaired by the append's
        // deterministic correlation on the next sweep, without duplicate text.
        mark_remote_peer_origin_applied_admitted(
            &origin.source_device_id,
            &origin.target_device_id,
            &origin.request_digest,
            &completion.proposal_id,
        )
        .await?;
        Ok(Some(appended))
    }

    pub(crate) async fn poll_once(&mut self) -> Result<PullReport> {
        let origins = next_remote_peer_origin_sync_page_admitted(PAGE_SIZE).await?;
        let mut report = PullReport::default();
        let mut queries = Vec::new();
        for origin in origins {
            report.checked += 1;
            let key = format!("{}:{}", origin.target_device_id, origin.request_digest);
            if self
                .retry_after
                .get(&key)
                .is_some_and(|(until, _)| *until > Instant::now())
            {
                report.pending += 1;
                continue;
            }
            let puller = &*self;
            queries.push(async move { (key, puller.pull_origin(origin).await) });
        }
        // Four bounded queries run independently: one offline workshop cannot
        // hold the other nodes behind its network deadline.
        for (key, outcome) in futures_util::future::join_all(queries).await {
            match outcome {
                Ok(Some(appended)) => {
                    self.retry_after.remove(&key);
                    report.applied += usize::from(appended);
                }
                Ok(None) => {
                    self.defer(key, false);
                    report.pending += 1;
                }
                Err(error) => {
                    self.defer(key, true);
                    report.unavailable += 1;
                    tracing::warn!(%error, "remote completion retained for retry");
                }
            }
        }
        Ok(report)
    }

    fn defer(&mut self, key: String, unavailable: bool) {
        let attempts = self
            .retry_after
            .get(&key)
            .map_or(1, |(_, attempts)| attempts.saturating_add(1));
        let delay = retry_delay(attempts, unavailable);
        self.retry_after
            .insert(key, (Instant::now() + delay, attempts));
    }
}

fn retry_delay(attempts: u32, unavailable: bool) -> Duration {
    let (base, exponent, cap) = if unavailable {
        (5u64, 6, 300)
    } else {
        (15u64, 4, 240)
    };
    Duration::from_secs(base.saturating_mul(1u64 << attempts.min(exponent)).min(cap))
}

pub fn start(
    state: AppState,
    mut shutdown: watch::Receiver<bool>,
) -> Result<tokio::task::JoinHandle<()>> {
    let mut puller = CompletionPuller::new(
        state.daemon_workers,
        crate::session_store::get_session_store(),
        crate::workshop_authority::current()
            .map_err(anyhow::Error::msg)?
            .clone(),
        state.profile_registry,
    );
    Ok(tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(15));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            if *shutdown.borrow() {
                break;
            }
            tokio::select! {
                changed = shutdown.changed() => { if changed.is_err() || *shutdown.borrow() { break; } }
                _ = interval.tick() => {
                    tokio::select! {
                        changed = shutdown.changed() => { if changed.is_err() || *shutdown.borrow() { break; } }
                        report = puller.poll_once() => {
                            match report {
                                Ok(report) if report.applied > 0 => tracing::info!(applied = report.applied, "remote answers committed to source chats"),
                                Ok(_) => {},
                                Err(error) => tracing::warn!(%error, "remote completion sweep deferred"),
                            }
                        }
                    }
                }
            }
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn backoff_is_bounded_for_pending_and_unavailable_workshops() {
        assert_eq!(retry_delay(1, false), Duration::from_secs(30));
        assert_eq!(retry_delay(1, true), Duration::from_secs(10));
        assert_eq!(retry_delay(u32::MAX, false), Duration::from_secs(240));
        assert_eq!(retry_delay(u32::MAX, true), Duration::from_secs(300));
    }
}
