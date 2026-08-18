//! Medousa product placement compiled onto the generic typed-tool catalog.

use genai::chat::Tool;

use crate::agent_runtime::turn_worker::{
    TurnWorkerIntent, allowed_tool_names_for_intent, host_bus_tool_names, tool_allowed,
};
use crate::typed_tools::{
    EmptyCallMetadata, ModeToolAdapter, ModeToolAdapterError, RegisteredToolKind, ToolCapabilityId,
    ToolCatalog, ToolDomainId, ToolEffect, ToolExposureRef, ToolId, ToolModeId, ToolPlacementIndex,
    ToolPolicyId, ToolSurfaceId,
};

pub(crate) const GENERAL_MODE_ID: ToolModeId = ToolModeId::new("general");
pub(crate) const WORKSHOP_MODE_ID: ToolModeId = ToolModeId::new("workshop");
pub(crate) const CODER_MODE_ID: ToolModeId = ToolModeId::new("coder");

pub(crate) const BOOTSTRAP_SURFACE_ID: ToolSurfaceId = ToolSurfaceId::new("bootstrap");
pub(crate) const AUTHORIZED_SURFACE_ID: ToolSurfaceId = ToolSurfaceId::new("authorized");
pub(crate) const DOMAIN_SURFACE_ID: ToolSurfaceId = ToolSurfaceId::new("domain");
pub(crate) const INITIAL_SURFACE_ID: ToolSurfaceId = ToolSurfaceId::new("initial");
pub(crate) const EDITOR_CONTEXT_SURFACE_ID: ToolSurfaceId = ToolSurfaceId::new("editor_context");

pub(crate) fn first_party_placement_index() -> ToolPlacementIndex {
    let mut index = ToolPlacementIndex::default();

    add_ids(
        &mut index,
        crate::tool_bootstrap::HOST_BOOTSTRAP_TOOLS,
        ToolExposureRef::new(GENERAL_MODE_ID, BOOTSTRAP_SURFACE_ID),
    );
    add_ids(
        &mut index,
        crate::tool_bootstrap::WORKER_BOOTSTRAP_TOOLS,
        ToolExposureRef::new(WORKSHOP_MODE_ID, BOOTSTRAP_SURFACE_ID),
    );

    let host = host_bus_tool_names();
    for name in crate::tool_names::registered_cognition_tools() {
        if tool_allowed(name, &host) {
            index.add_exposure(
                ToolId::new(name),
                ToolExposureRef::new(GENERAL_MODE_ID, AUTHORIZED_SURFACE_ID),
            );
        }
    }
    for (intent, policy) in worker_policies() {
        let allowed = allowed_tool_names_for_intent(intent);
        for name in crate::tool_names::registered_cognition_tools() {
            if tool_allowed(name, &allowed) {
                index.add_exposure(
                    ToolId::new(name),
                    ToolExposureRef::policy(WORKSHOP_MODE_ID, AUTHORIZED_SURFACE_ID, policy),
                );
            }
        }
    }

    for entry in crate::tool_bootstrap::host_tool_domain_catalog() {
        add_ids(
            &mut index,
            entry.tools,
            ToolExposureRef::domain(
                GENERAL_MODE_ID,
                DOMAIN_SURFACE_ID,
                ToolDomainId::new(entry.domain),
            ),
        );
    }
    for entry in crate::tool_bootstrap::worker_tool_domain_catalog() {
        add_ids(
            &mut index,
            entry.tools,
            ToolExposureRef::domain(
                WORKSHOP_MODE_ID,
                DOMAIN_SURFACE_ID,
                ToolDomainId::new(entry.domain),
            ),
        );
    }

    crate::agent_runtime::coder_tools::register_catalog_placements(&mut index);
    add_effects_and_capabilities(&mut index);
    add_presentation_overrides(&mut index);
    index
}

