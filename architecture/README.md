# Medousa Architecture Map

This directory documents Medousa as a runtime product.

It focuses on:

- executable surfaces and their boundaries
- state ownership (ephemeral UI, persisted user state, durable runtime state)
- runtime composition and orchestration behavior
- operational interaction flows across local and daemon modes

Start here for product-level usage:

- [../README.md](../README.md)

## Documents

1. [system-overview.md](system-overview.md)
2. [enterprise-architecture-and-flow-guide.md](enterprise-architecture-and-flow-guide.md)
3. [component-cli.md](component-cli.md)
4. [component-tui.md](component-tui.md)
5. [component-daemon.md](component-daemon.md)
6. [interaction-and-state-model.md](interaction-and-state-model.md)
7. [tui-performance-target-plan.md](tui-performance-target-plan.md)
8. [centralized-ingester-roadmap.md](centralized-ingester-roadmap.md)
9. [outbox-channel-delivery-roadmap.md](outbox-channel-delivery-roadmap.md)
10. [centralized-agent-runtime-roadmap.md](centralized-agent-runtime-roadmap.md)
11. [component-mcp-gateway.md](component-mcp-gateway.md)
12. [dlq-replay-turn-continuation-plan.md](dlq-replay-turn-continuation-plan.md)
13. [recurring-delivery-roadmap.md](recurring-delivery-roadmap.md)
14. [tool-loop-interim-text-fix.md](tool-loop-interim-text-fix.md)
15. [agent-experience-gap-analysis.md](agent-experience-gap-analysis.md)
16. [turn-completion-gatekeeper.md](turn-completion-gatekeeper.md)
17. [turn-worker-bus-plan.md](turn-worker-bus-plan.md) — host/worker delegation; **daemon bus**, comms adapters are not the bus
18. [turn-ledger-phase0.md](turn-ledger-phase0.md) — Phase 0 loop discipline (ledger, stuck detector, control messages)
19. [turn-worker-phase1.md](turn-worker-phase1.md) — Phase 1 in-process worker bus (spawn / synthesis)
20. [turn-worker-phase2.md](turn-worker-phase2.md) — Phase 2 host routing + auto slim host
21. [context-lanes-and-scratchpad-plan.md](context-lanes-and-scratchpad-plan.md) — **Planned:** tiered context pools (user / host tool / worker), scratchpad, lane classification; Locus prompt git last
22. [worker-continuity-plan.md](worker-continuity-plan.md) — worker = same Medousa, workshop hat; continuity bundle + curated STTP (Phase A–E)
23. [cognitive-identity-memory-plan.md](cognitive-identity-memory-plan.md) — identity store as relational memory (preferences, contacts, cognitive mode, remember tool); Stasis 0.4.0; Phases 0–5
24. [identity-manuscripts-and-recall-plan.md](identity-manuscripts-and-recall-plan.md) — **Planned:** relevance-ranked digest, `cognition_identity_recall`, YAML identity manuscripts (workers + cron + delivery); revises Phase 4 sequencing
25. [medousa-home-plan.md](medousa-home-plan.md) — **Design:** daemon-first Medousa Home (workspace feed + Kanban cards from Stasis, vault notes, deferred Tauri); API contracts W1–V3 before any UI
26. [presentation-and-envelope-plan.md](presentation-and-envelope-plan.md) — **Plan:** channel-agnostic turn envelope, surface formatters, Home tool chips + Obsidian markdown, attachment hooks
27. [media-and-attachments-plan.md](media-and-attachments-plan.md) — **Draft (P5):** user attachments, vision, generated images, voice; blob store + upload + `parts[]` phasing (P5a–P5e)
28. [turn-loop-single-writer-plan.md](turn-loop-single-writer-plan.md) — **Done:** tool-call loop gate, `cognition_turn_begin_work`, single-writer commit, FSM/interim cleanup (Phase 5)
29. [continuity-first-redesign.md](continuity-first-redesign.md) — **Approved plan:** Model 3 continuity pipeline, slice-indexed tool history, runtime-owned limits, phased 8A–8E
30. [medousa-home-m7-vault-garage-plan.md](medousa-home-m7-vault-garage-plan.md) — **Epic M7:** Library as life garage — spaces, typed notes, premium editor, Vault↔Work↔Chat bridges (8 sprints)
31. [medousa-home-m8-real-garage-plan.md](medousa-home-m8-real-garage-plan.md) — **Epic M8:** The real garage — formatting affordances, attachments, external desk, spreadsheet preview (8 sprints)

## Primary Code Anchors

- `medousa/src/lib.rs`
- `medousa/src/tools.rs`
- `medousa/src/bin/medousa_cli.rs`
- `medousa/src/bin/medousa_tui.rs`
- `medousa/src/bin/medousa_daemon.rs`
- `medousa/src/session.rs`
- `medousa/src/events.rs`
- `medousa/src/daemon_api.rs`
