//! Phase 0 contract snapshots and cross-surface catalog audits.
//!
//! This is test-only until the typed catalog from Phase 1 becomes the runtime
//! authority. Keeping the baseline out of production avoids creating a second
//! registration path while still freezing every current model-visible shape.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use genai::chat::Tool;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use stasis::application::orchestration::tool_registry::{InMemoryToolRegistry, ToolRegistry};
use stasis::prelude::RuntimeBackend;

use crate::agent_runtime::turn_worker::{
    TurnWorkerIntent, allowed_tool_names_for_intent, host_bus_tool_names, tool_allowed,
};
use crate::tool_bootstrap::{
    HOST_BOOTSTRAP_TOOLS, ToolDomainCatalogEntry, WORKER_BOOTSTRAP_TOOLS, host_tool_domain_catalog,
    worker_tool_domain_catalog,
};

const BASELINE_FORMAT_VERSION: u32 = 1;
const BASELINE_PATH: &str = "tests/fixtures/first_party_tool_contracts.json";
const FOOTPRINT_BASELINE_PATH: &str = "tests/fixtures/first_party_tool_footprints.json";
const FOOTPRINT_BASELINE: &str = include_str!("../tests/fixtures/first_party_tool_footprints.json");
const LARGEST_TOOL_COUNT: usize = 8;

/// Static policy references that intentionally resolve outside first-party
/// assembled registries. There are none today; keeping the classification
/// explicit prevents future exceptions from becoming silent strings.
const EXTERNAL_OR_CONDITIONAL_POLICY_REFERENCES: &[(&str, &str)] = &[];

const HOST_BOOTSTRAP_AUTHORIZATION_EXCEPTIONS: &[(&str, &str)] = &[
    (
        "cognition_ui_build",
        "advertised for handoff, but execution remains Workshop-scoped",
    ),
    (
        "cognition_ui_present",
        "advertised for handoff, but execution remains Workshop-scoped",
    ),
    (
        "cognition_ui_scene",
        "advertised for handoff, but execution remains Workshop-scoped",
    ),
];

