# Component: medousa-daemon

## Role in the Product

medousa-daemon is the service-mode control plane for Medousa.

It is used when you need:

- long-running scheduling and execution
- HTTP-accessible enqueue and recurring APIs
- separation between clients and runtime workers

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

- GET /health
- GET /v1/stats
- POST /v1/jobs/ask
- POST /v1/jobs/prompt
- POST /v1/recurring/prompt

Optional local dashboard mount (in-memory backend):

- /dashboard

## Service State Ownership

Daemon AppState stores runtime composition and service metadata:

- runtime handle
- backend label
- worker identifier
- last_tick_at marker

## Request Handling Pattern

For enqueue-style writes:

1. validate request contract
2. construct workflow payload and NewJob
3. enqueue into runtime
4. return accepted response with identifiers

Recurring registration also computes and stores next_run_at.

## Durability Model

Daemon process does not maintain separate custom persistence files.

Durability is delegated to runtime backend stores:

- job and attempt records
- recurring definitions
- outbox event state

## Operational Expectations

- --once performs a single tick and exits
- --interval-ms controls steady-state scheduler cadence
- graceful shutdown is signal-driven
- backend selection defines execution durability profile
