//! Turn tool-round budget extension requests — operator approve/deny while the loop waits.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex as AsyncMutex, oneshot};
use uuid::Uuid;

use crate::session;

const BUDGET_REQUESTS_FILE: &str = "workspace/turn_budget_requests.json";

pub const MAX_REQUESTED_ROUNDS_PER_ASK: usize = 8;
pub const MAX_APPROVALS_PER_TURN: usize = 2;
pub const ABSOLUTE_MAX_TOOL_ROUNDS: usize = 32;

static STORE: Lazy<TurnBudgetRequestStore> = Lazy::new(TurnBudgetRequestStore::new);

pub fn turn_budget_request_store() -> &'static TurnBudgetRequestStore {
    &STORE
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnBudgetRequestStatus {
    Pending,
    Approved,
    Denied,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnBudgetRequest {
    pub request_id: String,
    pub turn_correlation_id: Option<String>,
    pub stream_turn_id: u64,
    pub session_id: String,
    pub channel: Option<String>,
    pub rounds_executed: usize,
    pub max_tool_rounds: usize,
    pub requested_rounds: usize,
    pub granted_rounds: Option<usize>,
    pub reason: String,
    pub progress_summary: Option<String>,
    pub status: TurnBudgetRequestStatus,
    pub resolved_by: Option<String>,
    pub created_at_utc: DateTime<Utc>,
    pub updated_at_utc: DateTime<Utc>,
    pub resolved_at_utc: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetResolution {
    Approved { granted_rounds: usize },
    Denied,
}

pub struct CreateTurnBudgetRequest {
    pub turn_correlation_id: Option<String>,
    pub stream_turn_id: u64,
    pub session_id: Option<String>,
    pub channel: Option<String>,
    pub rounds_executed: usize,
    pub max_tool_rounds: usize,
    pub requested_rounds: usize,
    pub reason: String,
    pub progress_summary: Option<String>,
}

struct WaiterState {
    tx: Option<oneshot::Sender<BudgetResolution>>,
}

pub struct TurnBudgetRequestStore {
    records: Mutex<HashMap<String, TurnBudgetRequest>>,
    waiters: Arc<AsyncMutex<HashMap<String, WaiterState>>>,
}

impl TurnBudgetRequestStore {
    fn new() -> Self {
        let store = Self {
            records: Mutex::new(HashMap::new()),
            waiters: Arc::new(AsyncMutex::new(HashMap::new())),
        };
        store.reload_from_disk();
        store
    }

    fn path() -> PathBuf {
        session::medousa_data_dir().join(
            BUDGET_REQUESTS_FILE
                .strip_prefix("workspace/")
                .unwrap_or(BUDGET_REQUESTS_FILE),
        )
    }

    fn reload_from_disk(&self) {
        let _ = fs::create_dir_all(session::medousa_data_dir().join("workspace"));
        let Ok(raw) = fs::read_to_string(Self::path()) else {
            return;
        };
        let Ok(mut map) = serde_json::from_str::<HashMap<String, TurnBudgetRequest>>(&raw) else {
            return;
        };
        let mut changed = false;
        for record in map.values_mut() {
            if record.status == TurnBudgetRequestStatus::Pending {
                record.status = TurnBudgetRequestStatus::Expired;
                record.updated_at_utc = Utc::now();
                record.resolved_at_utc = Some(Utc::now());
                changed = true;
            }
        }
        if changed {
            let _ = Self::write_map(&map);
        }
        *self.records.lock().expect("turn budget records") = map;
    }

    fn write_map(map: &HashMap<String, TurnBudgetRequest>) -> std::io::Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let body = serde_json::to_string_pretty(map)?;
        fs::write(path, body)
    }

    fn persist(&self) {
        let snapshot = self.records.lock().expect("turn budget records").clone();
        let _ = Self::write_map(&snapshot);
    }

    pub fn approvals_for_turn(&self, turn_correlation_id: Option<&str>) -> usize {
        let Some(turn_id) = turn_correlation_id.filter(|value| !value.trim().is_empty()) else {
            return 0;
        };
        self.records
            .lock()
            .expect("turn budget records")
            .values()
            .filter(|record| {
                record.turn_correlation_id.as_deref() == Some(turn_id)
                    && record.status == TurnBudgetRequestStatus::Approved
            })
            .count()
    }

    pub async fn create_and_register_wait(
        &self,
        input: CreateTurnBudgetRequest,
    ) -> Result<(String, oneshot::Receiver<BudgetResolution>), String> {
        if self.approvals_for_turn(input.turn_correlation_id.as_deref()) >= MAX_APPROVALS_PER_TURN {
            return Err(format!(
                "turn already received {MAX_APPROVALS_PER_TURN} round extensions this turn"
            ));
        }

        let requested_rounds = input.requested_rounds.clamp(1, MAX_REQUESTED_ROUNDS_PER_ASK);
        if input.max_tool_rounds >= ABSOLUTE_MAX_TOOL_ROUNDS {
            return Err(format!(
                "turn is already at the absolute max tool rounds ({ABSOLUTE_MAX_TOOL_ROUNDS})"
            ));
        }

        let request_id = Uuid::new_v4().simple().to_string();
        let now = Utc::now();
        let record = TurnBudgetRequest {
            request_id: request_id.clone(),
            turn_correlation_id: input.turn_correlation_id,
            stream_turn_id: input.stream_turn_id,
            session_id: input
                .session_id
                .unwrap_or_else(|| "default".to_string()),
            channel: input.channel,
            rounds_executed: input.rounds_executed,
            max_tool_rounds: input.max_tool_rounds,
            requested_rounds,
            granted_rounds: None,
            reason: input.reason,
            progress_summary: input.progress_summary,
            status: TurnBudgetRequestStatus::Pending,
            resolved_by: None,
            created_at_utc: now,
            updated_at_utc: now,
            resolved_at_utc: None,
        };

        {
            let mut guard = self.records.lock().expect("turn budget records");
            guard.insert(request_id.clone(), record);
        }
        self.persist();
        notify_budget_request_changed(&request_id);

        let (tx, rx) = oneshot::channel();
        self.waiters
            .lock()
            .await
            .insert(request_id.clone(), WaiterState { tx: Some(tx) });

        Ok((request_id, rx))
    }

    pub async fn wait_for_resolution(
        &self,
        request_id: &str,
        rx: oneshot::Receiver<BudgetResolution>,
    ) -> BudgetResolution {
        match rx.await {
            Ok(resolution) => resolution,
            Err(_) => {
                let _ = self.expire(request_id, "waiter dropped");
                BudgetResolution::Denied
            }
        }
    }

    fn signal_waiter(&self, request_id: &str, resolution: BudgetResolution) {
        if let Ok(mut guard) = self.waiters.try_lock() {
            if let Some(waiter) = guard.remove(request_id) {
                if let Some(tx) = waiter.tx {
                    let _ = tx.send(resolution);
                }
            }
        } else {
            let store = Arc::clone(&self.waiters);
            let request_id = request_id.to_string();
            tokio::spawn(async move {
                if let Some(waiter) = store.lock().await.remove(&request_id) {
                    if let Some(tx) = waiter.tx {
                        let _ = tx.send(resolution);
                    }
                }
            });
        }
    }

    pub fn get(&self, request_id: &str) -> Option<TurnBudgetRequest> {
        self.records
            .lock()
            .expect("turn budget records")
            .get(request_id)
            .cloned()
    }

    pub fn list_pending(&self, limit: usize) -> Vec<TurnBudgetRequest> {
        let limit = limit.clamp(1, 200);
        let mut rows: Vec<_> = self
            .records
            .lock()
            .expect("turn budget records")
            .values()
            .filter(|record| record.status == TurnBudgetRequestStatus::Pending)
            .cloned()
            .collect();
        rows.sort_by(|left, right| right.updated_at_utc.cmp(&left.updated_at_utc));
        rows.truncate(limit);
        rows
    }

    pub fn list_for_workspace(&self, include_terminal: bool) -> Vec<TurnBudgetRequest> {
        self.records
            .lock()
            .expect("turn budget records")
            .values()
            .filter(|record| {
                include_terminal || record.status == TurnBudgetRequestStatus::Pending
            })
            .cloned()
            .collect()
    }

    pub fn approve(
        &self,
        request_id: &str,
        extra_rounds: Option<usize>,
        resolved_by: Option<String>,
    ) -> Result<TurnBudgetRequest, String> {
        let mut guard = self.records.lock().expect("turn budget records");
        let Some(record) = guard.get_mut(request_id) else {
            return Err(format!("budget request not found: {request_id}"));
        };
        if record.status != TurnBudgetRequestStatus::Pending {
            return Err(format!(
                "budget request {} is not pending (status={:?})",
                request_id, record.status
            ));
        }

        let requested = record.requested_rounds.min(MAX_REQUESTED_ROUNDS_PER_ASK);
        let granted = extra_rounds.unwrap_or(requested).clamp(1, MAX_REQUESTED_ROUNDS_PER_ASK);
        let new_max = record
            .max_tool_rounds
            .saturating_add(granted)
            .min(ABSOLUTE_MAX_TOOL_ROUNDS);
        let granted = new_max.saturating_sub(record.max_tool_rounds).max(1);

        record.status = TurnBudgetRequestStatus::Approved;
        record.granted_rounds = Some(granted);
        record.resolved_by = resolved_by;
        record.updated_at_utc = Utc::now();
        record.resolved_at_utc = Some(record.updated_at_utc);
        let updated = record.clone();
        drop(guard);
        self.persist();
        notify_budget_request_changed(request_id);
        self.signal_waiter(
            request_id,
            BudgetResolution::Approved {
                granted_rounds: granted,
            },
        );
        Ok(updated)
    }

    pub fn deny(
        &self,
        request_id: &str,
        resolved_by: Option<String>,
    ) -> Result<TurnBudgetRequest, String> {
        let mut guard = self.records.lock().expect("turn budget records");
        let Some(record) = guard.get_mut(request_id) else {
            return Err(format!("budget request not found: {request_id}"));
        };
        if record.status != TurnBudgetRequestStatus::Pending {
            return Err(format!(
                "budget request {} is not pending (status={:?})",
                request_id, record.status
            ));
        }
        record.status = TurnBudgetRequestStatus::Denied;
        record.resolved_by = resolved_by;
        record.updated_at_utc = Utc::now();
        record.resolved_at_utc = Some(record.updated_at_utc);
        let updated = record.clone();
        drop(guard);
        self.persist();
        notify_budget_request_changed(request_id);
        self.signal_waiter(request_id, BudgetResolution::Denied);
        Ok(updated)
    }

    fn expire(&self, request_id: &str, _reason: &str) -> Result<(), String> {
        let mut guard = self.records.lock().expect("turn budget records");
        let Some(record) = guard.get_mut(request_id) else {
            return Err(format!("budget request not found: {request_id}"));
        };
        if record.status != TurnBudgetRequestStatus::Pending {
            return Ok(());
        }
        record.status = TurnBudgetRequestStatus::Expired;
        record.updated_at_utc = Utc::now();
        record.resolved_at_utc = Some(record.updated_at_utc);
        drop(guard);
        self.persist();
        notify_budget_request_changed(request_id);
        Ok(())
    }
}

fn notify_budget_request_changed(request_id: &str) {
    crate::workspace::domain_event::notify_workspace_event(
        crate::workspace::domain_event::WorkspaceDomainEvent::BudgetRequestChanged {
            request_id: request_id.to_string(),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn approve_grants_capped_rounds() {
        let store = TurnBudgetRequestStore {
            records: Mutex::new(HashMap::new()),
            waiters: Arc::new(AsyncMutex::new(HashMap::new())),
        };
        let (request_id, _rx) = store
            .create_and_register_wait(CreateTurnBudgetRequest {
                turn_correlation_id: Some("turn-1".to_string()),
                stream_turn_id: 1,
                session_id: Some("sess".to_string()),
                channel: Some("home-ios".to_string()),
                rounds_executed: 4,
                max_tool_rounds: 8,
                requested_rounds: 5,
                reason: "need more MCP".to_string(),
                progress_summary: Some("half done".to_string()),
            })
            .await
            .expect("create");
        let updated = store
            .approve(&request_id, None, Some("operator".to_string()))
            .expect("approve");
        assert_eq!(updated.status, TurnBudgetRequestStatus::Approved);
        assert_eq!(updated.granted_rounds, Some(5));
    }
}
