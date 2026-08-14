//! Phase 9 — progressive tool surface: bootstrap ring, session unlocks, turn hints.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

use crate::agent_runtime::prompt_prep::truncate_text_for_budget;
use crate::agent_runtime::turn_worker::{host_bus_tool_names, tool_allowed};
use crate::session::{self, ConversationTurn};
use crate::turn_slice::{DEFAULT_TOOL_HISTORY_SUMMARY_TURNS, tool_history_summary_rows};

pub const COGNITION_TOOLS_DISCOVER: &str = "cognition_tools_discover";

pub const DEFAULT_TOOL_HINTS_BLOCK_CHARS: usize = 700;

static SESSION_SURFACE_FILES: Lazy<crate::session_storage::SessionFileStore> = Lazy::new(|| {
    crate::session_storage::SessionFileStore::new(
        session::medousa_data_dir().join("session_surfaces"),
        "json",
    )
});

/// Host console domains unlocked at session start (no `cognition_tools_discover` step).
pub const DEFAULT_HOST_AUTO_UNLOCK_DOMAINS: &[&str] = &["memory", "vault", "calendar"];

pub const CALENDAR_DOMAIN_TOOLS: &[&str] = &[
    "cognition_calendar_list",
    "cognition_calendar_create",
    "cognition_calendar_update",
    "cognition_calendar_delete",
    "cognition_calendar_import",
    "cognition_calendar_export",
];

pub const BROWSER_HOST_AUTO_UNLOCK_DOMAIN: &str = "browser";

pub const ENVIRONMENT_HOST_AUTO_UNLOCK_DOMAIN: &str = "environment";

/// Host discover domain: environment spec, component canvas, context pointers.
pub const ENVIRONMENT_DOMAIN_TOOLS: &[&str] = &[
    "cognition_environment_wiki",
    "cognition_environment_get",
    "cognition_component_create",
    "cognition_environment_propose",
    "cognition_component_list",
    "cognition_environment_apply",
    "cognition_environment_activate_preset",
    "cognition_component_get",
    "cognition_component_update",
    "cognition_component_delete",
    "cognition_context_follow_pointer",
    "cognition_context_list_pointers",
    "cognition_intent_resolve",
    "cognition_feed_subscribe",
    "cognition_feed_publish",
    "cognition_layout_get",
    "cognition_layout_apply",
    "cognition_layout_reset",
    "cognition_environment_patch",
    "cognition_custom_view_doctor",
    "cognition_custom_view_compose",
];

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
    "cognition_vault_grep",
    "cognition_calendar_list",
    "cognition_artifact_list",
    "cognition_artifact_read",
    "cognition_artifact_grep",
    "cognition_turn_begin_work",
    "cognition_turn_update_user",
    "cognition_turn_checkpoint",
    "cognition_turn_finish",
    "cognition_turn_propose_mode",
    "cognition_turn_worker_status",
    "cognition_ui_present",
    "cognition_ui_build",
    "cognition_ui_scene",
];

