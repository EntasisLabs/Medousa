//! Phase 9 — progressive tool surface: bootstrap ring, session unlocks, turn hints.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::agent_runtime::prompt_prep::truncate_text_for_budget;
use crate::agent_runtime::turn_worker::{host_bus_tool_names, tool_allowed};
use crate::session::{self, ConversationTurn};
use crate::turn_slice::{tool_history_summary_rows, DEFAULT_TOOL_HISTORY_SUMMARY_TURNS};

pub const COGNITION_TOOLS_DISCOVER: &str = "cognition_tools_discover";

pub const DEFAULT_TOOL_HINTS_BLOCK_CHARS: usize = 700;

/// Always-visible host console tools (~12+).
pub const HOST_BOOTSTRAP_TOOLS: &[&str] = &[
    COGNITION_TOOLS_DISCOVER,
    "cognition_capability_search",
    "cognition_tool_history_summary",
    "cognition_spawn_turn_worker",
    "cognition_memory_context",
    "cognition_memory_store",
    "cognition_identity_recall",
    "cognition_identity_remember",
    "cognition_web_search",
    "cognition_vault_search",
    "cognition_turn_begin_work",
    "cognition_turn_checkpoint",
    "cognition_turn_finish",
    "cognition_turn_worker_status",
];

/// Always-visible worker workshop tools.
pub const WORKER_BOOTSTRAP_TOOLS: &[&str] = &[
    COGNITION_TOOLS_DISCOVER,
    "cognition_turn_begin_work",
    "cognition_turn_finish",
    "cognition_capability_invoke",
    "cognition_web_search",
    "cognition_grapheme_script_load",
    "cognition_grapheme_template_run",
    "cognition_memory_context",
    "cognition_memory_store",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolSurfaceLane {
    Host,
    Worker,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionToolSurface {
    pub session_id: String,
    #[serde(default)]
    pub unlocked_domains: Vec<String>,
    #[serde(default)]
    pub discover_events: Vec<ToolDiscoverEvent>,
    pub updated_at_utc: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDiscoverEvent {
    pub domain: String,
    pub lane: String,
    pub tool_count: usize,
    pub timestamp_utc: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct ToolDomainCatalogEntry {
    pub domain: &'static str,
    pub summary: &'static str,
    pub tools: &'static [&'static str],
}

pub fn host_tool_domain_catalog() -> &'static [ToolDomainCatalogEntry] {
    static HOST: OnceLock<Vec<ToolDomainCatalogEntry>> = OnceLock::new();
    HOST.get_or_init(|| {
        vec![
            ToolDomainCatalogEntry {
                domain: "memory",
                summary: "Locus session memory — schema, calibrate, moods, store, recall",
                tools: &[
                    "cognition_memory_schema",
                    "cognition_memory_moods",
                    "cognition_memory_calibrate",
                    "cognition_memory_list",
                    "cognition_memory_recall",
                    "cognition_memory_store",
                    "cognition_identity_recall",
                    "cognition_identity_remember",
                ],
            },
            ToolDomainCatalogEntry {
                domain: "catalog",
                summary: "Inspect capabilities, manuscripts, saved Grapheme scripts",
                tools: &[
                    "cognition_capability_list",
                    "cognition_capability_resolve",
                    "cognition_manuscript_list",
                    "cognition_manuscript_resolve",
                    "cognition_grapheme_script_list",
                    "cognition_grapheme_script_search",
                    "cognition_grapheme_script_load",
                    "cognition_grapheme_script_save",
                ],
            },
            ToolDomainCatalogEntry {
                domain: "runtime",
                summary: "Durable jobs, workflows, recurring schedules, delivery",
                tools: &[
                    "cognition_job_enqueue",
                    "cognition_runtime_workflow_run",
                    "cognition_runtime_workflow_plan",
                    "cognition_runtime_workflow_status",
                    "cognition_runtime_workflow_cancel",
                    "cognition_runtime_workflow_schedule",
                    "cognition_runtime_jobs_list",
                    "cognition_runtime_jobs_status",
                    "cognition_runtime_jobs_cancel",
                    "cognition_runtime_delivery_status",
                    "cognition_runtime_recurring_list",
                    "cognition_runtime_recurring_register",
                    "cognition_runtime_recurring_pause",
                    "cognition_runtime_recurring_cancel",
                    "cognition_runtime_recurring_doctor",
                    "cognition_runtime_recurring_preview",
                ],
            },
            ToolDomainCatalogEntry {
                domain: "vault",
                summary: "Vault notes — list, read, write (tag runtime-learning for recall)",
                tools: &[
                    "cognition_vault_list",
                    "cognition_vault_read",
                    "cognition_vault_write",
                ],
            },
            ToolDomainCatalogEntry {
                domain: "history",
                summary: "Drill into prior turn tool receipts by slice id",
                tools: &["cognition_tool_history_detail"],
            },
            ToolDomainCatalogEntry {
                domain: "identity",
                summary: "Identity graph inspect and commit (operator-gated writes)",
                tools: &[
                    "cognition_identity_context",
                    "cognition_identity_propose",
                    "cognition_identity_commit",
                ],
            },
            ToolDomainCatalogEntry {
                domain: "skill",
                summary: "OpenShell / imported skill observe and propose",
                tools: &[
                    "cognition_skill_discover",
                    "cognition_skill_propose",
                    "cognition_openshell_status",
                ],
            },
            ToolDomainCatalogEntry {
                domain: "overlay",
                summary: "Propose manuscript overlay appendices (operator approves)",
                tools: &[
                    "cognition_manuscript_overlay_propose",
                    "cognition_manuscript_overlay_list",
                ],
            },
        ]
    })
    .as_slice()
}

pub fn worker_tool_domain_catalog() -> &'static [ToolDomainCatalogEntry] {
    static WORKER: OnceLock<Vec<ToolDomainCatalogEntry>> = OnceLock::new();
    WORKER.get_or_init(|| {
        vec![
            ToolDomainCatalogEntry {
                domain: "execute",
                summary: "Run resolved capabilities, Grapheme scripts, MCP invokes",
                tools: &[
                    "cognition_capability_invoke",
                    "cognition_web_search",
                    "cognition_grapheme_run",
                    "cognition_grapheme_cli_run",
                    "cognition_mcp_invoke",
                    "cognition_grapheme_template_run",
                ],
            },
            ToolDomainCatalogEntry {
                domain: "discover",
                summary: "Capability/Grapheme discovery when handoff lacks resolution",
                tools: &[
                    "cognition_capability_search",
                    "cognition_capability_resolve",
                    "cognition_grapheme_modules",
                    "cognition_grapheme_modules_info",
                    "cognition_grapheme_modules_ops",
                    "cognition_grapheme_examples",
                    "cognition_mcp_discover",
                    "cognition_mcp_servers",
                ],
            },
            ToolDomainCatalogEntry {
                domain: "memory",
                summary: "Workshop memory ritual tools",
                tools: &[
                    "cognition_memory_schema",
                    "cognition_memory_moods",
                    "cognition_memory_calibrate",
                    "cognition_memory_list",
                    "cognition_memory_recall",
                    "cognition_memory_store",
                    "cognition_identity_recall",
                ],
            },
            ToolDomainCatalogEntry {
                domain: "vault",
                summary: "Vault read/search/write for workshop notes and journal articles",
                tools: &[
                    "cognition_vault_list",
                    "cognition_vault_read",
                    "cognition_vault_search",
                    "cognition_vault_write",
                ],
            },
            ToolDomainCatalogEntry {
                domain: "openshell",
                summary: "Sandbox skill probe and run",
                tools: &[
                    "cognition_openshell_status",
                    "cognition_openshell_sandbox_run",
                    "cognition_skill_discover",
                    "cognition_skill_propose",
                    "cognition_skill_probe",
                ],
            },
            ToolDomainCatalogEntry {
                domain: "scripts",
                summary: "Save/load reusable Grapheme scripts",
                tools: &[
                    "cognition_grapheme_script_save",
                    "cognition_grapheme_script_list",
                    "cognition_grapheme_script_search",
                ],
            },
        ]
    })
    .as_slice()
}

pub fn tool_one_liner(name: &str) -> &'static str {
    match name {
        COGNITION_TOOLS_DISCOVER => "Unlock a tool domain for this session (memory, catalog, runtime, …)",
        "cognition_capability_search" => "Search capability catalog by keyword",
        "cognition_capability_resolve" => "Resolve capability id to bindings",
        "cognition_tool_history_summary" => "Summarize recent turn tool slices",
        "cognition_tool_history_detail" => "Full tool receipt for slice_id=turn:N",
        "cognition_spawn_turn_worker" => "Delegate execution to workshop lane",
        "cognition_memory_context" => "Load Locus AVEC + session memory context",
        "cognition_memory_store" => "Store episodic STTP node in Locus memory",
        "cognition_identity_recall" => "Look up preferences, people, and identity facts",
        "cognition_identity_remember" => "Remember durable personal facts in identity memory",
        "cognition_vault_search" => "Search vault notes",
        "cognition_web_search" => "Search the public web (provider fallback from config)",
        "cognition_turn_begin_work" => "Progress line before heavy tools",
        "cognition_turn_checkpoint" => "Mid-task update; hand turn to principal",
        "cognition_turn_finish" => "Commit principal-ready answer",
        "cognition_turn_worker_status" => "Pending worker status",
        "cognition_capability_invoke" => "One-shot capability execution",
        "cognition_grapheme_script_load" => "Load saved Grapheme script body",
        "cognition_grapheme_template_run" => "Run preset Grapheme template",
        _ => "Session-unlocked tool — see cognition_tools_discover catalog",
    }
}

