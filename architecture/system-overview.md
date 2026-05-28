# Medousa System Overview

## Purpose

Medousa is a runtime product for evidence-aware cognitive workflows.

It is built to support:

- interactive reasoning and tool use
- durable execution and replayability
- explicit operator control over behavior

The implementation composes on top of Stasis runtime primitives rather than introducing a separate orchestration core.

## Product Surfaces

Medousa exposes three executables:

- CLI (`medousa_cli`): one-shot command execution and daemon client operations
- TUI (`medousa_tui`): interactive workspace for chat, scripting, observability, settings, routing, and verification flows
- Daemon (`medousa_daemon`): long-running API and scheduler process for continuous operation

All surfaces share Medousa library/runtime composition code and Stasis domain/application infrastructure.

## Core Runtime Model

Shared runtime helpers in `medousa/src/lib.rs`:

- `build_runtime(...)`
- `parse_backend(...)`
- `process_once(...)`
- `publish_pending(...)`

Runtime backends:

- `in-memory`
- `surreal-mem`

LLM target resolution order:

1. explicit flags/arguments
2. configured defaults/environment
3. fallback defaults (`openai` + `gpt-4o-mini`)

## Capability Layering

Medousa combines these capability tracks:

1. Interaction and tooling
  - chat loop
  - tool invocations
  - script execution and promotion paths
2. Evidence and confidence
  - artifact persistence and chunk references
  - extraction and verification paths
  - context-pack-driven prompt augmentation
3. Operator controls
  - settings draft/apply model
  - role-based stage routing controls
  - response depth and runtime behavior tuning
4. Runtime operations
  - scheduler ticks and recurring materialization
  - job-attempt diagnostics
  - outbox publishing and service endpoints

## Responsibility Split

## CLI

- builds runtime per command invocation
- submits and processes jobs synchronously for local commands
- calls daemon endpoints for remote/control-plane operations

## TUI

- owns interactive state machine (`TuiState`)
- assembles runtime and tool-loop pipeline (`build_tui_runtime`)
- consumes asynchronous runtime/tool events (`TuiEvent`) into deterministic UI updates
- persists session history, user defaults, and secure key material

## Daemon

- exposes HTTP APIs for enqueue, prompt, recurring, health, and stats
- runs scheduler loop that:
  - materializes due recurring definitions
  - processes queued jobs
  - publishes pending outbox events

## Shared Execution Backbone

Execution is job-oriented across all surfaces:

1. submit `NewJob`
2. execute through runtime handler pipeline
3. inspect `JobAttempt` diagnostics and outcomes

This keeps behavior consistent whether work is started from TUI, CLI, or daemon API.

## Typical End-to-End Paths

## Interactive path (TUI)

1. user submits prompt
2. runtime tool loop streams response + tool events
3. artifacts/verification/context-pack flows contribute evidence signals
4. final response and diagnostics are persisted for later inspection

## Service path (daemon + CLI/API)

1. request is accepted and enqueued
2. scheduler loop materializes due recurring work and processes jobs
3. results and diagnostics are available through runtime stores and API surfaces

## Design Intent

Medousa prioritizes:

- understandable behavior under load and failure
- clear operator controls over model/routing/verification behavior
- traceable outputs where evidence and confidence can be inspected without forcing full-detail views by default
