use std::collections::BTreeSet;
use std::hint::black_box;
use std::time::Instant;

use medousa_world::{
    WorldActionIntent, WorldAdmission, WorldAuthority, WorldAuthorityId, WorldCapability,
    WorldEffectClass, WorldGrantId, WorldGrantRequest, WorldId, WorldIntentId, WorldOwnership,
    WorldPrincipal, WorldPrincipalId, WorldResourceId, WorldResourceScope, WorldSessionSpec,
    WorldSurfaceKind, WorldTraceId,
};

const ITERATIONS: usize = 20_000;

fn main() {
    let world_id = WorldId::new("world:benchmark");
    let resource_id = WorldResourceId::new("browser-tab:benchmark");
    let system = WorldPrincipal::system(WorldPrincipalId::new("runtime:benchmark"));
    let agent = WorldPrincipal::agent(WorldPrincipalId::new("agent:benchmark"));
    let mut authority = WorldAuthority::new(128);
    authority
        .create_world(
            WorldSessionSpec {
                world_id: world_id.clone(),
                authority_id: WorldAuthorityId::new("workshop:benchmark"),
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
                issued_by: system,
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
    let lease = authority
        .acquire_control(&world_id, agent.clone(), None, 3)
        .unwrap();

    let mut samples_ns = Vec::with_capacity(ITERATIONS);
    for iteration in 0..ITERATIONS {
        let now_ms = 4 + iteration as u64;
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

    samples_ns.sort_unstable();
    let percentile = |percent: usize| {
        let index = ((samples_ns.len() - 1) * percent) / 100;
        samples_ns[index]
    };
    println!(
        "world hot path: iterations={ITERATIONS} p50={}ns p95={}ns p99={}ns",
        percentile(50),
        percentile(95),
        percentile(99)
    );
}
