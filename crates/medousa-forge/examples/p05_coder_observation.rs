//! P05 — Coder observation evidence harness (H06.11).
//!
//! Covers clean, small-dirty, many-untracked, large-diff, concurrent-mutation,
//! and bounded/truncated observation scenarios with wall-clock and retained-
//! memory budgets. CI size defaults to 1k tracked files via `MEDOUSA_P05_FILES`.

use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use medousa_forge::git::GitEngine;
use medousa_forge::model::WorkId;
use medousa_forge::observation::{
    GenerationCapture, GenerationSource, ObservationBudgets, ObservationCompleteness,
    SharedWatcherFence, WorkspaceObserver,
};

fn rss_bytes() -> u64 {
    let Ok(output) = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p"])
        .arg(std::process::id().to_string())
        .output()
    else {
        return 0;
    };
    let text = String::from_utf8_lossy(&output.stdout);
    text.trim()
        .parse::<u64>()
        .map(|kib| kib.saturating_mul(1024))
        .unwrap_or(0)
}

fn git_init(root: &Path) {
    fs::create_dir_all(root).expect("repo");
    std::process::Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(root)
        .status()
        .expect("git init");
    std::process::Command::new("git")
        .args(["config", "user.email", "p05@example.com"])
        .current_dir(root)
        .status()
        .expect("git email");
    std::process::Command::new("git")
        .args(["config", "user.name", "p05"])
        .current_dir(root)
        .status()
        .expect("git name");
}

fn write_tracked(root: &Path, files: usize) {
    for index in 0..files {
        fs::write(
            root.join(format!("f{index}.txt")),
            format!("file {index}\n"),
        )
        .unwrap();
    }
    std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(root)
        .status()
        .expect("git add");
    std::process::Command::new("git")
        .args(["commit", "-m", "p05-base", "--quiet"])
        .current_dir(root)
        .status()
        .expect("git commit");
}

fn capture(generation: u64) -> GenerationCapture {
    GenerationCapture {
        workspace_generation: generation,
        watcher_generation: generation,
        repository_generation: generation,
        watcher_overflow: false,
    }
}

struct ScenarioReport {
    scenario: &'static str,
    files: usize,
    completeness: String,
    limits: String,
    wall_ms: f64,
    rss_before: u64,
    rss_after: u64,
    cache_entries: usize,
    changed_paths: usize,
}

fn print_report(report: &ScenarioReport) {
    println!(
        "p05 scenario={} files={} completeness={} limits={} wall_ms={:.3} \
rss_before={} rss_after={} cache_entries={} changed_paths={}",
        report.scenario,
        report.files,
        report.completeness,
        report.limits,
        report.wall_ms,
        report.rss_before,
        report.rss_after,
        report.cache_entries,
        report.changed_paths,
    );
}

fn observe_report(
    scenario: &'static str,
    files: usize,
    root: &Path,
    git: &GitEngine,
    observer: &WorkspaceObserver,
    source: &impl GenerationSource,
    budgets: ObservationBudgets,
) -> ScenarioReport {
    let rss_before = rss_bytes();
    let started = Instant::now();
    let observation = observer
        .observe_with_budgets(
            git,
            &WorkId::from(format!("work1-p05-{scenario}")),
            root,
            source,
            budgets,
        )
        .expect("observe");
    let wall_ms = started.elapsed().as_secs_f64() * 1000.0;
    let rss_after = rss_bytes();
    ScenarioReport {
        scenario,
        files,
        completeness: format!("{:?}", observation.completeness),
        limits: format!("{:?}", observation.limits_hit),
        wall_ms,
        rss_before,
        rss_after,
        cache_entries: observer.cache_len(),
        changed_paths: observation.changed_paths.len(),
    }
}

struct FlipSource {
    first: GenerationCapture,
    second: GenerationCapture,
    calls: Mutex<usize>,
}

impl GenerationSource for FlipSource {
    fn capture(&self) -> GenerationCapture {
        let mut calls = self.calls.lock().expect("calls");
        *calls += 1;
        if *calls == 1 {
            self.first.clone()
        } else {
            self.second.clone()
        }
    }
}

