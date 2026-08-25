use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use medousa_engine::{
    TURN_PIPELINE_BYTE_CAPACITY, TurnPipelineEmission, TurnPipelineEnvelope, TurnPipelineError,
    TurnPipelineHandle, TurnPipelineOutput,
};
use medousa_types::turn_stream::{TurnStreamEnvelopeV2, TurnStreamEventV2};
use serde::Serialize;
use tokio::sync::Semaphore;

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

struct BenchmarkOutput {
    started: Instant,
    first_publish_ns: AtomicU64,
    events: AtomicU64,
    content_bytes: AtomicU64,
}

impl TurnPipelineOutput for BenchmarkOutput {
    async fn publish(&self, emission: TurnPipelineEmission) -> Result<(), TurnPipelineError> {
        let elapsed = u64::try_from(self.started.elapsed().as_nanos()).unwrap_or(u64::MAX);
        let _ = self.first_publish_ns.compare_exchange(
            0,
            elapsed.max(1),
            Ordering::Relaxed,
            Ordering::Relaxed,
        );
        if let TurnPipelineEnvelope::V2(TurnStreamEnvelopeV2 {
            event: TurnStreamEventV2::ContentAppend { text },
            ..
        }) = emission.envelope
        {
            self.content_bytes.fetch_add(
                u64::try_from(text.len()).unwrap_or(u64::MAX),
                Ordering::Relaxed,
            );
        }
        self.events.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}

#[derive(Serialize)]
struct Sample {
    fixture: &'static str,
    fragments: usize,
    fragment_bytes: usize,
    elapsed_ms: f64,
    first_publish_us: f64,
    admission_p50_us: f64,
    admission_p95_us: f64,
    admission_p99_us: f64,
    allocations: u64,
    allocated_bytes: u64,
    emitted_batches: u64,
    coalesced_commands: u64,
    message_high_water: usize,
    byte_high_water: usize,
    output_bytes: u64,
}

fn percentile(values: &mut [u128], percentile: usize) -> f64 {
    values.sort_unstable();
    let index = (values.len().saturating_sub(1) * percentile) / 100;
    values[index] as f64 / 1_000.0
}

async fn run(fragment_bytes: usize) -> Sample {
    let started = Instant::now();
    let output = Arc::new(BenchmarkOutput {
        started,
        first_publish_ns: AtomicU64::new(0),
        events: AtomicU64::new(0),
        content_bytes: AtomicU64::new(0),
    });
    let pipeline = TurnPipelineHandle::spawn(
        "p01-turn-pipeline",
        0,
        Arc::new(Semaphore::new(TURN_PIPELINE_BYTE_CAPACITY * 4)),
        Arc::clone(&output),
    );
    let delta = "x".repeat(fragment_bytes);
    let mut latencies = Vec::with_capacity(10_000);
    ALLOCATIONS.store(0, Ordering::Relaxed);
    ALLOCATED_BYTES.store(0, Ordering::Relaxed);

    for _ in 0..10_000 {
        let before = Instant::now();
        pipeline
            .admit(TurnStreamEventV2::ContentAppend {
                text: delta.clone(),
            })
            .await
            .expect("bounded pipeline admission");
        latencies.push(before.elapsed().as_nanos());
    }
    pipeline.flush().await.expect("pipeline flush");

    let elapsed_ms = started.elapsed().as_secs_f64() * 1_000.0;
    let allocations = ALLOCATIONS.load(Ordering::Relaxed);
    let allocated_bytes = ALLOCATED_BYTES.load(Ordering::Relaxed);
    let metrics = pipeline.metrics();
    let admission_p50_us = percentile(&mut latencies.clone(), 50);
    let admission_p95_us = percentile(&mut latencies.clone(), 95);
    let admission_p99_us = percentile(&mut latencies, 99);

    Sample {
        fixture: "P01-turn-pipeline-v2",
        fragments: 10_000,
        fragment_bytes,
        elapsed_ms,
        first_publish_us: output.first_publish_ns.load(Ordering::Relaxed) as f64 / 1_000.0,
        admission_p50_us,
        admission_p95_us,
        admission_p99_us,
        allocations,
        allocated_bytes,
        emitted_batches: output.events.load(Ordering::Relaxed),
        coalesced_commands: metrics.coalesced_commands,
        message_high_water: metrics.message_high_water,
        byte_high_water: metrics.byte_high_water,
        output_bytes: output.content_bytes.load(Ordering::Relaxed),
    }
}

#[tokio::main]
async fn main() {
    for fragment_bytes in [1, 8, 32, 256] {
        println!(
            "{}",
            serde_json::to_string(&run(fragment_bytes).await).expect("serialize benchmark sample")
        );
    }
}
