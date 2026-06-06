//! YAML identity manuscripts — declarative specialty packs for turns and workers.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::agent_runtime::turn_worker::TurnWorkerIntent;
use crate::cognitive_identity::DigestCompileOptions;
use crate::openshell_sandbox_run::resolve_policy_template_path;
use crate::openshell_tools::is_openshell_cognition_tool;

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
    #[serde(default)]
    pub extends: Option<String>,
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
    #[serde(default)]
    pub openshell: ManuscriptOpenshellSpec,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ManuscriptPersonaSpec {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub voice_appendix: Option<String>,
    #[serde(default)]
    pub soul_md: Option<String>,
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ManuscriptOpenshellSpec {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub policy_template: Option<String>,
    #[serde(default)]
    pub sandbox_from: Option<String>,
    #[serde(default)]
    pub allow_scheduled: bool,
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
    pub openshell_enabled: bool,
    pub openshell_policy_template: Option<String>,
    pub openshell_sandbox_from: Option<String>,
    pub openshell_allow_scheduled: bool,
    pub extends_from: Option<String>,
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
        if is_openshell_cognition_tool(tool) && !manuscript.openshell_allow_scheduled {
            continue;
        }
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
    if manuscript
        .tools_allow
        .iter()
        .any(|tool| is_openshell_cognition_tool(tool))
        && !manuscript.openshell_allow_scheduled
    {
        bail!(
            "openshell tools are denied on scheduled lane unless spec.openshell.allow_scheduled=true"
        );
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

pub fn load_manuscript_merged(id: &str) -> Result<(IdentityManuscriptFile, PathBuf)> {
    let path = resolve_manuscript_path(id)?;
    let file = load_manuscript_file(&path)?;
    let merged = merge_manuscript_inheritance(&file, &path)?;
    Ok((merged, path))
}

fn merge_manuscript_inheritance(
    file: &IdentityManuscriptFile,
    _path: &Path,
) -> Result<IdentityManuscriptFile> {
    let Some(base_id) = file
        .metadata
        .extends
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(file.clone());
    };

    if base_id == file.metadata.id {
        bail!("manuscript cannot extend itself");
    }

    let (base_file, base_path) = load_manuscript(base_id)
        .with_context(|| format!("resolve base manuscript '{base_id}' for extends"))?;
    validate_extends_chain(base_id, &base_path)?;

    let merged_base = merge_manuscript_inheritance(&base_file, &base_path)?;
    Ok(merge_manuscript_layers(merged_base, file.clone()))
}

fn validate_extends_chain(base_id: &str, base_path: &Path) -> Result<()> {
    let mut visited = HashSet::from([base_path.to_path_buf()]);
    let mut current = load_manuscript_file(base_path)?;
    while let Some(parent_id) = current
        .metadata
        .extends
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if parent_id == base_id {
            bail!("manuscript extends cycle detected at '{parent_id}'");
        }
        let parent_path = resolve_manuscript_path(parent_id)?;
        if !visited.insert(parent_path.clone()) {
            bail!("manuscript extends cycle detected at '{parent_id}'");
        }
        current = load_manuscript_file(&parent_path)?;
    }
    Ok(())
}

fn merge_manuscript_layers(
    base: IdentityManuscriptFile,
    child: IdentityManuscriptFile,
) -> IdentityManuscriptFile {
    IdentityManuscriptFile {
        api_version: child.api_version,
        kind: child.kind,
        metadata: child.metadata,
        spec: ManuscriptSpec {
            persona: merge_persona_spec(&base.spec.persona, &child.spec.persona),
            prompts: merge_prompts_spec(&base.spec.prompts, &child.spec.prompts),
            identity: merge_identity_spec(&base.spec.identity, &child.spec.identity),
            worker: merge_worker_spec(&base.spec.worker, &child.spec.worker),
            tools: merge_tools_spec(&base.spec.tools, &child.spec.tools),
            locus: merge_locus_spec(&base.spec.locus, &child.spec.locus),
            delivery: merge_delivery_spec(&base.spec.delivery, &child.spec.delivery),
            schedule: merge_schedule_spec(&base.spec.schedule, &child.spec.schedule),
            openshell: merge_openshell_spec(&base.spec.openshell, &child.spec.openshell),
        },
    }
}

fn merge_persona_spec(
    base: &ManuscriptPersonaSpec,
    child: &ManuscriptPersonaSpec,
) -> ManuscriptPersonaSpec {
    ManuscriptPersonaSpec {
        display_name: child
            .display_name
            .clone()
            .or_else(|| base.display_name.clone()),
        voice_appendix: child.voice_appendix.clone().or_else(|| base.voice_appendix.clone()),
        soul_md: child.soul_md.clone().or_else(|| base.soul_md.clone()),
    }
}

fn merge_prompts_spec(
    base: &ManuscriptPromptsSpec,
    child: &ManuscriptPromptsSpec,
) -> ManuscriptPromptsSpec {
    ManuscriptPromptsSpec {
        system_appendix_sttp: child
            .system_appendix_sttp
            .clone()
            .or_else(|| base.system_appendix_sttp.clone()),
        task_template: child.task_template.clone().or_else(|| base.task_template.clone()),
    }
}

fn merge_identity_spec(
    base: &ManuscriptIdentitySpec,
    child: &ManuscriptIdentitySpec,
) -> ManuscriptIdentitySpec {
    ManuscriptIdentitySpec {
        pins: ManuscriptIdentityPins {
            preferences: merge_string_lists(&base.pins.preferences, &child.pins.preferences),
            contacts: merge_string_lists(&base.pins.contacts, &child.pins.contacts),
        },
        recall_hints: merge_string_lists(&base.recall_hints, &child.recall_hints),
    }
}

fn merge_worker_spec(base: &ManuscriptWorkerSpec, child: &ManuscriptWorkerSpec) -> ManuscriptWorkerSpec {
    ManuscriptWorkerSpec {
        intent: child.intent.clone().or_else(|| base.intent.clone()),
        max_tool_rounds: child.max_tool_rounds.or(base.max_tool_rounds),
        override_sttp: child.override_sttp || base.override_sttp,
    }
}

fn merge_tools_spec(base: &ManuscriptToolsSpec, child: &ManuscriptToolsSpec) -> ManuscriptToolsSpec {
    ManuscriptToolsSpec {
        allow: merge_string_lists(&base.allow, &child.allow),
    }
}

fn merge_locus_spec(base: &ManuscriptLocusSpec, child: &ManuscriptLocusSpec) -> ManuscriptLocusSpec {
    ManuscriptLocusSpec {
        session_id: child
            .session_id
            .clone()
            .or_else(|| base.session_id.clone()),
    }
}

fn merge_delivery_spec(
    base: &ManuscriptDeliverySpec,
    child: &ManuscriptDeliverySpec,
) -> ManuscriptDeliverySpec {
    ManuscriptDeliverySpec {
        mode: child.mode.clone().or_else(|| base.mode.clone()),
        on_complete: child.on_complete.clone().or_else(|| base.on_complete.clone()),
    }
}

fn merge_schedule_spec(
    base: &ManuscriptScheduleSpec,
    child: &ManuscriptScheduleSpec,
) -> ManuscriptScheduleSpec {
    ManuscriptScheduleSpec {
        cron: child.cron.clone().or_else(|| base.cron.clone()),
        execution_mode: child
            .execution_mode
            .clone()
            .or_else(|| base.execution_mode.clone()),
    }
}

fn merge_openshell_spec(
    base: &ManuscriptOpenshellSpec,
    child: &ManuscriptOpenshellSpec,
) -> ManuscriptOpenshellSpec {
    ManuscriptOpenshellSpec {
        enabled: child.enabled || base.enabled,
        policy_template: child
            .policy_template
            .clone()
            .or_else(|| base.policy_template.clone()),
        sandbox_from: child
            .sandbox_from
            .clone()
            .or_else(|| base.sandbox_from.clone()),
        allow_scheduled: child.allow_scheduled || base.allow_scheduled,
    }
}

fn merge_string_lists(base: &[String], child: &[String]) -> Vec<String> {
    let mut merged = base.to_vec();
    for value in child {
        if !merged.iter().any(|existing| existing == value) {
            merged.push(value.clone());
        }
    }
    merged
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

    if let Some(base_id) = file.metadata.extends.as_deref().map(str::trim).filter(|v| !v.is_empty())
    {
        if base_id == file.metadata.id {
            bail!("metadata.extends cannot reference the manuscript's own id");
        }
        resolve_manuscript_path(base_id).with_context(|| {
            format!("metadata.extends base manuscript '{base_id}' not found")
        })?;
    }

    validate_openshell_spec(&file.spec.openshell, &file.spec.tools.allow)?;

    Ok(())
}

fn validate_openshell_spec(openshell: &ManuscriptOpenshellSpec, tools_allow: &[String]) -> Result<()> {
    let openshell_tools: Vec<_> = tools_allow
        .iter()
        .filter(|tool| is_openshell_cognition_tool(tool))
        .collect();

    if openshell.enabled {
        let template = openshell
            .policy_template
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                anyhow::Error::msg(
                    "spec.openshell.policy_template is required when spec.openshell.enabled=true",
                )
            })?;
        if resolve_policy_template_path(template).is_none() {
            bail!(
                "spec.openshell.policy_template '{template}' not found under ~/.config/medousa/openshell-policies/"
            );
        }
        if openshell_tools.is_empty() {
            bail!(
                "spec.openshell.enabled=true requires at least one cognition_openshell_* tool in spec.tools.allow"
            );
        }
    }

    if !openshell_tools.is_empty() && !openshell.enabled {
        bail!(
            "spec.tools.allow lists openshell tools but spec.openshell.enabled is not true"
        );
    }

    Ok(())
}