const WORKER_BOOTSTRAP_AUTHORIZATION_EXCEPTIONS: &[(&str, &str)] = &[];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ContractBaseline {
    format_version: u32,
    contracts: Vec<ContractSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ContractSnapshot {
    registry: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    input_schema: Option<Value>,
    output_schema_present: bool,
    placements: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ToolFootprintBaseline {
    estimator: String,
    surfaces: Vec<ToolSurfaceFootprintSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ToolSurfaceFootprintSnapshot {
    id: String,
    tool_count: usize,
    total_chars: usize,
    tokens_estimate: u32,
    tools_over_1000_chars: usize,
    largest_tools_share_bps: u32,
    largest_tools: Vec<ToolFootprintEntrySnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ToolFootprintEntrySnapshot {
    name: String,
    chars: usize,
    tokens_estimate: u32,
}

fn snapshot_tool_surface(id: &str, tools: &[Tool]) -> ToolSurfaceFootprintSnapshot {
    let footprint =
        crate::agent_runtime::context_usage::measure_tool_schema_footprint_for_tools(tools);
    let largest_chars = footprint
        .tools
        .iter()
        .take(LARGEST_TOOL_COUNT)
        .map(|tool| tool.chars)
        .sum::<usize>();
    let largest_tools_share_bps = if footprint.total_chars == 0 {
        0
    } else {
        u32::try_from(
            largest_chars
                .checked_div(footprint.total_chars)
                .and_then(|quotient| quotient.checked_mul(10_000))
                .unwrap_or(usize::MAX),
        )
        .unwrap_or(u32::MAX)
    };
    ToolSurfaceFootprintSnapshot {
        id: id.to_string(),
        tool_count: footprint.tool_count,
        total_chars: footprint.total_chars,
        tokens_estimate: crate::agent_runtime::context_usage::chars_to_tokens(
            footprint.total_chars,
        ),
        tools_over_1000_chars: footprint
            .tools
            .iter()
            .filter(|tool| tool.chars >= 1_000)
            .count(),
        largest_tools_share_bps,
        largest_tools: footprint
            .tools
            .into_iter()
            .take(LARGEST_TOOL_COUNT)
            .map(|tool| ToolFootprintEntrySnapshot {
                name: tool.name,
                chars: tool.chars,
                tokens_estimate: tool.tokens_estimate,
            })
            .collect(),
    }
}

fn filtered_surface(
    general_tools: &[Tool],
    allowlist: &HashSet<String>,
    supports_ui_artifacts: bool,
    supports_browser_host: bool,
) -> Vec<Tool> {
    general_tools
        .iter()
        .filter(|tool| {
            crate::public_api::is_public_api_tool(tool.name.as_str())
                || tool_allowed(tool.name.as_str(), allowlist)
        })
        .filter(|tool| {
            supports_ui_artifacts
                || !matches!(
                    tool.name.as_str(),
                    crate::ui_present_tools::COGNITION_UI_PRESENT
                        | crate::ui_scene_tools::COGNITION_UI_SCENE
                        | crate::ui_build_tools::COGNITION_UI_BUILD
                )
        })
        .filter(|tool| {
            supports_browser_host
                || !crate::browser_tools::BROWSER_COGNITION_TOOLS.contains(&tool.name.as_str())
        })
        .cloned()
        .collect()
}

fn tool_footprint_baseline(
    general_tools: &[Tool],
    coder_setup_tools: &[Tool],
    coder_work_tools: &[Tool],
) -> ToolFootprintBaseline {
    let host_bus = crate::agent_runtime::turn_worker::host_bus_tool_names();
    let worker_general = allowed_tool_names_for_intent(TurnWorkerIntent::General);
    let worker_research = allowed_tool_names_for_intent(TurnWorkerIntent::Research);
    let worker_memory = allowed_tool_names_for_intent(TurnWorkerIntent::MemoryContext);

    ToolFootprintBaseline {
        estimator: crate::agent_runtime::context_usage::ESTIMATOR_LABEL.to_string(),
        surfaces: vec![
            snapshot_tool_surface("general", general_tools),
            snapshot_tool_surface(
                "host_bus_home",
                &filtered_surface(general_tools, &host_bus, true, true),
            ),
            snapshot_tool_surface(
                "host_bus_thin",
                &filtered_surface(general_tools, &host_bus, false, false),
            ),
            snapshot_tool_surface(
                "workshop_general_bound",
                &filtered_surface(general_tools, &worker_general, true, true),
            ),
            snapshot_tool_surface(
                "worker_general",
                &filtered_surface(general_tools, &worker_general, false, false),
            ),
            snapshot_tool_surface(
                "worker_research",
                &filtered_surface(general_tools, &worker_research, false, false),
            ),
            snapshot_tool_surface(
                "worker_memory",
                &filtered_surface(general_tools, &worker_memory, false, false),
            ),
            snapshot_tool_surface("coder_setup", coder_setup_tools),
            snapshot_tool_surface("coder_work", coder_work_tools),
        ],
    }
}

fn snapshot_tool(
    registry: &str,
    tool: Tool,
    title: Option<String>,
    mut placements: Vec<String>,
    output_schema_present: bool,
) -> ContractSnapshot {
    placements.sort();
    placements.dedup();
    ContractSnapshot {
        registry: registry.to_string(),
        name: tool.name.as_str().to_string(),
        title,
        description: tool
            .description
            .map(|description| description.trim().replace("\r\n", "\n").replace('\r', "\n")),
        input_schema: tool.schema.map(|schema| normalize_json(schema, None)),
        output_schema_present,
        placements,
    }
}

fn normalize_json(value: Value, parent_key: Option<&str>) -> Value {
    match value {
        Value::Object(object) => {
            let sorted = object.into_iter().collect::<BTreeMap<_, _>>();
            Value::Object(
                sorted
                    .into_iter()
                    .map(|(key, value)| {
                        let value = normalize_json(value, Some(key.as_str()));
                        (key, value)
                    })
                    .collect::<Map<_, _>>(),
            )
        }
        Value::Array(values) => {
            let mut values = values
                .into_iter()
                .map(|value| normalize_json(value, None))
                .collect::<Vec<_>>();
            if matches!(parent_key, Some("required" | "enum")) {
                values.sort_by_key(|value| serde_json::to_string(value).unwrap_or_default());
                values.dedup();
            }
            Value::Array(values)
        }
        scalar => scalar,
    }
}

fn catalog_placements(
    catalog: &crate::typed_tools::ToolCatalog,
    tool_name: &str,
    mode: Option<crate::typed_tools::ToolModeId>,
) -> Vec<String> {
    catalog
        .resolve_wire_id(tool_name)
        .ok()
        .and_then(|id| catalog.get(id))
        .into_iter()
        .flat_map(|entry| entry.placement.exposures.iter())
        .filter(|exposure| {
            mode.map_or(
                exposure.mode != crate::tool_catalog::CODER_MODE_ID,
                |mode| exposure.mode == mode,
            )
        })
        .map(|exposure| exposure.label())
        .collect()
}

fn worker_intents() -> [(TurnWorkerIntent, &'static str); 4] {
    [
        (TurnWorkerIntent::General, "general"),
        (TurnWorkerIntent::Research, "research"),
        (TurnWorkerIntent::MemoryContext, "memory_context"),
        (
            TurnWorkerIntent::MemoryAvecCalibrate,
            "memory_avec_calibrate",
        ),
    ]
}

async fn assembled_contract_baseline() -> (ContractBaseline, ToolFootprintBaseline) {
    let (event_tx, _event_rx) = tokio::sync::mpsc::channel(8);
    let runtime = crate::tools::build_tui_runtime(
        RuntimeBackend::InMemory,
        None,
        None,
        None,
        Vec::new(),
        "contract-baseline",
        true,
        event_tx,
    )
    .await
    .expect("assemble in-memory first-party tool registry");
    let registered_tools = runtime
        .tool_registry
        .list_tools()
        .await
        .expect("list assembled first-party tools");
    let general_tools = crate::tool_catalog::compile_general_surface(&runtime.tool_catalog)
        .expect("compile General surface from assembled catalog");
    assert_catalog_matches_registered_surface(&general_tools, &registered_tools);

    audit_static_inventory(&general_tools);
    audit_typed_contract_inventory(&runtime.tool_catalog);
    audit_policy_references(&runtime.tool_catalog, &general_tools);

    let coder_setup_contracts =
        crate::agent_runtime::coder_setup_tools::contract_tool_definitions();
    let mut coder_setup_surface = coder_setup_contracts.clone();
    coder_setup_surface.extend(
        general_tools
            .iter()
            .find(|tool| tool.name.as_str() == crate::public_api::COGNITION_TURN)
            .cloned(),
    );
    let coder_work_tools =
        crate::agent_runtime::coder_tools::contract_projected_tools(&runtime.tool_catalog);
    let footprint =
        tool_footprint_baseline(&general_tools, &coder_setup_surface, &coder_work_tools);

    let mut contracts = general_tools
        .iter()
        .cloned()
        .map(|tool| {
            let placements = catalog_placements(&runtime.tool_catalog, tool.name.as_str(), None);
            let output_schema_present =
                catalog_output_schema_present(&runtime.tool_catalog, tool.name.as_str());
            snapshot_tool(
                "general_runtime",
                tool,
                None,
                placements,
                output_schema_present,
            )
        })
        .collect::<Vec<_>>();

    contracts.extend(coder_setup_contracts.into_iter().map(|tool| {
        snapshot_tool(
            "coder_setup",
            tool,
            None,
            vec!["coder:unbound".to_string()],
            true,
        )
    }));

    contracts.extend(coder_work_tools.into_iter().map(|tool| {
        let placements = crate::agent_runtime::coder_tools::contract_placement_labels(
            &runtime.tool_catalog,
            tool.name.as_str(),
        );
        let output_schema_present =
            catalog_output_schema_present(&runtime.tool_catalog, tool.name.as_str());
        snapshot_tool("coder_bound", tool, None, placements, output_schema_present)
    }));

    contracts.extend(medousa_mcp_server::space_tools().into_iter().map(|spec| {
        let tool = Tool::new(spec.name)
            .with_description(spec.description)
            .with_schema(spec.input_schema);
        snapshot_tool(
            "mcp_space_server",
            tool,
            Some(spec.title.to_string()),
            vec!["external_agent:read_only".to_string()],
            false,
        )
    }));

    contracts.sort_by(|left, right| {
        (left.registry.as_str(), left.name.as_str())
            .cmp(&(right.registry.as_str(), right.name.as_str()))
    });
    for pair in contracts.windows(2) {
        assert_ne!(
            (&pair[0].registry, &pair[0].name),
            (&pair[1].registry, &pair[1].name),
            "duplicate tool id in one first-party registry"
        );
    }
    (
        ContractBaseline {
            format_version: BASELINE_FORMAT_VERSION,
            contracts,
        },
        footprint,
    )
}

fn assert_catalog_matches_registered_surface(catalog_tools: &[Tool], registered_tools: &[Tool]) {
    let normalize = |tools: &[Tool]| {
        tools
            .iter()
            .map(|tool| {
                (
                    tool.name.as_str().to_string(),
                    tool.description.clone(),
                    tool.schema
                        .clone()
                        .map(|schema| normalize_json(schema, None).to_string()),
                )
            })
            .collect::<BTreeSet<_>>()
    };
    assert_eq!(
        normalize(catalog_tools),
        normalize(registered_tools),
        "the assembled typed catalog must exactly match the registered General surface"
    );
}

fn catalog_output_schema_present(
    catalog: &crate::typed_tools::ToolCatalog,
    tool_name: &str,
) -> bool {
    catalog
        .resolve_wire_id(tool_name)
        .ok()
        .and_then(|id| catalog.get(id))
        .is_some_and(|entry| entry.contract.output_schema.is_some())
}

fn audit_static_inventory(general_tools: &[Tool]) {
    let mut assembled = general_tools
        .iter()
        .map(|tool| tool.name.as_str())
        .collect::<BTreeSet<_>>();
    let setup = crate::agent_runtime::coder_setup_tools::contract_tool_definitions();
    assembled.extend(setup.iter().map(|tool| tool.name.as_str()));
    let declared = crate::tool_names::registered_cognition_tools().collect::<BTreeSet<_>>();
    assert_eq!(
        assembled, declared,
        "manual/typed inventory must exactly match General plus Coder setup assembly"
    );
}

fn audit_typed_contract_inventory(catalog: &crate::typed_tools::ToolCatalog) {
    let mut actual = catalog
        .entries()
        .filter(|entry| entry.contract.kind == crate::typed_tools::RegisteredToolKind::Typed)
        .map(|entry| entry.id.as_str())
        .collect::<BTreeSet<_>>();
    actual.extend(
        crate::agent_runtime::coder_setup_tools::typed_contract_ids()
            .into_iter()
            .map(|id| id.as_str()),
    );
    let declared = crate::tool_names::TYPED_TOOL_CONTRACTS
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    assert_eq!(
        actual, declared,
        "typed tool inventory must exactly match catalog registrations"
    );
}

fn audit_policy_references(catalog: &crate::typed_tools::ToolCatalog, general_tools: &[Tool]) {
    let mut actual = general_tools
        .iter()
        .map(|tool| tool.name.as_str().to_string())
        .collect::<HashSet<_>>();
    actual.extend(
        crate::agent_runtime::coder_setup_tools::contract_tool_definitions()
            .into_iter()
            .map(|tool| tool.name.as_str().to_string()),
    );
    actual.extend(
        crate::agent_runtime::coder_tools::contract_projected_tools(catalog)
            .into_iter()
            .map(|tool| tool.name.as_str().to_string()),
    );
    let exceptions = EXTERNAL_OR_CONDITIONAL_POLICY_REFERENCES
        .iter()
        .map(|(name, _classification)| (*name).to_string())
        .collect::<HashSet<_>>();

    for (group, names) in policy_reference_groups() {
        let unresolved = names
            .into_iter()
            .map(|name| crate::tool_aliases::sanitize_tool_advertised_name(&name))
            .filter(|name| !actual.contains(name) && !exceptions.contains(name))
            .collect::<BTreeSet<_>>();
        assert!(
            unresolved.is_empty(),
            "policy group '{group}' references unassembled tools: {unresolved:?}"
        );
    }
}

fn policy_reference_groups() -> BTreeMap<String, BTreeSet<String>> {
    let mut groups = BTreeMap::new();
    insert_group(&mut groups, "bootstrap:host", HOST_BOOTSTRAP_TOOLS);
    insert_group(&mut groups, "bootstrap:worker", WORKER_BOOTSTRAP_TOOLS);
    for entry in host_tool_domain_catalog() {
        insert_group(
            &mut groups,
            format!("domain:host:{}", entry.domain),
            entry.tools,
        );
    }
    for entry in worker_tool_domain_catalog() {
        insert_group(
            &mut groups,
            format!("domain:worker:{}", entry.domain),
            entry.tools,
        );
    }
    groups.insert(
        "policy:host".to_string(),
        host_bus_tool_names().into_iter().collect(),
    );
    for (intent, label) in worker_intents() {
        groups.insert(
            format!("policy:worker:{label}"),
            allowed_tool_names_for_intent(intent).into_iter().collect(),
        );
    }
    groups.insert(
        "policy:coder".to_string(),
        crate::agent_runtime::coder_tools::contract_policy_references()
            .into_iter()
            .collect(),
    );
    groups
}

fn insert_group(
    groups: &mut BTreeMap<String, BTreeSet<String>>,
    name: impl Into<String>,
    tools: &[&str],
) {
    groups.insert(
        name.into(),
        tools.iter().map(|tool| (*tool).to_string()).collect(),
    );
}

fn assert_unique_domains(lane: &str, catalog: &[ToolDomainCatalogEntry]) {
    let mut domains = HashSet::new();
    for entry in catalog {
        assert!(
            domains.insert(entry.domain),
            "duplicate {lane} tool domain: {}",
            entry.domain
        );
        let tools = entry.tools.iter().copied().collect::<HashSet<_>>();
        assert_eq!(
            tools.len(),
            entry.tools.len(),
            "duplicate tool id in {lane} domain '{}'",
            entry.domain
        );
    }
}

#[tokio::test]
async fn assembled_first_party_contracts_match_baseline() {
    assert_unique_domains("host", host_tool_domain_catalog());
    assert_unique_domains("worker", worker_tool_domain_catalog());
    assert_bootstrap_authorized(
        "host",
        HOST_BOOTSTRAP_TOOLS,
        &host_bus_tool_names(),
        HOST_BOOTSTRAP_AUTHORIZATION_EXCEPTIONS,
    );
    assert_bootstrap_authorized(
        "worker",
        WORKER_BOOTSTRAP_TOOLS,
        &allowed_tool_names_for_intent(TurnWorkerIntent::General),
        WORKER_BOOTSTRAP_AUTHORIZATION_EXCEPTIONS,
    );

    let (baseline, footprint) = assembled_contract_baseline().await;
    let rendered = format!(
        "{}\n",
        serde_json::to_string_pretty(&baseline).expect("serialize tool contract baseline")
    );
    if std::env::var_os("MEDOUSA_UPDATE_TOOL_CONTRACT_BASELINE").is_some() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        std::fs::write(manifest_dir.join(BASELINE_PATH), rendered)
            .expect("write updated tool contract baseline");
        let footprint_rendered = format!(
            "{}\n",
            serde_json::to_string_pretty(&footprint).expect("serialize tool footprint baseline")
        );
        std::fs::write(
            manifest_dir.join(FOOTPRINT_BASELINE_PATH),
            footprint_rendered,
        )
        .expect("write updated tool footprint baseline");
        return;
    }

    let baseline = include_str!("../tests/fixtures/first_party_tool_contracts.json");
    if baseline != rendered {
        let first_mismatch = baseline
            .bytes()
            .zip(rendered.bytes())
            .position(|(left, right)| left != right)
            .unwrap_or_else(|| baseline.len().min(rendered.len()));
        let context_start = first_mismatch.saturating_sub(120);
        let context_end = (first_mismatch + 160).min(baseline.len().min(rendered.len()));
        eprintln!(
            "baseline context: {:?}",
            baseline.get(context_start..context_end).unwrap_or_default()
        );
        eprintln!(
            "rendered context: {:?}",
            rendered.get(context_start..context_end).unwrap_or_default()
        );
        panic!(
            "first-party tool contract drifted at byte {first_mismatch} (baseline {} bytes, rendered {} bytes); review the model-visible change, then regenerate with MEDOUSA_UPDATE_TOOL_CONTRACT_BASELINE=1 cargo test -p medousa assembled_first_party_contracts_match_baseline",
            baseline.len(),
            rendered.len()
        );
    }
    let expected: ToolFootprintBaseline =
        serde_json::from_str(FOOTPRINT_BASELINE).expect("parse tool footprint baseline");
    assert_eq!(
        footprint,
        expected,
        "tool surface footprint drifted; current baseline:\n{}",
        serde_json::to_string_pretty(&footprint).expect("serialize tool footprint baseline")
    );
}

fn assert_bootstrap_authorized(
    lane: &str,
    bootstrap: &[&str],
    authorized: &HashSet<String>,
    exceptions: &[(&str, &str)],
) {
    for (tool, classification) in exceptions {
        assert!(!classification.trim().is_empty());
        assert!(
            bootstrap.contains(tool),
            "stale {lane} bootstrap exception for {tool}"
        );
        assert!(
            !tool_allowed(tool, authorized),
            "remove resolved {lane} bootstrap exception for {tool}"
        );
    }
    for tool in bootstrap {
        assert!(
            tool_allowed(tool, authorized)
                || exceptions.iter().any(|(exception, _)| exception == tool),
            "{lane} bootstrap tool is filtered by its lane policy without classification: {tool}"
        );
    }
}

#[test]
fn normalized_snapshot_detects_contract_and_placement_drift() {
    let base_tool = Tool::new("sample_tool")
        .with_description("Stable description")
        .with_schema(json!({
            "type": "object",
            "properties": {
                "mode": { "type": "string", "enum": ["read", "write"] },
                "path": { "type": "string" }
            },
            "required": ["path"]
        }));
    let baseline = snapshot_tool(
        "sample",
        base_tool.clone(),
        None,
        vec!["general:authorized".to_string()],
        false,
    );

    let mutations = [
        snapshot_tool(
            "sample",
            Tool::new("renamed_tool")
                .with_description("Stable description")
                .with_schema(json!({
                    "type": "object",
                    "properties": {
                        "mode": { "type": "string", "enum": ["read", "write"] },
                        "path": { "type": "string" }
                    },
                    "required": ["path"]
                })),
            None,
            vec!["general:authorized".to_string()],
            false,
        ),
        snapshot_tool(
            "sample",
            base_tool.clone().with_description("Changed description"),
            None,
            vec!["general:authorized".to_string()],
            false,
        ),
        snapshot_tool(
            "sample",
            base_tool.clone().with_schema(json!({
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": []
            })),
            None,
            vec!["general:authorized".to_string()],
            false,
        ),
        snapshot_tool(
            "sample",
            base_tool.clone().with_schema(json!({
                "type": "object",
                "properties": {
                    "mode": { "type": "string", "enum": ["read", "execute"] },
                    "path": { "type": "string" }
                },
                "required": ["path"]
            })),
            None,
            vec!["general:authorized".to_string()],
            false,
        ),
        snapshot_tool(
            "sample",
            base_tool,
            None,
            vec!["workshop:authorized:general".to_string()],
            false,
        ),
    ];
    for mutation in mutations {
        assert_ne!(baseline, mutation);
    }
    let mut output_schema_mutation = baseline.clone();
    output_schema_mutation.output_schema_present = true;
    assert_ne!(baseline, output_schema_mutation);
    let mut registry_mutation = baseline.clone();
    registry_mutation.registry = "other_surface".to_string();
    assert_ne!(baseline, registry_mutation);
}

#[tokio::test]
async fn legacy_invocation_fixture_covers_required_input_and_aliases() {
    let registry = InMemoryToolRegistry::default();
    let scheduler =
        std::sync::Arc::new(crate::agent_runtime::turn_worker::TurnWorkerScheduler::new(
            crate::agent_runtime::turn_worker::turn_worker_store(),
        ));
    registry
        .register_tool(crate::turn_api::CognitionTurnTool::new(
            scheduler,
            "fixture".to_string(),
            crate::agent_runtime::execution_context::TurnScopeAccess::default(),
        ))
        .expect("register fixture tool");
    let accepted = registry
        .invoke_tool(
            crate::public_api::COGNITION_TURN,
            json!({ "action": "turn.update_user", "message": "Still working" }),
        )
        .await
        .expect("valid update fixture");
    assert_eq!(accepted["message"], "Still working");
    let rejected = registry
        .invoke_tool(
            crate::public_api::COGNITION_TURN,
            json!({ "action": "turn.update_user" }),
        )
        .await
        .expect_err("missing required message must remain invalid");
    assert!(rejected.to_string().contains("message"));
}

#[test]
fn legacy_invocation_fixture_covers_defaults_enums_nested_inputs_and_bounds() {
    use crate::agent_runtime::coder_activity::CoderAgentIdentity;
    use crate::agent_runtime::coder_memory::{
        CoderMemoryScope, build_commit, overview_limit, parse_recall_query,
    };

    assert_eq!(overview_limit(&json!({})), 10);
    assert_eq!(overview_limit(&json!({ "limit": 0 })), 1);
    assert_eq!(overview_limit(&json!({ "limit": 999 })), 20);

    let recall = parse_recall_query(&json!({ "query": "next work", "limit": 999 }))
        .expect("valid bounded recall fixture");
    assert_eq!(recall.limit, 12);
    let bad_enum = parse_recall_query(&json!({
        "query": "next work",
        "kind": "private_thought"
    }))
    .expect_err("unknown memory kind must remain invalid");
    assert!(bad_enum.to_string().contains("unknown Coder memory kind"));

    let scope = CoderMemoryScope::for_environment("repo", "work", "branch", 1);
    let identity = CoderAgentIdentity::for_turn(&scope.session_id, "turn", "attempt");
    build_commit(
        &json!({
            "kind": "decision",
            "summary": "Keep the typed boundary",
            "relations": [{
                "rel": "supports",
                "target": "engineering:decision:1",
                "confidence": 0.9
            }]
        }),
        &scope,
        &identity,
        "0123456789abcdef",
    )
    .expect("valid nested relation fixture");
    let bad_nested = build_commit(
        &json!({
            "kind": "decision",
            "summary": "Keep the typed boundary",
            "relations": [{
                "rel": "supports",
                "target": "engineering:decision:1",
                "confidence": 1.5
            }]
        }),
        &scope,
        &identity,
        "0123456789abcdef",
    )
    .expect_err("out-of-range nested confidence must remain invalid");
    assert!(bad_nested.to_string().contains("between 0 and 1"));
}
