//! ACP permission requests — operator approve/deny while the agent session waits.

use std::cmp::Reverse;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::Utc;
use once_cell::sync::Lazy;
use tokio::sync::{Mutex as AsyncMutex, oneshot};
use uuid::Uuid;

use medousa_types::{
    AgentPermissionOption, AgentPermissionRequestRecord, AgentPermissionRequestStatus,
};

static STORE: Lazy<AgentPermissionRequestStore> = Lazy::new(AgentPermissionRequestStore::new);

pub fn agent_permission_request_store() -> &'static AgentPermissionRequestStore {
    &STORE
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionResolution {
    Selected(String),
    Approved,
    Denied,
}

pub struct CreateAgentPermissionRequest {
    pub agent_session_id: String,
    pub session_id: String,
    pub runtime: String,
    pub summary: String,
    pub options: Vec<AgentPermissionOption>,
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
            options: input.options,
            selected_option_id: None,
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

    pub fn deny_for_session(&self, agent_session_id: &str) {
        let ids: Vec<_> = self
            .records
            .lock()
            .expect("agent permission records")
            .values()
            .filter(|record| {
                record.agent_session_id == agent_session_id
                    && record.status == AgentPermissionRequestStatus::Pending
            })
            .map(|record| record.request_id.clone())
            .collect();
        for id in ids {
            let _ = self.deny(&id, Some("agent_session_ended".into()));
        }
    }

    pub fn get(&self, request_id: &str) -> Option<AgentPermissionRequestRecord> {
        self.records
            .lock()
            .expect("agent permission records")
            .get(request_id)
            .cloned()
    }

    pub(crate) fn has_pending_for_session(&self, agent_session_id: &str, session_id: &str) -> bool {
        self.records
            .lock()
            .expect("agent permission records")
            .values()
            .any(|record| {
                record.agent_session_id == agent_session_id
                    && record.session_id == session_id
                    && record.status == AgentPermissionRequestStatus::Pending
            })
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
        rows.sort_by_key(|row| Reverse(row.created_at_utc));
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
        rows.sort_by_key(|row| Reverse(row.created_at_utc));
        rows.truncate(limit.max(1));
        rows
    }

    fn permission_timeout_secs() -> u64 {
        std::env::var("MEDOUSA_ACP_PERMISSION_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(300)
    }

    pub async fn wait_for_resolution(
        &self,
        request_id: &str,
    ) -> Result<PermissionResolution, String> {
        let (tx, rx) = oneshot::channel();
        {
            let mut waiters = self.waiters.lock().await;
            waiters.insert(request_id.to_string(), WaiterState { tx: Some(tx) });
        }
        if let Some(record) = self.get(request_id)
            && record.status != AgentPermissionRequestStatus::Pending
        {
            self.waiters.lock().await.remove(request_id);
            return Ok(record
                .selected_option_id
                .map(PermissionResolution::Selected)
                .unwrap_or_else(|| {
                    if record.status == AgentPermissionRequestStatus::Approved {
                        PermissionResolution::Approved
                    } else {
                        PermissionResolution::Denied
                    }
                }));
        }
        let timeout = std::time::Duration::from_secs(Self::permission_timeout_secs());
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(resolution)) => Ok(resolution),
            Ok(Err(_)) => {
                let _ = self.expire(request_id, Some("waiter_dropped".into()));
                Ok(PermissionResolution::Denied)
            }
            Err(_) => {
                let _ = self.expire(request_id, Some("timeout".into()));
                Ok(PermissionResolution::Denied)
            }
        }
    }

    pub fn select_option(
        &self,
        request_id: &str,
        option_id: String,
        resolved_by: Option<String>,
    ) -> Result<AgentPermissionRequestRecord, String> {
        self.resolve(
            request_id,
            AgentPermissionRequestStatus::Approved,
            resolved_by,
            Some(option_id),
        )
    }

    pub fn approve(
        &self,
        request_id: &str,
        resolved_by: Option<String>,
    ) -> Result<AgentPermissionRequestRecord, String> {
        self.resolve(
            request_id,
            AgentPermissionRequestStatus::Approved,
            resolved_by,
            None,
        )
    }

    pub fn deny(
        &self,
        request_id: &str,
        resolved_by: Option<String>,
    ) -> Result<AgentPermissionRequestRecord, String> {
        self.resolve(
            request_id,
            AgentPermissionRequestStatus::Denied,
            resolved_by,
            None,
        )
    }

    pub fn expire(
        &self,
        request_id: &str,
        resolved_by: Option<String>,
    ) -> Result<AgentPermissionRequestRecord, String> {
        self.resolve(
            request_id,
            AgentPermissionRequestStatus::Expired,
            resolved_by,
            None,
        )
    }

    fn resolve(
        &self,
        request_id: &str,
        status: AgentPermissionRequestStatus,
        resolved_by: Option<String>,
        option_id: Option<String>,
    ) -> Result<AgentPermissionRequestRecord, String> {
        let now = Utc::now();
        let mut records = self.records.lock().expect("agent permission records");
        let record = records
            .get_mut(request_id)
            .ok_or_else(|| format!("permission request not found: {request_id}"))?;
        if record.status != AgentPermissionRequestStatus::Pending {
            return Err(format!("permission request {request_id} is not pending"));
        }
        let selected = if let Some(id) = option_id {
            Some(
                record
                    .options
                    .iter()
                    .find(|option| option.option_id == id)
                    .ok_or_else(|| "invalid agent permission option".to_string())?
                    .clone(),
            )
        } else if status == AgentPermissionRequestStatus::Approved && !record.options.is_empty() {
            let allows: Vec<_> = record
                .options
                .iter()
                .filter(|option| option.kind.starts_with("allow"))
                .collect();
            if allows.len() != 1 {
                return Err("choose an explicit agent permission option".into());
            }
            Some(allows[0].clone())
        } else {
            None
        };
        let status = if selected
            .as_ref()
            .is_some_and(|option| option.kind.starts_with("reject"))
        {
            AgentPermissionRequestStatus::Denied
        } else {
            status
        };
        record.selected_option_id = selected.map(|option| option.option_id);
        record.status = status;
        record.updated_at_utc = now;
        record.resolved_at_utc = Some(now);
        record.resolved_by = resolved_by;
        let out = record.clone();
        drop(records);

        let resolution = out
            .selected_option_id
            .clone()
            .map(PermissionResolution::Selected)
            .unwrap_or_else(|| match status {
                AgentPermissionRequestStatus::Approved => PermissionResolution::Approved,
                _ => PermissionResolution::Denied,
            });
        // Best-effort notify waiter (spawned from async context elsewhere).
        let request_id = request_id.to_string();
        tokio::spawn(async move {
            let mut waiters = STORE.waiters.lock().await;
            if let Some(mut waiter) = waiters.remove(&request_id)
                && let Some(tx) = waiter.tx.take()
            {
                let _ = tx.send(resolution);
            }
        });
        Ok(out)
    }
}

