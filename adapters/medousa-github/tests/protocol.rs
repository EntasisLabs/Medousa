//! Exercise the independently launched package and its real gh invocation path.
//! The CLI fixture has no network or user credentials.
#![cfg(unix)]
use medousa_types::review_provider::*;
use serde_json::{Value, json};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};

fn run(request: Value, scenario: &str) -> Value {
    run_mode(request, scenario, "--invoke")
}
fn run_mode(request: Value, scenario: &str, mode: &str) -> Value {
    let directory = tempfile::tempdir().unwrap();
    let gh = directory.path().join("gh");
    std::fs::write(&gh, r#"#!/bin/sh
if [ "$REVIEW_FIXTURE_SCENARIO" = "unauthenticated" ]; then echo 'gh: HTTP 401' >&2; exit 1; fi
if [ "$REVIEW_FIXTURE_SCENARIO" = "malformed" ]; then printf '%s' 'not-json'; exit 0; fi
if [ "$REVIEW_FIXTURE_SCENARIO" = "credential" ] && [ "$GH_TOKEN" != "fixture-only-token" ]; then exit 2; fi
for arg do case "$arg" in user|repos/*) endpoint="$arg" ;; esac; done
if [ "$REVIEW_FIXTURE_SCENARIO" = "publish-lost" ] && [ "$endpoint" = "repos/team/repo/pulls" ]; then echo 'network lost' >&2; exit 1; fi
case "$endpoint" in
  *'/pulls?'*) if [ "$REVIEW_FIXTURE_SCENARIO" = "existing" ]; then printf '%s' '[{"number":42,"html_url":"https://github.com/team/repo/pull/42","state":"open"}]'; else printf '%s' '[]'; fi ;;
  user) printf '%s' '{"login":"fixture-user"}' ;;
  *'/files?'*) printf '%s' '[{"filename":"src/lib.rs","status":"modified","additions":1,"deletions":1,"patch":"@@ -1 +1 @@\n-old\n+new"}]' ;;
  *'/comments?'*) printf '%s' '[{"id":12,"body":"Please check this","user":{"login":"reviewer"},"path":"src/lib.rs","line":2,"side":"RIGHT","commit_id":"abc","in_reply_to_id":11}]' ;;
  *'/check-runs?'*) printf '%s' '{"check_runs":[{"id":21,"name":"CI","status":"completed","conclusion":"success"}]}' ;;
  *) printf '%s' '{"number":42,"html_url":"https://github.com/team/repo/pull/42","title":"Fix","body":"Description","state":"open","user":{"login":"author"},"head":{"ref":"feature","sha":"abc"},"base":{"ref":"main","sha":"def"},"changed_files":1}' ;;
esac
"#).unwrap();
    std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_medousa_review_github"))
        .arg(mode)
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", directory.path().display()),
        )
        .env("REVIEW_FIXTURE_SCENARIO", scenario)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let bytes = serde_json::to_vec(&request).unwrap();
    child.stdin.take().unwrap().write_all(&bytes).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    serde_json::from_slice(&output.stdout).unwrap()
}
fn reference() -> Value {
    json!({"provider_id":"github","host":"github.com","repository":"team/repo","native_id":"42"})
}
fn invocation(operation: Value) -> Value {
    json!({"protocol_version":1,"credential":null,"query":{"provider_id":"github","host":"github.com","connection_id":null,"request":operation}})
}

#[test]
fn resolves_review_without_a_checkout_and_keeps_revision_identity() {
    let response = run(
        invocation(json!({"operation":"resolve","url":"https://github.com/team/repo/pull/42"})),
        "",
    );
    assert_eq!(
        response["outcome"]["result"]["review"]["head_revision"],
        "abc"
    );
    assert_eq!(
        response["outcome"]["result"]["review"]["reference"]["native_id"],
        "42"
    );
    assert_eq!(response["outcome"]["status"], "success");
}
#[test]
fn refuses_to_mix_a_new_revision_into_an_open_diff() {
    let response = run(
        invocation(
            json!({"operation":"diff","review":reference(),"revision":"old","path":"src/lib.rs"}),
        ),
        "",
    );
    assert_eq!(response["outcome"]["error"]["code"], "stale_revision");
}
#[test]
fn diff_and_inline_comment_locations_survive_the_protocol() {
    let response = run(
        invocation(
            json!({"operation":"diff","review":reference(),"revision":"abc","path":"src/lib.rs"}),
        ),
        "",
    );
    assert!(
        response["outcome"]["result"]["patch"]
            .as_str()
            .unwrap()
            .contains("+new")
    );
    let response = run(
        invocation(json!({"operation":"discussion","review":reference(),"cursor":"inline:1"})),
        "",
    );
    let comment = &response["outcome"]["result"]["entries"][0];
    assert_eq!(comment["kind"], "inline_comment");
    assert_eq!(comment["id"], "inline:12");
    assert_eq!(comment["reply_to"], "inline:11");
    assert_eq!(comment["line"], 2);
    assert_eq!(comment["revision"], "abc");
}
#[test]
fn reports_actual_authentication_and_keeps_credentials_out_of_results() {
    let mut request = invocation(json!({"operation":"connection_status"}));
    let response = run(request.clone(), "");
    assert_eq!(response["outcome"]["result"]["authenticated"], true);
    request["credential"] = json!("{\"unexpected\":\"fixture-only-token\"}");
    let invalid = run(request, "");
    assert!(!invalid.to_string().contains("fixture-only-token"));
    let response = run(
        invocation(json!({"operation":"connection_status"})),
        "unauthenticated",
    );
    assert_eq!(response["outcome"]["result"]["authenticated"], false);
}
#[test]
fn malformed_upstream_and_incompatible_protocol_have_typed_errors() {
    let response = run(
        invocation(json!({"operation":"get","review":reference()})),
        "malformed",
    );
    assert_eq!(response["outcome"]["error"]["code"], "provider_error");
    let mut request = invocation(json!({"operation":"connection_status"}));
    request["protocol_version"] = json!(99);
    let response = run(request, "");
    assert_eq!(response["outcome"]["error"]["code"], "unsupported");
}
#[test]
fn descriptor_advertises_only_implemented_operations() {
    let output = Command::new(env!("CARGO_BIN_EXE_medousa_review_github"))
        .arg("--describe")
        .output()
        .unwrap();
    let descriptor: ReviewProviderDescriptor = serde_json::from_slice(&output.stdout).unwrap();
    assert!(descriptor.operations.contains(&"checks".into()));
    assert!(descriptor.operations.contains(&"merge".into()));
    assert!(descriptor.operations.contains(&"inbox".into()));
    assert!(descriptor.operations.contains(&"submit_review".into()));
}

fn publish_request(update_existing: bool) -> Value {
    json!({"protocol_version":1,"credential":null,"mutation":{"provider_id":"github","host":"github.com","connection_id":null,"request":{"operation":"publish","repository":"team/repo","head":"feature","base":"main","title":"A title","body":"Actual body\nwith newlines","draft":true,"update_existing":update_existing}}})
}
#[test]
fn publication_creates_or_reuses_an_open_review_and_reports_uncertain_results() {
    let created = run_mode(publish_request(false), "", "--mutate");
    assert_eq!(created["outcome"]["result"]["existing"], false);
    assert_eq!(
        created["outcome"]["result"]["review"]["reference"]["native_id"],
        "42"
    );
    let reused = run_mode(publish_request(false), "existing", "--mutate");
    assert_eq!(reused["outcome"]["result"]["existing"], true);
    let updated = run_mode(publish_request(true), "existing", "--mutate");
    assert_eq!(updated["outcome"]["result"]["existing"], true);
    let uncertain = run_mode(publish_request(false), "publish-lost", "--mutate");
    assert!(
        uncertain["outcome"]["error"]["message"]
            .as_str()
            .unwrap()
            .contains("existing review first")
    );
}
#[test]
fn write_operations_cannot_be_sent_through_read_ipc() {
    let mut request = invocation(
        json!({"operation":"publish","repository":"team/repo","head":"feature","base":"main","title":"A title","body":"","draft":false,"update_existing":false}),
    );
    let response = run(request.clone(), "");
    assert_eq!(response["outcome"]["error"]["code"], "invalid_parameter");
    request["query"]["request"] =
        json!({"operation":"resolve_repository","remote_url":"git@github.com:team/repo.git"});
    let response = run(request, "");
    assert_eq!(response["outcome"]["result"]["repository"], "team/repo");
}
