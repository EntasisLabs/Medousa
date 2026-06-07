//! Chat/TUI ingest helpers — list skill specialties and trigger worker runs like Medousa would.

use anyhow::{Context, Result, bail};

use crate::identity_manuscript::list_manuscripts;
use crate::skill_execution::{SkillScriptRiskClass, discover_skill_for_manuscript};

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
    if parts
        .first()
        .is_some_and(|value| value.contains('/') || value.ends_with(".sh") || value.ends_with(".py"))
    {
        script = Some(parts.remove(0).to_string());
    }
    let extra = parts.join(" ");
    Ok(SkillRunArgs {
        manuscript_id,
        script,
        extra,
    })
}

pub fn resolve_skill_run_script(manuscript_id: &str, script: Option<&str>) -> Result<String> {
    if let Some(path) = script.map(str::trim).filter(|value| !value.is_empty()) {
        return Ok(path.to_string());
    }
    let discovery = discover_skill_for_manuscript(manuscript_id)
        .with_context(|| format!("discover scripts for manuscript '{manuscript_id}'"))?;
    discovery
        .scripts
        .first()
        .map(|entry| entry.relative_path.clone())
        .ok_or_else(|| {
            anyhow::anyhow!("manuscript '{manuscript_id}' has no runnable scripts")
        })
}

/// Host-bus prompt: orchestrate via `cognition_spawn_turn_worker` (worker gets manuscript tools).
pub fn build_skill_run_ingest_prompt(args: &SkillRunArgs) -> Result<String> {
    let script = resolve_skill_run_script(&args.manuscript_id, args.script.as_deref())?;
    let discovery = discover_skill_for_manuscript(&args.manuscript_id)?;
    let risk = discovery
        .scripts
        .iter()
        .find(|entry| entry.relative_path == script)
        .map(|entry| risk_class_label(entry.risk_class))
        .unwrap_or("unknown");

    let task_extra = if args.extra.trim().is_empty() {
        String::new()
    } else {
        format!(" Extra operator note: {}", args.extra.trim())
    };

    let lines = vec![
        format!(
            "Run imported skill specialty '{}' the way Medousa would — delegate to a research worker, never execute scripts on the host.",
            args.manuscript_id
        ),
        "Steps:".to_string(),
        format!(
            "1. Call cognition_spawn_turn_worker with intent=research, manuscript_id=\"{}\",",
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

pub fn format_skill_manuscripts_list() -> Result<String> {
    let entries = list_manuscripts()?;
    let mut lines = vec![
        "Imported skill specialties (manuscripts with runnable scripts):".to_string(),
    ];
    let mut count = 0usize;
    for entry in entries {
        let Ok(discovery) = discover_skill_for_manuscript(&entry.id) else {
            continue;
        };
        if !discovery.has_scripts {
            continue;
        }
        count += 1;
        let scripts: Vec<_> = discovery
            .scripts
            .iter()
            .map(|script| {
                format!(
                    "{} ({})",
                    script.relative_path,
                    risk_class_label(script.risk_class)
                )
            })
            .collect();
        let openshell = crate::identity_manuscript::build_manuscript_context(&entry.id)
            .ok()
            .map(|ctx| ctx.openshell_enabled)
            .unwrap_or(false);
        lines.push(format!(
            "• {} ({}) — scripts: {} | openshell={}",
            entry.id,
            entry.name,
            if scripts.is_empty() {
                "(none)".to_string()
            } else {
                scripts.join(", ")
            },
            if openshell { "yes" } else { "no" }
        ));
    }
    if count == 0 {
        lines.push("  (none — run: medousa skill-import <path>)".to_string());
    } else {
        lines.push(String::new());
        lines.push("Trigger: /skill <id> [scripts/run.sh] [extra]".to_string());
        lines.push("Example: /skill echo-skill scripts/echo.sh".to_string());
    }
    Ok(lines.join("\n"))
}

pub fn risk_class_label(risk: SkillScriptRiskClass) -> &'static str {
    match risk {
        SkillScriptRiskClass::ReadOnly => "readonly",
        SkillScriptRiskClass::FilesystemWrite => "fs_write",
        SkillScriptRiskClass::Network => "network",
        SkillScriptRiskClass::ShellExec => "shell",
        SkillScriptRiskClass::Destructive => "destructive",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_skill_args_with_script() {
        let args = parse_skill_command_args("echo-skill scripts/echo.sh focus calendar").expect("parse");
        assert_eq!(args.manuscript_id, "echo-skill");
        assert_eq!(args.script.as_deref(), Some("scripts/echo.sh"));
        assert_eq!(args.extra, "focus calendar");
    }

    #[test]
    fn parse_skill_args_id_only() {
        let args = parse_skill_command_args("echo-skill").expect("parse");
        assert_eq!(args.manuscript_id, "echo-skill");
        assert!(args.script.is_none());
    }
}
