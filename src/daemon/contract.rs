//! Map H01 `DeclaredRouter` inventories onto H10 `OperationSpec` values.

use axum::http::Method;

use medousa_api_contract::{
    Audience, ContractRegistry, FeatureProfile, HttpMethod, OperationSpec, ParameterLocation,
    ParameterSpec, ResponseSpec, SchemaRef, Stability, StreamTransport, stable_operation_id,
};

use crate::daemon::contract_bindings::{json_body, stream_binding, stream_spec, wire_binding};

use crate::daemon::route_policy::{RateLimitClass, RouteGroup, RouteInventory, RoutePolicy};
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
    let operation_id = stable_operation_id(policy.method.as_str(), policy.path);
    let stream = stream_binding(&operation_id);
    let binding = wire_binding(&operation_id);
    let response_schema = match (&stream, binding) {
        (Some((_, name)), _) => SchemaRef::named(*name),
        (None, Some(wire)) => SchemaRef::named(wire.response),
        (None, None) => SchemaRef::deferred(format!(
            "{}Response",
            medousa_api_contract::const_name(&operation_id)
        )),
    };
    let media_type = match stream {
        Some((StreamTransport::Sse, _)) => "text/event-stream",
        _ => "application/json",
    };
    let mut spec = OperationSpec {
        operation_id: operation_id.clone(),
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
        request_body: binding.and_then(|wire| {
            if matches!(
                method,
                HttpMethod::Get | HttpMethod::Delete | HttpMethod::Head
            ) {
                None
            } else {
                wire.request.map(json_body)
            }
        }),
        responses: vec![ResponseSpec {
            status: 200,
            media_type: media_type.into(),
            schema: response_schema.clone(),
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
        stream: stream.map(|(transport, name)| stream_spec(transport, name)),
        deprecation: None,
    };
    if !policy.bootstrap_public {
        spec.responses.push(ResponseSpec {
            status: 401,
            media_type: "application/json".into(),
            schema: SchemaRef::named("ApiErrorEnvelope"),
        });
        spec.responses.push(ResponseSpec {
            status: 403,
            media_type: "application/json".into(),
            schema: SchemaRef::named("ApiErrorEnvelope"),
        });
    }
    if operation_id == "sessions.derive.post" {
        spec.parameters.push(ParameterSpec {
            name: "Idempotency-Key".to_string(),
            location: ParameterLocation::Header,
            required: true,
            schema: SchemaRef::named("string"),
        });
    }
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
        } else if pairing_keys
            .contains(&(policy.method.as_str().to_string(), policy.path.to_string()))
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

/// Unprefixed H08 browser compatibility aliases. Canonical `/v1` copies are
/// registered on `browser_surface` / `DeclaredRouter`.
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

/// Reviewed freeze of the Stasis dashboard router in `stasis-rs` 0.9.0.
/// Stasis does not export method/path descriptors, so this list cannot be
/// derived from `dashboard_router` and must not silently join DeclaredRouter.
pub const DASHBOARD_COMPATIBILITY_MOUNTS: &[(&str, &str)] = &[
    ("GET", "/"),
    ("GET", "/dashboard"),
    ("GET", "/view/{name}"),
    ("GET", "/stream/jobs"),
    ("GET", "/stream/outbox"),
    ("GET", "/stream/nodes"),
    ("GET", "/stream/workflow-reflection"),
    ("GET", "/inspect/job/{id}"),
    ("GET", "/inspect/attempt/{id}"),
    ("GET", "/inspect/node/{id}"),
    ("GET", "/inspect/endpoint/{id}"),
    ("GET", "/inspect/event/{id}"),
    ("GET", "/assets/{name}"),
    ("POST", "/action/scheduler/materialize"),
    ("POST", "/action/scheduler/process"),
    ("POST", "/action/scheduler/publish"),
    ("POST", "/action/scheduler/replay"),
    ("POST", "/action/workflows/run-draft"),
    ("POST", "/action/workflows/save"),
    ("POST", "/action/workflows/execute"),
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
    use crate::daemon::route_policy::{
        BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RoutePolicy,
    };
    use crate::daemon::router::build_liveness_surface;
    use crate::peer_scope::{DaemonAccessState, assemble_daemon_access_boundary_with_declared};
    use crate::request_principal::Capability;
    use axum::Router;
    use axum::body::Body;
    use axum::extract::ConnectInfo;
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use medousa_api_contract::{
        CompatibilityClass, HttpMethod, StreamTransport, diff_contracts,
        generate_artifacts_with_catalog,
    };
    use std::net::SocketAddr;
    use tower::ServiceExt;

    fn schema_catalog() -> std::collections::BTreeMap<String, serde_json::Value> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("sdk-contract/medousa-types.schema.json");
        serde_json::from_str(&std::fs::read_to_string(path).expect("schema catalog")).unwrap()
    }

    fn artifacts(registry: &ContractRegistry) -> medousa_api_contract::GeneratedArtifacts {
        generate_artifacts_with_catalog(registry, &schema_catalog())
    }

    /// Bound names that are not `medousa-types` schemars titles (constants,
    /// JSON-RPC, or Forge event bags). They must not silently grow; prefer
    /// adding a DTO + `export_type!` instead.
    const UNCATALOGUED_WIRE_NAMES: &[&str] = &[
        "HealthLiveness",
        "JsonRpcMessage",
        "ForgeStreamEvent",
        "ForgeProjectEvent",
    ];

    #[test]
    fn production_profiles_match_declared_counts() {
        let without_pairing = production_registry(false);
        let with_pairing = production_registry(true);
        assert_eq!(without_pairing.len(), 403);
        assert_eq!(with_pairing.len(), 422);
        let artifacts = artifacts(&with_pairing);
        let inventory: serde_json::Value =
            serde_json::from_str(&artifacts.route_inventory_json).unwrap();
        assert_eq!(inventory["operations"].as_array().unwrap().len(), 422);
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
        assert_eq!(
            surface.inventory().entries().len(),
            surface.contract().len()
        );
        assert!(surface.contract().get("liveness.get").is_some());
    }

    #[test]
    fn parity_routes_stay_deleted() {
        let parity = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("crates/medousa-sdk/tests/contract_parity.rs"),
        )
        .expect("parity tests");
        assert!(
            !parity.contains("const PARITY_ROUTES"),
            "handwritten PARITY_ROUTES stays deleted; uniqueness uses generated ops"
        );
        assert!(
            !std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("sdk-contract/manifest.yaml")
                .exists(),
            "manifest.yaml is deleted; OpenAPI and generated ops own the public contract"
        );
    }

    #[test]
    fn schema_catalog_covers_named_contract_bindings() {
        let catalog = schema_catalog();
        let registry = production_registry(true);
        let mut missing = Vec::new();
        for spec in registry.operations() {
            let mut refs = Vec::new();
            if let Some(body) = &spec.request_body {
                refs.push(&body.schema);
            }
            for response in &spec.responses {
                refs.push(&response.schema);
            }
            if let Some(stream) = &spec.stream {
                refs.push(&stream.item_schema);
            }
            for schema in refs {
                if schema.opaque {
                    continue;
                }
                if catalog.contains_key(&schema.name) {
                    continue;
                }
                if UNCATALOGUED_WIRE_NAMES.contains(&schema.name.as_str()) {
                    continue;
                }
                missing.push(format!("{}: {}", spec.operation_id, schema.name));
            }
        }
        assert!(
            missing.is_empty(),
            "named contract schemas missing from medousa-types.schema.json:\n{}",
            missing.join("\n")
        );
    }

    #[test]
    fn websocket_streams_are_not_event_stream() {
        let registry = production_registry(true);
        for id in [
            "code.lsp.get",
            "grapheme.lsp.get",
            "sessions.shell.by_id.get",
        ] {
            let spec = registry.get(id).expect(id);
            let stream = spec.stream.as_ref().expect("websocket metadata");
            assert_eq!(stream.transport, StreamTransport::WebSocket);
            assert!(
                spec.responses
                    .iter()
                    .all(|response| response.media_type != "text/event-stream"),
                "{id} must not advertise text/event-stream"
            );
        }
        let interactive = registry
            .get("interactive.turn.by_turn_id.stream.get")
            .unwrap();
        assert_eq!(
            interactive.stream.as_ref().unwrap().transport,
            StreamTransport::Sse
        );
        assert_eq!(interactive.responses[0].media_type, "text/event-stream");
    }

    #[test]
    fn named_vault_and_health_schemas_are_not_opaque() {
        let health = production_registry(false)
            .get("health.get")
            .unwrap()
            .clone();
        assert!(!health.responses[0].schema.opaque);
        assert_eq!(health.responses[0].schema.name, "HealthResponse");
        let create_session = production_registry(false)
            .get("sessions.post")
            .unwrap()
            .clone();
        assert_eq!(
            create_session.request_body.as_ref().unwrap().schema.name,
            "CreateSessionRequest"
        );
        assert_eq!(
            create_session.responses[0].schema.name,
            "CreateSessionResponse"
        );
        assert!(!create_session.responses[0].schema.opaque);
        let vault = production_registry(false)
            .get("vault.notes.get")
            .unwrap()
            .clone();
        assert_eq!(vault.responses[0].schema.name, "VaultNotesListResponse");
        assert!(!vault.responses[0].schema.opaque);
        let openapi = artifacts(&production_registry(true)).openapi_json;
        assert!(openapi.contains("\"VaultNotesListResponse\""));
        assert!(!openapi.contains("x-medousa-unresolved"));
        let ingest = production_registry(false)
            .get("ingest.post")
            .unwrap()
            .clone();
        assert_eq!(
            ingest.request_body.as_ref().unwrap().schema.name,
            "IngestRequest"
        );
    }

    #[test]
    fn browser_compatibility_aliases_stay_unprefixed() {
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
                declared.contains(&nested),
                "canonical browser adapter {nested} must be imported onto DeclaredRouter"
            );
        }
        let inventory = crate::daemon::router::build_declared_route_inventory(false);
        for (method, path) in BROWSER_COMPATIBILITY_MOUNTS {
            let nested = format!("/v1{path}");
            let entry = inventory
                .entries()
                .find(|entry| entry.method == *method && entry.path == nested)
                .unwrap_or_else(|| panic!("missing declared browser mount {method} {nested}"));
            assert_eq!(
                entry.browser_policy,
                crate::daemon::route_policy::BrowserPolicy::ExactOrigin,
                "{method} {nested} must not inherit NativeOnly"
            );
        }
    }

    #[test]
    fn dashboard_compatibility_stays_off_declared_router() {
        let declared: std::collections::HashSet<String> = production_registry(true)
            .operations()
            .map(|spec| format!("{} {}", spec.method.as_str(), spec.path))
            .collect();
        assert!(
            !DASHBOARD_COMPATIBILITY_MOUNTS.is_empty(),
            "Stasis dashboard_router does not export method/path descriptors; keep a reviewed freeze"
        );
        for (method, path) in DASHBOARD_COMPATIBILITY_MOUNTS {
            assert!(
                !declared.contains(&format!("{method} {path}")),
                "dashboard adapter {method} {path} is third-party opaque and must stay off DeclaredRouter"
            );
        }
        let router_src = include_str!("router.rs");
        assert!(
            router_src.contains("dashboard_router("),
            "dashboard remains a raw Stasis mount until that crate exports descriptors"
        );
    }

    #[tokio::test]
    async fn declared_plaintext_errors_become_api_error_envelope() {
        use crate::request_principal::{RequestPrincipal, TransportClass};
        use axum::extract::Extension;
        use axum::http::header::CONTENT_TYPE;
        use std::sync::Arc;

        let router = DeclaredRouter::default()
            .route(
                RoutePolicy {
                    method: axum::http::Method::GET,
                    path: "/v1/vault/search",
                    group: RouteGroup::Portal,
                    required_capability: Some(Capability::WorkshopRead),
                    bootstrap_public: false,
                    browser_policy: BrowserPolicy::NativeOnly,
                    body_limit: 1024,
                    rate_limit_class: RateLimitClass::Read,
                },
                get(|| async { (StatusCode::BAD_REQUEST, "q or tags is required".to_string()) }),
            )
            .into_router()
            .layer(Extension(RequestPrincipal::local_app(
                Arc::from("test-local"),
                TransportClass::Loopback,
            )));
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/vault/search")
                    .header("x-request-id", "req-slice2")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response.headers().get(CONTENT_TYPE).unwrap(),
            "application/json"
        );
        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        let envelope: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(envelope["schema_version"], 1);
        assert_eq!(envelope["code"], "invalid_parameter");
        assert_eq!(envelope["message"], "q or tags is required");
        assert_eq!(envelope["request_id"], "req-slice2");
    }

    #[test]
    fn checked_in_contract_artifacts_match_generation() {
        let artifacts = artifacts(&production_registry(true));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("sdk-contract");
        if std::env::var("UPDATE_API_CONTRACT").as_deref() == Ok("1") {
            std::fs::create_dir_all(root.join("generated")).unwrap();
            std::fs::write(root.join("openapi.json"), &artifacts.openapi_json).unwrap();
            std::fs::write(
                root.join("route-inventory.json"),
                &artifacts.route_inventory_json,
            )
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
        let checked: serde_json::Value = serde_json::from_str(&openapi).unwrap();
        let generated: serde_json::Value = serde_json::from_str(&artifacts.openapi_json).unwrap();
        assert_eq!(checked, generated);
        let inventory = std::fs::read_to_string(root.join("route-inventory.json")).unwrap();
        let checked_inventory: serde_json::Value = serde_json::from_str(&inventory).unwrap();
        let generated_inventory: serde_json::Value =
            serde_json::from_str(&artifacts.route_inventory_json).unwrap();
        assert_eq!(checked_inventory, generated_inventory);
    }

    #[test]
    fn released_baseline_is_not_breaking() {
        let candidate = production_registry(true);
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("sdk-contract/released");
        if std::env::var("UPDATE_RELEASED_CONTRACT").as_deref() == Ok("1") {
            std::fs::create_dir_all(&root).unwrap();
            let ops: Vec<_> = candidate.operations().cloned().collect();
            std::fs::write(
                root.join("registry.json"),
                serde_json::to_string_pretty(&ops).unwrap(),
            )
            .unwrap();
            return;
        }
        let ops: Vec<OperationSpec> = serde_json::from_str(
            &std::fs::read_to_string(root.join("registry.json"))
                .expect("sdk-contract/released/registry.json; run UPDATE_RELEASED_CONTRACT=1 cargo test -p medousa --lib daemon::contract::tests::released_baseline_is_not_breaking"),
        )
        .unwrap();
        let mut baseline = ContractRegistry::new();
        for spec in ops {
            baseline.register(spec).expect("released baseline ops");
        }
        let diff = diff_contracts(&baseline, &candidate);
        assert_ne!(
            diff.class,
            CompatibilityClass::Breaking,
            "released contract baseline is breaking: {:?}",
            diff.breaking_changes
        );
    }
}
