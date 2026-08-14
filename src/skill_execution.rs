//! Skill script discovery, risk assessment, and OpenShell execution helpers (H6–H7).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::identity_manuscript::{
    ManuscriptContext, resolve_manuscript_path, user_manuscripts_dir,
};
use crate::openshell_sandbox_run::OpenshellSandboxRunPayload;
use crate::store_root::{StoreEntryKind, StorePath, StoreRoot};

const SKILL_SCRIPT_DIRS: &[&str] = &["scripts", "bin"];
const MAX_SKILL_SCRIPT_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SkillSecurityLevel {
    /// Inspect SKILL.md and script inventory only — no execution.
    Observe,
    /// Classify risk and emit an adoption proposal — no execution.
    Propose,
    /// Execute inside OpenShell sandbox when manuscript/policy allow.
    Sandbox,
    /// Blocked by policy or risk class.
    Deny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SkillScriptRiskClass {
    ReadOnly,
    FilesystemWrite,
    Network,
    ShellExec,
    Destructive,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SkillScriptEntry {
    pub relative_path: String,
    pub risk_class: SkillScriptRiskClass,
    pub risk_score: u8,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SkillDiscoveryReport {
    pub skill_id: String,
    pub assets_dir: String,
    pub scripts: Vec<SkillScriptEntry>,
    pub max_risk_class: SkillScriptRiskClass,
    pub max_risk_score: u8,
    pub has_scripts: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SkillAdoptionProposal {
    pub skill_id: String,
    pub requested_level: SkillSecurityLevel,
    pub granted_level: SkillSecurityLevel,
    pub requires_approval: bool,
    pub approval_reasons: Vec<String>,
    pub policy_template: Option<String>,
    pub sandbox_from: Option<String>,
    pub script: Option<String>,
    pub rationale: String,
}

struct SkillAssetsLocation {
    root: StoreRoot,
    base_dir: PathBuf,
    relative: StorePath,
}

impl SkillAssetsLocation {
    fn ambient_path(&self) -> PathBuf {
        self.base_dir.join(self.relative.file_name())
    }
}

fn resolve_skill_assets_location(manuscript_id: &str) -> Result<SkillAssetsLocation> {
    let manuscript_id = medousa_types::authority_id::ManuscriptId::parse(manuscript_id)?;
    let manuscript_path = resolve_manuscript_path(manuscript_id.as_str())?;
    let base = manuscript_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(user_manuscripts_dir);
    let root = StoreRoot::open_nofollow(&base)
        .with_context(|| format!("open skill assets root {}", base.display()))?;
    let skill_md = StorePath::parse("SKILL.md")?;
    let storage_key = manuscript_id.storage_key();
    for name in [storage_key.as_str(), manuscript_id.as_str()] {
        let relative = StorePath::parse(name)?;
        let manifest = relative.join(&skill_md)?;
        if root.is_dir(&relative)? && root.is_file(&manifest)? {
            return Ok(SkillAssetsLocation {
                root,
                base_dir: base,
                relative,
            });
        }
    }
    bail!(
        "skill assets not found for manuscript '{manuscript_id}' under {}",
        base.display()
    )
}

pub fn resolve_skill_assets_dir(manuscript_id: &str) -> Result<PathBuf> {
    Ok(resolve_skill_assets_location(manuscript_id)?.ambient_path())
}

pub fn discover_skill_scripts(assets_dir: &Path) -> Result<Vec<SkillScriptEntry>> {
    let mut scripts = Vec::new();
    for dir_name in SKILL_SCRIPT_DIRS {
        let scripts_dir = assets_dir.join(dir_name);
        if !scripts_dir.is_dir() {
            continue;
        }
        collect_scripts_recursive(&scripts_dir, assets_dir, &mut scripts)?;
    }
    scripts.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    Ok(scripts)
}

fn collect_scripts_recursive(
    current: &Path,
    assets_root: &Path,
    scripts: &mut Vec<SkillScriptEntry>,
) -> Result<()> {
    for entry in
        std::fs::read_dir(current).with_context(|| format!("read {}", current.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let metadata = path.symlink_metadata()?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            collect_scripts_recursive(&path, assets_root, scripts)?;
            continue;
        }
        let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if file_name.starts_with('.') {
            continue;
        }
        let is_script = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| {
                matches!(
                    ext.to_ascii_lowercase().as_str(),
                    "sh" | "bash" | "py" | "rb" | "pl" | "js" | "ts"
                )
            })
            || script_path_executable(&path);
        if !is_script {
            continue;
        }
        let relative = path
            .strip_prefix(assets_root)
            .unwrap_or(&path)
            .display()
            .to_string();
        let assessment = assess_skill_script(&path)?;
        scripts.push(SkillScriptEntry {
            relative_path: relative,
            risk_class: assessment.risk_class,
            risk_score: assessment.risk_score,
            rationale: assessment.rationale,
        });
    }
    Ok(())
}

pub(crate) struct ScriptAssessment {
    risk_class: SkillScriptRiskClass,
    risk_score: u8,
    rationale: String,
}

pub(crate) fn assess_skill_script(path: &Path) -> Result<ScriptAssessment> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("read skill script {}", path.display()))?;
    Ok(assess_skill_script_text(&raw))
}

fn assess_skill_script_text(raw: &str) -> ScriptAssessment {
    let lower = raw.to_ascii_lowercase();
    let mut risk_score = 10u8;
    let mut tags = Vec::new();

    if lower.contains("rm -rf")
        || lower.contains("mkfs")
        || lower.contains(":(){")
        || lower.contains("dd if=")
    {
        tags.push("destructive");
        risk_score = risk_score.saturating_add(70);
    }
    if lower.contains("curl ")
        || lower.contains("wget ")
        || lower.contains("requests.")
        || lower.contains("http://")
        || lower.contains("https://")
    {
        tags.push("network");
        risk_score = risk_score.saturating_add(40);
    }
    if lower.contains("chmod ")
        || lower.contains("chown ")
        || lower.contains("> /")
        || lower.contains("tee /")
    {
        tags.push("filesystem_write");
        risk_score = risk_score.saturating_add(25);
    }
    if lower.contains("eval ")
        || lower.contains("bash -c")
        || lower.contains("sh -c")
        || lower.contains("exec ")
    {
        tags.push("shell_exec");
        risk_score = risk_score.saturating_add(20);
    }

    let risk_class = if tags.contains(&"destructive") {
        SkillScriptRiskClass::Destructive
    } else if tags.contains(&"network") {
        SkillScriptRiskClass::Network
    } else if tags.contains(&"shell_exec") {
        SkillScriptRiskClass::ShellExec
    } else if tags.contains(&"filesystem_write") {
        SkillScriptRiskClass::FilesystemWrite
    } else {
        SkillScriptRiskClass::ReadOnly
    };

    let rationale = if tags.is_empty() {
        "No elevated risk markers detected in script body.".to_string()
    } else {
        format!("Risk markers: {}", tags.join(", "))
    };

    ScriptAssessment {
        risk_class,
        risk_score: risk_score.min(100),
        rationale,
    }
}

fn discover_skill_scripts_in_store(
    root: &StoreRoot,
    assets: Option<&StorePath>,
) -> Result<Vec<SkillScriptEntry>> {
    let mut scripts = Vec::new();
    for dir_name in SKILL_SCRIPT_DIRS {
        let child = StorePath::parse(dir_name)?;
        let scripts_dir = match assets {
            Some(base) => base.join(&child)?,
            None => child,
        };
        if root.is_dir(&scripts_dir)? {
            collect_scripts_from_store(root, &scripts_dir, assets, &mut scripts)?;
        }
    }
    scripts.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    Ok(scripts)
}

fn collect_scripts_from_store(
    root: &StoreRoot,
    current: &StorePath,
    assets: Option<&StorePath>,
    scripts: &mut Vec<SkillScriptEntry>,
) -> Result<()> {
    for entry in root.list_directory(current)? {
        let path = current.join(&entry.path)?;
        match entry.kind {
            StoreEntryKind::Directory => {
                collect_scripts_from_store(root, &path, assets, scripts)?;
            }
            StoreEntryKind::File => {
                let file_name = entry.path.file_name();
                if file_name.starts_with('.') {
                    continue;
                }
                let bytes = root.read_limited(&path, MAX_SKILL_SCRIPT_BYTES)?;
                let known_extension = Path::new(file_name)
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| {
                        matches!(
                            ext.to_ascii_lowercase().as_str(),
                            "sh" | "bash" | "py" | "rb" | "pl" | "js" | "ts"
                        )
                    });
                if !known_extension && !bytes.starts_with(b"#!") {
                    continue;
                }
                let raw = std::str::from_utf8(&bytes)
                    .with_context(|| format!("skill script {path} is not UTF-8"))?;
                let assessment = assess_skill_script_text(raw);
                let full_path = path.to_string();
                let relative_path = assets
                    .map(|base| {
                        full_path
                            .strip_prefix(&format!("{base}/"))
                            .unwrap_or(&full_path)
                            .to_string()
                    })
                    .unwrap_or(full_path);
                scripts.push(SkillScriptEntry {
                    relative_path,
                    risk_class: assessment.risk_class,
                    risk_score: assessment.risk_score,
                    rationale: assessment.rationale,
                });
            }
            StoreEntryKind::Link | StoreEntryKind::Other => continue,
        }
    }
    Ok(())
}