pub(crate) fn compile_general_surface(
    catalog: &ToolCatalog,
) -> Result<Vec<Tool>, ModeToolAdapterError> {
    ModeToolAdapter::<EmptyCallMetadata>::new(GENERAL_MODE_ID)?.compile_surface(catalog, |entry| {
        entry.contract.kind != RegisteredToolKind::RuntimeAdapter
    })
}

fn add_ids(index: &mut ToolPlacementIndex, names: &[&'static str], exposure: ToolExposureRef) {
    for name in names {
        index.add_exposure(ToolId::new(name), exposure);
    }
}

fn worker_policies() -> [(TurnWorkerIntent, ToolPolicyId); 4] {
    [
        (TurnWorkerIntent::General, ToolPolicyId::new("general")),
        (TurnWorkerIntent::Research, ToolPolicyId::new("research")),
        (
            TurnWorkerIntent::MemoryContext,
            ToolPolicyId::new("memory_context"),
        ),
        (
            TurnWorkerIntent::MemoryAvecCalibrate,
            ToolPolicyId::new("memory_avec_calibrate"),
        ),
    ]
}

fn add_effects_and_capabilities(index: &mut ToolPlacementIndex) {
    for name in [
        "cognition_utility_day_of_week",
        "cognition_utility_time_now",
        "cognition_utility_uuid",
        "cognition_store_read",
        crate::agent_runtime::coder_tools::COGNITION_ENGINEERING_POINTERS,
        "cognition_runtime_query",
        crate::public_api::COGNITION_SCHEMA,
    ] {
        index.set_effect(ToolId::new(name), ToolEffect::Observe);
    }
    index.set_effect(
        ToolId::new(crate::agent_runtime::coder_tools::COGNITION_CODER_TOOLS_DISCOVER),
        ToolEffect::Coordinate,
    );

    let browser_host = ToolCapabilityId::new("browser_host");
    for name in [
        "cognition_browser_fetch",
        "cognition_browser_snapshot",
        "cognition_browser_act",
    ] {
        index.require_capability(ToolId::new(name), browser_host);
    }
    let ui_artifacts = ToolCapabilityId::new("ui_artifacts");
    for name in [
        "cognition_ui_build",
        "cognition_ui_scene",
        "cognition_ui_present",
    ] {
        index.require_capability(ToolId::new(name), ui_artifacts);
    }
}

fn add_presentation_overrides(index: &mut ToolPlacementIndex) {
    for (name, summary) in PRESENTATION_OVERRIDES {
        index.set_presentation_summary(ToolId::new(name), summary);
    }
}

const PRESENTATION_OVERRIDES: &[(&str, &str)] = &[
    (
        crate::tool_bootstrap::COGNITION_TOOLS_DISCOVER,
        "Unlock a tool domain for this session (memory, catalog, runtime, …)",
    ),
    (
        "cognition_capability",
        "Find or invoke a capability, MCP tool, or Grapheme module by typed action name",
    ),
    (
        "cognition_schema",
        "Fetch typed action parameter schemas (batch several types in one call)",
    ),
    (
        "cognition_runtime_query",
        "Inspect jobs, recurring, workflows, or delivery by typed action name",
    ),
    (
        "cognition_runtime_mutate",
        "Enqueue, cancel, register, pause, run, schedule, or plan by typed action name",
    ),
    (
        "cognition_tool_history_summary",
        "Summarize recent turn tool slices",
    ),
    (
        "cognition_tool_history_detail",
        "Full tool receipt for slice_id=turn:N",
    ),
    (
        "cognition_spawn_turn_worker",
        "Delegate execution to workshop lane",
    ),
    (
        "cognition_memory_context",
        "Load Locus AVEC + session memory context",
    ),
    (
        "cognition_memory_store",
        "Store episodic STTP node in Locus memory",
    ),
    (
        "cognition_identity_recall",
        "Look up preferences, people, and identity facts",
    ),
    (
        "cognition_identity_remember",
        "Remember durable personal facts in identity memory",
    ),
    (
        "cognition_store_read",
        "Read or search vault, artifacts, code, or saved scripts",
    ),
    (
        "cognition_store_write",
        "Write, delete, or move vault, artifacts, code, or saved scripts",
    ),
    (
        "cognition_calendar_list",
        "List personal calendar events in a time range",
    ),
    (
        "cognition_calendar_create",
        "Create a calendar event in vault .ics",
    ),
    (
        "cognition_calendar_update",
        "Update a calendar event by uid",
    ),
    (
        "cognition_calendar_delete",
        "Delete a calendar event by uid",
    ),
    (
        "cognition_calendar_import",
        "Import VEVENTs from raw ICS text",
    ),
    (
        "cognition_calendar_export",
        "Export the vault calendar as ICS",
    ),
    (
        "cognition_web_search",
        "Search the public web (provider fallback from config)",
    ),
    (
        "cognition_browser_fetch",
        "Fetch a URL via Agent Browser and return markdown excerpt",
    ),
    (
        "cognition_browser_snapshot",
        "Snapshot a URL via Agent Browser for synthesis",
    ),
    (
        "cognition_browser_act",
        "Click/type/scroll on the shared Web tab (agent control required)",
    ),
    (
        "cognition_turn",
        "Turn control: begin work, update the principal, checkpoint, or finish (action=turn.finish / turn.checkpoint / …)",
    ),
    (
        "cognition_ui_build",
        "Streaming interactive Liquid scenes (begin → set_prose/add_section/add_card/add_actions → done) when markdown embeds aren't enough; prefer over cognition_ui_scene",
    ),
    (
        "cognition_ui_scene",
        "Legacy freeform scene ops (plan_layout/fill_slot) — prefer markdown embeds or cognition_ui_build for chat",
    ),
    (
        "cognition_ui_present",
        "Publish a new HTML artifact in chat (inline, panel, or fullscreen)",
    ),
    (
        "cognition_environment_wiki",
        "Canvas SDK STTP nodes — schemas, merge_spec, recipes; call before guessing propose JSON",
    ),
    (
        "cognition_environment_get",
        "Read environment spec — custom surfaces + components; start canvas work here",
    ),
    (
        "cognition_environment_propose",
        "Validate environment spec patch (errors[] on failure)",
    ),
    (
        "cognition_environment_apply",
        "Apply approved environment spec changes",
    ),
    (
        "cognition_environment_activate_preset",
        "Switch active layout preset (nav + chrome)",
    ),
    (
        "cognition_component_list",
        "List persisted canvas components",
    ),
    (
        "cognition_component_create",
        "Add presentation/chrome_action on a custom surface (camelCase surfaceId)",
    ),
    ("cognition_component_update", "Patch a canvas component"),
    ("cognition_component_delete", "Remove a canvas component"),
    (
        "cognition_context_follow_pointer",
        "Resolve a pointer id to a focused context slice",
    ),
    (
        "cognition_context_list_pointers",
        "List ranked context pointers (bootstrap usually sufficient)",
    ),
    (
        "cognition_intent_resolve",
        "Resolve intent to capability + suggested feeds and component template",
    ),
    (
        "cognition_feed_subscribe",
        "Bind feed ids on a custom-surface component",
    ),
    (
        "cognition_feed_publish",
        "Publish a bounded feed event to subscribed components",
    ),
    (
        "cognition_layout_get",
        "Read stack layout tree for a custom surface main body",
    ),
    (
        "cognition_layout_apply",
        "Apply vstack/hstack/grid layout to custom surface main body",
    ),
    (
        "cognition_layout_reset",
        "Clear layoutRoot to implicit vertical stack",
    ),
    (
        "cognition_environment_patch",
        "Incremental environment ops — new custom surfaces go live; preset rewrites propose",
    ),
    (
        "cognition_custom_view_doctor",
        "Diagnose custom surfaces — nav, feeds, recurring bindings, mismatches",
    ),
    (
        "cognition_custom_view_compose",
        "One-shot custom view + HTML + feeds + layout + recurring poll",
    ),
    ("cognition_turn_worker_status", "Pending worker status"),
];
