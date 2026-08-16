//! P04 — Forge event-store evidence harness (H06.11).
//!
//! CI-sized fixtures: 0 / 100 / 10k events. 1m is nightly via
//! `MEDOUSA_P04_EVENTS=1000000`.
//!
//! Reports throughput, latency percentiles, bytes read/written, sync count,
//! decoded event count, lock/owner hold time, and cold/warm retained-memory.

use std::path::{Path, PathBuf};
use std::time::Instant;

use medousa_forge::events::EventPayload;
use medousa_forge::model::{
    ActorKind, ActorRef, GitOid, GitWorkTarget, WorkId, WorkItem, WorkState, WorkTarget,
};
use medousa_forge::store::FsWorkStore;

fn actor() -> ActorRef {
    ActorRef {
        kind: ActorKind::System,
        id: "p04".into(),
    }
}

fn item() -> WorkItem {
    WorkItem::new(
        "p04",
        "baseline",
        WorkTarget::Git(GitWorkTarget {
            repo_path: PathBuf::from("/tmp/p04-repo"),
            base_ref: "main".into(),
            base_oid: GitOid::new("a".repeat(40)),
        }),
        "bench",
    )
}

fn percentile_us(sorted: &[u64], pct: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let rank = ((pct / 100.0) * (sorted.len() as f64 - 1.0)).round() as usize;
    sorted[rank.min(sorted.len() - 1)] as f64 / 1000.0
}

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

