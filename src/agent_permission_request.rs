//! ACP permission requests — operator approve/deny while the agent session waits.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::Utc;
use once_cell::sync::Lazy;
use tokio::sync::{Mutex as AsyncMutex, oneshot};
use uuid::Uuid;

use medousa_types::{
    AgentPermissionRequestRecord, AgentPermissionRequestStatus,
};

static STORE: Lazy<AgentPermissionRequestStore> = Lazy::new(AgentPermissionRequestStore::new);

pub fn agent_permission_request_store() -> &'static AgentPermissionRequestStore {
    &STORE
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionResolution {
    Approved,
    Denied,
}

pub struct CreateAgentPermissionRequest {
    pub agent_session_id: String,
    pub session_id: String,
    pub runtime: String,
    pub summary: String,
}

struct WaiterState {
    tx: Option<oneshot::Sender<PermissionResolution>>,
}

pub struct AgentPermissionRequestStore {
    records: Mutex<HashMap<String, AgentPermissionRequestRecord>>,
    waiters: AsyncMutex<HashMap<String, WaiterState>>,
}

impl AgentPermissionRequestStore {
    fn new() -> Self {
        Self {
            records: Mutex::new(HashMap::new()),
            waiters: AsyncMutex::new(HashMap::new()),
        }
    }

    pub fn create(&self, input: CreateAgentPermissionRequest) -> AgentPermissionRequestRecord {
        let now = Utc::now();
        let record = AgentPermissionRequestRecord {
            request_id: format!("aperm-{}", Uuid::new_v4()),
            agent_session_id: input.agent_session_id,
            session_id: input.session_id,
            runtime: input.runtime,
            summary: input.summary,
            status: AgentPermissionRequestStatus::Pending,
            created_at_utc: now,
            updated_at_utc: now,
            resolved_at_utc: None,
            resolved_by: None,
        };
        self.records
            .lock()
            .expect("agent permission records")
            .insert(record.request_id.clone(), record.clone());
        record
    }

    pub fn get(&self, request_id: &str) -> Option<AgentPermissionRequestRecord> {
        self.records
            .lock()
            .expect("agent permission records")
            .get(request_id)
            .cloned()
    }

    pub fn list_pending(&self, limit: usize) -> Vec<AgentPermissionRequestRecord> {
        let mut rows: Vec<_> = self
            .records
            .lock()
            .expect("agent permission records")
            .values()
            .filter(|r| r.status == AgentPermissionRequestStatus::Pending)
            .cloned()
            .collect();
        rows.sort_by(|a, b| b.created_at_utc.cmp(&a.created_at_utc));
        rows.truncate(limit.max(1));
        rows
    }

    pub fn list_all(&self, limit: usize) -> Vec<AgentPermissionRequestRecord> {
        let mut rows: Vec<_> = self
            .records
            .lock()
            .expect("agent permission records")
            .values()
            .cloned()
            .collect();
        rows.sort_by(|a, b| b.created_at_utc.cmp(&a.created_at_utc));
        rows.truncate(limit.max(1));
        rows
    }

    pub async fn wait_for_resolution(
        &self,
        request_id: &str,
    ) -> Result<PermissionResolution, String> {
        let (tx, rx) = oneshot::channel();
        {
            let mut waiters = self.waiters.lock().await;
            waiters.insert(
                request_id.to_string(),
                WaiterState { tx: Some(tx) },
            );
        }
        rx.await.map_err(|_| "permission waiter dropped".to_string())
    }

    pub fn approve(
        &self,
        request_id: &str,
        resolved_by: Option<String>,
    ) -> Result<AgentPermissionRequestRecord, String> {
        self.resolve(request_id, AgentPermissionRequestStatus::Approved, resolved_by)
    }

    pub fn deny(
        &self,
        request_id: &str,
        resolved_by: Option<String>,
    ) -> Result<AgentPermissionRequestRecord, String> {
        self.resolve(request_id, AgentPermissionRequestStatus::Denied, resolved_by)
    }

    fn resolve(
        &self,
        request_id: &str,
        status: AgentPermissionRequestStatus,
        resolved_by: Option<String>,
    ) -> Result<AgentPermissionRequestRecord, String> {
        let now = Utc::now();
        let mut records = self.records.lock().expect("agent permission records");
        let record = records
            .get_mut(request_id)
            .ok_or_else(|| format!("permission request not found: {request_id}"))?;
        if record.status != AgentPermissionRequestStatus::Pending {
            return Err(format!(
                "permission request {request_id} is not pending"
            ));
        }
        record.status = status;
        record.updated_at_utc = now;
        record.resolved_at_utc = Some(now);
        record.resolved_by = resolved_by;
        let out = record.clone();
        drop(records);

        let resolution = match status {
            AgentPermissionRequestStatus::Approved => PermissionResolution::Approved,
            _ => PermissionResolution::Denied,
        };
        // Best-effort notify waiter (spawned from async context elsewhere).
        let request_id = request_id.to_string();
        tokio::spawn(async move {
            let mut waiters = STORE.waiters.lock().await;
            if let Some(mut waiter) = waiters.remove(&request_id) {
                if let Some(tx) = waiter.tx.take() {
                    let _ = tx.send(resolution);
                }
            }
        });
        Ok(out)
    }
}

pub type SharedPermissionStore = Arc<AgentPermissionRequestStore>;
