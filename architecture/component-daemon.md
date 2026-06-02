# Component: medousa-daemon

## Role in the Product

medousa-daemon is the service-mode control plane for Medousa.

It is used when you need:

- long-running scheduling and execution
- HTTP-accessible enqueue and recurring APIs
- separation between clients and runtime workers
- **centralized agent runtime** for interactive turns (TUI, ingester, API)

## Entry Point

- Binary: medousa/src/bin/medousa_daemon.rs

## Process Model

The daemon runs two concurrent paths:

1. HTTP API server
2. scheduler/runtime tick loop

Each scheduler tick performs:

1. recurring materialization (due definitions -> jobs)
2. queued job processing
3. outbox publish progression

## API Surface

Defined through shared daemon contracts:

- GET /health — includes agent runtime version, tool count, last turn latency
- GET /v1/stats
- POST /v1/ingest — channel adapters (Telegram, Discord, CLI)
- POST /v1/interactive/turn — TUI daemon-primary chat turns (SSE)
- POST /v1/jobs/ask — direct API ask (agent runtime, poll `/v1/jobs/{id}/result`)
- POST /v1/jobs/report — structured report ask (agent runtime)
- POST /v1/jobs/prompt — bare prompt Stasis job (legacy scheduler path)
- POST /v1/recurring/prompt
- GET /v1/delivery/status — outbox + delivery health

Optional local dashboard mount (in-memory backend):

- /dashboard

## Agent Runtime

At startup the daemon builds `MedousaAgentRuntime` via `build_daemon_agent_runtime()`.

Interactive paths that use the shared turn engine:

| Route | Client |
|-------|--------|
| `/v1/interactive/turn` | TUI (daemon-primary) |
| `/v1/ingest` (EnqueueAsk) | Telegram, Discord, CLI |
| `/v1/jobs/ask` | TUI `/daemon ask`, CLI, direct API |
| `/v1/jobs/report` | CLI report flows |

Legacy Stasis scheduler jobs remain for `/v1/jobs/prompt` and recurring materialization.

Planned **host/worker bus** (delegation, pending work, synthesis turns) lives in this runtime layer so every route above behaves the same; TUI/Telegram/API only adapt presentation. See [turn-worker-bus-plan.md](turn-worker-bus-plan.md).

## Service State Ownership

Daemon AppState stores runtime composition and service metadata:

- runtime handle + agent runtime
- backend label, worker identifier
- interactive turn streams, ingest session mappings
- channel delivery registry + agent turn job results
- last_tick_at marker

## Request Handling Pattern

For agent-runtime ask/report:

1. validate request contract
2. register pending job in `agent_turn_jobs`
3. spawn `run_agent_turn` with API stream sink
4. return accepted response with `job_id`
5. clients poll `/v1/jobs/{job_id}/result`

For Stasis enqueue-style writes (prompt/recurring):

1. validate request contract
2. construct workflow payload and NewJob
3. enqueue into runtime
4. return accepted response with identifiers

## Durability Model

Daemon process does not maintain separate custom persistence files.

Durability is delegated to runtime backend stores:

- job and attempt records
- recurring definitions
- outbox event state
- session history (via session store)

Agent turn job results are in-memory until polled (same process lifetime as daemon).

## Operational Expectations

- --once performs a single tick and exits
- --interval-ms controls steady-state scheduler cadence
- graceful shutdown is signal-driven
- backend selection defines execution durability profile
