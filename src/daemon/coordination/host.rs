//! Daemon-composed terminal inbox recovery. Never launches or reattaches a peer
//! process from a disk claim. Owner turns remain explicitly approved and may
//! only prepare follow-up proposals; they cannot dispatch peer work.
use super::{LocalPeerDispatcher, MAX_CONTEXT_BYTES, OwnerIntakeResult};
use crate::{daemon::state::AppState, request_principal::RequestPrincipal};
use anyhow::{Result, bail};
use medousa_acp_client::coordination::store::CoordinationStore;
use medousa_forge::execution::ExecutionClass;
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, OnceLock},
};
use tokio::sync::watch;

static HOST: OnceLock<Arc<LocalPeerDispatcher>> = OnceLock::new();

/// Internal service composition; model intent tools must authenticate the admitted
/// owner turn and cannot use this getter to infer operator approval.
pub fn local_coordination_host() -> Option<Arc<LocalPeerDispatcher>> {
    HOST.get().cloned()
}

pub async fn start_local_coordination_host(
    state: AppState,
    runtime_id: String,
    shutdown: watch::Receiver<bool>,
) -> Result<tokio::task::JoinHandle<()>> {
    if HOST.get().is_some() {
        bail!("local coordination host already composed");
    }
    let store = state
        .forge_execution
        .run(ExecutionClass::StoreIo, MAX_CONTEXT_BYTES, || {
            Ok(CoordinationStore::open(
                &crate::paths::medousa_data_dir().join("coordination"),
            ))
        })
        .await??;
    let host = Arc::new(LocalPeerDispatcher::new(
        state,
        runtime_id,
        Arc::new(store),
    )?);
    HOST.set(host.clone())
        .map_err(|_| anyhow::anyhow!("coordination host composition race"))?;
    Ok(tokio::spawn(run_host(host, shutdown)))
}

async fn run_host(host: Arc<LocalPeerDispatcher>, mut shutdown: watch::Receiver<bool>) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut workers = tokio::task::JoinSet::new();
    let mut active = HashSet::new();
    let mut reconcile = HashSet::new();
    let mut retry_after = HashMap::new();
    let mut cursor: Option<String> = None;
    loop {
        if *shutdown.borrow() {
            break;
        }
        tokio::select! {
            changed = shutdown.changed() => { if changed.is_err() || *shutdown.borrow() { break; } },
            _ = interval.tick() => {},
            _ = host.wake.notified() => { cursor = None; },
            finished = workers.join_next(), if !workers.is_empty() => {
                match finished {
                    Some(Ok((id, outcome))) => {
                        active.remove(&id);
                        match outcome {
                            Ok(OwnerIntakeResult::NeedsReconciliation) => { tracing::warn!(receipt_id = %id, "owner intake retained for explicit reconciliation"); reconcile.insert(id); },
                            Ok(OwnerIntakeResult::DeferredBusy) => { retry_after.insert(id, tokio::time::Instant::now() + std::time::Duration::from_secs(30)); },
                            Ok(_) => {},
                            Err(error) => { tracing::warn!(receipt_id = %id, %error, "owner receipt retained; recovery approval/visibility unavailable"); retry_after.insert(id, tokio::time::Instant::now() + std::time::Duration::from_secs(300)); },
                        }
                    },
                    Some(Err(error)) => { tracing::error!(%error, "owner intake worker lost; retaining its in-process fence until restart"); },
                    None => {},
                }
            },
        }
        if workers.len() >= 4 {
            continue;
        }
        let store = host.store.clone();
        let runtime_id = host.local_runtime_id.clone();
        let after = cursor.clone();
        let authority = match crate::workshop_authority::current() {
            Ok(authority) => authority.clone(),
            Err(error) => {
                tracing::error!(%error, "coordination recovery authority unavailable");
                break;
            }
        };
        let pending = host
            .state
            .forge_execution
            .run(
                ExecutionClass::StoreIo,
                medousa_forge::execution::MAX_STORE_PAYLOAD_BYTES,
                move || {
                    Ok(store.pending_local_owner_receipts(
                        &authority,
                        &runtime_id,
                        8,
                        after.as_deref(),
                    ))
                },
            )
            .await;
        let receipts = match pending {
            Ok(Ok(receipts)) => receipts,
            other => {
                tracing::warn!(error = ?other, "coordination inbox recovery scan failed closed");
                continue;
            }
        };
        if *shutdown.borrow() {
            break;
        }
        // Rotate through the bounded inbox: blocked receipts must not starve
        // later pages. Reconciliation fences survive pagination until restart.
        if receipts.is_empty() {
            cursor = None;
        }
        retry_after.retain(|_, when| *when > tokio::time::Instant::now());
        for receipt in receipts {
            let id = receipt.receipt_id.clone();
            if workers.len() >= 4 {
                break;
            }
            cursor = Some(id.clone());
            if active.contains(&id)
                || reconcile.contains(&id)
                || retry_after
                    .get(&id)
                    .is_some_and(|when| *when > tokio::time::Instant::now())
            {
                continue;
            }
            active.insert(id.clone());
            let host = host.clone();
            workers.spawn(async move {
                let principal =
                    RequestPrincipal::continuation(receipt.binding.owner_principal_id.clone());
                let result = host
                    .resume_owner_intake(
                        &principal,
                        receipt.binding.channel,
                        &receipt.binding.assignment_id,
                    )
                    .await;
                (id, result)
            });
        }
    }
    workers.abort_all();
    while workers.join_next().await.is_some() {}
}
