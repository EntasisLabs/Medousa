//! Shared golden cases must match generated operation tables.

use std::collections::HashMap;
use std::path::PathBuf;

use medousa_sdk::operations;
use serde::Deserialize;

#[derive(Deserialize)]
struct GoldenFile {
    cases: Vec<GoldenCase>,
}

#[derive(Deserialize)]
struct GoldenCase {
    id: String,
    method: String,
    path: String,
    streaming: bool,
    #[serde(default)]
    params: HashMap<String, String>,
    expanded: Option<String>,
}

fn load_cases() -> Vec<GoldenCase> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../sdk-contract/golden/client-cases.json");
    let raw = std::fs::read_to_string(path).expect("read golden client cases");
    serde_json::from_str::<GoldenFile>(&raw)
        .expect("parse golden client cases")
        .cases
}

#[test]
fn generated_ops_match_shared_golden_cases() {
    for case in load_cases() {
        let op = operations::by_id(&case.id).unwrap_or_else(|| panic!("missing {}", case.id));
        assert_eq!(op.method, case.method, "{}", case.id);
        assert_eq!(op.path, case.path, "{}", case.id);
        assert_eq!(op.streaming, case.streaming, "{}", case.id);
        assert_ne!(op.method, "SSE", "{}", case.id);
        let pairs: Vec<(&str, &str)> = case
            .params
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect();
        let expanded = medousa_sdk::generated::expand_path(op.path, &pairs).expect(&case.id);
        if let Some(expected) = &case.expanded {
            assert_eq!(&expanded, expected, "{}", case.id);
        }
    }
}

#[test]
fn golden_mutation_rejects_wrong_verb_or_path() {
    let health = operations::by_id("health.get").expect("health.get");
    assert_ne!(health.method, "POST");
    assert_ne!(health.path, "/v1/healthz");
    let stream = operations::by_id("workspace.stream.get").expect("workspace.stream.get");
    assert!(stream.streaming);
    assert!(!health.streaming);
}