/// Always-visible worker workshop tools.
pub const WORKER_BOOTSTRAP_TOOLS: &[&str] = &[
    COGNITION_TOOLS_DISCOVER,
    "cognition_turn_begin_work",
    "cognition_turn_update_user",
    "cognition_turn_finish",
    "cognition_capability_invoke",
    "cognition_web_search",
    "cognition_grapheme_script_load",
    "cognition_grapheme_template_run",
    "cognition_memory_context",
    "cognition_memory_store",
    "cognition_ui_build",
    "cognition_ui_scene",
    "cognition_ui_present",
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
                    "cognition_vault_grep",
                    "cognition_vault_write",
                    "cognition_vault_delete",
                    "cognition_vault_move",
                    "cognition_vault_tags",
                ],
            },
            ToolDomainCatalogEntry {
                domain: "calendar",
                summary: "Personal calendar (.ics) — list, create, update, delete, import, export",
                tools: CALENDAR_DOMAIN_TOOLS,
            },
            ToolDomainCatalogEntry {
                domain: "documents",
                summary: "Edit files: vault markdown (cognition_vault_*) or HTML presentations (cognition_artifact_*). Workflow: list/search → grep → read → write.",
                tools: &[
                    "cognition_vault_list",
                    "cognition_vault_read",
                    "cognition_vault_grep",
                    "cognition_vault_search",
                    "cognition_vault_write",
                    "cognition_vault_delete",
                    "cognition_vault_move",
                    "cognition_artifact_list",
                    "cognition_artifact_read",
                    "cognition_artifact_grep",
                    "cognition_artifact_write",
                    "cognition_artifact_delete",
                    "cognition_ui_present",
                ],
            },
            ToolDomainCatalogEntry {
                domain: "presentation",
                summary: "Liquid chat embeds (markdown) first; cognition_ui_build for streaming scenes; legacy ui_scene + HTML artifacts",
                tools: &[
                    "cognition_ui_build",
                    "cognition_ui_scene",
                    "cognition_ui_present",
                    "cognition_artifact_list",
                    "cognition_artifact_read",
                    "cognition_artifact_grep",
                    "cognition_artifact_write",
                    "cognition_artifact_delete",
                ],
            },
            ToolDomainCatalogEntry {
                domain: "history",
                summary: "Read prior chats or drill into prior turn tool receipts",
                tools: &[
                    "cognition_chat_history_search",
                    "cognition_chat_history_read",
                    "cognition_tool_history_detail",
                ],
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
            ToolDomainCatalogEntry {
                domain: "browser",
                summary: "Agent Browser fetch/snapshot/act on shared Web tab (requires supports_browser_host client)",
                tools: &[
                    "cognition_browser_fetch",
                    "cognition_browser_snapshot",
                    "cognition_browser_act",
                ],
            },
            ToolDomainCatalogEntry {
                domain: ENVIRONMENT_HOST_AUTO_UNLOCK_DOMAIN,
                summary: "Home environment — surfaces, chrome, persistent components, context pointers",
                tools: ENVIRONMENT_DOMAIN_TOOLS,
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
                domain: "detamu",
                summary: "Detamu world-model — repo snapshots at commit OIDs (status/files/impact/Code AVEC)",
                tools: crate::detamu_tools::DETAMU_COGNITION_TOOLS,
            },
            ToolDomainCatalogEntry {
                domain: "coding",
                summary: "Workshop coding toolkit — read/patch under scripts/Forge roots, shared shell sessions (opt-in)",
                tools: crate::coding_tools::CODING_COGNITION_TOOLS,
            },
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
                    "cognition_shell_status",
                    "cognition_shell_run",
                    "cognition_code_hover",
                    "cognition_code_definition",
                    "cognition_code_diagnostics",
                    "cognition_code_symbols",
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
                    "cognition_vault_grep",
                    "cognition_vault_search",
                    "cognition_vault_write",
                    "cognition_vault_delete",
                    "cognition_vault_move",
                    "cognition_vault_tags",
                ],
            },
            ToolDomainCatalogEntry {
                domain: "calendar",
                summary: "Personal calendar (.ics) for workshop scheduling",
                tools: CALENDAR_DOMAIN_TOOLS,
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
            ToolDomainCatalogEntry {
                domain: ENVIRONMENT_HOST_AUTO_UNLOCK_DOMAIN,
                summary: "Home environment — surfaces, chrome, persistent components, context pointers",
                tools: ENVIRONMENT_DOMAIN_TOOLS,
            },
            ToolDomainCatalogEntry {
                domain: "presentation",
                summary: "Liquid chat embeds (markdown) first; ui_build for streaming scenes; legacy ui_scene + HTML artifacts",
                tools: &[
                    "cognition_ui_build",
                    "cognition_ui_scene",
                    "cognition_ui_present",
                    "cognition_artifact_list",
                    "cognition_artifact_read",
                    "cognition_artifact_grep",
                    "cognition_artifact_write",
                ],
            },
            ToolDomainCatalogEntry {
                domain: BROWSER_HOST_AUTO_UNLOCK_DOMAIN,
                summary: "Agent Browser fetch/snapshot/act on shared Web tab (requires supports_browser_host client)",
                tools: &[
                    "cognition_browser_fetch",
                    "cognition_browser_snapshot",
                    "cognition_browser_act",
                ],
            },
        ]
    })
    .as_slice()
}

/// Domains unlocked when a bound workshop starts (execution lane for Home).
pub const BOUND_WORKSHOP_AUTO_UNLOCK_DOMAINS: &[&str] = &[
    "environment",
    "presentation",
    "execute",
    "discover",
    "vault",
    "calendar",
    "memory",
];

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

