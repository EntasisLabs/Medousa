//! Full-daemon Docker/OCI work-environment adapter.
//!
//! The daemon remains the authority. Docker owns only the disposable process
//! and filesystem locality described by the runtime-neutral environment port.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use medousa_forge::execution::{
    ExecutionClass, ForgeExecutionService, MAX_CAPTURE_BYTES, supervise_command,
    supervise_command_with_input, supervise_git,
};
use medousa_forge::git::{CheckpointAuthor, GitEngine};
use medousa_runtime::{
    MAX_WORK_ENVIRONMENT_ARTIFACT_BYTES, MAX_WORK_ENVIRONMENT_STDIN_BYTES,
    WORK_ENVIRONMENT_CHECKPOINT_SCHEMA_VERSION, WORK_ENVIRONMENT_WORKSPACE_ROOT,
    WorkEnvironmentArtifact, WorkEnvironmentCheckpoint, WorkEnvironmentCheckpointManifest,
    WorkEnvironmentCheckpointPolicy, WorkEnvironmentError, WorkEnvironmentExecRequest,
    WorkEnvironmentExecResult, WorkEnvironmentFence, WorkEnvironmentHandle, WorkEnvironmentId,
    WorkEnvironmentMountAccess, WorkEnvironmentMountKind, WorkEnvironmentNetworkPolicy,
    WorkEnvironmentPhase, WorkEnvironmentPort, WorkEnvironmentPtyHandle, WorkEnvironmentPtyRequest,
    WorkEnvironmentPublicationResult, WorkEnvironmentRetention, WorkEnvironmentSpec,
    WorkEnvironmentState, WorkEnvironmentStopReason,
};
use medousa_store::{StorePath, StoreRoot};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use stasis::ports::outbound::runtime::blob_transfer::BlobTransferPort;

use super::blob_transfer_host::FsBlobTransferPort;
use super::work_environment_publication_host::{
    FsWorkEnvironmentPublicationStore, PublicationCasOutcome,
};