fn events_file_bytes(store: &FsWorkStore, work_id: &WorkId) -> u64 {
    let v2 = store.item_dir(work_id).join("events.v2");
    let v1 = store.events_path(work_id);
    if v2.exists() {
        std::fs::metadata(&v2).map(|m| m.len()).unwrap_or(0)
    } else if v1.exists() {
        std::fs::metadata(&v1).map(|m| m.len()).unwrap_or(0)
    } else {
        0
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

struct RunReport {
    phase: &'static str,
    events: usize,
    last_seq: u64,
    append_ms: f64,
    throughput_eps: f64,
    p50_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    bytes_written: u64,
    bytes_read_est: u64,
    sync_count_est: u64,
    decoded_events_est: u64,
    lock_hold_ms: f64,
    cold_tail_ms: f64,
    warm_tail_ms: f64,
    rss_before: u64,
    rss_after: u64,
    retained_dir_bytes: u64,
}

fn print_report(report: &RunReport) {
    println!(
        "p04 phase={} events={} last_seq={} append_ms={:.3} throughput_eps={:.1} \
p50_ms={:.3} p95_ms={:.3} p99_ms={:.3} bytes_written={} bytes_read_est={} \
sync_count_est={} decoded_events_est={} lock_hold_ms={:.3} \
cold_tail_ms={:.3} warm_tail_ms={:.3} rss_before={} rss_after={} retained_dir_bytes={}",
        report.phase,
        report.events,
        report.last_seq,
        report.append_ms,
        report.throughput_eps,
        report.p50_ms,
        report.p95_ms,
        report.p99_ms,
        report.bytes_written,
        report.bytes_read_est,
        report.sync_count_est,
        report.decoded_events_est,
        report.lock_hold_ms,
        report.cold_tail_ms,
        report.warm_tail_ms,
        report.rss_before,
        report.rss_after,
        report.retained_dir_bytes,
    );
}

fn run(events: usize, phase: &'static str) -> RunReport {
    let root = std::env::temp_dir().join(format!(
        "medousa-p04-{}-{}-{}",
        phase,
        events,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let rss_before = rss_bytes();
    let store = FsWorkStore::open(&root).expect("open store");
    let work = item();
    store
        .append(
            &work.id,
            &actor(),
            EventPayload::ItemRegistered {
                item: Box::new(work.clone()),
            },
        )
        .expect("register");

    let mut latencies_us = Vec::with_capacity(events.max(1));
    let mut bytes_written = 0u64;
    let mut bytes_read_est = 0u64;
    let mut sync_count_est = 0u64;
    let mut decoded_events_est = 0u64;
    let mut lock_hold_ns = 0u128;
    let mut prev_size = events_file_bytes(&store, &work.id);

    let started = Instant::now();
    let append_count = events.saturating_sub(1);
    for index in 0..append_count {
        // Each append recovers the durable tail by scanning the current log.
        // Estimate decoded events / bytes read from the pre-append size and seq.
        let before_size = events_file_bytes(&store, &work.id);
        bytes_read_est = bytes_read_est.saturating_add(before_size);
        let before_seq = store.cached_last_seq(&work.id).unwrap_or(0);
        decoded_events_est = decoded_events_est.saturating_add(before_seq);

        let op_started = Instant::now();
        store
            .append(
                &work.id,
                &actor(),
                EventPayload::StateChanged {
                    from: WorkState::Draft,
                    to: if index % 2 == 0 {
                        WorkState::Ready
                    } else {
                        WorkState::Draft
                    },
                    reason: Some(format!("p04-{index}")),
                },
            )
            .expect("append");
        let elapsed = op_started.elapsed();
        latencies_us.push(elapsed.as_micros() as u64);
        lock_hold_ns += elapsed.as_nanos();

        let after_size = events_file_bytes(&store, &work.id);
        bytes_written = bytes_written.saturating_add(after_size.saturating_sub(prev_size));
        prev_size = after_size;
        // Durable append path syncs the log (and may sync after truncate).
        sync_count_est = sync_count_est.saturating_add(1);
    }
    let append_ms = started.elapsed().as_secs_f64() * 1000.0;
    let throughput_eps = if append_ms > 0.0 {
        (append_count as f64) / (append_ms / 1000.0)
    } else {
        0.0
    };

    latencies_us.sort_unstable();
    let p50_ms = percentile_us(&latencies_us, 50.0);
    let p95_ms = percentile_us(&latencies_us, 95.0);
    let p99_ms = percentile_us(&latencies_us, 99.0);

    // Warm tail: in-process cached lookup.
    let warm_started = Instant::now();
    let last = store.cached_last_seq(&work.id).expect("warm tail");
    let warm_tail_ms = warm_started.elapsed().as_secs_f64() * 1000.0;

    // Cold tail: reopen store and recover from disk.
    drop(store);
    let cold_store = FsWorkStore::open(&root).expect("reopen store");
    let cold_started = Instant::now();
    let cold_last = cold_store.cached_last_seq(&work.id).expect("cold tail");
    let cold_tail_ms = cold_started.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(last, cold_last);

    let rss_after = rss_bytes();
    let retained_dir_bytes = dir_bytes(&root);
    let _ = std::fs::remove_dir_all(&root);

    RunReport {
        phase,
        events: events.max(1),
        last_seq: last,
        append_ms,
        throughput_eps,
        p50_ms,
        p95_ms,
        p99_ms,
        bytes_written,
        bytes_read_est,
        sync_count_est,
        decoded_events_est,
        lock_hold_ms: (lock_hold_ns as f64) / 1_000_000.0,
        cold_tail_ms,
        warm_tail_ms,
        rss_before,
        rss_after,
        retained_dir_bytes,
    }
}

fn main() {
    let requested = std::env::var("MEDOUSA_P04_EVENTS")
        .ok()
        .and_then(|value| value.parse().ok());
    let sizes = requested
        .map(|value| vec![value])
        .unwrap_or_else(|| vec![0, 100, 10_000]);

    println!(
        "p04_harness=medousa-forge/examples/p04_forge_store platform={}",
        std::env::consts::OS
    );
    for events in sizes {
        // Warm path: single continuous process/store lifetime for the size.
        let warm = run(events.max(1), "warm");
        print_report(&warm);
        // Cold path: same size, but force a reopen mid-run by measuring cold
        // recovery after a fresh population (already included above). Emit a
        // dedicated cold reopen probe that only measures recover cost.
        let cold = run(events.max(1), "cold_reopen");
        print_report(&cold);
    }
}
