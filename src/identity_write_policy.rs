//! Medousa overlay on Stasis identity commit tiers (AX-4d partial).

use serde_json::Value;
use stasis::ports::outbound::memory::identity_memory_models::{
    CommitEntityUpdateRequest, IdentityEntityType, ProposeEntityUpdateRequest, UpdateSource,
    UpdateTier,
};

use crate::product_config::IdentityProductConfig;

#[derive(Debug, Clone)]
pub struct IdentityCommitGate {
    pub allowed: bool,
    pub reason: Option<String>,
}

pub fn load_identity_product_config() -> IdentityProductConfig {
    crate::load_product_config().identity
}

pub fn evaluate_identity_commit(
    config: &IdentityProductConfig,
    proposal: &ProposeEntityUpdateRequest,
    tier: UpdateTier,
    commit: &CommitEntityUpdateRequest,
) -> IdentityCommitGate {
    if !config.enabled {
        return IdentityCommitGate {
            allowed: false,
            reason: Some("identity writes disabled in product_config".to_string()),
        };
    }

    if matches!(tier, UpdateTier::ApprovalRequired) && commit.approver.is_none() {
        return IdentityCommitGate {
            allowed: false,
            reason: Some(
                "approval_required tier: do not commit from agent; surface proposal_id to operator"
                    .to_string(),
            ),
        };
    }

    if proposal.source == UpdateSource::UserDirect {
        return IdentityCommitGate {
            allowed: true,
            reason: None,
        };
    }

    if proposal.confidence < config.auto_commit_min_confidence {
        return IdentityCommitGate {
            allowed: false,
            reason: Some(format!(
                "confidence {} below auto_commit_min_confidence {}",
                proposal.confidence, config.auto_commit_min_confidence
            )),
        };
    }

    if !patch_fields_allowed(config, &proposal.patch) {
        return IdentityCommitGate {
            allowed: false,
            reason: Some(
                "patch contains fields not on model_inferred_auto_commit_fields allowlist"
                    .to_string(),
            ),
        };
    }

    if blocks_autonomy_widen(&proposal.patch) {
        return IdentityCommitGate {
            allowed: false,
            reason: Some(
                "policy denied: autonomy_scope.allow cannot be widened via model_inferred commit"
                    .to_string(),
            ),
        };
    }

    let _ = proposal.entity_type;
    IdentityCommitGate {
        allowed: true,
        reason: None,
    }
}

fn flatten_patch_paths(prefix: &str, value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let next = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                flatten_patch_paths(&next, child, out);
            }
        }
        _ => out.push(prefix.to_string()),
    }
}

fn patch_field_allowed(config: &IdentityProductConfig, path: &str) -> bool {
    config.model_inferred_auto_commit_fields.iter().any(|allowed| {
        if let Some(prefix) = allowed.strip_suffix(".*") {
            path.starts_with(prefix)
                && path.len() > prefix.len()
                && path.as_bytes().get(prefix.len()) == Some(&b'.')
        } else {
            allowed == path
        }
    })
}

fn patch_fields_allowed(config: &IdentityProductConfig, patch: &Value) -> bool {
    let mut paths = Vec::new();
    flatten_patch_paths("", patch, &mut paths);
    if paths.is_empty() {
        return false;
    }
    paths.iter().all(|path| patch_field_allowed(config, path))
}

fn blocks_autonomy_widen(patch: &Value) -> bool {
    let mut paths = Vec::new();
    flatten_patch_paths("", patch, &mut paths);
    paths.iter().any(|p| p == "autonomy_scope.allow")
}

pub fn parse_identity_entity_type(raw: &str) -> Result<IdentityEntityType, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "persona" | "persona_entity" | "personaentity" => Ok(IdentityEntityType::PersonaEntity),
        "user" | "user_entity" | "userentity" => Ok(IdentityEntityType::UserEntity),
        "contact" | "contact_entity" | "contactentity" => Ok(IdentityEntityType::ContactEntity),
        "channel" | "channel_profile" | "channel_profile_entity" | "channelprofileentity" => {
            Ok(IdentityEntityType::ChannelProfileEntity)
        }
        "policy" | "policy_profile" | "policy_profile_entity" | "policyprofileentity" => {
            Ok(IdentityEntityType::PolicyProfileEntity)
        }
        "relationship" | "relationship_entity" | "relationshipentity" => {
            Ok(IdentityEntityType::RelationshipEntity)
        }
        other => Err(format!("unsupported identity entity type: {other}")),
    }
}

pub fn parse_update_source(raw: Option<&str>) -> Result<UpdateSource, String> {
    match raw
        .unwrap_or("model_inferred")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "user_direct" | "user" => Ok(UpdateSource::UserDirect),
        "model_inferred" | "model" => Ok(UpdateSource::ModelInferred),
        "system_event" | "system" => Ok(UpdateSource::SystemEvent),
        other => Err(format!(
            "unsupported update source '{other}', expected user_direct|model_inferred|system_event"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn preferences_prefix_allowlist_matches_nested_paths() {
        let config = IdentityProductConfig::default();
        let patch = json!({ "preferences.beverage": "matcha" });
        assert!(patch_fields_allowed(&config, &patch));
    }
}
