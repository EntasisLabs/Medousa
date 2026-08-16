//! P06 — Vault backend evidence harness (H07.0).
//!
//! CI-sized fixtures stay small (24–32 notes). Scale runs are retained/nightly:
//! `MEDOUSA_P06_NOTES=100000 cargo run --example p06_vault_backend`.
//!
//! Records walks, reads, link rebuilds, list/get/search/write latency. Does not
//! assert P06 target properties — those gates attach in later H07 cars.

use std::path::Path;
use std::time::Instant;

use medousa::vault::baseline::vault_baseline_counters;
use medousa::vault::fixtures::{
    VaultFixtureShape, VaultFixtureSpec, generate_vault_fixture,
};
use medousa::vault::service::VaultService;
use medousa::vault::store::vault_store;
use tempfile::tempdir;

fn shapes() -> [VaultFixtureShape; 4] {
    [
        VaultFixtureShape::Shallow,
        VaultFixtureShape::Deep,
        VaultFixtureShape::Wide,
        VaultFixtureShape::LinkHeavy,
    ]
}

fn note_counts() -> Vec<usize> {
    if let Ok(raw) = std::env::var("MEDOUSA_P06_NOTES")
        && let Ok(value) = raw.parse::<usize>()
    {
        return vec![value.max(1)];
    }
    // CI defaults — deliberately small; 100k is nightly via env.
    vec![24, 64]
}

fn run_shape(shape: VaultFixtureShape, notes: usize) {
    let dir = tempdir().expect("tempdir");
    let root = dir.path().join(shape.as_str());
    let spec = VaultFixtureSpec::scaled(shape, notes);
    generate_vault_fixture(&root, &spec).expect("generate fixture");

    medousa::vault::roots::set_test_vault_root_override(Some(
        root.canonicalize().expect("canonical root"),
    ));
    let counters = vault_baseline_counters();
    counters.reset();

    let cold = Instant::now();
    vault_store().refresh_from_disk().expect("refresh");
    let cold_ms = cold.elapsed().as_secs_f64() * 1000.0;

    let warm_list = Instant::now();
    let listed = VaultService::list_notes(None, 50, None, None);
    let list_ms = warm_list.elapsed().as_secs_f64() * 1000.0;

    let path = listed
        .notes
        .first()
        .map(|note| note.path.clone())
        .unwrap_or_else(|| spec_path_fallback(shape));
    let warm_get = Instant::now();
    let _ = VaultService::get_note(&path);
    let get_ms = warm_get.elapsed().as_secs_f64() * 1000.0;

    let warm_search = Instant::now();
    let _ = VaultService::search(Some("index="), 10, None);
    let search_ms = warm_search.elapsed().as_secs_f64() * 1000.0;

    let write_path = format!("bench/write-{}.md", notes);
    let write = Instant::now();
    let _ = VaultService::write_note(
        None,
        &medousa::daemon_api::VaultWriteRequest {
            path: Some(write_path),
            content: "# Bench\n\nwrite\n".to_string(),
            ..Default::default()
        },
        None,
    );
    let write_ms = write.elapsed().as_secs_f64() * 1000.0;

    let snap = counters.snapshot();
    println!(
        "p06_harness platform={} shape={} notes={} cold_ms={cold_ms:.3} list_ms={list_ms:.3} get_ms={get_ms:.3} search_ms={search_ms:.3} write_ms={write_ms:.3}",
        std::env::consts::OS,
        shape.as_str(),
        notes
    );
    println!("{}", snap.render_line(&format!("{}:{notes}", shape.as_str())));
    println!(
        "p06_retained_dir_bytes={}",
        dir_bytes(root.as_path())
    );

    medousa::vault::roots::set_test_vault_root_override(None);
    let _ = vault_store().refresh_from_disk();
}

fn spec_path_fallback(shape: VaultFixtureShape) -> String {
    match shape {
        VaultFixtureShape::Shallow => "note-0000.md".into(),
        VaultFixtureShape::Deep => "d0/leaf-0000.md".into(),
        VaultFixtureShape::Wide => "bucket-0/note-0000.md".into(),
        VaultFixtureShape::LinkHeavy => "hub.md".into(),
    }
}

fn dir_bytes(path: &Path) -> u64 {
    let mut total = 0u64;
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            total = total.saturating_add(dir_bytes(&path));
        } else if let Ok(meta) = entry.metadata() {
            total = total.saturating_add(meta.len());
        }
    }
    total
}

fn main() {
    println!(
        "p06_harness=start platform={} arch={}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    for notes in note_counts() {
        for shape in shapes() {
            run_shape(shape, notes);
        }
    }
    println!("p06_harness=done");
}
