//! P05 — Coder observation baseline (H06.0.2).
//!
//! CI-sized fixture: 1k files. Larger sets via `MEDOUSA_P05_FILES`.

use std::fs;
use std::time::Instant;

use medousa_forge::git::GitEngine;
use medousa_forge::model::WorkId;
use medousa_forge::observation::{GenerationCapture, ObservationCompleteness, WorkspaceObserver};

fn main() {
    let files = std::env::var("MEDOUSA_P05_FILES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1_000usize);
    let root = std::env::temp_dir().join(format!(
        "medousa-p05-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("repo");
    let git = GitEngine::detect().expect("git");
    std::process::Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(&root)
        .status()
        .expect("git init");
    for index in 0..files {
        fs::write(root.join(format!("f{index}.txt")), format!("file {index}")).unwrap();
    }
    let observer = WorkspaceObserver::default();
    let started = Instant::now();
    let observation = observer
        .observe_exact(
            &git,
            &WorkId::from("work1-p05".to_string()),
            &root,
            &GenerationCapture {
                workspace_generation: 1,
                watcher_generation: 1,
                repository_generation: 1,
                watcher_overflow: false,
            },
            false,
        )
        .expect("observe");
    println!(
        "p05 files={files} completeness={:?} limits={:?} ms={:.3}",
        observation.completeness,
        observation.limits_hit,
        started.elapsed().as_secs_f64() * 1000.0
    );
    let _ = ObservationCompleteness::Exact;
}
