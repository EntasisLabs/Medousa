# Turn loop — single writer & explicit loop entry

**Status:** ✅ Complete (2026-06-07)

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

| Tool | Host bus | Worker lane |
|------|----------|-------------|
| `cognition_turn_begin_work` | ✅ | ✅ |
| `cognition_turn_finish` | ✅ | ✅ |
| `cognition_turn_request_more_rounds` | ✅ | ✅ |
| `cognition_turn_prepare_final` | ❌ removed | ✅ workshop only (deprecated) |

---

## FSM policy (shipped)

### No tool debt (zero invocations)

| Condition | Action |
|-----------|--------|
| `prepare_final` pending + non-empty draft (workshop) | EndTurn |
| At max rounds | EndTurn (fuse) |
| Clarifying question | EndTurn |
| **Otherwise** | **EndTurn** (`no_tools_prose`) |

### Tool debt (invocations exist)

| Condition | Action |
|-----------|--------|
| Workshop + `prepare_final` + non-empty draft | EndTurn |
| At max rounds | EndTurn (fuse) |
| Missing AVEC/calibrate receipts | ContinueLoop (MissingReceipts) |
| `prepare_final` + non-empty draft | EndTurn |
| Default | EndTurn |

---

## Bus events

| Event | `final_text` | UI |
|-------|--------------|-----|
| `content_delta` | — | Append preview |
| `turn_progress` | **no** | `statusLine`; fill content only if bubble empty |
| `final` | yes | Terminal merge (`resolveTurnContent.ts` / TUI reducer) |
| `scratch_reset` | — | Clear preview (Home + TUI) |

---

## Orchestrator

**Continuation synthesis** disabled on principal interactive surfaces (`interactive`, `tui`, `home-*`). Scheduled/headless lanes may still run it.

---

## Rollout

| Phase | Scope | Status |
|-------|--------|--------|
| **A** | Doc + `cognition_turn_begin_work` + `turn_progress` bus + FSM simplify | ✅ |
| **B** | Home/TUI reducers, drop `final_pending` body, disable interactive continuation | ✅ |
| **C** | Deprecate `prepare_final` on host; Home scratch_reset + terminal merge | ✅ |
| **D** | Host allowlist: `begin_work` in, `prepare_final` out; docs closed | ✅ |

---

## Success criteria (met)

- [x] Prose-only turn ends in one delivery (no interim continue)
- [x] Progress lines do not swap bubble body on terminal
- [x] Host cannot call `prepare_final` (workshop retains it)
- [x] Interactive continuation synthesis off
- [x] Home + TUI handle `scratch_reset` and terminal merge

---

## Key files

| Area | Path |
|------|------|
| Control tools | `src/turn_control_tools.rs` |
| Host allowlist | `src/agent_runtime/turn_worker/policy.rs` |
| FSM | `src/agent_runtime/turn_completion_fsm.rs` |
| Tool loop | `src/medousa_tool_loop.rs` |
| Home merge | `apps/medousa-home/src/lib/utils/resolveTurnContent.ts` |
| Home reducer | `apps/medousa-home/src/lib/stores/chat.svelte.ts` |
| Orchestrator | `src/agent_runtime/turn_orchestrator.rs` |