pub fn domain_catalog(lane: ToolSurfaceLane) -> &'static [ToolDomainCatalogEntry] {
    match lane {
        ToolSurfaceLane::Host => host_tool_domain_catalog(),
        ToolSurfaceLane::Worker => worker_tool_domain_catalog(),
    }
}

pub fn bootstrap_tools(lane: ToolSurfaceLane) -> &'static [&'static str] {
    match lane {
        ToolSurfaceLane::Host => HOST_BOOTSTRAP_TOOLS,
        ToolSurfaceLane::Worker => WORKER_BOOTSTRAP_TOOLS,
    }
}

pub fn load_session_tool_surface(session_id: &str) -> SessionToolSurface {
    let path = session_surface_path(session_id);
    if let Ok(raw) = fs::read_to_string(&path) {
        if let Ok(mut surface) = serde_json::from_str::<SessionToolSurface>(&raw) {
            surface.session_id = session_id.to_string();
            return surface;
        }
    }
    SessionToolSurface {
        session_id: session_id.to_string(),
        unlocked_domains: Vec::new(),
        discover_events: Vec::new(),
        updated_at_utc: Utc::now(),
    }
}

/// Remove persisted per-session tool surface (MCP servers, allowlists, etc.).
pub fn delete_session_tool_surface(session_id: &str) {
    let path = session_surface_path(session_id);
    if path.exists() {
        let _ = fs::remove_file(path);
    }
}

