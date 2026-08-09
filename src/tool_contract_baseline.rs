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

/// Stasis 0.8 does not carry `StasisTool::output_schema` into `genai::Tool`.
/// No current manual Medousa implementation overrides it, so the Phase 0
/// declaration is empty. Phase 1 moves this bit into the typed contract.
const OUTPUT_SCHEMA_CONTRACTS: &[(&str, &str)] = &[];

/// Static policy references that intentionally resolve outside first-party
/// assembled registries. There are none today; keeping the classification
/// explicit prevents future exceptions from becoming silent strings.
const EXTERNAL_OR_CONDITIONAL_POLICY_REFERENCES: &[(&str, &str)] = &[];

const HOST_BOOTSTRAP_AUTHORIZATION_EXCEPTIONS: &[(&str, &str)] = &[
    (
        "cognition_artifact_grep",
        "advertised for handoff, but execution remains Workshop-scoped",
    ),
    (
        "cognition_artifact_list",
        "advertised for handoff, but execution remains Workshop-scoped",
    ),
    (
        "cognition_artifact_read",
        "advertised for handoff, but execution remains Workshop-scoped",
    ),
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
    (
        "cognition_vault_grep",
        "advertised for handoff, but execution remains Workshop-scoped",
    ),
];

const WORKER_BOOTSTRAP_AUTHORIZATION_EXCEPTIONS: &[(&str, &str)] = &[(
    "cognition_turn_begin_work",
    "Workshop cannot recursively enter another bound Workshop",
)];

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

fn snapshot_tool(
    registry: &str,
    tool: Tool,
    title: Option<String>,
    mut placements: Vec<String>,
) -> ContractSnapshot {
    placements.sort();
    placements.dedup();
    let output_schema_present = OUTPUT_SCHEMA_CONTRACTS.contains(&(registry, tool.name.as_str()));
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

fn general_placements(tool_name: &str) -> Vec<String> {
    let mut placements = Vec::new();
    if HOST_BOOTSTRAP_TOOLS.contains(&tool_name) {
        placements.push("general:bootstrap".to_string());
    }
    if WORKER_BOOTSTRAP_TOOLS.contains(&tool_name) {
        placements.push("workshop:bootstrap".to_string());
    }

    let host = host_bus_tool_names();
    if tool_allowed(tool_name, &host) {
        placements.push("general:authorized".to_string());
    }
    for (intent, label) in worker_intents() {
        let allowed = allowed_tool_names_for_intent(intent);
        if tool_allowed(tool_name, &allowed) {
            placements.push(format!("workshop:authorized:{label}"));
        }
    }
    for entry in host_tool_domain_catalog() {
        if entry.tools.contains(&tool_name) {
            placements.push(format!("general:domain:{}", entry.domain));
        }
    }
    for entry in worker_tool_domain_catalog() {
        if entry.tools.contains(&tool_name) {
            placements.push(format!("workshop:domain:{}", entry.domain));
        }
    }
    placements
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

async fn assembled_contract_baseline() -> ContractBaseline {
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
    let general_tools = runtime
        .tool_registry
        .list_tools()
        .await
        .expect("list assembled first-party tools");

    audit_static_inventory(&general_tools);
    audit_policy_references(&general_tools);

    let mut contracts = general_tools
        .iter()
        .cloned()
        .map(|tool| {
            let placements = general_placements(tool.name.as_str());
            snapshot_tool("general_runtime", tool, None, placements)
        })
        .collect::<Vec<_>>();

    contracts.extend(
        crate::agent_runtime::coder_setup_tools::contract_tool_definitions()
            .into_iter()
            .map(|tool| {
                snapshot_tool("coder_setup", tool, None, vec!["coder:unbound".to_string()])
            }),
    );

    contracts.extend(
        crate::agent_runtime::coder_tools::contract_projected_tools(general_tools)
            .into_iter()
            .map(|tool| {
                let placements = crate::agent_runtime::coder_tools::contract_placement_labels(
                    tool.name.as_str(),
                );
                snapshot_tool("coder_bound", tool, None, placements)
            }),
    );

    contracts.extend(medousa_mcp_server::space_tools().into_iter().map(|spec| {
        let tool = Tool::new(spec.name)
            .with_description(spec.description)
            .with_schema(spec.input_schema);
        snapshot_tool(
            "mcp_space_server",
            tool,
            Some(spec.title.to_string()),
            vec!["external_agent:read_only".to_string()],
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
    ContractBaseline {
        format_version: BASELINE_FORMAT_VERSION,
        contracts,
    }
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

fn audit_policy_references(general_tools: &[Tool]) {
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
        crate::agent_runtime::coder_tools::contract_projected_tools(general_tools.to_vec())
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

    let baseline = assembled_contract_baseline().await;
    let rendered = format!(
        "{}\n",
        serde_json::to_string_pretty(&baseline).expect("serialize tool contract baseline")
    );
    if std::env::var_os("MEDOUSA_UPDATE_TOOL_CONTRACT_BASELINE").is_some() {
        std::fs::write(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(BASELINE_PATH),
            rendered,
        )
        .expect("write updated tool contract baseline");
        return;
    }

    assert_eq!(
        include_str!("../tests/fixtures/first_party_tool_contracts.json"),
        rendered,
        "first-party tool contract drifted; review the model-visible change, then regenerate with MEDOUSA_UPDATE_TOOL_CONTRACT_BASELINE=1 cargo test -p medousa assembled_first_party_contracts_match_baseline"
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
        ),
        snapshot_tool(
            "sample",
            base_tool.clone().with_description("Changed description"),
            None,
            vec!["general:authorized".to_string()],
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
        ),
        snapshot_tool(
            "sample",
            base_tool,
            None,
            vec!["workshop:authorized:general".to_string()],
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
    registry
        .register_tool(crate::turn_control_tools::CognitionTurnUpdateUserTool)
        .expect("register fixture tool");
    let accepted = registry
        .invoke_tool(
            crate::turn_control_tools::COGNITION_TURN_UPDATE_USER,
            json!({ "message": "Still working" }),
        )
        .await
        .expect("valid update fixture");
    assert_eq!(accepted["message"], "Still working");
    let rejected = registry
        .invoke_tool(
            crate::turn_control_tools::COGNITION_TURN_UPDATE_USER,
            json!({}),
        )
        .await
        .expect_err("missing required message must remain invalid");
    assert!(
        rejected
            .to_string()
            .contains("missing required field 'message'")
    );
    assert!(crate::turn_control_tools::is_update_user_tool_name(
        crate::turn_control_tools::COGNITION_TURN_UPDATE_USER_DOTTED
    ));
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