pub(crate) fn skill_has_runnable_scripts_in_store(root: &StoreRoot) -> bool {
    discover_skill_scripts_in_store(root, None).is_ok_and(|scripts| !scripts.is_empty())
}

pub fn discover_skill_for_manuscript(manuscript_id: &str) -> Result<SkillDiscoveryReport> {
    let assets = resolve_skill_assets_location(manuscript_id)?;
    let assets_dir = assets.ambient_path();
    let scripts = discover_skill_scripts_in_store(&assets.root, Some(&assets.relative))?;
    let has_scripts = !scripts.is_empty();
    let (max_risk_class, max_risk_score) = max_script_risk(&scripts);
    Ok(SkillDiscoveryReport {
        skill_id: manuscript_id.to_string(),
        assets_dir: assets_dir.display().to_string(),
        scripts,
        max_risk_class,
        max_risk_score,
        has_scripts,
    })
}

fn max_script_risk(scripts: &[SkillScriptEntry]) -> (SkillScriptRiskClass, u8) {
    scripts.iter().fold(
        (SkillScriptRiskClass::ReadOnly, 0u8),
        |(class, score), script| {
            if script.risk_score >= score {
                (script.risk_class, script.risk_score)
            } else {
                (class, score)
            }
        },
    )
}

pub fn skill_security_level_parse(raw: &str) -> Option<SkillSecurityLevel> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "observe" | "inspect" | "read" => Some(SkillSecurityLevel::Observe),
        "propose" | "proposal" => Some(SkillSecurityLevel::Propose),
        "sandbox" | "run" | "execute" => Some(SkillSecurityLevel::Sandbox),
        "deny" | "block" => Some(SkillSecurityLevel::Deny),
        _ => None,
    }
}

