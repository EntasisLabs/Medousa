//! Typed workspace invalidation — incremental projector input.

use crate::workspace::projector::workspace_hub;

/// Source change that maps to one board card (or removal).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum WorkspaceDomainEvent {
    StasisJobChanged { job_id: String },
    TurnWorkerChanged { work_id: String },
    AskJobChanged { job_id: String },
    BudgetRequestChanged { request_id: String },
}

impl WorkspaceDomainEvent {
    pub fn card_id(&self) -> &str {
        match self {
            Self::StasisJobChanged { job_id } => job_id,
            Self::TurnWorkerChanged { work_id } => work_id,
            Self::AskJobChanged { job_id } => job_id,
            Self::BudgetRequestChanged { request_id } => request_id,
        }
    }
}

/// Incremental refresh for a known source record.
pub fn notify_workspace_event(event: WorkspaceDomainEvent) {
    if let Some(hub) = workspace_hub() {
        hub.notify_event(event);
    }
}

/// Unknown or multi-card change — re-project visible cards only (not full Stasis scan).
pub fn notify_workspace_invalidate() {
    if let Some(hub) = workspace_hub() {
        hub.trigger_invalidate();
    }
}

/// Force a full Stasis rescan (startup, rebuild endpoint, explicit reconcile).
pub async fn rebuild_workspace_full() {
    if let Some(hub) = workspace_hub() {
        hub.rebuild_full().await;
    }
}
