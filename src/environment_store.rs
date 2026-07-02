//! Persistent environment spec store per profile.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::Utc;
use medousa_types::environment::{EnvironmentSpec, EnvironmentStreamEvent, EnvironmentPendingProposal};
use medousa_types::environment_default::{default_environment_spec, DEFAULT_PROFILE_ID};
use medousa_types::environment_validate::is_valid_environment_spec;
use tokio::sync::{broadcast, RwLock as AsyncRwLock};

const STORE_DIR: &str = "environment";

#[derive(Debug, Clone)]
pub struct EnvironmentRecord {
    pub spec: EnvironmentSpec,
    pub revision: u64,
}

#[derive(Clone)]
pub struct EnvironmentHub {
    inner: Arc<AsyncRwLock<HashMap<String, EnvironmentRecord>>>,
    pending: Arc<AsyncRwLock<HashMap<String, EnvironmentPendingProposal>>>,
    revision: Arc<AsyncRwLock<u64>>,
    tx: broadcast::Sender<EnvironmentStreamEvent>,
}

impl EnvironmentHub {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(64);
        Self {
            inner: Arc::new(AsyncRwLock::new(HashMap::new())),
            pending: Arc::new(AsyncRwLock::new(HashMap::new())),
            revision: Arc::new(AsyncRwLock::new(0)),
            tx,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<EnvironmentStreamEvent> {
        self.tx.subscribe()
    }

    fn store_root() -> PathBuf {
        crate::paths::medousa_data_dir().join(STORE_DIR)
    }

    fn spec_path(profile_id: &str) -> PathBuf {
        Self::store_root().join(format!("{profile_id}.json"))
    }

    pub async fn load_or_default(profile_id: &str) -> Result<EnvironmentRecord> {
        let path = Self::spec_path(profile_id);
        if path.exists() {
            let raw = tokio::fs::read_to_string(&path)
                .await
                .with_context(|| format!("read environment spec {}", path.display()))?;
            let spec: EnvironmentSpec = serde_json::from_str(&raw)
                .with_context(|| format!("parse environment spec {}", path.display()))?;
            if is_valid_environment_spec(&spec) {
                return Ok(EnvironmentRecord {
                    revision: spec.updated_at.timestamp() as u64,
                    spec,
                });
            }
            tracing::warn!(
                profile_id,
                "invalid environment spec on disk; falling back to default"
            );
        }
        let spec = default_environment_spec(profile_id);
        let record = EnvironmentRecord {
            revision: 1,
            spec,
        };
        Self::persist_record(profile_id, &record).await?;
        Ok(record)
    }

    async fn persist_record(profile_id: &str, record: &EnvironmentRecord) -> Result<()> {
        let root = Self::store_root();
        tokio::fs::create_dir_all(&root).await?;
        let path = Self::spec_path(profile_id);
        let json = serde_json::to_string_pretty(&record.spec)?;
        tokio::fs::write(&path, json)
            .await
            .with_context(|| format!("write environment spec {}", path.display()))?;
        Ok(())
    }

    pub async fn get(&self, profile_id: &str) -> Result<EnvironmentRecord> {
        {
            let guard = self.inner.read().await;
            if let Some(record) = guard.get(profile_id) {
                return Ok(record.clone());
            }
        }
        let record = Self::load_or_default(profile_id).await?;
        let mut guard = self.inner.write().await;
        guard.insert(profile_id.to_string(), record.clone());
        Ok(record)
    }

    pub async fn put(&self, mut spec: EnvironmentSpec, updated_by: &str) -> Result<EnvironmentRecord> {
        if !is_valid_environment_spec(&spec) {
            anyhow::bail!("invalid environment spec");
        }
        spec.updated_at = Utc::now();
        spec.updated_by = updated_by.to_string();
        let mut revision = self.revision.write().await;
        *revision += 1;
        let record = EnvironmentRecord {
            revision: *revision,
            spec: spec.clone(),
        };
        Self::persist_record(&spec.profile_id, &record).await?;
        {
            let mut guard = self.inner.write().await;
            guard.insert(spec.profile_id.clone(), record.clone());
        }
        let _ = self.tx.send(EnvironmentStreamEvent {
            revision: record.revision,
            event_type: "spec_updated".to_string(),
            emitted_at_utc: Utc::now(),
            spec: Some(spec),
        });
        Ok(record)
    }

    pub async fn set_pending(
        &self,
        profile_id: &str,
        proposal: EnvironmentPendingProposal,
    ) {
        let mut guard = self.pending.write().await;
        guard.insert(profile_id.to_string(), proposal);
    }

    pub async fn pending(&self, profile_id: &str) -> Option<EnvironmentPendingProposal> {
        self.pending.read().await.get(profile_id).cloned()
    }

    pub async fn clear_pending(&self, profile_id: &str) {
        self.pending.write().await.remove(profile_id);
    }

    pub async fn apply_pending(&self, profile_id: &str) -> Result<EnvironmentRecord> {
        let proposal = self
            .pending(profile_id)
            .await
            .ok_or_else(|| anyhow::anyhow!("no pending environment proposal"))?;
        if !proposal.errors.is_empty() {
            anyhow::bail!("pending proposal has validation errors");
        }
        let record = self.put(proposal.proposed_spec, "operator").await?;
        self.clear_pending(profile_id).await;
        Ok(record)
    }
}

static ENVIRONMENT_HUB: std::sync::OnceLock<EnvironmentHub> = std::sync::OnceLock::new();

pub fn environment_hub() -> &'static EnvironmentHub {
    ENVIRONMENT_HUB.get_or_init(EnvironmentHub::new)
}

pub fn resolve_profile_id(profile_id: Option<&str>) -> String {
    profile_id
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| DEFAULT_PROFILE_ID.to_string())
}

pub fn ensure_store_dir() -> Result<()> {
    let root = EnvironmentHub::store_root();
    std::fs::create_dir_all(&root)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_profile_defaults() {
        assert_eq!(resolve_profile_id(None), DEFAULT_PROFILE_ID);
        assert_eq!(resolve_profile_id(Some("  ")), DEFAULT_PROFILE_ID);
        assert_eq!(resolve_profile_id(Some("work")), "work");
    }
}
