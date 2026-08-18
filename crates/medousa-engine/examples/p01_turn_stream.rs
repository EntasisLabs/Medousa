use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use medousa_engine::{Principal, TurnEnvelope, TurnEvent, TurnEventLog};
use serde::Serialize;

struct CountingAllocator;

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        ALLOCATED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        ALLOCATED_BYTES.fetch_add(size as u64, Ordering::Relaxed);
        unsafe { System.realloc(pointer, layout, size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[derive(Serialize)]
struct Sample {
    fixture: &'static str,
    fragments: usize,
    fragment_bytes: usize,
    transcript_messages: usize,
    subscribers: usize,
    elapsed_ms: f64,
    first_delta_us: f64,
    latency_p50_us: f64,
    latency_p95_us: f64,
    latency_p99_us: f64,
    allocations: u64,
    allocated_bytes: u64,
    journal_writes: u64,
    journal_flushes: u64,
    journal_bytes: u64,
    retained_events: usize,
    replay_events: usize,
    replay_us: f64,
}

fn percentile(values: &mut [u128], percentile: usize) -> f64 {
    values.sort_unstable();
    let index = (values.len().saturating_sub(1) * percentile) / 100;
    values[index] as f64 / 1_000.0
}

fn ci_mode() -> bool {
    std::env::var("MEDOUSA_P01_CI").ok().as_deref() == Some("1")
}

fn fragment_count() -> usize {
    std::env::var("MEDOUSA_P01_FRAGMENTS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(if ci_mode() { 256 } else { 10_000 })
}

fn run(fragment_bytes: usize, transcript_messages: usize, subscribers: usize) -> Sample {
    let fragments = fragment_count();
    let root = std::env::temp_dir().join(format!("medousa-p01-{}", uuid::Uuid::new_v4().simple()));
    let envelope = TurnEnvelope::new(
        format!("turn-{}", uuid::Uuid::new_v4().simple()),
        Principal::operator(),
    );
    let log = TurnEventLog::open_in(&root, envelope).expect("open benchmark journal");
    for index in 0..transcript_messages {
        log.append(TurnEvent::Notice {
            message: format!("existing message {index}"),
        }).expect("append benchmark history");
    }

    let delta = "x".repeat(fragment_bytes);
    let mut latencies = Vec::with_capacity(fragments);
    ALLOCATIONS.store(0, Ordering::Relaxed);
    ALLOCATED_BYTES.store(0, Ordering::Relaxed);
    let started = Instant::now();
    let mut first_delta_us = 0.0;
    let mut subscriber_seq = vec![transcript_messages as u64; subscribers];
    for index in 0..fragments {
        let before = Instant::now();
        let event = log.append(TurnEvent::ContentDelta {
            delta: delta.clone(),
        }).expect("append benchmark delta");
        for since in &mut subscriber_seq {
            let delivered = log.snapshot_since(*since);
            *since = event.seq();
            std::hint::black_box(delivered);
        }
        let latency = before.elapsed().as_nanos();
        if index == 0 {
            first_delta_us = latency as f64 / 1_000.0;
        }
        latencies.push(latency);
    }
    let elapsed_ms = started.elapsed().as_secs_f64() * 1_000.0;
    let allocations = ALLOCATIONS.load(Ordering::Relaxed);
    let allocated_bytes = ALLOCATED_BYTES.load(Ordering::Relaxed);
    let metrics = log.metrics();
    let replay_started = Instant::now();
    let replay = log.snapshot_since(0);
    let replay_us = replay_started.elapsed().as_secs_f64() * 1_000_000.0;
    let replay_events = replay.len();
    std::hint::black_box(replay);
    let latency_p50_us = percentile(&mut latencies.clone(), 50);
    let latency_p95_us = percentile(&mut latencies.clone(), 95);
    let latency_p99_us = percentile(&mut latencies, 99);
    drop(log);
    std::fs::remove_dir_all(root).expect("remove benchmark journal");

    Sample {
        fixture: "P01-turn-stream-spine-v1",
        fragments,
        fragment_bytes,
        transcript_messages,
        subscribers,
        elapsed_ms,
        first_delta_us,
        latency_p50_us,
        latency_p95_us,
        latency_p99_us,
        allocations,
        allocated_bytes,
        journal_writes: metrics.journal_writes,
        journal_flushes: metrics.journal_flushes,
        journal_bytes: metrics.journal_bytes,
        retained_events: metrics.retained_events,
        replay_events,
        replay_us,
    }
}

fn main() {
    let fragment_bytes = if ci_mode() {
        vec![32]
    } else {
        vec![1, 8, 32, 256]
    };
    let transcripts = if ci_mode() {
        vec![0]
    } else {
        vec![0, 100, 1_000]
    };
    let subscribers = if ci_mode() { vec![1] } else { vec![0, 1, 4] };
    for fragment_bytes in fragment_bytes {
        for transcript_messages in &transcripts {
            for subscriber_count in &subscribers {
                println!(
                    "{}",
                    serde_json::to_string(&run(
                        fragment_bytes,
                        *transcript_messages,
                        *subscriber_count
                    ))
                    .expect("serialize benchmark sample")
                );
            }
        }
    }
}
