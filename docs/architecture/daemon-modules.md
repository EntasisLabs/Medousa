# Daemon crate layout (`medousa::daemon`)

The `medousa_daemon` binary is a thin launcher (~500 lines). HTTP handlers live in the library under `src/daemon/`.

## Modules

| Module | Responsibility |
|--------|----------------|
| `state` | `AppState`, `AgentTurnJobRecord` |
| `router` | `build_daemon_router()` — core + feature route merge |
| `core` | Health, stats, runtime defaults, artifact/config commands |
| `jobs` | Enqueue ask/report/prompt, recurring, job result/report |
| `interactive` | Turn tickets, interactive turns, session active-turn |
| `ingest` | Channel ingest, delivery webhooks, agent-turn streaming for channels |
| `identity` | Identity memory + user profile HTTP handlers |
| `continuations` | Turn continuation status, lineage, replay-and-resume |
| `heartbeat` | Scheduler tick, heartbeat notify, retention side effects |
| `http` | Shared `internal_error` helper |

## Process split (inference)

| Process | Port | Role |
|---------|------|------|
| `medousa_daemon` | 7419 | Catalog, jobs, ingest, identity — **no embedded mistralrs** |
| `medousa_local` | 7421 | Offline Gemma inference (mistralrs) |

Spawn helpers: [`medousa-host`](../../crates/medousa-host/). Desktop spawns via `workshop_runtime::ensure_local_brain` (Tauri).

## Clients

- [`medousa-sdk`](../../crates/medousa-sdk/) + [`medousa-types`](../../crates/medousa-types/) for HTTP DTOs
- Tauri: `daemon/sdk.rs` (`Transport` + workshop LAN/Iroh failover)
