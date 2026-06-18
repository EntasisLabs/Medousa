# Turn worker bus — Phase 2 (implemented)

Extends [turn-worker-phase1.md](turn-worker-phase1.md) with **automatic host routing** and **per-intent execution policy** (no new adapter code).

## Host route classification

| Route | When (heuristic) | Host tools | Worker intent |
|-------|------------------|------------|---------------|
| `handle_inline` | Default; short Q&A; ambiguous tool asks | Orchestrator allowlist | — (spawn worker if execution needed) |
| `delegate:memory.avec_calibrate` | AVEC/calibrate ritual, pull+preset+memory | Orchestrator allowlist | `memory.avec_calibrate` |
| `delegate:memory.context` | Memory pull/recall/context without calibrate focus | Orchestrator allowlist | `memory.context` |
| `delegate:research` | Web/search/fetch/latest/evidence (not memory-only) | Orchestrator allowlist | `research` |

Classification: `classify_host_turn_route_heuristic` in `src/agent_runtime/turn_worker/routing.rs`.

## Host bus activation (`MEDOUSA_TURN_HOST_BUS`)

| Value | Behavior |
|-------|----------|
| *(unset)* or `auto` | Orchestrator allowlist on **every** host turn (default) |
| `1` / `true` / `on` / `force` | Same as `auto` |
| `0` / `off` / `false` | Full registry (legacy); spawn tools still registered |

## Round budgets

| Role | Policy |
|------|--------|
| Host (bus active) | `min(settings max_tool_rounds, cap)` — cap default **8**, override `MEDOUSA_HOST_BUS_MAX_TOOL_ROUNDS` (1–50) |
| Worker `memory.avec_calibrate` | 12 |
| Worker `memory.context` | 10 |
| Worker `research` | 14 |
| Worker `general` | 10 |

## Prompt injection

When host bus is active, system prompt includes `[MEDOUSA_HOST_BUS]` plus `[MEDOUSA_HOST_ROUTE]` with recommended `cognition_spawn_turn_worker` intent.

Obs: `◈ host_route route=delegate intent=memory.avec_calibrate host_bus=true reason=...`

## Verify

```bash
cargo test host_turn
cargo check
```

Unset env — host should see memory, capability catalog (list/search/resolve), runtime workflow/job tools, and spawn/status/cancel (not Grapheme/MCP invoke). AVEC+calibrate prompts should show `host_route route=delegate` and recommend `memory.avec_calibrate` worker intent.
