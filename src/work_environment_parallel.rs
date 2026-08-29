//! Durable fan-out and reconciliation contracts for parallel work environments.
//!
//! Children remain ordinary `workflow.medousa.work_environment` jobs. Their
//! immutable checkpoints become one portable reconciliation checkpoint; no
//! workspace path or container handle crosses this boundary.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use medousa_runtime::{
    WorkEnvironmentArtifact, WorkEnvironmentCheckpoint, WorkEnvironmentCheckpointManifest,
    WorkEnvironmentPublication,
};
use serde::{Deserialize, Serialize};
use stasis::domain::runtime::blob_descriptor::BlobDescriptor;
use stasis::ports::outbound::runtime::blob_transfer::BlobTransferPort;
use stasis::prelude::{Result as StasisResult, StasisError};

use crate::work_environment_job::WorkEnvironmentJobPayload;

pub const PARALLEL_WORK_PLAN_SCHEMA_VERSION: u32 = 1;
pub const RECONCILIATION_INPUT_SCHEMA_VERSION: u32 = 1;
pub const RECONCILIATION_INPUT_MEDIA_TYPE: &str =
    "application/vnd.medousa.work-environment-reconciliation-input+json";
pub const RECONCILIATION_CHECKPOINT_MEDIA_TYPE: &str =
    "application/vnd.medousa.work-environment-checkpoint+json";
