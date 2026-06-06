//! YAML identity manuscripts — declarative specialty packs for turns and workers.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::agent_runtime::turn_worker::TurnWorkerIntent;
use crate::cognitive_identity::DigestCompileOptions;

pub const MANUSCRIPT_API_VERSION: &str = "medousa.dev/v1";
pub const MANUSCRIPT_KIND: &str = "IdentityManuscript";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityManuscriptFile {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    pub metadata: ManuscriptMetadata,
    pub spec: ManuscriptSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManuscriptMetadata {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ManuscriptSpec {
    #[serde(default)]
    pub persona: ManuscriptPersonaSpec,
    #[serde(default)]
    pub prompts: ManuscriptPromptsSpec,
    #[serde(default)]
    pub identity: ManuscriptIdentitySpec,
    #[serde(default)]
    pub worker: ManuscriptWorkerSpec,
    #[serde(default)]
    pub tools: ManuscriptToolsSpec,
    #[serde(default)]
    pub locus: ManuscriptLocusSpec,
    #[serde(default)]
    pub delivery: ManuscriptDeliverySpec,
    #[serde(default)]
    pub schedule: ManuscriptScheduleSpec,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ManuscriptPersonaSpec {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub voice_appendix: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ManuscriptPromptsSpec {
    #[serde(default, rename = "system_appendix_sttp")]
    pub system_appendix_sttp: Option<String>,
    #[serde(default)]
    pub task_template: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ManuscriptIdentitySpec {
    #[serde(default)]
    pub pins: ManuscriptIdentityPins,
    #[serde(default)]
    pub recall_hints: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ManuscriptIdentityPins {
    #[serde(default)]
    pub preferences: Vec<String>,
    #[serde(default)]
    pub contacts: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ManuscriptWorkerSpec {
    #[serde(default)]
    pub intent: Option<String>,
    #[serde(default)]
    pub max_tool_rounds: Option<usize>,
    #[serde(default)]
    pub override_sttp: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ManuscriptToolsSpec {
    #[serde(default)]
    pub allow: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ManuscriptLocusSpec {
    #[serde(default)]
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ManuscriptDeliverySpec {
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub on_complete: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ManuscriptScheduleSpec {
    #[serde(default)]
    pub cron: Option<String>,
    #[serde(default)]
    pub execution_mode: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ManuscriptContext {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub display_name: Option<String>,
    pub voice_appendix: Option<String>,
    pub system_appendix: Option<String>,
    pub task_template: Option<String>,
    pub pinned_preferences: Vec<String>,
    pub pinned_contact_ids: Vec<String>,
    pub recall_hints: Vec<String>,
    pub worker_intent: Option<String>,
    pub max_tool_rounds: Option<usize>,
    pub tools_allow: Vec<String>,
    pub locus_session_id: Option<String>,
    pub delivery_mode: Option<String>,
    pub delivery_on_complete: Option<String>,
    pub schedule_cron: Option<String>,
    pub schedule_execution_mode: Option<String>,
    pub source_path: PathBuf,
}

const SCHEDULED_ESSENTIAL_TOOLS: &[&str] = &[
    "cognition_turn_prepare_final",
    "cognition.turn.prepare_final",
    "cognition_utility_time_now",
    "cognition_utility_day_of_week",
    "cognition_utility_uuid",
];

pub fn scheduled_lane_tool_universe() -> HashSet<String> {
    use crate::agent_runtime::turn_worker::{TurnWorkerIntent, allowed_tool_names_for_intent};

    let mut universe = allowed_tool_names_for_intent(TurnWorkerIntent::Research);
    universe.extend(allowed_tool_names_for_intent(TurnWorkerIntent::MemoryContext));
    universe.remove("cognition_identity_remember");
    universe.remove("cognition_spawn_turn_worker");
    universe
}

pub fn scheduled_tool_allowlist_for_manuscript(manuscript: &ManuscriptContext) -> HashSet<String> {
    use crate::agent_runtime::turn_worker::tool_allowed;

    let universe = scheduled_lane_tool_universe();
    let mut allow = HashSet::new();
    for tool in &manuscript.tools_allow {
        if tool_allowed(tool, &universe) {
            allow.insert(tool.to_string());
        }
    }
    for essential in SCHEDULED_ESSENTIAL_TOOLS {
        if universe.contains(*essential) {
            allow.insert((*essential).to_string());
        }
    }
    allow
}

pub fn validate_manuscript_for_scheduled_lane(manuscript: &ManuscriptContext) -> Result<()> {
    if manuscript.tools_allow.is_empty() {
        bail!("scheduled lane requires spec.tools.allow to be non-empty");
    }
    if manuscript
        .tools_allow
        .iter()
        .any(|tool| tool.contains("identity_remember"))
    {
        bail!("cognition_identity_remember is not allowed on scheduled manuscript lane");
    }

    let allow = scheduled_tool_allowlist_for_manuscript(manuscript);
    let manuscript_tools: HashSet<_> = manuscript.tools_allow.iter().cloned().collect();
    if allow.intersection(&manuscript_tools).count() == 0 {
        bail!("spec.tools.allow has no tools permitted on scheduled lane");
    }
    Ok(())
}

pub fn render_manuscript_task_prompt(
    manuscript: &ManuscriptContext,
    override_prompt: Option<&str>,
) -> Result<String> {
    if let Some(prompt) = override_prompt.map(str::trim).filter(|value| !value.is_empty()) {
        return Ok(prompt.to_string());
    }
    manuscript
        .task_template
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "task prompt required: provide prompt or spec.prompts.task_template for manuscript '{}'",
                manuscript.id
            )
        })
}

pub fn manuscript_wants_locus_store_on_complete(manuscript: &ManuscriptContext) -> bool {
    manuscript.locus_session_id.is_some()
        && manuscript
            .delivery_on_complete
            .as_deref()
            .map(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "locus" | "store" | "locus_store"
                )
            })
            .unwrap_or(false)
}

#[derive(Debug, Clone)]
pub struct ManuscriptListing {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub path: PathBuf,
    pub scope: ManuscriptScope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManuscriptScope {
    Project,
    User,
}

pub fn project_manuscripts_dir() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".medousa")
        .join("manuscripts")
}

pub fn user_manuscripts_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("medousa")
        .join("manuscripts")
}

pub fn manuscript_search_dirs() -> Vec<PathBuf> {
    vec![project_manuscripts_dir(), user_manuscripts_dir()]
}

pub fn resolve_manuscript_path(id: &str) -> Result<PathBuf> {
    let stem = id.trim();
    if stem.is_empty() {
        bail!("manuscript id is required");
    }

    for dir in manuscript_search_dirs() {
        for ext in ["yaml", "yml"] {
            let candidate = dir.join(format!("{stem}.{ext}"));
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }

    bail!("manuscript '{stem}' not found in project or user manuscript dirs")
}

pub fn load_manuscript_file(path: &Path) -> Result<IdentityManuscriptFile> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("read manuscript {}", path.display()))?;
    let parsed: IdentityManuscriptFile = serde_yaml::from_str(&raw)
        .with_context(|| format!("parse manuscript yaml {}", path.display()))?;
    Ok(parsed)
}

pub fn load_manuscript(id: &str) -> Result<(IdentityManuscriptFile, PathBuf)> {
    let path = resolve_manuscript_path(id)?;
    let file = load_manuscript_file(&path)?;
    Ok((file, path))
}

pub fn validate_manuscript(file: &IdentityManuscriptFile, path: &Path) -> Result<()> {
    if file.api_version != MANUSCRIPT_API_VERSION {
        bail!(
            "unsupported apiVersion '{}' (expected {MANUSCRIPT_API_VERSION})",
            file.api_version
        );
    }
    if file.kind != MANUSCRIPT_KIND {
        bail!(
            "unsupported kind '{}' (expected {MANUSCRIPT_KIND})",
            file.kind
        );
    }
    if file.metadata.id.trim().is_empty() {
        bail!("metadata.id is required");
    }
    if file.metadata.name.trim().is_empty() {
        bail!("metadata.name is required");
    }

    if let Some(stem) = path.file_stem().and_then(|value| value.to_str()) {
        if stem != file.metadata.id {
            bail!(
                "metadata.id '{}' must match filename stem '{}'",
                file.metadata.id,
                stem
            );
        }
    }

    if let Some(intent) = file.spec.worker.intent.as_deref() {
        if TurnWorkerIntent::parse(intent).is_none() {
            bail!(
                "spec.worker.intent '{intent}' is invalid (expected research|general|memory.context|memory.avec_calibrate)"
            );
        }
    }

    if let Some(mode) = file.spec.delivery.mode.as_deref() {
        match mode.trim().to_ascii_lowercase().as_str() {
            "telegram" | "webhook" | "linked_channel" | "store_only" => {}
            other => bail!("spec.delivery.mode '{other}' is unsupported"),
        }
    }

    Ok(())
}

pub fn build_manuscript_context(id: &str) -> Result<ManuscriptContext> {
    let (file, path) = load_manuscript(id)?;
    validate_manuscript(&file, &path)?;
    let base_dir = path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(user_manuscripts_dir);

    let system_appendix = file
        .spec
        .prompts
        .system_appendix_sttp
        .as_deref()
        .map(|raw| resolve_prompt_field(&base_dir, raw))
        .transpose()?;

    Ok(ManuscriptContext {
        id: file.metadata.id.clone(),
        name: file.metadata.name.clone(),
        description: file.metadata.description.clone(),
        display_name: file.spec.persona.display_name.clone(),
        voice_appendix: file.spec.persona.voice_appendix.clone(),
        system_appendix,
        task_template: file.spec.prompts.task_template.clone(),
        pinned_preferences: file.spec.identity.pins.preferences.clone(),
        pinned_contact_ids: file.spec.identity.pins.contacts.clone(),
        recall_hints: file.spec.identity.recall_hints.clone(),
        worker_intent: file.spec.worker.intent.clone(),
        max_tool_rounds: file.spec.worker.max_tool_rounds,
        tools_allow: file.spec.tools.allow.clone(),
        locus_session_id: file.spec.locus.session_id.clone(),
        delivery_mode: file.spec.delivery.mode.clone(),
        delivery_on_complete: file.spec.delivery.on_complete.clone(),
        schedule_cron: file.spec.schedule.cron.clone(),
        schedule_execution_mode: file.spec.schedule.execution_mode.clone(),
        source_path: path,
    })
}

pub fn list_manuscripts() -> Result<Vec<ManuscriptListing>> {
    let mut by_id: std::collections::BTreeMap<String, ManuscriptListing> =
        std::collections::BTreeMap::new();

    for (scope, dir) in [
        (ManuscriptScope::User, user_manuscripts_dir()),
        (ManuscriptScope::Project, project_manuscripts_dir()),
    ] {
        if !dir.is_dir() {
            continue;
        }
        for entry in std::fs::read_dir(&dir).with_context(|| format!("read {}", dir.display()))? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(ext) = path.extension().and_then(|value| value.to_str()) else {
                continue;
            };
            if ext != "yaml" && ext != "yml" {
                continue;
            }
            let file = match load_manuscript_file(&path) {
                Ok(file) => file,
                Err(_) => continue,
            };
            by_id.insert(
                file.metadata.id.clone(),
                ManuscriptListing {
                    id: file.metadata.id.clone(),
                    name: file.metadata.name.clone(),
                    description: file.metadata.description.clone(),
                    path,
                    scope,
                },
            );
        }
    }

    Ok(by_id.into_values().collect())
}

pub async fn compile_manuscript_identity_summary(
    store: &std::sync::Arc<dyn stasis::ports::outbound::memory::identity_memory_store::IdentityMemoryStore>,
    manuscript: &ManuscriptContext,
    query_hints: Option<&str>,
) -> String {
    use crate::cognitive_identity::{
        DigestCompileOptions, cognitive_identity_diagnostics_with_stats,
        compile_relational_memory_digest_with_options, load_cognitive_identity_snapshot,
        DEFAULT_RELATIONAL_DIGEST_BUDGET,
    };
    use crate::identity_memory::resolve_identity_user_id;

    let user_id = resolve_identity_user_id(None);
    let snapshot = load_cognitive_identity_snapshot(
        Some(store),
        &user_id,
        Some("interactive"),
        32,
    )
    .await;
    let mut options = DigestCompileOptions::from_product_config(DEFAULT_RELATIONAL_DIGEST_BUDGET)
        .with_query_hints(query_hints.unwrap_or_default());
    options = digest_options_for_manuscript(options, manuscript);
    let ranked = compile_relational_memory_digest_with_options(&snapshot, options);
    let diagnostics =
        cognitive_identity_diagnostics_with_stats(&snapshot, Some(&ranked.stats));
    format!("{}\n{diagnostics}", ranked.text)
}

pub fn digest_options_for_manuscript(
    mut base: DigestCompileOptions,
    manuscript: &ManuscriptContext,
) -> DigestCompileOptions {
    for pin in &manuscript.pinned_preferences {
        if !base.pinned_preferences.iter().any(|existing| existing == pin) {
            base.pinned_preferences.push(pin.clone());
        }
    }
    for pin in &manuscript.pinned_contact_ids {
        if !base.pinned_contact_ids.iter().any(|existing| existing == pin) {
            base.pinned_contact_ids.push(pin.clone());
        }
    }
    if !manuscript.recall_hints.is_empty() {
        let hints = manuscript.recall_hints.join(" ");
        base.query_hints = Some(match base.query_hints {
            Some(existing) if !existing.trim().is_empty() => format!("{existing} {hints}"),
            _ => hints,
        });
    }
    base
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WorkerManuscriptHandoff {
    pub id: String,
    pub name: String,
    pub voice_appendix: Option<String>,
    pub system_appendix: Option<String>,
    pub tools_allow: Vec<String>,
}

impl From<&ManuscriptContext> for WorkerManuscriptHandoff {
    fn from(manuscript: &ManuscriptContext) -> Self {
        Self {
            id: manuscript.id.clone(),
            name: manuscript.name.clone(),
            voice_appendix: manuscript.voice_appendix.clone(),
            system_appendix: manuscript.system_appendix.clone(),
            tools_allow: manuscript.tools_allow.clone(),
        }
    }
}

pub fn format_worker_manuscript_block(manuscript: &WorkerManuscriptHandoff) -> String {
    let mut lines = vec![
        "[MEDOUSA_MANUSCRIPT]".to_string(),
        format!("id={}", manuscript.id),
        format!("name={}", manuscript.name),
    ];
    if let Some(voice) = manuscript.voice_appendix.as_deref().filter(|v| !v.is_empty()) {
        lines.push("voice_appendix:".to_string());
        lines.push(voice.trim().to_string());
    }
    if let Some(appendix) = manuscript.system_appendix.as_deref().filter(|v| !v.is_empty()) {
        lines.push("system_appendix:".to_string());
        lines.push(appendix.trim().to_string());
    }
    lines.join("\n")
}

pub fn format_manuscript_prompt_block(manuscript: &ManuscriptContext) -> String {
    let mut lines = vec![
        "[MEDOUSA_MANUSCRIPT]".to_string(),
        format!("id={}", manuscript.id),
        format!("name={}", manuscript.name),
    ];
    if let Some(description) = manuscript.description.as_deref().filter(|v| !v.is_empty()) {
        lines.push(format!("description={description}"));
    }
    if let Some(display_name) = manuscript.display_name.as_deref().filter(|v| !v.is_empty()) {
        lines.push(format!("display_name={display_name}"));
    }
    if let Some(voice) = manuscript.voice_appendix.as_deref().filter(|v| !v.is_empty()) {
        lines.push("voice_appendix:".to_string());
        lines.push(voice.trim().to_string());
    }
    if let Some(appendix) = manuscript.system_appendix.as_deref().filter(|v| !v.is_empty()) {
        lines.push("system_appendix:".to_string());
        lines.push(appendix.trim().to_string());
    }
    if let Some(task) = manuscript.task_template.as_deref().filter(|v| !v.is_empty()) {
        lines.push("task_template:".to_string());
        lines.push(task.trim().to_string());
    }
    lines.join("\n")
}

fn resolve_prompt_field(base_dir: &Path, raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }

    let candidate = base_dir.join(trimmed);
    if candidate.is_file() {
        return Ok(std::fs::read_to_string(&candidate)
            .with_context(|| format!("read manuscript prompt file {}", candidate.display()))?);
    }

    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_sample_manuscript(dir: &Path) -> PathBuf {
        fs::create_dir_all(dir).expect("dir");
        let path = dir.join("morning-brief.yaml");
        fs::write(
            &path,
            r#"apiVersion: medousa.dev/v1
kind: IdentityManuscript
metadata:
  id: morning-brief
  name: Morning Brief
  description: Daily operator summary
spec:
  persona:
    display_name: Medousa — Morning Brief
    voice_appendix: |
      Concise, proactive chief-of-staff.
  prompts:
    system_appendix_sttp: |
      Lead with what changed overnight.
    task_template: |
      Produce today's brief.
  identity:
    pins:
      preferences: [timezone, beverage]
      contacts: [contact:mario]
    recall_hints: [priorities, team]
  worker:
    intent: research
    max_tool_rounds: 8
"#,
        )
        .expect("write");
        path
    }

    #[test]
    fn validate_and_build_context_from_yaml() {
        let dir = std::env::temp_dir().join(format!(
            "medousa-manuscript-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        let path = write_sample_manuscript(&dir);

        let file = load_manuscript_file(&path).expect("load");
        validate_manuscript(&file, &path).expect("validate");
        let context = build_manuscript_context_from_file(&file, &path).expect("context");

        assert_eq!(context.id, "morning-brief");
        assert_eq!(context.pinned_preferences, vec!["timezone", "beverage"]);
        assert!(context.system_appendix.as_ref().is_some_and(|v| v.contains("overnight")));

        let options = digest_options_for_manuscript(
            DigestCompileOptions::from_product_config(800),
            &context,
        );
        assert!(options.pinned_preferences.contains(&"timezone".to_string()));
        assert!(options
            .query_hints
            .as_ref()
            .is_some_and(|hints| hints.contains("priorities")));

        let block = format_manuscript_prompt_block(&context);
        assert!(block.contains("morning-brief"));
        assert!(block.contains("chief-of-staff"));

        let _ = fs::remove_dir_all(&dir);
    }

    fn build_manuscript_context_from_file(
        file: &IdentityManuscriptFile,
        path: &Path,
    ) -> Result<ManuscriptContext> {
        validate_manuscript(file, path)?;
        let base_dir = path.parent().unwrap_or(path).to_path_buf();
        let system_appendix = file
            .spec
            .prompts
            .system_appendix_sttp
            .as_deref()
            .map(|raw| resolve_prompt_field(&base_dir, raw))
            .transpose()?;
        Ok(ManuscriptContext {
            id: file.metadata.id.clone(),
            name: file.metadata.name.clone(),
            description: file.metadata.description.clone(),
            display_name: file.spec.persona.display_name.clone(),
            voice_appendix: file.spec.persona.voice_appendix.clone(),
            system_appendix,
            task_template: file.spec.prompts.task_template.clone(),
            pinned_preferences: file.spec.identity.pins.preferences.clone(),
            pinned_contact_ids: file.spec.identity.pins.contacts.clone(),
            recall_hints: file.spec.identity.recall_hints.clone(),
            worker_intent: file.spec.worker.intent.clone(),
            max_tool_rounds: file.spec.worker.max_tool_rounds,
            tools_allow: file.spec.tools.allow.clone(),
            locus_session_id: file.spec.locus.session_id.clone(),
            delivery_mode: file.spec.delivery.mode.clone(),
            delivery_on_complete: file.spec.delivery.on_complete.clone(),
            schedule_cron: file.spec.schedule.cron.clone(),
            schedule_execution_mode: file.spec.schedule.execution_mode.clone(),
            source_path: path.to_path_buf(),
        })
    }

    #[test]
    fn scheduled_lane_requires_tools_allow_and_intersects_universe() {
        let manuscript = ManuscriptContext {
            id: "morning-brief".to_string(),
            name: "Morning Brief".to_string(),
            description: None,
            display_name: None,
            voice_appendix: None,
            system_appendix: None,
            task_template: Some("Produce today's brief.".to_string()),
            pinned_preferences: Vec::new(),
            pinned_contact_ids: Vec::new(),
            recall_hints: Vec::new(),
            worker_intent: Some("research".to_string()),
            max_tool_rounds: None,
            tools_allow: vec![
                "cognition_identity_recall".to_string(),
                "cognition_memory_context".to_string(),
                "cognition_spawn_turn_worker".to_string(),
            ],
            locus_session_id: None,
            delivery_mode: None,
            delivery_on_complete: None,
            schedule_cron: None,
            schedule_execution_mode: None,
            source_path: PathBuf::from("morning-brief.yaml"),
        };

        validate_manuscript_for_scheduled_lane(&manuscript).expect("valid allowlist");
        let allow = scheduled_tool_allowlist_for_manuscript(&manuscript);
        assert!(allow.contains("cognition_identity_recall"));
        assert!(allow.contains("cognition_memory_context"));
        assert!(!allow.contains("cognition_spawn_turn_worker"));
        assert!(!allow.contains("cognition_identity_remember"));
    }

    #[test]
    fn scheduled_lane_rejects_empty_tools_allow() {
        let manuscript = ManuscriptContext {
            id: "empty".to_string(),
            name: "Empty".to_string(),
            description: None,
            display_name: None,
            voice_appendix: None,
            system_appendix: None,
            task_template: None,
            pinned_preferences: Vec::new(),
            pinned_contact_ids: Vec::new(),
            recall_hints: Vec::new(),
            worker_intent: None,
            max_tool_rounds: None,
            tools_allow: Vec::new(),
            locus_session_id: None,
            delivery_mode: None,
            delivery_on_complete: None,
            schedule_cron: None,
            schedule_execution_mode: None,
            source_path: PathBuf::from("empty.yaml"),
        };
        assert!(validate_manuscript_for_scheduled_lane(&manuscript).is_err());
    }
}
