//! Real package IPC and CLI transport with a local-only GitHub fixture.
#![cfg(unix)]
use serde_json::{Value, json};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};
fn reference() -> Value {
    json!({"provider_id":"github","host":"github.com","repository":"team/repo","native_id":"42"})
}
fn run(request: Value, write: bool, scenario: &str) -> (Value, Vec<Value>) {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("gh");
    let trace = dir.path().join("trace");
    std::fs::write(&script, r#"#!/usr/bin/python3
import json,sys,os
args=sys.argv[1:]
endpoint=args[args.index('--method')+2]
method=args[args.index('--method')+1]
body=json.load(sys.stdin) if '--input' in args else None
with open(os.environ['FIXTURE_TRACE'],'a') as f: f.write(json.dumps({'endpoint':endpoint,'method':method,'body':body})+'\n')
scenario=os.environ['FIXTURE_SCENARIO']
pr={'number':42,'node_id':'PR_fixture','html_url':'https://github.com/team/repo/pull/42','title':'Change','state':'open','user':{'login':'author'},'head':{'sha':'abc','ref':'feature'},'base':{'sha':'def','ref':'main'},'requested_reviewers':[{'login':'reviewer'}],'mergeable_state':'clean','labels':[{'name':'bug','description':'A bug','color':'ff0000'}]}
if scenario=='draft': pr['draft']=True
if scenario=='lost' and method!='GET': sys.exit(1)
if endpoint=='user': result={'login':'author' if scenario=='owner' else 'reviewer'}
elif endpoint=='repos/team/repo': result={'permissions':{'push':scenario!='read-only'},'allow_squash_merge':True}
elif endpoint.startswith('repos/team/repo/labels?'): result=[{'name':'needs review/ci','description':'CI work','color':'00ff11'}]
elif endpoint.startswith('repos/team/repo/collaborators?'): result=[{'login':'reviewer'},{'login':'author'}]
elif endpoint.startswith('repos/team/repo/pulls/42/reviews?'): result=[{'user':{'login':'reviewer'},'state':'APPROVED'},{'user':{'login':'other'},'state':'CHANGES_REQUESTED'},{'user':{'login':'private'},'state':'PENDING'}]
elif endpoint.startswith('search/issues?'): result={'items':[pr],'total_count':1,'incomplete_results':False}
elif endpoint.endswith('/merge'): result={'merged':scenario!='blocked'}
elif endpoint=='graphql': result={'data':{('markPullRequestReadyForReview' if 'markPullRequestReadyForReview' in body['query'] else 'convertPullRequestToDraft'):{'pullRequest':{'id':'PR_fixture'}}}} if scenario!='graphql-error' else {'errors':[{'message':'Denied'}]}
elif '/actions/runs/9/jobs?' in endpoint: result={'total_count':1,'jobs':[{'name':'Build','conclusion':'failure','steps':[{'name':'Compile','conclusion':'failure'}]}]}
elif '/actions/runs/9' in endpoint: result={'head_sha':'other' if scenario=='wrong-run' else 'abc','name':'CI','status':'completed','conclusion':'failure'}
elif '/check-runs/3/annotations?' in endpoint: result=[{'path':'src/lib.rs','start_line':2,'annotation_level':'failure','message':'Failed assertion'}]
elif '/check-runs/3' in endpoint: result={'head_sha':'abc','output':{'title':'Build','summary':'Failed','annotations_count':1}}
else: result=pr
print(json.dumps(result))
"#).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_medousa_review_github"))
        .arg(if write { "--mutate" } else { "--invoke" })
        .env("PATH", format!("{}:/usr/bin:/bin", dir.path().display()))
        .env("FIXTURE_TRACE", &trace)
        .env("FIXTURE_SCENARIO", scenario)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let key = if write { "mutation" } else { "query" };
    let mut invocation = json!({"protocol_version":1,"credential":null});
    invocation[key] =
        json!({"provider_id":"github","host":"github.com","connection_id":null,"request":request});
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&invocation).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let response = serde_json::from_slice(&output.stdout).unwrap();
    let calls = std::fs::read_to_string(trace)
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    (response, calls)
}
#[test]
fn inbox_uses_the_authenticated_account_and_returns_external_identities() {
    for (folder, qualifier) in [
        ("authored", "author%3Areviewer"),
        ("review_requested", "review-requested%3Areviewer"),
        ("participating", "involves%3Areviewer"),
    ] {
        let (result, calls) = run(
            json!({"operation":"inbox","folder":folder,"cursor":null}),
            false,
            "",
        );
        assert_eq!(
            result["outcome"]["result"]["reviews"][0]["reference"],
            reference()
        );
        assert!(calls[1]["endpoint"].as_str().unwrap().contains(qualifier));
    }
}
#[test]
fn review_actions_follow_ownership_repository_settings_and_draft_state() {
    for scenario in ["", "owner", "draft"] {
        let (result, _) = run(
            json!({"operation":"get","review":reference()}),
            false,
            scenario,
        );
        let review = &result["outcome"]["result"]["review"];
        assert_eq!(review["requested_reviewers"], json!(["reviewer"]));
        assert!(
            review["actions"]
                .as_array()
                .unwrap()
                .contains(&json!("comment"))
        );
        assert_eq!(
            review["actions"]
                .as_array()
                .unwrap()
                .contains(&json!("approve")),
            scenario.is_empty()
        );
        assert_eq!(
            review["merge_methods"],
            if scenario == "draft" {
                json!([])
            } else {
                json!(["squash"])
            }
        );
    }
}
#[test]
fn submits_review_with_commit_and_both_diff_sides_and_rejects_stale_submission() {
    let request = json!({"operation":"submit_review","review":reference(),"revision":"abc","decision":"request_changes","body":"Please revise","comments":[{"path":"src/lib.rs","side":"old","line":4,"body":"Old"},{"path":"src/lib.rs","side":"new","line":5,"body":"New"}]});
    let (response, calls) = run(request.clone(), true, "");
    assert_eq!(response["outcome"]["status"], "success");
    let write = calls.last().unwrap();
    assert_eq!(write["endpoint"], "repos/team/repo/pulls/42/reviews");
    assert_eq!(write["body"]["event"], "REQUEST_CHANGES");
    assert_eq!(write["body"]["commit_id"], "abc");
    assert_eq!(write["body"]["comments"][0]["side"], "LEFT");
    assert_eq!(write["body"]["comments"][1]["side"], "RIGHT");
    let mut stale = request;
    stale["revision"] = json!("old");
    let (response, calls) = run(stale, true, "");
    assert_eq!(response["outcome"]["error"]["code"], "stale_revision");
    assert!(calls.iter().all(|c| c["method"] == "GET"));
}
#[test]
fn comment_reply_edit_and_reviewer_requests_use_correct_resources() {
    for (request, endpoint, field, expected) in [
        (
            json!({"operation":"comment","review":reference(),"body":"Hello","reply_to":null}),
            "repos/team/repo/issues/42/comments",
            "body",
            json!("Hello"),
        ),
        (
            json!({"operation":"comment","review":reference(),"body":"Reply","reply_to":"inline:12"}),
            "repos/team/repo/pulls/42/comments/12/replies",
            "body",
            json!("Reply"),
        ),
        (
            json!({"operation":"edit","review":reference(),"title":"New","body":"Description"}),
            "repos/team/repo/pulls/42",
            "title",
            json!("New"),
        ),
        (
            json!({"operation":"request_reviewers","review":reference(),"reviewers":["teammate"]}),
            "repos/team/repo/pulls/42/requested_reviewers",
            "reviewers",
            json!(["teammate"]),
        ),
    ] {
        let (response, calls) = run(request, true, "");
        assert_eq!(response["outcome"]["status"], "success");
        assert_eq!(calls.last().unwrap()["endpoint"], endpoint);
        assert_eq!(calls.last().unwrap()["body"][field], expected);
    }
}
#[test]
fn lifecycle_and_merge_use_upstream_confirmation_and_exact_head() {
    for state in ["close", "reopen", "ready", "draft"] {
        let (response, calls) = run(
            json!({"operation":"set_state","review":reference(),"state":state}),
            true,
            "",
        );
        assert_eq!(response["outcome"]["status"], "success");
        if state == "ready" || state == "draft" {
            assert_eq!(calls.last().unwrap()["endpoint"], "graphql");
        }
    }
    let request =
        json!({"operation":"merge","review":reference(),"revision":"abc","method":"squash"});
    let (response, calls) = run(request.clone(), true, "");
    assert_eq!(response["outcome"]["status"], "success");
    assert_eq!(
        calls.last().unwrap()["body"],
        json!({"sha":"abc","merge_method":"squash"})
    );
    let (response, _) = run(request, true, "blocked");
    assert_eq!(response["outcome"]["status"], "error");
    let (response, _) = run(
        json!({"operation":"set_state","review":reference(),"state":"draft"}),
        true,
        "graphql-error",
    );
    assert_eq!(response["outcome"]["status"], "error");
}
#[test]
fn ci_details_and_actions_are_bound_to_the_displayed_revision() {
    let (response, _) = run(
        json!({"operation":"check_details","review":reference(),"revision":"abc","check_id":"runs:3"}),
        false,
        "",
    );
    assert!(
        response["outcome"]["result"]["annotations"][0]
            .as_str()
            .unwrap()
            .contains("Failed assertion")
    );
    let (response, _) = run(
        json!({"operation":"check_details","review":reference(),"revision":"abc","check_id":"workflows:9"}),
        false,
        "",
    );
    assert!(
        response["outcome"]["result"]["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a.as_str().unwrap().contains("Compile"))
    );
    for operation in ["rerun_check", "cancel_check"] {
        let request = json!({"operation":operation,"review":reference(),"revision":"abc","check_id":"workflows:9","failed_only":true});
        let (response, calls) = run(request.clone(), true, "");
        assert_eq!(response["outcome"]["status"], "success");
        assert!(
            calls.last().unwrap()["endpoint"]
                .as_str()
                .unwrap()
                .ends_with(if operation == "rerun_check" {
                    "rerun-failed-jobs"
                } else {
                    "cancel"
                })
        );
        let (response, calls) = run(request, true, "wrong-run");
        assert_eq!(response["outcome"]["status"], "error");
        assert!(calls.iter().all(|c| c["method"] == "GET"));
    }
}
#[test]
fn rejects_cross_host_writes_and_never_retries_an_uncertain_post() {
    let mut request =
        json!({"operation":"comment","review":reference(),"body":"hello","reply_to":null});
    let (response, calls) = run(request.clone(), true, "lost");
    assert_eq!(response["outcome"]["status"], "error");
    assert_eq!(calls.len(), 1);
    request["review"]["host"] = json!("other.test");
    let (response, calls) = run(request, true, "");
    assert_eq!(response["outcome"]["status"], "error");
    assert!(calls.is_empty());
}

#[test]
fn metadata_catalogs_and_review_states_use_repository_scoped_reads() {
    for (operation, kind, field) in [
        ("labels", "labels", "labels"),
        ("reviewer_candidates", "people", "people"),
        ("reviewers", "people", "people"),
    ] {
        let (response, calls) = run(
            json!({"operation":operation,"review":reference(),"cursor":null}),
            false,
            "",
        );
        assert_eq!(response["outcome"]["status"], "success", "{response}");
        assert_eq!(response["outcome"]["result"]["kind"], kind);
        assert!(
            !response["outcome"]["result"][field]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(calls.iter().all(|c| c["method"] == "GET"));
        if operation == "reviewers" {
            let people = &response["outcome"]["result"]["people"];
            assert_eq!(people.as_array().unwrap().len(), 2);
            assert_eq!(people[0]["state"], "approved");
            assert_eq!(people[1]["state"], "changes_requested");
        }
    }
    let (response, _) = run(json!({"operation":"get","review":reference()}), false, "");
    assert_eq!(
        response["outcome"]["result"]["review"]["labels"][0]["id"],
        "bug"
    );
    assert_eq!(
        response["outcome"]["result"]["review"]["merge_conflicts"],
        false
    );
}
#[test]
fn label_edits_are_incremental_and_review_removal_only_removes_requests() {
    for (request, method, endpoint) in [
        (
            json!({"operation":"add_labels","review":reference(),"labels":["bug"]}),
            "POST",
            "repos/team/repo/issues/42/labels",
        ),
        (
            json!({"operation":"remove_label","review":reference(),"label":"needs review/ci"}),
            "DELETE",
            "repos/team/repo/issues/42/labels/needs%20review%2Fci",
        ),
        (
            json!({"operation":"remove_reviewers","review":reference(),"reviewers":["reviewer"]}),
            "DELETE",
            "repos/team/repo/pulls/42/requested_reviewers",
        ),
    ] {
        let (response, calls) = run(request, true, "");
        assert_eq!(response["outcome"]["status"], "success", "{response}");
        let call = calls.last().unwrap();
        assert_eq!(call["method"], method);
        assert_eq!(call["endpoint"], endpoint);
    }
    for request in [
        json!({"operation":"add_labels","review":reference(),"labels":[""]}),
        json!({"operation":"remove_reviewers","review":reference(),"reviewers":["../bad"]}),
    ] {
        let (response, calls) = run(request, true, "");
        assert_eq!(response["outcome"]["error"]["code"], "invalid_parameter");
        assert!(calls.is_empty());
    }
}

#[test]
fn metadata_controls_respect_repository_permissions_and_support_team_requests() {
    let (response, _) = run(
        json!({"operation":"get", "review":reference()}),
        false,
        "read-only",
    );
    let actions = response["outcome"]["result"]["review"]["actions"]
        .as_array()
        .unwrap();
    assert!(!actions.contains(&json!("add_labels")));
    assert!(!actions.contains(&json!("request_reviewers")));
    let (response, calls) = run(
        json!({"operation":"remove_reviewers", "review":reference(), "reviewers":["team:core", "person"]}),
        true,
        "",
    );
    assert_eq!(response["outcome"]["status"], "success");
    assert_eq!(
        calls.last().unwrap()["body"],
        json!({"reviewers":["person"], "team_reviewers":["core"]})
    );
}
