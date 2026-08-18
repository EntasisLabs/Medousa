//! P03 — Feed and workspace persistence micro-CI harness (H04 content, H12 gate).
//!
//! Appends a small number of feed events into an isolated store and reports
//! elapsed milliseconds. Scale runs: `MEDOUSA_P03_RECORDS=10000`.

use std::time::Instant;

use chrono::Utc;
use medousa::feed_store::FeedStore;
use medousa_types::feed::{FeedEvent, FeedSource};
use serde::Serialize;
use tempfile::tempdir;

#[derive(Serialize)]
struct Sample {
    fixture: &'static str,
    records: usize,
    append_ms: f64,
    tail_ms: f64,
    tail_len: usize,
}

fn record_count() -> usize {
    std::env::var("MEDOUSA_P03_RECORDS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(32)
}

fn event(index: usize) -> FeedEvent {
    FeedEvent {
        id: format!("p03-{index}"),
        feed_id: "workshop.pulse".to_string(),
        emitted_at_utc: Utc::now(),
        source: FeedSource::Agent.as_str().to_string(),
        summary: format!("p03 event {index}"),
        refs: Vec::new(),
        payload: None,
    }
}

#[tokio::main]
async fn main() {
    let records = record_count();
    let directory = tempdir().expect("tempdir");
    let root = directory
        .path()
        .canonicalize()
        .expect("canonical tempdir")
        .join("feeds");
    std::fs::create_dir_all(&root).expect("feeds dir");
    let store = FeedStore::new_in(root);

    let started = Instant::now();
    for index in 0..records {
        store
            .append("personal", "workshop.pulse", event(index))
            .await
            .expect("append");
    }
    let append_ms = started.elapsed().as_secs_f64() * 1_000.0;

    let tail_started = Instant::now();
    let tail = store.tail("personal", "workshop.pulse", records).await;
    let tail_ms = tail_started.elapsed().as_secs_f64() * 1_000.0;

    let sample = Sample {
        fixture: "P03-feed-append-v1",
        records,
        append_ms,
        tail_ms,
        tail_len: tail.len(),
    };
    println!("{}", serde_json::to_string(&sample).expect("serialize"));
}
