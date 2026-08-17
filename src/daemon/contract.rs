//! Map H01 `DeclaredRouter` inventories onto H10 `OperationSpec` values.

use axum::http::Method;

use medousa_api_contract::{
    Audience, ContractRegistry, FeatureProfile, HttpMethod, OperationSpec, ResponseSpec,
    SchemaRef, Stability, StreamSpec, stable_operation_id,
};

use crate::daemon::route_policy::{
    RateLimitClass, RouteGroup, RouteInventory, RoutePolicy,
};
use crate::daemon::router::build_declared_route_inventory;
use crate::request_principal::Capability;

pub type ContractRouter<S = ()> = crate::daemon::route_policy::DeclaredRouter<S>;

pub fn policy_from_operation(spec: &OperationSpec) -> RoutePolicy {
    spec.validate().expect("operation spec must be valid");
    let method = Method::from_bytes(spec.method.as_str().as_bytes()).expect("http method");
    RoutePolicy {
        method,
        path: intern_path(&spec.path),
        group: trust_group_from_spec(spec),
        required_capability: spec
            .capabilities
            .first()
            .and_then(|name| parse_capability(name)),
        bootstrap_public: spec.bootstrap_public,
        browser_policy: browser_policy_from_spec(&spec.browser_policy),
        body_limit: spec.body_limit,
        rate_limit_class: rate_class_from_spec(&spec.rate_limit_class),
    }
}

pub fn operation_from_policy(policy: &RoutePolicy, profile: FeatureProfile) -> OperationSpec {
    let method = HttpMethod::parse(policy.method.as_str()).expect("declared methods are HTTP");
    let streaming = matches!(policy.rate_limit_class, RateLimitClass::Stream);
    let operation_id = stable_operation_id(policy.method.as_str(), policy.path);
    let schema = schema_for_operation(&operation_id, streaming);
    let mut spec = OperationSpec {
        operation_id,
        stability: Stability::Stable,
        feature_profile: profile,
        audience: if matches!(
            policy.browser_policy,
            crate::daemon::route_policy::BrowserPolicy::Public
        ) {
            Audience::PublicSdk
        } else {
            Audience::NativeOnly
        },
        method,
        path: policy.path.to_string(),
        parameters: Vec::new(),
        request_body: None,
        responses: vec![ResponseSpec {
            status: 200,
            media_type: if streaming {
                "text/event-stream".into()
            } else {
                "application/json".into()
            },
            schema: schema.clone(),
        }],
        error_codes: default_error_codes(policy.bootstrap_public),
        trust_group: trust_group_name(policy.group).into(),
        credential_scheme: if policy.bootstrap_public {
            "none".into()
        } else if matches!(policy.group, RouteGroup::Preview) {
            "preview_token".into()
        } else {
            "bearer".into()
        },
        capabilities: policy
            .required_capability
            .map(Capability::as_str)
            .map(|name| vec![name.to_string()])
            .unwrap_or_default(),
        browser_policy: browser_policy_name(policy.browser_policy).into(),
        body_limit: policy.body_limit,
        rate_limit_class: rate_class_name(policy.rate_limit_class).into(),
        bootstrap_public: policy.bootstrap_public,
        stream: streaming.then(|| StreamSpec::json_events(schema)),
        deprecation: None,
    };
    spec = spec.with_path_parameters();
    spec
}

pub fn registry_from_inventory(
    inventory: &RouteInventory,
    pairing_only: &RouteInventory,
) -> ContractRegistry {
    let pairing_keys: std::collections::HashSet<(String, String)> = pairing_only
        .entries()
        .map(|entry| (entry.method.clone(), entry.path.to_string()))
        .collect();
    let mut registry = ContractRegistry::new();
    for policy in inventory.policies() {
        let profile = if matches!(policy.group, RouteGroup::Preview) {
            FeatureProfile::Preview
        } else if pairing_keys.contains(&(policy.method.as_str().to_string(), policy.path.to_string()))
        {
            FeatureProfile::Pairing
        } else {
            FeatureProfile::Core
        };
        registry
            .register(operation_from_policy(policy, profile))
            .expect("declared inventory must form a valid contract");
    }
    registry
}

