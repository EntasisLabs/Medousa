//! P04 — Forge event-store baseline (H06.0.1).
//!
//! CI-sized fixtures: 0 / 100 / 10k events. 1m is nightly via
//! `MEDOUSA_P04_EVENTS=1000000`.

use std::path::PathBuf;
use std::time::Instant;

use medousa_forge::events::EventPayload;
use medousa_forge::model::{
    ActorKind, ActorRef, GitOid, GitWorkTarget, WorkItem, WorkState, WorkTarget,
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

fn run(events: usize) {
    let root = std::env::temp_dir().join(format!(
        "medousa-p04-{}-{}",
        events,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
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
    let started = Instant::now();
    for index in 0..events.saturating_sub(1) {
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
    }
    let append_ms = started.elapsed().as_secs_f64() * 1000.0;
    let load_started = Instant::now();
    let last = store.cached_last_seq(&work.id).expect("tail");
    let load_ms = load_started.elapsed().as_secs_f64() * 1000.0;
    println!(
        "p04 events={events} last_seq={last} append_ms={append_ms:.3} tail_ms={load_ms:.3}"
    );
}

fn main() {
    let requested = std::env::var("MEDOUSA_P04_EVENTS")
        .ok()
        .and_then(|value| value.parse().ok());
    for events in requested.map(|value| vec![value]).unwrap_or_else(|| vec![0, 100, 10_000]) {
        run(events.max(1));
    }
}
