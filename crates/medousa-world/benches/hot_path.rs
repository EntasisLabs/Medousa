use std::collections::BTreeSet;
use std::hint::black_box;
use std::time::Instant;

use medousa_world::{
    WorldActionIntent, WorldAdmission, WorldAuthority, WorldAuthorityId, WorldCapability,
    WorldDriverId, WorldEffectClass, WorldGrantId, WorldGrantRequest, WorldId, WorldIntentId,
    WorldOwnership, WorldPrincipal, WorldPrincipalId, WorldResourceId, WorldResourceScope,
    WorldSessionSpec, WorldSurfaceKind, WorldTraceId,
};

const ITERATIONS: usize = 20_000;

fn main() {
    let world_id = WorldId::new("world:benchmark");
    let resource_id = WorldResourceId::new("browser-tab:benchmark");
    let system = WorldPrincipal::system(WorldPrincipalId::new("runtime:benchmark"));
    let agent = WorldPrincipal::agent(WorldPrincipalId::new("agent:benchmark"));
    let human = WorldPrincipal::human(WorldPrincipalId::new("human:benchmark"));
    let mut authority = WorldAuthority::new(128);
    authority
        .create_world(
            WorldSessionSpec {
                world_id: world_id.clone(),
                authority_id: WorldAuthorityId::new("workshop:benchmark"),
                driver_id: WorldDriverId::new("driver:benchmark"),
                ownership: WorldOwnership::Managed,
                surface: WorldSurfaceKind::Browser,
            },
            system.clone(),
            1,
        )
        .unwrap();
    authority
        .grant_capabilities(
            &world_id,
            WorldGrantRequest {
                grant_id: WorldGrantId::new("grant:benchmark"),
                issued_by: system.clone(),
                subject: agent.clone(),
                capabilities: [WorldCapability::Interact]
                    .into_iter()
                    .collect::<BTreeSet<_>>(),
                resource_scope: WorldResourceScope::All,
                expires_at_ms: None,
            },
            2,
        )
        .unwrap();
    authority
        .grant_capabilities(
            &world_id,
            WorldGrantRequest {
                grant_id: WorldGrantId::new("grant:benchmark-human"),
                issued_by: system.clone(),
                subject: human.clone(),
                capabilities: [WorldCapability::Interact]
                    .into_iter()
                    .collect::<BTreeSet<_>>(),
                resource_scope: WorldResourceScope::All,
                expires_at_ms: None,
            },
            3,
        )
        .unwrap();
    let lease = authority
        .acquire_control(&world_id, agent.clone(), None, 4)
        .unwrap();

    let mut samples_ns = Vec::with_capacity(ITERATIONS);
    for iteration in 0..ITERATIONS {
        let now_ms = 5 + iteration as u64;
        let state = authority.world(&world_id).unwrap();
        let idempotency_key = format!("benchmark:{iteration}");
        let intent = WorldActionIntent {
            intent_id: WorldIntentId::new(format!("intent:{iteration}")),
            trace_id: WorldTraceId::new("trace:benchmark"),
            principal: agent.clone(),
            resource_id: resource_id.clone(),
            expected_revision: state.revision,
            expected_control_generation: Some(lease.generation),
            required_capability: WorldCapability::Interact,
            effect_class: WorldEffectClass::LocalMutation,
            idempotency_key,
            permit_expires_at_ms: now_ms + 10_000,
            summary: "benchmark click".to_string(),
        };

        let started = Instant::now();
        let permit = match authority
            .admit_action(&world_id, black_box(intent), now_ms)
            .unwrap()
        {
            WorldAdmission::Admitted { permit } => permit,
            WorldAdmission::Replay { .. } => panic!("benchmark unexpectedly replayed"),
        };
        let outcome = authority
            .complete_action(&permit, "benchmark complete", now_ms)
            .unwrap();
        black_box(outcome);
        samples_ns.push(started.elapsed().as_nanos());
    }

    let mut takeover_samples_ns = Vec::with_capacity(ITERATIONS);
    let takeover_start_ms = 5 + ITERATIONS as u64;
    for iteration in 0..ITERATIONS {
        let now_ms = takeover_start_ms + (iteration as u64 * 3);
        let started = Instant::now();
        let human_lease = authority
            .acquire_control(&world_id, human.clone(), None, now_ms)
            .unwrap();
        black_box(human_lease);
        takeover_samples_ns.push(started.elapsed().as_nanos());
        authority
            .release_control(&world_id, &human, now_ms + 1)
            .unwrap();
        authority
            .acquire_control(&world_id, agent.clone(), None, now_ms + 2)
            .unwrap();
    }

    samples_ns.sort_unstable();
    takeover_samples_ns.sort_unstable();
    let admission_p50 = percentile(&samples_ns, 50);
    let admission_p95 = percentile(&samples_ns, 95);
    let admission_p99 = percentile(&samples_ns, 99);
    let takeover_p95 = percentile(&takeover_samples_ns, 95);
    println!(
        "world hot path: iterations={ITERATIONS} p50={}ns p95={}ns p99={}ns",
        admission_p50, admission_p95, admission_p99
    );
    println!(
        "{{\"probe\":\"world_authority\",\"iterations\":{ITERATIONS},\"admission_completion_p50_ns\":{admission_p50},\"admission_completion_p95_ns\":{admission_p95},\"admission_completion_p99_ns\":{admission_p99},\"human_takeover_p95_ns\":{takeover_p95}}}"
    );
}

fn percentile(samples: &[u128], percent: usize) -> u128 {
    let index = ((samples.len() - 1) * percent) / 100;
    samples[index]
}