const MANAGED_LABEL: &str = "io.medousa.work_environment.managed";
const ENVIRONMENT_LABEL: &str = "io.medousa.work_environment.id";
const WORKSPACE_LABEL: &str = "io.medousa.work_environment.workspace";
const FENCE_LABEL: &str = "io.medousa.work_environment.fence";
const CONTROL_OUTPUT_BYTES: usize = 256 * 1024;
const CONTROL_TIMEOUT: Duration = Duration::from_secs(30);
const RUNTIME_PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const GIT_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const MAX_EXEC_TIMEOUT_SECONDS: u64 = 60 * 60;
const MAX_EXEC_OUTPUT_BYTES: usize = 1024 * 1024;
const MAX_EXECUTION_RECORD_BYTES: u64 = 8 * 1024 * 1024;
const MAX_MANIFEST_BYTES: u64 = 256 * 1024;
const MAX_CHECKPOINT_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_CHECKPOINT_BUNDLE_BYTES: u64 = 512 * 1024 * 1024;
const CHECKPOINT_GIT_ADMISSION_BYTES: usize = 1024 * 1024;
const WORKSPACE_TARGET: &str = WORK_ENVIRONMENT_WORKSPACE_ROOT;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct LocalEnvironmentRecord {
    spec: WorkEnvironmentSpec,
    container_name: String,
    state: WorkEnvironmentState,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct StoredExecution {
    request: WorkEnvironmentExecRequest,
    result: Option<WorkEnvironmentExecResult>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WorkEnvironmentReconcileReport {
    pub recovered: Vec<String>,
    pub missing: Vec<String>,
    pub corrupt_preserved: Vec<String>,
    pub unknown_preserved: Vec<String>,
}

#[derive(Debug)]
struct CommandOutput {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    truncated: bool,
    status: std::process::ExitStatus,
}

/// Initial OCI adapter: Docker-compatible CLI, local Git, prebuilt images.
pub struct DockerCliWorkEnvironmentPort {
    docker: PathBuf,
    git: PathBuf,
    root: PathBuf,
    execution: Arc<ForgeExecutionService>,
    blobs: Arc<FsBlobTransferPort>,
    publications: Arc<FsWorkEnvironmentPublicationStore>,
    lifecycle: tokio::sync::Mutex<()>,
}

impl DockerCliWorkEnvironmentPort {
    /// Detect the host boundary without making daemon startup depend on it.
    /// `Ok(None)` means this daemon must decline OCI placement.
    pub async fn detect(
        root: PathBuf,
        execution: Arc<ForgeExecutionService>,
    ) -> Result<Option<Arc<Self>>, WorkEnvironmentError> {
        let (docker, git) = execution
            .run(ExecutionClass::StoreIo, 64, || {
                let docker = medousa_host::find_command_in_path(if cfg!(windows) {
                    "docker.exe"
                } else {
                    "docker"
                });
                let git = medousa_host::find_command_in_path(if cfg!(windows) {
                    "git.exe"
                } else {
                    "git"
                });
                Ok((docker, git))
            })
            .await
            .map_err(map_forge_error)?;
        let Some(docker) = docker else {
            return Ok(None);
        };
        let Some(git) = git else {
            return Ok(None);
        };
        let blobs = FsBlobTransferPort::open(&root.join("durable/blobs"), execution.clone())
            .map_err(|error| WorkEnvironmentError::Adapter(error.to_string()))?;
        let publications = FsWorkEnvironmentPublicationStore::open(
            &root.join("durable/publications"),
            execution.clone(),
        )?;
        let adapter = Arc::new(Self {
            docker,
            git,
            root,
            execution,
            blobs,
            publications,
            lifecycle: tokio::sync::Mutex::new(()),
        });
        adapter.ensure_root().await?;
        let garbage = adapter
            .blobs
            .collect_garbage(Utc::now(), Duration::from_secs(60 * 60))
            .await
            .map_err(|error| WorkEnvironmentError::Adapter(error.to_string()))?;
        if garbage.deleted_objects > 0 || garbage.expired_roots > 0 {
            tracing::info!(
                deleted_objects = garbage.deleted_objects,
                expired_roots = garbage.expired_roots,
                active_roots = garbage.active_roots,
                "collected unreferenced work-environment content"
            );
        }
        let probe = adapter
            .run_docker(
                vec![
                    "version".into(),
                    "--format".into(),
                    "{{.Server.Version}}".into(),
                ],
                RUNTIME_PROBE_TIMEOUT,
                CONTROL_OUTPUT_BYTES,
            )
            .await?;
        if !probe.status.success() {
            return Ok(None);
        }
        Ok(Some(adapter))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn environments_root(&self) -> PathBuf {
        self.root.join("environments")
    }

    fn environment_root(&self, environment_id: &WorkEnvironmentId) -> PathBuf {
        self.environments_root().join(environment_id.as_str())
    }

    fn manifest_path(&self, environment_id: &WorkEnvironmentId) -> PathBuf {
        self.environment_root(environment_id).join("state.json")
    }

    fn workspace_path(&self, environment_id: &WorkEnvironmentId) -> PathBuf {
        self.environment_root(environment_id).join("workspace")
    }

    fn execution_path(
        &self,
        environment_id: &WorkEnvironmentId,
        fence: &WorkEnvironmentFence,
        key: &str,
    ) -> PathBuf {
        let digest = Sha256::digest(key.as_bytes());
        self.environment_root(environment_id)
            .join("executions")
            .join(format!("{}-{digest:x}.json", fence_fingerprint(fence)))
    }

    async fn ensure_root(&self) -> Result<(), WorkEnvironmentError> {
        let root = self.environments_root();
        self.execution
            .run(ExecutionClass::StoreIo, 64 * 1024, move || {
                std::fs::create_dir_all(root)?;
                Ok(())
            })
            .await
            .map_err(map_forge_error)
    }

    async fn load_record(
        &self,
        environment_id: &WorkEnvironmentId,
    ) -> Result<Option<LocalEnvironmentRecord>, WorkEnvironmentError> {
        let path = self.manifest_path(environment_id);
        self.execution
            .run(ExecutionClass::StoreIo, 256 * 1024, move || {
                let bytes = match read_bounded(&path, MAX_MANIFEST_BYTES) {
                    Ok(bytes) => bytes,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                    Err(error) => return Err(error.into()),
                };
                Ok(Some(serde_json::from_slice(&bytes)?))
            })
            .await
            .map_err(map_forge_error)
    }

    async fn save_record(
        &self,
        record: &LocalEnvironmentRecord,
    ) -> Result<(), WorkEnvironmentError> {
        let directory = self.environment_root(&record.spec.environment_id);
        let path = self.manifest_path(&record.spec.environment_id);
        let bytes = serde_json::to_vec_pretty(record)
            .map_err(|error| WorkEnvironmentError::Adapter(error.to_string()))?;
        self.execution
            .run(ExecutionClass::StoreIo, bytes.len().max(1), move || {
                std::fs::create_dir_all(directory)?;
                crate::session::atomic_write(&path, &bytes)?;
                Ok(())
            })
            .await
            .map_err(map_forge_error)
    }

    async fn load_execution(
        &self,
        environment_id: &WorkEnvironmentId,
        fence: &WorkEnvironmentFence,
        key: &str,
    ) -> Result<Option<StoredExecution>, WorkEnvironmentError> {
        let path = self.execution_path(environment_id, fence, key);
        self.execution
            .run(
                ExecutionClass::WorkEnvironment,
                MAX_CAPTURE_BYTES,
                move || {
                    let bytes = match read_bounded(&path, MAX_EXECUTION_RECORD_BYTES) {
                        Ok(bytes) => bytes,
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                            return Ok(None);
                        }
                        Err(error) => return Err(error.into()),
                    };
                    Ok(Some(serde_json::from_slice(&bytes)?))
                },
            )
            .await
            .map_err(map_forge_error)
    }

    async fn save_execution(
        &self,
        environment_id: &WorkEnvironmentId,
        fence: &WorkEnvironmentFence,
        key: &str,
        execution_record: &StoredExecution,
    ) -> Result<(), WorkEnvironmentError> {
        let path = self.execution_path(environment_id, fence, key);
        let directory = path
            .parent()
            .expect("execution path always has a parent")
            .to_path_buf();
        let bytes = serde_json::to_vec(execution_record)
            .map_err(|error| WorkEnvironmentError::Adapter(error.to_string()))?;
        if bytes.len() as u64 > MAX_EXECUTION_RECORD_BYTES {
            return Err(WorkEnvironmentError::Adapter(
                "execution record exceeded its durable storage bound".into(),
            ));
        }
        self.execution
            .run(ExecutionClass::WorkEnvironment, bytes.len(), move || {
                std::fs::create_dir_all(directory)?;
                crate::session::atomic_write(&path, &bytes)?;
                Ok(())
            })
            .await
            .map_err(map_forge_error)
    }

    async fn run_docker(
        &self,
        args: Vec<String>,
        timeout: Duration,
        max_output: usize,
    ) -> Result<CommandOutput, WorkEnvironmentError> {
        self.run_docker_with_environment(args, Vec::new(), None, timeout, max_output)
            .await
    }

    async fn run_docker_with_environment(
        &self,
        args: Vec<String>,
        environment: Vec<(String, String)>,
        stdin: Option<Vec<u8>>,
        timeout: Duration,
        max_output: usize,
    ) -> Result<CommandOutput, WorkEnvironmentError> {
        let docker = self.docker.clone();
        let result = self
            .execution
            .run_async(
                ExecutionClass::WorkEnvironment,
                max_output.max(1),
                None,
                async move {
                    match stdin {
                        Some(stdin) => {
                            supervise_command_with_input(
                                docker,
                                None,
                                args,
                                environment,
                                stdin,
                                timeout,
                                max_output,
                            )
                            .await
                        }
                        None => {
                            supervise_command(docker, None, args, environment, timeout, max_output)
                                .await
                        }
                    }
                },
            )
            .await
            .map_err(map_forge_error)?;
        Ok(CommandOutput {
            stdout: result.0,
            stderr: result.1,
            truncated: result.2,
            status: result.3,
        })
    }

    async fn docker_success(
        &self,
        args: Vec<String>,
        timeout: Duration,
        max_output: usize,
    ) -> Result<CommandOutput, WorkEnvironmentError> {
        let output = self.run_docker(args, timeout, max_output).await?;
        if output.truncated {
            return Err(WorkEnvironmentError::Adapter(
                "OCI runtime control output exceeded its bound".into(),
            ));
        }
        if !output.status.success() {
            return Err(WorkEnvironmentError::Adapter(command_failure(&output)));
        }
        Ok(output)
    }

    async fn run_git(
        &self,
        cwd: PathBuf,
        args: Vec<String>,
    ) -> Result<Vec<u8>, WorkEnvironmentError> {
        let git = self.git.clone();
        let repo_key = cwd.display().to_string();
        let (stdout, _, truncated) = self
            .execution
            .run_async(
                ExecutionClass::NetworkGit,
                MAX_CAPTURE_BYTES,
                Some(repo_key),
                async move { supervise_git(git, cwd, args, GIT_TIMEOUT, MAX_CAPTURE_BYTES).await },
            )
            .await
            .map_err(map_forge_error)?;
        if truncated {
            return Err(WorkEnvironmentError::Adapter(
                "Git materialization output exceeded its bound".into(),
            ));
        }
        Ok(stdout)
    }

    fn handle_for(record: &LocalEnvironmentRecord) -> WorkEnvironmentHandle {
        WorkEnvironmentHandle::new_local(
            record.spec.environment_id.clone(),
            format!("docker:{}", record.container_name),
        )
    }

    fn ensure_handle(
        record: &LocalEnvironmentRecord,
        handle: &WorkEnvironmentHandle,
    ) -> Result<(), WorkEnvironmentError> {
        if handle.environment_id() != &record.spec.environment_id
            || handle.adapter_token() != format!("docker:{}", record.container_name)
        {
            return Err(WorkEnvironmentError::NotFound(
                handle.environment_id().to_string(),
            ));
        }
        Ok(())
    }

    fn ensure_fence(
        record: &LocalEnvironmentRecord,
        fence: &WorkEnvironmentFence,
    ) -> Result<(), WorkEnvironmentError> {
        if &record.spec.fence != fence {
            return Err(WorkEnvironmentError::StaleFence);
        }
        Ok(())
    }

    fn validate_supported_spec(
        &self,
        spec: &WorkEnvironmentSpec,
    ) -> Result<(), WorkEnvironmentError> {
        if !matches!(spec.network_policy, WorkEnvironmentNetworkPolicy::Deny) {
            return Err(WorkEnvironmentError::Unsupported(
                "the Docker CLI adapter currently supports deny-network environments only".into(),
            ));
        }
        if spec.requirements.accelerator.is_some() {
            return Err(WorkEnvironmentError::AdmissionDenied(
                "the initial Docker CLI adapter does not advertise an accelerator".into(),
            ));
        }
        for mount in &spec.mounts {
            if mount.kind != WorkEnvironmentMountKind::Workspace
                || mount.target != WORKSPACE_TARGET
                || mount.access != WorkEnvironmentMountAccess::ReadWrite
            {
                return Err(WorkEnvironmentError::Unsupported(format!(
                    "the initial adapter only materializes a read-write {WORKSPACE_TARGET} mount"
                )));
            }
        }
        Ok(())
    }

    async fn admit_disk(&self, spec: &WorkEnvironmentSpec) -> Result<(), WorkEnvironmentError> {
        let Some(required) = spec.requirements.disk_bytes else {
            return Ok(());
        };
        let root = self.root.clone();
        let available = self
            .execution
            .run(ExecutionClass::StoreIo, 64, move || {
                Ok(fs2::available_space(root)?)
            })
            .await
            .map_err(map_forge_error)?;
        if available < required {
            return Err(WorkEnvironmentError::AdmissionDenied(format!(
                "environment requires {required} disk bytes but only {available} are available"
            )));
        }
        Ok(())
    }

    async fn ensure_image(
        &self,
        spec: &WorkEnvironmentSpec,
    ) -> Result<String, WorkEnvironmentError> {
        let image = format!(
            "{}@{}:{}",
            spec.image.reference, spec.image.digest.algorithm, spec.image.digest.hex
        );
        let output = self
            .run_docker(
                vec![
                    "image".into(),
                    "inspect".into(),
                    "--format".into(),
                    "{{.Os}}/{{.Architecture}}".into(),
                    image.clone(),
                ],
                CONTROL_TIMEOUT,
                CONTROL_OUTPUT_BYTES,
            )
            .await?;
        if !output.status.success() {
            return Err(WorkEnvironmentError::ImageUnavailable(command_failure(
                &output,
            )));
        }
        let actual = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if actual != spec.image.platform {
            return Err(WorkEnvironmentError::AdmissionDenied(format!(
                "image platform is {actual}, requested {}",
                spec.image.platform
            )));
        }
        Ok(image)
    }

    async fn load_checkpoint_manifest(
        &self,
        checkpoint: &WorkEnvironmentCheckpoint,
    ) -> Result<WorkEnvironmentCheckpointManifest, WorkEnvironmentError> {
        checkpoint.validate()?;
        if checkpoint.manifest.size_bytes > MAX_CHECKPOINT_MANIFEST_BYTES {
            return Err(WorkEnvironmentError::CheckpointMissing(format!(
                "manifest exceeds {MAX_CHECKPOINT_MANIFEST_BYTES} bytes"
            )));
        }
        let bytes = self
            .blobs
            .get(&checkpoint.manifest)
            .await
            .map_err(|error| WorkEnvironmentError::CheckpointMissing(error.to_string()))?;
        let manifest: WorkEnvironmentCheckpointManifest =
            serde_json::from_slice(&bytes).map_err(|error| {
                WorkEnvironmentError::CheckpointMissing(format!(
                    "decode checkpoint manifest: {error}"
                ))
            })?;
        manifest.validate()?;
        Ok(manifest)
    }

    async fn verify_checkpoint_content(
        &self,
        checkpoint: &WorkEnvironmentCheckpoint,
    ) -> Result<WorkEnvironmentCheckpointManifest, WorkEnvironmentError> {
        let manifest = self.load_checkpoint_manifest(checkpoint).await?;
        let descriptors = std::iter::once(&manifest.source_bundle)
            .chain(manifest.artifacts.iter().map(|artifact| &artifact.blob));
        for descriptor in descriptors {
            let exists = self
                .blobs
                .exists(descriptor)
                .await
                .map_err(|error| WorkEnvironmentError::CheckpointMissing(error.to_string()))?;
            if !exists {
                return Err(WorkEnvironmentError::CheckpointMissing(format!(
                    "{}:{}",
                    descriptor.digest.algorithm, descriptor.digest.hex
                )));
            }
        }
        Ok(manifest)
    }

    async fn restore_checkpoint(
        &self,
        workspace: &Path,
        spec: &WorkEnvironmentSpec,
        manifest: &WorkEnvironmentCheckpointManifest,
    ) -> Result<(), WorkEnvironmentError> {
        if manifest.workspace_id != spec.workspace_id || manifest.base_commit != spec.base_commit {
            return Err(WorkEnvironmentError::CheckpointMissing(
                "checkpoint workspace or base does not match the environment spec".to_string(),
            ));
        }
        if manifest.source_bundle.size_bytes > MAX_CHECKPOINT_BUNDLE_BYTES {
            return Err(WorkEnvironmentError::CheckpointMissing(format!(
                "source bundle exceeds {MAX_CHECKPOINT_BUNDLE_BYTES} bytes"
            )));
        }
        let environment_root = self.environment_root(&spec.environment_id);
        let environment_store = Arc::new(StoreRoot::open_nofollow(&environment_root).map_err(
            |error| {
                WorkEnvironmentError::CheckpointMissing(format!(
                    "open environment for checkpoint restore: {error}"
                ))
            },
        )?);
        let staging_name = format!("restore-{}.bundle", uuid::Uuid::new_v4().simple());
        let staging_path = StorePath::parse(&staging_name).map_err(|error| {
            WorkEnvironmentError::CheckpointMissing(format!("restore bundle path: {error}"))
        })?;
        self.blobs
            .materialize_file(
                &manifest.source_bundle,
                Arc::clone(&environment_store),
                staging_path.clone(),
                MAX_CHECKPOINT_BUNDLE_BYTES,
            )
            .await
            .map_err(|error| WorkEnvironmentError::CheckpointMissing(error.to_string()))?;
        let staging = environment_root.join(&staging_name);
        let git = GitEngine::with_binary(self.git.clone());
        let workspace_for_git = workspace.to_path_buf();
        let staging_for_git = staging.clone();
        let checkpoint = medousa_forge::model::GitOid::new(&manifest.checkpoint_commit);
        let repo_key = workspace.display().to_string();
        let import_result = self
            .execution
            .run_on_repo(
                ExecutionClass::LocalMutation,
                CHECKPOINT_GIT_ADMISSION_BYTES,
                Some(repo_key),
                move || {
                    git.import_checkpoint_bundle(&workspace_for_git, &staging_for_git, &checkpoint)
                },
            )
            .await
            .map_err(map_forge_error);
        let _ = environment_store.remove_file(&staging_path);
        import_result?;

        let workspace_store = Arc::new(StoreRoot::open_nofollow(workspace).map_err(|error| {
            WorkEnvironmentError::CheckpointMissing(format!("open restored workspace: {error}"))
        })?);
        for artifact in &manifest.artifacts {
            let path = StorePath::parse(&artifact.path).map_err(|error| {
                WorkEnvironmentError::CheckpointMissing(format!(
                    "invalid checkpoint artifact path: {error}"
                ))
            })?;
            self.blobs
                .materialize_file(
                    &artifact.blob,
                    Arc::clone(&workspace_store),
                    path,
                    MAX_WORK_ENVIRONMENT_ARTIFACT_BYTES,
                )
                .await
                .map_err(|error| WorkEnvironmentError::CheckpointMissing(error.to_string()))?;
        }
        if let Some(checkpoint) = spec.checkpoint_ref.as_ref() {
            let mut roots = vec![checkpoint.manifest.clone(), manifest.source_bundle.clone()];
            roots.extend(
                manifest
                    .artifacts
                    .iter()
                    .map(|artifact| artifact.blob.clone()),
            );
            self.blobs
                .pin_root(
                    &format!("checkpoint:{}", checkpoint.manifest.digest.hex),
                    roots,
                    None,
                )
                .await
                .map_err(|error| WorkEnvironmentError::Adapter(error.to_string()))?;
        }
        Ok(())
    }

    async fn materialize_repository(
        &self,
        spec: &WorkEnvironmentSpec,
    ) -> Result<PathBuf, WorkEnvironmentError> {
        let checkpoint_manifest = match spec.checkpoint_ref.as_ref() {
            Some(checkpoint) => Some(self.load_checkpoint_manifest(checkpoint).await?),
            None => None,
        };
        let expected_head = checkpoint_manifest
            .as_ref()
            .map(|manifest| manifest.checkpoint_commit.as_str())
            .unwrap_or(spec.base_commit.as_str());
        let workspace = self.workspace_path(&spec.environment_id);
        let workspace_for_check = workspace.clone();
        let (is_repository, workspace_exists) = self
            .execution
            .run(ExecutionClass::StoreIo, 64, move || {
                Ok((
                    workspace_for_check.join(".git").is_dir(),
                    workspace_for_check.exists(),
                ))
            })
            .await
            .map_err(map_forge_error)?;
        if is_repository {
            let head = self
                .run_git(workspace.clone(), vec!["rev-parse".into(), "HEAD".into()])
                .await?;
            if String::from_utf8_lossy(&head).trim() != expected_head {
                return Err(WorkEnvironmentError::AdmissionDenied(
                    "preserved workspace is at a different commit".into(),
                ));
            }
            return Ok(workspace);
        }
        if workspace_exists {
            return Err(WorkEnvironmentError::AdmissionDenied(format!(
                "preserving unrecognized workspace at {}",
                workspace.display()
            )));
        }
        let parent = workspace
            .parent()
            .expect("workspace path always has a parent")
            .to_path_buf();
        self.run_git(
            parent,
            vec![
                "clone".into(),
                "--no-checkout".into(),
                "--".into(),
                spec.repository.authorized_origin.clone(),
                workspace.display().to_string(),
            ],
        )
        .await?;
        self.run_git(
            workspace.clone(),
            vec![
                "checkout".into(),
                "--detach".into(),
                spec.base_commit.clone(),
            ],
        )
        .await?;
        if let Some(manifest) = checkpoint_manifest.as_ref() {
            self.restore_checkpoint(&workspace, spec, manifest).await?;
        }
        Ok(workspace)
    }

    async fn create_durable_checkpoint(
        &self,
        record: &LocalEnvironmentRecord,
        policy: &WorkEnvironmentCheckpointPolicy,
    ) -> Result<WorkEnvironmentCheckpoint, WorkEnvironmentError> {
        policy.validate()?;
        let workspace = self.workspace_path(&record.spec.environment_id);
        let environment_root = self.environment_root(&record.spec.environment_id);
        let staging_name = format!("checkpoint-{}.bundle", uuid::Uuid::new_v4().simple());
        let staging = environment_root.join(&staging_name);
        let git = GitEngine::with_binary(self.git.clone());
        let workspace_for_git = workspace.clone();
        let staging_for_git = staging.clone();
        let include_untracked = policy.include_untracked;
        let label = policy.label.clone();
        let repo_key = workspace.display().to_string();
        let checkpoint_commit = self
            .execution
            .run_on_repo(
                ExecutionClass::LocalMutation,
                CHECKPOINT_GIT_ADMISSION_BYTES,
                Some(repo_key),
                move || {
                    let exclusions = if include_untracked {
                        Vec::new()
                    } else {
                        git.status_porcelain(&workspace_for_git)?
                            .into_iter()
                            .filter(|entry| {
                                entry.kind == medousa_forge::git::PorcelainKind::Untracked
                            })
                            .map(|entry| entry.path)
                            .collect()
                    };
                    let message = label
                        .map(|label| format!("forge: checkpoint {label}"))
                        .unwrap_or_else(|| "forge: work environment checkpoint".to_string());
                    let commit = git.commit_checkpoint_with_exclusions(
                        &workspace_for_git,
                        &message,
                        &CheckpointAuthor::default(),
                        &exclusions,
                    )?;
                    git.export_checkpoint_bundle(&workspace_for_git, &commit, &staging_for_git)?;
                    Ok(commit)
                },
            )
            .await
            .map_err(map_forge_error)?;
        let environment_store = Arc::new(StoreRoot::open_nofollow(&environment_root).map_err(
            |error| WorkEnvironmentError::Adapter(format!("open checkpoint staging: {error}")),
        )?);
        let staging_path = StorePath::parse(&staging_name).map_err(|error| {
            WorkEnvironmentError::Adapter(format!("checkpoint bundle path: {error}"))
        })?;
        let source_bundle = self
            .blobs
            .put_file(
                Arc::clone(&environment_store),
                staging_path.clone(),
                Some("application/vnd.git.bundle"),
                MAX_CHECKPOINT_BUNDLE_BYTES,
            )
            .await;
        let _ = environment_store.remove_file(&staging_path);
        let source_bundle =
            source_bundle.map_err(|error| WorkEnvironmentError::Adapter(error.to_string()))?;

        let mut artifacts = Vec::with_capacity(policy.artifacts.len());
        let mut artifact_total = 0_u64;
        let workspace_store = Arc::new(StoreRoot::open_nofollow(&workspace).map_err(|error| {
            WorkEnvironmentError::Adapter(format!("open checkpoint workspace: {error}"))
        })?);
        for request in &policy.artifacts {
            let path = StorePath::parse(&request.path).map_err(|error| {
                WorkEnvironmentError::InvalidSpec(format!("artifact path: {error}"))
            })?;
            let blob = self
                .blobs
                .put_file(
                    Arc::clone(&workspace_store),
                    path,
                    request.media_type.as_deref(),
                    policy.max_artifact_bytes,
                )
                .await
                .map_err(|error| WorkEnvironmentError::Adapter(error.to_string()))?;
            artifact_total = artifact_total.checked_add(blob.size_bytes).ok_or_else(|| {
                WorkEnvironmentError::InvalidSpec(
                    "checkpoint artifact byte total overflowed".to_string(),
                )
            })?;
            if artifact_total > policy.max_artifact_total_bytes {
                return Err(WorkEnvironmentError::InvalidSpec(format!(
                    "checkpoint artifacts exceed {} bytes",
                    policy.max_artifact_total_bytes
                )));
            }
            artifacts.push(WorkEnvironmentArtifact {
                path: request.path.clone(),
                blob,
            });
        }

        let manifest = WorkEnvironmentCheckpointManifest {
            schema_version: WORK_ENVIRONMENT_CHECKPOINT_SCHEMA_VERSION,
            environment_id: record.spec.environment_id.clone(),
            workspace_id: record.spec.workspace_id.clone(),
            base_commit: record.spec.base_commit.clone(),
            checkpoint_commit: checkpoint_commit.as_str().to_string(),
            source_bundle,
            artifacts,
            fence: record.spec.fence.clone(),
            label: policy.label.clone(),
            created_at: Utc::now(),
        };
        manifest.validate()?;
        let manifest_bytes = serde_json::to_vec(&manifest)
            .map_err(|error| WorkEnvironmentError::Adapter(error.to_string()))?;
        if manifest_bytes.len() as u64 > MAX_CHECKPOINT_MANIFEST_BYTES {
            return Err(WorkEnvironmentError::Adapter(format!(
                "checkpoint manifest exceeds {MAX_CHECKPOINT_MANIFEST_BYTES} bytes"
            )));
        }
        let manifest_blob = self
            .blobs
            .put(
                &manifest_bytes,
                Some("application/vnd.medousa.work-environment-checkpoint+json"),
            )
            .await
            .map_err(|error| WorkEnvironmentError::Adapter(error.to_string()))?;
        let persisted_manifest = self
            .blobs
            .get(&manifest_blob)
            .await
            .map_err(|error| WorkEnvironmentError::Adapter(error.to_string()))?;
        if persisted_manifest != manifest_bytes {
            return Err(WorkEnvironmentError::Adapter(
                "checkpoint manifest verification failed".to_string(),
            ));
        }
        let checkpoint = WorkEnvironmentCheckpoint::from_manifest(manifest_blob);
        let mut roots = vec![checkpoint.manifest.clone(), manifest.source_bundle.clone()];
        roots.extend(
            manifest
                .artifacts
                .iter()
                .map(|artifact| artifact.blob.clone()),
        );
        self.blobs
            .pin_root(
                &format!("checkpoint:{}", checkpoint.manifest.digest.hex),
                roots,
                policy.retain_until,
            )
            .await
            .map_err(|error| WorkEnvironmentError::Adapter(error.to_string()))?;
        Ok(checkpoint)
    }

    async fn pin_publication_root(
        &self,
        target_ref: &str,
        checkpoint: &WorkEnvironmentCheckpoint,
    ) -> Result<(), WorkEnvironmentError> {
        let manifest = self.load_checkpoint_manifest(checkpoint).await?;
        let mut roots = vec![checkpoint.manifest.clone(), manifest.source_bundle];
        roots.extend(manifest.artifacts.into_iter().map(|artifact| artifact.blob));
        self.blobs
            .pin_root(&format!("publication:{target_ref}"), roots, None)
            .await
            .map_err(|error| WorkEnvironmentError::Adapter(error.to_string()))
    }

    async fn create_container(
        &self,
        spec: &WorkEnvironmentSpec,
        container_name: &str,
        workspace: &Path,
        image: &str,
    ) -> Result<(), WorkEnvironmentError> {
        let mut args = vec![
            "create".into(),
            "--name".into(),
            container_name.into(),
            "--label".into(),
            format!("{MANAGED_LABEL}=true"),
            "--label".into(),
            format!("{ENVIRONMENT_LABEL}={}", spec.environment_id),
            "--label".into(),
            format!("{WORKSPACE_LABEL}={}", spec.workspace_id.as_str()),
            "--label".into(),
            format!("{FENCE_LABEL}={}", fence_fingerprint(&spec.fence)),
            "--platform".into(),
            spec.image.platform.clone(),
            "--network".into(),
            "none".into(),
            "--mount".into(),
            format!(
                "type=bind,source={},target={WORKSPACE_TARGET}",
                workspace.display()
            ),
            "--workdir".into(),
            WORKSPACE_TARGET.into(),
        ];
        if let Some(cpu_millis) = spec.requirements.cpu_millis {
            args.extend([
                "--cpus".into(),
                format!("{:.3}", cpu_millis as f64 / 1000.0),
            ]);
        }
        if let Some(memory_bytes) = spec.requirements.memory_bytes {
            args.extend(["--memory".into(), memory_bytes.to_string()]);
        }
        args.extend([
            "--entrypoint".into(),
            "/bin/sh".into(),
            image.into(),
            "-c".into(),
            "while :; do sleep 3600; done".into(),
        ]);
        self.docker_success(args, Duration::from_secs(2 * 60), CONTROL_OUTPUT_BYTES)
            .await?;
        Ok(())
    }

    async fn inspect_phase(
        &self,
        container_name: &str,
    ) -> Result<WorkEnvironmentPhase, WorkEnvironmentError> {
        let output = self
            .run_docker(
                vec![
                    "inspect".into(),
                    "--format".into(),
                    "{{.State.Status}}".into(),
                    container_name.into(),
                ],
                CONTROL_TIMEOUT,
                CONTROL_OUTPUT_BYTES,
            )
            .await?;
        if !output.status.success() {
            return Ok(WorkEnvironmentPhase::Absent);
        }
        Ok(match String::from_utf8_lossy(&output.stdout).trim() {
            "created" => WorkEnvironmentPhase::Ready,
            "running" | "paused" | "restarting" => WorkEnvironmentPhase::Running,
            "exited" => WorkEnvironmentPhase::Stopped,
            _ => WorkEnvironmentPhase::Failed,
        })
    }

    pub async fn reconcile(&self) -> Result<WorkEnvironmentReconcileReport, WorkEnvironmentError> {
        let _guard = self.lifecycle.lock().await;
        let output = self
            .docker_success(
                vec![
                    "ps".into(),
                    "--all".into(),
                    "--filter".into(),
                    format!("label={MANAGED_LABEL}=true"),
                    "--format".into(),
                    format!(
                        "{{{{.Names}}}}\t{{{{.State}}}}\t{{{{.Label \"{ENVIRONMENT_LABEL}\"}}}}\t{{{{.Label \"{FENCE_LABEL}\"}}}}"
                    ),
                ],
                CONTROL_TIMEOUT,
                CONTROL_OUTPUT_BYTES,
            )
            .await?;
        let mut containers = HashMap::new();
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let mut fields = line.splitn(4, '\t');
            let Some(name) = fields.next() else { continue };
            let Some(state) = fields.next() else { continue };
            let Some(environment_id) = fields.next() else {
                continue;
            };
            let Some(fence) = fields.next() else {
                continue;
            };
            containers.insert(
                name.to_string(),
                (
                    state.to_string(),
                    environment_id.to_string(),
                    fence.to_string(),
                ),
            );
        }

        let environments_root = self.environments_root();
        let environment_ids = self
            .execution
            .run(
                ExecutionClass::WorkEnvironment,
                MAX_CAPTURE_BYTES,
                move || {
                    let mut environment_ids = Vec::new();
                    for entry in std::fs::read_dir(environments_root)? {
                        let entry = entry?;
                        if !entry.path().join("state.json").is_file() {
                            continue;
                        }
                        if environment_ids.len() >= 10_000 {
                            return Err(medousa_forge::ForgeError::Overloaded(
                                "work-environment manifest count exceeds 10000".into(),
                            ));
                        }
                        environment_ids.push(entry.file_name().to_string_lossy().into_owned());
                    }
                    Ok(environment_ids)
                },
            )
            .await
            .map_err(map_forge_error)?;

        let mut report = WorkEnvironmentReconcileReport::default();
        let mut known_names = HashMap::new();
        for raw_environment_id in environment_ids {
            let environment_id = match WorkEnvironmentId::parse(&raw_environment_id) {
                Ok(environment_id) => environment_id,
                Err(_) => {
                    report.corrupt_preserved.push(raw_environment_id);
                    continue;
                }
            };
            let mut record = match self.load_record(&environment_id).await {
                Ok(Some(record)) => record,
                Ok(None) => continue,
                Err(_) => {
                    report.corrupt_preserved.push(raw_environment_id);
                    continue;
                }
            };
            known_names.insert(
                record.container_name.clone(),
                record.spec.environment_id.to_string(),
            );
            match containers.get(&record.container_name) {
                Some((state, environment_id, fence))
                    if environment_id == record.spec.environment_id.as_str()
                        && fence == &fence_fingerprint(&record.spec.fence) =>
                {
                    record.state.phase = docker_state_phase(state);
                    record.state.updated_at = Utc::now();
                    self.save_record(&record).await?;
                    report
                        .recovered
                        .push(record.spec.environment_id.to_string());
                }
                _ => {
                    record.state.phase = WorkEnvironmentPhase::Failed;
                    record.state.message =
                        Some("managed container missing; preserved local workspace".into());
                    record.state.updated_at = Utc::now();
                    self.save_record(&record).await?;
                    report.missing.push(record.spec.environment_id.to_string());
                }
            }
        }
        for (name, (_, environment_id, _)) in containers {
            if !known_names.contains_key(&name) {
                report
                    .unknown_preserved
                    .push(format!("{environment_id}:{name}"));
            }
        }
        Ok(report)
    }
}

#[async_trait]
impl WorkEnvironmentPort for DockerCliWorkEnvironmentPort {
    async fn materialize(
        &self,
        spec: WorkEnvironmentSpec,
    ) -> Result<WorkEnvironmentHandle, WorkEnvironmentError> {
        let _guard = self.lifecycle.lock().await;
        spec.validate(Utc::now())?;
        self.validate_supported_spec(&spec)?;
        self.admit_disk(&spec).await?;
        let image = self.ensure_image(&spec).await?;
        let container_name = container_name(&spec.environment_id);
        let existing = self.load_record(&spec.environment_id).await?;
        let mut record = if let Some(mut record) = existing {
            let phase = self.inspect_phase(&record.container_name).await?;
            if record.spec == spec && phase != WorkEnvironmentPhase::Absent {
                return Ok(Self::handle_for(&record));
            }
            if record.spec != spec {
                if !same_spec_except_fence(&record.spec, &spec) {
                    return Err(WorkEnvironmentError::AdmissionDenied(format!(
                        "environment {} already exists with a different immutable spec",
                        spec.environment_id
                    )));
                }
                if !newer_fence(&spec.fence, &record.spec.fence) {
                    return Err(WorkEnvironmentError::StaleFence);
                }
                if phase != WorkEnvironmentPhase::Absent {
                    self.docker_success(
                        vec!["rm".into(), "--force".into(), record.container_name.clone()],
                        CONTROL_TIMEOUT,
                        CONTROL_OUTPUT_BYTES,
                    )
                    .await?;
                }
                record.spec = spec;
            }
            record.state.phase = WorkEnvironmentPhase::Materializing;
            record.state.message = None;
            record.state.updated_at = Utc::now();
            record
        } else {
            LocalEnvironmentRecord {
                state: WorkEnvironmentState {
                    environment_id: spec.environment_id.clone(),
                    phase: WorkEnvironmentPhase::Materializing,
                    checkpoint_ref: spec
                        .checkpoint_ref
                        .as_ref()
                        .map(|checkpoint| checkpoint.provenance.clone()),
                    message: None,
                    updated_at: Utc::now(),
                },
                spec,
                container_name,
            }
        };
        self.save_record(&record).await?;
        let workspace = self.materialize_repository(&record.spec).await?;
        self.create_container(&record.spec, &record.container_name, &workspace, &image)
            .await?;
        record.state.phase = WorkEnvironmentPhase::Ready;
        record.state.updated_at = Utc::now();
        self.save_record(&record).await?;
        Ok(Self::handle_for(&record))
    }

    async fn inspect(
        &self,
        handle: &WorkEnvironmentHandle,
    ) -> Result<WorkEnvironmentState, WorkEnvironmentError> {
        let _guard = self.lifecycle.lock().await;
        let Some(mut record) = self.load_record(handle.environment_id()).await? else {
            return Ok(WorkEnvironmentState {
                environment_id: handle.environment_id().clone(),
                phase: WorkEnvironmentPhase::Absent,
                checkpoint_ref: None,
                message: None,
                updated_at: Utc::now(),
            });
        };
        Self::ensure_handle(&record, handle)?;
        record.state.phase = self.inspect_phase(&record.container_name).await?;
        record.state.updated_at = Utc::now();
        self.save_record(&record).await?;
        Ok(record.state)
    }

    async fn start(
        &self,
        handle: &WorkEnvironmentHandle,
        fence: &WorkEnvironmentFence,
    ) -> Result<WorkEnvironmentState, WorkEnvironmentError> {
        let _guard = self.lifecycle.lock().await;
        let mut record = self
            .load_record(handle.environment_id())
            .await?
            .ok_or_else(|| WorkEnvironmentError::NotFound(handle.environment_id().to_string()))?;
        Self::ensure_handle(&record, handle)?;
        Self::ensure_fence(&record, fence)?;
        let phase = self.inspect_phase(&record.container_name).await?;
        if phase == WorkEnvironmentPhase::Absent {
            return Err(WorkEnvironmentError::InvalidState {
                operation: "start",
                phase,
            });
        }
        if phase != WorkEnvironmentPhase::Running {
            self.docker_success(
                vec!["start".into(), record.container_name.clone()],
                CONTROL_TIMEOUT,
                CONTROL_OUTPUT_BYTES,
            )
            .await?;
        }
        record.state.phase = WorkEnvironmentPhase::Running;
        record.state.updated_at = Utc::now();
        self.save_record(&record).await?;
        Ok(record.state)
    }

    async fn exec(
        &self,
        handle: &WorkEnvironmentHandle,
        request: WorkEnvironmentExecRequest,
        fence: &WorkEnvironmentFence,
    ) -> Result<WorkEnvironmentExecResult, WorkEnvironmentError> {
        let _guard = self.lifecycle.lock().await;
        let record = self
            .load_record(handle.environment_id())
            .await?
            .ok_or_else(|| WorkEnvironmentError::NotFound(handle.environment_id().to_string()))?;
        Self::ensure_handle(&record, handle)?;
        Self::ensure_fence(&record, fence)?;
        validate_exec_request(&request)?;
        if self.inspect_phase(&record.container_name).await? != WorkEnvironmentPhase::Running {
            return Err(WorkEnvironmentError::InvalidState {
                operation: "exec",
                phase: record.state.phase,
            });
        }
        if let Some(stored) = self
            .load_execution(handle.environment_id(), fence, &request.idempotency_key)
            .await?
        {
            if stored.request != request {
                return Err(WorkEnvironmentError::AdmissionDenied(
                    "exec idempotency key was reused with different input".into(),
                ));
            }
            return stored.result.ok_or_else(|| WorkEnvironmentError::AdmissionDenied("previous execution outcome is unknown after interruption; refusing to run it twice".into()));
        }
        self.save_execution(
            handle.environment_id(),
            fence,
            &request.idempotency_key,
            &StoredExecution {
                request: request.clone(),
                result: None,
            },
        )
        .await?;
        let execution_id = format!(
            "exec:{}:{}",
            record.spec.environment_id,
            short_hash(&request.idempotency_key)
        );
        let started_at = Utc::now();
        let mut args = vec!["exec".into()];
        let stdin = request
            .stdin
            .as_ref()
            .map(|stdin| stdin.as_bytes().to_vec());
        if stdin.is_some() {
            args.push("-i".into());
        }
        if let Some(cwd) = request.working_directory.as_ref() {
            args.extend(["--workdir".into(), cwd.clone()]);
        }
        for key in request.environment.keys() {
            args.extend(["--env".into(), key.clone()]);
        }
        args.push(record.container_name.clone());
        args.push(request.program.clone());
        args.extend(request.args.clone());
        let environment = request.environment.clone().into_iter().collect();
        let output = self
            .run_docker_with_environment(
                args,
                environment,
                stdin,
                Duration::from_secs(request.timeout_seconds),
                request.max_output_bytes as usize,
            )
            .await?;
        let (stdout, stderr, clipped) = bound_combined_output(
            output.stdout,
            output.stderr,
            request.max_output_bytes as usize,
        );
        let result = WorkEnvironmentExecResult {
            execution_id,
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&stdout).into_owned(),
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
            output_truncated: output.truncated || clipped,
            started_at,
            finished_at: Utc::now(),
        };
        let idempotency_key = request.idempotency_key.clone();
        self.save_execution(
            handle.environment_id(),
            fence,
            &idempotency_key,
            &StoredExecution {
                request,
                result: Some(result.clone()),
            },
        )
        .await?;
        Ok(result)
    }

    async fn attach_pty(
        &self,
        _handle: &WorkEnvironmentHandle,
        _request: WorkEnvironmentPtyRequest,
        _fence: &WorkEnvironmentFence,
    ) -> Result<WorkEnvironmentPtyHandle, WorkEnvironmentError> {
        Err(WorkEnvironmentError::Unsupported(
            "PTY attachment is intentionally deferred to the interactive phase".into(),
        ))
    }

    async fn checkpoint(
        &self,
        handle: &WorkEnvironmentHandle,
        policy: WorkEnvironmentCheckpointPolicy,
        fence: &WorkEnvironmentFence,
    ) -> Result<WorkEnvironmentCheckpoint, WorkEnvironmentError> {
        let _guard = self.lifecycle.lock().await;
        let mut record = self
            .load_record(handle.environment_id())
            .await?
            .ok_or_else(|| WorkEnvironmentError::NotFound(handle.environment_id().to_string()))?;
        Self::ensure_handle(&record, handle)?;
        Self::ensure_fence(&record, fence)?;
        let phase = self.inspect_phase(&record.container_name).await?;
        if !matches!(
            phase,
            WorkEnvironmentPhase::Ready
                | WorkEnvironmentPhase::Running
                | WorkEnvironmentPhase::Stopped
        ) {
            return Err(WorkEnvironmentError::InvalidState {
                operation: "checkpoint",
                phase,
            });
        }
        record.state.phase = WorkEnvironmentPhase::Checkpointing;
        record.state.message = None;
        record.state.updated_at = Utc::now();
        self.save_record(&record).await?;
        match self.create_durable_checkpoint(&record, &policy).await {
            Ok(checkpoint) => {
                record.state.phase = phase;
                record.state.checkpoint_ref = Some(checkpoint.provenance.clone());
                record.state.updated_at = Utc::now();
                self.save_record(&record).await?;
                Ok(checkpoint)
            }
            Err(error) => {
                record.state.phase = phase;
                record.state.message = Some(format!("checkpoint failed: {error}"));
                record.state.updated_at = Utc::now();
                let _ = self.save_record(&record).await;
                Err(error)
            }
        }
    }

    async fn publish(
        &self,
        handle: &WorkEnvironmentHandle,
        checkpoint: &WorkEnvironmentCheckpoint,
        fence: &WorkEnvironmentFence,
    ) -> Result<WorkEnvironmentPublicationResult, WorkEnvironmentError> {
        let _guard = self.lifecycle.lock().await;
        let record = self
            .load_record(handle.environment_id())
            .await?
            .ok_or_else(|| WorkEnvironmentError::NotFound(handle.environment_id().to_string()))?;
        Self::ensure_handle(&record, handle)?;
        Self::ensure_fence(&record, fence)?;
        checkpoint.validate()?;
        if record.state.checkpoint_ref.as_ref() != Some(&checkpoint.provenance) {
            return Err(WorkEnvironmentError::CheckpointMissing(
                checkpoint.provenance.compact(),
            ));
        }
        let publication = record.spec.publication.as_ref().ok_or_else(|| {
            WorkEnvironmentError::Unsupported("environment has no publication target".to_string())
        })?;
        self.verify_checkpoint_content(checkpoint).await?;
        let value = checkpoint.provenance.compact();
        let outcome = self
            .publications
            .compare_and_swap(
                &publication.target_ref,
                publication.expected_value.as_deref(),
                &value,
            )
            .await?;
        match outcome {
            PublicationCasOutcome::Published { previous } => {
                self.pin_publication_root(&publication.target_ref, checkpoint)
                    .await?;
                Ok(WorkEnvironmentPublicationResult::Published {
                    target_ref: publication.target_ref.clone(),
                    value,
                    previous,
                })
            }
            PublicationCasOutcome::AlreadyPublished => {
                self.pin_publication_root(&publication.target_ref, checkpoint)
                    .await?;
                Ok(WorkEnvironmentPublicationResult::AlreadyPublished {
                    target_ref: publication.target_ref.clone(),
                    value,
                })
            }
            PublicationCasOutcome::Conflict { found } => {
                Ok(WorkEnvironmentPublicationResult::Conflict {
                    target_ref: publication.target_ref.clone(),
                    expected: publication.expected_value.clone(),
                    found,
                    preserved_checkpoint: checkpoint.clone(),
                })
            }
        }
    }

    async fn stop(
        &self,
        handle: &WorkEnvironmentHandle,
        reason: WorkEnvironmentStopReason,
        fence: &WorkEnvironmentFence,
    ) -> Result<WorkEnvironmentState, WorkEnvironmentError> {
        let _guard = self.lifecycle.lock().await;
        let mut record = self
            .load_record(handle.environment_id())
            .await?
            .ok_or_else(|| WorkEnvironmentError::NotFound(handle.environment_id().to_string()))?;
        Self::ensure_handle(&record, handle)?;
        Self::ensure_fence(&record, fence)?;
        let phase = self.inspect_phase(&record.container_name).await?;
        if phase == WorkEnvironmentPhase::Running {
            self.docker_success(
                vec![
                    "stop".into(),
                    "--time".into(),
                    "10".into(),
                    record.container_name.clone(),
                ],
                Duration::from_secs(20),
                CONTROL_OUTPUT_BYTES,
            )
            .await?;
        } else if phase == WorkEnvironmentPhase::Absent {
            return Err(WorkEnvironmentError::InvalidState {
                operation: "stop",
                phase,
            });
        }
        record.state.phase = WorkEnvironmentPhase::Stopped;
        record.state.message = reason.message;
        record.state.updated_at = Utc::now();
        self.save_record(&record).await?;
        Ok(record.state)
    }

    async fn release(
        &self,
        handle: &WorkEnvironmentHandle,
        retention: WorkEnvironmentRetention,
        fence: &WorkEnvironmentFence,
    ) -> Result<WorkEnvironmentState, WorkEnvironmentError> {
        let _guard = self.lifecycle.lock().await;
        let mut record = self
            .load_record(handle.environment_id())
            .await?
            .ok_or_else(|| WorkEnvironmentError::NotFound(handle.environment_id().to_string()))?;
        Self::ensure_handle(&record, handle)?;
        Self::ensure_fence(&record, fence)?;
        match retention {
            WorkEnvironmentRetention::Delete => {
                let phase = self.inspect_phase(&record.container_name).await?;
                if phase != WorkEnvironmentPhase::Absent {
                    self.docker_success(
                        vec!["rm".into(), "--force".into(), record.container_name.clone()],
                        CONTROL_TIMEOUT,
                        CONTROL_OUTPUT_BYTES,
                    )
                    .await?;
                }
                let root = self.environment_root(&record.spec.environment_id);
                self.execution
                    .run(ExecutionClass::StoreIo, 64 * 1024, move || {
                        if root.exists() {
                            std::fs::remove_dir_all(root)?;
                        }
                        Ok(())
                    })
                    .await
                    .map_err(map_forge_error)?;
                record.state.phase = WorkEnvironmentPhase::Released;
                record.state.updated_at = Utc::now();
            }
            WorkEnvironmentRetention::RetainWarmUntil(until)
            | WorkEnvironmentRetention::PreserveForDebugUntil(until) => {
                if until <= Utc::now()
                    || until > Utc::now() + medousa_runtime::MAX_WORK_ENVIRONMENT_RETENTION
                {
                    return Err(WorkEnvironmentError::InvalidSpec(
                        "retention deadline must be in the future and no more than seven days away"
                            .into(),
                    ));
                }
                if self.inspect_phase(&record.container_name).await?
                    == WorkEnvironmentPhase::Running
                {
                    self.docker_success(
                        vec![
                            "stop".into(),
                            "--time".into(),
                            "10".into(),
                            record.container_name.clone(),
                        ],
                        Duration::from_secs(20),
                        CONTROL_OUTPUT_BYTES,
                    )
                    .await?;
                }
                record.state.phase = WorkEnvironmentPhase::Stopped;
                record.state.updated_at = Utc::now();
                self.save_record(&record).await?;
            }
        }
        Ok(record.state)
    }
}