const MIN_PARALLEL_CHILDREN: usize = 2;
const MAX_PARALLEL_CHILDREN: usize = 16;
const RECONCILIATION_ROOT: &str = ".medousa/reconciliation";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ParallelWorkChild {
    pub child_id: String,
    pub work: WorkEnvironmentJobPayload,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ParallelWorkPlan {
    pub schema_version: u32,
    pub plan_id: String,
    pub base_commit: String,
    pub children: Vec<ParallelWorkChild>,
    /// Ordinary work-environment job used to combine, verify, and publish the
    /// child results. Its checkpoint is filled by the coordinator.
    pub reconciliation: WorkEnvironmentJobPayload,
}

impl ParallelWorkPlan {
    pub fn validate(&self, now: DateTime<Utc>) -> StasisResult<()> {
        if self.schema_version != PARALLEL_WORK_PLAN_SCHEMA_VERSION {
            return Err(invalid(format!(
                "unsupported parallel plan schema_version={}",
                self.schema_version
            )));
        }
        validate_id("plan_id", &self.plan_id)?;
        if !(MIN_PARALLEL_CHILDREN..=MAX_PARALLEL_CHILDREN).contains(&self.children.len()) {
            return Err(invalid(format!(
                "parallel plan must contain {MIN_PARALLEL_CHILDREN}..={MAX_PARALLEL_CHILDREN} children"
            )));
        }
        self.reconciliation
            .validate(now)
            .map_err(|error| invalid(error.to_string()))?;
        if self.reconciliation.spec.base_commit != self.base_commit {
            return Err(invalid(
                "reconciliation base_commit does not match the parallel plan",
            ));
        }
        if self.reconciliation.spec.checkpoint_ref.is_some() {
            return Err(invalid(
                "reconciliation checkpoint is coordinator-owned and must be unset",
            ));
        }
        if self.reconciliation.federation.is_some() {
            return Err(invalid(
                "reconciliation federation context is destination-owned",
            ));
        }
        require_expected_base(
            self.reconciliation.spec.publication.as_ref(),
            &self.base_commit,
        )?;

        let repository = &self.reconciliation.spec.repository;
        let mut child_ids = BTreeSet::new();
        let mut environment_ids = BTreeSet::new();
        let mut workspace_ids = BTreeSet::new();
        environment_ids.insert(self.reconciliation.spec.environment_id.as_str().to_string());
        workspace_ids.insert(self.reconciliation.spec.workspace_id.as_str().to_string());
        for child in &self.children {
            validate_id("child_id", &child.child_id)?;
            if !child_ids.insert(child.child_id.as_str()) {
                return Err(invalid(format!(
                    "duplicate parallel child_id: {}",
                    child.child_id
                )));
            }
            child
                .work
                .validate(now)
                .map_err(|error| invalid(error.to_string()))?;
            if child.work.spec.base_commit != self.base_commit
                || child.work.spec.repository != *repository
            {
                return Err(invalid(format!(
                    "parallel child {} does not share the exact repository and base",
                    child.child_id
                )));
            }
            if child.work.spec.publication.is_some() {
                return Err(invalid(format!(
                    "parallel child {} must preserve a checkpoint instead of publishing",
                    child.child_id
                )));
            }
            if child.work.federation.is_some() {
                return Err(invalid(format!(
                    "parallel child {} contains destination-owned federation context",
                    child.child_id
                )));
            }
            if !environment_ids.insert(child.work.spec.environment_id.as_str().to_string()) {
                return Err(invalid("parallel environments must be distinct"));
            }
            if !workspace_ids.insert(child.work.spec.workspace_id.as_str().to_string()) {
                return Err(invalid("parallel workspaces must be distinct"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ParallelWorkChildResult {
    pub child_id: String,
    pub job_id: String,
    pub checkpoint: WorkEnvironmentCheckpoint,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReconciliationArtifactInput {
    pub source_path: String,
    pub materialized_path: String,
    pub blob: BlobDescriptor,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReconciliationChildInput {
    pub child_id: String,
    pub job_id: String,
    pub checkpoint_commit: String,
    pub source_bundle: BlobDescriptor,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub materialized_bundle_path: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<ReconciliationArtifactInput>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReconciliationInputManifest {
    pub schema_version: u32,
    pub plan_id: String,
    pub base_commit: String,
    pub primary_child_id: String,
    pub children: Vec<ReconciliationChildInput>,
    pub created_at: DateTime<Utc>,
}

pub struct PreparedReconciliationInput {
    pub checkpoint: WorkEnvironmentCheckpoint,
    pub manifest: ReconciliationInputManifest,
}

pub fn parallel_child_job_id(parent_job_id: &str, child_id: &str) -> StasisResult<String> {
    validate_id("child_id", child_id)?;
    if parent_job_id.trim().is_empty() {
        return Err(invalid("parallel parent job id is required"));
    }
    Ok(format!("{}:parallel:{child_id}", parent_job_id.trim()))
}

pub fn parallel_reconciliation_job_id(parent_job_id: &str) -> StasisResult<String> {
    if parent_job_id.trim().is_empty() {
        return Err(invalid("parallel parent job id is required"));
    }
    Ok(format!("{}:reconcile", parent_job_id.trim()))
}

/// Assemble every child result into one portable checkpoint. The first child
/// is restored as the reconciliation worktree; every other Git bundle and all
/// declared artifacts are materialized under `.medousa/reconciliation/`.
pub async fn prepare_reconciliation_input(
    blobs: &dyn BlobTransferPort,
    plan: &ParallelWorkPlan,
    results: &[ParallelWorkChildResult],
    now: DateTime<Utc>,
) -> StasisResult<PreparedReconciliationInput> {
    plan.validate(now)?;
    let results_by_child = exact_results(plan, results)?;
    let mut inputs = Vec::with_capacity(plan.children.len());
    let mut checkpoint_artifacts = Vec::new();
    let mut primary_manifest = None;

    for (index, child) in plan.children.iter().enumerate() {
        let result = results_by_child
            .get(child.child_id.as_str())
            .expect("exact result set was validated");
        result
            .checkpoint
            .validate()
            .map_err(|error| invalid(error.to_string()))?;
        let bytes = blobs.get(&result.checkpoint.manifest).await?;
        let manifest: WorkEnvironmentCheckpointManifest = serde_json::from_slice(&bytes)
            .map_err(|error| invalid(format!("decode child checkpoint manifest: {error}")))?;
        manifest
            .validate()
            .map_err(|error| invalid(error.to_string()))?;
        if manifest.base_commit != plan.base_commit
            || manifest.environment_id != child.work.spec.environment_id
            || manifest.workspace_id != child.work.spec.workspace_id
        {
            return Err(invalid(format!(
                "parallel child {} checkpoint does not match its exact base, environment, and workspace",
                child.child_id
            )));
        }
        for descriptor in std::iter::once(&manifest.source_bundle)
            .chain(manifest.artifacts.iter().map(|artifact| &artifact.blob))
        {
            if !blobs.exists(descriptor).await? {
                return Err(invalid(format!(
                    "parallel child {} checkpoint content is missing: {}:{}",
                    child.child_id, descriptor.digest.algorithm, descriptor.digest.hex
                )));
            }
        }

        let materialized_bundle_path = (index != 0)
            .then(|| format!("{RECONCILIATION_ROOT}/children/{}.bundle", child.child_id));
        if let Some(path) = materialized_bundle_path.as_ref() {
            checkpoint_artifacts.push(WorkEnvironmentArtifact {
                path: path.clone(),
                blob: manifest.source_bundle.clone(),
            });
        } else {
            primary_manifest = Some(manifest.clone());
        }

        let mut artifacts = Vec::with_capacity(manifest.artifacts.len());
        for artifact in &manifest.artifacts {
            let materialized_path = format!(
                "{RECONCILIATION_ROOT}/children/{}/artifacts/{}",
                child.child_id, artifact.path
            );
            checkpoint_artifacts.push(WorkEnvironmentArtifact {
                path: materialized_path.clone(),
                blob: artifact.blob.clone(),
            });
            artifacts.push(ReconciliationArtifactInput {
                source_path: artifact.path.clone(),
                materialized_path,
                blob: artifact.blob.clone(),
            });
        }
        inputs.push(ReconciliationChildInput {
            child_id: child.child_id.clone(),
            job_id: result.job_id.clone(),
            checkpoint_commit: manifest.checkpoint_commit,
            source_bundle: manifest.source_bundle,
            materialized_bundle_path,
            artifacts,
        });
    }

    let primary = primary_manifest.expect("parallel plan always has a first child");
    let manifest = ReconciliationInputManifest {
        schema_version: RECONCILIATION_INPUT_SCHEMA_VERSION,
        plan_id: plan.plan_id.clone(),
        base_commit: plan.base_commit.clone(),
        primary_child_id: plan.children[0].child_id.clone(),
        children: inputs,
        created_at: now,
    };
    let manifest_bytes = serde_json::to_vec(&manifest)
        .map_err(|error| invalid(format!("encode reconciliation input: {error}")))?;
    let manifest_blob = blobs
        .put(&manifest_bytes, Some(RECONCILIATION_INPUT_MEDIA_TYPE))
        .await?;
    checkpoint_artifacts.push(WorkEnvironmentArtifact {
        path: format!("{RECONCILIATION_ROOT}/manifest.json"),
        blob: manifest_blob,
    });

    let reconciliation_manifest = WorkEnvironmentCheckpointManifest {
        schema_version: medousa_runtime::WORK_ENVIRONMENT_CHECKPOINT_SCHEMA_VERSION,
        environment_id: plan.reconciliation.spec.environment_id.clone(),
        workspace_id: plan.reconciliation.spec.workspace_id.clone(),
        base_commit: plan.base_commit.clone(),
        checkpoint_commit: primary.checkpoint_commit,
        source_bundle: primary.source_bundle,
        artifacts: checkpoint_artifacts,
        fence: plan.reconciliation.spec.fence.clone(),
        label: Some(format!("parallel-reconciliation: {}", plan.plan_id)),
        created_at: now,
    };
    reconciliation_manifest
        .validate()
        .map_err(|error| invalid(error.to_string()))?;
    let bytes = serde_json::to_vec(&reconciliation_manifest)
        .map_err(|error| invalid(format!("encode reconciliation checkpoint: {error}")))?;
    let descriptor = blobs
        .put(&bytes, Some(RECONCILIATION_CHECKPOINT_MEDIA_TYPE))
        .await?;
    Ok(PreparedReconciliationInput {
        checkpoint: WorkEnvironmentCheckpoint::from_manifest(descriptor),
        manifest,
    })
}

pub fn reconciliation_work_payload(
    plan: &ParallelWorkPlan,
    prepared: &PreparedReconciliationInput,
) -> WorkEnvironmentJobPayload {
    let mut payload = plan.reconciliation.clone();
    payload.spec.checkpoint_ref = Some(prepared.checkpoint.clone());
    payload.execution.environment.insert(
        "MEDOUSA_RECONCILIATION_MANIFEST".to_string(),
        format!("/workspace/{RECONCILIATION_ROOT}/manifest.json"),
    );
    payload
}

fn exact_results<'a>(
    plan: &ParallelWorkPlan,
    results: &'a [ParallelWorkChildResult],
) -> StasisResult<BTreeMap<&'a str, &'a ParallelWorkChildResult>> {
    if results.len() != plan.children.len() {
        return Err(invalid("parallel result set is incomplete or has extras"));
    }
    let expected: BTreeSet<&str> = plan
        .children
        .iter()
        .map(|child| child.child_id.as_str())
        .collect();
    let mut found = BTreeMap::new();
    for result in results {
        validate_id("result child_id", &result.child_id)?;
        if !expected.contains(result.child_id.as_str())
            || found.insert(result.child_id.as_str(), result).is_some()
        {
            return Err(invalid("parallel result identity is unknown or duplicated"));
        }
    }
    Ok(found)
}

fn require_expected_base(
    publication: Option<&WorkEnvironmentPublication>,
    base_commit: &str,
) -> StasisResult<()> {
    let Some(publication) = publication else {
        return Err(invalid(
            "reconciliation must publish through expected-base CAS",
        ));
    };
    if publication.expected_value.as_deref() != Some(base_commit) {
        return Err(invalid(
            "reconciliation publication expected_value must equal the exact base",
        ));
    }
    Ok(())
}

fn validate_id(name: &str, value: &str) -> StasisResult<()> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(invalid(format!("{name} is invalid")));
    }
    Ok(())
}

fn invalid(message: impl Into<String>) -> StasisError {
    StasisError::PortFailure(message.into())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chrono::Duration;
    use medousa_runtime::{
        WorkEnvironmentCheckpointPolicy, WorkEnvironmentExecRequest, WorkEnvironmentFence,
        WorkEnvironmentId, WorkEnvironmentImage, WorkEnvironmentNetworkPolicy,
        WorkEnvironmentPublication, WorkEnvironmentRepository, WorkEnvironmentRequirements,
        WorkEnvironmentRetention, WorkEnvironmentSpec, WorkspaceId,
    };
    use stasis::domain::runtime::provenance::ContentDigest;
    use stasis::domain::runtime::resource_lease::FencingToken;
    use stasis::infrastructure::runtime::in_memory_blob_transfer::InMemoryBlobTransfer;

    use super::*;

    const BASE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn payload(id: &str, publication: bool) -> WorkEnvironmentJobPayload {
        WorkEnvironmentJobPayload {
            spec: WorkEnvironmentSpec {
                environment_id: WorkEnvironmentId::parse(format!("environment-{id}")).unwrap(),
                workspace_id: WorkspaceId::parse(format!("workspace-{id}")).unwrap(),
                repository: WorkEnvironmentRepository {
                    repository_id: "repository".to_string(),
                    authorized_origin: "https://example.test/repository.git".to_string(),
                },
                base_commit: BASE.to_string(),
                image: WorkEnvironmentImage {
                    reference: "registry.example.test/medousa/dev".to_string(),
                    digest: ContentDigest::sha256_bytes(b"phase-7-image"),
                    platform: "linux/amd64".to_string(),
                },
                checkpoint_ref: None,
                requirements: WorkEnvironmentRequirements::default(),
                mounts: Vec::new(),
                network_policy: WorkEnvironmentNetworkPolicy::Deny,
                secret_refs: Vec::new(),
                fence: WorkEnvironmentFence {
                    stasis_attempt: FencingToken(1),
                    forge_environment_generation: None,
                    forge_execution_generation: None,
                },
                publication: publication.then(|| WorkEnvironmentPublication {
                    target_ref: "refs/heads/main".to_string(),
                    expected_value: Some(BASE.to_string()),
                }),
                retention: WorkEnvironmentRetention::Delete,
            },
            execution: WorkEnvironmentExecRequest {
                idempotency_key: format!("execute-{id}"),
                program: "sh".to_string(),
                args: vec!["-lc".to_string(), "true".to_string()],
                working_directory: Some("/workspace".to_string()),
                environment: BTreeMap::new(),
                stdin: None,
                timeout_seconds: 60,
                max_output_bytes: 1024,
            },
            checkpoint: WorkEnvironmentCheckpointPolicy::default(),
            require_successful_exit: true,
            deadline_at: Some(Utc::now() + Duration::minutes(5)),
            display_name: Some(id.to_string()),
            federation: None,
        }
    }

    fn plan() -> ParallelWorkPlan {
        ParallelWorkPlan {
            schema_version: PARALLEL_WORK_PLAN_SCHEMA_VERSION,
            plan_id: "parallel-plan".to_string(),
            base_commit: BASE.to_string(),
            children: ["alpha", "beta", "gamma"]
                .into_iter()
                .map(|id| ParallelWorkChild {
                    child_id: id.to_string(),
                    work: payload(id, false),
                })
                .collect(),
            reconciliation: payload("reconcile", true),
        }
    }

    async fn child_result(
        blobs: &InMemoryBlobTransfer,
        child: &ParallelWorkChild,
        byte: u8,
    ) -> ParallelWorkChildResult {
        let bundle = blobs
            .put(&[byte; 32], Some("application/vnd.git.bundle"))
            .await
            .unwrap();
        let artifact = blobs.put(&[byte; 8], Some("text/plain")).await.unwrap();
        let manifest = WorkEnvironmentCheckpointManifest {
            schema_version: medousa_runtime::WORK_ENVIRONMENT_CHECKPOINT_SCHEMA_VERSION,
            environment_id: child.work.spec.environment_id.clone(),
            workspace_id: child.work.spec.workspace_id.clone(),
            base_commit: BASE.to_string(),
            checkpoint_commit: format!("{byte:040x}"),
            source_bundle: bundle,
            artifacts: vec![WorkEnvironmentArtifact {
                path: "evidence/report.txt".to_string(),
                blob: artifact,
            }],
            fence: child.work.spec.fence.clone(),
            label: Some(child.child_id.clone()),
            created_at: Utc::now(),
        };
        let bytes = serde_json::to_vec(&manifest).unwrap();
        let descriptor = blobs
            .put(&bytes, Some(RECONCILIATION_CHECKPOINT_MEDIA_TYPE))
            .await
            .unwrap();
        ParallelWorkChildResult {
            child_id: child.child_id.clone(),
            job_id: format!("job-{}", child.child_id),
            checkpoint: WorkEnvironmentCheckpoint::from_manifest(descriptor),
        }
    }

    #[tokio::test]
    async fn three_isolated_results_become_one_portable_reconciliation_checkpoint() {
        let blobs = InMemoryBlobTransfer::new();
        let plan = plan();
        let mut results = Vec::new();
        for (index, child) in plan.children.iter().enumerate() {
            results.push(child_result(&blobs, child, (index + 1) as u8).await);
        }
        results.reverse();

        let prepared = prepare_reconciliation_input(&blobs, &plan, &results, Utc::now())
            .await
            .unwrap();
        assert_eq!(prepared.manifest.primary_child_id, "alpha");
        assert_eq!(prepared.manifest.children.len(), 3);
        assert!(
            prepared.manifest.children[0]
                .materialized_bundle_path
                .is_none()
        );
        assert!(
            prepared.manifest.children[1]
                .materialized_bundle_path
                .as_deref()
                .unwrap()
                .ends_with("beta.bundle")
        );

        let bytes = blobs.get(&prepared.checkpoint.manifest).await.unwrap();
        let checkpoint: WorkEnvironmentCheckpointManifest = serde_json::from_slice(&bytes).unwrap();
        checkpoint.validate().unwrap();
        assert_eq!(checkpoint.artifacts.len(), 6);
        assert!(
            checkpoint.artifacts.iter().any(|artifact| {
                artifact.path == ".medousa/reconciliation/children/gamma.bundle"
            })
        );
        assert!(
            checkpoint
                .artifacts
                .iter()
                .any(|artifact| { artifact.path == ".medousa/reconciliation/manifest.json" })
        );

        let work = reconciliation_work_payload(&plan, &prepared);
        assert_eq!(work.spec.checkpoint_ref, Some(prepared.checkpoint));
        assert_eq!(
            work.execution
                .environment
                .get("MEDOUSA_RECONCILIATION_MANIFEST")
                .map(String::as_str),
            Some("/workspace/.medousa/reconciliation/manifest.json")
        );
    }

    #[tokio::test]
    async fn missing_or_duplicated_child_results_never_choose_a_silent_winner() {
        let blobs = InMemoryBlobTransfer::new();
        let plan = plan();
        let alpha = child_result(&blobs, &plan.children[0], 1).await;
        let duplicate = alpha.clone();
        let error = prepare_reconciliation_input(&blobs, &plan, &[alpha, duplicate], Utc::now())
            .await
            .err()
            .unwrap();
        assert!(error.to_string().contains("incomplete or has extras"));
    }

    #[test]
    fn parallel_children_cannot_race_the_reconciliation_publication() {
        let mut plan = plan();
        plan.children[0].work.spec.publication = Some(WorkEnvironmentPublication {
            target_ref: "refs/heads/main".to_string(),
            expected_value: Some(BASE.to_string()),
        });
        let error = plan.validate(Utc::now()).unwrap_err();
        assert!(error.to_string().contains("instead of publishing"));
    }

    #[test]
    fn child_and_reconciliation_job_identities_are_replay_stable() {
        assert_eq!(
            parallel_child_job_id("parent-job", "alpha").unwrap(),
            "parent-job:parallel:alpha"
        );
        assert_eq!(
            parallel_reconciliation_job_id("parent-job").unwrap(),
            "parent-job:reconcile"
        );
    }
}
