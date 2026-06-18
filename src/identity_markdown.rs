//! Export identity memory as derived markdown views (ranked cognitive slice).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use stasis::ports::outbound::memory::identity_memory_models::IdentityContextMode;
use stasis::ports::outbound::memory::identity_memory_store::IdentityMemoryStore;

use crate::cognitive_identity::{
    CognitiveIdentitySnapshot, DigestCompileOptions, compile_relational_memory_digest_with_options,
    DEFAULT_RELATIONAL_DIGEST_BUDGET,
};

pub fn identity_markdown_export_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("medousa")
        .join("identity-export")
}

pub struct IdentityMarkdownExport {
    pub soul_md: String,
    pub user_md: String,
    pub people_md: String,
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
                32,
                IdentityContextMode::Cognitive,
            ),
        )
        .await
        .context("load identity context for markdown export")?;

    let snapshot = CognitiveIdentitySnapshot {
        user_id: user_id.clone(),
        user: context.user,
        contacts: context.contacts,
        relationships: context.relationships,
        error: None,
    };

    let ranked = compile_relational_memory_digest_with_options(
        &snapshot,
        DigestCompileOptions::from_product_config(DEFAULT_RELATIONAL_DIGEST_BUDGET),
    );

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
         ## Recall\n\
         This export is a **derived ranked slice** of identity memory. For live lookup use \
         `cognition_identity_recall` or `medousa identity-remember`.\n"
    );

    let preferences_body = if ranked.preference_lines.is_empty() {
        "_No preferences in ranked export slice._".to_string()
    } else {
        ranked
            .preference_lines
            .iter()
            .map(|line| format!("- {line}"))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let user_md = format!(
        "# USER\n\n\
         User id: {user_id}\n\n\
         ## Preferences (ranked export)\n\
         {preferences_body}\n\n\
         Omitted from export: {} preference(s). Use `cognition_identity_recall` for full lookup.\n",
        ranked.stats.omitted_preferences
    );

    let people_body = if ranked.people_lines.is_empty() {
        "_No people in ranked export slice._".to_string()
    } else {
        ranked
            .people_lines
            .iter()
            .map(|line| format!("- {line}"))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let people_md = format!(
        "# PEOPLE\n\n\
         ## Relationships (ranked export)\n\
         {people_body}\n\n\
         Omitted from export: {} relationship(s). Use `cognition_identity_recall` for full lookup.\n",
        ranked.stats.omitted_people
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
    identity_md.push_str(&format!(
        "\n## Ranked digest\n```text\n{}\n```\n",
        ranked.text
    ));

    Ok(IdentityMarkdownExport {
        soul_md,
        user_md,
        people_md,
        identity_md,
    })
}

pub async fn compile_identity_digest_preview(
    store: &dyn IdentityMemoryStore,
    user_id: Option<&str>,
) -> Result<crate::cognitive_identity::RankedDigest> {
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
                32,
                IdentityContextMode::Cognitive,
            ),
        )
        .await
        .context("load identity context for digest preview")?;

    let snapshot = CognitiveIdentitySnapshot {
        user_id,
        user: context.user,
        contacts: context.contacts,
        relationships: context.relationships,
        error: None,
    };

    Ok(compile_relational_memory_digest_with_options(
        &snapshot,
        DigestCompileOptions::from_product_config(DEFAULT_RELATIONAL_DIGEST_BUDGET),
    ))
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
    std::fs::write(dir.join("PEOPLE.md"), export.people_md)?;
    std::fs::write(dir.join("IDENTITY.md"), export.identity_md)?;

    Ok(dir.to_path_buf())
}