fn map_forge_error(error: medousa_forge::ForgeError) -> WorkEnvironmentError {
    match error {
        medousa_forge::ForgeError::Overloaded(message) => {
            WorkEnvironmentError::AdmissionDenied(message)
        }
        error => WorkEnvironmentError::Adapter(error.to_string()),
    }
}

fn command_failure(output: &CommandOutput) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        output.status.to_string()
    }
}

fn short_hash(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    format!("{digest:x}")[..16].to_string()
}

fn fence_fingerprint(fence: &WorkEnvironmentFence) -> String {
    short_hash(&format!(
        "{}:{:?}:{:?}",
        fence.stasis_attempt.0,
        fence.forge_environment_generation,
        fence.forge_execution_generation
    ))
}

fn same_spec_except_fence(left: &WorkEnvironmentSpec, right: &WorkEnvironmentSpec) -> bool {
    let mut left = left.clone();
    left.fence = right.fence.clone();
    &left == right
}

fn newer_fence(candidate: &WorkEnvironmentFence, current: &WorkEnvironmentFence) -> bool {
    (
        candidate.stasis_attempt.0,
        candidate.forge_environment_generation.unwrap_or(0),
        candidate.forge_execution_generation.unwrap_or(0),
    ) > (
        current.stasis_attempt.0,
        current.forge_environment_generation.unwrap_or(0),
        current.forge_execution_generation.unwrap_or(0),
    )
}