pub fn evaluate_skill_adoption(
    discovery: &SkillDiscoveryReport,
    manuscript: Option<&ManuscriptContext>,
    requested: SkillSecurityLevel,
    script: Option<&str>,
) -> SkillAdoptionProposal {
    let mut approval_reasons = Vec::new();
    let script_entry = script.and_then(|relative| {
        discovery
            .scripts
            .iter()
            .find(|entry| entry.relative_path == relative)
    });

    let mut granted = requested;
    if requested == SkillSecurityLevel::Sandbox {
        if !discovery.has_scripts {
            granted = SkillSecurityLevel::Deny;
            approval_reasons.push("no runnable scripts discovered".to_string());
        }
        if let Some(entry) = script_entry {
            if entry.risk_class == SkillScriptRiskClass::Destructive {
                granted = SkillSecurityLevel::Propose;
                approval_reasons
                    .push("destructive_command: script contains destructive markers".to_string());
            } else if entry.risk_class == SkillScriptRiskClass::Network {
                approval_reasons
                    .push("external_side_effect: script may perform network I/O".to_string());
            }
        } else if script.is_some() {
            granted = SkillSecurityLevel::Deny;
            approval_reasons.push("requested script not found in skill assets".to_string());
        }
        if let Some(manuscript) = manuscript
            && !manuscript.openshell_enabled
        {
            granted = SkillSecurityLevel::Propose;
            approval_reasons.push(
                "manuscript spec.openshell.enabled is false — propose import/enabled first"
                    .to_string(),
            );
        }
    }

    if requested == SkillSecurityLevel::Deny {
        granted = SkillSecurityLevel::Deny;
    }

    let requires_approval = !approval_reasons.is_empty() && granted == SkillSecurityLevel::Sandbox;

    let policy_template = manuscript
        .and_then(|ctx| ctx.openshell_policy_template.clone())
        .or_else(|| Some(default_policy_for_risk(discovery.max_risk_class).to_string()));
    let sandbox_from = manuscript
        .and_then(|ctx| ctx.openshell_sandbox_from.clone())
        .or_else(|| Some("medousa-openshell-sandbox:local".to_string()));

    let rationale = match granted {
        SkillSecurityLevel::Observe => {
            "Observation only — inventory scripts and SKILL.md without execution.".to_string()
        }
        SkillSecurityLevel::Propose => {
            "Adoption requires operator review before sandbox execution.".to_string()
        }
        SkillSecurityLevel::Sandbox => {
            if requires_approval {
                "Sandbox execution permitted after operator approval for flagged risk classes."
                    .to_string()
            } else {
                "Sandbox execution permitted under manuscript OpenShell policy.".to_string()
            }
        }
        SkillSecurityLevel::Deny => "Skill action denied by policy or missing assets.".to_string(),
    };

    SkillAdoptionProposal {
        skill_id: discovery.skill_id.clone(),
        requested_level: requested,
        granted_level: granted,
        requires_approval,
        approval_reasons,
        policy_template,
        sandbox_from,
        script: script.map(str::to_string),
        rationale,
    }
}

