//! Ephemeral content-addressed backing for oversized, non-replayable Coder evidence.
//!
//! Payloads are redacted before hashing, compressed at rest, globally
//! deduplicated, and referenced only through an undertaking-scoped receipt.

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Narrow boundary used by the perception governor to stage a compact receipt
/// for Forge. Implementations must never serialize the underlying payload.
pub trait CompactEvidenceReceiptSink: Send + Sync {
    fn stage_compact_receipt(
        &self,
        source_tool: &str,
        source_call_id: Option<&str>,
        receipt: &CoderEvidenceReceipt,
    ) -> Result<(), String>;
}

const INDEX_VERSION: u32 = 1;
const INDEX_FILE: &str = "index.json";
const OBJECTS_DIR: &str = "objects";
const MAX_READ_BYTES: usize = 32 * 1024;
static EVIDENCE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvidencePolicy {
    pub max_object_logical_bytes: u64,
    pub max_object_physical_bytes: u64,
    pub max_undertaking_physical_bytes: u64,
    pub max_global_physical_bytes: u64,
    pub short_ttl_seconds: u64,
    pub extended_ttl_seconds: u64,
}

impl Default for EvidencePolicy {
    fn default() -> Self {
        Self {
            max_object_logical_bytes: 8 * 1024 * 1024,
            max_object_physical_bytes: 8 * 1024 * 1024,
            max_undertaking_physical_bytes: 64 * 1024 * 1024,
            max_global_physical_bytes: 512 * 1024 * 1024,
            short_ttl_seconds: 6 * 60 * 60,
            extended_ttl_seconds: 72 * 60 * 60,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceRetention {
    SuccessfulOrReproducible,
    FailedOrNonReproducible,
}

impl EvidenceRetention {
    fn ttl_seconds(self, policy: EvidencePolicy) -> u64 {
        match self {
            Self::SuccessfulOrReproducible => policy.short_ttl_seconds,
            Self::FailedOrNonReproducible => policy.extended_ttl_seconds,
        }
    }

    fn eviction_priority(self) -> u8 {
        match self {
            Self::SuccessfulOrReproducible => 0,
            Self::FailedOrNonReproducible => 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoderEvidenceReceipt {
    pub reference: String,
    pub digest: String,
    pub content_type: String,
    pub logical_bytes: u64,
    pub physical_bytes: u64,
    pub retention: EvidenceRetention,
    pub expires_at_unix_seconds: u64,
    pub redacted: bool,
    pub deduplicated: bool,
    pub read_tool: String,
    pub max_read_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoderEvidenceRead {
    pub digest: String,
    pub content_type: String,
    pub logical_bytes: u64,
    pub offset: usize,
    pub end: usize,
    pub remaining_bytes: usize,
    pub next_offset: Option<usize>,
    pub content: String,
    pub redacted: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EvidenceMaintenanceReport {
    pub expired_objects: u64,
    pub pressure_evicted_objects: u64,
    pub orphan_objects: u64,
    pub reclaimed_physical_bytes: u64,
    pub remaining_physical_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct CoderEvidenceStore {
    root: PathBuf,
    policy: EvidencePolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct EvidenceIndex {
    version: u32,
    objects: HashMap<String, EvidenceObject>,
}

impl Default for EvidenceIndex {
    fn default() -> Self {
        Self {
            version: INDEX_VERSION,
            objects: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EvidenceObject {
    content_type: String,
    logical_bytes: u64,
    physical_bytes: u64,
    created_at_unix_seconds: u64,
    last_accessed_unix_seconds: u64,
    expires_at_unix_seconds: u64,
    retention: EvidenceRetention,
    refs: HashMap<String, u64>,
}

impl CoderEvidenceStore {
    pub fn for_data_root(data_root: &Path) -> Self {
        Self::new(data_root.join("coder-evidence"), EvidencePolicy::default())
    }

    pub fn new(root: PathBuf, policy: EvidencePolicy) -> Self {
        Self { root, policy }
    }

    pub fn put(
        &self,
        undertaking_id: &str,
        output: &Value,
        retention: EvidenceRetention,
        read_tool: &str,
    ) -> Result<CoderEvidenceReceipt, String> {
        validate_undertaking_id(undertaking_id)?;
        let _guard = evidence_lock()?;
        self.ensure_layout()?;
        let now = unix_seconds(SystemTime::now());
        let mut index = self.load_index()?;
        let mut maintenance = EvidenceMaintenanceReport::default();
        self.prune_expired_locked(&mut index, now, &mut maintenance)?;
        self.remove_orphans_locked(&index, &mut maintenance)?;

        let safe = redact_evidence_value(&crate::settings_guard::redact_json_value(output));
        let serialized = serde_json::to_vec(&safe).map_err(|err| err.to_string())?;
        let logical_bytes = u64::try_from(serialized.len()).unwrap_or(u64::MAX);
        if logical_bytes > self.policy.max_object_logical_bytes {
            return Err(format!(
                "redacted evidence is {logical_bytes} bytes; per-object limit is {} bytes",
                self.policy.max_object_logical_bytes
            ));
        }
        let digest = format!("{:x}", Sha256::digest(&serialized));
        let path = self.object_path(&digest)?;
        if let Some(parent) = path.parent()
            && parent.exists()
        {
            require_real_directory(parent)?;
        }
        let object_exists = if path.exists() {
            require_real_file(&path)?;
            true
        } else {
            false
        };
        let ttl = retention.ttl_seconds(self.policy);
        let expires_at = now.saturating_add(ttl);

        if object_exists && let Some(object) = index.objects.get_mut(&digest) {
            object.last_accessed_unix_seconds = now;
            object.expires_at_unix_seconds = object.expires_at_unix_seconds.max(expires_at);
            if retention == EvidenceRetention::FailedOrNonReproducible {
                object.retention = retention;
            }
            object.refs.insert(undertaking_id.to_string(), now);
            self.enforce_undertaking_budget_locked(
                &mut index,
                undertaking_id,
                Some(&digest),
                &mut maintenance,
            )?;
            self.enforce_global_budget_locked(&mut index, None, &mut maintenance)?;
            let Some(object) = index.objects.get(&digest) else {
                self.save_index(&index)?;
                return Err("evidence could not fit the configured global physical budget".into());
            };
            let receipt = receipt_for(&digest, object, true, read_tool);
            self.save_index(&index)?;
            return Ok(receipt);
        }

        let compressed = compress(&serialized)?;
        let compressed_bytes = u64::try_from(compressed.len()).unwrap_or(u64::MAX);
        if compressed_bytes > self.policy.max_object_physical_bytes {
            return Err(format!(
                "compressed evidence is {compressed_bytes} bytes; per-object physical limit is {} bytes",
                self.policy.max_object_physical_bytes
            ));
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
            require_real_directory(parent)?;
        }
        crate::session::atomic_write(&path, &compressed).map_err(|err| err.to_string())?;
        let physical_bytes = physical_bytes(&path).unwrap_or(compressed_bytes);
        if physical_bytes > self.policy.max_object_physical_bytes {
            let _ = fs::remove_file(&path);
            return Err(format!(
                "stored evidence occupies {physical_bytes} bytes; per-object physical limit is {} bytes",
                self.policy.max_object_physical_bytes
            ));
        }
        if physical_bytes > self.policy.max_undertaking_physical_bytes
            || physical_bytes > self.policy.max_global_physical_bytes
        {
            let _ = fs::remove_file(&path);
            return Err(
                "evidence object cannot fit the undertaking or global physical budget".into(),
            );
        }

        let object = EvidenceObject {
            content_type: "application/json".to_string(),
            logical_bytes,
            physical_bytes,
            created_at_unix_seconds: now,
            last_accessed_unix_seconds: now,
            expires_at_unix_seconds: expires_at,
            retention,
            refs: HashMap::from([(undertaking_id.to_string(), now)]),
        };
        index.objects.insert(digest.clone(), object);
        self.enforce_undertaking_budget_locked(
            &mut index,
            undertaking_id,
            Some(&digest),
            &mut maintenance,
        )?;
        self.enforce_global_budget_locked(&mut index, None, &mut maintenance)?;
        let Some(object) = index.objects.get(&digest) else {
            self.save_index(&index)?;
            return Err("evidence could not fit the configured global physical budget".into());
        };
        let receipt = receipt_for(&digest, object, false, read_tool);
        self.save_index(&index)?;
        Ok(receipt)
    }

    pub fn read_range(
        &self,
        undertaking_id: &str,
        reference: &str,
        offset: usize,
        requested_bytes: usize,
    ) -> Result<CoderEvidenceRead, String> {
        validate_undertaking_id(undertaking_id)?;
        let digest = normalize_reference(reference)?;
        let _guard = evidence_lock()?;
        self.ensure_layout()?;
        let now = unix_seconds(SystemTime::now());
        let mut index = self.load_index()?;
        let mut maintenance = EvidenceMaintenanceReport::default();
        self.prune_expired_locked(&mut index, now, &mut maintenance)?;
        if maintenance.expired_objects > 0 {
            self.save_index(&index)?;
        }

        let path = self.object_path(&digest)?;
        let parent = path
            .parent()
            .ok_or_else(|| "Coder evidence object has no parent".to_string())?;
        require_real_directory(parent)
            .map_err(|_| "ephemeral evidence payload is unavailable or expired".to_string())?;
        require_real_file(&path)
            .map_err(|_| "ephemeral evidence payload is unavailable or expired".to_string())?;
        let object = index
            .objects
            .get_mut(&digest)
            .ok_or_else(|| "ephemeral evidence is unavailable or expired".to_string())?;
        if !object.refs.contains_key(undertaking_id) {
            return Err("evidence reference does not belong to this undertaking".into());
        }
        let compressed = fs::read(&path)
            .map_err(|_| "ephemeral evidence payload is unavailable or expired".to_string())?;
        let payload = decompress(&compressed, self.policy.max_object_logical_bytes)?;
        if format!("{:x}", Sha256::digest(&payload)) != digest {
            return Err("ephemeral evidence digest verification failed".into());
        }
        let text = String::from_utf8(payload)
            .map_err(|_| "ephemeral evidence is not valid UTF-8 JSON".to_string())?;
        if offset > text.len() {
            return Err(format!(
                "evidence offset {offset} exceeds the {} byte payload; use an offset from 0 through {}",
                text.len(),
                text.len()
            ));
        }
        let start = next_char_boundary(&text, offset);
        let limit = requested_bytes.clamp(1, MAX_READ_BYTES);
        let end = previous_char_boundary(&text, start.saturating_add(limit).min(text.len()));
        let retention_ttl = object.retention.ttl_seconds(self.policy);
        object.last_accessed_unix_seconds = now;
        object.expires_at_unix_seconds = now.saturating_add(retention_ttl);
        object.refs.insert(undertaking_id.to_string(), now);
        let read = CoderEvidenceRead {
            digest: format!("sha256:{digest}"),
            content_type: object.content_type.clone(),
            logical_bytes: object.logical_bytes,
            offset: start,
            end,
            remaining_bytes: text.len().saturating_sub(end),
            next_offset: (end < text.len()).then_some(end),
            content: text[start..end].to_string(),
            redacted: true,
        };
        self.save_index(&index)?;
        Ok(read)
    }

    pub fn maintain(&self) -> Result<EvidenceMaintenanceReport, String> {
        if !self.root.exists() {
            return Ok(EvidenceMaintenanceReport::default());
        }
        let _guard = evidence_lock()?;
        self.ensure_layout()?;
        let mut index = self.load_index()?;
        let mut report = EvidenceMaintenanceReport::default();
        self.prune_expired_locked(&mut index, unix_seconds(SystemTime::now()), &mut report)?;
        self.remove_orphans_locked(&index, &mut report)?;
        self.enforce_global_budget_locked(&mut index, None, &mut report)?;
        report.remaining_physical_bytes = estimated_global_physical_bytes(&index);
        self.save_index(&index)?;
        Ok(report)
    }

    fn ensure_layout(&self) -> Result<(), String> {
        fs::create_dir_all(&self.root).map_err(|err| err.to_string())?;
        require_real_directory(&self.root)?;
        let objects = self.root.join(OBJECTS_DIR);
        fs::create_dir_all(&objects).map_err(|err| err.to_string())?;
        require_real_directory(&objects)
    }

    fn index_path(&self) -> PathBuf {
        self.root.join(INDEX_FILE)
    }

    fn object_path(&self, digest: &str) -> Result<PathBuf, String> {
        validate_digest(digest)?;
        Ok(self
            .root
            .join(OBJECTS_DIR)
            .join(&digest[..2])
            .join(format!("{digest}.json.gz")))
    }

    fn load_index(&self) -> Result<EvidenceIndex, String> {
        let path = self.index_path();
        if !path.exists() {
            return Ok(EvidenceIndex::default());
        }
        require_real_file(&path)?;
        let raw = fs::read(&path).map_err(|err| err.to_string())?;
        let index: EvidenceIndex = serde_json::from_slice(&raw)
            .map_err(|err| format!("cannot read Coder evidence index: {err}"))?;
        if index.version != INDEX_VERSION {
            return Err(format!(
                "unsupported Coder evidence index version {}",
                index.version
            ));
        }
        Ok(index)
    }

    fn save_index(&self, index: &EvidenceIndex) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(index).map_err(|err| err.to_string())?;
        crate::session::atomic_write(&self.index_path(), &bytes).map_err(|err| err.to_string())
    }

    fn prune_expired_locked(
        &self,
        index: &mut EvidenceIndex,
        now: u64,
        report: &mut EvidenceMaintenanceReport,
    ) -> Result<(), String> {
        let expired = index
            .objects
            .iter()
            .filter(|(_, object)| object.expires_at_unix_seconds <= now)
            .map(|(digest, _)| digest.clone())
            .collect::<Vec<_>>();
        for digest in expired {
            if let Some(object) = self.remove_object_locked(index, &digest)? {
                report.expired_objects = report.expired_objects.saturating_add(1);
                report.reclaimed_physical_bytes = report
                    .reclaimed_physical_bytes
                    .saturating_add(object.physical_bytes);
            }
        }
        Ok(())
    }

    fn enforce_undertaking_budget_locked(
        &self,
        index: &mut EvidenceIndex,
        undertaking_id: &str,
        preserve_digest: Option<&str>,
        report: &mut EvidenceMaintenanceReport,
    ) -> Result<(), String> {
        loop {
            let total = index
                .objects
                .values()
                .filter(|object| object.refs.contains_key(undertaking_id))
                .fold(0u64, |sum, object| {
                    sum.saturating_add(object.physical_bytes)
                });
            if total <= self.policy.max_undertaking_physical_bytes {
                return Ok(());
            }
            let candidate = index
                .objects
                .iter()
                .filter(|(digest, object)| {
                    object.refs.contains_key(undertaking_id)
                        && preserve_digest.is_none_or(|preserve| preserve != digest.as_str())
                })
                .min_by(|(left_digest, left), (right_digest, right)| {
                    (
                        left.refs.get(undertaking_id).copied().unwrap_or(0),
                        left_digest.as_str(),
                    )
                        .cmp(&(
                            right.refs.get(undertaking_id).copied().unwrap_or(0),
                            right_digest.as_str(),
                        ))
                })
                .map(|(digest, _)| digest.clone());
            let Some(digest) = candidate else {
                return Err(format!(
                    "evidence cannot fit the {} byte undertaking budget",
                    self.policy.max_undertaking_physical_bytes
                ));
            };
            let remove_object = if let Some(object) = index.objects.get_mut(&digest) {
                object.refs.remove(undertaking_id);
                object.refs.is_empty()
            } else {
                false
            };
            if remove_object && let Some(object) = self.remove_object_locked(index, &digest)? {
                report.pressure_evicted_objects = report.pressure_evicted_objects.saturating_add(1);
                report.reclaimed_physical_bytes = report
                    .reclaimed_physical_bytes
                    .saturating_add(object.physical_bytes);
            }
        }
    }

    fn enforce_global_budget_locked(
        &self,
        index: &mut EvidenceIndex,
        preserve_digest: Option<&str>,
        report: &mut EvidenceMaintenanceReport,
    ) -> Result<(), String> {
        loop {
            let total = estimated_global_physical_bytes(index);
            if total <= self.policy.max_global_physical_bytes {
                return Ok(());
            }
            let candidate = index
                .objects
                .iter()
                .filter(|(digest, _)| {
                    preserve_digest.is_none_or(|preserve| preserve != digest.as_str())
                })
                .min_by(|(left_digest, left), (right_digest, right)| {
                    (
                        left.retention.eviction_priority(),
                        left.last_accessed_unix_seconds,
                        left.created_at_unix_seconds,
                        left_digest.as_str(),
                    )
                        .cmp(&(
                            right.retention.eviction_priority(),
                            right.last_accessed_unix_seconds,
                            right.created_at_unix_seconds,
                            right_digest.as_str(),
                        ))
                })
                .map(|(digest, _)| digest.clone());
            let Some(digest) = candidate else {
                return Err(format!(
                    "evidence cannot fit the {} byte global budget",
                    self.policy.max_global_physical_bytes
                ));
            };
            if let Some(object) = self.remove_object_locked(index, &digest)? {
                report.pressure_evicted_objects = report.pressure_evicted_objects.saturating_add(1);
                report.reclaimed_physical_bytes = report
                    .reclaimed_physical_bytes
                    .saturating_add(object.physical_bytes);
            }
        }
    }

    fn remove_object_locked(
        &self,
        index: &mut EvidenceIndex,
        digest: &str,
    ) -> Result<Option<EvidenceObject>, String> {
        let Some(object) = index.objects.remove(digest) else {
            return Ok(None);
        };
        let path = self.object_path(digest)?;
        let parent = path
            .parent()
            .ok_or_else(|| "Coder evidence object has no parent".to_string())?;
        require_real_directory(parent)?;
        match fs::remove_file(path) {
            Ok(()) => Ok(Some(object)),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Some(object)),
            Err(err) => {
                index.objects.insert(digest.to_string(), object);
                Err(err.to_string())
            }
        }
    }

    fn remove_orphans_locked(
        &self,
        index: &EvidenceIndex,
        report: &mut EvidenceMaintenanceReport,
    ) -> Result<(), String> {
        let objects_root = self.root.join(OBJECTS_DIR);
        let Ok(prefixes) = fs::read_dir(objects_root) else {
            return Ok(());
        };
        for prefix in prefixes.flatten() {
            let Ok(file_type) = prefix.file_type() else {
                continue;
            };
            if !file_type.is_dir() || file_type.is_symlink() {
                continue;
            }
            let Ok(files) = fs::read_dir(prefix.path()) else {
                continue;
            };
            for file in files.flatten() {
                let path = file.path();
                let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                    continue;
                };
                let Some(digest) = name.strip_suffix(".json.gz") else {
                    continue;
                };
                if validate_digest(digest).is_err() {
                    continue;
                }
                if index.objects.contains_key(digest) {
                    continue;
                }
                let bytes = physical_bytes(&path).unwrap_or(0);
                if fs::remove_file(&path).is_ok() {
                    report.orphan_objects = report.orphan_objects.saturating_add(1);
                    report.reclaimed_physical_bytes =
                        report.reclaimed_physical_bytes.saturating_add(bytes);
                }
            }
        }
        Ok(())
    }
}

pub fn maintain_default_store(data_root: &Path) -> Result<EvidenceMaintenanceReport, String> {
    CoderEvidenceStore::for_data_root(data_root).maintain()
}

fn receipt_for(
    digest: &str,
    object: &EvidenceObject,
    deduplicated: bool,
    read_tool: &str,
) -> CoderEvidenceReceipt {
    CoderEvidenceReceipt {
        reference: format!("coder-evidence:sha256:{digest}"),
        digest: format!("sha256:{digest}"),
        content_type: object.content_type.clone(),
        logical_bytes: object.logical_bytes,
        physical_bytes: object.physical_bytes,
        retention: object.retention,
        expires_at_unix_seconds: object.expires_at_unix_seconds,
        redacted: true,
        deduplicated,
        read_tool: read_tool.to_string(),
        max_read_bytes: MAX_READ_BYTES,
    }
}

fn estimated_global_physical_bytes(index: &EvidenceIndex) -> u64 {
    let object_bytes = index.objects.values().fold(0u64, |sum, object| {
        sum.saturating_add(object.physical_bytes)
    });
    let index_bytes = serde_json::to_vec_pretty(index)
        .ok()
        .and_then(|bytes| u64::try_from(bytes.len()).ok())
        .unwrap_or(u64::MAX);
    object_bytes.saturating_add(estimated_allocated_bytes(index_bytes))
}

fn estimated_allocated_bytes(logical_bytes: u64) -> u64 {
    const CONSERVATIVE_BLOCK_BYTES: u64 = 4 * 1024;
    logical_bytes.saturating_add(CONSERVATIVE_BLOCK_BYTES - 1) / CONSERVATIVE_BLOCK_BYTES
        * CONSERVATIVE_BLOCK_BYTES
}

fn compress(payload: &[u8]) -> Result<Vec<u8>, String> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(payload).map_err(|err| err.to_string())?;
    encoder.finish().map_err(|err| err.to_string())
}

fn decompress(payload: &[u8], max_bytes: u64) -> Result<Vec<u8>, String> {
    let decoder = GzDecoder::new(payload);
    let mut bounded = decoder.take(max_bytes.saturating_add(1));
    let mut output = Vec::new();
    bounded
        .read_to_end(&mut output)
        .map_err(|err| err.to_string())?;
    if u64::try_from(output.len()).unwrap_or(u64::MAX) > max_bytes {
        return Err("ephemeral evidence exceeds its decompression boundary".into());
    }
    Ok(output)
}

fn normalize_reference(reference: &str) -> Result<String, String> {
    let digest = reference
        .strip_prefix("coder-evidence:sha256:")
        .or_else(|| reference.strip_prefix("sha256:"))
        .unwrap_or(reference);
    validate_digest(digest)?;
    Ok(digest.to_string())
}

fn redact_evidence_value(value: &Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| (key.clone(), redact_evidence_value(value)))
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(redact_evidence_value).collect()),
        Value::String(value) => Value::String(redact_evidence_text(value)),
        _ => value.clone(),
    }
}

fn redact_evidence_text(value: &str) -> String {
    const TOKEN_MARKERS: &[&str] = &[
        "bearer ",
        "api_key=",
        "apikey=",
        "access_token=",
        "auth_token=",
        "token=",
        "secret=",
        "password=",
    ];
    const LINE_MARKERS: &[&str] = &["authorization:", "x-api-key:"];
    let mut redacted = value.to_string();
    for marker in TOKEN_MARKERS {
        redacted = redact_after_marker(&redacted, marker, false);
    }
    for marker in LINE_MARKERS {
        redacted = redact_after_marker(&redacted, marker, true);
    }
    redacted
}

fn redact_after_marker(value: &str, marker: &str, through_line: bool) -> String {
    let mut output = String::with_capacity(value.len());
    let mut remaining = value;
    loop {
        let lower = remaining.to_ascii_lowercase();
        let Some(start) = lower.find(marker) else {
            output.push_str(remaining);
            break;
        };
        let secret_start = start.saturating_add(marker.len());
        output.push_str(&remaining[..secret_start]);
        output.push_str("[REDACTED]");
        let tail = &remaining[secret_start..];
        let secret_end = if through_line {
            tail.find(['\r', '\n']).unwrap_or(tail.len())
        } else {
            tail.find(|character: char| {
                character.is_whitespace() || matches!(character, '"' | '\'' | ',' | ';')
            })
            .unwrap_or(tail.len())
        };
        remaining = &tail[secret_end..];
    }
    output
}

fn validate_digest(digest: &str) -> Result<(), String> {
    if digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err("invalid Coder evidence digest".into())
    }
}

fn validate_undertaking_id(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 160
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        Err("invalid Coder evidence undertaking id".into())
    } else {
        Ok(())
    }
}

fn require_real_directory(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|err| err.to_string())?;
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        Ok(())
    } else {
        Err("Coder evidence storage rejected a non-regular directory".into())
    }
}

fn require_real_file(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|err| err.to_string())?;
    if metadata.is_file() && !metadata.file_type().is_symlink() {
        Ok(())
    } else {
        Err("Coder evidence storage rejected a non-regular file".into())
    }
}

fn evidence_lock() -> Result<std::sync::MutexGuard<'static, ()>, String> {
    EVIDENCE_LOCK
        .lock()
        .map_err(|_| "Coder evidence lock is poisoned".to_string())
}