pub fn build_manuscript_context(id: &str) -> Result<ManuscriptContext> {
    let (file, path) = load_manuscript_merged(id)?;
    validate_manuscript(&file, &path)?;
    build_manuscript_context_from_file(&file, &path)
}

pub fn build_manuscript_context_from_file(
    file: &IdentityManuscriptFile,
    path: &Path,
) -> Result<ManuscriptContext> {
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
    let voice_appendix = resolve_voice_appendix(&base_dir, &file.spec.persona)?;

    Ok(ManuscriptContext {
        id: file.metadata.id.clone(),
        name: file.metadata.name.clone(),
        description: file.metadata.description.clone(),
        display_name: file.spec.persona.display_name.clone(),
        voice_appendix,
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
        openshell_enabled: file.spec.openshell.enabled,
        openshell_policy_template: file.spec.openshell.policy_template.clone(),
        openshell_sandbox_from: file.spec.openshell.sandbox_from.clone(),
        openshell_allow_scheduled: file.spec.openshell.allow_scheduled,
        extends_from: file
            .metadata
            .extends
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        source_path: path.to_path_buf(),
    })
}

pub fn manuscript_catalog_entry(manuscript: &ManuscriptContext) -> serde_json::Value {
    serde_json::json!({
        "id": manuscript.id,
        "name": manuscript.name,
        "description": manuscript.description,
        "extends_from": manuscript.extends_from,
        "display_name": manuscript.display_name,
        "worker_intent": manuscript.worker_intent,
        "max_tool_rounds": manuscript.max_tool_rounds,
        "tools_allow": manuscript.tools_allow,
        "pinned_preferences": manuscript.pinned_preferences,
        "pinned_contacts": manuscript.pinned_contact_ids,
        "recall_hints": manuscript.recall_hints,
        "locus_session_id": manuscript.locus_session_id,
        "delivery_mode": manuscript.delivery_mode,
        "delivery_on_complete": manuscript.delivery_on_complete,
        "schedule_cron": manuscript.schedule_cron,
        "schedule_execution_mode": manuscript.schedule_execution_mode,
        "openshell": {
            "enabled": manuscript.openshell_enabled,
            "policy_template": manuscript.openshell_policy_template,
            "sandbox_from": manuscript.openshell_sandbox_from,
            "allow_scheduled": manuscript.openshell_allow_scheduled,
        },
        "source_path": manuscript.source_path.display().to_string(),
    })
}

