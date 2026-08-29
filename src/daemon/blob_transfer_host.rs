//! Daemon-owned filesystem adapter for Stasis content-addressed blobs.
//!
//! Durable jobs and checkpoint manifests carry descriptors; bytes stay behind
//! this port. Object names come only from verified SHA-256 digests and all IO
//! is confined beneath one retained `StoreRoot` capability.

use std::collections::HashSet;
use std::io::Read as _;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use medousa_forge::execution::{ExecutionClass, ForgeExecutionService};
use medousa_store::{StoreEntryKind, StorePath, StoreRoot, StoreRootError};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use stasis::domain::runtime::blob_descriptor::BlobDescriptor;
use stasis::domain::runtime::provenance::ContentDigest;
use stasis::ports::outbound::runtime::blob_transfer::BlobTransferPort;
use stasis::prelude::{Result as StasisResult, StasisError};

const MAX_DURABLE_BLOB_BYTES: u64 = 512 * 1024 * 1024;

pub struct FsBlobTransferPort {
    root: Arc<StoreRoot>,
    execution: Arc<ForgeExecutionService>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct DurableContentRoot {
    root_id: String,
    blobs: Vec<BlobDescriptor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    retain_until: Option<DateTime<Utc>>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BlobGarbageCollectionReport {
    pub active_roots: usize,
    pub expired_roots: usize,
    pub deleted_objects: usize,
    pub preserved_recent_orphans: usize,
}

impl FsBlobTransferPort {
    pub fn open(path: &Path, execution: Arc<ForgeExecutionService>) -> StasisResult<Arc<Self>> {
        let root = StoreRoot::open_or_create_nofollow(path)
            .map_err(|error| StasisError::PortFailure(format!("open blob store: {error}")))?;
        let store = Arc::new(Self {
            root: Arc::new(root),
            execution,
        });
        for path in ["objects/sha256", "roots"] {
            store
                .root
                .create_dir_all(&StorePath::parse(path).map_err(|error| {
                    StasisError::PortFailure(format!("initialize blob store: {error}"))
                })?)
                .map_err(|error| {
                    StasisError::PortFailure(format!("initialize blob store: {error}"))
                })?;
        }
        Ok(store)
    }

    fn validate_descriptor(descriptor: &BlobDescriptor) -> StasisResult<()> {
        if descriptor.digest.algorithm != ContentDigest::SHA256
            || descriptor.digest.hex.len() != 64
            || !descriptor
                .digest
                .hex
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(StasisError::PortFailure(
                "blob descriptor must use a sha256 digest".to_string(),
            ));
        }
        if descriptor.size_bytes > MAX_DURABLE_BLOB_BYTES {
            return Err(StasisError::PortFailure(format!(
                "blob exceeds {MAX_DURABLE_BLOB_BYTES} bytes"
            )));
        }
        Ok(())
    }

    fn object_path(descriptor: &BlobDescriptor) -> StasisResult<StorePath> {
        Self::validate_descriptor(descriptor)?;
        StorePath::parse(&format!(
            "objects/sha256/{}/{}",
            &descriptor.digest.hex[..2],
            descriptor.digest.hex
        ))
        .map_err(|error| StasisError::PortFailure(format!("blob object path: {error}")))
    }

    fn map_store(error: StoreRootError) -> medousa_forge::ForgeError {
        medousa_forge::ForgeError::Store(format!("blob store: {error}"))
    }

    fn map_forge(error: medousa_forge::ForgeError) -> StasisError {
        StasisError::PortFailure(error.to_string())
    }

    fn descriptor_for_file(
        root: &StoreRoot,
        path: &StorePath,
        media_type: Option<String>,
        max_bytes: u64,
    ) -> Result<BlobDescriptor, medousa_forge::ForgeError> {
        let mut file = root.open_read_file(path).map_err(Self::map_store)?;
        let size = file.metadata()?.len();
        if size > max_bytes || size > MAX_DURABLE_BLOB_BYTES {
            return Err(medousa_forge::ForgeError::Store(format!(
                "blob source exceeds {} bytes",
                max_bytes.min(MAX_DURABLE_BLOB_BYTES)
            )));
        }
        let mut digest = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
        let mut read = 0_u64;
        loop {
            let count = file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            read = read.saturating_add(count as u64);
            if read > max_bytes || read > MAX_DURABLE_BLOB_BYTES {
                return Err(medousa_forge::ForgeError::Store(
                    "blob source exceeded its streaming bound".to_string(),
                ));
            }
            digest.update(&buffer[..count]);
        }
        Ok(BlobDescriptor {
            digest: ContentDigest {
                algorithm: ContentDigest::SHA256.to_string(),
                hex: format!("{:x}", digest.finalize()),
            },
            size_bytes: read,
            media_type,
            transfer_hint: None,
        })
    }

    fn verify_file(
        root: &StoreRoot,
        path: &StorePath,
        descriptor: &BlobDescriptor,
    ) -> Result<(), medousa_forge::ForgeError> {
        let actual = Self::descriptor_for_file(
            root,
            path,
            descriptor.media_type.clone(),
            descriptor.size_bytes,
        )?;
        if actual.digest != descriptor.digest || actual.size_bytes != descriptor.size_bytes {
            return Err(medousa_forge::ForgeError::Store(
                "blob file failed digest verification".to_string(),
            ));
        }
        Ok(())
    }

    fn root_path(root_id: &str) -> StasisResult<StorePath> {
        if root_id.is_empty() || root_id.len() > 2_048 || root_id.chars().any(char::is_control) {
            return Err(StasisError::PortFailure(
                "durable content root id is invalid".to_string(),
            ));
        }
        let digest = format!("{:x}", Sha256::digest(root_id.as_bytes()));
        StorePath::parse(&format!("roots/{digest}.json"))
            .map_err(|error| StasisError::PortFailure(format!("content root path: {error}")))
    }

    pub async fn pin_root(
        &self,
        root_id: &str,
        blobs: Vec<BlobDescriptor>,
        retain_until: Option<DateTime<Utc>>,
    ) -> StasisResult<()> {
        let path = Self::root_path(root_id)?;
        let mut seen = HashSet::new();
        for blob in &blobs {
            Self::validate_descriptor(blob)?;
            if !seen.insert((blob.digest.algorithm.clone(), blob.digest.hex.clone())) {
                continue;
            }
            if !self.exists(blob).await? {
                return Err(StasisError::PortFailure(format!(
                    "cannot pin missing blob {}:{}",
                    blob.digest.algorithm, blob.digest.hex
                )));
            }
        }
        let record = DurableContentRoot {
            root_id: root_id.to_string(),
            blobs,
            retain_until,
            updated_at: Utc::now(),
        };
        let bytes = serde_json::to_vec(&record)
            .map_err(|error| StasisError::PortFailure(format!("encode content root: {error}")))?;
        let root = Arc::clone(&self.root);
        self.execution
            .run(ExecutionClass::StoreIo, bytes.len(), move || {
                root.atomic_write(&path, &bytes).map_err(Self::map_store)
            })
            .await
            .map_err(Self::map_forge)
    }

    pub async fn release_root(&self, root_id: &str) -> StasisResult<()> {
        let path = Self::root_path(root_id)?;
        let root = Arc::clone(&self.root);
        self.execution
            .run(ExecutionClass::StoreIo, 64, move || {
                root.remove_file(&path).map_err(Self::map_store)
            })
            .await
            .map_err(Self::map_forge)
    }

    pub async fn collect_garbage(
        &self,
        now: DateTime<Utc>,
        orphan_grace: Duration,
    ) -> StasisResult<BlobGarbageCollectionReport> {
        let root = Arc::clone(&self.root);
        self.execution
            .run(ExecutionClass::StoreIo, 1024 * 1024, move || {
                let roots_path = StorePath::parse("roots").map_err(Self::map_store)?;
                let mut report = BlobGarbageCollectionReport::default();
                let mut marked = HashSet::new();
                for entry in root.list_directory(&roots_path).map_err(Self::map_store)? {
                    if entry.kind != StoreEntryKind::File {
                        continue;
                    }
                    let entry_path = roots_path.join(&entry.path).map_err(Self::map_store)?;
                    let bytes = root
                        .read_limited(&entry_path, 1024 * 1024)
                        .map_err(Self::map_store)?;
                    let record: DurableContentRoot =
                        serde_json::from_slice(&bytes).map_err(|error| {
                            medousa_forge::ForgeError::Store(format!(
                                "decode durable content root: {error}"
                            ))
                        })?;
                    if record.retain_until.is_some_and(|until| until <= now) {
                        root.remove_file(&entry_path).map_err(Self::map_store)?;
                        report.expired_roots += 1;
                        continue;
                    }
                    report.active_roots += 1;
                    for blob in record.blobs {
                        marked.insert(blob.digest.hex);
                    }
                }

                let objects_path = StorePath::parse("objects/sha256").map_err(Self::map_store)?;
                let cutoff = SystemTime::from(now)
                    .checked_sub(orphan_grace)
                    .unwrap_or(SystemTime::UNIX_EPOCH);
                for shard in root
                    .list_directory(&objects_path)
                    .map_err(Self::map_store)?
                {
                    if shard.kind != StoreEntryKind::Directory {
                        continue;
                    }
                    let shard_path = objects_path.join(&shard.path).map_err(Self::map_store)?;
                    for object in root.list_directory(&shard_path).map_err(Self::map_store)? {
                        if object.kind != StoreEntryKind::File
                            || marked.contains(object.path.file_name())
                        {
                            continue;
                        }
                        if object.modified.is_none_or(|modified| modified > cutoff) {
                            report.preserved_recent_orphans += 1;
                            continue;
                        }
                        let object_path = shard_path.join(&object.path).map_err(Self::map_store)?;
                        root.remove_file(&object_path).map_err(Self::map_store)?;
                        report.deleted_objects += 1;
                    }
                }
                Ok(report)
            })
            .await
            .map_err(Self::map_forge)
    }

    /// Stream a confined file into the content-addressed store without
    /// retaining the complete artifact in daemon memory.
    pub async fn put_file(
        &self,
        source_root: Arc<StoreRoot>,
        source: StorePath,
        media_type: Option<&str>,
        max_bytes: u64,
    ) -> StasisResult<BlobDescriptor> {
        if media_type.is_some_and(|value| {
            value.is_empty() || value.len() > 256 || value.chars().any(char::is_control)
        }) {
            return Err(StasisError::PortFailure(
                "blob media type is invalid".to_string(),
            ));
        }
        let destination_root = Arc::clone(&self.root);
        let media_type = media_type.map(str::to_string);
        self.execution
            .run(ExecutionClass::StoreIo, 64 * 1024, move || {
                let mut descriptor =
                    Self::descriptor_for_file(&source_root, &source, media_type, max_bytes)?;
                let destination = StorePath::parse(&format!(
                    "objects/sha256/{}/{}",
                    &descriptor.digest.hex[..2],
                    descriptor.digest.hex
                ))
                .map_err(Self::map_store)?;
                if !destination_root
                    .is_file(&destination)
                    .map_err(Self::map_store)?
                {
                    destination_root
                        .atomic_copy_from(
                            &destination,
                            &source_root,
                            &source,
                            descriptor.size_bytes,
                        )
                        .map_err(Self::map_store)?;
                }
                Self::verify_file(&destination_root, &destination, &descriptor)?;
                descriptor.transfer_hint =
                    Some(format!("medousa-cas://sha256/{}", descriptor.digest.hex));
                Ok(descriptor)
            })
            .await
            .map_err(Self::map_forge)
    }

    /// Stream a verified CAS object into a confined destination file.
    pub async fn materialize_file(
        &self,
        descriptor: &BlobDescriptor,
        destination_root: Arc<StoreRoot>,
        destination: StorePath,
        max_bytes: u64,
    ) -> StasisResult<()> {
        Self::validate_descriptor(descriptor)?;
        if descriptor.size_bytes > max_bytes {
            return Err(StasisError::PortFailure(format!(
                "blob exceeds destination bound of {max_bytes} bytes"
            )));
        }
        let source = Self::object_path(descriptor)?;
        let source_root = Arc::clone(&self.root);
        let descriptor = descriptor.clone();
        self.execution
            .run(ExecutionClass::StoreIo, 64 * 1024, move || {
                Self::verify_file(&source_root, &source, &descriptor)?;
                destination_root
                    .atomic_copy_from(&destination, &source_root, &source, descriptor.size_bytes)
                    .map_err(Self::map_store)?;
                Self::verify_file(&destination_root, &destination, &descriptor)
            })
            .await
            .map_err(Self::map_forge)
    }
}

#[async_trait]
impl BlobTransferPort for FsBlobTransferPort {
    async fn put(&self, bytes: &[u8], media_type: Option<&str>) -> StasisResult<BlobDescriptor> {
        if bytes.len() as u64 > MAX_DURABLE_BLOB_BYTES {
            return Err(StasisError::PortFailure(format!(
                "blob exceeds {MAX_DURABLE_BLOB_BYTES} bytes"
            )));
        }
        if media_type.is_some_and(|value| {
            value.is_empty() || value.len() > 256 || value.chars().any(char::is_control)
        }) {
            return Err(StasisError::PortFailure(
                "blob media type is invalid".to_string(),
            ));
        }
        let mut descriptor = BlobDescriptor::from_bytes(bytes);
        descriptor.media_type = media_type.map(str::to_string);
        descriptor.transfer_hint = Some(format!("medousa-cas://sha256/{}", descriptor.digest.hex));
        let path = Self::object_path(&descriptor)?;
        let root = Arc::clone(&self.root);
        let stored = bytes.to_vec();
        let verify = descriptor.clone();
        self.execution
            .run(ExecutionClass::StoreIo, stored.len(), move || {
                if !root.is_file(&path).map_err(Self::map_store)? {
                    match root.atomic_create(&path, &stored) {
                        Ok(()) => {}
                        Err(StoreRootError::Io { ref source, .. })
                            if source.kind() == std::io::ErrorKind::AlreadyExists => {}
                        Err(error) => return Err(Self::map_store(error)),
                    }
                }
                let persisted = root
                    .read_limited(&path, verify.size_bytes)
                    .map_err(Self::map_store)?;
                if !verify.verify(&persisted) {
                    return Err(medousa_forge::ForgeError::Store(
                        "existing blob failed digest verification".to_string(),
                    ));
                }
                Ok(())
            })
            .await
            .map_err(Self::map_forge)?;
        Ok(descriptor)
    }

