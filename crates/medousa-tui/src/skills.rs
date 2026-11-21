use super::daemon_commands::daemon_client;
use anyhow::{Context, Result, bail};
use medousa_types::ManuscriptCatalogQuery;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillRunArgs {
    pub manuscript_id: String,
    pub script: Option<String>,
    pub extra: String,
}

pub fn parse_skill_command_args(args: &str) -> Result<SkillRunArgs> {
    let trimmed = args.trim();
    if trimmed.is_empty() {
        bail!("usage: /skill <manuscript-id> [scripts/run.sh] [extra instructions]");
    }
    let mut parts = trimmed.split_whitespace().collect::<Vec<_>>();
    let manuscript_id = parts.remove(0).to_string();
    let mut script = None;
    if parts.first().is_some_and(|value| {
        value.contains('/') || value.ends_with(".sh") || value.ends_with(".py")
    }) {
        script = Some(parts.remove(0).to_string());
    }
    let extra = parts.join(" ");
    Ok(SkillRunArgs {
        manuscript_id,
        script,
        extra,
    })
}

pub async fn list(url: &str) -> Result<String> {
    let catalog = daemon_client(url)?
        .manuscripts()
        .list(&ManuscriptCatalogQuery {
            prefix: None,
            limit: None,
            skills_only: Some(true),
        })
        .await?;
    let lines: Vec<_> = catalog
        .manuscripts
        .iter()
        .map(|m| {
            let scripts = m
                .scripts
                .iter()
                .map(|s| format!("{} ({})", s.relative_path, s.risk_class))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "{} ({}) — scripts: {scripts} | openshell={}",
                m.id, m.name, m.openshell_enabled
            )
        })
        .collect();
    Ok(if lines.is_empty() {
        "No imported skills".into()
    } else {
        lines.join("\n")
    })
}

pub async fn prompt(url: &str, input: &str) -> Result<String> {
    let args = parse_skill_command_args(input)?;
    let entry = daemon_client(url)?
        .manuscripts()
        .get(&args.manuscript_id)
        .await?;
    let script = args
        .script
        .as_deref()
        .or_else(|| entry.scripts.first().map(|s| s.relative_path.as_str()))
        .context("manuscript has no runnable scripts")?;
    let risk = entry
        .scripts
        .iter()
        .find(|s| s.relative_path == script)
        .map(|s| s.risk_class.as_str())
        .unwrap_or("unknown");
    let task_extra = if args.extra.trim().is_empty() {
        String::new()
    } else {
        format!(" Extra operator note: {}", args.extra.trim())
    };

    let lines = [
        format!(
            "Run imported skill specialty '{}' the way Medousa would — delegate to a research worker, never execute scripts on the host.",
            args.manuscript_id
        ),
        "Steps:".to_string(),
        format!(
            "1. Call cognition_workshop_mutate action=workshop.spawn with intent=research, manuscript_id=\"{}\",",
            args.manuscript_id
        ),
        format!(
            "   task: \"Discover skill scripts, cognition_skill_propose security_level=sandbox script={script} (risk {risk}), then cognition_skill_probe with operator_approved=true. Return stdout, exit code, and policy receipts.{task_extra}\"",
        ),
        format!(
            "   user_ack: \"Running skill {} in sealed OpenShell sandbox…\"",
            args.manuscript_id
        ),
        "2. After the worker completes, synthesize a concise operator summary.".to_string(),
    ];
    Ok(lines.join("\n"))
}