/// H08 browser compatibility routes, mounted both unprefixed and under `/v1`.
/// These are not in the declared 361/373 inventory until they use `ContractRouter`.
pub const BROWSER_COMPATIBILITY_MOUNTS: &[(&str, &str)] = &[
    ("POST", "/clients/register"),
    ("GET", "/clients"),
    ("GET", "/clients/{client_id}/tools/next"),
    ("POST", "/clients/{client_id}/tools/{request_id}/result"),
    ("GET", "/browser/sessions/{session_id}"),
    ("POST", "/browser/sessions/{session_id}/complete"),
    ("POST", "/browser/sessions/{session_id}/complete-act"),
    ("POST", "/browser/sessions/{session_id}/resume"),
];

pub fn production_registry(pairing_enabled: bool) -> ContractRegistry {
    let core = build_declared_route_inventory(false);
    let all = build_declared_route_inventory(pairing_enabled);
    let pairing_only = pairing_only_inventory(&core, &all);
    registry_from_inventory(&all, &pairing_only)
}

fn pairing_only_inventory(core: &RouteInventory, all: &RouteInventory) -> RouteInventory {
    let core_keys: std::collections::HashSet<(String, &'static str)> = core
        .entries()
        .map(|entry| (entry.method.clone(), entry.path))
        .collect();
    let mut pairing = RouteInventory::default();
    for policy in all.policies() {
        if !core_keys.contains(&(policy.method.as_str().to_string(), policy.path)) {
            pairing
                .declare(policy.clone())
                .expect("pairing-only routes are unique");
        }
    }
    pairing
}

fn schema_for_operation(operation_id: &str, streaming: bool) -> SchemaRef {
    match operation_id {
        "liveness.get" => SchemaRef::named("HealthLiveness"),
        "health.get" => SchemaRef::named("HealthResponse"),
        "ingest.post" => SchemaRef::named("IngestResponse"),
        "interactive.turn.post" => SchemaRef::named("InteractiveTurnResponse"),
        "interactive.turn.by_turn_id.stream.get"
        | "agents.sessions.by_agent_session_id.stream.get" => {
            SchemaRef::named("TurnStreamEnvelopeV2")
        }
        _ if streaming => SchemaRef::named("TurnStreamEnvelopeV2"),
        other => SchemaRef::deferred(format!("{}Response", medousa_api_contract::const_name(other))),
    }
}

fn default_error_codes(bootstrap: bool) -> Vec<String> {
    let mut codes = vec![
        medousa_types::ERROR_INTERNAL_FAILURE.to_string(),
        medousa_types::ERROR_INVALID_PARAMETER.to_string(),
    ];
    if !bootstrap {
        codes.extend([
            medousa_types::ERROR_AUTHENTICATION_REQUIRED.to_string(),
            medousa_types::ERROR_INVALID_CREDENTIAL.to_string(),
            medousa_types::ERROR_FORBIDDEN.to_string(),
        ]);
    }
    codes
}

fn trust_group_name(group: RouteGroup) -> &'static str {
    match group {
        RouteGroup::Liveness => "liveness",
        RouteGroup::PairingCeremony => "pairing_ceremony",
        RouteGroup::Portal => "portal",
        RouteGroup::PeerExchange => "peer_exchange",
        RouteGroup::Administration => "administration",
        RouteGroup::Preview => "preview",
    }
}

fn trust_group_from_spec(spec: &OperationSpec) -> RouteGroup {
    match spec.trust_group.as_str() {
        "liveness" => RouteGroup::Liveness,
        "pairing_ceremony" => RouteGroup::PairingCeremony,
        "peer_exchange" => RouteGroup::PeerExchange,
        "administration" => RouteGroup::Administration,
        "preview" => RouteGroup::Preview,
        _ => RouteGroup::Portal,
    }
}

fn browser_policy_name(policy: crate::daemon::route_policy::BrowserPolicy) -> &'static str {
    match policy {
        crate::daemon::route_policy::BrowserPolicy::Public => "public",
        crate::daemon::route_policy::BrowserPolicy::NativeOnly => "native_only",
        crate::daemon::route_policy::BrowserPolicy::ExactOrigin => "exact_origin",
    }
}

fn browser_policy_from_spec(name: &str) -> crate::daemon::route_policy::BrowserPolicy {
    match name {
        "public" => crate::daemon::route_policy::BrowserPolicy::Public,
        "exact_origin" => crate::daemon::route_policy::BrowserPolicy::ExactOrigin,
        _ => crate::daemon::route_policy::BrowserPolicy::NativeOnly,
    }
}

fn rate_class_name(class: RateLimitClass) -> &'static str {
    match class {
        RateLimitClass::Liveness => "liveness",
        RateLimitClass::PairingCeremony => "pairing_ceremony",
        RateLimitClass::Read => "read",
        RateLimitClass::Mutation => "mutation",
        RateLimitClass::PeerExchange => "peer_exchange",
        RateLimitClass::Administration => "administration",
        RateLimitClass::Stream => "stream",
    }
}

