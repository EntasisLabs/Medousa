# Medousa TUI Performance Target Plan

This document captures the agreed phased plan for improving TUI responsiveness without changing core runtime behavior.

## Goals

- keep input handling and render loop responsive under load
- prevent command handlers from blocking the UI thread
- preserve current runtime semantics and observability guarantees
- stage high-risk optimizations behind smaller, validated milestones

## Baseline Symptoms

- key input latency from polling + sleep cadence
- command paths awaiting runtime/network work inline
- redraw path rebuilding expensive text/markdown buffers each frame
- dropped mouse events reducing interaction quality

## Phase 1 (Now): Non-Blocking Worker and Channel Refactor

### Scope

- move expensive `/run` execution off the UI path into background tasks
- move daemon network commands (`/daemon health`, `/daemon ask`, `/watch add`) off the UI path
- route worker completion/errors back to UI over `TuiEvent` channel
- keep UI state updates centralized in `handle_tui_event`

### Deliverables

- new `TuiEvent::UiNotice(String)` event for worker notifications
- background execution helper for editor grapheme run orchestration
- slash command handlers enqueue work and return immediately
- grapheme console updates sourced from `ToolPayload` event handling

### Acceptance Criteria

- command submission no longer stalls typing/scrolling
- runtime/network operations still emit equivalent success/failure diagnostics
- `cargo check -p medousa` passes
- `cargo test -p medousa --bin medousa_tui` passes

## Phase 2: Event Loop and Input Cadence

### Scope

- tighten poll cadence and remove fixed idle sleeps where possible
- process richer crossterm event set (mouse + resize + paste)
- ensure event drain strategy avoids starvation

### Targets

- lower input-to-visual-response latency
- no regressions in keyboard shortcuts or overlays

## Phase 3: Render-Path Cost Controls

### Scope

- cache markdown/plain conversions and wrapped line fragments
- reduce per-frame allocations in conversation, observability, and console panes
- update caches incrementally on state mutation boundaries

### Targets

- stable frame times during long sessions
- reduced CPU spikes while idle or during stream output

## Phase 4: Instrumentation and Guardrails

### Scope

- add internal timing probes for event handling and render duration
- track queue depth and worker completion timings
- include optional debug overlay/log line for frame budget tracking

### Targets

- measurable latency budget with before/after deltas
- easier regression detection for future feature work

## Risks and Mitigations

- race risk from async workers: keep all UI mutation in event handler
- diagnostic ordering changes: preserve event type sequence (`ToolInvoked`, `JobEnqueued`, `JobProcessed`, payload)
- behavior drift: validate with existing tests and focused manual scenarios

## Exit Conditions

- Phase 1 merged and validated
- follow-up work prioritized as separate PRs/issues per phase
- future phases only begin after baseline metrics are captured