fn run_scenarios(files: usize) {
    let git = GitEngine::detect().expect("git");
    let base = std::env::temp_dir().join(format!(
        "medousa-p05-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    // --- clean ---
    {
        let root = base.join("clean");
        git_init(&root);
        write_tracked(&root, files);
        let observer = WorkspaceObserver::default();
        let report = observe_report(
            "clean",
            files,
            &root,
            &git,
            &observer,
            &capture(1),
            ObservationBudgets::for_resume(false),
        );
        print_report(&report);
    }

    // --- small dirty ---
    {
        let root = base.join("small_dirty");
        git_init(&root);
        write_tracked(&root, files);
        fs::write(root.join("f0.txt"), "dirty-small\n").unwrap();
        let observer = WorkspaceObserver::default();
        let report = observe_report(
            "small_dirty",
            files,
            &root,
            &git,
            &observer,
            &capture(2),
            ObservationBudgets::for_resume(false),
        );
        print_report(&report);
    }

    // --- many untracked ---
    {
        let root = base.join("many_untracked");
        git_init(&root);
        write_tracked(&root, files.clamp(20, 200));
        let untracked = 500usize;
        for index in 0..untracked {
            fs::write(
                root.join(format!("u{index}.txt")),
                format!("untracked {index}\n"),
            )
            .unwrap();
        }
        let observer = WorkspaceObserver::default();
        let report = observe_report(
            "many_untracked",
            files.clamp(20, 200) + untracked,
            &root,
            &git,
            &observer,
            &capture(3),
            ObservationBudgets::for_resume(false),
        );
        print_report(&report);
    }

    // --- large diff ---
    {
        let root = base.join("large_diff");
        git_init(&root);
        write_tracked(&root, files.clamp(50, 400));
        for index in 0..(files.clamp(50, 400)) {
            fs::write(
                root.join(format!("f{index}.txt")),
                format!("changed {index}\n{}", "x".repeat(256)),
            )
            .unwrap();
        }
        let observer = WorkspaceObserver::default();
        let report = observe_report(
            "large_diff",
            files.clamp(50, 400),
            &root,
            &git,
            &observer,
            &capture(4),
            ObservationBudgets::for_resume(false),
        );
        print_report(&report);
    }

    // --- concurrent mutation (generation flips mid-observe) ---
    {
        let root = base.join("concurrent_mutation");
        git_init(&root);
        write_tracked(&root, files.clamp(20, 200));
        let observer = WorkspaceObserver::default();
        let source = FlipSource {
            first: capture(5),
            second: GenerationCapture {
                workspace_generation: 6,
                watcher_generation: 6,
                repository_generation: 6,
                watcher_overflow: false,
            },
            calls: Mutex::new(0),
        };
        let report = observe_report(
            "concurrent_mutation",
            files.clamp(20, 200),
            &root,
            &git,
            &observer,
            &source,
            ObservationBudgets::for_resume(false),
        );
        print_report(&report);
        assert_eq!(
            report.completeness,
            format!("{:?}", ObservationCompleteness::Unknown)
        );
    }

    // --- concurrent mutation via watcher fence bump while hashing ---
    {
        let root = base.join("concurrent_watcher");
        git_init(&root);
        write_tracked(&root, 40);
        for index in 0..80 {
            fs::write(root.join(format!("u{index}.bin")), vec![1u8; 64 * 1024]).unwrap();
        }
        let fence = SharedWatcherFence::new();
        let observer = WorkspaceObserver::default();
        let bound = fence.bind(10, 10);
        let fence_clone = fence.clone();
        let root_clone = root.clone();
        let stopper = Arc::new(Mutex::new(false));
        let stopper_clone = Arc::clone(&stopper);
        let mutator = thread::spawn(move || {
            while !*stopper_clone.lock().unwrap() {
                fence_clone.bump_generation();
                let _ = fs::write(root_clone.join("mut.txt"), b"mut");
                thread::sleep(Duration::from_millis(2));
            }
        });
        let report = observe_report(
            "concurrent_watcher",
            120,
            &root,
            &git,
            &observer,
            &bound,
            ObservationBudgets::for_resume(false),
        );
        *stopper.lock().unwrap() = true;
        let _ = mutator.join();
        print_report(&report);
    }

    // --- bounded / truncated observation ---
    {
        let root = base.join("bounded_truncated");
        git_init(&root);
        write_tracked(&root, 30);
        for index in 0..40 {
            fs::write(root.join(format!("big{index}.bin")), vec![7u8; 256 * 1024]).unwrap();
        }
        let observer = WorkspaceObserver::default();
        let tight = ObservationBudgets {
            aggregate_bytes: 64 * 1024,
            per_file_bytes: 32 * 1024,
            wall: Duration::from_millis(50),
            untracked_entries: 8,
            diff_bytes: 16 * 1024,
        };
        let report = observe_report(
            "bounded_truncated",
            70,
            &root,
            &git,
            &observer,
            &capture(7),
            tight,
        );
        print_report(&report);
        assert_ne!(
            report.completeness,
            format!("{:?}", ObservationCompleteness::Exact)
        );
    }

    // --- wall-clock / retained-memory budget envelope (resume budgets) ---
    {
        let root = base.join("budget_envelope");
        git_init(&root);
        write_tracked(&root, files.clamp(50, 300));
        let observer = WorkspaceObserver::default();
        let report = observe_report(
            "budget_envelope",
            files.clamp(50, 300),
            &root,
            &git,
            &observer,
            &capture(8),
            ObservationBudgets::for_resume(true),
        );
        print_report(&report);
        println!(
            "p05 budget_note wall_budget_ms={} rss_delta={}",
            ObservationBudgets::for_resume(true).wall.as_millis(),
            report.rss_after.saturating_sub(report.rss_before),
        );
    }

    let _ = fs::remove_dir_all(&base);
}

fn main() {
    let files = std::env::var("MEDOUSA_P05_FILES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1_000usize);
    println!(
        "p05_harness=medousa-forge/examples/p05_coder_observation platform={} files={}",
        std::env::consts::OS,
        files
    );
    run_scenarios(files);
}
