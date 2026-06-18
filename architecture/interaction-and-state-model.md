# Interaction and State Model

This document describes how Medousa behaves at runtime across interaction surfaces and where state is owned.

**Turn engine (host/worker/FSM/lanes):** [turn-runtime-and-lanes.md](turn-runtime-and-lanes.md)

---

## End-to-End Interaction Flows

## 1) TUI chat turn

1. user submits prompt from chat input
2. TUI calls daemon `POST /v1/interactive/turn` (primary) or local `execute_local_turn` fallback
3. shared agent runtime runs tool loop + FSM + optional worker delegation
4. streaming chunks update visible conversation incrementally
5. tool events are emitted into observability stream
6. final response is committed to conversation history

State touched:

- in-memory: conversation buffers, processing flags, overlay state
- persisted user state: session history append (+ optional scratch metadata)

## 2) TUI script execution flow

1. script source is resolved from editor/file command
2. allowlist precheck validates referenced operations
3. grapheme workflow job is enqueued and processed
4. attempt diagnostics are collected
5. UI updates job list, observability, and console output

State touched:

- in-memory: job list, diagnostics view state, console pane
- runtime durable state: job and attempt records

## 3) CLI local execution flow

1. CLI builds runtime composition
2. prompt/ask payload is converted to job contract
3. single processing cycle executes
4. result + diagnostics are printed

State touched:

- process-local state during command lifecycle
- runtime backend state for durable job/attempt data

## 4) Daemon enqueue and scheduler flow

1. API request enqueues job or recurring definition
2. scheduler tick materializes due recurring jobs
3. scheduler processes queued work
4. outbox publisher advances pending events

State touched:

- runtime backend durable stores
- daemon in-memory service metadata (for example last_tick_at)

## 5) Daemon interactive turn (Home, TUI, ingest)

1. adapter POSTs turn request (optional `manuscript_id`, routing, depth)
2. `run_agent_turn` prepares prompt probes and runs host tool loop
3. host may delegate via `cognition_spawn_turn_worker` → worker lane → synthesis
4. SSE stream events + turn ledger JSONL record lifecycle
5. session history append on terminal outcome

State touched:

- agent runtime in-memory: turn workers store, stream sinks, ask job results
- persisted: session files, turn ledger, `workspace/turn_workers.json`, Locus/identity stores

See [turn-runtime-and-lanes.md](turn-runtime-and-lanes.md) for FSM, host bus, and specialist/manuscript wiring.

---

## Agent turn bus (shipped)

Host/worker delegation, synthesis, and pending-work lifecycle are **daemon agent runtime** concerns, not TUI-specific. Any comms medium (Home SSE, TUI, Telegram ingest, `interactive/turn`, `jobs/ask`) uses the same `run_agent_turn` path and observes the same bus semantics.

Historical design notes: [archive/turn-worker-bus-plan.md](archive/turn-worker-bus-plan.md).

Async chat unlock (sync UI → background workers): [archive/async-chat-unlock-plan.md](archive/async-chat-unlock-plan.md).

---

## State Domains

## A) UI state domain (TUI / Home)

Owned by client stores (`TuiState`, Home Svelte stores):

- mode and panel projections
- drafts and editor buffers
- scroll/selection/transient display state

Properties:

- volatile
- deterministic updates from input + runtime events

## B) user persistence domain

Owned by session.rs + profile registry:

- session history files
- defaults (settings/routing/depth)
- last-session pointer
- secure key material (keyring/file fallback)
- user profiles (`user_profiles.json`)

Properties:

- local host persistence
- backend-independent

## C) runtime execution domain

Owned by Stasis backend + agent runtime stores:

- jobs and lifecycle transitions
- attempts and diagnostics
- recurring definitions
- outbox event progression
- turn worker records (`TurnWorkRecord`)

Properties:

- source of truth for execution outcomes
- shared across surfaces when backend context is shared

## Configuration Resolution Pattern

Observed precedence:

1. explicit runtime arguments
2. saved defaults
3. environment values
4. hardcoded fallback defaults

For TUI runtime/env overrides:

- env overrides are applied before runtime rebuild so the new composition reads updated process environment.

## Coupling Boundaries

- TUI / Home ↔ daemon: HTTP + SSE (`/v1/interactive/turn`); local TUI fallback in-process only
- CLI ↔ runtime: direct in-process orchestration calls
- CLI ↔ daemon: HTTP API contract only
- daemon ↔ runtime: `MedousaAgentRuntime` + scheduler tick

Adapters stay thin; orchestration lives in `src/agent_runtime/`. See [turn-runtime-and-lanes.md](turn-runtime-and-lanes.md).
