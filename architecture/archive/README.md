# Architecture archive — historical milestone plans

These documents record **how we built** Medousa Home and the runtime. They remain accurate as design history; **status headers** mark what shipped vs what was superseded.

For **what to work on now**, see [../NEXT.md](../NEXT.md).

For **current system shape**, start at [../README.md](../README.md) → [system-overview.md](../system-overview.md).

---

## Medousa Home milestones (shipped)

| Doc | Milestone | Shipped |
|-----|-----------|---------|
| [medousa-home-plan.md](../medousa-home-plan.md) | Daemon-first workspace + vault design | Home app live |
| [medousa-home-tauri-design.md](../medousa-home-tauri-design.md) | Tauri shell, IPC, streams | ✅ |
| [medousa-home-polish-plan.md](../medousa-home-polish-plan.md) | M4 Obsidian theme, operator strip | ✅ 2026-05-30 |
| [medousa-home-main-workspace-plan.md](../medousa-home-main-workspace-plan.md) | Workshop shell, activity rail | ✅ |
| [medousa-home-m5-plan.md](../medousa-home-m5-plan.md) | M5 surfaces | ✅ |
| [medousa-home-product-ux-plan.md](../medousa-home-product-ux-plan.md) | M7 messaging, cron, skills, tools | ✅ |
| [medousa-home-m7-vault-garage-plan.md](../medousa-home-m7-vault-garage-plan.md) | Library / garage | ✅ |
| [medousa-home-m8-real-garage-plan.md](../medousa-home-m8-real-garage-plan.md) | Garage import funnel | ✅ |
| [medousa-home-m10-settings-runtime-plan.md](../medousa-home-m10-settings-runtime-plan.md) | Settings ↔ runtime split | ✅ |
| [medousa-home-m11-settings-charter-plan.md](../medousa-home-m11-settings-charter-plan.md) | Memory & voice charter | ✅ |
| [medousa-home-messaging-polish-plan.md](../medousa-home-messaging-polish-plan.md) | Messaging UX | ✅ |
| [medousa-home-mobile-plan.md](../medousa-home-mobile-plan.md) | Mobile shell | ✅ |
| [medousa-home-mobile-m9-plan.md](../medousa-home-mobile-m9-plan.md) | Mobile M9 | ✅ |
| [medousa-home-context-view-plan.md](../medousa-home-context-view-plan.md) | Context view M12 | ✅ |
| [medousa-home-context-map-plan.md](../medousa-home-context-map-plan.md) | Context map M13a | ✅ |
| [medousa-home-work-hub-plan.md](../medousa-home-work-hub-plan.md) | Work Hub grid, trays, manifestations | ✅ W1–W3 + nav tiers |
| [presentation-and-envelope-plan.md](../presentation-and-envelope-plan.md) | Tool chips, markdown, `parts[]` | ✅ P0–P4 |
| [normie-onboarding-and-lan-pairing-plan.md](../normie-onboarding-and-lan-pairing-plan.md) | Wizard + LAN pairing | ✅ Phases A–D |
| [normie-product-gap-analysis.md](../normie-product-gap-analysis.md) | Steve Jobs pass A–D | ✅ 2026-06-07 |
| [session-catalog-index-plan.md](../session-catalog-index-plan.md) | Fast session list + search | ✅ |

---

## Runtime & turn loop (shipped)

| Doc | Topic |
|-----|-------|
| [turn-ledger-phase0.md](../turn-ledger-phase0.md) | Loop discipline |
| [turn-worker-phase1.md](../turn-worker-phase1.md) | In-process worker bus |
| [turn-worker-phase2.md](../turn-worker-phase2.md) | Host routing |
| [turn-worker-bus-plan.md](../turn-worker-bus-plan.md) | Host/worker delegation |
| [turn-loop-single-writer-plan.md](../turn-loop-single-writer-plan.md) | Single-writer FSM |
| [turn-state-machine-plan.md](../turn-state-machine-plan.md) | Turn completion FSM |
| [turn-control-tools-plan.md](../turn-control-tools-plan.md) | finish / budget / checkpoint |
| [turn-completion-gatekeeper.md](../turn-completion-gatekeeper.md) | Gatekeeper design |
| [tool-loop-interim-text-fix.md](../tool-loop-interim-text-fix.md) | Interim text fix |
| [async-chat-unlock-plan.md](../async-chat-unlock-plan.md) | Async chat tiers |
| [async-chat-tier2-plan.md](../async-chat-tier2-plan.md) | Tier 2 |
| [async-chat-tier3-plan.md](../async-chat-tier3-plan.md) | Tier 3 worker synthesis |
| [continuity-first-redesign.md](../continuity-first-redesign.md) | Continuity pipeline 8A–8E |
| [runtime-collaborator-voice.md](../runtime-collaborator-voice.md) | Collaborator voice |

---

## Roadmaps & future epics (not active blockers)

| Doc | Notes |
|-----|-------|
| [media-and-attachments-plan.md](../media-and-attachments-plan.md) | P5 draft |
| [embedded-local-inference-plan.md](../embedded-local-inference-plan.md) | Embedded Gemma — partial ship |
| [durable-turn-worker-plan.md](../durable-turn-worker-plan.md) | Stasis durable workers |
| [worker-continuity-plan.md](../worker-continuity-plan.md) | Worker continuity |
| [cognitive-identity-memory-plan.md](../cognitive-identity-memory-plan.md) | Identity memory |
| [identity-manuscripts-and-recall-plan.md](../identity-manuscripts-and-recall-plan.md) | Recall + manuscripts |
| [context-lanes-and-scratchpad-plan.md](../context-lanes-and-scratchpad-plan.md) | Context lanes |
| [centralized-ingester-roadmap.md](../centralized-ingester-roadmap.md) | Ingester |
| [centralized-agent-runtime-roadmap.md](../centralized-agent-runtime-roadmap.md) | Agent runtime |
| [outbox-channel-delivery-roadmap.md](../outbox-channel-delivery-roadmap.md) | Channel delivery |
| [recurring-delivery-roadmap.md](../recurring-delivery-roadmap.md) | Recurring |
| [dlq-replay-turn-continuation-plan.md](../dlq-replay-turn-continuation-plan.md) | DLQ replay |
| [tui-performance-target-plan.md](../tui-performance-target-plan.md) | TUI perf |

---

## Component reference (living)

These stay at the top level — they describe **current** boundaries, not a sprint:

- [system-overview.md](../system-overview.md)
- [component-cli.md](../component-cli.md)
- [component-tui.md](../component-tui.md)
- [component-daemon.md](../component-daemon.md)
- [component-mcp-gateway.md](../component-mcp-gateway.md)
- [interaction-and-state-model.md](../interaction-and-state-model.md)
- [enterprise-architecture-and-flow-guide.md](../enterprise-architecture-and-flow-guide.md)
