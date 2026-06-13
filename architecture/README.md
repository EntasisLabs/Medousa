# Medousa Architecture Map

Documentation for Medousa's runtime, surfaces, and shipped product work.

**Download the app:** [../README.md](../README.md)  
**Developers & operators:** [../docs/README.md](../docs/README.md)  
**What's next:** [NEXT.md](NEXT.md)  
**Historical milestone plans:** [archive/README.md](archive/README.md)

---

## Start here

| Doc | Purpose |
|-----|---------|
| [system-overview.md](system-overview.md) | End-to-end system shape |
| [interaction-and-state-model.md](interaction-and-state-model.md) | Who owns what state |
| [component-daemon.md](component-daemon.md) | Engine HTTP API, persistence |
| [component-cli.md](component-cli.md) | `medousa` CLI |
| [component-tui.md](component-tui.md) | Terminal workspace |
| [component-mcp-gateway.md](component-mcp-gateway.md) | MCP Client gateway |

---

## Shipped product (Medousa app)

| Area | Doc | Notes |
|------|-----|-------|
| **Normie onboarding** | [normie-product-gap-analysis.md](normie-product-gap-analysis.md) | Phases A–D ✅ |
| **Wizard + pairing** | [normie-onboarding-and-lan-pairing-plan.md](normie-onboarding-and-lan-pairing-plan.md) | LAN QR, BYOM, engine sidecar |
| **Home shell** | [medousa-home-tauri-design.md](medousa-home-tauri-design.md) | Tauri, IPC, streams |
| **Nav tiers** | [medousa-home-product-ux-plan.md](medousa-home-product-ux-plan.md) | Life orbit / capability / utility rail |
| **Work Hub** | [medousa-home-work-hub-plan.md](medousa-home-work-hub-plan.md) | Manifestation grid, trays, pop-out |
| **Chat presentation** | [presentation-and-envelope-plan.md](presentation-and-envelope-plan.md) | Tool chips, Obsidian markdown, `parts[]` |
| **Session catalog** | [session-catalog-index-plan.md](session-catalog-index-plan.md) | Fast list, search, rename |
| **Settings charter** | [medousa-home-m11-settings-charter-plan.md](medousa-home-m11-settings-charter-plan.md) | Memory, voice, connection |

Implementation: `apps/medousa-home/`

---

## Shipped runtime (turn loop & workers)

| Doc | Topic |
|-----|-------|
| [turn-loop-single-writer-plan.md](turn-loop-single-writer-plan.md) | FSM, `begin_work`, single writer |
| [turn-control-tools-plan.md](turn-control-tools-plan.md) | `finish`, budget request, `checkpoint` |
| [turn-state-machine-plan.md](turn-state-machine-plan.md) | Completion state machine |
| [async-chat-unlock-plan.md](async-chat-unlock-plan.md) | Async chat + worker synthesis |
| [continuity-first-redesign.md](continuity-first-redesign.md) | Tool slices, runtime limits |

Code anchors: `src/medousa_tool_loop.rs`, `src/agent_runtime/`, `src/bin/medousa_daemon.rs`

---

## Active work

See **[NEXT.md](NEXT.md)** — configuration reference, provider picker, MCP/capabilities UI.

Quick list:

1. [`docs/configuration-reference.md`](../docs/configuration-reference.md) — env + paths catalog  
2. LLM providers surfaced in Home (genai catalog)  
3. MCP server add/edit without TOML  
4. Capabilities toggles without TOML  

---

## Future / deferred (not blockers)

| Doc | Topic |
|-----|-------|
| [media-and-attachments-plan.md](media-and-attachments-plan.md) | P5 media |
| [embedded-local-inference-plan.md](embedded-local-inference-plan.md) | Embedded catalog expansion |
| [normie-onboarding-and-lan-pairing-plan.md](normie-onboarding-and-lan-pairing-plan.md) | Phase E cloud auth, Phase F packaging |
| [durable-turn-worker-plan.md](durable-turn-worker-plan.md) | Durable worker hardening |
| [identity-manuscripts-and-recall-plan.md](identity-manuscripts-and-recall-plan.md) | Recall ranking |

Full historical index: [archive/README.md](archive/README.md)

---

## Diagrams

- [medousa-state.mmd](medousa-state.mmd)
- [medousa-prompt.mmd](medousa-prompt.mmd)
- [MedousaFlow.mmd](MedousaFlow.mmd)

---

## Primary code anchors

| Path | Role |
|------|------|
| `src/lib.rs` | Crate root, LLM resolution |
| `src/tools.rs` | Tool registry |
| `src/bin/medousa_daemon.rs` | Engine |
| `src/bin/medousa_tui.rs` | TUI |
| `src/bin/medousa.rs` | CLI + doctor |
| `src/session.rs` | Sessions + history |
| `src/capability_catalog.rs` | Capabilities manifest |
| `src/mcp_gateway/` | MCP gateway |
| `apps/medousa-home/` | Medousa app (Tauri + Svelte) |
