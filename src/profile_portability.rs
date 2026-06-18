//! Profile export/import — identity subgraph + Locus scoped sessions.

use std::sync::Arc;

use anyhow::{Context, Result, bail};
use chrono::Utc;
use locus_core_rs::{
    ContextQueryService, NodeStore, StoreContextService, SttpNodeParser, TreeSitterValidator,
};
use serde::{Deserialize, Serialize};
use stasis::ports::outbound::memory::identity_memory_models::{
    GetIdentityContextResponse, RelationshipEntity,
};
use stasis::ports::outbound::memory::identity_memory_store::IdentityMemoryStore;

use crate::identity_memory::{
    full_identity_context_request, profile_channel_id_for_user_id, resolve_identity_persona_id,
    seed_workshop_profile_user,
};
use crate::identity_store_ext::MedousaIdentityMemoryStore;
use crate::locus_memory::{
    IDENTITY_BRIDGE_CHAT_SESSION, default_interactive_ingest_profile,
    interactive_store_retry_policy, scoped_locus_session,
};
use crate::session_catalog;
use crate::user_profiles::{UserProfileRegistry, profile_slug_from_id};

pub const PROFILE_EXPORT_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileExportBundle {
    pub format_version: u32,
    pub profile_id: String,
    pub display_name: String,
    pub exported_at: chrono::DateTime<Utc>,
    pub identity: ProfileIdentityExport,
    pub locus: ProfileLocusExport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileIdentityExport {
    pub user_id: String,
    pub channel_id: String,
    pub identity_context: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileLocusExport {
    pub profile_slug: String,
    pub sessions: Vec<ProfileLocusSessionExport>,
    pub node_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileLocusSessionExport {
    pub chat_session_id: String,
    pub scoped_session_id: String,
    pub nodes: Vec<ProfileLocusNodeExport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileLocusNodeExport {
    pub sync_key: String,
    pub raw: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfileImportSummary {
    pub dry_run: bool,
    pub profile_id: String,
    pub created_profile: bool,
    pub identity_user_imported: bool,
    pub contacts_imported: usize,
    pub relationships_imported: usize,
    pub locus_nodes_imported: usize,
    pub locus_sessions_touched: usize,
}

pub fn profile_display_name(registry: &UserProfileRegistry, profile_id: &str) -> Option<String> {
    registry
        .list_profiles()
        .into_iter()
        .find(|profile| profile.profile_id == profile_id)
        .map(|profile| profile.display_name)
}

pub async fn export_profile_bundle(
    registry: &UserProfileRegistry,
    identity_store: &MedousaIdentityMemoryStore,
    locus_store: Arc<dyn NodeStore>,
    profile_id: &str,
    session_limit: usize,
    node_limit_per_session: usize,
) -> Result<ProfileExportBundle> {
    let profile_id = profile_id.trim();
    if profile_id.is_empty() {
        bail!("profile_id must not be empty");
    }

    let display_name = profile_display_name(registry, profile_id)
        .ok_or_else(|| anyhow::anyhow!("profile not found: {profile_id}"))?;
    let profile_slug = profile_slug_from_id(profile_id)
        .ok_or_else(|| anyhow::anyhow!("invalid profile_id: {profile_id}"))?;

    let channel_id = profile_channel_id_for_user_id(profile_id);
    let identity_context = identity_store
        .get_identity_context(&full_identity_context_request(
            profile_id.to_string(),
            resolve_identity_persona_id(),
            channel_id.clone(),
            64,
        ))
        .await
        .context("load identity context for export")?;
    let identity_context =
        serde_json::to_value(&identity_context).context("encode identity context")?;

    let mut chat_session_ids =
        session_catalog::list_chat_session_ids_for_profile(profile_id, session_limit.max(1));
    if !chat_session_ids
        .iter()
        .any(|id| id == IDENTITY_BRIDGE_CHAT_SESSION)
    {
        chat_session_ids.push(IDENTITY_BRIDGE_CHAT_SESSION.to_string());
    }

    let context_query = ContextQueryService::new(locus_store.clone());
    let node_limit = node_limit_per_session.clamp(1, 500);
    let mut sessions = Vec::new();
    let mut node_count = 0usize;

    for chat_session_id in chat_session_ids {
        let scoped_session_id = scoped_locus_session(profile_slug, &chat_session_id);
        let nodes = context_query
            .list_nodes_async(node_limit, Some(scoped_session_id.as_str()))
            .await
            .with_context(|| format!("list locus nodes for {scoped_session_id}"))?
            .nodes;
        if nodes.is_empty() {
            continue;
        }
        node_count += nodes.len();
        sessions.push(ProfileLocusSessionExport {
            chat_session_id,
            scoped_session_id,
            nodes: nodes
                .into_iter()
                .map(|node| ProfileLocusNodeExport {
                    sync_key: node.sync_key,
                    raw: node.raw,
                })
                .collect(),
        });
    }

    Ok(ProfileExportBundle {
        format_version: PROFILE_EXPORT_FORMAT_VERSION,
        profile_id: profile_id.to_string(),
        display_name,
        exported_at: Utc::now(),
        identity: ProfileIdentityExport {
            user_id: profile_id.to_string(),
            channel_id,
            identity_context,
        },
        locus: ProfileLocusExport {
            profile_slug: profile_slug.to_string(),
            sessions,
            node_count,
        },
    })
}

pub async fn import_profile_bundle(
    registry: &mut UserProfileRegistry,
    identity_store: &MedousaIdentityMemoryStore,
    locus_store: Arc<dyn NodeStore>,
    bundle: &ProfileExportBundle,
    dry_run: bool,
) -> Result<ProfileImportSummary> {
    if bundle.format_version != PROFILE_EXPORT_FORMAT_VERSION {
        bail!(
            "unsupported export format version {} (expected {PROFILE_EXPORT_FORMAT_VERSION})",
            bundle.format_version
        );
    }

    let profile_id = bundle.profile_id.trim();
    if profile_id.is_empty() {
        bail!("bundle profile_id must not be empty");
    }

    let slug = profile_slug_from_id(profile_id)
        .ok_or_else(|| anyhow::anyhow!("invalid bundle profile_id: {profile_id}"))?;

    let mut summary = ProfileImportSummary {
        dry_run,
        profile_id: profile_id.to_string(),
        ..Default::default()
    };

    let profile_exists = registry
        .list_profiles()
        .iter()
        .any(|profile| profile.profile_id == profile_id);
    if !profile_exists {
        summary.created_profile = true;
        if !dry_run {
            registry
                .create_profile(slug, &bundle.display_name)
                .with_context(|| format!("create profile {profile_id}"))?;
            seed_workshop_profile_user(identity_store, profile_id)
                .await
                .with_context(|| format!("seed profile user {profile_id}"))?;
        }
    }

    let context: GetIdentityContextResponse =
        serde_json::from_value(bundle.identity.identity_context.clone())
            .context("decode identity export")?;

    if let Some(user) = context.user {
        summary.identity_user_imported = true;
        if !dry_run {
            identity_store
                .upsert_user_entity(user)
                .await
                .context("import user entity")?;
        }
    }

    summary.contacts_imported = context.contacts.len();
    if !dry_run {
        for contact in &context.contacts {
            identity_store
                .upsert_contact_entity(contact.clone())
                .await
                .with_context(|| format!("import contact {}", contact.contact_id))?;
        }
    }

    let relationships: Vec<RelationshipEntity> = context
        .relationships
        .into_iter()
        .filter(|relationship| relationship_touches_user(relationship, profile_id))
        .collect();
    summary.relationships_imported = relationships.len();
    if !dry_run {
        for relationship in relationships {
            identity_store
                .upsert_relationship_entity(relationship)
                .await
                .context("import relationship")?;
        }
    }

    summary.locus_sessions_touched = bundle.locus.sessions.len();
    if !dry_run {
        let validator = Arc::new(TreeSitterValidator::new());
        let parser = SttpNodeParser::with_profile(default_interactive_ingest_profile());
        let store_service = Arc::new(StoreContextService::new_with_policy(
            locus_store,
            validator,
            interactive_store_retry_policy(),
            parser,
        ));

        for session in &bundle.locus.sessions {
            let target_scoped = scoped_locus_session(slug, &session.chat_session_id);
            for node in &session.nodes {
                let result = store_service
                    .store_async(&node.raw, &target_scoped)
                    .await;
                if let Some(err) = result.validation_error {
                    bail!(
                        "import locus node sync_key={} session={target_scoped}: {err}",
                        node.sync_key
                    );
                }
                summary.locus_nodes_imported += 1;
            }
        }
    } else {
        summary.locus_nodes_imported = bundle.locus.node_count;
    }

    Ok(summary)
}

fn relationship_touches_user(relationship: &RelationshipEntity, user_id: &str) -> bool {
    relationship.source_entity_ref.entity_id == user_id
        || relationship.target_entity_ref.entity_id == user_id
}
