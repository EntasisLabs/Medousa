# Interaction and State Model

This document describes how Medousa behaves at runtime across interaction surfaces and where state is owned.

## End-to-End Interaction Flows

## 1) TUI chat turn

1. user submits prompt from chat input
2. TUI starts prompt run through tool loop execution path
3. streaming chunks update visible conversation incrementally
4. tool events are emitted into observability stream
5. final response is committed to conversation history

State touched:

- in-memory: conversation buffers, processing flags, overlay state
- persisted user state: session history append

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

## State Domains

## A) UI state domain (TUI)

Owned by TuiState:

- mode and panel projections
- drafts and editor buffers
- scroll/selection/transient display state

Properties:

- volatile
- deterministic updates from input + runtime events

## B) user persistence domain

Owned by session.rs:

- session history files
- defaults (settings/routing/depth)
- last-session pointer
- secure key material (keyring/file fallback)

Properties:

- local host persistence
- backend-independent

## C) runtime execution domain

Owned by Stasis backend:

- jobs and lifecycle transitions
- attempts and diagnostics
- recurring definitions
- outbox event progression

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

- TUI <-> runtime: in-process through TuiRuntime and event channel
- CLI <-> runtime: direct in-process orchestration calls
- CLI <-> daemon: HTTP API contract only
- daemon <-> runtime: direct runtime orchestration and scheduler tick

This separation keeps transport semantics explicit while preserving shared execution primitives.
