# Daemon crate layout (`medousa::daemon`)

The `medousa_daemon` binary owns process composition, startup recovery and shutdown. HTTP handlers live in the library under `src/daemon/`.

Turn spine and lifecycle: [`medousa-engine` crate](../../crates/medousa-engine/) — see [component-engine.md](../../architecture/component-engine.md).

## Shared runtime and terminal boundary

`agent_runtime::runtime::MedousaAgentRuntime` is the concrete shared service
bundle. `runtime::agent_services` constructs it and `runtime::settings` owns its
settings. `tools::TuiRuntime`, `events::TuiEvent` and the terminal settings module
are compatibility names, not alternate runtime/store authorities.

The default `full-daemon` profile excludes Ratatui and direct TUI dependencies.
Use `--features tui` for `medousa_tui` and the terminal `medousa` launcher. Build
the daemon separately from these targets: Cargo unifies a package's features
within an invocation, even when more than one binary is selected.

The platform owns its best-effort headless event drain; process recovery,
scheduler/task shutdown, durable turn ordering and store drains remain owned by
the existing launcher/services. Grapheme data still transitively needs Polars
formatting and Crossterm 0.29; this is not a claim that no terminal-related crate
remains. See the [slice report and capability accounting](../reviews/daemon-ownership-and-dependencies-2026-11-12.md).

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
