//! Loopback-only service fixtures. No daemon, credentials, provider or inference.
use super::*;
use medousa_sdk::HttpTransport;
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

async fn response(
    resource: Resource,
    path: &str,
    status: u16,
    value: Value,
) -> Result<Vec<Record>, SdkError> {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let path = path.to_string();
    let server = thread::spawn(move || {
        // Parallel text/layout tests can starve ephemeral fixture threads on
        // shared hosts. Keep a bounded scheduling budget; GET/path/privacy
        // assertions remain exact. This does not change production deadlines.
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut socket = loop {
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(2)),
                Err(e) => panic!("fixture request missing: {e}"),
            }
        };
        socket
            .set_read_timeout(Some(Duration::from_secs(15)))
            .unwrap();
        let mut request = Vec::new();
        while !request.windows(4).any(|w| w == b"\r\n\r\n") {
            let mut buffer = [0; 2048];
            let n = socket.read(&mut buffer).unwrap();
            assert!(n > 0);
            request.extend_from_slice(&buffer[..n]);
        }
        let request = String::from_utf8(request).unwrap();
        assert!(
            request.starts_with(&format!("GET {path} HTTP/1.1\r\n")),
            "{request}"
        );
        assert!(!request.to_ascii_lowercase().contains("authorization:"));
        let body = value.to_string();
        write!(socket, "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
    });
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let client = MedousaClient::with_transport(Arc::new(HttpTransport::with_client(http)), origin);
    let result = read(&client, resource).await;
    server.join().unwrap();
    result
}
#[tokio::test]
async fn all_eleven_admin_models_use_existing_get_contracts_and_safe_projections() {
    let cases = vec![
        (
            Resource::Health,
            "/v1/health",
            json!({"runtime":{"authority_id":format!("auth_{}", "a".repeat(64)), "product_version":"fixture", "build_revision":"test", "contract_revision":1,"base_schema_revision":1,"deployment_profile":"test","deployment_target":"fixture", "advertised_capabilities":[]},"status":"ok","backend":"fixture", "worker_id":"test", "now_utc":"2026-11-13T00:00:00Z"}),
        ),
        (
            Resource::Models,
            "/v1/models/catalog",
            json!({"models":[{"provider":"fixture", "model_id":"test-model", "api_key":"NEVER-RENDER"}]}),
        ),
        (
            Resource::Runtimes,
            "/v1/agents/runtimes",
            json!({"runtimes":[{"runtime":"codex", "available":false, "binary_present":true,"uses_native_turns":true,"auth_status":"signed_out","command":"NEVER-RENDER","auth_detail":"NEVER-RENDER","detail":"NEVER-RENDER"}]}),
        ),
        (
            Resource::Modes,
            "/v1/agent-modes",
            json!({"modes":[{"mode":"general","label":"Default", "available":true,"description":"fixture","unavailable_reason":null}]}),
        ),
        (
            Resource::Mcp,
            "/v1/mcp/gateway/status",
            json!({"gatewayUrl":"NEVER-RENDER","reachable":false,"message":"NEVER-RENDER","health":null,"servers":[{"serverId":"fixture", "title":"Disconnected test server", "enabled":true,"connected":false,"toolCount":0,"allowedLanes":[]}]}),
        ),
        (
            Resource::LocalEngine,
            "/v1/local/engine/status",
            json!({"featureEnabled":false, "loaded":false,"baseUrl":"NEVER-RENDER", "message":"NEVER-RENDER"}),
        ),
        (
            Resource::Schedules,
            "/v1/recurring",
            json!({"count":1,"recurring":[{"recurring_id":"test", "queue":"scheduled","job_type":"prompt","cron_expr":"0 9 * * MON","timezone":"UTC","enabled":false,"next_run_at_utc":"2026-11-16T09:00:00Z","display_name":"Test schedule","prompt":"NEVER-RENDER"}]}),
        ),
        (
            Resource::Sharing,
            "/v1/shared-mode",
            json!({"mode":"single","enabled_at":null,"root_profile_id":"root", "general_profile_id":"general"}),
        ),
        (
            Resource::Capacity,
            "/v1/runtime/workers",
            json!({"max_in_flight":2,"agents":1,"scheduled":1,"delivery":1,"maintenance":1}),
        ),
        (
            Resource::ReviewProviders,
            "/v1/reviews/providers",
            json!({"providers":[{"protocol_version":1,"id":"test","label":"Test provider","hosts":[],"operations":[]}],"problems":["NEVER-RENDER"]}),
        ),
        (
            Resource::Activity,
            "/v1/workspace/cards?limit=100&include_terminal=true",
            json!({"workspace_revision":42,"cards":[{"id":"job-fixture", "column":"in_flight", "title":"NEVER-RENDER prompt excerpt", "status_label":"NEVER-RENDER diagnostics", "created_at_utc":"2026-11-13T00:00:00Z", "updated_at_utc":"2026-11-13T01:00:00Z", "output":"NEVER-RENDER"}]}),
        ),
    ];
    assert_eq!(cases.len(), Resource::ALL.len());
    for (resource, path, body) in cases {
        let rows = response(resource, path, 200, body)
            .await
            .unwrap_or_else(|e| panic!("{resource:?}: {e}"));
        assert!(!rows.is_empty(), "{resource:?}");
        assert!(!format!("{rows:?}").contains("NEVER-RENDER"));
    }
}
#[tokio::test]
async fn errors_and_malformed_models_never_become_empty_success_or_raw_diagnostics() {
    for status in [401, 403, 404, 500] {
        let error = response(
            Resource::Models,
            "/v1/models/catalog",
            status,
            json!({"error":"NEVER-RENDER"}),
        )
        .await
        .unwrap_err();
        let expected = if status == 401 || status == 403 {
            ReadError::Permission
        } else {
            ReadError::Unavailable
        };
        assert_eq!(classify(error), expected);
        assert!(!expected.message().contains("NEVER-RENDER"));
    }
    for body in [
        json!({}),
        json!({"models":[{"provider":"test"}]}),
        json!({"models":{}}),
    ] {
        assert_eq!(
            classify(
                response(Resource::Models, "/v1/models/catalog", 200, body)
                    .await
                    .unwrap_err()
            ),
            ReadError::Incompatible
        );
    }
    assert!(
        response(
            Resource::Models,
            "/v1/models/catalog",
            200,
            json!({"models":[]})
        )
        .await
        .unwrap()
        .is_empty()
    );
}
#[tokio::test]
async fn activity_caps_rows_and_does_not_guess_empty_or_permission_success() {
    let path = "/v1/workspace/cards?limit=100&include_terminal=true";
    let card = json!({"id":"job-fixture", "column":"blocked", "title":"NEVER-RENDER", "status_label":"NEVER-RENDER", "created_at_utc":"2026-11-13T00:00:00Z", "updated_at_utc":"2026-11-13T01:00:00Z"});
    let rows = response(
        Resource::Activity,
        path,
        200,
        json!({"workspace_revision":2,"cards":vec![card; 120]}),
    )
    .await
    .unwrap();
    assert_eq!(rows.len(), 100);
    assert_eq!(rows[0].title, "Work item 1 · Blocked");
    assert!(!format!("{rows:?}").contains("NEVER-RENDER"));
    assert_eq!(
        classify(
            response(
                Resource::Activity,
                path,
                403,
                json!({"error":"NEVER-RENDER"})
            )
            .await
            .unwrap_err()
        ),
        ReadError::Permission
    );
    for body in [
        json!({}),
        json!({"workspace_revision":2,"cards":[{"id":"x"}]}),
    ] {
        assert_eq!(
            classify(
                response(Resource::Activity, path, 200, body)
                    .await
                    .unwrap_err()
            ),
            ReadError::Incompatible
        );
    }
    assert!(
        response(
            Resource::Activity,
            path,
            200,
            json!({"workspace_revision":2,"cards":[]})
        )
        .await
        .unwrap()
        .is_empty()
    );
}
#[test]
fn rendering_bounds_and_filters_are_plain_local_text() {
    let row = Record::new("A\nB\0C", vec![("Field", "x".repeat(1000))]);
    assert_eq!(row.title, "ABC");
    assert_eq!(row.fields[0].1.chars().count(), 240);
    assert!(row.matches("abc"));
    assert!(!row.matches("not present"));
}