pub fn save_session_tool_surface(surface: &SessionToolSurface) -> Result<(), String> {
    let path = session_surface_path(&surface.session_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let mut surface = surface.clone();
    surface.updated_at_utc = Utc::now();
    let raw = serde_json::to_string_pretty(&surface).map_err(|err| err.to_string())?;
    fs::write(path, raw).map_err(|err| err.to_string())
}

pub fn unlock_session_domains(
    session_id: &str,
    lane: ToolSurfaceLane,
    domains: &[&str],
) -> Result<SessionToolSurface, String> {
    let mut surface = load_session_tool_surface(session_id);
    for domain in domains {
        let normalized = domain.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            continue;
        }
        if !domain_catalog(lane)
            .iter()
            .any(|entry| entry.domain == normalized)
        {
            return Err(format!("unknown tool domain for lane: {normalized}"));
        }
        if !surface.unlocked_domains.iter().any(|d| d == &normalized) {
            surface.unlocked_domains.push(normalized.clone());
        }
    }
    save_session_tool_surface(&surface)?;
    Ok(surface)
}

pub fn discover_session_domain(
    session_id: &str,
    lane: ToolSurfaceLane,
    domain: &str,
    full_allowlist: &HashSet<String>,
) -> Result<(SessionToolSurface, Vec<String>), String> {
    let catalog = domain_catalog(lane)
        .iter()
        .find(|entry| entry.domain == domain.trim().to_ascii_lowercase())
        .ok_or_else(|| format!("unknown domain: {domain}"))?;

    let surface = unlock_session_domains(session_id, lane, &[catalog.domain])?;
    let tools = catalog
        .tools
        .iter()
        .filter(|name| tool_allowed(name, full_allowlist))
        .map(|name| (*name).to_string())
        .collect::<Vec<_>>();

    let mut surface = surface;
    surface.discover_events.push(ToolDiscoverEvent {
        domain: catalog.domain.to_string(),
        lane: match lane {
            ToolSurfaceLane::Host => "host",
            ToolSurfaceLane::Worker => "worker",
        }
        .to_string(),
        tool_count: tools.len(),
        timestamp_utc: Utc::now(),
    });
    save_session_tool_surface(&surface)?;

    Ok((surface, tools))
}

