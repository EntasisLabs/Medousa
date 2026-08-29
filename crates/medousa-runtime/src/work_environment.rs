//! Runtime-neutral daemon-owned work-environment contract.
//!
//! OCI clients, host paths, container ids, and daemon storage stay behind the
//! adapter. The portable runtime carries only validated intent, an opaque local
//! handle, and the active execution fence.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use stasis::domain::runtime::blob_descriptor::BlobDescriptor;
use stasis::domain::runtime::placement::PlacementConstraints;
use stasis::domain::runtime::provenance::{ContentDigest, ProvenanceRef, ProvenanceScheme};
use stasis::domain::runtime::resource_lease::FencingToken;
use thiserror::Error;

pub const OCI_WORK_ENVIRONMENT_CAPABILITY: &str = "work_environment.oci";
pub const MAX_WORK_ENVIRONMENT_RETENTION: Duration = Duration::days(7);
pub const WORK_ENVIRONMENT_WORKSPACE_ROOT: &str = "/workspace";
pub const MAX_WORK_ENVIRONMENT_STDIN_BYTES: usize = 4 * 1024 * 1024;
pub const WORK_ENVIRONMENT_CHECKPOINT_SCHEMA_VERSION: u32 = 1;
pub const MAX_WORK_ENVIRONMENT_ARTIFACTS: usize = 64;
pub const MAX_WORK_ENVIRONMENT_ARTIFACT_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_WORK_ENVIRONMENT_ARTIFACT_TOTAL_BYTES: u64 = 256 * 1024 * 1024;