/// Unlock memory + vault + calendar on host console turns so ritual/write tools are callable without discover.
pub fn ensure_host_session_tool_defaults(session_id: &str) {
    let _ = unlock_session_domains(
        session_id,
        ToolSurfaceLane::Host,
        DEFAULT_HOST_AUTO_UNLOCK_DOMAINS,
    );
}

/// Unlock environment/canvas domain when the connected client can render UI artifacts (Home).
pub fn ensure_environment_domain_for_ui_clients(session_id: &str, supports_ui_artifacts: bool) {
    if supports_ui_artifacts {
        let _ = unlock_session_domains(
            session_id,
            ToolSurfaceLane::Host,
            &[ENVIRONMENT_HOST_AUTO_UNLOCK_DOMAIN],
        );
    }
}

/// Unlock browser domain when the connected client advertises Agent Browser Host.
pub fn ensure_browser_domain_for_capable_clients(session_id: &str, supports_browser_host: bool) {
    if supports_browser_host {
        let _ = unlock_session_domains(
            session_id,
            ToolSurfaceLane::Host,
            &[BROWSER_HOST_AUTO_UNLOCK_DOMAIN],
        );
    }
}

/// Unlock browser domain on the worker lane (bound workshop / Home execution).
pub fn ensure_worker_browser_domain_for_capable_clients(
    session_id: &str,
    supports_browser_host: bool,
) {
    if supports_browser_host {
        let _ = unlock_session_domains(
            session_id,
            ToolSurfaceLane::Worker,
            &[BROWSER_HOST_AUTO_UNLOCK_DOMAIN],
        );
    }
}

/// Unlock environment/canvas domain on the worker lane when the client renders UI artifacts.
pub fn ensure_worker_environment_domain_for_ui_clients(
    session_id: &str,
    supports_ui_artifacts: bool,
) {
    if supports_ui_artifacts {
        let _ = unlock_session_domains(
            session_id,
            ToolSurfaceLane::Worker,
            &[ENVIRONMENT_HOST_AUTO_UNLOCK_DOMAIN],
        );
    }
}

/// Auto-unlock execution domains when entering a bound workshop on the worker lane.
pub fn ensure_bound_workshop_session_tool_defaults(session_id: &str) {
    let _ = unlock_session_domains(
        session_id,
        ToolSurfaceLane::Worker,
        BOUND_WORKSHOP_AUTO_UNLOCK_DOMAINS,
    );
}

/// Unlock the opt-in coding domain (read/patch/session shell) for a session.
/// Callers gate this on manuscript / Forge `work_id` / Settings — never default.
pub fn ensure_coding_domain_for_session(session_id: &str) {
    let _ = unlock_session_domains(session_id, ToolSurfaceLane::Worker, &["coding"]);
}

/// Unlock the opt-in Detamu world-model domain for a session.
/// Callers gate this on manuscript / Forge `work_id` / Settings — never default.
pub fn ensure_detamu_domain_for_session(session_id: &str) {
    let _ = unlock_session_domains(session_id, ToolSurfaceLane::Worker, &["detamu"]);
}

pub fn load_session_tool_surface(session_id: &str) -> SessionToolSurface {
    let parsed_session_id = crate::session_storage::SessionId::parse(session_id).ok();
    if let Some(session_id) = parsed_session_id
        && let Ok(raw) = SESSION_SURFACE_FILES.read(&session_id)
        && let Ok(mut surface) = serde_json::from_slice::<SessionToolSurface>(&raw)
    {
        surface.session_id = session_id.to_string();
        return surface;
    }
    SessionToolSurface {
        session_id: session_id.to_string(),
        unlocked_domains: Vec::new(),
        discover_events: Vec::new(),
        updated_at_utc: Utc::now(),
    }
}

/// Remove persisted per-session tool surface (MCP servers, allowlists, etc.).
pub fn delete_session_tool_surface(session_id: &str) -> Result<(), String> {
    let session_id =
        crate::session_storage::SessionId::parse(session_id).map_err(|error| error.to_string())?;
    SESSION_SURFACE_FILES
        .remove(&session_id)
        .map_err(|error| error.to_string())
}