    async fn get(&self, descriptor: &BlobDescriptor) -> StasisResult<Vec<u8>> {
        let path = Self::object_path(descriptor)?;
        let root = Arc::clone(&self.root);
        let verify = descriptor.clone();
        self.execution
            .run(
                ExecutionClass::StoreIo,
                descriptor.size_bytes.min(usize::MAX as u64) as usize,
                move || {
                    let bytes = root
                        .read_limited(&path, verify.size_bytes)
                        .map_err(Self::map_store)?;
                    if !verify.verify(&bytes) {
                        return Err(medousa_forge::ForgeError::Store(
                            "blob digest or size mismatch".to_string(),
                        ));
                    }
                    Ok(bytes)
                },
            )
            .await
            .map_err(Self::map_forge)
    }

    async fn exists(&self, descriptor: &BlobDescriptor) -> StasisResult<bool> {
        let path = Self::object_path(descriptor)?;
        let root = Arc::clone(&self.root);
        let verify = descriptor.clone();
        let exists = self
            .execution
            .run(ExecutionClass::StoreIo, 64, move || {
                let Some(metadata) = root
                    .metadata(&path)
                    .map(Some)
                    .or_else(|error| {
                        if error.is_not_found() {
                            Ok(None)
                        } else {
                            Err(error)
                        }
                    })
                    .map_err(Self::map_store)?
                else {
                    return Ok(false);
                };
                if metadata.size != verify.size_bytes {
                    return Err(medousa_forge::ForgeError::Store(
                        "blob size does not match its descriptor".to_string(),
                    ));
                }
                Self::verify_file(&root, &path, &verify)?;
                Ok(true)
            })
            .await
            .map_err(Self::map_forge)?;
        Ok(exists)
    }

