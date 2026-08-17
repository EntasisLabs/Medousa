//! Handwritten helpers must not embed daemon `/v1` route literals.
//!
//! Generated tables, reconnect path-rewriter fixtures, and this test are
//! excluded. Architecture CI runs this test via `scripts/check-api-contract.sh`.

use std::path::{Path, PathBuf};

fn skip(path: &Path) -> bool {
    let text = path.to_string_lossy();
    text.contains("/generated/") || text.ends_with("reconnect.rs")
}

fn collect_hits(dir: &Path, hits: &mut Vec<String>) {
    let entries = std::fs::read_dir(dir).unwrap_or_else(|_| panic!("read {}", dir.display()));
    for entry in entries {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.is_dir() {
            collect_hits(&path, hits);
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("rs") || skip(&path) {
            continue;
        }
        let source = std::fs::read_to_string(&path).expect("read helper");
        for (index, line) in source.lines().enumerate() {
            if line.contains("\"/v1/") || line.contains("\"/v1\"") {
                hits.push(format!("{}:{}: {line}", path.display(), index + 1));
            }
        }
    }
}

#[test]
fn helpers_do_not_embed_daemon_route_literals() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut hits = Vec::new();
    collect_hits(&root, &mut hits);
    assert!(
        hits.is_empty(),
        "helpers still embed /v1 literals:\n{}",
        hits.join("\n")
    );
}
