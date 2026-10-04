//! Exact, owner-bound Forge metadata and governed project-overlay observations.
use super::native_vault::VaultResolveTarget;
use super::*;
use crate::vault::{
    contracts::VaultRootId,
    identity::{VaultIdentityTransaction, VaultResourceKind},
    owner::VaultIndexOwner,
    path::VaultPath,
};
use medousa_forge::{
    forge::Forge,
    model::{WorkId, WorkItem, WorkTarget},
};
use medousa_store::{StorePath, StoreRoot};
use sha2::{Digest, Sha256};

mod identity;
#[cfg(test)]
mod tests;

fn project_workspace_schema(_: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
    crate::schema_api::string_enum_schema(&["isolated", "attached_checkout"])
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkProjectCreateInput {
    /// Stable identity for exact retries. A different request must use a new key.
    pub request_key: String,
    pub title: String,
    /// Outcome authorized by the user; creation does not launch an executor.
    pub brief: String,
    /// Existing Git repository on this workshop's disk, including git-init-only repositories.
    pub repo_path: String,
    /// Optional selected revision; omission uses the native suggested/current branch.
    #[serde(default)]
    pub base_ref: Option<String>,
    /// Use isolated unless the user explicitly selects the current checkout.
    #[serde(default)]
    #[schemars(schema_with = "project_workspace_schema")]
    pub workspace_mode: medousa_forge::model::WorkspaceMode,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkProjectResolveInput {
    /// Exact native Forge work ID, retained across chats; never a path or title.
    pub work_id: String,
    pub target: ProjectResolveTarget,
}
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectResolveTarget {
    Project {},
    ForgeWork {},
    Reference {
        reference: ResourceRef,
    },
    Overlay {
        /// Pin the exact governed environment; no fallback to another checkout.
        environment_generation: u32,
        /// Forge lineage includes branch because candidate environments share a generation.
        environment_branch: String,
        target: VaultResolveTarget,
    },
}
fn work_reference(domain: &UserDomainRef, namespace: &str, work_id: &WorkId) -> ResourceRef {
    ResourceRef {
        authority_id: domain.authority_id.clone(),
        kind: ResourceKind::ForgeWork,
        id: format!("forge:{namespace}:work:{work_id}"),
    }
}
fn project_reference(
    domain: &UserDomainRef,
    namespace: &str,
    record: &identity::ProjectIdentity,
) -> ResourceRef {
    ResourceRef {
        authority_id: domain.authority_id.clone(),
        kind: ResourceKind::Project,
        id: format!("forge:{namespace}:project:{}", record.id),
    }
}
fn common_dir(forge: &Forge, item: &WorkItem) -> Result<PathBuf> {
    if let Some(env) = item.workspace_environment().or(item.environment.as_ref()) {
        return Ok(env.repo.common_dir.clone());
    }
    let WorkTarget::Git(target) = &item.target;
    Ok(forge.git().repo_identity(&target.repo_path)?.common_dir)
}
fn publish_project(
    store: &WorkGraphStore,
    domain: &UserDomainRef,
    namespace: &str,
    record: &identity::ProjectIdentity,
) -> Result<serde_json::Value> {
    native_graph::publish(
        store,
        domain,
        project_reference(domain, namespace, record),
        serde_json::json!({"source":"forge_repository","common_dir":record.common_dir}).to_string(),
        format!("forge-project-v1:{namespace}:{}", record.revision),
        if record.available {
            ResourceResolution::Available
        } else {
            ResourceResolution::Unavailable
        },
        "adapter:forge-project",
    )
}
pub(super) fn resolve(
    store: &WorkGraphStore,
    domain: &UserDomainRef,
    forge: &Forge,
    input: WorkProjectResolveInput,
) -> Result<serde_json::Value> {
    let work_id = WorkId::parse_storage(&input.work_id)
        .map_err(|e| anyhow::anyhow!("invalid Forge work ID: {e}"))?;
    if !forge.store().item_exists(&work_id) {
        bail!("Forge work is unavailable");
    }
    let _custody = forge
        .store()
        .try_lock_item(&work_id)?
        .ok_or_else(|| anyhow::anyhow!("Forge item custody unavailable; retry resolution"))?;
    let item = forge.load(&work_id)?;
    if item.owner != domain.user_id || item.id != work_id {
        bail!("Forge work is not visible to this owner");
    }
    if let ProjectResolveTarget::Reference { reference } = &input.target
        && (reference.authority_id != domain.authority_id
            || !matches!(
                reference.kind,
                ResourceKind::Project | ResourceKind::ForgeWork
            ))
    {
        bail!("project resolution requires a local project or Forge work reference");
    }
    if let ProjectResolveTarget::Overlay {
        environment_generation,
        environment_branch,
        target,
    } = input.target
    {
        return resolve_overlay(
            store,
            domain,
            forge,
            &item,
            (environment_generation, &environment_branch),
            target,
        );
    }
    let mut registry = identity::ProjectRegistry::open(forge)?;
    let namespace = registry.namespace().to_string();
    if matches!(input.target, ProjectResolveTarget::ForgeWork {})
        || matches!(&input.target,ProjectResolveTarget::Reference {reference} if reference.kind == ResourceKind::ForgeWork)
    {
        let reference = work_reference(domain, &namespace, &work_id);
        if let ProjectResolveTarget::Reference {
            reference: presented,
        } = &input.target
            && *presented != reference
        {
            bail!("Forge reference does not name the selected work or store");
        }
        let bytes = serde_json::to_vec(&item)?;
        if bytes.len() > MAX_SNAPSHOT_BYTES {
            bail!("Forge metadata exceeds observation limit");
        }
        let seq = forge.store().cached_last_seq(&work_id)?;
        let mut observation = native_graph::publish(store,domain,reference,
            serde_json::json!({"source":"forge_work","work_id":work_id,"state":item.state,"environment_generation":item.workspace_environment().map(|e|e.generation),"environment_branch":item.workspace_environment().map(|e|&e.branch)}).to_string(),
            format!("forge-work-v1:{namespace}:{seq}:{:x}",Sha256::digest(bytes)),ResourceResolution::Available,"adapter:forge-work")?;
        observation["coverage"] = "forge_work_lifecycle_metadata".into();
        observation["file_effects_replayed"] = false.into();
        return Ok(observation);
    }
    let common = common_dir(forge, &item)?;
    let requested = if let ProjectResolveTarget::Reference { reference } = &input.target {
        let parts: Vec<_> = reference.id.split(':').collect();
        if parts.len() != 4 || parts[0] != "forge" || parts[1] != namespace || parts[2] != "project"
        {
            bail!("project reference belongs to a different native store");
        }
        let record = registry.get(parts[3])?;
        if common.to_str() != Some(record.common_dir.as_str()) {
            bail!("project reference does not belong to the selected repository");
        }
        Some(record.id)
    } else {
        None
    };
    // Open the exact common directory with no-follow custody. Neither remotes,
    // equal commits, nor a title authorize a shared project identity.
    let records = match StoreRoot::open_nofollow(&common) {
        Ok(files) => {
            if forge.git().repo_identity(&common)?.common_dir != common {
                bail!("native common directory is not the selected repository");
            }
            registry.observe(&common, &files)?
        }
        Err(error) if error.is_not_found() => registry.observe_missing(&common)?,
        Err(error) => return Err(error.into()),
    };
    let selected = if let Some(id) = requested {
        registry.get(&id)?
    } else {
        records[0].clone()
    };
    // Include every retained replacement at this locator on retry, so a graph
    // failure after native publication cannot leave an old readiness fact live.
    let mut affected = Vec::new();
    for record in &records {
        affected.push(publish_project(store, domain, &namespace, record)?);
    }
    let observation = affected
        .iter()
        .find(|v| {
            v["resource"]["reference"]["id"] == project_reference(domain, &namespace, &selected).id
        })
        .ok_or_else(|| anyhow::anyhow!("selected project observation missing"))?;
    bounded_response(
        serde_json::json!({"resource":observation["resource"],"graph_revision":affected.last().map(|v|&v["graph_revision"]),"receipt":observation["receipt"],"affected_resources":affected.iter().map(|v|&v["resource"]).collect::<Vec<_>>(),"coverage":"forge_repository_identity_metadata","file_effects_replayed":false}),
    )
}
fn resolve_overlay(
    store: &WorkGraphStore,
    domain: &UserDomainRef,
    forge: &Forge,
    item: &WorkItem,
    environment: (u32, &str),
    target: VaultResolveTarget,
) -> Result<serde_json::Value> {
    let (generation, branch) = environment;
    let env = item
        .workspace_environment()
        .ok_or_else(|| anyhow::anyhow!("Forge work has no governed overlay environment"))?;
    if env.generation != generation || env.branch != branch {
        bail!("governed environment branch or generation changed; inspect Forge work again");
    }
    if let VaultResolveTarget::Reference { reference } = &target
        && (reference.authority_id != domain.authority_id
            || !matches!(
                reference.kind,
                ResourceKind::VaultNote | ResourceKind::VaultFolder
            ))
    {
        bail!("overlay resolution requires a local note or folder reference");
    }
    let workspace = StoreRoot::open_nofollow(&env.worktree)?;
    if forge.git().worktree_root(&env.worktree)?.canonicalize()? != env.worktree.canonicalize()?
        || forge.git().repo_identity(&env.worktree)?.common_dir != env.repo.common_dir
        || forge.git().current_branch(&env.worktree)?.as_deref() != Some(branch)
    {
        bail!("governed workspace repository changed");
    }
    let overlay = Arc::new(workspace.open_subroot(&StorePath::parse(".medousa/vault")?)?);
    let owner = VaultIndexOwner::new(
        VaultRootId::new(format!("overlay:{}:{generation}", item.id)),
        overlay,
    );
    crate::vault::mutation::recover_all_pending_writes(&owner)?;
    let mut identities = VaultIdentityTransaction::begin(&owner, false)?;
    let record = match target {
        VaultResolveTarget::Note { path } => {
            identities.observe_path(&VaultPath::parse(&path)?, VaultResourceKind::Note)?
        }
        VaultResolveTarget::Folder { path } => {
            identities.observe_path(&VaultPath::parse(&path)?, VaultResourceKind::Folder)?
        }
        VaultResolveTarget::Reference { reference } => {
            let parts: Vec<_> = reference.id.split(':').collect();
            if parts.len() != 4
                || parts[0] != "vault"
                || parts[1] != identities.vault_id()
                || parts[2] != "project"
            {
                bail!("overlay reference belongs to a different physical root or source");
            }
            let kind = identities.resource_kind(parts[3])?;
            if !matches!(
                (reference.kind, kind),
                (ResourceKind::VaultNote, VaultResourceKind::Note)
                    | (ResourceKind::VaultFolder, VaultResourceKind::Folder)
            ) {
                bail!("overlay reference kind mismatch");
            }
            identities.observe_id(parts[3])?
        }
    };
    let mut observation = native_graph::publish(
        store,
        domain,
        ResourceRef {
            authority_id: domain.authority_id.clone(),
            kind: match record.kind {
                VaultResourceKind::Note => ResourceKind::VaultNote,
                VaultResourceKind::Folder => ResourceKind::VaultFolder,
            },
            id: format!(
                "vault:{}:project:{}",
                identities.vault_id(),
                record.resource_id
            ),
        },
        serde_json::json!({"vault_id":identities.vault_id(),"source":"project","path":record.path})
            .to_string(),
        format!(
            "vault-observation-v1:{}:{}",
            identities.vault_id(),
            record.revision
        ),
        record.resolution,
        "adapter:forge-overlay",
    )?;
    observation["coverage"] = "forge_project_overlay_exact_resource".into();
    observation["work_id"] = item.id.as_str().into();
    observation["environment_generation"] = generation.into();
    observation["environment_branch"] = branch.into();
    observation["file_effects_replayed"] = false.into();
    Ok(observation)
}
fn publish_creation(
    store: &WorkGraphStore,
    domain: &UserDomainRef,
    forge: &Forge,
    item: WorkItem,
) -> Result<serde_json::Value> {
    let project = resolve(
        store,
        domain,
        forge,
        WorkProjectResolveInput {
            work_id: item.id.to_string(),
            target: ProjectResolveTarget::Project {},
        },
    )?;
    let work = resolve(
        store,
        domain,
        forge,
        WorkProjectResolveInput {
            work_id: item.id.to_string(),
            target: ProjectResolveTarget::ForgeWork {},
        },
    )?;
    let relationship = native_graph::link_created_project(
        store,
        domain,
        serde_json::from_value(work["resource"]["reference"].clone())?,
        serde_json::from_value(project["resource"]["reference"].clone())?,
    )?;
    bounded_response(serde_json::json!({
        "forge_work_id": item.id, "title": item.title, "state": item.state,
        "workspace_mode": item.workspace_mode, "target": item.target,
        "project": project, "forge_work": work, "relationship": relationship,
        "executor_started": false, "session_binding_changed": false,
        "next": "Use forge_work_id with peer_propose; the user must approve and start the coder separately."
    }))
}

impl WorkUnitHost {
    pub async fn create_project(
        &self,
        turn: &TurnExecutionContext,
        input: WorkProjectCreateInput,
    ) -> Result<serde_json::Value> {
        let domain = admitted_domain(turn, true)?;
        if !turn
            .principal()
            .capabilities()
            .contains(Capability::WorkspaceWrite)
        {
            bail!("undertaking creation requires workspace write authority");
        }
        if input.title.trim().is_empty()
            || input.title.len() > 512
            || input.brief.trim().is_empty()
            || input.brief.len() > 64 * 1024
            || input.repo_path.trim().is_empty()
            || !PathBuf::from(&input.repo_path).is_absolute()
            || input.base_ref.as_ref().is_some_and(|reference| {
                reference.trim().is_empty()
                    || reference != reference.trim()
                    || reference.starts_with('-')
                    || reference.chars().any(char::is_control)
            })
        {
            bail!(
                "creation requires a title, brief, absolute workshop repository path, and valid base ref"
            );
        }
        let host = crate::daemon::coordination::local_coordination_host().ok_or_else(|| {
            anyhow::anyhow!("undertaking creation is unavailable on this workshop")
        })?;
        let item = host
            .create_undertaking_for_owner(domain.user_id.clone(), input)
            .await?;
        // The native graph authors both resource identities. Retries also repair an
        // interrupted publication after Forge creation, without creating another item.
        let store = self.store.clone();
        let forge = self.forge.clone();
        self.execution
            .run(ExecutionClass::Observation, 8 * 1024 * 1024, move || {
                Ok(publish_creation(&store, &domain, &forge, item))
            })
            .await?
    }

    pub async fn resolve_project(
        &self,
        turn: &TurnExecutionContext,
        input: WorkProjectResolveInput,
    ) -> Result<serde_json::Value> {
        let domain = admitted_domain(turn, true)?;
        let store = self.store.clone();
        let forge = self.forge.clone();
        self.execution
            .run(ExecutionClass::Observation, 8 * 1024 * 1024, move || {
                Ok(resolve(&store, &domain, &forge, input))
            })
            .await?
    }
}