fn rate_class_from_spec(name: &str) -> RateLimitClass {
    match name {
        "liveness" => RateLimitClass::Liveness,
        "pairing_ceremony" => RateLimitClass::PairingCeremony,
        "mutation" => RateLimitClass::Mutation,
        "peer_exchange" => RateLimitClass::PeerExchange,
        "administration" => RateLimitClass::Administration,
        "stream" => RateLimitClass::Stream,
        _ => RateLimitClass::Read,
    }
}

fn intern_path(path: &str) -> &'static str {
    Box::leak(path.to_owned().into_boxed_str())
}

fn parse_capability(name: &str) -> Option<Capability> {
    match name {
        "workshop.read" => Some(Capability::WorkshopRead),
        "workshop.interact" => Some(Capability::WorkshopInteract),
        "content.read" => Some(Capability::ContentRead),
        "content.write" => Some(Capability::ContentWrite),
        "workspace.write" => Some(Capability::WorkspaceWrite),
        "peer.exchange" => Some(Capability::PeerExchange),
        "profile.self" => Some(Capability::ProfileSelf),
        "admin.identity" => Some(Capability::AdminIdentity),
        "admin.runtime" => Some(Capability::AdminRuntime),
        "admin.execute" => Some(Capability::AdminExecute),
        "mcp.policy.evaluate" => Some(Capability::McpPolicyEvaluate),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::router::build_liveness_surface;
    use crate::daemon::route_policy::{
        BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RoutePolicy,
    };
    use crate::peer_scope::{DaemonAccessState, assemble_daemon_access_boundary_with_declared};
    use crate::request_principal::Capability;
    use axum::Router;
    use axum::extract::ConnectInfo;
    use std::net::SocketAddr;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use medousa_api_contract::{
        CompatibilityClass, DiscrepancyKind, HttpMethod, ListedRoute, discrepancy_report,
        diff_contracts, generate_artifacts, parse_manifest_yaml,
    };
    use tower::ServiceExt;

    #[test]
    fn production_profiles_match_declared_counts() {
        let without_pairing = production_registry(false);
        let with_pairing = production_registry(true);
        assert_eq!(without_pairing.len(), 361);
        assert_eq!(with_pairing.len(), 373);
        let artifacts = generate_artifacts(&with_pairing);
        let inventory: serde_json::Value =
            serde_json::from_str(&artifacts.route_inventory_json).unwrap();
        assert_eq!(inventory["operations"].as_array().unwrap().len(), 373);
        assert!(artifacts.openapi_json.contains("\"openapi\": \"3.2.0\""));
    }

    #[test]
    fn exact_set_equals_declared_inventory() {
        let registry = production_registry(true);
        let declared = build_declared_route_inventory(true);
        let mut declared_keys: Vec<_> = declared
            .entries()
            .map(|entry| format!("{} {}", entry.method, entry.path))
            .collect();
        let mut generated_keys: Vec<_> = registry
            .operations()
            .map(|spec| format!("{} {}", spec.method.as_str(), spec.path))
            .collect();
        declared_keys.sort();
        generated_keys.sort();
        assert_eq!(declared_keys, generated_keys);
    }

    #[test]
    fn mutation_sentinel_fails_when_verb_changes() {
        let baseline = production_registry(false);
        let mut mutated = ContractRegistry::new();
        for spec in baseline.operations().cloned() {
            mutated.register(spec).unwrap();
        }
        let mut flipped = mutated.get("liveness.get").unwrap().clone();
        flipped.method = HttpMethod::Post;
        let mut broken = ContractRegistry::new();
        for spec in mutated.operations().cloned() {
            if spec.operation_id == "liveness.get" {
                broken.register(flipped.clone()).unwrap();
            } else {
                broken.register(spec).unwrap();
            }
        }
        let diff = diff_contracts(&baseline, &broken);
        assert_eq!(diff.class, CompatibilityClass::Breaking);
    }

    #[tokio::test]
    async fn shadow_liveness_and_unauthenticated_workshop() {
        let liveness = build_liveness_surface().into_router();
        let response = liveness
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let declared = DeclaredRouter::default().route(
            RoutePolicy {
                method: axum::http::Method::GET,
                path: "/v1/health",
                group: RouteGroup::Portal,
                required_capability: Some(Capability::WorkshopRead),
                bootstrap_public: false,
                browser_policy: BrowserPolicy::NativeOnly,
                body_limit: 1024,
                rate_limit_class: RateLimitClass::Read,
            },
            get(|| async { StatusCode::NO_CONTENT }),
        );
        let app = assemble_daemon_access_boundary_with_declared(
            Router::new(),
            declared,
            DeclaredRouter::default(),
            Router::new(),
            DaemonAccessState::new(None),
        );
        let mut request = Request::builder()
            .uri("/v1/health")
            .body(Body::empty())
            .unwrap();
        request.extensions_mut().insert(ConnectInfo(
            "127.0.0.1:43101".parse::<SocketAddr>().unwrap(),
        ));
        let denied = app.oneshot(request).await.unwrap();
        assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn liveness_surface_records_contract_with_inventory() {
        let surface = build_liveness_surface();
        assert_eq!(surface.inventory().entries().len(), surface.contract().len());
        assert!(surface.contract().get("liveness.get").is_some());
    }

    #[test]
    fn yaml_manifest_remains_known_incomplete_shadow() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let yaml = std::fs::read_to_string(root.join("sdk-contract/manifest.yaml"))
            .expect("Slice 1 keeps manifest.yaml until generated clients own SDK accessors");
        let manifest = parse_manifest_yaml(&yaml).expect("manifest.yaml parses");
        let registry = production_registry(true);
        let listed: Vec<ListedRoute> = registry
            .operations()
            .map(|spec| ListedRoute {
                method: spec.method.as_str().to_string(),
                path: spec.path.clone(),
            })
            .collect();
        let report = discrepancy_report(&registry, &manifest, &listed);
        assert_eq!(report.declared_count, 373);
        assert!(
            report
                .rows
                .iter()
                .any(|row| row.kind == DiscrepancyKind::MissingFromManifest),
            "YAML must stay a known-incomplete shadow of the declared router"
        );
        let parity = std::fs::read_to_string(root.join("crates/medousa-sdk/tests/contract_parity.rs"))
            .expect("parity tests");
        assert!(
            !parity.contains("const PARITY_ROUTES"),
            "handwritten PARITY_ROUTES stays deleted; uniqueness uses generated ops"
        );
    }

    #[test]
    fn browser_compatibility_mounts_are_outside_declared_inventory() {
        let declared: std::collections::HashSet<String> = production_registry(true)
            .operations()
            .map(|spec| format!("{} {}", spec.method.as_str(), spec.path))
            .collect();
        for (method, path) in BROWSER_COMPATIBILITY_MOUNTS {
            let nested = format!("{method} /v1{path}");
            assert!(
                !declared.contains(&format!("{method} {path}")),
                "unprefixed browser adapter {method} {path} must not silently join DeclaredRouter"
            );
            assert!(
                !declared.contains(&nested),
                "browser adapter {nested} must stay on the H08 compatibility router until imported"
            );
        }
    }

    #[test]
    fn checked_in_contract_artifacts_match_generation() {
        let artifacts = generate_artifacts(&production_registry(true));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("sdk-contract");
        if std::env::var("UPDATE_API_CONTRACT").as_deref() == Ok("1") {
            std::fs::create_dir_all(root.join("generated")).unwrap();
            std::fs::write(root.join("openapi.json"), &artifacts.openapi_json).unwrap();
            std::fs::write(root.join("route-inventory.json"), &artifacts.route_inventory_json)
                .unwrap();
            std::fs::write(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("crates/medousa-sdk/src/generated/ops.rs"),
                &artifacts.rust_ops,
            )
            .unwrap();
            std::fs::write(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("python/medousa-sdk/src/medousa/_generated/ops.py"),
                &artifacts.python_ops,
            )
            .unwrap();
            std::fs::write(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("apps/medousa-home/src/lib/daemon/generatedOps.ts"),
                &artifacts.typescript_ops,
            )
            .unwrap();
            std::fs::write(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("apps/medousa-home/src-tauri/src/daemon/generated_ops.rs"),
                &artifacts.tauri_enum,
            )
            .unwrap();
            return;
        }
        let openapi = std::fs::read_to_string(root.join("openapi.json"))
            .expect("checked-in openapi.json; run UPDATE_API_CONTRACT=1 cargo test -p medousa --lib daemon::contract::tests::checked_in_contract_artifacts_match_generation");
        assert_eq!(openapi, artifacts.openapi_json);
        let inventory = std::fs::read_to_string(root.join("route-inventory.json")).unwrap();
        assert_eq!(inventory, artifacts.route_inventory_json);
    }
}