pub type SharedPermissionStore = Arc<AgentPermissionRequestStore>;

#[cfg(test)]
mod tests {
    use super::*;

    fn request(store: &AgentPermissionRequestStore) -> AgentPermissionRequestRecord {
        store.create(CreateAgentPermissionRequest {
            agent_session_id: "agent-test".into(),
            session_id: "chat-test".into(),
            runtime: "pi".into(),
            summary: "Choose an option".into(),
            options: vec![
                AgentPermissionOption {
                    option_id: "first".into(),
                    name: "First".into(),
                    kind: "allow_once".into(),
                },
                AgentPermissionOption {
                    option_id: "second".into(),
                    name: "Second".into(),
                    kind: "allow_once".into(),
                },
                AgentPermissionOption {
                    option_id: "reject".into(),
                    name: "Reject".into(),
                    kind: "reject_once".into(),
                },
            ],
        })
    }

    #[tokio::test]
    async fn exact_choice_survives_resolution_before_wait_registration() {
        let store = AgentPermissionRequestStore::new();
        let record = request(&store);
        let selected = store
            .select_option(&record.request_id, "second".into(), None)
            .unwrap();
        assert_eq!(selected.selected_option_id.as_deref(), Some("second"));
        assert_eq!(
            store.wait_for_resolution(&record.request_id).await.unwrap(),
            PermissionResolution::Selected("second".into())
        );
    }

    #[tokio::test]
    async fn arbitrary_choices_are_never_guessed_and_invalid_choice_keeps_request_pending() {
        let store = AgentPermissionRequestStore::new();
        let record = request(&store);
        assert!(store.approve(&record.request_id, None).is_err());
        assert!(
            store
                .select_option(&record.request_id, "missing".into(), None)
                .is_err()
        );
        assert_eq!(
            store.get(&record.request_id).unwrap().status,
            AgentPermissionRequestStatus::Pending
        );
        let selected = store
            .select_option(&record.request_id, "reject".into(), None)
            .unwrap();
        assert_eq!(selected.status, AgentPermissionRequestStatus::Denied);
        assert_eq!(
            store.wait_for_resolution(&record.request_id).await.unwrap(),
            PermissionResolution::Selected("reject".into())
        );
    }

    #[tokio::test]
    async fn session_shutdown_resolves_pending_choices_without_affecting_other_sessions() {
        let store = AgentPermissionRequestStore::new();
        let record = request(&store);
        store.deny_for_session("different-agent");
        assert_eq!(
            store.get(&record.request_id).unwrap().status,
            AgentPermissionRequestStatus::Pending
        );
        store.deny_for_session("agent-test");
        assert_eq!(
            store.wait_for_resolution(&record.request_id).await.unwrap(),
            PermissionResolution::Denied
        );
    }
}