pub fn effective_tool_names(
    session_id: &str,
    lane: ToolSurfaceLane,
    full_allowlist: &HashSet<String>,
) -> HashSet<String> {
    let mut names = HashSet::new();
    for tool in bootstrap_tools(lane) {
        if tool_allowed(tool, full_allowlist) {
            names.insert((*tool).to_string());
        }
    }

    let surface = load_session_tool_surface(session_id);
    for domain in domain_catalog(lane) {
        if !surface
            .unlocked_domains
            .iter()
            .any(|unlocked| unlocked == domain.domain)
        {
            continue;
        }
        for tool in domain.tools {
            if tool_allowed(tool, full_allowlist) {
                names.insert((*tool).to_string());
            }
        }
    }
    names
}

pub fn build_tool_hints_block(
    session_id: &str,
    prompt: &str,
    turns: &[ConversationTurn],
    char_budget: usize,
) -> String {
    let full_allow = host_bus_tool_names();
    let surface = load_session_tool_surface(session_id);
    let mut lines = vec![
        "Bootstrap tools are always available; call cognition_tools_discover(domain) to unlock groups for this session.".to_string(),
        format!(
            "Unlocked domains: {}",
            if surface.unlocked_domains.is_empty() {
                "(none yet)".to_string()
            } else {
                surface.unlocked_domains.join(", ")
            }
        ),
    ];

    for tool in HOST_BOOTSTRAP_TOOLS {
        lines.push(format!("- {tool}: {}", tool_one_liner(tool)));
    }

    let ranked = rank_hint_domains(prompt, turns);
    if !ranked.is_empty() {
        lines.push(format!(
            "Suggested discover next: {}",
            ranked
                .iter()
                .take(3)
                .map(|domain| format!("cognition_tools_discover(domain=\"{domain}\")"))
                .collect::<Vec<_>>()
                .join(" · ")
        ));
    }

    if !surface.unlocked_domains.is_empty() {
        let mut unlocked_lines = Vec::new();
        for domain_name in &surface.unlocked_domains {
            if let Some(domain) = host_tool_domain_catalog()
                .iter()
                .find(|entry| &entry.domain == domain_name)
            {
                let preview: Vec<String> = domain
                    .tools
                    .iter()
                    .take(4)
                    .filter(|name| tool_allowed(name, &full_allow))
                    .map(|name| format!("{name}"))
                    .collect();
                if !preview.is_empty() {
                    unlocked_lines.push(format!(
                        "- {} → {}",
                        domain.domain,
                        preview.join(", ")
                    ));
                }
            }
        }
        if !unlocked_lines.is_empty() {
            lines.push("Unlocked tool preview:".to_string());
            lines.extend(unlocked_lines);
        }
    }

    let body = truncate_text_for_budget(&lines.join("\n"), char_budget);
    if body.trim().is_empty() {
        return String::new();
    }
    format!("[MEDOUSA_TOOL_HINTS]\n{body}")
}

