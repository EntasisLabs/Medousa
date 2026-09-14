# Expressive Chat Checkpoint C — interactive Liquid validation

> **Status:** Implementation complete; physical and cross-surface validation pending
> **Implementation:** Slices 5–6
> **App version:** `medousa-home` 0.10.3
> **Prepared:** 2026-09-11

This record is the final release gate for the
[Expressive Chat epic](expressive-chat-media-ink-liquid-epic.md). The implementation
stops here until the product owner accepts this checkpoint or records named
follow-ups.

## Automated evidence

| Gate | Result |
|------|--------|
| Full Medousa Rust library suite | ✅ 1,694 passed, 3 ignored, 0 failed |
| Portable Liquid grammar and static recipe rendering | ✅ 14 tests passed |
| Focused Home interaction, timer, and registration behavior | ✅ 12 tests passed |
| Focused daemon state, admission, policy/evals, and typed-builder behavior | ✅ 5 tests passed |
| Generated Rust schema and HTTP contract | ✅ 250 types and 449 declared operations |
| Rust and Python SDK parity | ✅ 24 Rust tests; Ruff clean; 25 Python tests passed |
| Medousa Svelte/TypeScript check | ✅ 0 errors, 0 warnings |
| Tauri shell compile | ✅ Passed (`cargo check`) |
| Embedded-daemon feature compile | ✅ Passed |
| Full Medousa Home Vitest suite | ✅ 305 files, 1,581 tests passed |
| Strict documentation verification | ✅ Passed |

## Required matrix

| Surface | Workshop | Interactive renderer | Result | Notes |
|---------|----------|----------------------|--------|-------|
| Desktop | Local | Home recipe/timers | ⬜ | |
| Desktop | Remote | Home recipe/timers | ⬜ | |
| iPad | Local + remote | Home recipe/timers + notifications | ⬜ | |
| Android | Local + remote | Home recipe/timers + notifications | ⬜ | |
| VS Code / Obsidian / export | Any | Static portable recipe | ⬜ | |
| Any Home client | Any | Notifications denied | ⬜ | Must degrade cleanly |

## Test procedure

- Ask naturally for a recipe without mentioning Liquid. Confirm the response is
  a polished recipe experience and that a simple factual answer remains prose.
- Start a timer, background or kill the app, reopen it, and verify the remaining
  or completed state is derived from the absolute deadline.
- Allow notifications and confirm one completion notification fires. Repeat
  with permission denied and confirm the timer remains correct without errors.
- Run multiple timers in one message and across messages; pause, resume, and
  reset them independently.
- Use a scaling or substitution action and confirm it intentionally submits one
  coherent follow-up turn.
- Pause or reset timers and confirm those local events do not submit model
  turns. On the next relevant user turn, confirm bounded interaction context
  reaches the workshop daemon.
- Reload the session from a remote Home client and confirm component state is
  workshop-owned, revision-safe, and not dependent on a client-local path.
- Open the same recipe in VS Code, Obsidian, or an export and confirm complete
  ingredients, ordered steps, and durations remain readable.
- Exercise compare, plan, chart, decision, and action prompts. Confirm useful
  structure appears when warranted without turning ordinary prose into UI.

## Findings and sign-off

| Finding | Surface | Severity | Owner / follow-up | Accepted? |
|---------|---------|----------|-------------------|-----------|
| _Record during testing_ | | | | |

- Overall result: **🔄 Test required**
- Tested by: _Pending_
- Date: _Pending_
- Notes: _Pending_