fn unix_seconds(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn next_char_boundary(value: &str, mut offset: usize) -> usize {
    while offset < value.len() && !value.is_char_boundary(offset) {
        offset = offset.saturating_add(1);
    }
    offset
}

fn previous_char_boundary(value: &str, mut offset: usize) -> usize {
    while offset > 0 && !value.is_char_boundary(offset) {
        offset = offset.saturating_sub(1);
    }
    offset
}

#[cfg(unix)]
fn physical_bytes(path: &Path) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    fs::metadata(path)
        .ok()
        .map(|metadata| metadata.blocks().saturating_mul(512))
}

#[cfg(not(unix))]
fn physical_bytes(path: &Path) -> Option<u64> {
    fs::metadata(path).ok().map(|metadata| metadata.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn policy() -> EvidencePolicy {
        EvidencePolicy {
            max_object_logical_bytes: 256 * 1024,
            max_object_physical_bytes: 256 * 1024,
            max_undertaking_physical_bytes: 256 * 1024,
            max_global_physical_bytes: 256 * 1024,
            short_ttl_seconds: 60,
            extended_ttl_seconds: 600,
        }
    }

    #[test]
    fn redacts_compresses_and_deduplicates_globally() {
        let temp = tempfile::TempDir::new().unwrap();
        let store = CoderEvidenceStore::new(temp.path().join("evidence"), policy());
        let output = json!({
            "ok": false,
            "headers": {"authorization": "Bearer secret"},
            "stderr": format!("TOKEN=raw-secret\n{}", "diagnostic".repeat(2_000)),
        });
        let first = store
            .put(
                "work-a",
                &output,
                EvidenceRetention::FailedOrNonReproducible,
                "cognition_coder_evidence_read",
            )
            .unwrap();
        let second = store
            .put(
                "work-b",
                &output,
                EvidenceRetention::FailedOrNonReproducible,
                "cognition_coder_evidence_read",
            )
            .unwrap();
        assert_eq!(first.digest, second.digest);
        assert!(second.deduplicated);
        let read = store
            .read_range("work-a", &first.reference, 0, MAX_READ_BYTES)
            .unwrap();
        assert!(read.content.contains("[REDACTED]"));
        assert!(!read.content.contains("Bearer secret"));
        assert!(!read.content.contains("raw-secret"));
        let index = store.load_index().unwrap();
        assert_eq!(index.objects.len(), 1);
        assert_eq!(index.objects.values().next().unwrap().refs.len(), 2);
    }

    #[test]
    fn range_reads_are_scoped_and_actionable() {
        let temp = tempfile::TempDir::new().unwrap();
        let store = CoderEvidenceStore::new(temp.path().join("evidence"), policy());
        let receipt = store
            .put(
                "work-a",
                &json!({"stdout": "αβγ".repeat(20_000)}),
                EvidenceRetention::SuccessfulOrReproducible,
                "cognition_coder_evidence_read",
            )
            .unwrap();
        assert!(
            store
                .read_range("work-b", &receipt.reference, 0, 64)
                .is_err()
        );
        let first = store
            .read_range("work-a", &receipt.reference, 0, 64)
            .unwrap();
        assert!(first.next_offset.is_some());
        assert!(first.end > first.offset);
        let second = store
            .read_range("work-a", &receipt.reference, first.next_offset.unwrap(), 64)
            .unwrap();
        assert_eq!(second.offset, first.end);
    }

    #[test]
    fn global_pressure_discards_short_retention_before_failed_evidence() {
        let temp = tempfile::TempDir::new().unwrap();
        let mut constrained = policy();
        constrained.max_global_physical_bytes = 12 * 1024;
        let store = CoderEvidenceStore::new(temp.path().join("evidence"), constrained);
        let failed = store
            .put(
                "work-a",
                &json!({"stderr": "failed".repeat(1_000)}),
                EvidenceRetention::FailedOrNonReproducible,
                "read",
            )
            .unwrap();
        let successful = store
            .put(
                "work-a",
                &json!({"stdout": "success".repeat(1_000)}),
                EvidenceRetention::SuccessfulOrReproducible,
                "read",
            )
            .unwrap();
        let _third = store
            .put(
                "work-a",
                &json!({"stderr": "new-failure".repeat(1_000)}),
                EvidenceRetention::FailedOrNonReproducible,
                "read",
            )
            .unwrap();
        assert!(store.read_range("work-a", &failed.reference, 0, 32).is_ok());
        assert!(
            store
                .read_range("work-a", &successful.reference, 0, 32)
                .is_err()
        );
    }

    #[test]
    fn rejects_oversized_objects_before_writing() {
        let temp = tempfile::TempDir::new().unwrap();
        let mut tiny = policy();
        tiny.max_object_logical_bytes = 100;
        let store = CoderEvidenceStore::new(temp.path().join("evidence"), tiny);
        let error = store
            .put(
                "work-a",
                &json!({"stderr": "x".repeat(1_000)}),
                EvidenceRetention::FailedOrNonReproducible,
                "read",
            )
            .unwrap_err();
        assert!(error.contains("per-object limit"));
        assert_eq!(store.load_index().unwrap().objects.len(), 0);
    }

    #[test]
    fn undertaking_budget_drops_old_references_without_duplicating_shared_blobs() {
        let temp = tempfile::TempDir::new().unwrap();
        let mut constrained = policy();
        constrained.max_undertaking_physical_bytes = 8 * 1024;
        let store = CoderEvidenceStore::new(temp.path().join("evidence"), constrained);
        for label in ["one", "two", "three"] {
            store
                .put(
                    "work-a",
                    &json!({"stdout": label.repeat(2_000)}),
                    EvidenceRetention::SuccessfulOrReproducible,
                    "read",
                )
                .unwrap();
        }
        let index = store.load_index().unwrap();
        let scoped_physical = index
            .objects
            .values()
            .filter(|object| object.refs.contains_key("work-a"))
            .fold(0u64, |total, object| {
                total.saturating_add(object.physical_bytes)
            });
        assert!(scoped_physical <= constrained.max_undertaking_physical_bytes);
    }

    #[test]
    fn maintenance_expires_objects_and_removes_orphans() {
        let temp = tempfile::TempDir::new().unwrap();
        let mut expiring = policy();
        expiring.short_ttl_seconds = 0;
        let store = CoderEvidenceStore::new(temp.path().join("evidence"), expiring);
        let receipt = store
            .put(
                "work-a",
                &json!({"stdout": "short-lived".repeat(1_000)}),
                EvidenceRetention::SuccessfulOrReproducible,
                "read",
            )
            .unwrap();
        let orphan_digest = "a".repeat(64);
        let orphan_path = store.object_path(&orphan_digest).unwrap();
        fs::create_dir_all(orphan_path.parent().unwrap()).unwrap();
        fs::write(&orphan_path, b"orphan").unwrap();

        let report = store.maintain().unwrap();
        assert_eq!(report.expired_objects, 1);
        assert_eq!(report.orphan_objects, 1);
        assert!(
            store
                .read_range("work-a", &receipt.reference, 0, 32)
                .is_err()
        );
        assert!(!orphan_path.exists());
    }

    #[cfg(unix)]
    #[test]
    fn maintenance_never_follows_object_prefix_symlinks() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::TempDir::new().unwrap();
        let root = temp.path().join("evidence");
        let outside = temp.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        let outside_object = outside.join(format!("{}.json.gz", "a".repeat(64)));
        fs::write(&outside_object, b"must-remain").unwrap();
        let store = CoderEvidenceStore::new(root.clone(), policy());
        store.ensure_layout().unwrap();
        symlink(&outside, root.join(OBJECTS_DIR).join("aa")).unwrap();

        let report = store.maintain().unwrap();
        assert_eq!(report.orphan_objects, 0);
        assert_eq!(fs::read(&outside_object).unwrap(), b"must-remain");
    }
}
