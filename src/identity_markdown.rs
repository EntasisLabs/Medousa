//! Export identity memory entities to OpenClaw-compatible markdown files.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use stasis::ports::outbound::memory::identity_memory_models::IdentityContextMode;
use stasis::ports::outbound::memory::identity_memory_store::IdentityMemoryStore;

pub fn identity_markdown_export_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("medousa")
        .join("identity-export")
}

pub struct IdentityMarkdownExport {
    pub soul_md: String,
    pub user_md: String,
    pub identity_md: String,
}

pub async fn export_identity_markdown(
    store: &dyn IdentityMemoryStore,
    user_id: Option<&str>,
) -> Result<IdentityMarkdownExport> {
    let user_id = user_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| crate::identity_memory::resolve_identity_user_id(None));

    let context = store
        .get_identity_context(
            &crate::identity_memory::build_identity_context_request(
                user_id.clone(),
                crate::identity_memory::resolve_identity_persona_id(),
                crate::identity_memory::resolve_identity_channel_id(None),
                8,
                IdentityContextMode::Cognitive,
            ),
        )
        .await
        .context("load identity context for markdown export")?;

    let persona = context
        .persona
        .as_ref()
        .map(|entity| entity.display_name.as_str())
        .unwrap_or("Medousa Operator Assistant");

    let soul_md = format!(
        "# SOUL\n\n\
         Display name: {persona}\n\n\
         ## Operating stance\n\
         Policy-guided cognitive operator. Evidence-first, tool-grounded, lane-aware.\n\n\
         ## Autonomy\n\
         Interactive lane: full tool surface. Scheduled lane: recurring + workflow schedule only. \
         Heartbeat lane: read-only observability.\n"
    );

    let user_md = format!(
        "# USER\n\n\
         User id: {user_id}\n\n\
         ## Preferences\n\
         _(Edit this file or use identity memory propose/commit API.)_\n"
    );

    let mut identity_md = String::from("# IDENTITY\n\n");
    identity_md.push_str(&format!("User: {user_id}\n"));
    if let Some(persona_entity) = context.persona.as_ref() {
        identity_md.push_str(&format!(
            "Persona: {} ({})\n",
            persona_entity.display_name, persona_entity.persona_id
        ));
    }
    identity_md.push_str(&format!(
        "Policy profiles: {}\n",
        context
            .policy_profiles
            .iter()
            .map(|profile| profile.policy_profile_id.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    ));

    Ok(IdentityMarkdownExport {
        soul_md,
        user_md,
        identity_md,
    })
}

pub async fn write_identity_markdown_export(
    store: &dyn IdentityMemoryStore,
    user_id: Option<&str>,
    dir: &Path,
) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)
        .with_context(|| format!("create identity export dir {}", dir.display()))?;

    let export = export_identity_markdown(store, user_id).await?;
    std::fs::write(dir.join("SOUL.md"), export.soul_md)?;
    std::fs::write(dir.join("USER.md"), export.user_md)?;
    std::fs::write(dir.join("IDENTITY.md"), export.identity_md)?;

    Ok(dir.to_path_buf())
}
