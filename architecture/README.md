# Medousa architecture

Living documentation for system shape, component boundaries, and active roadmap.

**Product README:** [../README.md](../README.md)  
**Developer docs:** [../docs/README.md](../docs/README.md)  
**Active roadmap:** [ROADMAP.md](ROADMAP.md)  
**Architecture decisions (ADRs):** [../docs/architecture/decisions/README.md](../docs/architecture/decisions/README.md)  
**Historical milestone plans:** [archive/README.md](archive/README.md)

---

## Start here

| Doc | Purpose |
|-----|---------|
| [system-overview.md](system-overview.md) | End-to-end system shape |
| [interaction-and-state-model.md](interaction-and-state-model.md) | Who owns what state |
| [enterprise-architecture-and-flow-guide.md](enterprise-architecture-and-flow-guide.md) | Topology, flows, accountability framing |
| [component-daemon.md](component-daemon.md) | Engine HTTP API, persistence |
| [component-cli.md](component-cli.md) | `medousa` CLI |
| [component-tui.md](component-tui.md) | Terminal workspace |
| [component-mcp-gateway.md](component-mcp-gateway.md) | MCP Client gateway |

---

## Diagrams

- [medousa-state.mmd](medousa-state.mmd)
- [medousa-prompt.mmd](medousa-prompt.mmd)
- [MedousaFlow.mmd](MedousaFlow.mmd)

---

## Active epics & roadmaps

| Doc | Topic |
|-----|--------|
| [ROADMAP.md](ROADMAP.md) | Current priorities (attachments, Iroh, distribution, …) |
| [iroh-p2p-pairing-plan.md](iroh-p2p-pairing-plan.md) | Encrypted phone ↔ desktop transport |
| [media-and-attachments-plan.md](media-and-attachments-plan.md) | Local chat attachments (P5) |
| [embedded-local-inference-plan.md](embedded-local-inference-plan.md) | Embedded Gemma engine |
| [desktop-distribution-plan.md](desktop-distribution-plan.md) | Signed app bundles |
| [durable-turn-worker-plan.md](durable-turn-worker-plan.md) | Durable worker hardening |

Future / deferred (not blockers):

| Doc | Topic |
|-----|--------|
| [identity-manuscripts-and-recall-plan.md](identity-manuscripts-and-recall-plan.md) | Recall ranking + manuscripts |
| [cognitive-identity-memory-plan.md](cognitive-identity-memory-plan.md) | Identity memory phases |
| [worker-continuity-plan.md](worker-continuity-plan.md) | Worker continuity |
| [context-lanes-and-scratchpad-plan.md](context-lanes-and-scratchpad-plan.md) | Context lanes |
| [centralized-ingester-roadmap.md](centralized-ingester-roadmap.md) | Ingester |
| [centralized-agent-runtime-roadmap.md](centralized-agent-runtime-roadmap.md) | Agent runtime |
| [outbox-channel-delivery-roadmap.md](outbox-channel-delivery-roadmap.md) | Channel delivery |
| [recurring-delivery-roadmap.md](recurring-delivery-roadmap.md) | Recurring delivery |
| [dlq-replay-turn-continuation-plan.md](dlq-replay-turn-continuation-plan.md) | DLQ replay |
| [tui-performance-target-plan.md](tui-performance-target-plan.md) | TUI performance |

---

## Code anchors

| Path | Role |
|------|------|
| `src/lib.rs` | Crate root, LLM resolution |
| `src/tools.rs` | Tool registry |
| `src/bin/medousa_daemon.rs` | Engine |
| `src/bin/medousa_tui.rs` | TUI |
| `src/bin/medousa.rs` | CLI + doctor |
| `src/user_profiles.rs` | Workshop profile registry |
| `src/locus_memory.rs` | Locus session scoping |
| `apps/medousa-home/` | Medousa app (Tauri + Svelte) |
