# Daemon crate layout (`medousa::daemon`)

The `medousa_daemon` binary is a thin launcher (~500 lines). HTTP handlers live in the library under `src/daemon/`.

Turn spine and lifecycle: [`medousa-engine` crate](../../crates/medousa-engine/) — see [component-engine.md](../../architecture/component-engine.md).

## Daemon HTTP modules

| Module | Responsibility |
|--------|----------------|
| `state` | `AppState`, `AgentTurnJobRecord` |
| `router` | `build_daemon_router()` — core + feature route merge |
| `core` | Health, stats, runtime defaults, artifact/config commands |
| `jobs` | Enqueue ask/report/prompt, recurring, job result/report |
| `interactive` | Turn tickets, interactive turns, session active-turn, SSE `?since=` |
| `ingest` | Channel ingest, delivery webhooks, agent-turn streaming for channels |
| `identity` | Identity memory + user profile HTTP handlers |
| `continuations` | Turn continuation status, lineage, replay-and-resume |
| `heartbeat` | Scheduler tick, heartbeat notify, retention side effects |
| `http` | Shared `internal_error` helper |

## Library modules (outside `daemon/`)

| Module | Responsibility |
|--------|----------------|
| `agent_runtime` | Turn orchestration, tool loop, host/worker bus, specialists |
| `comms` | Transport trait, pool, LAN/Iroh adapters, route selector, gateway helpers |
| `observability` | Tracing init, log rotation, dead-letter cap, correlation spans |
| `engine_recovery` | Startup `recover_uncommitted` wiring for `TurnEventLog` |
| `sse_turn_projection` | Project spine events to `InteractiveTurnStreamEvent` SSE payloads |

## Process split (inference)

| Process | Port | Role |
|---------|------|------|
| `medousa_daemon` | 7419 | Catalog, jobs, ingest, identity — **no embedded mistralrs** |
| `medousa_local` | 7421 | Offline Gemma inference (mistralrs) |

Spawn helpers: [`medousa-host`](../../crates/medousa-host/). Desktop spawns via `workshop_runtime::ensure_local_brain` (Tauri).

## Clients

- [`medousa-sdk`](../../crates/medousa-sdk/) + [`medousa-types`](../../crates/medousa-types/) for HTTP DTOs
- Tauri: `daemon/sdk.rs` → [`medousa-sdk-iroh`](../../crates/medousa-sdk-iroh/) `WorkshopTransport` (LAN/Iroh failover)