fn default_policy_for_risk(risk: SkillScriptRiskClass) -> &'static str {
    match risk {
        SkillScriptRiskClass::Network => "research-readonly",
        SkillScriptRiskClass::Destructive | SkillScriptRiskClass::ShellExec => "skill-sandbox",
        SkillScriptRiskClass::FilesystemWrite | SkillScriptRiskClass::ReadOnly => "skill-sandbox",
    }
}

pub fn build_skill_script_command(script_path: &Path) -> Result<Vec<String>> {
    let extension = script_path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase);
    let script = script_path.display().to_string();
    Ok(match extension.as_deref() {
        Some("py") => vec!["python3".to_string(), script],
        Some("rb") => vec!["ruby".to_string(), script],
        Some("js") | Some("ts") => vec!["node".to_string(), script],
        Some("pl") => vec!["perl".to_string(), script],
        _ => vec!["bash".to_string(), script],
    })
}

pub fn build_sandbox_payload_for_skill(
    manuscript_id: &str,
    script_relative: &str,
    manuscript: &ManuscriptContext,
    correlation_id: Option<String>,
) -> Result<OpenshellSandboxRunPayload> {
    let assets = resolve_skill_assets_location(manuscript_id)?;
    let assets_dir = assets.ambient_path();
    let script_relative = StorePath::parse(script_relative)?;
    let script_store_path = assets.relative.join(&script_relative)?;
    let script_path = assets_dir.join(script_relative.to_string());
    if !assets.root.is_file(&script_store_path)? {
        bail!("skill script not found: {}", script_path.display());
    }
    let command = build_skill_script_command(&script_path)?;
    let upload_dest = format!("/sandbox/{manuscript_id}");
    Ok(OpenshellSandboxRunPayload {
        command,
        sandbox_from: manuscript
            .openshell_sandbox_from
            .clone()
            .or_else(|| Some("medousa-openshell-sandbox:local".to_string())),
        policy_template: manuscript
            .openshell_policy_template
            .clone()
            .or_else(|| Some("skill-sandbox".to_string())),
        destroy_on_complete: true,
        workdir: Some(upload_dest.clone()),
        timeout_secs: Some(300),
        manuscript_id: Some(manuscript_id.to_string()),
        correlation_id,
        skill_assets_dir: Some(assets_dir.display().to_string()),
        skill_upload_dest: Some(upload_dest),
        skill_script: Some(script_relative.to_string()),
    })
}

