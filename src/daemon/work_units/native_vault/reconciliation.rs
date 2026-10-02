//! Native repair admission; agents propose exact evidence, the runtime verifies it.

use super::*;
use crate::vault::identity::{ReconciliationDisposition, VaultReconciliationKey};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkNativeReconcileInput {
    pub root_id: String,
    pub command: VaultReconcileCommand,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum VaultReconcileCommand {
    Inspect {},
    AdoptMove {
        command_id: String,
        reference: ResourceRef,
        expected_native_revision: String,
        path: String,
    },
    QuarantineJournal {
        command_id: String,
        operation_id: String,
        expected_intent_digest: String,
    },
}

fn key(
    domain: &UserDomainRef,
    input: &WorkNativeReconcileInput,
    command_id: &str,
) -> Result<VaultReconciliationKey> {
    use sha2::{Digest, Sha256};
    if command_id.trim().is_empty()
        || command_id.len() > 128
        || command_id.chars().any(char::is_control)
    {
        bail!("reconciliation command ID must be 1–128 bytes without control characters");
    }
    Ok(VaultReconciliationKey {
        id: format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&(domain, command_id))?)
        ),
        digest: format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&(domain, input))?)
        ),
    })
}

pub(super) fn reconcile(
    store: &WorkGraphStore,
    domain: &UserDomainRef,
    owner: &Arc<VaultIndexOwner>,
    input: WorkNativeReconcileInput,
) -> Result<serde_json::Value> {
    // Attempt ordinary native recovery first. Failure is diagnostic evidence,
    // not authority to replay bytes or merge physical objects.
    let recovery_error = crate::vault::mutation::recover_all_pending_writes(owner)
        .err()
        .map(|error| error.to_string().chars().take(512).collect::<String>());
    let mut identities = VaultIdentityTransaction::begin(owner, true)?;
    let receipt = match &input.command {
        VaultReconcileCommand::Inspect {} => {
            return bounded_response(serde_json::json!({
                "vault_id":identities.vault_id(), "identity_revision":identities.current_revision(),
                "pending_journals":identities.inspect_pending()?, "recovery_error":recovery_error,
                "coverage":"user_vault_repair_metadata",
            }));
        }
        VaultReconcileCommand::AdoptMove {
            command_id,
            reference,
            expected_native_revision,
            path,
        } => {
            if reference.authority_id != domain.authority_id
                || !matches!(
                    reference.kind,
                    ResourceKind::VaultNote | ResourceKind::VaultFolder
                )
            {
                bail!("reconciliation requires a local vault reference");
            }
            let parts: Vec<_> = reference.id.split(':').collect();
            if parts.len() != 4
                || parts[0] != "vault"
                || parts[1] != identities.vault_id()
                || parts[2] != "user"
            {
                bail!("reference belongs to a different physical root or source");
            }
            // Validate kind without refreshing or changing its observation CAS.
            let kind = identities.resource_kind(parts[3])?;
            if !matches!(
                (reference.kind, kind),
                (ResourceKind::VaultNote, VaultResourceKind::Note)
                    | (ResourceKind::VaultFolder, VaultResourceKind::Folder)
            ) {
                bail!("reference kind does not match its native identity");
            }
            identities.reconcile_move(
                key(domain, &input, command_id)?,
                parts[3],
                expected_native_revision,
                &VaultPath::parse(path)?,
            )?
        }
        VaultReconcileCommand::QuarantineJournal {
            command_id,
            operation_id,
            expected_intent_digest,
        } => identities.quarantine_journal(
            key(domain, &input, command_id)?,
            operation_id,
            expected_intent_digest,
        )?,
    };
    let mut resources = Vec::new();
    let mut graph_revision = None;
    for id in &receipt.resource_ids {
        // Replays publish the current observation, never the historical locator.
        // Quarantine must project unavailable facts before a separate refresh.
        let retained = identities.retained_resource(id)?;
        let record = if receipt.disposition == ReconciliationDisposition::ExternalMoveAdopted
            || retained.resolution == ResourceResolution::Available
        {
            identities.observe_id(id)?
        } else {
            retained
        };
        let observation = project(store, domain, identities.vault_id(), record)?;
        resources.push(observation["resource"].clone());
        graph_revision = observation["graph_revision"].as_u64();
    }
    bounded_response(serde_json::json!({
        "reconciliation":receipt, "resources":resources, "graph_revision":graph_revision,
        "coverage":"user_vault_reconciliation",
        "native_outcome":if receipt.disposition == ReconciliationDisposition::JournalQuarantined { "unresolved" } else { "physical_move_verified" },
        "file_effects_replayed":false,
    }))
}

impl WorkUnitHost {
    pub async fn reconcile_native(
        &self,
        turn: &TurnExecutionContext,
        input: WorkNativeReconcileInput,
    ) -> Result<serde_json::Value> {
        let domain = admitted_domain(turn, true)?;
        if input.root_id.is_empty() || input.root_id.len() > 256 {
            bail!("explicit vault root ID is required");
        }
        let store = self.store.clone();
        self.execution
            .run(ExecutionClass::Observation, 8 * 1024 * 1024, move || {
                Ok((|| {
                    let path = crate::vault::roots::vault_root_by_id(&input.root_id)?;
                    let owner = crate::vault::owner::ensure_owner_for_root_for_repair(path)?;
                    reconcile(&store, &domain, &owner, input)
                })())
            })
            .await?
    }
}
