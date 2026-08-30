//! Atomic expected-value publication for durable work-environment results.

use std::path::Path;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use fs2::FileExt as _;
use medousa_forge::execution::{ExecutionClass, ForgeExecutionService};
use medousa_store::{StorePath, StoreRoot, StoreRootError};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use medousa_runtime::WorkEnvironmentError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PublicationCasOutcome {
    Published { previous: Option<String> },
    AlreadyPublished,
    Conflict { found: Option<String> },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PublicationPointer {
    target_ref: String,
    value: String,
    updated_at: DateTime<Utc>,
}

pub struct FsWorkEnvironmentPublicationStore {
    root: Arc<StoreRoot>,
    execution: Arc<ForgeExecutionService>,
}

impl FsWorkEnvironmentPublicationStore {
    pub fn open(
        path: &Path,
        execution: Arc<ForgeExecutionService>,
    ) -> Result<Arc<Self>, WorkEnvironmentError> {
        let root = StoreRoot::open_or_create_nofollow(path).map_err(|error| {
            WorkEnvironmentError::Adapter(format!("open publication store: {error}"))
        })?;
        for path in ["pointers", "locks"] {
            root.create_dir_all(&StorePath::parse(path).map_err(Self::map_store)?)
                .map_err(Self::map_store)?;
        }
        Ok(Arc::new(Self {
            root: Arc::new(root),
            execution,
        }))
    }

    fn map_store(error: StoreRootError) -> WorkEnvironmentError {
        WorkEnvironmentError::Adapter(format!("publication store: {error}"))
    }

    fn map_forge(error: medousa_forge::ForgeError) -> WorkEnvironmentError {
        WorkEnvironmentError::Adapter(error.to_string())
    }

    fn key(target_ref: &str) -> Result<String, WorkEnvironmentError> {
        if target_ref.is_empty()
            || target_ref.len() > 2_048
            || target_ref.chars().any(char::is_control)
        {
            return Err(WorkEnvironmentError::InvalidSpec(
                "publication target_ref is invalid".to_string(),
            ));
        }
        Ok(format!("{:x}", Sha256::digest(target_ref.as_bytes())))
    }

    fn paths(target_ref: &str) -> Result<(StorePath, StorePath), WorkEnvironmentError> {
        let key = Self::key(target_ref)?;
        Ok((
            StorePath::parse(&format!("pointers/{key}.json")).map_err(Self::map_store)?,
            StorePath::parse(&format!("locks/{key}.lock")).map_err(Self::map_store)?,
        ))
    }

    pub async fn compare_and_swap(
        &self,
        target_ref: &str,
        expected: Option<&str>,
        value: &str,
    ) -> Result<PublicationCasOutcome, WorkEnvironmentError> {
        if value.is_empty() || value.len() > 2_048 || value.chars().any(char::is_control) {
            return Err(WorkEnvironmentError::InvalidSpec(
                "publication value is invalid".to_string(),
            ));
        }
        let (pointer_path, lock_path) = Self::paths(target_ref)?;
        let target_ref = target_ref.to_string();
        let expected = expected.map(str::to_string);
        let value = value.to_string();
        let root = Arc::clone(&self.root);
        self.execution
            .run(ExecutionClass::StoreIo, 16 * 1024, move || {
                let lock = root.open_lock_file(&lock_path).map_err(|error| {
                    medousa_forge::ForgeError::Store(format!("publication lock: {error}"))
                })?;
                lock.lock_exclusive().map_err(|error| {
                    medousa_forge::ForgeError::Store(format!("publication lock: {error}"))
                })?;
                let found = if root.is_file(&pointer_path).map_err(|error| {
                    medousa_forge::ForgeError::Store(format!("publication pointer: {error}"))
                })? {
                    let bytes = root
                        .read_limited(&pointer_path, 16 * 1024)
                        .map_err(|error| {
                            medousa_forge::ForgeError::Store(format!(
                                "publication pointer: {error}"
                            ))
                        })?;
                    let pointer: PublicationPointer =
                        serde_json::from_slice(&bytes).map_err(|error| {
                            medousa_forge::ForgeError::Store(format!(
                                "decode publication pointer: {error}"
                            ))
                        })?;
                    if pointer.target_ref != target_ref {
                        return Err(medousa_forge::ForgeError::Store(
                            "publication target hash collision".to_string(),
                        ));
                    }
                    Some(pointer.value)
                } else {
                    None
                };
                if found.as_deref() == Some(value.as_str()) {
                    return Ok(PublicationCasOutcome::AlreadyPublished);
                }
                if found != expected {
                    return Ok(PublicationCasOutcome::Conflict { found });
                }
                let pointer = PublicationPointer {
                    target_ref,
                    value,
                    updated_at: Utc::now(),
                };
                let bytes = serde_json::to_vec(&pointer).map_err(|error| {
                    medousa_forge::ForgeError::Store(format!("encode publication pointer: {error}"))
                })?;
                root.atomic_write(&pointer_path, &bytes).map_err(|error| {
                    medousa_forge::ForgeError::Store(format!("publish pointer: {error}"))
                })?;
                Ok(PublicationCasOutcome::Published { previous: found })
            })
            .await
            .map_err(Self::map_forge)
    }

    pub async fn resolve(&self, target_ref: &str) -> Result<Option<String>, WorkEnvironmentError> {
        let (pointer_path, _) = Self::paths(target_ref)?;
        let target_ref = target_ref.to_string();
        let root = Arc::clone(&self.root);
        self.execution
            .run(ExecutionClass::StoreIo, 16 * 1024, move || {
                if !root.is_file(&pointer_path).map_err(|error| {
                    medousa_forge::ForgeError::Store(format!("publication pointer: {error}"))
                })? {
                    return Ok(None);
                }
                let bytes = root
                    .read_limited(&pointer_path, 16 * 1024)
                    .map_err(|error| {
                        medousa_forge::ForgeError::Store(format!("publication pointer: {error}"))
                    })?;
                let pointer: PublicationPointer =
                    serde_json::from_slice(&bytes).map_err(|error| {
                        medousa_forge::ForgeError::Store(format!(
                            "decode publication pointer: {error}"
                        ))
                    })?;
                if pointer.target_ref != target_ref {
                    return Err(medousa_forge::ForgeError::Store(
                        "publication target hash collision".to_string(),
                    ));
                }
                Ok(Some(pointer.value))
            })
            .await
            .map_err(Self::map_forge)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn publication_is_atomic_idempotent_and_reports_conflicts() {
        let temp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(temp.path()).unwrap();
        let store =
            FsWorkEnvironmentPublicationStore::open(&root, Arc::new(ForgeExecutionService::new()))
                .unwrap();
        assert_eq!(
            store
                .compare_and_swap("work/result", None, "cas:sha256:first")
                .await
                .unwrap(),
            PublicationCasOutcome::Published { previous: None }
        );
        assert_eq!(
            store
                .compare_and_swap("work/result", None, "cas:sha256:first")
                .await
                .unwrap(),
            PublicationCasOutcome::AlreadyPublished
        );
        assert_eq!(
            store
                .compare_and_swap("work/result", None, "cas:sha256:second")
                .await
                .unwrap(),
            PublicationCasOutcome::Conflict {
                found: Some("cas:sha256:first".to_string())
            }
        );
        assert_eq!(
            store.resolve("work/result").await.unwrap().as_deref(),
            Some("cas:sha256:first")
        );
    }
}