fn validate_identifier(name: &str, value: &str) -> Result<(), WorkEnvironmentError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(WorkEnvironmentError::InvalidSpec(format!(
            "{name} is required"
        )));
    }
    if value.len() > 256 {
        return Err(WorkEnvironmentError::InvalidSpec(format!(
            "{name} exceeds 256 bytes"
        )));
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(WorkEnvironmentError::InvalidSpec(format!(
            "{name} contains unsupported characters"
        )));
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkEnvironmentId(String);

impl WorkEnvironmentId {
    pub fn parse(value: impl Into<String>) -> Result<Self, WorkEnvironmentError> {
        let value = value.into();
        validate_identifier("environment_id", &value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for WorkEnvironmentId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkspaceId(String);

impl WorkspaceId {
    pub fn parse(value: impl Into<String>) -> Result<Self, WorkEnvironmentError> {
        let value = value.into();
        validate_identifier("workspace_id", &value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkEnvironmentRepository {
    pub repository_id: String,
    pub authorized_origin: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkEnvironmentImage {
    /// Registry/repository locator. The adapter always combines this with the
    /// immutable digest below; a mutable tag is never executed by itself.
    pub reference: String,
    pub digest: ContentDigest,
    pub platform: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkEnvironmentRequirements {
    #[serde(default)]
    pub placement: PlacementConstraints,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_millis: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disk_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accelerator: Option<String>,
}

impl WorkEnvironmentRequirements {
    pub fn placement_constraints(&self) -> PlacementConstraints {
        self.placement
            .clone()
            .require_capability(OCI_WORK_ENVIRONMENT_CAPABILITY)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkEnvironmentMountAccess {
    ReadOnly,
    ReadWrite,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkEnvironmentMountKind {
    Workspace,
    Artifact,
    Cache,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkEnvironmentMount {
    pub kind: WorkEnvironmentMountKind,
    pub source: ProvenanceRef,
    pub target: String,
    pub access: WorkEnvironmentMountAccess,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum WorkEnvironmentNetworkPolicy {
    #[default]
    Deny,
    AllowList {
        hosts: Vec<String>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkEnvironmentFence {
    pub stasis_attempt: FencingToken,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forge_environment_generation: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forge_execution_generation: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkEnvironmentPublication {
    pub target_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_value: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", content = "until", rename_all = "snake_case")]
pub enum WorkEnvironmentRetention {
    Delete,
    RetainWarmUntil(DateTime<Utc>),
    PreserveForDebugUntil(DateTime<Utc>),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkEnvironmentSpec {
    pub environment_id: WorkEnvironmentId,
    pub workspace_id: WorkspaceId,
    pub repository: WorkEnvironmentRepository,
    pub base_commit: String,
    pub image: WorkEnvironmentImage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint_ref: Option<WorkEnvironmentCheckpoint>,
    #[serde(default)]
    pub requirements: WorkEnvironmentRequirements,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mounts: Vec<WorkEnvironmentMount>,
    #[serde(default)]
    pub network_policy: WorkEnvironmentNetworkPolicy,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secret_refs: Vec<String>,
    pub fence: WorkEnvironmentFence,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publication: Option<WorkEnvironmentPublication>,
    pub retention: WorkEnvironmentRetention,
}

impl WorkEnvironmentSpec {
    pub fn placement_constraints(&self) -> PlacementConstraints {
        self.requirements.placement_constraints()
    }

    pub fn validate(&self, now: DateTime<Utc>) -> Result<(), WorkEnvironmentError> {
        validate_identifier("environment_id", self.environment_id.as_str())?;
        validate_identifier("workspace_id", self.workspace_id.as_str())?;
        validate_identifier("repository_id", &self.repository.repository_id)?;
        validate_reference(
            "repository authorized_origin",
            &self.repository.authorized_origin,
        )?;
        validate_git_commit(&self.base_commit)?;
        validate_image(&self.image)?;
        validate_requirements(&self.requirements)?;
        if self.fence.stasis_attempt.0 == 0 {
            return Err(WorkEnvironmentError::InvalidSpec(
                "stasis fence must be non-zero".to_string(),
            ));
        }
        if let Some(checkpoint) = self.checkpoint_ref.as_ref() {
            checkpoint.validate()?;
        }

        let mut targets = BTreeSet::new();
        for mount in &self.mounts {
            validate_reference("mount source", &mount.source.locator)?;
            validate_container_path(&mount.target)?;
            if !targets.insert(mount.target.as_str()) {
                return Err(WorkEnvironmentError::InvalidSpec(format!(
                    "duplicate mount target: {}",
                    mount.target
                )));
            }
        }

        let mut secrets = BTreeSet::new();
        for secret in &self.secret_refs {
            validate_identifier("secret_ref", secret)?;
            if !secrets.insert(secret.as_str()) {
                return Err(WorkEnvironmentError::InvalidSpec(format!(
                    "duplicate secret_ref: {secret}"
                )));
            }
        }
        if let WorkEnvironmentNetworkPolicy::AllowList { hosts } = &self.network_policy {
            if hosts.is_empty() {
                return Err(WorkEnvironmentError::InvalidSpec(
                    "network allow list must contain at least one host".to_string(),
                ));
            }
            for host in hosts {
                validate_network_host(host)?;
            }
        }
        if let Some(publication) = self.publication.as_ref() {
            validate_reference("publication target_ref", &publication.target_ref)?;
        }
        validate_retention(&self.retention, now)
    }
}

fn validate_reference(name: &str, value: &str) -> Result<(), WorkEnvironmentError> {
    let value = value.trim();
    if value.is_empty() || value.len() > 2_048 || value.chars().any(char::is_control) {
        return Err(WorkEnvironmentError::InvalidSpec(format!(
            "{name} is invalid"
        )));
    }
    Ok(())
}

fn validate_git_commit(value: &str) -> Result<(), WorkEnvironmentError> {
    let value = value.trim();
    if !matches!(value.len(), 40 | 64) || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(WorkEnvironmentError::InvalidSpec(
            "base_commit must be an immutable 40- or 64-character Git object id".to_string(),
        ));
    }
    Ok(())
}

fn validate_image(image: &WorkEnvironmentImage) -> Result<(), WorkEnvironmentError> {
    validate_reference("image reference", &image.reference)?;
    if image.reference.starts_with('-')
        || image.reference.contains('@')
        || image.reference.chars().any(char::is_whitespace)
        || !image.reference.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'/' | b':' | b'_' | b'-')
        })
    {
        return Err(WorkEnvironmentError::InvalidSpec(
            "image reference must not contain a digest or whitespace".to_string(),
        ));
    }
    if image.digest.algorithm != ContentDigest::SHA256
        || image.digest.hex.len() != 64
        || !image
            .digest
            .hex
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(WorkEnvironmentError::InvalidSpec(
            "image must use a sha256 digest".to_string(),
        ));
    }
    validate_reference("image platform", &image.platform)
}

fn validate_blob_descriptor(
    name: &str,
    descriptor: &BlobDescriptor,
) -> Result<(), WorkEnvironmentError> {
    if descriptor.digest.algorithm != ContentDigest::SHA256
        || descriptor.digest.hex.len() != 64
        || !descriptor
            .digest
            .hex
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(WorkEnvironmentError::InvalidSpec(format!(
            "{name} must use a sha256 digest"
        )));
    }
    if descriptor.size_bytes == 0 {
        return Err(WorkEnvironmentError::InvalidSpec(format!(
            "{name} must not be empty"
        )));
    }
    if let Some(media_type) = descriptor.media_type.as_deref() {
        validate_reference(&format!("{name} media_type"), media_type)?;
    }
    if let Some(transfer_hint) = descriptor.transfer_hint.as_deref() {
        validate_reference(&format!("{name} transfer_hint"), transfer_hint)?;
    }
    Ok(())
}

const fn default_max_artifact_bytes() -> u64 {
    MAX_WORK_ENVIRONMENT_ARTIFACT_BYTES
}

const fn default_max_artifact_total_bytes() -> u64 {
    MAX_WORK_ENVIRONMENT_ARTIFACT_TOTAL_BYTES
}

fn validate_requirements(
    requirements: &WorkEnvironmentRequirements,
) -> Result<(), WorkEnvironmentError> {
    for (name, value) in [
        ("cpu_millis", requirements.cpu_millis),
        ("memory_bytes", requirements.memory_bytes),
        ("disk_bytes", requirements.disk_bytes),
    ] {
        if value == Some(0) {
            return Err(WorkEnvironmentError::InvalidSpec(format!(
                "{name} must be greater than zero"
            )));
        }
    }
    if let Some(accelerator) = requirements.accelerator.as_deref() {
        validate_identifier("accelerator", accelerator)?;
    }
    Ok(())
}

fn validate_container_path(value: &str) -> Result<(), WorkEnvironmentError> {
    let has_non_normal_segment = value != "/"
        && value
            .split('/')
            .skip(1)
            .any(|segment| segment.is_empty() || matches!(segment, "." | ".."));
    if !value.starts_with('/')
        || value.len() > 1_024
        || has_non_normal_segment
        || value.chars().any(char::is_control)
    {
        return Err(WorkEnvironmentError::InvalidSpec(format!(
            "mount target must be a normalized absolute container path: {value}"
        )));
    }
    Ok(())
}

fn validate_workspace_relative_path(value: &str) -> Result<(), WorkEnvironmentError> {
    if value.is_empty()
        || value.len() > 1_024
        || !value.is_ascii()
        || value.starts_with(['/', '\\'])
        || value.contains('\\')
        || value
            .split('/')
            .any(|segment| segment.is_empty() || matches!(segment, "." | ".."))
        || value.chars().any(char::is_control)
    {
        return Err(WorkEnvironmentError::InvalidSpec(format!(
            "artifact path must be normalized and relative to /workspace: {value}"
        )));
    }
    Ok(())
}

fn validate_network_host(value: &str) -> Result<(), WorkEnvironmentError> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 253
        || value.contains('/')
        || value.contains(char::is_whitespace)
        || value.chars().any(char::is_control)
    {
        return Err(WorkEnvironmentError::InvalidSpec(format!(
            "network allow-list host is invalid: {value}"
        )));
    }
    Ok(())
}

fn validate_retention(
    retention: &WorkEnvironmentRetention,
    now: DateTime<Utc>,
) -> Result<(), WorkEnvironmentError> {
    let until = match retention {
        WorkEnvironmentRetention::Delete => return Ok(()),
        WorkEnvironmentRetention::RetainWarmUntil(until)
        | WorkEnvironmentRetention::PreserveForDebugUntil(until) => *until,
    };
    if until <= now || until > now + MAX_WORK_ENVIRONMENT_RETENTION {
        return Err(WorkEnvironmentError::InvalidSpec(
            "retention deadline must be in the future and no more than seven days away".to_string(),
        ));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkEnvironmentPhase {
    Absent,
    Materializing,
    Ready,
    Running,
    Checkpointing,
    Stopped,
    Failed,
    Released,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkEnvironmentState {
    pub environment_id: WorkEnvironmentId,
    pub phase: WorkEnvironmentPhase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint_ref: Option<ProvenanceRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub updated_at: DateTime<Utc>,
}

/// Adapter-local capability. It intentionally has no serialization contract.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct WorkEnvironmentHandle {
    environment_id: WorkEnvironmentId,
    adapter_token: Arc<str>,
}

impl WorkEnvironmentHandle {
    /// Host adapters mint handles after materializing local runtime state.
    pub fn new_local(
        environment_id: WorkEnvironmentId,
        adapter_token: impl Into<Arc<str>>,
    ) -> Self {
        Self {
            environment_id,
            adapter_token: adapter_token.into(),
        }
    }

    pub fn environment_id(&self) -> &WorkEnvironmentId {
        &self.environment_id
    }

    pub fn adapter_token(&self) -> &str {
        &self.adapter_token
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkEnvironmentExecRequest {
    pub idempotency_key: String,
    pub program: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub environment: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdin: Option<String>,
    pub timeout_seconds: u64,
    pub max_output_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkEnvironmentExecResult {
    pub execution_id: String,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub output_truncated: bool,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkEnvironmentPtyRequest {
    pub attachment_id: String,
    pub program: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    pub columns: u16,
    pub rows: u16,
}

/// Adapter-local PTY attachment capability.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct WorkEnvironmentPtyHandle {
    pub attachment_id: String,
    adapter_token: Arc<str>,
}

impl WorkEnvironmentPtyHandle {
    pub fn new_local(attachment_id: impl Into<String>, adapter_token: impl Into<Arc<str>>) -> Self {
        Self {
            attachment_id: attachment_id.into(),
            adapter_token: adapter_token.into(),
        }
    }

    pub fn adapter_token(&self) -> &str {
        &self.adapter_token
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkEnvironmentArtifactRequest {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkEnvironmentArtifact {
    pub path: String,
    pub blob: BlobDescriptor,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkEnvironmentCheckpointManifest {
    pub schema_version: u32,
    pub environment_id: WorkEnvironmentId,
    pub workspace_id: WorkspaceId,
    pub base_commit: String,
    pub checkpoint_commit: String,
    pub source_bundle: BlobDescriptor,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<WorkEnvironmentArtifact>,
    pub fence: WorkEnvironmentFence,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkEnvironmentCheckpoint {
    pub manifest: BlobDescriptor,
    pub provenance: ProvenanceRef,
}

impl WorkEnvironmentCheckpoint {
    pub fn from_manifest(manifest: BlobDescriptor) -> Self {
        let mut provenance = ProvenanceRef::cas(manifest.digest.clone());
        provenance.media_type = manifest.media_type.clone();
        Self {
            manifest,
            provenance,
        }
    }

    pub fn validate(&self) -> Result<(), WorkEnvironmentError> {
        validate_blob_descriptor("checkpoint manifest", &self.manifest)?;
        if self.provenance.scheme != ProvenanceScheme::Cas
            || self.provenance.digest.as_ref() != Some(&self.manifest.digest)
        {
            return Err(WorkEnvironmentError::InvalidSpec(
                "checkpoint provenance must identify its manifest digest".to_string(),
            ));
        }
        Ok(())
    }
}

impl WorkEnvironmentCheckpointManifest {
    pub fn validate(&self) -> Result<(), WorkEnvironmentError> {
        if self.schema_version != WORK_ENVIRONMENT_CHECKPOINT_SCHEMA_VERSION {
            return Err(WorkEnvironmentError::InvalidSpec(format!(
                "unsupported checkpoint manifest schema_version={}",
                self.schema_version
            )));
        }
        validate_identifier("checkpoint environment_id", self.environment_id.as_str())?;
        validate_identifier("checkpoint workspace_id", self.workspace_id.as_str())?;
        validate_git_commit(&self.base_commit)?;
        validate_git_commit(&self.checkpoint_commit)?;
        validate_blob_descriptor("checkpoint source bundle", &self.source_bundle)?;
        if self.fence.stasis_attempt.0 == 0 {
            return Err(WorkEnvironmentError::InvalidSpec(
                "checkpoint stasis fence must be non-zero".to_string(),
            ));
        }
        if let Some(label) = self.label.as_deref() {
            validate_reference("checkpoint label", label)?;
        }
        if self.artifacts.len() > MAX_WORK_ENVIRONMENT_ARTIFACTS {
            return Err(WorkEnvironmentError::InvalidSpec(format!(
                "checkpoint has more than {MAX_WORK_ENVIRONMENT_ARTIFACTS} artifacts"
            )));
        }
        let mut paths = BTreeSet::new();
        let mut total = 0_u64;
        for artifact in &self.artifacts {
            validate_workspace_relative_path(&artifact.path)?;
            validate_blob_descriptor("checkpoint artifact", &artifact.blob)?;
            if !paths.insert(artifact.path.as_str()) {
                return Err(WorkEnvironmentError::InvalidSpec(format!(
                    "duplicate checkpoint artifact path: {}",
                    artifact.path
                )));
            }
            total = total.checked_add(artifact.blob.size_bytes).ok_or_else(|| {
                WorkEnvironmentError::InvalidSpec(
                    "checkpoint artifact byte total overflowed".to_string(),
                )
            })?;
        }
        if total > MAX_WORK_ENVIRONMENT_ARTIFACT_TOTAL_BYTES {
            return Err(WorkEnvironmentError::InvalidSpec(format!(
                "checkpoint artifacts exceed {MAX_WORK_ENVIRONMENT_ARTIFACT_TOTAL_BYTES} bytes"
            )));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum WorkEnvironmentPublicationResult {
    Published {
        target_ref: String,
        value: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        previous: Option<String>,
    },
    AlreadyPublished {
        target_ref: String,
        value: String,
    },
    Conflict {
        target_ref: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        found: Option<String>,
        preserved_checkpoint: WorkEnvironmentCheckpoint,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkEnvironmentCheckpointPolicy {
    #[serde(default)]
    pub include_untracked: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<WorkEnvironmentArtifactRequest>,
    #[serde(default = "default_max_artifact_bytes")]
    pub max_artifact_bytes: u64,
    #[serde(default = "default_max_artifact_total_bytes")]
    pub max_artifact_total_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retain_until: Option<DateTime<Utc>>,
}

impl Default for WorkEnvironmentCheckpointPolicy {
    fn default() -> Self {
        Self {
            include_untracked: false,
            label: None,
            artifacts: Vec::new(),
            max_artifact_bytes: MAX_WORK_ENVIRONMENT_ARTIFACT_BYTES,
            max_artifact_total_bytes: MAX_WORK_ENVIRONMENT_ARTIFACT_TOTAL_BYTES,
            retain_until: None,
        }
    }
}

impl WorkEnvironmentCheckpointPolicy {
    pub fn validate(&self) -> Result<(), WorkEnvironmentError> {
        if let Some(label) = self.label.as_deref() {
            validate_reference("checkpoint label", label)?;
        }
        if self.artifacts.len() > MAX_WORK_ENVIRONMENT_ARTIFACTS {
            return Err(WorkEnvironmentError::InvalidSpec(format!(
                "checkpoint policy has more than {MAX_WORK_ENVIRONMENT_ARTIFACTS} artifacts"
            )));
        }
        if self.max_artifact_bytes == 0
            || self.max_artifact_bytes > MAX_WORK_ENVIRONMENT_ARTIFACT_BYTES
        {
            return Err(WorkEnvironmentError::InvalidSpec(format!(
                "max_artifact_bytes must be between one and {MAX_WORK_ENVIRONMENT_ARTIFACT_BYTES}"
            )));
        }
        if self.max_artifact_total_bytes == 0
            || self.max_artifact_total_bytes > MAX_WORK_ENVIRONMENT_ARTIFACT_TOTAL_BYTES
        {
            return Err(WorkEnvironmentError::InvalidSpec(format!(
                "max_artifact_total_bytes must be between one and {MAX_WORK_ENVIRONMENT_ARTIFACT_TOTAL_BYTES}"
            )));
        }
        if let Some(until) = self.retain_until
            && (until <= Utc::now() || until > Utc::now() + MAX_WORK_ENVIRONMENT_RETENTION)
        {
            return Err(WorkEnvironmentError::InvalidSpec(
                "checkpoint retention must be in the future and no more than seven days away"
                    .to_string(),
            ));
        }
        let mut paths = BTreeSet::new();
        for artifact in &self.artifacts {
            validate_workspace_relative_path(&artifact.path)?;
            if let Some(media_type) = artifact.media_type.as_deref() {
                validate_reference("artifact media_type", media_type)?;
            }
            if !paths.insert(artifact.path.as_str()) {
                return Err(WorkEnvironmentError::InvalidSpec(format!(
                    "duplicate artifact path: {}",
                    artifact.path
                )));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkEnvironmentStopReason {
    pub code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Error, Clone, Eq, PartialEq)]
pub enum WorkEnvironmentError {
    #[error("invalid work environment spec: {0}")]
    InvalidSpec(String),
    #[error("work environment not found: {0}")]
    NotFound(String),
    #[error("work environment image unavailable: {0}")]
    ImageUnavailable(String),
    #[error("work environment admission denied: {0}")]
    AdmissionDenied(String),
    #[error("work environment checkpoint missing: {0}")]
    CheckpointMissing(String),
    #[error("stale work environment fence")]
    StaleFence,
    #[error("work environment runtime unavailable: {0}")]
    RuntimeUnavailable(String),
    #[error("work environment operation unsupported: {0}")]
    Unsupported(String),
    #[error("work environment is not ready for {operation}: {phase:?}")]
    InvalidState {
        operation: &'static str,
        phase: WorkEnvironmentPhase,
    },
    #[error("work environment adapter failure: {0}")]
    Adapter(String),
}

#[async_trait]
pub trait WorkEnvironmentPort: Send + Sync {
    async fn materialize(
        &self,
        spec: WorkEnvironmentSpec,
    ) -> Result<WorkEnvironmentHandle, WorkEnvironmentError>;
    async fn inspect(
        &self,
        handle: &WorkEnvironmentHandle,
    ) -> Result<WorkEnvironmentState, WorkEnvironmentError>;
    async fn start(
        &self,
        handle: &WorkEnvironmentHandle,
        fence: &WorkEnvironmentFence,
    ) -> Result<WorkEnvironmentState, WorkEnvironmentError>;
    async fn exec(
        &self,
        handle: &WorkEnvironmentHandle,
        request: WorkEnvironmentExecRequest,
        fence: &WorkEnvironmentFence,
    ) -> Result<WorkEnvironmentExecResult, WorkEnvironmentError>;
    async fn attach_pty(
        &self,
        handle: &WorkEnvironmentHandle,
        request: WorkEnvironmentPtyRequest,
        fence: &WorkEnvironmentFence,
    ) -> Result<WorkEnvironmentPtyHandle, WorkEnvironmentError>;
    async fn checkpoint(
        &self,
        handle: &WorkEnvironmentHandle,
        policy: WorkEnvironmentCheckpointPolicy,
        fence: &WorkEnvironmentFence,
    ) -> Result<WorkEnvironmentCheckpoint, WorkEnvironmentError>;
    async fn publish(
        &self,
        handle: &WorkEnvironmentHandle,
        checkpoint: &WorkEnvironmentCheckpoint,
        fence: &WorkEnvironmentFence,
    ) -> Result<WorkEnvironmentPublicationResult, WorkEnvironmentError>;
    async fn stop(
        &self,
        handle: &WorkEnvironmentHandle,
        reason: WorkEnvironmentStopReason,
        fence: &WorkEnvironmentFence,
    ) -> Result<WorkEnvironmentState, WorkEnvironmentError>;
    async fn release(
        &self,
        handle: &WorkEnvironmentHandle,
        retention: WorkEnvironmentRetention,
        fence: &WorkEnvironmentFence,
    ) -> Result<WorkEnvironmentState, WorkEnvironmentError>;
}

/// Per-execution environment binding. The daemon chooses it during admission;
/// tools receive it through the existing execution context.
#[derive(Clone)]
pub struct WorkEnvironmentBinding {
    pub port: Arc<dyn WorkEnvironmentPort>,
    pub handle: WorkEnvironmentHandle,
    pub fence: WorkEnvironmentFence,
}

impl fmt::Debug for WorkEnvironmentBinding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WorkEnvironmentBinding")
            .field("handle", &self.handle)
            .field("fence", &self.fence)
            .finish_non_exhaustive()
    }
}

#[derive(Clone)]
struct InMemoryEntry {
    spec: WorkEnvironmentSpec,
    handle: WorkEnvironmentHandle,
    state: WorkEnvironmentState,
    executions: HashMap<String, (WorkEnvironmentExecRequest, WorkEnvironmentExecResult)>,
    checkpoint_count: u64,
}

/// Deterministic lifecycle adapter used for contract and composition tests.
#[derive(Default)]
pub struct InMemoryWorkEnvironmentPort {
    entries: Mutex<HashMap<WorkEnvironmentId, InMemoryEntry>>,
    publications: Mutex<HashMap<String, String>>,
}

impl InMemoryWorkEnvironmentPort {
    pub fn new() -> Self {
        Self::default()
    }

    fn with_entry<T>(
        &self,
        handle: &WorkEnvironmentHandle,
        operation: impl FnOnce(&mut InMemoryEntry) -> Result<T, WorkEnvironmentError>,
    ) -> Result<T, WorkEnvironmentError> {
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| WorkEnvironmentError::Adapter("in-memory lock poisoned".to_string()))?;
        let entry = entries
            .get_mut(handle.environment_id())
            .ok_or_else(|| WorkEnvironmentError::NotFound(handle.environment_id().to_string()))?;
        if entry.handle != *handle {
            return Err(WorkEnvironmentError::NotFound(
                handle.environment_id().to_string(),
            ));
        }
        operation(entry)
    }

    fn ensure_fence(
        entry: &InMemoryEntry,
        fence: &WorkEnvironmentFence,
    ) -> Result<(), WorkEnvironmentError> {
        if &entry.spec.fence != fence {
            return Err(WorkEnvironmentError::StaleFence);
        }
        Ok(())
    }
}

#[async_trait]
impl WorkEnvironmentPort for InMemoryWorkEnvironmentPort {
    async fn materialize(
        &self,
        spec: WorkEnvironmentSpec,
    ) -> Result<WorkEnvironmentHandle, WorkEnvironmentError> {
        let now = Utc::now();
        spec.validate(now)?;
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| WorkEnvironmentError::Adapter("in-memory lock poisoned".to_string()))?;
        if let Some(existing) = entries.get(&spec.environment_id) {
            if existing.spec == spec {
                return Ok(existing.handle.clone());
            }
            return Err(WorkEnvironmentError::AdmissionDenied(format!(
                "environment {} already exists with a different spec",
                spec.environment_id
            )));
        }
        let handle = WorkEnvironmentHandle::new_local(
            spec.environment_id.clone(),
            format!("memory:{}", spec.environment_id),
        );
        let state = WorkEnvironmentState {
            environment_id: spec.environment_id.clone(),
            phase: WorkEnvironmentPhase::Ready,
            checkpoint_ref: spec
                .checkpoint_ref
                .as_ref()
                .map(|checkpoint| checkpoint.provenance.clone()),
            message: None,
            updated_at: now,
        };
        entries.insert(
            spec.environment_id.clone(),
            InMemoryEntry {
                spec,
                handle: handle.clone(),
                state,
                executions: HashMap::new(),
                checkpoint_count: 0,
            },
        );
        Ok(handle)
    }

    async fn inspect(
        &self,
        handle: &WorkEnvironmentHandle,
    ) -> Result<WorkEnvironmentState, WorkEnvironmentError> {
        let entries = self
            .entries
            .lock()
            .map_err(|_| WorkEnvironmentError::Adapter("in-memory lock poisoned".to_string()))?;
        let Some(entry) = entries.get(handle.environment_id()) else {
            return Ok(WorkEnvironmentState {
                environment_id: handle.environment_id().clone(),
                phase: WorkEnvironmentPhase::Absent,
                checkpoint_ref: None,
                message: None,
                updated_at: Utc::now(),
            });
        };
        if entry.handle != *handle {
            return Err(WorkEnvironmentError::NotFound(
                handle.environment_id().to_string(),
            ));
        }
        Ok(entry.state.clone())
    }

    async fn start(
        &self,
        handle: &WorkEnvironmentHandle,
        fence: &WorkEnvironmentFence,
    ) -> Result<WorkEnvironmentState, WorkEnvironmentError> {
        self.with_entry(handle, |entry| {
            Self::ensure_fence(entry, fence)?;
            match entry.state.phase {
                WorkEnvironmentPhase::Ready
                | WorkEnvironmentPhase::Running
                | WorkEnvironmentPhase::Stopped => {
                    entry.state.phase = WorkEnvironmentPhase::Running;
                    entry.state.updated_at = Utc::now();
                    Ok(entry.state.clone())
                }
                phase => Err(WorkEnvironmentError::InvalidState {
                    operation: "start",
                    phase,
                }),
            }
        })
    }

    async fn exec(
        &self,
        handle: &WorkEnvironmentHandle,
        request: WorkEnvironmentExecRequest,
        fence: &WorkEnvironmentFence,
    ) -> Result<WorkEnvironmentExecResult, WorkEnvironmentError> {
        self.with_entry(handle, |entry| {
            Self::ensure_fence(entry, fence)?;
            if entry.state.phase != WorkEnvironmentPhase::Running {
                return Err(WorkEnvironmentError::InvalidState {
                    operation: "exec",
                    phase: entry.state.phase,
                });
            }
            validate_identifier("exec idempotency_key", &request.idempotency_key)?;
            validate_reference("exec program", &request.program)?;
            if request.timeout_seconds == 0 || request.max_output_bytes == 0 {
                return Err(WorkEnvironmentError::InvalidSpec(
                    "exec timeout and output bound must be greater than zero".to_string(),
                ));
            }
            if request
                .stdin
                .as_ref()
                .is_some_and(|stdin| stdin.len() > MAX_WORK_ENVIRONMENT_STDIN_BYTES)
            {
                return Err(WorkEnvironmentError::InvalidSpec(format!(
                    "exec stdin exceeds {MAX_WORK_ENVIRONMENT_STDIN_BYTES} bytes"
                )));
            }
            if let Some(working_directory) = request.working_directory.as_deref() {
                validate_container_path(working_directory)?;
            }
            if let Some((existing_request, result)) = entry.executions.get(&request.idempotency_key)
            {
                if existing_request == &request {
                    return Ok(result.clone());
                }
                return Err(WorkEnvironmentError::AdmissionDenied(
                    "exec idempotency key was reused with different input".to_string(),
                ));
            }
            let started_at = Utc::now();
            let result = WorkEnvironmentExecResult {
                execution_id: format!(
                    "exec:{}:{}",
                    entry.spec.environment_id, request.idempotency_key
                ),
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
                output_truncated: false,
                started_at,
                finished_at: started_at,
            };
            entry
                .executions
                .insert(request.idempotency_key.clone(), (request, result.clone()));
            Ok(result)
        })
    }

    async fn attach_pty(
        &self,
        handle: &WorkEnvironmentHandle,
        request: WorkEnvironmentPtyRequest,
        fence: &WorkEnvironmentFence,
    ) -> Result<WorkEnvironmentPtyHandle, WorkEnvironmentError> {
        self.with_entry(handle, |entry| {
            Self::ensure_fence(entry, fence)?;
            if entry.state.phase != WorkEnvironmentPhase::Running {
                return Err(WorkEnvironmentError::InvalidState {
                    operation: "attach_pty",
                    phase: entry.state.phase,
                });
            }
            validate_identifier("pty attachment_id", &request.attachment_id)?;
            validate_reference("pty program", &request.program)?;
            if request.columns == 0 || request.rows == 0 {
                return Err(WorkEnvironmentError::InvalidSpec(
                    "pty dimensions must be greater than zero".to_string(),
                ));
            }
            Ok(WorkEnvironmentPtyHandle::new_local(
                request.attachment_id,
                format!("memory-pty:{}", entry.spec.environment_id),
            ))
        })
    }

    async fn checkpoint(
        &self,
        handle: &WorkEnvironmentHandle,
        policy: WorkEnvironmentCheckpointPolicy,
        fence: &WorkEnvironmentFence,
    ) -> Result<WorkEnvironmentCheckpoint, WorkEnvironmentError> {
        self.with_entry(handle, |entry| {
            Self::ensure_fence(entry, fence)?;
            policy.validate()?;
            if !matches!(
                entry.state.phase,
                WorkEnvironmentPhase::Ready
                    | WorkEnvironmentPhase::Running
                    | WorkEnvironmentPhase::Stopped
            ) {
                return Err(WorkEnvironmentError::InvalidState {
                    operation: "checkpoint",
                    phase: entry.state.phase,
                });
            }
            entry.checkpoint_count += 1;
            let bytes =
                serde_json::to_vec(&(&entry.spec.environment_id, entry.checkpoint_count, &policy))
                    .map_err(|error| WorkEnvironmentError::Adapter(error.to_string()))?;
            let manifest = BlobDescriptor::from_bytes(&bytes)
                .with_media_type("application/vnd.medousa.work-environment-checkpoint+json");
            let checkpoint = WorkEnvironmentCheckpoint::from_manifest(manifest);
            entry.state.checkpoint_ref = Some(checkpoint.provenance.clone());
            entry.state.updated_at = Utc::now();
            Ok(checkpoint)
        })
    }

    async fn publish(
        &self,
        handle: &WorkEnvironmentHandle,
        checkpoint: &WorkEnvironmentCheckpoint,
        fence: &WorkEnvironmentFence,
    ) -> Result<WorkEnvironmentPublicationResult, WorkEnvironmentError> {
        checkpoint.validate()?;
        let publication = self.with_entry(handle, |entry| {
            Self::ensure_fence(entry, fence)?;
            if entry.state.checkpoint_ref.as_ref() != Some(&checkpoint.provenance) {
                return Err(WorkEnvironmentError::CheckpointMissing(
                    checkpoint.provenance.compact(),
                ));
            }
            entry.spec.publication.clone().ok_or_else(|| {
                WorkEnvironmentError::Unsupported(
                    "environment has no publication target".to_string(),
                )
            })
        })?;
        let value = checkpoint.provenance.compact();
        let mut publications = self
            .publications
            .lock()
            .map_err(|_| WorkEnvironmentError::Adapter("publication lock poisoned".to_string()))?;
        let found = publications.get(&publication.target_ref).cloned();
        if found.as_deref() == Some(value.as_str()) {
            return Ok(WorkEnvironmentPublicationResult::AlreadyPublished {
                target_ref: publication.target_ref,
                value,
            });
        }
        if found != publication.expected_value {
            return Ok(WorkEnvironmentPublicationResult::Conflict {
                target_ref: publication.target_ref,
                expected: publication.expected_value,
                found,
                preserved_checkpoint: checkpoint.clone(),
            });
        }
        publications.insert(publication.target_ref.clone(), value.clone());
        Ok(WorkEnvironmentPublicationResult::Published {
            target_ref: publication.target_ref,
            value,
            previous: found,
        })
    }

    async fn stop(
        &self,
        handle: &WorkEnvironmentHandle,
        reason: WorkEnvironmentStopReason,
        fence: &WorkEnvironmentFence,
    ) -> Result<WorkEnvironmentState, WorkEnvironmentError> {
        self.with_entry(handle, |entry| {
            Self::ensure_fence(entry, fence)?;
            validate_identifier("stop reason code", &reason.code)?;
            match entry.state.phase {
                WorkEnvironmentPhase::Ready
                | WorkEnvironmentPhase::Running
                | WorkEnvironmentPhase::Stopped => {
                    entry.state.phase = WorkEnvironmentPhase::Stopped;
                    entry.state.message = reason.message;
                    entry.state.updated_at = Utc::now();
                    Ok(entry.state.clone())
                }
                phase => Err(WorkEnvironmentError::InvalidState {
                    operation: "stop",
                    phase,
                }),
            }
        })
    }

    async fn release(
        &self,
        handle: &WorkEnvironmentHandle,
        retention: WorkEnvironmentRetention,
        fence: &WorkEnvironmentFence,
    ) -> Result<WorkEnvironmentState, WorkEnvironmentError> {
        self.with_entry(handle, |entry| {
            Self::ensure_fence(entry, fence)?;
            validate_retention(&retention, Utc::now())?;
            entry.state.phase = match retention {
                WorkEnvironmentRetention::Delete => WorkEnvironmentPhase::Released,
                WorkEnvironmentRetention::RetainWarmUntil(_)
                | WorkEnvironmentRetention::PreserveForDebugUntil(_) => {
                    WorkEnvironmentPhase::Stopped
                }
            };
            entry.state.updated_at = Utc::now();
            Ok(entry.state.clone())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_spec(now: DateTime<Utc>) -> WorkEnvironmentSpec {
        WorkEnvironmentSpec {
            environment_id: WorkEnvironmentId::parse("env-1").unwrap(),
            workspace_id: WorkspaceId::parse("workspace-1").unwrap(),
            repository: WorkEnvironmentRepository {
                repository_id: "repo-1".to_string(),
                authorized_origin: "ssh://git@example.invalid/repo.git".to_string(),
            },
            base_commit: "a".repeat(40),
            image: WorkEnvironmentImage {
                reference: "registry.example.invalid/medousa/dev".to_string(),
                digest: ContentDigest::sha256_bytes(b"image"),
                platform: "linux/arm64".to_string(),
            },
            checkpoint_ref: None,
            requirements: WorkEnvironmentRequirements::default(),
            mounts: vec![WorkEnvironmentMount {
                kind: WorkEnvironmentMountKind::Workspace,
                source: ProvenanceRef::cas(ContentDigest::sha256_bytes(b"workspace")),
                target: "/workspace".to_string(),
                access: WorkEnvironmentMountAccess::ReadWrite,
            }],
            network_policy: WorkEnvironmentNetworkPolicy::Deny,
            secret_refs: vec!["secret:github".to_string()],
            fence: WorkEnvironmentFence {
                stasis_attempt: FencingToken(1),
                forge_environment_generation: Some(2),
                forge_execution_generation: Some(3),
            },
            publication: Some(WorkEnvironmentPublication {
                target_ref: "refs/heads/codex/work".to_string(),
                expected_value: Some("b".repeat(40)),
            }),
            retention: WorkEnvironmentRetention::RetainWarmUntil(now + Duration::hours(1)),
        }
    }

    fn exec_request() -> WorkEnvironmentExecRequest {
        WorkEnvironmentExecRequest {
            idempotency_key: "run-tests".to_string(),
            program: "cargo".to_string(),
            args: vec!["test".to_string()],
            working_directory: Some("/workspace".to_string()),
            environment: BTreeMap::new(),
            stdin: None,
            timeout_seconds: 300,
            max_output_bytes: 1024 * 1024,
        }
    }

    #[test]
    fn spec_requires_immutable_inputs_and_oci_placement() {
        let now = Utc::now();
        let spec = sample_spec(now);
        spec.validate(now).unwrap();
        assert!(
            spec.placement_constraints()
                .required_capabilities
                .contains(OCI_WORK_ENVIRONMENT_CAPABILITY)
        );

        let mut invalid = spec;
        invalid.base_commit = "main".to_string();
        assert!(matches!(
            invalid.validate(now),
            Err(WorkEnvironmentError::InvalidSpec(_))
        ));
    }

    #[tokio::test]
    async fn fake_adapter_exercises_idempotent_lifecycle() {
        let now = Utc::now();
        let spec = sample_spec(now);
        let fence = spec.fence.clone();
        let adapter = InMemoryWorkEnvironmentPort::new();
        let handle = adapter.materialize(spec.clone()).await.unwrap();
        assert_eq!(
            adapter.materialize(spec).await.unwrap(),
            handle,
            "materialization is idempotent"
        );
        assert_eq!(
            adapter.start(&handle, &fence).await.unwrap().phase,
            WorkEnvironmentPhase::Running
        );
        let first = adapter.exec(&handle, exec_request(), &fence).await.unwrap();
        let replay = adapter.exec(&handle, exec_request(), &fence).await.unwrap();
        assert_eq!(first, replay);
        let checkpoint = adapter
            .checkpoint(&handle, WorkEnvironmentCheckpointPolicy::default(), &fence)
            .await
            .unwrap();
        assert_eq!(
            adapter.inspect(&handle).await.unwrap().checkpoint_ref,
            Some(checkpoint.provenance.clone())
        );
        let published = adapter.publish(&handle, &checkpoint, &fence).await.unwrap();
        assert!(matches!(
            published,
            WorkEnvironmentPublicationResult::Conflict {
                expected: Some(_),
                found: None,
                ..
            }
        ));
        assert_eq!(
            adapter
                .stop(
                    &handle,
                    WorkEnvironmentStopReason {
                        code: "complete".to_string(),
                        message: None,
                    },
                    &fence,
                )
                .await
                .unwrap()
                .phase,
            WorkEnvironmentPhase::Stopped
        );
    }

    #[tokio::test]
    async fn fake_adapter_rejects_stale_fence() {
        let spec = sample_spec(Utc::now());
        let adapter = InMemoryWorkEnvironmentPort::new();
        let handle = adapter.materialize(spec).await.unwrap();
        let stale = WorkEnvironmentFence {
            stasis_attempt: FencingToken(2),
            forge_environment_generation: Some(2),
            forge_execution_generation: Some(3),
        };
        assert_eq!(
            adapter.start(&handle, &stale).await,
            Err(WorkEnvironmentError::StaleFence)
        );
    }

    #[tokio::test]
    async fn fake_adapter_reports_absent_without_materializing() {
        let adapter = InMemoryWorkEnvironmentPort::new();
        let handle = WorkEnvironmentHandle::new_local(
            WorkEnvironmentId::parse("env-absent").unwrap(),
            "memory:env-absent",
        );

        assert_eq!(
            adapter.inspect(&handle).await.unwrap().phase,
            WorkEnvironmentPhase::Absent
        );
    }
}
