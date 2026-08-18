//! Persistent environment spec store per profile.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::Utc;
use medousa_types::authority_id::EnvironmentProfileId;
use medousa_types::environment::{
    EnvironmentPendingProposal, EnvironmentSpec, EnvironmentStreamEvent,
};
use medousa_types::environment_default::{DEFAULT_PROFILE_ID, default_environment_spec};
use medousa_types::environment_validate::is_valid_environment_spec;
use tokio::sync::{RwLock as AsyncRwLock, broadcast};

use crate::store_root::{StorePath, StoreRoot};

const STORE_DIR: &str = "environment";
const MAX_ENVIRONMENT_SPEC_BYTES: u64 = 4 * 1024 * 1024;

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

impl Default for EnvironmentHub {
    fn default() -> Self {
        Self::new()
    }
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

    fn store() -> Result<StoreRoot> {
        StoreRoot::open_or_create_nofollow(&Self::store_root()).map_err(anyhow::Error::from)
    }

    fn spec_path(profile_id: &EnvironmentProfileId) -> StorePath {
        StorePath::parse(&format!("{}.json", profile_id.storage_key().as_str()))
            .expect("opaque environment profile key is a valid store path")
    }

    fn legacy_spec_path(profile_id: &EnvironmentProfileId) -> Option<StorePath> {
        StorePath::parse(&format!("{}.json", profile_id.as_str())).ok()
    }

    pub async fn load_or_default(profile_id: &str) -> Result<EnvironmentRecord> {
        let typed_profile = EnvironmentProfileId::parse(profile_id)?;
        let store = Self::store()?;
        let path = Self::spec_path(&typed_profile);
        let raw = match store.read_limited(&path, MAX_ENVIRONMENT_SPEC_BYTES) {
            Ok(raw) => Some(raw),
            Err(error) if error.is_not_found() => {
                let Some(legacy_path) = Self::legacy_spec_path(&typed_profile) else {
                    return Self::persist_default(profile_id).await;
                };
                match store.read_limited(&legacy_path, MAX_ENVIRONMENT_SPEC_BYTES) {
                    Ok(raw) => Some(raw),
                    Err(error) if error.is_not_found() => None,
                    Err(error) => return Err(error.into()),
                }
            }
            Err(error) => return Err(error.into()),
        };
        if let Some(raw) = raw {
            let spec: EnvironmentSpec =
                serde_json::from_slice(&raw).context("parse environment spec")?;
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
        Self::persist_default(profile_id).await
    }

    async fn persist_default(profile_id: &str) -> Result<EnvironmentRecord> {
        let spec = default_environment_spec(profile_id);
        let record = EnvironmentRecord { revision: 1, spec };
        Self::persist_record(profile_id, &record).await?;
        Ok(record)
    }

    async fn persist_record(profile_id: &str, record: &EnvironmentRecord) -> Result<()> {
        let profile_id = EnvironmentProfileId::parse(profile_id)?;
        let store = Self::store()?;
        let path = Self::spec_path(&profile_id);
        let json = serde_json::to_string_pretty(&record.spec)?;
        store.atomic_write(&path, json.as_bytes())?;
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

    pub async fn put(
        &self,
        mut spec: EnvironmentSpec,
        updated_by: &str,
    ) -> Result<EnvironmentRecord> {
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
            component_patches: None,
            feed_event: None,
            runtime_probe: None,
        });
        Ok(record)
    }

    pub async fn set_pending(&self, profile_id: &str, proposal: EnvironmentPendingProposal) {
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

    pub async fn emit_stream_event(&self, event: EnvironmentStreamEvent) {
        let _ = self.tx.send(event);
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
    EnvironmentHub::store()?;
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

    #[test]
    fn environment_profile_paths_are_opaque_and_collision_free() {
        let work = EnvironmentProfileId::parse("work").unwrap();
        let worker = EnvironmentProfileId::parse("worker").unwrap();
        let work_path = EnvironmentHub::spec_path(&work);
        let worker_path = EnvironmentHub::spec_path(&worker);
        assert_ne!(work_path, worker_path);
        assert!(!work_path.file_name().contains("work"));
    }
}
