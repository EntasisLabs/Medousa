# Turn worker bus — Phase 2 (implemented)

Extends [turn-worker-phase1.md](turn-worker-phase1.md) with **automatic host routing** and **per-intent execution policy** (no new adapter code).

## Host route classification

| Route | When (heuristic) | Host tools | Worker intent |
|-------|------------------|------------|---------------|
| `handle_inline` | Default; short Q&A; ambiguous tool asks | Full registry | — |
| `delegate:memory.avec_calibrate` | AVEC/calibrate ritual, pull+preset+memory | Host bus only | `memory.avec_calibrate` |
| `delegate:memory.context` | Memory pull/recall/context without calibrate focus | Host bus only | `memory.context` |
| `delegate:research` | Web/search/fetch/latest/evidence (not memory-only) | Host bus only | `research` |

Classification: `classify_host_turn_route_heuristic` in `src/agent_runtime/turn_worker/routing.rs`.

## Host bus activation (`MEDOUSA_TURN_HOST_BUS`)

| Value | Behavior |
|-------|----------|
| *(unset)* or `auto` | Slim host **only** when route is `delegate:*` |
| `1` / `true` / `on` / `force` | Slim host on **every** tool turn; delegation intent still from heuristic |
| `0` / `off` / `false` | Never slim host (Phase 1 full registry); spawn tools still registered |

## Round budgets

| Role | Policy |
|------|--------|
| Host (bus active) | `min(configured, 4)` — `HOST_BUS_MAX_TOOL_ROUNDS` |
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

Unset env, send an AVEC+calibrate prompt — obs should show `host_route route=delegate` and host should only see spawn/status/cancel + utilities.
