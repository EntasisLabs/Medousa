# Turn loop — single writer & explicit loop entry

**Status:** Phase A–B in progress (2026-06-07)

Replaces interim-heuristic loop management with a deterministic rule: **no tool call → EndTurn; any tool call → loop semantics**. Interim user-facing prose is declared via `cognition_turn_begin_work`, not inferred from text.

Related: [turn-state-machine-plan.md](turn-state-machine-plan.md), [turn-control-tools-plan.md](turn-control-tools-plan.md)

---

## Problem

Interactive turns showed assistant text swapping ~3× per turn. Root cause: **multiple producers writing the same AnswerBuffer** (stream deltas, `final_pending`, terminal `final`, continuation synthesis) with last-writer-wins `final_text` replace in Home.

Legacy interim heuristics (`looks_like_interim_status`) forced extra model rounds when the model spoke before calling tools — token burn and “competing personalities.”

---

## Design principles

| Principle | Meaning |
|-----------|---------|
| **Tool call = loop** | Zero invocations this turn → EndTurn on prose. Any tool → loop / receipt / fuse rules apply. |
| **Explicit control tools = FSM** | `begin_work`, `finish`, `request_more_rounds` — no prose NLP for continue/stop. |
| **Single writer commit** | Terminal body only from `cognition_turn_finish` or EndTurn delivery. Progress ≠ final. |
| **Status ≠ body** | `turn_progress` events set status (and optional empty-bubble preview), never `final_text` replace. |

---

## Control-plane lifecycle

```
begin_work(message)  →  turn_progress on bus (status / empty bubble)
     ↓
  [tools…]
     ↓
finish(message)      →  terminal commit (existing)
```

| Tool | Role |
|------|------|
| `cognition_turn_begin_work` | Signal tool loop start; principal-facing interim via bus progress |
| `cognition_turn_finish` | Hard stop + final message (unchanged) |
| `cognition_turn_request_more_rounds` | Budget pause (unchanged) |
| `cognition_turn_prepare_final` | **Deprecated** — flag only; no `final_pending` body injection |

---

## FSM policy (post-change)

### No tool debt (zero invocations)

| Condition | Action |
|-----------|--------|
| `prepare_final` pending + non-empty draft | EndTurn |
| At max rounds | EndTurn (fuse) |
| Clarifying question | EndTurn |
| **Otherwise** | **EndTurn** (`no_tools_prose`) |

No `ContinueLoop(AwaitingTools)` on interim phrasing.

### Tool debt (invocations exist)

| Condition | Action |
|-----------|--------|
| Workshop + `prepare_final` + non-empty draft | EndTurn |
| At max rounds | EndTurn (fuse) |
| Missing AVEC/calibrate receipts | ContinueLoop (MissingReceipts) |
| `prepare_final` + non-empty draft | EndTurn |
| Substantive / clarifying / default | EndTurn |

No `ContinueLoop` on interim phrasing or `PrepareFinalInterim`.

---

## Bus events

| Event | `final_text` | UI |
|-------|--------------|-----|
| `content_delta` | — | Append preview |
| `turn_progress` | **no** | `statusLine`; fill content only if bubble empty |
| `final` | yes | Terminal replace / merge (TUI) |
| `scratch_reset` | — | Clear preview (TUI); Home TBD |

---

## Orchestrator

**Continuation synthesis** (`should_run_continuation`) disabled on principal interactive surfaces (`interactive`, `tui`, `home-*`). Scheduled/headless lanes may still run it.

---

## Rollout

| Phase | Scope | Status |
|-------|--------|--------|
| **A** | Doc + `cognition_turn_begin_work` + `turn_progress` bus + FSM simplify | ✅ |
| **B** | Home/TUI reducers, drop `final_pending` body, disable interactive continuation | ✅ |
| **C** | Deprecate `prepare_final` in prompts; optional AnswerBuffer struct in sink | Planned |

---

## Key files

| Area | Path |
|------|------|
| Control tools | `src/turn_control_tools.rs` |
| FSM | `src/agent_runtime/turn_completion_fsm.rs` |
| Tool loop | `src/medousa_tool_loop.rs` |
| Stream sink | `src/agent_runtime/stream_sink.rs` |
| SSE events | `src/interactive_turn_runtime.rs` |
| Home reducer | `apps/medousa-home/src/lib/stores/chat.svelte.ts` |
| Orchestrator | `src/agent_runtime/turn_orchestrator.rs` |