fn read_bounded(path: &Path, max_bytes: u64) -> std::io::Result<Vec<u8>> {
    use std::io::Read as _;

    let file = std::fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take(max_bytes + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max_bytes {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("{} exceeds {max_bytes} bytes", path.display()),
        ));
    }
    Ok(bytes)
}

fn container_name(environment_id: &WorkEnvironmentId) -> String {
    let safe: String = environment_id
        .as_str()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '-'
            }
        })
        .take(40)
        .collect();
    format!("medousa-{safe}-{}", short_hash(environment_id.as_str()))
}

fn docker_state_phase(state: &str) -> WorkEnvironmentPhase {
    match state {
        "created" => WorkEnvironmentPhase::Ready,
        "running" | "paused" | "restarting" => WorkEnvironmentPhase::Running,
        "exited" => WorkEnvironmentPhase::Stopped,
        _ => WorkEnvironmentPhase::Failed,
    }
}

fn validate_exec_request(request: &WorkEnvironmentExecRequest) -> Result<(), WorkEnvironmentError> {
    if request.idempotency_key.trim().is_empty() || request.idempotency_key.len() > 256 {
        return Err(WorkEnvironmentError::InvalidSpec(
            "exec idempotency key is invalid".into(),
        ));
    }
    if request.program.trim().is_empty()
        || request.program.len() > 2_048
        || request.program.chars().any(char::is_control)
    {
        return Err(WorkEnvironmentError::InvalidSpec(
            "exec program is invalid".into(),
        ));
    }
    if request.timeout_seconds == 0 || request.timeout_seconds > MAX_EXEC_TIMEOUT_SECONDS {
        return Err(WorkEnvironmentError::InvalidSpec(
            "exec timeout must be between one second and one hour".into(),
        ));
    }
    if request.max_output_bytes == 0 || request.max_output_bytes as usize > MAX_EXEC_OUTPUT_BYTES {
        return Err(WorkEnvironmentError::InvalidSpec(format!(
            "exec output bound must be between one byte and {MAX_EXEC_OUTPUT_BYTES} bytes"
        )));
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
    if request.args.len() > 256
        || request.args.iter().any(|arg| arg.contains('\0'))
        || request.args.iter().map(String::len).sum::<usize>() > 256 * 1024
    {
        return Err(WorkEnvironmentError::InvalidSpec(
            "exec arguments exceed their count or byte bound".into(),
        ));
    }
    if let Some(cwd) = request.working_directory.as_deref()
        && (cwd != "/"
            && (!cwd.starts_with('/')
                || cwd
                    .split('/')
                    .skip(1)
                    .any(|part| part.is_empty() || matches!(part, "." | ".."))))
    {
        return Err(WorkEnvironmentError::InvalidSpec(
            "exec working directory must be a normalized absolute container path".into(),
        ));
    }
    for (key, value) in &request.environment {
        if key.is_empty()
            || !key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            || key.as_bytes()[0].is_ascii_digit()
            || value.contains('\0')
        {
            return Err(WorkEnvironmentError::InvalidSpec(
                "exec environment contains an invalid entry".into(),
            ));
        }
    }
    if request.environment.len() > 256
        || request
            .environment
            .iter()
            .map(|(key, value)| key.len() + value.len())
            .sum::<usize>()
            > 256 * 1024
    {
        return Err(WorkEnvironmentError::InvalidSpec(
            "exec environment exceeds its count or byte bound".into(),
        ));
    }
    Ok(())
}

fn bound_combined_output(
    mut stdout: Vec<u8>,
    mut stderr: Vec<u8>,
    max: usize,
) -> (Vec<u8>, Vec<u8>, bool) {
    if stdout.len().saturating_add(stderr.len()) <= max {
        return (stdout, stderr, false);
    }
    if stdout.len() >= max {
        stdout.truncate(max);
        stderr.clear();
    } else {
        stderr.truncate(max - stdout.len());
    }
    (stdout, stderr, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use medousa_runtime::{
        WorkEnvironmentBinding, WorkEnvironmentImage, WorkEnvironmentPublication,
        WorkEnvironmentRepository, WorkEnvironmentRequirements, WorkspaceId,
    };
    use stasis::domain::runtime::provenance::ContentDigest;
    use stasis::domain::runtime::resource_lease::FencingToken;
    use std::collections::BTreeMap;

    #[test]
    fn container_names_are_safe_stable_and_collision_resistant() {
        let first = WorkEnvironmentId::parse("job:attempt.1").unwrap();
        let second = WorkEnvironmentId::parse("job-attempt.1").unwrap();
        let first_name = container_name(&first);
        assert!(first_name.starts_with("medousa-job-attempt.1-"));
        assert_ne!(first_name, container_name(&second));
        assert_eq!(first_name, container_name(&first));
    }

    #[test]
    fn output_bound_is_shared_across_streams() {
        let (stdout, stderr, truncated) = bound_combined_output(vec![b'a'; 7], vec![b'b'; 7], 10);
        assert_eq!(stdout.len() + stderr.len(), 10);
        assert!(truncated);
    }

    #[test]
    fn docker_states_map_to_domain_states() {
        assert_eq!(docker_state_phase("created"), WorkEnvironmentPhase::Ready);
        assert_eq!(docker_state_phase("running"), WorkEnvironmentPhase::Running);
        assert_eq!(docker_state_phase("exited"), WorkEnvironmentPhase::Stopped);
        assert_eq!(docker_state_phase("dead"), WorkEnvironmentPhase::Failed);
    }

    #[test]
    fn fence_order_includes_stasis_and_forge_generations() {
        let current = WorkEnvironmentFence {
            stasis_attempt: FencingToken(4),
            forge_environment_generation: Some(2),
            forge_execution_generation: Some(7),
        };
        assert!(newer_fence(
            &WorkEnvironmentFence {
                stasis_attempt: FencingToken(5),
                forge_environment_generation: Some(1),
                forge_execution_generation: Some(1),
            },
            &current
        ));
        assert!(!newer_fence(&current, &current));
        assert!(!newer_fence(
            &WorkEnvironmentFence {
                stasis_attempt: FencingToken(3),
                forge_environment_generation: Some(99),
                forge_execution_generation: Some(99),
            },
            &current
        ));
    }

    #[tokio::test]
    #[ignore = "requires a running Docker engine and an explicitly selected local image"]
    async fn docker_lifecycle_exec_and_restart_reconcile() {
        let image_reference = std::env::var("MEDOUSA_TEST_OCI_IMAGE")
            .expect("set MEDOUSA_TEST_OCI_IMAGE to a locally cached image repository");
        let image_digest = std::env::var("MEDOUSA_TEST_OCI_DIGEST")
            .expect("set MEDOUSA_TEST_OCI_DIGEST to its sha256 hex digest");
        let image_platform = std::env::var("MEDOUSA_TEST_OCI_PLATFORM")
            .expect("set MEDOUSA_TEST_OCI_PLATFORM to the image OS/architecture");
        let pinned_image = format!("{image_reference}@sha256:{image_digest}");
        let temp = tempfile::tempdir().unwrap();
        let test_root = std::fs::canonicalize(temp.path()).unwrap();
        let repository = test_root.join("origin");
        std::fs::create_dir_all(&repository).unwrap();
        run_test_git(&repository, &["init", "--quiet"]);
        std::fs::write(repository.join("README.md"), "pinned input\n").unwrap();
        std::fs::write(repository.join(".gitignore"), "dist/\n").unwrap();
        run_test_git(&repository, &["add", "README.md", ".gitignore"]);
        run_test_git(
            &repository,
            &[
                "-c",
                "user.name=Medousa Test",
                "-c",
                "user.email=test@medousa.local",
                "commit",
                "--quiet",
                "-m",
                "fixture",
            ],
        );
        let base_commit = test_git_output(&repository, &["rev-parse", "HEAD"]);
        let environment_id =
            WorkEnvironmentId::parse(format!("oci-test-{}", uuid::Uuid::new_v4().simple()))
                .unwrap();
        let fence = WorkEnvironmentFence {
            stasis_attempt: FencingToken(1),
            forge_environment_generation: Some(2),
            forge_execution_generation: Some(3),
        };
        let spec = WorkEnvironmentSpec {
            environment_id: environment_id.clone(),
            workspace_id: WorkspaceId::parse("oci-conformance").unwrap(),
            repository: WorkEnvironmentRepository {
                repository_id: "oci-conformance".into(),
                authorized_origin: repository.display().to_string(),
            },
            base_commit,
            image: WorkEnvironmentImage {
                reference: image_reference,
                digest: ContentDigest {
                    algorithm: ContentDigest::SHA256.into(),
                    hex: image_digest,
                },
                platform: image_platform,
            },
            checkpoint_ref: None,
            requirements: WorkEnvironmentRequirements {
                cpu_millis: Some(500),
                memory_bytes: Some(256 * 1024 * 1024),
                ..WorkEnvironmentRequirements::default()
            },
            mounts: Vec::new(),
            network_policy: WorkEnvironmentNetworkPolicy::Deny,
            secret_refs: Vec::new(),
            fence: fence.clone(),
            publication: Some(WorkEnvironmentPublication {
                target_ref: format!("work-environment/{environment_id}"),
                expected_value: None,
            }),
            retention: WorkEnvironmentRetention::Delete,
        };
        let execution = Arc::new(ForgeExecutionService::new());
        let adapter =
            DockerCliWorkEnvironmentPort::detect(test_root.join("environments"), execution)
                .await
                .unwrap()
                .expect("Docker adapter should be available");

        let handle = adapter.materialize(spec.clone()).await.unwrap();
        assert_eq!(
            adapter.inspect(&handle).await.unwrap().phase,
            WorkEnvironmentPhase::Ready
        );
        adapter.start(&handle, &fence).await.unwrap();
        let result = adapter
            .exec(
                &handle,
                WorkEnvironmentExecRequest {
                    idempotency_key: "read-pinned-input".into(),
                    program: "/bin/sh".into(),
                    args: vec![
                        "-c".into(),
                        "printf 'container:' && cat README.md && printf \"env:$MEDOUSA_TEST_VALUE\""
                            .into(),
                    ],
                    working_directory: Some(WORKSPACE_TARGET.into()),
                    environment: BTreeMap::from([("MEDOUSA_TEST_VALUE".into(), "scoped".into())]),
                    stdin: None,
                    timeout_seconds: 30,
                    max_output_bytes: 64 * 1024,
                },
                &fence,
            )
            .await
            .unwrap();
        assert_eq!(result.exit_code, Some(0));
        assert_eq!(result.stdout, "container:pinned input\nenv:scoped");

        let stdin_result = adapter
            .exec(
                &handle,
                WorkEnvironmentExecRequest {
                    idempotency_key: "bounded-stdin-roundtrip".into(),
                    program: "/bin/sh".into(),
                    args: vec![
                        "-c".into(),
                        "cat > generated.txt && cat generated.txt".into(),
                    ],
                    working_directory: Some(WORKSPACE_TARGET.into()),
                    environment: BTreeMap::new(),
                    stdin: Some("written through fenced stdin\n".into()),
                    timeout_seconds: 30,
                    max_output_bytes: 64 * 1024,
                },
                &fence,
            )
            .await
            .unwrap();
        assert_eq!(stdin_result.exit_code, Some(0));
        assert_eq!(stdin_result.stdout, "written through fenced stdin\n");

        let invocation = crate::work_environment_tools::EnvironmentToolInvocation::new(
            WorkEnvironmentBinding {
                port: adapter.clone(),
                handle: handle.clone(),
                fence: fence.clone(),
            },
            "oci-tool-routing",
        );
        let write = crate::work_environment_tools::code_write(
            &invocation,
            crate::work_environment_tools::EnvironmentCodeWriteRequest {
                path: "src/phase3.txt".into(),
                expected_sha256: "missing".into(),
                content: Some("written by the catalog adapter\n".into()),
                find: None,
                replace: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(write["ok"], true);
        let read = crate::work_environment_tools::code_read(
            &invocation,
            crate::work_environment_tools::EnvironmentCodeReadRequest {
                path: "src/phase3.txt".into(),
                line_start: None,
                line_end: None,
                byte_start: None,
                byte_end: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(read["content"], "written by the catalog adapter\n");
        let search =
            crate::work_environment_tools::code_search(&invocation, "catalog adapter", Some(10))
                .await
                .unwrap();
        assert_eq!(search["results"][0]["path"], "src/phase3.txt");
        let verify = crate::work_environment_tools::shell_exec(
            &invocation,
            "/bin/sh".into(),
            vec![
                "-c".into(),
                "test \"$(cat src/phase3.txt)\" = 'written by the catalog adapter' && printf verified"
                    .into(),
            ],
            None,
            None,
            30_000,
            64 * 1024,
        )
        .await
        .unwrap();
        assert_eq!(verify.exit_code, Some(0));
        assert_eq!(verify.stdout, "verified");

        let artifact_write = adapter
            .exec(
                &handle,
                WorkEnvironmentExecRequest {
                    idempotency_key: "write-ignored-artifact".into(),
                    program: "/bin/sh".into(),
                    args: vec![
                        "-c".into(),
                        "mkdir -p dist && printf 'durable artifact v1' > dist/result.txt".into(),
                    ],
                    working_directory: Some(WORKSPACE_TARGET.into()),
                    environment: BTreeMap::new(),
                    stdin: None,
                    timeout_seconds: 30,
                    max_output_bytes: 64 * 1024,
                },
                &fence,
            )
            .await
            .unwrap();
        assert_eq!(artifact_write.exit_code, Some(0));
        let checkpoint_policy = WorkEnvironmentCheckpointPolicy {
            include_untracked: true,
            label: Some("phase-4-live-proof".into()),
            artifacts: vec![medousa_runtime::WorkEnvironmentArtifactRequest {
                path: "dist/result.txt".into(),
                media_type: Some("text/plain".into()),
            }],
            ..WorkEnvironmentCheckpointPolicy::default()
        };
        let published_checkpoint = adapter
            .checkpoint(&handle, checkpoint_policy.clone(), &fence)
            .await
            .unwrap();
        assert!(matches!(
            adapter
                .publish(&handle, &published_checkpoint, &fence)
                .await
                .unwrap(),
            WorkEnvironmentPublicationResult::Published { .. }
        ));
        assert!(matches!(
            adapter
                .publish(&handle, &published_checkpoint, &fence)
                .await
                .unwrap(),
            WorkEnvironmentPublicationResult::AlreadyPublished { .. }
        ));

        let later_work = adapter
            .exec(
                &handle,
                WorkEnvironmentExecRequest {
                    idempotency_key: "write-conflicting-checkpoint".into(),
                    program: "/bin/sh".into(),
                    args: vec![
                        "-c".into(),
                        "printf 'checkpoint after publication\n' > README.md && printf 'durable artifact v2' > dist/result.txt"
                            .into(),
                    ],
                    working_directory: Some(WORKSPACE_TARGET.into()),
                    environment: BTreeMap::new(),
                    stdin: None,
                    timeout_seconds: 30,
                    max_output_bytes: 64 * 1024,
                },
                &fence,
            )
            .await
            .unwrap();
        assert_eq!(later_work.exit_code, Some(0));
        let preserved_checkpoint = adapter
            .checkpoint(&handle, checkpoint_policy, &fence)
            .await
            .unwrap();
        assert!(matches!(
            adapter
                .publish(&handle, &preserved_checkpoint, &fence)
                .await
                .unwrap(),
            WorkEnvironmentPublicationResult::Conflict { found: Some(_), .. }
        ));

        let unknown_name = format!("medousa-unknown-{}", uuid::Uuid::new_v4().simple());
        adapter
            .docker_success(
                vec![
                    "create".into(),
                    "--name".into(),
                    unknown_name.clone(),
                    "--label".into(),
                    format!("{MANAGED_LABEL}=true"),
                    "--label".into(),
                    format!("{ENVIRONMENT_LABEL}=unknown-test"),
                    "--label".into(),
                    format!("{FENCE_LABEL}=unknown"),
                    "--entrypoint".into(),
                    "/bin/sh".into(),
                    pinned_image.clone(),
                    "-c".into(),
                    "while :; do sleep 3600; done".into(),
                ],
                CONTROL_TIMEOUT,
                CONTROL_OUTPUT_BYTES,
            )
            .await
            .unwrap();
        let restarted = DockerCliWorkEnvironmentPort::detect(
            adapter.root().to_path_buf(),
            Arc::new(ForgeExecutionService::new()),
        )
        .await
        .unwrap()
        .expect("Docker adapter should reopen after restart");
        let report = restarted.reconcile().await.unwrap();
        restarted
            .docker_success(
                vec!["rm".into(), "--force".into(), unknown_name.clone()],
                CONTROL_TIMEOUT,
                CONTROL_OUTPUT_BYTES,
            )
            .await
            .unwrap();
        assert_eq!(report.recovered, vec![environment_id.to_string()]);
        assert!(
            report
                .unknown_preserved
                .contains(&format!("unknown-test:{unknown_name}")),
            "the deliberately unknown container must be reported and preserved"
        );
        let stale = WorkEnvironmentFence {
            stasis_attempt: FencingToken(2),
            ..fence.clone()
        };
        assert!(matches!(
            restarted
                .stop(
                    &handle,
                    WorkEnvironmentStopReason {
                        code: "test".into(),
                        message: None,
                    },
                    &stale,
                )
                .await,
            Err(WorkEnvironmentError::StaleFence)
        ));
        restarted
            .release(&handle, WorkEnvironmentRetention::Delete, &fence)
            .await
            .unwrap();
        assert_eq!(
            restarted.inspect(&handle).await.unwrap().phase,
            WorkEnvironmentPhase::Absent
        );

        let restored_environment_id =
            WorkEnvironmentId::parse(format!("oci-restored-{}", uuid::Uuid::new_v4().simple()))
                .unwrap();
        let mut restored_spec = spec;
        restored_spec.environment_id = restored_environment_id;
        restored_spec.checkpoint_ref = Some(preserved_checkpoint);
        restored_spec.publication = None;
        let restored_handle = restarted.materialize(restored_spec).await.unwrap();
        restarted.start(&restored_handle, &fence).await.unwrap();
        let restored = restarted
            .exec(
                &restored_handle,
                WorkEnvironmentExecRequest {
                    idempotency_key: "verify-restored-checkpoint".into(),
                    program: "/bin/sh".into(),
                    args: vec![
                        "-c".into(),
                        "printf 'source:' && cat README.md && printf 'artifact:' && cat dist/result.txt"
                            .into(),
                    ],
                    working_directory: Some(WORKSPACE_TARGET.into()),
                    environment: BTreeMap::new(),
                    stdin: None,
                    timeout_seconds: 30,
                    max_output_bytes: 64 * 1024,
                },
                &fence,
            )
            .await
            .unwrap();
        assert_eq!(restored.exit_code, Some(0));
        assert_eq!(
            restored.stdout,
            "source:checkpoint after publication\nartifact:durable artifact v2"
        );
        restarted
            .release(&restored_handle, WorkEnvironmentRetention::Delete, &fence)
            .await
            .unwrap();
    }

    fn run_test_git(cwd: &Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(cwd)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed");
    }

    fn test_git_output(cwd: &Path, args: &[&str]) -> String {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?} failed");
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    }
}