pub fn discovery_report_json(report: &SkillDiscoveryReport) -> Value {
    json!({
        "skill_id": report.skill_id,
        "assets_dir": report.assets_dir,
        "has_scripts": report.has_scripts,
        "max_risk_class": report.max_risk_class,
        "max_risk_score": report.max_risk_score,
        "scripts": report.scripts,
    })
}

pub fn proposal_json(proposal: &SkillAdoptionProposal) -> Value {
    json!({
        "skill_id": proposal.skill_id,
        "requested_level": proposal.requested_level,
        "granted_level": proposal.granted_level,
        "requires_approval": proposal.requires_approval,
        "approval_reasons": proposal.approval_reasons,
        "policy_template": proposal.policy_template,
        "sandbox_from": proposal.sandbox_from,
        "script": proposal.script,
        "rationale": proposal.rationale,
    })
}

fn script_path_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .map(|meta| meta.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        false
    }
}

pub fn skill_has_runnable_scripts(skill_dir: &Path) -> bool {
    discover_skill_scripts(skill_dir)
        .map(|scripts| !scripts.is_empty())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn assess_readonly_echo_script() {
        let dir = std::env::temp_dir().join(format!("medousa-skill-risk-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("scripts")).expect("dir");
        let script = dir.join("scripts/echo.sh");
        fs::write(&script, "#!/bin/bash\necho hello\n").expect("write");
        let assessment = assess_skill_script(&script).expect("assess");
        assert_eq!(assessment.risk_class, SkillScriptRiskClass::ReadOnly);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn assess_network_script_scores_higher() {
        let dir = std::env::temp_dir().join(format!("medousa-skill-net-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("scripts")).expect("dir");
        let script = dir.join("scripts/fetch.sh");
        fs::write(&script, "#!/bin/bash\ncurl -fsSL https://example.com\n").expect("write");
        let assessment = assess_skill_script(&script).expect("assess");
        assert_eq!(assessment.risk_class, SkillScriptRiskClass::Network);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn sandbox_denied_without_openshell_enabled() {
        let discovery = SkillDiscoveryReport {
            skill_id: "demo".to_string(),
            assets_dir: "/tmp/demo".to_string(),
            scripts: vec![SkillScriptEntry {
                relative_path: "scripts/echo.sh".to_string(),
                risk_class: SkillScriptRiskClass::ReadOnly,
                risk_score: 10,
                rationale: "ok".to_string(),
            }],
            max_risk_class: SkillScriptRiskClass::ReadOnly,
            max_risk_score: 10,
            has_scripts: true,
        };
        let manuscript = ManuscriptContext {
            id: "demo".to_string(),
            name: "Demo".to_string(),
            description: None,
            display_name: None,
            voice_appendix: None,
            system_appendix: None,
            task_template: None,
            pinned_preferences: Vec::new(),
            pinned_contact_ids: Vec::new(),
            recall_hints: Vec::new(),
            worker_intent: None,
            worker_stage_role: None,
            worker_model_hint: None,
            max_tool_rounds: None,
            tools_allow: Vec::new(),
            locus_session_id: None,
            delivery_mode: None,
            delivery_on_complete: None,
            schedule_cron: None,
            schedule_execution_mode: None,
            openshell_enabled: false,
            openshell_policy_template: None,
            openshell_sandbox_from: None,
            openshell_allow_scheduled: false,
            extends_from: None,
            source_path: PathBuf::from("demo.yaml"),
        };
        let proposal = evaluate_skill_adoption(
            &discovery,
            Some(&manuscript),
            SkillSecurityLevel::Sandbox,
            Some("scripts/echo.sh"),
        );
        assert_eq!(proposal.granted_level, SkillSecurityLevel::Propose);
        assert!(
            proposal
                .approval_reasons
                .iter()
                .any(|reason| reason.contains("openshell.enabled"))
        );
    }
}
