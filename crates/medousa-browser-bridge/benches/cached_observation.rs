use std::hint::black_box;
use std::time::Instant;

use medousa_browser_bridge::{
    BrowserObservationCapture, BrowserObservationViewport, BrowserSemanticNode, TabGroupManager,
    TabOpenedBy,
};

const ITERATIONS: usize = 5_000;
const NODE_COUNT: usize = 1_024;

fn main() {
    let group = TabGroupManager::create_group("driver:observation-benchmark", None, None);
    let tab = TabGroupManager::navigate_active_tab(
        &group.id,
        "https://benchmark.invalid/",
        Some("Observation benchmark"),
        TabOpenedBy::Agent,
    )
    .expect("benchmark tab");
    let nodes = (0..NODE_COUNT)
        .map(|index| BrowserSemanticNode {
            element_ref: format!("ref:{index}"),
            parent_ref: None,
            role: "button".to_string(),
            name: format!("Benchmark target {index}"),
            tag: "button".to_string(),
            value: None,
            href: None,
            disabled: false,
            checked: None,
            selected: None,
            bounds: None,
            sensitive: false,
        })
        .collect();
    TabGroupManager::record_observation(
        &group.id,
        BrowserObservationCapture {
            tab_id: tab.id.clone(),
            url: tab.url.clone(),
            title: tab.title.clone(),
            document_id: "document:benchmark".to_string(),
            viewport: BrowserObservationViewport {
                width: 1_440,
                height: 900,
                scroll_x: 0,
                scroll_y: 0,
                device_scale_factor: 2.0,
            },
            nodes,
            truncated: false,
            unchanged: false,
            captured_at_ms: 1,
        },
        None,
        NODE_COUNT,
    )
    .expect("prime observation mirror");

    let mut samples_ns = Vec::with_capacity(ITERATIONS);
    for _ in 0..ITERATIONS {
        let started = Instant::now();
        let observation =
            TabGroupManager::current_observation(&group.id, &tab.id).expect("cached observation");
        black_box(observation);
        samples_ns.push(started.elapsed().as_nanos());
    }
    TabGroupManager::remove_group(&group.id).expect("remove benchmark group");

    samples_ns.sort_unstable();
    let p50 = percentile(&samples_ns, 50);
    let p95 = percentile(&samples_ns, 95);
    let p99 = percentile(&samples_ns, 99);
    println!(
        "cached semantic observation: iterations={ITERATIONS} nodes={NODE_COUNT} p50={p50}ns p95={p95}ns p99={p99}ns"
    );
    println!(
        "{{\"probe\":\"world_cached_observation\",\"iterations\":{ITERATIONS},\"nodes\":{NODE_COUNT},\"p50_ns\":{p50},\"p95_ns\":{p95},\"p99_ns\":{p99}}}"
    );
}

fn percentile(samples: &[u128], percent: usize) -> u128 {
    let index = ((samples.len() - 1) * percent) / 100;
    samples[index]
}