    async fn delete(&self, descriptor: &BlobDescriptor) -> StasisResult<bool> {
        let path = Self::object_path(descriptor)?;
        let root = Arc::clone(&self.root);
        self.execution
            .run(ExecutionClass::StoreIo, 64, move || {
                let existed = root.is_file(&path).map_err(Self::map_store)?;
                if existed {
                    root.remove_file(&path).map_err(Self::map_store)?;
                }
                Ok(existed)
            })
            .await
            .map_err(Self::map_forge)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn filesystem_blob_port_deduplicates_and_verifies_content() {
        let temp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(temp.path()).unwrap();
        let port = FsBlobTransferPort::open(&root, Arc::new(ForgeExecutionService::new())).unwrap();
        let first = port
            .put(b"durable bytes", Some("text/plain"))
            .await
            .unwrap();
        let replay = port
            .put(b"durable bytes", Some("text/plain"))
            .await
            .unwrap();
        assert_eq!(first, replay);
        assert!(port.exists(&first).await.unwrap());
        assert_eq!(port.get(&first).await.unwrap(), b"durable bytes");
        port.root
            .atomic_write(
                &FsBlobTransferPort::object_path(&first).unwrap(),
                b"corrupt bytes",
            )
            .unwrap();
        assert!(port.exists(&first).await.is_err());
        assert!(port.delete(&first).await.unwrap());
        assert!(!port.exists(&first).await.unwrap());
    }

    #[tokio::test]
    async fn filesystem_blob_port_streams_confined_files_in_both_directions() {
        let temp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(temp.path()).unwrap();
        let port =
            FsBlobTransferPort::open(&root.join("blobs"), Arc::new(ForgeExecutionService::new()))
                .unwrap();
        let source = Arc::new(StoreRoot::open_or_create_nofollow(&root.join("source")).unwrap());
        let destination =
            Arc::new(StoreRoot::open_or_create_nofollow(&root.join("destination")).unwrap());
        let path = StorePath::parse("nested/artifact.bin").unwrap();
        let bytes = vec![0x5a; 256 * 1024];
        source.atomic_write(&path, &bytes).unwrap();

        let descriptor = port
            .put_file(
                Arc::clone(&source),
                path.clone(),
                Some("application/octet-stream"),
                bytes.len() as u64,
            )
            .await
            .unwrap();
        port.materialize_file(
            &descriptor,
            Arc::clone(&destination),
            path.clone(),
            bytes.len() as u64,
        )
        .await
        .unwrap();

        assert_eq!(descriptor.size_bytes, bytes.len() as u64);
        assert_eq!(
            destination.read_limited(&path, bytes.len() as u64).unwrap(),
            bytes
        );
    }

    #[tokio::test]
    async fn garbage_collection_keeps_named_roots_and_sweeps_old_orphans() {
        let temp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(temp.path()).unwrap();
        let port = FsBlobTransferPort::open(&root, Arc::new(ForgeExecutionService::new())).unwrap();
        let rooted = port.put(b"rooted", None).await.unwrap();
        let orphan = port.put(b"orphan", None).await.unwrap();
        port.pin_root("checkpoint:one", vec![rooted.clone()], None)
            .await
            .unwrap();

        let report = port
            .collect_garbage(Utc::now() + chrono::Duration::seconds(1), Duration::ZERO)
            .await
            .unwrap();
        assert_eq!(report.active_roots, 1);
        assert!(port.exists(&rooted).await.unwrap());
        assert!(!port.exists(&orphan).await.unwrap());

        port.release_root("checkpoint:one").await.unwrap();
        port.collect_garbage(Utc::now() + chrono::Duration::seconds(2), Duration::ZERO)
            .await
            .unwrap();
        assert!(!port.exists(&rooted).await.unwrap());
    }
}