pub fn install_manuscript(source: &Path, scope: ManuscriptScope) -> Result<PathBuf> {
    let file = load_manuscript_file(source)?;
    validate_manuscript(&file, source)?;

    let target_dir = match scope {
        ManuscriptScope::Project => project_manuscripts_dir(),
        ManuscriptScope::User => user_manuscripts_dir(),
    };
    std::fs::create_dir_all(&target_dir)
        .with_context(|| format!("create manuscript dir {}", target_dir.display()))?;

    let stem = file.metadata.id.trim();
    let target = target_dir.join(format!("{stem}.yaml"));
    if target.exists() {
        bail!(
            "manuscript '{}' already installed at {}",
            stem,
            target.display()
        );
    }

    let bytes = std::fs::read(source)
        .with_context(|| format!("read manuscript source {}", source.display()))?;
    std::fs::write(&target, bytes)
        .with_context(|| format!("write manuscript {}", target.display()))?;
    Ok(target)
}

fn resolve_voice_appendix(base_dir: &Path, persona: &ManuscriptPersonaSpec) -> Result<Option<String>> {
    let inline = persona
        .voice_appendix
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string());
    let soul = persona
        .soul_md
        .as_deref()
        .map(|raw| resolve_prompt_field(base_dir, raw))
        .transpose()?
        .filter(|value| !value.trim().is_empty());

    match (inline, soul) {
        (Some(mut voice), Some(soul_text)) => {
            voice.push_str("\n\n");
            voice.push_str(soul_text.trim());
            Ok(Some(voice))
        }
        (Some(voice), None) => Ok(Some(voice)),
        (None, Some(soul_text)) => Ok(Some(soul_text.trim().to_string())),
        (None, None) => Ok(None),
    }
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
    if manuscript.openshell_enabled {
        lines.push("openshell=enabled".to_string());
        if let Some(template) = manuscript
            .openshell_policy_template
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            lines.push(format!("openshell_policy_template={template}"));
        }
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

    #[test]
    fn extends_merges_tools_and_identity_pins() {
        let base = IdentityManuscriptFile {
            api_version: MANUSCRIPT_API_VERSION.to_string(),
            kind: MANUSCRIPT_KIND.to_string(),
            metadata: ManuscriptMetadata {
                id: "base-researcher".to_string(),
                name: "Base Researcher".to_string(),
                description: None,
                extends: None,
            },
            spec: ManuscriptSpec {
                tools: ManuscriptToolsSpec {
                    allow: vec![
                        "cognition_memory_context".to_string(),
                        "cognition_identity_recall".to_string(),
                    ],
                },
                identity: ManuscriptIdentitySpec {
                    pins: ManuscriptIdentityPins {
                        preferences: vec!["timezone".to_string()],
                        contacts: Vec::new(),
                    },
                    recall_hints: vec!["research".to_string()],
                },
                ..Default::default()
            },
        };
        let child = IdentityManuscriptFile {
            api_version: MANUSCRIPT_API_VERSION.to_string(),
            kind: MANUSCRIPT_KIND.to_string(),
            metadata: ManuscriptMetadata {
                id: "morning-brief".to_string(),
                name: "Morning Brief".to_string(),
                description: None,
                extends: Some("base-researcher".to_string()),
            },
            spec: ManuscriptSpec {
                tools: ManuscriptToolsSpec {
                    allow: vec!["cognition_capability_invoke".to_string()],
                },
                identity: ManuscriptIdentitySpec {
                    pins: ManuscriptIdentityPins {
                        preferences: vec!["beverage".to_string()],
                        contacts: Vec::new(),
                    },
                    recall_hints: vec!["priorities".to_string()],
                },
                ..Default::default()
            },
        };

        let merged = merge_manuscript_layers(base, child);
        assert_eq!(
            merged.spec.tools.allow,
            vec![
                "cognition_memory_context".to_string(),
                "cognition_identity_recall".to_string(),
                "cognition_capability_invoke".to_string(),
            ]
        );
        assert_eq!(
            merged.spec.identity.pins.preferences,
            vec!["timezone".to_string(), "beverage".to_string()]
        );
        assert_eq!(
            merged.spec.identity.recall_hints,
            vec!["research".to_string(), "priorities".to_string()]
        );
    }

    #[test]
    fn soul_md_file_appends_to_voice_appendix() {
        let dir = std::env::temp_dir().join(format!(
            "medousa-manuscript-soul-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("dir");
        fs::write(dir.join("brief-soul.md"), "Long soul prose for brief.").expect("write soul");
        let path = dir.join("brief.yaml");
        fs::write(
            &path,
            r#"apiVersion: medousa.dev/v1
kind: IdentityManuscript
metadata:
  id: brief
  name: Brief
spec:
  persona:
    voice_appendix: Short voice.
    soul_md: ./brief-soul.md
"#,
        )
        .expect("write yaml");

        let file = load_manuscript_file(&path).expect("load");
        let context = build_manuscript_context_from_file(&file, &path).expect("context");
        assert!(context
            .voice_appendix
            .as_ref()
            .is_some_and(|voice| voice.contains("Short voice.") && voice.contains("Long soul prose")));
        let _ = fs::remove_dir_all(&dir);
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
            openshell_enabled: false,
            openshell_policy_template: None,
            openshell_sandbox_from: None,
            openshell_allow_scheduled: false,
            extends_from: None,
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
            openshell_enabled: false,
            openshell_policy_template: None,
            openshell_sandbox_from: None,
            openshell_allow_scheduled: false,
            extends_from: None,
            source_path: PathBuf::from("empty.yaml"),
        };
        assert!(validate_manuscript_for_scheduled_lane(&manuscript).is_err());
    }

    #[test]
    fn scheduled_lane_rejects_openshell_tools_by_default() {
        let manuscript = ManuscriptContext {
            id: "openshell-brief".to_string(),
            name: "OpenShell Brief".to_string(),
            description: None,
            display_name: None,
            voice_appendix: None,
            system_appendix: None,
            task_template: Some("Run sandbox task.".to_string()),
            pinned_preferences: Vec::new(),
            pinned_contact_ids: Vec::new(),
            recall_hints: Vec::new(),
            worker_intent: Some("research".to_string()),
            max_tool_rounds: None,
            tools_allow: vec![
                "cognition_identity_recall".to_string(),
                "cognition_openshell_sandbox_run".to_string(),
            ],
            locus_session_id: None,
            delivery_mode: None,
            delivery_on_complete: None,
            schedule_cron: None,
            schedule_execution_mode: None,
            openshell_enabled: true,
            openshell_policy_template: Some("research-readonly".to_string()),
            openshell_sandbox_from: Some("base".to_string()),
            openshell_allow_scheduled: false,
            extends_from: None,
            source_path: PathBuf::from("openshell-brief.yaml"),
        };
        assert!(validate_manuscript_for_scheduled_lane(&manuscript).is_err());
    }
}