fn rank_hint_domains(prompt: &str, turns: &[ConversationTurn]) -> Vec<String> {
    let prompt_lower = prompt.to_ascii_lowercase();
    let mut scores: HashMap<&'static str, i32> = HashMap::new();

    let bump = |scores: &mut HashMap<&'static str, i32>, domain: &'static str, amount: i32| {
        *scores.entry(domain).or_default() += amount;
    };

    if contains_any(&prompt_lower, &["memory", "calibrate", "avec", "mood", "locus"]) {
        bump(&mut scores, "memory", 3);
    }
    if contains_any(
        &prompt_lower,
        &["capability", "manuscript", "module", "grapheme", "script", "research"],
    ) {
        bump(&mut scores, "catalog", 3);
    }
    if contains_any(
        &prompt_lower,
        &["workflow", "job", "cron", "schedule", "recurring", "enqueue"],
    ) {
        bump(&mut scores, "runtime", 3);
    }
    if contains_any(&prompt_lower, &["vault", "note", "journal", "learning"]) {
        bump(&mut scores, "vault", 3);
    }
    if contains_any(&prompt_lower, &["identity", "contact", "preference", "remember"]) {
        bump(&mut scores, "identity", 2);
    }
    if contains_any(&prompt_lower, &["skill", "openshell", "sandbox"]) {
        bump(&mut scores, "skill", 2);
    }
    if contains_any(&prompt_lower, &["worker", "spawn", "delegate", "workshop"]) {
        bump(&mut scores, "catalog", 2);
    }

    let rows = tool_history_summary_rows(turns, DEFAULT_TOOL_HISTORY_SUMMARY_TURNS, None, None);
    if !rows.is_empty() {
        bump(&mut scores, "history", 2);
    }

    let mut ordered: Vec<_> = scores.into_iter().collect();
    ordered.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    ordered.into_iter().map(|(domain, _)| domain.to_string()).collect()
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

pub fn handoff_implies_resolved_execution(
    capsule: Option<&crate::agent_runtime::turn_context::WorkerHandoffCapsule>,
) -> bool {
    capsule.is_some_and(|handoff| {
        !handoff.host_tool_digests.is_empty() || !handoff.relevant_slice_ids.is_empty()
    })
}

/// Workers that may write journal/vault notes should start with the vault domain unlocked.
pub fn worker_should_unlock_vault(
    task_prompt: &str,
    intent: crate::agent_runtime::turn_worker::TurnWorkerIntent,
) -> bool {
    use crate::agent_runtime::turn_worker::TurnWorkerIntent;
    if matches!(intent, TurnWorkerIntent::Research | TurnWorkerIntent::General) {
        return true;
    }
    let prompt_lower = task_prompt.to_ascii_lowercase();
    contains_any(
        &prompt_lower,
        &[
            "vault",
            "journal",
            "article",
            "write",
            "note",
            "learning",
            "runtime-learning",
        ],
    )
}

fn session_surface_path(session_id: &str) -> PathBuf {
    session::medousa_data_dir()
        .join("session_surfaces")
        .join(format!("{}.json", sanitize_session_filename(session_id)))
}

fn sanitize_session_filename(session_id: &str) -> String {
    session_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn surface_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .expect("tool surface test lock")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_host_tools_are_small() {
        assert!(HOST_BOOTSTRAP_TOOLS.len() >= 8);
        assert!(HOST_BOOTSTRAP_TOOLS.len() <= 16);
        assert!(HOST_BOOTSTRAP_TOOLS.contains(&COGNITION_TOOLS_DISCOVER));
        assert!(HOST_BOOTSTRAP_TOOLS.contains(&"cognition_identity_remember"));
        assert!(HOST_BOOTSTRAP_TOOLS.contains(&"cognition_identity_recall"));
    }

    #[test]
    fn session_unlock_expands_effective_tools() {
        let _guard = surface_test_lock();
        let session_id = format!("sess-test-{}", uuid::Uuid::new_v4().simple());
        let allow = host_bus_tool_names();
        let before = effective_tool_names(&session_id, ToolSurfaceLane::Host, &allow);
        assert!(!before.contains("cognition_memory_schema"));
        assert!(before.contains("cognition_identity_remember"));
        assert!(before.contains("cognition_identity_recall"));

        unlock_session_domains(&session_id, ToolSurfaceLane::Host, &["memory"]).expect("unlock");
        let after = effective_tool_names(&session_id, ToolSurfaceLane::Host, &allow);
        assert!(after.contains("cognition_memory_schema"));

        let _ = fs::remove_file(session_surface_path(&session_id));
    }

    #[test]
    fn tool_hints_block_mentions_discover() {
        let block = build_tool_hints_block("sess-hints", "calibrate memory posture", &[], 600);
        assert!(block.contains("[MEDOUSA_TOOL_HINTS]"));
        assert!(block.contains("cognition_tools_discover"));
        assert!(block.contains("memory"));
    }
}