pub fn save_session_tool_surface(surface: &SessionToolSurface) -> Result<(), String> {
    let session_id = crate::session_storage::SessionId::parse(&surface.session_id)
        .map_err(|error| error.to_string())?;
    let mut surface = surface.clone();
    surface.updated_at_utc = Utc::now();
    let raw = serde_json::to_vec_pretty(&surface).map_err(|err| err.to_string())?;
    SESSION_SURFACE_FILES
        .atomic_write(&session_id, &raw)
        .map_err(|err| err.to_string())
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
    catalog: &crate::typed_tools::ToolCatalog,
    session_id: &str,
    prompt: &str,
    turns: &[ConversationTurn],
    char_budget: usize,
) -> String {
    let full_allow = host_bus_tool_names();
    let surface = load_session_tool_surface(session_id);
    let mut lines = vec![
        "Bootstrap tools are always available. Chat auto-unlocks memory, vault read, identity, and catalog/runtime orchestration. Studio/canvas tools unlock in Workshop after begin_work.".to_string(),
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
        lines.push(format!(
            "- {tool}: {}",
            catalog.presentation_summary_for_wire(tool)
        ));
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
                .find(|entry| entry.domain == domain_name)
            {
                let preview: Vec<String> = domain
                    .tools
                    .iter()
                    .take(4)
                    .filter(|name| tool_allowed(name, &full_allow))
                    .map(|name| name.to_string())
                    .collect();
                if !preview.is_empty() {
                    unlocked_lines.push(format!("- {} → {}", domain.domain, preview.join(", ")));
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

    if contains_any(
        &prompt_lower,
        &["memory", "calibrate", "avec", "mood", "locus"],
    ) {
        bump(&mut scores, "memory", 3);
    }
    if contains_any(
        &prompt_lower,
        &[
            "capability",
            "manuscript",
            "module",
            "grapheme",
            "script",
            "research",
        ],
    ) {
        bump(&mut scores, "catalog", 3);
    }
    if contains_any(
        &prompt_lower,
        &[
            "workflow",
            "job",
            "cron",
            "schedule",
            "recurring",
            "enqueue",
        ],
    ) {
        bump(&mut scores, "runtime", 3);
    }
    if contains_any(&prompt_lower, &["vault", "note", "journal", "learning"]) {
        bump(&mut scores, "vault", 3);
    }
    if contains_any(
        &prompt_lower,
        &[
            "calendar",
            "meeting",
            "event",
            "appointment",
            "ics",
            "schedule",
        ],
    ) {
        bump(&mut scores, "calendar", 3);
    }
    if contains_any(
        &prompt_lower,
        &["identity", "contact", "preference", "remember"],
    ) {
        bump(&mut scores, "identity", 2);
    }
    if contains_any(&prompt_lower, &["skill", "openshell", "sandbox"]) {
        bump(&mut scores, "skill", 2);
    }
    if contains_any(&prompt_lower, &["worker", "spawn", "delegate", "workshop"]) {
        bump(&mut scores, "catalog", 2);
    }
    if contains_any(
        &prompt_lower,
        &[
            "canvas",
            "component",
            "environment",
            "surface",
            "pointer",
            "writing studio",
            "home layout",
            "fab",
            "chrome",
        ],
    ) {
        bump(&mut scores, ENVIRONMENT_HOST_AUTO_UNLOCK_DOMAIN, 4);
    }

    let rows = tool_history_summary_rows(turns, DEFAULT_TOOL_HISTORY_SUMMARY_TURNS, None, None);
    if !rows.is_empty() {
        bump(&mut scores, "history", 2);
    }

    let mut ordered: Vec<_> = scores.into_iter().collect();
    ordered.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    ordered
        .into_iter()
        .map(|(domain, _)| domain.to_string())
        .collect()
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
    if matches!(
        intent,
        TurnWorkerIntent::Research | TurnWorkerIntent::General
    ) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::{Mutex, OnceLock};

    fn surface_test_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[test]
    fn bootstrap_host_tools_are_small() {
        assert!(HOST_BOOTSTRAP_TOOLS.len() >= 8);
        assert!(HOST_BOOTSTRAP_TOOLS.len() <= 24);
        assert!(HOST_BOOTSTRAP_TOOLS.contains(&COGNITION_TOOLS_DISCOVER));
        assert!(HOST_BOOTSTRAP_TOOLS.contains(&"cognition_identity_remember"));
        assert!(HOST_BOOTSTRAP_TOOLS.contains(&"cognition_identity_recall"));
        assert!(HOST_BOOTSTRAP_TOOLS.contains(&"cognition_calendar_list"));
    }

    #[test]
    fn environment_tools_are_worker_scoped() {
        let host = host_bus_tool_names();
        assert!(!host.contains("cognition_environment_propose"));
        assert!(!host.contains("cognition_component_create"));
        assert!(!host.contains("cognition_context_follow_pointer"));

        let worker = crate::agent_runtime::turn_worker::allowed_tool_names_for_intent(
            crate::agent_runtime::turn_worker::TurnWorkerIntent::General,
        );
        assert!(worker.contains("cognition_environment_propose"));
        assert!(worker.contains("cognition_component_create"));
        assert!(worker.contains("cognition_context_follow_pointer"));
    }

    #[test]
    fn ui_client_auto_unlocks_environment_domain() {
        let _guard = surface_test_lock();
        let session_id = format!("sess-env-{}", uuid::Uuid::new_v4().simple());
        let allow = host_bus_tool_names();
        let before = effective_tool_names(&session_id, ToolSurfaceLane::Host, &allow);
        assert!(!before.contains("cognition_environment_get"));

        ensure_environment_domain_for_ui_clients(&session_id, true);
        let surface = load_session_tool_surface(&session_id);
        assert!(
            surface
                .unlocked_domains
                .iter()
                .any(|domain| domain == ENVIRONMENT_HOST_AUTO_UNLOCK_DOMAIN)
        );
        let after = effective_tool_names(&session_id, ToolSurfaceLane::Host, &allow);
        assert!(!after.contains("cognition_environment_get"));
        assert!(!after.contains("cognition_environment_wiki"));
        assert!(!after.contains("cognition_component_create"));

        let _ = delete_session_tool_surface(&session_id);
    }

    #[test]
    fn ui_present_is_worker_scoped() {
        let host_allow = host_bus_tool_names();
        let host = effective_tool_names("sess-ui-present", ToolSurfaceLane::Host, &host_allow);
        assert!(!host.contains("cognition_ui_present"));

        let worker_allow = crate::agent_runtime::turn_worker::allowed_tool_names_for_intent(
            crate::agent_runtime::turn_worker::TurnWorkerIntent::General,
        );
        let worker =
            effective_tool_names("sess-ui-present", ToolSurfaceLane::Worker, &worker_allow);
        assert!(worker.contains("cognition_ui_present"));
    }

    #[test]
    fn ensure_bound_workshop_unlocks_environment_on_worker_lane() {
        let _guard = surface_test_lock();
        let session_id = format!("sess-bound-{}", uuid::Uuid::new_v4().simple());
        let mut allow = HashSet::new();
        for name in ENVIRONMENT_DOMAIN_TOOLS {
            allow.insert((*name).to_string());
        }
        allow.insert(COGNITION_TOOLS_DISCOVER.to_string());
        allow.insert("cognition_mcp_discover".to_string());

        ensure_bound_workshop_session_tool_defaults(&session_id);
        let after = effective_tool_names(&session_id, ToolSurfaceLane::Worker, &allow);
        assert!(after.contains("cognition_environment_get"));
        assert!(after.contains("cognition_environment_propose"));
        assert!(after.contains("cognition_mcp_discover"));

        let _ = delete_session_tool_surface(&session_id);
    }

    #[test]
    fn ensure_host_defaults_unlocks_memory_and_vault() {
        let _guard = surface_test_lock();
        let session_id = format!("sess-defaults-{}", uuid::Uuid::new_v4().simple());
        let allow = host_bus_tool_names();
        let before = effective_tool_names(&session_id, ToolSurfaceLane::Host, &allow);
        assert!(!before.contains("cognition_vault_write"));
        assert!(!before.contains("cognition_memory_calibrate"));

        ensure_host_session_tool_defaults(&session_id);
        let after = effective_tool_names(&session_id, ToolSurfaceLane::Host, &allow);
        assert!(after.contains("cognition_vault_read"));
        assert!(!after.contains("cognition_vault_write"));
        assert!(after.contains("cognition_memory_calibrate"));

        let _ = delete_session_tool_surface(&session_id);
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

        let _ = delete_session_tool_surface(&session_id);
    }

    #[test]
    fn tool_hints_block_mentions_discover() {
        let block = build_tool_hints_block(
            &crate::typed_tools::ToolCatalog::default(),
            "sess-hints",
            "calibrate memory posture",
            &[],
            600,
        );
        assert!(block.contains("[MEDOUSA_TOOL_HINTS]"));
        assert!(block.contains("cognition_tools_discover"));
        assert!(block.contains("memory"));
    }
}
